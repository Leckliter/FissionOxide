# Parity batch: ui-mesh-body-and-insert

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 14) in an unsaved Part Design document in each app (Fusion in
inches, FissionOxide in mm), FissionOxide built from `main` at `25d185b8`. Both apps inserted the
same file, `tests/reference/464-stl-millimeter/lprism.stl` (an L-shaped prism, 12 triangles, in mm),
with MESH > CREATE > Insert Mesh. The Insert window and the Windows file dialog's filter are already
covered by `ui:insert:file-chooser` (batch `ui-insert-new-design`); these items start after the file
is picked. This batch is the base the other wave-29 mesh batches build on: FissionOxide has no mesh
body kind yet. See `parity/reports/2026-10-22-mesh-tab.md`.

## Mesh bodies are their own body kind (P1)
- key: `ui:mesh:mesh-body-kind`
- Fusion: An inserted STL stays a mesh body: its browser row is named after the file ("lprism") with a yellow triangulated mesh-body icon; a click anywhere on it selects the whole body (blue), never a single face; it is drawn smooth-shaded with no face edges. Mesh commands (Remesh, Reduce, Plane Cut, Smooth, Shell, Separate, Scale Mesh, Convert Mesh, Repair, Generate Face Groups) take it with their "Body/Face Groups" or "Body" chip pre-filled as "1 selected"; solid commands do not see it as a solid until Convert Mesh.
- FissionOxide: The STL becomes a faceted solid named "Body1" with the grey solid-body icon (`exchange` describes STL as "imported meshes become faceted bodies"); a click picks one planar face ("1 Face | Area : 400.00 mm^2") and the face marking menu (Create Sketch, Extrude, Offset Plane, Shell) opens on it.
- Start here: `crates/exchange/src/lib.rs:98` (STL/OBJ/3MF/PLY read as faceted bodies), `crates/engine/src/files.rs:112` (the import path that builds the body; keep the triangles as a mesh body and name it after the file), `crates/ui/src/panels.rs:1960` (browser row icon: a mesh icon for mesh bodies), whole-body picking in `crates/ui/src/viewport.rs`.

## INSERT MESH dialog after the file pick (P1)
- key: `ui:mesh:insert-mesh-dialog`
- Fusion: After the file is picked, a docked "INSERT MESH" dialog opens with the mesh previewed, selected blue and carrying a move/rotate triad (three arrows, a centre square, rotation rings): Unit Type dropdown (Centimeter, Millimeter, Meter, Inch, Foot; the design's unit, Inch, ticked first), Flip Up Direction (icon button), a Position group with Center and Move To Ground (icon buttons), a collapsed Numerical Inputs group (X Distance, Y Distance, Z Distance "0.00 in", X Angle, Y Angle, Z Angle "0.0 deg"), Reset (icon), OK and Cancel. Choosing Millimeter rescales the preview at once. Nothing is added until OK.
- FissionOxide: The file is inserted at once with no dialog, at the file's own scale, and the status line says "Inserted C:\...\lprism.stl"; there is no unit choice, placement triad or numerical placement.
- Start here: `crates/ui/src/lib.rs:948` (`App::import` / `import_path` run "import" straight away; for mesh files open a new INSERT MESH dialog first, in `crates/ui/src/dialogs.rs`), `crates/ui/src/menus.rs:424` (Insert Mesh is wired to `Cmd::InsertStep`).

## Insert Mesh timeline item (P2)
- key: `ui:mesh:insert-timeline-item`
- Fusion: The insert adds a timeline item with a yellow mesh-feature icon whose hover reads "Base Mesh Feature1" (Unsaved).
- FissionOxide: The timeline item is "Import1" with the grey import-arrow icon.
- Start here: `crates/ui/src/panels.rs:3548` (feature_icon: a mesh icon for mesh inserts) and the import feature's name in `crates/engine/src/files.rs:112`.

## Mesh body right-click menus (P1)
- key: `ui:mesh:body-context-menu`
- Fusion: The mesh body's browser row menu: Move/Copy (M), Move to Group, Create Selection Set | Edit Feature, Configure | Physical Material, Appearance (A), Texture Map Controls, Properties | Save As Mesh | Copy (Ctrl+C), Cut (Ctrl+X), Delete (Del), Remove, Rename | Show/Hide (V) | Selectable/Unselectable | Opacity Control > | Find in Window | Flat Shaded, Direct Edit. Right-click on the mesh in the canvas shows the ring (Repeat, Delete, Undo, Move/Copy, Press Pull, Redo, Hole, Sketch) and a list with the same entries plus Find in Browser.
- FissionOxide: The row shows the solid body menu (Move/Copy, Create Components from Bodies, Physical Material greyed, Appearance, Properties, Save As Mesh, Delete, Remove, Rename, Show/Hide, Isolate, Find in Window, Selectable/Unselectable, Opacity Control); in the canvas the face menu opens (Create Selection Set greyed, Create Sketch, Extrude, Offset Plane, Shell, Edit Feature (Import1), Edit Profile Sketch greyed, Appearance, Texture Map Controls greyed, Properties, Show/Hide, Selectable/Unselectable, Find in Browser, Find in Window).
- Start here: `crates/ui/src/panels.rs:2007` (body_menu: a mesh-body variant with Edit Feature, Flat Shaded and Direct Edit, no Create Components from Bodies), the canvas body list in `crates/ui/src/marking.rs`.

## Open-mesh warning on the browser row (P2)
- key: `ui:mesh:open-mesh-warning`
- Fusion: After a Plane Cut with Fill Type "No Fill" the mesh row gets a yellow warning triangle; its hover reads "Mesh is not closed." and "Mesh does not have positive volume." on two lines. The timeline item is "MeshPlaneCut1".
- FissionOxide: There are no mesh bodies, so no open-mesh state or warning (Plane Cut runs Split Body on the faceted solid).
- Start here: after `ui:mesh:mesh-body-kind`, a closed/positive-volume check on mesh bodies and the warning icon next to the row in `crates/ui/src/panels.rs:1960` (the timeline's failed-item warning drawing is the model).
