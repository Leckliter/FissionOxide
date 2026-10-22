# Parity batch: ui-manage-bom-palette

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 16) in an unsaved Part Design document in each app, both windows
908 px wide, FissionOxide built from `main` at `c94d76ac`, on the MANAGE tab (MANAGE > BOM > Bill of
Materials; the same command sits at the end of the SOLID, PLASTIC and SHEET METAL MODIFY menus).
Fusion's document held one body and had never been saved, so its table showed the empty state. See
`parity/reports/2026-10-22-plastic-manage-utilities.md`.

## Bill of Materials palette (P1)
- key: `ui:manage:bom-palette`
- Fusion: Bill of Materials opens a wide docked palette "BILL OF MATERIALS" (a "-" collapse at the left of the header, ">>" at the right) over the lower right of the canvas, about 450 x 370 px: a toolbar row, then a table with a header row (select-all checkbox, Part Name, a blank thumbnail column, an icon column, Description, Part Number ... scrolling sideways, a horizontal scrollbar under the rows), and a blue-outlined "Close" button at the bottom right. With no saved version the table body shows a filing-cabinet illustration, the bold line "Can't show bill of materials data for this design." and under it "Design must be saved at least once for BOM data to be available." The ribbon button does not stay highlighted while the palette is open.
- FissionOxide: Bill of Materials opens the small command dialog "BILL OF MATERIALS" (about 230 x 80 px, header icon, ">>" button) with one line "No components" (or a name / material / mass line per component), an info icon and "Close"; the BOM ribbon button stays highlighted blue while it is open.
- Start here: `crates/ui/src/dialogs.rs:3795` (Kind::Bom rows), `crates/ui/src/dialogs.rs:175` (its title), `crates/ui/src/menus.rs:980` (MANAGE). Make it a palette like the Physical Material palette (`crates/ui/src/material_ui.rs`), with the columns below and FissionOxide's own data when the design has components.

## Bill of Materials toolbar, dropdowns and column settings (P2)
- key: `ui:manage:bom-toolbar-menus`
- Fusion: the palette's toolbar row holds a "Structured" dropdown (Structured ticked, Parts Only), a "Working" dropdown (Working ticked; Released and Pending greyed), then at the right a link icon, a download icon (both greyed with no data), a gear and a kebab. The kebab opens a small menu: "Refresh BOM" (with a refresh icon) and "Compare to Model", both greyed. The gear opens a "Column Settings" popover with a "Reset Order" button and two collapsible groups of checkboxes: GENERAL (Thumbnail, Description, Revision/Version, Part Number, Quantity, Material all checked; Material (Legacy) unchecked) and MANAGE (Item Number, Lifecycle, Revision, State, Change Order, all checked). Each dropdown opens under its field and closes when the field is clicked again.
- FissionOxide: none of these controls exist; the dialog has no toolbar, no view or version choice and no column settings.
- Start here: `crates/ui/src/dialogs.rs:3795` (or the new palette's module), with the column list kept in the app's preferences.
