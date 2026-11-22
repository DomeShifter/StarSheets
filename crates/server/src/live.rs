//! Live editing (docs/live-editing.md): tabs connect to /api/live with a WebSocket and name a
//! file; every open file has a room, a thread that holds the file in a GridCraft session, applies
//! changes in the order they arrive, sends them to every tab and saves the file a moment after
//! things go quiet.

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use gridcraft_engine::Session;
use gridcraft_engine::model::Workbook;
use serde_json::json;
use starsheets_sync::{self as sync, Change, ToClient, ToServer};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::Shared;
use crate::store::now;

/// Quiet time before saving, and the longest a change waits to be saved.
const SAVE_IDLE: Duration = Duration::from_secs(2);
const SAVE_MAX: Duration = Duration::from_secs(30);
/// A room with nobody in it closes after this (once saved).
const EMPTY_FOR: Duration = Duration::from_secs(10);
/// Every this many changes everyone gets a fingerprint of the whole file to check.
const CHECK_EVERY: u64 = 25;
/// Changes kept for undo.
const HISTORY: usize = 200;

enum RoomMsg {
    Join { client: String, user: Option<crate::auth::User>, out: UnboundedSender<String> },
    Leave { client: String },
    From { client: String, msg: ToServer },
    /// The file was replaced through the files API (a version restored): read it again.
    Replaced,
}

#[derive(Default)]
pub struct Rooms {
    map: Mutex<HashMap<String, Sender<RoomMsg>>>,
}

impl Rooms {
    /// Sends to the room for `file`, starting it if needed.
    fn send(&self, app: &Shared, file: &str, msg: RoomMsg) {
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        let msg = match map.get(file) {
            Some(tx) => match tx.send(msg) {
                Ok(()) => return,
                Err(e) => e.0,
            },
            None => msg,
        };
        let (tx, rx) = channel();
        let _ = tx.send(msg);
        map.insert(file.to_string(), tx);
        let (app, file) = (app.clone(), file.to_string());
        let _ = std::thread::Builder::new().name(format!("room-{file}")).spawn(move || Room::run(app, file, rx));
    }

    /// The files API replaced this file: an open room reads it again.
    pub fn replaced(&self, file: &str) {
        if let Some(tx) = self.map.lock().unwrap_or_else(|e| e.into_inner()).get(file) {
            let _ = tx.send(RoomMsg::Replaced);
        }
    }
}

// ---------------------------------------------------------------- connections

pub async fn ws(State(app): State<Shared>, headers: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    if !crate::signed_in(&app, &headers) {
        return crate::error(401, "please sign in").into_response();
    }
    let user = crate::current_user(&app, &headers);
    upgrade.max_message_size(crate::MAX_BYTES as usize).on_upgrade(move |socket| connection(app, socket, user))
}

fn text(m: &ToClient) -> String {
    serde_json::to_string(m).unwrap_or_default()
}

async fn connection(app: Shared, mut socket: WebSocket, user: Option<crate::auth::User>) {
    // The first message says which file.
    let (file, client) = loop {
        match socket.recv().await {
            Some(Ok(Message::Text(t))) => match serde_json::from_str::<ToServer>(&t) {
                Ok(ToServer::Hello { file, build, client }) => {
                    if build != sync::BUILD {
                        let _ = socket.send(Message::Text(text(&ToClient::Reload).into())).await;
                        return;
                    }
                    break (file, client);
                }
                _ => return,
            },
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
            _ => return,
        }
    };
    let (out, mut outbox) = unbounded_channel::<String>();
    app.rooms.send(&app, &file, RoomMsg::Join { client: client.clone(), user, out });
    // A ping now and then keeps proxies from closing a quiet connection.
    let mut ping = tokio::time::interval(Duration::from_secs(30));
    ping.tick().await;
    loop {
        tokio::select! {
            _ = ping.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(t))) => match serde_json::from_str::<ToServer>(&t) {
                    Ok(msg) => app.rooms.send(&app, &file, RoomMsg::From { client: client.clone(), msg }),
                    Err(e) => {
                        let _ = socket.send(Message::Text(text(&ToClient::Message { text: format!("bad message: {e}") }).into())).await;
                    }
                },
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_))) => {}
                _ => break,
            },
            outgoing = outbox.recv() => match outgoing {
                Some(t) => {
                    if socket.send(Message::Text(t.into())).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
        }
    }
    app.rooms.send(&app, &file, RoomMsg::Leave { client });
}

// ---------------------------------------------------------------- rooms

struct Client {
    out: UnboundedSender<String>,
    person: sync::Person,
    /// Where the tab is (its last Presence), sent to people who join later.
    presence: Option<(usize, gridcraft_engine::selection::Selection)>,
    /// Waiting for this change as its finished state (after NeedState); other changes from
    /// this tab are dropped until it comes, and the tab sends them again.
    blocked: Option<u64>,
    /// Changes this tab undid, newest last.
    redo: Vec<Step>,
}

#[derive(Clone)]
struct Step {
    client: String,
    before: Arc<Workbook>,
    after: Arc<Workbook>,
}

struct Room {
    app: Shared,
    file: String,
    session: Session,
    seq: u64,
    clients: HashMap<String, Client>,
    history: VecDeque<Step>,
    /// Who made each recent change, to tell whether a tab's change was made on exactly what
    /// the server had (then its fingerprint must match).
    authors: VecDeque<(u64, String)>,
    dirty_since: Option<Instant>,
    last_change: Instant,
    save_failed_at: Option<Instant>,
    empty_since: Option<Instant>,
    guests: u32,
}

impl Room {
    fn run(app: Shared, file: String, rx: Receiver<RoomMsg>) {
        let mut room = Room {
            app,
            file,
            session: Session::new(),
            seq: 0,
            clients: HashMap::new(),
            history: VecDeque::new(),
            authors: VecDeque::new(),
            dirty_since: None,
            last_change: Instant::now(),
            save_failed_at: None,
            empty_since: None,
            guests: 0,
        };
        let opened = room.open();
        loop {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(msg) => {
                    if let Err(why) = &opened {
                        // The file couldn't be read: say so to whoever joins, and close.
                        if let RoomMsg::Join { out, .. } = msg {
                            let _ = out.send(text(&ToClient::Message { text: why.clone() }));
                        }
                        room.close(&rx);
                        return;
                    }
                    room.handle(msg);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            room.save_if_due(false);
            if room.clients.is_empty() && room.dirty_since.is_none() && room.empty_since.is_some_and(|t| t.elapsed() > EMPTY_FOR) && room.close(&rx) {
                return;
            }
        }
    }

    /// Takes the room out of the list, unless a message is waiting (someone just joined).
    fn close(&mut self, rx: &Receiver<RoomMsg>) -> bool {
        let mut map = self.app.rooms.map.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(msg) = rx.try_recv() {
            drop(map);
            self.handle(msg);
            return false;
        }
        map.remove(&self.file);
        true
    }

    fn open(&mut self) -> Result<(), String> {
        let (name, bytes) = {
            let _g = self.app.lock.lock().unwrap_or_else(|e| e.into_inner());
            let index = self.app.store.synced_index().map_err(|e| format!("Can't read the file list: {e}"))?;
            let path = index.get(&self.file).cloned().ok_or("This file isn't on the server any more.")?;
            let bytes = std::fs::read(self.app.store.full(&path)).map_err(|e| format!("Can't read the file: {e}"))?;
            (crate::store::base_of(&path), bytes)
        };
        let mut s = Session::new();
        let b64 = gridcraft_engine::io::base64_encode(&bytes);
        s.execute("file.open", json!({ "name": name, "base64": b64 })).map_err(|e| format!("Can't open the file: {e}"))?;
        self.session = s;
        Ok(())
    }

    fn wb(&self) -> Option<Arc<Workbook>> {
        self.session.active().map(|d| d.wb.clone())
    }

    fn to(&self, client: &str, m: &ToClient) {
        if let Some(c) = self.clients.get(client) {
            let _ = c.out.send(text(m));
        }
    }

    fn all(&self, m: &ToClient) {
        let t = text(m);
        for c in self.clients.values() {
            let _ = c.out.send(t.clone());
        }
    }

    fn snapshot(&self) -> serde_json::Value {
        self.wb().map(|wb| sync::workbook_json(&wb)).unwrap_or_default()
    }

    fn handle(&mut self, msg: RoomMsg) {
        match msg {
            RoomMsg::Join { client, user, out } => {
                let _ = out.send(text(&ToClient::Welcome { seq: self.seq, workbook: self.snapshot() }));
                // Without sign-in (STARSHEETS_AUTH=off) people are guests, numbered as they come.
                let person = match user {
                    Some(u) => sync::Person { client: client.clone(), username: u.username, name: u.name, colour: u.colour },
                    None => {
                        self.guests += 1;
                        let colour = crate::auth::COLOURS[(self.guests as usize - 1) % crate::auth::COLOURS.len()].to_string();
                        sync::Person { client: client.clone(), username: String::new(), name: format!("Guest {}", self.guests), colour }
                    }
                };
                for (other, c) in &self.clients {
                    if let Some((sheet, sel)) = &c.presence {
                        let _ = out.send(text(&ToClient::Presence { client: other.clone(), sheet: *sheet, sel: sel.clone() }));
                    }
                }
                self.clients.insert(client, Client { out, person, presence: None, blocked: None, redo: Vec::new() });
                self.empty_since = None;
                self.people();
            }
            RoomMsg::Leave { client } => {
                self.clients.remove(&client);
                self.people();
                if self.clients.is_empty() {
                    self.empty_since = Some(Instant::now());
                    self.save_if_due(true);
                }
            }
            RoomMsg::Replaced => {
                if self.open().is_ok() {
                    self.history.clear();
                    self.dirty_since = None;
                    self.seq += 1;
                    let wb = self.snapshot();
                    self.all(&ToClient::Reset { seq: self.seq, workbook: wb });
                }
            }
            RoomMsg::From { client, msg } => match msg {
                ToServer::Change { cseq, base, change } => self.change(&client, cseq, base, change),
                ToServer::Undo => self.undo(&client),
                ToServer::Redo => self.redo(&client),
                ToServer::Resync => self.to(&client, &ToClient::Reset { seq: self.seq, workbook: self.snapshot() }),
                ToServer::Presence { sheet, sel } => {
                    if let Some(c) = self.clients.get_mut(&client) {
                        c.presence = Some((sheet, sel.clone()));
                    }
                    let m = text(&ToClient::Presence { client: client.clone(), sheet, sel });
                    for (other, c) in &self.clients {
                        if *other != client {
                            let _ = c.out.send(m.clone());
                        }
                    }
                }
                ToServer::Hello { .. } => {}
            },
        }
    }

    /// Tells everyone who has the file open (each tab learns which entry is its own).
    fn people(&self) {
        let mut people: Vec<sync::Person> = self.clients.values().map(|c| c.person.clone()).collect();
        people.sort_by(|a, b| a.name.cmp(&b.name).then(a.client.cmp(&b.client)));
        for (id, c) in &self.clients {
            let _ = c.out.send(text(&ToClient::People { people: people.clone(), you: id.clone() }));
        }
    }

    /// Whether every change after `base` came from `client`, so its change was made on exactly
    /// what the server has.
    fn same_base(&self, client: &str, base: u64) -> bool {
        base == self.seq || self.authors.iter().filter(|(s, _)| *s > base).all(|(_, c)| c == client) && self.authors.front().is_some_and(|(s, _)| *s <= base + 1)
    }

    fn change(&mut self, client: &str, cseq: u64, base: u64, change: Change) {
        let Some(c) = self.clients.get(client) else { return };
        if let Some(waiting) = c.blocked {
            if cseq != waiting || !matches!(change, Change::State { .. }) {
                return;
            }
        }
        if let Change::Cmd { id, .. } = &change
            && !sync::server_allows(id)
        {
            self.to(client, &ToClient::Rejected { cseq, why: format!("The server doesn't run \"{id}\" for a tab.") });
            return;
        }
        let Some(before) = self.wb() else { return };
        if let Err(why) = sync::apply(&mut self.session, &change) {
            sync::restore_workbook(&mut self.session, &before);
            self.to(client, &ToClient::Rejected { cseq, why });
            return;
        }
        let Some(after) = self.wb() else { return };
        if let Change::Cmd { fp, .. } = &change
            && self.same_base(client, base)
            && sync::change_fingerprint(&before, &after) != *fp
        {
            sync::restore_workbook(&mut self.session, &before);
            if let Some(c) = self.clients.get_mut(client) {
                c.blocked = Some(cseq);
            }
            self.to(client, &ToClient::NeedState { cseq });
            return;
        }
        if let Some(c) = self.clients.get_mut(client) {
            c.blocked = None;
            c.redo.clear();
        }
        self.record(client, before, after);
        self.all(&ToClient::Applied { seq: self.seq, client: client.to_string(), cseq, change });
        if self.seq.is_multiple_of(CHECK_EVERY) {
            self.check();
        }
    }

    fn record(&mut self, client: &str, before: Arc<Workbook>, after: Arc<Workbook>) {
        self.seq += 1;
        self.history.push_back(Step { client: client.to_string(), before, after });
        if self.history.len() > HISTORY {
            self.history.pop_front();
        }
        self.authors.push_back((self.seq, client.to_string()));
        if self.authors.len() > 1000 {
            self.authors.pop_front();
        }
        self.dirty_since.get_or_insert_with(Instant::now);
        self.last_change = Instant::now();
    }

    fn check(&self) {
        if let Some(wb) = self.wb() {
            self.all(&ToClient::Check { seq: self.seq, hash: sync::fingerprint(&wb) });
        }
    }

    /// Undo (first version): only your own latest change, while nobody has changed the file since.
    fn undo(&mut self, client: &str) {
        let current = self.wb();
        let ok = self.history.back().is_some_and(|s| s.client == client && current.as_ref().is_some_and(|c| Arc::ptr_eq(c, &s.after)));
        if !ok {
            let text = if self.history.iter().any(|s| s.client == client) {
                "Can't undo: someone else has changed the file since your last change."
            } else {
                "Nothing to undo."
            };
            self.to(client, &ToClient::Message { text: text.into() });
            return;
        }
        let Some(step) = self.history.pop_back() else { return };
        sync::restore_workbook(&mut self.session, &step.before);
        self.after_history_move();
        if let Some(c) = self.clients.get_mut(client) {
            c.redo.push(step);
        }
    }

    fn redo(&mut self, client: &str) {
        let current = self.wb();
        let ok = self.clients.get(client).and_then(|c| c.redo.last()).is_some_and(|s| current.as_ref().is_some_and(|c| Arc::ptr_eq(c, &s.before)));
        if !ok {
            self.to(client, &ToClient::Message { text: "Nothing to redo.".into() });
            return;
        }
        let Some(step) = self.clients.get_mut(client).and_then(|c| c.redo.pop()) else { return };
        sync::restore_workbook(&mut self.session, &step.after);
        self.history.push_back(step);
        self.after_history_move();
    }

    fn after_history_move(&mut self) {
        self.seq += 1;
        self.authors.push_back((self.seq, String::new()));
        self.dirty_since.get_or_insert_with(Instant::now);
        self.last_change = Instant::now();
        let wb = self.snapshot();
        self.all(&ToClient::Reset { seq: self.seq, workbook: wb });
    }

    fn save_if_due(&mut self, now_please: bool) {
        let Some(since) = self.dirty_since else { return };
        if self.save_failed_at.is_some_and(|t| t.elapsed() < Duration::from_secs(10)) {
            return;
        }
        if !now_please && self.last_change.elapsed() < SAVE_IDLE && since.elapsed() < SAVE_MAX {
            return;
        }
        let Some(wb) = self.wb() else { return };
        match self.save(&wb) {
            Ok(()) => {
                self.dirty_since = None;
                self.save_failed_at = None;
                self.all(&ToClient::Saved { at: now() as i64 });
            }
            Err(why) => {
                self.save_failed_at = Some(Instant::now());
                self.all(&ToClient::Message { text: format!("Couldn't save: {why}") });
            }
        }
    }

    fn save(&self, wb: &Workbook) -> Result<(), String> {
        let _g = self.app.lock.lock().unwrap_or_else(|e| e.into_inner());
        let index = self.app.store.synced_index().map_err(|e| e.to_string())?;
        let path = index.get(&self.file).cloned().ok_or("the file was moved to the bin")?;
        let bytes = gridcraft_engine::io::save_bytes(wb, &crate::store::base_of(&path)).map_err(|e| e.to_string())?;
        let full = self.app.store.full(&path);
        self.app.store.keep_version(&self.file, &full).map_err(|e| e.to_string())?;
        self.app.store.write_file(&full, &bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use crate::store::Store;
    use tokio::sync::mpsc::UnboundedReceiver;

    struct Tab {
        name: String,
        rx: UnboundedReceiver<String>,
        session: Session,
        seq: u64,
        cseq: u64,
        /// Who-is-here messages, kept aside.
        seen: Vec<ToClient>,
    }

    fn app() -> (Shared, String, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("starsheets-live-{}", crate::store::random_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("t.csv"), "a,b\n1,2\n").unwrap();
        let store = Store { root: dir.clone(), keep: 3 };
        let id = store.synced_index().unwrap().keys().next().unwrap().clone();
        let app = Arc::new(App { store, lock: Mutex::new(()), location: String::new(), auth: false, site: None, rooms: Rooms::default() });
        (app, id, dir)
    }

    fn next(t: &mut Tab) -> ToClient {
        for _ in 0..400 {
            if let Ok(m) = t.rx.try_recv() {
                let m: ToClient = serde_json::from_str(&m).unwrap();
                if matches!(m, ToClient::People { .. } | ToClient::Presence { .. }) {
                    t.seen.push(m);
                    continue;
                }
                return m;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("{}: nothing from the server", t.name);
    }

    fn join(app: &Shared, file: &str, name: &str) -> Tab {
        let (out, rx) = unbounded_channel();
        app.rooms.send(app, file, RoomMsg::Join { client: name.into(), user: None, out });
        let mut t = Tab { name: name.into(), rx, session: Session::new(), seq: 0, cseq: 0, seen: Vec::new() };
        match next(&mut t) {
            ToClient::Welcome { seq, workbook } => {
                t.session.execute("file.new", json!({})).unwrap();
                sync::set_workbook(&mut t.session, sync::workbook_from_json(&workbook, None).unwrap());
                t.seq = seq;
            }
            m => panic!("expected welcome, got {m:?}"),
        }
        t.session.sync_log = Some(Vec::new());
        t
    }

    /// Runs a command in the tab and sends what it logged, as the web app does.
    fn edit(app: &Shared, file: &str, t: &mut Tab, id: &str, p: serde_json::Value) {
        t.session.execute(id, p).unwrap();
        for st in t.session.sync_log.replace(Vec::new()).unwrap() {
            let change = if sync::replayable(&st.id) && sync::server_allows(&st.id) {
                let fp = sync::change_fingerprint(&st.before, &st.after);
                Change::Cmd { sheet: st.sheet, sel: st.selection, id: st.id, params: st.params, locale: sync::InputLocale::current(), fp }
            } else {
                Change::State { patch: sync::make_patch(&st.before, &st.after) }
            };
            t.cseq += 1;
            app.rooms.send(app, file, RoomMsg::From { client: t.name.clone(), msg: ToServer::Change { cseq: t.cseq, base: t.seq, change } });
        }
    }

    /// Takes one Applied: the sender just notes the revision, others apply the change.
    fn take_applied(t: &mut Tab) -> (String, u64) {
        match next(t) {
            ToClient::Applied { seq, client, cseq, change } => {
                if client != t.name {
                    sync::apply(&mut t.session, &change).unwrap();
                }
                t.seq = seq;
                (client, cseq)
            }
            m => panic!("{}: expected applied, got {m:?}", t.name),
        }
    }

    fn print(t: &Tab) -> String {
        sync::fingerprint(&t.session.active().unwrap().wb)
    }

    #[test]
    fn two_tabs_stay_identical_and_the_file_is_saved() {
        let (app, file, dir) = app();
        let mut a = join(&app, &file, "a");
        let mut b = join(&app, &file, "b");
        edit(&app, &file, &mut a, "cell.set", json!({"cell": "C1", "input": "from a"}));
        edit(&app, &file, &mut b, "cell.set", json!({"cell": "C2", "input": "from b"}));
        for _ in 0..2 {
            take_applied(&mut a);
            take_applied(&mut b);
        }
        assert_eq!(print(&a), print(&b));
        // A paste travels as finished state.
        a.session.sync_log = None;
        a.session.execute("selection.set", json!({"range": "A1:B2"})).unwrap();
        a.session.execute("edit.copy", json!({})).unwrap();
        a.session.execute("selection.set", json!({"range": "E5"})).unwrap();
        a.session.sync_log = Some(Vec::new());
        edit(&app, &file, &mut a, "edit.paste", json!({}));
        take_applied(&mut a);
        take_applied(&mut b);
        assert_eq!(print(&a), print(&b));
        // Leaving saves at once.
        app.rooms.send(&app, &file, RoomMsg::Leave { client: "a".into() });
        app.rooms.send(&app, &file, RoomMsg::Leave { client: "b".into() });
        std::thread::sleep(Duration::from_millis(400));
        let saved = std::fs::read_to_string(dir.join("t.csv")).unwrap();
        assert!(saved.contains("from a") && saved.contains("from b"), "{saved}");
        assert_eq!(std::fs::read_dir(dir.join(".starsheets/versions").join(&file)).unwrap().count(), 1, "the old copy is kept");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_wrong_replay_asks_for_the_finished_state() {
        let (app, file, dir) = app();
        let mut a = join(&app, &file, "a");
        let change = Change::Cmd {
            sheet: 0,
            sel: gridcraft_engine::selection::Selection::default(),
            id: "cell.set".into(),
            params: json!({"cell": "A1", "input": "x"}),
            locale: sync::InputLocale::current(),
            fp: "not the right one".into(),
        };
        app.rooms.send(&app, &file, RoomMsg::From { client: "a".into(), msg: ToServer::Change { cseq: 1, base: 0, change } });
        assert!(matches!(next(&mut a), ToClient::NeedState { cseq: 1 }));
        // Until the state comes, its next change is dropped; the state is accepted.
        edit(&app, &file, &mut a, "cell.set", json!({"cell": "B9", "input": "later"}));
        let wb = a.session.active().unwrap().wb.clone();
        let patch = sync::make_patch(&wb, &wb);
        app.rooms.send(&app, &file, RoomMsg::From { client: "a".into(), msg: ToServer::Change { cseq: 1, base: 0, change: Change::State { patch } } });
        assert_eq!(take_applied(&mut a), ("a".into(), 1));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn people_and_where_they_are() {
        let (app, file, dir) = app();
        let mut a = join(&app, &file, "a");
        let sel = gridcraft_engine::selection::Selection::default();
        app.rooms.send(&app, &file, RoomMsg::From { client: "a".into(), msg: ToServer::Presence { sheet: 0, sel: sel.clone() } });
        let mut b = join(&app, &file, "b");
        // b hears where a already is, and both hear that two people are here.
        std::thread::sleep(Duration::from_millis(100));
        while let Ok(m) = b.rx.try_recv() {
            b.seen.push(serde_json::from_str(&m).unwrap());
        }
        assert!(b.seen.iter().any(|m| matches!(m, ToClient::Presence { client, .. } if client == "a")), "{:?}", b.seen.len());
        assert!(b.seen.iter().any(|m| matches!(m, ToClient::People { people, you } if people.len() == 2 && you == "b")));
        while let Ok(m) = a.rx.try_recv() {
            a.seen.push(serde_json::from_str(&m).unwrap());
        }
        assert!(a.seen.iter().any(|m| matches!(m, ToClient::People { people, .. } if people.iter().any(|p| p.name == "Guest 2"))));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn undo_only_your_own_latest_change() {
        let (app, file, dir) = app();
        let mut a = join(&app, &file, "a");
        let mut b = join(&app, &file, "b");
        let start = print(&a);
        edit(&app, &file, &mut a, "cell.set", json!({"cell": "A5", "input": "a's"}));
        take_applied(&mut a);
        take_applied(&mut b);
        app.rooms.send(&app, &file, RoomMsg::From { client: "b".into(), msg: ToServer::Undo });
        assert!(matches!(next(&mut b), ToClient::Message { .. }));
        app.rooms.send(&app, &file, RoomMsg::From { client: "a".into(), msg: ToServer::Undo });
        for t in [&mut a, &mut b] {
            match next(t) {
                ToClient::Reset { workbook, .. } => sync::set_workbook(&mut t.session, sync::workbook_from_json(&workbook, None).unwrap()),
                m => panic!("expected reset, got {m:?}"),
            }
        }
        assert_eq!(print(&a), start);
        assert_eq!(print(&b), start);
        let _ = std::fs::remove_dir_all(dir);
    }
}
