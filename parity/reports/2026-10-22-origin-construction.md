# Parity probe: origin planes and axes, construction geometry, sketch visibility (2026-10-22, prober 8)

Fusion: a new design (Untitled(1)), Y up, inches. FissionOxide: a new design, built from `main` at
`0c453dd1`, Z up, mm. Both windows left of the Claude overlay. The browser's Origin folder, its
seven rows, an Offset Plane, Create Sketch's plane chooser and a finished sketch were driven in
both apps; pixel crops were taken at native resolution and upscaled.

## Re-verified
- `ui:canvas:origin-axes-always-drawn`: verified (no axis lines until Origin is shown).
- `ui:canvas:construction-plane-colors`: verified (hover grey, selected blue, status "XY Plane").
- `ui:canvas:construction-axis-colors`: verified for the axis (hover solid blue, selected blue,
  status "X Axis"); the look of the axis and point lines moved to `ui:canvas:origin-axis-style`.
- `ui:canvas:origin-planes-size`: reopened. Its old text said Fusion centres the squares on the
  origin; on screen today Fusion puts each square in the positive quadrant of its two axes, both
  with Origin shown and in Create Sketch. FissionOxide centres them.

## What matched
`ui:construct:construct-menu-list` (CONSTRUCT entries and order), `ui:dialogs:create-sketch-selected-plane`
(Create Sketch starts at once on a selected construction plane). Origin hidden by default, the
greyed child eyes while the folder is off, and the Sketches folder position were already verified.

## Differences
P1:
- `ui:canvas:origin-planes-size` (reopened): positive-quadrant squares with a gap along the axes
  against centred squares that cross into a star. `crates/ui/src/origin.rs:266`.
- `ui:canvas:origin-draw-order`: Fusion's planes are see-through, hidden behind bodies, with the
  axes and point on top; FissionOxide paints near-opaque planes over everything, covering the axes,
  the origin point and any body at the origin. `crates/ui/src/origin.rs:274`.
- `ui:canvas:origin-in-sketch`: Fusion keeps the origin items drawn inside a sketch; FissionOxide
  hides them. `crates/ui/src/origin.rs:133`.
- `ui:canvas:create-sketch-plane-grid-preview`: Fusion's plane chooser (Create Sketch and the
  primitives) treats the origin planes as infinite, so any pointer position hovers one, and previews
  its labelled sketch grid; FissionOxide picks only the squares and previews nothing.
  `crates/ui/src/viewport.rs:4698`.
- `ui:canvas:construction-plane-display`: Fusion draws Plane1 as a translucent peach square with
  hover and selected looks; FissionOxide draws a dashed outline sized from the model, invisible in
  an empty design, with no hover or selected look. `crates/engine/src/scene.rs:531`.
- `ui:browser:construction-folder-eyes`: no eyes on the Construction folder or its rows; the folder
  appears during the preview. `crates/ui/src/panels.rs:2371`.
- `ui:canvas:visibility-v-key`: V on a selected plane is refused ("Show/Hide: select bodies, faces
  or edges first"). `crates/ui/src/lib.rs:1296`.
- `ui:construct:construction-geometry-dialog`: Fusion's Offset Plane now opens a unified
  "CONSTRUCTION GEOMETRY" dialog (Type, Method, Plane, Extent, Distance, Resize); FissionOxide's is
  "OFFSET PLANE" with two rows. Found here; it belongs to the Construct area.

P2:
- `ui:canvas:origin-axis-style`: pure-colour dash-dot positive half-axes ending in a dot and a
  white-filled origin ring, against pastel even dashes and a hollow ring.
- `ui:browser:origin-row-hover-canvas`: hovering an origin or construction row greys the plane in
  Fusion's canvas. (The verified `ui:browser:origin-rows` says hovering a row does not highlight
  the canvas; that was observed with Origin hidden, where nothing is drawn.)
- `ui:browser:construction-plane-menu`: twelve entries against three.

## Retractions
None.

## Batches (wave 22)
| batch | items | keys |
|---|---|---|
| `ui-origin-display` | 6 | origin-planes-size, origin-draw-order, origin-axis-style, origin-in-sketch, origin-row-hover-canvas, create-sketch-plane-grid-preview |
| `ui-construction-display` | 5 | construction-plane-display, construction-folder-eyes, visibility-v-key, construction-plane-menu, construction-geometry-dialog |

## Not covered
Construction axes and points (only a plane was made), Origin Plane Names and the other Object
Visibility rows (their list is `ui:navbar:object-visibility-list`), sketch visibility beyond the
Sketches folder eye (the sketch row eyes are `ui:browser:sketch-row-eyes`), the colour of a
hovered origin point in FissionOxide, the vertex marker style (`ui:canvas:vertex-marker-style`
stays fixing-merged: the pointer landed on a plane in Fusion).
