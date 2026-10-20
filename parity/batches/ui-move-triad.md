# Parity batch: ui-move-triad

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## move copy fields (P1)
- key: `ui:dialogs:move-copy-fields`
- Fusion: Move/Copy dialog: Move Object (Bodies/Components/Faces/Sketch objects), Move Type (Free/Translate/Rotate/Point to Point/Point to Position), triad manipulator with arrows, rings and planes, Create Copy.
- FissionOxide: MOVE / ROTATE is a form: Bodies, X/Y/Z distance, Rotate about (axis dropdown), Angle (defaults to 360 deg). Missing Fusion's Move Object type (bodies / components / faces / sketch objects), Move Type (Free move / Translate / Rotate / Point to Point / Point to Position), Create Copy, and the triad manipulator (see ui:canvas:triad-manipulator).
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/dialogs/solid.rs`, `crates/ui/src/forms.rs`, `crates/ui/src/viewport.rs`

## triad manipulator (P1)
- key: `ui:canvas:triad-manipulator`
- Fusion: Move and similar commands show a triad with arrows, rotation rings and plane squares; dragging shows the value in a box that accepts typing.
- FissionOxide: Move/Copy shows no triad. Fusion's Move draws arrows, rotation rings and plane squares at the selection; dragging one shows the value in a box you can type into. Only single-arrow manipulators exist (extrude/press pull distance).
- Start here: `crates/ui/src/viewport.rs`, `crates/ui/src/preview.rs`
