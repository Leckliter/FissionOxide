# Parity batch: ui-vertex-and-construction

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Colours were sampled from the screen on 2026-10-21 (Fusion default light theme; FissionOxide
`main` at `548bc6c`).

## vertices pickable by default (P1)
- key: `ui:canvas:vertex-filter-default`
- Fusion: With the default selection filters (every type on, including Body Vertices) hovering a body corner shows a vertex marker and clicking it selects the vertex: status "Vertex | X: -1.974 Y: 0.00 Z: 1.741 in".
- FissionOxide: The Vertices filter is off by default and vertex markers are only drawn with it on, so every hover near a corner picks an edge and a vertex can never be selected without first opening SELECT > Selection Filters.
- Start here: `crates/ui/src/viewport.rs:530` (vertex pick order, "drawn only with their filter on"), `crates/ui/src/menus.rs:318` (the vertices filter entry), `crates/engine/src/scene.rs:1308` (`vertex_markers`). Turn the filter on by default and only draw the marker for the hovered or selected vertex.

## vertex marker style (P2)
- key: `ui:canvas:vertex-marker-style`
- Fusion: A vertex has no mark until hovered; hovered it is a hollow ring about 9 px (grey ring (100..167), light inside); selected it is a filled light-blue dot, (147,167,185) at the centre and (191,214,237) at the rim, with a dark ring (92..122).
- FissionOxide: With the filter on, every vertex draws a 3 px dark dot (`VERTEX_COLOR` (40,44,52)) all the time; hovered a 4.5 px `HOVER_COLOR` dot, selected a 4.5 px `SELECT_COLOR` dot.
- Start here: `crates/engine/src/scene.rs:1306-1330` (`VERTEX_COLOR`, marker sizes). Hollow ring on hover, filled blue with a dark ring when selected, nothing otherwise.

## origin browser rows (P1)
- key: `ui:browser:origin-rows`
- Fusion: The Origin folder holds O, X, Y, Z, XY, XZ, YZ, each with an eye toggle (point, axis and plane icons). The folder's eye is off by default and the children's eyes are greyed; a child shows in the canvas only when its eye and the folder's are both on. Clicking a row selects the item (status "XZ Plane") even while it is hidden, and draws nothing in the canvas; hovering a row does not highlight the canvas.
- FissionOxide: The Origin folder holds O, XY, XZ, YZ with no eye toggles and no X, Y or Z axis rows. Clicking a row draws the plane in the canvas as an orange square and leaves the status bar empty; hovering a row shows the tooltip "Double-click to sketch on this plane".
- Start here: `crates/ui/src/panels/browser_rows.rs:359-420` (Origin folder, `origin_show_hide`, `set_origin_planes`). Add axis rows and per-row eyes that follow Fusion's folder-and-child rule.

## construction plane colors (P1)
- key: `ui:canvas:construction-plane-colors`
- Fusion: A visible origin plane is a translucent peach square (254,228,177). Hovered it turns grey (232,229,222); selected it turns blue (127,174,216) and the status reads "XY Plane". Clicking it again or Escape clears it; the square stays visible.
- FissionOxide: The plane is drawn (236,202,157) only while selected from the browser; hovering it in the canvas changes nothing; clicking it in the canvas makes it disappear (selection cleared) and the status bar stays empty.
- Start here: `crates/ui/src/viewport.rs:3479` (origin plane sizing test) and the origin plane drawing in `crates/engine/src/scene.rs` (`ORIGIN_X_COLOR` and friends at `:31`). Give planes plain, hover and selected colours and a status readout.

## construction axis and origin point (P2)
- key: `ui:canvas:construction-axis-colors`
- Fusion: A visible origin axis is a dashed line in its colour (X red, Y green, Z blue). Hovered it becomes a solid blue line (74,132,246) about 4 px; selected (2,126,246) about 4 px with a pale halo, status "Y Axis". The origin point is a hollow grey circle; hovered a hollow ring; selected a filled blue dot, status "Origin Point | X: 0.00 Y: 0.00 Z: 0.00 in".
- FissionOxide: Axes and the origin point cannot be shown from the browser (no rows) and are not pickable.
- Start here: `crates/engine/src/scene.rs:31` (`ORIGIN_X_COLOR` etc.) and the pick table; add `Pick` kinds for axes and the origin point with the colours above.
