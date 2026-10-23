# Parity batch: ui-reverify-mesh-2

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 6) re-checking the MESH keys with an inserted
`lprism.stl` and a two-cube STL on `main` at `dadbcad3`, with Fusion's own `lprism` mesh (Untitled*(1))
beside it for the menu and Convert Mesh. Three rows are reopened, two are new.
Every row is already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this
batch); read each row's `history` before starting.

## Convert Mesh Method icons (P2)
- key: `ui:mesh:convert-mesh-dialog`
- Fusion: CONVERT MESH's Method row has two icon toggles, Faceted (lit) and Prismatic, and Prismatic carries a small blue subscription badge.
- FissionOxide: three icons (Faceted, Prismatic, a greyed Organic) and no badge. Rows, tooltip, prompt and the result already match.
- Start here: `crates/ui/src/forms.rs:2155`, `crates/ui/src/icons/mesh.rs:126`

## Scale Mesh arrow (P2)
- key: `ui:mesh:scale-mesh-dialog`
- Fusion: SCALE MESH draws a scale arrow on the mesh (drag it to scale).
- FissionOxide: rows match; no arrow on the mesh.
- Start here: `crates/ui/src/dialogs/mesh_modify.rs:55`, `crates/ui/src/viewport.rs` (the Scale manipulators)

## Generate Face Groups pick outline (P2)
- key: `ui:mesh:face-groups-dialogs`
- Fusion: with GENERATE FACE GROUPS open the picked mesh is outlined in blue, not filled.
- FissionOxide: the picked mesh is filled selection blue. Rows and the "GenerateFaceGroups1" item match.
- Start here: `crates/ui/src/mesh_ui.rs:69`, `crates/ui/src/dialogs/mesh.rs:482`

## mesh body menu greyed entries (P2, new)
- key: `ui:mesh:body-menu-greyed-entries`
- Fusion: the mesh body's browser row menu has Move to Group, Create Selection Set, Physical Material, Texture Map Controls and Cut enabled; the canvas ring's top slot is "Repeat <last command>".
- FissionOxide: the row and canvas menus list the right entries but grey Move to Group, Create Selection Set, Physical Material, Texture Map Controls, Cut and Direct Edit; right after Insert Mesh the ring's Repeat slot is empty.
- Start here: `crates/ui/src/marking.rs:1715`

## mesh shading across sharp edges (P2, new)
- key: `ui:mesh:sharp-edge-shading`
- Fusion: a mesh body is drawn with its facet normals: an STL box or prism has flat faces and crisp 90 deg edges.
- FissionOxide: mesh bodies are smooth-shaded across sharp edges, so the prism and cubes look like soft pillows with rounded, gradient-lit corners. Split normals at edges sharper than a threshold (or use facet normals) for mesh bodies.
- Start here: `crates/render/src/lib.rs`, `crates/model/src/regen/mesh.rs`
