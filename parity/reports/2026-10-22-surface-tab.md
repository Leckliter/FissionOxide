# 2026-10-22: SURFACE tab (prober 13)

Fusion (Personal licence, Part Design "Untitled*(1)", inches) and FissionOxide (`main` at
`de1a1c5a`, release build, mm) side by side, Design workspace, SURFACE tab.

## What was compared
- Ribbon: CREATE and MODIFY quick buttons, their tooltips (Patch, Extrude, Revolve, Sweep, Stitch,
  Unstitch, Trim, Extend), the CREATE and MODIFY dropdowns, the Pattern submenu, the shared panels.
- Dialogs as they open: Patch, Extrude, Revolve, Sweep, Loft, Ruled, Offset, Trim, Untrim, Extend,
  Stitch, Unstitch, Reverse Normal (Fusion); the same in FissionOxide (Sweep and Loft are missing).
- After the first pick (Fusion): Patch (disc edge), Revolve (axis), Ruled (edge), Offset (patch face),
  Extend (edge), Trim (Plane1: error toast); dropdown lists of Untrim Type, Extend Type and Edge
  Alignment, the Offset Type tooltip.
- Results: Fusion's Patch (browser row, timeline icon and name, both right-click menus);
  FissionOxide's surface Extrude and Patch (rows, timeline icons, both menus).
- Finish Sketch from the SURFACE tab in both apps.

## Matches
- `ui:surface:shared-panels`: CONFIGURE, CONSTRUCT, INSPECT, INSERT, ASSEMBLE, SELECT as on SOLID.
- `ui:surface:modify-menu-order`: MODIFY order and shortcut hints (apart from two extra Trim rows).
- `ui:surface:patch-result`: Patch closes an edge loop with a new surface body and timeline item.
- CREATE order (apart from the extra rows) and the Pattern submenu (Rectangular, Circular, Pattern on
  Path) are the same.

## Differences (20 rows, wave 28)
P1:
- Finish Sketch started from SURFACE returns to SOLID in FissionOxide; Fusion stays on SURFACE
  (`ui:surface:finish-sketch-returns-tab`, `crates/ui/src/panels.rs:861`).
- CREATE has extra "Patch from Sketch" and "Variable Offset" and an "E" hint on Extrude; MODIFY has
  extra "Trim with Body" / "Trim with Sketch" (`crates/ui/src/menus.rs:368`, `:391`).
- Surface Sweep and Loft are missing ("Sweep isn't in FissionOxide yet") (`menus.rs:375`).
- Every surface dialog uses the generic form: Patch has one row (Fusion: Boundary Edges, Group Edges,
  Interior Rails/Points, Enable Chaining, Flip, a continuity table after the pick); Extrude has Sketch /
  Curves / Distance 10 mm / Symmetric / Flip (Fusion: Profiles, Chaining, Start, Direction, Extent Type,
  Distance 0, Taper Angle, a manipulator); Revolve, Offset, Extend, Stitch, Trim rows differ; Ruled is a
  two-edge surface where Fusion's is one edge plus distance and angle (`crates/ui/src/forms.rs`).
- Surface bodies use the solid body icon; surface features use blue solid timeline icons
  (`panels.rs:1733`, `panels.rs:3595`).

P2: toolbar quick buttons (Offset/Create Form instead of Sweep, an extra BOM button, outline icons),
tooltip texts, Untrim's Type list, Unstitch / Reverse Normal pick rows, the surface body right-click
menu, Patch's timeline name ("Patch1" vs "SurfacePatch1"), the dimension hover tooltip's light box.

## Re-verified
- `ui:sketch:horizontal-conflict`: verified (line coincident with a circle swings about that end).
- `ui:sketch:tangent-arc-snap-marker`: verified (square marker on the hovered point).
- `ui:sketch:dimension-label-and-hover`: verified for text, line and tooltip text; the tooltip's
  light box is split out as `ui:sketch:dimension-hover-tooltip-style`.

## Batches
| batch | items | keys |
|---|---|---|
| ui-surface-menus | 5 | create-menu-extras, modify-menu-extras, toolbar-quick-buttons, tooltip-texts, finish-sketch-returns-tab |
| ui-surface-create-dialogs | 6 | patch-rows, extrude-rows, revolve-rows, ruled-rows, offset-rows, sweep-loft-missing |
| ui-surface-modify-dialogs | 5 | trim-rows, extend-rows, untrim-rows, stitch-rows, unstitch-reverse-rows |
| ui-surface-results | 4 | body-row-icon, timeline-icons, body-context-menu, sketch:dimension-hover-tooltip-style |

## Not covered
- Completing Revolve, Ruled, Offset, Extend, Stitch, Unstitch, Reverse Normal, Untrim, Trim (with a
  tool that cuts) in either app; their results' timeline icons and menus; Edit Feature of a surface
  feature; the Analysis tabs; surface Thicken / Boundary Fill / Pattern / Mirror (same dialogs as
  SOLID, recorded in earlier batches); Create Form (FORM workspace); Derive.
- FissionOxide's Ruled, Offset, Untrim, Unstitch and Reverse Normal dialogs were read from the form
  definitions (the generic form draws exactly those rows, confirmed on Stitch, Trim and Extend).

## Traps seen
- The sketch shortcut "D" twice did nothing in FissionOxide after the Coincident and
  Horizontal/Vertical toolbar flows (CREATE > Sketch Dimension worked); not recorded, re-check.
- Fusion's Ctrl+Z right after Finish Sketch reopens the sketch instead of removing it.
- FissionOxide's window opened at x 45..1137; clicks at x >= 1040 (its dialog's Cancel) did nothing.
  Win+Left then widening from the right edge with mouse down/move/up put it at 0..850.
- `left_click_drag` does not drag sketch points in FissionOxide; use left_mouse_down, several
  mouse_move steps and left_mouse_up.
