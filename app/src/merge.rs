//! Merging cells the Google Sheets way: Merge all, Merge vertically, Merge horizontally and
//! Unmerge, each greyed out when it can't apply to the selection, and Google's "Heads up!" warning
//! when merging would drop values (only the top-left one of each merged block is kept), with
//! "Don't show this again for 5 minutes".

use gridcraft_engine::core::{CellRef, RangeRef};
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

/// Asks for a merge command, through egui's memory (menus and toolbar set it).
pub const REQUEST: &str = "starsheets_merge";

#[derive(Default)]
pub struct MergeGuard {
    pending: Option<String>,
    dont_ask: bool,
    /// When (in seconds, egui's clock) the warning may show again.
    quiet_until: f64,
}

/// Whether a merge command can do anything to the selection (as Sheets greys out the rest).
pub fn enabled(app: &SheetApp, cmd: &str) -> bool {
    let Some(d) = app.session.active() else { return false };
    let ranges = &d.selection.ranges;
    match cmd {
        "home.unmergeCells" => d.wb.active().is_some_and(|sh| sh.merges.iter().any(|m| ranges.iter().any(|r| m.intersects(r)))),
        "home.mergeDown" => ranges.iter().any(|r| r.end.row > r.start.row),
        "home.mergeAcross" => ranges.iter().any(|r| r.end.col > r.start.col),
        _ => ranges.iter().any(|r| !r.is_single()),
    }
}

/// The blocks a merge command makes from the selection.
fn blocks(r: RangeRef, cmd: &str) -> Vec<RangeRef> {
    match cmd {
        "home.mergeDown" => (r.start.col..=r.end.col).map(|c| RangeRef::new(CellRef::new(r.start.row, c), CellRef::new(r.end.row, c))).collect(),
        "home.mergeAcross" => (r.start.row..=r.end.row).map(|w| RangeRef::new(CellRef::new(w, r.start.col), CellRef::new(w, r.end.col))).collect(),
        _ => vec![r],
    }
}

/// Whether merging would drop a value: some block has something besides its top-left cell.
fn loses_data(app: &SheetApp, cmd: &str) -> bool {
    let Some(d) = app.session.active() else { return false };
    let Some(sh) = d.wb.active() else { return false };
    d.selection.ranges.iter().flat_map(|r| blocks(*r, cmd)).any(|b| {
        sh.cells.iter_range(b).any(|(c, cell)| c != b.start && (cell.formula.is_some() || !cell.value.is_empty()))
    })
}

impl MergeGuard {
    pub fn ui(&mut self, app: &mut SheetApp, ctx: &egui::Context) {
        if let Some(cmd) = ctx.data_mut(|m| m.remove_temp::<String>(egui::Id::new(REQUEST))) {
            let now = ctx.input(|i| i.time);
            if cmd != "home.unmergeCells" && now >= self.quiet_until && loses_data(app, &cmd) {
                self.pending = Some(cmd);
                self.dont_ask = false;
            } else {
                app.run_or_alert(&cmd, json!({}));
            }
        }
        let Some(cmd) = self.pending.clone() else { return };
        let mut answer = None;
        let mut closed = false;
        crate::house::dialog(ctx, "starsheets_merge_warning", 460.0, "Heads up!", Some(&mut closed), |ui| {
            crate::house::text(ui, "Merging cells will only preserve the top-leftmost value. Merge anyway?");
            ui.checkbox(&mut self.dont_ask, "Don't show this again for 5 minutes");
            match crate::house::actions(ui, &["Cancel", "OK"]) {
                Some(0) => answer = Some(false),
                Some(_) => answer = Some(true),
                None => {}
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                answer = Some(false);
            }
        });
        if closed {
            answer = Some(false);
        }
        match answer {
            Some(true) => {
                if self.dont_ask {
                    self.quiet_until = ctx.input(|i| i.time) + 300.0;
                }
                self.pending = None;
                app.run_or_alert(&cmd, json!({}));
            }
            Some(false) => self.pending = None,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use gridcraft_engine::Session;
    use gridcraft_engine::core::{CellRef, RangeRef, Value};
    use serde_json::json;

    #[test]
    fn merge_vertically_makes_one_block_per_column() {
        let mut s = Session::new();
        s.execute("file.new", json!({})).unwrap();
        for (c, v) in [("A1", "1"), ("A2", "2"), ("B1", "3"), ("B2", "4")] {
            s.execute("cell.set", json!({"cell": c, "input": v})).unwrap();
        }
        s.execute("home.mergeDown", json!({"range": "A1:B2"})).unwrap();
        let sh = s.active().unwrap().wb.active().unwrap().clone();
        let mut merges: Vec<String> = sh.merges.iter().map(RangeRef::a1).collect();
        merges.sort();
        assert_eq!(merges, ["A1:A2", "B1:B2"]);
        assert_eq!(sh.value(CellRef::parse("B1").unwrap()), Value::number(3.0));
        assert_eq!(sh.value(CellRef::parse("A2").unwrap()), Value::Empty);
    }
}
