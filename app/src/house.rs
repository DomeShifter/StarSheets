//! The StarSheets house style: one set of sizes, colours and shapes, taken from Google Sheets,
//! that every dialog, menu, card and panel uses (docs/house-style.md explains each choice).
//! Build new UI from these helpers rather than with numbers of its own.

use egui::{Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke, Ui, vec2};

use crate::theme;

// ── Spacing: a 4 px scale ────────────────────────────────────────────────────────────────
pub const XS: f32 = 4.0;
pub const S: f32 = 8.0;
pub const M: f32 = 12.0;
pub const L: f32 = 16.0;
pub const XL: f32 = 24.0;

// ── Type ─────────────────────────────────────────────────────────────────────────────────
/// Dialog titles.
pub const TITLE: f32 = 22.0;
/// Field labels and section headings inside dialogs and panels.
pub const LABEL: f32 = 14.0;
/// Everything else people read: menu rows, values, buttons.
pub const BODY: f32 = 14.0;
/// Help text under a label.
pub const NOTE: f32 = 12.0;
/// Small capitals headings, such as STANDARD and CUSTOM in the colour menu.
pub const CAPS: f32 = 11.0;

// ── Colours (on top of theme.rs) ─────────────────────────────────────────────────────────
/// Sheets' green for the main button in a dialog, and the text of the others.
pub const GREEN: Color32 = Color32::from_rgb(0x14, 0x6C, 0x2E);
/// The soft blue behind a selected item (a chosen list row, a picked chip).
pub const SELECTED: Color32 = Color32::from_rgb(0xE8, 0xF0, 0xFE);
/// Error text.
pub const ERROR: Color32 = Color32::from_rgb(0xB3, 0x26, 0x1E);

// ── Shapes ───────────────────────────────────────────────────────────────────────────────
/// Dialogs, menus and cards.
pub const PANEL_RADIUS: u8 = 8;
/// Text boxes, drop-downs and chips.
pub const FIELD_RADIUS: u8 = 4;
/// Buttons are pills: half their 36 px height.
pub const BUTTON_RADIUS: u8 = 18;
pub const BUTTON_HEIGHT: f32 = 36.0;
pub const FIELD_HEIGHT: f32 = 36.0;
/// Menu rows and list rows.
pub const ROW_HEIGHT: f32 = 32.0;
/// The space between a menu's edge and anything in it that isn't a row (palettes, headings).
pub const MENU_INSET: i8 = 16;

/// Applies the parts of the house style that egui can carry for everything at once, including
/// GridCraft's own windows: window padding, corners and shadows.
pub fn install(ctx: &egui::Context) {
    ctx.all_styles_mut(|s| {
        s.spacing.window_margin = Margin::same(XL as i8);
        s.spacing.menu_margin = Margin::symmetric(0, S as i8);
        s.visuals.window_corner_radius = CornerRadius::same(PANEL_RADIUS);
        s.visuals.menu_corner_radius = CornerRadius::same(PANEL_RADIUS);
        s.visuals.window_stroke = Stroke::NONE;
        s.visuals.window_fill = Color32::WHITE;
        s.visuals.window_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 24, spread: 0, color: Color32::from_black_alpha(60) };
        s.visuals.popup_shadow = egui::epaint::Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_black_alpha(45) };
    });
}

/// The frame of every dialog: white, 24 px padding, 8 px corners, a soft shadow.
pub fn dialog_frame() -> Frame {
    Frame::NONE
        .fill(Color32::WHITE)
        .inner_margin(Margin::same(XL as i8))
        .corner_radius(CornerRadius::same(PANEL_RADIUS))
        .shadow(egui::epaint::Shadow { offset: [0, 4], blur: 24, spread: 0, color: Color32::from_black_alpha(60) })
}

/// The frame of a floating card (a note, a link, the date picker): 16 px padding.
pub fn card_frame() -> Frame {
    Frame::NONE
        .fill(Color32::WHITE)
        .inner_margin(Margin::same(L as i8))
        .corner_radius(CornerRadius::same(PANEL_RADIUS))
        .shadow(egui::epaint::Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_black_alpha(45) })
}

/// A small floating bubble (the calculation preview): 8 x 4 padding, 4 px corners.
pub fn bubble_frame() -> Frame {
    card_frame().inner_margin(Margin::symmetric(S as i8, XS as i8)).corner_radius(CornerRadius::same(FIELD_RADIUS))
}

/// The frame of a side panel (data validation, conditional formatting): 16 px all round.
pub fn panel_frame() -> Frame {
    Frame::NONE.fill(Color32::WHITE).inner_margin(Margin::same(L as i8)).stroke(Stroke::new(1.0, theme::DIVIDER))
}

/// Sets a dialog or panel `Ui` up in the house style: 14 px text, 36 px fields with a thin grey
/// outline and 4 px corners, 8 px between items.
pub fn form_style(ui: &mut Ui) {
    let st = ui.style_mut();
    st.spacing.item_spacing = vec2(S, S);
    st.spacing.button_padding = vec2(M, S);
    st.spacing.interact_size.y = FIELD_HEIGHT;
    st.spacing.combo_height = 320.0;
    for ts in [egui::TextStyle::Body, egui::TextStyle::Button] {
        st.text_styles.insert(ts, egui::FontId::proportional(BODY));
    }
    let w = &mut st.visuals.widgets;
    for (state, stroke) in [(&mut w.inactive, theme::BOX), (&mut w.hovered, theme::TEXT), (&mut w.active, theme::BLUE), (&mut w.open, theme::BLUE)] {
        state.bg_fill = Color32::WHITE;
        state.weak_bg_fill = Color32::WHITE;
        state.bg_stroke = Stroke::new(1.0, stroke);
        state.corner_radius = CornerRadius::same(FIELD_RADIUS);
        state.expansion = 0.0;
    }
    st.visuals.selection.stroke = Stroke::new(2.0, theme::BLUE);
}

/// Shows a dialog in the house style: title (with ✕ when `close` is given), then the body.
pub fn dialog<R>(ctx: &egui::Context, id: &str, width: f32, title: &str, close: Option<&mut bool>, body: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Modal::new(egui::Id::new(id))
        .frame(dialog_frame())
        .show(ctx, |ui| {
            form_style(ui);
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).size(TITLE).color(theme::TEXT));
                if let Some(close) = close {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if close_button(ui) {
                            *close = true;
                        }
                    });
                }
            });
            ui.add_space(L - S);
            body(ui)
        })
        .inner
}

/// A ✕ button for a dialog's corner.
pub fn close_button(ui: &mut Ui) -> bool {
    let b = match crate::icons::icon("close") {
        Some(img) => egui::Button::image(img.fit_to_exact_size(vec2(20.0, 20.0))),
        None => egui::Button::new(RichText::new("✕").size(16.0)),
    };
    ui.add(b.frame(false).min_size(vec2(32.0, 32.0))).on_hover_text("Close").clicked()
}

/// A field label, with its help text underneath when there is some.
pub fn label(ui: &mut Ui, text: &str, note: &str) {
    ui.label(RichText::new(text).size(LABEL).strong().color(theme::TEXT));
    if !note.is_empty() {
        self::note(ui, note);
    }
}

/// Help text: small and grey.
pub fn note(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(NOTE).color(theme::TEXT_DIM));
}

/// A small-capitals heading (STANDARD, CUSTOM).
pub fn caps(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).size(CAPS).strong().color(theme::TEXT_DIM));
}

/// Body text.
pub fn text(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(BODY).color(theme::TEXT));
}

/// The main button: a green pill with white text.
pub fn primary(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).size(BODY).color(Color32::WHITE))
        .fill(GREEN)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(BUTTON_RADIUS))
        .min_size(vec2(88.0, BUTTON_HEIGHT))
}

/// Any other button: a white pill with a grey outline and green text.
pub fn secondary(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).size(BODY).color(GREEN))
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, theme::BOX))
        .corner_radius(CornerRadius::same(BUTTON_RADIUS))
        .min_size(vec2(88.0, BUTTON_HEIGHT))
}

/// A text-only button (green, no outline), for actions inside a list such as "Restore".
pub fn link(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).size(BODY).color(GREEN)).frame(false)
}

/// A dialog's buttons, at the bottom right with 24 px above and 8 px between. The last one is
/// the main (green) one. Returns the index of the one clicked.
pub fn actions(ui: &mut Ui, labels: &[&str]) -> Option<usize> {
    ui.add_space(XL - S);
    let mut hit = None;
    ui.allocate_ui_with_layout(vec2(ui.available_width(), BUTTON_HEIGHT), Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = S;
        for (i, l) in labels.iter().enumerate().rev() {
            let b = if i + 1 == labels.len() { primary(l) } else { secondary(l) };
            if ui.add(b).clicked() {
                hit = Some(i);
            }
        }
    });
    hit
}

/// Space between two groups of fields in a dialog or panel.
pub fn gap(ui: &mut Ui) {
    ui.add_space(L);
}

/// Content inside a menu that isn't a row (a palette, a heading), kept 16 px from the sides.
pub fn menu_inset<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::NONE.inner_margin(Margin::symmetric(MENU_INSET, XS as i8)).show(ui, add).inner
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_spacing_scale_steps_in_fours() {
        for v in [super::XS, super::S, super::M, super::L, super::XL] {
            assert_eq!(v % 4.0, 0.0);
        }
        assert_eq!(super::BUTTON_RADIUS as f32 * 2.0, super::BUTTON_HEIGHT);
    }
}
