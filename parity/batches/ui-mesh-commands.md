# Parity batch: ui-mesh-commands

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## mesh commands (P1)
- key: `ui:tabs:mesh-commands`
- Fusion: MESH tab: Tessellate (B-Rep to mesh with refinement settings), Remesh, Repair (close holes, stitch, wrap), Generate Face Groups, Combine Face Groups, Shell, Combine, Separate, Scale Mesh, Create Mesh Section Sketch, Create Base Mesh Feature, Direct Edit, and INSPECT > Display Mesh Face Groups (Shift+F). Each opens its own dialog with a live preview.
- FissionOxide: All of these are inert menu items (menus.rs:387-470, x()). Only Insert Mesh, Reduce, Plane Cut, Smooth, Reverse Normal and Convert Mesh work.
- Start here: `crates/ui/src/menus.rs`, `crates/ui/src/forms.rs`

## Fusion's dialogs, read on screen (Fusion 2705, before any selection)
- **TESSELLATE:** Body (select). A Refinement Settings section: Refinement (Coarse/Medium/Fine/Custom), Surface Deviation (mm, with a slider), Normal Deviation (15.00 deg, slider), Maximum Edge Length (mm, slider), Aspect Ratio (21.50, slider), Create Quads checkbox, then Preview.
- **REMESH:** Body/Face Groups. Type (Adaptive / Uniform), Method (Density / ...), Density 0.25 (slider), Shape Preservation 0.50 (slider), Preserve Sharp Edges, Preserve Boundaries, Preview.
- **REPAIR:** Body. Repair Type ("Stitch and Remove..." / Close Holes / Wrap / Rebuild), Preview, and a collapsible Detailed Analysis section.
- **GENERATE FACE GROUPS:** Bodies/Face Groups. Type (Fast / Accurate), Angle Threshold 25.0 deg, Minimum Face Group Size 0.20 (slider), Preview.
- **COMBINE FACE GROUPS:** Face Groups (select).
- **SHELL (mesh):** Body. Thickness 2.00 mm (slider), Preview.
- **COMBINE (mesh):** Target Body, Tool Bodies, Operation (Join/Cut/Intersect/Merge icons), New Component, Keep Tools, Algorithm (Standard / ...), Preview.
- **SEPARATE:** Body. Type (Mesh Shells / Face Groups), Keep Body.
- **SCALE MESH:** Entities, Point, Scale Type (Uniform / Non-uniform), then the scale factor(s).
- **CREATE MESH SECTION SKETCH:** Body, Section Plane, and a collapsible Numerical Inputs section.
