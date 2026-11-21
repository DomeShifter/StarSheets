# Changelog

Changes that people using StarSheets would notice, newest first. Versions follow
[semantic versioning](https://semver.org): while we're below 1.0, the middle number goes up for
new features and the last one for fixes.

## [Unreleased]

### Added
- Format › Merge cells › **Merge vertically**, and Google's "Heads up!" warning when merging would
  drop values (with "Don't show this again for 5 minutes"). Merge options that can't apply to the
  selection are greyed out.
- A calculation preview while you type a formula, as in Google Sheets: a small bubble shows the
  result so far, even for an unfinished formula, or of just the part you select.

### Fixed
- The screen no longer flashes when you change a cell's font size.
- Copied cells no longer get an outline.

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
