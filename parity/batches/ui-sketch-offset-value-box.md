# Parity batch: ui-sketch-offset-value-box

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 4): a circle, then a line, selected first and O pressed, in Fusion
(inches) and FissionOxide `main` at `feea62e` (mm).

## offset dialog rows (P2)
- key: `ui:sketch:offset-dialog-rows`
- Fusion: OFFSET: Curves [1 selected x], Chain Selection (checkbox, unchecked when a curve was pre-selected), Direction (icon + text dropdown: One Side, Two Sides, Symmetric, each with an icon), Distance "0.00 in", Flip (an icon button); info, OK, Cancel. The dialog docks to the LEFT of the Sketch Palette, side by side, not above it.
- FissionOxide: Curves, Chain Selection (checked), Direction (text dropdown: One Side, Symmetric), "Offset Position" "2 mm", Flip (a checkbox); the dialog stacks above the palette.
- Start here: `crates/ui/src/dialogs.rs:3225` (`DIRECTIONS`, add Two Sides with icons), `crates/ui/src/dialogs.rs:3228` (label "Distance"), Flip as an icon button like `ui:dialogs:flip-icon-toggles`, `crates/ui/src/panels.rs:2513` (dialog/palette placement: OFFSET sits left of the palette).

## offset default and follow (P1)
- key: `ui:sketch:offset-default-follow`
- Fusion: With a curve pre-selected, the offset starts at 0.00 in and stays on the curve until the arrow is dragged or a value typed: the blue arrow sits on the curve pointing outward, the floating box "0.00 in" (value selected, ellipsis menu) sits by the pointer, and the prompt is "Modify offset distance or side settings". Typing ".1" and Enter makes the offset curve, an offset dimension "0.10" and an offset-constraint glyph pair, and the tool ends.
- FissionOxide: The offset follows the pointer from the start (53.5 mm with the pointer far away, -6 mm inside the circle), the arrow is drawn from the curve to the pointer, and the prompt is "Hold Ctrl to modify selection".
- Start here: `crates/ui/src/dialogs.rs:4787` (`value_box_edited` / `follow`), the `Kind::SkOffset` live preview in `crates/ui/src/dialogs.rs:908`, `crates/ui/src/dialogs.rs:2201` (prompt).

## value box stale selection (P1)
- key: `ui:sketch:value-box-stale-selection`
- Fusion: Typing into the floating value box always replaces the whole value: "3" over "-6 mm" gives "3".
- FissionOxide: After the box's text changed from "53.5 mm" to "-6 mm" while it had focus, typing "3" left "3m" (the select-all range was computed for the earlier, longer text), so Enter applied an offset of 3 metres; the OFFSET dialog's field showed "3m" too.
- Start here: `crates/ui/src/value_box.rs:47-73` (`show`, `focus_text(.., true)`), `crates/ui/src/viewport.rs:2282` (`focus_text` sets a `CCursorRange` from the text length). Re-select the whole text whenever the text changes, and add a test that typing a digit after a value change replaces everything.

## status readout under the palette (P2)
- key: `ui:sketch:status-readout-under-palette`
- Fusion: The selection readout ("1 Sketch Line | Length : 0.734 in") sits at the lower right of the canvas, left of the Sketch Palette, never under it.
- FissionOxide: When a command dialog pushes the Sketch Palette down, the readout "1 Sketch Curve" is painted at the canvas's lower right underneath the palette's Finish Sketch button and is mostly hidden.
- Start here: `crates/ui/src/panels.rs:2887` (readout text) and where it is placed relative to `sketch_palette` (`crates/ui/src/panels.rs:2526`); keep it left of the palette's rect.
