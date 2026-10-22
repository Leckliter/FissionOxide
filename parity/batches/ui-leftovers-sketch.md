# Parity batch: ui-leftovers-sketch

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

These rows were recorded by probers and fixers between 2026-10-21 and 2026-10-22 but never got a
batch (most were found by a fixer while working on a neighbouring item). The coordinator grouped
them on 2026-10-22; the wording below is the ledger row's own. Re-read the row in
`parity/ui-features.jsonl` (its `history`) before starting, and where a row says Fusion's side was
not observed, check it in Fusion's documentation and say so in your ledger note.

## vertical on a horizontal line (P2)
- key: `ui:sketch:vertical-on-horizontal-line`
- Fusion: Applying Vertical to a line that is already constrained Horizontal is refused as a conflict; the line stays.
- FissionOxide: Vertical on a line that already has a Horizontal constraint (or the other way round) is accepted: the solver satisfies both by collapsing the line to a point. Fusion refuses the second constraint as conflicting. Found while testing ui:sketch:horizontal-conflict (crates/engine/src/commands.rs constrain, crates/sketch/src/solve.rs): a zero-length line satisfies every residual, so the conflict needs a check of its own.
- Start here: `crates/engine/src/commands.rs`, `crates/sketch/src/solve.rs`.

## dimension placement kept (P2)
- key: `ui:sketch:dimension-placement-kept`
- Fusion: A placed sketch dimension stays where the placement click put it (its offset from the geometry and side), and can be dragged later.
- FissionOxide: Found while fixing ui:sketch:dimension-placement-flow: the engine's dimension constraints carry no label offset.
- Start here: `crates/ui/src/viewport.rs`, `crates/sketch/src/lib.rs`.

## distance and angle chamfer dimensions (P3)
- key: `ui:sketch:chamfer-angle-dimension`
- Fusion: A Distance and Angle Chamfer leaves a distance dimension along the first line and an angle dimension between the chamfer and that line.
- FissionOxide: The Distance and Angle chamfer leaves two chamfer-distance dimensions (the second computed from the angle); an angle dimension would need its quadrant pinned to the line's direction.
- Start here: `crates/sketch/src/edit.rs`.

## blend curve fully constrained (P2)
- key: `ui:sketch:blend-curve-constrained`
- Fusion: A blend curve is drawn black (fully constrained) with no fit-point ticks: its shape follows from the two end conditions and adds no degrees of freedom.
- FissionOxide: The blend's eight interior quintic points are free sketch points (drawn blue with ticks, 14 degrees of freedom); the solver would need to derive them from the end frames (quintic::blend_points) instead of solving for them.
- Start here: `crates/sketch/src/freeform.rs`, `crates/sketch/src/solve.rs`.

## sketch marking menu ring (P2)
- key: `ui:sketch:marking-ring-in-sketch`
- Fusion: Right-clicking in a sketch (on a glyph or geometry) opens the ring with Repeat <last command>, Delete, Press Pull, Redo, Hole, a Sketch slot, Move/Copy and Undo, and the list OK (Return), Cancel (Esc), Create Selection Set, Find in Browser, Find in Window.
- FissionOxide: FissionOxide's in-sketch ring is Repeat, Delete, Redo, Circle, Dimension, Rectangle, Undo, Line; Fusion's (per prober 5, right-clicking a glyph) is the SOLID ring with Press Pull, Hole, Move/Copy and a Sketch slot. Needs a prober's confirmation.
- Start here: `crates/ui/src/marking.rs`.

## 3D sketch input (P2)
- key: `ui:sketch:3d-sketch-input`
- Fusion: With the palette's 3D Sketch on, the sketch tools accept points off the sketch plane (snapping to model vertices and edges in 3D, with a 3D triad to move along an axis) and the lines drawn are 3D sketch lines.
- FissionOxide: The 3D Sketch checkbox toggles and is remembered (ui-sketch-palette-options) but the sketch tools still take every point on the sketch plane; no out-of-plane input or triad.
- Start here: `crates/ui/src/viewport.rs`.

## Measure from the dimension box fills it (P3)
- key: `ui:sketch:dim-box-measure-fill`
- Fusion: Picking Measure in a dimension box's menu opens Measure; the measured length is written into the box when the Measure command finishes.
- FissionOxide: Measure opens from the box menu but its readout has to be typed back by hand.
- Start here: `crates/ui/src/viewport.rs`.

## project specified edge (P2)
- key: `ui:sketch:project-specified-edge`
- Fusion: With PROJECT's Specified Entities filter, picking a body edge projects that edge alone (purple, linked to the edge).
- FissionOxide: An edge pick under Specified Entities projects the whole body's silhouette; sketch.project takes only {body, face?}.
- Start here: `crates/ui/src/dialogs.rs`, `crates/engine/src/commands.rs`.
