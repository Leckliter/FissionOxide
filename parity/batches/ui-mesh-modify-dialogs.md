# Parity batch: ui-mesh-modify-dialogs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 14) in an unsaved Part Design document in each app (Fusion in
inches, FissionOxide in mm), FissionOxide built from `main` at `25d185b8`, on the MESH tab with the
inserted `lprism.stl` selected before each command (Fusion: mesh body "lprism"; FissionOxide: the
faceted "Body1", see `ui:mesh:mesh-body-kind` in batch `ui-mesh-body-and-insert`). These are the mesh
commands FissionOxide already has a dialog for. Every Fusion mesh dialog has an "i" info icon at the
lower left and Preview unticked by default; FissionOxide's forms tick Preview. See
`parity/reports/2026-10-22-mesh-tab.md`.

## Reduce dialog (P1)
- key: `ui:mesh:reduce-dialog`
- Fusion: "REDUCE": Body/Face Groups chip ("1 selected" with an x), Type dropdown (Tolerance, Proportion ticked, Face Count), Remesh Type dropdown with icon (Adaptive; Uniform), Proportion "25.00" with a slider under it, Preview (unticked); cursor prompt "Adjust reduce type and settings".
- FissionOxide: "REDUCE": Bodies chip ("Body1"), "Keep (fraction)" 0.5, Preview ticked. The preview of the 12-triangle prism at 0.5 collapses it into a flat L-shaped sheet.
- Start here: `crates/ui/src/forms.rs:1303` (MESH_REDUCE: Type, Remesh Type and a Proportion percentage with a slider), the reduction in `crates/engine/src/features.rs:264` (never collapse a closed body: a mesh already at its minimum should stay unchanged).

## Smooth dialog (P2)
- key: `ui:mesh:smooth-dialog`
- Fusion: "SMOOTH": Body/Face Groups chip, Smoothness "1.00" with a slider (at its right end), Preview (unticked); cursor prompt "Adjust smoothness".
- FissionOxide: "SMOOTH": Bodies chip, "Passes" 3, Preview ticked; the preview shrinks the prism to a small rounded blob about a third its size.
- Start here: `crates/ui/src/forms.rs:1317` (MESH_SMOOTH: one Smoothness value 0..1 with a slider), `crates/engine/src/features.rs:265` (a volume-preserving smooth rather than plain Laplacian passes).

## Plane Cut dialog (P1)
- key: `ui:mesh:plane-cut-dialog`
- Fusion: "PLANE CUT": Body ("1 selected"), Cut Plane (Select), Flip (icon), Type (Trim ticked, Split Body, Split Faces), Fill Type with icon (No Fill, Uniform, Minimal), a Numerical Inputs group; OK is grey and the prompt reads "Select mesh body to cut". After a plane pick (XZ) the group shows X Distance "0.00 in", Y Angle and Z Angle "0.0 deg", the plane gets an arrow and two rotation handles, the part to be removed is tinted red, and OK lights. OK adds "MeshPlaneCut1" to the timeline.
- FissionOxide: Plane Cut opens the solid "SPLIT BODY" dialog: Bodies to split ("1 selected"), Splitting tool dropdown already set to "XY plane", OK lit at once; no Flip, Type, Fill Type, numerical inputs, handles or red preview.
- Start here: `crates/ui/src/menus.rs:496` (Plane Cut is wired to `Kind::SplitBody`; give it its own PLANE CUT dialog in `crates/ui/src/dialogs.rs` next to the SplitBody one at `crates/ui/src/dialogs.rs:155`).

## Convert Mesh dialog and result (P1)
- key: `ui:mesh:convert-mesh-dialog`
- Fusion: "CONVERT MESH": Body ("1 selected"), Operation dropdown (Parametric ticked, Base Feature), Method icon toggles (Faceted, whose tooltip reads "Faceted" / "Converts individual faces on the mesh to individual faces on the new solid", and Prismatic with a blue subscription badge); no Preview row; prompt "Adjust conversion settings". OK adds "Convert Mesh1" (with a space) to the timeline, hides the mesh body (its eye turns off) and adds a new solid body row.
- FissionOxide: "CONVERT MESH": Bodies chip, Preview ticked; prompt "Hold Ctrl to modify selection". OK converts Body1 in place (same row, same icon) and the timeline item is "ConvertMesh1".
- Start here: `crates/ui/src/forms.rs:1331` (MESH_CONVERT rows), the feature name and keep-the-mesh-hidden result in `crates/engine/src/features.rs:266`.

## MESH ribbon quick buttons and icons (P2)
- key: `ui:mesh:ribbon-quick-buttons`
- Fusion: The MESH tab's ribbon shows CREATE: Insert Mesh, Tessellate, Create Mesh Section Sketch; PREPARE: Repair, Generate Face Groups, Combine Face Groups; MODIFY: only Remesh and Reduce; then CONFIGURE, CONSTRUCT, INSPECT, INSERT, ASSEMBLE, SELECT, EXPORT (3D Print). Each icon is distinct (Insert Mesh a blue arrow into a yellow mesh box, Tessellate a grey cylinder turning yellow, Remesh/Reduce split cubes with a dense and a sparse mesh).
- FissionOxide: MODIFY shows six quick buttons (Remesh, Reduce, Plane Cut, Shell, Combine, Move/Copy), and most mesh icons are the same plain yellow box in different poses.
- Start here: `crates/ui/src/menus.rs:1128` (MESH group `quick` lists: MODIFY should be Remesh and Reduce), the mesh icons in `crates/ui/src/icons.rs`.
