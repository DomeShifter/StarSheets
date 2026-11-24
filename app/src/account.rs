//! Accounts (live editing step 2): signing in with a username and password kept on the
//! StarSheets server, the first-time setup of the owner's account, your avatar menu (name and
//! colour, password, people, sign out), the owner's list of people, and sign-up (people making
//! their own account, when the owner allows it).
//! No sign-in through other companies (docs/decisions.md).

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Align2, Color32, RichText, Sense, Ui, vec2};
use serde_json::{Value as Json, json};
use wasm_bindgen::JsCast;

use crate::files::{Method, fetch};
use crate::house;
use crate::theme;

/// Colours people can pick (the same ones the server hands out).
const COLOURS: [&str; 8] = ["#D93025", "#188038", "#E37400", "#9334E6", "#007B83", "#E52592", "#B06000", "#5F6368"];

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
    let ask = |ui: &Ui, what: &str| ui.ctx().data_mut(|m| m.insert_temp(egui::Id::unique(REQUEST), what.to_string()));
    egui::Popup::menu(&r).show(|ui| {
        house::menu(ui, 260.0);
        house::menu_inset(ui, |ui| {
            ui.label(RichText::new(&me.name).size(house::BODY).strong().color(theme::TEXT));
            house::note(ui, &me.username);
        });
        house::menu_sep(ui);
        if house::menu_row(ui, Some("edit"), "Your name and colour…", "", true) {
            ask(ui, "profile");
        }
        if house::menu_row(ui, Some("lock"), "Change password…", "", true) {
            ask(ui, "password");
        }
        if me.owner && house::menu_row(ui, Some("folder"), "Where files are kept…", "", true) {
            ask(ui, "storage");
        }
        if me.owner && house::menu_row(ui, Some("group"), "People…", "", true) {
            ask(ui, "people");
        }
        house::menu_sep(ui);
        if house::menu_row(ui, Some(""), "Sign out", "", true) {
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
    Info { auth: bool, setup: bool, signup: bool, signup_code: bool, signed_in: bool, user: Option<Me> },
    Signup { on: bool, code: String },
    /// Signed in or out, or the password changed: start the page again.
    Reload,
    Saved(Me),
    People(Vec<Me>),
    Error(String),
    Done(String),
    /// Where files are kept now, and the folders that could be chosen (GET /api/storage).
    Storage(Json),
    /// The setup made the owner's account: now move to the folder picked, if any.
    SetUp,
    /// The server is restarting in its new folder: wait for it, then start the page again.
    Restarting,
}

#[derive(Default, PartialEq)]
enum Dialog {
    #[default]
    None,
    SignIn,
    SignUp,
    Setup,
    Profile,
    Password,
    People,
    Storage,
}

#[derive(Default)]
pub struct Account {
    inbox: Rc<RefCell<Vec<Reply>>>,
    booted: bool,
    auth: bool,
    /// The server lets people make their own account (and whether that needs a code).
    signup: bool,
    signup_code: bool,
    /// The owner's sign-up setting, as shown in People.
    signup_on: bool,
    signup_secret: String,
    dialog: Dialog,
    busy: bool,
    error: Option<String>,
    done: Option<String>,
    focused: bool,
    /// The HTML sign-in form (index.html) is showing.
    dom_sign_in: bool,
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
    /// Where files are kept (dialog and first-time setup): what the server said, the folder
    /// picked ("" = the top folder), whether to switch to StarSheets files already there, and
    /// when the restart began (waiting for the server to answer again).
    storage: Option<Json>,
    storage_pick: Option<String>,
    storage_use: bool,
    restarting: Option<f64>,
    polled: f64,
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

fn signup_reply(v: Json) -> Reply {
    Reply::Signup { on: v["on"].as_bool().unwrap_or(false), code: v["code"].as_str().unwrap_or("").to_string() }
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
                signup: v["signup"].as_bool().unwrap_or(false),
                signup_code: v["signup_code"].as_bool().unwrap_or(false),
                signed_in: v["signed_in"].as_bool().unwrap_or(true),
                user: me_from(&v["user"]),
            });
        }
        let replies: Vec<Reply> = std::mem::take(&mut *self.inbox.borrow_mut());
        for r in replies {
            self.busy = false;
            match r {
                Reply::Info { auth, setup, signup, signup_code, signed_in, user } => {
                    self.auth = auth;
                    self.signup = signup;
                    self.signup_code = signup_code;
                    ME.with(|m| *m.borrow_mut() = user);
                    if auth && !signed_in {
                        self.open(if setup { Dialog::Setup } else { Dialog::SignIn });
                        if setup {
                            self.load_storage(ctx);
                        }
                    }
                }
                Reply::Reload => reload(),
                Reply::Saved(me) => {
                    ME.with(|m| *m.borrow_mut() = Some(me));
                    self.dialog = Dialog::None;
                }
                Reply::People(p) => self.people = p,
                Reply::Storage(v) => {
                    self.storage_pick.get_or_insert_with(|| v["current"].as_str().unwrap_or("").to_string());
                    self.storage = Some(v);
                }
                Reply::SetUp => match self.storage_pick.clone().filter(|p| !p.is_empty()) {
                    Some(folder) => call(&self.inbox, ctx, Method::Post, "api/storage", Some(json!({ "folder": folder, "mode": "move" })), |v| if v["restarting"].as_bool().unwrap_or(false) { Reply::Restarting } else { Reply::Reload }),
                    None => reload(),
                },
                Reply::Restarting => {
                    self.dialog = Dialog::None;
                    self.restarting = Some(ctx.input(|i| i.time));
                }
                Reply::Signup { on, code } => {
                    self.signup_on = on;
                    self.signup_secret = code;
                }
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
        if let Some(req) = ctx.data_mut(|m| m.remove_temp::<String>(egui::Id::unique(REQUEST))) {
            match req.as_str() {
                "profile" => self.open(Dialog::Profile),
                "password" => self.open(Dialog::Password),
                "storage" => {
                    self.open(Dialog::Storage);
                    self.load_storage(ctx);
                }
                "people" => {
                    self.open(Dialog::People);
                    call(&self.inbox, ctx, Method::Get, "api/users", None, |v| Reply::People(v.as_array().into_iter().flatten().filter_map(me_from).collect()));
                    call(&self.inbox, ctx, Method::Get, "api/signup/settings", None, signup_reply);
                }
                "signout" => call(&self.inbox, ctx, Method::Post, "api/logout", None, |_| Reply::Reload),
                _ => {}
            }
        }
        if SIGN_IN.with(|s| std::mem::take(&mut *s.borrow_mut())) && !matches!(self.dialog, Dialog::Setup | Dialog::SignUp) {
            self.open(Dialog::SignIn);
        }
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
        self.sign_in_dom(ctx);
        match self.dialog {
            Dialog::None => {}
            Dialog::SignIn => {}
            Dialog::SignUp => self.sign_up(ctx),
            Dialog::Setup => self.setup(ctx),
            Dialog::Profile => self.profile(ctx),
            Dialog::Password => self.change_password(ctx),
            Dialog::People => self.people(ctx),
            Dialog::Storage => self.storage_ui(ctx),
        }
        self.restarting_ui(ctx);
    }

    fn load_storage(&mut self, ctx: &egui::Context) {
        self.storage = None;
        self.storage_pick = None;
        self.storage_use = false;
        call(&self.inbox, ctx, Method::Get, "api/storage", None, Reply::Storage);
    }

    /// Where files are kept (owner): the folder in use, and a list of the folders the server can
    /// reach to choose another. Files move there (or, if StarSheets files are already there,
    /// StarSheets can switch to those); the server then restarts.
    fn storage_ui(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut cancel = false;
        let mut go = false;
        let v = self.storage.clone();
        house::dialog(ctx, "starsheets_storage", 540.0, "Where files are kept", Some(&mut close), |ui| {
            let Some(v) = &v else {
                ui.label(RichText::new("Loading…").color(theme::TEXT_DIM));
                return;
            };
            let location = v["location"].as_str().unwrap_or("the server");
            let current = v["current"].as_str().unwrap_or("");
            let here = |p: &str| if p.is_empty() { location.to_string() } else { format!("{location} › {}", p.replace('/', " › ")) };
            house::text(ui, &format!("Now: {}", here(current)));
            house::note(ui, "Choose another folder below. These are the folders the server can reach; to offer others, give the server a bigger folder when it's installed.");
            house::gap(ui);
            let folders: Vec<String> = v["folders"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(str::to_string)).collect();
            let with_files: Vec<String> = v["has_files"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(str::to_string)).collect();
            let pick = self.storage_pick.clone().unwrap_or_default();
            egui::Frame::NONE.stroke(egui::Stroke::new(1.0, theme::DIVIDER)).corner_radius(8).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(280.0).min_scrolled_height(280.0).auto_shrink([false, false]).show(ui, |ui| {
                    for f in std::iter::once(String::new()).chain(folders.iter().cloned()) {
                        let depth = if f.is_empty() { 0 } else { f.matches('/').count() + 1 };
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                        let p = ui.painter().clone();
                        if pick == f {
                            p.rect_filled(r, 6, theme::SHARE);
                        } else if resp.hovered() {
                            p.rect_filled(r, 6, theme::MENU_HOVER);
                        }
                        let x = r.left() + 8.0 + depth as f32 * 18.0;
                        if let Some(img) = crate::icons::icon("folder") {
                            img.paint_at(ui, egui::Rect::from_center_size(egui::pos2(x + 10.0, r.center().y), vec2(18.0, 18.0)));
                        }
                        let mut label = if f.is_empty() { format!("{location} (the top)") } else { f.rsplit('/').next().unwrap_or(&f).to_string() };
                        if f == current {
                            label.push_str("  · in use");
                        } else if with_files.contains(&f) || (f.is_empty() && v["top_has_files"].as_bool().unwrap_or(false)) {
                            label.push_str("  · has StarSheets files");
                        }
                        p.text(egui::pos2(x + 28.0, r.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(14.0), theme::TEXT);
                        if resp.clicked() {
                            self.storage_pick = Some(f.clone());
                            self.storage_use = with_files.contains(&f) || (f.is_empty() && v["top_has_files"].as_bool().unwrap_or(false));
                        }
                    }
                });
            });
            let target_has = with_files.contains(&pick) || (pick.is_empty() && v["top_has_files"].as_bool().unwrap_or(false));
            if pick != current {
                house::gap(ui);
                if target_has {
                    ui.radio_value(&mut self.storage_use, true, "Use the StarSheets files already in that folder (accounts there too: you may need to sign in again)");
                    ui.radio_value(&mut self.storage_use, false, "Move my files there");
                } else {
                    house::note(ui, "Your spreadsheets, folders, versions, bin and accounts move there. Nobody can have a spreadsheet open while it happens; StarSheets restarts afterwards (a few seconds).");
                }
            }
            self.messages(ui);
            match house::actions(ui, &["Cancel", "Use this folder"]) {
                Some(0) => cancel = true,
                Some(_) => go = pick != current,
                None => {}
            }
        });
        if close || cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.dialog = Dialog::None;
        } else if go && !self.busy {
            self.busy = true;
            self.error = None;
            let body = json!({ "folder": self.storage_pick.clone().unwrap_or_default(), "mode": if self.storage_use { "use" } else { "move" } });
            call(&self.inbox, ctx, Method::Post, "api/storage", Some(body), |v| if v["restarting"].as_bool().unwrap_or(false) { Reply::Restarting } else { Reply::Reload });
        }
    }

    /// After a change of folder: "StarSheets is restarting…" until the server answers again
    /// (checked once a second), then the page starts again.
    fn restarting_ui(&mut self, ctx: &egui::Context) {
        let Some(since) = self.restarting else { return };
        let now = ctx.input(|i| i.time);
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
        if now - since > 1.5 && now - self.polled > 1.0 {
            self.polled = now;
            wasm_bindgen_futures::spawn_local(async {
                if matches!(fetch(Method::Get, "api/info", None, None).await, Ok((200, _, _))) {
                    reload();
                }
            });
        }
        house::dialog(ctx, "starsheets_restarting", 360.0, "Moving to the new folder", None, |ui| {
            house::text(ui, "StarSheets is restarting in its new folder. This page starts again by itself in a few seconds.");
        });
    }

    /// The sign-in box is a real HTML form (index.html) so password managers can fill it; the
    /// form signs in itself and reloads the page. Here: show it, hide it, and follow its
    /// "Create an account" link.
    fn sign_in_dom(&mut self, ctx: &egui::Context) {
        let want = self.dialog == Dialog::SignIn;
        let Some(w) = web_sys::window() else { return };
        if want != self.dom_sign_in {
            self.dom_sign_in = want;
            if let Some(f) = js_sys::Reflect::get(&w, &"__ssSignIn".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok()) {
                let _ = f.call2(&w, &want.into(), &self.signup.into());
            }
        }
        if want && js_sys::Reflect::get(&w, &"__ssWantSignUp".into()).is_ok_and(|v| v.is_truthy()) {
            let _ = js_sys::Reflect::set(&w, &"__ssWantSignUp".into(), &false.into());
            self.open(Dialog::SignUp);
            ctx.request_repaint();
        }
    }

    /// Making your own account (only offered when the owner allows sign-up).
    fn sign_up(&mut self, ctx: &egui::Context) {
        let mut go = false;
        let mut back = false;
        house::dialog(ctx, "starsheets_sign_up", 400.0, "Create an account", None, |ui| {
            house::label(ui, "Your name", "What other people see next to your cursor.");
            let r = Self::field(ui, &mut self.name, "Name", false);
            self.first(&r);
            ui.add_space(house::S);
            house::label(ui, "Username and password", "For signing in. The username is letters and numbers.");
            Self::field(ui, &mut self.username, "Username", false);
            ui.add_space(house::S);
            Self::field(ui, &mut self.password, "Password (8 characters or more)", true);
            ui.add_space(house::S);
            let r = Self::field(ui, &mut self.password2, "Password again", true);
            go |= Self::enter(ui, &r);
            if self.signup_code {
                house::gap(ui);
                house::label(ui, "Sign-up code", "The person who runs this server gives you this.");
                let r = Self::field(ui, &mut self.code, "Sign-up code", false);
                go |= Self::enter(ui, &r);
            }
            self.messages(ui);
            if ui.add(house::link("Already have an account? Sign in")).clicked() {
                back = true;
            }
            go |= house::actions(ui, &["Create account"]).is_some();
        });
        if back {
            self.open(Dialog::SignIn);
        } else if go && !self.busy {
            if self.password != self.password2 {
                self.error = Some("The two passwords are different.".into());
                return;
            }
            self.busy = true;
            self.error = None;
            let body = json!({ "code": self.code.trim(), "name": self.name, "username": self.username, "password": self.password });
            call(&self.inbox, ctx, Method::Post, "api/signup", Some(body), |_| Reply::Reload);
        }
    }

    fn setup(&mut self, ctx: &egui::Context) {
        let mut go = false;
        house::dialog(ctx, "starsheets_setup", 400.0, "Welcome to StarSheets", None, |ui| {
            house::note(ui, "This server has no accounts yet. Make yours: you'll be its owner and can add other people afterwards.");
            house::note(ui, "The setup code is printed in the server's log when it starts (with Docker: docker logs followed by the container's name). If the server was started with STARSHEETS_PASSWORD, that is the code.");
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
            // Where files go: the server's own folder, or a folder inside it.
            let folders: Vec<String> = self.storage.as_ref().map(|v| v["folders"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(str::to_string)).collect()).unwrap_or_default();
            if !folders.is_empty() {
                house::gap(ui);
                house::label(ui, "Where to keep files", "Your spreadsheets are saved here. You can change it later (your avatar › Where files are kept).");
                let mut labels = vec!["The top folder".to_string()];
                labels.extend(folders.iter().map(|f| f.replace('/', " › ")));
                let mut i = self.storage_pick.as_ref().and_then(|p| folders.iter().position(|f| f == p)).map_or(0, |i| i + 1);
                let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
                if house::select(ui, "starsheets_setup_folder", ui.available_width(), &mut i, &refs, &[0]) {
                    self.storage_pick = Some(if i == 0 { String::new() } else { folders[i - 1].clone() });
                }
            }
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
            call(&self.inbox, ctx, Method::Post, "api/setup", Some(body), |_| Reply::SetUp);
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
        let mut save_signup = false;
        let me = me().unwrap_or_default();
        house::dialog(ctx, "starsheets_people", 460.0, "People", Some(&mut close), |ui| {
            house::note(ui, "Everyone with an account on this server. Each person sees only their own files and the ones shared with them.");
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
            house::label(ui, "Sign-up", "Let people make their own account from the sign-in box. Only turn this on if strangers can't reach your server, or set a code.");
            let mut changed = ui.checkbox(&mut self.signup_on, "Let people make their own account").changed();
            if self.signup_on {
                ui.add_space(house::S);
                let r = Self::field(ui, &mut self.signup_secret, "Sign-up code (optional, you give it out)", false);
                changed |= r.lost_focus();
            }
            if changed {
                save_signup = true;
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
        } else if save_signup {
            let body = json!({ "on": self.signup_on, "code": self.signup_secret.trim() });
            let on = self.signup_on;
            call(&self.inbox, ctx, Method::Post, "api/signup/settings", Some(body), move |_| {
                Reply::Done(if on { "Sign-up is on.".into() } else { "Sign-up is off.".into() })
            });
        } else if let Some(who) = remove {
            self.remove_armed = None;
            let url = format!("api/users?username={}", crate::files::enc(&who));
            call(&self.inbox, ctx, Method::Delete, &url, None, move |_| Reply::Done(format!("Removed {who}.")));
        }
    }
}
