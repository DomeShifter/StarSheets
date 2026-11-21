//! StarSheets' own chrome: the title row, the menu bar and the toolbar, drawn above GridCraft's
//! formula bar and grid. Menus and toolbar are data tables (`MENUS`, `TOOLBAR`): adding an item is
//! one line naming its label and the GridCraft command it runs (`Session::execute` ids).
//! Target layout: docs/sheets-reference.md §1-3.

use egui::{Align, Button, Frame, Layout, Margin, RichText, Sense, Stroke, Ui, vec2};
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

use crate::theme;

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
    /// Glyph or short text, tooltip, command id.
    Btn(&'static str, &'static str, &'static str),
    Gap,
}
use Tool::{Btn, Gap};

/// The toolbar, left to right (docs/sheets-reference.md §3). Phase 0 uses text glyphs; phase 1
/// swaps in Material Symbols icons.
pub const TOOLBAR: &[Tool] = &[
    Btn("↶", "Undo (Ctrl+Z)", "edit.undo"),
    Btn("↷", "Redo (Ctrl+Y)", "edit.redo"),
    Btn("⎙", "Print (Ctrl+P)", "file.print"),
    Btn("🖌", "Paint format", "edit.formatPainter"),
    Gap,
    Btn("£", "Format as currency", "home.accounting"),
    Btn("%", "Format as percent", "home.percent"),
    Btn(".0", "Decrease decimal places", "home.decreaseDecimal"),
    Btn(".00", "Increase decimal places", "home.increaseDecimal"),
    Btn("123", "More formats", "home.formatCells"),
    Gap,
    Btn("−", "Decrease font size", "home.decreaseFontSize"),
    Btn("+", "Increase font size", "home.increaseFontSize"),
    Gap,
    Btn("B", "Bold (Ctrl+B)", "home.bold"),
    Btn("I", "Italic (Ctrl+I)", "home.italic"),
    Btn("S", "Strikethrough (Alt+Shift+5)", "home.strikethrough"),
    Btn("A", "Text colour", "home.fontColor"),
    Gap,
    Btn("◧", "Fill colour", "home.fillColor"),
    Btn("▦", "Borders", "home.borders"),
    Btn("⊞", "Merge cells", "home.mergeCells"),
    Gap,
    Btn("≡", "Horizontal align: left", "home.alignLeft"),
    Btn("≣", "Horizontal align: centre", "home.alignCenter"),
    Btn("↵", "Text wrapping", "home.wrapText"),
    Gap,
    Btn("🔗", "Insert link (Ctrl+K)", "insert.link"),
    Btn("💬", "Insert comment (Ctrl+Alt+M)", "review.newComment"),
    Btn("📊", "Insert chart", "insert.chart"),
    Btn("⏷", "Create a filter", "data.filter"),
    Btn("Σ", "Functions", "formulas.autoSum"),
];

/// Per-session state of the chrome.
#[derive(Default)]
pub struct Shell {
    /// "Hide the menus" (Ctrl+Shift+F in Sheets): only the toolbar shows.
    pub compact: bool,
}

impl Shell {
    pub fn ui(&mut self, app: &mut SheetApp, ui: &mut Ui) {
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
        Frame::NONE.fill(theme::TOOLBAR).corner_radius(24.0).inner_margin(Margin::symmetric(12, 4)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for tool in TOOLBAR {
                    match tool {
                        Btn(glyph, tip, id) => {
                            let text = RichText::new(*glyph).size(15.0).color(theme::TEXT_DIM);
                            let text = if *glyph == "B" { text.strong() } else if *glyph == "I" { text.italics() } else if *glyph == "S" { text.strikethrough() } else { text };
                            let b = ui.add(Button::new(text).frame(false).min_size(vec2(28.0, 28.0))).on_hover_text(*tip);
                            if b.clicked() {
                                app.run_or_alert(id, json!({}));
                            }
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
                    let label = if self.compact { "⌄" } else { "⌃" };
                    if ui.add(Button::new(RichText::new(label).size(15.0)).frame(false)).on_hover_text("Hide the menus (Ctrl+Shift+F)").clicked() {
                        self.compact = !self.compact;
                    }
                });
            });
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
            if let Btn(_, _, id) = tool {
                all.push(id);
            }
        }
        let missing: Vec<_> = all.into_iter().filter(|id| gridcraft_engine::find_command(id).is_none()).collect();
        assert!(missing.is_empty(), "unknown command ids: {missing:?}");
    }
}
