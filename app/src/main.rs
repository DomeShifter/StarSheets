//! StarSheets: a spreadsheet for the web that looks and works like Google Sheets, built on
//! GridCraft's engine and grid (see README.md and docs/architecture.md).
//!
//! Build with `trunk build --release` from `app/` after `scripts/fetch-upstream.sh`.

#[cfg(target_arch = "wasm32")]
mod prefs;
mod autolink;
mod bottom;
mod cfpanel;
mod fonts;
mod icons;
mod linkcard;
mod shell;
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
