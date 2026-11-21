//! Sheets turns a typed web or e-mail address into a link when the edit is committed. GridCraft
//! doesn't, so we watch for an edit that ends and run its `insert.link` on that cell.

use gridcraft_engine::core::CellRef;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

#[derive(Default)]
pub struct AutoLink {
    /// The cell being edited last frame: (document, sheet, cell).
    editing: Option<(u64, usize, CellRef)>,
}

impl AutoLink {
    /// Call once per frame, after GridCraft's UI ran.
    pub fn after_frame(&mut self, app: &mut SheetApp) {
        let now = app.editor.as_ref().map(|e| (e.doc, e.sheet, e.cell));
        if let Some((doc, sheet, cell)) = self.editing
            && now != self.editing
        {
            link_if_address(app, doc, sheet, cell);
        }
        self.editing = now;
    }
}

/// The link target for a cell's text, when the whole text is one address.
pub fn address(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || t.chars().any(char::is_whitespace) {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    let host_ok = |rest: &str| {
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        host.contains('.') && !host.starts_with('.') && !host.ends_with('.')
    };
    if let Some(rest) = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")) {
        return host_ok(rest).then(|| t.to_string());
    }
    if lower.starts_with("www.") && host_ok(&lower) {
        return Some(format!("https://{t}"));
    }
    if let Some(rest) = lower.strip_prefix("mailto:") {
        return rest.contains('@').then(|| t.to_string());
    }
    None
}

fn link_if_address(app: &mut SheetApp, doc: u64, sheet: usize, cell: CellRef) {
    // Only on the sheet still showing (the command works on the active sheet).
    let Some(target) = app.session.active().filter(|d| d.uid == doc && d.wb.active_sheet() == sheet).and_then(|d| {
        let sh = d.wb.active()?;
        if sh.hyperlinks.contains_key(&cell) || sh.cell(cell).is_some_and(|c| c.formula.is_some()) {
            return None;
        }
        address(sh.value(cell).as_text()?)
    }) else {
        return;
    };
    app.run_or_alert("insert.link", json!({ "cell": cell.a1(), "target": target }));
}

#[cfg(test)]
mod tests {
    use super::address;

    #[test]
    fn spots_addresses() {
        assert_eq!(address("https://example.com/a?b=1").as_deref(), Some("https://example.com/a?b=1"));
        assert_eq!(address(" http://x.org ").as_deref(), Some("http://x.org"));
        assert_eq!(address("www.bbc.co.uk").as_deref(), Some("https://www.bbc.co.uk"));
        assert_eq!(address("mailto:a@b.com").as_deref(), Some("mailto:a@b.com"));
        for no in ["hello", "http://", "https://localhost", "see www.x.com", "www.", "3.14", "mailto:nobody"] {
            assert_eq!(address(no), None, "{no}");
        }
    }
}
