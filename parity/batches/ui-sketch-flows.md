# Parity batch: ui-sketch-flows

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## fillet flow (P1)
- key: `ui:sketch:fillet-flow`
- Fusion: Sketch Fillet: no side dialog. Click a corner (or two curves) and the fillet previews at once, with a heads-up radius box at the arc (default 5 mm here) and a drag arrow. Enter or a click accepts it, and the tool stays active for the next corner. Tooltip: 'Select first line or arc'.
- FissionOxide: FissionOxide opens a docked SKETCH FILLET dialog (Lines: Select, Radius 2 mm, OK/Cancel). Clicking the corner set the status bar to '2 objects', but the Lines row still read 'Select' and nothing previewed: no arc, no heads-up radius box at the corner, no drag arrow. Match Fusion's in-canvas flow and its 5 mm default, and keep the tool active after each fillet.
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/viewport.rs`

## profile shading (P2)
- key: `ui:sketch:profile-shading`
- Fusion: Closed profiles shade light orange.
- FissionOxide: Re-checked 2026-10-20 on Fusion 2705: idle profiles are pale blue, not orange. See ui:sketch:profile-fill-color.

## profile fill color (P2)
- key: `ui:sketch:profile-fill-color`
- Fusion: While a sketch is open (and when it is visible outside the sketch), closed profiles are filled pale blue, a light tint of the blue curve colour. Every closed profile is filled, circles included.
- FissionOxide: FissionOxide fills profiles pale yellow. While editing Sketch1 (a 40x20 rectangle and an r8 circle), the circle wasn't filled at all, though the palette counted 2 profiles. Fusion 2705 fills both pale blue, in and out of sketch edit. The earlier ui:sketch:profile-shading 'match' (light orange) may have been a hovered or selected profile. Check Fusion's colour for hovered versus idle profiles before changing it.
- Start here: `crates/render/src/lib.rs`, `crates/ui/src/viewport.rs`

## mirror pattern picks (P2)
- key: `ui:sketch:mirror-pattern-picks`
- Fusion: Sketch Mirror (Objects, Mirror Line), Circular Pattern (Objects, Center Point, then Type Full/Angle/Symmetric, Quantity, Angle, Suppress) and Rectangular Pattern (Objects, Direction/s, then Distance Type Extent/Spacing, Quantity, Distance, Direction One/Symmetric, a second direction, Suppress) take their mirror line, centre point and directions as canvas selection rows. Patterns preview live, with drag arrows for distance and angle.
- FissionOxide: FissionOxide's sketch Mirror picks its mirror line from a dropdown of 'Sketch line N'. Circular Pattern takes the centre as typed 'Center X, Y' numbers rather than a picked point. Rectangular Pattern's Direction is a Horizontal/Vertical dropdown rather than a picked line (dialogs.rs fields(), Kind::SkMirror/SkCircPattern/SkRectPattern). Make each one a canvas selection row like Fusion's, keep the typed values as fallbacks, and add the pattern drag arrows. Circular Pattern also needs Type (Full/Angle/Symmetric) and Suppress.
- Start here: `crates/ui/src/dialogs.rs`
