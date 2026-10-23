# Parity batch: ui-reverify-render-1

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 5) re-checking the render keys in the rebuilt app (`main` at `c6232d9f`), with Fusion's in-canvas render on its empty Assembly tab beside it. Two rows are new, one reopened.
Every row is already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this
batch); read each row's `history` before starting.

## in-canvas render ground (P1)
- key: `ui:render:in-canvas-ground-slab`
- Fusion: In-canvas Render on the default scene (Background Solid Color) draws the model on the flat grey background; an empty design stays plain grey. No dark ground plane is drawn.
- FissionOxide: In-canvas Render draws a large dark grey to black ground slab: on an empty design it fills most of the canvas with a black quad; with a box the box sits on a dark slab with a black triangle behind it; the navigation bar icons nearly vanish while it renders.
- Start here: `crates/ui/src/render_ui.rs`, `crates/render/src/lib.rs:879`

## ground plane look in Design (P2)
- key: `ui:render:design-ground-look`
- Fusion: With the default effects a body in the DESIGN workspace sits on the white canvas with no visible ground plane and at most a faint soft shadow (Untitled*(3)'s sheet metal part, seen 2026-10-22).
- FissionOxide: As soon as a design has a body, the DESIGN (and sketch, Manufacture, Animation) canvas draws a large light-grey ground quad reaching the canvas edges and a hard dark-grey drop shadow; a click on that quad does not clear the selection. default_effects() turns Ground Plane and Ground Shadow on.
- Start here: `crates/ui/src/navbar.rs:316`, `crates/render/src/lib.rs:840`

## Scene Settings is a docked dialog with Settings and Environment Library tabs (P1)
- key: `ui:render:scene-settings-dialog`
- Fusion: a docked right-hand dialog "SCENE SETTINGS" with tabs Settings | Environment Library and a Close button. Settings: Environment (Brightness "1000.000 lx" with a slider, Position (a move icon), Background "Solid Color" dropdown, Color swatch), Ground (Ground Plane checked, Flatten Ground, Reflections), Camera (Camera "Perspective" dropdown, Focal Length "90 mm" with a slider, Exposure "9.5 EV" with a slider, Depth of Field checkbox, Aspect Ratio "Viewport aspect ratio"), then Save as Default, Restore Saved, Restore Defaults. Environment Library: Current Environment (thumbnail "Sharp Highlights"), Library ("Fusion Library" dropdown) with thumbnail rows Cool Light, Grid Light, Rim Highlights, Sharp Highlights, Soft Light, Warm Light, Crossroads (download icon) ..., and "Attach Custom Environment" with a folder icon.
- FissionOxide: Tabs, rows, Environment Library and Close match; Focal Length reads 45 mm (Fusion 90 mm) and Position shows an extra '0 deg' field beside its move icon.
- Start here: `crates/ui/src/render_panels.rs:243`
