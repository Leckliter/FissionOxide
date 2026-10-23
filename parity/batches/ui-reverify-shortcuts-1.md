# Parity batch: ui-reverify-shortcuts-1

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-23 by the incremental re-verification pass (verifier 7) on `main` at `905a4701`, with
Fusion's OFFSET seen side by side. Both rows are reopened from `ui-reverify-toolbar-1`. Every row is
already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this batch); read each
row's `history` before starting.

## ui-reverify-shortcuts-1 (P1)

- key: `ui:shortcuts:sketch-keys-from-3d`
- Fusion: in 3D with nothing selected, C, L, R, T and P ask 'Select a plane or planar face'; picking a plane creates the sketch with that tool already running (L: draw a line at once).
- FissionOxide: the prompt and the plane hover match, but after L and a click on the XY plane the sketch opens with no tool: the palette has no Line Feature Options and canvas clicks draw nothing until Line is started again. `sketch_first` stores the command in `sketch_then` while the plane pick runs; check that it runs once the plane is picked (the plane pick's click path, not only a pre-selected plane).
- Start here: `crates/ui/src/lib.rs:1693` (`sketch_first`, `sketch_then`)

- key: `ui:shortcuts:offset-from-3d-dialog`
- Fusion: O in 3D opens OFFSET with Curves, Chain Selection, Direction and Flip only (the distance appears after a curve is picked) and the prompt 'Select a plane or planar face'.
- FissionOxide: the waiting OFFSET has a fifth row, 'Distance 0 mm', between Direction and Flip.
- Start here: `crates/ui/src/dialogs.rs:3995` (SkOffset rows), `crates/ui/src/dialogs.rs:2769` (`Dialog::frame`)
