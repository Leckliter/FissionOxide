# Parity batch: ui-feature-dialogs-2

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## offset sketch fields (P2)
- key: `ui:dialogs:offset-sketch-fields`
- Fusion: OFFSET: Curves Select, Chain Selection (checkbox, on), Direction (One Side ...), Flip, plus the cursor prompt "Select a curve to offset" and a drag arrow.
- FissionOxide: OFFSET: Curves and Offset position only.
- Start here: `crates/ui/src/dialogs.rs:1340` (`Kind::SkOffset`). The engine command takes `both_sides` and `topology_matched` already.

## chamfer fields 2 (P2)
- key: `ui:dialogs:chamfer-fields-2`
- Fusion: CHAMFER: a selection-set table ("2 Edges | 0.00 in | 0.00 in") with + and x, Type (Equal Distance / Two Distances / Distance and Angle), Edges/Faces/Features Select, Tangent Chain (on), Corner Type (Chamfer / Miter / Blend), a distance arrow with a floating value box, the prompt "Specify chamfer distance" and the status "Chain". Picking one vertical edge selected its tangent chain (2 edges).
- FissionOxide: Edges chip, Chamfer type, Distance, Preview. One edge picked, no table, no Tangent Chain row, no Corner Type, no manipulator.
- Start here: `crates/ui/src/dialogs.rs:728, 1465` (`Kind::Chamfer`). Tangent Chain came with the driver session's #91; Corner Type has no engine support, so show it greyed. Share the floating-box code with ui-feature-manipulators.

## shell type toggles (P2)
- key: `ui:dialogs:shell-type-toggles`
- Fusion: SHELL: Faces/Body, Tangent Chain, Shell Type (two icon toggles), Inside Thickness, Direction; a floating thickness box; the prompt "Select faces to remove or body to shell".
- FissionOxide: Faces/Body, Tangent chain, Direction, Inside thickness, a greyed "Round corners" row Fusion does not show, Preview.
- Start here: `crates/ui/src/dialogs.rs:729, 1466` (`Kind::Shell`).

## press pull offset face (P2)
- key: `ui:dialogs:press-pull-offset-face`
- Fusion: Press Pull (Q) on a planar face switches the dialog to OFFSET FACE: Faces (1 selected), Offset Type (Automatic / Modify Existing Feature / New Offset), Distance, with a floating box, the prompt "Select face(s) to offset" and the status "1 Face | Area: 0.40 in^2". On a profile it becomes EXTRUDE, on an edge FILLET.
- FissionOxide: The dialog stays titled PRESS PULL with Faces, Distance and Preview.
- Start here: `crates/ui/src/dialogs.rs:133, 778` (`Kind::PressPull`). Retitle by selection kind and add the Offset Type row (Automatic and New Offset map to the existing offset-face command).

## move set pivot (P2)
- key: `ui:dialogs:move-set-pivot`
- Fusion: MOVE/COPY: Move Object, Selection, Move Type as five icons (Free Move, Translate, Rotate, Point to Point, Point to Position), Set Pivot, X/Y/Z Distance, X/Y/Z Angle, Create Copy; the triad has rotation rings; prompt "Select bodies to move".
- FissionOxide: Titled MOVE / ROTATE; Move Type is a text dropdown; no Set Pivot row.
- Start here: `crates/ui/src/dialogs.rs:126, 747` (`Kind::Move`).

## hole fields 2 (P2)
- key: `ui:dialogs:hole-fields-2`
- Fusion: HOLE: Placement as two icons, Face, two Reference rows, a collapsible "Shape Settings" group with Extents, Hole Type (3 icons), Hole Tap Type (4 icons), Drill Point (2 icons), editable fields on the diagram (depth, 118.0 deg, diameter), OK/Cancel; the hole previews in red with a manipulator; prompt "Select linear or circular reference edges to fully define hole location". No sketch row appears in the browser.
- FissionOxide: Placement and Drill point as text radios, Hole type / Tap type / Extents as text dropdowns, one References row, Flip direction and Preview checkboxes, no Shape Settings group. Placing a hole adds a "Sketch3" row under Sketches.
- Start here: `crates/ui/src/dialogs.rs:768` (`Kind::Hole`). Use icon rows, group the shape rows, and hide (or do not create) the placement sketch in the browser.
