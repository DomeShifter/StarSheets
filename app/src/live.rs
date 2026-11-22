//! Live editing in the tab (docs/live-editing.md). While a file from the server is open, this
//! tab keeps a WebSocket to the server's room for it: each change made here is shown at once and
//! sent; changes from other tabs and people arrive in the server's order. When one arrives before
//! ours are confirmed, the tab goes back to the server's copy, applies it and replays its own
//! unconfirmed changes on top. The server saves the file, so the tab's own autosave is off.
//! A server without live editing (an older one) is noticed and the tab saves as before.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use gridcraft_engine::model::Workbook;
use gridcraft_ui_egui::SheetApp;
use starsheets_sync::{self as sync, Change, ToClient, ToServer};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::Closure;

/// What the title bar shows for a live file, and whether closing the tab would lose changes.
#[derive(Clone)]
pub struct LiveStatus {
    pub doc: u64,
    pub text: String,
    pub unsaved: bool,
    pub saved_at: Option<i64>,
}

thread_local! {
    /// Other people (tabs) in the open file, for the avatars in the title row.
    static OTHERS: RefCell<Vec<sync::Person>> = const { RefCell::new(Vec::new()) };
    static STATUS: RefCell<Option<LiveStatus>> = const { RefCell::new(None) };
    /// The document live editing looks after (the tab's own autosave leaves it alone).
    static HANDLED: RefCell<Option<u64>> = const { RefCell::new(None) };
}

/// The live status of `doc`, if live editing looks after it.
pub fn status_for(doc: u64) -> Option<LiveStatus> {
    STATUS.with(|s| s.borrow().clone()).filter(|s| s.doc == doc)
}

/// The other people with the open file, one per person (a person's other tabs count once).
pub fn others() -> Vec<sync::Person> {
    OTHERS.with(|o| o.borrow().clone())
}

/// Whether live editing saves `doc` (so the tab must not).
pub fn handles(doc: u64) -> bool {
    HANDLED.with(|h| *h.borrow() == Some(doc))
}

enum Event {
    Open,
    Msg(ToClient),
    Closed,
}

struct Pending {
    cseq: u64,
    change: Change,
    before: Arc<Workbook>,
    after: Arc<Workbook>,
}

pub struct Live {
    /// The open file: (document uid, file id on the server).
    file: Option<(u64, String)>,
    ws: Option<web_sys::WebSocket>,
    inbox: Rc<RefCell<Vec<Event>>>,
    _handlers: Vec<Closure<dyn FnMut(web_sys::Event)>>,
    client: String,
    open: bool,
    joined: bool,
    seq: u64,
    confirmed: Option<Arc<Workbook>>,
    /// Our changes the server confirmed since `confirmed` was last brought up to date.
    acked: Vec<Change>,
    pending: VecDeque<Pending>,
    cseq: u64,
    retry_at: f64,
    failures: u32,
    /// This server has no live editing (or runs another build): the tab saves as before.
    unsupported: bool,
    server_dirty: bool,
    saved_at: Option<i64>,
    /// Everyone in the file, which entry is this tab, and where the others are (sheet,
    /// selection, when it last moved).
    people: Vec<sync::Person>,
    you: String,
    places: std::collections::HashMap<String, (usize, gridcraft_engine::selection::Selection, f64)>,
    sent_place: Option<(usize, gridcraft_engine::selection::Selection)>,
    sent_place_at: f64,
}

impl Default for Live {
    fn default() -> Self {
        Live {
            file: None,
            ws: None,
            inbox: Rc::default(),
            _handlers: Vec::new(),
            client: format!("{:012x}", (js_sys::Math::random() * 2f64.powi(48)) as u64),
            open: false,
            joined: false,
            seq: 0,
            confirmed: None,
            acked: Vec::new(),
            pending: VecDeque::new(),
            cseq: 0,
            retry_at: 0.0,
            failures: 0,
            unsupported: false,
            server_dirty: false,
            saved_at: None,
            people: Vec::new(),
            you: String::new(),
            places: std::collections::HashMap::new(),
            sent_place: None,
            sent_place_at: 0.0,
        }
    }
}

fn now_ms() -> f64 {
    gridcraft_ui_egui::monotonic_ms()
}

impl Live {
    pub fn logic(&mut self, app: &mut SheetApp, ctx: &egui::Context, files: &crate::files::Files) {
        let target = files.linked().filter(|(doc, _)| app.session.document_index(*doc).is_some());
        if target != self.file {
            self.disconnect();
            *self = Live { client: self.client.clone(), unsupported: self.unsupported, ..Live::default() };
            self.file = target.clone();
        }
        let Some((doc, _)) = self.file.clone() else {
            app.session.sync_log = None;
            STATUS.with(|s| *s.borrow_mut() = None);
            HANDLED.with(|h| *h.borrow_mut() = None);
            return;
        };
        if self.unsupported {
            app.session.sync_log = None;
            STATUS.with(|s| *s.borrow_mut() = None);
            HANDLED.with(|h| *h.borrow_mut() = None);
            return;
        }
        HANDLED.with(|h| *h.borrow_mut() = Some(doc));
        if self.ws.is_none() && now_ms() >= self.retry_at {
            self.connect(ctx);
        }
        let log = app.session.sync_log.get_or_insert_with(Vec::new);
        let steps = std::mem::take(log);
        let mut changed = false;
        for st in steps.into_iter().filter(|s| s.doc == doc) {
            match st.id.as_str() {
                // Undo and redo are the server's: put this tab's own undo back and ask.
                "edit.undo" | "edit.redo" => {
                    if app.session.active().is_some_and(|d| d.uid == doc) {
                        sync::restore_workbook(&mut app.session, &st.before);
                        changed = true;
                    }
                    self.send(&if st.id == "edit.undo" { ToServer::Undo } else { ToServer::Redo });
                }
                _ => {
                    let change = make_change(&st.id, st.sheet, st.selection.clone(), st.params.clone(), &st.before, &st.after);
                    self.cseq += 1;
                    let p = Pending { cseq: self.cseq, change, before: st.before, after: st.after };
                    if self.joined && self.open {
                        self.send(&ToServer::Change { cseq: p.cseq, base: self.seq, change: p.change.clone() });
                    }
                    self.pending.push_back(p);
                }
            }
        }
        let events: Vec<Event> = std::mem::take(&mut *self.inbox.borrow_mut());
        for ev in events {
            changed |= self.handle(app, ev);
        }
        if changed {
            app.after_engine();
            ctx.request_repaint();
        }
        // Where this tab is, for the others (a few times a second at most).
        if self.open && self.joined
            && let Some(d) = app.session.active().filter(|d| d.uid == doc)
        {
            let place = (d.wb.active_sheet(), d.selection.clone());
            if self.sent_place.as_ref() != Some(&place) && now_ms() - self.sent_place_at > 120.0 {
                self.send(&ToServer::Presence { sheet: place.0, sel: place.1.clone() });
                self.sent_place = Some(place);
                self.sent_place_at = now_ms();
            } else if self.sent_place.as_ref() != Some(&place) {
                ctx.request_repaint_after(std::time::Duration::from_millis(130));
            }
        }
        let mut seen = std::collections::HashSet::new();
        let mine = crate::account::me().map(|m| m.username).unwrap_or_default();
        let others: Vec<sync::Person> = self
            .people
            .iter()
            .filter(|p| p.client != self.you && (p.username.is_empty() || p.username != mine))
            .filter(|p| p.username.is_empty() || seen.insert(p.username.clone()))
            .cloned()
            .collect();
        OTHERS.with(|o| *o.borrow_mut() = others);
        let text = if !self.open || !self.joined {
            if self.pending.is_empty() { "Connecting…" } else { "Offline: changes will sync" }
        } else if !self.pending.is_empty() || self.server_dirty {
            "Saving…"
        } else {
            "Saved"
        };
        let status = LiveStatus { doc, text: text.to_string(), unsaved: !self.pending.is_empty(), saved_at: self.saved_at };
        STATUS.with(|s| *s.borrow_mut() = Some(status));
        if !self.open {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    fn handle(&mut self, app: &mut SheetApp, ev: Event) -> bool {
        match ev {
            Event::Open => {
                self.open = true;
                if let Some((_, id)) = &self.file {
                    self.send(&ToServer::Hello { file: id.clone(), build: sync::BUILD.to_string(), client: self.client.clone() });
                }
                false
            }
            Event::Closed => {
                self.ws = None;
                self.open = false;
                self.failures += 1;
                if !self.joined && self.failures >= 3 {
                    // Never got in: an older server, or a plain web host. Save as before.
                    log::warn!("live editing: no connection to the server; saving the usual way");
                    self.unsupported = true;
                }
                self.retry_at = now_ms() + (500.0 * f64::from(self.failures.min(20))).min(10_000.0);
                false
            }
            Event::Msg(m) => self.message(app, m),
        }
    }

    fn message(&mut self, app: &mut SheetApp, m: ToClient) -> bool {
        match m {
            ToClient::Welcome { seq, workbook } => {
                let current = app.session.active().map(|d| d.wb.clone());
                match sync::workbook_from_json(&workbook, current.as_deref()) {
                    Ok(wb) => {
                        self.confirmed = Some(Arc::new(wb));
                        self.acked.clear();
                        self.seq = seq;
                        self.joined = true;
                        self.failures = 0;
                        self.sent_place = None;
                        self.rebuild(app, None);
                        // Everything not confirmed goes (again) on top of what the server has.
                        let resend: Vec<ToServer> = self.pending.iter().map(|p| ToServer::Change { cseq: p.cseq, base: seq, change: p.change.clone() }).collect();
                        for r in resend {
                            self.send(&r);
                        }
                    }
                    Err(e) => log::error!("live editing: {e}"),
                }
                true
            }
            ToClient::Applied { seq, client, cseq, change } => {
                self.seq = seq;
                self.server_dirty = true;
                if client == self.client {
                    if self.pending.front().is_some_and(|p| p.cseq == cseq) {
                        let p = self.pending.pop_front();
                        if self.pending.is_empty() {
                            self.confirmed = app.session.active().map(|d| d.wb.clone());
                            self.acked.clear();
                        } else if let Some(p) = p {
                            self.acked.push(p.change);
                        }
                    }
                    false
                } else if self.pending.is_empty() && self.acked.is_empty() {
                    if let Err(e) = sync::apply(&mut app.session, &change) {
                        log::warn!("live editing: couldn't apply a change ({e}); asking for the file again");
                        self.send(&ToServer::Resync);
                    }
                    self.confirmed = app.session.active().map(|d| d.wb.clone());
                    true
                } else {
                    self.rebuild(app, Some(&change));
                    true
                }
            }
            ToClient::Rejected { cseq, why } => {
                self.pending.retain(|p| p.cseq != cseq);
                self.rebuild(app, None);
                app.message = Some(("Change not saved".into(), why));
                true
            }
            ToClient::NeedState { cseq } => {
                let mut resend = Vec::new();
                let mut from = false;
                for p in self.pending.iter_mut() {
                    if p.cseq == cseq {
                        p.change = Change::State { patch: sync::make_patch(&p.before, &p.after) };
                        from = true;
                    }
                    if from {
                        resend.push(ToServer::Change { cseq: p.cseq, base: self.seq, change: p.change.clone() });
                    }
                }
                for r in resend {
                    self.send(&r);
                }
                false
            }
            ToClient::Reset { seq, workbook } => {
                let current = app.session.active().map(|d| d.wb.clone());
                if let Ok(wb) = sync::workbook_from_json(&workbook, current.as_deref()) {
                    self.confirmed = Some(Arc::new(wb));
                    self.acked.clear();
                    self.seq = seq;
                    self.server_dirty = true;
                    self.rebuild(app, None);
                }
                true
            }
            ToClient::Check { seq, hash } => {
                if seq == self.seq && self.pending.is_empty() && self.acked.is_empty()
                    && let Some(d) = app.session.active()
                    && sync::fingerprint(&d.wb) != hash
                {
                    log::warn!("live editing: this tab's copy differs from the server's; getting it again");
                    self.send(&ToServer::Resync);
                }
                false
            }
            ToClient::Saved { at } => {
                self.server_dirty = false;
                self.saved_at = Some(at);
                false
            }
            ToClient::Message { text } => {
                app.message = Some(("StarSheets".into(), text));
                false
            }
            ToClient::People { people, you } => {
                self.places.retain(|c, _| people.iter().any(|p| &p.client == c));
                self.people = people;
                self.you = you;
                false
            }
            ToClient::Presence { client, sheet, sel } => {
                self.places.insert(client, (sheet, sel, now_ms()));
                false
            }
            ToClient::Reload => {
                // The server runs a newer build: reload once to get it; if that didn't help (the
                // site isn't updated yet), carry on saving the usual way.
                let key = format!("starsheets_reload_{}", sync::BUILD);
                let storage = web_sys::window().and_then(|w| w.session_storage().ok().flatten());
                let tried = storage.as_ref().and_then(|s| s.get_item(&key).ok().flatten()).is_some();
                if !tried && self.pending.is_empty() {
                    if let Some(s) = &storage {
                        let _ = s.set_item(&key, "1");
                    }
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().reload();
                    }
                } else {
                    self.unsupported = true;
                    self.disconnect();
                }
                false
            }
        }
    }

    /// Back to the server's copy (plus our confirmed changes and `remote`), then our unconfirmed
    /// changes on top, each again from where it now starts.
    fn rebuild(&mut self, app: &mut SheetApp, remote: Option<&Change>) {
        let Some(confirmed) = self.confirmed.clone() else { return };
        sync::restore_workbook(&mut app.session, &confirmed);
        for c in self.acked.drain(..).collect::<Vec<_>>() {
            let _ = sync::apply(&mut app.session, &c);
        }
        if let Some(r) = remote
            && let Err(e) = sync::apply(&mut app.session, r)
        {
            log::warn!("live editing: couldn't apply a change ({e}); asking for the file again");
            self.send(&ToServer::Resync);
        }
        self.confirmed = app.session.active().map(|d| d.wb.clone());
        let mut kept = VecDeque::new();
        for mut p in self.pending.drain(..) {
            let Some(before) = app.session.active().map(|d| d.wb.clone()) else { break };
            if sync::apply(&mut app.session, &p.change).is_err() {
                // It no longer applies (its sheet went): the server will reject it too.
                kept.push_back(p);
                continue;
            }
            let Some(after) = app.session.active().map(|d| d.wb.clone()) else { break };
            if let Change::Cmd { fp, .. } = &mut p.change {
                *fp = sync::change_fingerprint(&before, &after);
            }
            p.before = before;
            p.after = after;
            kept.push_back(p);
        }
        self.pending = kept;
    }

    /// Other people's selections on this sheet, in their colours; their name shows for a few
    /// seconds after they move, and while the pointer is over their cell.
    pub fn draw(&self, app: &SheetApp, ctx: &egui::Context) {
        let (Some(cells), Some(d)) = (app.grid.cells_rect, app.session.active()) else { return };
        if self.file.as_ref().map(|f| f.0) != Some(d.uid) || !self.joined {
            return;
        }
        let sheet = d.wb.active_sheet();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("starsheets_presence"))).with_clip_rect(cells);
        let hover = ctx.pointer_hover_pos();
        let now = now_ms();
        let mut fading = false;
        for p in self.people.iter().filter(|p| p.client != self.you) {
            let Some((s, sel, moved)) = self.places.get(&p.client) else { continue };
            if *s != sheet {
                continue;
            }
            let colour = crate::account::parse_colour(&p.colour);
            for r in &sel.ranges {
                if let (Some(a), Some(b)) = (crate::linkcard::cell_rect(app, r.start), crate::linkcard::cell_rect(app, r.end)) {
                    let rect = a.union(b);
                    painter.rect_filled(rect, 0.0, colour.gamma_multiply(0.10));
                    painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, colour), egui::StrokeKind::Inside);
                }
            }
            let Some(at) = crate::linkcard::cell_rect(app, sel.active) else { continue };
            painter.rect_stroke(at, 0.0, egui::Stroke::new(2.0, colour), egui::StrokeKind::Inside);
            let recent = now - moved < 5000.0;
            fading |= recent;
            if recent || hover.is_some_and(|h| at.contains(h)) {
                let galley = painter.layout_no_wrap(p.name.clone(), egui::FontId::proportional(11.0), egui::Color32::WHITE);
                let size = galley.size() + egui::vec2(8.0, 4.0);
                let tag = egui::Rect::from_min_size(egui::pos2(at.left(), at.top() - size.y), size);
                painter.rect_filled(tag, 2.0, colour);
                painter.galley(tag.min + egui::vec2(4.0, 2.0), galley, egui::Color32::WHITE);
            }
        }
        if fading {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    fn send(&self, m: &ToServer) {
        if let (Some(ws), true) = (&self.ws, self.open)
            && let Ok(t) = serde_json::to_string(m)
        {
            let _ = ws.send_with_str(&t);
        }
    }

    fn disconnect(&mut self) {
        if let Some(ws) = self.ws.take() {
            ws.set_onopen(None);
            ws.set_onmessage(None);
            ws.set_onclose(None);
            ws.set_onerror(None);
            let _ = ws.close();
        }
        self._handlers.clear();
        self.open = false;
    }

    fn connect(&mut self, ctx: &egui::Context) {
        let Some(url) = live_url() else {
            self.unsupported = true;
            return;
        };
        let Ok(ws) = web_sys::WebSocket::new(&url) else {
            self.unsupported = true;
            return;
        };
        let mut handlers: Vec<Closure<dyn FnMut(web_sys::Event)>> = Vec::new();
        let on = |inbox: Rc<RefCell<Vec<Event>>>, ctx: egui::Context, f: fn(web_sys::Event) -> Option<Event>| {
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                if let Some(ev) = f(e) {
                    inbox.borrow_mut().push(ev);
                    ctx.request_repaint();
                }
            })
        };
        let open = on(self.inbox.clone(), ctx.clone(), |_| Some(Event::Open));
        let close = on(self.inbox.clone(), ctx.clone(), |_| Some(Event::Closed));
        let message = on(self.inbox.clone(), ctx.clone(), |e| {
            let text = e.dyn_into::<web_sys::MessageEvent>().ok()?.data().as_string()?;
            match serde_json::from_str::<ToClient>(&text) {
                Ok(m) => Some(Event::Msg(m)),
                Err(err) => {
                    log::warn!("live editing: unreadable message: {err}");
                    None
                }
            }
        });
        ws.set_onopen(Some(open.as_ref().unchecked_ref()));
        ws.set_onclose(Some(close.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(message.as_ref().unchecked_ref()));
        handlers.extend([open, close, message]);
        self._handlers = handlers;
        self.ws = Some(ws);
    }
}

/// What a step becomes on the wire: the command itself when it replays exactly, else the
/// finished state of what it changed.
fn make_change(id: &str, sheet: usize, sel: gridcraft_engine::selection::Selection, params: serde_json::Value, before: &Workbook, after: &Workbook) -> Change {
    if sync::replayable(id) && sync::server_allows(id) {
        let fp = sync::change_fingerprint(before, after);
        Change::Cmd { sheet, sel, id: id.to_string(), params, locale: sync::InputLocale::current(), fp }
    } else {
        Change::State { patch: sync::make_patch(before, after) }
    }
}

/// ws(s)://<this site>/api/live, next to the page (the app may live in a sub-folder).
fn live_url() -> Option<String> {
    let href = web_sys::window()?.location().href().ok()?;
    let url = web_sys::Url::new_with_base("api/live", &href).ok()?;
    url.set_protocol(if url.protocol() == "https:" { "wss:" } else { "ws:" });
    url.set_search("");
    Some(url.href())
}
