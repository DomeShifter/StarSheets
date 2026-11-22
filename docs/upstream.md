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
| 0018-ui-egui-sheets-dropdown-look | With `sheets_marks`: a grey ▾ inside every drop-down cell (click opens the list), and a red corner on cells that break a "warning" validation rule | to offer as a PR |
| 0019-ui-egui-sheets-filter-menu | With a host menu look: Sheets' filter menu (Sort A to Z / Z to A, Filter by values with Select all / Clear, Displaying N, ticked list, Cancel / OK); with `sheets_marks`: Sheets' green filter icon in headers | to offer as a PR |
| 0020-ui-egui-sheets-formula-bar-press-edits | With `sheets_formula_bar`: a press in the formula bar starts editing with the caret where it was pressed; the shown text isn't selectable (a selection there made Ctrl+C copy the whole cell) | to offer as a PR |
| 0021-ui-egui-web-font-faces | `fonts::add_web_face(family, bold, italic, bytes)`: a web host adds font faces at run time (a web build has no installed fonts); workbook fonts rebind when one arrives | to offer as a PR |
| 0022-sheets-array-literals-arrayformula | Google Sheets formulas: `{…}` holding formulas/ranges is an array literal (`,` side by side, `;` under; stored as ARRAY_LITERAL/ARRAY_ROW, printed back in braces; constant arrays unchanged), ARRAYFORMULA (pass-through, spills); with `sheets_keys`, Ctrl+Shift+Enter wraps in ARRAYFORMULA and legacy array formulas show/edit as ARRAYFORMULA. Two parser tests updated to match | StarSheets-specific (Sheets syntax) |
| 0023-functions-query | Registers QUERY from StarSheets' own crate (crates/sheets-functions) and has the calculator pass the data range's first column, so a query names columns by their sheet letters | StarSheets-specific |
| 0024-sheets-merge-vertically-copy-outline-font-flash | `home.mergeDown` (Google's Merge vertically: one merge per column); no copy outline in Sheets mode; fonts are rebuilt only when a new typeface is needed (a font size change used to rebuild them and flash the screen) | StarSheets-specific (the font fix could help upstream) |
| 0025-ui-egui-sheets-notes-no-comments | In Sheets mode: no "Comment" in the cell menu, and notes are drawn by the host (StarSheets' Google-style note card) | StarSheets-specific |
| 0026-core-input-locale | `parse::set_input_locale`: a host locale for typed input: day-first dates (05/08 = 5 August) and the formats typed dates, times, date-times and plain numbers get. Unset = unchanged (US reading). Patch 0025 also maps the ISO \\ key (IntlBackslash) for Ctrl+\\ | could help upstream |
| 0027-ui-egui-sheets-note-menu | Sheets cell menu: on a cell with a note, "Insert note" becomes "Edit note" and "Delete note" | Sheets-specific |
| 0028-ui-egui-kept-font-families | `fonts::keep_families`: families a host keeps bound in every font set (the font menu draws each name in its own font) | could help upstream |
| 0029-ui-egui-sheets-chart-menu | Sheets mode: a chart's right-click menu is "Edit chart" (opens the host's chart editor) and "Delete chart" | Sheets-specific |
| 0030-ui-egui-sheets-dialog-titles | Sheets mode: dialogs have no title band; the title sits top left (22 px) with ✕ at the right, as in Google Sheets | Sheets-specific |
| 0031-ui-egui-sheets-formula-bar-one-line | Sheets mode: the formula bar stays one line until the chevron expands it (long text runs on, no wrap), and fx is drawn larger; `widgets::icon_button_sized` | Sheets-specific |
| 0032-ui-egui-sheets-enter-accepts-suggestion | Sheets mode: Enter accepts the highlighted function suggestion ("=lef" → "=LEFT(") and editing continues, as in Google Sheets | Sheets-specific |
| 0033-core-engine-number-separators | `parse::set_number_separators`: a host decimal mark and thousands separator; typed numbers may use them ("1.234,5"), `display::format` shows non-date numbers with them, and `edit_text` edits plain numbers with the decimal mark. Default unchanged | could help upstream |
| 0034-engine-fill-extends-validation | `edit.autoFill` (fill handle): a data-validation rule covering every source cell is extended to the target, as Google Sheets does (not for Fill Without Formatting) | could help upstream |
| 0035-model-xlsx-chart-pie-slice-colours | `Chart::point_colors`: a colour per pie/doughnut slice, read and written as `c:dPt`, drawn through the per-point palette; `chart.formatSelection {point}`; `chart.changeColors` colours every slice of a pie | could help upstream |
| 0036-ui-egui-spell-check-hook | `spelling::set`: a host word check; the cell editor underlines unknown words in red while editing plain text (not formulas, not the word being typed) | could help upstream |
| 0002-ui-egui-theme-token-override | `Tokens::install` / `Tokens::for_mode`: a host can replace the colour palettes | to offer as a PR |
| 0003-ui-egui-chrome-sheet-tabs-switch | `Chrome.sheet_tabs: bool`: a host can hide the sheet tabs and status bar | to offer as a PR |
| 0004-ui-egui-sheets-cell-keys | `UiState.sheets_keys`: Enter edits (and is consumed so the new editor doesn't commit at once), Backspace only clears, no Enter-paste, static copy outline | to offer as a PR (as an option) |
| 0005-ui-egui-host-font-faces | `theme::set_host_faces`: a host supplies the faces for the UI and cell font roles (web builds have no system fonts) | to offer as a PR |
| 0007-ui-egui-sheets-cell-menu | With `sheets_keys`, the cell right-click menu uses Sheets' items and order | to offer as a PR (as an option) |
| 0008-ui-egui-sheets-cell-marks | `Tokens.sheets_marks`: red corner on error cells, black note / orange comment corners (top-right) | to offer as a PR (as an option) |
| 0009-ui-egui-sheets-link-click | With `sheets_keys`, a plain click on a link cell only selects it (StarSheets shows a link card) | to offer as a PR (as an option) |
| 0017-engine-colour-scale-stops | `home.conditionalFormat` colour scales take `stops: [{type, value?, color}]` (2 or 3 points: min/max/num/percent/percentile) | to offer as a PR |
| 0016-ui-egui-named-range-suggestions | Typing a formula also suggests the named ranges the sheet can see (any case), listed first with ▦ and what they refer to; accepting one adds no `(` | to offer as a PR |
| 0015-ui-egui-host-menu-look | `UiState.menu_look`: a host can draw the cell and header menus with its own rows, style and dividers; Sheets wording with counts ("Insert 3 rows above", "Delete rows 3 - 5"); Sheets row/column header menus (insert below/right, clear, hide, resize, sort sheet) | to offer as a PR (as an option) |
| 0014-model-sheets-link-colour | Default theme's link colours (hlink, folHlink) set to Sheets' blue #1155CC; files with their own theme keep theirs | StarSheets only |
| 0013-ui-egui-sheets-hand-cursors | Open hand over a selection's edge (and a selected column header), closed hand while dragging cells/columns/rows, instead of the generic move cursor | to offer as a PR (as an option) |
| 0012-ui-egui-sheets-grid-scale-and-formula-bar | Grid drawn 1:1 (`DISPLAY_SCALE` 0.8 → 1.0) so 100 px / 21 px cells and 10 pt text match Sheets; column header 23 px, row header at least 44 px; plain arrow cursor over cells (cross only on the fill handle); `Tokens.sheets_formula_bar` hides ✕/✓ and the Name Box/field boxes | scale: StarSheets only; formula-bar flag: offer as an option |
| 0011-ui-egui-sheets-shortcuts | `keymap::lookup_sheets`: Google's shortcuts where they differ from Excel's, checked first with `sheets_keys` | to offer as a PR (as an option) |
| 0010-engine-new-sheet-sizes | `Sheet::blank`: sheets the user creates (new workbook, add sheet) get 100 px columns / 21 px rows; files keep their own sizes | StarSheets only |
| 0006-model-sheets-defaults | New workbooks use Roboto 10 pt instead of Calibri 11. Row/column sizes deliberately NOT changed here: the xlsx writer/reader treat `DEFAULT_COL_WIDTH`/`DEFAULT_ROW_HEIGHT` as the file default, so changing them would misread Excel files | StarSheets only (not for upstream) |

## Log
| Date | From → to | Notes |
|---|---|---|
| 2026-11-21 | → v0.19.1 | first pin |
