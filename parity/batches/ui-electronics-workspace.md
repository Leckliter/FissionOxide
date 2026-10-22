# Parity batch: ui-electronics-workspace

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 18) with both windows 908 px wide; FissionOxide built from `main` at
`2d74c694`. Fusion's entry is DESIGN > ELECTRONICS > New Electronics Design. `ui-workspace-menu`
edits `workspace_menu` in the same file (`panels.rs`); rebase onto it if it lands first. See
`parity/reports/2026-10-22-workspaces.md`.

## ELECTRONICS submenu entries (P2)
- key: `ui:workspaces:electronics-submenu`
- Fusion: DESIGN > ELECTRONICS opens a submenu of two rows: New Electronics Design, New Electronics Library.
- FissionOxide: five rows: New Electronics Design, New Schematic, New PCB, New Electronics Library, New 3D PCB.
- Start here: `crates/ui/src/panels.rs:998` (the `"Electronics"` submenu list) and `open_workspace_document` at `crates/ui/src/panels.rs:1069`.

## New Electronics Design opens its own document tab (P1)
- key: `ui:electronics:new-document-tab`
- Fusion: New Electronics Design adds a new document tab "Untitled" (green board icon) and makes it active; the Part Design tab stays as it was. Closing that tab asks Save / Don't Save / Cancel (name "Electronics Design").
- FissionOxide: the same "Untitled" tab switches in place to the electronics editor (the workspace button reads ELECTRONICS); the design is out of view until the workspace is switched back.
- Start here: `crates/ui/src/ecad_ui.rs:89` (`open_electronics`) and `crates/ui/src/panels.rs:1083` (`switch_workspace`).

## Electronics design start page (P1)
- key: `ui:electronics:start-page`
- Fusion: the new tab shows tabs COMMON | UTILITIES with panels CREATE (5 icons; dropdown New Schematic / Reference a Schematic document / New PCB / Reference a PCB document / From Design (greyed)), IMPORT, OUTPUTS; no workspace dropdown; a browser with one "(Unsaved)" row, a "Description" pane with an Edit button below it; the canvas is a start page: "New Electronics Design" heading, a board illustration, "Start with a schematic", buttons New Schematic (blue), Import Non-Fusion File, Learning Resources; TEXT COMMANDS expanded at the bottom with Txt / Py / TS radio buttons.
- FissionOxide: opens straight into a schematic sheet with the hint "Pick a part from the library to start a schematic, or open the example board".
- Start here: `crates/ui/src/ecad_ui.rs:350`.

## Schematic editor ribbon (P1)
- key: `ui:electronics:schematic-ribbon`
- Fusion: New Schematic opens another tab with tabs DESIGN, DOCUMENT, VALIDATE, LIBRARY, UTILITIES; DESIGN has icon panels SWITCH, VIEW, EDIT, LAYERS, PLACE, CONNECT, SIMULATE, REWORK, MODIFY, SHORTCUTS, SELECT (every entry an icon, labels under the panels).
- FissionOxide: a row of text buttons: SCHEMATIC / BOARD toggles, New electronics design, Open the example board, then Select, Wire, Net label, No-connect, Delete, ERC, Import library... .
- Start here: `crates/ui/src/ecad_ui.rs:126` (the SCHEMATIC / BOARD toolbar row).

## Schematic editor layout (P1)
- key: `ui:electronics:schematic-layout`
- Fusion: under the ribbon a row with a net class dropdown ("sc2 Nets" with a green swatch), the grid readout "0.1 inch (4.2 0.3)" and a command line "Click or press / to activate command line mode"; a BROWSER at the left (Schematic > Document Settings, Named Views, Sheet 1/1); a "PLACE COMPONENTS" panel at the right with icon tabs; a white canvas with a small in-canvas toolbar at its bottom (info, eye, zoom in/out/fit, grid, +, stop, select); the status prompt "Left-click diagonal drag to select objects" at the lower left; SHEETS, TEXT COMMANDS and COMMENTS headers along the bottom.
- FissionOxide: a LIBRARY parts list (search box, R_0603 ... PinHeader_1x04) where the browser would be, a beige grid sheet, and an "x, y mm" readout at the lower right; no command line, PLACE COMPONENTS panel, SHEETS or TEXT COMMANDS headers.
- Start here: `crates/ui/src/ecad_ui.rs:749` (the LIBRARY column).
