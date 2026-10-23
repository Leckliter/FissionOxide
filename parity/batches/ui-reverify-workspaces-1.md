# Parity batch: ui-reverify-workspaces-1

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 5) re-checking the MANAGE, MANUFACTURE and ELECTRONICS keys in the rebuilt app (`main` at `c6232d9f`). Five are reopened keys and one is new.
Every row is already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this
batch); read each row's `history` before starting.

## Bill of Materials palette (P1)
- key: `ui:manage:bom-palette`
- Fusion: Bill of Materials opens a wide docked palette "BILL OF MATERIALS" (a "-" collapse at the left of the header, ">>" at the right) over the lower right of the canvas, about 450 x 370 px: a toolbar row, then a table with a header row (select-all checkbox, Part Name, a blank thumbnail column, an icon column, Description, Part Number ... scrolling sideways, a horizontal scrollbar under the rows), and a blue-outlined "Close" button at the bottom right. With no saved version the table body shows a filing-cabinet illustration, the bold line "Can't show bill of materials data for this design." and under it "Design must be saved at least once for BOM data to be available." The ribbon button does not stay highlighted while the palette is open.
- FissionOxide: Palette, header, '-' and '>>', toolbar, illustration and both lines match, but there is no table header row (select-all checkbox, Part Name, thumbnail, icon, Description, Part Number ...) and no horizontal scrollbar above the empty state; the status bar text '1 Face | Area ...' draws over the palette's Close button.
- Start here: `crates/ui/src/bom_ui.rs:479`

## CAM browser and no timeline (P1)
- key: `ui:manufacture:cam-browser`
- Fusion: the browser shows "CAM Root" (eye) with Units: In, Named Views, Origin (eye off), Models (eye) and Setups (eye); the timeline is hidden; the navigation bar gains CAM display icons after the usual ones.
- FissionOxide: Browser rows and hidden timeline match; the navigation bar shows only the usual icons, no CAM display icons after them.
- Start here: `crates/ui/src/cam_ui.rs`

## MILLING panels and dropdowns (P1)
- key: `ui:manufacture:milling-panels`
- Fusion: panels SETUP (quick New Setup, Create NC Program; dropdown New Setup / Create NC Program / New Folder / New Pattern / Manual NC / Probe WCS / Insert Manufacturing Data / Create Manufacturing Model), 2D (2D Adaptive Roughing / 2D Offset Roughing (Pocket) / Face / 2D Contour / Slot / Trace / Thread / Bore / Circular / Engrave / 2D Chamfer), 3D (3D Adaptive Roughing / 3D Offset Roughing (Pocket) / Flat / Parallel / Scallop / 3D Contour / Ramp / Pencil / Horizontal / Spiral / Radial / Morphed Spiral / Project / Blend / Morph / Flow / Geodesic), DRILLING (Drill), ACTIONS (Generate Ctrl+G / Simulate / Simulate with Machine / Post Process / Setup Sheet / Machining Time), MANAGE (Form Mill / Solid Holder > / Tool Library / Machine Library / Post Library / Template Library / Task Manager / Export Defaults / Import Defaults / Reset Defaults), INSPECT, SELECT.
- FissionOxide: Every list matches except MANAGE's Solid Holder, which has no submenu arrow (Fusion: 'Solid Holder >').
- Start here: `crates/ui/src/menus.rs:1154`

## New Setup dialog (P1)
- key: `ui:manufacture:setup-dialog`
- Fusion: a docked dialog "SETUP : SETUP1" with tabs Setup | Stock | Post Process. Setup tab: Machine (a "Select..." button), Setup (Operation Type "Milling" dropdown with icon), Work Coordinate System (WCS) (Orientation "Model orientation", Origin "Stock box point", Stock Point "Box Point" picker, Accessibility Analysis checkbox), Model (the body already picked: "Body" chip with an x), Fixture ("Select"). Stock tab: Mode "Relative size box", Stock Offset Mode "Add stock to sides and top-bottom", Stock Side Offset 0.040 in, Stock Top Offset 0.040 in, Stock Bottom Offset 0.00 in, Round Up to Nearest 0.00 in, and a read-only Stock Dimensions group (Width (X), Depth (Y), Height (Z)).
- FissionOxide: Tabs and Stock tab match; the Setup tab's Machine row is a '3-axis mill' dropdown where Fusion has a 'Select...' button, and Operation Type has no icon.
- Start here: `crates/ui/src/forms.rs:2689`

## New Electronics Design opens its own document tab (P1)
- key: `ui:electronics:new-document-tab`
- Fusion: New Electronics Design adds a new document tab "Untitled" (green board icon) and makes it active; the Part Design tab stays as it was. Closing that tab asks Save / Don't Save / Cancel (name "Electronics Design").
- FissionOxide: New tab with the green board icon, the Part Design tab kept, close asks 'Save changes to Electronics Design?' with Don't Save / Cancel / Save; but the tab is named 'Untitled(4)' (four Untitled tabs open) where Fusion's row says 'Untitled'. Check Fusion's name with other Untitled tabs open before changing it.
- Start here: `crates/ui/src/documents.rs:258`

## closing a schematic tab (P2)
- key: `ui:electronics:schematic-close-prompt`
- Fusion: Closing a new, untouched schematic tab closes it without asking (seen by prober 18).
- FissionOxide: Closing an untouched new schematic tab opens 'Close / Save changes to Untitled(5)? / Your changes will be lost if you don't save them.' with Don't Save / Cancel / Save.
- Start here: `crates/ui/src/documents.rs:972`, `crates/ui/src/ecad_chrome.rs:59`
