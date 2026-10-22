# Parity batch: ui-reverify-dialogs-3

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 2) re-checking the dialogs area's
`fixing-merged` keys in the rebuilt app (`main` at `b399613d`). All three are reopened keys whose
dialogs now look like Fusion's but whose options are greyed or missing because the engine does
not do them yet. The rows are already in `parity/ui-features.jsonl` (do not run
`ledger_from_batches.py` for this batch); read each row's `history` before starting.

## Split Face types (P2)
- key: `ui:dialogs:split-face-rows`
- Fusion: Split Type offers Split with Surface, Along Vector and Closest Point, and Extend Splitting Tool can be turned off.
- FissionOxide: both rows are greyed; only a split by an extended plane works.
- Start here: `crates/ui/src/forms.rs:1632` (`SPLIT_FACE`) and the engine's `split_face` command in `crates/engine/src/commands.rs`.

## Remove Features recognises every type (P2)
- key: `ui:dialogs:remove-features-dialog`
- Fusion: Select All, Fillet, Hole, Chamfer, Extrude, Revolve and Other are all live.
- FissionOxide: Chamfer, Extrude, Revolve and Other are greyed; only fillets and holes are recognised.
- Start here: `crates/ui/src/forms.rs:4197` (`REMOVE_FEATURES`) and the engine's `remove_features`.

## Replace with Primitives rows (P2)
- key: `ui:dialogs:replace-with-primitives`
- Fusion (seen this session): REPLACE WITH PRIMITIVES has Object type ("Bodies" dropdown with icon), Selection (Select chip, prompt "Select Bodies"), Primitive Shape ("Box" dropdown) and a collapsible "Primitive Size" group; OK greyed; no Preview checkbox.
- FissionOxide: Bodies (chip, prompt "Select solid bodies"), Primitive Type (Box) and a Preview checkbox; no Object type row and no Primitive Size group.
- Start here: `crates/ui/src/forms.rs:4224` (`REPLACE WITH PRIMITIVES`).
