# Parity batch: ui-units-and-parameters

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## document units (P1)
- key: `ui:chrome:document-units`
- Fusion: Document Settings > Units: click the pencil to change the design's units; values in dialogs display in that unit.
- FissionOxide: Document Settings shows a fixed 'Units: mm' label (panels.rs:1042, hard-coded); designs have no unit setting, so values always display in mm. Fusion: hover the Units row, click the pencil, pick mm/cm/m/in/ft for the design, and every dialog and dimension displays in that unit (this file in Fusion is inches). PR #108 stores a default-units preference but has nothing to apply it to.
- Start here: `crates/ui/src/panels.rs`, `crates/model/src/lib.rs`

## parameters dialog (P2)
- key: `ui:manage:parameters-dialog`
- Fusion: Change Parameters: table of User and Model parameters with name, unit, expression, value, comment; + to add, favorites star, expressions live-update the model.
- FissionOxide: Change Parameters (via S) opens a small floating 'fx Parameters' window at the top left, over the toolbar, with a Name / Expression / Value / Comment table for user parameters and a New row. Missing Fusion's Model Parameters section (every feature's and sketch dimension's d1, d2... grouped by feature, editable), Favorites stars, units column and the filter box.
- Start here: `crates/ui/src/dialogs.rs`
