# Parity batch: ui-browser

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## context menu (P2)
- key: `ui:browser:context-menu`
- Fusion: Right-click on a node gives a node-specific menu (Rename, Delete, Create Components from Bodies, ...).
- FissionOxide: Body menu: Move/Copy, Appearance, Properties, Save As Mesh, Delete, Remove, Rename, Show/Hide, Isolate, Find in Window. Fusion's also has Create Components from Bodies, Create Selection Set, Physical Material, Copy, Cut, Selectable/Unselectable, Opacity Control, Display Detail Control.
- Start here: `crates/ui/src/panels.rs`, `crates/ui/src/menus.rs`

## rename dblclick (P2)
- key: `ui:browser:rename-dblclick`
- Fusion: Double-click or slow-click a name to rename it in place.
- FissionOxide: Double-clicking a body name only selects it. Fusion opens an in-place text box (Rename is only in the right-click menu here).
- Start here: `crates/ui/src/panels.rs`

## drag reorder (P2)
- key: `ui:browser:drag-reorder`
- Fusion: Bodies and components can be dragged between folders and components.
- FissionOxide: No drag and drop in the browser tree (no dnd handling in panels.rs). Fusion drags bodies between components and folders.
- Start here: `crates/ui/src/panels.rs`
