# Parity batch: ui-engine-gaps

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## project intersect (P1)
- key: `ui:sketch:project-intersect`
- Fusion: SKETCH > CREATE > Project/Include > Intersect: pick bodies, faces or curves; the curves where they cross the sketch plane appear as purple linked geometry.
- FissionOxide: #125: there is no engine command for Fusion's Intersect (the curves where bodies cross the sketch plane). sketch.intersection_curve is Fusion's separate Intersection Curve command, which #125 added as its own Project/Include entry.
- Start here: `crates/ui/src/menus.rs`

## ucs (P2)
- key: `ui:construct:ucs`
- Fusion: CONSTRUCT > User Coordinate System (UCS): pick an origin and axis directions to place a named coordinate system that shows in the browser.
- FissionOxide: #125 skipped it: the engine has no UCS command (Fox scripts write the UCS plane by hand). Needs an engine coordinate-system feature first.
- Start here: `crates/ui/src/menus.rs`

## duplicate with joints (P2)
- key: `ui:insert:duplicate-with-joints`
- Fusion: INSERT > Duplicate With Joints: pick a component and target geometry (holes, points); copies are placed and jointed at each target.
- FissionOxide: #125 skipped it: there is no single engine command. Needs a command or dialog that chains component.copy and joint for each target.
- Start here: `crates/ui/src/menus.rs`
