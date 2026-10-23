# 2026-10-22: re-verification sweep, canvas to manip and sketch (verifier 3)

Re-checked the `fixing-merged` keys of canvas, navbar, viewcube, marking-menu, browser, timeline and
manip on `main` at `afddd332` (release build), then the sketch keys, six of them on a rebuild at
`3e4cc76d` (the `ui-reverify-sketch-1` fixes landed after the first build). Work was done from each
row's `fusion` text; Fusion was looked at for the Create Sketch plane preview.

## Counts

| Area | fixing-merged at start | verified | reopened / new | left |
|---|---|---|---|---|
| canvas | 19 | 17 | 2 | 0 |
| navbar | 13 | 11 | 2 + 1 new | 0 |
| viewcube | 1 | 1 | 0 | 0 |
| marking-menu | 7 | 6 | 0 | 1 (`flick`: needs a right-button drag the tool cannot do) |
| browser | 10 | 10 | 0 | 0 |
| timeline | 19 | 17 | 1 | 1 (`feature-owner-component`: needs components) |
| manip | 12 | 11 | 0 + 1 new | 1 (`drag-snap-readout`: no snap seen with one body) |
| chrome | 1 of 14 | 1 (`status-two-faces-angle`, via Draft's two picks) | 0 | 13 |
| sketch | 11 of 18 | 10 | 1 | 7 (+4 from `ui-reverify-sketch-2`, merged after this session's checks) |

Total: 84 verified, 6 reopened, 2 new rows; 180 keys remain `fixing-merged`. (The first commit of this
session, `ae3a2f5c`, says 72 verified; the right figure for that commit is 74.)

## Differences (new batches, wave 39, all `ready`)

`ui-reverify-canvas-1`:
- P1 new `ui:manip:draft-drag-hang`: dragging Draft's angle disc to 40 deg on a face with a hole
  breaking out through it showed "the draft failed" and the app went Not Responding (CPU busy over
  two minutes; killed).
- P1 `ui:canvas:create-sketch-plane-grid-preview`: the plane under the pointer now hovers below the
  horizon, but no grid preview (Fusion draws the hovered plane's labelled grid across the canvas) and
  nothing hovers above the horizon.
- P2 `ui:timeline:error-dialog`: the Error window's icons overlap the first letter of "Fillet1" and
  "Compute Failed".
- P2 `ui:canvas:command-pick-colors`: Extrude's hovered profile fill turns almost white (Fusion: mid
  grey 192,197,193). Fillet's colours match.

`ui-reverify-navbar-1`:
- P2 `ui:navbar:ground-offset-applied`: the Offset row is a drag value that ignores typed digits.
- P2 `ui:navbar:visual-style-list`: Ctrl+5 (Shaded with Hidden Edges) looks like Shaded.
- P2 new `ui:navbar:popup-width`: the Grid menu and the Visual Style flyout are about 450 px wide; the
  flyout opens leftwards.

`ui-reverify-sketch-3`:
- P3 `ui:sketch:dim-box-measure-fill`: Measure from a dimension box's menu shows no result for a
  sketch line and writes nothing into the box.

## Seen working (selection)

Selection and hover colours on faces, edges, bodies and sketch curves; Select Other's Depth / Parents
list; origin planes, axes and names; construction plane look, V, row menus and folder eyes; every
right-click list checked (body, vertex, plane, sketch point, in-sketch); the failed-feature toast,
badge, menus, suppress / unsuppress; timeline drag, marker drag with live rollback, Play, groups and
their tooltip and menu, Shift+click multi-select (works from the automation tool now); Extrude, Fillet
and Draft manipulators; navbar flyouts and grid dialogs; Intersect, Slice, expression-driven HUD values,
the dimension box's recent values, window delete, Feature Options placement and 3D Sketch's triad.

## Not covered

`ui:marking-menu:flick`, `ui:manip:drag-snap-readout`, `ui:timeline:feature-owner-component`; sketch
`vertical-on-horizontal-line` (FissionOxide's Horizontal/Vertical is one tool, so Vertical on a
horizontal line could not be applied separately), `dimension-placement-kept` (Sketch Dimension clicks on
a projected line placed nothing; retry on a plain line), `tangent-line-circle-motion`,
`chamfer-angle-dimension`, `blend-curve-constrained`, `3d-sketch-input`, `project-specified-edge`, and
`ui-reverify-sketch-2`'s four keys; the YZ half of `look-at-sketch-up`. Not compared with Fusion: the
origin axes and plane names drawing over a body at the origin.
