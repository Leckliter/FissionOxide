# Parity batch: ui-sketch-constraint-solver

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 5): two separate unconstrained lines (line 1 about 70 px long,
line 2 about 95 px) and a circle in an XY sketch, FissionOxide `main` at `d99603d`. Fusion's solver
moves as little as possible and prefers moving the geometry picked last (or the less constrained
one); FissionOxide's solver spreads the change over everything it touches. Every item needs a
regression test on the sketch solver with the end points checked after `solve()`.

## coincident keeps the first pick in place (P1)
- key: `ui:sketch:coincident-motion`
- Fusion: Coincident on line 1's end and line 2's start leaves line 1 untouched and moves line 2 rigidly (same length and angle) so its start lands on line 1's end.
- FissionOxide: Both points move to their midpoint, so both lines change length and angle (the far ends stay).
- Start here: `crates/sketch/src/solve.rs:68` (`solve`; weight the points of the first-picked curve as anchors), `crates/engine/src/commands.rs:3326` (`constrain` has the pick order).

## parallel and perpendicular rotate the second line about the shared point (P1)
- key: `ui:sketch:parallel-perpendicular-motion`
- Fusion: With the two lines joined at one point, Parallel rotates the second-picked line about the shared point onto the first line's direction (lengths unchanged); Perpendicular does the same to a right angle.
- FissionOxide: Parallel rotated the first line instead (selection order is arbitrary), and Perpendicular stretched both lines to several times their length while rotating them.
- Start here: `crates/sketch/src/solve.rs:68`, the `Parallel`/`Perpendicular` residuals in `crates/sketch/src/lib.rs`; add length-preserving tests.

## collinear keeps the first line's direction (P1)
- key: `ui:sketch:collinear-motion`
- Fusion: Collinear rotates the second line about the shared point onto the first line's direction; the first line does not move.
- FissionOxide: Both lines were rotated to horizontal and shifted onto one y, losing the first line's direction.
- Start here: `crates/sketch/src/solve.rs:68`, `Collinear` residual in `crates/sketch/src/lib.rs`.

## concentric moves only the free circle (P1)
- key: `ui:sketch:concentric-motion`
- Fusion: Concentric between the free test circle and a circle that already carries tangent constraints moves only the free circle onto the other centre; nothing else in the sketch moves.
- FissionOxide: The constrained circle, its tangent lines and the neighbouring circle all shifted by several pixels and the constrained circle shrank; the free circle ended up merged into it.
- Start here: `crates/sketch/src/solve.rs:68` (anchor already-constrained geometry), `Concentric` residual.

## tangent pivots the line so its extension touches the circle (P2)
- key: `ui:sketch:tangent-line-circle-motion`
- Fusion: Tangent between a line joined to another line and a far circle pivots the line about the shared point until its infinite extension is tangent to the circle; the circle stays and the tangent glyph sits on the segment end nearest the tangency.
- FissionOxide: The same picks left the line as it was and put the tangent glyph beside the circle's centre; whether the circle or the line is meant to move is not expressed (the solver moves whichever residual is cheapest).
- Start here: `crates/sketch/src/solve.rs:68`, `Tangent` residual, `crates/ui/src/viewport.rs:2906` (`glyph_anchor` for `Tangent`).
