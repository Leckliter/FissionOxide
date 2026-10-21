# Parity batch: ui-sketch-solver

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## horizontal conflict (P1)
- key: `ui:sketch:horizontal-conflict`
- Fusion: Selecting a line and clicking Horizontal/Vertical always applies the constraint when the sketch has enough freedom: Fusion rotates the line about its fixed or coincident end, moving the free end.
- FissionOxide: On a line that was trimmed against a circle (one end coincident with the circle, the other free, 6 degrees of freedom in the sketch) Horizontal/Vertical shows "that constraint conflicts with the others (or over-constrains the sketch)" and nothing changes. On a free line it works.
- Start here: `crates/engine/src/commands.rs:3244-3254` (the conflict error) and the solver call it wraps in `crates/sketch`. The solve probably starts from the current geometry and fails to converge because the coincident point must slide along the circle; try a few relaxation steps or a different starting guess before reporting a conflict, and only report over-constraint when the DOF count says so.
