# Parity batch: ui-workflow-chrome-1

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-23 by verifier 9 on `main` at `4e1c9667`, in a side-by-side run of the most-used
workflow (Create Sketch on the ground plane, a typed 40 x 30 mm rectangle, Finish Sketch, Extrude
10 mm, Fillet 2 mm on one edge, then Extrude1 edited to 15 mm from the timeline) in a fresh mm
design in both apps. Every row is already in `parity/ui-features.jsonl` (do not run
`ledger_from_batches.py` for this batch); read each row's `history` before starting.

## ui-workflow-chrome-1 (P2)

- key: `ui:browser:folders-stay-collapsed` (P2)
- Fusion: Create Sketch adds a collapsed Sketches folder and selects nothing in the browser; the first Extrude adds a collapsed Bodies folder.
- FissionOxide: Create Sketch adds the Sketches folder expanded with the Sketch1 row highlighted (blue frame) while the sketch is open; the first Extrude adds the Bodies folder expanded with Body1 shown.
- Start here: `crates/ui/src/panels.rs:2243`, `crates/ui/src/panels.rs:2415`, `crates/ui/src/panels.rs:1513`

- key: `ui:chrome:assistant-side-tab` (P2)
- Fusion: Fusion draws no AUTODESK ASSISTANT tab at the right edge of the canvas (every design tab looked at today in the 1215 px window); the Assistant lives in the Help menu (Shift+H).
- FissionOxide: A vertical "AUTODESK ASSISTANT" tab with a << button sits at the canvas's right edge under the ViewCube in every design (added before the loop, de77c9dd).
- Start here: `crates/ui/src/panels.rs:3122`, `crates/ui/src/lib.rs:2276`, `crates/ui/src/palette.rs:74`

- key: `ui:chrome:status-in-command-picks` (P2)
- Fusion: In FILLET the status bar stays empty while edges are only hovered, and reads "Edge" once one is picked.
- FissionOxide: Hovering an edge in FILLET writes "1 Edge | Length : 40.00 mm", and the picked edge keeps that readout.
- Start here: `crates/ui/src/panels.rs`

- key: `ui:data:recent-dates-local-time` (P2)
- Fusion: The Data Panel's item cards show the date and time of the save.
- FissionOxide: The cards show the save time in UTC: Untitled saved 2026-10-22 12:39 local (file time, -05:00) reads "10/22/2026 5:39 PM", External1 saved 2026-10-22 21:45 local reads "10/23/2026 2:45 AM". Fusion's own cards were not compared on screen (no item with a known save time); the readout is off by the machine's UTC offset whatever Fusion shows.
- Start here: `crates/ui/src/data_panel.rs:1142`
