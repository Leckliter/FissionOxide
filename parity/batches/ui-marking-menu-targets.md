# Parity batch: ui-marking-menu-targets

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 (prober 20) right-clicking a body, a vertex, a sketch point, a construction
plane, a timeline item and inside a sketch in both apps (FissionOxide `main` at `379c2ae2`).
The ring is `ring_items` in `crates/ui/src/marking.rs:471`; slot `i` is drawn at `i * 45` degrees
clockwise from north (`:804`). What a right-click picks is decided where the menu opens
(`open` at `crates/ui/src/marking.rs:168`, `context` at `:201`, and the canvas pick in `crates/ui/src/viewport.rs`). See
`parity/reports/2026-10-22-startup-preferences-marking.md`.

Also reopened in this batch (an existing row, not added by the script): `ui:context-menus:face-list-contents`.
Fusion's face list starts with OK (Return) and Cancel (Esc); FissionOxide's face list has neither, and its
Properties carries an "I" hint Fusion's lacks. Start in `overflow_items`, `crates/ui/src/marking.rs:524`.

## Ring slots in Fusion's positions (P1)
- key: `ui:marking-menu:ring-slot-order`
- Fusion: around the centre (on a body, a vertex, a sketch point, a plane, empty canvas, and inside a sketch): N "Repeat <last command>", NE Press Pull, E Redo, SE Hole, S "Sketch v", SW Move/Copy, W Undo, NW Delete. On the SHEET METAL tab the SE slot is "Flange (Base/Edge/Contour)". The labels sit close: about 65 px left and right of the centre and 45 px above and below.
- FissionOxide: N Repeat, NE Delete, E Press Pull, SE Redo, S Hole, SW Sketch v, W Move/Copy, NW Undo: every slot after Repeat is one position clockwise of Fusion's, so a flick toward a Fusion slot runs a different command. The ring is wider (about 110 px left and right, 60 px up and down).
- Start here: the array returned by `ring_items`, `crates/ui/src/marking.rs:508`, should be `[repeat, east, Redo, south, sketch, Move, Undo, Delete]`; `RADIUS` at `:23`; update the slot test at `:1293`.

## Right-click on a selected body keeps the body (P2)
- key: `ui:marking-menu:selected-body`
- Fusion: with Body1 selected (from the browser), a right-click over the body keeps the body selected (status "Body1") and lists OK (Return), Cancel (Esc), Move to Group, Create Selection Set, Physical Material, Appearance (A), Texture Map Controls, Properties, Save As Mesh, Copy (Ctrl+C), Cut (Ctrl+X), Remove, Rename, Display Detail Control, Show/Hide (V), Selectable/Unselectable, Opacity Control >, Isolate, Find in Browser, Find in Window.
- FissionOxide: the right-click replaces the selection with the face under the pointer ("1 Face | Area") and shows the face list.
- Start here: the pick on right-click in `crates/ui/src/viewport.rs` (where the marking menu opens) and `overflow_items` in `crates/ui/src/marking.rs:524`; when the pointer is over a selected body, keep the selection and use the body list.

## Vertex marking menu (P2)
- key: `ui:marking-menu:vertex`
- Fusion: right-click on a body corner (status "Vertex | X: ... Y: ... Z: ... mm") selects the vertex and lists OK, Cancel, Create Selection Set, Rename, Show/Hide (V), Selectable/Unselectable, Find in Browser.
- FissionOxide: the status shows "Vertex | ..." on hover, but the right-click opens the empty-canvas list (Pan, Zoom, Constrained Orbit, Fit, Home View, Remove, Show All, Hide All Bodies, Measure, Workspace, 2-Point Rectangle, Create Sketch, Hole) and the vertex stays half-selected (a later Offset Plane read "1 Face 1 Vertex").
- Start here: `Context` and `context` in `crates/ui/src/marking.rs:186` and `:201` (add a vertex case) and `overflow_items` at `:524`.

## Construction plane marking menu (P2)
- key: `ui:marking-menu:construction-plane`
- Fusion: right-click on a construction plane (Plane1) selects it (status "Plane1") and lists OK, Cancel, Move to Group, Create Selection Set, Create Sketch, Offset Plane, Edit Construction Plane, Rename, Look At, Show/Hide (V), Find in Browser, Find in Window, Find in Timeline.
- FissionOxide: the status reads "Plane1" on hover but the right-click opens the empty-canvas list (Pan ... Extrude, 2-Point Rectangle, Create Sketch, Hole).
- Start here: `crates/ui/src/marking.rs` context for a `Sel::Plane` (the origin-plane case, `ui:context-menus:origin-plane`, already lists Create Sketch / Offset Plane / Show/Hide / Look At) and `crates/ui/src/construction.rs` for Edit Construction Plane.

## Ring and list inside a sketch (P2)
- key: `ui:marking-menu:in-sketch-ring`
- Fusion: with Sketch2 open for editing, a right-click on empty canvas showed the same ring as in 3D (Repeat greyed, Press Pull, Redo, Hole, Sketch v, Move/Copy, Undo, Delete) and the list OK (Return), Cancel (Esc), Pan, Zoom, Constrained Orbit, Set Orbit Center, Reset Orbit Center, Remove, Show All, Unisolate, Workspace >, Line (L), 2-Point Rectangle (R), Center Diameter Circle (C). Seen once; recheck on a new sketch before changing the ring.
- FissionOxide: the ring becomes Repeat, Delete, Redo, Circle, Sketch Dimension, Rectangle, Undo, Line and the list is Finish Sketch, Look At, Fit (F6), Measure (I).
- Start here: the `edit_sketch` branch of `ring_items`, `crates/ui/src/marking.rs:476`, and the sketch arm of `overflow_items`.

## Sketch point marking menu (P2)
- key: `ui:marking-menu:sketch-point`
- Fusion: right-click on a sketch point outside the sketch (status "Sketch Point | X: ... in") lists OK, Cancel, Create Selection Set, Edit Sketch, Break Link (greyed), Copy (Ctrl+C), Find in Browser, Find in Window; inside the sketch, on a projected point: OK, Cancel, Create Selection Set, Break Link, Copy, Find in Browser, Find in Window.
- FissionOxide: inside the sketch, on a rectangle corner: Finish Sketch, Fix/UnFix, Look At, Fit (F6), Measure (I).
- Start here: the sketch-point context in `overflow_items`, `crates/ui/src/marking.rs:524`.

## Selection readout stays at the canvas's lower right in a sketch (P2)
- key: `ui:canvas:readout-beside-palette`
- Fusion: in a sketch the readout ("Sketch Point | X: -2.974 Y: 0.50 Z: -0.259 in") stays at the canvas's bottom-right corner, below the SKETCH PALETTE, right-aligned.
- FissionOxide: in a sketch the readout moves left of the palette and is drawn over the navigation bar at the bottom centre ("Sketch Point | X: -17.45 ..." over the orbit / pan / zoom icons).
- Start here: `readout_anchor` in `crates/ui/src/panels.rs:2444` (and its test `the_selection_readout_keeps_left_of_the_palette` in `crates/ui/src/lib.rs:2554`); move left only when the palette actually covers the bottom-right corner, so `ui:sketch:status-readout-under-palette` (a palette pushed down by a dialog) keeps working.
