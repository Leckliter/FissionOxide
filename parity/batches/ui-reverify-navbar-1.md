# Parity batch: ui-reverify-navbar-1

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 3) re-checking the navbar area's
`fixing-merged` keys in the rebuilt app (`main` at `afddd332`). Two are reopened keys and one is new
(`ui:navbar:popup-width`). The rows are already in `parity/ui-features.jsonl` (do not run
`ledger_from_batches.py` for this batch); read each row's `history` before starting.

## Ground Plane Offset takes a typed value (P2)
- key: `ui:navbar:ground-offset-applied`
- Fusion: with Adaptive off, the Offset field takes a typed distance and the ground plane (and its shadow) moves that far below the model.
- FissionOxide: the Offset row is an `egui::DragValue`; a click nudged it to 0.2 mm and typed digits ("20", "20 mm") were ignored, so the ground could not be moved a visible distance.
- Start here: `crates/ui/src/navbar.rs:556` (`egui::DragValue::new(&mut form.offset)`): use the same text value box as the other dialog distance rows (typed value with units, expressions).

## Shaded with Hidden Edges draws the hidden edges (P2)
- key: `ui:navbar:visual-style-list`
- Fusion: Visual Style > Shaded with Hidden Edges (Ctrl+5) shades the body and draws its hidden edges as faint lines through it.
- FissionOxide: the list and Ctrl+4..9 work, Ctrl+7 and Ctrl+6 change the look, but Ctrl+5 looks exactly like Shaded (no hidden edges on a plain box).
- Start here: the visual style handling in `crates/ui/src/navbar.rs` (the Visual Style list at `:713`) and the edge pass in `crates/engine/src/scene.rs`.

## Narrow Grid menu and Visual Style flyout (P2)
- key: `ui:navbar:popup-width`
- Fusion: the Grid and Snaps menu and the Visual Style flyout are as wide as their rows plus the shortcut column (about 200-260 px); the flyout opens to the right of "Visual Style".
- FissionOxide: both are about 450 px wide with an empty gap before the shortcuts; the Visual Style flyout opens leftwards over the canvas to x 20. The Environment, Graphics Preset, Effects, Object Visibility flyouts and the Viewports menu are narrow and open right.
- Start here: the menu builders in `crates/ui/src/navbar.rs` (the grid menu and the Visual Style flyout): size them from their content like the other flyouts.
