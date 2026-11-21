# StarSheets

A spreadsheet for the web that looks and works like Google Sheets. It's free, open source and self-hostable.

StarSheets is built on the engine and grid of [GridCraft](https://github.com/storytold/gridcraft), a fast Rust spreadsheet by the ArtCraft team. GridCraft aims to match Microsoft Excel. StarSheets keeps GridCraft's calculation engine, file handling and grid, and replaces the Excel-style screen with a Google Sheets-style one: menu bar, a one-row toolbar and side panels. Saving like Google Drive and sharing come in later phases.

> **Status: phase 0 (test build).** Not ready for real work. See [ROADMAP.md](ROADMAP.md).

StarSheets is not affiliated with Google or with the ArtCraft team. "Google Sheets" is a trademark of Google LLC and is mentioned only to describe the look we aim for.

## How it's built

| Part | Where it comes from |
|---|---|
| Formula engine, xlsx/csv import and export, charts, grid, formula bar, dialogs | GridCraft crates, pinned to one release (`upstream.pin`), with a few small patches (`patches/`) |
| Title row, menu bar, toolbar, colours | StarSheets (`app/src/shell.rs`, `app/src/theme.rs`) |
| Web runner | adapted from GridCraft's web app (`app/src/web.rs`) |

Everything runs in the browser through WebAssembly. Details: [docs/architecture.md](docs/architecture.md).

## Build

You need Rust (stable) with the `wasm32-unknown-unknown` target, and [trunk](https://trunkrs.dev).

```sh
scripts/fetch-upstream.sh        # GridCraft at the pinned tag, patches applied
cd app && trunk build --release  # output in dist/
```

CI builds every push. The finished site is attached to each run as the `starsheets-web` artifact.

## Documents

| File | What |
|---|---|
| [ROADMAP.md](ROADMAP.md) | Phases and progress |
| [docs/sheets-reference.md](docs/sheets-reference.md) | The Google Sheets layout, menus, toolbar, shortcuts and features we emulate |
| [docs/decisions.md](docs/decisions.md) | Each design choice and why |
| [docs/upstream.md](docs/upstream.md) | Which GridCraft release we use, how to update it, our patches |
| [docs/architecture.md](docs/architecture.md) | How the code fits together |

## Licence

MIT OR Apache-2.0, the same as GridCraft. See [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE) and [NOTICE](NOTICE).
