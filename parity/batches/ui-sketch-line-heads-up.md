# Parity batch: ui-sketch-line-heads-up

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 5) with the Line tool after its first click, FissionOxide `main`
at `d99603d` (after `276c2a7`, whose units and padlock this prober verified).

## line heads-up layout (P1)
- key: `ui:sketch:line-heads-up-layout`
- Fusion: After the first click the length box ("0.790 in", text selected) sits above the rubber-band segment's midpoint between dimension-style extension lines with arrows, the angle box ("7.2 deg", no border until focused) sits at the end of an arc drawn from a dashed horizontal reference line that starts at the first point, and the prompt reads "Specify next point". Tab moves the selection to the angle box; typing replaces it and a yellow padlock appears to the right of the box; Tab returns to the length box.
- FissionOxide: The length box ("24.37 mm") sits on the segment's midpoint and the angle box ("341.8 deg") just below-left of it, both as plain boxes with no extension lines, arc or reference line until a value is typed; the prompt reads "Place next point"; the padlock is drawn inside the box at its left.
- Start here: `crates/ui/src/viewport.rs:200` (`hud_fields`), `crates/ui/src/viewport.rs:1198-1470` (`hud_locks`, `hud_display`, HUD drawing), the dimension-style box drawing used for Rectangle.

## typed line values create both dimensions (P1)
- key: `ui:sketch:line-typed-values-dimensions`
- Fusion: "Tab 40 Tab 1 Enter" creates the line at 40 degrees with a "1.00" linear dimension along it and a "40.0" degree angular dimension against a dashed construction reference line that stays in the sketch; the Line tool continues from the new end.
- FissionOxide: The same keys create the line with only the "30.00" length dimension: no angle dimension and no reference line.
- Start here: `crates/ui/src/viewport.rs:1245` (`hud_apply`), where the typed length becomes a dimension (search `sketch.dimension` in `viewport.rs`), `crates/engine/src/commands.rs` (`dimension` with `type: "angle"`).
