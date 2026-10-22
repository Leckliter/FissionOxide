# Parity batch: ui-sheet-flat-pattern-env

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 15) in a new unsaved Part Design document in each app (millimeters),
FissionOxide built from `main` at `8b29a812`, on a base flange with one edge flange. Fusion's flat
pattern is an environment of its own, entered from Create Flat Pattern and left with Finish Flat
Pattern, and Unfold opens a temporary REFOLD panel. See `parity/reports/2026-10-22-sheet-metal-tab.md`.

## Create Flat Pattern pick and environment (P1)
- key: `ui:sheet:flat-pattern-environment`
- Fusion: Create Flat Pattern opens "CREATE FLAT PATTERN" with one row, Stationary Face [Select] (lit), OK greyed, prompt "Select Stationary face to create Flat Pattern"; after a face pick (blue) OK enters the flat pattern environment: the toolbar shows tabs FLAT PATTERN SOLID and FLAT PATTERN SURFACE with panels CREATE, MODIFY, CONSTRUCT, INSPECT, INSERT, SELECT, EXPORT and a green FINISH FLAT PATTERN (its dropdown holds "Finish Flat Pattern"); the browser root reads "Flat Pattern" and Bodies holds a "Bend Lines" folder (Center Lines, Extent Lines, each with an eye) and Body1; the view turns to Front; the timeline shows only the flat pattern's own item.
- FissionOxide: Create Flat Pattern opens an info panel "FLAT PATTERN" (Body "the sheet metal body", Flat size "60.00 × 110.26 mm", Bends 1, Ready to cut "passes the shop checks", Export "DXF...", Close): no face pick and no environment.
- Start here: `crates/ui/src/dialogs.rs:164` (title) and the FlatPattern rows after `crates/ui/src/dialogs.rs:3362`; a flat pattern environment like the sketch one (toolbar tabs in `crates/ui/src/menus.rs`, browser root in `crates/ui/src/panels.rs`).

## Export Flat Pattern dialog (P1)
- key: `ui:sheet:export-flat-pattern`
- Fusion: in the environment, EXPORT > Export Flat Pattern opens "EXPORT FLAT PATTERN": Flat Pattern Name "(Unsaved)", File Type dropdown (DXF, STEP), Units "Millimeter", "Splines to Polylines" checkbox (unticked), a "Visible Geometry" group with Center Lines and Extent Lines (both ticked), OK and Cancel.
- FissionOxide: the info panel's "DXF..." button; no STEP choice, units, spline or bend line options.
- Start here: the FlatPattern rows after `crates/ui/src/dialogs.rs:3362`, the DXF writer used by Export DXF (`ui:browser:export-dxf-dialog`, commit 6d42942).

## Flat Pattern browser row (P1)
- key: `ui:sheet:flat-pattern-row`
- Fusion: after Finish Flat Pattern the design's browser gains a "Flat Pattern" row (flat pattern icon) under the Rule row; right-click gives Activate, Export Flat Pattern, Delete (Del). After a later Unfold the row's icon turns into a yellow warning triangle until the model is refolded.
- FissionOxide: no Flat Pattern row.
- Start here: `crates/ui/src/panels.rs:1439` (browser rows under Document Settings), `crates/ui/src/marking.rs` (the row's context menu).

## Unfold dialog and the REFOLD panel (P1)
- key: `ui:sheet:unfold-refold`
- Fusion: "UNFOLD" opens with Reference [Select] (lit), Bends [Select], "Unfold all bends" checkbox, OK greyed; prompt "Select stationary entity"; while it is open the ribbon's MODIFY panel shrinks and a greyed REFOLD panel appears at the right. Picking the base face (blue) and ticking Unfold all bends previews the bends green. OK adds "Unfold1" (unfold icon) and the REFOLD panel becomes active; REFOLD > Refold runs at once with no dialog, adds a Refold item and the REFOLD panel goes away. There is no "Refold Faces" entry in MODIFY.
- FissionOxide: "UNFOLD" opens with Body "the sheet metal body", Stationary face [Select], Preview, OK lit and the sheet already shown flat; no Bends row or Unfold all bends; no REFOLD panel after OK; MODIFY > "Refold Faces" opens a "REFOLD FACES" dialog (Body "the unfolded sheet", Preview, OK).
- Start here: `crates/ui/src/dialogs.rs:162` (Unfold) and its rows near `crates/ui/src/dialogs.rs:3313`, `crates/ui/src/menus.rs:463` (SHEET_MODIFY's Refold Faces), the ribbon panels in `crates/ui/src/menus.rs:1109`.
