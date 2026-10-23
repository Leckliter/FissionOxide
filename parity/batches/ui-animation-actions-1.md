# Parity batch: ui-animation-actions-1

Make FissionOxide match Autodesk Fusion for the item below. It says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to it, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Batched on 2026-10-23 by the open-row pass (verifier 8) on `main` at `6c5c8b6b`: the row was found
by the ui-animation-workspace fixer and had no batch; it was not re-checked in the app this pass,
so look at Fusion's ANIMATION workspace yourself (it needs a design with components; never save to
Fusion's cloud) before matching details. The row is already in `parity/ui-features.jsonl` (do not
run `ledger_from_batches.py`); read its `history` first. This is a large item: land it in steps
(storyboard actions and Play first, then Transform Components, then explode, then callouts / views /
publish) and keep the row `fox-missing` until all of it works.

## ui-animation-actions-1 (P1)

- key: `ui:animation:commands`
- Fusion: Restore Home, Auto Explode: One Level / All Levels, Manual Explode, Show/Hide and Appearance record actions in the storyboard; Transform Components moves components with a manipulator and records the move; Create Callout, View and Publish Video add callouts, camera actions and a video; Play plays the storyboard's actions.
- FissionOxide: the ANIMATION tab lists these commands, but storyboards hold no actions; the explode, callout, view and publish commands say they aren't in FissionOxide yet and Transform Components records no move. Storyboards are not saved with the design.
- Start here: `crates/ui/src/anim_ui.rs`, `crates/ui/src/menus.rs`
