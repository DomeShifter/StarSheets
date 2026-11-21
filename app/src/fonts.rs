//! Roboto (OFL 1.1, app/fonts/OFL.txt) for the interface and for sans-serif cells. A web build has
//! no system fonts, so these lead GridCraft's font roles (patch 0005, `set_host_faces`).

static REGULAR: &[u8] = include_bytes!("../fonts/Roboto-Regular.ttf");
static MEDIUM: &[u8] = include_bytes!("../fonts/Roboto-Medium.ttf");
static BOLD: &[u8] = include_bytes!("../fonts/Roboto-Bold.ttf");
static ITALIC: &[u8] = include_bytes!("../fonts/Roboto-Italic.ttf");
static BOLD_ITALIC: &[u8] = include_bytes!("../fonts/Roboto-BoldItalic.ttf");

/// Call before the app builds its fonts.
pub fn install() {
    gridcraft_ui_egui::theme::set_host_faces(vec![
        ("ui", REGULAR),
        ("ui-bold", MEDIUM),
        ("cell", REGULAR),
        ("cell-bold", BOLD),
        ("cell-italic", ITALIC),
        ("cell-bold-italic", BOLD_ITALIC),
    ]);
}
