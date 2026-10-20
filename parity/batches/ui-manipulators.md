# Parity batch: ui-manipulators

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## manipulator coverage (P1)
- key: `ui:dialogs:manipulator-coverage`
- Fusion: Every feature with a length or angle shows canvas manipulators with a heads-up value box, both when creating and when editing the feature: Extrude (distance arrow plus a taper-angle arc handle, and a second arrow for Two Sides), Revolve (angle rotator), Shell (thickness arrow), Hole (depth arrow and a draggable centre point), Draft (angle rotator), Thicken (thickness arrow), Offset Face (distance arrow), Circular Pattern (angle and quantity handles), Plane at Angle (rotator), Scale (scale arrow), Rib/Web (thickness and depth arrows), Emboss (depth arrow), Chamfer (distance arrows), Fillet (one radius arrow per selection row).
- FissionOxide: FissionOxide has arrows only for Extrude (one distance arrow, no taper handle, no second arrow for Two Sides), Offset Plane, Fillet, Chamfer, Press Pull, Sketch Offset and Rectangular Pattern, plus the Move and Joint triads. manip.rs's refresh_manip/more_manipulator return None whenever self.editing is set, so editing an existing feature shows no manipulators. Fusion shows them when editing too. Missing entirely: Revolve angle, Shell thickness, Hole depth and point, Draft, Thicken, Offset Face, Circular Pattern, Plane at Angle, Scale, Rib/Web and Emboss.
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/dialogs/manip.rs`, `crates/ui/src/viewport.rs`
