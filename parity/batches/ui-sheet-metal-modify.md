# Parity batch: ui-sheet-metal-modify

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## sheet metal modify (P1)
- key: `ui:tabs:sheet-metal-modify`
- Fusion: SHEET METAL > MODIFY: Join by Bend (pick two flanges, a bend joins them), Sheet Metal Fillet and Sheet Metal Chamfer (corner edges of a sheet metal body, kept consistent through unfold).
- FissionOxide: All three are inert (menus.rs:431, 434, 435); ledger entries toolbar:FusionJoinByBendCommand / FusionSheetMetalFilletCommand / FusionSheetMetalChamferCommand untested.
- Start here: `crates/ui/src/menus.rs`, `crates/ui/src/forms.rs`
