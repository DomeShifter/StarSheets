# Architecture

```
browser
└─ app (crate `starsheets`, WebAssembly via trunk)
   ├─ web.rs     eframe web runner, file open/download, preferences   (adapted from GridCraft)
   ├─ shell.rs   title row, menu bar, toolbar: data tables → command ids
   ├─ theme.rs   Sheets palette installed over GridCraft's tokens
   └─ GridCraft  SheetApp (ui-egui): formula bar, grid, sheet tabs, dialogs
                 Session (engine): every action is a command `Session::execute(id, params)`
                 calc, formula, functions, model, xlsx, chart …
```

Each frame, `shell.ui` adds its top panels first. Then `SheetApp::ui` draws the formula bar, tabs and grid below them. GridCraft's own title bar, File tab and ribbon are turned off through `Chrome`.

Later phases add a small server (Rust, same engine) for the file list, autosave, version history and sharing. Design notes for that go here when phase 2 starts.
