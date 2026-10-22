# Parity batch: ui-manufacture-workspace

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 18) with both windows 908 px wide; Fusion's Part Design "Untitled*"
(inches, Body1), FissionOxide built from `main` at `2d74c694` with one 34 x 20 x 10 mm box. Fusion's
workspace is reached with DESIGN > MANUFACTURE. See `parity/reports/2026-10-22-workspaces.md`.

## Manufacture tabs (P1)
- key: `ui:manufacture:tabs`
- Fusion: six tabs MILLING, TURNING, ADDITIVE, INSPECTION, FABRICATION, UTILITIES. ADDITIVE: SETUP (2 icons), ACTIONS (3), MANAGE, INSPECT, SELECT. INSPECTION: SETUP, PROBING, MANUAL, ACTIONS, MANAGE, INSPECT, SELECT. FABRICATION: SETUP, CUTTING, ACTIONS, MANAGE, INSPECT, SELECT. UTILITIES: ACTIONS, MANAGE, INSPECT, ADD-INS, SELECT.
- FissionOxide: only MILLING and TURNING.
- Start here: `crates/ui/src/panels.rs:1158` (`tab_row`, the `app.manufacture` branch) and `crates/ui/src/panels.rs:61` (`Tab`); toolbars in `crates/ui/src/menus.rs:976`.

## MILLING panels and dropdowns (P1)
- key: `ui:manufacture:milling-panels`
- Fusion: panels SETUP (quick New Setup, Create NC Program; dropdown New Setup / Create NC Program / New Folder / New Pattern / Manual NC / Probe WCS / Insert Manufacturing Data / Create Manufacturing Model), 2D (2D Adaptive Roughing / 2D Offset Roughing (Pocket) / Face / 2D Contour / Slot / Trace / Thread / Bore / Circular / Engrave / 2D Chamfer), 3D (3D Adaptive Roughing / 3D Offset Roughing (Pocket) / Flat / Parallel / Scallop / 3D Contour / Ramp / Pencil / Horizontal / Spiral / Radial / Morphed Spiral / Project / Blend / Morph / Flow / Geodesic), DRILLING (Drill), ACTIONS (Generate Ctrl+G / Simulate / Simulate with Machine / Post Process / Setup Sheet / Machining Time), MANAGE (Form Mill / Solid Holder > / Tool Library / Machine Library / Post Library / Template Library / Task Manager / Export Defaults / Import Defaults / Reset Defaults), INSPECT, SELECT.
- FissionOxide: SETUP (New Setup / Fixtures / Tool Library), 2D (Face / 2D Pocket / 2D Contour / 2D Adaptive Clearing / Slot / Trace / Thread / Bore / Engrave), 3D (Parallel / Scallop / Adaptive Clearing / Steep and Shallow / Contour / Pencil), DRILLING (Drill, matches), a MULTI-AXIS panel (Rotary / Swarf) that Fusion's MILLING tab does not show, ACTIONS (Generate Ctrl+G / Simulate / Post Process / Delete Setup / 2D Cut (Laser, Plasma, Waterjet) / Cut Service Check / All Flat Patterns on One Sheet); no MANAGE, INSPECT or SELECT panels.
- Start here: `crates/ui/src/menus.rs:976` (the MILLING groups, `CAM_MULTI_AXIS` at 982).

## TURNING panels and dropdowns (P1)
- key: `ui:manufacture:turning-panels`
- Fusion: panels SETUP (the same list as MILLING's), TURNING (Turning Face / Turning Profile Roughing / Turning Profile Finishing / Turning Groove Roughing / Turning Groove Finishing / Turning Adaptive Roughing / Turning Groove / Turning Single Groove / Turning Thread / Turning Chamfer / Turning Part / Turning Trace), PART HANDLING (Subspindle Grab / Bar Pull / Subspindle Return / Tool Call), DRILLING, ACTIONS, MANAGE, INSPECT, SELECT.
- FissionOxide: SETUP (New Turning Setup / Tool Library), TURNING (Turning Face / Profile Roughing / Profile Finishing / Groove / Turning Thread / Part Off), ACTIONS; no PART HANDLING, DRILLING, MANAGE, INSPECT, SELECT.
- Start here: `crates/ui/src/menus.rs:988` (the TURNING groups) and `crates/ui/src/menus.rs:903`.

## CAM browser and no timeline (P1)
- key: `ui:manufacture:cam-browser`
- Fusion: the browser shows "CAM Root" (eye) with Units: In, Named Views, Origin (eye off), Models (eye) and Setups (eye); the timeline is hidden; the navigation bar gains CAM display icons after the usual ones.
- FissionOxide: the design browser stays (Document Settings, Named Views, Origin, Bodies) and the design timeline stays at the bottom.
- Start here: `crates/ui/src/panels.rs` (browser tree and `timeline` at 3414 while `app.manufacture`).

## New Setup dialog (P1)
- key: `ui:manufacture:setup-dialog`
- Fusion: a docked dialog "SETUP : SETUP1" with tabs Setup | Stock | Post Process. Setup tab: Machine (a "Select..." button), Setup (Operation Type "Milling" dropdown with icon), Work Coordinate System (WCS) (Orientation "Model orientation", Origin "Stock box point", Stock Point "Box Point" picker, Accessibility Analysis checkbox), Model (the body already picked: "Body" chip with an x), Fixture ("Select"). Stock tab: Mode "Relative size box", Stock Offset Mode "Add stock to sides and top-bottom", Stock Side Offset 0.040 in, Stock Top Offset 0.040 in, Stock Bottom Offset 0.00 in, Round Up to Nearest 0.00 in, and a read-only Stock Dimensions group (Width (X), Depth (Y), Height (Z)).
- FissionOxide: a docked "SETUP" with one flat list: Name, Stock side offset 2, Stock top offset 1, Post-processor Fanuc, G-code units Millimetres, Fixtures (vise, clamps) Select, Machine 3-axis mill, Tilt travel from -120, Tilt travel to 120, Head pivot to tool tip (0 = TCP) 0; no tabs, no WCS or Model rows.
- Start here: `crates/ui/src/forms.rs:1864` (`CAM_SETUP`) and `crates/ui/src/forms.rs:1886` (`CAM_TURN_SETUP`).

## Setup preview: WCS triad and translucent stock (P2)
- key: `ui:manufacture:setup-canvas`
- Fusion: while New Setup is open the canvas shows the WCS triad (red X, green Y, blue Z arrows) at the stock box corner and the stock as a translucent box around the body.
- FissionOxide: the stock is a dashed outline box and there is no triad.
- Start here: `crates/ui/src/cut_ui.rs:71` and the CAM stock drawing in `crates/ui/src/viewport.rs`.
