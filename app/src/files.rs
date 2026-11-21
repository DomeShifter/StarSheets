//! Files on the StarSheets file server (server/files.py, under /api/ on the same site), the way
//! Google Drive and Sheets work: a spreadsheet saves itself a moment after each change (a new one
//! as soon as any cell has something in it, as "Untitled spreadsheet"), the address bar keeps its
//! file (`?file=<id>`) so a reload reopens it, and File › Open lists the files in the folder.
//! The server picks free names; a save based on an older copy (the same file changed in another
//! tab) never overwrites: it becomes "<name> (conflicted copy <date>)".

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use egui::{Color32, RichText, Ui, vec2};
use gridcraft_engine::model::Workbook;
use gridcraft_ui_egui::SheetApp;
use serde_json::{Value as Json, json};
use wasm_bindgen::JsCast as _;

use crate::theme;

/// Asks for the Open dialog, the version list, or another file action (menus set it in egui memory).
pub const REQUEST: &str = "starsheets_files_request";
/// The title-bar status line ("Saving…", "Saved to the server", …), in egui memory.
pub const STATUS: &str = "starsheets_save_status";
const API: &str = "api/files";
/// Quiet time after the last change before saving (ms).
const SAVE_AFTER: f64 = 1500.0;

#[derive(Clone)]
struct Link {
    doc: u64,
    id: String,
    name: String,
    version: String,
}

#[derive(Clone)]
pub struct Info {
    id: String,
    name: String,
    modified: i64,
}

enum Reply {
    Saved { doc: u64, sent: Arc<Workbook>, link: Link },
    Conflict { doc: u64, sent: Arc<Workbook>, bytes: Vec<u8> },
    Failed(String),
    Listed(Vec<Info>),
    Versions(Vec<(String, i64)>),
    Opened { id: String, name: String, version: String, bytes: Vec<u8> },
    Renamed(String),
    Navigate(String),
}

#[derive(Default)]
pub struct Files {
    link: Option<Link>,
    inbox: Rc<RefCell<Vec<Reply>>>,
    busy: bool,
    booted: bool,
    seen: Option<(u64, usize)>,
    changed_at: f64,
    status: String,
    error: Option<String>,
    /// The Open dialog's list (None = closed) and its search text.
    list: Option<Vec<Info>>,
    search: String,
    versions: Option<Vec<(String, i64)>>,
    /// A rename sent to the server and not answered yet.
    renaming: bool,
}

impl Files {
    pub fn logic(&mut self, app: &mut SheetApp, ctx: &egui::Context) {
        // A new workbook is "Untitled spreadsheet", as in Sheets (GridCraft calls it Book1). The
        // workbook appears after the first frames, so this is checked every frame.
        if self.link.is_none()
            && let Some(d) = app.session.active_mut()
            && d.path.is_none()
            && d.title.strip_prefix("Book").is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
        {
            d.title = "Untitled spreadsheet".into();
        }
        if !self.booted {
            self.booted = true;
            if let Some(id) = query_file() {
                self.status = "Opening…".into();
                self.busy = true;
                get_file(id, self.inbox.clone(), ctx.clone());
            }
        }
        let replies: Vec<Reply> = std::mem::take(&mut *self.inbox.borrow_mut());
        for r in replies {
            self.handle(app, ctx, r);
        }
        if let Some(req) = ctx.data_mut(|m| m.remove_temp::<String>(egui::Id::new(REQUEST))) {
            self.request(app, ctx, &req);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O))) {
            self.request(app, ctx, "open");
        }
        self.autosave(app, ctx);
        self.rename_if_needed(app, ctx);
        let unsaved = self.busy || app.session.active().is_some_and(|d| self.needs_save(d));
        set_unsaved_flag(unsaved);
        let status = if let Some(e) = &self.error {
            format!("Not saved: {e}")
        } else if unsaved {
            "Saving…".to_string()
        } else if self.link.as_ref().is_some_and(|l| app.session.active().is_some_and(|d| d.uid == l.doc)) {
            "Saved".to_string()
        } else {
            self.status.clone()
        };
        ctx.data_mut(|m| m.insert_temp(egui::Id::new(STATUS), status));
    }

    fn request(&mut self, app: &mut SheetApp, ctx: &egui::Context, req: &str) {
        match req {
            "open" => {
                self.search.clear();
                self.list = Some(vec![]);
                list_files(self.inbox.clone(), ctx.clone());
            }
            "new" => open_tab(""),
            "versions" => {
                if let Some(l) = &self.link {
                    self.versions = Some(vec![]);
                    send(Method::Get, format!("{API}/{}/versions", l.id), None, None, self.inbox.clone(), ctx.clone(), |_, _, body| {
                        let v: Json = serde_json::from_slice(&body).unwrap_or_default();
                        Reply::Versions(v.as_array().into_iter().flatten().map(|x| (x["v"].as_str().unwrap_or("").to_string(), x["modified"].as_i64().unwrap_or(0))).collect())
                    });
                } else {
                    app.message = Some(("Version history".into(), "This spreadsheet has no saved versions yet.".into()));
                }
            }
            "copy" => {
                let Some(d) = app.session.active() else { return };
                let Ok(bytes) = gridcraft_engine::io::save_bytes(&d.wb, "copy.xlsx") else { return };
                let name = format!("Copy of {}", d.title);
                send(Method::Post, format!("{API}?name={}", enc(&name)), Some(bytes), None, self.inbox.clone(), ctx.clone(), |_, _, body| {
                    let v: Json = serde_json::from_slice(&body).unwrap_or_default();
                    Reply::Navigate(v["id"].as_str().unwrap_or("").to_string())
                });
            }
            "bin" => {
                let Some(l) = self.link.clone() else {
                    app.message = Some(("Move to bin".into(), "This spreadsheet hasn't been saved yet.".into()));
                    return;
                };
                send(Method::Delete, format!("{API}/{}", l.id), None, None, self.inbox.clone(), ctx.clone(), |_, _, _| Reply::Navigate(String::new()));
            }
            _ => {}
        }
    }

    fn handle(&mut self, app: &mut SheetApp, ctx: &egui::Context, r: Reply) {
        match r {
            Reply::Saved { doc, sent, link } => {
                self.busy = false;
                self.error = None;
                if let Some(d) = app.session.active_mut().filter(|d| d.uid == doc) {
                    d.saved = sent;
                    d.title = stem(&link.name);
                    // An imported file keeps its own name only until it is on the server.
                    d.path = None;
                }
                set_query_file(&link.id);
                self.link = Some(link);
            }
            Reply::Conflict { doc, sent, bytes } => {
                // Someone saved this file elsewhere since we opened it: keep both.
                let name = format!("{} (conflicted copy {})", self.link.as_ref().map_or("Untitled spreadsheet".into(), |l| stem(&l.name)), now_text());
                app.message = Some(("Saved as a copy".into(), format!("This file was changed in another tab or by someone else, so your changes were saved as \"{name}\".")));
                self.link = None;
                post_new(doc, sent, bytes, name, self.inbox.clone(), ctx.clone());
            }
            Reply::Failed(e) => {
                self.busy = false;
                self.renaming = false;
                if !e.is_empty() {
                    self.error = Some(e);
                }
            }
            Reply::Listed(files) => self.list = Some(files),
            Reply::Versions(v) => self.versions = Some(v),
            Reply::Opened { id, name, version, bytes } => {
                self.busy = false;
                let old = app.session.active().filter(|d| !d.is_dirty()).map(|d| d.uid);
                let b64 = gridcraft_engine::io::base64_encode(&bytes);
                match app.run("file.open", json!({ "name": name, "base64": b64 })) {
                    Ok(_) => {
                        if let Some(i) = old.and_then(|u| app.session.document_index(u)) {
                            app.session.close_document(i);
                        }
                        let doc = app.session.active().map_or(0, |d| d.uid);
                        if let Some(d) = app.session.active_mut() {
                            d.title = stem(&name);
                            d.path = None;
                            d.saved = d.wb.clone();
                        }
                        self.link = Some(Link { doc, id, name, version });
                        app.after_engine();
                    }
                    Err(e) => self.error = Some(gridcraft_ui_egui::clean_error(&e)),
                }
            }
            Reply::Renamed(name) => {
                self.renaming = false;
                if let Some(l) = &mut self.link {
                    l.name = name.clone();
                }
                if let Some(d) = app.session.active_mut() {
                    d.title = stem(&name);
                }
            }
            Reply::Navigate(id) => {
                set_unsaved_flag(false);
                go_to(&id);
            }
        }
    }

    /// Saves the active spreadsheet once it has data and nothing changed for a moment.
    fn autosave(&mut self, app: &mut SheetApp, ctx: &egui::Context) {
        let now = gridcraft_ui_egui::monotonic_ms();
        let Some(d) = app.session.active() else { return };
        let key = (d.uid, Arc::as_ptr(&d.wb) as usize);
        if self.seen != Some(key) {
            self.seen = Some(key);
            self.changed_at = now;
        }
        if self.busy || self.renaming || !self.needs_save(d) || app.editor.is_some() {
            return;
        }
        if now - self.changed_at < SAVE_AFTER {
            ctx.request_repaint_after(std::time::Duration::from_millis(400));
            return;
        }
        let sent = d.wb.clone();
        let doc = d.uid;
        let title = display_name(d);
        let bytes = match gridcraft_engine::io::save_bytes(&sent, "book.xlsx") {
            Ok(b) => b,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        self.busy = true;
        match self.link.clone().filter(|l| l.doc == doc) {
            Some(l) => {
                let inbox = self.inbox.clone();
                let (sent2, bytes2) = (sent.clone(), bytes.clone());
                send(Method::Put, format!("{API}/{}", l.id), Some(bytes), Some(l.version.clone()), inbox, ctx.clone(), move |status, _, body| {
                    if status == 409 {
                        return Reply::Conflict { doc, sent: sent2, bytes: bytes2 };
                    }
                    let v: Json = serde_json::from_slice(&body).unwrap_or_default();
                    let name = v["name"].as_str().map_or(l.name.clone(), str::to_string);
                    Reply::Saved { doc, sent: sent2, link: Link { version: v["version"].as_str().unwrap_or("").to_string(), name, ..l } }
                });
            }
            None => post_new(doc, sent, bytes, title, self.inbox.clone(), ctx.clone()),
        }
    }

    /// A title changed in the app (click the title to rename) renames the file on the server.
    fn rename_if_needed(&mut self, app: &mut SheetApp, ctx: &egui::Context) {
        if self.busy || self.renaming {
            return;
        }
        let Some(l) = self.link.clone() else { return };
        let Some(d) = app.session.active().filter(|d| d.uid == l.doc) else { return };
        let want = display_name(d);
        if want == stem(&l.name) || want.trim().is_empty() {
            return;
        }
        self.renaming = true;
        send(Method::Post, format!("{API}/{}/rename?name={}", l.id, enc(&want)), None, None, self.inbox.clone(), ctx.clone(), |_, _, body| {
            let v: Json = serde_json::from_slice(&body).unwrap_or_default();
            match v["name"].as_str() {
                Some(n) => Reply::Renamed(n.to_string()),
                None => Reply::Failed("rename failed".into()),
            }
        });
    }

    /// The Open dialog and the version list.
    pub fn ui(&mut self, app: &mut SheetApp, ui: &mut Ui) {
        if let Some(files) = self.list.clone() {
            let mut open = true;
            let mut pick = None;
            egui::Window::new("Open a file").open(&mut open).collapsible(false).resizable(false).default_width(560.0).anchor(egui::Align2::CENTER_TOP, vec2(0.0, 90.0)).show(ui.ctx(), |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search").desired_width(f32::INFINITY).margin(vec2(8.0, 8.0)));
                ui.add_space(8.0);
                let q = self.search.to_lowercase();
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    ui.set_width(540.0);
                    let shown: Vec<&Info> = files.iter().filter(|f| q.is_empty() || f.name.to_lowercase().contains(&q)).collect();
                    if shown.is_empty() {
                        ui.label(RichText::new(if files.is_empty() { "No files yet. A new spreadsheet saves itself as soon as you type in it." } else { "No file matches." }).color(theme::TEXT_DIM));
                    }
                    for f in shown {
                        let row = ui.add(egui::Button::new("").frame(false).min_size(vec2(540.0, 40.0)));
                        let p = ui.painter();
                        if row.hovered() {
                            p.rect_filled(row.rect, 4.0, theme::MENU_HOVER);
                        }
                        if let Some(img) = crate::icons::icon("starsheets") {
                            img.paint_at(ui, egui::Rect::from_center_size(row.rect.left_center() + vec2(18.0, 0.0), vec2(22.0, 22.0)));
                        }
                        p.text(row.rect.left_center() + vec2(40.0, 0.0), egui::Align2::LEFT_CENTER, &f.name, egui::FontId::proportional(14.0), theme::TEXT);
                        p.text(row.rect.right_center() - vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, date_text(f.modified), egui::FontId::proportional(12.0), theme::TEXT_DIM);
                        if row.clicked() {
                            pick = Some(f.id.clone());
                        }
                    }
                });
            });
            if let Some(id) = pick {
                open = false;
                // This tab if what's here is saved (or empty), else a new tab.
                let keep = app.session.active().is_some_and(|d| self.needs_save(d)) || self.busy;
                if keep { open_tab(&id) } else { go_to(&id) }
            }
            if !open {
                self.list = None;
            }
        }
        if let Some(vs) = self.versions.clone() {
            let mut open = true;
            let mut restore = None;
            egui::Window::new("Version history").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::RIGHT_TOP, vec2(-20.0, 90.0)).show(ui.ctx(), |ui| {
                ui.set_width(320.0);
                ui.label(RichText::new("Earlier copies are kept each time the file saves (the last 6).").color(theme::TEXT_DIM));
                ui.add_space(6.0);
                if vs.is_empty() {
                    ui.label("No earlier versions yet.");
                }
                for (v, when) in &vs {
                    ui.horizontal(|ui| {
                        ui.label(date_text(*when));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new("Restore").color(Color32::from_rgb(0x14, 0x6C, 0x2E))).frame(false)).clicked() {
                                restore = Some(v.clone());
                            }
                        });
                    });
                }
            });
            if let (Some(v), Some(l)) = (restore, self.link.clone()) {
                open = false;
                restore_version(l, v, self.inbox.clone(), ui.ctx().clone());
            }
            if !open {
                self.versions = None;
            }
        }
    }
}

impl Files {
    /// Whether the workbook should be (re)saved: it has data and either changed or isn't on the
    /// server yet (an imported file opens unchanged but still needs its first upload).
    fn needs_save(&self, d: &gridcraft_engine::DocState) -> bool {
        let linked = self.link.as_ref().is_some_and(|l| l.doc == d.uid);
        has_data(&d.wb) && (d.is_dirty() || !linked)
    }
}

/// The name a workbook is saved under: an imported file's own name (renaming it in the title
/// changes that), else its title.
fn display_name(d: &gridcraft_engine::DocState) -> String {
    match d.path.as_deref() {
        Some(p) => stem(p.rsplit(['/', '\\']).next().unwrap_or(p)),
        None => d.title.clone(),
    }
}

/// Whether any sheet has anything in it.
fn has_data(wb: &Workbook) -> bool {
    wb.sheets.iter().any(|s| s.used_range().is_some())
}

fn stem(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((s, ext)) if ext.len() <= 4 => s.to_string(),
        _ => name.to_string(),
    }
}

fn enc(s: &str) -> String {
    js_sys::encode_uri_component(s).as_string().unwrap_or_default()
}

fn now_text() -> String {
    let d = js_sys::Date::new_0();
    format!("{:04}-{:02}-{:02} {:02}:{:02}", d.get_full_year(), d.get_month() + 1, d.get_date(), d.get_hours(), d.get_minutes())
}

fn date_text(unix: i64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(unix as f64 * 1000.0));
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{} {} {}, {:02}:{:02}", d.get_date(), M[d.get_month() as usize % 12], d.get_full_year(), d.get_hours(), d.get_minutes())
}

fn query_file() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&search).ok()?.get("file").filter(|s| !s.is_empty())
}

/// Puts `?file=<id>` in the address bar without reloading.
fn set_query_file(id: &str) {
    let Some(w) = web_sys::window() else { return };
    if query_file().as_deref() == Some(id) {
        return;
    }
    if let Ok(h) = w.history() {
        let _ = h.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&format!("?file={id}")));
    }
}

fn page_url(id: &str) -> String {
    let path = web_sys::window().and_then(|w| w.location().pathname().ok()).unwrap_or_else(|| "/".into());
    if id.is_empty() { path } else { format!("{path}?file={id}") }
}

fn go_to(id: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.location().set_href(&page_url(id));
    }
}

fn open_tab(id: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.open_with_url_and_target(&page_url(id), "_blank");
    }
}

/// Tells the page (index.html) whether leaving now would lose changes.
fn set_unsaved_flag(on: bool) {
    if let Some(w) = web_sys::window() {
        let _ = js_sys::Reflect::set(&w, &"__ssUnsaved".into(), &on.into());
    }
}

#[derive(Clone, Copy)]
enum Method {
    Get,
    Post,
    Put,
    Delete,
}

/// Sends a request and turns the answer into a Reply (network and server errors become Failed).
fn send(
    method: Method,
    url: String,
    body: Option<Vec<u8>>,
    if_match: Option<String>,
    inbox: Rc<RefCell<Vec<Reply>>>,
    ctx: egui::Context,
    then: impl FnOnce(u16, web_sys::Headers, Vec<u8>) -> Reply + 'static,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let reply = match fetch(method, &url, body, if_match).await {
            Ok((status, headers, bytes)) if status < 400 || status == 409 => then(status, headers, bytes),
            Ok((status, _, bytes)) => {
                let v: Json = serde_json::from_slice(&bytes).unwrap_or_default();
                Reply::Failed(v["error"].as_str().map_or(format!("server said {status}"), str::to_string))
            }
            Err(e) => Reply::Failed(e),
        };
        inbox.borrow_mut().push(reply);
        ctx.request_repaint();
    });
}

async fn fetch(method: Method, url: &str, body: Option<Vec<u8>>, if_match: Option<String>) -> Result<(u16, web_sys::Headers, Vec<u8>), String> {
    let js = |e: wasm_bindgen::JsValue| format!("{e:?}");
    let init = web_sys::RequestInit::new();
    init.set_method(match method {
        Method::Get => "GET",
        Method::Post => "POST",
        Method::Put => "PUT",
        Method::Delete => "DELETE",
    });
    if let Some(b) = body {
        init.set_body(&js_sys::Uint8Array::from(b.as_slice()));
    }
    let headers = web_sys::Headers::new().map_err(js)?;
    if let Some(v) = if_match {
        headers.set("If-Match", &v).map_err(js)?;
    }
    init.set_headers(&headers);
    let req = web_sys::Request::new_with_str_and_init(url, &init).map_err(js)?;
    let w = web_sys::window().ok_or("no window")?;
    let resp: web_sys::Response = wasm_bindgen_futures::JsFuture::from(w.fetch_with_request(&req))
        .await
        .map_err(|_| "the file server can't be reached".to_string())?
        .dyn_into()
        .map_err(js)?;
    let buf = wasm_bindgen_futures::JsFuture::from(resp.array_buffer().map_err(js)?).await.map_err(js)?;
    Ok((resp.status(), resp.headers(), js_sys::Uint8Array::new(&buf).to_vec()))
}

fn post_new(doc: u64, sent: Arc<Workbook>, bytes: Vec<u8>, name: String, inbox: Rc<RefCell<Vec<Reply>>>, ctx: egui::Context) {
    send(Method::Post, format!("{API}?name={}", enc(&name)), Some(bytes), None, inbox, ctx, move |_, _, body| {
        let v: Json = serde_json::from_slice(&body).unwrap_or_default();
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        Reply::Saved { doc, sent, link: Link { doc, id: s("id"), name: s("name"), version: s("version") } }
    });
}

fn get_file(id: String, inbox: Rc<RefCell<Vec<Reply>>>, ctx: egui::Context) {
    send(Method::Get, format!("{API}/{id}"), None, None, inbox, ctx, move |status, headers, bytes| {
        if status != 200 {
            return Reply::Failed("file not found".into());
        }
        let name = headers.get("X-File-Name").ok().flatten().and_then(|n| serde_json::from_str::<String>(&n).ok()).unwrap_or_else(|| "file.xlsx".into());
        let version = headers.get("ETag").ok().flatten().unwrap_or_default();
        Reply::Opened { id, name, version, bytes }
    });
}

fn list_files(inbox: Rc<RefCell<Vec<Reply>>>, ctx: egui::Context) {
    send(Method::Get, API.to_string(), None, None, inbox, ctx, |_, _, body| {
        let v: Json = serde_json::from_slice(&body).unwrap_or_default();
        Reply::Listed(
            v.as_array()
                .into_iter()
                .flatten()
                .map(|f| Info { id: f["id"].as_str().unwrap_or("").into(), name: f["name"].as_str().unwrap_or("").into(), modified: f["modified"].as_i64().unwrap_or(0) })
                .collect(),
        )
    });
}

/// Puts an older copy back as the current file (the current one becomes a version), then reloads.
fn restore_version(l: Link, v: String, inbox: Rc<RefCell<Vec<Reply>>>, ctx: egui::Context) {
    let id = l.id.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let reply = match fetch(Method::Get, &format!("{API}/{id}/versions/{v}"), None, None).await {
            Ok((200, _, bytes)) => match fetch(Method::Put, &format!("{API}/{id}"), Some(bytes), None).await {
                Ok((s, ..)) if s < 400 => Reply::Navigate(id),
                Ok((s, ..)) => Reply::Failed(format!("restore failed ({s})")),
                Err(e) => Reply::Failed(e),
            },
            Ok((s, ..)) => Reply::Failed(format!("restore failed ({s})")),
            Err(e) => Reply::Failed(e),
        };
        inbox.borrow_mut().push(reply);
        ctx.request_repaint();
    });
}
