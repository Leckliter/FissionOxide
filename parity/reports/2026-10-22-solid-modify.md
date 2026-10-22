# Parity probe: SOLID > MODIFY (2026-10-22, prober 9)

FissionOxide built from `main` at `ffa348e2` (release), window at screenshot x 8..915 after the
re-check below, Data Panel closed; a new design with Body1 (20 mm box at the origin), Body2 (20 mm
box at X 40) and Draft1 (3 deg on Body1's +X face). Fusion: the "Untitled(1)*" tab from prober 8
(box, cylinder, sphere, torus, coil, Plane1, Sketch1), with a new Draft (5 deg on the box's +X face,
pull direction its top face) at the end of the timeline. Every MODIFY entry except Press Pull,
Fillet, Chamfer, Shell and Move/Copy (first pass done earlier) and Delete / Bill of Materials was
opened in both apps; Draft was completed and edited in both; the other dialogs were driven through
their first picks and cancelled.

## Re-verified (from the progress list)
- `ui:toolbar:responsive-collapse`: reopened. At a 907 px window FissionOxide now keeps every
  panel through SELECT visible and CREATE has 5 icons, but MODIFY still shows 6 against Fusion's 4
  (moved to `ui-modify-menu-tooltips`).
- Not re-checked: `ui:sketch:horizontal-conflict`, `ui:sketch:dimension-label-and-hover` (hover),
  `ui:sketch:tangent-arc-snap-marker`, `ui:canvas:vertex-marker-style` (a hover on a box corner
  showed no permanent dark dots on the other corners, but the `zoom` was too small to read the
  ring; use a native-resolution crop as the progress traps say).

## What matched
`ui:toolbar:solid-modify-shortcut-hints` (Q, F, M, Del, A, Ctrl+B on the same rows); the MODIFY
entries themselves and their order from Press Pull to Bill of Materials; Draft completes, adds a
timeline icon and its timeline tooltip names it; the Parameters dialog's layout (filter row,
toolbar icons, Automatic Compute, columns, sections, OK) apart from the details in
`ui:dialogs:parameters-dialog-details`; Appearance's Close button and drag-or-click behaviour.

## How Fusion's Modify dialogs behave
As for CREATE: chips that say "Select" with the first one active, the prompt next to the cursor,
OK greyed until the picks are complete and any value is non-zero, value rows (Angle, Scale Factor,
Distance) appearing only after the picks with a manipulator and a focused floating box. Picked
bodies turn blue as a whole. Cancel leaves nothing selected. FissionOxide's Modify dialogs open
with every row, preset planes and non-zero defaults, enable OK early (Replace Face with a red
error line), pick faces where Fusion picks bodies, and keep their picks selected after Cancel.

## Differences
P1: `ui:dialogs:draft-rows`, `ui:dialogs:draft-edit` ("This feature has no editable properties
yet"), `ui:dialogs:scale-rows` (all bodies scaled x2 at open), `ui:dialogs:combine-rows`
(supersedes the old `ui:dialogs:combine-fields` match), `ui:dialogs:split-face-rows`,
`ui:dialogs:split-body-rows` (body pick not accepted), `ui:dialogs:physical-material-palette`,
`ui:dialogs:cancel-clears-selection`, `ui:dialogs:body-pick-highlight`.

P2: `ui:toolbar:solid-modify-menu-list` (extra separators, Direct Edit submenu, Simplify order),
`ui:toolbar:solid-modify-menu-tooltips`, `ui:dialogs:compute-all-message`,
`ui:dialogs:parameters-dialog-details`, `ui:toolbar:responsive-collapse` (reopened),
`ui:dialogs:offset-face-rows`, `ui:dialogs:replace-face-rows`, `ui:dialogs:silhouette-split`
(fox-missing), `ui:dialogs:align-rows`, `ui:dialogs:remove-dialog`,
`ui:dialogs:remove-features-dialog` (opens Remove Faces), `ui:dialogs:replace-with-primitives`
(fox-missing), `ui:dialogs:appearance-library`, `ui:dialogs:manage-materials` (fox-missing),
`ui:dialogs:preview-browser-unchanged`.

Fusion's rows, as seen: DRAFT Type (Fixed Plane / Parting Line icons), Pull Direction, Faces,
Tangent Chain, [Flip Pull Direction, Angle], Draft Sides (One Side / Two Side / Symmetric). SCALE
Entities, Point, Scale Type (Uniform / Non Uniform), [Scale Factor]. COMBINE Target Body, Tool
Bodies, Operation (3 icons), Keep Tools. OFFSET FACE Faces, [Distance]. REPLACE FACE Source Faces,
Tangent Chain, Target Faces. SPLIT FACE Faces to Split, Splitting Tool(s), Chain Selection, Split
Type (Split with Surface / Along Vector / Closest Point), Extend Splitting Tool. SPLIT BODY Body to
Split, Splitting Tool(s), Extend Splitting Tool(s). SILHOUETTE SPLIT View Direction, Target Body,
Operation (Split Shelled Body). ALIGN Object (Bodies...), From, To. REMOVE Objects. REMOVE
FEATURES Bodies, Features (Select All, Fillet, Hole, Chamfer, Extrude, Revolve, Other), Close.
PHYSICAL MATERIAL and APPEARANCE are palettes with a Close button; Manage Materials is a
"Material Browser" window. Rows in brackets appear after the picks.

## Retractions
None.

## Batches (wave 24)
| batch | items | keys |
|---|---|---|
| `ui-modify-menu-tooltips` | 5 | solid-modify-menu-list, solid-modify-menu-tooltips, compute-all-message, parameters-dialog-details, responsive-collapse (reopened) |
| `ui-draft-scale-dialogs` | 4 | draft-rows, draft-edit, scale-rows, offset-face-rows |
| `ui-combine-split-dialogs` | 5 | combine-rows, replace-face-rows, split-face-rows, split-body-rows, silhouette-split |
| `ui-align-remove-simplify` | 4 | align-rows, remove-dialog, remove-features-dialog, replace-with-primitives |
| `ui-material-appearance` | 3 | physical-material-palette, appearance-library, manage-materials |
| `ui-dialog-selection-state` | 3 | cancel-clears-selection, body-pick-highlight, preview-browser-unchanged |

## Not covered
Delete (key and menu, with and without a selection); Bill of Materials (MANAGE area); the rows of
Draft's Parting Line type, Draft Sides Two Side / Symmetric, Scale Non Uniform, Split Face's other
types, Combine Cut / Intersect, Align after its picks, Replace Face and Split Body completed;
results, timeline icons, undo names and right-click menus of every command but Draft; dragging a
material or appearance onto a body; Fusion's Direct Edit mode (history off). The "Direct Edit"
coverage item has no Fusion menu entry with history on; it belongs with the timeline's
"Do not capture Design History" area.
