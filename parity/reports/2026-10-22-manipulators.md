# Parity report: feature manipulators, Edit Feature and feature errors (2026-10-22, prober 10)

Area: "Manipulators for every feature" and the start of "Edit Feature for every feature; feature
errors and warnings in the timeline" (Design workspace: solid). Fusion in inches on prober 8/9's
"Untitled(1)" design (primitives, a plane sketch, a box); FissionOxide built from `main` at
`2a6b280b`, in mm, on a new design (a 26 x 30 mm rectangle extruded 25 mm with a -3 deg taper,
then a 4 mm fillet on a vertical edge). Manipulators were read from native-resolution crops
(`CopyFromScreen`, 3x nearest-neighbour) before, on hover, during and after a drag, and after Tab.

## Re-verified
- `ui:dialogs:floating-value-box` verified: focused box at the arrow, value selected, typing
  replaces it, Enter is OK.
- `ui:canvas:preview-color` verified: previews in the body's appearance, picks in blue.
- `ui:dialogs:fillet-fields-2`, `ui:dialogs:shell-type-toggles`, `ui:dialogs:press-pull-offset-face`
  verified (rows), with the leftovers noted on each row.
- Not re-checked this session (carried to the next prober): `ui:sketch:horizontal-conflict`,
  `ui:sketch:dimension-label-and-hover` (hover part), `ui:sketch:tangent-arc-snap-marker`,
  `ui:canvas:vertex-marker-style`, and the `fixing-merged` keys of waves 22-24 outside this area.

## Matches
- Enter in the floating box applies the value and clicks OK (`ui:manip:enter-is-ok`).
- Typing into the box previews live and fills the dialog row (`ui:manip:typed-live-preview`).
- Draft shows no floating box until its handle is used (`ui:manip:draft-box-on-drag`).
- A failed feature's timeline icon is pink with a red border (`ui:timeline:error-icon-fill`).

## Differences
P1
- `ui:manip:extrude-taper-handle`: FissionOxide's taper dial sits on the distance arrow's tip, so a
  press on the arrow tip drags the taper (-3 deg) instead of the distance. Fusion's taper handle is
  a separate disc on an arc at the profile edge. `crates/ui/src/dialogs/manip.rs:426`.
- `ui:manip:tab-cycles-boxes`: Tab in Fusion's box jumps to the taper box at its handle (handle
  blue, arrow grey); FissionOxide drops focus. `crates/ui/src/value_box.rs:87`.
- `ui:manip:value-box-covers-arrow`: Fillet, Shell and Offset Face put the box over their arrow, so
  it cannot be seen or dragged. `crates/ui/src/viewport.rs:4859`.
- `ui:manip:draft-angle-handle`: a press on FissionOxide's Draft ellipse starts a window selection;
  Fusion's disc-on-arc handle drags the angle with a dashed hover ring and a box during the drag.
- `ui:timeline:edit-rollback`: Fusion rolls the timeline back while editing; FissionOxide keeps
  later features applied. `crates/ui/src/panels.rs:3737`.
- `ui:dialogs:edit-selection-rows`: FissionOxide's edit dialogs show "(unchanged)" text where Fusion
  keeps live chips and the set row. `crates/ui/src/dialogs/solid.rs:180`.
- `ui:dialogs:failing-value-ok-disabled`: Fusion greys OK and shows the "1 error(s)" toast with the
  message; FissionOxide shows red text in the dialog and lets OK commit a failed fillet.
- `ui:timeline:error-toast`: Fusion's toast ("1 error(s) / Fillet1 / More Info") and status-bar
  badge after an upstream edit breaks a feature; FissionOxide has neither.

P2
- `ui:manip:arrow-style` (flat line and orange hover vs Fusion's 3D blue arrow, dashed drag track,
  grey when inactive), `ui:manip:distance-dimension-label` (Fusion's grey "1.30" dimension in the
  preview), `ui:manip:drag-snap-readout` ("Snap To : ..." tooltip and geometry snapping; box text
  unselected during the drag), `ui:manip:typed-value-units` ("2" becomes "2 in"; no spinner on a
  length box), `ui:manip:preview-hover-highlight` (FissionOxide pre-highlights the preview's faces
  and edges), `ui:manip:arrow-length-offscreen` (a 100 mm radius pushes the box onto the timeline),
  `ui:manip:zero-default-value` (Fillet and Shell start at 0.00 in in Fusion, 2 mm in FissionOxide),
  `ui:dialogs:edit-manipulators` (no taper handle while editing).

Already recorded elsewhere, not duplicated: the "EDIT FEATURE" title (`ui:dialogs:edit-feature-title`,
`ui-primitive-dialogs`), primitive manipulators (`ui:dialogs:primitive-manipulators`), Offset Face
and Draft rows (`ui-draft-scale-dialogs`). Offset Face now shows an (unfocused) box after the pick,
which `ui:dialogs:offset-face-rows` says it did not.

## Seen but not recorded
- Fusion's Extrude and Offset Face opened with 0.601 in and 0.524 in, apparently the last values
  used in this Fusion session; not a reproducible default, so not compared.
- FissionOxide's Draft reported "a face can't be drafted (is it parallel to the pull direction?)"
  on a face tapered -3 deg by the extrude; Fusion was not tried on a tapered face.
- The Enter that committed FissionOxide's Extrude left "1 Face" in the status readout.

## Traps
- Moving the mouse with `mouse_move` and then pressing with `left_mouse_down` in a later call
  pressed in Fusion (it came to the front and started a window selection). Keep the move and the
  press in one `computer_batch`.
- `powershell -File` needs `-ExecutionPolicy Bypass` for scratchpad scripts here.
- FissionOxide's window narrows only from x = window edge + 2 (the invisible resize border).

## Batches (wave 25, `parity/batches/map-wave25.json`)
| Batch | Items |
|---|---|
| `ui-extrude-manipulator` | arrow-style, extrude-taper-handle, distance-dimension-label, drag-snap-readout, tab-cycles-boxes, typed-value-units, preview-hover-highlight |
| `ui-feature-handles` | value-box-covers-arrow, arrow-length-offscreen, draft-angle-handle, zero-default-value |
| `ui-edit-feature-errors` | edit-rollback, edit-selection-rows, edit-manipulators, failing-value-ok-disabled, error-toast |

## Not covered
Revolve's angle manipulator, Chamfer, Scale's arrow, Move's triad (verified earlier), the
primitives' manipulators (open batch `ui-primitive-dialogs`), Offset Plane, Hole, patterns,
manipulator snapping with Incremental Move and keyboard nudges; Edit Feature for every feature
other than Extrude, Fillet and Box; yellow warnings; the error toast's More Info.
