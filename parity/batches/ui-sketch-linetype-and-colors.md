# Parity batch: ui-sketch-linetype-and-colors

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 6), FissionOxide `main` at `d90d1e6`. Colours are screen samples
over the white canvas (Fusion's over its white sketch background, FissionOxide's over its
(239..246) grey-white). Also in this batch, reopened: `ui:sketch:profile-color-3d` (row exists):
Fusion shades a closed profile (230,245,250) both inside the sketch and from outside it (a 3D view
of a visible sketch); FissionOxide shades (248,255,255) inside the sketch (nearly white) and a lit
steel blue (144,168,194) outside it, and draws the curves outside the sketch dark (60,70,90) where
Fusion keeps them blue (71,173,213). Start at `crates/engine/src/scene.rs:35` (`PROFILE_COLOR`,
`PROFILE_COLOR_EDIT`, `SKETCH_IDLE`).

## centerline style (P2)
- key: `ui:sketch:centerline-style`
- Fusion: A centerline draws dash-dot (about 9 px dash, 3 gap, 6 dash, 3 gap at the default zoom) in tan (207,174,155) when not selected, the same pattern in blue while selected; the palette's Centerline icon shows pressed while such a line is selected.
- FissionOxide: A centerline draws even grey dashes (128,128,132), 6 on 6 off, the same pattern as a construction line; selected it is even blue dashes.
- Start here: `crates/engine/src/scene.rs:709` (`push(..., c.construction)` passes only a dashed flag; the centerline flag `c.centerline` is not drawn), the dash pattern in `crates/render`.

## construction line colour and dash (P2)
- key: `ui:sketch:construction-color`
- Fusion: Construction geometry draws tan (207,174,155) with about 9 px dashes and 3 px gaps (over a shaded body it reads (176,143,124)); the dashed angle reference of a dimension is a darker orange (182,125,93).
- FissionOxide: Construction geometry draws grey (128,128,132) with 6 px dashes and 6 px gaps.
- Start here: `crates/engine/src/scene.rs:49` (`SKETCH_CONSTRUCTION`), `crates/engine/src/scene.rs:689`.

## construction toggle on a centerline (P2)
- key: `ui:sketch:construction-on-centerline`
- Fusion: X (Normal/Construction) on a selected centerline keeps the centerline flag: both Linetype icons show pressed and the line draws with even dashes in the same tan.
- FissionOxide: X on a selected centerline turns it into a normal solid line (both icons unlit).
- Start here: `crates/ui/src/lib.rs:1509` (`toggle_construction`), `crates/engine/src/commands.rs:1597` (`sketch.centerline` / `sketch.construction`, where one flag clears the other).

## sketch curve idle colour (P2)
- key: `ui:sketch:curve-idle-color`
- Fusion: An unconstrained curve in the open sketch draws (6,150,214) (anti-aliased edge (55,171,222)); outside the sketch a visible sketch's curves draw blue (71,173,213) and its points draw as small open circles.
- FissionOxide: An unconstrained curve draws (30,110,235) in the sketch and dark grey-blue (60,70,90) outside it.
- Start here: `crates/engine/src/scene.rs:46` (`SKETCH_FREE`), `crates/engine/src/scene.rs:50` (`SKETCH_IDLE`).

## grid label style (P2)
- key: `ui:sketch:grid-label-style`
- Fusion: Axis values sit on every major line ("-2.5 ... -0.5" along X just above the axis, "0.5 ... 2" along Y just left of the axis), rotated 90 degrees so they read along the axis, grey, with no "0" at the origin and no unit symbol; the origin carries a small hollow grey circle only.
- FissionOxide: Labels are horizontal, include a "0" at the origin and the unit on the outermost label ("100 mm"); right after Create Sketch no values are drawn along the Y axis at all (they appeared only after Look At re-fitted the view); the origin marker is a hollow circle with a red dot.
- Start here: `crates/ui/src/viewport.rs:2608` (`grid_labels`: `positions`, the `t != 0.0` Y branch and the `view.rect.contains` test).
