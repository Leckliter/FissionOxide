# Parity batch: ui-construction-display

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-22 (prober 8) after Offset Plane on an origin plane in each app (Fusion 1 in
from XZ, FissionOxide 25 mm from XY), FissionOxide built from `main` at `0c453dd1`. The CONSTRUCT
menu's entries and order already match. Construction planes are drawn by
`crates/engine/src/scene.rs:531`; the browser's Construction folder is in
`crates/ui/src/panels.rs:2371`.

## construction planes draw like origin planes (P1)
- key: `ui:canvas:construction-plane-display`
- Fusion: A construction plane is a translucent peach square (the origin planes' colour) about twice an origin square, centred on the plane's origin, with a faint darker edge; hovered it turns grey, selected blue (127,174,216) with a darker blue edge, status "Plane1". The Offset Plane preview is the same size.
- FissionOxide: It is a dashed orange outline sized from the model's extent (`scene_size * 0.35`), so in an empty design it is invisible, and it is huge next to small bodies; there is no fill, no hover or selected look, and selecting Plane1's row draws nothing and leaves the status empty. The Offset Plane preview fills half the canvas.
- Start here: `crates/engine/src/scene.rs:531` (draw a filled translucent square of a fixed screen size like `crates/ui/src/origin.rs`'s planes, with hover and selected looks), `crates/ui/src/preview.rs` (the preview size), the status readout for a picked construction plane.

## Construction folder and row eyes (P1)
- key: `ui:browser:construction-folder-eyes`
- Fusion: The Construction folder and each construction row carry an eye toggle (greyed when hidden); the folder appears after OK, collapsed.
- FissionOxide: Neither the folder nor the rows have an eye; the folder appears (expanded) as soon as the Offset Plane preview starts, before OK.
- Start here: `crates/ui/src/panels.rs:2371` (the Construction folder `tree_row`: eye, collapsed by default, only for committed features), `crates/engine/src/scene.rs:533` (`hide_planes` per plane).

## V toggles the selection's visibility (P1)
- key: `ui:canvas:visibility-v-key`
- Fusion: With a construction plane (or any object) selected, V toggles its visibility: the row's eye greys; the object stays drawn blue while still selected and disappears when deselected. Show/Hide in the row menu does the same.
- FissionOxide: V on a selected construction plane writes "Show/Hide: select bodies, faces or edges first" in the lower left and changes nothing.
- Start here: `crates/ui/src/lib.rs:1296` (the Show/Hide command: accept construction planes, axes, points, sketches and origin items), `crates/ui/src/marking.rs:320`.

## construction plane row menu (P2)
- key: `ui:browser:construction-plane-menu`
- Fusion: Right-click a construction plane row: Move to Group, Create Selection Set, Create Sketch, Offset Plane, Edit Construction Plane, Configure, Delete (Del), Rename, Look At, Show/Hide (V), Find in Window, Find in Timeline.
- FissionOxide: Create Sketch, Rename, Delete.
- Start here: `crates/ui/src/panels/browser_rows.rs` (the construction row menu; reuse `crates/ui/src/panels.rs:2187`, `origin_plane_menu`, for Offset Plane and Look At).

## CONSTRUCTION GEOMETRY dialog (P1)
- key: `ui:construct:construction-geometry-dialog`
- Fusion: Offset Plane opens a docked dialog titled "CONSTRUCTION GEOMETRY": Type (three icon toggles: plane, axis, point; plane lit), Method (dropdown, "Offset"), Plane (chip "1 selected" with an x), Extent (dropdown, "Distance"), Distance ("1 in"), Resize (two icon toggles), OK / Cancel; a floating distance box and an arrow at the plane.
- FissionOxide: A dialog titled "OFFSET PLANE" with Plane (a dropdown, "XY plane") and Offset; no Type, Method, Extent or Resize rows.
- Start here: `crates/ui/src/dialogs.rs:1579` (`Kind::OffsetPlane`), the CONSTRUCT entries in `crates/ui/src/menus.rs` (every plane, axis and point command opens this one dialog with its Type and Method set).
