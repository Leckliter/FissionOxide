# Parity batch: ui-inspect-tools

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## analyses (P2)
- key: `ui:inspect:analyses`
- Fusion: INSPECT: Curvature Comb Analysis (combs on picked curves/edges with density and scale sliders), Isocurve Analysis (UV isocurves on faces), Minimum Radius Analysis (colour map below a radius), Environment Map Analysis (reflection map), each kept as an Analysis entry in the browser that can be shown/hidden.
- FissionOxide: Curvature Comb, Isocurve, Minimum Radius and Environment Map are inert menu items (menus.rs INSPECT and UTIL_INSPECT). Curvature Map, Draft, Zebra and Accessibility already work through Kind::Analysis.
- Start here: `crates/ui/src/menus.rs`, `crates/render/src/lib.rs`

## display toggles (P2)
- key: `ui:inspect:display-toggles`
- Fusion: INSPECT > Display Component Colors (Shift+N) colours each component differently; History shows a history panel; Navigator shows the design's dependency graph.
- FissionOxide: All three are inert menu items (menus.rs, UTIL_INSPECT and INSPECT).
- Start here: `crates/ui/src/menus.rs`

## utilities utility (P2)
- key: `ui:chrome:utilities-utility`
- Fusion: UTILITIES > UTILITY: Manage Materials opens the material browser (libraries, In This Design, edit properties); Manage Threads Library lists thread tables; ADD-INS > Scripts and Add-Ins (Shift+S) lists scripts/add-ins with Run, Stop, Run on Startup.
- FissionOxide: Manage Materials, Manage Threads Library and Scripts and Add-Ins are inert (menus.rs ADD_INS and UTILITY).
- Start here: `crates/ui/src/menus.rs`

## Fusion's analysis dialogs, read on screen (Fusion 2705)
- **ISOCURVE ANALYSIS:** Bodies/Faces. Type (three icons: U and V / U only / V only), Isocurves 5 (slider), Curvature Combs checkbox.
- **MINIMUM RADIUS ANALYSIS:** Body. Maximum Tool Radius 25.00 mm, Minimum Radius 0.000 mm (slider), Opacity 100 (slider), Sharp Edges checkbox.
- **ENVIRONMENT MAP ANALYSIS:** Bodies. Environment (Sharp Highlights / ...), Rotate 0.0 deg (slider), Apply Mirror checkbox.
- **CURVATURE COMB ANALYSIS:** a paid feature on Fusion Personal; starting it opens a subscription offer, so its dialog couldn't be read. Build it from Fusion's documentation: Curves/Edges selection, Density, Scale and Comb Color.
- Each analysis, once OK'd, appears under an Analysis folder in the browser with an eye toggle and can be edited again.
