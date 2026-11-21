//! StarSheets' own chrome: the title row, the menu bar and the toolbar, drawn above GridCraft's
//! formula bar and grid. Menus and toolbar are data tables (`MENUS`, `TOOLBAR`): adding an item is
//! one line naming its label and the GridCraft command it runs (`Session::execute` ids).
//! Target layout: docs/sheets-reference.md §1-3.

use egui::{Align, Button, Frame, Layout, Margin, RichText, Sense, Stroke, Ui, vec2};
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

use crate::{icons, theme};

/// One menu entry.
pub enum Item {
    /// Label, command id, shortcut hint shown on the right.
    Cmd(&'static str, &'static str, &'static str),
    /// A command that takes fixed parameters (JSON text).
    CmdWith(&'static str, &'static str, &'static str, &'static str),
    /// A submenu.
    Sub(&'static str, &'static [Item]),
    Sep,
}
use Item::{Cmd, CmdWith, Sep, Sub};

/// The menu bar: Sheets' order and wording, items limited to commands GridCraft already has.
pub const MENUS: &[(&str, &[Item])] = &[
    (
        "File",
        &[
            Cmd("New spreadsheet", "file.new", ""),
            Cmd("Open", "file.open", "Ctrl+O"),
            Sep,
            Sub(
                "Download",
                &[
                    Cmd("Microsoft Excel (.xlsx)", "file.saveAs", ""),
                    Cmd("PDF (.pdf)", "file.exportPdf", ""),
                    Cmd("Web page (.html)", "file.exportHtml", ""),
                    Cmd("Comma-separated values (.csv)", "file.exportCsv", ""),
                ],
            ),
            Sep,
            Cmd("Details", "file.properties", ""),
            Cmd("Settings", "file.options", ""),
            Sep,
            Cmd("Print", "file.print", "Ctrl+P"),
        ],
    ),
    (
        "Edit",
        &[
            Cmd("Undo", "edit.undo", "Ctrl+Z"),
            Cmd("Redo", "edit.redo", "Ctrl+Y"),
            Sep,
            Cmd("Cut", "edit.cut", "Ctrl+X"),
            Cmd("Copy", "edit.copy", "Ctrl+C"),
            Cmd("Paste", "edit.paste", "Ctrl+V"),
            Cmd("Paste special", "edit.pasteSpecial", "Ctrl+Shift+V"),
            Sep,
            Sub(
                "Delete",
                &[
                    Cmd("Values", "edit.clearContents", "Delete"),
                    Cmd("Row", "home.deleteRows", ""),
                    Cmd("Column", "home.deleteColumns", ""),
                    Cmd("Cells and shift", "home.deleteCells", ""),
                ],
            ),
            Sep,
            Cmd("Find and replace", "edit.replace", "Ctrl+H"),
        ],
    ),
    (
        "View",
        &[
            Sub(
                "Show",
                &[
                    Cmd("Gridlines", "view.gridlines", ""),
                    Cmd("Formula bar", "view.formulaBar", ""),
                    Cmd("Formulas", "formulas.showFormulas", "Ctrl+~"),
                ],
            ),
            Sub(
                "Freeze",
                &[
                    Cmd("No rows or columns", "view.unfreezePanes", ""),
                    Cmd("1 row", "view.freezeTopRow", ""),
                    Cmd("1 column", "view.freezeFirstColumn", ""),
                    Cmd("Up to current cell", "view.freezePanes", ""),
                ],
            ),
            Sub("Group", &[Cmd("Group", "data.group", "Alt+Shift+→"), Cmd("Ungroup", "data.ungroup", "Alt+Shift+←")]),
            Sep,
            Cmd("Zoom", "view.zoom", ""),
        ],
    ),
    (
        "Insert",
        &[
            Sub("Cells", &[Cmd("Insert cells", "home.insertCells", "")]),
            Sub("Rows", &[Cmd("Insert row above", "home.insertRows", "Ctrl+Alt+=")]),
            Sub("Columns", &[Cmd("Insert column left", "home.insertColumns", "Ctrl+Alt+=")]),
            Cmd("Sheet", "home.insertSheet", "Shift+F11"),
            Sep,
            Cmd("Chart", "insert.chart", ""),
            Cmd("Pivot table", "insert.pivotTable", ""),
            Cmd("Image", "insert.picture", ""),
            Sep,
            Cmd("Function", "formulas.insertFunction", ""),
            Cmd("Link", "insert.link", "Ctrl+K"),
            Cmd("Checkbox", "insert.checkbox", ""),
            Cmd("Dropdown", "data.validation", ""),
            Sep,
            Cmd("Comment", "review.newComment", "Ctrl+Alt+M"),
            Cmd("Note", "review.newNote", "Shift+F2"),
            Cmd("Table", "insert.table", "Ctrl+Alt+T"),
        ],
    ),
    (
        "Format",
        &[
            Cmd("Number", "home.formatCells", ""),
            Sub(
                "Text",
                &[
                    Cmd("Bold", "home.bold", "Ctrl+B"),
                    Cmd("Italic", "home.italic", "Ctrl+I"),
                    Cmd("Underline", "home.underline", "Ctrl+U"),
                    Cmd("Strikethrough", "home.strikethrough", "Alt+Shift+5"),
                ],
            ),
            Sub(
                "Alignment",
                &[
                    Cmd("Left", "home.alignLeft", "Ctrl+Shift+L"),
                    Cmd("Centre", "home.alignCenter", "Ctrl+Shift+E"),
                    Cmd("Right", "home.alignRight", "Ctrl+Shift+R"),
                    Sep,
                    Cmd("Top", "home.alignTop", ""),
                    Cmd("Middle", "home.alignMiddle", ""),
                    Cmd("Bottom", "home.alignBottom", ""),
                ],
            ),
            Sub("Wrapping", &[Cmd("Wrap", "home.wrapText", "")]),
            Sep,
            Sub("Merge cells", &[Cmd("Merge all", "home.mergeCells", ""), Cmd("Merge horizontally", "home.mergeAcross", ""), Cmd("Unmerge", "home.unmergeCells", "")]),
            Cmd("Convert to table", "insert.table", "Ctrl+Alt+T"),
            Cmd("Conditional formatting", "home.conditionalFormat", ""),
            Sep,
            Cmd("Clear formatting", "edit.clearFormats", "Ctrl+\\"),
        ],
    ),
    (
        "Data",
        &[
            Cmd("Sort range A → Z", "data.sortAscending", ""),
            Cmd("Sort range Z → A", "data.sortDescending", ""),
            Cmd("Advanced range sorting options", "data.sort", ""),
            Sep,
            Cmd("Create a filter", "data.filter", ""),
            Cmd("Add a slicer", "insert.slicer", ""),
            Sep,
            Cmd("Protect sheets and ranges", "review.protectSheet", ""),
            Cmd("Named ranges", "formulas.nameManager", ""),
            Sep,
            Cmd("Data validation", "data.validation", ""),
            Sub("Data cleanup", &[Cmd("Remove duplicates", "data.removeDuplicates", "")]),
            Cmd("Split text to columns", "data.textToColumns", ""),
        ],
    ),
    (
        "Tools",
        &[
            Cmd("Spelling", "review.spelling", ""),
            Cmd("Accessibility", "review.checkAccessibility", ""),
            Sep,
            CmdWith("Calculate now", "formulas.calculateNow", "", "{}"),
        ],
    ),
    ("Help", &[Cmd("Function list", "formulas.insertFunction", "")]),
];

/// One toolbar control.
pub enum Tool {
    /// Icon name, tooltip, command id, parameters (JSON text).
    Btn(&'static str, &'static str, &'static str, &'static str),
    /// Text instead of an icon (e.g. "£", "123"), tooltip, command id, parameters.
    Txt(&'static str, &'static str, &'static str, &'static str),
    /// Icon (or text when no icon of that name), tooltip, items: (label or icon:<name>, command id, parameters).
    Menu(&'static str, &'static str, &'static [(&'static str, &'static str, &'static str)]),
    /// Icon, tooltip, command id taking `color`, the "reset" label and value.
    Colour(&'static str, &'static str, &'static str, &'static str, &'static str),
    Font,
    FontSize,
    Zoom,
    Gap,
}
use Tool::{Btn, Colour, Font, FontSize, Gap, Menu, Txt, Zoom};

/// The toolbar, left to right (docs/sheets-reference.md §3).
pub const TOOLBAR: &[Tool] = &[
    Btn("undo", "Undo (Ctrl+Z)", "edit.undo", "{}"),
    Btn("redo", "Redo (Ctrl+Y)", "edit.redo", "{}"),
    Btn("print", "Print (Ctrl+P)", "file.print", "{}"),
    Btn("format_paint", "Paint format", "edit.formatPainter", "{}"),
    Zoom,
    Gap,
    Txt("£", "Format as currency", "home.numberFormat", r#"{"code":"£#,##0.00"}"#),
    Btn("percent", "Format as percent", "home.numberFormat", r#"{"code":"0.00%"}"#),
    Btn("decimal_decrease", "Decrease decimal places", "home.decreaseDecimal", "{}"),
    Btn("decimal_increase", "Increase decimal places", "home.increaseDecimal", "{}"),
    Menu("123", "More formats", NUMBER_FORMATS),
    Gap,
    Font,
    Gap,
    FontSize,
    Gap,
    Btn("format_bold", "Bold (Ctrl+B)", "home.bold", "{}"),
    Btn("format_italic", "Italic (Ctrl+I)", "home.italic", "{}"),
    Btn("strikethrough_s", "Strikethrough (Alt+Shift+5)", "home.strikethrough", "{}"),
    Colour("format_color_text", "Text colour", "home.fontColor", "Reset", "auto"),
    Gap,
    Colour("format_color_fill", "Fill colour", "home.fillColor", "Reset", "none"),
    Menu("border_all", "Borders", BORDERS),
    Menu("cell_merge", "Merge cells", MERGE),
    Gap,
    Menu("format_align_left", "Horizontal align", H_ALIGN),
    Menu("vertical_align_bottom", "Vertical align", V_ALIGN),
    Menu("wrap_text", "Text wrapping", WRAP),
    Menu("text_rotation_angleup", "Text rotation", ROTATION),
    Gap,
    Btn("link", "Insert link (Ctrl+K)", "insert.link", "{}"),
    Btn("add_comment", "Insert comment (Ctrl+Alt+M)", "review.newComment", "{}"),
    Btn("insert_chart", "Insert chart", "insert.chart", "{}"),
    Btn("filter_alt", "Create a filter", "data.filter", "{}"),
    Menu("functions", "Functions", FUNCTIONS),
];

const NUMBER_FORMATS: &[(&str, &str, &str)] = &[
    ("Automatic", "home.numberFormat", r#"{"format":"General"}"#),
    ("Plain text", "home.numberFormat", r#"{"format":"Text"}"#),
    ("-", "", ""),
    ("Number           1,000.12", "home.numberFormat", r##"{"code":"#,##0.00"}"##),
    ("Percent          10.12%", "home.numberFormat", r#"{"code":"0.00%"}"#),
    ("Scientific       1.01E+03", "home.numberFormat", r#"{"code":"0.00E+00"}"#),
    ("-", "", ""),
    ("Accounting       £ (1,000.12)", "home.numberFormat", r#"{"format":"Accounting"}"#),
    ("Financial        (1,000.12)", "home.numberFormat", r##"{"code":"#,##0.00;(#,##0.00)"}"##),
    ("Currency         £1,000.12", "home.numberFormat", r##"{"code":"£#,##0.00"}"##),
    ("Currency rounded £1,000", "home.numberFormat", r##"{"code":"£#,##0"}"##),
    ("-", "", ""),
    ("Date             26/09/2008", "home.numberFormat", r#"{"code":"dd/mm/yyyy"}"#),
    ("Time             15:59:00", "home.numberFormat", r#"{"code":"hh:mm:ss"}"#),
    ("Date time        26/09/2008 15:59:00", "home.numberFormat", r#"{"code":"dd/mm/yyyy hh:mm:ss"}"#),
    ("Duration         24:01:00", "home.numberFormat", r#"{"code":"[h]:mm:ss"}"#),
    ("-", "", ""),
    ("Custom number format", "home.formatCells", "{}"),
];

const BORDERS: &[(&str, &str, &str)] = &[
    ("icon:border_all", "home.borders", r#"{"preset":"all"}"#),
    ("icon:border_inner", "home.borders", r#"{"preset":"inside"}"#),
    ("icon:border_horizontal", "home.borders", r#"{"preset":"insideHorizontal"}"#),
    ("icon:border_vertical", "home.borders", r#"{"preset":"insideVertical"}"#),
    ("icon:border_outer", "home.borders", r#"{"preset":"outside"}"#),
    ("icon:border_left", "home.borders", r#"{"preset":"left"}"#),
    ("icon:border_top", "home.borders", r#"{"preset":"top"}"#),
    ("icon:border_right", "home.borders", r#"{"preset":"right"}"#),
    ("icon:border_bottom", "home.borders", r#"{"preset":"bottom"}"#),
    ("icon:border_clear", "home.borders", r#"{"preset":"none"}"#),
];

const MERGE: &[(&str, &str, &str)] = &[
    ("Merge all", "home.mergeCells", "{}"),
    ("Merge horizontally", "home.mergeAcross", "{}"),
    ("Unmerge", "home.unmergeCells", "{}"),
];

const H_ALIGN: &[(&str, &str, &str)] = &[
    ("icon:format_align_left", "home.alignLeft", "{}"),
    ("icon:format_align_center", "home.alignCenter", "{}"),
    ("icon:format_align_right", "home.alignRight", "{}"),
];

const V_ALIGN: &[(&str, &str, &str)] = &[
    ("icon:vertical_align_top", "home.alignTop", "{}"),
    ("icon:vertical_align_center", "home.alignMiddle", "{}"),
    ("icon:vertical_align_bottom", "home.alignBottom", "{}"),
];

const WRAP: &[(&str, &str, &str)] = &[("Overflow", "home.wrapText", r#"{"on":false}"#), ("Wrap", "home.wrapText", r#"{"on":true}"#)];

const ROTATION: &[(&str, &str, &str)] = &[
    ("None", "home.orientation", r#"{"angle":0}"#),
    ("Tilt up", "home.orientation", r#"{"angle":45}"#),
    ("Tilt down", "home.orientation", r#"{"angle":-45}"#),
    ("Stack vertically", "home.orientation", r#"{"angle":"vertical"}"#),
    ("Rotate up", "home.orientation", r#"{"angle":90}"#),
    ("Rotate down", "home.orientation", r#"{"angle":-90}"#),
];

const FUNCTIONS: &[(&str, &str, &str)] = &[
    ("SUM", "formulas.autoSum", r#"{"function":"SUM"}"#),
    ("AVERAGE", "formulas.autoSum", r#"{"function":"AVERAGE"}"#),
    ("COUNT", "formulas.autoSum", r#"{"function":"COUNT"}"#),
    ("MAX", "formulas.autoSum", r#"{"function":"MAX"}"#),
    ("MIN", "formulas.autoSum", r#"{"function":"MIN"}"#),
    ("-", "", ""),
    ("More functions…", "formulas.insertFunction", "{}"),
];

/// The font list, Sheets-like with Roboto first (Roboto is bundled; sans-serif names render in it
/// until more faces are bundled, serif and monospace names use built-in faces).
const FONTS: &[&str] = &["Roboto", "Arial", "Calibri", "Comic Sans MS", "Courier New", "Georgia", "Times New Roman", "Trebuchet MS", "Verdana"];

const FONT_SIZES: &[u32] = &[6, 7, 8, 9, 10, 11, 12, 14, 18, 24, 36];
const ZOOMS: &[u32] = &[50, 75, 90, 100, 125, 150, 200];

/// Google's classic colour palette (greys, brights, then six rows of tints and shades).
const PALETTE: [[u32; 10]; 8] = [
    [0x000000, 0x434343, 0x666666, 0x999999, 0xB7B7B7, 0xCCCCCC, 0xD9D9D9, 0xEFEFEF, 0xF3F3F3, 0xFFFFFF],
    [0x980000, 0xFF0000, 0xFF9900, 0xFFFF00, 0x00FF00, 0x00FFFF, 0x4A86E8, 0x0000FF, 0x9900FF, 0xFF00FF],
    [0xE6B8AF, 0xF4CCCC, 0xFCE5CD, 0xFFF2CC, 0xD9EAD3, 0xD0E0E3, 0xC9DAF8, 0xCFE2F3, 0xD9D2E9, 0xEAD1DC],
    [0xDD7E6B, 0xEA9999, 0xF9CB9C, 0xFFE599, 0xB6D7A8, 0xA2C4C9, 0xA4C2F4, 0x9FC5E8, 0xB4A7D6, 0xD5A6BD],
    [0xCC4125, 0xE06666, 0xF6B26B, 0xFFD966, 0x93C47D, 0x76A5AF, 0x6D9EEB, 0x6FA8DC, 0x8E7CC3, 0xC27BA0],
    [0xA61C00, 0xCC0000, 0xE69138, 0xF1C232, 0x6AA84F, 0x45818E, 0x3C78D8, 0x3D85C6, 0x674EA7, 0xA64D79],
    [0x85200C, 0x990000, 0xB45F06, 0xBF9000, 0x38761D, 0x134F5C, 0x1155CC, 0x0B5394, 0x351C75, 0x741B47],
    [0x5B0F00, 0x660000, 0x783F04, 0x7F6000, 0x274E13, 0x0C343D, 0x1C4587, 0x073763, 0x20124D, 0x4C1130],
];

/// Per-session state of the chrome.
#[derive(Default)]
pub struct Shell {
    /// "Hide the menus" (Ctrl+Shift+F in Sheets): only the toolbar shows.
    pub compact: bool,
    /// The font size box while it's being typed in.
    size_draft: Option<String>,
}

/// What the toolbar shows for the active cell.
struct Current {
    font: String,
    size: f32,
    zoom: u16,
}

fn current(app: &SheetApp) -> Current {
    let d = app.session.active();
    let style = d.and_then(|d| {
        let sh = d.wb.active()?;
        Some(d.wb.styles.get(sh.style_id(d.selection.active)).clone())
    });
    let zoom = d.and_then(|d| d.wb.active().map(|sh| sh.zoom)).unwrap_or(100);
    match style {
        Some(st) => Current { font: st.font.name.clone(), size: st.font.size, zoom },
        None => Current { font: "Arial".into(), size: 10.0, zoom },
    }
}

fn params(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|_| json!({}))
}

fn icon_or_text(ui: &mut Ui, name: &str, tip: &str) -> egui::Response {
    match icons::icon(name) {
        Some(img) => ui.add(Button::image(img).frame(false).min_size(vec2(28.0, 28.0))),
        None => ui.add(Button::new(RichText::new(name).size(14.0).color(theme::TEXT_DIM)).frame(false).min_size(vec2(28.0, 28.0))),
    }
    .on_hover_text(tip)
}

impl Shell {
    pub fn ui(&mut self, app: &mut SheetApp, ui: &mut Ui) {
        // Ctrl+Shift+F: hide/show the menus (taken before the grid sees it).
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::F)) {
            self.compact = !self.compact;
        }
        if !self.compact {
            egui::Panel::top("starsheets_title")
                .exact_size(64.0)
                .frame(Frame::NONE.fill(theme::PAGE).inner_margin(Margin { left: 12, right: 16, top: 6, bottom: 0 }))
                .show(ui, |ui| title_and_menus(app, ui));
        }
        egui::Panel::top("starsheets_toolbar")
            .exact_size(48.0)
            .frame(Frame::NONE.fill(theme::PAGE).inner_margin(Margin { left: 12, right: 12, top: 4, bottom: 6 }))
            .show(ui, |ui| self.toolbar(app, ui));
    }

    fn toolbar(&mut self, app: &mut SheetApp, ui: &mut Ui) {
        let cur = current(app);
        Frame::NONE.fill(theme::TOOLBAR).corner_radius(24.0).inner_margin(Margin::symmetric(12, 4)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for tool in TOOLBAR {
                    match tool {
                        Btn(icon, tip, id, p) => {
                            if icon_or_text(ui, icon, tip).clicked() {
                                app.run_or_alert(id, params(p));
                            }
                        }
                        Txt(text, tip, id, p) => {
                            let b = ui.add(Button::new(RichText::new(*text).size(15.0).color(theme::TEXT_DIM)).frame(false).min_size(vec2(28.0, 28.0)));
                            if b.on_hover_text(*tip).clicked() {
                                app.run_or_alert(id, params(p));
                            }
                        }
                        Menu(icon, tip, items) => {
                            let r = match icons::icon(icon) {
                                Some(img) => ui.menu_button(img, |ui| tool_menu(app, ui, items)).response,
                                None => ui.menu_button(RichText::new(*icon).size(14.0).color(theme::TEXT_DIM), |ui| tool_menu(app, ui, items)).response,
                            };
                            r.on_hover_text(*tip);
                        }
                        Colour(icon, tip, id, reset_label, reset_value) => {
                            let img = icons::icon(icon);
                            let body = |ui: &mut Ui| colour_menu(app, ui, id, reset_label, reset_value);
                            let r = match img {
                                Some(img) => ui.menu_button(img, body).response,
                                None => ui.menu_button(*icon, body).response,
                            };
                            r.on_hover_text(*tip);
                        }
                        Font => {
                            let label = RichText::new(format!("{}  ▾", cur.font)).size(13.0).color(theme::TEXT);
                            ui.menu_button(label, |ui| {
                                ui.set_min_width(200.0);
                                for f in FONTS {
                                    if ui.add(Button::new(RichText::new(*f).size(14.0)).selected(*f == cur.font)).clicked() {
                                        app.run_or_alert("home.fontName", json!({ "name": f }));
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Font");
                        }
                        FontSize => self.font_size(app, ui, cur.size),
                        Zoom => {
                            ui.menu_button(RichText::new(format!("{}%  ▾", cur.zoom)).size(13.0).color(theme::TEXT), |ui| {
                                for z in ZOOMS {
                                    if ui.add(Button::new(format!("{z}%")).selected(u32::from(cur.zoom) == *z)).clicked() {
                                        app.run_or_alert("view.zoom", json!({ "percent": z }));
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Zoom");
                        }
                        Gap => {
                            ui.add_space(4.0);
                            let (r, _) = ui.allocate_exact_size(vec2(1.0, 20.0), Sense::hover());
                            ui.painter().vline(r.center().x, r.y_range(), Stroke::new(1.0, theme::HOVER));
                            ui.add_space(4.0);
                        }
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (icon, tip) = if self.compact { ("keyboard_arrow_down", "Show the menus (Ctrl+Shift+F)") } else { ("keyboard_arrow_up", "Hide the menus (Ctrl+Shift+F)") };
                    if icon_or_text(ui, icon, tip).clicked() {
                        self.compact = !self.compact;
                    }
                });
            });
        });
    }

    /// Sheets' size box: − [10] +, type a size and press Enter; the arrow lists common sizes.
    fn font_size(&mut self, app: &mut SheetApp, ui: &mut Ui, size: f32) {
        let shown = if size.fract() == 0.0 { format!("{}", size as i32) } else { format!("{size}") };
        if icon_or_text(ui, "remove", "Decrease font size").clicked() {
            app.run_or_alert("home.decreaseFontSize", json!({}));
        }
        let mut text = self.size_draft.clone().unwrap_or_else(|| shown.clone());
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(30.0).horizontal_align(Align::Center));
        if r.changed() {
            self.size_draft = Some(text.clone());
        }
        if r.lost_focus() {
            if ui.input(|i| i.key_pressed(egui::Key::Enter))
                && let Ok(v) = text.trim().parse::<f64>()
            {
                app.run_or_alert("home.fontSize", json!({ "size": v }));
            }
            self.size_draft = None;
        }
        r.on_hover_text("Font size");
        ui.menu_button("▾", |ui| {
            for s in FONT_SIZES {
                if ui.button(s.to_string()).clicked() {
                    app.run_or_alert("home.fontSize", json!({ "size": s }));
                    ui.close();
                }
            }
        });
        if icon_or_text(ui, "add", "Increase font size").clicked() {
            app.run_or_alert("home.increaseFontSize", json!({}));
        }
    }
}

fn tool_menu(app: &mut SheetApp, ui: &mut Ui, items: &[(&str, &str, &str)]) {
    let icon_row = items.iter().all(|(l, _, _)| l.starts_with("icon:"));
    let mut run = |app: &mut SheetApp, ui: &mut Ui, id: &str, p: &str| {
        app.run_or_alert(id, params(p));
        ui.close();
    };
    if icon_row {
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(5.0 * 32.0);
            for (label, id, p) in items {
                let name = label.trim_start_matches("icon:");
                if icon_or_text(ui, name, name).clicked() {
                    run(app, ui, id, p);
                }
            }
        });
        return;
    }
    ui.set_min_width(220.0);
    for (label, id, p) in items {
        if *label == "-" {
            ui.separator();
        } else if ui.button(RichText::new(*label).monospace()).clicked() {
            run(app, ui, id, p);
        }
    }
}

fn colour_menu(app: &mut SheetApp, ui: &mut Ui, id: &str, reset_label: &str, reset_value: &str) {
    if ui.button(reset_label).clicked() {
        app.run_or_alert(id, json!({ "color": reset_value }));
        ui.close();
    }
    ui.add_space(4.0);
    palette_grid(ui, |hex| app.run_or_alert(id, json!({ "color": hex })));
}

/// Sheets' colour grid; `pick` gets "#RRGGBB" and the menu closes.
pub fn palette_grid(ui: &mut Ui, mut pick: impl FnMut(String)) {
    for (i, row) in PALETTE.iter().enumerate() {
        if i == 2 {
            ui.add_space(6.0);
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
            for c in row {
                let colour = egui::Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, *c as u8);
                let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                ui.painter().circle_filled(r.center(), 8.0, colour);
                if *c == 0xFFFFFF {
                    ui.painter().circle_stroke(r.center(), 8.0, Stroke::new(1.0, theme::HOVER));
                }
                if resp.hovered() {
                    ui.painter().circle_stroke(r.center(), 9.0, Stroke::new(1.5, theme::BLUE));
                }
                if resp.on_hover_text(format!("#{c:06X}")).clicked() {
                    pick(format!("#{c:06X}"));
                    ui.close();
                }
            }
        });
    }
}

fn title_and_menus(app: &mut SheetApp, ui: &mut Ui) {
    let name = app.session.active().map(|d| d.display_title().to_string()).unwrap_or_else(|| "Untitled spreadsheet".into());
    let dirty = app.session.active().is_some_and(|d| d.is_dirty());
    ui.horizontal(|ui| {
        // App mark: a green-ish tile would copy Google's icon; ours is a star on blue.
        ui.label(RichText::new("★").size(26.0).color(theme::BLUE));
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).size(18.0).color(theme::TEXT));
                ui.label(RichText::new(if dirty { "Not saved" } else { "Saved" }).size(12.0).color(theme::TEXT_DIM));
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (title, items) in MENUS {
                    ui.menu_button(RichText::new(*title).size(14.0).color(theme::TEXT), |ui| menu_items(app, ui, items));
                }
            });
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let share = Button::new(RichText::new("Share").size(14.0).color(theme::TEXT)).fill(theme::SHARE).corner_radius(18.0).min_size(vec2(84.0, 36.0));
            ui.add_enabled(false, share).on_disabled_hover_text("Sharing arrives in phase 3");
        });
    });
}

fn menu_items(app: &mut SheetApp, ui: &mut Ui, items: &[Item]) {
    ui.set_min_width(240.0);
    for item in items {
        match item {
            Cmd(label, id, keys) => {
                if ui.add(Button::new(*label).shortcut_text(*keys)).clicked() {
                    app.run_or_alert(id, json!({}));
                    ui.close();
                }
            }
            CmdWith(label, id, keys, params) => {
                if ui.add(Button::new(*label).shortcut_text(*keys)).clicked() {
                    app.run_or_alert(id, serde_json::from_str(params).unwrap_or_else(|_| json!({})));
                    ui.close();
                }
            }
            Sub(label, sub) => {
                ui.menu_button(*label, |ui| menu_items(app, ui, sub));
            }
            Sep => {
                ui.separator();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(items: &[Item], out: &mut Vec<&'static str>) {
        for item in items {
            match item {
                Cmd(_, id, _) | CmdWith(_, id, _, _) => out.push(id),
                Sub(_, sub) => ids(sub, out),
                Sep => {}
            }
        }
    }

    /// Every menu and toolbar entry names a command GridCraft's engine knows.
    #[test]
    fn every_entry_names_a_real_command() {
        let mut all = Vec::new();
        for (_, items) in MENUS {
            ids(items, &mut all);
        }
        for tool in TOOLBAR {
            match tool {
                Btn(icon, _, id, _) => {
                    assert!(icons::has(icon), "missing icon {icon}");
                    all.push(id);
                }
                Txt(_, _, id, _) | Colour(_, _, id, _, _) => all.push(id),
                Menu(_, _, items) => {
                    for (label, id, _) in *items {
                        if let Some(icon) = label.strip_prefix("icon:") {
                            assert!(icons::has(icon), "missing icon {icon}");
                        }
                        if !id.is_empty() {
                            all.push(id);
                        }
                    }
                }
                Font | FontSize | Zoom | Gap => {}
            }
        }
        let missing: Vec<_> = all.into_iter().filter(|id| gridcraft_engine::find_command(id).is_none()).collect();
        assert!(missing.is_empty(), "unknown command ids: {missing:?}");
    }
}
