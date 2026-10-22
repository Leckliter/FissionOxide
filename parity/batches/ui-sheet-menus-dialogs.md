# Parity batch: ui-sheet-menus-dialogs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 15) in a new unsaved Part Design document in each app (millimeters),
FissionOxide built from `main` at `8b29a812`, on the SHEET METAL tab with a base flange and one edge
flange. Fusion on this Personal licence answers Join by bend, Sheet Metal Fillet and Sheet Metal
Chamfer with an "Upgrade to a Fusion subscription" card, so those dialogs are not compared. See
`parity/reports/2026-10-22-sheet-metal-tab.md`.

## SHEET METAL ribbon and dropdown lists (P2)
- key: `ui:sheet:ribbon-lists`
- Fusion: ribbon CREATE [Flange, Create Flat Pattern], MODIFY [Sheet Metal Rules, Sheet Metal Fillet, Sheet Metal Chamfer, Fillet, Chamfer, Bill of Materials] (Sheet Metal Fillet / Chamfer and Join by bend carry the blue round "subscription" badge). CREATE menu: Create Sketch, Flange (Base/Edge/Contour), Hem, Lofted Flange, Fold, Convert to Sheet Metal (Ctrl+Alt+S), Derive | Extrude (E) | Hole (H), Thread, Pattern >, Mirror | Create Flat Pattern, Joint Origin (no separator between Thread and Pattern). MODIFY menu: Unfold, Sheet Metal Rules, Corner Closure, Join by bend, Rip | Sheet Metal Fillet, Sheet Metal Chamfer, Fillet (F), Chamfer | Move/Copy (M), Align, Delete (Del), Remove | Physical Material, Appearance (A), Manage Materials | Change Parameters, Compute All (Ctrl+B) | Bill of Materials. The Bill of Materials ribbon tooltip is the bare name.
- FissionOxide: ribbon MODIFY is [Sheet Metal Rules, Join by bend, Sheet Metal Fillet, Fillet, Chamfer, Bill of Materials] (Join by bend instead of Sheet Metal Chamfer, no badges); CREATE menu adds Miter Flange, Flat Pattern Sketch, All Flat Patterns on One Sheet and Cut Service Check and a separator between Thread and Pattern; MODIFY adds Refold Faces; the Bill of Materials tooltip has a description line.
- Start here: `crates/ui/src/menus.rs:438` (SHEET_CREATE), `crates/ui/src/menus.rs:463` (SHEET_MODIFY), `crates/ui/src/menus.rs:1109` (the ribbon's quick buttons). Keep the extra FissionOxide commands reachable elsewhere (UTILITIES or a FissionOxide submenu) rather than deleting them.

## Fold dialog (P2)
- key: `ui:sheet:fold-dialog`
- Fusion: "FOLD" opens with Reference [Select] (lit) above an empty selection list with + and x, OK greyed; prompt "Click on the stationary side of a face on a sheet metal body."
- FissionOxide: "FOLD" opens with Sketch dropdown "Sketch1", Bend line dropdown "Sketch line 8", Angle "90 deg", Flip direction and "Fold the other side" checkboxes, a red "the bend line must cross the sheet" and OK lit.
- Start here: `crates/ui/src/dialogs.rs:159` (Fold) and its defaults near `crates/ui/src/dialogs.rs:931`.

## Convert to Sheet Metal dialog (P2)
- key: `ui:sheet:convert-dialog`
- Fusion: "CONVERT TO SHEET METAL" opens with one row, Source [Select] (lit), OK greyed.
- FissionOxide: Body [Select], Stationary face [Select], Rule "Default", Preview; prompt "Select a body".
- Start here: `crates/ui/src/dialogs.rs:160` (ConvertSheet) and its rows in `crates/ui/src/dialogs.rs`.

## Rip and Corner Closure dialogs (P2)
- key: `ui:sheet:rip-corner-dialogs`
- Fusion: "RIP" opens with one row, Face/Edge/Vertex [Select] (lit), OK greyed. "CORNER CLOSURE" opens with one row, Edges [Select] (lit), OK greyed.
- FissionOxide: "RIP" has Edges [Select], Gap "0.5 mm", Preview (prompt "Select edges"); "CORNER CLOSURE" has Corners "Miter", Corner relief "None", "Relief size (empty = default)" and OK lit with nothing picked.
- Start here: `crates/ui/src/dialogs.rs:161` (Rip), `crates/ui/src/forms.rs:2554` (CORNER_CLOSURE).

## Hem rows after the edge pick (P2)
- key: `ui:sheet:hem-rows`
- Fusion: after an edge pick, HEM (the Flange dialog with the Hem icon lit) shows "Edge 1 | Full Edge", Edges "1 selected", Type dropdown with icons (Flat, Open, Rolled, Teardrop, Rope), Length "Thickness * 4" (fx), Bend Position "Adjacent", Flip, a Bend Override group; an arrow and a floating box "Thickness * 4" on the edge; prompt "Adjust settings or hold Ctrl to modify selection".
- FissionOxide: HEM has Edges, Length "5 mm", Gap "0.5 mm", Flip and Preview; no hem types.
- Start here: `crates/ui/src/dialogs.rs:927` (Hem defaults), the Hem rows near `crates/ui/src/dialogs.rs:3271`.
