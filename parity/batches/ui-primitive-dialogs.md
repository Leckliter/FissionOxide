# Parity batch: ui-primitive-dialogs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-22 (prober 8): Box, Cylinder, Sphere, Torus, Coil and Pipe from SOLID > CREATE in
each app; Fusion in inches, FissionOxide in mm, built from `main` at `0c453dd1`. The CREATE list
itself matches Fusion entry for entry. Box, Cylinder, Sphere and Torus share
`crates/ui/src/dialogs.rs:2478` (rows) and `:1567` (parameters); Coil and Pipe are forms in
`crates/ui/src/forms.rs:746` and `:775`.

## primitives start with a plane pick and a drawing step (P1)
- key: `ui:dialogs:primitive-plane-pick-flow`
- Fusion: Box, Cylinder, Sphere, Torus and Coil open no dialog at first: the cursor asks "Select a plane or planar face" (planes previewed as for Create Sketch), then the base is drawn in a temporary sketch (a Sketch row appears in the browser and timeline): Box "Place first corner" / "Specify size of rectangle"; Cylinder and Torus "Place center point" then a circle with a diameter box and "Specify diameter of circle"; Sphere "Draw point". Only then does the dialog open; after OK the temporary sketch is gone from the browser and timeline.
- FissionOxide: The dialog opens at once on the XY plane with a 20 mm default shape at the origin (Position X, Y fields); there is no plane pick and no drawing step.
- Start here: `crates/ui/src/dialogs.rs:768` (`Kind::Cylinder` and the other primitive starts): begin with the plane chooser, then a rectangle / circle / point step reusing the sketch tools' heads-up boxes, then the dialog.

## Box, Cylinder, Sphere, Torus rows (P1)
- key: `ui:dialogs:primitive-rows`
- Fusion: BOX: Placement (chip with the picked plane), Length, Width, Height ("0.10 in" by default, the floating box focused), Operation ("New Body" with icon). CYLINDER: Placement, Diameter, Height, Operation. SPHERE: Point (chip), Diameter ("0.40 in"), Operation. TORUS: Profile (chip), Inner Diameter, Torus Diameter ("0.20 in"), Position ("On Center" dropdown: On Center, Inside, Outside), Operation.
- FissionOxide: BOX: Plane (plain text "XY plane"), Width (X), Depth (Y), Height, Position X, Y, Operation ("New body"); CYLINDER: Radius, Height; SPHERE: Radius; TORUS: Major radius, Minor radius; all with Position X, Y and no Position dropdown for Torus.
- Start here: `crates/ui/src/dialogs.rs:2478` (labels: Length / Width / Height, Diameter, Inner Diameter / Torus Diameter, a placement chip; Torus Position) and `crates/ui/src/dialogs.rs:1567` (convert diameters to the engine's radii).

## primitive manipulators (P2)
- key: `ui:dialogs:primitive-manipulators`
- Fusion: Box shows a blue height arrow on the top and four black-outlined side arrows for length and width, plus a floating value box; Cylinder a height arrow and a diameter arrow; Sphere one arrow; Torus two arrows; Coil arrows for height, diameter and section size and a rotation handle. The preview body is grey with its base edges blue.
- FissionOxide: No manipulators and no floating value box on any primitive.
- Start here: `crates/ui/src/dialogs/manip.rs` (add handles for the primitives like Extrude's distance arrow).

## Coil rows (P2)
- key: `ui:dialogs:coil-rows`
- Fusion: COIL: Profile (chip), Type ("Revolution and Height" dropdown), Rotation (icon toggle, clockwise / counter-clockwise), Diameter, Revolutions (3), Height, Angle ("0.0 deg"), Section ("Circular"), Section Position ("On Center"), Section Size ("0.25 in"), Operation.
- FissionOxide: COIL: Plane (dropdown), Diameter, Pitch, Revolutions (5), Section, Section size, a Left-handed checkbox, Operation (no icon); no Type, Rotation, Height, Angle or Section Position.
- Start here: `crates/ui/src/forms.rs:746` (`COIL`).

## Pipe rows (P1)
- key: `ui:dialogs:pipe-rows`
- Fusion: PIPE opens with Path (Select chip), Chain Selection (checked), Section ("Circular"), Hollow (unchecked), Operation, OK disabled, prompt "Select a path to sweep the profile along"; after a path pick it adds two Distance rows (1.00 and 0.00), Section Size ("0.20 in", floating box focused), and with the preview cutting a body switches Operation to Cut (red preview) and shows an "Objects To Cut" group; arrows at both ends and on the section size.
- FissionOxide: PIPE: Path sketch (dropdown), "Path curves (default all)", Section, Size (10 mm), "Wall (empty = solid)" text field, "Distance (fraction of the path)", Operation; OK is enabled with no path and the dialog shows a red "the sketch has no lines or arcs for the contour".
- Start here: `crates/ui/src/forms.rs:775` (`PIPE`): a path chip with chain selection, Hollow checkbox (wall thickness appears under it), two distance rows, OK disabled until a path is picked.

## edit dialog title (P2)
- key: `ui:dialogs:edit-feature-title`
- Fusion: Double-clicking a primitive's timeline icon opens the same rows under the title "EDIT FEATURE" (Placement chip without text), with the feature's dimension shown in the canvas.
- FissionOxide: The title is "EDIT BOX" ("EDIT <name>" for every feature).
- Start here: `crates/ui/src/dialogs.rs:2119` (`format!("EDIT {base}")` becomes "EDIT FEATURE").
