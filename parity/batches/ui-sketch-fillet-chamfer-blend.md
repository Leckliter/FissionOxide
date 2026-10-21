# Parity batch: ui-sketch-fillet-chamfer-blend

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 4) in a Fusion sketch on the XY plane holding a rectangle, a circle
crossing its right edge and two short lines, against the same sketch in FissionOxide `main` at
`feea62e`. Fusion's sketch Fillet, Chamfer and Blend Curve have no dialog at all: they are
pick-pick tools whose only input is a floating heads-up box (and, for Chamfer and Blend Curve, the
Feature Options row in the Sketch Palette).

## sketch fillet flow (P1)
- key: `ui:sketch:fillet-flow`
- Fusion: Fillet opens no dialog. The prompt beside the pointer reads "Select first line or arc"; after the first pick (status "1 Sketch Line | Length : 0.734 in") it reads "Select second line or arc" and hovering the second line shows a faint red preview of the arc. On the second pick the fillet is applied at once with the last radius (0.10 in), a floating value box "0.10 in" appears with its value selected and a vertical-ellipsis menu at its right, a blue arrow manipulator points out of the corner, and the prompt returns to "Select first line or arc" so more corners can be filleted. Typing replaces the value, Enter commits (a radius dimension "R0.20" and two tangent glyphs appear) and the tool ends.
- FissionOxide: Fillet opens a SKETCH FILLET dialog (Lines "Select", Radius "2 mm", OK/Cancel) that pushes the Sketch Palette down; the prompt is "Select two lines or arcs that meet" and after the second pick "Hold Ctrl to modify selection"; the arc is only previewed until Enter or OK, there is no arrow manipulator and no floating box, and nothing invites a third pick.
- Start here: `crates/ui/src/dialogs.rs:155` (`Kind::SkFillet` title), `crates/ui/src/dialogs.rs:902` (default radius), `crates/ui/src/dialogs.rs:2285` (prompt), `crates/ui/src/viewport.rs:4057` (the floating value box and drag arrow used by Extrude and Sketch Offset). Turn Fillet into a dialog-less tool that applies on the second pick, shows the value box and arrow, and keeps prompting for the next first pick.

## sketch chamfer flow (P1)
- key: `ui:sketch:chamfer-flow`
- Fusion: Equal Distance Chamfer opens no dialog; the Sketch Palette gains a "Feature Options" row "Chamfer" with three icon toggles (equal distance, distance and angle, two distance). The prompt reads "Select a vertex or two lines to chamfer", then "Select a second line". On the second pick the chamfer is applied with the last distance, a floating box "0.081 in" (value selected, ellipsis menu) and an arrow manipulator appear; typing ".1" and Enter commits. The result carries two driven distance dimensions "0.10" (one along each line from the old corner) and a warning toast at the canvas's lower right: "1 warning(s) / Constraints and/or dimensions were removed during operation." with a warning icon in the status bar.
- FissionOxide: A SKETCH CHAMFER dialog (Lines, Distance "2 mm", OK/Cancel); prompt "Select two lines" then "Hold Ctrl to modify selection"; no Feature Options; the result is a bare chamfer line with no dimensions and the status readout is left at "1 Sketch Point".
- Start here: `crates/ui/src/dialogs.rs:156` (`Kind::SkChamfer*`), `crates/ui/src/menus.rs:612` (`SK_CHAMFER`), `crates/ui/src/panels.rs:2526` (`sketch_palette`, Feature Options), `crates/engine/src/commands.rs:2393` (`sketch.chamfer` should add the two distance dimensions).

## sketch chamfer submenu (P2)
- key: `ui:sketch:chamfer-submenu`
- Fusion: The Chamfer submenu lists, in this order, "Equal Distance Chamfer", "Distance and Angle Chamfer", "Two Distance Chamfer". Their tooltips: "Creates a chamfer at the vertex, intersection, or apparent intersection of two lines in a sketch, based on an equal distance to each side. / Select a vertex or two lines, then specify the distance value for the chamfer." (then "...based on a single distance and an angle. / ...specify the distance and angle values..." and "...based on a different distance to each side. / ...specify the two distance values..."), each with an illustration.
- FissionOxide: "Equal Distance Chamfer", "Distance-Distance Chamfer", "Distance and Angle Chamfer"; tooltips "Bevels a sketch corner by the same distance along both lines." etc.
- Start here: `crates/ui/src/menus.rs:612` (`SK_CHAMFER` order and labels), `crates/ui/src/dialogs.rs:286` (dialog titles), `crates/ui/src/tips.rs:240`.

## sketch fillet result glyphs (P2)
- key: `ui:sketch:fillet-result-glyphs`
- Fusion: After a fillet the two tangent glyphs sit just outside the shape beside each arc end (on the line, near the arc), the horizontal/vertical glyphs stay at their line midpoints, and the arc's centre point is drawn as a small hollow circle.
- FissionOxide: The tangent glyphs are stacked next to the horizontal and vertical glyphs at the line midpoints, away from the arc, and no arc centre point is drawn.
- Start here: `crates/ui/src/viewport.rs` constraint glyph placement (search `tangent` near the glyph painter used for `ui:sketch:constraint-glyphs`) and `crates/engine/src/scene.rs:645` (sketch point marks; arc centres count).

## blend curve flow (P1)
- key: `ui:sketch:blend-curve-flow`
- Fusion: Blend Curve opens no dialog; the Sketch Palette shows "Feature Options / Blend Curve" with two icon toggles (tangent G1, selected; curvature G2). Prompts: "Select first curve or edge", "Select second curve or edge". The first pick draws the curve bold blue; hovering the second shows a faint red preview of the blend. With a vertical line (top end nearer the other curve) picked near its bottom and a diagonal line picked near its top, Fusion blended the two top ends with a smooth arch that leaves each line in its own direction; the spline is drawn black (fully constrained) with a tangent glyph at each end, and the prompt returns to "Select first curve or edge".
- FissionOxide: Prompts "Select first curve" / "Select second curve"; no Feature Options and no preview; the same two picks joined the diagonal's top end to the vertical line's bottom end, leaving the vertical line downward and looping back, a wild S-curve drawn blue with fit-point ticks and 14 new degrees of freedom. Shift for G2 is undiscoverable.
- Start here: `crates/ui/src/viewport.rs:1824` (`Tool::Blend` run with `near`), `crates/ui/src/viewport.rs:3784` (prompts), `crates/engine/src/commands.rs:329` (`sketch.blend` end choice), `crates/ui/src/panels.rs:2526` (Feature Options G1/G2).
