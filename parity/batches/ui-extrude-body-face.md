# Parity batch: ui-extrude-body-face

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 (prober 18) while re-verifying the extrude manipulator keys, FissionOxide built
from `main` at `2d74c694` with one box (CREATE > Box on the XY plane, 34 x 20 x 10 mm) in a new
design. One item; it blocks re-verifying `ui:manip:*` on body faces. See
`parity/reports/2026-10-22-workspaces.md`.

## Extrude takes a body's planar face (P1)
- key: `ui:dialogs:extrude-body-face-pick`
- Fusion: with Extrude (E) open, clicking a planar face of a body fills Profiles with "1 selected" (with an x), adds the Distance ("0.00 in") and Taper Angle ("0.0 deg") rows, draws the blue arrow and the taper disc on the face, and puts focus in the floating value box; the status bar reads "1 Face | Area : 2.00 in^2".
- FissionOxide: clicking the box's top face during Extrude turns the face blue and the status bar reads "1 Face | Area : 680.00 mm^2", but Profiles stays "Select", no Distance row, preview or arrow appears and OK stays greyed; a second click clears the highlight and a third sets it again with the same result. A face selected before pressing E is not taken either. Press Pull (Q) on the same face works (OFFSET FACE, 1 selected).
- Start here: the Extrude profile pick in `crates/ui/src/dialogs.rs:2580` (the prompt `"Select sketch profiles or planar faces to extrude"`) and the face-to-profile conversion in `crates/engine/src/commands.rs`; check whether faces of a Box primitive are refused while faces of an extruded body are taken.
