# Changelog

Changes that people using StarSheets would notice, newest first. Versions follow
[semantic versioning](https://semver.org): while we're below 1.0, the middle number goes up for
new features and the last one for fixes.

## [Unreleased]

### Changed
- Tooltips in the house style: dark grey with white text.
- The formula bar stays one line (long text runs on) until you expand it; bigger fx icon.
- No lines above and below the toolbar.
- Merging cells where only one holds a value keeps that value, with no warning.
- GridCraft's own dialogs (Insert link, Paste special, Find and replace…) have Sheets-style titles:
  top left with ✕, no grey title band.
- Dragging a sheet tab: the tab follows the pointer, the others slide aside, and it springs into
  place on release.
- Clicking anywhere on a note starts editing it.
- The sheet tab colour menu uses the same colour menu as the toolbar (custom colours included).
- Removed the Amatic SC font.
- Drop-downs in the conditional formatting and data validation panels show every choice at once;
  lists that don't fit the window scroll with a solid, always-visible scroll bar.
- The font menu shows each font's name in that font, scaled so the names look the same size.
- Right-click on a cell with a note offers Edit note and Delete note.
- Insert note writes straight into the note card (no separate box).

### Fixed
- Previewing a format on a blank sheet no longer saves an empty file (only values, formulas,
  notes, charts and pictures count as content).
- Side panels scroll when they are taller than the window, with a visible scroll bar.
- Note cards can be resized with their corner grip.
- "Today" in the date picker puts today's date in.
- Renaming a sheet tab: Enter keeps the name, Esc cancels.

- One house style for every dialog, menu, card and panel, following Google Sheets: 24 px dialog
  padding, pill-shaped buttons, 36 px fields, padded menus (docs/house-style.md).

### Added
- Format › Number › **Custom number formats**, after Google Sheets: a code box with Apply, a
  sample of the selected cell, and ready-made formats; the selection shows it as you type.
- **Chart editor**, after Google Sheets: a side panel with Setup (chart type, stacking, data range,
  X-axis, series, switch rows/columns) and Customise (colours, smooth lines, doughnut hole and
  rotation, chart and axis titles, series colour, trendline, data labels, legend, gridlines).
  Changes show on the chart straight away. Opens on Insert › Chart, a double-click on a chart or
  "Edit chart" in its right-click menu.
- Format › Merge cells › **Merge vertically**, and Google's "Heads up!" warning when merging would
  drop values (with "Don't show this again for 5 minutes"). Merge options that can't apply to the
  selection are greyed out.
- Text and fill colour menus like Google Sheets': a tick on the current colour, the standard
  colours, your own custom colours (remembered) with a full picker (colour square, hue slider,
  hex and RGB boxes), and a shortcut to conditional formatting.
- Notes like Google Sheets': Insert › Note, a white card beside the cell when you hover over or
  select it, edit it in place, and drag its corner to resize it. Comments are gone for now:
  notes cover what they were used for.
- A calculation preview while you type a formula, as in Google Sheets: a small bubble shows the
  result so far, even for an unfinished formula, or of just the part you select.

- **File › Settings**: your locale (how typed dates are read: 05/08 is 5 August in the UK), default
  date, time and number formats with examples, and the display language for dialogs. The default
  locale is now English (United Kingdom).
- Format › Number › **Custom date and time**, like Google Sheets': build a format from parts
  (day, month, year, hour…, each with its kinds), pick from ready-made ones, and see it on the
  selected cells as you go.
- Conditional formatting: a custom formula over whole columns, like Google's `=$A:$A=12` on
  A1:F13, now colours whole rows.

### Fixed
- The screen no longer flashes when you change a cell's font size.
- Copied cells no longer get an outline.
- Ctrl+\ (clear formatting) works on UK and other keyboards whose \ key sits next to Z.

## [0.1.0] - 2026-11-21

The first public preview. StarSheets is still early: please expect rough edges, and do tell me
about them.

### Looks and feels like Google Sheets
- Google Sheets' layout: title you can click to rename, menu bar, one-row toolbar, formula bar,
  sheet tabs with colours, drag to reorder, and right-click menus for cells, rows, columns and tabs.
- Google Sheets' sizes, fonts and keyboard shortcuts, and your own mouse pointer.
- 29 fonts, including free look-alikes for Arial, Calibri, Cambria, Comic Sans, Courier New,
  Georgia, Impact, Times New Roman, Trebuchet MS and Verdana. Each one loads the first time a sheet uses it.

### Working with data
- Conditional formatting in a side panel: all of Google's conditions (including "date is today /
  in the past week…"), preset styles and colour scales, previewed live on the sheet as you choose.
- Data validation in a side panel: drop-down lists with coloured items, text, number and date
  rules, tick boxes and custom formulas, with help text and "warn" or "reject". A date picker for
  date cells.
- Filters with Google's filter menu, links that you can type or paste, and named ranges in formula
  suggestions.
- Google Sheets formulas: `QUERY` (select, where, group by, pivot, order by, limit, label,
  format), `ARRAYFORMULA`, and `{…}` to put ranges side by side or one under another.

### Files (self-hosted)
- Your spreadsheets are ordinary .xlsx files in a folder on your server. A sheet saves itself a
  moment after each change; closing the tab saves at once.
- A file window with folders: new folder, rename, copy, cut and paste, drag to move, search, a bin
  you can restore from, and version history (the last six saves).
- Importing .xlsx and .csv files, downloading as .xlsx, PDF, HTML or CSV.

### Self-hosting
- One Docker image (`ghcr.io/domeshifter/starsheets`) with the app and its file storage.
- A password login, on by default (a random password is printed on first start if you don't set one).

[Unreleased]: https://github.com/DomeShifter/StarSheets/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/DomeShifter/StarSheets/releases/tag/v0.1.0
