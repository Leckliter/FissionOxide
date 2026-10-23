# Parity batch: ui-reverify-prefs-1

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-23 by the open-row pass (verifier 8) on `main` at `6c5c8b6b`: both rows were open
with no batch and are now confirmed in the app. Every row is already in `parity/ui-features.jsonl`
(do not run `ledger_from_batches.py` for this batch); read each row's `history` before starting.
Fusion's preference pages are written out row by row in
`parity/reports/2026-10-23-reverify-open-rows.md` (appendix "Fusion Preferences pages").

## ui-reverify-prefs-1 (P2)

- key: `ui:chrome:unit-display-hide-trailing-zeroes`
- Fusion: in a mm design at the default preferences (General precision 0.123, Hide trailing zeroes on, Minimum precision when hiding trailing zeroes 4.00) lengths show at most three decimals with trailing zeroes dropped down to two: a 12.3456 mm sketch dimension reads "12.346", 12.3 reads "12.30", 10 reads "10.00".
- FissionOxide: the same dimension reads "12.35" (10 reads "10.00"). Preferences already shows General precision 0.123 and a ticked Hide trailing zeroes (greyed), but lengths use the unit's two decimals. Make the default display follow precision 3 trimmed to the minimum precision 2, and let Hide trailing zeroes and the minimum precision row work.
- Start here: `crates/ui/src/units.rs:99` (`length_decimals`, `unit_decimals`), `crates/ui/src/preferences/pages.rs` (Unit and Value Display rows)

- key: `ui:chrome:preferences-unrecorded-pages`
- Fusion: the pages listed in the report's appendix, each with its header line, rows, controls and defaults: Manufacture (main page, eleven bold groups ending with CAM Data), Manufacture > Optional Features (info box and 19 check boxes), Electronics > Color, Misc, Default Grid and Units, Default Values, Drill Legend, Directory, 3D PCB, Drawing (every row), Network, Data Collection and Use, Preview Features.
- FissionOxide: Optional Features and the seven Electronics pages show only their title; Manufacture shows group names with no rows, ticks Enable Cloud Libraries (Fusion: unticked), has no "..." buttons beside the editor boxes and no CAM Data group; Drawing greys every row (in Fusion only the rows under "Override or Restore Format Defaults Below" are greyed), draws Edge as a check box (Fusion: dropdown), leaves Include Mesh / Surface Bodies unticked (Fusion: ticked) and stops at Large Text Height. Add the recorded rows (they can be inert where FissionOxide has no feature behind them).
- Start here: `crates/ui/src/preferences/pages.rs:202` (Manufacture rows), `crates/ui/src/preferences.rs:786` (page list)
