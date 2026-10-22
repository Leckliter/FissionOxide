# 2026-10-22: Construct methods, Inspect, Insert, Configure, Assemble (prober 12)

FissionOxide built from `main` at `c4d97aa9` (release), Fusion "Untitled*(1)" (Part Design, inches)
and a new Fusion "Untitled(2)" (Assembly Design). FissionOxide in a new Part Design (mm). Both
windows left of the Claude overlay.

## What matched
- The Method lists of Fusion's CONSTRUCTION GEOMETRY dialog (8 plane, 5 axis, 6 point methods) are
  FissionOxide's CONSTRUCT entries in the same order (`ui:construct:method-lists`).
- Point Through Three Planes (XY, XZ, YZ): a small orange point at the origin and a point icon at the
  end of the timeline in both (`ui:construct:point-result`).
- Axis Through Cylinder/Cone/Torus, Axis Through Edge, Plane Through Two Edges and Point Through Two
  Edges have the same single or double "Select" rows.
- Re-verified: `ui:canvas:vertex-marker-style` (no mark until hovered, filled light-blue dot with a
  darker ring when selected).

## Differences (wave 27)
P0
- `ui:chrome:new-tab-replaces-design`: "+" replaces the open, unsaved design without a prompt
  (`panels.rs:134` -> `lib.rs:786`). Fusion opens a chooser and a second tab.

P1
- `ui:construct:perpendicular-plane-wrong-dialog`: Perpendicular Plane opens PLANE ALONG PATH
  (`menus.rs:218`).
- `ui:construct:plane-method-rows`, `ui:construct:axis-method-rows`, `ui:construct:point-method-rows`:
  Fusion's methods are all canvas/browser picks ("Select" chips, OK disabled until picked);
  FissionOxide uses plane dropdowns defaulting to XY (OK enabled with a red error), coordinate
  boxes for points, and lacks rows (Tangent Reference, Axis Perpendicular To Face's Point, Chaining).
  Plane at Angle has no Extent/Offset/Resize rows or rotation handle.
- `ui:browser:construction-axes-points-rows`: construction points and axes get no browser row.
- `ui:inspect:section-analysis-dialog`: Fusion picks a cut plane, then Distance, two Angles, Flip,
  Section Color, Show Hatch, an arrow and two rotation handles; OK keeps "Section1" in an Analysis
  folder.
- `ui:inspect:analysis-dialogs-rows`, `ui:inspect:analysis-browser-folder`,
  `ui:inspect:missing-analyses`: Fusion's analyses start with a body pick and have opacity, scale
  and quality rows; they live in a browser Analysis folder. FissionOxide colours every body at
  once, keeps the colours after Close/Escape (even into a new design) with no browser row, and has
  no Curvature Comb, Environment Map, Isocurve or Minimum Radius.
- `ui:assemble:assembly-design-document`: Fusion's "What do you want to design?" chooser and the
  Assembly Design document with its ASSEMBLY tab (INSERT, CREATE, RELATIONSHIPS, MOTION, MODIFY,
  CONTACT...). FissionOxide has only Part Design.
- `ui:insert:file-chooser`: Fusion's Insert window (projects + "Insert from my computer...") for
  Decal, Canvas, SVG and Mesh, a docked INSERT DXF dialog, "Select Source" for Derive. FissionOxide:
  Decal and Derive unimplemented, SVG/DXF/Mesh are the STEP file dialog, Canvas a floating window.

P2
- `ui:inspect:measure-two-picks`: Fusion's black arrow with numbered tags and per-pick Selection
  sections vs FissionOxide's orange line and ΔX/ΔY/ΔZ rows; the dialog jumps after the first pick.
- `ui:inspect:interference-dialog`: body pick and Include Coincident Faces vs Result / Create
  components / Create Bodies.
- `ui:inspect:center-of-mass-dialog`: a Selection pick and a kept CenterOfMass1 row vs an
  all-bodies readout.
- `ui:assemble:assembly-new-component`: the assembly New Component (Design Type, Part Type,
  Design Name "External1", Location).

Blocked
- `ui:configure:configure-table`: Fusion shows "Upgrade to a Fusion subscription" for Configure on
  this Personal licence.

## Batches

| Batch | Items | Files |
|---|---|---|
| ui-construct-methods | 5 | forms.rs, menus.rs, panels.rs |
| ui-inspect-measure-section | 3 | dialogs.rs, viewport.rs, scene.rs |
| ui-inspect-analyses | 4 | dialogs.rs, lib.rs, menus.rs, panels.rs |
| ui-insert-new-design | 4 | panels.rs, lib.rs, menus.rs, canvas_ui.rs, file_dialogs.rs |

`ui-construct-methods` builds on `ui:construct:construction-geometry-dialog` (batch
`ui-construction-display`, being fixed when this was written).

## Retractions
None.

## Not covered
- Fusion's Tangent Plane on a sphere never took a reference pick (XY in the browser or the canvas,
  or Plane1); the rows were recorded, not the result.
- Measure angle between two planar faces; Measure on edges.
- Completing Curvature Comb, Map, Zebra, Draft, Isocurve, Accessibility and Minimum Radius in Fusion
  (only the dialogs before a pick); Interference with overlapping bodies.
- Completing any Insert (no files were chosen); Canvas calibrate (`ui:insert:canvas-calibrate`).
- Joints, motion, contact sets: an empty Fusion Assembly Design has nothing to joint, and creating
  external components would write cloud designs. Next: insert two saved designs into an assembly,
  or use a Hybrid Design.
- Re-verify of `ui:sketch:horizontal-conflict`, `ui:sketch:dimension-label-and-hover` (hover),
  `ui:sketch:tangent-arc-snap-marker`.
