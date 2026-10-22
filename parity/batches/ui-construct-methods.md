# Parity batch: ui-construct-methods

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 12) in an unsaved Part Design document in each app (Fusion in
inches, FissionOxide in mm), FissionOxide built from `main` at `c4d97aa9`. Fusion opens every
CONSTRUCT command in one docked "CONSTRUCTION GEOMETRY" dialog (Type: plane / axis / point icon
toggles, Method dropdown); that dialog itself is `ui:construct:construction-geometry-dialog` in batch
`ui-construction-display` (a fixer was on it when this was written: pull `main` first and build on
whatever it landed). These items are the rows of each Method. In Fusion every pick row is a
"Select" chip (arrow icon) that turns into "1 selected" with an x, picks come from the canvas or the
browser, OK stays disabled until every required pick is made, and the prompt beside the pointer
names what to pick. See `parity/reports/2026-10-22-construct-inspect-insert.md`.

## Perpendicular Plane opens the Plane Along Path dialog (P1)
- key: `ui:construct:perpendicular-plane-wrong-dialog`
- Fusion: CONSTRUCT > Perpendicular Plane opens Method "Perpendicular Plane" with rows Face ("Select"), "Plane E..." (two icon toggles) and Extent ("Distance" dropdown); the pointer reads "Select a face or plane".
- FissionOxide: The entry (its tooltip reads "Creates a plane square to an edge or path") opens the "PLANE ALONG PATH" dialog: Path ("Select") and Distance (0–1) "0.5".
- Start here: `crates/ui/src/menus.rs:218` (maps "Perpendicular Plane" to `forms::PLANE_ALONG_PATH`); add a perpendicular-plane form next to `crates/ui/src/forms.rs:967`.

## Plane method rows (P1)
- key: `ui:construct:plane-method-rows`
- Fusion: At Angle: Edge ("Select", pointer "Select an edge, axis or sketch line"); after a pick: Extent type ("Angle" or "To Object"), Angle "0.0 deg", Offset "0.00 in", Resize (two icon toggles), a rotation handle on the preview plane and a floating "0.0 deg" box. Tangent: Face, Reference (after a face pick the pointer reads "Select a reference object."). Midplane: Plane 1, Plane 2. Through Two Edges: Edge 1, Edge 2. Through Three Points: Point 1, Point 2, Point 3. Along Path: Path, Chaining (checked). OK disabled until the picks are made.
- FissionOxide: Plane at Angle: Line dropdown "Z axis", Reference plane dropdown "XY plane", Angle "45 deg", OK enabled at once and no manipulator. Tangent Plane: Face only. Midplane: First plane and Second plane dropdowns, both "XY plane", OK enabled. Plane Through Three Points: three rows of X/Y/Z number boxes (0 0 0, 10 0 0, 0 10 0). Plane Along Path: Path and Distance (0–1) "0.5", no Chaining.
- Start here: `crates/ui/src/forms.rs:904` (PLANE_ANGLE), `:919` (MIDPLANE), `:930` (TANGENT_PLANE), `:952` (PLANE_THREE_POINTS), `:967` (PLANE_ALONG_PATH).

## Axis method rows (P1)
- key: `ui:construct:axis-method-rows`
- Fusion: Through Cylinder/Cone/Torus: Face. Perpendicular To Face: Face, Point. Through Two Planes: Plane 1, Plane 2. Through Two Points: Point 1, Point 2. Through Edge: Edge. All "Select" chips; OK disabled until picked.
- FissionOxide: Axis Perpendicular To Face has Face only (no Point). Axis Through Two Planes has two dropdowns, both "XY plane", with a red "the planes are parallel" line and OK enabled. Axis Through Two Points has two rows of X/Y/Z number boxes (0 0 0 and 0 0 10). Cylinder and Edge match (one Select row).
- Start here: `crates/ui/src/forms.rs:992` (AXIS_NORMAL), `:1003` (AXIS_TWO_PLANES), `:1025` (AXIS_TWO_POINTS).

## Point method rows (P1)
- key: `ui:construct:point-method-rows`
- Fusion: At Vertex: Point. Through Two Edges: Edge 1, Edge 2. Through Three Planes: Plane 1, Plane 2, Plane 3 (picking XY, XZ, YZ from the browser shows "3 WorkPlanes" in the status bar). At Center Of Circle/Sphere/Torus: "Face/Edge". At Edge And Plane: Edge, Plane. Along Path: Path, Chaining (checked). All "Select" chips; OK disabled until picked.
- FissionOxide: Point At Vertex: Edge ("Select") and End dropdown "Start". Point Through Three Planes: three dropdowns all "XY plane" with a red "the planes don't meet in a point", OK enabled. Point At Center: Face only. Point At Edge And Plane: Edge ("Select") and a Plane dropdown "XY plane". Point Along Path: Path and Distance (0–1) "0.5", no Chaining. Through Two Edges matches.
- Start here: `crates/ui/src/forms.rs:1039` (POINT_VERTEX), `:1064` (POINT_THREE_PLANES), `:1079` (POINT_CENTER), `:1090` (POINT_EDGE_PLANE), `:1101` (POINT_ALONG_PATH).

## Construction axes and points get no browser row (P1)
- key: `ui:browser:construction-axes-points-rows`
- Fusion: After OK, a construction point (or axis) appears in the Construction folder next to the planes ("Plane1", "Point1" with an orange point icon); the timeline gets the matching icon.
- FissionOxide: Point Through Three Planes adds the point to the canvas and a yellow point icon to the timeline, but the browser shows no Construction folder and no row; the folder lists construction planes only.
- Start here: `crates/ui/src/panels.rs:2253` (`planes_section` filters `FeatureKind::ConstructionPlane`; list axes and points too, with their icons, in timeline order).
