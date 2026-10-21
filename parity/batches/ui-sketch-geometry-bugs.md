# Parity batch: ui-sketch-geometry-bugs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 with both apps sketching on a vertical origin plane. Two items are
wrong geometry (P1); the rest are the construction geometry, previews and constraint glyphs that
Fusion's create tools leave behind and FissionOxide's do not.

## three point arc slot geometry (P1)
- key: `ui:sketch:three-point-arc-slot-geometry`
- Fusion: Three Point Arc Slot: start centre, end centre, a point on the arc, then the width; the preview is the arc through the three points and the result a small arc slot (two concentric arcs, two end caps) with a dashed arc centreline and a centre dot.
- FissionOxide: With the same four clicks (start, end, a point above the chord, then the width) the preview during the arc pick is a triangle of straight lines and the result is a huge three-quarter ring centred near the origin: the arc is built through the wrong points (the tool's order is "start, a point on the arc, end" per its status text, the opposite of Fusion's) and the width applied to it.
- Start here: `crates/ui/src/viewport.rs:977-993` (`Tool::ArcSlot3 | Tool::ArcSlotCenter if n == 4`), `:2156` (preview), `:2556` (status text). Use Fusion's order (start, end, point on arc, width), preview the arc through the three points and then the slot outline.

## edge polygon side (P1)
- key: `ui:sketch:edge-polygon-side`
- Fusion: Edge Polygon: after the two edge points the hexagon previews on whichever side of the edge the cursor is ("Specify the side of the polygon") and the third click fixes that side.
- FissionOxide: The third click is ignored for the side: with the cursor below the edge the hexagon was built above it (into the body). The preview during the third step shows the polygon on the wrong side too.
- Start here: `crates/ui/src/viewport.rs:965` (`Tool::EdgePolygon if n == 2` commits on the second click), `:2179` (preview). Add the third step and pass the side to `sketch.polygon`.

## slot preview outline (P2)
- key: `ui:sketch:slot-preview-outline`
- Fusion: Every slot tool previews the finished outline (two lines and two caps) while the width is set; the arc slots preview the arc, then the curved outline.
- FissionOxide: Center to Center, Overall and Center Point slots preview two full circles joined by a line; the arc slots preview bare lines (a triangle for the three-point one) and no outline until the commit.
- Start here: `crates/ui/src/viewport.rs:2232` (`Tool::Slot` preview), `:2180-2181` (`OverallSlot | CenterSlot`), `:2156` (arc slots).

## polygon preview and glyph (P2)
- key: `ui:sketch:polygon-preview-and-glyph`
- Fusion: Inscribed/Circumscribed Polygon preview the polygon and its circle together while the radius is set; the result carries one small pentagon glyph beside the centre point and no construction circle.
- FissionOxide: Inscribed Polygon previews only the circle until the click; the result draws an orange dashed construction circle around the hexagon and a row of five "=" glyph icons above it plus four more around (one per equal-length constraint).
- Start here: `crates/ui/src/viewport.rs:2179` (preview), `:2084-2090` (glyph icons; collapse a polygon's equal/coincident set into one polygon glyph), `crates/engine/src/commands.rs` (`sketch.polygon` adds the circle as construction geometry; Fusion keeps none).

## center rectangle construction (P2)
- key: `ui:sketch:center-rect-construction`
- Fusion: Center Rectangle previews and keeps two dashed construction diagonals (grey) crossing at the centre point, with horizontal/vertical glyphs on the sides, a midpoint glyph and an "=" glyph; 3-Point Rectangle leaves grey perpendicular glyphs at three corners.
- FissionOxide: One orange dashed diagonal, horizontal/vertical glyph icons, no "=" glyph; the 3-point rectangle's perpendicular glyphs are small square icons.
- Start here: `crates/engine/src/commands.rs` (`sketch.rect` with `center`), `crates/ui/src/viewport.rs:2084` (glyph style). Construction geometry should be grey dashed, not orange (see also `ui:sketch:palette-linetype`).

## ellipse construction axes (P2)
- key: `ui:sketch:ellipse-construction-axes`
- Fusion: Ellipse previews a dashed axis line with a parallel glyph while the first axis is set; the result keeps dashed construction major and minor axes through the centre point and a horizontal glyph on the axis.
- FissionOxide: A plain ellipse with a centre mark; no axes, no glyph.
- Start here: `crates/ui/src/viewport.rs:994` (`Tool::Ellipse if n == 3`), `:2181-2184` (preview), `crates/engine/src/commands.rs` (`sketch.ellipse`).

## slot centerline and glyphs (P2)
- key: `ui:sketch:slot-centerline-and-glyphs`
- Fusion: A finished slot has a dashed construction centreline between two filled centre points, four tangent glyphs (circle-with-bar) at the cap junctions, a parallel glyph and an "=" glyph; Center Point Slot adds a midpoint triangle at the centre; arc slots have a dashed arc centreline and a centre dot.
- FissionOxide: Slots show two "= =" glyph-icon pairs above and below and a horizontal glyph at the left; no centreline, no centre points, no tangent glyphs; arc slots likewise.
- Start here: `crates/ui/src/viewport.rs:953` (`Tool::Slot if n == 3` commit), `:977` (arc slots), `crates/engine/src/commands.rs` (`sketch.slot`), `crates/ui/src/viewport.rs:2084` (glyph icons: tangent, parallel, equal, midpoint).

## midpoint and perpendicular glyph style (P2)
- key: `ui:sketch:glyph-style-create-tools`
- Fusion: Constraint glyphs are small grey line-art symbols without a box: a triangle for midpoint, an "L" corner for perpendicular, a circle-with-bar for tangent, "=" for equal, a pentagon for polygon; they sit beside the geometry and a hovered glyph highlights its items.
- FissionOxide: Glyphs are white boxed icons with a blue symbol (the midpoint glyph on a midpoint line is a boxed line icon above the line; the perpendicular ones are boxed "L" icons). Several per curve stack into rows.
- Start here: `crates/ui/src/viewport.rs:2084-2090` (`ConstraintKind` to `Icon` mapping and the box drawing). Related: `ui:sketch:constraint-glyphs` (match for placement).
