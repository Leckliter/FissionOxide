# Parity batch: ui-reverify-sketch-4

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 4) re-checking the sketch area's
`fixing-merged` keys in the rebuilt app (`main` at `ff53fda7`). Three are reopened keys and one is
new; every row is already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for
this batch); read each row's `history` before starting.

## Project / Include dialogs: enabled rows and label column (P2)
- key: `ui:sketch:project-include-menu`
- Fusion (seen 2026-10-22, mm design): Project To Surface and Intersection Curve have "Projection Link" enabled and checked; Spun Profile has Projection Link, Project Axis and Add Centerline all enabled and checked, Tolerance "0.01 mm" enabled, Flip Result an icon toggle. Checkbox rows carry the label in the label column with the box in the value column.
- FissionOxide: Projection Link greyed in Project To Surface and Intersection Curve; Spun Profile's Project Axis and Tolerance ("0 mm") greyed and Flip Result a checkbox; checkbox labels sit beside the box in the value column.
- Start here: `crates/ui/src/forms.rs:3836` (the `Input::CheckOn { key: "associative", label: "Projection Link" }` rows at 3836 / 3919 / 3935 / 3956 / 3974), `crates/ui/src/dialogs.rs:6460` (Spun Profile rows), `crates/ui/src/dialogs.rs:4305` (`Self::check`).

## Distance and Angle chamfer leaves no distance dimension (P3)
- key: `ui:sketch:chamfer-angle-dimension`
- Fusion: a Distance and Angle chamfer leaves a distance dimension along the first line and an angle dimension between the chamfer and that line.
- FissionOxide: on an H/V corner only the "45.0 deg" angle dimension shows; no distance dimension, even zoomed in. A "Constraints and/or dimensions were removed during operation" toast shows while applying it, so the distance may be added and then dropped.
- Start here: `crates/sketch/src/edit.rs:528` (`chamfer_angle`) and the toast's caller.

## 3D Sketch: edge snaps and the triad (P2)
- key: `ui:sketch:3d-sketch-input`
- Fusion: with the palette's 3D Sketch on, the sketch tools snap to model vertices and points along model edges in 3D, a 3D triad moves a point along an axis, and the other tools draw in 3D too.
- FissionOxide: a Line ends on a model vertex off the plane (good); points along edges, the triad, the vertex hover highlight and the other tools are still missing (the fixer listed these as not done).
- Start here: `sketch.line3d` in `crates/engine/src/commands.rs` and the 3D pick in `crates/ui/src/viewport.rs` (test `three_d_sketch_line_takes_a_model_vertex_off_the_plane`).

## Circular pattern: centre pick prompt and status (P2)
- key: `ui:sketch:circular-pattern-center-status`
- Fusion: after the objects pick the next canvas pick goes to Center Point; the status bar then reads "Sketch Point | X: 0.00 Y: 0.00 Z: 0.00 in".
- FissionOxide: the pointer says "Select the center point" but a click on a sketch point goes into Objects (status "Multiple selections | 3") until Center Point's Select is clicked; after the centre pick the status reads "1 Sketch Line 1 Constraint".
- Start here: `crates/ui/src/dialogs.rs:3112` (`Kind::SkCircPattern` prompt) and the field focus after the first pick; `crates/ui/src/panels.rs:3190` (`selection_text`).
