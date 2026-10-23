# Parity batch: ui-workflow-dialogs-1

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-23 by verifier 9 on `main` at `4e1c9667`, in a side-by-side run of the most-used
workflow (Create Sketch on the ground plane, a typed 40 x 30 mm rectangle, Finish Sketch, Extrude
10 mm, Fillet 2 mm on one edge, then Extrude1 edited to 15 mm from the timeline) in a fresh mm
design in both apps. Every row is already in `parity/ui-features.jsonl` (do not run
`ledger_from_batches.py` for this batch); read each row's `history` before starting.

`ui:dialogs:extrude-auto-profile`, `ui:manip:value-box-covers-arrow` and (in
`ui-workflow-sketch-1`) `ui:sketch:rectangle-heads-up-layout` were verified before and are
reopened because the earlier Fusion text was incomplete: the fixes matched the old text, not
Fusion. Their notes say exactly what Fusion did today.

## ui-workflow-dialogs-1 (P1)

- key: `ui:dialogs:extrude-auto-profile` (P1)
- Fusion: With exactly one visible profile in the design (a just-finished sketch holding one closed rectangle), E opens EXTRUDE with that profile already picked: Profiles "1 selected" with an x, the blue arrow and taper handle on the profile, the floating box "0.00 mm" focused, the Distance and Taper Angle rows, the status "1 Profile | Area : 1200.00 mm^2", OK greyed until a distance. With two or more visible profiles (a sketch with two circles) or none (the consumed sketch hidden), Profiles reads "Select" with no Distance or Taper rows.
- FissionOxide: Second reopen. The 82b2cf5 fix made E never pre-pick (only a selected Sel::Profile counts), after the first probe saw a pre-pick with Sketch1 selected and recorded Fusion as never pre-picking; that Fusion text was incomplete. Seen today in Fusion on Untitled(4): after Finish Sketch on a sketch with one rectangle, E opened with "1 selected" and focus in the 0.00 mm box; after a second sketch with two circles, E opened with Select; with only the consumed sketch, Select. FissionOxide after Finish Sketch on its only sketch (one rectangle): E opens with Profiles Select, no arrow, no Distance/Taper rows. Fix: when the visible, unconsumed sketches offer exactly one profile, pick it on open (at 0.00, see ui:dialogs:extrude-start-distance); keep Select otherwise. Keep extrude_waits_for_a_picked_profile for the two-profile case.
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/dialogs/extrude.rs`

- key: `ui:dialogs:extrude-start-distance` (P1)
- Fusion: When Extrude gets a profile (picked, or the only one taken automatically) Distance starts at "0.00 mm": the floating box "0.00 mm" is focused, the preview is only the blue profile with the arrow, and OK is greyed until a distance is typed or dragged.
- FissionOxide: Picking a profile starts Distance at "10 mm" with a 10 mm preview body and OK enabled (a body face already starts at 0.00).
- Start here: `crates/ui/src/dialogs.rs:800`, `crates/ui/src/dialogs/extrude.rs:304`

- key: `ui:dialogs:fillet-before-pick` (P2)
- Fusion: FILLET opens with an empty selection table (only + and a greyed x under it) and no Tangency Weight row; the Edges/Faces/Features chip reads "Select" (lit). After the first edge pick the table gets the row "1 Edge | 0.00 mm | 0.00 mm (grey) | Tangent (G...", a Tangency Weight "1.00" row (editable) appears, and the chip still reads "Select". Zero values read "0.00 mm" in the row and the floating box; typing 2 makes the grey box read "2.00 mm".
- FissionOxide: Before any pick the table already holds a row "Select | 0 mm | set... | Tangent (G1)" and a greyed Tangency weight "1.00" row is always shown; after the pick the chip turns into "1 Edge" with an x, the radius reads "0 mm" and the grey setback box keeps "set..." instead of mirroring the radius.
- Start here: `crates/ui/src/dialogs/solid.rs:262`

- key: `ui:dialogs:title-case-labels` (P2)
- Fusion: Dialog row labels and choices use Title Case. EXTRUDE: Start "Profile Plane", Direction "One Side", "Extent Type" "Distance", "Taper Angle", Operation "New Body". FILLET: "Radius Type" "Constant", "Tangent Chain", "Tangency Weight", "Corner Type" "Rolling Ball".
- FissionOxide: Sentence case: "Profile plane", "One side", "Extent type", "Taper angle", "Radius type", "Tangent chain", "Tangency weight", "Corner type", "Rolling ball" (the code also has "Two sides", "To object" and Construct's "Extent type").
- Start here: `crates/ui/src/dialogs/extrude.rs:35`, `crates/ui/src/dialogs/extrude.rs:140`, `crates/ui/src/dialogs/extrude.rs:169`, `crates/ui/src/dialogs/solid.rs:262`, `crates/ui/src/dialogs/solid.rs:388`, `crates/ui/src/forms.rs:1803`

- key: `ui:manip:value-box-covers-arrow` (P2)
- Fusion: Fillet puts a blue radius arrow on the picked edge (with a black-and-white setback handle at the edge's end); the floating value box sits well below and to the right of the arrow, clear of the body (picked top front edge of a 40 x 30 x 10 box at the home view: arrow head at about (488, 372) in screenshot px, box at (596..690, 515), under the docked dialog), so the arrow stays visible and grabbable.
- FissionOxide: Second reopen. FissionOxide, same pick on the same box: the arrow points up-left from the edge and the box "0 mm" is placed LEFT of the arrow head on the body's top face (box (398..463, 364), head (490, 355)), the "next corner round" rule from the 2026-10-22 fix (handles::box_beside) picking the left side because the shaft runs below-right. Fusion keeps below-right and goes further out (about 110 px right and 140 px down of the head) so it clears the shaft; it never moves the box onto the body.
- Start here: `crates/ui/src/handles.rs`, `crates/ui/src/viewport.rs`
