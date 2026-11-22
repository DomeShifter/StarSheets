//! GridCraft's alerts ("GridCraft: The cell or chart you're trying to change…") shown as house
//! dialogs with StarSheets' wording, one at a time.

use gridcraft_ui_egui::SheetApp;

use crate::house;

#[derive(Default)]
pub struct Messages {
    showing: Option<(String, String)>,
}

/// Sheets' words for the messages people meet most.
fn reword(title: &str, text: &str) -> (String, String) {
    let title = if title == "GridCraft" || title.is_empty() { "Heads up".to_string() } else { title.to_string() };
    let text = if text.contains("protected sheet") {
        "You're trying to edit a protected cell. To change it, remove its protection in Data › Protect sheets and ranges.".to_string()
    } else {
        text.replace("GridCraft", "StarSheets")
    };
    (title, text)
}

impl Messages {
    pub fn ui(&mut self, app: &mut SheetApp, ctx: &egui::Context) {
        if self.showing.is_none()
            && let Some((t, m)) = app.message.take()
        {
            self.showing = Some(reword(&t, &m));
        }
        let Some((title, text)) = self.showing.clone() else { return };
        let mut closed = false;
        let mut ok = false;
        house::dialog(ctx, "starsheets_message", 420.0, &title, Some(&mut closed), |ui| {
            house::text(ui, &text);
            ok = house::actions(ui, &["OK"]).is_some() || ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape));
        });
        if closed || ok {
            self.showing = None;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn gridcraft_messages_are_reworded() {
        let (t, m) = super::reword("GridCraft", "The cell or chart you're trying to change is on a protected sheet.");
        assert_eq!(t, "Heads up");
        assert!(m.contains("Protect sheets and ranges"));
        assert_eq!(super::reword("Version history", "x").0, "Version history");
    }
}
