# Parity batch: ui-reverify-dialogs-1

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 2) re-checking the dialogs area's
`fixing-merged` keys in the rebuilt app (`main` at `b399613d`). Seven are reopened keys (the fix
landed but part of Fusion's behaviour is still missing) and one is a new row seen along the way.
The rows are already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this
batch); read each row's `history` before starting.

## Thicken does not open from the CREATE menu (P1)
- key: `ui:dialogs:thicken-rows`
- Fusion: SOLID > CREATE > Thicken opens THICKEN with nothing picked: Faces / Surface Bodies (Select chip, prompt "Select Surfaces to Thicken"), Chain Selection, Direction, Thicken Type icons, Operation.
- FissionOxide: in a design with only solid bodies, CREATE > Thicken highlights and takes the click but opens nothing, twice, with no lower-left message. The form exists and its test passes, so the menu path or a precondition swallows it.
- Start here: `crates/ui/src/menus.rs:239` and `:477` (`D(Kind::Form(&forms::THICKEN))`), `crates/ui/src/forms.rs:1701` (`THICKEN`), the test `dialogs::solid::thicken_rows` at `crates/ui/src/dialogs/solid.rs:1646`.

## Hole preview, Face chip and prompt (P2)
- key: `ui:dialogs:hole-fields-2`
- Fusion: the hole previews in red with a manipulator on it; the Face chip reads "1 selected"; the prompt after placing is "Select linear or circular reference edges to fully define hole location"; no Preview checkbox.
- FissionOxide: the rows match now, but the hole previews as a plain grey cut with no manipulator, the Face chip reads "Face of Body1", the prompt is "Specify hole diameter and depth", and a Preview checkbox sits in the footer.
- Start here: `crates/ui/src/dialogs.rs:525` (the "Face of" chip text) and `:924`; the Hole rows in `crates/ui/src/dialogs/solid.rs`.

## Chamfer Corner Type, Type text and prompt (P2)
- key: `ui:dialogs:chamfer-fields-2`
- Fusion: Corner Type is live (Chamfer / Miter / Blend, each with an icon); Type reads "Equal Distance" with an icon; after an edge pick the prompt is "Specify chamfer distance"; no Preview checkbox.
- FissionOxide: Corner Type is greyed at Chamfer, Type reads "Equal distance" with no icon, the prompt reads "Hold Ctrl to modify selection", and a Preview checkbox is in the footer.
- Start here: `crates/ui/src/dialogs/solid.rs:642` (Corner Type) and the chamfer type dropdown near it.

## Move/Copy's Selection row, prompt, units and pick highlight (P2)
- key: `ui:dialogs:move-set-pivot`
- Fusion: MOVE/COPY's second row is "Selection"; the prompt is "Select bodies to move"; the distance boxes read "0.00 in" (document unit); a picked body turns blue as a whole.
- FissionOxide: the row is labelled "Bodies", the prompt is "Select bodies", the boxes read "0" with no unit, and a pick turns only the clicked face blue (status "1 Face"). Edit Move already gets these right (Selection, "0 mm", whole body blue): reuse that.
- Start here: the Move dialog rows in `crates/ui/src/dialogs/solid.rs` (search "Set Pivot"), and `crates/ui/src/dialogs/body_picks.rs:19` (`body_pick_priority`) for the whole-body highlight.

## Extrude from a body face starts at 10 mm (P2)
- key: `ui:dialogs:extrude-body-face-pick`
- Fusion: E then a click on a body's planar face adds Distance "0.00" and Taper Angle "0.0 deg"; OK stays greyed until a distance is typed; no Preview checkbox.
- FissionOxide: Distance starts at "10 mm" with a preview already shown, Operation switches to Join, and a Preview checkbox appears in the footer only after the face pick.
- Start here: the Extrude face-pick path added in `05f467f` (search `extrude` and `Sel::Face` in `crates/ui/src/dialogs.rs`), and `crates/ui/src/dialogs/live.rs:200` for the checkbox.

## Draft: both picks blue, angle unit (P2)
- key: `ui:dialogs:draft-rows`
- Fusion: after the pull face and a face to draft, both picks are blue on the preview; Angle shows "5.0 deg" after typing 5.
- FissionOxide: only the pull face stays blue; the drafted face is not; the Angle row shows "5" with no unit.
- Start here: the Draft dialog in `crates/ui/src/dialogs/solid.rs` (commit `2e7c2d7`).

## Preview checkbox Fusion's dialogs do not have (P2)
- key: `ui:dialogs:preview-checkbox-extra`
- Fusion: command dialogs end with the (i) icon and OK / Cancel; there is no Preview checkbox (Chamfer and Replace with Primitives seen in Fusion this session).
- FissionOxide: a ticked "Preview" checkbox in Extrude (after a body-face pick), Chamfer, Hole, Combine, Replace Face, Split Face, Silhouette Split, Pattern on Path, Offset Face (Press Pull) and Replace with Primitives.
- Start here: `crates/ui/src/dialogs/live.rs:213` (`ui.checkbox(&mut self.live.on, "Preview")`) and `forms::preview_box` / `forms::no_preview` in `crates/ui/src/forms.rs`.

## Boundary Fill dialog layout (P2)
- key: `ui:dialogs:boundary-fill-rows`
- Fusion: BOUNDARY FILL is a normal-width dialog: Select Tools, Select Cells, Operation, then Remove Tools (unchecked) on its own row.
- FissionOxide: the dialog opens about 370 px wide, its title underline stops halfway, and Remove Tools sits at the far right of the Operation row.
- Start here: `crates/ui/src/forms.rs:3449` (`BOUNDARY_FILL`) and the form row layout that puts a checkbox beside the previous row.
