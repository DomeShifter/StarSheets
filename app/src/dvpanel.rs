//! Data validation as a side panel, the way Google Sheets does it (docs/sheets-dv.md): a list of
//! rule cards, and an editor with "Apply to range", "Criteria", drop-down items with colours and
//! "Advanced options" (help text, warning or reject, display style). Rules are stored by
//! GridCraft (`data.validation`); Sheets' text and tick-box criteria become custom formulas, and
//! drop-down item colours become ordinary conditional-formatting rules, so both survive in .xlsx.
//! Like Sheets, every change is applied to the sheet at once (undone again on Cancel).

use egui::{Color32, Frame, Margin, RichText, Sense, Stroke, Ui, vec2};
use gridcraft_engine::core::RangeRef;
use gridcraft_engine::model::{CfOperator, CfRule, ErrorStyle, Validation, ValidationKind};
use gridcraft_ui_egui::SheetApp;
use serde_json::{Value as Json, json};

use crate::cfpanel::{c32, date_number, hex, rgb_of};
use crate::theme;

const GREEN: Color32 = Color32::from_rgb(0x14, 0x6C, 0x2E);
const PANEL_W: f32 = 320.0;
/// Asks the panel to open with a new drop-down rule (Insert › Drop-down), through egui's memory.
pub const DROPDOWN_REQUEST: &str = "starsheets_dv_dropdown";

#[derive(Clone, Copy, PartialEq)]
enum Crit {
    Dropdown,
    DropdownRange,
    TextContains,
    TextNotContains,
    TextExactly,
    Email,
    Url,
    ValidDate,
    /// Date is / before / on or before / after / on or after / between / not between.
    Date(&'static str),
    /// Greater than … not between.
    Number(&'static str),
    TickBox,
    Custom,
}

/// Google Sheets' criteria, in its order; `true` = a divider after it.
const CRITERIA: &[(&str, Crit, bool)] = &[
    ("Drop-down", Crit::Dropdown, false),
    ("Drop-down (from a range)", Crit::DropdownRange, true),
    ("Text contains", Crit::TextContains, false),
    ("Text does not contain", Crit::TextNotContains, false),
    ("Text is exactly", Crit::TextExactly, false),
    ("Text is valid email", Crit::Email, false),
    ("Text is valid URL", Crit::Url, true),
    ("Is valid date", Crit::ValidDate, false),
    ("Date is", Crit::Date("equal"), false),
    ("Date is before", Crit::Date("less"), false),
    ("Date is on or before", Crit::Date("lessOrEqual"), false),
    ("Date is after", Crit::Date("greater"), false),
    ("Date is on or after", Crit::Date("greaterOrEqual"), false),
    ("Date is between", Crit::Date("between"), false),
    ("Date is not between", Crit::Date("notBetween"), true),
    ("Greater than", Crit::Number("greater"), false),
    ("Greater than or equal to", Crit::Number("greaterOrEqual"), false),
    ("Less than", Crit::Number("less"), false),
    ("Less than or equal to", Crit::Number("lessOrEqual"), false),
    ("Is equal to", Crit::Number("equal"), false),
    ("Is not equal to", Crit::Number("notEqual"), false),
    ("Is between", Crit::Number("between"), false),
    ("Is not between", Crit::Number("notBetween"), true),
    ("Tick box", Crit::TickBox, false),
    ("Custom formula is", Crit::Custom, false),
];

/// "Date is" choices: label, GridCraft operator, first and second bound (formulas).
const DATE_IS: &[(&str, &str, &str, &str)] = &[
    ("exact date", "equal", "", ""),
    ("today", "equal", "TODAY()", ""),
    ("tomorrow", "equal", "TODAY()+1", ""),
    ("yesterday", "equal", "TODAY()-1", ""),
    ("in the past week", "between", "TODAY()-7", "TODAY()"),
    ("in the past month", "between", "EDATE(TODAY(),-1)", "TODAY()"),
    ("in the past year", "between", "EDATE(TODAY(),-12)", "TODAY()"),
];
/// Choices for before / after: label, formula ("" = a typed date).
const DATE_REL: &[(&str, &str)] = &[
    ("exact date", ""),
    ("today", "TODAY()"),
    ("tomorrow", "TODAY()+1"),
    ("yesterday", "TODAY()-1"),
    ("one week ago", "TODAY()-7"),
    ("one month ago", "EDATE(TODAY(),-1)"),
    ("one year ago", "EDATE(TODAY(),-12)"),
];
/// The last valid spreadsheet date (31/12/9999).
const LAST_DATE: &str = "2958465";

/// Sheets' light chip colours for drop-down items (first = its default grey).
const CHIP_COLOURS: &[u32] = &[0xE8EAED, 0xC9DAF8, 0xD9EAD3, 0xFFF2CC, 0xFCE5CD, 0xF4CCCC, 0xD9D2E9, 0xEAD1DC, 0xC6E7E8, 0xB7B7B7];

#[derive(Clone, Default, PartialEq)]
struct Item {
    text: String,
    colour: Option<u32>,
}

/// What an edited rule was, so it can be taken off before the new one goes on.
#[derive(Clone, PartialEq)]
struct Orig {
    range: String,
    /// Conditional-formatting rules that colour its drop-down items.
    colour_rules: Vec<usize>,
    tick_box: bool,
}

#[derive(Clone, PartialEq)]
struct Draft {
    orig: Option<Orig>,
    range: String,
    crit: usize,
    /// Which "Date is" / before-after choice (index into DATE_IS or DATE_REL).
    when: usize,
    v1: String,
    v2: String,
    items: Vec<Item>,
    help_on: bool,
    help: String,
    reject: bool,
    plain: bool,
}

impl Draft {
    fn new(range: String) -> Draft {
        Draft {
            orig: None,
            range,
            crit: 0,
            when: 0,
            v1: String::new(),
            v2: String::new(),
            items: vec![Item::default(), Item::default()],
            help_on: false,
            help: String::new(),
            reject: true,
            plain: false,
        }
    }
    fn crit(&self) -> Crit {
        CRITERIA[self.crit].1
    }
}

#[derive(Default)]
pub struct DvPanel {
    pub open: bool,
    draft: Option<Draft>,
    error: Option<String>,
    /// Live apply: (what was applied, how many undo steps it took).
    preview: Option<(String, usize)>,
    /// Set on the frame the panel opened, so the other side panel can close.
    pub just_opened: bool,
}

impl DvPanel {
    /// Opens the panel: the rule list when the sheet has rules, else a new rule, as in Sheets.
    pub fn open(&mut self, app: &SheetApp, dropdown: bool) {
        self.open = true;
        self.just_opened = true;
        self.error = None;
        let has_rules = app.session.active().and_then(|d| d.wb.active()).is_some_and(|sh| !sh.validations.is_empty());
        self.draft = if has_rules && !dropdown { None } else { Some(Draft::new(selection_text(app))) };
    }

    /// Closes the panel, keeping what was applied (Sheets has no Cancel for a live rule).
    pub fn close(&mut self) {
        self.preview = None;
        self.draft = None;
        self.open = false;
    }

    /// Takes over GridCraft's own Data Validation dialog, and Insert › Drop-down.
    pub fn intercept(&mut self, app: &mut SheetApp, ctx: &egui::Context) {
        self.just_opened = false;
        if app.dialog.as_ref().is_some_and(|d| d.command == Some("data.validation") || d.name == "dataValidation") {
            app.dialog = None;
            self.open(app, false);
        }
        if ctx.data_mut(|m| m.remove_temp::<bool>(egui::Id::new(DROPDOWN_REQUEST))).is_some() {
            self.preview = None;
            self.open(app, true);
        }
    }

    pub fn ui(&mut self, app: &mut SheetApp, ui: &mut Ui) {
        if !self.open {
            return;
        }
        egui::Panel::right("starsheets_dv")
            .exact_size(PANEL_W + 32.0)
            .resizable(false)
            .frame(Frame::NONE.fill(Color32::WHITE).inner_margin(Margin::symmetric(16, 12)).stroke(Stroke::new(1.0, theme::DIVIDER)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Data validation rules").size(16.0).color(theme::TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("✕").size(16.0)).frame(false)).on_hover_text("Close").clicked() {
                            self.close();
                        }
                    });
                });
                ui.add_space(8.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_width(PANEL_W);
                    if let Some(mut d) = self.draft.take() {
                        match self.editor(ui, &mut d) {
                            // Done: keep what is applied, back to the list.
                            Some(true) => {
                                self.update_preview(app, &d);
                                if self.error.is_none() {
                                    self.preview = None;
                                } else {
                                    self.draft = Some(d);
                                }
                            }
                            // Remove rule: undo the live rule, then take the old one off too.
                            Some(false) => {
                                self.revert_preview(app);
                                if let Some(o) = &d.orig {
                                    remove(app, o);
                                }
                                self.error = None;
                                let none_left = app.session.active().and_then(|x| x.wb.active()).is_none_or(|sh| sh.validations.is_empty());
                                if none_left {
                                    self.open = false;
                                }
                            }
                            None => {
                                self.update_preview(app, &d);
                                self.draft = Some(d);
                            }
                        }
                    } else {
                        self.list(app, ui);
                    }
                });
            });
    }

    /// The rule cards: range and criterion; click to edit, bin to remove.
    fn list(&mut self, app: &mut SheetApp, ui: &mut Ui) {
        let Some((rules, cfs, theme_colours)) = app.session.active().and_then(|d| d.wb.active().map(|sh| (sh.validations.clone(), sh.cond_formats.clone(), d.wb.theme.clone()))) else {
            return;
        };
        let mut remove_it = None;
        for (i, dv) in rules.iter().enumerate() {
            let d = draft_from(dv, &cfs, &theme_colours);
            let (r, resp) = ui.allocate_exact_size(vec2(PANEL_W, 60.0), Sense::click());
            let p = ui.painter();
            if resp.hovered() {
                p.rect_filled(r, 0.0, theme::MENU_HOVER);
            }
            p.text(r.left_center() + vec2(12.0, -10.0), egui::Align2::LEFT_CENTER, &d.range, egui::FontId::proportional(13.5), theme::TEXT);
            p.text(r.left_center() + vec2(12.0, 10.0), egui::Align2::LEFT_CENTER, clip(&describe(&d), 44), egui::FontId::proportional(12.0), theme::TEXT_DIM);
            let bin = egui::Rect::from_center_size(egui::pos2(r.right() - 20.0, r.center().y), vec2(28.0, 28.0));
            let b = ui.interact(bin, ui.id().with(("dv_bin", i)), Sense::click());
            if (resp.hovered() || b.hovered())
                && let Some(img) = crate::icons::icon("delete")
            {
                img.paint_at(ui, bin.shrink(5.0));
            }
            if b.on_hover_text("Remove rule").clicked() {
                remove_it = d.orig.clone();
                continue;
            }
            if resp.clicked() {
                self.draft = Some(d);
            }
            ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, theme::DIVIDER));
        }
        if let Some(o) = remove_it {
            remove(app, &o);
        }
        ui.add_space(12.0);
        if ui.add(egui::Button::new(RichText::new("+  Add rule").size(14.0).color(GREEN)).frame(false)).clicked() {
            self.draft = Some(Draft::new(selection_text(app)));
        }
    }

    /// The rule editor. Returns Some(true) for Done, Some(false) for Remove rule.
    fn editor(&mut self, ui: &mut Ui, d: &mut Draft) -> Option<bool> {
        ui.label(RichText::new("Apply to range").size(14.0).strong());
        ui.add(egui::TextEdit::singleline(&mut d.range).desired_width(PANEL_W).margin(vec2(8.0, 8.0)));
        ui.add_space(12.0);
        ui.label(RichText::new("Criteria").size(14.0).strong());
        ui.add_space(4.0);
        egui::ComboBox::from_id_salt("dv_crit").width(PANEL_W - 8.0).height(1000.0).selected_text(CRITERIA[d.crit].0).show_ui(ui, |ui| {
            for (i, (label, _, sep)) in CRITERIA.iter().enumerate() {
                ui.selectable_value(&mut d.crit, i, *label);
                if *sep {
                    ui.separator();
                }
            }
        });
        ui.add_space(6.0);
        criterion_inputs(ui, d);

        // Advanced options.
        ui.add_space(10.0);
        egui::CollapsingHeader::new(RichText::new("Advanced options").size(14.0).strong()).default_open(false).show(ui, |ui| {
            ui.checkbox(&mut d.help_on, "Show help text for a selected cell");
            if d.help_on {
                let hint = default_help(d);
                ui.add(egui::TextEdit::singleline(&mut d.help).hint_text(hint).desired_width(PANEL_W - 24.0).margin(vec2(8.0, 6.0)));
            }
            ui.add_space(6.0);
            ui.label(RichText::new("If the data is invalid:").color(theme::TEXT_DIM));
            ui.radio_value(&mut d.reject, false, "Show a warning");
            ui.radio_value(&mut d.reject, true, "Reject the input");
            if matches!(d.crit(), Crit::Dropdown | Crit::DropdownRange) {
                ui.add_space(6.0);
                ui.label(RichText::new("Display style").color(theme::TEXT_DIM));
                ui.radio_value(&mut d.plain, false, "Arrow");
                ui.radio_value(&mut d.plain, true, "Plain text");
            }
        });

        if let Some(e) = &self.error {
            ui.add_space(6.0);
            ui.label(RichText::new(e).color(Color32::from_rgb(0xC5, 0x39, 0x29)));
        }
        ui.add_space(14.0);
        let mut out = None;
        ui.horizontal(|ui| {
            ui.add_space(PANEL_W - 230.0);
            if ui.add(egui::Button::new(RichText::new("Remove rule").color(GREEN)).stroke(Stroke::new(1.0, theme::BOX)).fill(Color32::WHITE).corner_radius(4.0).min_size(vec2(120.0, 36.0))).clicked() {
                out = Some(false);
            }
            if ui.add(egui::Button::new(RichText::new("Done").color(Color32::WHITE)).fill(GREEN).corner_radius(4.0).min_size(vec2(90.0, 36.0))).clicked() {
                out = Some(true);
            }
        });
        out
    }

    fn revert_preview(&mut self, app: &mut SheetApp) {
        if let Some((_, n)) = self.preview.take() {
            for _ in 0..n {
                let _ = app.run("edit.undo", json!({}));
            }
        }
    }

    /// Applies the draft as it stands (Sheets applies a rule as you build it), replacing what was
    /// applied before. An incomplete draft applies nothing and says what is missing.
    fn update_preview(&mut self, app: &mut SheetApp, d: &Draft) {
        let plan = commands(d);
        let key = format!("{:?}", plan.as_ref().map_err(|_| ()).ok());
        if self.preview.as_ref().is_some_and(|(k, _)| *k == key) {
            self.error = plan.err();
            return;
        }
        self.revert_preview(app);
        let mut applied = 0;
        match plan {
            Ok(cmds) => {
                self.error = None;
                if let Some(o) = &d.orig {
                    applied += remove(app, o);
                }
                for (cmd, params) in cmds {
                    match app.run(cmd, params) {
                        Ok(_) => applied += 1,
                        Err(e) => {
                            self.error = Some(gridcraft_ui_egui::clean_error(&e));
                            break;
                        }
                    }
                }
            }
            Err(e) => self.error = Some(e),
        }
        self.preview = Some((key, applied));
    }
}

/// The inputs under the criteria box for the chosen criterion.
fn criterion_inputs(ui: &mut Ui, d: &mut Draft) {
    let boxed = |ui: &mut Ui, s: &mut String, hint: &str| {
        ui.add(egui::TextEdit::singleline(s).hint_text(hint).desired_width(PANEL_W - 8.0).margin(vec2(8.0, 8.0)));
    };
    match d.crit() {
        Crit::Dropdown => {
            let mut drop = None;
            for (i, it) in d.items.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    chip_colour(ui, i, &mut it.colour);
                    ui.add(egui::TextEdit::singleline(&mut it.text).hint_text(format!("Option {}", i + 1)).desired_width(PANEL_W - 84.0).margin(vec2(8.0, 6.0)));
                    if let Some(img) = crate::icons::icon("delete")
                        && ui.add(egui::Button::image(img.fit_to_exact_size(vec2(18.0, 18.0))).frame(false)).on_hover_text("Delete item").clicked()
                    {
                        drop = Some(i);
                    }
                });
            }
            if let Some(i) = drop {
                d.items.remove(i);
            }
            if ui.add(egui::Button::new(RichText::new("Add another item").color(GREEN)).stroke(Stroke::new(1.0, theme::BOX)).fill(Color32::WHITE).corner_radius(4.0).min_size(vec2(140.0, 32.0))).clicked() {
                d.items.push(Item::default());
            }
        }
        Crit::DropdownRange => boxed(ui, &mut d.v1, "e.g. Sheet1!A1:A10"),
        Crit::TextContains | Crit::TextNotContains | Crit::TextExactly => boxed(ui, &mut d.v1, "Value or formula"),
        Crit::Email | Crit::Url | Crit::ValidDate | Crit::TickBox => {}
        Crit::Date(op) => {
            let between = matches!(op, "between" | "notBetween");
            if !between {
                let labels: Vec<&str> = if op == "equal" { DATE_IS.iter().map(|x| x.0).collect() } else { DATE_REL.iter().map(|x| x.0).collect() };
                egui::ComboBox::from_id_salt("dv_when").width(PANEL_W - 8.0).selected_text(labels[d.when.min(labels.len() - 1)]).show_ui(ui, |ui| {
                    for (i, l) in labels.iter().enumerate() {
                        ui.selectable_value(&mut d.when, i, *l);
                    }
                });
            }
            if between || d.when == 0 {
                boxed(ui, &mut d.v1, "dd/mm/yyyy");
            }
            if between {
                boxed(ui, &mut d.v2, "dd/mm/yyyy");
            }
        }
        Crit::Number(op) => {
            boxed(ui, &mut d.v1, "Value or formula");
            if matches!(op, "between" | "notBetween") {
                boxed(ui, &mut d.v2, "Value or formula");
            }
        }
        Crit::Custom => boxed(ui, &mut d.v1, "=A1>0"),
    }
}

/// A drop-down item's colour: a small chip that opens Sheets' light colours.
fn chip_colour(ui: &mut Ui, i: usize, colour: &mut Option<u32>) {
    let fill = c32(colour.unwrap_or(CHIP_COLOURS[0]));
    let resp = ui.add(egui::Button::new("").fill(fill).corner_radius(10.0).min_size(vec2(22.0, 22.0)).stroke(Stroke::new(1.0, theme::BOX))).on_hover_text("Colour");
    egui::Popup::menu(&resp).id(egui::Id::new(("dv_chip", i))).show(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(140.0);
            for (k, c) in CHIP_COLOURS.iter().enumerate() {
                let pick = if k == 0 { None } else { Some(*c) };
                let mut b = egui::Button::new("").fill(c32(*c)).corner_radius(10.0).min_size(vec2(22.0, 22.0));
                if *colour == pick {
                    b = b.stroke(Stroke::new(2.0, GREEN));
                }
                if ui.add(b).clicked() {
                    *colour = pick;
                    ui.close();
                }
            }
        });
    });
}

fn selection_text(app: &SheetApp) -> String {
    app.session.active().map(|d| d.selection.ranges.iter().map(|r| r.a1()).collect::<Vec<_>>().join(",")).unwrap_or_default()
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n - 1).collect::<String>()) }
}

/// The top-left cell of a range list, written relatively (custom formulas are relative to it).
fn top_left(range: &str) -> Option<String> {
    let first = range.split(',').next()?.trim();
    Some(RangeRef::parse(first)?.start.a1())
}

/// A formula string literal.
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// A typed value: a formula (`=…`) is used as is, anything else as text.
fn text_arg(v: &str) -> String {
    match v.trim().strip_prefix('=') {
        Some(f) => f.to_string(),
        None => quoted(v.trim()),
    }
}

/// A date box: dd/mm/yyyy, or a formula.
fn date_arg(v: &str) -> Result<String, String> {
    let v = v.trim();
    if let Some(f) = v.strip_prefix('=') {
        return Ok(f.to_string());
    }
    date_number(v).map(|n| n.to_string()).ok_or_else(|| "Enter the date as dd/mm/yyyy.".to_string())
}

/// What the rule checks, for its card and the default help text.
fn describe(d: &Draft) -> String {
    let (label, crit, _) = CRITERIA[d.crit];
    match crit {
        Crit::Dropdown => format!("Drop-down: {}", d.items.iter().map(|i| i.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(", ")),
        Crit::DropdownRange => format!("Drop-down from {}", d.v1.trim()),
        Crit::Email | Crit::Url | Crit::ValidDate | Crit::TickBox => label.to_string(),
        Crit::Date(op) if !matches!(op, "between" | "notBetween") && d.when > 0 => {
            let when = if op == "equal" { DATE_IS[d.when.min(DATE_IS.len() - 1)].0 } else { DATE_REL[d.when.min(DATE_REL.len() - 1)].0 };
            format!("{label} {when}")
        }
        Crit::Date(op) | Crit::Number(op) if matches!(op, "between" | "notBetween") => format!("{label} {} and {}", d.v1.trim(), d.v2.trim()),
        Crit::Custom => format!("Custom formula is {}", d.v1.trim()),
        _ => format!("{label} {}", d.v1.trim()),
    }
}

/// Lower-cases only the first letter (keeps item names and values as typed).
fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().chain(c).collect()).unwrap_or_default()
}

fn default_help(d: &Draft) -> String {
    match d.crit() {
        Crit::Dropdown | Crit::DropdownRange => "Choose an item from the list".to_string(),
        _ => format!("Enter a value: {}", lower_first(&describe(d))),
    }
}

/// The commands that put the draft on the sheet, in order.
fn commands(d: &Draft) -> Result<Vec<(&'static str, Json)>, String> {
    let range = d.range.trim().to_string();
    let tl = top_left(&range).ok_or("Choose the cells the rule applies to (e.g. A1:A10).")?;
    let need = |v: &str| if v.trim().is_empty() { Err("Enter a value for the rule.".to_string()) } else { Ok(()) };
    let mut p = json!({ "range": range, "errorStyle": if d.reject { "stop" } else { "warning" }, "dropdown": !d.plain });
    let mut extra = Vec::new();
    let custom = |f: String| json!({ "type": "custom", "formula1": format!("={f}") });
    let rule = match d.crit() {
        Crit::Dropdown => {
            let items: Vec<&Item> = d.items.iter().filter(|i| !i.text.trim().is_empty()).collect();
            if items.is_empty() {
                return Err("Add at least one item.".into());
            }
            if items.iter().any(|i| i.text.contains(',')) {
                return Err("An item can't contain a comma.".into());
            }
            for it in &items {
                if let Some(c) = it.colour {
                    let style = json!({ "fill": hex(c), "font": {} });
                    extra.push(("home.conditionalFormat", json!({ "range": range, "rule": { "type": "cellIs", "operator": "equal", "value": quoted(it.text.trim()), "style": style } })));
                }
            }
            json!({ "type": "list", "formula1": items.iter().map(|i| i.text.trim()).collect::<Vec<_>>().join(",") })
        }
        Crit::DropdownRange => {
            need(&d.v1)?;
            json!({ "type": "list", "formula1": format!("={}", d.v1.trim().trim_start_matches('=')) })
        }
        Crit::TextContains => {
            need(&d.v1)?;
            custom(format!("ISNUMBER(SEARCH({},{tl}))", text_arg(&d.v1)))
        }
        Crit::TextNotContains => {
            need(&d.v1)?;
            custom(format!("ISERROR(SEARCH({},{tl}))", text_arg(&d.v1)))
        }
        Crit::TextExactly => {
            need(&d.v1)?;
            custom(format!("EXACT({tl},{})", text_arg(&d.v1)))
        }
        Crit::Email => custom(format!("COUNTIF({tl},\"?*@?*.?*\")=1")),
        Crit::Url => custom(format!("OR(COUNTIF({tl},\"http://?*.?*\")=1,COUNTIF({tl},\"https://?*.?*\")=1)")),
        Crit::ValidDate => json!({ "type": "date", "operator": "between", "formula1": "1", "formula2": LAST_DATE }),
        Crit::Date(op) if matches!(op, "between" | "notBetween") => {
            json!({ "type": "date", "operator": op, "formula1": date_arg(&d.v1)?, "formula2": date_arg(&d.v2)? })
        }
        Crit::Date("equal") => {
            let (_, op, a, b) = DATE_IS[d.when.min(DATE_IS.len() - 1)];
            if a.is_empty() {
                json!({ "type": "date", "operator": "equal", "formula1": date_arg(&d.v1)? })
            } else if b.is_empty() {
                json!({ "type": "date", "operator": op, "formula1": a })
            } else {
                json!({ "type": "date", "operator": op, "formula1": a, "formula2": b })
            }
        }
        Crit::Date(op) => {
            let f = DATE_REL[d.when.min(DATE_REL.len() - 1)].1;
            let f = if f.is_empty() { date_arg(&d.v1)? } else { f.to_string() };
            json!({ "type": "date", "operator": op, "formula1": f })
        }
        Crit::Number(op) => {
            need(&d.v1)?;
            let mut r = json!({ "type": "decimal", "operator": op, "formula1": d.v1.trim() });
            if matches!(op, "between" | "notBetween") {
                need(&d.v2)?;
                r["formula2"] = json!(d.v2.trim());
            }
            r
        }
        Crit::TickBox => {
            extra.insert(0, ("insert.checkbox", json!({ "range": range })));
            custom(format!("ISLOGICAL({tl})"))
        }
        Crit::Custom => {
            need(&d.v1)?;
            custom(d.v1.trim().trim_start_matches('=').to_string())
        }
    };
    for (k, v) in rule.as_object().into_iter().flatten() {
        p[k] = v.clone();
    }
    if d.help_on {
        p["inputMessage"] = json!(if d.help.trim().is_empty() { default_help(d) } else { d.help.trim().to_string() });
    }
    p["errorTitle"] = json!("There was a problem");
    p["errorMessage"] = json!(format!("This cell's contents break its rule. Input must be: {}.", lower_first(&describe(d))));
    let mut out = vec![("data.validation", p)];
    out.extend(extra);
    Ok(out)
}

/// Takes a rule off: its validation, the colours of its items and its tick boxes. Returns the
/// number of steps it took (for undoing a preview).
fn remove(app: &mut SheetApp, o: &Orig) -> usize {
    let mut n = 0;
    if app.run("data.validation", json!({ "range": o.range, "clear": true })).is_ok() {
        n += 1;
    }
    let mut idx = o.colour_rules.clone();
    idx.sort_unstable_by(|a, b| b.cmp(a));
    for i in idx {
        if app.run("home.manageRules", json!({ "delete": i })).is_ok() {
            n += 1;
        }
    }
    if o.tick_box && app.run("home.numberFormat", json!({ "range": o.range, "format": "General" })).is_ok() {
        n += 1;
    }
    n
}

/// The text inside the first `"…"` of a formula (with `""` back to `"`), or the formula itself
/// with an `=` when the value was a formula.
fn first_quoted(f: &str, before: &str) -> String {
    let inner = &f[f.find('(').map_or(0, |i| i + 1)..];
    if let Some(rest) = inner.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    out.push('"');
                } else {
                    return out;
                }
            } else {
                out.push(c);
            }
        }
        return out;
    }
    let arg = inner.split(before).next().unwrap_or(inner);
    format!("={arg}")
}

/// Reads a stored rule back into the editor's terms (the reverse of `commands`).
fn draft_from(dv: &Validation, cfs: &[gridcraft_engine::model::CondFormat], theme_colours: &gridcraft_engine::model::Theme) -> Draft {
    let range = dv.ranges.iter().map(|r| r.a1()).collect::<Vec<_>>().join(",");
    let mut d = Draft::new(range.clone());
    let tl = top_left(&range).unwrap_or_default();
    let pos = |c: Crit| CRITERIA.iter().position(|x| x.1 == c).unwrap_or(0);
    let op = op_name(dv.op);
    let f1 = dv.f1.trim().to_string();
    let f2 = dv.f2.clone().unwrap_or_default();
    let mut colour_rules = Vec::new();
    let mut tick_box = false;
    let show_date = |f: &str| f.parse::<i64>().ok().map(date_text).unwrap_or_else(|| format!("={f}"));
    match dv.kind {
        ValidationKind::List if f1.starts_with('"') => {
            d.crit = pos(Crit::Dropdown);
            let body = f1.trim_matches('"').replace("\"\"", "\"");
            d.items = body.split(',').map(|t| Item { text: t.trim().to_string(), colour: None }).collect();
            for (i, cf) in cfs.iter().enumerate() {
                let same = cf.ranges.iter().map(|r| r.a1()).collect::<Vec<_>>().join(",") == range;
                if let (true, CfRule::CellIs { op: CfOperator::Equal, a, b: None, style }) = (same, &cf.rule)
                    && let Some(it) = d.items.iter_mut().find(|it| quoted(&it.text) == *a)
                {
                    it.colour = rgb_of(&style.fill.fg, theme_colours);
                    colour_rules.push(i);
                }
            }
        }
        ValidationKind::List => {
            d.crit = pos(Crit::DropdownRange);
            d.v1 = f1;
        }
        ValidationKind::Custom => {
            let f = f1.trim_start_matches('=');
            if f.starts_with("ISNUMBER(SEARCH(") {
                d.crit = pos(Crit::TextContains);
                d.v1 = first_quoted(&f["ISNUMBER(".len()..], ",");
            } else if f.starts_with("ISERROR(SEARCH(") {
                d.crit = pos(Crit::TextNotContains);
                d.v1 = first_quoted(&f["ISERROR(".len()..], ",");
            } else if let Some(rest) = f.strip_prefix("EXACT(") {
                d.crit = pos(Crit::TextExactly);
                let arg = rest.strip_prefix(&format!("{tl},")).unwrap_or(rest);
                d.v1 = first_quoted(&format!("({arg}"), ")");
                if d.v1.ends_with(')') && d.v1.starts_with('=') {
                    d.v1.pop();
                }
            } else if f.starts_with("COUNTIF(") && f.contains('@') {
                d.crit = pos(Crit::Email);
            } else if f.starts_with("OR(COUNTIF(") && f.contains("http") {
                d.crit = pos(Crit::Url);
            } else if f.starts_with("ISLOGICAL(") {
                d.crit = pos(Crit::TickBox);
                tick_box = true;
            } else {
                d.crit = pos(Crit::Custom);
                d.v1 = format!("={f}");
            }
        }
        ValidationKind::Date if op == "between" && f1 == "1" && f2 == LAST_DATE => d.crit = pos(Crit::ValidDate),
        ValidationKind::Date | ValidationKind::Time => {
            if let Some(w) = DATE_IS.iter().position(|x| x.1 == op && !x.2.is_empty() && x.2 == f1 && x.3 == f2) {
                d.crit = pos(Crit::Date("equal"));
                d.when = w;
            } else {
                let c = CRITERIA.iter().position(|x| x.1 == Crit::Date(static_op(op))).unwrap_or(0);
                d.crit = c;
                match DATE_REL.iter().position(|x| !x.1.is_empty() && x.1 == f1) {
                    Some(w) if !matches!(op, "between" | "notBetween") => d.when = w,
                    _ => {
                        d.v1 = show_date(&f1);
                        d.v2 = if f2.is_empty() { String::new() } else { show_date(&f2) };
                    }
                }
            }
        }
        _ => {
            d.crit = CRITERIA.iter().position(|x| x.1 == Crit::Number(static_op(op))).unwrap_or(0);
            d.v1 = f1;
            d.v2 = f2;
        }
    }
    d.help_on = !dv.input_message.is_empty();
    d.help = dv.input_message.clone();
    d.reject = dv.error_style == ErrorStyle::Stop;
    d.plain = !dv.in_cell_dropdown;
    d.orig = Some(Orig { range, colour_rules, tick_box });
    d
}

fn op_name(op: CfOperator) -> &'static str {
    match op {
        CfOperator::Between => "between",
        CfOperator::NotBetween => "notBetween",
        CfOperator::Equal => "equal",
        CfOperator::NotEqual => "notEqual",
        CfOperator::Greater => "greater",
        CfOperator::Less => "less",
        CfOperator::GreaterOrEqual => "greaterOrEqual",
        CfOperator::LessOrEqual => "lessOrEqual",
    }
}

fn static_op(op: &str) -> &'static str {
    ["between", "notBetween", "equal", "notEqual", "greater", "less", "greaterOrEqual", "lessOrEqual"].into_iter().find(|x| *x == op).unwrap_or("equal")
}

/// A spreadsheet date number as dd/mm/yyyy.
fn date_text(n: i64) -> String {
    // Civil from days (Howard Hinnant), from the 1899-12-30 epoch.
    let z = n - 25_569 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{day:02}/{month:02}/{year}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridcraft_engine::Session;
    use gridcraft_engine::core::CellRef;

    fn draft(crit: Crit, v1: &str, v2: &str) -> Draft {
        let mut d = Draft::new("B2:B9".into());
        d.crit = CRITERIA.iter().position(|x| x.1 == crit).unwrap();
        d.v1 = v1.into();
        d.v2 = v2.into();
        d
    }

    /// Puts a draft on a fresh sheet and checks typed input against it at B3.
    fn accepts(d: &Draft, input: &str) -> bool {
        let mut s = Session::new();
        s.execute("file.new", json!({})).unwrap();
        for (cmd, p) in commands(d).unwrap() {
            s.execute(cmd, p).unwrap();
        }
        let doc = s.active().unwrap();
        gridcraft_engine::cmd::data::check_validation(&doc.wb, 0, CellRef::new(2, 1), input).is_none()
    }

    #[test]
    fn criteria_check_input_like_sheets() {
        let d = draft(Crit::TextContains, "cat", "");
        assert!(accepts(&d, "Concatenate") && !accepts(&d, "dog"));
        let d = draft(Crit::TextNotContains, "cat", "");
        assert!(!accepts(&d, "a cat") && accepts(&d, "dog"));
        let d = draft(Crit::TextExactly, "Yes", "");
        assert!(accepts(&d, "Yes") && !accepts(&d, "yes"));
        let d = draft(Crit::Email, "", "");
        assert!(accepts(&d, "a@b.com") && !accepts(&d, "a@b") && !accepts(&d, "hello"));
        let d = draft(Crit::Url, "", "");
        assert!(accepts(&d, "https://x.org/a") && !accepts(&d, "x"));
        let d = draft(Crit::Number("between"), "1", "10");
        assert!(accepts(&d, "5") && !accepts(&d, "11"));
        let d = draft(Crit::Date("greater"), "01/01/2026", "");
        assert!(accepts(&d, "2026-02-01") && !accepts(&d, "2025-12-31"));
        let d = draft(Crit::ValidDate, "", "");
        assert!(accepts(&d, "2026-02-01") && !accepts(&d, "soon"));
        let mut d = draft(Crit::Dropdown, "", "");
        d.items = vec![Item { text: "Open".into(), colour: Some(0xD9EAD3) }, Item { text: "Closed".into(), colour: None }];
        assert!(accepts(&d, "Open") && !accepts(&d, "Maybe"));
    }

    #[test]
    fn rules_read_back_into_the_editor() {
        let mut cases = vec![
            draft(Crit::TextContains, "cat", ""),
            draft(Crit::TextNotContains, "=C1", ""),
            draft(Crit::TextExactly, "Say \"hi\"", ""),
            draft(Crit::Email, "", ""),
            draft(Crit::Url, "", ""),
            draft(Crit::ValidDate, "", ""),
            draft(Crit::Date("less"), "05/03/2026", ""),
            draft(Crit::Date("between"), "01/01/2026", "31/12/2026"),
            draft(Crit::Number("notBetween"), "1", "10"),
            draft(Crit::TickBox, "", ""),
            draft(Crit::Custom, "=B2>C2", ""),
        ];
        let mut rel = draft(Crit::Date("equal"), "", "");
        rel.when = 4;
        cases.push(rel);
        let mut dd = draft(Crit::Dropdown, "", "");
        dd.items = vec![Item { text: "Open".into(), colour: Some(0xD9EAD3) }, Item { text: "Closed".into(), colour: None }];
        dd.help_on = true;
        dd.help = "Pick one".into();
        dd.reject = false;
        cases.push(dd);
        for want in cases {
            let mut s = Session::new();
            s.execute("file.new", json!({})).unwrap();
            for (cmd, p) in commands(&want).unwrap() {
                s.execute(cmd, p).unwrap();
            }
            let doc = s.active().unwrap();
            let sh = doc.wb.active().unwrap().clone();
            let mut got = draft_from(&sh.validations[0], &sh.cond_formats, &doc.wb.theme);
            got.orig = None;
            assert!(got == want, "{} came back as {} ({:?} / {:?})", describe(&want), describe(&got), got.v1, got.v2);
        }
    }

    #[test]
    fn dates_print_back() {
        assert_eq!(date_text(date_number("05/03/2026").unwrap()), "05/03/2026");
        assert_eq!(date_text(date_number("29/02/2024").unwrap()), "29/02/2024");
    }
}
