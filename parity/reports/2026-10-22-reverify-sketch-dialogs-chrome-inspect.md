# 2026-10-22: re-verification sweep, verifier 4

Session "Re-verification sweep, verifier 4" re-checked the `fixing-merged` keys left in sketch (11),
dialogs (8), chrome (14), construct (5), inspect (6) and insert (1) on `main` at `ff53fda7` (the release
binary was already current; `cargo build` finished in 15 s). FissionOxide ran at screenshot x 0..907
with the Data Panel closed. Fusion was looked at for the Project / Include dialogs (Sketch1 of
"Untitled*(3)" opened for editing and finished without changes), the profile menu and Preferences
(General, Design and the window size; cancelled). Nothing was saved in either app. Fusion was snapped
to the left half to reach Finish Sketch, so it now sits at x 0..920 behind FissionOxide.

Counts: 33 verified, 9 reopened, 4 new rows, 4 left `fixing-merged`. 138 keys remain `fixing-merged`
(mesh 21, surface 19, sheet 18, render 14, shortcuts 10, toolbar 9, documents 7, manufacture 6,
utilities 5, animation 5, menus 4, workspaces 4, electronics 4, manage 2 and one each in marking-menu,
chrome, context-menus, sketch, status, manip, plastic, assemble, dialogs, timeline).

## Verified
- Sketch (7): `constraint-glyph-context-menu`, `circular-pattern-rows`, `text-dialog-rows` (Arial),
  `dimension-placement-kept` (a dragged value stays put), `blend-curve-constrained`,
  `tangent-line-circle-motion` (circle first, then the joined line: it pivots), `project-specified-edge`.
- Dialogs (6): `flip-icon-toggles` (Draft, Contour Flange), `revolve-all-regions`, `pipe-cut-and-arrows`,
  `edit-selection-rows-others` (Shell, Offset Plane, Mirror), `combine-join-disjoint`,
  `replace-face-chain-panic` (no panic).
- Chrome (9): `home-start-page`, `check-updates-job-status`, `window-title-format`, and the Preferences
  tree, Design, Graphics, Default Units, system (API) and workspace (Render) pages.
- Construct (5): all; every method opens CONSTRUCTION GEOMETRY with Fusion's rows.
- Inspect (5): `measure-dialog-rows`, `analysis-dialogs-rows`, `analysis-browser-folder`,
  `missing-analyses`, `center-of-mass-dialog`.
- Insert (1): `file-chooser`.

## Reopened or new, by batch
- `ui-reverify-sketch-4`: `project-include-menu` (Projection Link greyed in Project To Surface and
  Intersection Curve; Spun Profile's Project Axis / Tolerance greyed, "0 mm" vs "0.01 mm", Flip Result a
  checkbox), `chamfer-angle-dimension` (no distance dimension), `3d-sketch-input` (edge snaps, triad,
  other tools), new `circular-pattern-center-status` (centre click goes into Objects; status reads
  "1 Sketch Line 1 Constraint").
- `ui-reverify-dialogs-4`: `primitive-drawing-extras` (Coil rotation handle; typed Tab / Enter in Box's
  base), `ui:inspect:measure-two-picks` (status "Multiple selections | 3"; a sketch-point pick adds no
  section), new `replace-face-plane-target` (Plane1 not taken as target), new
  `replace-face-splits-lumps` (a two-lump Body1 becomes Body1 + Body3).
- `ui-reverify-chrome-2`: `preferences-general-settings` (eight defaults differ from Fusion's),
  `preferences-unit-display-page` (General precision 0.12 vs 0.123), `preferences-window-look` (579 x 360
  vs 748 x 447 screenshot px: `SIZE` used screenshot px as points), `profile-menu` (453 vs about 108 px
  wide), new `home-unsaved-banner`.

## Left `fixing-merged`
- `ui:sketch:vertical-on-horizontal-line`: Horizontal/Vertical is one tool in both apps and picks by
  direction; re-applying it to a horizontal line adds a second Horizontal glyph. No UI route found.
- `ui:dialogs:remove-components`: the prompt now says "bodies or components"; a Part Design has no
  New Component (as in Fusion), so a component pick was not tried.
- `ui:chrome:comments-expand`: needs a saved design.
- `ui:assemble:convert-design-type-assembly`: needs an Assembly Design document.

## Not covered
Hem, Fold and Joint flips; Revolve and pattern edit dialogs; Network / Data Collection pages; Section
Analysis's folder row; Environment Map and Isocurve dialogs; Fusion's Graphics and unit pages (compared
with the recorded `fusion` text only).
