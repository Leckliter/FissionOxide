# Parity batch: ui-combine-split-dialogs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-22 (prober 9): SOLID > MODIFY in each app; Fusion in inches, FissionOxide in mm on
two 20 mm boxes, built from `main` at `ffa348e2`. Fusion dialogs open with selection chips ("Select",
OK greyed), add value rows after the picks, and show the prompt next to the cursor. See
`parity/reports/2026-10-22-solid-modify.md`.

## Combine rows (P1)
- key: `ui:dialogs:combine-rows`
- Fusion: COMBINE: Target Body (chip, active, prompt "Select target body"), Tool Bodies (chip; after the target pick it becomes active and the prompt reads "Select bodies to combine with target body"), Operation (three icon toggles Join / Cut / Intersect), Keep Tools (checkbox, label on the left), OK greyed. The picked target body turns blue as a whole and the status bar reads "Body1".
- FissionOxide: Target (chip showing the body's name), Tools (a plain number "0"), Operation (dropdown "Join"), a "Keep tools" checkbox with the label on the right, Preview; prompt "Select tool bodies" after the target; only the clicked face of the target turns blue and the status bar reads "1 Face | Area ...". Supersedes the old `ui:dialogs:combine-fields` match, which compared row names only.
- Start here: `crates/ui/src/dialogs.rs:2614` (rows: a Tool Bodies chip "n selected", icon toggles for Operation as in Extrude, Fusion's prompts) and `crates/ui/src/dialogs.rs:1592`.

## Replace Face rows (P2)
- key: `ui:dialogs:replace-face-rows`
- Fusion: REPLACE FACE: Source Faces (chip, prompt "Select faces to replace"), Tangent Chain (checkbox, off), Target Faces (chip). After a source pick the Source chip stays active; OK greyed until a target is picked; no error line.
- FissionOxide: Source faces, Target face, "Or a plane" dropdown (None), Preview; prompt "Source faces"; after the source pick a red line "missing 'target' (a face) or 'plane'" shows and OK is enabled.
- Start here: `crates/ui/src/forms.rs:2271` (`REPLACE_FACE`: Fusion's labels, Tangent Chain, no plane dropdown or error line, OK greyed until complete).

## Split Face rows (P1)
- key: `ui:dialogs:split-face-rows`
- Fusion: SPLIT FACE: Faces to Split (chip, prompt "Select Face to split"), Splitting Tool(s) (chip), Chain Selection (checked), Split Type ("Split with Surface" dropdown: Split with Surface, Along Vector, Closest Point), Extend Splitting Tool (checked); OK greyed.
- FissionOxide: Faces to split (chip), Splitting tool (dropdown preset "XY plane"), Preview; prompt "Faces to split".
- Start here: `crates/ui/src/forms.rs:847` (`SPLIT_FACE`: a tool chip instead of the preset plane, Chain Selection, Split Type, Extend Splitting Tool).

## Split Body rows and pick (P1)
- key: `ui:dialogs:split-body-rows`
- Fusion: SPLIT BODY: Body to Split (chip, active, prompt "Select Body to split"), Splitting Tool(s) (chip), Extend Splitting Tool(s) (checked); OK greyed until both are picked; the picked body turns blue.
- FissionOxide: A "Bodies to split" chip reading "all bodies", a Splitting tool dropdown preset to "XY plane", OK enabled at once, no prompt, no Extend row; clicking a body selects one face and the chip still reads "all bodies".
- Start here: `crates/ui/src/dialogs.rs:3078` (rows), `crates/ui/src/dialogs.rs:825` (no all-bodies default, no preset plane) and `crates/ui/src/dialogs.rs:1276`.

## Silhouette Split missing (P2)
- key: `ui:dialogs:silhouette-split`
- Fusion: SILHOUETTE SPLIT: View Direction (chip, prompt "Select plane or axis for the pull direction"), Target Body (chip), Operation ("Split Shelled Body" dropdown); OK greyed.
- FissionOxide: The menu entry shows "Silhouette Split isn't in FissionOxide yet".
- Start here: `crates/ui/src/menus.rs:174` (`x("Silhouette Split")`): a form plus an engine command that splits a body along its silhouette seen from a direction (`crates/engine/src/commands.rs`).
