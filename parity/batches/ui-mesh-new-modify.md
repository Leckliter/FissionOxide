# Parity batch: ui-mesh-new-modify

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 14) in an unsaved Part Design document in each app (Fusion in
inches, FissionOxide in mm), FissionOxide built from `main` at `25d185b8`, on the MESH tab with the
inserted `lprism.stl` mesh selected before each command. Each of these MESH > MODIFY entries only
prints "<name> isn't in FissionOxide yet" at the lower left in FissionOxide (`x(...)` entries in
`crates/ui/src/menus.rs:492`). They need a mesh body kind to act on (`ui:mesh:mesh-body-kind`, batch
`ui-mesh-body-and-insert`); until that lands they can run on the faceted bodies FissionOxide makes
from mesh files. Every Fusion mesh dialog has an "i" info icon at the lower left. See
`parity/reports/2026-10-22-mesh-tab.md`.

## Remesh (P1)
- key: `ui:mesh:remesh-dialog`
- Fusion: "REMESH": Body/Face Groups chip ("1 selected" with an x), Type dropdown with icon (Adaptive, Uniform), Method (Density, Edge Length), Density "0.25" with a slider, Shape Preservation "0.50" with a slider, Preserve Sharp Edges, Preserve Boundaries and Preview check boxes (all unticked), OK lit. Ribbon tooltip: "Refines or coarsens faces on a mesh body or face groups." / "Select a mesh body or face groups on a mesh body, then adjust the settings to regenerate the mesh faces."
- FissionOxide: "Remesh isn't in FissionOxide yet".
- Start here: `crates/ui/src/menus.rs:494` (wire Remesh to a new form in `crates/ui/src/forms.rs` next to MESH_REDUCE), a `MeshOp::Remesh` in `crates/engine/src/features.rs:261`.

## Mesh Shell (P2)
- key: `ui:mesh:shell-dialog`
- Fusion: "SHELL" (mesh): Body ("1 selected"), Thickness "0.079 in" with a slider, Preview (unticked); prompt "Adjust direction and thickness".
- FissionOxide: "Shell isn't in FissionOxide yet" (MESH > MODIFY > Shell is a stub; the SOLID Shell is a different dialog).
- Start here: `crates/ui/src/menus.rs:497`, a mesh offset-and-join op beside `crates/engine/src/features.rs:261`.

## Mesh Combine (P2)
- key: `ui:mesh:combine-dialog`
- Fusion: "COMBINE" (mesh): Target Body ("1 selected"), Tool Bodies (Select), Operation as four icon toggles (Join first, lit), Keep Tools (unticked), Algorithm dropdown "Standard", Preview; OK grey until a tool body is picked; prompt "Select target body".
- FissionOxide: "Combine isn't in FissionOxide yet" on the MESH tab.
- Start here: `crates/ui/src/menus.rs:498`, mesh boolean in `crates/engine/src/features.rs`.

## Separate (P2)
- key: `ui:mesh:separate-dialog`
- Fusion: "SEPARATE": Body ("1 selected"), Type dropdown "Mesh Shells", Keep Body (unticked), OK lit; prompt "Select mesh body to separate".
- FissionOxide: "Separate isn't in FissionOxide yet".
- Start here: `crates/ui/src/menus.rs:502` (a form splitting a mesh body into its connected shells).

## Scale Mesh (P2)
- key: `ui:mesh:scale-mesh-dialog`
- Fusion: "SCALE MESH": Entities ("1 selected"), Point ("1 selected", filled in automatically), Scale Type with icon "Uniform", Scale Factor "1.00", a scale arrow on the mesh; prompt "Select bodies or sketches to scale".
- FissionOxide: "Scale Mesh isn't in FissionOxide yet" (the SOLID Scale dialog exists and could be reused).
- Start here: `crates/ui/src/menus.rs:505` (wire to the Scale dialog's kind with the SCALE MESH title and rows).
