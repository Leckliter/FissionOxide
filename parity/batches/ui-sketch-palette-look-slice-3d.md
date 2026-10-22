# Parity batch: ui-sketch-palette-look-slice-3d

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 6) in a fresh XY sketch in each app, FissionOxide `main` at
`d90d1e6` (binary rebuilt 19:17). Fusion's Sketch Palette was undocked and floating; FissionOxide's
is docked at the right of the canvas.

Also in this batch, reopened rather than new: `ui:sketch:look-at-sketch-up` (ledger row already
exists). On the XY plane Fusion's Create Sketch and Look At show the ViewCube face FRONT with +X to
the right and +Y up; FissionOxide shows BACK with +X to the left (grid labels "50 ... 0 ... -50"
read left to right) and the sketch's Y labels upside down ("-50" above the axis, "50" below), from
the same home camera in both apps. Start at `crates/ui/src/lib.rs:1461` (`look_at_plane`,
`looking_along`).

## look at tooltip and press feedback (P2)
- key: `ui:sketch:look-at-tooltip`
- Fusion: The palette's Look At button shows the plain dark tooltip "Look At" after 1 s (no expansion at 3 s); clicking it animates the camera to the sketch plane and the button draws the same afterwards (a push button, no checked state).
- FissionOxide: The tooltip reads "Look at the sketch plane"; the click turns the camera to the plane (keeping the BACK side, see the reopened key above).
- Start here: `crates/ui/src/panels.rs:2607` (the Look At row and its tooltip string), `crates/ui/src/lib.rs:1472` (`look_at_selection`).

## slice section look (P2)
- key: `ui:sketch:slice-section-look`
- Fusion: Slice on sections every body at the sketch plane: the cut face is drawn as a magenta/pink diagonal hatch with a black outline, the part of the body in front of the plane disappears, and Slice off restores it at once.
- FissionOxide: The Slice checkbox is greyed ("Not in FissionOxide yet") and nothing sections the model (the greyed row itself is `ui:sketch:palette-disabled-rows`).
- Start here: `crates/ui/src/panels.rs:2619` (the greyed `("Slice", false)` row), `crates/engine/src/scene.rs:376` (body drawing while a sketch is edited; a clip plane plus a hatched cap).

## 3D sketch manipulator (P2)
- key: `ui:sketch:3d-sketch-manipulator`
- Fusion: 3D Sketch on: the Line tool shows a 3D manipulator at its start point (a blue square plane handle plus red X / green Y / blue Z axis arrows) and the sketch grid hides while the tool runs; the heads-up boxes and "Specify next point" still show. The checkbox is greyed while a tool with no 3D form runs (Ellipse, Conic Curve, Point, Text, Spline) and enabled during dialog tools (Mirror, patterns, Project, Sketch Dimension, Fillet, Offset, Trim, Extend, Break, Sketch Scale, Move/Copy).
- FissionOxide: The 3D Sketch checkbox is always greyed; no manipulator exists.
- Start here: `crates/ui/src/panels.rs:2632` (greyed "3D Sketch" row), `crates/ui/src/viewport.rs:3335` (where the Line tool takes its next point).

## feature options for Line, Blend Curve and Spline (P2)
- key: `ui:sketch:feature-options-line-blend`
- Fusion: Line and Midpoint Line show a Feature Options row "Line" with two icon toggles, Line (L) and Midpoint Line (ribbon-style tooltips: title, description, prompt), the running one lit; Blend Curve shows a "Blend Curve" row with two icons; Fit Point Spline shows no Feature Options at all. Rectangle (3), Circle (5), Arc (3), Polygon (3) and Slot (5) rows match FissionOxide's.
- FissionOxide: Line and Midpoint Line show no Feature Options; Blend Curve shows none; Spline shows a two-icon row (fit point / control point) that Fusion does not have.
- Start here: `crates/ui/src/panels.rs:2697` (`feature_options`: add `[Line, MidpointLine]` and the blend curve pair, drop `SPLINES`), `crates/ui/src/panels.rs:2574` (the section).
