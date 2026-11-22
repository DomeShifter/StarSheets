//! StarSheets server: stores spreadsheets as ordinary .xlsx files in a folder tree (STORE) and,
//! with SITE set, serves the web app too. Replaces server/files.py with the same API and the
//! same files on disk (see docs/server.md), as the base for live editing.
//!
//! Settings (environment): STORE (folder, default /data), KEEP (versions kept, 6), PORT (8080),
//! LOCATION (how people are told where files are kept), STARSHEETS_AUTH (off = no login),
//! STARSHEETS_PASSWORD, SITE (a built StarSheets site to serve).

mod auth;
mod live;
mod sharing;
mod spaces;
mod store;
mod thumbs;

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Method, Response, Uri, header};
use percent_encoding::percent_decode_str;
use serde_json::{Value, json};

use store::{Bad, Reply, Store};

const MAX_BYTES: u64 = 200 * 1024 * 1024;
const COOKIE: &str = "starsheets_session";

struct App {
    store: Store,
    lock: Mutex<()>,
    location: String,
    auth: bool,
    site: Option<PathBuf>,
    rooms: live::Rooms,
}

type Shared = Arc<App>;

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

// ---------------------------------------------------------------- responses

/// JSON text with every non-ASCII character escaped, so it can go in an HTTP header
/// (what Python's json.dumps does by default).
fn json_ascii(s: &str) -> String {
    let mut out = String::new();
    for c in serde_json::to_string(s).unwrap_or_default().chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut buf = [0u16; 2];
            for u in c.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{u:04x}"));
            }
        }
    }
    out
}

fn send_json(v: &Value, code: u16) -> Response<Body> {
    Response::builder()
        .status(code)
        .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(serde_json::to_vec(v).unwrap_or_default()))
        .unwrap()
}

fn error(code: u16, msg: &str) -> Response<Body> {
    send_json(&json!({"error": msg}), code)
}

fn reply(r: Result<Reply, Bad>) -> Response<Body> {
    match r {
        Ok(Reply::Json(v, code)) => send_json(&v, code),
        Ok(Reply::File { data, name, version, folder, modified }) => {
            let mut b = Response::builder()
                .status(200)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(header::CACHE_CONTROL, "no-store")
                .header("X-File-Name", json_ascii(&name))
                .header("X-Folder", json_ascii(&folder))
                .header("X-Modified", modified.to_string());
            if !version.is_empty() {
                b = b.header(header::ETAG, version);
            }
            b.body(Body::from(data)).unwrap()
        }
        Err(e) => error(e.code, &e.msg),
    }
}

// ---------------------------------------------------------------- sign-in

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).collect::<Vec<_>>().join(";");
    cookies.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == COOKIE && !value.is_empty()).then(|| value.to_string())
    })
}

/// Who is asking (`None`: not signed in, or sign-in is off).
fn current_user(app: &App, headers: &HeaderMap) -> Option<auth::User> {
    if !app.auth {
        return None;
    }
    cookie_token(headers).and_then(|t| auth::session_user(&app.store, &t))
}

fn signed_in(app: &App, headers: &HeaderMap) -> bool {
    !app.auth || current_user(app, headers).is_some()
}

fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn cookie_reply(value: &str, max_age: u64, body: Value) -> Response<Body> {
    Response::builder()
        .status(200)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::SET_COOKIE, format!("{COOKIE}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Strict"))
        .body(Body::from(serde_json::to_vec(&body).unwrap_or_default()))
        .unwrap()
}

/// Runs account work (Argon2 is slow) off the async threads, under the store lock.
async fn blocking<T: Send + 'static>(app: &Shared, f: impl FnOnce(&App) -> T + Send + 'static) -> Option<T> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || {
        let _g = app.lock.lock().unwrap_or_else(|e| e.into_inner());
        f(&app)
    })
    .await
    .ok()
}

fn field(body: &Value, key: &str) -> String {
    body.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn opt_field(body: &Value, key: &str) -> Option<String> {
    body.get(key).and_then(Value::as_str).map(str::to_string)
}

/// /api/info, /api/setup, /api/signup, /api/login, /api/logout, /api/me and /api/users; `None` for other paths.
async fn accounts(app: &Shared, m: &str, p: &[String], q: &HashMap<String, String>, headers: &HeaderMap, body: &[u8]) -> Option<Response<Body>> {
    let path: Vec<&str> = p.iter().map(String::as_str).collect();
    let json: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let me = current_user(app, headers);
    let on = app.auth;
    let session = |app: &App, user: &auth::User| match auth::new_session(&app.store, &user.username) {
        Ok(t) => cookie_reply(&t, auth::SESSION_DAYS * 86400, json!({ "ok": true, "user": user.json() })),
        Err(_) => error(500, "storage error"),
    };
    Some(match (path.as_slice(), m) {
        (["info"], "GET") => {
            let setup = on && !auth::any_users(&app.store);
            let s = auth::signup(&app.store);
            let signup = on && !setup && s.on;
            send_json(&json!({ "location": app.location, "auth": on, "signed_in": !on || me.is_some(), "setup": setup, "signup": signup, "signup_code": signup && !s.code.is_empty(), "user": me.map(|u| u.json()) }), 200)
        }
        (["setup"], "POST") => {
            if !on {
                return Some(error(400, "Sign-in is off on this server."));
            }
            let r = blocking(app, move |app| {
                if auth::any_users(&app.store) {
                    return Err((400, "This server already has an owner. Sign in instead.".to_string()));
                }
                if !same(field(&json, "code").trim().as_bytes(), auth::setup_code(&app.store).as_bytes()) {
                    return Err((401, "That setup code isn't right.".to_string()));
                }
                let user = auth::add(&app.store, &field(&json, "username"), &field(&json, "name"), &field(&json, "password")).map_err(|e| (400, e))?;
                let _ = std::fs::remove_file(app.store.meta().join("setup-code"));
                // Files already in the folder become the new owner's.
                spaces::migrate(&app.store).map_err(|e| (500, e.to_string()))?;
                Ok(session(app, &user))
            })
            .await;
            match r {
                Some(Ok(resp)) => resp,
                Some(Err((code, why))) => {
                    if code == 401 {
                        tokio::time::sleep(Duration::from_millis(1500)).await;
                    }
                    error(code, &why)
                }
                None => error(500, "server error"),
            }
        }
        (["signup"], "POST") => {
            if !on {
                return Some(error(400, "Sign-in is off on this server."));
            }
            let r = blocking(app, move |app| {
                let user = auth::sign_up(&app.store, &field(&json, "username"), &field(&json, "name"), &field(&json, "password"), &field(&json, "code"))?;
                let _ = spaces::ensure_space(&app.store, &user.username);
                Ok(session(app, &user))
            })
            .await;
            match r {
                Some(Ok(resp)) => resp,
                Some(Err((code, why))) => {
                    if code == 401 {
                        tokio::time::sleep(Duration::from_millis(1500)).await; // slows down guessing
                    }
                    error(code, &why)
                }
                None => error(500, "server error"),
            }
        }
        (["signup", "settings"], "GET" | "POST") => {
            if me.as_ref().is_none_or(|u| !u.owner) {
                return Some(error(403, "Only the owner can do that."));
            }
            let posting = m == "POST";
            let r = blocking(app, move |app| {
                if posting { auth::set_signup(&app.store, json.get("on").and_then(Value::as_bool).unwrap_or(false), &field(&json, "code")) } else { Ok(auth::signup(&app.store)) }
            })
            .await;
            match r {
                Some(Ok(s)) => send_json(&json!({ "on": s.on, "code": s.code }), 200),
                Some(Err(why)) => error(400, &why),
                None => error(500, "server error"),
            }
        }
        (["login"], "POST") => {
            if !on {
                return Some(send_json(&json!({ "ok": true }), 200));
            }
            let r = blocking(app, move |app| auth::check(&app.store, &field(&json, "username"), &field(&json, "password")).map(|u| session(app, &u))).await.flatten();
            match r {
                Some(resp) => resp,
                None => {
                    tokio::time::sleep(Duration::from_millis(1500)).await; // slows down guessing
                    error(401, "That username or password isn't right.")
                }
            }
        }
        (["logout"], "POST") => {
            if let Some(t) = cookie_token(headers) {
                blocking(app, move |app| auth::end_session(&app.store, &t)).await;
            }
            cookie_reply("", 0, json!({ "ok": true }))
        }
        (["me"], "POST") => {
            let Some(me) = me else { return Some(error(401, "please sign in")) };
            let r = blocking(app, move |app| {
                let new_password = opt_field(&json, "password");
                if new_password.is_some() && auth::check(&app.store, &me.username, &field(&json, "current_password")).is_none() {
                    return Err("Your current password isn't right.".to_string());
                }
                auth::update(&app.store, &me.username, opt_field(&json, "name").as_deref(), opt_field(&json, "colour").as_deref(), new_password.as_deref())
                    .and_then(|u| if json.get("password").is_some() { auth::new_session(&app.store, &u.username).map(|t| (u, Some(t))).map_err(|e| e.to_string()) } else { Ok((u, None)) })
            })
            .await;
            match r {
                // A new password signs out everywhere else; this tab gets a new session.
                Some(Ok((u, Some(t)))) => cookie_reply(&t, auth::SESSION_DAYS * 86400, json!({ "ok": true, "user": u.json() })),
                Some(Ok((u, None))) => send_json(&json!({ "ok": true, "user": u.json() }), 200),
                Some(Err(why)) => error(400, &why),
                None => error(500, "server error"),
            }
        }
        (["users"], "GET") => {
            if !signed_in(app, headers) {
                return Some(error(401, "please sign in"));
            }
            let list = blocking(app, |app| auth::users(&app.store)).await.unwrap_or_default();
            send_json(&Value::Array(list.iter().map(auth::User::json).collect()), 200)
        }
        (["users"] | ["users", "password"], "POST" | "DELETE") => {
            let Some(me) = me.filter(|u| u.owner) else { return Some(error(403, "Only the owner can do that.")) };
            let (reset, deleting) = (path.len() == 2, m == "DELETE");
            let who = q.get("username").cloned().unwrap_or_default();
            let r = blocking(app, move |app| {
                if deleting {
                    if who == me.username {
                        return Err("You can't remove your own account.".to_string());
                    }
                    auth::remove(&app.store, &who).map(|_| json!({ "ok": true }))
                } else if reset {
                    auth::update(&app.store, &field(&json, "username"), None, None, Some(&field(&json, "password"))).map(|u| u.json())
                } else {
                    auth::add(&app.store, &field(&json, "username"), &field(&json, "name"), &field(&json, "password")).map(|u| {
                        let _ = spaces::ensure_space(&app.store, &u.username);
                        u.json()
                    })
                }
            })
            .await;
            match r {
                Some(Ok(v)) => send_json(&v, 200),
                Some(Err(why)) => error(400, &why),
                None => error(500, "server error"),
            }
        }
        _ => return None,
    })
}

// ---------------------------------------------------------------- sharing

/// Checks a files/folders/bin request against what the person may do, fills in where things go
/// by default (their own folder), and answers sharing and star requests itself. Returns the
/// query to use, or the answer to send.
async fn gate(app: &Shared, m: &str, p: &[String], mut q: HashMap<String, String>, me: Option<auth::User>, body: &[u8]) -> Result<HashMap<String, String>, Response<Body>> {
    if !app.auth {
        return Ok(q);
    }
    let Some(me) = me else { return Err(error(401, "please sign in")) };
    let (p, m) = (p.to_vec(), m.to_string());
    let json: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let r = blocking(app, move |app| -> Result<HashMap<String, String>, Response<Body>> {
        use sharing::{Item, Role};
        let store = &app.store;
        let _ = spaces::ensure_space(store, &me.username);
        let a = sharing::Access::new(store, true, Some(&me));
        let need = |have: Role, want: Role| -> Result<(), Response<Body>> {
            if have >= want {
                Ok(())
            } else if have == Role::None {
                Err(error(403, "You don't have access to that."))
            } else if want == Role::Owner {
                Err(error(403, "Only its owner can do that."))
            } else {
                Err(error(403, "You can only view that."))
            }
        };
        let home = me.username.clone();
        let rest: Vec<&str> = p.iter().map(String::as_str).collect();
        let path_q = |q: &HashMap<String, String>, k: &str| store::rel(q.get(k).map(String::as_str)).map_err(|b| error(b.code, &b.msg));
        match (rest.as_slice(), m.as_str()) {
            (["files"], "GET") => {}
            (["files"], "POST") => {
                let mut folder = path_q(&q, "folder")?;
                if folder.is_empty() {
                    folder = home.clone();
                }
                need(a.folder_role(&folder), Role::Edit)?;
                q.insert("folder".into(), folder);
            }
            (["files", id, sub @ ..], _) => {
                let role = a.file_role(id);
                let path = a.file_paths.get(*id).cloned().unwrap_or_default();
                match (sub, m.as_str()) {
                    ([], "GET") => {
                        need(role, Role::View)?;
                        let _ = spaces::note_opened(store, &me, id);
                    }
                    (["versions", ..], "GET") => need(role, Role::View)?,
                    ([], "PUT") | (["rename"], "POST") | ([], "DELETE") => need(role, Role::Edit)?,
                    (["copy"], "POST") => {
                        need(role, Role::View)?;
                        let parent = store::parent_of(&path);
                        let target = match q.get("folder") {
                            Some(f) => store::rel(Some(f)).map_err(|b| error(b.code, &b.msg))?,
                            None if a.folder_role(&parent) >= Role::Edit && !parent.is_empty() => parent,
                            None => home.clone(),
                        };
                        need(a.folder_role(&target), Role::Edit)?;
                        q.insert("folder".into(), target);
                    }
                    (["move"], "POST") => {
                        let target = path_q(&q, "folder")?;
                        need(a.folder_role(&target), Role::Edit)?;
                        // Into someone else's folder: it becomes theirs, so only its owner may.
                        need(role, if store::space_of(&target) == store::space_of(&path) { Role::Edit } else { Role::Owner })?;
                    }
                    (["sharing"], "GET") => {
                        need(role, Role::View)?;
                        return Err(send_json(&sharing::describe(store, Item::File(id), &path, role), 200));
                    }
                    (["sharing"], "POST") => {
                        need(role, Role::Owner)?;
                        return Err(match sharing::update(store, Item::File(id), &path, &json) {
                            Ok(()) => send_json(&sharing::describe(store, Item::File(id), &path, role), 200),
                            Err(why) => error(400, &why),
                        });
                    }
                    (["star"], "POST") => {
                        need(role, Role::View)?;
                        let on = json.get("on").and_then(Value::as_bool).unwrap_or(true);
                        return Err(match spaces::set_star(store, &me, &format!("f:{id}"), on) {
                            Ok(()) => send_json(&json!({ "ok": true }), 200),
                            Err(e) => error(500, &e.to_string()),
                        });
                    }
                    _ => need(role, Role::Owner)?,
                }
            }
            (["folders", sub @ ..], _) => {
                let mut path = path_q(&q, "path")?;
                match (sub, m.as_str()) {
                    ([], "POST") => {
                        if path.is_empty() {
                            path = home.clone();
                            q.insert("path".into(), path.clone());
                        }
                        need(a.folder_role(&path), Role::Edit)?;
                    }
                    (["rename"], "POST") | ([], "DELETE") => need(a.folder_role(&path), Role::Edit)?,
                    (["move"], "POST") => {
                        let to = path_q(&q, "to")?;
                        need(a.folder_role(&to), Role::Edit)?;
                        need(a.folder_role(&path), if store::space_of(&to) == store::space_of(&path) { Role::Edit } else { Role::Owner })?;
                    }
                    (["copy"], "POST") => {
                        need(a.folder_role(&path), Role::View)?;
                        if !q.contains_key("to") {
                            q.insert("to".into(), home.clone());
                        }
                        need(a.folder_role(&path_q(&q, "to")?), Role::Edit)?;
                    }
                    (["sharing" | "star"], _) => {
                        let role = a.folder_role(&path);
                        let Some(id) = a.folder_ids.get(&path).cloned() else { return Err(error(404, "no such folder")) };
                        if path == home || (!path.contains('/') && !path.is_empty()) {
                            return Err(error(400, "Share the folders inside it instead."));
                        }
                        return Err(match (sub, m.as_str()) {
                            (["sharing"], "GET") => {
                                need(role, Role::View)?;
                                send_json(&sharing::describe(store, Item::Folder(&id), &path, role), 200)
                            }
                            (["sharing"], "POST") => {
                                need(role, Role::Owner)?;
                                match sharing::update(store, Item::Folder(&id), &path, &json) {
                                    Ok(()) => send_json(&sharing::describe(store, Item::Folder(&id), &path, role), 200),
                                    Err(why) => error(400, &why),
                                }
                            }
                            (["star"], "POST") => {
                                need(role, Role::View)?;
                                let on = json.get("on").and_then(Value::as_bool).unwrap_or(true);
                                match spaces::set_star(store, &me, &format!("d:{id}"), on) {
                                    Ok(()) => send_json(&json!({ "ok": true }), 200),
                                    Err(e) => error(500, &e.to_string()),
                                }
                            }
                            _ => error(404, "not found"),
                        });
                    }
                    _ => need(a.folder_role(&path), Role::Owner)?,
                }
            }
            (["bin", ..], _) => {
                q.insert("space".into(), home.clone());
            }
            _ => {}
        }
        Ok(q)
    })
    .await;
    r.unwrap_or_else(|| Err(error(500, "server error")))
}

/// GET /api/files/ID/thumb: the file's preview picture (thumbs.rs), drawn if there isn't one for
/// this version yet. Outside the store lock while drawing. 404: no picture (the app shows an icon).
async fn thumb(app: &Shared, me: Option<auth::User>, file: String, headers: &HeaderMap) -> Response<Body> {
    enum Got {
        Have(Vec<u8>, String),
        Draw(String, Vec<u8>, String),
    }
    let f2 = file.clone();
    let got = blocking(app, move |app| -> Result<Got, Response<Body>> {
        if sharing::file_role(&app.store, app.auth, me.as_ref(), &f2) < sharing::Role::View {
            return Err(error(403, "You don't have access to that."));
        }
        let path = app.store.synced_index().ok().and_then(|i| i.get(&f2).cloned()).ok_or_else(|| error(404, "no such file"))?;
        let full = app.store.full(&path);
        let version = app.store.version_of(&full).map_err(|_| error(404, "no such file"))?;
        if let Some(png) = thumbs::cached(&app.store, &f2, &version) {
            return Ok(Got::Have(png, version));
        }
        if std::fs::metadata(&full).map(|m| m.len()).unwrap_or(0) > thumbs::MAX_READ {
            return Err(error(404, "too big for a picture"));
        }
        let bytes = std::fs::read(&full).map_err(|_| error(404, "no such file"))?;
        Ok(Got::Draw(store::base_of(&path), bytes, version))
    })
    .await
    .unwrap_or_else(|| Err(error(500, "server error")));
    let (png, version) = match got {
        Err(resp) => return resp,
        Ok(Got::Have(png, v)) => (png, v),
        Ok(Got::Draw(name, bytes, v)) => {
            let (store_root, f3, v2) = (app.clone(), file.clone(), v.clone());
            let drawn = tokio::task::spawn_blocking(move || {
                let png = thumbs::from_bytes(&name, &bytes)?;
                let _ = thumbs::keep(&store_root.store, &f3, &v2, &png);
                Ok::<_, String>(png)
            })
            .await;
            match drawn {
                Ok(Ok(png)) => (png, v),
                _ => return error(404, "no picture"),
            }
        }
    };
    let etag = format!("\"{version}\"");
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str()) {
        return Response::builder().status(304).header(header::ETAG, etag).body(Body::empty()).unwrap();
    }
    Response::builder()
        .status(200)
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "private, no-cache")
        .header(header::ETAG, etag)
        .body(Body::from(png))
        .unwrap()
}

/// After a files request, with accounts: the file list shows only what the person may see, with
/// who owns each thing, their role, stars and when they last opened it, and the tops of "Shared
/// with me".
fn after_files(app: &App, me: &auth::User, m: &str, p: &[String], r: Result<Reply, Bad>) -> Result<Reply, Bad> {
    let Ok(Reply::Json(v, code)) = r else { return r };
    if !(p.len() == 1 && p[0] == "files" && m == "GET") {
        return Ok(Reply::Json(v, code));
    }
    let a = sharing::Access::new(&app.store, true, Some(me));
    let starred = spaces::starred(&app.store, me);
    let opened = spaces::opened(&app.store, me);
    let files: Vec<Value> = v["files"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|mut f| {
            let id = f["id"].as_str()?.to_string();
            if !a.file_listed(&id) {
                return None;
            }
            let folder = f["folder"].as_str().unwrap_or("").to_string();
            f["owner"] = json!(store::space_of(&folder));
            f["role"] = json!(a.file_role(&id).name());
            f["starred"] = json!(starred.contains(&format!("f:{id}")));
            f["shared"] = json!(a.file_shared(&id));
            if let Some(t) = opened.get(&id) {
                f["opened"] = t.clone();
            }
            Some(f)
        })
        .collect();
    let folders: Vec<String> = v["folders"].as_array().cloned().unwrap_or_default().into_iter().filter_map(|f| f.as_str().map(str::to_string)).filter(|f| a.folder_listed(f)).collect();
    let info: serde_json::Map<String, Value> = folders
        .iter()
        .map(|f| {
            let id = a.folder_ids.get(f).cloned().unwrap_or_default();
            (f.clone(), json!({ "id": id, "owner": store::space_of(f), "role": a.folder_role(f).name(), "shared": a.folder_shared(f), "starred": starred.contains(&format!("d:{id}")) }))
        })
        .collect();
    let (shared_folders, shared_files) = a.shared_roots();
    let people: Vec<Value> = auth::users(&app.store).iter().map(|u| json!({ "username": u.username, "name": u.name, "colour": u.colour })).collect();
    Ok(Reply::Json(
        json!({
            "files": files, "folders": folders, "folder_info": info, "space": me.username, "admin": me.owner,
            "shared": { "folders": shared_folders, "files": shared_files }, "people": people,
        }),
        code,
    ))
}

// ---------------------------------------------------------------- the web app's files

fn is_hashed(name: &str) -> bool {
    let b = name.as_bytes();
    (0..b.len()).any(|i| {
        b[i] == b'-'
            && b.len() > i + 17
            && b[i + 1..i + 17].iter().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
            && matches!(b[i + 17], b'.' | b'_')
    })
}

async fn serve_site(site: &Path, path: &str, headers: &HeaderMap) -> Response<Body> {
    let mut rel = PathBuf::new();
    let mut escapes = false;
    for c in Path::new(path.trim_start_matches('/')).components() {
        match c {
            Component::Normal(p) => rel.push(p),
            Component::ParentDir => escapes = true,
            _ => {}
        }
    }
    if rel.as_os_str().is_empty() {
        rel.push("index.html");
    }
    let mut full = site.join(&rel);
    if escapes || !full.is_file() {
        let base = rel.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if base.contains('.') {
            return error(404, "not found");
        }
        full = site.join("index.html");
        if !full.is_file() {
            return error(404, "not found");
        }
    }
    let name = full.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let kind = if name.ends_with(".wasm") {
        "application/wasm".to_string()
    } else {
        mime_guess::from_path(&full).first_or_octet_stream().to_string()
    };
    let gz = PathBuf::from(format!("{}.gz", full.display()));
    let zipped = headers.get(header::ACCEPT_ENCODING).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("gzip")) && gz.is_file();
    let Ok(data) = tokio::fs::read(if zipped { &gz } else { &full }).await else {
        return error(404, "not found");
    };
    let mut b = Response::builder().status(200).header(header::CONTENT_TYPE, kind).header("X-Content-Type-Options", "nosniff");
    if zipped {
        b = b.header(header::CONTENT_ENCODING, "gzip").header(header::VARY, "Accept-Encoding");
    }
    if name == "index.html" || name == "build.json" {
        b = b.header(header::CACHE_CONTROL, "no-cache");
    } else if is_hashed(&name) {
        b = b.header(header::CACHE_CONTROL, "public, max-age=31536000, immutable");
    }
    b.body(Body::from(data)).unwrap()
}

// ---------------------------------------------------------------- requests

async fn handle(State(app): State<Shared>, method: Method, uri: Uri, headers: HeaderMap, body: Body) -> Response<Body> {
    let path = uri.path().to_string();
    if path == "/healthz" {
        return Response::builder().header(header::CONTENT_TYPE, "text/plain").body(Body::from("ok")).unwrap();
    }
    if let Some(site) = &app.site
        && !path.starts_with("/api/")
        && method == Method::GET
    {
        return serve_site(site, &path, &headers).await;
    }
    let parts: Vec<String> = path.split('/').filter(|x| !x.is_empty()).map(|x| percent_decode_str(x).decode_utf8_lossy().into_owned()).collect();
    if parts.first().map(String::as_str) != Some("api") {
        return error(404, "not found");
    }
    let p: Vec<String> = parts[1..].to_vec();
    let mut q = HashMap::new();
    for (k, v) in form_urlencoded::parse(uri.query().unwrap_or("").as_bytes()) {
        q.entry(k.into_owned()).or_insert_with(|| v.into_owned());
    }
    let length: u64 = headers.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0);
    if length > MAX_BYTES {
        return error(413, "file too large");
    }
    let body: Bytes = match axum::body::to_bytes(body, MAX_BYTES as usize).await {
        Ok(b) => b,
        Err(_) => return error(413, "file too large"),
    };
    let m = method.as_str().to_string();
    if let Some(resp) = accounts(&app, &m, &p, &q, &headers, &body).await {
        return resp;
    }
    if !signed_in(&app, &headers) {
        return error(401, "please sign in");
    }
    let me = current_user(&app, &headers);
    if m == "GET" && p.len() == 3 && p[0] == "files" && p[2] == "thumb" {
        return thumb(&app, me, p[1].clone(), &headers).await;
    }
    let q = match gate(&app, &m, &p, q, me.clone(), &body).await {
        Ok(q) => q,
        Err(resp) => return resp,
    };
    let if_match = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok()).map(str::to_string);
    // A file replaced through the API (a version restored) is read again by its live room.
    let replaced = (m == "PUT" && p.len() == 2 && p[0] == "files").then(|| p[1].clone());
    let (m2, p2) = (m.clone(), p.clone());
    let app2 = app.clone();
    let out = tokio::task::spawn_blocking(move || {
        let _g = app2.lock.lock().unwrap_or_else(|e| e.into_inner());
        app2.store.route(&m, &p, &q, if_match.as_deref(), &body)
    })
    .await;
    match out {
        Ok(mut r) => {
            if let (Some(id), Ok(Reply::Json(_, 200))) = (&replaced, &r) {
                app.rooms.replaced(id);
            }
            if app.auth && let Some(me) = me {
                r = blocking(&app, move |app| after_files(app, &me, &m2, &p2, r)).await.unwrap_or(Err(Bad::new(500, "server error")));
            }
            reply(r)
        }
        Err(_) => error(500, "server error"),
    }
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}

/// `starsheets-server healthcheck`: exits 0 when the server on PORT answers /healthz
/// (for Docker's HEALTHCHECK; the image has no curl or wget).
fn healthcheck() -> ! {
    use std::io::{Read, Write};
    let port = env("PORT", "8080");
    let ok = std::net::TcpStream::connect_timeout(&format!("127.0.0.1:{port}").parse().expect("port"), Duration::from_secs(4))
        .and_then(|mut s| {
            s.set_read_timeout(Some(Duration::from_secs(4)))?;
            s.write_all(b"GET /healthz HTTP/1.0\r\nHost: localhost\r\n\r\n")?;
            let mut buf = [0u8; 16];
            let n = s.read(&mut buf)?;
            Ok(buf[..n].starts_with(b"HTTP/1.1 200") || buf[..n].starts_with(b"HTTP/1.0 200"))
        })
        .unwrap_or(false);
    std::process::exit(if ok { 0 } else { 1 })
}

#[tokio::main]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        healthcheck();
    }
    let store = Store { root: PathBuf::from(env("STORE", "/data")), keep: env("KEEP", "6").parse().unwrap_or(6), spaces: !matches!(env("STARSHEETS_AUTH", "on").trim().to_lowercase().as_str(), "off" | "false" | "0" | "no") };
    let port: u16 = env("PORT", "8080").parse().unwrap_or(8080);
    let auth = !matches!(env("STARSHEETS_AUTH", "on").trim().to_lowercase().as_str(), "off" | "false" | "0" | "no");
    let site = Some(env("SITE", "")).filter(|s| !s.is_empty()).map(PathBuf::from);
    let app = Arc::new(App { store, lock: Mutex::new(()), location: env("LOCATION", "the server"), auth, site, rooms: live::Rooms::default() });
    if let Err(e) = std::fs::create_dir_all(app.store.meta()) {
        eprintln!("Can't use the folder {}: {e}", app.store.root.display());
        std::process::exit(1);
    }
    println!("StarSheets server {}: {} on :{port}, keeping {} versions", env!("CARGO_PKG_VERSION"), app.store.root.display(), app.store.keep);
    if !auth {
        println!("Sign-in is OFF (STARSHEETS_AUTH=off): anyone who can reach this port can read and change every file.");
    } else if !auth::any_users(&app.store) {
        println!("No accounts yet. Open StarSheets and create the owner account with this setup code: {}", auth::setup_code(&app.store));
    } else {
        println!("Sign-in is on ({} accounts).", auth::users(&app.store).len());
        match spaces::migrate(&app.store) {
            Ok(0) => {}
            Ok(n) => println!("Moved {n} files and folders into people's own folders (once, for accounts)."),
            Err(e) => {
                eprintln!("Couldn't move files into people's own folders: {e}");
                std::process::exit(1);
            }
        }
    }
    let router = Router::new().route("/api/live", axum::routing::get(live::ws)).fallback(handle).with_state(app).layer(axum::extract::DefaultBodyLimit::disable());
    let listener = match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Can't listen on port {port}: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = axum::serve(listener, router).with_graceful_shutdown(shutdown()).await {
        eprintln!("server stopped: {e}");
    }
}
