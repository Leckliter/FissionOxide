# Parity batch: ui-origin-display

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-22 (prober 8) in a new design in each app, with the browser's Origin folder
turned on; Fusion is Y up, FissionOxide Z up (that difference is `ui:canvas:up-axis`, not part of
this batch). FissionOxide built from `main` at `0c453dd1`. Already right and verified today: no
axis lines until Origin is shown, a hovered plane turns grey, a selected plane blue with the status
"XY Plane", a hovered axis solid blue and a selected axis blue with the status "X Axis". Almost
everything here is in `crates/ui/src/origin.rs` (`overlay` at line 271) and
`crates/ui/src/viewport.rs` (`origin_plane_half_size`, line 4656).

## origin planes sit in the positive quadrant (P1)
- key: `ui:canvas:origin-planes-size`
- Fusion: Each origin plane is a square in the positive quadrant of its two axes, starting a few pixels off each axis (a small gap along both axes) and reaching about 60 px from the origin at the home view, so XY, XZ and YZ never overlap; the axes run through the gaps. Create Sketch, Revolve, Loft and the primitives offer the same squares.
- FissionOxide: Each square is centred on the origin with `half` either side (`plane_corners`), so the three squares cross each other and draw a six-pointed star about twice Fusion's extent.
- Start here: `crates/ui/src/origin.rs:266` (`plane_corners`: from (gap, gap) to (size, size) in the plane's own u/v), `crates/ui/src/origin.rs:325` (`under`: pick only that quadrant), `crates/ui/src/viewport.rs:4656` (`origin_plane_half_size`) and the Create Sketch plane chooser that uses the same squares (`crates/ui/src/viewport.rs:4698`, `plane_pick`).

## origin draw order and depth (P1)
- key: `ui:canvas:origin-draw-order`
- Fusion: The planes are see-through (the grid and each other show through, overlaps read darker) and are hidden behind bodies (a box at the origin covers the parts of the squares inside and behind it); the axes and the origin point draw on top of the planes.
- FissionOxide: The planes are nearly opaque (alpha 230/255) and painted as a 2D overlay after the scene, over every body, and after the axes and point, so the axes and the origin ring are hidden under the planes and the three squares merge into one flat shape.
- Start here: `crates/ui/src/origin.rs:231` (`PLANE_ALPHA`) and `crates/ui/src/origin.rs:274` (`overlay`: draw planes first, then axes, then the point); depth-test the planes against bodies, for example by drawing them in the engine pass (`crates/engine/src/scene.rs`, as the construction planes are) instead of the egui overlay.

## origin axis and point style (P2)
- key: `ui:canvas:origin-axis-style`
- Fusion: Each axis is drawn only in its positive direction in pure red, green or blue about 2 px wide: solid for the first stretch from the origin, then long-dash / short-dash, running past the edge of the squares and ending in a dot. The origin point is a small ring (grey, about 7 px) filled white.
- FissionOxide: Axes are 1.5 px pastel lines of even 4 px dashes and 3 px gaps that stop at 1.3 times the plane half-size; the origin ring is hollow (the grid shows through it).
- Start here: `crates/ui/src/origin.rs:295` (the `Look::Plain` axis arm: colour, width and a dash-dot pattern with a solid start) and `crates/ui/src/origin.rs:310` (the plain origin point: white fill), `crates/ui/src/viewport.rs:4652` (`ORIGIN_AXIS_REACH`).

## origin stays visible in a sketch (P1)
- key: `ui:canvas:origin-in-sketch`
- Fusion: With the Origin folder on, the origin planes, axes and point stay drawn while a sketch is open (the sketch plane's own square shows in its quadrant behind the sketch grid, and the point at the origin).
- FissionOxide: Nothing from the Origin folder is drawn while a sketch is being edited, even after turning the folder's eye on inside the sketch.
- Start here: `crates/ui/src/origin.rs:133` (`visible`: drop the `edit_sketch.is_none()` condition) and the test in `crates/ui/src/origin.rs` that asserts an open sketch hides them.

## browser row hover highlights the canvas (P2)
- key: `ui:browser:origin-row-hover-canvas`
- Fusion: With Origin shown, hovering the XY row (or any origin or construction row, e.g. Plane1) turns that plane grey in the canvas, as hovering the plane itself does.
- FissionOxide: Hovering an origin row only highlights the row; the canvas does not change.
- Start here: `crates/ui/src/panels/browser_rows.rs:391` (origin and construction rows: set `app.hover_origin` / the construction hover when the row is hovered) and `crates/ui/src/origin.rs:255` (`look`).

## Create Sketch picks infinite planes and previews the grid (P1)
- key: `ui:canvas:create-sketch-plane-grid-preview`
- Fusion: While Create Sketch (and Box, Cylinder, Sphere, Torus, Coil) waits for "Select a plane or planar face", every pointer position hovers one origin plane, taken as an infinite plane (the one the pointer ray meets first), even far from the squares; the hovered square turns blue-grey and the sketch grid of that plane, with its coordinate labels, is previewed across the canvas.
- FissionOxide: Only the squares themselves pick; the hovered square turns a darker orange; no grid is previewed and pointing anywhere else hovers nothing.
- Start here: `crates/ui/src/viewport.rs:4698` (`plane_pick`: fall back to the ray's first infinite origin plane), the plane-chooser drawing in `crates/ui/src/viewport.rs` (hover colour, and draw the sketch grid of the hovered plane with labels as the sketch grid does).
