# Parity probe: Sketch Palette remainder, heads-up input, sketch display (2026-10-21, prober 6)

FissionOxide rebuilt from `main` at `d90d1e6` (the only code commit since the previous binary was
`42294b7`), launched fresh, so its document was a new design: one XY sketch with a test line, two
rectangles and an offset. Fusion kept the previous probers' document (a body from the first sketch,
the test sketch on XY). Fusion in inches, FissionOxide in mm.

## Method notes
- Desktop control of `fissionoxide.exe` was denied twice at the start of the session; the
  coordinator asked the user and the third request (after their message) was granted. Fusion's
  side was recorded in the meantime (draft in the shared notes folder).
- The Claude overlay again dictated the window placement: Fusion was restored and narrowed to
  x 0..907 (the maximised window's palette tab sat under the overlay); FissionOxide was dragged to
  x -50..1040 so its palette checkboxes are at x 995. A horizontal title-bar drag works; a drag
  whose end point is over the desktop is refused by the control tier of File Explorer.
- Fusion's collapsed palette tab opens a flyout that closed itself on the first click inside it;
  pinning it with the "«" button at its top-right made it a floating window that stays.
- Fusion's sketch Fillet never took two picks in three tries (each pick selected one line and
  the prompt stayed "Select second line or arc"); its value box is not covered here.
- Fusion's 3D Sketch mode swallowed the first click of a Line while the triad was up; turn the
  box off before drawing test geometry.

## What matched
`ui:sketch:slot-heads-up-boxes`, `ui:sketch:circle-heads-up-box`, `ui:sketch:centerline-tooltip`,
`ui:sketch:feature-options-families` (Rectangle 3, Circle 5, Arc 3, Polygon 3, Slot 5, none for
Ellipse, Conic, Point, Text, Mirror, patterns, Project, Dimension and the MODIFY tools),
`ui:sketch:text-dialog-type-row`, `ui:sketch:finish-restores-camera`,
`ui:sketch:create-modify-menu-lists`.

Verified (`fixing-merged` → `verified`): `enter-ends-rectangle`, `line-hud-angle-precision`,
`dim-edit-param-name`, `offset-dimension`, `polygon-tool-flow` (preview only),
`create-tool-heads-up-boxes` (Rectangle, Circle, Slot, Polygon), `palette-feature-options`,
`grid-axis-labels`, `modify-menu-list`.

Reopened: `profile-color-3d` (fill colours still differ, now in `ui-sketch-linetype-and-colors`),
`look-at-sketch-up` (the XY case: BACK instead of FRONT, now in
`ui-sketch-palette-look-slice-3d`), `point-glyph-style` (no corner marks; rides with
`ui:sketch:line-end-points`). Not re-checked: `ui:sketch:horizontal-conflict`.

## Differences, by priority

### P1
- `ui:sketch:look-at-sketch-up` (reopened): Fusion enters and looks at an XY sketch from the
  front (ViewCube FRONT, +X right, +Y up); FissionOxide from the back (BACK, +X left, Y labels
  upside down). `lib.rs:1461`.
- `ui:sketch:hud-parameter-expression`: Fusion takes "d1*2" in a heads-up box and makes an
  "fx: 2.00" dimension whose box shows "d1 * 2" with an fx tag; FissionOxide shows the text with a
  padlock, then ignores it on Enter (cursor size, no dimension). `viewport.rs:1255`.
- `ui:sketch:hud-letter-first-key`: a leading letter typed into the focused first box is swallowed
  in FissionOxide (digits go in); Fusion takes any character. `viewport.rs:3292`.

### P2
- Palette: `look-at-tooltip` ("Look At" vs "Look at the sketch plane"), `slice-section-look`
  (Fusion hatches the body's section at the plane in magenta with a black outline; FissionOxide's
  box is greyed), `3d-sketch-manipulator` (Fusion's Line gets a plane handle and XYZ arrows, the
  grid hides, and the checkbox greys for Ellipse/Conic/Point/Text/Spline; FissionOxide's box is
  always greyed), `feature-options-line-blend` (Fusion: a "Line" row with Line / Midpoint Line, a
  Blend Curve row, nothing for Spline; FissionOxide: the reverse). `panels.rs:2607-2697`.
- Linetype and colours: `centerline-style` (tan dash-dot vs grey even dashes),
  `construction-color` ((207,174,155) 9/3 vs (128,128,132) 6/6), `construction-on-centerline` (X
  keeps Fusion's centerline flag, clears FissionOxide's), `curve-idle-color` ((6,150,214) /
  (71,173,213) outside vs (30,110,235) / (60,70,90)), `grid-label-style` (rotated labels, no 0, no
  unit, hollow origin circle vs horizontal labels with "0" and "100 mm", a red origin dot, and no
  Y labels right after Create Sketch), `profile-color-3d` reopened ((230,245,250) in and out vs
  (248,255,255) in, (144,168,194) out). `scene.rs:35-50, 689-709`, `viewport.rs:2608`.
- Heads-up and dimension box: `hud-expression-normalised` (Tab turns "1/2" into
  "( 1 / 2 ) * 1 in"), `rectangle-heads-up-layout` (first box by the cursor, second under the
  bottom edge vs boxes on the top and right edges), `dim-box-menu` (recent values + Measure vs
  Change Parameters + Delete), `dim-box-empty-enter` (box stays open with a red underline vs box
  closes and a red message at the bottom-left), `three-point-arc-third-step` ("Place point on arc"
  with no box vs a circle preview, "Specify radius" and a radius box), `circle-diameter-prompt`
  ("Specify diameter of circle" vs "Specify diameter"). `viewport.rs:2342-2360, 3269, 3452, 3841-3862`.

## Other Fusion facts worth keeping
- Project with Selection Filter = Bodies projects the body's silhouette as purple curves
  (189,132,217) with dark-purple filled points at the vertices; the dialog rows are Geometry,
  Selection Filter (specified entities / bodies), Projection Link, OK/Cancel. Not compared in
  FissionOxide (its document had no body).
- Fusion's sketch Fillet prompt after one pick: "Select second line or arc"; the status reads
  "1 Sketch Line | Length : 2.00 in".
- Finish Sketch on a consumed sketch hides it (Sketches folder eye off); editing from the
  timeline rolls back and shows the TOP view for the XY sketch that made the body.
- The dimension box's recent-values list keeps the last four values typed anywhere, with units.

## Retractions
- The first "d1*2" test in FissionOxide was unfair: its fresh document had no parameter d1 yet.
  The retest with d1 present (the 0.5 mm dimension) still ignored the expression.
- Nothing else retracted; no click failure was blamed on the apps without a `WindowFromPoint`
  check (the overlay owned Fusion's palette tab at x 1444).

## Batches
| Batch | Items | Priority |
|---|---|---|
| `ui-sketch-palette-look-slice-3d` | look-at side (reopened), tooltip, slice look, 3D sketch manipulator, Line/Blend/Spline feature options | P1, P2 |
| `ui-sketch-linetype-and-colors` | centerline, construction colour, X on a centerline, curve idle colour, grid label style, profile fill (reopened) | P2 |
| `ui-sketch-hud-expressions` | parameter expression, first letter, Tab normalisation, rectangle layout, kebab menu, empty Enter, 3-point arc step, circle prompt | P1, P2 |

## Not covered
- Fusion's sketch Fillet value box (picks never paired); Offset's box was recorded by prober 4.
- Projected geometry colour in FissionOxide (needs a body); Slice in FissionOxide (greyed).
- Fully constrained colour (already `verified`) and `ui:sketch:horizontal-conflict` were not
  re-run; Look At after a user orbit in Fusion (only after Home was tested in FissionOxide).
- Fusion's heads-up boxes of Midpoint Line, 3-Point Rectangle, Center Rectangle, the other
  circles, arcs and slots are in `ui:sketch:create-tool-heads-up-boxes` from prober 4; only
  Rectangle, Circle, 3-Point Arc, Center to Center Slot and Polygon were re-run here.
