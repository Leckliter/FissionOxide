# Re-verification sweep: sketch (2026-10-22, verifier 1)

Binary: `main` at `ef887079`, rebuilt in release. Every sketch-area key marked `fixing-merged` (85) was
re-done in FissionOxide from the row's `fusion` text; Fusion was not re-driven (its behaviour is the
recorded text). Two sketches covered the lot: Sketch1 for the drawing tools, heads-up boxes, palette and
dimension boxes; Sketch2 for fillet, chamfer, blend, offset, trim / extend / break, the constraint
commands and motions, tangent circles, text, mirror and the patterns; Sketch3 on a vertical plane for
Project with the Bodies filter.

## Results
- 74 verified (each row's history has what was seen).
- 7 reopened: `ui:sketch:hud-parameter-expression` (tooltip shows the value, not the expression),
  `ui:sketch:dim-box-menu` (only Measure, no recent values), `ui:sketch:3d-sketch-manipulator` (grid
  stays), `ui:sketch:circular-pattern-rows` (preview before the centre pick, solid-blue instances, box
  position, concentric centre not pickable), `ui:sketch:constraint-glyph-context-menu` ("Repeat Line"
  after a constraint command; list lacks OK / Cancel / Create Selection Set / Find in Browser),
  `ui:sketch:text-dialog-rows` (font default), `ui:sketch:project-include-menu` (Spun Profile axis
  dropdown and greyed rows, Isoparametric OK greyed, extra Preview rows).
- 3 new rows: `ui:sketch:window-delete-error` (P1: window box + Delete fails with "no point Point(d2) in
  this sketch"), `ui:sketch:feature-options-position`, `ui:sketch:palette-stale-height`.
- 4 not re-checked: `ui:sketch:intersect-missing`, `ui:sketch:look-at-sketch-up`,
  `ui:sketch:tangent-line-circle-motion`, `ui:sketch:slice-section-look` (see `progress.md`).

Open rows in done batches: `ui:canvas:hover-sketch`, `ui:canvas:window-select`,
`ui:sketch:constraint-glyphs`, `ui:canvas:selection-filters`, `ui:canvas:sketch-point-in-3d` verified;
`ui:canvas:select-other` and `ui:dialogs:combine-fields` set `match` as superseded by
`ui:canvas:select-other-popup` and `ui:dialogs:combine-rows`. `ui:canvas:box-select-colors` verified.

## Batches (wave 37)
| Batch | Items | Severity |
|---|---|---|
| `ui-reverify-sketch-1` | window-delete-error, feature-options-position, palette-stale-height, hud-parameter-expression, dim-box-menu, 3d-sketch-manipulator | one P1, five P2 |
| `ui-reverify-sketch-2` | circular-pattern-rows, constraint-glyph-context-menu, text-dialog-rows, project-include-menu | P2 |

## Not covered
The other 266 `fixing-merged` keys (dialogs next), Fusion's side of `ui:sketch:window-delete-error`
(assumed ordinary Delete), Perpendicular motion separately, the ellipse preview's parallel glyph.
