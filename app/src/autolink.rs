//! Sheets turns a typed or pasted web or e-mail address into a link. GridCraft doesn't, so we
//! watch for an edit that ends, or a paste, and run its `insert.link` on those cells.

use gridcraft_engine::core::CellRef;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

#[derive(Default)]
pub struct AutoLink {
    /// The cell being edited last frame: (document, sheet, cell).
    editing: Option<(u64, usize, CellRef)>,
    /// Frames left in which to check the selection after a paste (the pasted block is selected).
    after_paste: u8,
}

/// The most cells one paste is checked over (a pasted whole column stays cheap).
const PASTE_SCAN_MAX: u64 = 20_000;

impl AutoLink {
    /// Call from the raw input hook: notes a paste so the next frames check what it filled.
    pub fn saw_input(&mut self, raw: &egui::RawInput) {
        if raw.events.iter().any(|e| matches!(e, egui::Event::Paste(_))) {
            self.after_paste = 3;
        }
    }

    /// Call once per frame, after GridCraft's UI ran.
    pub fn after_frame(&mut self, app: &mut SheetApp) {
        if self.after_paste > 0 && app.editor.is_none() {
            self.after_paste -= 1;
            link_pasted(app);
        }
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

/// Links every address in the selected block(s), within the used part of the sheet.
fn link_pasted(app: &mut SheetApp) {
    let found: Vec<(CellRef, String)> = app
        .session
        .active()
        .and_then(|d| {
            let sh = d.wb.active()?;
            let used = sh.used_range()?;
            let mut out = Vec::new();
            let mut seen = 0u64;
            for r in &d.selection.ranges {
                let (r0, r1) = (r.start.row.max(used.start.row), r.end.row.min(used.end.row));
                let (c0, c1) = (r.start.col.max(used.start.col), r.end.col.min(used.end.col));
                for row in r0..=r1 {
                    for col in c0..=c1 {
                        seen += 1;
                        if seen > PASTE_SCAN_MAX {
                            return Some(out);
                        }
                        let at = CellRef { row, col };
                        let Some(cell) = sh.cell(at) else { continue };
                        if cell.formula.is_some() || sh.hyperlinks.contains_key(&at) {
                            continue;
                        }
                        if let Some(t) = cell.value.as_text().and_then(address) {
                            out.push((at, t));
                        }
                    }
                }
            }
            Some(out)
        })
        .unwrap_or_default();
    for (at, target) in found {
        app.run_or_alert("insert.link", json!({ "cell": at.a1(), "target": target }));
    }
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
