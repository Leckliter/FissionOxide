# Parity batch: ui-reverify-workspaces-3

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-23 by the open-row pass (verifier 8) on `main` at `6c5c8b6b`: four rows that were
open with no batch. The light and the Solid Holder submenu were re-checked in both apps and their
Fusion side is now recorded; Show Tool on Cursor and user coordinate systems were not re-checked.
Every row is already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this
batch); read each row's `history` before starting.

## ui-reverify-workspaces-3 (P2)

- key: `ui:render:light-editing`
- Fusion: RENDER > SETUP > Lights opens NEW LIGHT (Light Type Photometric Light, Target). Picking a target face adds a Position row ("0.394 in"), draws a yellow wire-sphere glyph over the face and lights the canvas live. OK adds a "Lights" folder (check box and eye) above Origin in the browser and opens EDIT LIGHT at once: Frame (Mode Global; Light Type greyed), Position (Modify pencil, X/Y/Z), Rotation (Modify, Flip Direction, X/Y/Z in deg), Light properties (Intensity "1500.000 cd" with a slider, Intensity unit Candela, Distribution Uniform, Shape Point, Color Mode Kelvin, Color Preset Custom, Temperature 3500.00, a Color row None), with a move triad and rings on the glyph.
- FissionOxide: NEW LIGHT has Light Type and Target only; the target adds no Position row, glyph or live light; OK places a light with no glyph, no Lights folder and no EDIT LIGHT, and leaves the target face selected.
- Start here: `crates/ui/src/render_panels.rs:381` (NEW LIGHT values), `crates/ui/src/render_ui.rs`

- key: `ui:manufacture:solid-holder-submenu`
- Fusion: MILLING > MANAGE > Solid Holder (no icon, an arrow) opens one entry, "Milling Tool Holder" with a holder icon; its tooltip reads "Creates a milling tool holder from a solid body." with a paragraph on the revolved cross-section going into the Tool Library and a picture.
- FissionOxide: the submenu holds a guessed "Create Solid Holder" with a Cylinder icon.
- Start here: `crates/ui/src/menus.rs:1171`

- key: `ui:manufacture:show-tool-on-cursor`
- Fusion: the Manufacture navigation bar's Tool Visibility > Show Tool on Cursor (Ctrl+T, needs Show Tool) snaps the selected operation's tool to the pointer over the model.
- FissionOxide: the row is there but greyed; no tool follows the pointer and Ctrl+T does nothing.
- Start here: `crates/ui/src/navbar.rs:663`, `crates/ui/src/viewport.rs`

- key: `ui:navbar:user-coordinate-systems` (P3)
- Fusion: user coordinate systems exist and Object Visibility > User Coordinate Systems shows or hides them.
- FissionOxide: there are no user coordinate systems, so the Object Visibility row stays greyed.
- Start here: `crates/ui/src/navbar.rs`
