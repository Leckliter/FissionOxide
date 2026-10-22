# Parity batch: ui-reverify-sketch-2

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 1) re-checking the sketch area's
`fixing-merged` keys in the rebuilt app (`main` at `ef887079`). All four are reopened keys: the
earlier fix landed, and what is listed here is what still differs. The rows are already in
`parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this batch); read each row's
`history` before starting.

## Circular pattern preview and quantity box (P2)
- key: `ui:sketch:circular-pattern-rows` (reopened)
- Fusion: after the centre pick: Distribution (icon dropdown "Full"), Suppression, Quantity "3"; the instances preview as grey outlines; a floating quantity box "3" with a spinner and a rotate icon sits by the canvas top right. Nothing previews before the centre is picked.
- FissionOxide: the rows now match, but before any centre pick a solid-blue preview about a default centre and a "1" quantity box already show (the box over the palette's rows); after the pick the instances preview solid blue with long radial lines instead of grey outlines, and the quantity box sits at the canvas top left over the Unsaved banner. The centre point shared by two concentric circles could not be picked as Center Point (a rectangle corner could).
- Start here: `Kind::SkCircPattern` in `crates/ui/src/dialogs.rs:140` and its preview and floating box in `crates/ui/src/viewport.rs`; draw the preview only once the centre is set, with the guide grey (`SKETCH_GUIDE` in `crates/engine/src/scene.rs`).

## Repeat on the glyph menu repeats the last command (P2)
- key: `ui:sketch:constraint-glyph-context-menu` (reopened)
- Fusion: right-clicking a glyph opens the sketch marking menu with "Repeat <last command>" at the top, the last command being the constraint just used (e.g. "Repeat Horizontal/Vertical"), and the list OK (Return), Cancel (Esc), Create Selection Set, Find in Browser.
- FissionOxide: right after a Fix/UnFix command the ring shows "Repeat Line" (the last drawing tool), and the list is Finish Sketch, Look At, Fit, Measure.
- Start here: `ring_items` in `crates/ui/src/marking.rs:471` (`Action::Repeat` at `:474`): record constraint commands as the last command too; the glyph list in `overflow_items`.

## Text dialog font default (P2)
- key: `ui:sketch:text-dialog-rows` (reopened)
- Fusion: the TEXT dialog's Font starts on "Arial".
- FissionOxide: every row now matches except Font, which starts on "Sans (Ubuntu Light)" (the list is Sans and Mono only).
- Start here: `TEXT_FONTS` in `crates/ui/src/forms.rs:3080`; offer the system fonts (Arial first on Windows) or at least name the default Arial when it is installed.

## Project / Include dialogs: Spun Profile axis, Isoparametric OK, Preview rows (P2)
- key: `ui:sketch:project-include-menu` (reopened)
- Fusion: Spun Profile has Geometry, Selection Filter, Projection Link, Axis [Select], Project Axis checked, Add Centerline checked, Tolerance "0.00 in", Flip Result; Isoparametric Curve has Curve "Single", Face [Select], Projection Link, OK enabled. None of the Project / Include dialogs has a Preview checkbox, and Projection Link is an active checkbox.
- FissionOxide: the submenu lists all seven entries now. Spun Profile's Axis is a "Z axis" dropdown, with Projection Link, Project Axis and Tolerance greyed; Isoparametric Curve's Curve dropdown and OK are greyed; Intersect, Spun Profile, Project To Surface, Intersection Curve, Isoparametric Curve and Include 3D Geometry show an extra Preview checkbox, and Projection Link is greyed in Project To Surface, Intersection Curve, Isoparametric Curve and Spun Profile.
- Start here: `SK_SPUN_PROFILE` at `crates/ui/src/forms.rs:3934` and the axis choice at `crates/ui/src/dialogs.rs:5423`; the Preview row is the shared form footer in `crates/ui/src/forms.rs` (hide it for the sketch Project / Include forms).
