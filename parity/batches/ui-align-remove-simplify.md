# Parity batch: ui-align-remove-simplify

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-22 (prober 9): SOLID > MODIFY in each app; Fusion in inches, FissionOxide in mm on
two 20 mm boxes, built from `main` at `ffa348e2`. See `parity/reports/2026-10-22-solid-modify.md`.

## Align rows (P2)
- key: `ui:dialogs:align-rows`
- Fusion: ALIGN: Object ("Bodies..." dropdown), From (chip, active, prompt "Select geometry or snap point on alignable object"), To (chip); OK greyed.
- FissionOxide: Objects to move (chip), Align to (chip), Relation ("Automatic" dropdown), Offset "0 mm", Flip, Preview; prompt "Objects to move".
- Start here: `crates/ui/src/forms.rs:2757` (`ALIGN`: Object dropdown, From and To chips, Fusion's prompt).

## Remove opens a dialog (P2)
- key: `ui:dialogs:remove-dialog`
- Fusion: Remove with nothing selected opens REMOVE: Objects (chip, active, prompt "Select bodies or components to remove"), OK greyed.
- FissionOxide: No dialog; the lower left shows "Remove: select bodies first".
- Start here: `crates/ui/src/panels.rs:773` (`Cmd::RemoveBodies`: open a one-row dialog when nothing is selected).

## Remove Features dialog (P2)
- key: `ui:dialogs:remove-features-dialog`
- Fusion: Simplify > Remove Features opens REMOVE FEATURES: Bodies ("no selection" chip, prompt "Select solid bodies"), a Features group with Select All, Fillet, Hole, Chamfer, Extrude, Revolve, Other (all checked), and only a Close button.
- FissionOxide: Remove Features opens the REMOVE FACES dialog (Faces chip, Preview, OK/Cancel).
- Start here: `crates/ui/src/menus.rs:120` (give Remove Features its own form) and `crates/ui/src/forms.rs:861`.

## Replace with Primitives missing (P2)
- key: `ui:dialogs:replace-with-primitives`
- Fusion: Simplify > Replace with Primitives is a working command.
- FissionOxide: Shows "Replace with Primitives isn't in FissionOxide yet".
- Start here: `crates/ui/src/menus.rs:121` (probe its rows in Fusion first, then build the dialog and command).
