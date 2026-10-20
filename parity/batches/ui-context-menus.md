# Parity batch: ui-context-menus

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## empty canvas (P2)
- key: `ui:context-menus:empty-canvas`
- Fusion: Right-click empty canvas: ring with Repeat/Undo/Redo/Sketch/Press Pull/Hole/Delete/Move; list with Show All, Unisolate, Pan, Zoom, Orbit, Workspace.
- FissionOxide: Empty-canvas list: Pan, Zoom, Constrained Orbit, Fit F6, Home View, Measure I, Workspace, recent commands. Missing Fusion's Show All, Unisolate (when isolated), Show/Hide all bodies. Clicking anywhere in a slot's direction runs that slot, as in Fusion.
- Start here: `crates/ui/src/panels.rs`, `crates/ui/src/marking.rs`, `crates/ui/src/menus.rs`

## sketch in browser (P2)
- key: `ui:context-menus:sketch-in-browser`
- Fusion: Right-click a sketch in the browser: Edit Sketch, Redefine Sketch Plane, Slice Sketch, Show Profile, Show Dimensions, Look At, Save As DXF, Delete.
- FissionOxide: Sketch row menu has only Edit Sketch, Rename, Delete. Fusion adds Redefine Sketch Plane, Slice Sketch, Show/Hide Profile, Show/Hide Dimensions, Look At, Save As DXF, Find in Window, Show/Hide. Also: browser rows' right-click area runs to the panel's right edge (as in Fusion), fine.
- Start here: `crates/ui/src/panels.rs`, `crates/ui/src/marking.rs`, `crates/ui/src/menus.rs`

## sketch curve (P2)
- key: `ui:context-menus:sketch-curve`
- Fusion: Right-click a sketch curve while sketching: Normal/Construction, Fix/UnFix, Delete, Trim, Dimension.
- FissionOxide: In a sketch the ring has Repeat, Line, Rectangle, Circle, Undo, Redo, Delete and the list has Finish Sketch, Normal/Construction X, Look At, Fit F6, Measure I. Missing Fusion's Fix/UnFix (and Dimension / Trim) for a picked curve.
- Start here: `crates/ui/src/marking.rs`

## origin plane (P2)
- key: `ui:context-menus:origin-plane`
- Fusion: Right-click an origin plane: Create Sketch, Offset Plane, Show/Hide, Look At.
- FissionOxide: Right-click XZ in the browser shows only Create Sketch. Fusion: Create Sketch, Offset Plane, Show/Hide, Look At, Find in Window.
- Start here: `crates/ui/src/panels.rs`, `crates/ui/src/marking.rs`, `crates/ui/src/menus.rs`
