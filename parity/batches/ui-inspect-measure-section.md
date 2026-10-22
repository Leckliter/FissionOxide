# Parity batch: ui-inspect-measure-section

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 12) in an unsaved Part Design document in each app (Fusion in
inches, FissionOxide in mm), FissionOxide built from `main` at `c4d97aa9`. The Measure dialog's
first rows are `ui:inspect:measure-dialog-rows` in batch `ui-inspect-insert-assemble`; this batch is
what happens after two picks, plus Interference and Section Analysis. See
`parity/reports/2026-10-22-construct-inspect-insert.md`.

## Measure with two picks (P2)
- key: `ui:inspect:measure-two-picks`
- Fusion: Picking a sphere face and then a vertex: Results shows only "Distance 1.29 in"; below it a collapsible "Selection 1" (grey "Face selected", Area, Loop Length) and "Selection 2" (grey "Vertex selected", X/Y/Z Position); the canvas draws a thin black arrow between the two with a plain black "1.29 in" label and small numbered tags "1" and "2" at the picks; the status bar reads "1 Face 1 Vertex | Min Distance : 1.29 in". The dialog stays docked at the right the whole time.
- FissionOxide: Picking a sphere and a box face: Results "Copy all", Distance "43.852 mm", ΔX, ΔY, ΔZ, each with a Copy button, no Selection sections; the canvas draws an orange line with a filled orange "43.852 mm" label box and no numbered tags; the status bar reads "2 Faces". The dialog first opens wide in the middle of the canvas and jumps to the right edge after the first pick.
- Start here: `crates/ui/src/dialogs.rs:4611` (`measure_rows`), `crates/ui/src/viewport.rs:4969` (`measure_overlay`).

## Interference dialog rows (P2)
- key: `ui:inspect:interference-dialog`
- Fusion: INTERFERENCE: Select (body picks, "Sel..." chip), Include Coincident Faces (checkbox), Compute (an icon button), OK (disabled until computed) and Cancel.
- FissionOxide: INTERFERENCE: Result "No interference", a Compute text button, Create components (checked), Create Bodies (greyed button) and Close; no body selection (it checks every body).
- Start here: `crates/ui/src/dialogs.rs:3419` and `:4075` (the Interference rows and apply).

## Section Analysis dialog, manipulator and result (P1)
- key: `ui:inspect:section-analysis-dialog`
- Fusion: SECTION ANALYSIS opens with only Cut Plane ("Select"; pointer "Select a planar object to define the cut plane") and OK disabled. After picking XZ: Distance "0.00 in", Angle 1 "0.0 deg", Angle 2 "0.0 deg", Flip (icon), Section Color ("From Component" dropdown), Show Hatch (checked); the canvas shows a large peach section plane, an arrow handle and two rotation handles with a floating "0.00 in" box, and the cut faces hatched. OK keeps the cut and adds an "Analysis" folder (between Origin and Bodies, with an eye) holding "Section1"; hiding the folder restores the full model.
- FissionOxide: SECTION ANALYSIS opens with Show section (checked), Plane dropdown "XZ plane", Offset "0.0 mm", Flip (keep the other side) checkbox and a Close button; no pick row, angle rows, colour, hatch toggle or manipulator, and no browser row afterwards.
- Start here: `crates/ui/src/dialogs.rs:4982` (`section_fields`), `crates/engine/src/scene.rs:201` (the section options); the browser folder is shared with `ui:inspect:analysis-browser-folder` (batch `ui-inspect-analyses`).
