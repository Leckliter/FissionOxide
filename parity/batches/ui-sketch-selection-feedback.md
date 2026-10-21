# Parity batch: ui-sketch-selection-feedback

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Colours were sampled from the screen on 2026-10-21 (Fusion default light theme, a sketch on a
vertical plane; FissionOxide `main` at `548bc6c`). Screen pixels, sRGB; match the rule and tune the
constants until the samples agree within a few units.

## sketch curve selected color (P1)
- key: `ui:sketch:curve-selected-color`
- Fusion: Clicking a sketch line in the sketch draws it about 4 px wide in (3,123,253) with an anti-aliased fringe, and the status bar reads "1 Sketch Line | Length : 0.524 in". Escape clears it.
- FissionOxide: Clicking a sketch line changes nothing on screen: the line stays 3 px (30,110,235) and the status text stays "Select / drag points - Del deletes - constraints apply to the selection"; the selection exists (Delete removes the line) but is invisible.
- Start here: `crates/engine/src/scene.rs:390` (sketch curves are built with the selected list), `crates/engine/src/scene.rs:26` (`SKETCH_FREE`), `crates/ui/src/viewport.rs:2415`. Draw selected curves wider in a selected colour and put the pick in the status bar.

## sketch curve hover color (P1)
- key: `ui:sketch:curve-hover-color`
- Fusion: Hovering a sketch line draws it about 4 px wide in (74,130,252): darker and more saturated than the plain 2 px (54,170,222) line. The hover needs the pointer within about 1 px of the stroke.
- FissionOxide: Hovering a sketch line draws it 3 px in (128,190,238), lighter than the plain (30,110,235) line (`HOVER_COLOR`).
- Start here: `crates/engine/src/scene.rs:16` (`HOVER_COLOR`) and the sketch curve width and colour in `crates/engine/src/scene.rs:1289`. Sketch hover should darken, not lighten.

## sketch line end points (P1)
- key: `ui:sketch:line-end-points`
- Fusion: Every line end and rectangle corner draws a filled dark-grey dot (50,50,50) about 7 px across. Hovering it shows a hollow ring about 9 px (dark-grey ring, white inside); clicking it fills the ring (125,158,213) with a dark ring and the status reads "Sketch Point | X: ... Y: ... Z: ... in". The same marks appear outside the sketch on a visible sketch, where they win over body vertices.
- FissionOxide: Nothing is drawn at line ends or rectangle corners, nothing changes on hover, and clicking a corner shows nothing (the Points checkbox in the palette is greyed).
- Start here: `crates/engine/src/scene.rs:645-710` (sketch point marks) and `crates/ui/src/viewport.rs:520` (`Pick::SketchPoint` hover). Related: `ui:sketch:point-glyph-style` (free points and centres).

## crossing box picks the wrong curve (P1)
- key: `ui:sketch:crossing-select-wrong-curve`
- Fusion: A right-to-left box that crosses only the rectangle's right line selects that line (and a box crossing a slot's four curves selects them and shades the enclosed profile selected-blue).
- FissionOxide: After clicking the top line, pressing Escape, and dragging a right-to-left box over only the right line, Delete removed the top line and left the right one: either Escape does not clear the (invisible) sketch selection or the crossing box picks the wrong curve.
- Start here: `crates/ui/src/viewport.rs:335` (box selection) and the Escape handling in sketch mode. Add a test: a box over one curve selects exactly that curve.

## dimension label and hover (P2)
- key: `ui:sketch:dimension-label-and-hover`
- Fusion: A dimension's text is plain dark text (43,43,43) with no background; its line is (64,64,64). Hovering it turns the text blue (70,113,219), the line (69,92,149), and shows a dark tooltip under the text with the parameter: "d9 = 0.4787837888 in".
- FissionOxide: The text (29,29,34) sits on a white rounded label box (253,253,254); the line is (150,150,152). Hovering turns the box light blue (232,242,252) with a (30,136,229) border; no tooltip.
- Start here: `crates/ui/src/viewport.rs:2658` (dimension ink) and the dimension label painter near it. Drop the box, colour the text, add the "name = value unit" tooltip.

## sketch canvas background (P2)
- key: `ui:sketch:canvas-background`
- Fusion: The canvas behind a sketch is white (255,255,255).
- FissionOxide: The canvas behind a sketch is (241,242,244).
- Start here: the clear colour in `crates/engine/src/scene.rs` or the sketch-mode background in `crates/ui/src/viewport.rs`.
