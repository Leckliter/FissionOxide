# Parity batch: ui-sketch-create-dialogs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21: the sketch CREATE entries that open a dialog (Text, Mirror, the two
patterns, Project / Include) and the CREATE menu itself. Fusion's sketch dialogs open small,
with only the selection rows and OK disabled, and grow once something is picked.

## text tool flow (P1)
- key: `ui:sketch:text-tool-flow`
- Fusion: Text opens a tiny TEXT dialog with one row, "Type", holding two icon toggles (text box / text on path), OK disabled, and the prompt "Place first corner of text frame"; the user drags a frame on the canvas and only then the dialog grows (text, font, size, alignment) and the text appears in the frame.
- FissionOxide: Text opens a full form at once (Text "FissionOxide", Placement, X, Y, Corner 2 X/Y, Height 5, Angle, Font, Bold/Italic/Underline, alignments, spacing, Along path, Below the path, Path alignment) with OK enabled, and a live preview "FissionOxide" already sits at the origin, drawn vertically on this plane though Angle is 0.
- Start here: `crates/ui/src/forms.rs:1140-1175` (`SK_TEXT`), the form dialog in `crates/ui/src/dialogs.rs`. Start with the Type row and a frame drag; fix the preview's orientation on non-XY planes.

## mirror dialog (P2)
- key: `ui:sketch:mirror-dialog`
- Fusion: The dialog is titled "MIRROR" with rows Objects [Select] and Mirror Line [Select] (both pick buttons, Objects active), OK disabled until both are set.
- FissionOxide: Titled "SKETCH MIRROR"; Mirror line is a dropdown "Pick a line"; OK is enabled with nothing picked.
- Start here: `crates/ui/src/dialogs.rs:156` (title), `:290` and the `Kind::SkMirror` body (search `SkMirror`).

## circular pattern dialog (P2)
- key: `ui:sketch:circular-pattern-dialog`
- Fusion: Sketch Circular Pattern opens with Objects [Select] and Center Point [Select] only, OK disabled; Quantity, angle and Suppress rows appear after the picks.
- FissionOxide: Opens with Objects [Select], "Center X, Y" as two typed number boxes (0, 0), Quantity 6, Total angle 360 deg, Symmetric, OK enabled.
- Start here: `crates/ui/src/dialogs.rs:157`, `:291` and the `Kind::SkCircPattern` body. The centre is a pick (a sketch point or circle centre), not typed coordinates.

## rectangular pattern dialog (P2)
- key: `ui:sketch:rectangular-pattern-dialog`
- Fusion: Opens with Objects [Select] and Direction/s [Select] only, OK disabled; the Distance Type, Quantity, Distance rows (and the second direction) appear after the picks.
- FissionOxide: Opens with Objects, Direction dropdown "Horizontal", Distance type "Spacing", Quantity 3, Distance "20 mm", Symmetric and Second direction checkboxes, OK enabled.
- Start here: `crates/ui/src/dialogs.rs:158`, `:292` and the `Kind::SkRectPattern` body; `crates/ui/src/dialogs/hole_pattern.rs` for the 3D pattern's row logic.

## intersect missing (P1)
- key: `ui:sketch:intersect-missing`
- Fusion: Project / Include > Intersect opens INTERSECT with Geometry [Select], Selection Filter (two icon toggles: bodies / faces-edges) and Projection Link checked, and creates the curves where the picked bodies cut the sketch plane.
- FissionOxide: The entry is listed but inert: the status text says "Intersect isn't in FissionOxide yet".
- Start here: `crates/ui/src/menus.rs:581` (`x("Intersect", ...)`), the Project To Surface form at `crates/ui/src/forms.rs:2456` as the pattern, `crates/engine/src/commands/sketch_include.rs` for the sectioning. Fusion's `Sketch.intersect` is a body/face section with the plane.

## project include menu extras (P2)
- key: `ui:sketch:project-include-menu`
- Fusion: The Project / Include submenu is Project (P), Intersect, Spun Profile, Include 3D Geometry, Project To Surface, Intersection Curve, Isoparametric Curve. Spun Profile's dialog has Geometry, Selection Filter, Projection Link, Axis [Select], Project Axis checked, Add Centerline checked, Tolerance "0.00 in", Flip Result; Isoparametric Curve's has Curve "Single" dropdown, Face [Select], Projection Link, OK enabled.
- FissionOxide: Five entries: Project, Intersect, Include 3D Geometry, Project To Surface, Intersection Curve. The engine already has `sketch.spun_profile` (`crates/engine/src/commands.rs:2692`) with no menu entry; there is no isoparametric curve.
- Start here: `crates/ui/src/menus.rs:578-586` (the submenu), `crates/ui/src/forms.rs:2456` (a form to copy for Spun Profile). Add Spun Profile with the rows above; add Isoparametric Curve as a form (grey it with a note if the kernel cannot evaluate iso-curves).

## project dialogs rows (P2)
- key: `ui:sketch:project-dialogs-rows`
- Fusion: PROJECT: Geometry [Select], Selection Filter (two icon toggles), Projection Link checked, OK disabled until a pick. PROJECT TO SURFACE: Faces [Select], Curves [Select], Project Type dropdown "Closest Point" (Along Vector adds a Direction row), Projection Link checked. INTERSECTION CURVE: Curve [Select], Curves or faces [Select], Projection Link checked. INCLUDE 3D GEOMETRY: Geometry [Select], Projection Link checked.
- FissionOxide: PROJECT: Geometry [Select], "Projected link (follows the body)" checked, OK enabled, no filter. PROJECT TO SURFACE: Faces, "Curves (default all)", an extra Sketch dropdown "Sketch3", Projection type, "Direction (Along Vector)" always shown, Preview instead of Projection Link. INTERSECTION CURVE: "Sketch curves (default all)", Sketch dropdown, Faces, Preview. INCLUDE 3D GEOMETRY: Edges [Select], Preview.
- Start here: `crates/ui/src/forms.rs:2376` (`SK_INCLUDE`), `:2456` (`SK_PROJECT_SURFACE`), `:2473` (`SK_INTERSECTION_CURVE`), the Project dialog (search `"PROJECT"` in `crates/ui/src/dialogs.rs`). Match the row names, hide Direction unless Along Vector, drop the Sketch row (the active sketch is implied), rename Preview to Projection Link where that is what it does.

## arc shortcut hint (P2)
- key: `ui:sketch:arc-shortcut-hint`
- Fusion: The Arc submenu shows no shortcut letter on 3-Point Arc (Fusion's defaults: L, R, C, D, P, X, T, O).
- FissionOxide: 3-Point Arc shows "A" in the submenu and "(A)" in its tooltip.
- Start here: `crates/ui/src/menus.rs:555` (`.k("A")`), `crates/ui/src/panels.rs:506`. Keep the key working only if Fusion's customised shortcuts are mirrored; otherwise drop the hint.
