# Live editing

Goal: open the same spreadsheet in several tabs, or share it with other people, and see every
change everywhere as it happens, like Google Sheets. This is a core feature, so the rest of
StarSheets adapts to it rather than the other way round.

## How it works

| Part | What it does |
|---|---|
| Server (`crates/server`) | Holds the true copy of each open file and runs the same GridCraft engine as the browser. Takes changes in the order they arrive, applies them, numbers them and sends them to every tab. Saves the .xlsx itself, a couple of seconds after things go quiet |
| Tab | Shows your own change at once and sends it to the server. When someone else's change arrives before yours is confirmed, it goes back to the last confirmed copy, applies theirs and then yours again (cheap: copies of the workbook are shared, not duplicated) |
| A change | One GridCraft command with its sheet, selection and settings. Only commands that change the file are sent; scrolling, zoom and your selection stay in your tab |
| Safety checks | Every so often the server sends a fingerprint of the file; a tab that doesn't match reloads its copy. Tabs and server must run the same build, or the tab reloads |

Why this design, and what was rejected: [decisions.md](decisions.md) (2026-11-22).

## Rules

- **Undo** only undoes your own changes. If someone has since changed the same cells, StarSheets
  says so instead of undoing over their work.
- **NOW(), RAND() and similar** are worked out in each tab, as in Google Sheets.
- **Accounts** are local to your server (username and password, hashed with Argon2). No sign-in
  through Google or other companies.
- **Sharing** per file: owner, can edit, can view, and an optional "anyone with the link".
- Changes from other people never go through the clipboard, files on their computer or macros:
  a paste carries its own contents, and the server only accepts commands on an allow-list.

## Steps

| Step | What | Done when |
|---|---|---|
| 0 | Rust server with everything server/files.py did | The app works unchanged against it; same answers as the old server |
| 1 | Live sync between tabs, saving on the server | Two tabs editing one file stay identical; random command runs give identical workbooks |
| 2 | Accounts, names and colours, other people's cursors | Two people see each other's cursors |
| 3 | Undo of your own changes only | Undo never wipes someone else's change |
| 4 | Share dialog, view-only mode | A viewer can't edit; the owner can change roles |
| Later | Desktop apps run the same server for local files; changes kept while offline; comment threads | |

Progress is ticked off in [ROADMAP.md](../ROADMAP.md).
