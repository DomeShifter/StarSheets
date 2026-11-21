# Upstream: GridCraft

| | |
|---|---|
| Repo | https://github.com/storytold/gridcraft |
| Pinned | **v0.19.1** (`476448d7`), see `upstream.pin` |
| Licence | MIT OR Apache-2.0 |
| Pace | alpha, roughly 40 commits a day (Nov 2026) |

## How we use it
`scripts/fetch-upstream.sh` clones the pinned tag into `upstream/gridcraft` (git-ignored), checks the commit hash and applies `patches/*.patch` in order. Our `Cargo.toml` points at the crates there by path. It also copies upstream's `[patch.crates-io]` block (GridCraft builds on its own egui fork); the script fails if the two blocks differ.

What we depend on: `gridcraft-engine` (commands through `Session::execute`) and `gridcraft-ui-egui` (`SheetApp`, `Chrome`, `theme::Tokens`, `run_or_alert`). Our code calls ui-egui only from `app/src/web.rs`, `app/src/shell.rs` and `app/src/theme.rs`.

## Updating (monthly, not nightly)
1. Branch `upstream-vX.Y.Z`; set `tag` and `commit` in `upstream.pin`.
2. Run `scripts/fetch-upstream.sh`. If a patch fails, rebase it (or drop it if upstream merged an equivalent).
3. Copy upstream's `[patch.crates-io]` block if the script reports a difference. Start from upstream's `Cargo.lock` if dependency resolution fails.
4. Push; CI must be green. Check the app by eye (menus, toolbar, open/save an xlsx).
5. Merge, add a line to the log below.

## Our patches
| Patch | What | Upstream status |
|---|---|---|
| 0001-ui-egui-chrome-ribbon-switch | `Chrome.ribbon: bool`: a host can hide the ribbon and its key tips | to offer as a PR |
| 0002-ui-egui-theme-token-override | `Tokens::install` / `Tokens::for_mode`: a host can replace the colour palettes | to offer as a PR |
| 0003-ui-egui-chrome-sheet-tabs-switch | `Chrome.sheet_tabs: bool`: a host can hide the sheet tabs and status bar | to offer as a PR |
| 0004-ui-egui-sheets-cell-keys | `UiState.sheets_keys`: Enter edits (and is consumed so the new editor doesn't commit at once), Backspace only clears, no Enter-paste, static copy outline | to offer as a PR (as an option) |
| 0005-ui-egui-host-font-faces | `theme::set_host_faces`: a host supplies the faces for the UI and cell font roles (web builds have no system fonts) | to offer as a PR |
| 0007-ui-egui-sheets-cell-menu | With `sheets_keys`, the cell right-click menu uses Sheets' items and order | to offer as a PR (as an option) |
| 0008-ui-egui-sheets-cell-marks | `Tokens.sheets_marks`: red corner on error cells, black note / orange comment corners (top-right) | to offer as a PR (as an option) |
| 0006-model-sheets-defaults | New workbooks use Roboto 10 pt instead of Calibri 11. Row/column sizes deliberately NOT changed here: the xlsx writer/reader treat `DEFAULT_COL_WIDTH`/`DEFAULT_ROW_HEIGHT` as the file default, so changing them would misread Excel files | StarSheets only (not for upstream) |

## Log
| Date | From → to | Notes |
|---|---|---|
| 2026-11-21 | → v0.19.1 | first pin |
