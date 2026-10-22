# Parity batch: ui-edit-more-features

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 11): a model with a box, a cylinder, a chamfer, a hole, a mirror, a
move, a combine and a press pull in each app, then a double-click on each timeline icon. Fusion
opened "EDIT FEATURE" for every one (Cylinder, Chamfer, Mirror, Hole, Move, Combine); FissionOxide
built from `main` at `ed3546c5`. `Dialog::edit` (`crates/ui/src/dialogs.rs:1084`) returns `None` for
every kind it does not list (Hole, Mirror, Combine, OffsetFaces / Press Pull, Draft, Scale, Loft,
Sweep, Coil, Pipe, Thread, Rib, Thicken, PlaneBy, ConstructionAxis, ConstructionPoint and more), and
`edit_feature` (`crates/ui/src/panels.rs:3622`) then shows "This feature has no editable properties
yet". Draft's edit is already `ui:dialogs:draft-edit` (batch `ui-draft-scale-dialogs`); the general
rollback and live selection rows are `ui:timeline:edit-rollback` and `ui:dialogs:edit-selection-rows`
(batch `ui-edit-feature-errors`). See `parity/reports/2026-10-22-edit-construct.md`.

## Hole can be edited (P1)
- key: `ui:dialogs:edit-hole`
- Fusion: Double-clicking Hole1 opens "EDIT FEATURE" with the whole Hole dialog: Placement icons, Face "1 selected" chip, Reference / Distance / Reference rows, Shape Settings (Extents, Hole Type, Hole Tap Type, Drill Point icons, the drawing with its value boxes), the position glyph and depth arrow on the face, and the origin planes shown.
- FissionOxide: Double-clicking Hole1 selects the body and shows "This feature has no editable properties yet" at the lower left.
- Start here: `crates/ui/src/dialogs.rs:1084` (`Dialog::edit`: a `FeatureKind::Hole` arm loading diameter, depth, kind, head and tip values) and `crates/ui/src/dialogs.rs:1601` (the `feature.edit` fields).

## Mirror can be edited (P1)
- key: `ui:dialogs:edit-mirror`
- Fusion: Double-clicking Mirror1 opens "EDIT FEATURE" with Object Type (Bodies), Objects "1 selected" chip with its x, Mirror Plane "1 selected" chip with its x, and Operation (New Body); the source body and plane are drawn selected blue, the mirrored body as a grey see-through preview, status "2 selections".
- FissionOxide: "This feature has no editable properties yet"; the mirrored body is selected.
- Start here: `crates/ui/src/dialogs.rs:1084` (a `FeatureKind::Mirror` arm, with the picks loaded as chips).

## Combine can be edited (P1)
- key: `ui:dialogs:edit-combine`
- Fusion: Double-clicking Combine1 rolls back so target and tool are separate again, both selected blue, and opens "EDIT FEATURE" with Target Body "1 selected", Tool Bodies "1 selected", the Operation icons and Keep Tools; status "2 Bodies".
- FissionOxide: "This feature has no editable properties yet".
- Start here: `crates/ui/src/dialogs.rs:1084` (a `FeatureKind::Combine` arm) and `crates/ui/src/dialogs/solid.rs` (the Combine rows).

## Press Pull / Offset Face can be edited (P1)
- key: `ui:dialogs:edit-press-pull`
- Fusion: Every Press Pull result can be edited; Fusion's Offset Face opens the same Faces / Offset Type / Distance dialog titled "EDIT FEATURE" (and a Press Pull on a primitive's end face edits that primitive, see `ui:dialogs:press-pull-edits-owner`).
- FissionOxide: Press Pull on a cylinder top adds "PressPull1"; double-clicking it shows "This feature has no editable properties yet".
- Start here: `crates/ui/src/dialogs.rs:1084` (a `FeatureKind::OffsetFaces` arm with Distance and Offset Type; faces as a chip).

## Edit Move matches Fusion's (P2)
- key: `ui:dialogs:edit-move-rows`
- Fusion: Edit Move shows Move Object as its icon only, Selection "1 selected" (live chip, the body blue), Move Type with Free Move pressed, Set Pivot, all six boxes reset to "0.00 in" / "0.0 deg" (a further move adds to the stored one), the triad on the body, and no Create Copy row.
- FissionOxide: Edit Move/Copy shows a greyed "Bodies" dropdown, Bodies "Select" (empty), the Translate type with only X/Y/Z Distance showing the stored "10", a greyed Create Copy checkbox, and no triad.
- Start here: `crates/ui/src/dialogs/move_copy.rs:174` (`load_move`) and the Move rows' `editing` branches in `crates/ui/src/dialogs/move_copy.rs`.

## Edit boxes show the unit (P2)
- key: `ui:dialogs:edit-values-units`
- Fusion: A chamfer created by typing "0.1" reopens with "0.1 in" in the set row and the floating box.
- FissionOxide: A chamfer created by typing "2" reopens with "2" (no unit) in the set row and the floating box.
- Start here: `crates/ui/src/dialogs.rs:1215` (the `u.shown` pass covers distance, radius and spacing; the chamfer's set values and the floating box are loaded elsewhere, `load_chamfer`), or store typed bare numbers with the design unit when the feature is made.
