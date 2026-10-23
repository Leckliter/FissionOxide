# Parity batch: ui-reverify-workspaces-2

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-23 by the incremental re-verification pass (verifier 7) on `main` at `905a4701`, with
Fusion's Manufacture workspace and In-canvas Render run side by side. One row is reopened (from
`ui-reverify-workspaces-1`), one is new (P1). Every row is already in `parity/ui-features.jsonl` (do not
run `ledger_from_batches.py` for this batch); read each row's `history` before starting. The two items
touch separate code and can be done in either order.

## ui-reverify-workspaces-2 (P1)

- key: `ui:render:in-canvas-body-shading`
- Fusion: In-canvas Render on the default scene draws bodies with the default appearance as bright, reflective satin steel (light grey with highlights) and casts faint, soft shadows on the grey ground.
- FissionOxide: the grey background is right (no slab), but two default boxes render near black (dark charcoal faces, the tops only a little lighter) with hard, dark shadows running far across the ground.
- Start here: `crates/render-gl/src/pbr.rs:199` (diffuse term), `crates/render-gl/src/pbr.rs:78` (`sh_irr`), the default appearance's base colour and metallic values

- key: `ui:manufacture:navbar-cam-icons`
- Fusion: in Manufacture the navigation bar adds six icons after the usual ones, each with a fly-out arrow: Toolpath visibility, Synchronize active setup, Stock visibility, Machine visibility, Tool visibility, Turning profile visibility (seen on the TURNING tab). Tool visibility's fly-out is a narrow list (about 150 px) of check boxes: Show Tool, Show Tool Holder, Show Tool Shaft (all checked), Show Tool on Cursor (Ctrl+T, unchecked).
- FissionOxide: only Tool Visibility is added; its fly-out is about 450 px wide with Show Tool and a greyed Show Tool on Cursor, no check boxes, no Holder / Shaft rows.
- Start here: `crates/ui/src/navbar.rs:660` (`TOOL_VISIBILITY` and its fly-out), `crates/ui/src/icons.rs:89`
