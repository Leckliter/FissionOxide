# Parity batch: ui-sketch-text-mirror-patterns

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 4); FissionOxide `main` at `ef41107`.

## text frame flow (P1)
- key: `ui:sketch:text-frame-flow`
- Fusion: Text opens a TEXT dialog with only a Type row and OK disabled, prompt "Place first corner of text frame". A click places the first corner; a dashed rubber-band frame then follows the pointer with "Place second corner of text frame"; a second click places it (a drag's release does not). The dialog then grows, the Text box gets keyboard focus, the frame shows "Sample Text" in blue with a rotate handle at its corner, the prompt reads "Adjust text formatting or rotate text frame", and typing replaces the sample text live.
- FissionOxide: The first stage matches (Type row, OK disabled, same prompt) but two clicks do nothing: only a drag places the frame, with no rubber band and no second-corner prompt. After the drag the Text box is not focused (typed text is lost), the placeholder is "FissionOxide" drawn black, and there is no rotate handle or prompt.
- Start here: `crates/ui/src/forms.rs:552` (the frame prompt), `crates/ui/src/dialogs.rs:3691` (frame placement), `crates/ui/src/forms.rs:1242` (default text; use "Sample Text" and focus the box).

## text dialog rows (P2)
- key: `ui:sketch:text-dialog-rows`
- Fusion: After the frame: Type (two icons), Text (multi-line box with an fx button), Font ("Arial" dropdown), Typeface (B and I icon toggles), Height "0.394 in", Character Spacing "0.00", Flip (two icon toggles), Alignment (a row of three horizontal and three vertical icon toggles); info, OK, Cancel.
- FissionOxide: Type, Text (single line), Font "Sans (Ubuntu Light)", Height "5" (no unit), Bold / Italic / Underline checkboxes, Horizontal Alignment and Vertical Alignment text dropdowns, Character Spacing (%), Angle (deg); no Flip row.
- Start here: `crates/ui/src/forms.rs:1232` (`SK_TEXT` inputs and labels).

## mirror line picker (P1)
- key: `ui:sketch:mirror-line-picker`
- Fusion: MIRROR: Objects [Select], Mirror Line [Select], OK disabled, prompt "Select sketch objects to mirror"; clicking the Mirror Line row and then a line fills it, the mirrored copy previews as a thin grey outline, and OK draws it blue with symmetry glyphs.
- FissionOxide: After the objects pick the prompt changes to "Select the mirror line" but the Objects picker stays active: the next line click lands in Objects ("2 selected") and OK stays disabled, so the command cannot be finished without clicking the Mirror Line row; the preview is solid blue; the first prompt reads "Select sketch curves to mirror".
- Start here: `crates/ui/src/dialogs.rs:5262` (the test that documents the prompt change), the `Kind::SkMirror` picker focus in `crates/ui/src/dialogs.rs:376`; move the active picker to Mirror Line when Objects has one pick and the prompt advances.

## circular pattern dialog (P2)
- key: `ui:sketch:circular-pattern-rows`
- Fusion: After the centre pick: Distribution (icon dropdown, "Full"), Suppression (checkbox), Quantity "3" (spinner); the instances preview as grey outlines; a floating quantity box "3" with a spinner and a rotate icon sits by the canvas top right; status "Sketch Point | X: 0.00 Y: 0.00 Z: 0.00 in".
- FissionOxide: Quantity "6", Total Angle "360 deg", Symmetric (checkbox), Suppress (disabled checkbox); the instances preview solid blue with centre points; no floating box; prompt "Select sketch curves to pattern".
- Start here: `crates/ui/src/dialogs.rs:922` (`Kind::SkCircPattern` rows and defaults), the preview colour where pattern instances are added to the scene.

## rectangular pattern dialog (P2)
- key: `ui:sketch:rectangular-pattern-rows`
- Fusion: After the objects pick: Distribution (icon dropdown, "Extent"), Suppression, Quantity "3", Distance "0.00 in", Direction (icon dropdown, "One Direction"), then a second Quantity / Distance / Direction set; OK disabled until a direction is picked; floating boxes "0.00 in" (selected) and "3" (spinner) at the canvas's lower right; the dialog keeps its width.
- FissionOxide: The dialog jumps to about 500 px wide and shows Distance Type ("Spacing"), Quantity "3", Distance "20 mm", Symmetric, Suppress, and an inline "Quantity 3  Distance 20 mm  Symmetric" row for the second direction; OK is enabled at once and a 3 x 3 blue preview appears with no direction picked.
- Start here: `crates/ui/src/dialogs.rs:161` (`Kind::SkRectPattern`) and its row builder near `crates/ui/src/dialogs.rs:922`; keep the dialog width fixed (see `ui:dialogs:*` docking rows).
