# Parity batch: ui-select-menus-and-other

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (Fusion default light theme; FissionOxide `main` at `548bc6c`).

## selection filters list (P1)
- key: `ui:menus:selection-filters-list`
- Fusion: SELECT > Selection Filters lists, all checked by default: Select All, Select Through, Bodies, Body Edges, Body Faces, Body Vertices, Canvas, Components, Construction Geometry, Coordinate Systems, Custom Graphics, Decal, Dimension, Features, Joint Origins, Joints, Mesh Bodies, Mesh Face Groups, Mesh Faces, PMI, Sketch Curves, Sketch Geometry Constraint, Sketch Points, Sketch Profiles, Sketch Surface, T-Spline Body, Text Selection. The submenu's tooltip reads "Specifies which object types can be selected. Use Select All to include or exclude all object types. Check only specific types to limit selection to those types. Check Select Through to enable selection of hidden objects during Window or Freeform selection."
- FissionOxide: Bodies, Faces, Edges, Vertices (unchecked), Sketches, Sketch Profiles, Construction.
- Start here: `crates/ui/src/menus.rs:313-321` (`SELECTION_FILTERS`) and `crates/ui/src/select.rs` (the filter set). Use Fusion's names and default every type on.

## selection priority list (P2)
- key: `ui:menus:selection-priority-list`
- Fusion: SELECT > Selection Priority lists Select Body Priority, Select Face Priority, Select Edge Priority, with no check mark on the current one.
- FissionOxide: Adds a fourth entry, Select Component Priority, and shows a check mark on Select Face Priority.
- Start here: `crates/ui/src/menus.rs:306-311` (`SELECTION_PRIORITY`).

## selection tools list (P2)
- key: `ui:menus:selection-tools-list`
- Fusion: SELECT > Selection Tools lists Select By Name, Select By Boundary, Select By Size, Invert Selection, Seed And Boundary.
- FissionOxide: Select All (Ctrl+A), Select Other (Alt+click), Invert Selection.
- Start here: `crates/ui/src/menus.rs:301-305` (`SELECTION_TOOLS`). Add the missing commands (a dialog each is fine to start) and drop Select All and Select Other from this list.

## select other popup (P2)
- key: `ui:canvas:select-other-popup`
- Fusion: Pressing and holding the left button on a body opens a small list at the pointer with two tabs, "Depth" (selected) and "Parents", listing one row per entity under the pointer by type ("Face", "Face"). Nothing is selected on the press; the list stays after release until a row is picked or Escape; hovering a row highlights it. Alt+click simply selects the face, status "1 Face | Area : 0.80 in^2".
- FissionOxide: The face is selected on the press ("1 Face | Area: 933.64 mm^2"); after about two seconds a "SELECT OTHER" popup opens on release listing "Face 6 (Plane) of Body1", "Face 2 (Plane) of Body1", "Body: Body1". Alt+click also opens it.
- Start here: `crates/ui/src/select.rs:209-300` (the list and its header) and `crates/ui/src/viewport.rs:3307-3345` (press-and-hold, Alt+click). Open while holding, Depth and Parents tabs, type-only rows, no selection on press, no Alt+click.

## multi-selection status text (P2)
- key: `ui:canvas:multi-select-status`
- Fusion: One face reads "1 Face | Area : 0.80 in^2" (spaces around the colon); two faces from a window box read "2 Faces" with no area; nine faces from a crossing box read "Multiple selections | 9".
- FissionOxide: "1 Face | Area: 933.64 mm^2" and "2 Faces | Area: 1416.14 mm^2"; there is no "Multiple selections | N" form.
- Start here: `crates/ui/src/panels.rs` (status bar readout for selections). Match the spacing and the two-or-more forms.

## box selection colors (P2)
- key: `ui:canvas:box-select-colors`
- Fusion: The left-to-right window box is a peach fill (253,223,185) with a 1 px solid purple-grey border (170,152,195); the right-to-left crossing box is a pale yellow fill (255,253,202) with a dashed dark border (55,61,47). Same in a sketch and in 3D.
- FissionOxide: The window box is a light blue fill with a solid blue border; the crossing box is a light green fill with a solid green border.
- Start here: `crates/ui/src/viewport.rs:335` ("left to right: window (blue, solid); right to left: crossing (green)").

## sketch modify menu list (P2)
- key: `ui:sketch:modify-menu-list`
- Fusion: The sketch MODIFY dropdown lists Fillet, Chamfer (submenu), Blend Curve, Offset (O), Trim (T), Extend, Break, Sketch Scale, Move/Copy (M), Change Parameters.
- FissionOxide: The same ten plus Scale to Size, then a group Edit Text, Explode Text, then Remove Duplicates, Clean Up; Move/Copy shows no "M" shortcut hint.
- Start here: `crates/ui/src/menus.rs` (the sketch MODIFY list). Drop the extra entries from the dropdown (keep the commands reachable elsewhere if needed) and add the M hint.
