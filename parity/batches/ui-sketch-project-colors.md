# Parity batch: ui-sketch-project-colors

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 7) in a new XY sketch with one body in the document, Project (P)
run in each app; FissionOxide built from `main` at `42dec181`. Fusion's projected curves are
purple (189,132,217) with dark-purple filled points at their vertices; a fully constrained ordinary
curve is black in both apps (FissionOxide `SKETCH_FIXED` (20,20,24), verified again this session).

## projected geometry color (P1)
- key: `ui:sketch:projected-geometry-color`
- Fusion: A projected body silhouette or edge draws purple (189,132,217) with dark-purple filled points at the vertices, inside the sketch and after Finish Sketch alike; Hide Projected Geometries in the sketch row menu and the palette's Projected Geometries box hide it.
- FissionOxide: The projected rectangle draws black (20,20,24) 3 px, the same as fully constrained geometry, with no vertex points; it cannot be told from a dimensioned rectangle.
- Start here: `crates/engine/src/scene.rs:47` (`SKETCH_FIXED` is used for projected curves; add a `SKETCH_PROJECTED` purple and a point colour), the sketch curve colour choice in `crates/engine/src/scene.rs` (where a curve's fixed or projected flag picks the colour), `crates/ui/src/viewport.rs` (sketch point marks).

## project filter order (P2)
- key: `ui:sketch:project-filter-order`
- Fusion: PROJECT's Selection Filter row shows two icons, Specified Entities (lit by default) then Bodies; the geometry chip starts "Select".
- FissionOxide: The two icons are Bodies (lit by default, tooltip "Bodies") then "Faces and edges"; the cursor prompt starts "Select a body".
- Start here: `crates/ui/src/dialogs.rs:3848` (the `choice("bodies", ...), choice("faces", ...)` pair: swap the order, default to the specified-entities filter, name it "Specified Entities").

## project body pick chip (P2)
- key: `ui:sketch:project-body-pick-chip`
- Fusion: With the Bodies filter, hovering the body lights the whole body blue and the status reads "Body1"; the pick fills the chip with the body.
- FissionOxide: With the Bodies filter the hover lights one face, the chip reads "1 face" and the status "1 Face | Area : 473.77 mm^2"; the projection still takes the body's silhouette.
- Start here: `crates/ui/src/dialogs.rs:3848` (the project dialog's pick filter) and the viewport pick that feeds it (`crates/ui/src/viewport.rs`, body-priority picking as the Extrude body pick does).
