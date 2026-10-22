# Parity report: keyboard shortcuts (prober 19, 2026-10-22)

FissionOxide built from `main` at `c3d74430`, window 908 px wide at the left of the screen; Fusion's
"Untitled*" Part Design (inches, Body1, Sketch1..4, then Sketch5 made by this session on the body's end
face). Every letter was pressed on the canvas of both apps in three states: 3D with nothing selected,
3D with a planar face selected (A few keys), and inside an open sketch. Then F6, Home, Ctrl+B, Ctrl+A,
Shift+S, Shift+J, Shift+N, 1/2/3, Ctrl+N and closing an unsaved tab.

## Fusion's list of shortcuts

Fusion has no window that lists the default keys: the Help menu (Take a Tour, Learning and
Documentation, Quick Setup, Autodesk Assistant Shift+H, Community, Support and Diagnostics, Check for
Updates, What's New, Release Notes, About) has no Keyboard Shortcuts entry and Preferences has no
shortcut page. The keys show as hints on the menu rows and are changed per command
(`ui-toolbar-customize-2`). The list below is what pressing the keys did.

| Key | Fusion, 3D | Fusion, in a sketch | FissionOxide |
|---|---|---|---|
| A | Appearance | Appearance over the sketch; Escape returns to it | Appearance; from a sketch it finishes the sketch |
| C, L, R, T, P | plane prompt, then the sketch tool; with a face selected a sketch on it | Circle, Line, Rectangle, Trim, Project | nothing in 3D; same tools in a sketch |
| O | OFFSET dialog with a plane prompt | Offset | nothing in 3D |
| D | nothing | Sketch Dimension | same |
| E, F, H, Q | Extrude, Fillet, Hole, Press Pull | same, over the open sketch | same; from a sketch they finish it |
| I | Measure | Measure in the sketch view | same |
| J, Shift+J | nothing (Part Design) | nothing | JOINT dialog |
| M | Move/Copy | sketch Move/Copy | same |
| S | Design Shortcuts toolbox | same | same |
| Shift+S | Scripts and Add-Ins | | the S toolbox |
| Shift+N | Display Component Colors on/off | | nothing |
| V | hides the selected body, keeps it selected; nothing selected: silent | silent | hides and deselects; nothing selected: lower-left message |
| X | nothing | toggles the palette's Construction linetype (nothing selected) | nothing |
| F6 | fit | | fit |
| Home | nothing | | home view |
| Ctrl+N | new Part Design tab "Untitled(4)" | | new tab "Untitled" |
| B G K N U W Y Z, Ctrl+B, Ctrl+A | nothing visible | nothing | nothing |

## Matches
3D keys A, E, F, H, I, M, Q, S and the silent letters; sketch keys L, R, D, M, O, P, T, I; F and Q
with a face selected take the face; F6; Ctrl+N's new tab; Escape closes Appearance. Rows
`ui:shortcuts:letter-keys-3d`, `ui:shortcuts:letter-keys-sketch`, `ui:shortcuts:face-preselect`,
`ui:shortcuts:f6-fit`, `ui:shortcuts:ctrl-n-new-tab`, `ui:shortcuts:escape-closes-appearance`.

## Differences
P1
- `ui:shortcuts:sketch-keys-from-3d`: C, L, R, T, P, O start their sketch tool from 3D in Fusion; nothing in FissionOxide (`lib.rs:1240`).
- `ui:shortcuts:sketch-key-on-face`: a sketch key with a planar face selected sketches on that face.
- `ui:shortcuts:3d-command-keeps-sketch`: A/E/F/H/Q from a sketch keep it open and Escape returns to it; FissionOxide finishes the sketch (`lib.rs:1447`).

P2
- `ui:shortcuts:x-construction-mode`, `ui:shortcuts:j-in-part-design`, `ui:shortcuts:home-key`,
  `ui:shortcuts:v-nothing-selected`, `ui:shortcuts:v-keeps-selection`, `ui:shortcuts:shift-s-scripts`,
  `ui:shortcuts:shift-n-component-colors`, `ui:chrome:help-menu-current`.
- Documents, seen on the way: `ui:documents:tab-dirty-star`, `ui:documents:new-tab-numbering`,
  `ui:documents:close-unsaved-save-dialog` (Fusion opens its Save dialog when closing a never-saved tab).

Prompt wording differences noted in the match rows (Hole, Move, Offset) were left unbatched.

## Verified
`ui:chrome:new-tab-replaces-design` (Ctrl+N and "+" keep the first design), `ui:sketch:move-copy-shortcut`
(M in a sketch; the MODIFY hint not looked at), `ui:toolbar:shortcuts-toolbox-style`.

## Batches (wave 34, `parity/batches/map-wave34.json`)
| Batch | Items | Files |
|---|---|---|
| ui-shortcut-sketch-scope | 4 (3 P1) | `crates/ui/src/lib.rs` key dispatch |
| ui-shortcut-extra-keys | 7 | `lib.rs` (`toggle_visibility`, Home), `menus.rs`, `customize.rs`, `panels.rs` Help, `help.rs` |
| ui-documents-tabs-close | 3 | `crates/ui/src/documents.rs` |

The first two both edit `crates/ui/src/lib.rs` (different functions): start one first or tell the
second to rebase.

## Not covered
Delete / Backspace, Ctrl+C / Ctrl+V / Ctrl+Shift+V, Ctrl+Z / Ctrl+Y, Ctrl+S / Ctrl+O, Ctrl+Shift+F and
the visual-style Ctrl keys, 1/2/3 in FissionOxide (Fusion's SELECT icon changed), Shift+F, Shift+H,
Ctrl+/, function keys other than F6, Space / Enter repeat (probed earlier), keys on the SURFACE, MESH,
SHEET METAL and other workspaces' tabs, keys with an edge or a body selected beyond V, and
customised keys. `ui:manip:*`, `ui:inspect:measure-two-picks` and the other `fixing-merged` keys from
`progress.md` were not re-checked.

## Retractions
None.
