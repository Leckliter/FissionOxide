# Parity batch: ui-wire-engine-menus

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## modify move copy (P1)
- key: `ui:sketch:modify-move-copy`
- Fusion: SKETCH > MODIFY > Move/Copy (M in a sketch) moves or copies selected sketch curves and points with the triad manipulator: Move Type Free/Translate/Rotate/Point to Point, Create Copy, values in the dialog.
- FissionOxide: Sketch MODIFY's Move/Copy is an inert menu item (menus.rs:617, x()). The solid Move dialog already supports sketch objects (ui:dialogs:move-copy-fields verified), so wire it up for sketch curves while sketching.
- Start here: `crates/ui/src/menus.rs`

## modify sketch scale (P1)
- key: `ui:sketch:modify-sketch-scale`
- Fusion: SKETCH > MODIFY > Sketch Scale: select sketch entities, a base point and a scale factor; the curves scale about the point.
- FissionOxide: Inert menu item (menus.rs:616). The engine has the command (the ledger's toolbar:SketchScaleCmd matches through the CLI), so the toolbar item only needs wiring and a dialog.
- Start here: `crates/ui/src/menus.rs`

## project intersect (P1)
- key: `ui:sketch:project-intersect`
- Fusion: SKETCH > CREATE > Project/Include > Intersect: pick bodies, faces or curves; the curves where they cross the sketch plane appear as purple linked geometry.
- FissionOxide: Inert menu item (menus.rs:572). The engine has sketch.intersection_curve (ledger toolbar:IntersectCmd = match), so only the toolbar item and dialog are missing.
- Start here: `crates/ui/src/menus.rs`

## project to surface (P1)
- key: `ui:sketch:project-to-surface`
- Fusion: SKETCH > CREATE > Project/Include > Project To Surface: pick faces and curves, Projection type Closest Point / Along Vector; the result curves lie on the faces.
- FissionOxide: Inert menu item (menus.rs:574). The engine has sketch.project_to_surface (ledger toolbar:ProjectToSurface = verified), so only the toolbar item and dialog are missing.
- Start here: `crates/ui/src/menus.rs`

## align (P1)
- key: `ui:dialogs:align`
- Fusion: MODIFY > Align: pick a geometry to move and a geometry to align to; options Flip, Angle, Capture Position; the moving object snaps to align with a preview.
- FissionOxide: Inert menu item on SOLID, SURFACE and SHEET METAL MODIFY (menus.rs:174, 373, 440). The engine has an align command (ledger toolbar:AlignCmd = match).
- Start here: `crates/ui/src/menus.rs`

## panel order (P2)
- key: `ui:toolbar:panel-order`
- Fusion: Design tabs show panels in the order CREATE, MODIFY, ASSEMBLE, CONFIGURE, CONSTRUCT, INSPECT, INSERT, SELECT. ASSEMBLE sits right after MODIFY on screen, although the API lists it after INSERT. EXPORT is a panel only on the MESH tab.
- FissionOxide: Fox's shared_groups! (menus.rs:484) orders CONFIGURE, CONSTRUCT, INSPECT, INSERT, ASSEMBLE, SELECT, EXPORT, so ASSEMBLE is near the end and every Design tab has an EXPORT panel (3D Print). Fusion: ASSEMBLE comes right after the tab's MODIFY panel, there's no EXPORT panel on SOLID/SURFACE/SHEET METAL/PLASTIC, and 3D Print is under UTILITIES > MAKE (and MESH > EXPORT).
- Start here: `crates/ui/src/menus.rs`

## polygon constraint (P2)
- key: `ui:sketch:polygon-constraint`
- Fusion: SKETCH > CONSTRAINTS > Polygon: pick the edges of a closed polyline to make it a regular polygon (equal sides, inscribed construction circle).
- FissionOxide: Inert menu item (menus.rs:638). The engine supports it (ledger toolbar:SketchPolygonConstraintCmd = verified), so only the toolbar item is missing.
- Start here: `crates/ui/src/menus.rs`

## create base feature (P2)
- key: `ui:dialogs:create-base-feature`
- Fusion: SOLID > CREATE > Create Base Feature enters a base feature edit (no history) with Finish Base Feature on the toolbar.
- FissionOxide: SOLID CREATE lists Create Base Feature as inert (menus.rs:152), while the configure/direct-edit menu has a working one (menus.rs:197, config_ui::BASE_START). Wire the CREATE entry to the same thing.
- Start here: `crates/ui/src/menus.rs`

## ucs (P2)
- key: `ui:construct:ucs`
- Fusion: CONSTRUCT > User Coordinate System (UCS): pick an origin and axis directions to place a named coordinate system that shows in the browser.
- FissionOxide: Inert menu item (menus.rs:209). The engine has it (ledger toolbar:CoordinateSystemCommand = match).
- Start here: `crates/ui/src/menus.rs`

## duplicate with joints (P2)
- key: `ui:insert:duplicate-with-joints`
- Fusion: INSERT > Duplicate With Joints: pick a component and target geometry (holes, points); copies are placed and jointed at each target.
- FissionOxide: Inert and disabled (.off()) in INSERT (menus.rs:267, 650). The engine has it (ledger toolbar:FusionDuplicateWithJointsCommand = match).
- Start here: `crates/ui/src/menus.rs`

## filter construction (P2)
- key: `ui:canvas:filter-construction`
- Fusion: SELECT > Selection Filters > Construction toggles whether planes, axes and points are selectable.
- FissionOxide: The Construction filter item is inert (menus.rs:316) while the other filters work through Cmd::Filter.
- Start here: `crates/ui/src/menus.rs`, `crates/ui/src/select.rs`
