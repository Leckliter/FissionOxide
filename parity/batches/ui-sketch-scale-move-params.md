# Parity batch: ui-sketch-scale-move-params

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 4) inside a sketch; FissionOxide `main` at `feea62e`.

## sketch scale dialog (P1)
- key: `ui:sketch:scale-dialog`
- Fusion: SKETCH SCALE has two pickers, Entities [Select] and Point [Select], OK disabled, and the pointer prompt "Select the sketch geometry to scale and the point from which to scale, then specify the scale factor." After the point pick a Scale Factor row "1.00" appears, a blue arrow manipulator sits at the point and a floating box "1.00" (value selected, ellipsis menu) by the pointer; the status reads "Sketch Point | X: 0.202 Y: -0.516 Z: 0.00 in".
- FissionOxide: SKETCH SCALE: Entities (default all) [Select], Point X "0", Point Y "0" (typed), Scale factor "1"; OK always enabled; no prompt, no manipulator, no floating box.
- Start here: `crates/ui/src/forms.rs:2437` (`SK_SCALE`; make Point a sketch-point picker and the factor appear after it), the drag arrow + value box of `crates/ui/src/viewport.rs:4057`.

## move/copy shortcut in the sketch (P1)
- key: `ui:sketch:move-copy-shortcut`
- Fusion: In a sketch, M opens MOVE/COPY and the MODIFY row shows the "M" hint.
- FissionOxide: Pressing M in the sketch does nothing (a following click just selects the line) and the sketch MODIFY row shows no key hint, although `menus.rs` declares `.k("M")`.
- Start here: `crates/ui/src/menus.rs:629` and `crates/ui/src/menus.rs:1328` (`key_of` for the Sketch scope), `crates/ui/src/lib.rs:1048` (`key_for`), the single-letter dispatch in `crates/ui/src/lib.rs:872` (`shortcuts`).

## move/copy dialog in the sketch (P2)
- key: `ui:sketch:move-copy-dialog`
- Fusion: MOVE/COPY in a sketch: Move Object (an icon), Selection [Select], Move Type (five icons), Set Pivot (icon); OK disabled and the prompt "Select sketch geometry to move" until a pick; then X/Y/Z Distance "0.00 in", X/Y/Z Angle "0.0 deg" and Create Copy appear and a triad (arrow, ring, plane square) sits on the selection.
- FissionOxide: Move Object is a text dropdown "Sketch Objects", the picker is labelled Objects, there is no Set Pivot, X/Y Distance "0" and Angle "0 deg" are shown before any pick, OK is enabled, no prompt.
- Start here: `crates/ui/src/dialogs.rs:129` (`Kind::Move` title) and `crates/ui/src/dialogs.rs:763` (rows); reuse the `ui:dialogs:move-set-pivot` work for the sketch case and gate the value rows on a selection.

## change parameters dialog (P2)
- key: `ui:sketch:change-parameters-dialog`
- Fusion: PARAMETERS is a modal titled in caps with a toolbar row: fx, filter-favourites and star icons, a "Filter all parameters" box, then +, copy, delete, import and export icons and an "Automatic Compute" checkbox; columns Parameter (with a sort arrow), Name, Unit, Expression, Value, Comments; rows Favorites, User Parameters, Model Parameters > (document name) > each sketch; info and OK at the bottom; OK closes it.
- FissionOxide: "Parameters" (mixed case) with a "User Parameter" button and a Filter box only; rows "Favorites (0)" with "Star a parameter to keep it here.", "User Parameters (0)", "Model Parameters (1) > Sketch1 (1)" (no document level, counts in the labels); no OK, no toolbar icons; it stays open while other commands run and Escape does not close it (the x does).
- Start here: `crates/ui/src/params_ui.rs:163` (window), `crates/ui/src/params_ui.rs:212-229` (rows).
