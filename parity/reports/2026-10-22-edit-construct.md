# Parity report: Edit Feature (finished), Construct / Inspect / Insert / Assemble / Select (started) (2026-10-22, prober 11)

Areas: the rest of "Edit Feature for every feature; feature errors and warnings in the timeline"
(now done) and the start of "Configure, Construct, Inspect, Insert, Assemble, Select menu".
FissionOxide built from `main` at `ed3546c5`; Fusion "Untitled*(1)" (inches, Part Design) with the
primitives document prober 10 left, plus a Chamfer, Mirror, Hole, Press Pull, Move and Combine added
this session. FissionOxide: a new design (mm) with a box, a cylinder, Chamfer1, a hole (Sketch +
Hole1), Mirror1, Move1, Combine1 and PressPull1.

## Matched
- Edit Feature in Fusion is the creation dialog titled "EDIT FEATURE" for every type tried
  (Cylinder, Chamfer, Mirror, Hole, Move, Combine); FissionOxide opens one for Chamfer, Move and the
  primitives (`ui:dialogs:edit-chamfer-rows`: rows match apart from items already recorded).
- Roll Timeline Marker Here on a failed item (`ui:timeline:failed-item-roll-here`).
- CONSTRUCT list (already `ui:construct:construct-menu-list`), CONFIGURE list
  (`ui:menus:configure-list`).
- Re-verified in the app: `ui:menus:selection-filters-list`, `ui:menus:selection-priority-list`,
  `ui:menus:selection-tools-list` (now `verified`).

## Differences
P1:
- `ui:dialogs:edit-hole`, `ui:dialogs:edit-mirror`, `ui:dialogs:edit-combine`,
  `ui:dialogs:edit-press-pull`: double-click shows "This feature has no editable properties yet".
  `Dialog::edit` (`crates/ui/src/dialogs.rs:1084`) returns `None` for every kind it does not list,
  so the same is true (from the code, not tried on screen) of Scale, Loft, Sweep, Coil, Pipe, Thread,
  Rib, Thicken, every construction plane but Offset, every axis and point, and the sheet-metal and
  surface features.
- `ui:timeline:error-dialog`: Fusion's status-bar badge and Review warning open an "Error" window
  with a feature > "Compute Failed" > message tree; FissionOxide has neither.
- `ui:menus:assemble-part-design`: Fusion's ASSEMBLE in a Part Design document holds only "Add To
  Assembly"; FissionOxide lists every assembly command.

P2: `ui:dialogs:edit-move-rows`, `ui:dialogs:edit-values-units`, `ui:timeline:failed-item-menu`
(no Review warning), `ui:timeline:suppressed-item-menu` (Edit Feature still offered),
`ui:chrome:stale-status-message` (the "no editable properties" message stayed over three minutes),
`ui:timeline:hole-extra-sketch`, `ui:dialogs:combine-disjoint-names`,
`ui:dialogs:press-pull-edits-owner` (Fusion's Press Pull on a primitive's end face changes its
height), `ui:inspect:measure-dialog-rows`, `ui:menus:inspect-properties-extra`,
`ui:menus:insert-list` (Insert Component enabled, extra Insert STEP), `ui:menus:construct-icons`.

## Batches (wave 26, `map-wave26.json`)
| Batch | Items | Keys |
|---|---|---|
| `ui-edit-more-features` | 6 | edit-hole, edit-mirror, edit-combine, edit-press-pull, edit-move-rows, edit-values-units |
| `ui-timeline-failed-items` | 7 | error-dialog, failed-item-menu, suppressed-item-menu, stale-status-message, hole-extra-sketch, combine-disjoint-names, press-pull-edits-owner |
| `ui-inspect-insert-assemble` | 5 | assemble-part-design, measure-dialog-rows, inspect-properties-extra, insert-list, construct-icons |

## Retractions and notes
- A FissionOxide chamfer made to fail by its own value (50 mm on a 20 mm box) showed its message in
  the timeline tooltip with a warning glyph; at the zoom available the icon's pink fill could not be
  told apart, so nothing was recorded against `ui:timeline:error-icon-fill`.
- Fusion's "More Info" link in the error toast closed the toast; whatever it opened (likely the web
  help) was outside the granted apps.
- Fusion's ASSEMBLE and SELECT dropdowns sometimes ignore a click right after Escape; click again.
- A PowerShell pixel sample was refused by the auto-mode classifier this session; zooms were used.

## Not covered
- Edit Feature: Revolve, Shell, Draft (recorded earlier), Scale, patterns and sketches were not
  double-clicked in both apps; a yellow warning (no reliable way to make one was found); the toast's
  More Info target.
- Area "Configure, Construct, Inspect, Insert, Assemble, Select menu": every Construct dialog after
  Offset Plane and Axis Through Cylinder (each method's rows and previews in Fusion's single dialog),
  Inspect dialogs other than Measure (Interference, Section Analysis, Curvature Comb / Map, Zebra,
  Draft, Isocurve, Accessibility, Center of Mass, Minimum Radius, Navigator, Component Colors,
  Sustainability), Insert dialogs (Decal, Canvas, SVG, DXF, Mesh, Derive), Assemble in an assembly
  document (Add To Assembly, joints, motion, contact), the Configure table, the SELECT menu's top
  level in Fusion (it overflows under the overlay) and the selection modes.
- The older `fixing-merged` keys `ui:sketch:horizontal-conflict`, `ui:sketch:dimension-label-and-hover`
  (hover part), `ui:sketch:tangent-arc-snap-marker` and `ui:canvas:vertex-marker-style` were not
  re-checked.
