# Parity batch: ui-sketch-tools-2

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## polygon tool flow (P1)
- key: `ui:sketch:polygon-tool-flow`
- Fusion: Circumscribed Polygon: after the centre click a polygon and its circumscribed circle follow the cursor, two heads-up boxes show the radius ("0.147 in", focused) and the number of sides ("6"), the Sketch Palette shows Feature Options (circumscribed / inscribed / edge), and the cursor prompt reads "Place point on polygon". Typing ".3 Tab 5 Enter" makes a pentagon with a "0.30" radius dimension and a polygon glyph.
- FissionOxide: After the centre click only a rubber line from the centre and a coordinate readout follow the cursor: no polygon preview, no circle, no heads-up boxes. Typing "8 Tab 5 Enter" creates nothing, and Escape leaves no polygon.
- Start here: `crates/ui/src/viewport.rs:950` (`Tool::Polygon if n == 2`), `:2208` (the preview branch draws only a circle), `:2542` (hint), `crates/ui/src/menus.rs:562` (the three polygon items). Add a `Field::Radius` + `Field::Sides` HUD pair, a polygon preview, and make Enter commit the typed values.

## diameter glyph (P2)
- key: `ui:sketch:diameter-glyph`
- Fusion: A circle's dimension reads "⌀0.40" (document precision), drawn as a line through the centre with arrowheads and the text sitting on the line; the radius glyph is "R".
- FissionOxide: The label renders as "□12" because the font has no ⌀ (U+2300) glyph, and the dimension is a 45-degree line inside the circle with the label at its end.
- Start here: `crates/ui/src/viewport.rs:2059` (the "⌀" prefix). Add a font with the glyph (or draw the symbol as a stroked circle-slash), and lay the diameter dimension out horizontally through the centre as Fusion does.

## line hud angle precision (P2)
- key: `ui:sketch:line-hud-angle-precision`
- Fusion: Line heads-up boxes read "0.492 in" and "336.8 deg": units on both, one decimal on the angle.
- FissionOxide: The boxes read "14.76" and "336.7876°": no units and four decimals on the angle.
- Start here: `crates/ui/src/viewport.rs:1053` (`hud_text`). Format lengths with the design's unit and angles to one decimal with " deg".

## dim edit param name (P2)
- key: `ui:sketch:dim-edit-param-name`
- Fusion: Double-clicking a dimension opens a box reading "0.4 in" with a ⋮ menu and a tooltip "d7 : 0.4 in" naming the parameter; Enter commits.
- FissionOxide: The box reads "10" with no units and no parameter name.
- Start here: `crates/ui/src/viewport.rs:1575` (`dim_editor`). Show the value with units and the parameter name (the Parameters dialog already names them `d1`, `d2`, ...).

## offset dimension (P2)
- key: `ui:sketch:offset-dimension`
- Fusion: Offset adds an offset constraint drawn as a dimension ("0.10") between the source and the offset curve; editing it moves the offset.
- FissionOxide: Offset works with a live preview and a typed distance, but the result has no dimension and no link to the source curve.
- Start here: `crates/ui/src/dialogs.rs:1340` (`Kind::SkOffset` → `sketch.offset`) and the command in `crates/engine/src/commands.rs:2386`. Add an offset dimension (a driving distance constraint) when the command commits.

## trim hover color (P2)
- key: `ui:sketch:trim-hover-color`
- Fusion: Trim highlights the piece under the cursor in dark blue/purple with the prompt "Select curve section to trim".
- FissionOxide: The piece draws red.
- Start here: the trim hover colour in `crates/ui/src/viewport.rs` (search for the Trim tool's hover paint) and `crates/ui/src/theme.rs`.

## create menu extras (P2)
- key: `ui:sketch:create-menu-extras`
- Fusion: The sketch CREATE list is Line, Midpoint Line, Rectangle, Circle, Arc, Polygon, Ellipse, Slot, Spline, Conic Curve, Point, Text, Mirror, Circular Pattern, Rectangular Pattern, Project / Include, Sketch Dimension.
- FissionOxide: The same list plus "Trace Image" after Text. Keep it if wanted, but it is not a Fusion command.
- Start here: `crates/ui/src/menus.rs:600` and `:657`. Move Trace Image under INSERT (next to Canvas) or keep it at the end of the list.

## data panel reopens (P2)
- key: `ui:chrome:data-panel-reopens`
- Fusion: The Data Panel stays closed until its button or the Show Data Panel item is used.
- FissionOxide: The panel was hidden at the start of the session and was open again after Finish Sketch on a second sketch, with no click on its button in between. One of the keys pressed meanwhile (R, C, L, T, O, Escape, Ctrl+Z, Tab, Enter) or a command appears to toggle it.
- Start here: `crates/ui/src/lib.rs:306` (`(None, None) => app.data.toggle()`, a shortcut with no key and no modifier that may match an empty chord) and the other `data.toggle` calls in `crates/ui/src/panels.rs:95, 252, 749`.
