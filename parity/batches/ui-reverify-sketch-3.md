# Parity batch: ui-reverify-sketch-3

Make FissionOxide match Autodesk Fusion for the item below. It says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to it, add a test, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 3) re-checking the sketch area's
`fixing-merged` keys in the rebuilt app (`main` at `3e4cc76d`). A reopened key: the row is already
in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this batch); read its
`history` before starting.

## Measure from a dimension box fills the box (P3)
- key: `ui:sketch:dim-box-measure-fill`
- Fusion: picking Measure in a dimension edit box's menu opens Measure; the measured length is written into the box when the Measure command finishes.
- FissionOxide: Measure opens beside the box; picking a 10 mm sketch line selects it (status "1 Sketch Line | Length : 10.00 mm") but MEASURE lists no result rows, and after Close the box still holds its old text ("d1 * 2"). Tried twice, with the left and the right line of a dimensioned square.
- Start here: `crates/ui/src/viewport.rs:2987` (`measure_fill`) and `crates/ui/src/dialogs.rs:5364` (`fill_dim_from_measure`); the sketch-line pick may not reach Measure's result while a sketch is open.
