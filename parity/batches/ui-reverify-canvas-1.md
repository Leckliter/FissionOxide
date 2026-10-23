# Parity batch: ui-reverify-canvas-1

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 3) re-checking the canvas, manip and
timeline areas' `fixing-merged` keys in the rebuilt app (`main` at `afddd332`). Three are reopened
keys (the fix landed but part of Fusion's behaviour is still missing) and one is new
(`ui:manip:draft-drag-hang`). The rows are already in `parity/ui-features.jsonl` (do not run
`ledger_from_batches.py` for this batch); read each row's `history` before starting.

## Draft angle drag hangs the app (P1)
- key: `ui:manip:draft-drag-hang`
- Fusion: dragging Draft's angle handle to an angle that fails only stops the preview; the dialog stays usable and Cancel works.
- FissionOxide: on an extruded box with a 3 mm fillet on one vertical edge and a hole on the top face that breaks out through the left side face, Draft with pull direction = top face, Faces = that left face, Tangent Chain on: dragging the angle disc from 0 to 40 deg showed "1 error(s) draft: the draft failed", and on release the window went "Not Responding" with the CPU busy for over two minutes until it was killed.
- Start here: the Draft preview / compute path for a dragged angle (`crates/ui/src/dialogs.rs`, the draft command in `crates/engine/src/commands.rs` and the kernel draft call); make a failing draft return an error instead of looping, and keep recomputes off the UI thread or bounded.

## Error window icons overlap their labels (P2)
- key: `ui:timeline:error-dialog`
- Fusion: the "Error" window's tree puts each icon before its text: the feature row (icon with a red outline, then "Chamfer1"), "Compute Failed" with a red circled X before it, then the message.
- FissionOxide: the window, title, tree and Close match, but Fillet1's feature icon is drawn over the "F" of "Fillet1" and the red circled X over the "C" of "Compute Failed".
- Start here: the tree rows in `crates/ui/src/error_dialog.rs`; give the icons their own width before the label.

## Extrude's hovered profile turns almost white (P2)
- key: `ui:canvas:command-pick-colors`
- Fusion: in Extrude, hovering a profile draws its outline black and lightens its fill to a mid grey (192,197,193); a picked profile fills (83,138,210) with a red outline.
- FissionOxide: Fillet's colours match now; Extrude's unhovered profile is a light blue fill and the hovered one is almost white (brighter than the body's top face) with a black outline.
- Start here: `crates/engine/src/scene.rs:66` (`PROFILE_HOVER = lighten(PROFILE_COLOR, HOVER_LIGHTEN)`); the hovered fill wants Fusion's (192,197,193) rather than a lightened profile blue.

## Create Sketch: whole-canvas plane hover and grid preview (P1)
- key: `ui:canvas:create-sketch-plane-grid-preview`
- Fusion: while Create Sketch (and Box, Cylinder, Sphere, Torus, Coil) waits for "Select a plane or planar face", every pointer position hovers one origin plane (the infinite plane the pointer ray meets first), and that plane's sketch grid with its coordinate labels is previewed across the whole canvas (seen again on 2026-10-22: the vertical grid with 25 / 50 / 75 labels).
- FissionOxide: a pointer below the horizon now hovers XY (blue-grey) far from the squares, but no grid is previewed, and a pointer in the upper canvas hovers nothing.
- Start here: the plane-pick hover in `crates/ui/src/origin.rs` / `crates/ui/src/viewport.rs` (the ray test against the three planes) and the sketch grid drawing used inside a sketch (`crates/engine/src/scene.rs`), drawn for the hovered plane while the pick is pending.
