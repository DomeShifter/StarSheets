//! Spell check while typing in a cell (Google underlines unknown words in red, and only while you
//! edit). Capitalised words count as names and are never marked. The British English word list (SCOWL, dictionary/en-GB.txt, ~300 KB compressed) is
//! fetched the first time a cell is edited; until it arrives nothing is underlined.

use std::collections::HashSet;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static WORDS: OnceLock<HashSet<String>> = OnceLock::new();
static ASKED: AtomicBool = AtomicBool::new(false);

/// Whether `word` is a word (true while the list hasn't loaded, so nothing is marked).
pub fn known(word: &str) -> bool {
    let Some(words) = WORDS.get() else { return true };
    let w = word.trim_matches('\'');
    let w = w.strip_suffix("'s").or_else(|| w.strip_suffix("’s")).unwrap_or(w);
    // Capitalised words are taken as names (London, Priya): the list has no proper nouns, and
    // spreadsheets are full of them. That covers abbreviations (UK, NHS) too.
    if w.chars().next().is_some_and(char::is_uppercase) {
        return true;
    }
    words.contains(w) || words.contains(&w.to_lowercase())
}

/// Fills the list from the file's text.
fn load(text: &str) {
    let _ = WORDS.set(text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect());
}

/// Call each frame: fetches the list once, when a cell is first edited.
pub fn logic(app: &gridcraft_ui_egui::SheetApp, ctx: &egui::Context) {
    if app.editor.is_none() || ASKED.swap(true, Ordering::Relaxed) {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    {
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok((200, _, bytes)) = crate::files::get_bytes("dictionary/en-GB.txt").await {
                load(&String::from_utf8_lossy(&bytes));
                ctx.request_repaint();
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = ctx;
}

#[cfg(test)]
mod tests {
    #[test]
    fn words_are_checked_without_case_or_possessives() {
        super::load("colour\nlondon\nthe\ncat\n");
        assert!(super::known("colour") && super::known("cat's") && super::known("NHS") && super::known("Priya"));
        assert!(!super::known("colr"));
    }
}
