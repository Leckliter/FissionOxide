# Parity batch: ui-leftovers-dialogs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

These rows were recorded by probers and fixers between 2026-10-21 and 2026-10-22 but never got a
batch (most were found by a fixer while working on a neighbouring item). The coordinator grouped
them on 2026-10-22; the wording below is the ledger row's own. Re-read the row in
`parity/ui-features.jsonl` (its `history`) before starting, and where a row says Fusion's side was
not observed, check it in Fusion's documentation and say so in your ledger note.

## flip icon toggles (P3)
- key: `ui:dialogs:flip-icon-toggles`
- Fusion: Where a dialog can flip a direction (Draft, Flange, Hem, Fold, Joint), Fusion shows an icon toggle button beside the value, not a labelled checkbox row.
- FissionOxide: Draft, Flange, Hem, Fold ("Flip direction") and Joint show a "Flip" checkbox row (Self::check in dialogs.rs). Found while removing Extrude's Flip row (ui-dialog-docking).
- Start here: `crates/ui/src/dialogs.rs`.

## revolve all regions (P2)
- key: `ui:dialogs:revolve-all-regions`
- Fusion: Revolve opens with Profiles = Select and nothing previewed until a profile is picked, as Extrude does.
- FissionOxide: Revolve opens with the chip "Select (all regions)" and previews a revolve of every closed region of the visible sketch (Dialog::ready treats Revolve as ready with just a sketch). Found while fixing ui:dialogs:extrude-all-regions.
- Start here: `crates/ui/src/dialogs.rs`.

## primitive drawing extras (P2)
- key: `ui:dialogs:primitive-drawing-extras`
- Fusion: While a primitive's base is drawn a temporary Sketch row shows in the browser and timeline (gone after OK); the rectangle's and circle's sizes are typed heads-up boxes; the preview body is grey with its base edges blue; Coil has a rotation handle.
- FissionOxide: Found while fixing ui-primitive-dialogs: the drawing step has no temporary sketch row, its sizes are read-outs, the preview body is drawn as any body and Coil has no rotation handle.
- Start here: `crates/ui/src/dialogs/primitive.rs`.

## pipe cut switch and arrows (P2)
- key: `ui:dialogs:pipe-cut-and-arrows`
- Fusion: With the pipe preview cutting a body, Operation switches to Cut (red preview) and an "Objects To Cut" group shows; arrows at both ends of the path and on the section size.
- FissionOxide: Found while fixing ui-primitive-dialogs: Pipe keeps New Body over a body and has no Objects To Cut group or arrows.
- Start here: `crates/ui/src/dialogs/primitive.rs`.

## Other edit dialogs keep their selection rows live (P2)
- key: `ui:dialogs:edit-selection-rows-others`
- Fusion: Every edit dialog is the creation dialog filled in, its selection chips live (Revolve's Profile and Axis, Shell's Faces/Bodies, Offset Plane's and Mirror's Plane, patterns' objects).
- FissionOxide: Revolve, Shell, Offset Plane, Mirror and the patterns still show "(unchanged)" in their edit dialogs.
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/dialogs/solid.rs`.

## join of separate bodies (P2)
- key: `ui:dialogs:combine-join-disjoint`
- Fusion: Combine Join of two bodies that don't touch makes one body (Body1, two lumps); the tool body's row goes and no new body appears.
- FissionOxide: Combine (Join) of two boxes 20 mm apart leaves Body1 and makes a new Body3 for the second lump (Body2 goes); found by the ui-dialog-selection-state fixer, not fixed there.
- Start here: `crates/model/src/regen.rs`.

## Replace Face with a tangent chain of curved faces (P2)
- key: `ui:dialogs:replace-face-chain-panic`
- Fusion: REPLACE FACE with Tangent Chain on: a rounded slab's top picked, the top, its edge rounds and the faces they continue are replaced by the target plane.
- FissionOxide: replace_face with tangent_chain over flat and rounded faces panics in the engine (slice index); single flat faces work.
- Start here: `crates/engine/src/solid.rs`.

## Remove takes components (P3)
- key: `ui:dialogs:remove-components`
- Fusion: REMOVE's Objects chip takes bodies or components (prompt "Select bodies or components to remove").
- FissionOxide: FissionOxide's REMOVE Objects chip takes bodies only (body.remove removes bodies); a picked component isn't taken.
- Start here: `crates/ui/src/forms.rs`, `crates/engine/src/commands.rs`.

## Convert Design Type in an Assembly Design (P2)
- key: `ui:assemble:convert-design-type-assembly`
- Fusion: Convert Design Type opens on the design's own type and converts between Part, Assembly and Hybrid Design.
- FissionOxide: Designs carry a design type now (batch ui-insert-new-design), but CONVERT still opens on Part, lists Assembly and Hybrid greyed and its Convert button stays greyed, so an Assembly Design can't be converted.
- Start here: `crates/ui/src/doc_dialogs.rs`.
