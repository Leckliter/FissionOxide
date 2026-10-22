# Parity batch: ui-inspect-insert-assemble

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 11) in an unsaved Part Design document in each app (Fusion in
inches, FissionOxide in mm), FissionOxide built from `main` at `ed3546c5`. The CONSTRUCT, CONFIGURE
and SELECT lists match (Selection Tools, Priority and Filters re-verified); Fusion's single
"CONSTRUCTION GEOMETRY" dialog is already `ui:construct:construction-geometry-dialog` (batch
`ui-construction-display`). See `parity/reports/2026-10-22-edit-construct.md`.

## ASSEMBLE in a Part Design document (P1)
- key: `ui:menus:assemble-part-design`
- Fusion: In a document whose browser shows "Part Design", the ASSEMBLE dropdown holds a single entry, "Add To Assembly".
- FissionOxide: The same kind of document ("Part Design" row in the browser) lists Add To Assembly, New Component, Joint (J), As-built Joint (Shift+J), Drive Joints, Motion Study, Duplicate Component, Joint Origin, Rigid Group, Motion Link, Contact Set, Insert Component and Cam Follower.
- Start here: `crates/ui/src/menus.rs:283` (the ASSEMBLE list: only "Add To Assembly" while the design's type is Part Design; the full list in an assembly design).

## Measure dialog rows (P2)
- key: `ui:inspect:measure-dialog-rows`
- Fusion: MEASURE has Selection Filter as three icon toggles, Precision "0.123", Secondary Units "None", Clear Selection (a blue undo-arrow button), Show Snap Points (checkbox) and only a Close button; picking a face adds a collapsible "Selection 1" section with a grey "Face selected" line, Area "0.663 in^2" and Loop Length "4.235 in". The pointer reads "Select objects to measure."
- FissionOxide: Selection Filter is two text toggles ("Body", "Face/Edge/Vertex"), Precision "0.000", Units "mm", then Results "Copy all", Face "Plane" with Copy, Area "380.365 mm²" with Copy; no Clear Selection, no Show Snap Points, no Selection 1 section, no Loop Length. The face stays selected after Close.
- Start here: `crates/ui/src/dialogs.rs:4780` (the measure rows around "Copy all").

## INSPECT has no Properties entry (P2)
- key: `ui:menus:inspect-properties-extra`
- Fusion: INSPECT ends with History and Sustainability Insights.
- FissionOxide: INSPECT adds a separator and "Properties" after Sustainability Insights.
- Start here: `crates/ui/src/menus.rs:261` (the INSPECT list; Properties lives on the body and component right-click menus in Fusion).

## INSERT list (P2)
- key: `ui:menus:insert-list`
- Fusion: INSERT in an unsaved design: Decal, Canvas, Insert SVG, Insert DXF, Insert Mesh, Insert Component (greyed), Insert Fastener (greyed), Duplicate With Joints (greyed), Insert Derive, Insert McMaster-Carr Component (greyed), Insert a manufacturer part (greyed), Insert TraceParts Supplier Components (greyed); no Insert STEP.
- FissionOxide: Same entries but Insert Component is enabled, and a separator and "Insert STEP" are added at the end.
- Start here: `crates/ui/src/menus.rs:279` (the INSERT list; STEP import belongs to File > Open / Upload in Fusion).

## Construct menu icons (P2)
- key: `ui:menus:construct-icons`
- Fusion: Every CONSTRUCT entry has its own icon (Offset Plane two green-orange planes, Plane at Angle, Tangent Plane, Midplane and so on each drawn differently; axes in green with a cylinder, face, planes, points or edge; points in orange with a vertex, edges, planes, circle, edge and plane or path).
- FissionOxide: All plane entries but Offset Plane and Midplane share one yellow plane icon, all axis entries one yellow axis icon, all point entries one point icon.
- Start here: `crates/ui/src/menus.rs` (the CONSTRUCT list) and the icon set in `crates/ui/src/icons.rs`.
