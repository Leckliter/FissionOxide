# Parity batch: ui-sketch-modify-tooltips

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 4) by hovering every row of the sketch MODIFY dropdown for 1.5 s in
both apps; FissionOxide `main` at `feea62e`.

## sketch modify tooltips (P2)
- key: `ui:sketch:modify-tooltips`
- Fusion: Fillet: "Places an arc of a specified radius at the intersection of two lines or arcs. / Select the vertex or the two lines or arcs. Specify a radius for the fillet. / Press Ctrl+/ for more help." Blend Curve: "Creates a tangent or curvature continuous blend curve between the endpoints of two sketch curves or edges. / Select two sketch curves or edges in the canvas to blend." Offset: "Copies the selected sketch curves a specified distance from the original curves. / Select the curves to offset then specify the offset distance. / Press Ctrl+/ for more help." Trim: "Trims a sketch curve to the nearest intersecting curve or boundary geometry. / Pause the cursor over the curve to preview the portion to be trimmed. Select the curve to trim. / Press Ctrl+/ for more help." Extend: "Extends a curve to the nearest intersecting curve or boundary geometry. / Pause the cursor over the curve to preview the portion to be extended. Select the curve to extend it. / Press Ctrl+/ for more help." Break: "Breaks a sketch entity into two or more segments where it intersects other sketch entities. / Pause the cursor over the sketch entity to preview where it will be broken, then click to break it." Sketch Scale: "Scales sketch geometry. / Select sketch geometry to scale, select the point from which to scale, then drag the manipulator handle in the canvas or specify an exact Scale Factor." Move/Copy: "Moves the selected face, body, sketch, or construction geometry a specified distance or angle. / Select the objects to modify then specify the distance..." Change Parameters: "Displays the Parameters dialog, where you can create and manage equations and relationships to control the size of objects..."
- FissionOxide: Fillet "Rounds the corner where two sketch curves meet."; Blend Curve "Joins the ends of two curves with a smooth spline."; Offset "Copies a chain of sketch curves at a constant distance. Shortcut: O"; Trim "Removes the piece of a sketch curve you click, up to where it crosses others. Shortcut: T"; Extend "Lengthens a sketch curve until it meets the next curve"; Break "Splits a sketch curve where it crosses other curves, keeping every piece."; Sketch Scale "Scales sketch geometry about a point."; Move/Copy "Moves, rotates or copies sketch geometry."; Change Parameters "Edits the named parameters that drive the design."
- Start here: `crates/ui/src/tips.rs:240-275` (sketch MODIFY), `crates/ui/src/tips.rs:1063` (Change Parameters), `crates/ui/src/panels.rs:535-556`.

## menu row options glyph (P2)
- key: `ui:menus:row-options-glyph`
- Fusion: A hovered dropdown row shows a vertical ellipsis (three stacked dots) at its right edge, the row's options menu.
- FissionOxide: The hovered row shows a hollow square there: the vertical ellipsis U+22EE the painter draws is not in the UI font and renders as a missing-glyph box.
- Start here: `crates/ui/src/menus.rs:1518` (paint three dots or add a font that has U+22EE; check the same glyph in the value boxes of `crates/ui/src/value_box.rs`).

## menu tooltip style (P2)
- key: `ui:menus:tooltip-style`
- Fusion: A dropdown row's tooltip is a dark grey rounded panel (white text) placed to the right of the open menu, two or three paragraphs with a blank line between them, no title line, and it does not cover the menu.
- FissionOxide: A light rounded box below the row, overlapping the menu's lower rows, with a bold title line, one sentence and a "Shortcut: O" line.
- Start here: `crates/ui/src/menus.rs:1441` (`rich_tip` for menu rows) and the toolbar tooltip painter it shares (`ui:toolbar:rich-tooltip`): place menu tooltips right of the menu in the dark style.
