# Parity batch: ui-sketch-redefine-and-dxf

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 7) from a sketch row's context menu in each app; FissionOxide built
from `main` at `42dec181`. Both commands are reached from the sketch row menu
(`crates/ui/src/panels.rs:2159`; batch `ui-browser-sketch-rows` renames Save As DXF to Export DXF).

## redefine sketch plane (P1)
- key: `ui:browser:redefine-sketch-plane`
- Fusion: Redefine Sketch Plane opens a docked dialog "REDEFINE SKETCH PLANE" with one row, Plane [Select] (the Select chip lit with a pointer icon), OK disabled, Cancel; the timeline rolls back to the sketch (later rows leave the browser and the canvas); the cursor prompt reads "Select a planar face or a work plane"; hovering a body face lights it and boxes the Body1 row; the pick fills the chip "1 selected" with an x, enables OK, selects the face (status "1 Face | Area : 1.44 in^2") and redraws the sketch's curves live on the new plane (a green-edged plane preview); Cancel restores the sketch, the camera and the timeline.
- FissionOxide: The entry is greyed with the tooltip "Not in FissionOxide yet".
- Start here: `crates/ui/src/panels.rs:2165` (`RowCmd::NotYet`), `crates/ui/src/dialogs.rs` (a one-row pick dialog like Offset Plane's), `crates/engine/src/commands.rs` (a `sketch.plane` edit that re-targets the sketch's plane and recomputes the features after it).

## export dxf dialog (P1)
- key: `ui:browser:export-dxf-dialog`
- Fusion: Export DXF opens a docked dialog "EXPORT DXF": Sketch "Sketch3" (read-only text), Units dropdown "Inch" (Centimeter, Millimeter, Meter, Inch (checked), Foot, Yard, Micron, Hectometer, Mile, Mil), a "Visible Geometry" group with Points, Projected Geometries and Construction Geometries checkboxes (all on, labels clipped to "Projected Geome..."), OK, Cancel. Escape on the open Units dropdown cancels the whole dialog. OK then opens Fusion's own white "Export DXF" window: Name "Sketch3" (selected), Type "DXF Files (*.dxf)", an unchecked "Save to a project in the cloud" with a project dropdown, a checked "Save to my computer" with the path "C:/Users/<user>/Downloads" and a "..." button, Cancel and a blue Save.
- FissionOxide: Save As DXF opens the Windows "Save As" dialog at once (File name "Sketch2.dxf", type "DXF (*.dxf)", Documents folder) with no options; the egui row menu stays painted behind the modal dialog.
- Start here: `crates/ui/src/panels.rs:2215` (`run_sketch_cmd`, `RowCmd::SaveAsDxf` calls `rfd` directly), `crates/ui/src/dialogs.rs` (add an EXPORT DXF dialog with the units and visible-geometry rows, then the file picker from OK), `crates/engine/src/commands.rs` (the `export` command's sketch DXF writer: honour units and the three visibility flags).
