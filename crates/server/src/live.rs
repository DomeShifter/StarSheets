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
/// How often an open file is checked for changes made outside StarSheets (Excel, a file share).
#[cfg(not(test))]
const DISK_CHECK: Duration = Duration::from_secs(3);
#[cfg(test)]
const DISK_CHECK: Duration = Duration::from_millis(200);
/// A room with nobody in it closes after this (once saved).
const EMPTY_FOR: Duration = Duration::from_secs(10);
/// Every this many changes everyone gets a fingerprint of the whole file to check.
const CHECK_EVERY: u64 = 25;
/// Changes kept for undo.
const HISTORY: usize = 200;

enum RoomMsg {
    Join { client: String, user: Option<crate::auth::User>, can_edit: bool, out: UnboundedSender<String> },
    Leave { client: String },
    From { client: String, msg: ToServer },
    /// The file was replaced through the files API (a version restored): read it again.
    Replaced,
    /// Copy every sheet of this spreadsheet file (bytes, name) to the end of the room's file;
    /// answers with the new sheets' names.
    AddSheets { name: String, bytes: Vec<u8>, by: String, reply: std::sync::mpsc::Sender<Result<Vec<String>, String>> },
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

    /// Copies sheets into `file` through its room (starting one if needed), so everyone with it
    /// open sees them at once; waits up to 30 s for the answer.
    pub fn add_sheets(&self, app: &Shared, file: &str, name: String, bytes: Vec<u8>, by: String) -> Result<Vec<String>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.send(app, file, RoomMsg::AddSheets { name, bytes, by, reply: tx });
        rx.recv_timeout(Duration::from_secs(30)).map_err(|_| "The spreadsheet didn't answer.".to_string())?
    }

    /// The files API replaced this file: an open room reads it again.
    /// How many files are open in rooms right now.
    pub fn open_count(&self) -> usize {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

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

/// A person for the log: their username, or "a guest" with sign-in off.
fn who(user: &Option<crate::auth::User>) -> String {
    user.as_ref().map_or_else(|| "a guest".to_string(), |u| u.username.clone())
}

async fn connection(app: Shared, mut socket: WebSocket, user: Option<crate::auth::User>) {
    // The first message says which file.
    let (file, client) = loop {
        match socket.recv().await {
            Some(Ok(Message::Text(t))) => match serde_json::from_str::<ToServer>(&t) {
                Ok(ToServer::Hello { file, build, client, protocol }) => {
                    // Only a different protocol needs the tab to reload (not every new build).
                    if protocol != sync::PROTOCOL {
                        crate::log::note(&app.store, &format!("tab of {} on an older StarSheets (build {build}) asked to reload", who(&user)));
                        let _ = socket.send(Message::Text(text(&ToClient::Reload).into())).await;
                        return;
                    }
                    break (file, client);
                }
                // A tab older than protocol 3 sends `{"t": "hello", …}` without "c": it reads our
                // Reload (`{"t": "reload"}`) the same as before, and gets the new app.
                Err(_) if t.contains("\"hello\"") => {
                    crate::log::note(&app.store, &format!("tab of {} on an older StarSheets asked to reload", who(&user)));
                    let _ = socket.send(Message::Text(text(&ToClient::Reload).into())).await;
                    return;
                }
                _ => return,
            },
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
            _ => return,
        }
    };
    // Only people the file is shared with get in; viewers can't change it.
    let role = {
        let (app, user, file) = (app.clone(), user.clone(), file.clone());
        tokio::task::spawn_blocking(move || {
            let _g = app.lock.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(u) = &user {
                let _ = crate::spaces::note_opened(&app.store, u, &file);
            }
            let name = app.store.synced_index().ok().and_then(|i| i.get(&file).cloned()).unwrap_or_else(|| file.clone());
            crate::log::note(&app.store, &format!("{} opened {name}", who(&user)));
            crate::sharing::file_role(&app.store, app.auth, user.as_ref(), &file)
        })
        .await
        .unwrap_or_default()
    };
    if role == crate::sharing::Role::None {
        let _ = socket.send(Message::Text(text(&ToClient::Message { text: "You don't have access to this file.".into() }).into())).await;
        return;
    }
    let can_edit = role >= crate::sharing::Role::Edit;
    let (out, mut outbox) = unbounded_channel::<String>();
    app.rooms.send(&app, &file, RoomMsg::Join { client: client.clone(), user, can_edit, out });
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
    can_edit: bool,
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
    /// The file's version on disk when we last read or wrote it, and when we last looked.
    disk: Option<String>,
    disk_checked: Instant,
    /// When the preview picture was last drawn (at most once a minute while people edit).
    thumb_at: Option<Instant>,
    /// Where the file is (its path in the store), to notice it being moved or binned.
    path: Option<String>,
    /// Who made the last change not yet saved (for "Last changed by" in Details).
    last_editor: Option<String>,
    /// The last "couldn't save" told to everyone, so it isn't repeated every 10 s.
    told: Option<String>,
}

/// Why a save didn't happen.
enum SaveError {
    /// The file changed on disk since we read it: never write over that.
    Outside,
    Failed(String),
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
            empty_since: Some(Instant::now()), // until someone joins (a room opened only to add sheets closes too)
            guests: 0,
            disk: None,
            disk_checked: Instant::now(),
            thumb_at: None,
            path: None,
            last_editor: None,
            told: None,
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
            if room.disk_checked.elapsed() >= DISK_CHECK {
                room.check_disk();
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
            let full = self.app.store.full(&path);
            self.path = Some(path.clone());
            let bytes = std::fs::read(&full).map_err(|e| format!("Can't read the file: {e}"))?;
            self.disk = self.app.store.version_of(&full).ok();
            self.disk_checked = Instant::now();
            (crate::store::base_of(&path), bytes)
        };
        let mut s = Session::new();
        let b64 = gridcraft_engine::io::base64_encode(&bytes);
        s.execute("file.open", json!({ "name": name, "base64": b64 })).map_err(|e| format!("Can't open the file: {e}"))?;
        // Google Sheets' Clip lives in StarSheets' section of the file: back onto the cells.
        if let Some(d) = s.active_mut() {
            sync::clip_from_host(Arc::make_mut(&mut d.wb));
        }
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

    fn snapshot(&self) -> sync::Whole {
        match self.wb() {
            Some(wb) => sync::workbook_json(&wb),
            None => serde_json::value::to_raw_value(&serde_json::Value::Null).expect("null"),
        }
    }

    fn handle(&mut self, msg: RoomMsg) {
        match msg {
            RoomMsg::Join { client, user, can_edit, out } => {
                let _ = out.send(text(&ToClient::Welcome { seq: self.seq, workbook: self.snapshot(), can_edit }));
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
                self.clients.insert(client, Client { out, person, can_edit, presence: None, blocked: None, redo: Vec::new() });
                self.empty_since = None;
                self.people();
            }
            RoomMsg::Leave { client } => {
                if let Some(c) = self.clients.get(&client) {
                    let name = if c.person.username.is_empty() { c.person.name.clone() } else { c.person.username.clone() };
                    crate::log::note(&self.app.store, &format!("{name} closed {}", self.path.clone().unwrap_or_default()));
                }
                self.clients.remove(&client);
                self.people();
                if self.clients.is_empty() {
                    self.empty_since = Some(Instant::now());
                    self.save_if_due(true);
                }
            }
            RoomMsg::Replaced => self.reload(),
            RoomMsg::AddSheets { name, bytes, by, reply } => {
                let _ = reply.send(self.add_sheets(&name, &bytes, &by));
            }
            RoomMsg::From { client, msg } if matches!(msg, ToServer::Change { .. } | ToServer::Undo | ToServer::Redo) && !self.clients.get(&client).is_some_and(|c| c.can_edit) => {
                if let ToServer::Change { cseq, .. } = msg {
                    self.to(&client, &ToClient::Rejected { cseq, why: gridcraft_engine::VIEW_ONLY.into() });
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
            if !c.person.username.is_empty() {
                self.last_editor = Some(c.person.username.clone());
            }
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

    /// Undo of your own latest change. When it's also the file's latest change the whole file goes
    /// back; otherwise the cells it changed go back, as long as it changed only cells and nobody
    /// has changed those cells since (`sync::undo_cells`).
    fn undo(&mut self, client: &str) {
        let Some(k) = self.history.iter().rposition(|s| s.client == client) else {
            self.to(client, &ToClient::Message { text: "Nothing to undo.".into() });
            return;
        };
        let Some(current) = self.wb() else { return };
        let step = self.history[k].clone();
        if k + 1 == self.history.len() && Arc::ptr_eq(&current, &step.after) {
            sync::restore_workbook(&mut self.session, &step.before);
        } else {
            match sync::undo_cells(&step.before, &step.after, &current) {
                Ok(wb) => sync::set_workbook_recalc(&mut self.session, wb),
                Err(why) => {
                    self.to(client, &ToClient::Message { text: why.into() });
                    return;
                }
            }
        }
        self.history.remove(k);
        if let Some(c) = self.clients.get_mut(client) {
            c.redo.push(step);
        }
        self.server_change(current);
    }

    /// Redo of what you undid, the same way round.
    fn redo(&mut self, client: &str) {
        let Some(step) = self.clients.get(client).and_then(|c| c.redo.last()).cloned() else {
            self.to(client, &ToClient::Message { text: "Nothing to redo.".into() });
            return;
        };
        let Some(current) = self.wb() else { return };
        if Arc::ptr_eq(&current, &step.before) {
            sync::restore_workbook(&mut self.session, &step.after);
        } else {
            match sync::undo_cells(&step.after, &step.before, &current) {
                Ok(wb) => sync::set_workbook_recalc(&mut self.session, wb),
                Err(why) => {
                    self.to(client, &ToClient::Message { text: why.replace("undo", "redo") });
                    return;
                }
            }
        }
        if let Some(c) = self.clients.get_mut(client) {
            c.redo.pop();
        }
        self.history.push_back(step);
        self.server_change(current);
    }

    /// A change the server made (undo, redo): everyone applies it, as the finished state of what
    /// it changed (sender "" = the server).
    fn server_change(&mut self, before: Arc<Workbook>) {
        let Some(after) = self.wb() else { return };
        self.seq += 1;
        self.authors.push_back((self.seq, String::new()));
        self.dirty_since.get_or_insert_with(Instant::now);
        self.last_change = Instant::now();
        let patch = sync::make_patch(&before, &after);
        self.all(&ToClient::Applied { seq: self.seq, client: String::new(), cseq: 0, change: Change::State { patch } });
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
                self.told = None;
                self.all(&ToClient::Saved { at: now() as i64 });
                if self.clients.is_empty() || self.thumb_at.is_none_or(|t| t.elapsed() > Duration::from_secs(60)) {
                    self.thumb_at = Some(Instant::now());
                    if let (Some(v), Ok(png)) = (self.disk.clone(), crate::thumbs::draw(&wb)) {
                        let _ = crate::thumbs::keep(&self.app.store, &self.file, &v, &png);
                    }
                }
            }
            Err(SaveError::Outside) => self.check_disk(),
            Err(SaveError::Failed(why)) => {
                self.save_failed_at = Some(Instant::now());
                if self.told.as_deref() != Some(why.as_str()) {
                    crate::log::note(&self.app.store, &format!("couldn't save {}: {why}", self.path.clone().unwrap_or_default()));
                }
                // Moved to the bin: check_disk says so once, in plain words; other failures are
                // told once each, not every 10 s.
                if self.disk_now().is_some() && self.told.as_deref() != Some(why.as_str()) {
                    self.all(&ToClient::Message { text: format!("Couldn't save: {why}") });
                    self.told = Some(why);
                }
            }
        }
    }

    fn save(&mut self, wb: &Workbook) -> Result<(), SaveError> {
        let _g = self.app.lock.lock().unwrap_or_else(|e| e.into_inner());
        let failed = |e: String| SaveError::Failed(e);
        let index = self.app.store.synced_index().map_err(|e| failed(e.to_string()))?;
        let path = index.get(&self.file).cloned().ok_or_else(|| failed("the file was moved to the bin".into()))?;
        let full = self.app.store.full(&path);
        if self.app.store.version_of(&full).ok() != self.disk {
            return Err(SaveError::Outside);
        }
        // Cells with Google Sheets' Clip are listed in StarSheets' section (Excel has no such setting).
        let mut wb = wb.clone();
        sync::clip_into_host(&mut wb);
        let bytes = gridcraft_engine::io::save_bytes(&wb, &crate::store::base_of(&path)).map_err(|e| failed(e.to_string()))?;
        self.app.store.keep_version(&self.file, &full).map_err(|e| failed(e.to_string()))?;
        self.app.store.write_file(&full, &bytes).map_err(|e| failed(e.to_string()))?;
        self.disk = self.app.store.version_of(&full).ok();
        let editor = self.last_editor.take();
        if let Some(who) = &editor {
            let _ = crate::spaces::note_changed(&self.app.store, &self.file, who);
        }
        crate::log::note(&self.app.store, &format!("saved {path} (last change by {})", editor.as_deref().unwrap_or("someone")));
        Ok(())
    }

    /// The file's version on disk now (None: gone, moved to the bin, or unreadable).
    fn disk_now(&self) -> Option<String> {
        let _g = self.app.lock.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.app.store.synced_index().ok()?.get(&self.file).cloned()?;
        self.app.store.version_of(&self.app.store.full(&path)).ok()
    }

    /// Someone may have changed the file outside StarSheets (in Excel, over a file share). If so,
    /// their version wins the file: changes made here that weren't saved yet go into a copy next
    /// to it, and every tab is shown the file as it is on disk now. Nothing is written over.
    fn check_disk(&mut self) {
        self.disk_checked = Instant::now();
        self.check_place();
        let now_on_disk = self.disk_now();
        if now_on_disk.is_none() || now_on_disk == self.disk {
            return;
        }
        let kept = if self.dirty_since.is_some() { self.wb().map(|wb| self.keep_copy(&wb)) } else { None };
        crate::log::note(&self.app.store, &format!("{} changed outside StarSheets; {}", self.path.clone().unwrap_or_default(), match &kept {
            Some(Ok(n)) => format!("unsaved changes kept in {n}"),
            Some(Err(e)) => format!("unsaved changes could NOT be kept: {e}"),
            None => "nothing was unsaved".into(),
        }));
        if matches!(kept, Some(Ok(_))) {
            self.dirty_since = None; // kept: don't make a second copy if reading fails below
        }
        self.reload();
        let text = match kept {
            Some(Ok(name)) => format!("This file was changed outside StarSheets (for example in Excel). You're now seeing that version; the changes made here in the last few seconds are kept in \u{201c}{name}\u{201d} in the same folder."),
            Some(Err(why)) => format!("This file was changed outside StarSheets, and the last few seconds of changes made here couldn't be kept: {why}"),
            None => "This file was changed outside StarSheets (for example in Excel), so it was opened again.".to_string(),
        };
        self.all(&ToClient::Message { text });
    }

    /// Notices the file being renamed, moved (here or in another tab, by anyone) or put in the bin,
    /// and tells every tab: they show the new name and place; in the bin, nothing saves until
    /// it's restored (then saving carries on by itself).
    fn check_place(&mut self) {
        let now = {
            let _g = self.app.lock.lock().unwrap_or_else(|e| e.into_inner());
            self.app.store.synced_index().ok().and_then(|i| i.get(&self.file).cloned())
        };
        if now == self.path {
            return;
        }
        let (name, folder) = match (&now, &self.path) {
            (Some(p), _) | (None, Some(p)) => (crate::store::base_of(p), crate::store::parent_of(p)),
            (None, None) => return,
        };
        crate::log::note(&self.app.store, &format!("{} is now {}", self.path.clone().unwrap_or_default(), now.clone().unwrap_or_else(|| "in the bin".into())));
        self.all(&ToClient::Moved { name, folder, binned: now.is_none() });
        self.path = now;
    }

    /// Copies the sheets of another spreadsheet file to the end of this one, as a change made by
    /// the server (every tab gets it at once; undo history isn't touched).
    fn add_sheets(&mut self, name: &str, bytes: &[u8], by: &str) -> Result<Vec<String>, String> {
        let mut s = Session::new();
        s.execute("file.open", json!({ "name": name, "base64": gridcraft_engine::io::base64_encode(bytes) })).map_err(|e| format!("Can't read that sheet: {e}"))?;
        let src = s.active().map(|d| d.wb.clone()).ok_or("nothing to copy")?;
        let before = self.wb().ok_or("the spreadsheet isn't open")?;
        let mut wb = (*before).clone();
        let mut names = Vec::new();
        for i in 0..src.sheets.len() {
            if let Some(n) = sync::copy_sheet_into(&src, i, &mut wb) {
                names.push(n);
            }
        }
        sync::set_workbook_recalc(&mut self.session, wb);
        self.server_change(before);
        if !by.is_empty() {
            self.last_editor = Some(by.to_string());
        }
        Ok(names)
    }

    /// Saves the room's workbook as a new file next to this one; returns its name.
    fn keep_copy(&self, wb: &Workbook) -> Result<String, String> {
        let _g = self.app.lock.lock().unwrap_or_else(|e| e.into_inner());
        let store = &self.app.store;
        let path = store.synced_index().map_err(|e| e.to_string())?.get(&self.file).cloned().ok_or("the file is gone")?;
        let folder = crate::store::parent_of(&path);
        let (stem, _) = crate::store::split_name(&crate::store::base_of(&path));
        let name = store.claim_name(&folder, &format!("{stem} (StarSheets copy {})", crate::store::readable_stamp()), ".xlsx", None).map_err(|e| e.msg)?;
        let bytes = gridcraft_engine::io::save_bytes(wb, &name).map_err(|e| e.to_string())?;
        store.write_file(&store.full(&crate::store::join(&folder, &name)), &bytes).map_err(|e| e.to_string())?;
        Ok(name)
    }

    /// Reads the file again and gives every tab the new state (history starts again).
    fn reload(&mut self) {
        if self.open().is_ok() {
            self.history.clear();
            self.dirty_since = None;
            self.seq += 1;
            let wb = self.snapshot();
            self.all(&ToClient::Reset { seq: self.seq, workbook: wb });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use crate::store::Store;
    use std::collections::HashMap;
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
        let store = Store { root: dir.clone(), keep: 3, spaces: false };
        let id = store.synced_index().unwrap().keys().next().unwrap().clone();
        let app = Arc::new(App { store, base: dir.clone(), lock: Mutex::new(()), location: String::new(), auth: false, site: None, rooms: Rooms::default() });
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
        app.rooms.send(app, file, RoomMsg::Join { client: name.into(), user: None, can_edit: true, out });
        let mut t = Tab { name: name.into(), rx, session: Session::new(), seq: 0, cseq: 0, seen: Vec::new() };
        match next(&mut t) {
            ToClient::Welcome { seq, workbook, .. } => {
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

    /// Waits for the next Message, skipping anything else.
    fn message(t: &mut Tab) -> String {
        for _ in 0..20 {
            if let ToClient::Message { text } = next(t) {
                return text;
            }
        }
        panic!("no message");
    }

    #[test]
    fn open_tabs_hear_about_moves_and_get_copied_sheets() {
        let (app, file, dir) = app();
        let mut a = join(&app, &file, "a");
        // Renamed (by the files API, or anyone): the tab is told the new name and folder.
        std::fs::create_dir_all(dir.join("Work")).unwrap();
        {
            let _g = app.lock.lock().unwrap();
            let q: HashMap<String, String> = [("folder".to_string(), "Work".to_string())].into();
            app.store.route("POST", &["files".into(), file.clone(), "move".into()], &q, None, b"").unwrap();
        }
        let mut moved = None;
        for _ in 0..20 {
            if let ToClient::Moved { name, folder, binned } = next(&mut a) {
                moved = Some((name, folder, binned));
                break;
            }
        }
        assert_eq!(moved, Some(("t.csv".into(), "Work".into(), false)));
        // Another spreadsheet's sheet copied in: every tab gets it.
        let names = app.rooms.add_sheets(&app, &file, "x.csv".into(), b"from,another\n1,2\n".to_vec(), String::new()).unwrap();
        assert_eq!(names.len(), 1);
        assert!(matches!(next(&mut a), ToClient::Applied { .. }));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn changes_made_outside_starsheets_are_never_overwritten() {
        let (app, file, dir) = app();
        let mut a = join(&app, &file, "a");
        // Changed outside while nothing is unsaved: the room opens the file again.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(dir.join("t.csv"), "a,b\n1,2\nexcel,3\n").unwrap();
        assert!(matches!(next(&mut a), ToClient::Reset { .. }));
        assert!(message(&mut a).contains("opened again"));
        // Changed outside while a change waits to be saved: ours goes into a copy, theirs stays.
        edit(&app, &file, &mut a, "cell.set", json!({"cell": "C1", "input": "ours"}));
        take_applied(&mut a);
        std::fs::write(dir.join("t.csv"), "a,b\ntheirs,9\n").unwrap();
        assert!(matches!(next(&mut a), ToClient::Reset { .. }));
        assert!(message(&mut a).contains("StarSheets copy"));
        app.rooms.send(&app, &file, RoomMsg::Leave { client: "a".into() });
        std::thread::sleep(Duration::from_millis(400));
        assert!(std::fs::read_to_string(dir.join("t.csv")).unwrap().contains("theirs"), "their version is untouched");
        let copies: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.contains("StarSheets copy")).collect();
        assert_eq!(copies.len(), 1, "{copies:?}");
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
            assert_eq!(take_applied(t).0, "", "undo comes from the server");
        }
        assert_eq!(print(&a), start);
        assert_eq!(print(&b), start);
        // Undo of a's change after b changed another cell keeps b's change.
        edit(&app, &file, &mut a, "cell.set", json!({"cell": "A7", "input": "a again"}));
        take_applied(&mut a);
        take_applied(&mut b);
        edit(&app, &file, &mut b, "cell.set", json!({"cell": "B7", "input": "b's"}));
        take_applied(&mut a);
        take_applied(&mut b);
        app.rooms.send(&app, &file, RoomMsg::From { client: "a".into(), msg: ToServer::Undo });
        take_applied(&mut a);
        take_applied(&mut b);
        let csv = |t: &Tab| format!("{:?}", t.session.active().unwrap().wb.sheets[0].cells.iter().map(|(c, x)| (c.a1(), format!("{:?}", x.value))).collect::<Vec<_>>());
        assert!(!csv(&b).contains("a again") && csv(&b).contains("b's"), "{}", csv(&b));
        assert_eq!(print(&a), print(&b));
        // b overwrites a's cell: a's undo is refused.
        edit(&app, &file, &mut a, "cell.set", json!({"cell": "C8", "input": "a3"}));
        take_applied(&mut a);
        take_applied(&mut b);
        edit(&app, &file, &mut b, "cell.set", json!({"cell": "C8", "input": "b over"}));
        take_applied(&mut a);
        take_applied(&mut b);
        app.rooms.send(&app, &file, RoomMsg::From { client: "a".into(), msg: ToServer::Undo });
        assert!(matches!(next(&mut a), ToClient::Message { text } if text == sync::UNDO_CHANGED));
        let _ = std::fs::remove_dir_all(dir);
    }
}
