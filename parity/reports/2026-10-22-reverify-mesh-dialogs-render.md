# 2026-10-22: re-verification sweep, verifier 6 (mesh, dialogs, render, singles)

Built `main` at `dadbcad3` on Windows as is (no build fix needed) and re-checked the 41 keys that were
`fixing-merged`; after `git pull` twelve more had been merged (`ui-reverify-dialogs-1` ce56bfa,
`ui-reverify-navbar-1` fddb0df, `ui-reverify-sketch-3` 67deacb), so the app was rebuilt at `d2202676` and
those were checked too, along with Remesh, Reduce, Smooth, Shell, Combine, Separate and Repair again.
FissionOxide ran at x 0..728 of the 1456-wide screenshot frame, left of the Claude overlay, Data Panel
closed. Fusion was used for the mesh row menu, the canvas ring and Convert Mesh on its own `lprism` mesh
(Untitled*(1)); nothing was saved.

Test files: `lprism.stl` (a 30 x 10 x 5 mm L-prism from prober 14) and `twoboxes.stl` (two 10 mm cubes,
two shells), both copied to `C:\Users\Leckliter\Documents\fox-mesh\`.

## Counts

| | verified | reopened | new rows | left |
|---|---|---|---|---|
| mesh (21) | 13 | 8 | 2 | 0 |
| dialogs (18) | 15 | 3 | 1 | 0 |
| navbar (3) | 3 | 0 | 0 | 0 |
| sketch (2) | 0 | 1 | 0 | 1 |
| render (2) | 0 | 2 | 1 | 0 |
| singles (7) | 4 | 0 | 0 | 3 |
| chrome, after `ui-reverify-chrome-1` (8) | 7 | 1 | 0 | 0 |
| total (61) | 42 | 15 | 4 | 4 |

The eight chrome keys arrived with `f6aaa91d` while this session was writing up; they were checked on a third
build (`3aa0fa4c`): Text Commands bar and View entry, the dark theme round trip, one-click projects, list rows
with their details card, the Open window's tree, the View submenu and the glyph boxes now match; Export still
greys Inventor, SketchUp and SMT and starts at FissionOxide's own archive type (reopened into
`ui-reverify-dialogs-6`).

## Verified

Mesh: mesh body kind, INSERT MESH dialog (units rescale the preview at once, triad, numerical inputs, nothing
added until OK), "Base Mesh Feature1", the body menu's entries, the open-mesh warning after a No Fill Plane
Cut, PLANE CUT, the MODIFY quick buttons, SEPARATE, TESSELLATE, REPAIR, CREATE MESH SECTION SKETCH, the
DIRECT MESH EDITING environment and palette ("twoboxes (1)_Edited"), solid Combine skipping mesh bodies.

Dialogs: whole-body picks, Edit Draft, SPLIT FACE, REMOVE FEATURES (finds the hole), REPLACE WITH PRIMITIVES
(works), Edit Hole, Edit Mirror, Edit Combine; on d2202676 Move/Copy, Hole, Thicken (opens from CREATE),
Boundary Fill, Draft (5 -> "5.0 deg", picks stay blue), Extrude from a body face, and no Preview box in the
solid dialogs. Navbar: Ctrl+5 hidden edges, narrow Grid and Visual Style popups, typed Ground Plane Offset.
Singles: the face context list, the sketch curve status readouts, the Extrude arrow's "Snap To" readout,
Convert Design Type in an Assembly Design.

## Reopened and new (wave 42, all `ready`)

- `ui-reverify-mesh-1` (5): ce56bfa's `has_preview_box` took the Preview row off Remesh, Reduce, Smooth, Shell
  and mesh Combine (Fusion has it there); Remesh previews a melted blob at its defaults (P1); opening Smooth
  freezes the app 5-10 s. On `dadbcad3` Remesh and Reduce rows ran side by side (icon drop-down not ending its
  grid row), fixed by the same commit.
- `ui-reverify-mesh-2` (5): Convert Mesh's extra greyed Organic icon and missing subscription badge, no Scale
  Mesh arrow, Generate Face Groups fills the pick instead of outlining it, mesh body menus grey five entries
  Fusion enables (new), mesh bodies smooth-shaded across sharp edges (new).
- `ui-reverify-dialogs-6` (5): Mirror's original loses its blue once the plane row is active, Remove does not
  take a component's browser row, Chamfer's empty status bar and 1 mm start, Measure from a dimension box
  does not fill the box with an edge length, angle boxes without the decimal (new).
- `ui-reverify-render-2` (3): an applied appearance (Wood - Oak) is not drawn at all (new, P1), Texture Map
  Controls has no canvas handles, the Environment background is never drawn so Flatten Ground shows nothing.

## Left `fixing-merged`

`ui:marking-menu:flick` (no right-button drag in the automation tool), `ui:chrome:comments-expand` (needs a
saved design), `ui:sketch:vertical-on-horizontal-line` (no UI route: H/V is one tool),
`ui:timeline:feature-owner-component` (needs features made inside components).

## Traps

- FissionOxide sometimes stops responding for several seconds (Smooth, and once after a ribbon click);
  Windows then puts an "unidentified" ghost window in front and every click is refused. Waiting rarely helps;
  `open_application("Autodesk Fusion")` brings Fusion's launcher ("Multiple instances are not supported"):
  Cancel it, then click FissionOxide's taskbar button (twice if the first click only minimises Fusion).
- Triple-click does not select a FissionOxide value box; click it and press Ctrl+A before typing.
- The file picker refuses typed text with a newline: type the path, then click Open.
- Snapping FissionOxide with Win+Left gives a 728-px-wide window (in the 1456 frame); dragging its right edge
  past Fusion onto the desktop is refused by the File Explorer click tier.

## Where things stand

FissionOxide (rebuilt `d2202676`) has three tabs: "Untitled*" (lprism mesh), "Untitled(1)*" (a 30 x 20 x 10
box with a sketch of two lines, one dimensioned; Ground Plane Offset 20 mm, Adaptive off), and the earlier
binary's documents are gone (it was relaunched for the rebuild). Fusion is untouched apart from Convert Mesh
opened and cancelled and the lprism row / canvas menus opened on Untitled*(1), now on its MESH tab.
