//! StarSheets server: stores spreadsheets as ordinary .xlsx files in a folder tree (STORE) and,
//! with SITE set, serves the web app too. Replaces server/files.py with the same API and the
//! same files on disk (see docs/server.md), as the base for live editing.
//!
//! Settings (environment): STORE (folder, default /data), KEEP (versions kept, 6), PORT (8080),
//! LOCATION (how people are told where files are kept), STARSHEETS_AUTH (off = no login),
//! STARSHEETS_PASSWORD, SITE (a built StarSheets site to serve).

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
use sha2::{Digest, Sha256};

use store::{Bad, Reply, Store, now, random_hex};

const MAX_BYTES: u64 = 200 * 1024 * 1024;
const SESSION_DAYS: u64 = 30;
const COOKIE: &str = "starsheets_session";

struct App {
    store: Store,
    lock: Mutex<()>,
    location: String,
    auth: bool,
    site: Option<PathBuf>,
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

// ---------------------------------------------------------------- login

/// The password: STARSHEETS_PASSWORD, else one made (and kept) on first start.
fn password(app: &App) -> String {
    let given = env("STARSHEETS_PASSWORD", "");
    if !given.is_empty() {
        return given;
    }
    let path = app.store.meta().join("password");
    if let Ok(s) = std::fs::read_to_string(&path) {
        return s.trim().to_string();
    }
    let made = random_hex(9);
    let _ = std::fs::create_dir_all(app.store.meta());
    let _ = std::fs::write(&path, format!("{made}\n"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    made
}

fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn sessions_path(app: &App) -> PathBuf {
    app.store.meta().join("sessions.json")
}

fn new_session(app: &App) -> std::io::Result<String> {
    let token = random_hex(32);
    let t = now();
    let mut sessions = app.store.load_json(&sessions_path(app));
    sessions.retain(|_, v| v.as_f64().unwrap_or(0.0) > t);
    sessions.insert(token_hash(&token), json!(t + (SESSION_DAYS * 86400) as f64));
    app.store.save_json(&sessions_path(app), &Value::Object(sessions))?;
    Ok(token)
}

fn signed_in(app: &App, headers: &HeaderMap) -> bool {
    if !app.auth {
        return true;
    }
    let cookies = headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).collect::<Vec<_>>().join(";");
    for part in cookies.split(';') {
        let (name, value) = part.trim().split_once('=').unwrap_or((part.trim(), ""));
        if name == COOKIE && !value.is_empty() {
            let sessions = app.store.load_json(&sessions_path(app));
            return sessions.get(&token_hash(value)).and_then(Value::as_f64).unwrap_or(0.0) > now();
        }
    }
    false
}

fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn cookie_reply(value: &str, max_age: u64, body: &'static str) -> Response<Body> {
    Response::builder()
        .status(200)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::SET_COOKIE, format!("{COOKIE}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Strict"))
        .body(Body::from(body))
        .unwrap()
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
    if p == ["info"] && m == "GET" {
        return send_json(&json!({"location": app.location, "auth": app.auth, "signed_in": signed_in(&app, &headers)}), 200);
    }
    if p == ["login"] && m == "POST" {
        let given = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|v| v.get("password").map(|p| p.as_str().map(str::to_string).unwrap_or_else(|| p.to_string())))
            .unwrap_or_default();
        let app2 = app.clone();
        let good = tokio::task::spawn_blocking(move || same(given.as_bytes(), password(&app2).as_bytes())).await.unwrap_or(false);
        if !good {
            tokio::time::sleep(Duration::from_millis(1500)).await; // slows down guessing
            return error(401, "wrong password");
        }
        let app2 = app.clone();
        let token = tokio::task::spawn_blocking(move || {
            let _g = app2.lock.lock().unwrap_or_else(|e| e.into_inner());
            new_session(&app2)
        })
        .await;
        return match token {
            Ok(Ok(t)) => cookie_reply(&t, SESSION_DAYS * 86400, "{\"ok\": true}"),
            _ => error(500, "storage error"),
        };
    }
    if p == ["logout"] && m == "POST" {
        return cookie_reply("", 0, "");
    }
    if !signed_in(&app, &headers) {
        return error(401, "please sign in");
    }
    let if_match = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok()).map(str::to_string);
    let out = tokio::task::spawn_blocking(move || {
        let _g = app.lock.lock().unwrap_or_else(|e| e.into_inner());
        app.store.route(&m, &p, &q, if_match.as_deref(), &body)
    })
    .await;
    match out {
        Ok(r) => reply(r),
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
    let app = Arc::new(App { store, lock: Mutex::new(()), location: env("LOCATION", "the server"), auth, site });
    if let Err(e) = std::fs::create_dir_all(app.store.meta()) {
        eprintln!("Can't use the folder {}: {e}", app.store.root.display());
        std::process::exit(1);
    }
    println!("StarSheets server {}: {} on :{port}, keeping {} versions", env!("CARGO_PKG_VERSION"), app.store.root.display(), app.store.keep);
    if !auth {
        println!("Login is OFF (STARSHEETS_AUTH=off): anyone who can reach this port can read and change every file.");
    } else if !env("STARSHEETS_PASSWORD", "").is_empty() {
        println!("Login is on, with the password from STARSHEETS_PASSWORD.");
    } else {
        println!("Login is on. Password (set STARSHEETS_PASSWORD to choose your own): {}", password(&app));
    }
    let router = Router::new().fallback(handle).with_state(app).layer(axum::extract::DefaultBodyLimit::disable());
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
