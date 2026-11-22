//! StarSheets: a spreadsheet for the web that looks and works like Google Sheets, built on
//! GridCraft's engine and grid (see README.md and docs/architecture.md).
//!
//! Build with `trunk build --release` from `app/` after `scripts/fetch-upstream.sh`.

#[cfg(target_arch = "wasm32")]
mod prefs;
mod protectpanel;
#[cfg(target_arch = "wasm32")]
mod account;
mod autolink;
mod calcpreview;
mod bottom;
mod cfpanel;
mod chartpanel;
mod colourpicker;
mod customdate;
mod customnumber;
mod datepicker;
mod dvpanel;
mod errortip;
#[cfg(target_arch = "wasm32")]
mod filebrowser;
#[cfg(target_arch = "wasm32")]
mod files;
mod findreplace;
mod fonts;
mod house;
mod icons;
mod linkcard;
#[cfg(target_arch = "wasm32")]
mod live;
mod merge;
mod messages;
mod namespanel;
mod notes;
mod sheets_formulas;
mod settings;
#[cfg(target_arch = "wasm32")]
mod share;
mod shell;
mod spelling;
mod theme;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("starsheets only runs in the browser: build it with `trunk build --release` in app/");
}
