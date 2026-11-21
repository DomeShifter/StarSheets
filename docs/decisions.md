# Decisions

One line per decision: what, why, what we rejected. Newest first.

| Date | Decision | Why | Rejected |
|---|---|---|---|
| 2026-11-21 | Roboto is the default font (bundled, OFL) and leads the interface and sans-serif cell fonts | Owner's choice; web builds have no system fonts, so it must be bundled | Arial (Sheets' default, not openly licensed) |
| 2026-11-21 | Sheets cell behaviour is one switch (`sheets_keys`) in a GridCraft patch, not a fork of the key handling | Small diff that's easy to rebase and to offer upstream as an option | Copying GridCraft's keyboard module |
| 2026-11-21 | Toolbar icons are Material Symbols SVGs drawn through egui's SVG loader | GridCraft rebuilds its font list often, so an icon font added by us would be dropped; images are independent | Icon font; copying GridCraft's Excel-style icons |
| 2026-11-21 | Build in CI (GitHub Actions); the home server only hosts the finished site | Public repos get free CI; building ~270k lines of Rust needs more memory than the small server has | Building on a laptop; building on the server |
| 2026-11-21 | Web only | That's how it will be used; one target keeps the work small | Desktop app (GridCraft has one; we could add it later) |
| 2026-11-21 | Menus and toolbar are data tables of GridCraft command ids, checked by a test | Adding an item is one line; an upstream rename fails the test instead of a silent dead button | Hand-written UI per item |
| 2026-11-21 | Patch queue (`patches/`) applied to a pinned upstream tag; every patch is also offered upstream | Upstream changes about 40 times a day; small, named patches are easy to re-apply on each update | Forking the GridCraft repo (can't keep up); copying the crates in (loses upstream fixes) |
| 2026-11-21 | Keep GridCraft's egui UI and replace only the chrome | Grid, editing, fill, clipboard and dialogs already work; GridCraft keeps its UI layer separate from the engine | Rewriting the UI in TypeScript (months of work to reach the same grid) |
| 2026-11-21 | Name StarSheets; licence MIT OR Apache-2.0 | Owner's choice; licence matches upstream so code can move both ways | |
