# Parity batch: ui-select-and-marking

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## double click loop (P1)
- key: `ui:canvas:double-click-loop`
- Fusion: Double-clicking an edge selects its tangent chain or loop.
- FissionOxide: Double-clicking the plate's front edge selects that one edge (1 Edge | 74.00 mm). Fusion's double-click selects the edge's loop / tangent chain. There's no double-click handling in the canvas pick path (handle_clicks only handles double-click on sketch text).
- Start here: `crates/ui/src/viewport.rs`, `crates/ui/src/select.rs`

## overflow (P1)
- key: `ui:marking-menu:overflow`
- Fusion: A list under the ring: OK/Cancel, Pan, Zoom, Orbit options, Show All, Workspace, recent commands with shortcuts.
- FissionOxide: Right-click on a face in the middle of the canvas: the overflow list (about 460 px tall) is drawn over the ring and hides its top and left slots. marking.rs:752 puts the list under the ring and then clamp() pushes it back up inside the canvas. Fusion keeps the ring clear: when the list won't fit below, move the whole menu (ring centre and list) up, or open the list beside the ring. Content matches Fusion (context commands, Show/Hide V, Look At, Find in Browser, Pan/Zoom/Orbit/Fit F6, Measure I, Workspace >, recent commands with keys).
- Start here: `crates/ui/src/marking.rs`
