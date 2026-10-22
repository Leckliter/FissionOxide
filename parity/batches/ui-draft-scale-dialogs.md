# Parity batch: ui-draft-scale-dialogs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-22 (prober 9): SOLID > MODIFY in each app; Fusion in inches, FissionOxide in mm on
two 20 mm boxes, built from `main` at `ffa348e2`. Fusion dialogs open with selection chips ("Select",
OK greyed), add value rows after the picks, and show the prompt next to the cursor. See
`parity/reports/2026-10-22-solid-modify.md`.

## Draft dialog rows and flow (P1)
- key: `ui:dialogs:draft-rows`
- Fusion: DRAFT opens with Type (two icon toggles: Fixed Plane, Parting Line), Pull Direction (chip, active, prompt "Select pull direction"; the origin planes show for picking), Faces (chip), Tangent Chain (checked), Draft Sides ("One Side" dropdown: One Side, Two Side, Symmetric), OK greyed. After a planar face as pull direction and a face to draft, "Flip Pull Direction" (icon) and "Angle 0.0 deg" rows appear, the prompt reads "Specify draft angle", the pull face shows a black arrow with an angle-ring handle, both picks are blue, and OK stays greyed until the angle is not zero.
- FissionOxide: DRAFT opens with Faces (chip, first), Pull direction (dropdown preset "XY plane"), Angle "3 deg", Flip (checkbox), Preview; prompt "Select faces to draft", then "Hold Ctrl to modify selection"; the picked face is not shown blue; OK is enabled as soon as a face is picked.
- Start here: `crates/ui/src/dialogs.rs:3066` (rows), `crates/ui/src/dialogs.rs:820` (defaults: no preset plane, 0 deg), `crates/ui/src/dialogs.rs:2279` (prompts) and `crates/ui/src/dialogs/manip.rs:276` (angle handle).

## Draft is not editable (P1)
- key: `ui:dialogs:draft-edit`
- Fusion: Double-clicking the Draft timeline icon opens EDIT FEATURE with the same rows filled (Pull Direction 1 selected, Faces 1 selected, Angle 5 deg) and the arrow and angle handle on the model.
- FissionOxide: Double-clicking Draft1 shows "This feature has no editable properties yet" at the lower left.
- Start here: `crates/ui/src/panels.rs:3768` (route Draft to the DRAFT dialog in edit mode, as Extrude and Fillet do).

## Scale dialog rows and flow (P1)
- key: `ui:dialogs:scale-rows`
- Fusion: SCALE opens with Entities (chip, active, prompt "Select bodies or sketches to scale"), Point (chip), Scale Type ("Uniform" dropdown: Uniform, Non Uniform), OK greyed. Picking a body fills Point automatically ("1 selected"), adds "Scale Factor 1.00", shows an arrow manipulator and a focused floating box "1.00", and enables OK.
- FissionOxide: SCALE opens with a "Bodies (default all)" chip, Point as three text boxes (0 0 0), "Scale factor 2" and OK enabled; the preview doubles every body at once; a click on a body does not fill the chip; no Scale Type, manipulator or floating box.
- Start here: `crates/ui/src/forms.rs:832` (`SCALE` form: chips for Entities and Point, Scale Type, factor 1 after the pick, no all-bodies default).

## Offset Face rows (P2)
- key: `ui:dialogs:offset-face-rows`
- Fusion: OFFSET FACE opens with Faces only (prompt "Select face(s) to offset"); after a pick "Distance 0.00 in" appears with an arrow on the face and a focused floating box, and OK stays greyed until the distance is not zero.
- FissionOxide: Opens with Faces, Distance "1 mm" and Preview, prompt "Faces"; no arrow or floating box after the pick; OK enabled after the pick.
- Start here: `crates/ui/src/forms.rs:872` (`OFFSET_FACE`: Distance row only after a pick, default 0, the prompt) and `crates/ui/src/dialogs/manip.rs` (distance arrow).
