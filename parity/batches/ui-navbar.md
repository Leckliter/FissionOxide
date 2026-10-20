# Parity batch: ui-navbar

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## display settings (P1)
- key: `ui:navbar:display-settings`
- Fusion: Display Settings flyout: visual style, environment, effects, object visibility, camera, ground plane.
- FissionOxide: Has 2 of Fusion's 7 visual styles (Shaded, Shaded with Visible Edges Only), Ortho/Perspective, ViewCube and GPU toggles. Missing: Shaded with Hidden Edges, Wireframe x3, Environment, Effects (shadows, reflections, ground plane), Object Visibility submenu (origins, sketches, joints, construction), Perspective with Ortho Faces, Full Screen.
- Start here: `crates/ui/src/panels.rs`

## orbit (P2)
- key: `ui:navbar:orbit`
- Fusion: The bottom navigation bar has Orbit (free / constrained), Look At, Pan, Zoom, Fit.
- FissionOxide: Orbit, Look At, Pan, Zoom and Fit are there, but the carets after Orbit and Fit are just drawn (nav_caret, Sense::hover). Fusion's Orbit caret picks Free Orbit or Constrained Orbit, and the Zoom caret offers Zoom Window and Fit.
- Start here: `crates/ui/src/panels.rs`

## grid snaps (P2)
- key: `ui:navbar:grid-snaps`
- Fusion: Grid and Snaps flyout: layout grid, snap to grid, grid settings, incremental move.
- FissionOxide: Has Layout Grid and Snap to Grid. Missing: Layout Grid Lock, Grid Settings (adaptive/fixed spacing), Incremental Move, Set Increments.
- Start here: `crates/ui/src/panels.rs`
