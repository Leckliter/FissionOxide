# Parity batch: ui-dialog-selection-state

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-22 (prober 9): SOLID > MODIFY dialogs in each app, built from `main` at
`ffa348e2`. See `parity/reports/2026-10-22-solid-modify.md`.

## Cancel clears the dialog's picks (P1)
- key: `ui:dialogs:cancel-clears-selection`
- Fusion: After picking faces or bodies in Combine, Offset Face, Replace Face, Split Face or Split Body, Cancel closes the dialog with nothing selected (no blue, empty status bar).
- FissionOxide: After Cancel the picks stay selected: Combine's two bodies stay blue with their browser rows highlighted and the status bar reads "2 Bodies"; Offset Face's face stays blue with "1 Face | Area ...".
- Start here: `crates/ui/src/dialogs.rs:1829` (`cancel`: clear the selection the dialog's picks made, as OK does).

## body picks highlight the body (P1)
- key: `ui:dialogs:body-pick-highlight`
- Fusion: With a body chip active (Combine target and tools, Split Body, Scale, Remove), hovering and picking a body turns the whole body blue and the status bar names the body ("Body1").
- FissionOxide: Only the clicked face turns blue and the status bar reads "1 Face | Area : 389.52 mm^2"; Split Body and Scale do not accept the pick at all.
- Start here: `crates/ui/src/select.rs` (body-only filter while a body chip is active) and `crates/ui/src/panels.rs:2954` (status text for a body pick).

## preview leaves the browser alone (P2)
- key: `ui:dialogs:preview-browser-unchanged`
- Fusion: While a Combine (Join) preview shows, the browser keeps every body row; rows change only after OK.
- FissionOxide: As soon as the tool body is picked the Body2 row disappears from the browser (the preview edits the model); it comes back on Cancel.
- Start here: `crates/ui/src/dialogs.rs:2058` (preview for Combine / Mirror / Circular Pattern) and the browser's body list in `crates/ui/src/panels.rs`: hide preview-consumed bodies in the canvas only.
