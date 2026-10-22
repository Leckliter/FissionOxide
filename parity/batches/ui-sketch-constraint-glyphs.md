# Parity batch: ui-sketch-constraint-glyphs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 5), FissionOxide `main` at `d99603d` (after the wave-7 glyph
restyle in `788352e`, which this prober verified: glyphs are now grey line art). Colours sampled
with `System.Drawing` on the screen while the pointer rested.

## glyph placement per constraint (P1)
- key: `ui:sketch:constraint-glyph-placement`
- Fusion: Horizontal/Vertical: one boxed bar glyph just below (vertical: right of) the line's midpoint. Parallel and Collinear: one glyph under the midpoint of EACH line. Equal: an "=" box centred ON each line. Perpendicular: a single corner glyph inside the angle at the shared vertex. MidPoint: a triangle above the point. Symmetry: a "[ ]" glyph above each of the two elements and one on the axis line. Concentric: a double ring beside the shared centre. Coincident: no glyph at rest; hovering the merged point shows an "L"-corner-with-point glyph below-left of it. Tangent: at the tangency, or at the segment end nearest it.
- FissionOxide: Every constraint draws exactly one glyph at anchor + (10, -10) px (above-right): Parallel and Collinear show one glyph instead of two, Equal one small "=" above one line, Horizontal above the line rather than below, Coincident a permanent small square glyph, Tangent beside the circle centre; MidPoint's triangle matches.
- Start here: `crates/ui/src/viewport.rs:2490-2525` (glyph loop, `at + Vec2::new(10.0 + 14.0 * k, -10.0)`), `crates/ui/src/viewport.rs:2906` (`glyph_anchor`), `crates/ui/src/glyphs.rs`.

## glyph hover (P2)
- key: `ui:sketch:constraint-glyph-hover`
- Fusion: Hovering a glyph fills its box pale blue (sampled (189,215,226)) with dark grey ink (68,69,69), highlights every glyph of the same constraint (both Parallel glyphs), does not highlight the geometry, and shows no tooltip even after 4 s.
- FissionOxide: The hovered glyph turns the accent blue, its curves and points draw in the hover colour, and a tooltip "Horizontal (click to select, Del to remove)" appears after about a second.
- Start here: `crates/ui/src/viewport.rs:2505-2521` (`resp.hovered()`, `hovered_items`, `on_hover_text`).

## selected glyph status text (P2)
- key: `ui:sketch:constraint-selected-status`
- Fusion: Clicking a glyph selects it (blue) and the status bar reads the constraint's name: "Horizontal Constraint", "Parallel Constraint", "Perpendicular Constraint", "Vertical Constraint"; Delete removes it (both glyphs of a Parallel) and leaves the geometry where it is.
- FissionOxide: The status bar reads "1 Constraint"; Delete matches.
- Start here: `crates/ui/src/panels.rs:2889` (`Sel::SketchConstraint` status label), `ConstraintKind::name()`.

## glyph right-click menu (P2)
- key: `ui:sketch:constraint-glyph-context-menu`
- Fusion: Right-clicking a glyph opens the sketch marking menu with "Repeat Horizontal/Vertical" (the last command) at the top, Delete, Press Pull, Undo/Redo, Move/Copy, Hole, a Sketch slot, and the list OK (Return), Cancel (Esc), Create Selection Set, Find in Browser, Find in Window; the status bar reads "Vertical Constraint".
- FissionOxide: Not exercised this session (the pre-selection flow made it moot); the fixer should check `marking.rs` opens the same menu for a hovered glyph once glyphs are hit-tested by the marking menu.
- Start here: `crates/ui/src/marking.rs` (sketch marking menu), `crates/ui/src/viewport.rs:2510` (glyph `Sense::click()` has no secondary click).

## constraint menu tooltips (P2)
- key: `ui:sketch:constraint-menu-tooltips`
- Fusion: Hovering a CONSTRAINTS entry shows, after about a second, a dark grey panel to the right of the menu with only the description ("Constrains the position of two points or a point and a line or curve together." plus a second paragraph) and "Press Ctrl+/ for more help."; no title, no image, and it does not change at 3 s. Fix/UnFix: "Locks the size and location of a point or object."
- FissionOxide: A light tooltip with the title "Coincident" and one line, which at 3 s expands with an illustration and a longer paragraph.
- Start here: `crates/ui/src/menus.rs:97` and the rich tooltip helper used for ribbon buttons (`ui:toolbar:rich-tooltip`); dropdown rows need the plain dark variant.
