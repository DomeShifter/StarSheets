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
/// How tabs and the server talk. Bump it when a message changes shape: only then must open tabs
/// reload. (A tab on another build of the same protocol keeps working: replayed changes are
/// checked by fingerprint, and a difference falls back to sending the finished state.)
pub const PROTOCOL: u32 = 2;

pub const BUILD: &str = match option_env!("STARSHEETS_BUILD") {
    Some(b) => b,
    None => "dev",
};

// ---------------------------------------------------------------- messages

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ToServer {
    /// First message: the file (its id), the tab's build and a random id for this tab.
    Hello {
        file: String,
        build: String,
        client: String,
        /// PROTOCOL (0 from tabs older than it).
        #[serde(default)]
        protocol: u32,
    },
    /// A change made on top of the server's revision `base`.
    Change { cseq: u64, base: u64, change: Change },
    Undo,
    Redo,
    /// The tab's copy doesn't match the server's: send the whole file again.
    Resync,
    /// Where this tab is (its sheet and selection), for other people's screens.
    Presence { sheet: usize, sel: Selection },
}

/// Someone with the file open: one entry per tab.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub client: String,
    pub username: String,
    pub name: String,
    pub colour: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ToClient {
    /// The file as the server has it, at revision `seq`.
    Welcome {
        seq: u64,
        workbook: Json,
        /// False for someone who may only view the file.
        #[serde(default = "yes")]
        can_edit: bool,
    },
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
    /// The file was renamed or moved (by anyone, here or elsewhere): its name and folder now.
    /// `binned`: it went to the bin (changes aren't saved until it's restored).
    Moved { name: String, folder: String, binned: bool },
    /// The tab runs a different build than the server: reload the page.
    Reload,
    /// Everyone who has the file open now (this tab included).
    People { people: Vec<Person>, you: String },
    /// Where someone is.
    Presence { client: String, sheet: usize, sel: Selection },
}

fn yes() -> bool {
    true
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

/// Whether the server may run this command for a tab: only undoable editing commands, and never
/// one that reaches files, the network, macros, scripts or printing (a tab sends anything else as
/// its finished state, which is only data).
pub fn server_allows(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    let denied = ["file.", "macro", "automate.", "script", "python", "control.", "app.", "window.", "print", "export", "link.open", "data.get", "data.refresh", "data.from", "data.query", "picture", "object", "addin", "options", "solver", "equation"];
    gridcraft_engine::find_command(id).is_some_and(|spec| spec.undoable) && !denied.iter().any(|d| lower.contains(d))
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
    // Someone else's change is applied in a view-only tab too.
    let read_only = std::mem::take(&mut s.read_only);
    let result = s.execute(id, params.clone());
    s.read_only = read_only;
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

// ---------------------------------------------------------------- undo of one change among others

/// Why an undo (or redo) can't be done, in plain words.
pub const UNDO_CHANGED: &str = "Can't undo: someone has changed those cells since.";
pub const UNDO_LAYOUT: &str = "Can't undo that change any more: it changed more than cells (rows, columns, sheets or rules) and someone has edited the file since.";

/// Two cells hold the same thing (a formula's result may differ: it follows other cells).
fn same_cell(a: Option<&gridcraft_engine::model::Cell>, b: Option<&gridcraft_engine::model::Cell>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.style == b.style && a.formula == b.formula && a.py == b.py && (a.formula.is_some() || a.value == b.value),
        (None, None) => true,
        (Some(c), None) | (None, Some(c)) => c.formula.is_none() && c.py.is_none() && matches!(c.value, gridcraft_engine::core::Value::Empty) && c.style == gridcraft_engine::model::StyleId::DEFAULT,
    }
}

/// Puts back what one change (`from` → `to`) did to cells, on top of later changes by others:
/// each cell it changed goes back to its `from` state, provided nobody has changed that cell
/// since (it still holds its `to` state). Only a change that touched nothing but cells (values,
/// formulas, formats) can be undone this way. Undo is (before, after); redo is (after, before).
pub fn undo_cells(from: &Workbook, to: &Workbook, current: &Workbook) -> Result<Workbook, &'static str> {
    if from.sheets.len() != to.sheets.len() || from.sheets.iter().zip(&to.sheets).any(|(a, b)| a.name != b.name) {
        return Err(UNDO_LAYOUT);
    }
    // Nothing outside the sheets may differ, except styles the change added (they're kept).
    let outside = |w: &Workbook| {
        let mut c = w.clone();
        c.sheets = Vec::new();
        c.styles = from.styles.clone();
        c.active_sheet = Default::default();
        c
    };
    if outside(from) != outside(to) {
        return Err(UNDO_LAYOUT);
    }
    let mut out = current.clone();
    for (a, b) in from.sheets.iter().zip(&to.sheets) {
        if Arc::ptr_eq(a, b) {
            continue;
        }
        // Only cells: the sheet with `from`'s cells (and results that follow them) must equal `from`.
        let mut only_cells = (**b).clone();
        only_cells.cells = a.cells.clone();
        only_cells.spill = a.spill.clone();
        only_cells.spill_ranges = a.spill_ranges.clone();
        if only_cells != **a {
            return Err(UNDO_LAYOUT);
        }
        let Some(j) = current.sheet_index(&b.name) else { return Err(UNDO_LAYOUT) };
        let touched = a.cells.diff(&b.cells);
        let now = &current.sheets[j];
        if touched.iter().any(|c| !same_cell(now.cells.get(*c), b.cells.get(*c))) {
            return Err(UNDO_CHANGED);
        }
        let sh = Arc::make_mut(&mut out.sheets[j]);
        for c in touched {
            match a.cells.get(c) {
                Some(cell) => sh.cells.set(c, cell.clone()),
                None => {
                    sh.cells.remove(c);
                }
            }
        }
    }
    Ok(out)
}

/// Puts `wb` in place (keeping this side's sheet and selection) and recalculates everything.
pub fn set_workbook_recalc(s: &mut Session, wb: Workbook) {
    set_workbook(s, wb);
    if let Some(d) = s.active_mut() {
        let (wb, calc) = (&mut d.wb, &mut d.calc);
        calc.recalc_all(Arc::make_mut(wb));
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

/// Copies sheet `index` of `src` to the end of `dst` (another workbook): cell, row and column
/// formats are added to `dst`'s own style list, the copy gets a free name ("Name (2)" when taken),
/// a new id, and table names that don't clash. Formulas that point at other sheets keep their
/// text (they may show #REF! where those sheets don't exist), as in Sheets. Returns its name.
pub fn copy_sheet_into(src: &Workbook, index: usize, dst: &mut Workbook) -> Option<String> {
    use gridcraft_engine::model::StyleId;
    let mut sheet: Sheet = (**src.sheets.get(index)?).clone();
    let mut map: std::collections::HashMap<StyleId, StyleId> = std::collections::HashMap::new();
    let mut remap = |id: StyleId, dst: &mut Workbook| *map.entry(id).or_insert_with(|| if id == StyleId::DEFAULT { StyleId::DEFAULT } else { dst.styles.intern(src.styles.get(id).clone()) });
    let styled: Vec<(gridcraft_engine::core::CellRef, StyleId)> = sheet.cells.iter().filter(|(_, c)| c.style != StyleId::DEFAULT).map(|(r, c)| (r, c.style)).collect();
    for (r, id) in styled {
        let new = remap(id, dst);
        sheet.set_style(r, new);
    }
    for line in sheet.rows.values_mut().chain(sheet.cols.values_mut()) {
        if let Some(id) = line.style {
            line.style = Some(remap(id, dst));
        }
    }
    let taken = |n: &str, dst: &Workbook| dst.sheets.iter().any(|s| s.name.eq_ignore_ascii_case(n));
    let base: String = sheet.name.chars().take(26).collect();
    let mut name = sheet.name.clone();
    let mut k = 2;
    while taken(&name, dst) {
        name = format!("{base} ({k})");
        k += 1;
    }
    sheet.name = name.clone();
    sheet.uid = gridcraft_engine::model::SheetUid::fresh();
    let tables: Vec<String> = dst.sheets.iter().flat_map(|s| s.tables.iter().map(|t| t.name.to_lowercase())).collect();
    for t in &mut sheet.tables {
        let mut n = t.name.clone();
        let mut k = 2;
        while tables.contains(&n.to_lowercase()) {
            n = format!("{}_{k}", t.name);
            k += 1;
        }
        t.name = n;
    }
    dst.sheets.push(Arc::new(sheet));
    Some(name)
}
