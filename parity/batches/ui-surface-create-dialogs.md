# Parity batch: ui-surface-create-dialogs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 13) in an unsaved Part Design document in each app (Fusion in
inches, FissionOxide in mm), FissionOxide built from `main` at `de1a1c5a`, from the SURFACE tab's
CREATE panel. FissionOxide's surface commands use the generic form dialog (`crates/ui/src/forms.rs`)
with a Preview checkbox in the footer; Fusion's dialogs have no Preview checkbox, use "Select"
chips that become "1 selected" with an x, show some rows only after the first pick, keep OK
disabled until the picks (and a non-zero distance) are made, and put an arrow manipulator with a
focused floating value box ("0.00 in" with a ⋮ menu) on the canvas. None of Fusion's surface
dialogs has an Operation row. See `parity/reports/2026-10-22-surface-tab.md`.

## Patch dialog rows (P1)
- key: `ui:surface:patch-rows`
- Fusion: PATCH has Feature and Analysis tabs. Feature: Boundary Edges (active pick, icon only), Group Edges (unchecked), Interior Rails/Points ("Select"), Enable Chaining (checked), Flip (icon button); OK disabled. The pointer reads "Select edge". After an edge pick a table appears under Group Edges with one row per boundary: "Edge..." and a continuity dropdown "Connected" (with icon); the picked loop draws bold black with an "Edge 1" label on the canvas; OK enables. The result is a surface body row "Body8 (1)" with an orange icon and an orange Patch timeline icon named "SurfacePatch1".
- FissionOxide: PATCH has one row, "Boundary edges" ("Select", becomes "1 selected" with an x), and a Preview checkbox; the hover tooltip reads "Boundary edges", after the pick "Hold Ctrl to modify selection"; no continuity table, no Group Edges / Interior Rails/Points / Enable Chaining / Flip, no tabs; the picked loop draws dashed light blue with no label.
- Start here: `crates/ui/src/forms.rs:1148` (PATCH); engine `crates/engine/src/features.rs:214` (surface.patch takes `edges` only: continuity G0/G1/G2 needs a fill with constraints).

## Surface Extrude rows (P1)
- key: `ui:surface:extrude-rows`
- Fusion: EXTRUDE (surface) opens with the sketch profile already picked: Profiles ("1 selected" x), Chaining (checked), Start ("Profile Plane"), Direction ("One Side"), Extent Type ("Distance"), Distance "0.00 in", Taper Angle "0.0 deg"; OK disabled at 0; an arrow manipulator on the profile and a focused floating "0.00 in" box; status "Chain".
- FissionOxide: EXTRUDE: Sketch (dropdown "Sketch1"), Curves (default all) ("Select"), Distance "10 mm", Symmetric (checkbox), Flip (checkbox), Preview; OK enabled at once; the preview appears with no manipulator and no floating box.
- Start here: `crates/ui/src/forms.rs:1115` (SURFACE_EXTRUDE); reuse the solid Extrude dialog's Start / Direction / Extent rows and manipulator (`crates/ui/src/dialogs.rs`) without the Operation row.

## Surface Revolve rows (P1)
- key: `ui:surface:revolve-rows`
- Fusion: REVOLVE (surface): Profile ("1 selected" x, picked automatically), Chaining (checked), Axis (active "Select"), Project Axis (checked); OK disabled. After an axis pick: Extent Type ("Partial"), Angle "360.0 deg", Direction ("One Side"); an angle manipulator with a focused "360.0 deg" box and the hint "Adjust angle value or modify the selection"; status "2 selections".
- FissionOxide: REVOLVE: Sketch (dropdown), Curves (default all), Axis (dropdown), Angle "360 deg", Preview.
- Start here: `crates/ui/src/forms.rs:1131` (SURFACE_REVOLVE).

## Ruled is one edge plus a distance, not two edges (P1)
- key: `ui:surface:ruled-rows`
- Fusion: RULED has Feature and Analysis tabs: Edges ("Select"), Chain Selection (checked), Type ("Normal" dropdown); OK disabled. After an edge pick: Distance "0.00 in", Angle "0.0 deg", Mitered Corners (unchecked); an arrow on the edge with the hint "Adjust distance and angle or modify selection". The ruled surface grows from one edge chain along the face normal (Normal type) by the distance.
- FissionOxide: RULED: "First edge" and "Second edge" pick rows: a ruled surface spanning two edges, which is a different command.
- Start here: `crates/ui/src/forms.rs:1547` (RULED); engine `crates/engine/src/features.rs:240` (surface.ruled).

## Surface Offset rows (P1)
- key: `ui:surface:offset-rows`
- Fusion: OFFSET: Faces / Surface Bodies ("Select"; pointer "Select faces or surface bodies to offset"), Chain Selection (checked), Offset Type (two icon toggles, the second "Rounded Offset: Creates rounded offset corners."); after a pick: Distance "0.00 in" with an arrow and a focused "0.00 in" box; OK enabled after the pick.
- FissionOxide: OFFSET: Faces ("Select"), Distance "2 mm", Preview; no Chain Selection or Offset Type; a separate "Variable Offset" menu entry.
- Start here: `crates/ui/src/forms.rs:1558` (SURFACE_OFFSET); VARIABLE_OFFSET at `crates/ui/src/forms.rs:2286`.

## Surface Sweep and Loft are missing (P1)
- key: `ui:surface:sweep-loft-missing`
- Fusion: SURFACE > CREATE > Sweep opens SWEEP (Feature and Analysis tabs; Type "Single Path", Profile "Select", Path "Select", Chain Selection checked, Orientation "Perpendicular"; no Operation). Loft opens LOFT (Feature and Analysis tabs; Profiles pick list with + and x, Guide Type two icon toggles (rails / centerline), Rails list, Chain Selection unchecked, Closed unchecked, Tangent Edges two icon toggles; no Operation). Both make surface bodies from open or closed curves.
- FissionOxide: Both entries only show "Sweep isn't in FissionOxide yet" (and the Loft equivalent) at the lower left.
- Start here: `crates/ui/src/menus.rs:375` (`x("Sweep", ...)`, `x("Loft", ...)`); the solid Sweep/Loft dialogs (batch ui-sweep-loft-revolve) with a surface output flag in the engine.
