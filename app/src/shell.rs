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
            Sub(
                "Zoom",
                &[
                    CmdWith("50%", "view.zoom", "", r#"{"percent":50}"#),
                    CmdWith("75%", "view.zoom", "", r#"{"percent":75}"#),
                    CmdWith("90%", "view.zoom", "", r#"{"percent":90}"#),
                    CmdWith("100%", "view.zoom", "", r#"{"percent":100}"#),
                    CmdWith("125%", "view.zoom", "", r#"{"percent":125}"#),
                    CmdWith("150%", "view.zoom", "", r#"{"percent":150}"#),
                    CmdWith("200%", "view.zoom", "", r#"{"percent":200}"#),
                ],
            ),
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
            Sub(
                "Function",
                &[
                    CmdWith("SUM", "formulas.autoSum", "", r#"{"function":"SUM"}"#),
                    CmdWith("AVERAGE", "formulas.autoSum", "", r#"{"function":"AVERAGE"}"#),
                    CmdWith("COUNT", "formulas.autoSum", "", r#"{"function":"COUNT"}"#),
                    CmdWith("MAX", "formulas.autoSum", "", r#"{"function":"MAX"}"#),
                    CmdWith("MIN", "formulas.autoSum", "", r#"{"function":"MIN"}"#),
                    Sep,
                    Cmd("All functions…", "formulas.insertFunction", ""),
                ],
            ),
            Cmd("Link", "insert.link", "Ctrl+K"),
            Sep,
            Cmd("Tick box", "insert.checkbox", ""),
            Cmd("Drop-down", "data.validation", ""),
            Sep,
            Cmd("Comment", "review.newComment", "Ctrl+Alt+M"),
            Cmd("Note", "review.newNote", "Shift+F2"),
        ],
    ),
    (
        "Format",
        &[
            Sub(
                "Number",
                &[
                    CmdWith("Automatic", "home.numberFormat", "", r#"{"format":"General"}"#),
                    CmdWith("Plain text", "home.numberFormat", "", r#"{"format":"Text"}"#),
                    Sep,
                    CmdWith("Number", "home.numberFormat", "1,000.12", r##"{"code":"#,##0.00"}"##),
                    CmdWith("Percent", "home.numberFormat", "10.12%", r#"{"code":"0.00%"}"#),
                    CmdWith("Scientific", "home.numberFormat", "1.01E+03", r#"{"code":"0.00E+00"}"#),
                    Sep,
                    CmdWith("Accounting", "home.numberFormat", "£ (1,000.12)", r#"{"format":"Accounting"}"#),
                    CmdWith("Financial", "home.numberFormat", "(1,000.12)", r##"{"code":"#,##0.00;(#,##0.00)"}"##),
                    CmdWith("Currency", "home.numberFormat", "£1,000.12", r##"{"code":"£#,##0.00"}"##),
                    CmdWith("Currency rounded", "home.numberFormat", "£1,000", r##"{"code":"£#,##0"}"##),
                    Sep,
                    CmdWith("Date", "home.numberFormat", "26/09/2008", r#"{"code":"dd/mm/yyyy"}"#),
                    CmdWith("Time", "home.numberFormat", "15:59:00", r#"{"code":"hh:mm:ss"}"#),
                    CmdWith("Date time", "home.numberFormat", "26/09/2008 15:59:00", r#"{"code":"dd/mm/yyyy hh:mm:ss"}"#),
                    CmdWith("Duration", "home.numberFormat", "24:01:00", r#"{"code":"[h]:mm:ss"}"#),
                    Sep,
                    Cmd("Custom number format", "home.formatCells", ""),
                ],
            ),
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
            Sub("Wrapping", &[CmdWith("Overflow", "home.wrapText", "", r#"{"on":false}"#), CmdWith("Wrap", "home.wrapText", "", r#"{"on":true}"#)]),
            Sub(
                "Rotation",
                &[
                    CmdWith("None", "home.orientation", "", r#"{"angle":0}"#),
                    CmdWith("Tilt up", "home.orientation", "", r#"{"angle":45}"#),
                    CmdWith("Tilt down", "home.orientation", "", r#"{"angle":-45}"#),
                    CmdWith("Stack vertically", "home.orientation", "", r#"{"angle":"vertical"}"#),
                    CmdWith("Rotate up", "home.orientation", "", r#"{"angle":90}"#),
                    CmdWith("Rotate down", "home.orientation", "", r#"{"angle":-90}"#),
                ],
            ),
            Sep,
            Sub(
                "Font size",
                &[
                    CmdWith("6", "home.fontSize", "", r#"{"size":6}"#),
                    CmdWith("8", "home.fontSize", "", r#"{"size":8}"#),
                    CmdWith("9", "home.fontSize", "", r#"{"size":9}"#),
                    CmdWith("10", "home.fontSize", "", r#"{"size":10}"#),
                    CmdWith("11", "home.fontSize", "", r#"{"size":11}"#),
                    CmdWith("12", "home.fontSize", "", r#"{"size":12}"#),
                    CmdWith("14", "home.fontSize", "", r#"{"size":14}"#),
                    CmdWith("18", "home.fontSize", "", r#"{"size":18}"#),
                    CmdWith("24", "home.fontSize", "", r#"{"size":24}"#),
                    CmdWith("36", "home.fontSize", "", r#"{"size":36}"#),
                ],
            ),
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
            Sub(
                "Sort range",
                &[
                    Cmd("Sort range by column A to Z", "data.sortAscending", ""),
                    Cmd("Sort range by column Z to A", "data.sortDescending", ""),
                    Sep,
                    Cmd("Advanced range sorting options", "data.sort", ""),
                ],
            ),
            Sep,
            Cmd("Create a filter", "data.filter", ""),
            Cmd("Add a slicer", "insert.slicer", ""),
            Sep,
            Cmd("Protect sheets and ranges", "review.protectSheet", ""),
            Cmd("Named ranges", "formulas.nameManager", ""),
            Sep,
            Cmd("Data validation", "data.validation", ""),
            Sub("Data clean-up", &[Cmd("Remove duplicates", "data.removeDuplicates", "")]),
            Cmd("Split text to columns", "data.textToColumns", ""),
        ],
    ),
    (
        "Tools",
        &[
            Sub("Spelling", &[Cmd("Spell check", "review.spelling", "")]),
            Sub("Calculation controls", &[CmdWith("Calculate now", "formulas.calculateNow", "", "{}")]),
            Sep,
            Cmd("Accessibility", "review.checkAccessibility", ""),
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
    ("Number|1,000.12", "home.numberFormat", r##"{"code":"#,##0.00"}"##),
    ("Percent|10.12%", "home.numberFormat", r#"{"code":"0.00%"}"#),
    ("Scientific|1.01E+03", "home.numberFormat", r#"{"code":"0.00E+00"}"#),
    ("-", "", ""),
    ("Accounting|£ (1,000.12)", "home.numberFormat", r#"{"format":"Accounting"}"#),
    ("Financial|(1,000.12)", "home.numberFormat", r##"{"code":"#,##0.00;(#,##0.00)"}"##),
    ("Currency|£1,000.12", "home.numberFormat", r##"{"code":"£#,##0.00"}"##),
    ("Currency rounded|£1,000", "home.numberFormat", r##"{"code":"£#,##0"}"##),
    ("-", "", ""),
    ("Date|26/09/2008", "home.numberFormat", r#"{"code":"dd/mm/yyyy"}"#),
    ("Time|15:59:00", "home.numberFormat", r#"{"code":"hh:mm:ss"}"#),
    ("Date time|26/09/2008 15:59:00", "home.numberFormat", r#"{"code":"dd/mm/yyyy hh:mm:ss"}"#),
    ("Duration|24:01:00", "home.numberFormat", r#"{"code":"[h]:mm:ss"}"#),
    ("-", "", ""),
    ("Custom number format", "home.formatCells", "{}"),
];

const BORDERS: &[(&str, &str, &str)] = &[
    ("icon:border_all|All borders", "home.borders", r#"{"preset":"all"}"#),
    ("icon:border_inner|Inner borders", "home.borders", r#"{"preset":"inside"}"#),
    ("icon:border_horizontal|Horizontal borders", "home.borders", r#"{"preset":"insideHorizontal"}"#),
    ("icon:border_vertical|Vertical borders", "home.borders", r#"{"preset":"insideVertical"}"#),
    ("icon:border_outer|Outer borders", "home.borders", r#"{"preset":"outside"}"#),
    ("icon:border_left|Left border", "home.borders", r#"{"preset":"left"}"#),
    ("icon:border_top|Top border", "home.borders", r#"{"preset":"top"}"#),
    ("icon:border_right|Right border", "home.borders", r#"{"preset":"right"}"#),
    ("icon:border_bottom|Bottom border", "home.borders", r#"{"preset":"bottom"}"#),
    ("icon:border_clear|Clear borders", "home.borders", r#"{"preset":"none"}"#),
];

const MERGE: &[(&str, &str, &str)] = &[
    ("Merge all", "home.mergeCells", "{}"),
    ("Merge horizontally", "home.mergeAcross", "{}"),
    ("Unmerge", "home.unmergeCells", "{}"),
];

const H_ALIGN: &[(&str, &str, &str)] = &[
    ("icon:format_align_left|Left", "home.alignLeft", "{}"),
    ("icon:format_align_center|Centre", "home.alignCenter", "{}"),
    ("icon:format_align_right|Right", "home.alignRight", "{}"),
];

const V_ALIGN: &[(&str, &str, &str)] = &[
    ("icon:vertical_align_top|Top", "home.alignTop", "{}"),
    ("icon:vertical_align_center|Middle", "home.alignMiddle", "{}"),
    ("icon:vertical_align_bottom|Bottom", "home.alignBottom", "{}"),
];

// Sheets also has Clip; GridCraft has no clip mode yet.
const WRAP: &[(&str, &str, &str)] = &[("icon:keyboard_tab|Overflow", "home.wrapText", r#"{"on":false}"#), ("icon:wrap_text|Wrap", "home.wrapText", r#"{"on":true}"#)];

const ROTATION: &[(&str, &str, &str)] = &[
    ("icon:text_rotation_none|None", "home.orientation", r#"{"angle":0}"#),
    ("icon:text_rotation_angleup|Tilt up", "home.orientation", r#"{"angle":45}"#),
    ("icon:text_rotation_angledown|Tilt down", "home.orientation", r#"{"angle":-45}"#),
    ("icon:text_rotate_vertical|Stack vertically", "home.orientation", r#"{"angle":"vertical"}"#),
    ("icon:text_rotate_up|Rotate up", "home.orientation", r#"{"angle":90}"#),
    ("icon:text_rotation_down|Rotate down", "home.orientation", r#"{"angle":-90}"#),
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
    /// The spreadsheet's name while it's being renamed (click the title).
    title_draft: Option<String>,
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

/// Sheets-style flat buttons for the menu bar and toolbar: no border or fill at rest, a soft
/// rounded grey on hover and while a menu is open (egui's default draws a box round each one).
fn flat(ui: &mut Ui) {
    let w = &mut ui.style_mut().visuals.widgets;
    for (state, fill) in [
        (&mut w.inactive, egui::Color32::TRANSPARENT),
        (&mut w.hovered, theme::HOVER),
        (&mut w.active, theme::PRESSED),
        (&mut w.open, theme::HOVER),
    ] {
        state.bg_stroke = Stroke::NONE;
        state.weak_bg_fill = fill;
        state.corner_radius = egui::CornerRadius::same(4);
        state.expansion = 0.0;
    }
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
                .show(ui, |ui| title_and_menus(app, ui, &mut self.title_draft));
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
                flat(ui);
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
        // Sheets keeps a thin grey box round the size, unlike the flat buttons beside it.
        let r = ui
            .scope(|ui| {
                let w = &mut ui.style_mut().visuals.widgets;
                w.inactive.bg_stroke = Stroke::new(1.0, theme::BOX);
                w.hovered.bg_stroke = Stroke::new(1.0, theme::TEXT_DIM);
                ui.add(egui::TextEdit::singleline(&mut text).desired_width(30.0).horizontal_align(Align::Center))
            })
            .inner;
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

/// A toolbar dropdown, Sheets style: a strip of icons (borders, alignment, wrapping, rotation)
/// or a list whose entries may show a grey example on the right ("Currency|£1,000.12").
fn tool_menu(app: &mut SheetApp, ui: &mut Ui, items: &[(&str, &str, &str)]) {
    let icon_row = items.iter().all(|(l, _, _)| l.starts_with("icon:"));
    let mut run = |app: &mut SheetApp, ui: &mut Ui, id: &str, p: &str| {
        app.run_or_alert(id, params(p));
        ui.close();
    };
    if icon_row {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        flat(ui);
        let per_row = if items.len() > 6 { 5 } else { items.len() };
        egui::Grid::new(ui.id().with("strip")).spacing(vec2(4.0, 4.0)).show(ui, |ui| {
            for (i, (label, id, p)) in items.iter().enumerate() {
                let (name, tip) = label.trim_start_matches("icon:").split_once('|').unwrap_or((label.trim_start_matches("icon:"), ""));
                let tip = if tip.is_empty() { name } else { tip };
                if icon_or_text(ui, name, tip).clicked() {
                    run(app, ui, id, p);
                }
                if (i + 1) % per_row == 0 {
                    ui.end_row();
                }
            }
        });
        return;
    }
    let width = menu_style(ui, false).max(if items.iter().any(|(l, _, _)| l.contains('|')) { 280.0 } else { 0.0 });
    ui.set_min_width(width);
    ui.set_max_width(width);
    for (label, id, p) in items {
        if *label == "-" {
            menu_sep_width(ui, width);
            continue;
        }
        let (name, sample) = label.split_once('|').unwrap_or((label, ""));
        if ui.add_sized([width, 32.0], menu_row(name, sample, false)).clicked() {
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

fn title_and_menus(app: &mut SheetApp, ui: &mut Ui, title_draft: &mut Option<String>) {
    let name = app.session.active().map(|d| d.display_title().to_string()).unwrap_or_else(|| "Untitled spreadsheet".into());
    let dirty = app.session.active().is_some_and(|d| d.is_dirty());
    ui.horizontal(|ui| {
        // Our own mark (a white star on a green tile; same file as the favicon).
        match icons::logo(36.0) {
            Some(img) => ui.add(img),
            None => ui.label(RichText::new("★").size(26.0).color(theme::BLUE)),
        };
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                rename_box(app, ui, &name, title_draft);
                ui.label(RichText::new(if dirty { "Not saved" } else { "Saved" }).size(12.0).color(theme::TEXT_DIM));
            });
            ui.horizontal(|ui| {
                flat(ui);
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.spacing_mut().button_padding = vec2(7.0, 2.0);
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

/// The Material Symbols icon Sheets shows beside a menu item (docs/sheets-menus.md).
pub const MENU_ICONS: &[(&str, &str)] = &[
    ("New spreadsheet", "note_add"),
    ("Open", "folder_open"),
    ("Download", "download"),
    ("Details", "info"),
    ("Settings", "settings"),
    ("Print", "print"),
    ("Undo", "undo"),
    ("Redo", "redo"),
    ("Cut", "content_cut"),
    ("Copy", "content_copy"),
    ("Paste", "content_paste"),
    ("Paste special", "content_paste"),
    ("Delete", "delete"),
    ("Find and replace", "find_replace"),
    ("Show", "visibility"),
    ("Freeze", "grid_on"),
    ("Group", "add_box"),
    ("Zoom", "zoom_in"),
    ("Cells", "check_box_outline_blank"),
    ("Rows", "table_rows"),
    ("Columns", "view_column"),
    ("Sheet", "tab"),
    ("Chart", "insert_chart"),
    ("Pivot table", "pivot_table_chart"),
    ("Image", "image"),
    ("Function", "functions"),
    ("Link", "link"),
    ("Comment", "add_comment"),
    ("Note", "sticky_note_2"),
    ("Table", "table"),
    ("Number", "numbers_123"),
    ("Text", "format_bold"),
    ("Alignment", "format_align_left"),
    ("Wrapping", "wrap_text"),
    ("Merge cells", "cell_merge"),
    ("Convert to table", "table"),
    ("Conditional formatting", "format_paint"),
    ("Clear formatting", "format_clear"),
    ("Sort range A → Z", "sort_by_alpha"),
    ("Sort range Z → A", "sort_by_alpha"),
    ("Advanced range sorting options", "sort"),
    ("Create a filter", "filter_alt"),
    ("Add a slicer", "filter_list"),
    ("Protect sheets and ranges", "lock"),
    ("Named ranges", "label"),
    ("Data validation", "rule"),
    ("Split text to columns", "call_split"),
    ("Spelling", "spellcheck"),
    ("Accessibility", "accessibility_new"),
    ("Calculate now", "calculate"),
    ("Function list", "functions"),
    ("Rotation", "text_rotation_angleup"),
    ("Font size", "format_size"),
    ("Sort range", "sort"),
    ("Data clean-up", "cleaning_services"),
    ("Calculation controls", "functions"),
    ("Tick box", "check_box"),
    // Right-click menus (cells, row and column headers, sheet tabs).
    ("Insert cells", "add"),
    ("Delete cells", "delete"),
    ("Delete row", "delete"),
    ("Delete column", "delete"),
    ("Clear row", "close"),
    ("Clear column", "close"),
    ("Hide row", "visibility_off"),
    ("Hide column", "visibility_off"),
    ("Unhide rows", "visibility"),
    ("Unhide columns", "visibility"),
    ("Resize the row", "aspect_ratio"),
    ("Resize the column", "aspect_ratio"),
    ("Sort sheet A to Z", "sort_by_alpha"),
    ("Sort sheet Z to A", "sort_by_alpha"),
    ("Insert link", "link"),
    ("Insert note", "sticky_note_2"),
    ("Drop-down", "arrow_drop_down_circle"),
    ("Filter by cell value", "filter_list"),
];

fn menu_icon(label: &str) -> Option<egui::Image<'static>> {
    let exact = MENU_ICONS.iter().find(|(l, _)| *l == label).map(|(_, n)| *n);
    // GridCraft's counted labels ("Insert 3 rows above", "Delete rows 3 - 5", patch 0015).
    let name = exact.or_else(|| {
        if label.starts_with("Insert ") && (label.contains(" row") || label.contains(" column")) {
            Some("add")
        } else if label.starts_with("Delete rows") || label.starts_with("Delete columns") {
            Some("delete")
        } else {
            None
        }
    })?;
    icons::icon(name)
}

/// Grey for shortcuts and the submenu arrow, as in Sheets.
const MENU_GREY: egui::Color32 = egui::Color32::from_rgb(0x80, 0x86, 0x8B);

/// One menu row: icon (or the same space blank when the menu has icons), 14 px label, and the
/// shortcut in grey on the right. Rows are 32 px tall and fill the menu's width.
pub fn menu_row(label: &str, keys: &str, with_icons: bool) -> Button<'static> {
    let text = RichText::new(label.to_string()).size(14.0).color(theme::TEXT);
    let b = match (menu_icon(label).filter(|_| with_icons), with_icons) {
        (Some(img), _) => Button::image_and_text(img, text),
        // Keeps the label lined up with its iconed neighbours.
        (None, true) => match icons::icon("add") {
            Some(blank) => Button::image_and_text(blank.tint(egui::Color32::TRANSPARENT), text),
            None => Button::new(text),
        },
        (None, false) => Button::new(text),
    };
    // Always add the (maybe empty) shortcut: its stretch keeps the label at the left.
    let b = b.shortcut_text(RichText::new(keys.to_string()).size(13.0).color(MENU_GREY));
    b.frame_when_inactive(false).min_size(vec2(0.0, 32.0))
}

/// Sheets-style menu: flat full-width rows with icons, grey shortcuts, ▸ for submenus, and
/// dividers with breathing room (docs/sheets-menus.md).
/// Sets a menu `Ui` up the Sheets way (flat full-width rows, grey hover) and returns its width:
/// ~320 px with icons, 220 px for plain submenus.
pub fn menu_style(ui: &mut Ui, with_icons: bool) -> f32 {
    let width = if with_icons { 320.0 } else { 220.0 };
    ui.set_min_width(width);
    ui.set_max_width(width);
    let st = ui.style_mut();
    st.spacing.button_padding = vec2(12.0, 7.0);
    st.spacing.item_spacing = vec2(14.0, 0.0);
    st.spacing.icon_width = 10.0;
    let w = &mut st.visuals.widgets;
    for (state, fill) in [(&mut w.inactive, egui::Color32::TRANSPARENT), (&mut w.hovered, theme::MENU_HOVER), (&mut w.active, theme::HOVER), (&mut w.open, theme::MENU_HOVER)] {
        state.bg_stroke = Stroke::NONE;
        state.weak_bg_fill = fill;
        state.corner_radius = egui::CornerRadius::ZERO;
        state.expansion = 0.0;
    }
    width
}

/// A Sheets menu divider: a light line with 8 px above and below.
pub fn menu_sep_width(ui: &mut Ui, width: f32) {
    ui.add_space(8.0);
    let (r, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, theme::DIVIDER));
    ui.add_space(8.0);
}

/// GridCraft's cell and header menus drawn with our rows (patch 0015).
pub const MENU_LOOK: gridcraft_ui_egui::MenuLook = gridcraft_ui_egui::MenuLook {
    style: |ui| menu_style(ui, true),
    row: |label, keys| menu_row(label, keys, true),
    sep: |ui| menu_sep_width(ui, 320.0),
};

/// The spreadsheet's name: click it to rename, like Sheets (Enter keeps, Esc cancels).
fn rename_box(app: &mut SheetApp, ui: &mut Ui, name: &str, draft: &mut Option<String>) {
    let Some(text) = draft.as_mut() else {
        let r = ui.add(Button::new(RichText::new(name).size(18.0).color(theme::TEXT)).frame_when_inactive(false)).on_hover_text("Rename");
        if r.clicked() {
            *draft = Some(name.to_string());
        }
        return;
    };
    let r = ui.add(egui::TextEdit::singleline(text).font(egui::FontId::proportional(18.0)).desired_width(260.0));
    if !r.has_focus() && !r.lost_focus() {
        r.request_focus();
    }
    if r.lost_focus() {
        let new = text.trim().to_string();
        if !ui.input(|i| i.key_pressed(egui::Key::Escape)) && !new.is_empty() && new != name {
            rename_doc(app, &new);
        }
        *draft = None;
    }
}

/// Renames the active spreadsheet: a file keeps its extension (the download uses the new name).
fn rename_doc(app: &mut SheetApp, new: &str) {
    let Some(d) = app.session.active_mut() else { return };
    match d.path.as_deref() {
        Some(p) => {
            let ext = std::path::Path::new(p).extension().map(|e| e.to_string_lossy().to_string());
            let has_ext = std::path::Path::new(new).extension().is_some();
            d.path = Some(match ext {
                Some(e) if !has_ext => format!("{new}.{e}"),
                _ => new.to_string(),
            });
        }
        None => d.title = new.to_string(),
    }
}

fn menu_items(app: &mut SheetApp, ui: &mut Ui, items: &[Item]) {
    let with_icons = items.iter().any(|i| match i {
        Cmd(l, ..) | CmdWith(l, ..) | Sub(l, _) => menu_icon(l).is_some(),
        Sep => false,
    });
    let width = menu_style(ui, with_icons);
    for item in items {
        match item {
            Cmd(label, id, keys) => {
                if ui.add_sized([width, 32.0], menu_row(label, keys, with_icons)).clicked() {
                    app.run_or_alert(id, json!({}));
                    ui.close();
                }
            }
            CmdWith(label, id, keys, params) => {
                if ui.add_sized([width, 32.0], menu_row(label, keys, with_icons)).clicked() {
                    app.run_or_alert(id, serde_json::from_str(params).unwrap_or_else(|_| json!({})));
                    ui.close();
                }
            }
            Sub(label, sub) => {
                let b = menu_row(label, "", with_icons).min_size(vec2(width, 32.0)).right_text(egui::containers::menu::SubMenuButton::arrow_atom(Some(MENU_GREY)));
                egui::containers::menu::SubMenuButton::from_button(b).ui(ui, |ui| menu_items(app, ui, sub));
            }
            Sep => menu_sep_width(ui, width),
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
    fn every_menu_icon_is_embedded() {
        for (label, name) in MENU_ICONS {
            assert!(icons::has(name), "menu icon {name} for {label} isn't in app/icons");
        }
    }

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
                            let icon = icon.split('|').next().unwrap_or(icon);
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
