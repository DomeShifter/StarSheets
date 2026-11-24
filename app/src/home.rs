//! The home screen (StarSheets opened without a file), like Google Sheets' and Drive's: a folder
//! tree on the left (My files, Shared with me, Recent, Starred, Bin, and for the server's owner
//! Everyone's files), and the folders and spreadsheets of the chosen place on the right, as cards
//! with a preview picture or as a list. Everything is organised here: new spreadsheets and
//! folders, uploading (the New button, or dropping files on the page), opening, renaming, starring,
//! sharing (files and folders), copying, moving (drag onto a folder, the tree or the path; or cut
//! and paste) and the bin. Moving something into another person's folder makes it theirs, so the
//! app asks first.
//!
//! Server: crates/server (GET /api/files lists what you may see, with owner, role, stars and when
//! you opened each file; /api/files/ID/thumb is the preview picture).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use egui::{Align2, Color32, FontId, Key, Rect, RichText, Sense, Stroke, Ui, vec2};
use serde_json::Value as Json;

use crate::files::{Method, enc, fetch};
use crate::house;
use crate::theme;

const LEFT: f32 = 256.0;
const TOP: f32 = 64.0;
const CARD_W: f32 = 216.0;
const CARD_H: f32 = 196.0;
const PIC_H: f32 = 144.0;
const CHIP_H: f32 = 48.0;
const ROW: f32 = 40.0;
const GREEN: Color32 = house::GREEN;
/// Preview pictures fetched at once.
const THUMBS_AT_ONCE: usize = 4;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Item {
    File(String),
    Folder(String),
}

#[derive(Clone, Debug, Default)]
struct FileRow {
    id: String,
    name: String,
    folder: String,
    modified: i64,
    size: u64,
    version: String,
    owner: String,
    role: String,
    starred: bool,
    shared: bool,
    opened: Option<i64>,
    /// Who saved the last change (username), when known.
    changed_by: String,
}

#[derive(Clone, Debug, Default)]
struct FolderRow {
    path: String,
    owner: String,
    role: String,
    starred: bool,
    shared: bool,
    /// The colour you gave it (house::FOLDER_COLOURS), if any.
    colour: Option<Color32>,
}

#[derive(Clone, Debug)]
struct BinRow {
    name: String,
    original: String,
    deleted: i64,
    folder: bool,
}

#[derive(Clone, Debug, Default)]
struct Person {
    name: String,
}

/// Where the right-hand side is.
#[derive(Clone, PartialEq, Debug)]
enum View {
    /// A folder in my own space (the path includes the space).
    Mine(String),
    /// Shared with me: the tops (None) or inside a shared folder.
    Shared(Option<String>),
    Recent,
    Starred,
    Bin,
    /// The server owner's view of every folder ("" = everyone's folders).
    Everyone(String),
}

enum Reply {
    Listed(Json),
    Bin(Vec<BinRow>),
    Refresh,
    NewFolder(String),
    Thumb(String, String, Option<egui::ColorImage>),
    Download(String, Vec<u8>),
    /// Who has access to the item in Details.
    Access(Item, Json),
    Error(String),
}

enum Thumb {
    Loading,
    Ready(egui::TextureHandle),
    None,
}

/// The Move to… picker: what's being moved, the folder picked, and which tree rows are open.
struct MoveTo {
    items: Vec<Item>,
    target: Option<String>,
    expanded: HashSet<String>,
}

/// A move that hands things to someone else, waiting for "Move" in the question.
struct PendingMove {
    items: Vec<Item>,
    to: String,
    owner: String,
}

#[derive(Default)]
pub struct Home {
    /// Shown instead of a spreadsheet (the page was opened without ?file=, ?new or ?folder=).
    pub active: bool,
    booted: bool,
    inbox: Rc<RefCell<Vec<Reply>>>,
    loading: bool,
    files: Vec<FileRow>,
    folders: Vec<FolderRow>,
    bin: Vec<BinRow>,
    /// My own folder ("" without accounts), whether I'm the server's owner, the tops of "Shared
    /// with me", and everyone's names.
    space: String,
    admin: bool,
    accounts: bool,
    shared_folders: Vec<String>,
    shared_files: Vec<String>,
    people: HashMap<String, Person>,
    view: Option<View>,
    /// Where you were before (Back) and where Back came from (Forward).
    back: Vec<View>,
    forward: Vec<View>,
    expanded: HashSet<String>,
    list: bool,
    /// Sort: by (0 name, 1 date modified, 2 size), Z–A / newest / largest first, and whether
    /// folders are mixed in with files instead of kept at the top.
    sort_by: u8,
    descending: bool,
    mixed: bool,
    /// Details: the item shown, and who has access to it (from the server) once known.
    details: Option<(Item, Option<Json>)>,
    search: String,
    selected: Vec<Item>,
    anchor: Option<Item>,
    clipboard: Option<(Vec<Item>, bool)>,
    renaming: Option<(Item, String, bool)>,
    dragging: Option<Vec<Item>>,
    confirm_purge: Option<Option<String>>,
    pending_move: Option<PendingMove>,
    /// Move to…: the folder picker, open for these items.
    move_to: Option<MoveTo>,
    thumbs: HashMap<String, (String, Thumb)>,
    fetching: usize,
    error: Option<String>,
    /// A folder to open once the list arrives (?dir= in the address).
    open_dir: Option<String>,
}

fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(p, _)| p)
}

fn space_of(path: &str) -> &str {
    path.split('/').next().unwrap_or("")
}

fn under(path: &str, top: &str) -> bool {
    path == top || path.starts_with(&format!("{top}/"))
}

fn query(name: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&search).ok()?.get(name)
}

/// Whether this page should be the home screen: no file, new spreadsheet or folder asked for.
pub fn wanted() -> bool {
    query("file").is_none() && query("new").is_none() && query("folder").is_none()
}

fn page(path_and_query: &str) -> String {
    let path = web_sys::window().and_then(|w| w.location().pathname().ok()).unwrap_or_else(|| "/".into());
    format!("{path}{path_and_query}")
}

fn go(url: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.location().set_href(url);
    }
}

fn new_tab(url: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.open_with_url_and_target(url, "_blank");
    }
}

fn call(inbox: &Rc<RefCell<Vec<Reply>>>, ctx: &egui::Context, method: Method, url: String, body: Option<Vec<u8>>, then: impl FnOnce(Vec<u8>) -> Reply + 'static) {
    let (inbox, ctx) = (inbox.clone(), ctx.clone());
    wasm_bindgen_futures::spawn_local(async move {
        let reply = match fetch(method, &url, body, None).await {
            Ok((s, _, bytes)) if s < 400 => then(bytes),
            Ok((401, _, _)) => {
                crate::account::ask_sign_in();
                Reply::Error(String::new())
            }
            Ok((s, _, bytes)) => {
                let v: Json = serde_json::from_slice(&bytes).unwrap_or_default();
                Reply::Error(v["error"].as_str().map_or(format!("The server said {s}."), str::to_string))
            }
            Err(e) => Reply::Error(e),
        };
        inbox.borrow_mut().push(reply);
        ctx.request_repaint();
    });
}

fn refresh_after(_: Vec<u8>) -> Reply {
    Reply::Refresh
}

fn decode_png(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let px = &buf[..info.buffer_size()];
    let size = [info.width as usize, info.height as usize];
    match info.color_type {
        png::ColorType::Rgb => Some(egui::ColorImage::from_rgb(size, px)),
        png::ColorType::Rgba => Some(egui::ColorImage::from_rgba_unmultiplied(size, px)),
        _ => None,
    }
}

fn ago(unix: i64) -> String {
    crate::files::date_text(unix)
}

impl Home {
    pub fn new() -> Home {
        Home { active: wanted(), ..Default::default() }
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        self.loading = true;
        call(&self.inbox, ctx, Method::Get, "api/files".into(), None, |b| Reply::Listed(serde_json::from_slice(&b).unwrap_or_default()));
        if self.view == Some(View::Bin) {
            self.load_bin(ctx);
        }
    }

    fn load_bin(&mut self, ctx: &egui::Context) {
        call(&self.inbox, ctx, Method::Get, "api/bin".into(), None, |b| {
            let v: Json = serde_json::from_slice(&b).unwrap_or_default();
            Reply::Bin(
                v.as_array()
                    .into_iter()
                    .flatten()
                    .map(|x| BinRow {
                        name: x["name"].as_str().unwrap_or("").into(),
                        original: x["original"].as_str().unwrap_or("").into(),
                        deleted: x["deleted"].as_i64().unwrap_or(0),
                        folder: x["folder"].as_bool().unwrap_or(false),
                    })
                    .collect(),
            )
        });
    }

    fn take_list(&mut self, v: Json) {
        self.loading = false;
        let s = |x: &Json, k: &str| x[k].as_str().unwrap_or("").to_string();
        self.accounts = v.get("space").is_some();
        self.space = s(&v, "space");
        self.admin = v["admin"].as_bool().unwrap_or(!self.accounts);
        self.files = v["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| FileRow {
                id: s(f, "id"),
                name: s(f, "name"),
                folder: s(f, "folder"),
                modified: f["modified"].as_i64().unwrap_or(0),
                size: f["size"].as_u64().unwrap_or(0),
                version: s(f, "version"),
                owner: s(f, "owner"),
                role: f["role"].as_str().unwrap_or("owner").into(),
                starred: f["starred"].as_bool().unwrap_or(false),
                shared: f["shared"].as_bool().unwrap_or(false),
                opened: f["opened"].as_i64(),
                changed_by: s(f, "changed_by"),
            })
            .collect();
        let info = v["folder_info"].as_object().cloned().unwrap_or_default();
        self.folders = v["folders"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| f.as_str())
            .map(|p| {
                let i = info.get(p).cloned().unwrap_or_default();
                FolderRow {
                    path: p.into(),
                    owner: s(&i, "owner"),
                    role: i["role"].as_str().unwrap_or("owner").into(),
                    starred: i["starred"].as_bool().unwrap_or(false),
                    shared: i["shared"].as_bool().unwrap_or(false),
                    colour: house::folder_colour(i["colour"].as_str().unwrap_or("")),
                }
            })
            .collect();
        let list = |k: &str| v["shared"][k].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect::<Vec<_>>();
        self.shared_folders = list("folders");
        self.shared_files = list("files");
        self.people = v["people"].as_array().into_iter().flatten().map(|p| (s(p, "username"), Person { name: s(p, "name") })).collect();
        if self.view.is_none() {
            self.view = Some(View::Mine(self.space.clone()));
            self.expanded.insert(format!("mine:{}", self.space));
        }
        if let Some(dir) = self.open_dir.take() {
            self.go_folder(&dir);
        }
        // Forget pictures of files that went away.
        let ids: HashSet<&String> = self.files.iter().map(|f| &f.id).collect();
        self.thumbs.retain(|k, _| ids.contains(k));
    }

    fn person(&self, username: &str) -> String {
        if username == self.space && self.accounts {
            return "me".into();
        }
        self.people.get(username).map_or_else(|| username.to_string(), |p| p.name.clone())
    }

    fn folder(&self, path: &str) -> Option<&FolderRow> {
        self.folders.iter().find(|f| f.path == path)
    }

    fn file(&self, id: &str) -> Option<&FileRow> {
        self.files.iter().find(|f| f.id == id)
    }

    /// Goes somewhere, remembering where you were for Back.
    fn navigate(&mut self, v: View) {
        if self.view.as_ref() == Some(&v) {
            return;
        }
        if let Some(old) = self.view.take() {
            self.back.push(old);
        }
        self.forward.clear();
        self.view = Some(v);
        self.selected.clear();
        self.search.clear();
        self.renaming = None;
    }

    fn go_back(&mut self) {
        if let Some(v) = self.back.pop() {
            if let Some(old) = self.view.take() {
                self.forward.push(old);
            }
            self.view = Some(v);
            self.selected.clear();
        }
    }

    fn go_forward(&mut self) {
        if let Some(v) = self.forward.pop() {
            if let Some(old) = self.view.take() {
                self.back.push(old);
            }
            self.view = Some(v);
            self.selected.clear();
        }
    }

    /// The folder above this one, if there is one to go up to.
    fn up_view(&self) -> Option<View> {
        match self.view.clone()? {
            View::Mine(p) if p != self.space => Some(View::Mine(parent_of(&p).to_string())),
            View::Shared(Some(p)) if self.shared_folders.contains(&p) => Some(View::Shared(None)),
            View::Shared(Some(p)) => Some(View::Shared(Some(parent_of(&p).to_string()))),
            View::Everyone(p) if !p.is_empty() => Some(View::Everyone(parent_of(&p).to_string())),
            _ => None,
        }
    }

    fn go_up(&mut self) {
        if let Some(v) = self.up_view() {
            self.navigate(v);
        }
    }

    /// Shows a folder in the right place: mine, shared with me, or (the server's owner) anyone's.
    fn go_folder(&mut self, path: &str) {
        let v = if !self.accounts || under(path, &self.space) {
            View::Mine(path.to_string())
        } else if self.shared_folders.iter().any(|top| under(path, top)) {
            View::Shared(Some(path.to_string()))
        } else if self.admin {
            View::Everyone(path.to_string())
        } else {
            View::Shared(None)
        };
        self.navigate(v);
    }

    /// The folder new things go into from here (None: nowhere here, e.g. Recent).
    fn here(&self) -> Option<String> {
        match self.view.as_ref()? {
            View::Mine(p) => Some(p.clone()),
            View::Shared(Some(p)) => self.folder(p).filter(|f| f.role != "view").map(|_| p.clone()),
            View::Everyone(p) if !p.is_empty() => Some(p.clone()),
            _ => None,
        }
    }

    // ------------------------------------------------------------ what's shown

    /// The folders and files of the current place (or the search results).
    fn contents(&self) -> (Vec<FolderRow>, Vec<FileRow>) {
        let q = self.search.trim().to_lowercase();
        if !q.is_empty() {
            let folders = self.folders.iter().filter(|f| name_of(&f.path).to_lowercase().contains(&q) && f.path != self.space && f.path.contains('/')).cloned().collect();
            let files = self.files.iter().filter(|f| f.name.to_lowercase().contains(&q)).cloned().collect();
            return self.sorted(folders, files);
        }
        let (folders, files): (Vec<FolderRow>, Vec<FileRow>) = match self.view.clone() {
            Some(View::Mine(p) | View::Shared(Some(p)) | View::Everyone(p)) => (
                self.folders.iter().filter(|f| parent_of(&f.path) == p && !f.path.is_empty() && f.path != p).cloned().collect(),
                self.files.iter().filter(|f| f.folder == p).cloned().collect(),
            ),
            Some(View::Shared(None)) => (
                self.folders.iter().filter(|f| self.shared_folders.contains(&f.path)).cloned().collect(),
                self.files.iter().filter(|f| self.shared_files.contains(&f.id)).cloned().collect(),
            ),
            Some(View::Starred) => (self.folders.iter().filter(|f| f.starred).cloned().collect(), self.files.iter().filter(|f| f.starred).cloned().collect()),
            Some(View::Recent) => {
                let mut files: Vec<FileRow> = self.files.iter().filter(|f| f.opened.is_some()).cloned().collect();
                files.sort_by_key(|f| std::cmp::Reverse(f.opened));
                files.truncate(60);
                return (Vec::new(), files);
            }
            Some(View::Bin) | None => (Vec::new(), Vec::new()),
        };
        self.sorted(folders, files)
    }

    /// Newest change and total size of everything in a folder (for sorting folders by date/size).
    fn folder_stats(&self, path: &str) -> (i64, u64) {
        let inside = self.files.iter().filter(|f| under(&f.folder, path));
        inside.fold((0, 0), |(t, s), f| (t.max(f.modified), s + f.size))
    }

    /// What an item sorts by (name, date or size), as a comparable key.
    fn sort_key(&self, item: &Item) -> (i64, u64, String) {
        let (name, date, size) = match item {
            Item::Folder(p) => {
                let (d, s) = self.folder_stats(p);
                (self.folder_label(p), d, s)
            }
            Item::File(id) => self.file(id).map_or((String::new(), 0, 0), |f| (f.name.clone(), f.modified, f.size)),
        };
        match self.sort_by {
            1 => (date, 0, name.to_lowercase()),
            2 => (0, size, name.to_lowercase()),
            _ => (0, 0, name.to_lowercase()),
        }
    }

    fn sort_items(&self, items: &mut [Item]) {
        items.sort_by_cached_key(|i| self.sort_key(i));
        if self.descending {
            items.reverse();
        }
    }

    fn sorted(&self, folders: Vec<FolderRow>, files: Vec<FileRow>) -> (Vec<FolderRow>, Vec<FileRow>) {
        let mut fo: Vec<Item> = folders.iter().map(|f| Item::Folder(f.path.clone())).collect();
        let mut fi: Vec<Item> = files.iter().map(|f| Item::File(f.id.clone())).collect();
        self.sort_items(&mut fo);
        self.sort_items(&mut fi);
        let fo = fo.iter().filter_map(|i| if let Item::Folder(p) = i { folders.iter().find(|f| f.path == *p).cloned() } else { None }).collect();
        let fi = fi.iter().filter_map(|i| if let Item::File(id) = i { files.iter().find(|f| f.id == *id).cloned() } else { None }).collect();
        (fo, fi)
    }

    /// Everything shown, in the order shown: folders first (the default) or mixed in by the sort.
    fn shown(&self, folders: &[FolderRow], files: &[FileRow]) -> Vec<Item> {
        let mut items: Vec<Item> = folders.iter().map(|f| Item::Folder(f.path.clone())).chain(files.iter().map(|f| Item::File(f.id.clone()))).collect();
        if self.mixed && self.view != Some(View::Recent) {
            self.sort_items(&mut items);
        }
        items
    }

    fn save_sort(&self) {
        store("starsheets_home_sort", &format!("{}{}{}", self.sort_by, u8::from(self.descending), u8::from(self.mixed)));
    }

    /// The Sort menu: Sort by, Sort direction, Folders at top.
    fn sort_menu(&mut self, ui: &mut Ui) {
        let label = RichText::new("Sort").size(house::BODY).color(theme::TEXT);
        let b = match crate::icons::icon("sort") {
            Some(img) => egui::Button::image_and_text(img.fit_to_exact_size(vec2(20.0, 20.0)), label),
            None => egui::Button::new(label),
        };
        let r = ui.add(b.fill(Color32::WHITE).stroke(Stroke::new(1.0, theme::BOX)).corner_radius(16).min_size(vec2(88.0, 32.0)));
        egui::Popup::menu(&r).show(|ui| {
            house::menu(ui, 240.0);
            let tick = |on: bool| Some(if on { "check" } else { "" });
            house::menu_inset(ui, |ui| house::caps(ui, "Sort by"));
            for (i, l) in ["Name", "Date modified", "Size"].iter().enumerate() {
                if house::menu_row(ui, tick(self.sort_by == i as u8), l, "", true) {
                    // Dates and sizes read best newest / largest first.
                    if self.sort_by != i as u8 {
                        self.descending = i != 0;
                    }
                    self.sort_by = i as u8;
                    self.save_sort();
                }
            }
            house::menu_sep(ui);
            house::menu_inset(ui, |ui| house::caps(ui, "Sort direction"));
            let (up, down) = match self.sort_by {
                1 => ("Oldest first", "Newest first"),
                2 => ("Smallest first", "Largest first"),
                _ => ("A–Z", "Z–A"),
            };
            if house::menu_row(ui, tick(!self.descending), up, "", true) {
                self.descending = false;
                self.save_sort();
            }
            if house::menu_row(ui, tick(self.descending), down, "", true) {
                self.descending = true;
                self.save_sort();
            }
            house::menu_sep(ui);
            let box_icon = if self.mixed { "check_box_outline_blank" } else { "check_box" };
            if house::menu_row(ui, Some(box_icon), "Folders at top", "", true) {
                self.mixed = !self.mixed;
                self.save_sort();
            }
        });
    }

    fn title(&self) -> Vec<(String, Option<View>)> {
        type Parts = Vec<(String, Option<View>)>;
        let mut parts: Parts = Vec::new();
        fn walk(parts: &mut Parts, top: &str, label: String, path: &str, make: &dyn Fn(String) -> View) {
            parts.push((label, Some(make(top.to_string()))));
            let mut acc = top.to_string();
            for p in path.strip_prefix(top).unwrap_or("").split('/').filter(|p| !p.is_empty()) {
                acc = format!("{acc}/{p}");
                parts.push((p.to_string(), Some(make(acc.clone()))));
            }
        }
        match self.view.clone() {
            Some(View::Mine(p)) => walk(&mut parts, &self.space.clone(), if self.accounts { "My files".into() } else { "All files".into() }, &p, &View::Mine),
            Some(View::Shared(None)) => parts.push(("Shared with me".into(), Some(View::Shared(None)))),
            Some(View::Shared(Some(p))) => {
                parts.push(("Shared with me".into(), Some(View::Shared(None))));
                if let Some(top) = self.shared_folders.iter().find(|t| under(&p, t)).cloned() {
                    walk(&mut parts, &top, name_of(&top).to_string(), &p, &|x| View::Shared(Some(x)));
                }
            }
            Some(View::Everyone(p)) => {
                parts.push(("Everyone's files".into(), Some(View::Everyone(String::new()))));
                if !p.is_empty() {
                    let top = space_of(&p).to_string();
                    let label = self.person(&top);
                    walk(&mut parts, &top, if label == "me" { "My files".into() } else { label }, &p, &View::Everyone);
                }
            }
            Some(View::Recent) => parts.push(("Recent".into(), None)),
            Some(View::Starred) => parts.push(("Starred".into(), None)),
            Some(View::Bin) => parts.push(("Bin".into(), None)),
            None => {}
        }
        parts
    }

    // ------------------------------------------------------------ each frame

    pub fn logic(&mut self, ctx: &egui::Context) {
        if !self.booted {
            self.booted = true;
            self.list = stored("starsheets_home_list") == Some("1".into());
            if let Some(s) = stored("starsheets_home_sort").filter(|s| s.len() == 3) {
                let b = s.as_bytes();
                self.sort_by = (b[0] - b'0').min(2);
                self.descending = b[1] == b'1';
                self.mixed = b[2] == b'1';
            }
            self.open_dir = query("dir").filter(|d| !d.is_empty());
            self.refresh(ctx);
        }
        let replies: Vec<Reply> = std::mem::take(&mut *self.inbox.borrow_mut());
        for r in replies {
            match r {
                Reply::Listed(v) => self.take_list(v),
                Reply::Bin(b) => {
                    self.bin = b;
                    self.loading = false;
                }
                Reply::Refresh => self.refresh(ctx),
                Reply::NewFolder(path) => {
                    self.refresh(ctx);
                    self.selected = vec![Item::Folder(path.clone())];
                    self.renaming = Some((Item::Folder(path.clone()), name_of(&path).to_string(), true));
                }
                Reply::Thumb(id, version, img) => {
                    self.fetching = self.fetching.saturating_sub(1);
                    let t = match img {
                        Some(img) => Thumb::Ready(ctx.load_texture(format!("thumb-{id}"), img, egui::TextureOptions::LINEAR)),
                        None => Thumb::None,
                    };
                    self.thumbs.insert(id, (version, t));
                }
                Reply::Access(item, v) => {
                    if let Some((i, a)) = self.details.as_mut()
                        && *i == item
                    {
                        *a = Some(v);
                    }
                }
                Reply::Download(name, bytes) => {
                    if let Err(e) = crate::web::download_file(&name, &bytes) {
                        self.error = Some(e);
                    }
                }
                Reply::Error(e) => {
                    self.loading = false;
                    if !e.is_empty() {
                        self.error = Some(e);
                    }
                    self.refresh(ctx);
                }
            }
        }
    }

    /// Files dropped on the page or picked with Upload: saved into the folder shown.
    pub fn upload(&mut self, ctx: &egui::Context, name: String, bytes: Vec<u8>) {
        let folder = self.here().unwrap_or_else(|| self.space.clone());
        call(&self.inbox, ctx, Method::Post, format!("api/files?name={}&folder={}", enc(&name), enc(&folder)), Some(bytes), refresh_after);
    }

    fn want_thumb(&mut self, ctx: &egui::Context, f: &FileRow) {
        if self.thumbs.get(&f.id).is_some_and(|(v, _)| *v == f.version) || self.fetching >= THUMBS_AT_ONCE {
            return;
        }
        // Keep showing the old picture while the new one comes.
        if let Some((v, _)) = self.thumbs.get_mut(&f.id) {
            *v = f.version.clone();
        } else {
            self.thumbs.insert(f.id.clone(), (f.version.clone(), Thumb::Loading));
        }
        self.fetching += 1;
        let (id, version) = (f.id.clone(), f.version.clone());
        let (inbox, ctx) = (self.inbox.clone(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let img = match fetch(Method::Get, &format!("api/files/{id}/thumb"), None, None).await {
                Ok((200, _, bytes)) => decode_png(&bytes),
                _ => None,
            };
            inbox.borrow_mut().push(Reply::Thumb(id, version, img));
            ctx.request_repaint();
        });
    }

    // ------------------------------------------------------------ drawing

    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        ui.painter().rect_filled(ui.max_rect(), 0, theme::PAGE);
        egui::Panel::top("home_top").show_separator_line(false).exact_size(TOP).frame(egui::Frame::NONE.fill(theme::PAGE).inner_margin(egui::Margin::symmetric(16, 0))).show(ui, |ui| self.top_ui(ui));
        egui::Panel::left("home_left").show_separator_line(false).exact_size(LEFT).resizable(false).frame(egui::Frame::NONE.fill(theme::PAGE).inner_margin(egui::Margin { left: 12, right: 12, top: 8, bottom: 8 })).show(ui, |ui| self.left_ui(ui));
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::WHITE).corner_radius(egui::CornerRadius { nw: 16, ..Default::default() }).inner_margin(egui::Margin { left: 24, right: 16, top: 16, bottom: 0 })).show(ui, |ui| self.main_ui(ui));
        self.drag_ui(&ctx);
        self.move_to_ui(&ctx);
        self.details_ui(&ctx);
        self.pending_move_ui(&ctx);
        self.keys(&ctx);
    }

    fn top_ui(&mut self, ui: &mut Ui) {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            if let Some(img) = crate::icons::logo(36.0) {
                ui.add(img);
            }
            ui.add_space(house::S);
            ui.label(RichText::new("StarSheets").size(22.0).color(theme::TEXT));
            ui.add_space(LEFT - 190.0);
            let search = house::field(&mut self.search, house::SEARCH_TEXT)
                .id(egui::Id::unique("home_search"))
                .hint_text(RichText::new("Search in StarSheets").size(house::SEARCH_TEXT))
                .frame(egui::Frame::NONE)
                .margin(vec2(0.0, 0.0))
                .min_size(vec2(0.0, 24.0))
                .desired_width((ui.available_width() - 140.0).clamp(200.0, 700.0));
            let w = (ui.available_width() - 100.0).clamp(220.0, 740.0);
            ui.allocate_ui_with_layout(vec2(w, 48.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                // One rounded box: the field itself draws no frame of its own.
                egui::Frame::NONE.fill(theme::TOOLBAR).corner_radius(24).inner_margin(egui::Margin::symmetric(20, 12)).show(ui, |ui| {
                    if let Some(img) = crate::icons::icon("search") {
                        ui.add(img.fit_to_exact_size(vec2(20.0, 20.0)));
                    }
                    ui.add_space(house::S);
                    ui.add(search);
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                crate::account::me_ui(ui);
            });
        });
    }

    fn left_ui(&mut self, ui: &mut Ui) {
        // New: a spreadsheet, a folder, or files from this computer.
        let here = self.here();
        // The house's raised button: white, 16 px corners, a soft shadow, the + icon.
        let label = RichText::new("New").size(15.0).color(theme::TEXT);
        let button = match crate::icons::icon("add") {
            Some(img) => egui::Button::image_and_text(img.fit_to_exact_size(vec2(24.0, 24.0)), label),
            None => egui::Button::new(label),
        };
        let new = egui::Frame::NONE
            .corner_radius(16)
            .shadow(egui::epaint::Shadow { offset: [0, 1], blur: 6, spread: 0, color: Color32::from_black_alpha(50) })
            .show(ui, |ui| {
                ui.spacing_mut().button_padding = vec2(house::L, house::M);
                ui.add(button.fill(Color32::WHITE).corner_radius(16).min_size(vec2(112.0, 56.0)).stroke(Stroke::NONE))
            })
            .inner;
        egui::Popup::menu(&new).show(|ui| {
            house::menu(ui, 260.0);
            let folder = here.clone().unwrap_or_else(|| self.space.clone());
            if house::menu_row(ui, Some("note_add"), "New spreadsheet", "", true) {
                go(&page(&format!("?folder={}", enc(&folder))));
            }
            if house::menu_row(ui, Some("create_new_folder"), "New folder", "", true) {
                call(&self.inbox, ui.ctx(), Method::Post, format!("api/folders?path={}&name=New%20folder", enc(&folder)), None, |b| {
                    let v: Json = serde_json::from_slice(&b).unwrap_or_default();
                    Reply::NewFolder(v["path"].as_str().unwrap_or("").to_string())
                });
                if here.is_none() {
                    self.navigate(View::Mine(self.space.clone()));
                }
            }
            house::menu_sep(ui);
            if house::menu_row(ui, Some("upload"), "Upload files…", "", true) {
                let (inbox, ctx) = (self.inbox.clone(), ui.ctx().clone());
                wasm_bindgen_futures::spawn_local(async move {
                    let Some(picked) = rfd::AsyncFileDialog::new().add_filter("Spreadsheets", &["xlsx", "xlsm", "xls", "csv", "ods"]).pick_files().await else { return };
                    for f in picked {
                        let (name, bytes) = (f.file_name(), f.read().await);
                        let (inbox2, ctx2) = (inbox.clone(), ctx.clone());
                        let url = format!("api/files?name={}&folder={}", enc(&name), enc(&folder));
                        call(&inbox2, &ctx2, Method::Post, url, Some(bytes), refresh_after);
                    }
                });
            }
        });
        ui.add_space(house::M);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let space = self.space.clone();
            let mine_label = if self.accounts { "My files" } else { "All files" };
            self.node(ui, mine_label, "home_storage", Some(View::Mine(space.clone())), Some(&space), "mine");
            if self.accounts {
                self.node(ui, "Shared with me", "folder_shared", Some(View::Shared(None)), None, "shared");
                if self.expanded.contains("shared:") {
                    for top in self.shared_folders.clone() {
                        self.tree(ui, &top, 1, "shared", &|p| View::Shared(Some(p)));
                    }
                }
            }
            self.node(ui, "Recent", "schedule", Some(View::Recent), None, "recent");
            self.node(ui, "Starred", "star", Some(View::Starred), None, "starred");
            self.node(ui, "Bin", "delete", Some(View::Bin), None, "bin");
            if self.accounts && self.admin {
                ui.add_space(house::S);
                ui.separator();
                self.node(ui, "Everyone's files", "groups", Some(View::Everyone(String::new())), Some(""), "every");
            }
        });
    }

    /// A row of the tree. `folder`: the folder it stands for (it can be opened up and dropped on).
    fn node(&mut self, ui: &mut Ui, label: &str, icon: &str, view: Option<View>, folder: Option<&str>, kind: &str) {
        let key = format!("{kind}:{}", folder.unwrap_or(""));
        let has_children = match (kind, folder) {
            ("shared", _) => !self.shared_folders.is_empty(),
            ("every", _) => self.folders.iter().any(|f| !f.path.contains('/')),
            (_, Some(f)) => self.folders.iter().any(|x| parent_of(&x.path) == f && x.path != f),
            _ => false,
        };
        let current = view.is_some() && self.view == view;
        if self.row(ui, label, icon, None, 0, current, has_children, &key, folder.map(str::to_string)) {
            if let Some(v) = view {
                self.navigate(v);
            }
            if self.view == Some(View::Bin) {
                self.loading = true;
                self.load_bin(ui.ctx());
            }
        }
        if self.expanded.contains(&key) {
            match (kind, folder) {
                ("every", _) => {
                    let mut tops: Vec<String> = self.folders.iter().filter(|f| !f.path.contains('/') && !f.path.is_empty()).map(|f| f.path.clone()).collect();
                    tops.sort();
                    for t in tops {
                        self.tree(ui, &t, 1, "every", &View::Everyone);
                    }
                }
                (_, Some(f)) if kind == "mine" => {
                    for c in self.children(f) {
                        self.tree(ui, &c, 1, "mine", &View::Mine);
                    }
                }
                _ => {}
            }
        }
    }

    fn children(&self, path: &str) -> Vec<String> {
        let mut c: Vec<String> = self.folders.iter().filter(|f| parent_of(&f.path) == path && f.path != path && !f.path.is_empty()).map(|f| f.path.clone()).collect();
        c.sort_by_key(|p| name_of(p).to_lowercase());
        c
    }

    fn tree(&mut self, ui: &mut Ui, path: &str, depth: usize, kind: &str, make: &dyn Fn(String) -> View) {
        let key = format!("{kind}:{path}");
        let view = make(path.to_string());
        let kids = self.children(path);
        let label = if kind == "every" && depth == 1 {
            let p = self.person(path);
            if p == "me" { "My files".to_string() } else { p }
        } else {
            name_of(path).to_string()
        };
        let current = self.view.as_ref() == Some(&view);
        let colour = self.folder(path).and_then(|f| f.colour);
        let icon = if colour.is_some() { "folder_fill" } else { "folder" };
        if self.row(ui, &label, icon, colour, depth, current, !kids.is_empty(), &key, Some(path.to_string())) {
            self.navigate(view);
        }
        if self.expanded.contains(&key) {
            for k in kids {
                self.tree(ui, &k, depth + 1, kind, make);
            }
        }
    }

    /// Draws one tree row; returns true when it was clicked (not on its arrow).
    #[allow(clippy::too_many_arguments)]
    fn row(&mut self, ui: &mut Ui, label: &str, icon: &str, tint: Option<Color32>, depth: usize, current: bool, has_children: bool, key: &str, folder: Option<String>) -> bool {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
        let p = ui.painter().clone();
        let dropping = self.dragging.is_some() && folder.is_some() && resp.contains_pointer();
        if current {
            p.rect_filled(r, 17, theme::SHARE);
        } else if dropping || resp.hovered() {
            p.rect_filled(r, 17, theme::HOVER);
        }
        if dropping {
            p.rect_stroke(r, 17, Stroke::new(2.0, GREEN), egui::StrokeKind::Inside);
            ui.ctx().data_mut(|m| m.insert_temp(egui::Id::unique("home_drop"), folder.clone().unwrap_or_default()));
        }
        let x = r.left() + 4.0 + depth as f32 * 16.0;
        let arrow = Rect::from_center_size(egui::pos2(x + 10.0, r.center().y), vec2(20.0, 20.0));
        let mut toggled = false;
        if has_children {
            let open = self.expanded.contains(key);
            if let Some(img) = crate::icons::icon(if open { "expand_more" } else { "chevron_right" }) {
                img.paint_at(ui, arrow);
            }
            if ui.input(|i| i.pointer.primary_clicked() && i.pointer.interact_pos().is_some_and(|pos| arrow.expand(4.0).contains(pos))) {
                toggled = true;
                if open {
                    self.expanded.remove(key);
                } else {
                    self.expanded.insert(key.to_string());
                }
            }
        }
        if let Some(img) = crate::icons::icon(icon) {
            let img = match tint {
                Some(c) => img.tint(c),
                None => img,
            };
            img.paint_at(ui, Rect::from_center_size(egui::pos2(x + 34.0, r.center().y), vec2(20.0, 20.0)));
        }
        let font = FontId::proportional(14.0);
        let galley = p.layout(label.to_string(), font, theme::TEXT, r.right() - x - 56.0);
        p.galley(egui::pos2(x + 52.0, r.center().y - galley.size().y / 2.0), galley, theme::TEXT);
        resp.clicked() && !toggled
    }

    fn main_ui(&mut self, ui: &mut Ui) {
        // Path (each part clickable and a drop target), view switch.
        // Anywhere empty in this area: right-click for New (items drawn later sit on top of it).
        let bg = ui.interact(ui.max_rect(), egui::Id::unique("home_bg"), Sense::click());
        let crumbs = self.title();
        ui.horizontal(|ui| {
            // Back, forward and up (the mouse's back button goes up too).
            let nav = |ui: &mut Ui, icon: &str, enabled: bool, tip: &str| {
                let b = match crate::icons::icon(icon) {
                    Some(img) => egui::Button::image(img.fit_to_exact_size(vec2(22.0, 22.0))),
                    None => egui::Button::new(tip),
                };
                ui.add_enabled(enabled, b.frame(false).min_size(vec2(36.0, 36.0)).corner_radius(18)).on_hover_text(tip).on_disabled_hover_text(tip).clicked()
            };
            if nav(ui, "arrow_back", !self.back.is_empty(), "Back (your mouse's back button)") {
                self.go_back();
            }
            if nav(ui, "arrow_forward", !self.forward.is_empty(), "Forward") {
                self.go_forward();
            }
            if nav(ui, "arrow_upward", self.up_view().is_some(), "Up one folder (Backspace)") {
                self.go_up();
            }
            ui.add_space(house::S);
            let last = crumbs.len().saturating_sub(1);
            for (i, (label, view)) in crumbs.into_iter().enumerate() {
                if i > 0 {
                    ui.label(RichText::new("›").size(20.0).color(theme::TEXT_DIM));
                }
                let r = ui.add(egui::Button::new(RichText::new(&label).size(22.0).color(if i == last { theme::TEXT } else { theme::TEXT_DIM })).frame(false));
                let folder = match &view {
                    Some(View::Mine(p) | View::Everyone(p)) if !p.is_empty() => Some(p.clone()),
                    Some(View::Shared(Some(p))) => Some(p.clone()),
                    _ => None,
                };
                if self.dragging.is_some()
                    && r.contains_pointer()
                    && let Some(f) = folder
                {
                    ui.painter().rect_stroke(r.rect, 6, Stroke::new(2.0, GREEN), egui::StrokeKind::Outside);
                    ui.ctx().data_mut(|m| m.insert_temp(egui::Id::unique("home_drop"), f));
                }
                if r.clicked()
                    && let Some(v) = view
                {
                    self.navigate(v);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let toggle = |ui: &mut Ui, icon: &str, on: bool, tip: &str| {
                    let b = match crate::icons::icon(icon) {
                        Some(img) => egui::Button::image(img.fit_to_exact_size(vec2(20.0, 20.0))),
                        None => egui::Button::new(tip),
                    };
                    ui.add(b.fill(if on { theme::SHARE } else { Color32::WHITE }).corner_radius(16).min_size(vec2(44.0, 32.0))).on_hover_text(tip).clicked()
                };
                if toggle(ui, "grid_view", !self.list, "Grid") {
                    self.list = false;
                    store("starsheets_home_list", "0");
                }
                if toggle(ui, "view_list", self.list, "List") {
                    self.list = true;
                    store("starsheets_home_list", "1");
                }
                ui.add_space(house::S);
                self.sort_menu(ui);
            });
        });
        if let Some(e) = self.error.clone() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(e).size(house::NOTE).color(house::ERROR));
                if ui.add(house::link("OK")).clicked() {
                    self.error = None;
                }
            });
        }
        ui.add_space(house::M);
        if self.view == Some(View::Bin) && self.search.trim().is_empty() {
            self.bin_ui(ui);
            return;
        }
        let (folders, files) = self.contents();
        let ctx = ui.ctx().clone();
        let mut clicked_item = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if folders.is_empty() && files.is_empty() {
                let empty = if self.loading {
                    "Loading…"
                } else if !self.search.trim().is_empty() {
                    "Nothing matches."
                } else {
                    match self.view {
                        Some(View::Shared(None)) => "Nothing has been shared with you yet. When someone shares a file or folder with you, it shows here.",
                        Some(View::Starred) => "Nothing starred yet. Right-click a file or folder and choose Star to keep it here.",
                        Some(View::Recent) => "Files you open show here.",
                        _ => "This folder is empty. Use New, or drop spreadsheets from your computer here.",
                    }
                };
                ui.add_space(house::XL);
                ui.label(RichText::new(empty).size(house::BODY).color(theme::TEXT_DIM));
            }
            let order = self.shown(&folders, &files);
            if self.list {
                self.list_ui(ui, &order, &mut clicked_item);
            } else if self.mixed && self.view != Some(View::Recent) {
                // Folders mixed in with files: everything as cards, in the sort's order.
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(house::M, house::M);
                    for item in &order {
                        match item {
                            Item::Folder(p) => {
                                if let Some(f) = folders.iter().find(|f| f.path == *p).cloned() {
                                    self.folder_card(ui, &f, &mut clicked_item);
                                }
                            }
                            Item::File(id) => {
                                if let Some(f) = files.iter().find(|f| f.id == *id).cloned() {
                                    self.file_card(ui, &ctx, &f, &mut clicked_item);
                                }
                            }
                        }
                    }
                });
            } else {
                if !folders.is_empty() {
                    house::caps(ui, "Folders");
                    ui.add_space(house::S);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(house::M, house::M);
                        for f in &folders {
                            self.folder_chip(ui, f, &mut clicked_item);
                        }
                    });
                    ui.add_space(house::XL);
                }
                if !files.is_empty() {
                    house::caps(ui, "Files");
                    ui.add_space(house::S);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(house::M, house::M);
                        for f in &files {
                            self.file_card(ui, &ctx, f, &mut clicked_item);
                        }
                    });
                }
            }
        });
        // Empty space: a click clears the selection; a right-click offers New and Paste (into the
        // folder shown, or your own folder where nothing can be made, e.g. Recent).
        if bg.clicked() && !clicked_item {
            self.selected.clear();
        }
        if !clicked_item {
            bg.context_menu(|ui| {
                house::menu(ui, 260.0);
                let here = self.here();
                let into = here.clone().unwrap_or_else(|| self.space.clone());
                if house::menu_row(ui, Some("note_add"), "New spreadsheet", "", true) {
                    go(&page(&format!("?folder={}", enc(&into))));
                }
                if house::menu_row(ui, Some("create_new_folder"), "New folder", "", true) {
                    if here.is_none() {
                        self.navigate(View::Mine(self.space.clone()));
                    }
                    call(&self.inbox, ui.ctx(), Method::Post, format!("api/folders?path={}&name=New%20folder", enc(&into)), None, |b| {
                        let v: Json = serde_json::from_slice(&b).unwrap_or_default();
                        Reply::NewFolder(v["path"].as_str().unwrap_or("").to_string())
                    });
                    ui.close();
                }
                house::menu_sep(ui);
                if house::menu_row(ui, Some("content_paste"), "Paste", "Ctrl+V", self.clipboard.is_some() && here.is_some()) {
                    self.paste_into(ui.ctx(), into.clone());
                    ui.close();
                }
            });
        }
    }

    fn select_click(&mut self, ui: &Ui, item: &Item, resp: &egui::Response, order: &[Item]) {
        if !(resp.clicked() || resp.secondary_clicked()) {
            return;
        }
        let selected = self.selected.contains(item);
        let m = ui.input(|i| i.modifiers);
        if resp.secondary_clicked() && selected {
        } else if m.command {
            if selected {
                self.selected.retain(|x| x != item);
            } else {
                self.selected.push(item.clone());
            }
            self.anchor = Some(item.clone());
        } else if m.shift
            && let (Some(a), Some(b)) = (self.anchor.as_ref().and_then(|a| order.iter().position(|x| x == a)), order.iter().position(|x| x == item))
        {
            let (lo, hi) = (a.min(b), a.max(b));
            self.selected = order[lo..=hi].to_vec();
        } else {
            self.selected = vec![item.clone()];
            self.anchor = Some(item.clone());
        }
    }

    fn order(&self) -> Vec<Item> {
        let (folders, files) = self.contents();
        self.shown(&folders, &files)
    }

    /// Common handling of a folder or file shown anywhere: select, open, drag, menu, drop.
    fn interact(&mut self, ui: &mut Ui, item: &Item, resp: &egui::Response, clicked_item: &mut bool) {
        if resp.clicked() || resp.secondary_clicked() || resp.double_clicked() {
            *clicked_item = true;
        }
        let order = self.order();
        self.select_click(ui, item, resp, &order);
        if resp.double_clicked() {
            self.activate(item);
        }
        if resp.drag_started() {
            let set = if self.selected.contains(item) { self.selected.clone() } else { vec![item.clone()] };
            self.selected = set.clone();
            self.dragging = Some(set);
        }
        if let Item::Folder(f) = item
            && self.dragging.as_ref().is_some_and(|d| !d.contains(item))
            && resp.contains_pointer()
        {
            ui.painter().rect_stroke(resp.rect, 8, Stroke::new(2.0, GREEN), egui::StrokeKind::Inside);
            ui.ctx().data_mut(|m| m.insert_temp(egui::Id::unique("home_drop"), f.clone()));
        }
        resp.context_menu(|ui| {
            *clicked_item = true;
            self.item_menu(ui, item);
        });
    }

    fn folder_chip(&mut self, ui: &mut Ui, f: &FolderRow, clicked_item: &mut bool) {
        let item = Item::Folder(f.path.clone());
        let (r, resp) = ui.allocate_exact_size(vec2(CARD_W, CHIP_H), Sense::click_and_drag());
        let sel = self.selected.contains(&item);
        let p = ui.painter().clone();
        p.rect_filled(r, 12, if sel { theme::SHARE } else if resp.hovered() { theme::HOVER } else { theme::TOOLBAR });
        folder_icon(ui, f, Rect::from_center_size(r.left_center() + vec2(24.0, 0.0), vec2(22.0, 22.0)));
        if f.shared
            && f.colour.is_some()
            && let Some(img) = crate::icons::icon("group")
        {
            img.paint_at(ui, Rect::from_center_size(r.right_center() - vec2(if f.starred { 40.0 } else { 18.0 }, 0.0), vec2(16.0, 16.0)));
        }
        if !self.renaming_ui(ui, &item, Rect::from_min_max(r.min + vec2(44.0, 8.0), r.max - vec2(8.0, 8.0))) {
            let galley = p.layout(self.folder_label(&f.path), FontId::proportional(14.0), theme::TEXT, CARD_W - 76.0);
            p.galley(egui::pos2(r.left() + 44.0, r.center().y - galley.size().y.min(20.0) / 2.0), galley, theme::TEXT);
        }
        if f.starred
            && let Some(img) = crate::icons::icon("star_fill")
        {
            img.paint_at(ui, Rect::from_center_size(r.right_center() - vec2(18.0, 0.0), vec2(16.0, 16.0)));
        }
        let hover = if self.accounts && f.owner != self.space { format!("{} · {}", name_of(&f.path), self.person(&f.owner)) } else { name_of(&f.path).to_string() };
        let resp = resp.on_hover_text(hover);
        self.interact(ui, &item, &resp, clicked_item);
    }

    /// A folder the size of a file card (when folders are mixed in with files).
    fn folder_card(&mut self, ui: &mut Ui, f: &FolderRow, clicked_item: &mut bool) {
        let item = Item::Folder(f.path.clone());
        let (r, resp) = ui.allocate_exact_size(vec2(CARD_W, CARD_H), Sense::click_and_drag());
        let sel = self.selected.contains(&item);
        let p = ui.painter().clone();
        p.rect_filled(r, 12, if sel { theme::SHARE } else if resp.hovered() { theme::HOVER } else { theme::TOOLBAR });
        let pic = Rect::from_min_size(r.min + vec2(8.0, 8.0), vec2(CARD_W - 16.0, PIC_H - 8.0));
        p.rect_filled(pic, 6, Color32::WHITE);
        folder_icon(ui, f, Rect::from_center_size(pic.center(), vec2(64.0, 64.0)));
        let line = Rect::from_min_max(egui::pos2(r.left() + 8.0, r.top() + PIC_H), r.max - vec2(8.0, 4.0));
        folder_icon(ui, f, Rect::from_center_size(egui::pos2(line.left() + 10.0, line.top() + 16.0), vec2(18.0, 18.0)));
        if !self.renaming_ui(ui, &item, Rect::from_min_size(egui::pos2(line.left() + 26.0, line.top() + 4.0), vec2(line.width() - 30.0, 26.0))) {
            let name = p.layout_no_wrap(self.folder_label(&f.path), FontId::proportional(14.0), theme::TEXT);
            let clip = Rect::from_min_size(egui::pos2(line.left() + 28.0, line.top() + 6.0), vec2(line.width() - 52.0, 20.0));
            p.with_clip_rect(clip).galley(clip.min, name, theme::TEXT);
        }
        let (newest, _) = self.folder_stats(&f.path);
        let sub = if newest > 0 { format!("Folder · {}", ago(newest)) } else { "Folder".into() };
        p.text(egui::pos2(line.left() + 28.0, line.top() + 36.0), Align2::LEFT_CENTER, sub, FontId::proportional(12.0), theme::TEXT_DIM);
        let resp = resp.on_hover_text(self.folder_label(&f.path));
        self.interact(ui, &item, &resp, clicked_item);
    }

    fn file_card(&mut self, ui: &mut Ui, ctx: &egui::Context, f: &FileRow, clicked_item: &mut bool) {
        let item = Item::File(f.id.clone());
        let (r, resp) = ui.allocate_exact_size(vec2(CARD_W, CARD_H), Sense::click_and_drag());
        if ui.is_rect_visible(r) {
            self.want_thumb(ctx, f);
        }
        let sel = self.selected.contains(&item);
        let p = ui.painter().clone();
        p.rect_filled(r, 12, if sel { theme::SHARE } else if resp.hovered() { theme::HOVER } else { theme::TOOLBAR });
        // The picture, on white, in the top part.
        let pic = Rect::from_min_size(r.min + vec2(8.0, 8.0), vec2(CARD_W - 16.0, PIC_H - 8.0));
        p.rect_filled(pic, 6, Color32::WHITE);
        match self.thumbs.get(&f.id).map(|(_, t)| t) {
            Some(Thumb::Ready(tex)) => {
                // The picture's full width (columns A to L, as Drive shows), cut off at the bottom.
                let tw = tex.size_vec2();
                let scale = pic.width() / tw.x;
                let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2((pic.width() / (tw.x * scale)).min(1.0), (pic.height() / (tw.y * scale)).min(1.0)));
                p.image(tex.id(), pic, uv, Color32::WHITE);
            }
            _ => {
                if let Some(img) = crate::icons::icon("starsheets") {
                    img.paint_at(ui, Rect::from_center_size(pic.center(), vec2(48.0, 48.0)));
                }
            }
        }
        p.rect_stroke(pic, 6, Stroke::new(1.0, theme::DIVIDER), egui::StrokeKind::Inside);
        let line = Rect::from_min_max(egui::pos2(r.left() + 8.0, r.top() + PIC_H), r.max - vec2(8.0, 4.0));
        if let Some(img) = crate::icons::icon("starsheets") {
            img.paint_at(ui, Rect::from_center_size(egui::pos2(line.left() + 10.0, line.top() + 16.0), vec2(18.0, 18.0)));
        }
        if !self.renaming_ui(ui, &item, Rect::from_min_size(egui::pos2(line.left() + 26.0, line.top() + 4.0), vec2(line.width() - 30.0, 26.0))) {
            let name = p.layout_no_wrap(f.name.clone(), FontId::proportional(14.0), theme::TEXT);
            let clip = Rect::from_min_size(egui::pos2(line.left() + 28.0, line.top() + 6.0), vec2(line.width() - 52.0, 20.0));
            p.with_clip_rect(clip).galley(clip.min, name, theme::TEXT);
        }
        if f.starred
            && let Some(img) = crate::icons::icon("star_fill")
        {
            img.paint_at(ui, Rect::from_center_size(egui::pos2(line.right() - 10.0, line.top() + 16.0), vec2(16.0, 16.0)));
        }
        let sub = match (&self.view, f.opened) {
            (Some(View::Recent), Some(t)) => format!("You opened {}", ago(t)),
            _ if self.accounts && f.owner != self.space => format!("{} · {}", self.person(&f.owner), ago(f.modified)),
            _ => format!("Modified {}", ago(f.modified)),
        };
        p.text(egui::pos2(line.left() + 28.0, line.top() + 36.0), Align2::LEFT_CENTER, sub, FontId::proportional(12.0), theme::TEXT_DIM);
        if f.shared
            && let Some(img) = crate::icons::icon("group")
        {
            img.paint_at(ui, Rect::from_center_size(egui::pos2(line.right() - 10.0, line.top() + 36.0), vec2(14.0, 14.0)));
        }
        let resp = resp.on_hover_text(&f.name);
        self.interact(ui, &item, &resp, clicked_item);
    }

    fn list_ui(&mut self, ui: &mut Ui, order: &[Item], clicked_item: &mut bool) {
        let w = ui.available_width() - 8.0;
        let (h, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
        let cols = [(0.0, "Name"), (w * 0.52, if self.accounts { "Owner" } else { "" }), (w * 0.70, "Last modified"), (w - 90.0, "Size")];
        for (x, label) in cols {
            ui.painter().text(h.left_center() + vec2(x + if x == 0.0 { 44.0 } else { 0.0 }, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(13.0), theme::TEXT_DIM);
        }
        ui.painter().hline(h.x_range(), h.bottom(), Stroke::new(1.0, theme::DIVIDER));
        let rows: Vec<ListRow> = order
            .iter()
            .filter_map(|item| match item {
                Item::Folder(p) => self.folder(p).map(|f| {
                    let (newest, size) = self.folder_stats(&f.path);
                    ListRow { item: item.clone(), name: self.folder_label(&f.path), icon: if f.shared { "folder_shared" } else { "folder" }, owner: f.owner.clone(), modified: newest, size: Some(size), starred: f.starred }
                }),
                Item::File(id) => self.file(id).map(|f| ListRow { item: item.clone(), name: f.name.clone(), icon: "starsheets", owner: f.owner.clone(), modified: f.modified, size: Some(f.size), starred: f.starred }),
            })
            .collect();
        for ListRow { item, name, icon, owner, modified, size, starred } in rows {
            let (r, resp) = ui.allocate_exact_size(vec2(w, ROW), Sense::click_and_drag());
            let p = ui.painter().clone();
            if self.selected.contains(&item) {
                p.rect_filled(r, 6, house::SELECTED);
            } else if resp.hovered() {
                p.rect_filled(r, 6, theme::MENU_HOVER);
            }
            let icon_rect = Rect::from_center_size(r.left_center() + vec2(20.0, 0.0), vec2(20.0, 20.0));
            match &item {
                Item::Folder(p) if let Some(f) = self.folder(p).cloned() => folder_icon(ui, &f, icon_rect),
                _ => {
                    if let Some(img) = crate::icons::icon(icon) {
                        img.paint_at(ui, icon_rect);
                    }
                }
            }
            if !self.renaming_ui(ui, &item, Rect::from_min_size(r.min + vec2(40.0, 5.0), vec2(w * 0.5 - 48.0, ROW - 10.0))) {
                let mut shown = name.clone();
                if starred {
                    shown.push_str("  ★");
                }
                p.text(r.left_center() + vec2(44.0, 0.0), Align2::LEFT_CENTER, shown, FontId::proportional(14.0), theme::TEXT);
            }
            if self.accounts {
                p.text(r.left_center() + vec2(w * 0.52, 0.0), Align2::LEFT_CENTER, self.person(&owner), FontId::proportional(13.0), theme::TEXT_DIM);
            }
            if modified > 0 {
                p.text(r.left_center() + vec2(w * 0.70, 0.0), Align2::LEFT_CENTER, ago(modified), FontId::proportional(13.0), theme::TEXT_DIM);
            }
            if let Some(s) = size {
                p.text(r.left_center() + vec2(w - 90.0, 0.0), Align2::LEFT_CENTER, size_text(s), FontId::proportional(13.0), theme::TEXT_DIM);
            }
            self.interact(ui, &item, &resp, clicked_item);
        }
    }

    /// The rename box over `rect` if `item` is being renamed; true when it is.
    fn renaming_ui(&mut self, ui: &mut Ui, item: &Item, rect: Rect) -> bool {
        let Some((ritem, text, focus)) = self.renaming.as_mut().filter(|(r, ..)| r == item) else { return false };
        let te = ui
            .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                house::form_style(ui);
                ui.add_sized(rect.size(), house::field(text, house::BODY))
            })
            .inner;
        if *focus {
            te.request_focus();
            *focus = false;
        }
        if te.lost_focus() {
            let (ritem, text) = (ritem.clone(), text.trim().to_string());
            let commit = !ui.input(|i| i.key_pressed(Key::Escape));
            self.renaming = None;
            if commit && !text.is_empty() {
                let url = match &ritem {
                    Item::File(id) => format!("api/files/{id}/rename?name={}", enc(&text)),
                    Item::Folder(p) => format!("api/folders/rename?path={}&name={}", enc(p), enc(&text)),
                };
                call(&self.inbox, ui.ctx(), Method::Post, url, None, refresh_after);
            }
        }
        true
    }

    fn label_of(&self, item: &Item) -> String {
        match item {
            Item::Folder(f) => name_of(f).to_string(),
            Item::File(id) => self.file(id).map_or_else(String::new, |f| f.name.clone()),
        }
    }

    fn role_of(&self, item: &Item) -> &str {
        match item {
            Item::Folder(f) => self.folder(f).map_or("owner", |f| f.role.as_str()),
            Item::File(id) => self.file(id).map_or("owner", |f| f.role.as_str()),
        }
    }

    fn activate(&mut self, item: &Item) {
        match item {
            Item::Folder(f) => {
                let f = f.clone();
                self.go_folder(&f);
            }
            // Files always open in a new tab, as from Google Drive, so the file browser stays
            // where it was (to-do 2 row 5; the separate "Open in new tab" entry went with it).
            Item::File(id) => new_tab(&page(&format!("?file={id}"))),
        }
    }

    fn start_rename(&mut self, item: &Item) {
        let name = self.label_of(item);
        let shown = match item {
            Item::File(_) => name.rsplit_once('.').map_or(name.clone(), |(s, _)| s.to_string()),
            Item::Folder(_) => name,
        };
        self.renaming = Some((item.clone(), shown, true));
    }

    fn item_menu(&mut self, ui: &mut Ui, item: &Item) {
        house::menu(ui, 280.0);
        let targets = if self.selected.contains(item) { self.selected.clone() } else { vec![item.clone()] };
        let single = targets.len() == 1;
        let role = self.role_of(item).to_string();
        let can_edit = role != "view";
        let is_space = matches!(item, Item::Folder(f) if !f.contains('/') && self.accounts);
        let row = |ui: &mut Ui, icon: &str, label: &str, keys: &str, enabled: bool| house::menu_row(ui, Some(icon), label, keys, enabled);
        if row(ui, "", "Open", "Enter", single) {
            self.activate(item);
            ui.close();
        }
        house::menu_sep(ui);
        if self.accounts && row(ui, "person_add", "Share…", "", single && !is_space) {
            let (folder, key) = match item {
                Item::Folder(p) => (true, p.clone()),
                Item::File(id) => (false, id.clone()),
            };
            crate::share::share_item(ui.ctx(), folder, &key, &self.label_of(item));
            ui.close();
        }
        let starred = match item {
            Item::Folder(f) => self.folder(f).is_some_and(|f| f.starred),
            Item::File(id) => self.file(id).is_some_and(|f| f.starred),
        };
        if self.accounts && row(ui, if starred { "star_fill" } else { "star" }, if starred { "Remove star" } else { "Star" }, "", true) {
            for t in &targets {
                let url = match t {
                    Item::File(id) => format!("api/files/{id}/star"),
                    Item::Folder(p) => format!("api/folders/star?path={}", enc(p)),
                };
                call(&self.inbox, ui.ctx(), Method::Post, url, Some(format!("{{\"on\":{}}}", !starred).into_bytes()), refresh_after);
            }
            ui.close();
        }
        if let Item::Folder(path) = item
            && self.accounts
        {
            self.colour_menu(ui, path, &targets);
        }
        if row(ui, "edit", "Rename", "F2", single && can_edit && !is_space) {
            self.start_rename(item);
            ui.close();
        }
        if let Item::File(id) = item {
            if row(ui, "content_copy", "Make a copy", "", single) {
                call(&self.inbox, ui.ctx(), Method::Post, format!("api/files/{id}/copy"), None, refresh_after);
                ui.close();
            }
            if row(ui, "download", "Download", "", single) {
                let name = self.label_of(item);
                call(&self.inbox, ui.ctx(), Method::Get, format!("api/files/{id}"), None, move |b| Reply::Download(name, b));
                ui.close();
            }
        }
        if row(ui, "info", "Details", "", single) {
            self.show_details(ui.ctx(), item.clone());
            ui.close();
        }
        house::menu_sep(ui);
        if row(ui, "content_copy", "Copy", "Ctrl+C", !is_space) {
            self.clipboard = Some((targets.clone(), false));
            ui.close();
        }
        if row(ui, "drive_file_move", "Move to…", "", can_edit && !is_space) {
            // Start in the folder they're in now, opened up to it.
            let here = match item {
                Item::File(id) => self.file(id).map(|f| f.folder.clone()),
                Item::Folder(p) => Some(parent_of(p).to_string()),
            };
            let mut expanded = HashSet::new();
            if let Some(h) = &here {
                let mut p = h.clone();
                while !p.is_empty() {
                    expanded.insert(p.clone());
                    p = parent_of(&p).to_string();
                }
            }
            self.move_to = Some(MoveTo { items: targets.clone(), target: None, expanded });
            ui.close();
        }
        if row(ui, "content_cut", "Cut", "Ctrl+X", can_edit && !is_space) {
            self.clipboard = Some((targets.clone(), true));
            ui.close();
        }
        if let Item::Folder(f) = item
            && row(ui, "content_paste", "Paste into folder", "Ctrl+V", self.clipboard.is_some() && single && can_edit)
        {
            self.paste_into(ui.ctx(), f.clone());
            ui.close();
        }
        house::menu_sep(ui);
        if row(ui, "delete", "Move to bin", "Delete", can_edit && !is_space) {
            self.delete(ui.ctx(), targets);
            ui.close();
        }
    }

    /// Folder colour ▸ : the house's folder colours as swatches (name on hover), and No colour.
    /// Personal: only you see the colours you pick.
    fn colour_menu(&mut self, ui: &mut Ui, path: &str, targets: &[Item]) {
        let current = self.folder(path).and_then(|f| f.colour);
        let text = RichText::new("Folder colour").size(house::BODY).color(theme::TEXT);
        let b = match crate::icons::icon("palette") {
            Some(img) => egui::Button::image_and_text(img.fit_to_exact_size(vec2(20.0, 20.0)), text),
            None => egui::Button::new(text),
        };
        let b = b.min_size(vec2(ui.available_width(), 32.0)).right_text(egui::containers::menu::SubMenuButton::arrow_atom(Some(theme::TEXT_DIM)));
        let mut pick: Option<Option<Color32>> = None;
        egui::containers::menu::SubMenuButton::from_button(b).ui(ui, |ui| {
            house::menu(ui, 7.0 * 32.0 + 2.0 * f32::from(house::MENU_INSET));
            house::menu_inset(ui, |ui| {
                house::note(ui, "Only you see the colours you pick.");
                ui.add_space(house::S);
                for chunk in house::FOLDER_COLOURS.chunks(7) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (name, c) in chunk {
                            let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                            let p = ui.painter();
                            p.circle_filled(r.center(), 11.0, *c);
                            if current == Some(*c) {
                                p.circle_stroke(r.center(), 14.0, Stroke::new(2.0, theme::TEXT));
                            } else if resp.hovered() {
                                p.circle_stroke(r.center(), 14.0, Stroke::new(2.0, theme::DIVIDER));
                            }
                            if resp.on_hover_text(*name).clicked() {
                                pick = Some(Some(*c));
                            }
                        }
                    });
                }
                ui.add_space(house::S);
            });
            if house::menu_row(ui, Some(""), "No colour", "", current.is_some()) {
                pick = Some(None);
            }
        });
        if let Some(choice) = pick {
            let colour = choice.map(house::hex).unwrap_or_default();
            for t in targets {
                if let Item::Folder(p) = t {
                    call(&self.inbox, ui.ctx(), Method::Post, format!("api/folders/colour?path={}", enc(p)), Some(format!("{{\"colour\":\"{colour}\"}}").into_bytes()), refresh_after);
                }
            }
            ui.close();
        }
    }

    fn delete(&mut self, ctx: &egui::Context, items: Vec<Item>) {
        for item in items {
            let url = match item {
                Item::File(id) => format!("api/files/{id}"),
                Item::Folder(p) => format!("api/folders?path={}", enc(&p)),
            };
            call(&self.inbox, ctx, Method::Delete, url, None, refresh_after);
        }
        self.selected.clear();
    }

    fn paste_into(&mut self, ctx: &egui::Context, folder: String) {
        if let Some((items, cut)) = self.clipboard.clone() {
            if cut {
                self.clipboard = None;
            }
            self.send_to(ctx, items, folder, cut);
        }
    }

    /// Copies or moves items into a folder; a move into someone else's folder asks first.
    fn send_to(&mut self, ctx: &egui::Context, items: Vec<Item>, folder: String, cut: bool) {
        let items: Vec<Item> = items.into_iter().filter(|i| !matches!(i, Item::Folder(p) if under(&folder, p))).collect();
        if items.is_empty() {
            return;
        }
        if cut && self.accounts {
            let to_owner = space_of(&folder).to_string();
            let owner_of = |i: &Item| match i {
                Item::File(id) => self.file(id).map(|f| f.owner.clone()).unwrap_or_default(),
                Item::Folder(p) => space_of(p).to_string(),
            };
            if items.iter().any(|i| owner_of(i) != to_owner) {
                self.pending_move = Some(PendingMove { items, to: folder, owner: to_owner });
                return;
            }
        }
        self.really_send(ctx, items, &folder, cut);
    }

    fn really_send(&mut self, ctx: &egui::Context, items: Vec<Item>, folder: &str, cut: bool) {
        for item in items {
            let url = match (&item, cut) {
                (Item::File(id), true) => format!("api/files/{id}/move?folder={}", enc(folder)),
                (Item::File(id), false) => format!("api/files/{id}/copy?folder={}", enc(folder)),
                (Item::Folder(p), true) => format!("api/folders/move?path={}&to={}", enc(p), enc(folder)),
                (Item::Folder(p), false) => format!("api/folders/copy?path={}&to={}", enc(p), enc(folder)),
            };
            call(&self.inbox, ctx, Method::Post, url, None, refresh_after);
        }
    }

    /// Move to…: a folder tree (your folders, folders shared with you that you can edit, and for
    /// the server's owner everyone's), pick one, Move here.
    fn move_to_ui(&mut self, ctx: &egui::Context) {
        let Some(mv) = self.move_to.as_ref() else { return };
        let (items, mut target, mut expanded) = (mv.items.clone(), mv.target.clone(), mv.expanded.clone());
        let what = if items.len() == 1 {
            let label = match &items[0] {
                Item::Folder(f) => name_of(f).to_string(),
                Item::File(id) => self.files.iter().find(|f| f.id == *id).map_or_else(String::new, |f| f.name.clone()),
            };
            format!("Move \u{201c}{label}\u{201d}")
        } else {
            format!("Move {} items", items.len())
        };
        // Where things can go: folders you can edit, not into a folder being moved.
        let moving: Vec<String> = items.iter().filter_map(|i| if let Item::Folder(p) = i { Some(p.clone()) } else { None }).collect();
        let ok = |path: &str, role: &str| role != "view" && !moving.iter().any(|m| under(path, m));
        let mut tops: Vec<(String, String)> = vec![(self.space.clone(), if self.accounts { "My files".into() } else { "All files".into() })];
        for t in &self.shared_folders {
            if self.folder(t).is_some_and(|f| f.role != "view") {
                tops.push((t.clone(), format!("{} (shared by {})", name_of(t), self.person(space_of(t)))));
            }
        }
        if self.accounts && self.admin {
            for f in self.folders.iter().filter(|f| !f.path.contains('/') && f.path != self.space && !f.path.is_empty()) {
                tops.push((f.path.clone(), format!("{}'s files", self.person(&f.path))));
            }
        }
        let folders = self.folders.clone();
        let mut close = false;
        let mut choice = None;
        house::dialog(ctx, "home_move_to", 460.0, &what, Some(&mut close), |ui| {
            house::note(ui, "Pick the folder to move to.");
            ui.add_space(house::S);
            egui::Frame::NONE.stroke(Stroke::new(1.0, theme::DIVIDER)).corner_radius(8).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(320.0).min_scrolled_height(320.0).auto_shrink([false, false]).show(ui, |ui| {
                    fn rows(ui: &mut Ui, t: &mut PickTree, path: &str, label: &str, depth: usize) {
                        let role = t.folders.iter().find(|f| f.path == path).map_or("owner", |f| f.role.as_str());
                        let allowed = (t.ok)(path, role);
                        let mut kids: Vec<&FolderRow> = t.folders.iter().filter(|f| parent_of(&f.path) == path && f.path != path).collect();
                        kids.sort_by_key(|f| name_of(&f.path).to_lowercase());
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                        let p = ui.painter().clone();
                        let picked = t.target.as_deref() == Some(path);
                        if picked {
                            p.rect_filled(r, 6, theme::SHARE);
                        } else if resp.hovered() && allowed {
                            p.rect_filled(r, 6, theme::MENU_HOVER);
                        }
                        let x = r.left() + 4.0 + depth as f32 * 18.0;
                        let arrow = Rect::from_center_size(egui::pos2(x + 10.0, r.center().y), vec2(20.0, 20.0));
                        let open = t.expanded.contains(path);
                        let mut toggled = false;
                        if !kids.is_empty() {
                            if let Some(img) = crate::icons::icon(if open { "expand_more" } else { "chevron_right" }) {
                                img.paint_at(ui, arrow);
                            }
                            if resp.clicked() && ui.input(|i| i.pointer.interact_pos().is_some_and(|q| arrow.expand(4.0).contains(q))) {
                                toggled = true;
                                if open {
                                    t.expanded.remove(path);
                                } else {
                                    t.expanded.insert(path.to_string());
                                }
                            }
                        }
                        let f = t.folders.iter().find(|f| f.path == path).cloned().unwrap_or_default();
                        folder_icon(ui, &f, Rect::from_center_size(egui::pos2(x + 34.0, r.center().y), vec2(20.0, 20.0)));
                        p.text(egui::pos2(x + 52.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.0), if allowed { theme::TEXT } else { theme::DIVIDER });
                        if resp.clicked() && !toggled && allowed {
                            *t.target = Some(path.to_string());
                        }
                        if resp.double_clicked() && !kids.is_empty() {
                            t.expanded.insert(path.to_string());
                        }
                        if t.expanded.contains(path) {
                            for k in kids {
                                rows(ui, t, &k.path, name_of(&k.path), depth + 1);
                            }
                        }
                    }
                    for (path, label) in &tops {
                        rows(ui, &mut PickTree { folders: &folders, ok: &ok, target: &mut target, expanded: &mut expanded }, path, label, 0);
                    }
                });
            });
            choice = house::actions(ui, &["Cancel", "Move here"]);
        });
        let mv = self.move_to.as_mut().expect("open");
        mv.target = target.clone();
        mv.expanded = expanded;
        if close || choice == Some(0) || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.move_to = None;
        } else if choice == Some(1) {
            match target {
                Some(to) => {
                    let items = self.move_to.take().map(|m| m.items).unwrap_or_default();
                    self.send_to(ctx, items, to, true);
                }
                None => self.error = Some("Pick a folder to move to first.".into()),
            }
        }
    }

    fn show_details(&mut self, ctx: &egui::Context, item: Item) {
        self.details = Some((item.clone(), None));
        if self.accounts && !matches!(&item, Item::Folder(p) if !p.contains('/')) {
            let url = match &item {
                Item::File(id) => format!("api/files/{id}/sharing"),
                Item::Folder(p) => format!("api/folders/sharing?path={}", enc(p)),
            };
            call(&self.inbox, ctx, Method::Get, url, None, move |b| Reply::Access(item, serde_json::from_slice(&b).unwrap_or_default()));
        }
    }

    /// Where a path is, in words: "My files › Budgets", "Ada's files › Team".
    fn place(&self, folder: &str) -> String {
        let top = space_of(folder);
        let mut parts = vec![if !self.accounts { "All files".to_string() } else if top == self.space { "My files".into() } else { format!("{}'s files", self.person(top)) }];
        parts.extend(folder.split('/').skip(usize::from(self.accounts)).filter(|p| !p.is_empty()).map(str::to_string));
        parts.join(" › ")
    }

    /// Details: what it is, where, whose, how big, when it changed and by whom, who can open it.
    fn details_ui(&mut self, ctx: &egui::Context) {
        let Some((item, access)) = self.details.clone() else { return };
        let mut rows: Vec<(&str, String)> = Vec::new();
        let title;
        match &item {
            Item::File(id) => {
                let Some(f) = self.file(id).cloned() else {
                    self.details = None;
                    return;
                };
                title = f.name.clone();
                let ext = f.name.rsplit_once('.').map_or("xlsx".to_string(), |(_, e)| e.to_lowercase());
                rows.push(("Type", format!("Spreadsheet (.{ext})")));
                rows.push(("Location", self.place(&f.folder)));
                if self.accounts {
                    rows.push(("Owner", if f.owner == self.space { "You".into() } else { self.person(&f.owner) }));
                }
                rows.push(("Size", size_text(f.size)));
                let by = if f.changed_by.is_empty() { String::new() } else if f.changed_by == self.space { " by you".into() } else { format!(" by {}", self.person(&f.changed_by)) };
                rows.push(("Modified", format!("{}{by}", ago(f.modified))));
                if let Some(t) = f.opened {
                    rows.push(("Opened by you", ago(t)));
                }
                if self.accounts {
                    rows.push(("Your access", match f.role.as_str() { "owner" => "Owner", "edit" => "Can edit", _ => "Can view" }.into()));
                }
            }
            Item::Folder(p) => {
                let Some(f) = self.folder(p).cloned() else {
                    self.details = None;
                    return;
                };
                title = self.folder_label(p);
                rows.push(("Type", "Folder".into()));
                if p.contains('/') || !self.accounts {
                    rows.push(("Location", self.place(parent_of(p))));
                }
                if self.accounts {
                    rows.push(("Owner", if f.owner == self.space { "You".into() } else { self.person(&f.owner) }));
                }
                let files = self.files.iter().filter(|x| under(&x.folder, p)).count();
                let folders = self.folders.iter().filter(|x| x.path != *p && under(&x.path, p)).count();
                let plural = |n: usize, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
                rows.push(("Contains", format!("{}, {}", plural(files, "spreadsheet"), plural(folders, "folder"))));
                let (newest, size) = self.folder_stats(p);
                rows.push(("Size", size_text(size)));
                if newest > 0 {
                    rows.push(("Last change inside", ago(newest)));
                }
                if let Some(c) = f.colour {
                    let name = house::FOLDER_COLOURS.iter().find(|(_, x)| *x == c).map_or("", |(n, _)| *n);
                    rows.push(("Colour (only you see it)", name.to_string()));
                }
            }
        }
        // Who has access: the owner, the people it's shared with, and the link setting.
        let mut access_lines: Vec<String> = Vec::new();
        if let Some(a) = &access {
            for person in a["people"].as_array().into_iter().flatten() {
                let role = match person["role"].as_str().unwrap_or("") {
                    "owner" => "owner",
                    "edit" => "can edit",
                    _ => "can view",
                };
                access_lines.push(format!("{} ({role})", person["name"].as_str().unwrap_or("")));
            }
            match a["link"].as_str().unwrap_or("none") {
                "view" => access_lines.push("Anyone signed in with the link can view".into()),
                "edit" => access_lines.push("Anyone signed in with the link can edit".into()),
                _ => {}
            }
        }
        let mut close = false;
        let mut done = false;
        let mut share = false;
        house::dialog(ctx, "home_details", 480.0, &title, Some(&mut close), |ui| {
            egui::Grid::new("home_details_grid").num_columns(2).spacing(vec2(house::L, house::S)).show(ui, |ui| {
                for (k, v) in &rows {
                    ui.label(RichText::new(*k).size(house::BODY).color(theme::TEXT_DIM));
                    ui.label(RichText::new(v).size(house::BODY).color(theme::TEXT));
                    ui.end_row();
                }
                if self.accounts && !matches!(&item, Item::Folder(p) if !p.contains('/')) {
                    ui.label(RichText::new("Who has access").size(house::BODY).color(theme::TEXT_DIM));
                    ui.vertical(|ui| {
                        if access.is_none() {
                            ui.label(RichText::new("Loading…").size(house::BODY).color(theme::TEXT_DIM));
                        }
                        for l in &access_lines {
                            ui.label(RichText::new(l).size(house::BODY).color(theme::TEXT));
                        }
                    });
                    ui.end_row();
                }
            });
            let can_share = self.accounts && self.role_of(&item) == "owner" && !matches!(&item, Item::Folder(p) if !p.contains('/'));
            let labels: &[&str] = if can_share { &["Share…", "Close"] } else { &["Close"] };
            match house::actions(ui, labels) {
                Some(0) if can_share => share = true,
                Some(_) => done = true,
                None => {}
            }
        });
        if share {
            let (folder, key) = match &item {
                Item::Folder(p) => (true, p.clone()),
                Item::File(id) => (false, id.clone()),
            };
            crate::share::share_item(ctx, folder, &key, &title);
            self.details = None;
        } else if close || done || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.details = None;
        }
    }

    fn pending_move_ui(&mut self, ctx: &egui::Context) {
        let Some(pm) = &self.pending_move else { return };
        let who = self.person(&pm.owner);
        let what = if pm.items.len() == 1 { format!("\u{201c}{}\u{201d}", self.label_of(&pm.items[0])) } else { format!("{} items", pm.items.len()) };
        let text = if who == "me" {
            format!("Move {what} into your own folder? You'll own it, and the people it was shared with through its old folder lose access.")
        } else {
            format!("Move {what} into {who}'s folder? {who} will own it and decide who else can open it.")
        };
        let mut close = false;
        let mut choice = None;
        house::dialog(ctx, "home_move_ask", 420.0, "Change who owns it?", Some(&mut close), |ui| {
            house::text(ui, &text);
            choice = house::actions(ui, &["Cancel", "Move"]);
        });
        if close || choice == Some(0) {
            self.pending_move = None;
        } else if choice == Some(1)
            && let Some(pm) = self.pending_move.take()
        {
            self.really_send(ctx, pm.items, &pm.to, true);
        }
    }

    /// Dragging: a label follows the pointer; letting go over a folder (card, tree row or path)
    /// moves there.
    fn drag_ui(&mut self, ctx: &egui::Context) {
        let target = ctx.data_mut(|m| m.remove_temp::<String>(egui::Id::unique("home_drop")));
        let Some(items) = self.dragging.clone() else { return };
        if let Some(pos) = ctx.pointer_latest_pos() {
            let label = if items.len() == 1 { self.label_of(&items[0]) } else { format!("{} items", items.len()) };
            egui::Area::new(egui::Id::unique("home_drag")).fixed_pos(pos + vec2(14.0, 10.0)).order(egui::Order::Tooltip).interactable(false).show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(label);
                });
            });
        }
        if ctx.input(|i| i.pointer.any_released()) {
            self.dragging = None;
            if let Some(to) = target {
                self.send_to(ctx, items, to, true);
            }
        }
    }

    /// The mouse's back and forward buttons go Back and Forward through the places you visited
    /// (not Up: Up is the parent folder, Back is where you came from), instead of the browser
    /// leaving the page: index.html stops the browser's own handling while the home screen is up
    /// (`__ssHomeActive`) and counts the presses (`__ssHomeUp` = back, `__ssHomeFwd`);
    /// `__ssAtTop` (nothing to go back to) lets the browser's back button leave.
    fn mouse_buttons(&mut self) {
        let Some(w) = web_sys::window() else { return };
        let get = |k: &str| js_sys::Reflect::get(&w, &k.into()).ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as usize;
        let set = |k: &str, v: wasm_bindgen::JsValue| {
            let _ = js_sys::Reflect::set(&w, &k.into(), &v);
        };
        set("__ssHomeActive", true.into());
        let (up, fwd) = (get("__ssHomeUp"), get("__ssHomeFwd"));
        if up > 0 {
            set("__ssHomeUp", 0.into());
            for _ in 0..up {
                self.go_back();
            }
        }
        if fwd > 0 {
            set("__ssHomeFwd", 0.into());
            for _ in 0..fwd {
                self.go_forward();
            }
        }
        set("__ssAtTop", self.back.is_empty().into());
    }

    /// A browser that still went back (the history fallback in index.html) gives no input event,
    /// so the home screen looks once a second; a real button press wakes it at once.
    fn keep_listening(ctx: &egui::Context) {
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }

    fn keys(&mut self, ctx: &egui::Context) {
        self.mouse_buttons();
        Self::keep_listening(ctx);
        if self.renaming.is_some() || ctx.memory(|m| m.focused()).is_some() || self.pending_move.is_some() || self.move_to.is_some() || self.details.is_some() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                egui::Event::Copy if !self.selected.is_empty() => self.clipboard = Some((self.selected.clone(), false)),
                egui::Event::Cut if !self.selected.is_empty() => self.clipboard = Some((self.selected.clone(), true)),
                egui::Event::Paste(_) => {
                    if let Some(h) = self.here() {
                        self.paste_into(ctx, h);
                    }
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } => match key {
                    Key::Enter if self.selected.len() == 1 => {
                        let item = self.selected[0].clone();
                        self.activate(&item);
                    }
                    Key::F2 if self.selected.len() == 1 => {
                        let item = self.selected[0].clone();
                        if self.role_of(&item) != "view" {
                            self.start_rename(&item);
                        }
                    }
                    Key::Delete if !self.selected.is_empty() => {
                        let items = std::mem::take(&mut self.selected);
                        self.delete(ctx, items);
                    }
                    Key::Backspace => self.go_up(),
                    Key::A if modifiers.command => self.selected = self.order(),
                    _ => {}
                },
                _ => {}
            }
        }
    }

    fn bin_ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            house::note(ui, "Things in the bin are deleted for good after 30 days.");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let sure = self.confirm_purge == Some(None);
                if ui.add_enabled(!self.bin.is_empty(), egui::Button::new(RichText::new(if sure { "Click again to empty the bin" } else { "Empty bin" }).color(house::ERROR)).frame(false)).clicked() {
                    if sure {
                        call(&self.inbox, ui.ctx(), Method::Delete, "api/bin".into(), None, refresh_after);
                        self.confirm_purge = None;
                    } else {
                        self.confirm_purge = Some(None);
                    }
                }
            });
        });
        ui.add_space(house::S);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.loading && self.bin.is_empty() {
                ui.label(RichText::new("Loading…").color(theme::TEXT_DIM));
            } else if self.bin.is_empty() {
                ui.label(RichText::new("The bin is empty.").color(theme::TEXT_DIM));
            }
            let w = ui.available_width() - 8.0;
            for b in self.bin.clone() {
                let (r, _) = ui.allocate_exact_size(vec2(w, ROW + 6.0), Sense::hover());
                let p = ui.painter().clone();
                if let Some(img) = crate::icons::icon(if b.folder { "folder" } else { "starsheets" }) {
                    img.paint_at(ui, Rect::from_center_size(r.left_center() + vec2(20.0, 0.0), vec2(20.0, 20.0)));
                }
                p.text(r.left_center() + vec2(44.0, -8.0), Align2::LEFT_CENTER, name_of(&b.original), FontId::proportional(14.0), theme::TEXT);
                let from = parent_of(&b.original);
                let from = if from.is_empty() || from == self.space { "My files".to_string() } else { from.strip_prefix(&format!("{}/", self.space)).unwrap_or(from).to_string() };
                p.text(r.left_center() + vec2(44.0, 10.0), Align2::LEFT_CENTER, format!("from {from} · deleted {}", ago(b.deleted)), FontId::proportional(12.0), theme::TEXT_DIM);
                let restore = Rect::from_min_size(r.right_top() + vec2(-260.0, 6.0), vec2(96.0, ROW - 6.0));
                let purge = Rect::from_min_size(r.right_top() + vec2(-156.0, 6.0), vec2(150.0, ROW - 6.0));
                if ui.put(restore, house::secondary("Restore")).clicked() {
                    call(&self.inbox, ui.ctx(), Method::Post, format!("api/bin/restore?name={}", enc(&b.name)), None, refresh_after);
                }
                let sure = self.confirm_purge.as_ref() == Some(&Some(b.name.clone()));
                if ui.put(purge, egui::Button::new(RichText::new(if sure { "Sure? Click again" } else { "Delete forever" }).color(house::ERROR)).frame(false)).clicked() {
                    if sure {
                        call(&self.inbox, ui.ctx(), Method::Delete, format!("api/bin?name={}", enc(&b.name)), None, refresh_after);
                        self.confirm_purge = None;
                    } else {
                        self.confirm_purge = Some(Some(b.name.clone()));
                    }
                }
            }
        });
    }
}

/// A folder's icon: filled in its colour if it has one, else the grey outline (with people on it
/// when it's shared).
fn folder_icon(ui: &Ui, f: &FolderRow, rect: Rect) {
    let img = match f.colour {
        Some(c) => crate::icons::icon("folder_fill").map(|i| i.tint(c)),
        None => crate::icons::icon(if f.shared { "folder_shared" } else { "folder" }),
    };
    if let Some(img) = img {
        img.paint_at(ui, rect);
    }
}

/// A per-browser convenience (grid or list); missing storage just means the default.
fn stored(key: &str) -> Option<String> {
    web_sys::window()?.local_storage().ok()??.get_item(key).ok()?
}

fn store(key: &str, value: &str) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(key, value);
    }
}

impl Home {
    /// How a folder is called on screen: someone's own folder by their name.
    fn folder_label(&self, path: &str) -> String {
        if self.accounts && !path.contains('/') {
            let p = self.person(path);
            return if p == "me" { "My files".into() } else { p };
        }
        name_of(path).to_string()
    }
}

fn size_text(n: u64) -> String {
    match n {
        0..=1023 => format!("{n} B"),
        1024..=1_048_575 => format!("{:.0} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.0),
    }
}

/// What the "Move to" folder tree reads and changes as it draws itself, row by row.
struct PickTree<'a> {
    folders: &'a [FolderRow],
    ok: &'a dyn Fn(&str, &str) -> bool,
    target: &'a mut Option<String>,
    expanded: &'a mut HashSet<String>,
}

/// One row of the list view: a folder or a file, as it is shown.
struct ListRow {
    item: Item,
    name: String,
    icon: &'static str,
    owner: String,
    modified: i64,
    size: Option<u64>,
    starred: bool,
}
