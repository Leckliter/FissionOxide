# Parity batch: ui-reverify-dialogs-6

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 6) on a 30 x 20 x 10 mm box (SOLID > Box) in the
rebuilt app (`main` at `dadbcad3`, then `d2202676`). Five rows are reopened (the Export one on `3aa0fa4c`), one is new.
Every row is already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this
batch); read each row's `history` before starting.

## Mirror originals stay blue (P2)
- key: `ui:dialogs:copy-ghost-preview`
- Fusion: Mirror and pattern previews draw each copy as a translucent grey ghost with dark edges, and the picked originals stay blue the whole time.
- FissionOxide: the ghost matches, but the picked original is blue only until the Mirror Plane row becomes active; from then on (and with both picks made, status "2 selections") it is drawn plain grey.
- Start here: `crates/ui/src/dialogs/copies.rs:98`, `crates/ui/src/dialogs/body_picks.rs`

## Remove takes components (P2)
- key: `ui:dialogs:remove-components`
- Fusion: REMOVE's Objects chip takes bodies or components (prompt "Select bodies or components to remove"), from the canvas or the browser.
- FissionOxide: in an Assembly Design with a new component (ASSEMBLY > CREATE > New Component, "External1"), Remove shows the right prompt, but a click on the component's browser row leaves Objects at Select.
- Start here: `crates/ui/src/dialogs.rs` (Remove's Objects chip), `crates/engine/src/commands.rs:1336`

## Chamfer status and starting distance (P2)
- key: `ui:dialogs:chamfer-fields-2`
- Fusion: after the edge pick the status bar reads "Chain" and the selection-set table starts at 0 ("2 Edges | 0.00 in | 0.00 in").
- FissionOxide: the status bar is empty and the table reads "1 Edge | 1 mm | 1 mm". Everything else in the row matches.
- Start here: `crates/ui/src/dialogs.rs:2978`, `crates/ui/src/panels.rs:3190`

## Measure from a dimension box fills the box (P2)
- key: `ui:sketch:dim-box-measure-fill`
- Fusion: picking Measure in a dimension box's menu opens Measure; the measured length is written into the box when Measure finishes.
- FissionOxide: dimension a line (box "16.1 mm"), box menu > Measure, pick another line (Measure shows Length "20.200 mm"), Close: the box still reads "16.1 mm". `fill_dim_from_measure` only takes `tstate.measured`, a distance between two picks, not a single edge's length.
- Start here: `crates/ui/src/viewport.rs:3032`

## angle boxes show one decimal (P2, new)
- key: `ui:dialogs:angle-box-precision`
- Fusion: angle boxes read "0.0 deg" / "118.0 deg": Extrude's Taper Angle, Hole's drill point, Move/Copy's X/Y/Z Angle.
- FissionOxide: Extrude's Taper angle is a greyed "0 deg", Hole's drill point "118 deg", Move/Copy's angles "0 deg" (Insert Mesh and Draft already show "0.0 deg").
- Start here: `crates/ui/src/dialogs/hole_pattern.rs:181`, `crates/ui/src/dialogs/extrude.rs:169`, `units::typed_angle`

## Export types and default (P2)
- key: `ui:chrome:export-dialog`
- Fusion: File > Export... starts at "Autodesk Fusion Archive Files (*.f3d)"; 3MF, Autodesk Inventor 2025 (*.ipt), FBX, OBJ, SketchUp (*.skp), SMT (*.smt), STEP, STL and USD are live; DWG, DXF, IGES and SAT are greyed.
- FissionOxide (`main` at `3aa0fa4c`, after `ui-reverify-chrome-1`): FBX is live now, but IPT, SketchUp and SMT are still greyed, and the Type starts at "FissionOxide Archive Files (*.foxz)" with the .f3d entry greyed.
- Start here: `crates/ui/src/file_dialogs.rs:561`, `crates/ui/src/file_dialogs.rs:564`
