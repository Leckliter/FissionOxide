# Parity batch: ui-inspect-analyses

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 12) in an unsaved Part Design document in each app (Fusion in
inches, FissionOxide in mm), FissionOxide built from `main` at `c4d97aa9`. Every Fusion analysis is a
docked dialog that starts with a body (or edge) pick, OK disabled until picked; OK keeps the analysis
as a row in a browser "Analysis" folder. Section Analysis is in batch `ui-inspect-measure-section`.
See `parity/reports/2026-10-22-construct-inspect-insert.md`.

## Analysis dialog rows (P1)
- key: `ui:inspect:analysis-dialogs-rows`
- Fusion: Curvature Map: Bodies ("Select"; pointer "Select bodies to analyze"), Type (three icon toggles), Display (two icon toggles), Scale "1.000" with a slider, Opacity "100" with a slider, High Quality (checkbox). Draft: Body, Direction (both "Select"), Draft Angle "-2.0 deg" / "2.0 deg" with a two-thumb slider, Tolerance (checked), Tolerance "-0.5 deg" / "0.5 deg" with a two-thumb slider, Opacity "100". Zebra: Bodies, Direction (two icon toggles), Repeats "20" slider, Opacity "100" slider, Lock Stripes, High Quality. Accessibility: Body, Direction (both "Select"), Opacity "100" slider. All with OK and Cancel.
- FissionOxide: Each opens with Show analysis (checked) and colours every body at once: Curvature Map has only a legend line; Draft has Pull direction dropdown "Z axis" and Draft angle "1.0°"; Zebra a hint line; Accessibility Tool direction "Z axis"; each with a Close button only.
- Start here: `crates/ui/src/dialogs.rs:5023` (`analysis_fields`), `crates/ui/src/lib.rs:241` (`analysis` option).

## Analyses live in a browser Analysis folder (P1)
- key: `ui:inspect:analysis-browser-folder`
- Fusion: OK on Section Analysis or Center of Mass adds an "Analysis" folder between Origin and Bodies with one row per analysis ("Section1", "CenterOfMass1", each with an eye); the folder's eye hides them all and Escape cancels a dialog without leaving anything behind.
- FissionOxide: There is no Analysis folder or row. Close and Escape both leave the analysis colours on the bodies; the only way to remove them is to reopen the same command and clear Show analysis. They even carry over to a new design: after "+", a new Box was drawn in Accessibility red and green.
- Start here: `crates/ui/src/lib.rs:241` (`analysis` is app state, not part of the design) and `crates/ui/src/panels.rs` (the browser sections; add the folder after Origin).

## Curvature Comb, Environment Map, Isocurve and Minimum Radius are missing (P1)
- key: `ui:inspect:missing-analyses`
- Fusion: Curvature Comb: Edges ("Select"; pointer "Select edges to analyze"), Density "50" slider, Scale "10.000" slider. Environment Map: Bodies, Environment "Sharp Highlights" dropdown, Rotate "0.0 deg" slider, Apply Mirror (checked). Isocurve: Bodies/Faces, Type (three icon toggles), Isocurves "5" slider, Curvature Combs (checkbox). Minimum Radius: Body, Maximum Tool Radius "0.984 in", Minimum Radius "0.000 in" with a slider, Opacity "100", Sharp Edges (checkbox).
- FissionOxide: Each entry only shows "<name> isn't in FissionOxide yet" at the lower left.
- Start here: `crates/ui/src/menus.rs:242` (the `x(...)` entries in INSPECT), `crates/ui/src/dialogs.rs:5023`.

## Center of Mass needs a pick and leaves a glyph row (P2)
- key: `ui:inspect:center-of-mass-dialog`
- Fusion: CENTER OF MASS has one row, Selection ("Select"; pointer "Select bodies and/or components"), OK disabled until a pick; OK adds "CenterOfMass1" (a black-and-white target icon) to the Analysis folder.
- FissionOxide: CENTER OF MASS computes over every body at once and lists Bodies "2", Centre, Mass and Volume with a Close button; the target glyph shows while the dialog is open and nothing is kept.
- Start here: `crates/ui/src/dialogs.rs:3453` and `:4082` (the Center of Mass rows).
