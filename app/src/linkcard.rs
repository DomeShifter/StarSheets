//! Sheets' link card: selecting a cell with a link shows a small card (the address, open, copy,
//! edit, remove) instead of following the link (patch 0009 stops GridCraft's click-to-follow).

use egui::{Align2, Button, Frame, RichText, Ui, vec2};
use gridcraft_engine::core::CellRef;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

use crate::{icons, theme};

#[derive(Default)]
pub struct LinkCard {
    /// The cell the card belongs to and where it's drawn.
    at: Option<(CellRef, egui::Pos2)>,
    /// Closed with ✕ for this cell.
    dismissed: Option<CellRef>,
}

impl LinkCard {
    pub fn ui(&mut self, app: &mut SheetApp, ui: &mut Ui) {
        let Some((cell, target)) = app.session.active().and_then(|d| {
            let c = d.selection.active;
            let single = d.selection.is_single_cell();
            d.wb.active().and_then(|sh| sh.hyperlinks.get(&c)).filter(|_| single).map(|h| (c, h.target.clone()))
        }) else {
            self.at = None;
            self.dismissed = None;
            return;
        };
        if app.editor.is_some() || self.dismissed == Some(cell) {
            return;
        }
        // Just under the cell's bottom-left corner, as in Sheets (re-measured every frame so it
        // follows scrolling); the pointer is only a fallback.
        let pos = cell_bottom_left(app, cell).map(|p| p + vec2(0.0, 4.0)).unwrap_or_else(|| {
            let p = self.at.filter(|(c, _)| *c == cell).map(|(_, p)| p);
            p.unwrap_or_else(|| ui.ctx().input(|i| i.pointer.latest_pos()).unwrap_or_else(|| ui.max_rect().center()) + vec2(-12.0, 18.0))
        });
        self.at = Some((cell, pos));
        // Hidden while the cell is scrolled out of the grid.
        if app.grid.cells_rect.is_some_and(|r| !r.contains(pos - vec2(0.0, 6.0))) {
            return;
        }
        egui::Area::new(egui::Id::new("starsheets_link_card")).fixed_pos(pos).pivot(Align2::LEFT_TOP).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            Frame::popup(ui.style()).fill(egui::Color32::WHITE).corner_radius(8.0).inner_margin(10.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let shown: String = if target.chars().count() > 48 { target.chars().take(45).chain("…".chars()).collect() } else { target.clone() };
                    if ui.link(RichText::new(shown).color(theme::BLUE)).on_hover_text("Open link").clicked() {
                        gridcraft_ui_egui::grid::follow_link(app, &target);
                    }
                    ui.add_space(8.0);
                    let mut button = |ui: &mut Ui, icon: &str, tip: &str| match icons::icon(icon) {
                        Some(img) => ui.add(Button::image(img).frame(false)).on_hover_text(tip),
                        None => ui.add(Button::new(tip).frame(false)),
                    };
                    if button(ui, "content_copy", "Copy link").clicked() {
                        ui.ctx().copy_text(target.clone());
                    }
                    if button(ui, "edit", "Edit link").clicked() {
                        app.open_dialog("insertLink", json!({}));
                    }
                    if button(ui, "link_off", "Remove link").clicked() {
                        app.run_or_alert("edit.clearHyperlinks", json!({}));
                    }
                    if ui.add(Button::new("✕").frame(false)).on_hover_text("Close").clicked() {
                        self.dismissed = Some(cell);
                    }
                });
            });
        });
    }
}

/// The on-screen bottom-left corner of `cell` on the active sheet, from GridCraft's own geometry.
pub(crate) fn cell_bottom_left(app: &SheetApp, cell: CellRef) -> Option<egui::Pos2> {
    cell_rect(app, cell).map(|r| r.left_bottom())
}

/// Where a cell is on screen.
pub(crate) fn cell_rect(app: &SheetApp, cell: CellRef) -> Option<egui::Rect> {
    let cells = app.grid.cells_rect?;
    let sh = app.session.active()?.wb.active()?;
    let scroll = app.view().scroll;
    // Geo takes the whole grid area (headings included); find the headings' size from a probe.
    let probe = gridcraft_ui_egui::grid::Geo::new(sh, cells, scroll);
    let grid = cells.translate(cells.min - probe.cells.min);
    let geo = gridcraft_ui_egui::grid::Geo::new(sh, grid, scroll);
    Some(geo.screen_cell_rect(sh, cell))
}
