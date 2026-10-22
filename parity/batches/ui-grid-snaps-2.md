# Parity batch: ui-grid-snaps-2

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 7) in the navigation bar's Grid and Snaps menu of each app in the
3D view (no sketch open); FissionOxide built from `main` at `42dec181`. The menu's six entries and
their order already match (Layout Grid, Layout Grid Lock, Snap to Grid, Grid Settings, separator,
Incremental Move, Set Increments). Everything is in `crates/ui/src/navbar.rs` (`grid_menu` at
line 366).

## grid menu defaults (P2)
- key: `ui:navbar:grid-menu-defaults`
- Fusion: In the 3D view Layout Grid is off by default (no grid on the ground; turning it on draws the grid with the red X and green Y axis lines), Layout Grid Lock is greyed while the grid is off and enabled (checked) once it is on, Snap to Grid is off, Incremental Move is on by default; Grid Settings and Set Increments carry icons.
- FissionOxide: Layout Grid is on by default (a grid in every 3D view), Layout Grid Lock is enabled while the grid is off, Incremental Move is off by default, and no entry has an icon.
- Start here: `crates/ui/src/navbar.rs:366` (`grid_menu`: Layout Grid Lock `add_enabled(app.show_grid, ...)`, icons on the two command rows), the `show_grid` and `increments.on` defaults in `crates/ui/src/lib.rs`, `crates/engine/src/scene.rs` (the 3D grid is drawn only with Layout Grid on; the sketch grid stays).

## grid settings dialog (P2)
- key: `ui:navbar:grid-settings-dialog`
- Fusion: Grid Settings opens a docked dialog "GRID SETTINGS" with radio rows Adaptive (on) and Fixed, a "Reference Numbers" checkbox (on), and OK / Cancel; Escape cancels it.
- FissionOxide: Grid Settings is a submenu with Adaptive / Fixed radios and a "Spacing" drag value (greyed while Adaptive), no Reference Numbers, no OK / Cancel.
- Start here: `crates/ui/src/navbar.rs:375` (the `menu_button("Grid Settings")`): open a docked dialog like the feature dialogs with the three rows (Fixed reveals the spacing field) and OK / Cancel; Reference Numbers toggles the grid's value labels (`ui:sketch:grid-axis-labels`).

## set increments dialog (P2)
- key: `ui:navbar:set-increments-dialog`
- Fusion: Set Increments opens a docked dialog "SET INCREMENTS" with radio rows Adaptive (on) and Fixed and OK / Cancel (Fixed reveals the linear and angular increment fields); Escape cancels it.
- FissionOxide: Set Increments is a submenu with "Linear 1.0 mm" and "Angular 15.0°" drag values and no Adaptive / Fixed choice.
- Start here: `crates/ui/src/navbar.rs:393` (the `menu_button("Set Increments")`): a docked dialog with Adaptive / Fixed, the two fields under Fixed, OK / Cancel; `crates/ui/src/navbar.rs:59` (the increments model gains an adaptive mode).
