use super::*;
use serde_json::json;

/// A session with a blank workbook and the sync log on.
fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({})).unwrap();
    s.sync_log = Some(Vec::new());
    s
}

/// A second session holding a copy of `a`'s workbook, as a tab gets it from the server.
fn copy_of(a: &Session) -> Session {
    let mut b = Session::new();
    b.execute("file.new", json!({})).unwrap();
    let wb = workbook_from_json(&workbook_json(&a.active().unwrap().wb), None).unwrap();
    set_workbook(&mut b, wb);
    b
}

fn wb(s: &Session) -> Arc<Workbook> {
    s.active().unwrap().wb.clone()
}

/// What a tab sends for each step its session logged.
fn changes(a: &mut Session) -> Vec<Change> {
    let locale = InputLocale::current();
    a.sync_log
        .replace(Vec::new())
        .unwrap()
        .into_iter()
        .map(|st| {
            if replayable(&st.id) {
                let fp = change_fingerprint(&st.before, &st.after);
                Change::Cmd { sheet: st.sheet, sel: st.selection, id: st.id, params: st.params, locale: locale.clone(), fp }
            } else {
                Change::State { patch: make_patch(&st.before, &st.after) }
            }
        })
        .collect()
}

/// Replays `a`'s changes in `b` and checks every one gives the same result.
fn replay(a: &mut Session, b: &mut Session) -> usize {
    let cs = changes(a);
    let n = cs.len();
    for c in cs {
        let before = wb(b);
        apply(b, &c).unwrap();
        if let Change::Cmd { fp, id, .. } = &c {
            assert_eq!(&change_fingerprint(&before, &wb(b)), fp, "replaying {id} gave a different result");
        }
    }
    assert_eq!(fingerprint(&wb(a)), fingerprint(&wb(b)));
    n
}

#[test]
fn commands_replay_to_the_same_workbook() {
    let mut a = session();
    let mut b = copy_of(&a);
    for (id, p) in [
        ("cell.set", json!({"cell": "A1", "input": "Item"})),
        ("cell.set", json!({"cell": "B1", "input": "12.5"})),
        ("cell.set", json!({"cell": "B2", "input": "=B1*2"})),
        ("cell.set", json!({"cell": "C3", "input": "=NOW()"})),
        ("home.bold", json!({"range": "A1:B1"})),
        ("home.insertRows", json!({"range": "A1"})),
        ("home.mergeCells", json!({"range": "D1:E2"})),
        ("sheet.rename", json!({"name": "Budget"})),
        ("home.insertSheet", json!({})),
    ] {
        a.execute(id, p).unwrap();
    }
    assert_eq!(replay(&mut a, &mut b), 9);
}

#[test]
fn a_change_uses_its_own_selection_not_the_tabs() {
    let mut a = session();
    let mut b = copy_of(&a);
    a.execute("selection.set", json!({"range": "B2:C3"})).unwrap();
    a.execute("home.bold", json!({})).unwrap();
    b.execute("selection.set", json!({"range": "F9"})).unwrap();
    assert_eq!(replay(&mut a, &mut b), 1);
    assert_eq!(b.active().unwrap().selection.active, b.active().unwrap().selection.ranges[0].start, "b keeps its own place");
}

#[test]
fn a_paste_travels_as_its_finished_state() {
    let mut a = session();
    a.execute("cell.set", json!({"cell": "A1", "input": "copy me"})).unwrap();
    let mut b = copy_of(&a);
    a.sync_log = Some(Vec::new());
    a.execute("selection.set", json!({"range": "A1"})).unwrap();
    a.execute("edit.copy", json!({})).unwrap();
    a.execute("selection.set", json!({"range": "C5"})).unwrap();
    a.execute("edit.paste", json!({})).unwrap();
    let cs = changes(&mut a);
    assert!(cs.iter().any(|c| matches!(c, Change::State { .. })), "{cs:?}");
    for c in &cs {
        apply(&mut b, c).unwrap();
    }
    assert_eq!(fingerprint(&wb(&a)), fingerprint(&wb(&b)));
}

#[test]
fn replaying_leaves_history_and_log_alone() {
    let mut a = session();
    let mut b = copy_of(&a);
    b.execute("cell.set", json!({"cell": "Z1", "input": "mine"})).unwrap();
    b.sync_log = Some(Vec::new());
    let undo = b.active().unwrap().undo.len();
    a.execute("cell.set", json!({"cell": "A1", "input": "theirs"})).unwrap();
    for c in changes(&mut a) {
        apply(&mut b, &c).unwrap();
    }
    assert_eq!(b.active().unwrap().undo.len(), undo);
    assert!(b.sync_log.as_ref().unwrap().is_empty());
}

#[test]
fn the_server_runs_only_editing_commands() {
    for id in ["cell.set", "home.bold", "home.insertRows", "edit.fillDown"] {
        assert!(server_allows(id), "{id}");
    }
    for id in ["file.saveAs", "file.open", "macro.run", "automate.runScript", "no.such.command", "selection.set"] {
        assert!(!server_allows(id), "{id}");
    }
}

#[test]
fn messages_round_trip() {
    let m = ToServer::Change { cseq: 3, base: 9, change: Change::State { patch: json!({"sheets": []}) } };
    let t = serde_json::to_string(&m).unwrap();
    assert!(matches!(serde_json::from_str::<ToServer>(&t).unwrap(), ToServer::Change { cseq: 3, base: 9, .. }));
}

/// Random command runs replay to identical workbooks (seeds 1-8, 150 steps each).
#[test]
fn random_runs_replay_exactly() {
    for seed in 1..=8u64 {
        let mut r = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut next = |n: u64| {
            r ^= r << 13;
            r ^= r >> 7;
            r ^= r << 17;
            r % n
        };
        let mut a = session();
        let mut b = copy_of(&a);
        for _ in 0..150 {
            let cell = format!("{}{}", (b'A' + next(8) as u8) as char, 1 + next(12));
            let range = format!("{cell}:{}{}", (b'A' + 8 + next(3) as u8) as char, 13 + next(5));
            let (id, p) = match next(11) {
                0..=2 => ("cell.set", json!({"cell": cell, "input": format!("{}", next(1000))})),
                3 => ("cell.set", json!({"cell": cell, "input": format!("=SUM(A1:A{})+{}", 1 + next(9), next(9))})),
                4 => ("home.bold", json!({"range": range})),
                5 => ("home.insertRows", json!({"range": cell})),
                6 => ("home.deleteRows", json!({"range": cell})),
                7 => ("home.insertColumns", json!({"range": cell})),
                8 => ("edit.clearContents", json!({"range": range})),
                9 => ("edit.fillDown", json!({"range": range})),
                _ => ("home.fillColor", json!({"range": range, "color": "#FFCC00"})),
            };
            let _ = a.execute(id, p);
        }
        let n = replay(&mut a, &mut b);
        assert!(n > 100, "seed {seed}: only {n} of 150 commands changed the workbook");
    }
}
