# Parity batch: ui-workflow-sketch-1

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-23 by verifier 9 on `main` at `4e1c9667`, in a side-by-side run of the most-used
workflow (Create Sketch on the ground plane, a typed 40 x 30 mm rectangle, Finish Sketch, Extrude
10 mm, Fillet 2 mm on one edge, then Extrude1 edited to 15 mm from the timeline) in a fresh mm
design in both apps. Every row is already in `parity/ui-features.jsonl` (do not run
`ledger_from_batches.py` for this batch); read each row's `history` before starting.

`ui:sketch:rectangle-heads-up-layout` was verified before and is reopened because the earlier
Fusion text ("first box just left of the cursor") was incomplete; its note says what Fusion did
today.

## ui-workflow-sketch-1 (P2)

- key: `ui:sketch:axis-lines-in-sketch` (P2)
- Fusion: While a sketch is open its X axis is a red line and its Y axis a green line running across the whole sketch grid through the origin, with the Origin folder hidden (seen on the ground plane of an empty mm design and on a body's top face).
- FissionOxide: The sketch's axis lines are drawn grey like a major grid line; no red or green line.
- Start here: `crates/engine/src/scene.rs:906`

- key: `ui:sketch:entry-scale-grid-steps` (P2)
- Fusion: Create Sketch on the ground plane of an empty mm design (window 1215 px wide) shows about 3.5 px per mm: grid labels every 25 mm (-125 ... 75 along X, 25 ... 75 along Y), minor lines every 5 mm (about 17 px) and major lines every 25 mm.
- FissionOxide: The same steps show about 5 px per mm, minor lines every 10 mm (about 50 px), major lines and labels every 50 mm (only -50 and 50 visible), so a 40 x 30 mm rectangle fills much more of the view.
- Start here: `crates/engine/src/scene.rs:899`, `crates/ui/src/lib.rs:2073`

- key: `ui:sketch:heads-up-mm-decimals` (P2)
- Fusion: In a mm design the rectangle's live heads-up boxes show three decimals: "22.833 mm" and "34.575 mm" (the committed dimensions then read "30.00" / "40.00").
- FissionOxide: The live boxes show two decimals: "15.75 mm" and "23.90 mm".
- Start here: `crates/ui/src/viewport.rs`

- key: `ui:sketch:rectangle-heads-up-layout` (P2)
- Fusion: In a mm design, first corner clicked lower left and the pointer moved to the upper right: the focused first box is the height ("22.833 mm", text selected) centred on a vertical dimension line with extension lines along the LEFT side (the first corner's vertical edge), well away from the pointer; the width box ("34.575 mm") is centred under the bottom edge between extension lines; the prompt "Specify size of rectangle" follows the pointer.
- FissionOxide: Second reopen. The 2026-10-22 fix put the focused box beside the pointer, following the row's earlier Fusion text ("just left of the cursor"). Today Fusion put the focused height box on the left edge through the first corner, about 170 px from the pointer, with its own vertical dimension line. FissionOxide, same clicks: the focused "15.75 mm" box sits on the top edge just left of the pointer with no vertical dimension line; after Tab the height moves to a dimension on the right edge. Make both boxes sit on the dimensions along the edges through the first corner (height on that vertical edge, width on that horizontal edge), whichever box has focus.
- Start here: `crates/ui/src/viewport.rs`

- key: `ui:sketch:vertical-dimension-text` (P2)
- Fusion: A typed-size rectangle (first corner lower left, drawn to the upper right) gets its height dimension on the left side, on the first corner's vertical edge, with the value "30.00" rotated 90 degrees to read upward along the dimension line; the width "40.00" sits under the bottom edge.
- FissionOxide: The height dimension goes on the right side and its value "30.00" is written horizontally beside the dimension line; the width sits under the bottom edge as in Fusion.
- Start here: `crates/ui/src/viewport.rs`
