# Parity batch: ui-reverify-dialogs-4

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 4) re-checking the dialogs and inspect
`fixing-merged` keys in the rebuilt app (`main` at `ff53fda7`). Two are reopened keys and two are
new; every row is already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for
this batch); read each row's `history` before starting.

## Primitive drawing step: Coil handle and typed base sizes (P2)
- key: `ui:dialogs:primitive-drawing-extras`
- Fusion: Coil has a rotation handle on its preview; a primitive's base sizes are typed heads-up boxes (type, Tab, type, Enter finishes the base).
- FissionOxide: the temporary Sketch row, size boxes and grey preview with blue base edges now match; but Coil shows only height, section and diameter arrows (no rotation handle), and in Box typing 8, Tab, 8, Enter set only Length 8, left Width following the pointer (6.5 mm) and did not finish the base.
- Start here: `crates/ui/src/viewport.rs:5059` (the primitives' drawing step) and the Coil manipulators.

## Measure: status bar and a sketch-point pick (P2)
- key: `ui:inspect:measure-two-picks`
- Fusion: two picks read e.g. "1 Face 1 Vertex | Min Distance : 1.29 in" in the status bar; every pick (including a sketch point) gets its own Selection section and the Results row.
- FissionOxide: arrow, label, tags and sections now match, but two face picks read "Multiple selections | 3" (it counted an earlier sketch-point pick); a sketch point picked second read "1 Face 1 Sketch Point | Min Distance : 0.00 mm" in the status but added no Selection 2 and no Results.
- Start here: `crates/ui/src/panels.rs:3190` (`selection_text`) and Measure's pick handling in `crates/ui/src/dialogs.rs`.

## Replace Face takes a construction plane as target (P2)
- key: `ui:dialogs:replace-face-plane-target`
- Fusion: REPLACE FACE's Target Faces takes a face, a construction plane or a surface body.
- FissionOxide: with Target Faces active, clicking Plane1 in the canvas or its browser row leaves the chip on "Select" (the browser click also cleared Source Faces); body faces are taken.
- Start here: `crates/ui/src/forms.rs:3512` (the "Target Faces" input's filter).

## Replace Face keeps a two-lump body whole (P2)
- key: `ui:dialogs:replace-face-splits-lumps`
- Fusion: Replace Face on one lump of a body made by Combine Join of two separate bodies keeps one body row.
- FissionOxide: Replace Face (Tangent Chain, face next to a fillet) on one lump of a two-lump Body1 left two rows, Body1 and a new Body3.
- Start here: the replace-face command in `crates/engine/src/commands.rs` (how the result's solids are split into bodies); test near `crates/engine/src/tests_solid.rs:573`.
