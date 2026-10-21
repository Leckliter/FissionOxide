# Parity coverage

Status per area: `todo`, `in progress (<date>, <session>)`, `done (<date>)`. Probers take the
first `todo` from the top, mark it, commit, then probe. Areas are ordered by how soon a Fusion
user meets them. Add areas when you find ones that are missing; never delete one.

## Design workspace: chrome
- [done (2026-10-21)] Application bar, document tabs, File menu, undo/redo lists, Help, profile, notifications
- [done (2026-10-21)] Toolbar tabs and panel dropdowns at full width; responsive collapse at narrow widths
- [todo] Tooltips on every ribbon button (short and expanded), pin/unpin, customise shortcuts
- [done (2026-10-21)] Preferences dialog (layout, General); todo: every other Preferences page vs Fusion's
- [todo] Data Panel: projects, folders, upload, versions, search, Recent/Activity/Trash
- [todo] Save, Save As, Open, Open Recent, Export, New Drawing, Insert into Design, Capture Image, 3D Print dialogs
- [todo] Document Settings, units change, Named Views (create, edit, restore), Comments panel
- [todo] Status bar readouts for every selection kind; lower-left messages

## Design workspace: canvas
- [done (2026-10-21)] ViewCube (faces, edges, corners, arrows, menu, home, fade, compass), navigation bar flyouts
- [todo] Camera: orbit/pan/zoom with every mouse preset, Fit, Look At, zoom-to-cursor, animation timing, orthographic/perspective switch
- [done (2026-10-21); not exercised: Body/Edge priority behaviour, Freeform and Paint selection, Select Through] Selection: hover/selected colours for face, edge, vertex, body, sketch curve, sketch point, construction plane, axis; window and crossing boxes; Select Other; selection filters and priority; Escape and empty-click
- [done (2026-10-21)] Marking menus on empty canvas, face, edge; todo: body, vertex, sketch point, construction geometry, timeline feature, in-sketch
- [todo] Display settings: every visual style, environment, effects, object visibility, ground plane, full screen
- [todo] Grid and snaps: layout grid in 3D and sketch, lock, snap, grid settings, incremental move
- [todo] Origin planes/axes visibility, construction geometry display, sketch visibility

## Design workspace: sketch
- [done (2026-10-21)] Create Sketch flow, rectangle, circle, line, trim, offset, polygon, dimension edit, Horizontal
- [done (2026-10-21), remainder todo: 2-/3-Tangent Circle, Text after the frame, Mirror/pattern previews, angular and diameter dimensions] Every CREATE tool: midpoint line, 3-point/center rectangles, every circle and arc type, inscribed/edge polygon, ellipse, every slot, spline (fit and control point), conic, point, text, mirror, circular and rectangular pattern, project/include, sketch dimension (every type)
- [in progress (2026-10-21, prober 4); menu lists compared 2026-10-21 (ui:sketch:modify-menu-list)] Every MODIFY tool: fillet, trim (drag), extend, break, offset (chain, both sides), scale, move/copy, change parameters
- [todo] Every constraint: coincident, collinear, concentric, midpoint, fix/unfix, parallel, perpendicular, horizontal/vertical, tangent, curvature, equal, symmetry, lock; glyph positions and hover; constraint deletion
- [todo] Sketch Palette: every option, linetype, look at, slice, 3D sketch; Feature Options per tool
- [todo] Heads-up input: every tool's boxes, Tab order, units, expressions, lock icon, parameter names
- [todo] Sketch display: grid labels, profile shading colours in and out of sketch, fully constrained colour, construction and centerline styles, projected geometry colour, sketch points
- [todo] Sketch browser rows, context menus, Edit Sketch, Redefine Sketch Plane, Save As DXF

## Design workspace: solid
- [done (2026-10-21)] Extrude, Fillet, Chamfer, Shell, Press Pull, Move, Hole dialogs (first pass)
- [todo] Create: New Component, Revolve, Sweep, Loft, Rib, Web, Emboss, Thread, Box/Cylinder/Sphere/Torus/Coil/Pipe, patterns, Mirror, Thicken, Boundary Fill, Base Feature, Joint Origin
- [todo] Modify: Draft, Scale, Combine, Offset Face, Replace Face, Split Face, Split Body, Silhouette Split, Align, Delete, Remove, Simplify, Physical Material, Appearance, Manage Materials, Change Parameters, Compute All, Direct Edit
- [todo] Manipulators for every feature: arrows, rings, floating value boxes, drag behaviour, snapping
- [todo] Edit Feature for every feature; feature errors and warnings in the timeline
- [todo] Configure, Construct (every plane/axis/point), Inspect (measure, interference, curvature, section, zebra, draft, CoG, CO2), Insert (decal, canvas, SVG, DXF, mesh, STEP, McMaster), Assemble (components, joints, motion, enable contact), Select menu

## Other tabs and workspaces
- [todo] SURFACE tab: every create/modify command and dialog
- [todo] MESH tab: every command
- [todo] SHEET METAL tab: rules, flange, bend, unfold, flat pattern
- [todo] PLASTIC tab
- [todo] MANAGE (BOM) and UTILITIES tabs (make, add-ins, scripts, inspect)
- [todo] Timeline: every item type icon, groups, suppress, rollback, play, context menus, drag, errors
- [todo] Browser: every row type, eye toggles, drag and drop, rename, activate, component rows, joints folder
- [todo] Workspaces: Render, Animation, Simulation, Manufacture (turning, milling tabs), Drawing, Electronics
- [todo] Keyboard shortcuts: every default shortcut in Fusion's list vs FissionOxide's
- [todo] Startup, new design, multiple documents, closing with unsaved changes, recovery, update flow
