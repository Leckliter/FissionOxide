# Parity batch: ui-history-leftovers

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## status bar units (P2)
- key: `ui:chrome:status-bar-units`
- Fusion: The status-bar selection readout (area, length) uses the design's units.
- FissionOxide: With the design in inches, selecting a profile still shows '1 Profile | Area: 800.00 mm^2' in the status bar. #119 converted dialogs, manipulators and measure but not this readout.
- Start here: `crates/ui/src/panels.rs`
## context menu (P2)
- key: `ui:timeline:context-menu`
- Fusion: Right-click: Edit Feature, Edit Profile Sketch, Suppress, Delete, Roll History Marker Here, Create Group.
- FissionOxide: Verified on parity2 (b2bb8fe) (#102): Edit Profile Sketch on sketch-based features and Find in Browser added. Still missing Fusion's Create Group / Move to Group.
- Start here: `crates/ui/src/panels.rs`

## undo history (P2)
- key: `ui:chrome:undo-history`
- Fusion: Undo/Redo dropdowns on the quick-access toolbar list the operations to step back to.
- FissionOxide: Verified on parity3 (main 8a0ba726), #108: the Undo caret lists steps, but names are coarse: six identical 'Edit Sketch6' entries for a rectangle, dims, fix and line, and a bare 'Edit' for the unit change. Fusion names each step by its command (Line, Dimension, Fix, Change Units).
- Start here: `crates/engine/src/history.rs`
