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

## Log
| Date | From → to | Notes |
|---|---|---|
| 2026-11-21 | → v0.19.1 | first pin |
