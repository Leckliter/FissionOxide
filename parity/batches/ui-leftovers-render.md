# Parity batch: ui-leftovers-render

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

These rows were recorded by probers and fixers between 2026-10-21 and 2026-10-22 but never got a
batch (most were found by a fixer while working on a neighbouring item). The coordinator grouped
them on 2026-10-22; the wording below is the ledger row's own. Re-read the row in
`parity/ui-features.jsonl` (its `history`) before starting, and where a row says Fusion's side was
not observed, check it in Fusion's documentation and say so in your ledger note.

## Texture Map Controls command (P2)
- key: `ui:render:texture-map-controls`
- Fusion: SETUP > Texture Map Controls adjusts a texture's projection (planar, box, spherical, cylindrical, automatic) and moves, turns and scales it in the canvas.
- FissionOxide: listed in RENDER > SETUP with no command (the status line says it isn't in FissionOxide yet); the Appearance window's material editor has projection, scale and rotation fields instead.
- Start here: `crates/ui/src/menus.rs`, `crates/ui/src/render_panels.rs`.

## Dielectric Priority Control command (P2)
- key: `ui:render:dielectric-priority`
- Fusion: OPTICAL OVERRIDES > Dielectric Priority Control gives transparent bodies priorities 0, 1, 2 for where they overlap.
- FissionOxide: listed with no command; the path tracer has no dielectric priorities.
- Start here: `crates/ui/src/menus.rs`, `crates/ui/src/render_panels.rs`.

## In-canvas Render Settings dialog (P2)
- key: `ui:render:in-canvas-settings`
- Fusion: IN-CANVAS RENDER > In-canvas Render Settings opens the in-canvas render's quality settings.
- FissionOxide: listed with no command.
- Start here: `crates/ui/src/menus.rs`, `crates/ui/src/render_panels.rs`.

## New Light places a light (P2)
- key: `ui:render:lights-in-scene`
- Fusion: NEW LIGHT with a target picked enables OK and adds a photometric light to the scene (up to eight per design).
- FissionOxide: the NEW LIGHT dialog shows but OK stays greyed: neither the path tracer nor the GPU look takes lights.
- Start here: `crates/ui/src/menus.rs`, `crates/ui/src/render_panels.rs`.

## Scene Settings Flatten Ground (P2)
- key: `ui:render:flatten-ground`
- Fusion: Scene Settings > Ground > Flatten Ground flattens the environment's lower half onto the ground plane.
- FissionOxide: the checkbox is greyed: the environments don't flatten.
- Start here: `crates/ui/src/menus.rs`, `crates/ui/src/render_panels.rs`.

## Scene Settings default Background (P2)
- key: `ui:render:default-background`
- Fusion: a new design's Scene Settings show Background "Solid Color" (a grey), so In-canvas Render draws the model on grey.
- FissionOxide: a new design's environment is Background Environment (fox-raytrace Environment::default); changing the default moves the path tracer's reference images and the look of saved designs, so it was left for its own change.
- Start here: `crates/ui/src/menus.rs`, `crates/ui/src/render_panels.rs`.

## Environment Library downloads (P2)
- key: `ui:render:downloadable-environments`
- Fusion: the Fusion Library lists downloadable environments (Crossroads, Dry Lake Bed, Field, Infinity Pool, Plaza, Tranquility Blue ...) fetched on first use.
- FissionOxide: listed greyed with a download mark; only the six built-in environments work.
- Start here: `crates/ui/src/menus.rs`, `crates/ui/src/render_panels.rs`.
