//! Who may open which file (live editing step 4), as in Google Drive: each file has an owner,
//! people it's shared with (can view or can edit) and a general setting ("anyone with the link",
//! meaning anyone signed in to this server who has the link). Kept in
//! STORE/.starsheets/sharing.json by file id.
//!
//! The server's owner (the first account, who runs the server and its folder) can open and manage
//! every file. Files from before sharing belong to the server's owner. With sign-in off everyone
//! may do everything, as before.

use serde_json::{Map, Value, json};

use crate::auth::{self, User};
use crate::store::Store;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    #[default]
    None,
    View,
    Edit,
    Owner,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::None => "none",
            Role::View => "view",
            Role::Edit => "edit",
            Role::Owner => "owner",
        }
    }
    fn parse(s: &str) -> Role {
        match s {
            "view" => Role::View,
            "edit" => Role::Edit,
            _ => Role::None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Share {
    pub owner: String,
    /// username → View or Edit, in the order they were added.
    pub people: Vec<(String, Role)>,
    /// What anyone signed in with the link may do (None = restricted).
    pub link: Role,
}

fn path(store: &Store) -> std::path::PathBuf {
    store.meta().join("sharing.json")
}

fn server_owner(store: &Store) -> String {
    auth::users(store).into_iter().find(|u| u.owner).map(|u| u.username).unwrap_or_default()
}

fn all(store: &Store) -> Map<String, Value> {
    store.load_json(&path(store)).get("files").and_then(Value::as_object).cloned().unwrap_or_default()
}

pub fn get(store: &Store, file: &str) -> Share {
    let files = all(store);
    let Some(f) = files.get(file) else { return Share { owner: server_owner(store), people: Vec::new(), link: Role::None } };
    let people = f.get("people").and_then(Value::as_array).cloned().unwrap_or_default();
    Share {
        owner: f.get("owner").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| server_owner(store)),
        people: people.iter().filter_map(|p| Some((p.get("username")?.as_str()?.to_string(), Role::parse(p.get("role")?.as_str()?)))).filter(|(_, r)| *r != Role::None).collect(),
        link: Role::parse(f.get("link").and_then(Value::as_str).unwrap_or("none")),
    }
}

pub fn set(store: &Store, file: &str, share: &Share) -> std::io::Result<()> {
    let mut files = all(store);
    let people: Vec<Value> = share.people.iter().map(|(u, r)| json!({ "username": u, "role": r.name() })).collect();
    files.insert(file.to_string(), json!({ "owner": share.owner, "people": people, "link": share.link.name() }));
    store.save_json(&path(store), &json!({ "files": files }))
}

/// A new file (or a copy) belongs to whoever made it.
pub fn new_file(store: &Store, file: &str, user: &User) -> std::io::Result<()> {
    set(store, file, &Share { owner: user.username.clone(), people: Vec::new(), link: Role::None })
}

/// What `user` may do with `file`. `auth` false (sign-in off): everything.
pub fn role(store: &Store, auth: bool, user: Option<&User>, file: &str) -> Role {
    if !auth {
        return Role::Owner;
    }
    let Some(u) = user else { return Role::None };
    if u.owner {
        return Role::Owner;
    }
    let share = get(store, file);
    if share.owner == u.username {
        return Role::Owner;
    }
    let mine = share.people.iter().find(|(p, _)| *p == u.username).map_or(Role::None, |p| p.1);
    mine.max(share.link)
}

/// Whether `file` shows in `user`'s file list: theirs or shared with them by name (a file opened
/// only through its link isn't listed, as in Drive before it's added).
pub fn listed(store: &Store, auth: bool, user: Option<&User>, file: &str) -> bool {
    if !auth || user.is_some_and(|u| u.owner) {
        return true;
    }
    let Some(u) = user else { return false };
    let share = get(store, file);
    share.owner == u.username || share.people.iter().any(|(p, _)| *p == u.username)
}

/// The sharing settings as the Share dialog shows them.
pub fn describe(store: &Store, file: &str, you: Role) -> Value {
    let share = get(store, file);
    let users = auth::users(store);
    let person = |username: &str, role: &str| {
        let u = users.iter().find(|u| u.username == username);
        json!({ "username": username, "name": u.map_or(username, |u| u.name.as_str()), "colour": u.map_or("#5F6368", |u| u.colour.as_str()), "role": role })
    };
    let mut people = vec![person(&share.owner, "owner")];
    people.extend(share.people.iter().map(|(u, r)| person(u, r.name())));
    json!({ "people": people, "link": share.link.name(), "you": you.name() })
}

/// Applies the Share dialog's changes: `people` maps username → "view", "edit" or "remove";
/// `link` is "none", "view" or "edit".
pub fn update(store: &Store, file: &str, body: &Value) -> Result<(), String> {
    let mut share = get(store, file);
    let known: Vec<String> = auth::users(store).into_iter().map(|u| u.username).collect();
    if let Some(people) = body.get("people").and_then(Value::as_object) {
        for (username, role) in people {
            let role = role.as_str().unwrap_or("");
            if *username == share.owner {
                continue;
            }
            share.people.retain(|(u, _)| u != username);
            if role != "remove" {
                if !known.contains(username) {
                    return Err(format!("There's no account called {username}."));
                }
                let r = Role::parse(role);
                if r == Role::None {
                    return Err("A person can view or edit.".into());
                }
                share.people.push((username.clone(), r));
            }
        }
    }
    if let Some(link) = body.get("link").and_then(Value::as_str) {
        share.link = Role::parse(link);
    }
    set(store, file, &share).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles() {
        let dir = std::env::temp_dir().join(format!("starsheets-share-{}", crate::store::random_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let s = Store { root: dir.clone(), keep: 2 };
        let boss = auth::add(&s, "boss", "Boss", "bosspassword").unwrap();
        let ann = auth::add(&s, "ann", "Ann", "annpassword").unwrap();
        let bob = auth::add(&s, "bob", "Bob", "bobpassword").unwrap();
        // Old files belong to the server's owner.
        assert_eq!(role(&s, true, Some(&ann), "old"), Role::None);
        assert_eq!(role(&s, true, Some(&boss), "old"), Role::Owner);
        new_file(&s, "f", &ann).unwrap();
        assert_eq!(role(&s, true, Some(&ann), "f"), Role::Owner);
        assert_eq!(role(&s, true, Some(&bob), "f"), Role::None);
        assert!(!listed(&s, true, Some(&bob), "f"));
        update(&s, "f", &json!({ "people": { "bob": "view" } })).unwrap();
        assert_eq!(role(&s, true, Some(&bob), "f"), Role::View);
        assert!(listed(&s, true, Some(&bob), "f"));
        update(&s, "f", &json!({ "link": "edit" })).unwrap();
        assert_eq!(role(&s, true, Some(&bob), "f"), Role::Edit, "the link gives more");
        update(&s, "f", &json!({ "people": { "bob": "remove" }, "link": "none" })).unwrap();
        assert_eq!(role(&s, true, Some(&bob), "f"), Role::None);
        assert!(update(&s, "f", &json!({ "people": { "nobody": "edit" } })).is_err());
        assert_eq!(role(&s, false, None, "f"), Role::Owner, "sign-in off");
        assert_eq!(role(&s, true, None, "f"), Role::None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
