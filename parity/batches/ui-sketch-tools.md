# Parity batch: ui-sketch-tools

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## line tangent arc (P1)
- key: `ui:sketch:line-tangent-arc`
- Fusion: With Line, press on the end of the last segment and drag to draw a tangent arc instead of a line.
- FissionOxide: Pressing on the end of the last line segment and dragging draws another straight segment (15.66 at 45.13 degrees). In Fusion that press-drag switches to a tangent arc for that segment, and releasing returns to lines. Tangent Arc exists as its own tool (Tool::TangentArc), so the arc maths is there; the Line tool needs the press-drag gesture.
- Start here: `crates/ui/src/viewport.rs`

## trim hover (P1)
- key: `ui:sketch:trim-hover`
- Fusion: Trim highlights the segment that will be removed in red as the pointer hovers it; drag across curves to trim many.
- FissionOxide: With Trim (T) active, hovering a segment only shows '1 Sketch Curve' in the status bar; nothing shows which piece will go. Fusion draws the piece between the nearest intersections in red before you click, and dragging across curves trims each one crossed. The viewport hint also still reads 'Sketching — draw with the Create tools' instead of a Trim hint.
- Start here: `crates/ui/src/viewport.rs`

## offset preview (P2)
- key: `ui:sketch:offset-preview`
- Fusion: Sketch Offset previews the offset loop live with a distance manipulator and flips side with the cursor.
- FissionOxide: Offset (O) on a selected line opens OFFSET (Curves 1 selected, Offset position 2 mm) and previews a fixed 2 mm copy. The preview doesn't follow the cursor (Fusion: distance and side track the pointer until you click) and there's no distance manipulator. The OFFSET dialog is also drawn over the Sketch Palette instead of stacking above it.
- Start here: `crates/ui/src/viewport.rs`, `crates/ui/src/dialogs.rs`

## sketch text (P2)
- key: `ui:sketch:sketch-text`
- Fusion: Text tool: click a point or two corners, dialog with font, height, bold/italic, alignment, and on-path option.
- FissionOxide: Text form (SK_TEXT) has text, height, angle, font and placement. Fusion's Text dialog adds bold/italic/underline, horizontal and vertical alignment, character spacing, a two-corner box, and Text on Path.
- Start here: `crates/ui/src/forms.rs`

## stale canvas hint (P2)
- key: `ui:chrome:stale-canvas-hint`
- Fusion: The canvas hint at the lower left always describes the active tool, and clears when no tool is active.
- FissionOxide: After finishing the sketch, running Measure, Combine and others, the lower-left hint still reads 'Horizontal/Vertical: select lines or two points' from the last constraint tool. app.status for tool hints isn't cleared when the tool ends.
- Start here: `crates/ui/src/viewport.rs`
