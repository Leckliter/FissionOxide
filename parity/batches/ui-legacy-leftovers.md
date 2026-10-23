# Parity batch: ui-legacy-leftovers

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

These nine rows come from the first waves (batches from 2026-10-19 to 2026-10-21 whose fixers ran
before the current loop and opened PRs #100-#122). Every other row of those batches is verified or
matches Fusion; these five were never closed. Several may since be covered by newer keys (for example
the sketch Text and in-sketch menu rows by the wave-7, wave-10 and wave-35 batches): read the row's
history and the newer keys first, and where a newer verified key already covers the behaviour, set this
row `verified` with a note naming that key instead of changing code.

## drag reorder (P2)
- key: `ui:browser:drag-reorder`
- Fusion: Bodies and components can be dragged between folders and components.
- FissionOxide: No drag and drop in the browser tree (no dnd handling in panels.rs). Fusion drags bodies between components and folders.
- Start here: `crates/ui/src/panels.rs`.

## context menu (P2)
- key: `ui:timeline:context-menu`
- Fusion: Right-click: Edit Feature, Edit Profile Sketch, Suppress, Delete, Roll History Marker Here, Create Group.
- FissionOxide: Verified on parity2 (b2bb8fe) (#102): Edit Profile Sketch on sketch-based features and Find in Browser added. Still missing Fusion's Create Group / Move to Group.
- Start here: `crates/ui/src/panels.rs`.

## no phantom saves (P1)
- key: `ui:chrome:no-phantom-saves`
- Fusion: Fusion only adds a Data panel entry when the user saves; unsaved untitled designs never appear there.
- FissionOxide: Root cause: fox-engine test tests::every_listed_command_is_handled runs every command (including hub.save) on a Session with no store_root, so each test run wrote empty 'Untitled' designs into the real %APPDATA% hub (38 hub touches per run). PR #116 makes test processes never resolve to the real hub; PR #100 (merged) guards the UI side.
- Start here: `crates/ui/src/data_panel.rs`, `crates/ui/src/lib.rs`, `crates/engine/src/hub.rs`.

## sketch curve (P2)
- key: `ui:context-menus:sketch-curve`
- Fusion: Right-click a sketch curve while sketching: Normal/Construction, Fix/UnFix, Delete, Trim, Dimension.
- FissionOxide: In a sketch the ring has Repeat, Line, Rectangle, Circle, Undo, Redo, Delete and the list has Finish Sketch, Normal/Construction X, Look At, Fit F6, Measure I. Missing Fusion's Fix/UnFix (and Dimension / Trim) for a picked curve.
- Start here: `crates/ui/src/marking.rs`.

## fillet fields (P2)
- key: `ui:dialogs:fillet-fields`
- Fusion: Fillet dialog: Type (Fillet/Rule Fillet/Full Round), selection rows each with a radius, Tangent Chain, Corner Type (Rolling Ball/Setback), radius manipulator.
- FissionOxide: Checked on parity2 (b2bb8fe, #111): Radius type now Constant / Chord length / Variable. Still missing the Type dropdown (Fillet / Rule Fillet / Full Round; full round is in driver PR #97, no rule fillet in the engine).
- Start here: `crates/ui/src/dialogs/solid.rs`.

## chamfer fields (P2)
- key: `ui:dialogs:chamfer-fields`
- Fusion: Chamfer dialog: Edges, Chamfer Type (Equal distance/Two distances/Distance and angle), Distance, Tangent Chain, Corner Type.
- FissionOxide: Checked on parity2 (b2bb8fe, #111): Chamfer type (Equal distance / Two distances / Distance and angle) added. Still missing Tangent Chain and Flip (engine support in driver PR #91) and Corner Type (no engine option).
- Start here: `crates/ui/src/dialogs/solid.rs`.

## sketch text (P2)
- key: `ui:sketch:sketch-text`
- Fusion: Text tool: click a point or two corners, dialog with font, height, bold/italic, alignment, and on-path option.
- FissionOxide: Text form (SK_TEXT) has text, height, angle, font and placement. Fusion's Text dialog adds bold/italic/underline, horizontal and vertical alignment, character spacing, a two-corner box, and Text on Path.
- Start here: `crates/ui/src/forms.rs`.

## undo history (P2)
- key: `ui:chrome:undo-history`
- Fusion: Undo/Redo dropdowns on the quick-access toolbar list the operations to step back to.
- FissionOxide: Verified on parity3 (main 8a0ba726), #108: the Undo caret lists steps, but names are coarse: six identical 'Edit Sketch6' entries for a rectangle, dims, fix and line, and a bare 'Edit' for the unit change. Fusion names each step by its command (Line, Dimension, Fix, Change Units).
- Start here: `crates/engine/src/history.rs`.

## canvas calibrate (P2)
- key: `ui:insert:canvas-calibrate`
- Fusion: Canvas: place an image on a plane, then right-click Calibrate by picking two points and typing a distance; opacity slider.
- FissionOxide: Canvas inserts an image on a plane, but there's no Calibrate (pick two points, type the real distance, image rescales) and no right-click Edit Canvas with opacity. Nothing in crates/ui mentions calibrate.
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/panels.rs`.
