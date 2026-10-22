# Parity batch: ui-reverify-dialogs-2

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 2) re-checking the dialogs area's
`fixing-merged` keys in the rebuilt app (`main` at `b399613d`). All six are reopened keys: Edit
Feature dialogs and body picks whose fix landed only in part. The rows are already in
`parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this batch); read each row's
`history` before starting.

## Edit Mirror keeps the old layout (P1)
- key: `ui:dialogs:edit-mirror`
- Fusion: double-clicking Mirror1 opens EDIT FEATURE with Object Type (Bodies), Objects "1 selected", Mirror Plane "1 selected", Operation (New Body); the source body and the plane are drawn blue, the mirrored body as a grey see-through preview, status "2 selections".
- FissionOxide: EDIT FEATURE shows the old rows (Plane, Type, Bodies, "Join to original"), the plane is not blue, there is no see-through preview, status "Body1". The creation dialog already has Fusion's rows (`ui:dialogs:mirror-rows` is verified): open the edit on the same rows.
- Start here: `crates/ui/src/dialogs.rs` (`Kind::Mirror` edit start) and the edit path of commit `967a7dc`.

## Body chips light only the clicked face (P1)
- key: `ui:dialogs:body-pick-highlight`
- Fusion: with a body chip active, hovering and picking a body turns the whole body blue and the status bar names it ("Body1").
- FissionOxide: Combine, Split Body and Scale do this now. Remove's Objects chip does not (hover lights nothing, status "1 Face | Area"); Move/Copy's Bodies, Mirror's Objects and Circular Pattern's Objects pick the body but turn only the clicked face blue (status "1 Face").
- Start here: `crates/ui/src/dialogs/body_picks.rs:13` (`picks_bodies`, the list of dialogs that take body priority) and `:19`.

## Mirror and pattern originals blue as a whole (P2)
- key: `ui:dialogs:copy-ghost-preview`
- Fusion: the copies preview as see-through grey ghosts and the picked originals turn blue.
- FissionOxide: the ghosts match now; the original shows only the clicked face blue. Fixing `ui:dialogs:body-pick-highlight` for Mirror and the patterns should cover this.
- Start here: as above.

## Edit Combine shows both bodies picked (P2)
- key: `ui:dialogs:edit-combine`
- Fusion: double-clicking Combine1 rolls back so target and tool are separate, both blue, status "2 Bodies".
- FissionOxide: the rows are filled, but only the target is blue, status "Body1", and the browser keeps the post-combine names (Body1, Body3) during the edit.
- Start here: the Combine edit path of commit `967a7dc` in `crates/ui/src/dialogs.rs`; `Dialog::browser_bodies` in `crates/ui/src/dialogs/live.rs`.

## Edit Draft handles (P2)
- key: `ui:dialogs:draft-edit`
- Fusion: EDIT FEATURE for a Draft shows the pull arrow and the angle handle on the model, both picks blue, Angle "5 deg".
- FissionOxide: rows filled, but no arrow or angle handle, picks not blue, Tangent Chain greyed, Angle "5" with no unit.
- Start here: the Draft dialog's handles in `crates/ui/src/dialogs/solid.rs` (commit `2e7c2d7` notes why they are skipped while editing: the stored picks name the body before the draft).

## Edit Hole glyph, arrow and planes (P2)
- key: `ui:dialogs:edit-hole`
- Fusion: Edit Hole shows the position glyph and the depth arrow on the face and the origin planes.
- FissionOxide: the whole Hole dialog opens filled, but none of the three is drawn, and the whole body turns blue.
- Start here: the Hole edit path of commit `967a7dc`; `Dialog::handles` in `crates/ui/src/dialogs.rs`.
