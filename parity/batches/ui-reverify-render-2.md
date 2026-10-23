# Parity batch: ui-reverify-render-2

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 6) in the RENDER workspace on a box (`main` at
`d2202676`). Two rows are reopened, one is new (P1). Fix `ui:render:appearance-not-drawn` first: the other
two can only be seen once an appearance and the environment are drawn.
Every row is already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this
batch); read each row's `history` before starting.

## an applied appearance is not drawn (P1, new)
- key: `ui:render:appearance-not-drawn`
- Fusion: applying an appearance (Wood - Oak from the Appearance palette) to a body or a face redraws it at once in that appearance (brown oak grain), in DESIGN and RENDER.
- FissionOxide: the palette applies Wood - Oak to the body ("In this design: Body3 - Wood - Oak") but the box stays the default warm grey; applied to a selected face the status says "Applied Wood - Oak" and nothing changes (the palette keeps "Steel - Satin").
- Start here: `crates/model/src/config.rs:320` (`body_color`), `crates/ui/src/render_tools.rs`

## Texture Map Controls handles (P2)
- key: `ui:render:texture-map-controls`
- Fusion: SETUP > Texture Map Controls adjusts a texture's projection (planar, box, spherical, cylindrical, automatic) and moves, turns and scales it in the canvas with on-model handles.
- FissionOxide: on the wood body the dialog has Object, Projection (the five), Scale "40.0 mm", Rotation, Offset U / V and Reset, but there are no canvas handles and the texture is not drawn; on a face with the wood it reads "No texture".
- Start here: `crates/ui/src/render_tools.rs:114`

## environment background and Flatten Ground (P2)
- key: `ui:render:flatten-ground`
- Fusion: Scene Settings > Background "Environment" draws the chosen environment (e.g. Warm Light) behind the model, and Flatten Ground flattens its lower half onto the ground plane.
- FissionOxide: with Background Environment and Warm Light the canvas keeps its plain grey backdrop and In-canvas Render a dark one, so ticking Flatten Ground changes nothing visible.
- Start here: `crates/render/src/lib.rs:933` (the dome), `crates/engine/src/rendering.rs:445`
