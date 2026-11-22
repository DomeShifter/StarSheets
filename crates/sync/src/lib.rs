//! Live editing, shared by the server and the web app (docs/live-editing.md).
//!
//! The server holds the true copy of an open file and applies changes in the order they arrive.
//! A change is normally a GridCraft command with the sheet, selection and input locale it ran
//! with ([`Change::Cmd`]); the server runs the same engine, so replaying gives the same result,
//! which the change's fingerprint checks. A command that depends on more than that (a paste reads
//! the tab's own clipboard) travels as the finished state of the sheets it changed
//! ([`Change::State`]).

use std::sync::Arc;

use gridcraft_engine::Session;
use gridcraft_engine::model::{Sheet, Workbook};
use gridcraft_engine::selection::Selection;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

/// Tabs and server must run the same engine. CI sets STARSHEETS_BUILD to the commit for both.
pub const BUILD: &str = match option_env!("STARSHEETS_BUILD") {
    Some(b) => b,
    None => "dev",
};

// ---------------------------------------------------------------- messages

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ToServer {
    /// First message: the file (its id), the tab's build and a random id for this tab.
    Hello { file: String, build: String, client: String },
    /// A change made on top of the server's revision `base`.
    Change { cseq: u64, base: u64, change: Change },
    Undo,
    Redo,
    /// The tab's copy doesn't match the server's: send the whole file again.
    Resync,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ToClient {
    /// The file as the server has it, at revision `seq`.
    Welcome { seq: u64, workbook: Json },
    /// A change everyone applies (also the sender's acknowledgement).
    Applied { seq: u64, client: String, cseq: u64, change: Change },
    /// The server couldn't apply this change; the tab drops it.
    Rejected { cseq: u64, why: String },
    /// Replaying gave a different result: send this change as its finished state, then the
    /// changes after it again.
    NeedState { cseq: u64 },
    /// Replace the whole file (undo, redo, or a tab that fell out of step).
    Reset { seq: u64, workbook: Json },
    /// The fingerprint of the whole file at `seq`, to check against.
    Check { seq: u64, hash: String },
    /// Saved to the server's folder (Unix seconds).
    Saved { at: i64 },
    /// Something to tell the person ("Can't undo: …").
    Message { text: String },
    /// The tab runs a different build than the server: reload the page.
    Reload,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum Change {
    Cmd { sheet: usize, sel: Selection, id: String, params: Json, locale: InputLocale, fp: String },
    State { patch: Json },
}

/// How typed input is read (dates, number separators): the person's setting, which is
/// thread-local in GridCraft, so it travels with each command.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InputLocale {
    pub day_first: bool,
    pub date: String,
    pub time: String,
    pub number: Option<String>,
    pub decimal: Option<char>,
    pub group: Option<char>,
}

thread_local! {
    static LOCALE: std::cell::RefCell<InputLocale> = std::cell::RefCell::new(InputLocale::default());
}

impl InputLocale {
    /// Hands the locale to GridCraft's input reader and remembers it as this thread's.
    pub fn apply(&self) {
        use gridcraft_engine::core::parse;
        let (date, time) = (Some(self.date.as_str()).filter(|s| !s.is_empty()), Some(self.time.as_str()).filter(|s| !s.is_empty()));
        let datetime = date.zip(time).map(|(d, t)| format!("{d} {t}"));
        parse::set_input_locale(self.day_first, date, time, datetime.as_deref(), self.number.as_deref());
        if let (Some(d), Some(g)) = (self.decimal, self.group) {
            parse::set_number_separators(d, g);
        }
        LOCALE.with(|l| *l.borrow_mut() = self.clone());
    }
    /// The locale last applied on this thread.
    pub fn current() -> InputLocale {
        LOCALE.with(|l| l.borrow().clone())
    }
}

// ---------------------------------------------------------------- which commands replay

/// Commands that read more than the workbook, sheet, selection and locale, so they travel as
/// their finished state. Anything missed here is caught by the fingerprint check.
pub fn replayable(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    !["paste", "copied", "formatpainter", "autofit", "picture", "file.", "import", "fromtext", "script", "macro"].iter().any(|w| id.contains(w))
}

// ---------------------------------------------------------------- applying changes

/// Runs a command the way it ran where it was made: on `sheet`, with `sel` and `locale`, without
/// adding an undo step or a sync step here. This tab's own sheet, selection, locale and history
/// come back afterwards.
pub fn apply_cmd(s: &mut Session, sheet: usize, sel: &Selection, id: &str, params: &Json, locale: &InputLocale) -> Result<(), String> {
    let log = s.sync_log.take();
    let requests = s.ui_requests.len();
    let Some(d) = s.active_mut() else {
        s.sync_log = log;
        return Err("no workbook".into());
    };
    let (view_sheet, view_sel, undo, redo) = (d.wb.active_sheet(), d.selection.clone(), d.undo.clone(), d.redo.clone());
    d.wb.set_active_sheet(sheet.min(d.wb.sheets.len().saturating_sub(1)));
    d.selection = sel.clone();
    let own = InputLocale::current();
    let switch = own != *locale;
    if switch {
        locale.apply();
    }
    let result = s.execute(id, params.clone());
    if switch {
        own.apply();
    }
    s.ui_requests.truncate(requests);
    if let Some(d) = s.active_mut() {
        d.undo = undo;
        d.redo = redo;
        d.wb.set_active_sheet(view_sheet.min(d.wb.sheets.len().saturating_sub(1)));
        d.selection = view_sel;
    }
    s.sync_log = log;
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Applies any change to the session's active workbook.
pub fn apply(s: &mut Session, change: &Change) -> Result<(), String> {
    match change {
        Change::Cmd { sheet, sel, id, params, locale, .. } => apply_cmd(s, *sheet, sel, id, params, locale),
        Change::State { patch } => {
            let current = s.active().map(|d| d.wb.clone()).ok_or("no workbook")?;
            let wb = apply_patch(&current, patch)?;
            set_workbook(s, wb);
            Ok(())
        }
    }
}

/// Puts `wb` in place of the active workbook, keeping this tab's sheet and selection.
pub fn set_workbook(s: &mut Session, wb: Workbook) {
    if let Some(d) = s.active_mut() {
        let sheet = d.wb.active_sheet().min(wb.sheets.len().saturating_sub(1));
        wb.set_active_sheet(sheet);
        d.wb = Arc::new(wb);
        d.calc.rebuild(&d.wb);
    }
}

/// Puts an `Arc` workbook back (a copy this side already had), keeping sheet and selection.
pub fn restore_workbook(s: &mut Session, wb: &Arc<Workbook>) {
    if let Some(d) = s.active_mut() {
        let sheet = d.wb.active_sheet().min(wb.sheets.len().saturating_sub(1));
        wb.set_active_sheet(sheet);
        d.wb = wb.clone();
        d.calc.rebuild(&d.wb);
    }
}

// ---------------------------------------------------------------- whole workbooks and patches

/// What JSON doesn't carry (unmodelled parts of the .xlsx, macros) comes from `from`.
fn keep_unserialized(wb: &mut Workbook, from: &Workbook) {
    wb.preserved = from.preserved.clone();
    wb.source_dir = from.source_dir.clone();
    wb.source_path = from.source_path.clone();
    wb.region = from.region;
    wb.macros = from.macros.clone();
    wb.macros_unread = from.macros_unread.clone();
    wb.vba_project = from.vba_project.clone();
}

pub fn workbook_json(wb: &Workbook) -> Json {
    serde_json::to_value(wb).unwrap_or(Json::Null)
}

pub fn workbook_from_json(v: &Json, current: Option<&Workbook>) -> Result<Workbook, String> {
    let mut wb: Workbook = serde_json::from_value(v.clone()).map_err(|e| format!("bad workbook: {e}"))?;
    if let Some(c) = current {
        keep_unserialized(&mut wb, c);
    }
    Ok(wb)
}

/// The finished state of what a change altered: sheets it changed in full, the others as
/// `{"keep": i}` (sheet `i` of the workbook it was made on), and the workbook's other parts.
pub fn make_patch(before: &Workbook, after: &Workbook) -> Json {
    let mut rest = after.clone();
    rest.sheets = Vec::new();
    let mut v = serde_json::to_value(&rest).unwrap_or(json!({}));
    let sheets: Vec<Json> = after
        .sheets
        .iter()
        .map(|sh| match before.sheets.iter().position(|b| Arc::ptr_eq(b, sh)) {
            Some(i) => json!({ "keep": i }),
            None => json!({ "sheet": serde_json::to_value(&**sh).unwrap_or(Json::Null) }),
        })
        .collect();
    v["sheets"] = Json::Array(sheets);
    v
}

pub fn apply_patch(current: &Workbook, patch: &Json) -> Result<Workbook, String> {
    let mut p = patch.clone();
    let entries = p.get_mut("sheets").map(Json::take).unwrap_or(json!([]));
    p["sheets"] = json!([]);
    let mut wb: Workbook = serde_json::from_value(p).map_err(|e| format!("bad change: {e}"))?;
    let mut sheets = Vec::new();
    for e in entries.as_array().cloned().unwrap_or_default() {
        if let Some(i) = e.get("keep").and_then(Json::as_u64) {
            sheets.push(current.sheets.get(i as usize).cloned().ok_or("the change refers to a sheet that isn't there")?);
        } else {
            let sh: Sheet = serde_json::from_value(e.get("sheet").cloned().unwrap_or(Json::Null)).map_err(|e| format!("bad sheet: {e}"))?;
            sheets.push(Arc::new(sh));
        }
    }
    if sheets.is_empty() {
        return Err("a workbook needs a sheet".into());
    }
    wb.sheets = sheets;
    keep_unserialized(&mut wb, current);
    Ok(wb)
}

// ---------------------------------------------------------------- fingerprints

struct Fnv(u64);

impl Fnv {
    fn new() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn bytes(&mut self, b: &[u8]) {
        for x in b {
            self.0 ^= u64::from(*x);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn json<T: Serialize + ?Sized>(&mut self, v: &T) {
        self.bytes(&serde_json::to_vec(v).unwrap_or_default());
    }
}

fn hash_sheet(h: &mut Fnv, i: usize, sh: &Sheet) {
    h.bytes(&(i as u64).to_le_bytes());
    h.bytes(sh.name.as_bytes());
    for (at, cell) in sh.cells.iter() {
        h.json(&at);
        h.json(&cell.style);
        // A formula's result can differ from tab to tab (NOW, RAND): only its text counts.
        match &cell.formula {
            Some(f) => h.json(&**f),
            None => h.json(&cell.value),
        }
    }
    h.json(&sh.merges);
    h.json(&sh.cols);
    h.json(&sh.rows);
}

/// Fingerprint of what a change altered: the sheets that changed between `before` and `after`,
/// the sheet names, the defined names and the size of the style table.
pub fn change_fingerprint(before: &Workbook, after: &Workbook) -> String {
    let mut h = Fnv::new();
    for (i, sh) in after.sheets.iter().enumerate() {
        h.bytes(sh.name.as_bytes());
        if !before.sheets.iter().any(|b| Arc::ptr_eq(b, sh)) {
            hash_sheet(&mut h, i, sh);
        }
    }
    h.json(&after.names);
    h.bytes(&(after.styles.len() as u64).to_le_bytes());
    format!("{:016x}", h.0)
}

/// Fingerprint of the whole workbook.
pub fn fingerprint(wb: &Workbook) -> String {
    let mut h = Fnv::new();
    for (i, sh) in wb.sheets.iter().enumerate() {
        hash_sheet(&mut h, i, sh);
    }
    h.json(&wb.names);
    h.bytes(&(wb.styles.len() as u64).to_le_bytes());
    format!("{:016x}", h.0)
}

#[cfg(test)]
mod tests;
