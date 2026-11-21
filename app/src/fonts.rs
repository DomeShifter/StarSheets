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

/// The web fonts (menu name, Google Fonts family), from app/webfonts.txt.
pub static WEB_FONTS: std::sync::LazyLock<Vec<(&'static str, &'static str)>> = std::sync::LazyLock::new(|| {
    include_str!("../webfonts.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('|').map(|(a, b)| (a.trim(), b.trim())))
        .collect()
});

/// The folder a web font's files are served from (as scripts/fetch-fonts.py names it).
pub fn slug(menu: &str) -> String {
    let mut out = String::new();
    for c in menu.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Fetches a web font the first time an open workbook uses it, and hands its faces to GridCraft
/// (patch 0021), which then draws those cells in it.
#[cfg(target_arch = "wasm32")]
#[derive(Default)]
pub struct WebFontLoader {
    requested: std::collections::HashSet<String>,
}

#[cfg(target_arch = "wasm32")]
impl WebFontLoader {
    pub fn logic(&mut self, app: &gridcraft_ui_egui::SheetApp, ctx: &egui::Context) {
        for name in app.used_fonts() {
            if self.requested.contains(&name) {
                continue;
            }
            let Some((menu, _)) = WEB_FONTS.iter().find(|(m, _)| m.eq_ignore_ascii_case(&name)) else { continue };
            self.requested.insert(name.clone());
            let folder = slug(menu);
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                for (file, bold, italic) in [("regular", false, false), ("bold", true, false), ("italic", false, true), ("bolditalic", true, true)] {
                    if let Ok((200, _, bytes)) = crate::files::get_bytes(&format!("webfonts/{folder}/{file}.ttf")).await {
                        gridcraft_ui_egui::fonts::add_web_face(&name, bold, italic, bytes);
                        ctx.request_repaint();
                    }
                }
            });
        }
    }
}
