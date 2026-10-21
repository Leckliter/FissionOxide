# Parity batch: ui-sketch-entry-polish

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## origin planes size (P1)
- key: `ui:canvas:origin-planes-size`
- Fusion: Create Sketch shows the three origin planes as small translucent orange squares, about twice the ViewCube's size (roughly 120 px on a 1456 px wide view), centred on the origin with a small coloured dashed axis triad. They highlight on hover.
- FissionOxide: The squares are about three times larger relative to the viewport (in a half-width window they reach the edges), overlap each other, and make the ground plane hard to pick. The hover highlight and tooltip work.
- Start here: `plane_half_size` and `plane_square` in `crates/ui/src/viewport.rs:2620-2664`, and the plane quads in `crates/engine/src/scene.rs`. Derive the half size from the camera distance so the square stays about 120 px on screen, and draw the axis triad stubs with it.

## origin axes always drawn (P2)
- key: `ui:canvas:origin-axes-always-drawn`
- Fusion: An empty design shows no axis lines (the Origin folder is hidden by default). Turning Origin on shows the planes and short axes at the origin, not lines across the grid.
- FissionOxide: Red, green and blue axis lines span the whole grid in every view, even with Origin hidden.
- Start here: `crates/engine/src/scene.rs:466` (`grid(scene, s, fixed, origin)`). Draw the axes only when the Origin folder is visible, as short stubs.

## grid axis labels (P2)
- key: `ui:sketch:grid-axis-labels`
- Fusion: The sketch grid has minor and major lines and prints coordinate values along both axes in document units (-2.5 ... 2.5 in), plus a small origin marker.
- FissionOxide: One grid level, no axis value labels, no origin marker.
- Start here: `crates/engine/src/scene.rs:466` (`grid`) for the lines; labels belong in `crates/ui/src/viewport.rs` beside the sketch overlay (project the major grid positions and draw text).

## profile color 3d (P2)
- key: `ui:sketch:profile-color-3d`
- Fusion: Outside the sketch a visible sketch's closed profiles shade light blue; inside the sketch they shade pale blue-white and a selected rectangle fills blue.
- FissionOxide: Outside the sketch the profile shades tan/orange; inside it shades pale yellow.
- Start here: `crates/engine/src/scene.rs:18` (`PROFILE_COLOR`) and the in-sketch profile fill in `crates/ui/src/viewport.rs`.

## heads-up units and lock (P2)
- key: `ui:sketch:heads-up-units-and-lock`
- Fusion: Heads-up boxes show the value with units ("2.635 in"); a typed value shows a lock icon and keeps the box editable until Enter.
- FissionOxide: Boxes show bare numbers ("33.35"); no units suffix, no lock glyph after typing.
- Start here: `crates/ui/src/viewport.rs:1015` (`hud_text`) and the HUD box at line 2365.

## enter ends rectangle (P2)
- key: `ui:sketch:enter-ends-rectangle`
- Fusion: After typing both heads-up values, Enter commits the rectangle and ends the Rectangle command (SELECT becomes active, Feature Options disappears). A click-drawn rectangle keeps the tool active.
- FissionOxide: Enter commits and the Rectangle tool stays active with the hint "Rectangle: click two corners" and a coordinate readout at the cursor.
- Start here: `crates/ui/src/viewport.rs:2325` (HUD commit for `Tool::Rect`). End the tool when the commit came from a typed value.

## palette feature options (P2)
- key: `ui:sketch:palette-feature-options`
- Fusion: While a sketch tool runs, the Sketch Palette shows a "Feature Options" section above Options (for Rectangle: 2-point / 3-point / center-point toggles; for Circle: the five circle types; for Line: nothing).
- FissionOxide: No Feature Options section; the palette is unchanged while a tool runs.
- Start here: `crates/ui/src/panels.rs:2207` (sketch palette rows). The tool variants already exist (`Tool::Rect`, `Tool::CenterRect`, the circle tools).

## palette linetype (P2)
- key: `ui:sketch:palette-linetype`
- Fusion: The Linetype row has two toggles, Construction and Centerline. There is no constraint-status line in the palette.
- FissionOxide: Linetype has one toggle. The palette adds "2 degrees of freedom · 1 profiles" (should read "1 profile" when singular).
- Start here: `crates/ui/src/panels.rs:2207` (Linetype row) and 2251 (the status line). Add a Centerline toggle (engine: `sketch.linetype` or equivalent) and fix the plural; keep the status line only if the user wants it.
