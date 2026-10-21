# Parity run, 2026-10-21 (fourth pass): every sketch CREATE tool

Prober 2, desktop control over Fusion (Personal, inches) and FissionOxide (`main` at `347ba78`,
mm), both windows placed left of the Claude overlay. In each app a new sketch was opened on a
vertical origin plane (the body sits above the x axis, the drawing area below it) and every entry
of the sketch CREATE menu was driven with the same clicks, zooming on the cursor, the heads-up
boxes, the preview and the result each step. Dialogs were opened and cancelled.

## What matched
- The CREATE list and all six submenus (Rectangle, Circle, Arc, Polygon, Slot, Spline): same
  entries, order and icons. The Project / Include submenu is the exception (below).
- The geometry of 3-Point Circle, Center Point Arc, Tangent Arc (with its tangent glyph),
  Center Point Arc Slot, Inscribed Polygon, Ellipse, Midpoint Line and Point.
- Tools repeat after each shape and Escape ends them; the palette's "n degrees of freedom ·
  m profiles" line; the Sketch Dimension value box opens after a pick.
- Eight match rows were added so the coverage is visible.

## Differences by priority

Every row is in `parity/ui-features.jsonl` with `file:line` pointers; the batches are in
`map-wave7.json`.

### P1
- **Three Point Arc Slot builds the wrong arc** (`ui:sketch:three-point-arc-slot-geometry`):
  the same four clicks give a small arc slot in Fusion and a three-quarter ring centred near
  the origin in FissionOxide. The tool's pick order is also reversed (start, point on arc, end).
- **Edge Polygon ignores the side pick** (`ui:sketch:edge-polygon-side`): the hexagon was built
  above the edge with the cursor below it.
- **No cursor prompts and no heads-up boxes on any create tool** (`ui:sketch:create-tool-cursor-prompts`,
  `ui:sketch:create-tool-heads-up-boxes`): Fusion shows a crosshair with a per-step prompt
  ("Select start center point", "Specify the side of the polygon", ...) and value boxes with
  units for every tool (length/angle, diameter, radius/angle, slot length/width, rho). FissionOxide
  shows a hollow circle with an "x, y" readout and the sentence in the status bar; only Center
  Rectangle has boxes, without units.
- **Splines**: no check-mark button, polyline preview, Escape commits a fit spline and discards
  a control spline (`ui:sketch:spline-accept-check`). In Fusion Escape discards both.
- **Sketch Dimension has no placement step and Escape discards it**
  (`ui:sketch:dimension-placement-flow`); radius reads "12.8556" instead of "R0.228".
- **Text opens the whole form with a preview at the origin** instead of a Type row plus a frame
  drag (`ui:sketch:text-tool-flow`); the preview is drawn vertically on this plane.
- **Intersect is inert** ("Intersect isn't in FissionOxide yet", `ui:sketch:intersect-missing`).

### P2
- Conic commits on the vertex click with no rho box (`ui:sketch:conic-rho-box`).
- Previews: slots preview as two circles and a line, arc slots as bare lines, polygons as the
  circle only (`ui:sketch:slot-preview-outline`, `ui:sketch:polygon-preview-and-glyph`).
- Construction geometry left by the tools: Center Rectangle's two grey dashed diagonals,
  Ellipse's axes, slot centrelines and centre points are missing; FissionOxide draws one orange
  diagonal and a construction circle round polygons instead (`ui:sketch:center-rect-construction`,
  `ui:sketch:ellipse-construction-axes`, `ui:sketch:slot-centerline-and-glyphs`).
- Glyphs: boxed blue icons stacked in rows versus Fusion's small grey symbols (one pentagon for
  a polygon, a triangle for midpoint, circle-with-bar for tangent); points and centres are blue
  diamonds instead of open circles (`ui:sketch:glyph-style-create-tools`, `ui:sketch:point-glyph-style`).
- Dialogs: Mirror ("SKETCH MIRROR", dropdown, OK enabled), Circular Pattern (typed centre),
  Rectangular Pattern (all rows up front), Project / Project To Surface / Intersection Curve /
  Include 3D Geometry row names and extra rows (`ui:sketch:mirror-dialog`,
  `ui:sketch:circular-pattern-dialog`, `ui:sketch:rectangular-pattern-dialog`,
  `ui:sketch:project-dialogs-rows`).
- Menu: Spun Profile (the engine has the command) and Isoparametric Curve are missing from
  Project / Include (`ui:sketch:project-include-menu`); 3-Point Arc shows an "A" shortcut that
  Fusion does not have (`ui:sketch:arc-shortcut-hint`).
- The square snap marker Fusion shows when a tool hovers an existing point is absent
  (`ui:sketch:tangent-arc-snap-marker`).

## Retractions and uncertainties
- 2-Tangent Circle: the second pick on a rectangle side registered in neither app (the picks
  were not close enough to the lines), so the tool is not compared; FissionOxide answered
  "Click a curve" in red. 3-Tangent Circle was not tried. Both stay open.
- Fusion's Include 3D Geometry dialog did not open from the submenu click (the click landed
  between rows); its row list above comes from the earlier prober's Project / Include row and
  should be re-checked.

## Batches

| Batch | Items | Main files |
|---|---|---|
| ui-sketch-hud-prompts | 7 | viewport.rs (hud, hud_fields, prompts, dimension tool), lib.rs (Escape/Enter) |
| ui-sketch-geometry-bugs | 8 | viewport.rs (tool commits and previews, glyphs), engine commands.rs (slot, rect, ellipse, polygon) |
| ui-sketch-create-dialogs | 8 | forms.rs (SK_TEXT, SK_INCLUDE, SK_PROJECT_SURFACE, SK_INTERSECTION_CURVE), dialogs.rs (SkMirror, SkCircPattern, SkRectPattern), menus.rs |

## Not covered
2-Tangent and 3-Tangent Circle; Circumscribed Polygon and 2-Point Rectangle / Center Diameter
Circle / 3-Point Arc / Line (covered by the first prober); Text after the frame is placed; Mirror
and the patterns with selections and their previews; Spun Profile and Isoparametric Curve dialogs
in use; angular and diameter dimensions and dimensions between two entities; typed values in the
heads-up boxes and Tab order (the "Heads-up input" coverage line); the Feature Options per tool
(the "Sketch Palette" coverage line).
