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
mod store;

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

/// /api/info, /api/setup, /api/login, /api/logout, /api/me and /api/users; `None` for other paths.
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
            send_json(&json!({ "location": app.location, "auth": on, "signed_in": !on || me.is_some(), "setup": setup, "user": me.map(|u| u.json()) }), 200)
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
                    auth::add(&app.store, &field(&json, "username"), &field(&json, "name"), &field(&json, "password")).map(|u| u.json())
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

/// Checks a files/folders/bin request against the person's role, and answers the sharing
/// endpoints (/api/files/ID/sharing). `None`: go ahead.
async fn sharing_gate(app: &Shared, m: &str, p: &[String], me: Option<auth::User>, body: &[u8]) -> Option<Response<Body>> {
    if !app.auth {
        return None;
    }
    let admin = me.as_ref().is_some_and(|u| u.owner);
    match p.first().map(String::as_str) {
        Some("folders" | "bin") if !admin && !(p[0] == "folders" && m == "POST" && p.len() == 1) => {
            return Some(error(403, "Only the owner of this server can change folders and the bin."));
        }
        Some("files") if p.len() >= 2 => {}
        _ => return None,
    }
    let (file, rest, m) = (p[1].clone(), p[2..].to_vec(), m.to_string());
    let json: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let r = blocking(app, move |app| {
        let role = sharing::role(&app.store, app.auth, me.as_ref(), &file);
        let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
        let need = match (rest.as_slice(), m.as_str()) {
            (["sharing"], "GET") | ([], "GET") | (["versions", ..], "GET") | (["copy"], "POST") => sharing::Role::View,
            ([], "PUT") | (["rename"], "POST") => sharing::Role::Edit,
            _ => sharing::Role::Owner,
        };
        if role < need {
            let why = if role == sharing::Role::None { "You don't have access to this file." } else { "You can only view this file." };
            return Some(error(403, why));
        }
        match (rest.as_slice(), m.as_str()) {
            (["sharing"], "GET") => Some(send_json(&sharing::describe(&app.store, &file, role), 200)),
            (["sharing"], "POST") => Some(match sharing::update(&app.store, &file, &json) {
                Ok(()) => send_json(&sharing::describe(&app.store, &file, role), 200),
                Err(why) => error(400, &why),
            }),
            _ => None,
        }
    })
    .await;
    r.unwrap_or_else(|| Some(error(500, "server error")))
}

/// After a files request: new files and copies belong to whoever made them, and the file list
/// shows only what the person may see (with the folders on the way to it).
fn after_files(app: &App, me: &auth::User, m: &str, p: &[String], r: Result<Reply, Bad>) -> Result<Reply, Bad> {
    let Ok(Reply::Json(mut v, code)) = r else { return r };
    let path: Vec<&str> = p.iter().map(String::as_str).collect();
    match (path.as_slice(), m) {
        (["files"], "POST") | (["files", _, "copy"], "POST") if code < 300 => {
            if let Some(id) = v.get("id").and_then(Value::as_str) {
                let _ = sharing::new_file(&app.store, id, me);
            }
        }
        (["files"], "GET") if !me.owner => {
            let files: Vec<Value> = v["files"].as_array().cloned().unwrap_or_default().into_iter().filter(|f| f["id"].as_str().is_some_and(|id| sharing::listed(&app.store, true, Some(me), id))).collect();
            let mut folders: Vec<String> = Vec::new();
            for f in &files {
                let mut path = String::new();
                for part in f["folder"].as_str().unwrap_or("").split('/').filter(|x| !x.is_empty()) {
                    path = if path.is_empty() { part.to_string() } else { format!("{path}/{part}") };
                    if !folders.contains(&path) {
                        folders.push(path.clone());
                    }
                }
            }
            v = json!({ "files": files, "folders": folders });
        }
        _ => {}
    }
    Ok(Reply::Json(v, code))
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
    if let Some(resp) = sharing_gate(&app, &m, &p, me.clone(), &body).await {
        return resp;
    }
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
    let store = Store { root: PathBuf::from(env("STORE", "/data")), keep: env("KEEP", "6").parse().unwrap_or(6) };
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
