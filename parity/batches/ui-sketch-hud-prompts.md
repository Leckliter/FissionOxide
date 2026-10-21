# Parity batch: ui-sketch-hud-prompts

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 with both apps sketching on a vertical origin plane, the same clicks in
each. Fusion's sketch tools all share one input model: a crosshair cursor, a prompt tooltip just
below-right of the cursor, and heads-up value boxes with units and dimension-style extension
lines that follow the cursor. FissionOxide's tools share a different one (a hollow blue circle
cursor with an "x, y" readout and a sentence in the lower-left status text), so most of these
items are one change in `crates/ui/src/viewport.rs` (`hud`, `hud_fields`, `HeadsUp`) applied to
every tool.

## create tool cursor prompts (P1)
- key: `ui:sketch:create-tool-cursor-prompts`
- Fusion: Every create tool shows a crosshair cursor and a small prompt box under it that changes per step: Midpoint Line "Select midpoint of line" / "Select end of line"; 3-Point Rectangle "Place first corner" / "Place second corner" / "Place third corner"; Center Rectangle "Place center point" / "Specify size of rectangle"; 3-Point Circle "Place first point on circle" ... "Place third point on circle"; Center Point Arc "Place center point" / "Specify start point" / "Specify end point"; Tangent Arc "Specify start point" / "Specify end point"; 2-Tangent Circle "Select first tangent of circle" / "Select second tangent of circle"; Inscribed Polygon "Place center point" / "Place point on polygon"; Edge Polygon "Place first point" / "Place second point" / "Specify the side of the polygon"; Ellipse "Place center point" / "Place first axis point" / "Place point on ellipse"; Center to Center Slot "Select start center point" / "Select end center point" / "Select point on slot"; Overall Slot "Select start point" / "Select end point" / "Select point on slot"; Center Point Slot "Select center" / "Select second point" / "Select point on slot"; Three Point Arc Slot "Select start center point" / "Select end center point" / "Select point on arc" / "Select point on slot"; Center Point Arc Slot "Select center point of arc" / "Select start point of arc" / "Select end point of arc" / "Select point on slot"; splines "Specify first point" / "Specify next point"; Conic "Place first point on conic curve" / "Place second point on conic curve" / "Place vertex point of conic curve"; Point "Draw point"; Text "Place first corner of text frame"; Sketch Dimension "Select sketch objects to dimension" / "Select additional sketch geometry or location to place dimension".
- FissionOxide: The cursor is a hollow blue circle with a coordinate readout ("15.05, -28.45", sometimes four decimals "29.9552, 18.025"); the only prompt is the lower-left status sentence ("Midpoint line: the midpoint, then one end", "Slot: first centre, second centre, then the width"), and a failed pick shows "Click a curve" in red there.
- Start here: `crates/ui/src/viewport.rs:2532-2571` (`hud` and the per-tool status sentences), `:1442` ("Click a curve"). Draw the prompt as a tooltip box at the cursor (the strings above, per step `n`), use a crosshair, and drop the coordinate readout (Fusion has none). Related: `ui:canvas:cursor-prompts` in batch ui-cursor-prompts covers the 3D commands.

## create tool heads-up boxes (P1)
- key: `ui:sketch:create-tool-heads-up-boxes`
- Fusion: Every tool shows heads-up boxes with units, the first focused (highlighted text), with dimension extension lines: Midpoint Line length "0.872 in" + angle "18.4 deg"; 3-Point Rectangle first side "0.383 in", then width "0.223 in"; Center Rectangle width "0.546 in" + height "0.411 in"; 3-Point Circle diameter "0.298 in" drawn as a diameter line through the circle; Center Point Arc radius "0.172 in" then angle "90.0 deg" with an angle-arc glyph; Tangent Arc radius "0.107 in"; Inscribed Polygon radius "0.169 in"; Edge Polygon edge length "0.206 in"; Ellipse first axis "0.411 in" then "0.203 in"; every slot: length (centre distance "0.371 in", overall "0.414 in", centre slot "0.239 in") then width "0.135 in"; Center Point Arc Slot radius "0.138 in", angle "90.0 deg", width "0.090 in"; Conic rho "0.75" with a blue arrow manipulator (Enter commits).
- FissionOxide: Only Center Rectangle shows boxes ("22.2104" and "30.8": no units, inconsistent decimals). Midpoint Line, both other rectangles, circles, arcs, polygons, ellipse, all five slots, splines and conic show nothing but the coordinate readout.
- Start here: `crates/ui/src/viewport.rs:141-200` (`HeadsUp`, `hud_fields`), `:2284` (box drawing), `:1048-1061` (`hud_text`). Add the fields per tool and step listed above; values carry the document unit ("in"/"mm", 3 decimals; angles one decimal + " deg"). Related rows: `ui:sketch:heads-up-units-and-lock`, `ui:sketch:line-hud-angle-precision`.

## spline accept button (P1)
- key: `ui:sketch:spline-accept-check`
- Fusion: While a fit-point or control-point spline is being drawn the curve previews smooth (fit spline with a tangent comb; control spline with its dashed control polygon) and a round check-mark button floats under the curve; clicking it or Enter commits, and Escape discards the unfinished spline.
- FissionOxide: The preview is a polyline through the clicked points (no curve until commit), there is no check-mark button, Escape commits a fit-point spline (`lib.rs:902` calls `finish_spline`) but discards a control-point spline; the status text says "click the last point again or Enter to finish".
- Start here: `crates/ui/src/viewport.rs:2156` (preview of `Tool::Spline | Tool::ControlSpline`), `:1175-1180` (`finish_spline`), `crates/ui/src/lib.rs:894-932` (Escape/Enter). Preview the real curve, draw the check-mark button, make Escape discard both spline kinds.

## conic rho box (P2)
- key: `ui:sketch:conic-rho-box`
- Fusion: After the vertex click the conic stays editable: a heads-up box "0.75" (rho) with a blue arrow manipulator on the curve; dragging or typing changes the curve and Enter commits.
- FissionOxide: The vertex click commits the conic at once with a fixed rho and the tool restarts (the next click began a new conic).
- Start here: `crates/ui/src/viewport.rs:1003` (`Tool::Conic if n == 3`), `:2156` (preview). Add a fourth step with the rho box and arrow; commit on Enter or a click on the arrow.

## dimension placement flow (P1)
- key: `ui:sketch:dimension-placement-flow`
- Fusion: Sketch Dimension (D): clicking a line shows a dimension preview ("0.479") that follows the cursor; the next click places it and opens a value box with the full-precision value and unit ("0.4787837888 in", text selected) plus a kebab menu; Enter or Escape both keep the dimension (Escape keeps the current value). An arc gives "R0.228" with a leader; a circle a diameter sign. The dimension text then reads at document precision.
- FissionOxide: Clicking the line opens an edit box "26.9" at the click point immediately (no placement step, no leader following the cursor); the second click does nothing; Escape discards the dimension entirely. A radius reads "12.8556" with no "R" prefix and four decimals.
- Start here: `crates/ui/src/viewport.rs:877` (dimension tool), `:1761` (edit box on click), `:1627` and `:2432` (Enter/Escape handling). Related: `ui:sketch:dim-edit-param-name` (ui-sketch-tools-2) for the box contents and `ui:sketch:diameter-glyph`.

## tangent arc snap marker (P2)
- key: `ui:sketch:tangent-arc-snap-marker`
- Fusion: Hovering a curve end with Tangent Arc (and any tool hovering an existing point) shows a small square snap marker on the point and the prompt "Specify start point"; the junction gets a tangent glyph after the click.
- FissionOxide: No marker on the hovered end point; the pick still snaps.
- Start here: `crates/ui/src/viewport.rs:2179` (`Tool::TangentArc` preview) and the inference drawing near `:2111`. Draw the square marker when the cursor is within the pick radius of a sketch point.

## sketch point and center glyph (P2)
- key: `ui:sketch:point-glyph-style`
- Fusion: A sketch point, a circle/arc centre and a slot centre draw as a small open circle (dark outline, white fill); a point that carries a coincident/midpoint constraint draws filled black. New geometry draws blue until deselected.
- FissionOxide: Points and centres draw as a tiny filled blue diamond; the Point tool result is the same mark, and 3-point circle/arc centres are the same mark.
- Start here: `crates/ui/src/viewport.rs` point drawing (search `sketch.point` and the circle-centre marks near the curve painter, around `:2084` for glyph icons). Use Fusion's open-circle style.
