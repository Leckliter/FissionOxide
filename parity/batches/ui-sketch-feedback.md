# Parity batch: ui-sketch-feedback

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## heads up input (P0)
- key: `ui:sketch:heads-up-input`
- Fusion: Drawing a line or rectangle shows length and angle boxes that follow the cursor; Tab cycles them.
- FissionOxide: Width/height boxes follow the cursor with dimension arrows, and typing goes into the width box. But Tab doesn't move to the height box: it re-selects the width text (checked with a real Tab key), so typing '40 Tab 20 Enter' gives width 20 and leaves the height at whatever the mouse made it, with only one dimension. In Fusion the same keys give a 40 x 20 rectangle with both dimensions. Code: hud loop at viewport.rs:1739-1781; the Tab press seems to be eaten before 'r.has_focus() && tab' runs (egui focus traversal, or lock_focus). Add a test that drives Tab between boxes.
- Start here: `crates/ui/src/viewport.rs`

## fully constrained color (P1)
- key: `ui:sketch:fully-constrained-color`
- Fusion: Fully constrained curves turn black; underconstrained curves stay blue.
- FissionOxide: Curves are always drawn blue. The palette shows the sketch's total DOF ('Fully constrained' at dof == 0, panels.rs:1693), but nothing colours individual curves. Fusion draws fully constrained curves black and free ones blue, so you can see which geometry still moves. Needs per-entity DOF from the solver, and the sketch painter picking black for fixed curves.
- Start here: `crates/ui/src/viewport.rs`, `crates/engine/src/sketch`
