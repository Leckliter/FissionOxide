# Parity probe: sketch constraints, Sketch Palette options, Line heads-up (2026-10-21, prober 5)

Both apps in an XY sketch, Fusion in inches with Layout Grid off, FissionOxide in mm, on
FissionOxide `main` at `d99603d` (binary from 17:58, after `788352e`). Test geometry drawn fresh
in each app: two separate lines and a small circle (Fusion at screenshot x 380..640, y 530..600;
FissionOxide at x 600..810, y 540..630). Fusion's Linetype had been left on Construction by the
previous session; X on the three curves put them back to normal.

## Method notes
- Fusion's second pick "missed" three times because my screenshot reading of a line was 20 px
  off; a vertical pixel scan (`px.ps1 "659,600,scanv,150"`) found the line's real row. Scan before
  calling a pick refused.
- FissionOxide's constraints need a pre-selection. Shift+click from the tool is missed (modifier in
  the click frame, known), so the selections came from window boxes, which also select the
  enclosed points; the engine ignores the extra points for two-curve constraints but takes
  `pts[0]` for MidPoint and Symmetry, so those results depend on box order.
- The Claude overlay swallowed every click on FissionOxide's palette checkboxes (x = 1046 >= 1040)
  until the window was dragged 120 px left. Two batches of "dead" checkbox clicks were the overlay,
  not the app.

## What matched
`ui:sketch:constraints-menu-list` (13 entries, same order), `ui:sketch:constraint-delete-key`,
`ui:sketch:midpoint-glyph` (triangle above the point), `ui:sketch:palette-rows-and-toggles`
(row list; Sketch Grid, Snap, Dimensions, Constraints toggles; Options collapse),
`ui:sketch:line-heads-up-tab` (Tab order, selection on arrival, padlock, Enter).

Re-verified: `ui:sketch:heads-up-units-and-lock`, `ui:sketch:glyph-style-create-tools`,
`ui:sketch:palette-linetype` (two icons; its leftover footer moved to a new key).
Reopened: `ui:sketch:constraint-tool-flow` and `ui:sketch:constraint-glyphs` (both had been
recorded as matches from the code, not the screen).

## Differences, by priority

### P0
- `ui:sketch:constraint-pick-command`: Fusion's constraints are live pick commands (icon lit,
  prompts "Select sketch objects or change constraint type" / "Select other geometries" /
  Symmetry "Select second sketch element", "Select a symmetry line"; the command stays active,
  a ribbon icon switches the type). FissionOxide applies the entry to the current selection and
  otherwise prints "'coincident' needs 2 point(s)" in red. `panels.rs:3419`.

### P1
- Solver motion (`ui-sketch-constraint-solver`): Fusion keeps the first pick and moves the second
  line rigidly (Coincident), rotates the second line about the shared point (Parallel,
  Perpendicular, Collinear), moves only the free circle (Concentric), pivots the line until its
  extension touches the circle (Tangent). FissionOxide pulls both coincident points to their
  midpoint, stretched both lines several times over for Perpendicular, dropped both lines to
  horizontal for Collinear, and dragged the already-constrained circles and lines for
  Concentric. `crates/sketch/src/solve.rs:68`.
- `ui:sketch:constraint-glyph-placement`: Fusion draws one glyph per line under the midpoint for
  Parallel/Collinear, an "=" box on each line, a single corner glyph for Perpendicular, "[ ]" on
  both elements and the axis for Symmetry, and a coincident glyph only while the merged point is
  hovered; FissionOxide draws one glyph per constraint above-right of its anchor.
  `viewport.rs:2490-2525`, `glyph_anchor` at `viewport.rs:2906`.
- `ui:sketch:fix-unfix-flow`: Fusion's fixed line turns green with no lock glyph and a second
  pick in the same command unfixes; FissionOxide paints it black with a padlock at each end.
- `ui:sketch:palette-disabled-rows`: Slice, Profile, Points, Projected Geometries, Construction
  Geometries and 3D Sketch are greyed in FissionOxide; all toggle the canvas in Fusion.
  `panels.rs:2604-2625`.
- `ui:sketch:line-heads-up-layout` and `ui:sketch:line-typed-values-dimensions`: Fusion's length
  box sits above the segment between extension lines, the angle box on an arc from a dashed
  horizontal reference, prompt "Specify next point"; "Tab 40 Tab 1 Enter" leaves a 1.00 linear and
  a 40.0 degree angular dimension plus the dashed reference line. FissionOxide stacks two plain boxes
  at the midpoint ("Place next point") and leaves only the length dimension. `viewport.rs:200`,
  `hud_apply` at `viewport.rs:1245`.

### P2
- `ui:sketch:constraint-glyph-hover` (Fusion: pale blue (189,215,226) box fill, dark ink
  (68,69,69), all glyphs of the constraint light together, geometry not highlighted, no tooltip;
  FissionOxide: accent glyph, geometry highlighted, tooltip "Horizontal (click to select, Del to
  remove)"), `ui:sketch:constraint-selected-status` ("Horizontal Constraint" vs "1 Constraint"),
  `ui:sketch:constraint-glyph-context-menu` (Fusion's marking menu on a glyph: Repeat
  Horizontal/Vertical, Delete, Press Pull, Undo/Redo, Move/Copy, Hole, Sketch; OK, Cancel, Create
  Selection Set, Find in Browser, Find in Window), `ui:sketch:constraint-menu-tooltips` (dark
  description-only panel with "Press Ctrl+/ for more help" vs titled light tooltip that expands),
  `ui:sketch:constraint-menu-icons`, `ui:sketch:constraint-stale-error`,
  `ui:sketch:curvature-polygon-refusal` (Fusion refuses the second line silently; Polygon prompt
  "Select a closed profile with equal sides and angles to co..."), `ui:sketch:tangent-line-circle-motion`,
  `ui:sketch:palette-collapse-chevron` (Fusion docks the palette into a vertical "SKETCH PALETTE"
  tab), `ui:sketch:palette-dof-footer`, `ui:sketch:palette-tooltips` ("Centerline" vs
  "Normal/Construction (X)").

## Other Fusion facts worth keeping
- Undo list after this session: "Undo Equal", "Undo Delete", "Undo Perpendicular", "Undo Parallel",
  "Undo Horizontal/Vertical", "Undo Coincident", "Undo Normal/Construction", "Undo Center Diameter
  Circle", each with the command's icon (Coincident had none). FissionOxide's list was not checked.
- Fusion's glyph in its unhovered state is grey line art in a faint box; a selected glyph is blue.
- Fusion's Fix/UnFix prompt after a pick: "Select sketch objects to constrain"; the generic one
  before any pick: "Select sketch objects or change constraint type" (H/V: "...or constraint type").
- Status readouts: picking a line "1 Sketch Line | Length : 0.489 in"; two points
  "2 Sketch Points | Min Distance : 0.549 in"; a hovered point "Sketch Point | X: -1.293 Y: -1.628 Z: 0.00 in".

## Retractions
- Two rounds of FissionOxide palette clicks (Snap, Dimensions, Constraints, Profile, chevron)
  were swallowed by the Claude overlay at x >= 1040; after moving the window the toggles work.
  Only the greyed rows, the chevron and the footer are real.
- Fusion's "missed" second picks were my coordinate misreads (lines 20 px off), not Fusion.
- A Ctrl+Z issued after a Symmetry whose third pick had not landed undid the earlier UnFix
  instead; Fusion's line was green for two batches as a result.

## Batches
| Batch | Items | Priority |
|---|---|---|
| `ui-sketch-constraint-flow` | pick command, stale error, fix/unfix, curvature/polygon refusals, menu icons | P0, P1, P2 |
| `ui-sketch-constraint-solver` | coincident, parallel/perpendicular, collinear, concentric, tangent motion | P1, P2 |
| `ui-sketch-constraint-glyphs` | placement, hover, selected status, right-click menu, menu tooltips | P1, P2 |
| `ui-sketch-palette-options` | greyed rows, chevron, DOF footer, tooltips | P1, P2 |
| `ui-sketch-line-heads-up` | layout, typed values create both dimensions | P1 |

## Not covered
- Horizontal/Vertical on two points, Symmetry and MidPoint through a real pick flow in
  FissionOxide (the pre-selection order decided the result), Curvature with a spline, Polygon on a
  real polygon, constraint glyphs' right-click menu in FissionOxide, the Undo list names in
  FissionOxide, Fusion's hover of a coincident point in FissionOxide.
- Sketch Palette: Look At's effect, Slice, 3D Sketch, Linetype's Centerline, Feature Options per
  tool (only seen for Circle and Blend Curve by earlier probers).
- Heads-up input: only the Line tool; the other tools' boxes, expressions, parameter names in
  boxes, the vertical-ellipsis menu on the dimension edit box.
