# StarSheets: notes for AI assistants

- Goal and phases: README.md, ROADMAP.md. Target behaviour: docs/sheets-reference.md. Choices: docs/decisions.md (add a line for every new one).
- **Files stay .xlsx** (docs/decisions.md 2026-11-23): never add a StarSheets-only file format. Data Excel can't hold goes in the StarSheets part inside the .xlsx; caches are derived and disposable.
- **Never edit `upstream/`.** A change GridCraft must make goes in `patches/NNNN-<crate>-<what>.patch` (made with `git diff` in a clean checkout of the pinned tag), gets a row in docs/upstream.md, and should be offered upstream.
- **House style:** build every dialog, menu, card and panel from `app/src/house.rs` (docs/house-style.md); no one-off sizes or colours.
- Menus and toolbar are data tables in `app/src/shell.rs`; only use command ids that exist (the test checks).
- This repo is **public**: no hostnames, IP addresses, credentials or details of anyone's home server. Deployment specifics live outside the repo.
- Keep docs short and in plain words; update ROADMAP.md checkboxes as work lands.
- Licence headers: files adapted from GridCraft say so in their first lines.
