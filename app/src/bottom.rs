//! The bottom bar, Sheets style (docs/sheets-reference.md §1 row 6): **+** adds a sheet, **≡** lists
//! all sheets, then the tabs (click to switch, right-click or ▾ for the tab menu, double-click to
//! rename), and on the right the quick sum of the selection (click to choose what it shows).

use egui::{Align, Button, Frame, Layout, Margin, RichText, Stroke, Ui, vec2};
use gridcraft_engine::model::{Color, Visibility};
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

use crate::{icons, theme};

/// What the quick-sum box shows.
#[derive(Clone, Copy, Default, PartialEq)]
pub enum QuickSum {
    #[default]
    Sum,
    Average,
    Min,
    Max,
    Count,
    CountNumbers,
}

impl QuickSum {
    const ALL: [QuickSum; 6] = [QuickSum::Sum, QuickSum::Average, QuickSum::Min, QuickSum::Max, QuickSum::Count, QuickSum::CountNumbers];

    fn label(self) -> &'static str {
        match self {
            QuickSum::Sum => "Sum",
            QuickSum::Average => "Average",
            QuickSum::Min => "Min",
            QuickSum::Max => "Max",
            QuickSum::Count => "Count",
            QuickSum::CountNumbers => "Count numbers",
        }
    }
}

#[derive(Default)]
pub struct Bottom {
    quick: QuickSum,
    /// Tab being renamed: (index, draft name).
    renaming: Option<(usize, String)>,
    /// The tab being dragged to a new place (its sheet index).
    dragging: Option<usize>,
    /// This frame's tab rectangles, for dropping a dragged tab.
    tab_rects: Vec<(usize, egui::Rect)>,
}

struct Tab {
    index: usize,
    name: String,
    colour: Option<egui::Color32>,
    hidden: bool,
}

fn tabs(app: &SheetApp) -> (Vec<Tab>, usize) {
    let Some(d) = app.session.active() else { return (Vec::new(), 0) };
    let theme = &d.wb.theme;
    let list = d
        .wb
        .sheets
        .iter()
        .enumerate()
        .map(|(index, sh)| Tab {
            index,
            name: sh.name.clone(),
            colour: sh.tab_color.as_ref().and_then(|c: &Color| c.resolve(theme)).map(|[r, g, b]| egui::Color32::from_rgb(r, g, b)),
            hidden: sh.visibility != Visibility::Visible,
        })
        .collect();
    (list, d.wb.active_sheet.get())
}

fn fmt(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        // Thousands separators, Sheets style.
        let s = format!("{}", v.abs() as i64);
        let mut out = String::new();
        for (i, ch) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(ch);
        }
        if v < 0.0 { format!("-{out}") } else { out }
    } else {
        format!("{v:.2}")
    }
}

impl Bottom {
    pub fn ui(&mut self, app: &mut SheetApp, ui: &mut Ui) {
        egui::Panel::bottom("starsheets_bottom")
            .exact_size(40.0)
            .frame(Frame::NONE.fill(theme::PAGE).inner_margin(Margin { left: 8, right: 12, top: 4, bottom: 4 }).stroke(Stroke::new(1.0, theme::HOVER)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let (list, active) = tabs(app);
                    // Alt+1: rename the current sheet.
                    if app.editor.is_none()
                        && ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::Num1))
                        && let Some(tab) = list.iter().find(|t| t.index == active)
                    {
                        self.renaming = Some((tab.index, tab.name.clone()));
                    }
                    if icon_button(ui, "add", "Add sheet (Shift+F11)").clicked() {
                        app.run_or_alert("home.insertSheet", json!({ "before": list.len() }));
                    }
                    let all = match icons::icon("menu") {
                        Some(img) => ui.menu_button(img, |ui| self.all_sheets(app, ui, &list, active)).response,
                        None => ui.menu_button("≡", |ui| self.all_sheets(app, ui, &list, active)).response,
                    };
                    all.on_hover_text("All sheets (Alt+Shift+K)");
                    ui.add_space(6.0);
                    let quick_width = 220.0;
                    let strip = (ui.available_width() - quick_width).max(80.0);
                    egui::ScrollArea::horizontal().max_width(strip).id_salt("starsheets_tabs").show(ui, |ui| {
                        ui.horizontal(|ui| {
                            self.tab_rects.clear();
                            for tab in list.iter().filter(|t| !t.hidden) {
                                self.tab(app, ui, tab, tab.index == active, list.len());
                            }
                            self.drop_tab(app, ui);
                        });
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| self.quick_sum(app, ui));
                });
            });
    }

    fn all_sheets(&mut self, app: &mut SheetApp, ui: &mut Ui, list: &[Tab], active: usize) {
        let width = crate::shell::menu_style(ui, true);
        for tab in list {
            let label = if tab.hidden { format!("{} (hidden)", tab.name) } else { tab.name.clone() };
            // Sheets ticks the current sheet.
            let row = match (tab.index == active, icons::icon("check")) {
                (true, Some(tick)) => Button::image_and_text(tick, RichText::new(label).size(14.0).color(theme::TEXT)).shortcut_text(""),
                _ => crate::shell::menu_row(&label, "", true),
            };
            if ui.add_sized([width, 32.0], row.frame_when_inactive(false)).clicked() {
                if tab.hidden {
                    app.run_or_alert("sheet.unhide", json!({ "sheet": tab.index }));
                }
                switch_to(app, tab.index);
                ui.close();
            }
        }
    }

    fn tab(&mut self, app: &mut SheetApp, ui: &mut Ui, tab: &Tab, active: bool, count: usize) {
        if let Some((i, draft)) = &mut self.renaming
            && *i == tab.index
        {
            let r = ui.add(egui::TextEdit::singleline(draft).desired_width(110.0));
            r.request_focus();
            if r.lost_focus() {
                let name = draft.trim().to_string();
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !name.is_empty() && name != tab.name {
                    app.run_or_alert("sheet.rename", json!({ "sheet": tab.index, "name": name }));
                }
                self.renaming = None;
            }
            return;
        }
        let colour = if active { theme::BLUE } else { theme::TEXT_DIM };
        let text = RichText::new(tab.name.clone()).size(13.5).color(colour);
        let text = if active { text.strong() } else { text };
        let fill = if active { theme::ACTIVE_TAB } else { egui::Color32::TRANSPARENT };
        // Sheets' tab: the name (click to switch, drag to reorder, double-click to rename) and a
        // small ▾ that opens the tab menu. Right-click anywhere on it opens the same menu once.
        let shown = Frame::NONE.fill(fill).corner_radius(6.0).inner_margin(Margin { left: 6, right: 2, top: 0, bottom: 0 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let name = ui.add(Button::new(text).frame(false).min_size(vec2(56.0, 30.0)).sense(egui::Sense::click_and_drag()));
            let arrow = ui.add(Button::new(RichText::new("▾").size(12.0).color(colour)).frame(false).min_size(vec2(20.0, 30.0))).on_hover_text("Sheet menu");
            (name, arrow)
        });
        let (name, arrow) = shown.inner;
        let whole = shown.response.rect;
        self.tab_rects.push((tab.index, whole));
        if let Some(c) = tab.colour {
            ui.painter().hline(whole.x_range().shrink(6.0), whole.bottom() - 2.0, Stroke::new(3.0, c));
        }
        if name.drag_started() {
            self.dragging = Some(tab.index);
        }
        if (name.clicked() || arrow.clicked()) && !active {
            switch_to(app, tab.index);
        }
        if name.double_clicked() {
            self.renaming = Some((tab.index, tab.name.clone()));
        }
        // One menu per right-click: the name and the arrow each own a different popup.
        name.context_menu(|ui| self.tab_menu(app, ui, tab, count));
        arrow.context_menu(|ui| self.tab_menu(app, ui, tab, count));
        egui::Popup::menu(&arrow).show(|ui| self.tab_menu(app, ui, tab, count));
    }

    /// While a tab is dragged: a blue line where it would land; on release, moves the sheet.
    fn drop_tab(&mut self, app: &mut SheetApp, ui: &mut Ui) {
        let Some(from) = self.dragging else { return };
        let Some(p) = ui.ctx().pointer_interact_pos().or_else(|| ui.ctx().pointer_latest_pos()) else { return };
        // The last tab (other than the dragged one) whose middle is left of the pointer.
        let after = self.tab_rects.iter().filter(|(i, r)| *i != from && r.center().x < p.x).max_by(|a, b| a.1.center().x.total_cmp(&b.1.center().x)).copied();
        let line_x = match after {
            Some((_, r)) => r.right() + 1.0,
            None => self.tab_rects.first().map_or(p.x, |(_, r)| r.left() - 1.0),
        };
        if let Some((_, first)) = self.tab_rects.first() {
            ui.painter().vline(line_x, first.y_range(), Stroke::new(2.0, theme::BLUE));
        }
        if ui.input(|i| i.pointer.any_released()) {
            self.dragging = None;
            let to = match after {
                Some((t, _)) if t < from => t + 1,
                Some((t, _)) => t,
                None => self.tab_rects.iter().map(|(i, _)| *i).min().unwrap_or(0),
            };
            if to != from {
                app.run_or_alert("sheet.move", json!({ "sheet": from, "to": to }));
            }
        }
    }

    fn tab_menu(&mut self, app: &mut SheetApp, ui: &mut Ui, tab: &Tab, count: usize) {
        // Sheets' tab menu: plain rows, no icons (docs/sheets-menus.md).
        let width = crate::shell::menu_style(ui, false);
        let row = |label: &str| crate::shell::menu_row(label, "", false).min_size(vec2(width, 32.0));
        let s = tab.index;
        if ui.add_enabled(count > 1, row("Delete")).clicked() {
            app.run_or_alert("home.deleteSheet", json!({ "sheet": s }));
            ui.close();
        }
        if ui.add(row("Duplicate")).clicked() {
            app.run_or_alert("sheet.move", json!({ "sheet": s, "to": s + 1, "copy": true }));
            ui.close();
        }
        if ui.add(row("Rename")).clicked() {
            self.renaming = Some((s, tab.name.clone()));
            ui.close();
        }
        let colour = row("Change colour").right_text(egui::containers::menu::SubMenuButton::arrow_atom(Some(theme::TEXT_DIM)));
        egui::containers::menu::SubMenuButton::from_button(colour).ui(ui, |ui| {
            if ui.button("Reset").clicked() {
                app.run_or_alert("sheet.tabColor", json!({ "sheet": s, "color": "none" }));
                ui.close();
            }
            crate::shell::palette_grid(ui, |hex| {
                app.run_or_alert("sheet.tabColor", json!({ "sheet": s, "color": hex }));
            });
        });
        if ui.add_enabled(count > 1, row("Hide sheet")).clicked() {
            app.run_or_alert("sheet.hide", json!({ "sheet": s }));
            ui.close();
        }
        crate::shell::menu_sep_width(ui, width);
        if ui.add_enabled(s + 1 < count, row("Move right")).clicked() {
            app.run_or_alert("sheet.move", json!({ "sheet": s, "to": s + 1 }));
            ui.close();
        }
        if ui.add_enabled(s > 0, row("Move left")).clicked() {
            app.run_or_alert("sheet.move", json!({ "sheet": s, "to": s - 1 }));
            ui.close();
        }
    }

    fn quick_sum(&mut self, app: &SheetApp, ui: &mut Ui) {
        let Some(d) = app.session.active() else { return };
        let Some(sh) = d.wb.active() else { return };
        // Like Sheets: only when more than one cell is selected.
        let cells: u64 = d.selection.ranges.iter().map(|r| u64::from(r.end.row - r.start.row + 1) * u64::from(r.end.col - r.start.col + 1)).sum();
        if cells < 2 {
            return;
        }
        let st = gridcraft_engine::display::stats(sh, &d.selection.ranges);
        if st.count == 0 {
            return;
        }
        let value = |q: QuickSum| -> String {
            match q {
                QuickSum::Sum => fmt(st.sum),
                QuickSum::Average => st.average.map(fmt).unwrap_or_else(|| "—".into()),
                QuickSum::Min if st.numeric > 0 => fmt(st.min),
                QuickSum::Max if st.numeric > 0 => fmt(st.max),
                QuickSum::Min | QuickSum::Max => "—".into(),
                QuickSum::Count => st.count.to_string(),
                QuickSum::CountNumbers => st.numeric.to_string(),
            }
        };
        // A text-only selection shows Count, as Sheets does.
        let shown = if st.numeric == 0 { QuickSum::Count } else { self.quick };
        let label = RichText::new(format!("{}: {}  ▾", shown.label(), value(shown))).size(13.0).color(theme::TEXT);
        ui.menu_button(label, |ui| {
            ui.set_min_width(200.0);
            for q in QuickSum::ALL {
                if ui.add(Button::new(format!("{}: {}", q.label(), value(q))).selected(q == shown)).clicked() {
                    self.quick = q;
                    ui.close();
                }
            }
        });
    }
}

fn icon_button(ui: &mut Ui, name: &str, tip: &str) -> egui::Response {
    match icons::icon(name) {
        Some(img) => ui.add(Button::image(img).frame(false).min_size(vec2(30.0, 30.0))),
        None => ui.add(Button::new(name).frame(false).min_size(vec2(30.0, 30.0))),
    }
    .on_hover_text(tip)
}

/// Goes to another sheet. Like Sheets, a cell being typed into is saved first (GridCraft keeps
/// the edit open across sheets, and the next click then jumps back to finish it). A formula
/// being typed stays open, so cells on the other sheet can still be clicked into it.
pub fn switch_to(app: &mut SheetApp, sheet: usize) {
    let formula = app.editor.as_ref().is_some_and(|e| e.text.trim_start().starts_with('='));
    if app.editor.is_some() && !formula && !app.commit_edit(0, 0, false, false) {
        // The entry was refused (GridCraft shows why); stay on this sheet.
        return;
    }
    app.run_or_alert("sheet.activate", json!({ "sheet": sheet }));
}
