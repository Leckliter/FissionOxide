# Parity batch: ui-material-appearance

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-22 (prober 9): SOLID > MODIFY in each app, built from `main` at `ffa348e2`. See
`parity/reports/2026-10-22-solid-modify.md`.

## Physical Material palette (P1)
- key: `ui:dialogs:physical-material-palette`
- Fusion: PHYSICAL MATERIAL is a palette: "In This Design" (collapsible, swatch thumbnails), "Library" (search box, Library dropdown "Fusion Material Library", folders Ceramic, Electronics, Fabric, Flooring, Gas, Glass, ...), and a Close button; a material is dragged onto a body or component.
- FissionOxide: A two-row dialog: Component (chip, prompt "Select a body of a component") and Material (dropdown, "Steel"), OK/Cancel.
- Start here: `crates/ui/src/dialogs.rs:3390` (`Kind::Material` rows): reuse the Appearance panel's layout (`crates/ui/src/dialogs.rs:4823`) with the physical-material list and drag-to-apply.

## Appearance library layout (P2)
- key: `ui:dialogs:appearance-library`
- Fusion: APPEARANCE: a collapsible "Apply To" group with two radio buttons (Bodies/Components, Faces), "In This Design" as swatch thumbnails, "Library" with a search box, a refresh button and tabs Fusion Appearances / My Appearances / Favorites, "Show downloadable materials" (checked), "Cancel all Downloads", then folders (Concrete, Fabric, ...); Close.
- FissionOxide: Apply To as two segmented buttons, In This Design as a text row "Steel - Satin (2)", Library as a flat list of 17 named colour swatches, and a hint line "Drag a swatch onto a body or face, or click one to apply it to the selection"; Close.
- Start here: `crates/ui/src/dialogs.rs:4823` (panel layout) and `crates/engine/src/features.rs:295` (`APPEARANCES`: group into folders).

## Manage Materials missing (P2)
- key: `ui:dialogs:manage-materials`
- Fusion: Manage Materials opens a separate "Material Browser" window: Physical / Appearance tabs, a search box, Home and library breadcrumbs, a library tree (Fusion Material Library with Ceramic ... Wood, Fusion Appearance Library, Fusion Nonlinear ...), a Name list with swatches, and a bottom toolbar.
- FissionOxide: Shows "Manage Materials isn't in FissionOxide yet".
- Start here: `crates/ui/src/menus.rs:185` (`x("Manage Materials")`): a window with the two tabs over the same libraries as the palettes.
