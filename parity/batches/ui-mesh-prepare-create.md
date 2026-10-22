# Parity batch: ui-mesh-prepare-create

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 14) in an unsaved Part Design document in each app (Fusion in
inches, FissionOxide in mm), FissionOxide built from `main` at `25d185b8`, on the MESH tab with the
inserted `lprism.stl` mesh (and, for Tessellate, a solid disc) available. Each of these MESH > CREATE
and PREPARE entries, and MODIFY > Direct Edit, only prints "<name> isn't in FissionOxide yet" in
FissionOxide (`x(...)` entries in `crates/ui/src/menus.rs:423` and `:431`). The mesh items need a mesh
body kind (`ui:mesh:mesh-body-kind`, batch `ui-mesh-body-and-insert`). See
`parity/reports/2026-10-22-mesh-tab.md`.

## Tessellate (P1)
- key: `ui:mesh:tessellate-dialog`
- Fusion: "TESSELLATE": Body (Select; OK grey until a solid or surface body is picked), a Refinement Settings group: Refinement dropdown (High, Medium ticked, Low, Custom), Surface Deviation "0.00 in" with a slider, Normal Deviation "15.00" with a spinner and slider, Maximum Edge Length "0.00 in" with a slider (becomes "1.414 in" after picking a disc), Aspect Ratio "21.50" with a spinner and slider, Create Quads (unticked), and a collapsed Preview group with its own check box. Tooltip: "Converts an existing solid or surface body into a mesh body." / "Select the solid or surface body to tessellate, then adjust the mesh refinement settings to convert the body to a mesh body."
- FissionOxide: "Tessellate isn't in FissionOxide yet".
- Start here: `crates/ui/src/menus.rs:425` (a form in `crates/ui/src/forms.rs`; the kernel's `tessellate` used by STL export in `crates/engine/src/commands.rs:2099` makes the triangles).

## Repair (P2)
- key: `ui:mesh:repair-dialog`
- Fusion: "REPAIR": Body ("1 selected"), Repair Type dropdown (Close Holes, Stitch and Remove ticked, Wrap, One Touch Fix, Rebuild), Preview, "Quick Analysis" with a green check for a clean mesh, and a collapsed Detailed Analysis group.
- FissionOxide: "Repair isn't in FissionOxide yet".
- Start here: `crates/ui/src/menus.rs:432`.

## Generate and Combine Face Groups (P2)
- key: `ui:mesh:face-groups-dialogs`
- Fusion: "GENERATE FACE GROUPS": Bodies/Face Groups ("1 selected"; the mesh is outlined blue rather than filled), Type "Fast", Angle Threshold "25.0 deg", Minimum Face Group size "0.20" with a slider, a collapsed Preview group with a check box; OK adds "GenerateFaceGroups1" to the timeline. "COMBINE FACE GROUPS": one Face Groups row (Select), OK grey, prompt "Select two or more adjacent face groups".
- FissionOxide: Both print "isn't in FissionOxide yet".
- Start here: `crates/ui/src/menus.rs:433` and `:434`.

## Create Mesh Section Sketch (P2)
- key: `ui:mesh:section-sketch-dialog`
- Fusion: "CREATE MESH SECTION SKETCH": Body (Select), Section Plane (Select), a Numerical Inputs group, OK grey until both are picked. Tooltip: "Creates a sketch that contains a section line where a plane intersects a mesh body."
- FissionOxide: "Create Mesh Section Sketch isn't in FissionOxide yet".
- Start here: `crates/ui/src/menus.rs:427` (the SOLID Slice/section code in `crates/ui/src/viewport.rs` can produce the polyline).

## Direct Edit and Create Base Mesh Feature environment (P1)
- key: `ui:mesh:direct-edit-environment`
- Fusion: MODIFY > Direct Edit (with a mesh selected) and CREATE > Create Base Mesh Feature both enter a contextual "DIRECT MESH EDITING" tab (CREATE, PREPARE, MODIFY, CONSTRUCT, INSPECT, SELECT, EXPORT panels and a green FINISH DIRECT EDITING check) with every other body ghosted white, a base-feature item added to the timeline, and a docked "MESH SELECTION PALETTE": Selection Type (Paint), Selection Filter (Faces), Brush Size slider, Select Back Faces, a Modify Selection group (Expand, Grow/Shrink, Invert icon pairs), Selected Faces / Selected Face Groups / Selected Mesh Bodies counts, and a "Finish Direct Editing" button. Finishing leaves the edited mesh as a new body ("lprism_Edited").
- FissionOxide: "Direct Edit isn't in FissionOxide yet" and "Create Base Mesh Feature isn't in FissionOxide yet".
- Start here: `crates/ui/src/menus.rs:428` and `:493`; the Create Base Feature environment on the SOLID tab (`Create Base Feature` in CREATE) is the model for the contextual tab.
