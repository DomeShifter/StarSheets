# StarSheets roadmap

> **Last updated:** 2026-11-21 · **Now:** phase 1 (look and feel); phase 0 done except the spike report

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
- [x] CI builds the web app (about 5 minutes per build)
- [x] Hosted on the home server
- [ ] Spike report: what works, what needs more patches (→ docs/decisions.md)

## Phase 1 checklist
- [x] Toolbar: Material icons, font and size boxes showing the active cell, zoom, number formats, colour palette, borders, merge, alignment, wrapping, rotation, functions
- [x] Bottom bar: + / sheet list / tabs / quick sum (patch 0003)
- [x] Roboto as the default font (patches 0005, 0006)
- [x] Enter edits, no selection animation, Backspace only clears (patch 0004)
- [ ] Cell interactions list, remaining rows (docs/sheets-reference.md §4b)
- [x] Sheets keyboard shortcuts that differ from Excel (patch 0011; Ctrl+Shift+F and Alt+1 in the app)
- [ ] Still to do from §5: Ctrl+/ shortcut list, Alt+/ tool finder, Alt+Shift+K sheet list, comment navigation keys
- [ ] Side panels instead of dialogs (conditional formatting, data validation, charts)
- [x] Default text size 10 pt
- [x] Default 21 px rows and 100 px columns for new sheets only (patch 0010; opened files keep their sizes)
- [x] Right-click menu, error/note markers, link card (patches 0007-0009)
- [x] Grid at Sheets' 1:1 size, Sheets header sizes, plain arrow cursor, flat menus/toolbar, unboxed formula bar (patch 0012)
- [x] Typed web/e-mail addresses become links; browser shortcuts that clash with Sheets ones (Ctrl+D, Ctrl+B…) are blocked; hand cursors for dragging cells (patch 0013)
- [x] Sheets-style menus: icons, 32 px rows, grey shortcuts, ▸ submenus, dividers, rounded panel with shadow (docs/sheets-menus.md); Sheets-blue links (patch 0014); link card under its cell
- [x] Menu contents matched to docs/sheets-menus.md where GridCraft has the command (Number, Rotation, Font size, Function, Zoom submenus; UK wording)
- [x] Right-click menus (cell, row and column headers, sheet tab + its ▾) and toolbar dropdowns (icon strips, number formats with examples) in the Sheets look (patch 0015)
- [ ] Still missing from Sheets menus: Clip wrapping, Help › Keyboard shortcuts (Ctrl+/), Search the menus (Alt+/), Hidden sheets, Full screen, Version history (phase 2)
- [x] Named ranges suggested while typing formulas (patch 0016); sheet tabs: ▾ opens the menu, drag to reorder
- [ ] Dark mode palette

The full Sheets feature list with a phase per item is in [docs/sheets-reference.md](docs/sheets-reference.md).
