# Parity batch: ui-mesh-commands

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## mesh commands (P1)
- key: `ui:tabs:mesh-commands`
- Fusion: MESH tab: Tessellate (B-Rep to mesh with refinement settings), Remesh, Repair (close holes, stitch, wrap), Generate Face Groups, Combine Face Groups, Shell, Combine, Separate, Scale Mesh, Create Mesh Section Sketch, Create Base Mesh Feature, Direct Edit, and INSPECT > Display Mesh Face Groups (Shift+F). Each opens its own dialog with a live preview.
- FissionOxide: All of these are inert menu items (menus.rs:387-470, x()). Only Insert Mesh, Reduce, Plane Cut, Smooth, Reverse Normal and Convert Mesh work.
- Start here: `crates/ui/src/menus.rs`, `crates/ui/src/forms.rs`
