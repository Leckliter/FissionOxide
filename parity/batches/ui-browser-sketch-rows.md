# Parity batch: ui-browser-sketch-rows

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 7) with a body from Sketch1 (consumed by Extrude1) and an
unconsumed Sketch2 on XY in each app; FissionOxide built from `main` at `42dec181`. Fusion's
browser rows: the Sketches folder has an eye, every sketch row has an eye, a sketch icon and the
name; a consumed sketch is hidden (greyed eye). Fusion names the hovered row nowhere (no tooltip,
no status text); the status bar at the lower right names the selected row.

## sketch row menu list (P1)
- key: `ui:browser:sketch-row-menu-list`
- Fusion: Right-click on a sketch row lists, with icons and shortcut hints: Move to Group, Create Selection Set, Extrude (E), Offset Plane, | Edit Sketch, Redefine Sketch Plane, Select Sketch Plane, Slice Sketch (missing on the hidden consumed sketch; reads Unslice Sketch once on), Configure, | Export DXF, Delete (Del), Rename, | Look At, | Hide Profile / Show Profile, Show Dimension / Hide Dimension, Hide Projected Geometries, Hide Construction Geometries, Show/Hide (V), | Find in Window, Find in Timeline. The menu opens above the row with its bottom edge at the pointer when there is no room below. Select Sketch Plane selects the face or plane the sketch sits on (status "1 Face | Area : 0.80 in^2"); Find in Timeline highlights the sketch's timeline icon and selects the sketch; Create Selection Set adds a "Selection Sets" folder under Named Views.
- FissionOxide: Edit Sketch, Redefine Sketch Plane (greyed), Slice Sketch (greyed), | Hide Profile, Show Dimensions, Look At, Save As DXF, | Rename, Delete, | Show/Hide, Find in Window: no Move to Group, Create Selection Set, Extrude, Offset Plane, Select Sketch Plane, Configure, Hide Projected Geometries, Hide Construction Geometries or Find in Timeline, no shortcut hints, "Save As DXF" and "Show Dimensions" wording, and the menu opens downward from the pointer.
- Start here: `crates/ui/src/panels.rs:2159` (`sketch_menu`), `crates/ui/src/panels.rs:2215` (`run_sketch_cmd`), `crates/ui/src/panels.rs:3914` (the label test), `crates/ui/src/panels/browser_rows.rs:152` (the root menu's greyed cloud entries for the same treatment of Move to Group and Create Selection Set).

## sketch row eyes (P1)
- key: `ui:browser:sketch-row-eyes`
- Fusion: The Sketches folder row and every sketch row carry an eye toggle left of the icon; clicking a sketch's eye hides its curves and dimensions in the canvas and greys the eye (the row stays selected); Show/Hide in the row menu does the same.
- FissionOxide: Neither the Sketches folder nor the sketch rows have an eye; Show/Hide in the menu hides the sketch but nothing in the browser shows that it is hidden.
- Start here: `crates/ui/src/panels.rs:2303` (the Sketches folder `tree_row` with `eye: None`) and the sketch rows below it (`tree_row` at `crates/ui/src/panels.rs:1302` takes the eye state); `crates/engine/src/scene.rs:604` (`sketch_visible`).

## consumed sketch hidden (P2)
- key: `ui:browser:consumed-sketch-hidden`
- Fusion: Finish Sketch on a sketch that a feature consumed (the body's Sketch1) leaves it hidden: greyed eye, nothing drawn in the canvas until the eye is clicked.
- FissionOxide: Sketch1 stays drawn under the body (its light-blue profile shows as soon as Body1 is hidden).
- Start here: `crates/ui/src/lib.rs:1398` (`finish_sketch`) and `crates/engine/src/scene.rs:604` (`sketch_visible`): hide a sketch when a feature consumes its profiles.

## sketches folder menu (P1)
- key: `ui:browser:sketches-folder-menu`
- Fusion: Right-click on the Sketches folder lists New Sketch Group, Create Selection Set, | Show/Hide (V), Show All; the status bar reads "Sketches".
- FissionOxide: Right-click on the Sketches folder selects it (status "Sketches") and opens nothing.
- Start here: `crates/ui/src/panels.rs:2303` (the Sketches folder row; add a `context_menu` as `crates/ui/src/panels/browser_rows.rs:320` does for the Bodies folder, greying the cloud-only entries).

## sketch row hover (P2)
- key: `ui:browser:sketch-row-hover`
- Fusion: Hovering a sketch row highlights that sketch in the canvas (its closed profiles fill a translucent light blue and its curves lighten) and shows no tooltip and no status text.
- FissionOxide: Hovering a sketch row shows the tooltip "Double-click to edit" and the status text "Sketch2", and nothing changes in the canvas.
- Start here: `crates/ui/src/panels.rs:2340` (`on_hover_text("Double-click to edit")` and the `marking::peek` call on hover), `crates/ui/src/marking.rs:375` (`peek_feature`, which should light the sketch's curves and profiles as the body rows light bodies).

## sketch row selected canvas (P2)
- key: `ui:browser:sketch-row-selected-canvas`
- Fusion: Clicking a sketch row selects the sketch: the row is boxed, its timeline icon is boxed, the status bar reads "Sketch3", and in the canvas its closed profiles fill solid blue (the body face under a sketch turned a saturated mid blue) with the curves drawn light over the fill.
- FissionOxide: The row and the timeline icon are boxed and the status reads "Sketch2", but the canvas keeps the plain light-blue profile fill; nothing marks the selected sketch there.
- Start here: `crates/ui/src/panels.rs:2330` (the row click sets `Sel::Feature`), `crates/engine/src/scene.rs` (profile fill colour while the sketch's feature is selected; `PROFILE_SELECTED` exists for in-sketch selection).

## units row mass unit (P2)
- key: `ui:browser:units-row-mass-unit`
- Fusion: The Document Settings row reads "Units: in, ouncemass" (length unit, then the mass unit).
- FissionOxide: The row reads "Units: mm" with no mass unit.
- Start here: `crates/ui/src/panels.rs:1453` (the `Units:` label); the design's units live in `app.s.design.units`.
