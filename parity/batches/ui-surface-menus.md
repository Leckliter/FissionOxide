# Parity batch: ui-surface-menus

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 13) in an unsaved Part Design document in each app (Fusion in
inches, FissionOxide in mm), FissionOxide built from `main` at `de1a1c5a`, both on the Design
workspace's SURFACE tab. The shared panels (CONFIGURE, CONSTRUCT, INSPECT, INSERT, ASSEMBLE,
SELECT) match; these items are the SURFACE tab's own CREATE and MODIFY panels and the tab Fusion
returns to after a sketch. See `parity/reports/2026-10-22-surface-tab.md`.

## SURFACE CREATE dropdown entries (P1)
- key: `ui:surface:create-menu-extras`
- Fusion: CREATE lists Create Sketch, Create Form, Derive | Extrude, Revolve, Sweep, Loft, Patch, Ruled, Offset | Pattern > (Rectangular Pattern, Circular Pattern, Pattern on Path) | Mirror | Thicken, Boundary Fill, Joint Origin. No row shows a shortcut (Extrude has no "E" here).
- FissionOxide: The same list plus two rows Fusion does not have, "Patch from Sketch" (after Patch) and "Variable Offset" (after Offset; Fusion's variable offset lives in Offset's Offset Type row, see batch ui-surface-create-dialogs), and Extrude shows the shortcut "E".
- Start here: `crates/ui/src/menus.rs:368` (SURFACE_CREATE: drop the Patch from Sketch and Variable Offset rows, keep their forms reachable from the Patch/Offset dialogs if needed; the "E" comes from the shared Extrude entry's shortcut lookup).

## SURFACE MODIFY dropdown entries (P1)
- key: `ui:surface:modify-menu-extras`
- Fusion: MODIFY lists Press Pull (Q), Fillet (F), Chamfer | Trim, Untrim, Extend, Stitch, Unstitch, Reverse Normal, Scale, Split Face, Split Body | Move/Copy (M), Align, Delete (Del), Remove | Physical Material, Appearance (A), Manage Materials | Change Parameters, Compute All (Ctrl+B), Bill of Materials. One Trim entry: the trim tool can be a body, a sketch curve or a plane.
- FissionOxide: The same list plus "Trim with Body" and "Trim with Sketch" after Trim.
- Start here: `crates/ui/src/menus.rs:391` (SURFACE_MODIFY); fold TRIM_BODY / TRIM_SKETCH into the single Trim dialog's Trim Tool pick (`crates/ui/src/forms.rs:1572`).

## SURFACE toolbar quick buttons (P2)
- key: `ui:surface:toolbar-quick-buttons`
- Fusion: The SURFACE ribbon shows five CREATE buttons (Create Sketch, Patch, Extrude, Revolve, Sweep) and four MODIFY buttons (Stitch, Unstitch, Trim, Extend). The surface icons are solid orange (Patch a folded orange sheet, Extrude an orange box, Stitch/Unstitch orange tiles).
- FissionOxide: CREATE shows Create Sketch, Patch, Extrude, Revolve, Offset and Create Form (no Sweep); MODIFY shows Stitch, Unstitch, Trim, Extend and Bill of Materials. The Patch and Revolve icons are thin orange outlines and Trim is a black scissors.
- Start here: `crates/ui/src/menus.rs:557` (SURFACE groups' `quick` lists); icons in `crates/ui/src/icons.rs:1506` (Icon::Patch).

## SURFACE ribbon tooltips (P2)
- key: `ui:surface:tooltip-texts`
- Fusion: Patch: "Creates a planar or 3D surface within the boundary of a specified closed loop." / "Select the loop to patch then apply connected (G0), tangent (G1), and curvature (G2) boundary conditions." (with an illustration). Extrude: "Adds depth to open or closed sketch profiles or planar faces." / "Select the profile or planar face then specify the distance to extrude." / "Press Ctrl+/ for more help." Revolve: "Revolves a sketch profile or planar face around a selected axis." / "Select a profile or planar face and an axis to revolve around, then specify the extent type, direction, and operation." Sweep: "Sweeps a sketch profile or planar face along a ..." Stitch: "Joins surfaces to make a single surface body or solid body ..." Unstitch: "Breaks joined surfaces into individual surfaces." Trim: "Removes the selected area of a surface body using ..." Extend: "Enlarges a surface by extending its edges a specified ..."
- FissionOxide: Patch reads "Creates a surface filling a closed boundary of edges." / "Pick the edges that form a closed loop; a surface spans them. Set how it meets the neighbouring faces, such as tangent, to close holes smoothly." The other SURFACE buttons use FissionOxide's own wording.
- Start here: `crates/ui/src/tips.rs:1660` (Patch; the other SURFACE entries are near it).

## Finish Sketch returns to the SURFACE tab (P1)
- key: `ui:surface:finish-sketch-returns-tab`
- Fusion: Create Sketch from the SURFACE tab, pick a plane, Finish Sketch: the ribbon comes back on the SURFACE tab.
- FissionOxide: The same steps come back on the SOLID tab, so every surface command needs a tab click after each sketch.
- Start here: `crates/ui/src/panels.rs:861` (`toolbar` sets `Tab::Solid` when the sketch tab goes away); remember the tab the sketch was started from (`crates/ui/src/lib.rs:1517` finish_sketch).
