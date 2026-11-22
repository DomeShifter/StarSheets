//! Accounts (live editing step 2): people sign in with a username and password kept on this
//! server (Argon2 hashes in STORE/.starsheets/users.json). No sign-in through other companies.
//!
//! The first account is the owner's. It is made in the app with a setup code from the server's
//! log (or STARSHEETS_PASSWORD when set), so nobody else can claim a new server first. The owner
//! adds other people; everyone can change their own name, colour and password.

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use serde_json::{Map, Value, json};

use crate::store::{Store, now, random_hex};

pub const SESSION_DAYS: u64 = 30;

/// Colours given to people in turn (cursors and avatars), readable on white.
pub const COLOURS: [&str; 10] = ["#1A73E8", "#D93025", "#188038", "#E37400", "#9334E6", "#007B83", "#E52592", "#B06000", "#3367D6", "#5F6368"];

#[derive(Clone, Debug, PartialEq)]
pub struct User {
    pub username: String,
    pub name: String,
    pub colour: String,
    pub owner: bool,
}

impl User {
    pub fn json(&self) -> Value {
        json!({ "username": self.username, "name": self.name, "colour": self.colour, "owner": self.owner })
    }
}

fn users_path(store: &Store) -> std::path::PathBuf {
    store.meta().join("users.json")
}

fn sessions_path(store: &Store) -> std::path::PathBuf {
    store.meta().join("sessions.json")
}

fn load(store: &Store) -> Vec<Map<String, Value>> {
    store.load_json(&users_path(store)).get("users").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().filter_map(|u| u.as_object().cloned()).collect()
}

fn save(store: &Store, users: &[Map<String, Value>]) -> std::io::Result<()> {
    store.save_json(&users_path(store), &json!({ "users": users }))
}

fn to_user(u: &Map<String, Value>) -> User {
    let s = |k: &str| u.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    User { username: s("username"), name: s("name"), colour: s("colour"), owner: u.get("owner").and_then(Value::as_bool).unwrap_or(false) }
}

pub fn users(store: &Store) -> Vec<User> {
    load(store).iter().map(to_user).collect()
}

pub fn any_users(store: &Store) -> bool {
    !load(store).is_empty()
}

// ---------------------------------------------------------------- passwords

pub fn hash(password: &str) -> Result<String, String> {
    let mut salt = [0u8; 16];
    rand::fill(&mut salt[..]);
    let h: PasswordHash = Argon2::default().hash_password_with_salt(password.as_bytes(), &salt).map_err(|e| e.to_string())?;
    Ok(h.to_string())
}

fn verify(password: &str, stored: &str) -> bool {
    PasswordHash::new(stored).is_ok_and(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

/// Checks a username and password (slow on purpose: Argon2).
pub fn check(store: &Store, username: &str, password: &str) -> Option<User> {
    let username = username.trim().to_lowercase();
    let users = load(store);
    let u = users.iter().find(|u| u.get("username").and_then(Value::as_str) == Some(username.as_str()))?;
    verify(password, u.get("hash").and_then(Value::as_str)?).then(|| to_user(u))
}

// ---------------------------------------------------------------- checks on what people type

pub fn clean_username(s: &str) -> Result<String, String> {
    let u = s.trim().to_lowercase();
    if !(2..=32).contains(&u.chars().count()) || !u.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
        return Err("A username is 2 to 32 letters, numbers, dots, dashes or underscores.".into());
    }
    Ok(u)
}

pub fn clean_display(s: &str) -> Result<String, String> {
    let n: String = s.trim().chars().filter(|c| !c.is_control()).take(60).collect();
    if n.is_empty() { Err("Please give a name.".into()) } else { Ok(n) }
}

pub fn good_password(p: &str) -> Result<(), String> {
    if p.chars().count() < 8 { Err("Use at least 8 characters for the password.".into()) } else { Ok(()) }
}

fn clean_colour(s: &str) -> Option<String> {
    let s = s.trim();
    (s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())).then(|| s.to_uppercase())
}

// ---------------------------------------------------------------- changing accounts

/// Adds an account (the first one is the owner's).
pub fn add(store: &Store, username: &str, name: &str, password: &str) -> Result<User, String> {
    let username = clean_username(username)?;
    let name = clean_display(name)?;
    good_password(password)?;
    let mut users = load(store);
    if users.iter().any(|u| u.get("username").and_then(Value::as_str) == Some(username.as_str())) {
        return Err("That username is taken.".into());
    }
    let owner = users.is_empty();
    let colour = COLOURS[users.len() % COLOURS.len()].to_string();
    let hash = hash(password)?;
    let mut u = Map::new();
    u.insert("username".into(), json!(username));
    u.insert("name".into(), json!(name));
    u.insert("colour".into(), json!(colour));
    u.insert("owner".into(), json!(owner));
    u.insert("hash".into(), json!(hash));
    u.insert("created".into(), json!(now() as i64));
    let user = to_user(&u);
    users.push(u);
    save(store, &users).map_err(|e| e.to_string())?;
    Ok(user)
}

/// Changes someone's name, colour or password (`None` leaves it).
pub fn update(store: &Store, username: &str, name: Option<&str>, colour: Option<&str>, password: Option<&str>) -> Result<User, String> {
    let mut users = load(store);
    let u = users.iter_mut().find(|u| u.get("username").and_then(Value::as_str) == Some(username)).ok_or("No such person.")?;
    if let Some(n) = name {
        u.insert("name".into(), json!(clean_display(n)?));
    }
    if let Some(c) = colour {
        u.insert("colour".into(), json!(clean_colour(c).ok_or("That isn't a colour.")?));
    }
    if let Some(p) = password {
        good_password(p)?;
        u.insert("hash".into(), json!(hash(p)?));
    }
    let user = to_user(u);
    save(store, &users).map_err(|e| e.to_string())?;
    if password.is_some() {
        end_sessions(store, username);
    }
    Ok(user)
}

pub fn remove(store: &Store, username: &str) -> Result<(), String> {
    let mut users = load(store);
    let before = users.len();
    users.retain(|u| u.get("username").and_then(Value::as_str) != Some(username) || u.get("owner").and_then(Value::as_bool) == Some(true));
    if users.len() == before {
        return Err("That person can't be removed.".into());
    }
    save(store, &users).map_err(|e| e.to_string())?;
    end_sessions(store, username);
    Ok(())
}

// ---------------------------------------------------------------- sessions

fn token_hash(token: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// A new sign-in for `username`: the cookie value.
pub fn new_session(store: &Store, username: &str) -> std::io::Result<String> {
    let token = random_hex(32);
    let t = now();
    let mut sessions = store.load_json(&sessions_path(store));
    // Sessions from before accounts (a bare expiry time) and expired ones go.
    sessions.retain(|_, v| v.get("expires").and_then(Value::as_f64).unwrap_or(0.0) > t);
    sessions.insert(token_hash(&token), json!({ "user": username, "expires": t + (SESSION_DAYS * 86400) as f64 }));
    store.save_json(&sessions_path(store), &Value::Object(sessions))?;
    Ok(token)
}

pub fn end_session(store: &Store, token: &str) {
    let mut sessions = store.load_json(&sessions_path(store));
    if sessions.remove(&token_hash(token)).is_some() {
        let _ = store.save_json(&sessions_path(store), &Value::Object(sessions));
    }
}

fn end_sessions(store: &Store, username: &str) {
    let mut sessions = store.load_json(&sessions_path(store));
    sessions.retain(|_, v| v.get("user").and_then(Value::as_str) != Some(username));
    let _ = store.save_json(&sessions_path(store), &Value::Object(sessions));
}

/// Who a session cookie belongs to.
pub fn session_user(store: &Store, token: &str) -> Option<User> {
    let sessions = store.load_json(&sessions_path(store));
    let s = sessions.get(&token_hash(token))?;
    if s.get("expires").and_then(Value::as_f64).unwrap_or(0.0) <= now() {
        return None;
    }
    let username = s.get("user").and_then(Value::as_str)?;
    users(store).into_iter().find(|u| u.username == username)
}

// ---------------------------------------------------------------- the setup code

/// The code that makes the owner account on a new server: STARSHEETS_PASSWORD when set (older
/// installs), else one made on first start and kept in STORE/.starsheets/setup-code.
pub fn setup_code(store: &Store) -> String {
    let given = std::env::var("STARSHEETS_PASSWORD").unwrap_or_default();
    if !given.is_empty() {
        return given;
    }
    let path = store.meta().join("setup-code");
    if let Ok(s) = std::fs::read_to_string(&path) {
        return s.trim().to_string();
    }
    let made = random_hex(5);
    let _ = std::fs::create_dir_all(store.meta());
    let _ = std::fs::write(&path, format!("{made}\n"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    made
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let dir = std::env::temp_dir().join(format!("starsheets-auth-{}", random_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        Store { root: dir, keep: 2 }
    }

    #[test]
    fn accounts_and_sessions() {
        let s = store();
        assert!(add(&s, "x", "X", "longenough").is_err(), "short username");
        assert!(add(&s, "james", "James", "short").is_err(), "short password");
        let owner = add(&s, " James ", "James", "correct horse").unwrap();
        assert!(owner.owner && owner.username == "james");
        let other = add(&s, "aurelie", "Aurélie", "another one").unwrap();
        assert!(!other.owner && other.colour != owner.colour);
        assert!(add(&s, "JAMES", "Again", "whatever12").is_err(), "taken");
        assert!(check(&s, "james", "wrong password").is_none());
        assert_eq!(check(&s, "James", "correct horse").unwrap().username, "james");
        let token = new_session(&s, "aurelie").unwrap();
        assert_eq!(session_user(&s, &token).unwrap().name, "Aurélie");
        update(&s, "aurelie", Some("A."), Some("#00ff00"), None).unwrap();
        assert_eq!(session_user(&s, &token).unwrap().colour, "#00FF00");
        update(&s, "aurelie", None, None, Some("a new password")).unwrap();
        assert!(session_user(&s, &token).is_none(), "a new password signs out everywhere");
        assert!(remove(&s, "james").is_err(), "the owner stays");
        remove(&s, "aurelie").unwrap();
        assert_eq!(users(&s).len(), 1);
        let _ = std::fs::remove_dir_all(&s.root);
    }
}
