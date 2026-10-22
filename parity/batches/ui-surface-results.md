# Parity batch: ui-surface-results

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 13) in an unsaved Part Design document in each app (Fusion in
inches, FissionOxide in mm), FissionOxide built from `main` at `de1a1c5a`. Fusion: a Patch on a
disc's edge (Body8, timeline item SurfacePatch1). FissionOxide: a surface Extrude of a sketch circle
(Body1) and a Patch on its top edge (Body2). The last item is the re-check of a fixed sketch key.
See `parity/reports/2026-10-22-surface-tab.md`.

## Surface body browser row icon (P1)
- key: `ui:surface:body-row-icon`
- Fusion: A surface body's browser row ("Body8 (1)") has an orange surface icon, so surface and solid bodies differ at a glance in the Bodies folder.
- FissionOxide: Surface bodies (Body1 from a surface Extrude, Body2 from Patch) use the same grey solid-body icon as solids.
- Start here: `crates/ui/src/panels.rs:1733` (`tree_row(..., Icon::Body, ...)` for every body; pick an orange surface icon when the body is a surface).

## Surface feature timeline icons and names (P1)
- key: `ui:surface:timeline-icons`
- Fusion: Surface features show orange icons in the timeline (Patch an orange folded sheet); the Patch item is named "SurfacePatch1" in the status bar and its hover.
- FissionOxide: The surface Extrude item uses the blue solid Extrude icon and Revolve the solid Revolve icon; Patch's icon is blue-grey; the Patch item is named "Patch1" (the surface Extrude is "SurfaceExtrude1").
- Start here: `crates/ui/src/panels.rs:3595` (feature_icon: SurfaceExtrude, SurfaceRevolve, Patch and the other surface kinds; draw them with the orange tint the menus use); the Patch feature name in `crates/engine/src/features.rs:214`.

## Surface body right-click menu (P2)
- key: `ui:surface:body-context-menu`
- Fusion: A surface body row's menu: Move/Copy (M), Move to Group, Create Selection Set, Configure, Physical Material, Appearance (A), Texture Map Controls, Properties, Save As Mesh | Copy (Ctrl+C), Cut (Ctrl+X), Delete (Del), Remove, Rename | Display Detail Control | Show/Hide (V), Selectable/Unselectable | Opacity Control >, Isolate | Find in Window. There is no "Create Components from Bodies" and Physical Material is enabled.
- FissionOxide: A surface body row shows the solid body menu: Move/Copy, Create Components from Bodies, Physical Material (greyed), Appearance, Properties, Save As Mesh, Delete, Remove, Rename, Show/Hide, Isolate, Find in Window, Selectable/Unselectable, Opacity Control.
- Start here: `crates/ui/src/panels.rs:2007` (body_menu: hide Create Components from Bodies for surface bodies and enable Physical Material).

## Dimension hover tooltip is dark (P2)
- key: `ui:sketch:dimension-hover-tooltip-style`
- Fusion: Hovering a sketch dimension shows the parameter in a dark tooltip under the text ("d9 = 0.4787837888 in"), as recorded in `ui:sketch:dimension-label-and-hover`.
- FissionOxide: Since the fix the text and line colours and the tooltip text match ("d2 = 19.0932944915 mm"), but the tooltip is a light box with dark text.
- Start here: `crates/ui/src/viewport.rs:2931` (`on_hover_text(dim_tooltip(..))` uses egui's light tooltip; draw a dark one).
