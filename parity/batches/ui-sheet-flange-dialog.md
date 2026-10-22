# Parity batch: ui-sheet-flange-dialog

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 15) in a new unsaved Part Design document in each app (millimeters),
FissionOxide built from `main` at `8b29a812`, on the SHEET METAL tab, after a 100 x 60 mm rectangle
sketched on XY. Fusion has one Flange command whose dialog changes with the pick (Base, Edge,
Contour) and whose Type row also holds Hem and Lofted Flange. See
`parity/reports/2026-10-22-sheet-metal-tab.md`.

## One Flange dialog with Type icons (P1)
- key: `ui:sheet:flange-dialog-open`
- Fusion: "FLANGE" opens with Type as three icon toggles (Flange (Base/Edge/Contour) lit, Hem, Lofted flange; hovering each shows its tooltip), an empty selection list with + and x below it, "Edges/Profiles" Select chip (lit), OK greyed, Cancel; cursor prompt "Select sheet metal edges or sketch geometry to use for flange". CREATE > Hem opens the same dialog titled "HEM" with the Hem icon lit and prompt "Select an edge to hem"; Lofted Flange likewise. Nothing is picked for you, even with one unconsumed profile.
- FissionOxide: "FLANGE" opens with a Type dropdown (Base (profile), Edge, Contour (open path)), Profiles chip already "1 selected" (the only profile, auto-picked, even the profile the first flange consumed), Rule dropdown, Operation (New Body), Flip, OK lit and a preview. Once a profile is picked the Type dropdown no longer opens and the chip's x does not clear it. Hem is a separate "HEM" dialog (Edges, Length 5 mm, Gap 0.5 mm, Flip, Preview; prompt "Select edges").
- Start here: `crates/ui/src/dialogs.rs:772` (auto-pick of the only profile for Flange and Fold), `crates/ui/src/dialogs.rs:912` (Flange defaults), `crates/ui/src/dialogs.rs:3237` (the Type row), `crates/ui/src/menus.rs:438` (SHEET_CREATE).

## Base Flange rows after the profile pick (P1)
- key: `ui:sheet:base-flange-rows`
- Fusion: after a profile pick the title becomes "BASE FLANGE"; the list shows "Profile 1"; rows: Profiles "1 selected" with x (tooltip "Select more closed sketch profiles, adjust settings, or hold Ctrl to modify selection"), Orientation dropdown with icons (Side 1, Side 2, Center), Sheet Metal Rule dropdown (".063 Aluminum", every library rule listed); the preview is solid blue; status bar "1 Profile | Area : 6000.00 mm^2". No Operation or Flip rows. The timeline item is "BaseFlange1" with a flat-sheet icon.
- FissionOxide: title stays "FLANGE"; rows Type, Profiles, Rule ("Default"), Operation, Flip; the preview is body grey; the timeline item is "Flange1" drawn with the Extrude icon.
- Start here: `crates/ui/src/dialogs.rs:157` (title), `crates/ui/src/dialogs.rs:3237`, `crates/ui/src/panels.rs:3696` (timeline icons of sheet metal features).

## Edge Flange rows, manipulator and direction (P1)
- key: `ui:sheet:edge-flange-rows`
- Fusion: picking a sheet edge makes the title "EDGE FLANGE": list "Edge 1 | Full Edge" (a dropdown), Edges/... "1 selected", Extent "Distance" (dropdown with icon), Height "0.00 mm" (OK greyed until it is non-zero), Height Datum "Outer Faces", Bend Position "Inside", Flip (icon), Bend Radius (from the rule), a "Bend Override" group (Relief shape "Default (Round)", greyed width / depth / remnant expressions with fx), "2-Bend Corner Override" (Relief shape, size, placement) and "3-Bend Corner Override" (Relief shape, radius). An arrow manipulator and a focused floating value box "0.00 mm" sit on the edge; cursor prompt "Select more sheet metal edges or specify height to extrude"; status "1 Edge | Length : 60.00 mm". Typing 20 folds the flange up, away from the sketch side, with an angle arc. Timeline "EdgeFlange2" (one counter for all flanges) with a bent-flange icon; Edit shows "EDIT EDGE FLANGE" without the Type row and with Angle "90.0 deg".
- FissionOxide: with the edge picked first, "FLANGE" opens as Type Edge: Edges, Height 10 mm, Bend angle 90 deg, Position Adjacent, Corners Miter, Flip, Preview; no manipulator or value box; the flange folds down, under the plate; the timeline item is "EdgeFlange1" with the Extrude icon.
- Start here: `crates/ui/src/dialogs.rs:912`, `crates/ui/src/dialogs.rs:3237`, the edge flange direction in `crates/model/src/regen/sheetmod.rs`, the manipulator wiring of Extrude in `crates/ui/src/viewport.rs`.

## Edit sheet metal features (P1)
- key: `ui:sheet:edit-feature`
- Fusion: double-clicking a sheet metal timeline item (BaseFlange, EdgeFlange, Unfold, ...) opens its dialog in edit mode ("EDIT EDGE FLANGE" with the current values and the manipulator).
- FissionOxide: double-clicking Flange1 or EdgeFlange1 prints "This feature has no editable properties yet" at the lower left: `Dialog::edit` has no arm for any sheet metal feature.
- Start here: `crates/ui/src/dialogs.rs:1121` (`Dialog::edit`), `crates/ui/src/panels.rs:3766` (`edit_feature`).
