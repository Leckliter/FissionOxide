# Parity batch: ui-utilities-addins-make

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 16) in an unsaved Part Design document in each app, both windows
908 px wide, FissionOxide built from `main` at `c94d76ac`, on the UTILITIES tab (MAKE, ADD-INS,
UTILITY, INSPECT, SELECT). FissionOxide's plugins (`.foxplugin`, LumenArt) are its own add-in system:
keep it, but present it the way Fusion presents scripts and add-ins. See
`parity/reports/2026-10-22-plastic-manage-utilities.md`.

## Scripts and Add-Ins window (P1)
- key: `ui:utilities:scripts-addins-window`
- Fusion: ADD-INS > Scripts and Add-Ins... opens a large window "Scripts and Add-Ins" (Fusion icon, X at the top right, about 800 x 510 px) in three columns. Left: a Search box; a "Browse" group (with a "-" collapse) holding "Favorites" (star) and "All scripts and add-ins"; a "Filters" group with collapsible "Type" (Scripts, Add-Ins), "Status" (Running, Run on startup) and "Source" (Installed, From device, Created by me, Fusion, Samples) checkbox lists. Middle: a toolbar ("+" with a dropdown arrow, pencil, folder, a greyed unlink icon) over a table with columns Name (sorted, arrow up), Run (a toggle switch for add-ins, blue when running; a play triangle for scripts), Run on Startup (a checkbox for add-ins only) and Version. The "+" menu lists "Download add-in from marketplace", "Script or add-in from device", "Create script or add-in". Right: "Select an item to see details here" until a row is picked; then the item's name with a star, a picture, and collapsible "Information" (Author, Version, Supported OS, Programming Language), "Description" and "Full Path" groups and an "Open File Location" link. A blue "Close" button at the bottom right; Escape closes the window.
- FissionOxide: a small floating "Plugins" window at the top right (X): "Install from file..." and "Data hub..." buttons, "You can also drop a .foxplugin file on the window.", and one row per plugin (checkbox, "LumenArt 1.0.0 built in", a "Details" button that expands the description, id, path, link and a bulleted command list in place).
- Start here: `crates/ui/src/plugins_ui.rs:98` (open_window), `crates/ui/src/panels.rs:823` (Cmd::Plugins). Map plugins to the add-in rows (Run toggle = enabled, Run on Startup, Version, details pane), and put "Install from file" and "Data hub" under the "+" menu.

## Scripts and Add-Ins shortcut and ADD-INS list (P2)
- key: `ui:utilities:scripts-addins-shortcut`
- Fusion: the ADD-INS dropdown reads "Scripts and Add-Ins...  Shift+S" (with its icon) and "Design and Make Marketplace" (no icon). Shift+S in the canvas opens the Scripts and Add-Ins window. The ribbon tooltip title is "Scripts and Add-Ins (Shift+S)". At a 908 px window the ADD-INS panel shows one icon (Scripts and Add-Ins).
- FissionOxide: no shortcut hint on the entry, a shopping-bag icon on Design and Make Marketplace, Shift+S opens the "Design Shortcuts" toolbox (it is reserved for the S toolbox), the tooltip title has no shortcut, and the panel shows two icons at 908 px.
- Start here: `crates/ui/src/menus.rs:983` (ADD_INS), `crates/ui/src/customize.rs:620` (Shift+S reserved for the S toolbox), the default key map in `crates/ui/src/customize.rs`.

## MAKE > 3D Print opens a Save As for STL (P1)
- key: `ui:utilities:make-3d-print`
- Fusion: UTILITIES > MAKE > 3D Print (ribbon button or dropdown) opens the same docked "3D PRINT" dialog as File > 3D Print: Preparation Type "Manufacture" with its text, OK and Cancel.
- FissionOxide: File > 3D Print opens the docked 3D PRINT dialog, but the MAKE button and its dropdown entry (and MESH > EXPORT > 3D Print) open the Windows "Save As" dialog for "Untitled.stl" at once.
- Start here: `crates/ui/src/panels.rs:761` (Cmd::Print3d runs app.export("stl")); route it to `crates/ui/src/capture.rs:217` (open_print), and update the test at `crates/ui/src/menus.rs:1821`.

## Plugin details show boxes for the arrow glyph (P2)
- key: `ui:utilities:plugin-details-glyphs`
- Fusion: (FissionOxide-only text; Fusion's add-in details show plain text.) No missing-glyph boxes anywhere in the Scripts and Add-Ins window.
- FissionOxide: in the Plugins window's Details, each command line reads "Clean Up Sketch (LumenArt □ PREPARE)" with a hollow box where the "→" between tab and panel should be: the UI font has no glyph for it.
- Start here: `crates/ui/src/plugins_ui.rs:384` and `crates/ui/src/plugins_ui.rs:157` (the "{tab} → {panel}" format); use a character the bundled font has (">" or "›" if present) or add the glyph to the font fallback.
