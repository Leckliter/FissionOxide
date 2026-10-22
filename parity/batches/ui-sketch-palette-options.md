# Parity batch: ui-sketch-palette-options

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 5), FissionOxide `main` at `d99603d`. The row list matches Fusion
(Linetype, Look At, Sketch Grid, Snap, Slice, Profile, Points, Dimensions, Constraints, Projected
Geometries, Construction Geometries, 3D Sketch, Finish Sketch) and Sketch Grid, Snap, Dimensions
and Constraints toggle the canvas like Fusion's; the Options header collapses like Fusion's.

## greyed palette rows (P1)
- key: `ui:sketch:palette-disabled-rows`
- Fusion: Every checkbox is enabled: Profile off removes the light-blue closed-profile shading, Points off hides the sketch points, Projected Geometries and Construction Geometries hide those curves, Slice sections the model at the sketch plane, 3D Sketch enables out-of-plane input.
- FissionOxide: Slice, Profile, Points, Projected Geometries, Construction Geometries and 3D Sketch are drawn greyed and ignore clicks.
- Start here: `crates/ui/src/panels.rs:2604-2625` (hard-coded `("Slice", false), ("Profile", true), ...` rows), `crates/engine/src/scene.rs` (profile fill, sketch point drawing).

## palette collapse chevron (P2)
- key: `ui:sketch:palette-collapse-chevron`
- Fusion: The chevron in the SKETCH PALETTE header docks the palette into a vertical "SKETCH PALETTE" tab at the right edge of the canvas; clicking the tab shows the palette again (floating over the canvas until pinned).
- FissionOxide: The chevron does nothing.
- Start here: `crates/ui/src/panels.rs:2526-2545` (`sketch_palette`, `dialog_header`).

## degrees-of-freedom footer (P2)
- key: `ui:sketch:palette-dof-footer`
- Fusion: Nothing below the 3D Sketch row but the Finish Sketch button.
- FissionOxide: A blue "20 degrees of freedom - 4 profiles" line under the rows (kept on purpose by the wave-4 fixer; the owner's goal is an exact copy, so it has to go or move behind a preference).
- Start here: `crates/ui/src/panels.rs:2657`.

## palette tooltips (P2)
- key: `ui:sketch:palette-tooltips`
- Fusion: Hovering the second Linetype icon shows "Centerline"; the first icon's tooltip did not appear within 1.3 s (likely "Construction"); the checkbox rows show no tooltip.
- FissionOxide: The first Linetype icon shows "Normal/Construction (X)" at once, the second icon shows nothing.
- Start here: `crates/ui/src/panels.rs:2583` (Linetype row).
