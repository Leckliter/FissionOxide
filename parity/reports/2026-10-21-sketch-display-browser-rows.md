# Parity probe: sketch display remainder, sketch browser rows and context menus (2026-10-21, prober 7)

FissionOxide rebuilt from `main` at `42dec181` (every fixer commit up to `039ebdf7`: selection
feedback, vertex marks, Origin rows, text/mirror/pattern dialogs, scale/move params, fillet /
chamfer / blend, modify tooltips) and launched fresh: a new design with Sketch1 (a rectangle,
extruded 10 mm into Body1) and Sketch2 on XY (the body projected with the Bodies filter, plus a
20 x 15 mm rectangle anchored on the origin). Fusion kept the earlier probers' document (Body1,
Sketch1 consumed, Sketch2 on the top face, Sketch3 on the side face, Sketch4 with text). Fusion in
inches, FissionOxide in mm.

## Method notes
- Desktop control was denied three times (all four apps) before the coordinator asked the user;
  the fourth request, after the user's message, was granted. Nothing could be recorded meanwhile.
- The running FissionOxide was the stale `d90d1e6` build; it was killed (`taskkill`), rebuilt in
  release in the background while Fusion's side was probed, and relaunched. The new window opened
  at x 66..1157, under the Claude overlay: a title-bar drag to the left put the browser off-screen,
  so it was dragged back and narrowed from its bottom-right corner (the right edge alone did not
  take the drag) to x 8..1010.
- Fusion's sketch row menu opens upward (its bottom edge at the pointer) when the window has no
  room below; the entries were clicked by their offset from the row.
- Escape on Fusion's open Units dropdown in EXPORT DXF cancels the whole dialog; the next click
  then landed on the canvas and deselected the sketch.
- Pixel samples: `px.ps1` in the session scratchpad; screen px = screenshot px x 1.3187.

## What matched
- Sketch display: a fully constrained curve draws black in both apps (FissionOxide (20,20,24) once
  the palette reads "Fully constrained"; a free curve (30,110,235)); `ui:sketch:fully-constrained-color`
  re-verified.
- Browser: `ui:browser:sketch-row-show-dimensions` (black dimensions in 3D),
  `sketch-row-hide-profile`, `sketch-row-look-at`, `sketch-row-rename` (inline editor, name selected),
  `sketch-row-delete` (timeline row goes too; Ctrl+Z restores), `sketch-row-double-click` (and Edit
  Sketch: SKETCH tab, palette, camera on the plane), `sketch-row-select-status` (row and timeline
  icon boxed, name at the lower right), `ui:sketch:project-dialog-rows-2` (Geometry, Selection
  Filter, Projection Link, OK/Cancel).

Verified (`fixing-merged` -> `verified`): `ui:browser:root-eye-and-origin-eye`,
`document-settings-expanded`, `root-unsaved-label`, `sketches-folder-after-finish`,
`folder-context-menus` (root, Bodies, Origin, Units all open Fusion's lists), `origin-rows`,
`ui:sketch:curve-hover-color` ((74,130,252), 5 px), `curve-selected-color` ((3,123,253), status
"1 Sketch Line | Length : 15.00 mm"), `canvas-background` (white), `line-end-points` ((50,50,50)
filled dots), `point-glyph-style`, `fully-constrained-color`. Not re-checked:
`ui:sketch:horizontal-conflict`, `dimension-label-and-hover` (the label part looks right: plain
dark text, no box; the hover was not tried).

## Differences, by priority

### P1
- `ui:sketch:projected-geometry-color`: Fusion draws projected curves purple (189,132,217) with
  dark-purple filled points; FissionOxide draws them black (20,20,24), the fully constrained colour,
  with no points. `crates/engine/src/scene.rs:47` (`SKETCH_FIXED`).
- `ui:browser:sketch-row-menu-list`: Fusion's 20-entry sketch menu (Move to Group, Create Selection
  Set, Extrude E, Offset Plane, Edit Sketch, Redefine Sketch Plane, Select Sketch Plane, Slice
  Sketch, Configure, Export DXF, Delete Del, Rename, Look At, Hide Profile, Show Dimension, Hide
  Projected Geometries, Hide Construction Geometries, Show/Hide V, Find in Window, Find in Timeline)
  against FissionOxide's 11 (no Extrude, Offset Plane, Select Sketch Plane, Configure, the two Hide
  … Geometries, Find in Timeline, Move to Group, Create Selection Set; "Save As DXF"; no shortcut
  hints; opens downward). `crates/ui/src/panels.rs:2159`.
- `ui:browser:sketch-row-eyes`: no eye on the Sketches folder or any sketch row (Fusion: eye on each,
  greyed when hidden). `crates/ui/src/panels.rs:2303`.
- `ui:browser:sketches-folder-menu`: right-click on the Sketches folder opens nothing (Fusion: New
  Sketch Group, Create Selection Set, Show/Hide V, Show All). `crates/ui/src/panels.rs:2303`.
- `ui:browser:redefine-sketch-plane` (fox-missing): greyed "Not in FissionOxide yet" (Fusion: a
  Plane [Select] dialog, timeline rolled back, prompt "Select a planar face or a work plane", live
  preview on the picked face, status "1 Face | Area : 1.44 in^2"). `crates/ui/src/panels.rs:2165`.
- `ui:browser:export-dxf-dialog`: FissionOxide goes straight to the Windows Save As (Fusion: EXPORT
  DXF with Sketch, Units (10 units), Visible Geometry: Points / Projected Geometries /
  Construction Geometries, then its own Export DXF window with Name, Type, cloud project or
  computer path). `crates/ui/src/panels.rs:2215`.

### P2
- `ui:browser:sketch-row-hover`: tooltip "Double-click to edit" and status text on hover, no canvas
  highlight (Fusion: canvas highlight, no tooltip, no status). `crates/ui/src/panels.rs:2340`.
- `ui:browser:sketch-row-selected-canvas`: the selected sketch's profiles keep the light fill
  (Fusion: solid blue fill). `crates/ui/src/panels.rs:2330`.
- `ui:browser:consumed-sketch-hidden`: Sketch1 stays visible under the body (Fusion hides a consumed
  sketch). `crates/ui/src/lib.rs:1398`.
- `ui:browser:units-row-mass-unit`: "Units: mm" (Fusion "Units: in, ouncemass").
  `crates/ui/src/panels.rs:1453`.
- `ui:sketch:project-filter-order`: Bodies first and default, then "Faces and edges" (Fusion:
  Specified Entities first and default, then Bodies). `crates/ui/src/dialogs.rs:3848`.
- `ui:sketch:project-body-pick-chip`: with the Bodies filter the chip reads "1 face" and the status
  "1 Face | Area : 473.77 mm^2" (Fusion lights the body, status "Body1").

## Retractions
None this session.

## Batches
| batch | items | keys |
|---|---|---|
| `ui-browser-sketch-rows` | 7 | sketch-row-menu-list, sketch-row-eyes, consumed-sketch-hidden, sketches-folder-menu, sketch-row-hover, sketch-row-selected-canvas, units-row-mass-unit |
| `ui-sketch-redefine-and-dxf` | 2 | redefine-sketch-plane, export-dxf-dialog |
| `ui-sketch-project-colors` | 3 | projected-geometry-color, project-filter-order, project-body-pick-chip |

## Not covered
- Fusion's Move to Group, Configure, Extrude and Offset Plane from the sketch row (listed, not run);
  Slice Sketch's effect on a plane that cuts the body (the test sketch's plane cut nothing);
  Find in Window in Fusion; the hover ring on a sketch point; `ui:sketch:horizontal-conflict`.
- Edit Sketch on a sketch that is not the last feature in FissionOxide (the timeline rollback).
- Fusion's hover highlight colour on a sketch row was seen but not pixel-sampled.
