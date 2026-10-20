# Parity batch: ui-manufacture

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## milling toolbar (P1)
- key: `ui:manufacture:milling-toolbar`
- Fusion: MANUFACTURE > MILLING tab panels: SETUP (New Setup*, Create NC Program*, New Folder, New Pattern, Manual NC, Probe WCS, Insert Manufacturing Data, Create Manufacturing Model), 2D (2D Adaptive Roughing*, 2D Offset Roughing (Pocket), Face*, 2D Contour*, Slot, Trace, Thread, Bore, Circular, Engrave, 2D Chamfer), 3D (3D Adaptive Roughing*, 3D Offset Roughing (Pocket), Flat*, Parallel*, Scallop*, 3D Contour*, Ramp, Pencil, Horizontal, Spiral, Radial, Morphed Spiral, Project, Blend, Morph, Flow, Geodesic), DRILLING (Drill*), ACTIONS (Generate, Simulate, Simulate with Machine*, Post Process*, Setup Sheet*, Machining Time), MANAGE (Tool Library*, Machine Library*, Post Library*, Template Library*, Task Manager, Export/Import/Reset Defaults), INSPECT (Measure*, analyses, Section Analysis), SELECT. * = toolbar icon.
- FissionOxide: FissionOxide's MILLING tab has only SETUP (New Setup, Tool Library), 2D (Face, 2D Pocket, 2D Contour, 2D Adaptive Clearing, plus inert Slot/Trace/Thread/Bore/Engrave), 3D (Parallel, Scallop, Adaptive Clearing, Steep and Shallow, Contour, Pencil), DRILLING and ACTIONS (Generate, Simulate, Post Process, Delete Setup, 2D Cut). Names differ from Fusion's current ones: 2D Adaptive Roughing, 2D Offset Roughing (Pocket), 3D Adaptive Roughing, 3D Contour. Missing: Create NC Program, Setup Sheet, Machining Time, the 2D Chamfer/Circular ops, the 3D Flat/Ramp/Horizontal/Spiral/Radial/Morphed Spiral/Project/Blend/Morph/Flow/Geodesic ops, the MANAGE panel (Machine/Post/Template libraries) and the INSPECT and SELECT panels. Slot, Trace, Thread, Bore and Engrave are inert.
- Start here: `crates/ui/src/menus.rs`, `crates/ui/src/forms.rs`, `crates/engine/src`

## workspace tabs (P2)
- key: `ui:manufacture:workspace-tabs`
- Fusion: The Manufacture workspace has tabs MILLING, TURNING (Turning Face, Profile Roughing/Finishing, Groove, Thread, Chamfer, Part, Part Handling), ADDITIVE (setup, print settings), INSPECTION (Probe WCS, manual inspection), FABRICATION (2D Profile cutting) and UTILITIES (in-process stock, add-ins).
- FissionOxide: FissionOxide's Manufacture workspace has a single MILLING tab. 2D cutting lives under ACTIONS as '2D Cut (Laser, Plasma, Waterjet)' instead of a FABRICATION tab with 2D Profile. TURNING, ADDITIVE, INSPECTION and UTILITIES are missing.
- Start here: `crates/ui/src/menus.rs`
