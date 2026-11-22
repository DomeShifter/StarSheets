//! Accounts (live editing step 2): signing in with a username and password kept on the
//! StarSheets server, the first-time setup of the owner's account, your avatar menu (name and
//! colour, password, people, sign out), and the owner's list of people.
//! No sign-in through other companies (docs/decisions.md).

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Align2, Color32, RichText, Sense, Ui, vec2};
use serde_json::{Value as Json, json};

use crate::files::{Method, fetch};
use crate::house;
use crate::theme;

/// Colours people can pick (the same ones the server hands out).
const COLOURS: [&str; 10] = ["#1A73E8", "#D93025", "#188038", "#E37400", "#9334E6", "#007B83", "#E52592", "#B06000", "#3367D6", "#5F6368"];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Me {
    pub username: String,
    pub name: String,
    pub colour: String,
    pub owner: bool,
}

thread_local! {
    static ME: RefCell<Option<Me>> = const { RefCell::new(None) };
    /// Set when a request came back "please sign in".
    static SIGN_IN: RefCell<bool> = const { RefCell::new(false) };
}

/// The person signed in in this tab (None with sign-in off).
pub fn me() -> Option<Me> {
    ME.with(|m| m.borrow().clone())
}

/// A request needed sign-in: show the sign-in box.
pub fn ask_sign_in() {
    SIGN_IN.with(|s| *s.borrow_mut() = true);
}

pub fn parse_colour(hex: &str) -> Color32 {
    let h = hex.trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("80"), 16).unwrap_or(0x80);
    Color32::from_rgb(p(0), p(2), p(4))
}

/// A round avatar with the first letter of `name`. Returns its response (hover, click).
pub fn avatar(ui: &mut Ui, name: &str, colour: &str, size: f32) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let p = ui.painter();
    p.circle_filled(rect.center(), size / 2.0, parse_colour(colour));
    let letter: String = name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into());
    p.text(rect.center(), Align2::CENTER_CENTER, letter, egui::FontId::proportional(size * 0.5), Color32::WHITE);
    r
}

/// What the avatar menu asked for (in egui memory, picked up by `Account::logic`).
const REQUEST: &str = "starsheets_account_request";

/// The avatars at the right of the title row: the other people in this file (left of Share),
/// drawn by `others_ui`, and yours with its menu (`me_ui`, at the far right).
pub fn me_ui(ui: &mut Ui) {
    let Some(me) = me() else { return };
    let r = avatar(ui, &me.name, &me.colour, 32.0).on_hover_text(format!("{} ({})", me.name, me.username));
    let ask = |ui: &Ui, what: &str| ui.ctx().data_mut(|m| m.insert_temp(egui::Id::new(REQUEST), what.to_string()));
    egui::Popup::menu(&r).show(|ui| {
        ui.set_min_width(220.0);
        house::menu_inset(ui, |ui| {
            ui.label(RichText::new(&me.name).size(house::BODY).strong().color(theme::TEXT));
            house::note(ui, &me.username);
        });
        ui.separator();
        if ui.button("Your name and colour…").clicked() {
            ask(ui, "profile");
        }
        if ui.button("Change password…").clicked() {
            ask(ui, "password");
        }
        if me.owner && ui.button("People…").clicked() {
            ask(ui, "people");
        }
        ui.separator();
        if ui.button("Sign out").clicked() {
            ask(ui, "signout");
        }
    });
}

/// The other people with this file open.
pub fn others_ui(ui: &mut Ui) {
    let others = crate::live::others();
    for p in others.iter().rev().take(5) {
        avatar(ui, &p.name, &p.colour, 30.0).on_hover_text(format!("{} is here", p.name));
        ui.add_space(2.0);
    }
    if others.len() > 5 {
        ui.label(RichText::new(format!("+{}", others.len() - 5)).size(house::NOTE).color(theme::TEXT_DIM)).on_hover_text(others[5..].iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join("\n"));
    }
}

enum Reply {
    Info { auth: bool, setup: bool, signed_in: bool, user: Option<Me> },
    /// Signed in or out, or the password changed: start the page again.
    Reload,
    Saved(Me),
    People(Vec<Me>),
    Error(String),
    Done(String),
}

#[derive(Default, PartialEq)]
enum Dialog {
    #[default]
    None,
    SignIn,
    Setup,
    Profile,
    Password,
    People,
}

#[derive(Default)]
pub struct Account {
    inbox: Rc<RefCell<Vec<Reply>>>,
    booted: bool,
    auth: bool,
    dialog: Dialog,
    busy: bool,
    error: Option<String>,
    done: Option<String>,
    focused: bool,
    // Form fields.
    code: String,
    username: String,
    name: String,
    password: String,
    password2: String,
    current: String,
    colour: String,
    people: Vec<Me>,
    remove_armed: Option<String>,
}

fn me_from(v: &Json) -> Option<Me> {
    let u = v.as_object()?;
    let s = |k: &str| u.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    Some(Me { username: s("username"), name: s("name"), colour: s("colour"), owner: u.get("owner").and_then(Json::as_bool).unwrap_or(false) })
}

/// Sends a JSON request; `then` turns a good answer into a Reply, errors become Reply::Error.
fn call(inbox: &Rc<RefCell<Vec<Reply>>>, ctx: &egui::Context, method: Method, url: &str, body: Option<Json>, then: impl FnOnce(Json) -> Reply + 'static) {
    let (inbox, ctx, url) = (inbox.clone(), ctx.clone(), url.to_string());
    wasm_bindgen_futures::spawn_local(async move {
        let body = body.map(|b| serde_json::to_vec(&b).unwrap_or_default());
        let reply = match fetch(method, &url, body, None).await {
            Ok((s, _, bytes)) => {
                let v: Json = serde_json::from_slice(&bytes).unwrap_or_default();
                if s < 400 { then(v) } else { Reply::Error(v["error"].as_str().map_or(format!("The server said {s}."), str::to_string)) }
            }
            Err(e) => Reply::Error(e),
        };
        inbox.borrow_mut().push(reply);
        ctx.request_repaint();
    });
}

fn reload() {
    crate::files::clear_unsaved_flag();
    if let Some(w) = web_sys::window() {
        let _ = w.location().reload();
    }
}

impl Account {
    pub fn logic(&mut self, ctx: &egui::Context) {
        if !self.booted {
            self.booted = true;
            call(&self.inbox, ctx, Method::Get, "api/info", None, |v| Reply::Info {
                auth: v["auth"].as_bool().unwrap_or(false),
                setup: v["setup"].as_bool().unwrap_or(false),
                signed_in: v["signed_in"].as_bool().unwrap_or(true),
                user: me_from(&v["user"]),
            });
        }
        let replies: Vec<Reply> = std::mem::take(&mut *self.inbox.borrow_mut());
        for r in replies {
            self.busy = false;
            match r {
                Reply::Info { auth, setup, signed_in, user } => {
                    self.auth = auth;
                    ME.with(|m| *m.borrow_mut() = user);
                    if auth && !signed_in {
                        self.open(if setup { Dialog::Setup } else { Dialog::SignIn });
                    }
                }
                Reply::Reload => reload(),
                Reply::Saved(me) => {
                    ME.with(|m| *m.borrow_mut() = Some(me));
                    self.dialog = Dialog::None;
                }
                Reply::People(p) => self.people = p,
                Reply::Error(e) => self.error = Some(e),
                Reply::Done(msg) => {
                    self.done = Some(msg);
                    self.error = None;
                    self.name.clear();
                    self.username.clear();
                    self.password.clear();
                    call(&self.inbox, ctx, Method::Get, "api/users", None, |v| Reply::People(v.as_array().into_iter().flatten().filter_map(me_from).collect()));
                }
            }
        }
        if let Some(req) = ctx.data_mut(|m| m.remove_temp::<String>(egui::Id::new(REQUEST))) {
            match req.as_str() {
                "profile" => self.open(Dialog::Profile),
                "password" => self.open(Dialog::Password),
                "people" => {
                    self.open(Dialog::People);
                    call(&self.inbox, ctx, Method::Get, "api/users", None, |v| Reply::People(v.as_array().into_iter().flatten().filter_map(me_from).collect()));
                }
                "signout" => call(&self.inbox, ctx, Method::Post, "api/logout", None, |_| Reply::Reload),
                _ => {}
            }
        }
        if SIGN_IN.with(|s| std::mem::take(&mut *s.borrow_mut())) && self.dialog != Dialog::Setup {
            self.open(Dialog::SignIn);
        }
    }

    /// Whether a sign-in or setup box is up (the rest of the app waits).
    pub fn blocking(&self) -> bool {
        matches!(self.dialog, Dialog::SignIn | Dialog::Setup)
    }

    fn open(&mut self, d: Dialog) {
        if self.dialog == d {
            return;
        }
        self.dialog = d;
        self.error = None;
        self.done = None;
        self.focused = false;
        self.password.clear();
        self.password2.clear();
        self.current.clear();
        self.remove_armed = None;
        if let Some(me) = me() {
            self.name = me.name;
            self.colour = me.colour;
        }
    }

    fn field(ui: &mut Ui, text: &mut String, hint: &str, password: bool) -> egui::Response {
        ui.add(egui::TextEdit::singleline(text).password(password).hint_text(hint).desired_width(f32::INFINITY).margin(vec2(house::M, house::S + 1.0)))
    }

    fn messages(&self, ui: &mut Ui) {
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).size(house::NOTE).color(house::ERROR));
        }
        if let Some(d) = &self.done {
            ui.label(RichText::new(d).size(house::NOTE).color(house::GREEN));
        }
    }

    /// Focuses the first box once (grabbing it every frame swallows Enter).
    fn first(&mut self, r: &egui::Response) {
        if !self.focused {
            r.request_focus();
            self.focused = true;
        }
    }

    fn enter(ui: &Ui, r: &egui::Response) -> bool {
        (r.lost_focus() || r.has_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter))
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        match self.dialog {
            Dialog::None => {}
            Dialog::SignIn => self.sign_in(ctx),
            Dialog::Setup => self.setup(ctx),
            Dialog::Profile => self.profile(ctx),
            Dialog::Password => self.change_password(ctx),
            Dialog::People => self.people(ctx),
        }
    }

    fn sign_in(&mut self, ctx: &egui::Context) {
        let mut go = false;
        house::dialog(ctx, "starsheets_sign_in", 360.0, "Sign in to StarSheets", None, |ui| {
            let r = Self::field(ui, &mut self.username, "Username", false);
            self.first(&r);
            go |= Self::enter(ui, &r);
            ui.add_space(house::S);
            let r = Self::field(ui, &mut self.password, "Password", true);
            go |= Self::enter(ui, &r);
            self.messages(ui);
            house::note(ui, "Forgotten your password? The person who runs this server can set a new one.");
            go |= house::actions(ui, &["Sign in"]).is_some();
        });
        if go && !self.busy && !self.username.is_empty() && !self.password.is_empty() {
            self.busy = true;
            self.error = None;
            call(&self.inbox, ctx, Method::Post, "api/login", Some(json!({ "username": self.username, "password": self.password })), |_| Reply::Reload);
        }
    }

    fn setup(&mut self, ctx: &egui::Context) {
        let mut go = false;
        house::dialog(ctx, "starsheets_setup", 400.0, "Welcome to StarSheets", None, |ui| {
            house::note(ui, "This server has no accounts yet. Make yours: you'll be its owner and can add other people afterwards.");
            house::note(ui, "The setup code is in the server's log (or is the STARSHEETS_PASSWORD it was started with).");
            house::gap(ui);
            house::label(ui, "Setup code", "");
            let r = Self::field(ui, &mut self.code, "From the server's log", false);
            self.first(&r);
            house::gap(ui);
            house::label(ui, "Your name", "What other people see next to your cursor.");
            Self::field(ui, &mut self.name, "Name", false);
            ui.add_space(house::S);
            house::label(ui, "Username and password", "For signing in. The username is letters and numbers.");
            Self::field(ui, &mut self.username, "Username", false);
            ui.add_space(house::S);
            Self::field(ui, &mut self.password, "Password (8 characters or more)", true);
            ui.add_space(house::S);
            let r = Self::field(ui, &mut self.password2, "Password again", true);
            go |= Self::enter(ui, &r);
            self.messages(ui);
            go |= house::actions(ui, &["Create account"]).is_some();
        });
        if go && !self.busy {
            if self.password != self.password2 {
                self.error = Some("The two passwords are different.".into());
                return;
            }
            self.busy = true;
            self.error = None;
            let body = json!({ "code": self.code.trim(), "name": self.name, "username": self.username, "password": self.password });
            call(&self.inbox, ctx, Method::Post, "api/setup", Some(body), |_| Reply::Reload);
        }
    }

    fn profile(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut cancel = false;
        let mut go = false;
        house::dialog(ctx, "starsheets_profile", 380.0, "Your name and colour", Some(&mut close), |ui| {
            house::label(ui, "Name", "Shown next to your cursor and on your avatar.");
            let r = Self::field(ui, &mut self.name, "Name", false);
            self.first(&r);
            go |= Self::enter(ui, &r);
            house::gap(ui);
            house::label(ui, "Colour", "");
            ui.horizontal_wrapped(|ui| {
                for c in COLOURS {
                    let picked = self.colour.eq_ignore_ascii_case(c);
                    let (rect, r) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                    ui.painter().circle_filled(rect.center(), 12.0, parse_colour(c));
                    if picked {
                        ui.painter().circle_stroke(rect.center(), 13.5, egui::Stroke::new(2.0, theme::TEXT));
                    }
                    if r.clicked() {
                        self.colour = c.to_string();
                    }
                }
            });
            self.messages(ui);
            match house::actions(ui, &["Cancel", "Save"]) {
                Some(0) => cancel = true,
                Some(_) => go = true,
                None => {}
            }
        });
        if close || cancel {
            self.dialog = Dialog::None;
        } else if go && !self.busy {
            self.busy = true;
            call(&self.inbox, ctx, Method::Post, "api/me", Some(json!({ "name": self.name, "colour": self.colour })), |v| match me_from(&v["user"]) {
                Some(me) => Reply::Saved(me),
                None => Reply::Error("Couldn't save.".into()),
            });
        }
    }

    fn change_password(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut cancel = false;
        let mut go = false;
        house::dialog(ctx, "starsheets_password", 380.0, "Change password", Some(&mut close), |ui| {
            let r = Self::field(ui, &mut self.current, "Current password", true);
            self.first(&r);
            ui.add_space(house::S);
            Self::field(ui, &mut self.password, "New password (8 characters or more)", true);
            ui.add_space(house::S);
            let r = Self::field(ui, &mut self.password2, "New password again", true);
            go |= Self::enter(ui, &r);
            house::note(ui, "You'll stay signed in here; other devices will need the new password.");
            self.messages(ui);
            match house::actions(ui, &["Cancel", "Change password"]) {
                Some(0) => cancel = true,
                Some(_) => go = true,
                None => {}
            }
        });
        if close || cancel {
            self.dialog = Dialog::None;
        } else if go && !self.busy {
            if self.password != self.password2 {
                self.error = Some("The two new passwords are different.".into());
                return;
            }
            self.busy = true;
            call(&self.inbox, ctx, Method::Post, "api/me", Some(json!({ "current_password": self.current, "password": self.password })), |v| match me_from(&v["user"]) {
                Some(me) => Reply::Saved(me),
                None => Reply::Error("Couldn't change the password.".into()),
            });
        }
    }

    fn people(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut cancel = false;
        let mut add = false;
        let mut remove = None;
        let me = me().unwrap_or_default();
        house::dialog(ctx, "starsheets_people", 460.0, "People", Some(&mut close), |ui| {
            house::note(ui, "Everyone with an account on this server. Sharing per file comes next; for now everyone can open every file.");
            house::gap(ui);
            for p in self.people.clone() {
                ui.horizontal(|ui| {
                    avatar(ui, &p.name, &p.colour, 28.0);
                    ui.add_space(house::S);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&p.name).size(house::BODY).color(theme::TEXT));
                        house::note(ui, &if p.owner { format!("{} · owner", p.username) } else { p.username.clone() });
                    });
                    if !p.owner && p.username != me.username {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let armed = self.remove_armed.as_deref() == Some(p.username.as_str());
                            if ui.add(house::link(if armed { "Click again to remove" } else { "Remove" })).clicked() {
                                if armed {
                                    remove = Some(p.username.clone());
                                } else {
                                    self.remove_armed = Some(p.username.clone());
                                }
                            }
                        });
                    }
                });
                ui.add_space(house::XS);
            }
            house::gap(ui);
            house::label(ui, "Add someone", "Give them their username and this password; they can change the password after signing in.");
            Self::field(ui, &mut self.name, "Their name", false);
            ui.add_space(house::S);
            Self::field(ui, &mut self.username, "Username", false);
            ui.add_space(house::S);
            Self::field(ui, &mut self.password, "Password for them (8 characters or more)", false);
            self.messages(ui);
            match house::actions(ui, &["Close", "Add"]) {
                Some(0) => cancel = true,
                Some(_) => add = true,
                None => {}
            }
        });
        if close || cancel {
            self.dialog = Dialog::None;
        } else if add && !self.busy {
            self.busy = true;
            let who = self.username.clone();
            call(&self.inbox, ctx, Method::Post, "api/users", Some(json!({ "name": self.name, "username": self.username, "password": self.password })), move |_| Reply::Done(format!("Added {who}.")));
        } else if let Some(who) = remove {
            self.remove_armed = None;
            let url = format!("api/users?username={}", crate::files::enc(&who));
            call(&self.inbox, ctx, Method::Delete, &url, None, move |_| Reply::Done(format!("Removed {who}.")));
        }
    }
}
