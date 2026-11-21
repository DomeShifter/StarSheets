# StarSheets roadmap

> **Last updated:** 2026-11-21 · **Now:** phase 0 (test build)

| Phase | Goal | Done when | Status |
|---|---|---|---|
| 0 Test build | Our app with a Sheets menu bar and toolbar instead of GridCraft's ribbon, Sheets colours | Opens and edits an xlsx in the browser with no ribbon; list of what can't be restyled | **in progress** |
| 1 Look and feel | Sheets menus, toolbar with icons, bottom bar (+, sheet list, tabs, quick sum), blue selection, side panels instead of dialogs, Sheets shortcuts | Side-by-side screenshots match closely | next |
| 2 Saving like Drive | Small server: file list home page, saving after every change (no Save button), version history | Close the tab, reopen, nothing lost; restore an older version | |
| 3 Sharing | Logins, view and edit links | A second person opens a shared sheet | |
| 4 Live co-editing (optional) | Two people editing at once | Two browsers edit together without clashes | |
| 5 Extras | Sheets-only functions (QUERY, SPLIT…), phone layout | as needed | |

## Phase 0 checklist
- [x] Repository, licence, docs
- [x] GridCraft pinned (v0.19.1) and fetched by script; patch queue
- [x] Patch: switch to hide the ribbon (`Chrome.ribbon`)
- [x] Patch: theme override (`Tokens::install`)
- [x] Title row, menu bar and toolbar driven by data tables
- [x] Sheets palette
- [ ] CI builds the web app
- [ ] Hosted on the home server
- [ ] Spike report: what works, what needs more patches (→ docs/decisions.md)

The full Sheets feature list with a phase per item is in [docs/sheets-reference.md](docs/sheets-reference.md).
