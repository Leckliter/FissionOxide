# Parity batch: ui-reverify-dialogs-8

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-23 by the open-row pass (verifier 8) on `main` at `6c5c8b6b`: two rows were open
with no batch and are confirmed in the app, two are new. Every row is already in
`parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this batch); read each row's
`history` before starting.

## ui-reverify-dialogs-8 (P2)

- key: `ui:dialogs:move-rows-after-pick`
- Fusion: MOVE/COPY in 3D opens with Move Object (Bodies...), Selection [Select], Move Type (five icons) and Set Pivot only; X/Y/Z Distance "0.00 mm", X/Y/Z Angle "0.0 deg" and Create Copy appear once a body is picked.
- FissionOxide: M with nothing selected shows all six boxes and Create Copy before any pick. The sketch MOVE/COPY already waits for a pick; do the same in 3D (Paste and Edit Move keep their rows).
- Start here: `crates/ui/src/dialogs/move_copy.rs:887`

- key: `ui:dialogs:checkbox-label-column`
- Fusion: a check box row puts its label in the label column and the box alone in the value column ("Create Copy  [ ]").
- FissionOxide: check boxes drawn directly (not through `Dialog::check`) still carry their label beside the box: MOVE/COPY's Create Copy (confirmed in the app), sketch Circular Pattern's Symmetric and Join instances, Zebra / Draft analysis rows, Interference's Include Coincident Faces and Create components, Section Analysis's Show Hatch, Tessellate's quads row.
- Start here: `crates/ui/src/dialogs/move_copy.rs:954`, `crates/ui/src/dialogs.rs` (~3198), `crates/ui/src/dialogs/analysis.rs`, `crates/ui/src/dialogs/interference.rs`, `crates/ui/src/dialogs/section.rs`, `crates/ui/src/dialogs/mesh.rs`

- key: `ui:surface:patch-interior-rails`
- Fusion: PATCH's Interior Rails/Points row ("Select") takes curves and points the patch passes through, picked in the canvas while the row is active.
- FissionOxide: the row's Select chip is greyed: a form can't route picks to a second edge row yet (the kernel's `patch_with` already takes interior curves).
- Start here: `crates/ui/src/dialogs/surface.rs:711`, `crates/ui/src/forms.rs`

- key: `ui:sketch:dim-edit-tooltip-style`
- Fusion: double-clicking a sketch dimension opens its edit box ("10 mm", selected, with a ⋮) and under it a dark tooltip with white text naming the parameter ("d25 : 10 mm").
- FissionOxide: the "d1 : 10 mm" tooltip appears with the pointer over the box, but as a light box with dark text. Use the dark style the hover tooltip (`ui:sketch:dimension-hover-tooltip-style`, verified) already has.
- Start here: `crates/ui/src/viewport.rs:3127`
