# Parity run, 2026-10-21 (prober 3): selection remainder, sketch MODIFY menu

Both apps at their defaults, windows left of the Claude overlay. Fusion: the unsaved design with
Body1 (filleted top edges) and Sketch3; a new Sketch4 on the XY plane was left open with a
rectangle and a short line. FissionOxide: a fresh `main` build (548bc6c) with a new design, a
rectangle sketch on XZ extruded 10 mm. Colours are screen pixels (sRGB) read with
`System.Drawing` after the pointer rested two seconds; Fusion draws its rubber-band box in an
overlay the sampler cannot see, so the box colours come from the screenshot tool's zoom JPEGs.

## Colour table

| State | Fusion | FissionOxide |
|---|---|---|
| Sketch line plain / hovered / selected | 2 px (54,170,222) / 4 px (74,130,252) / 4 px (3,123,253) | 3 px (30,110,235) / 3 px (128,190,238) / unchanged |
| Sketch point (line end) plain / hovered / selected | 7 px dot (50,50,50) / hollow ring / filled (125,158,213) with dark ring | nothing drawn |
| Dimension text plain / hovered | (43,43,43), no box / blue (70,113,219) + tooltip "d9 = 0.4787837888 in" | (29,29,34) on a white box / box turns (232,242,252) with (30,136,229) border |
| Sketch canvas background | (255,255,255) | (241,242,244) |
| Window box (L->R) | peach (253,223,185), solid (170,152,195) border | light blue, solid blue border |
| Crossing box (R->L) | pale yellow (255,253,202), dashed (55,61,47) border | light green, solid green border |
| Body vertex hovered / selected | hollow ring, grey (100..167) / filled (147,167,185)..(191,214,237), dark ring | edge picked instead (Vertices filter off) |
| Origin plane plain / hovered / selected | (254,228,177) / (232,229,222) / (127,174,216) | (236,202,157) only while selected from the browser |
| Origin axis hovered / selected | solid blue (74,132,246) / (2,126,246) with halo | no axis rows, not pickable |

## What matched
- The SELECT dropdown's seven entries and 1/2/3 hints; the window (inside) versus crossing
  (touching) rule; click selects and Escape clears in 3D; clicking an origin plane row selects it.
- Fusion's Finish Sketch returns to the previous 3D view, as does FissionOxide's.

## Differences
18 new rows in three batches (`map-wave8.json`), five old `match` rows reopened:
- `ui-sketch-selection-feedback` (P1 x4, P2 x2): selected sketch curves are invisible and the
  status bar never names the pick; hover lightens instead of darkening; no marks at line ends or
  corners (nor hover/selected states for them); a crossing box over the right line deleted the top
  line; dimension label box and missing parameter tooltip; sketch background grey instead of white.
- `ui-vertex-and-construction` (P1 x3, P2 x2): vertices are not pickable by default; the vertex
  marker style; Origin folder rows (no X/Y/Z, no eyes, folder/child visibility rule); construction
  plane plain/hover/selected colours and status; axes and origin point not pickable.
- `ui-select-menus-and-other` (P1 x1, P2 x6): Selection Filters list (Fusion has 27 entries, all
  on), Selection Priority (3, no check mark), Selection Tools (By Name/Boundary/Size, Invert,
  Seed And Boundary), Select Other popup (Depth/Parents tabs, opens while holding, nothing selected
  on press, no Alt+click), "Multiple selections | 9" and "Area :" spacing, box colours, and the
  sketch MODIFY dropdown's five extra entries.

## Retractions and notes
- `ui:canvas:hover-vertex`, `ui:canvas:window-select`, `ui:canvas:select-other`,
  `ui:canvas:selection-filters` and `ui:canvas:hover-sketch` were recorded as `match` by the first
  prober; on screen they differ as above, so they are reopened and point at the new keys.
- Fusion's vertex could only be hovered after hiding the Sketches folder: every body corner carries
  a sketch point from the base sketch, and sketch points win over vertices.
- The Claude overlay was not visible in any screenshot this session (the app window hides while
  desktop control runs); the right-hand clicks were still kept below x = 1040 screenshot px.

## Not covered
- Selection Priority behaviour (Body/Edge priority hover), Freeform and Paint selection, Select
  Through, the Selection Tools commands themselves, Fusion's Chamfer submenu.
- Every sketch MODIFY tool's flow (Fillet, Trim drag, Extend, Break, Offset, Sketch Scale,
  Move/Copy, Change Parameters): only the menu lists were compared. Fusion's Fillet prompts
  "Select first line or arc", highlights the first pick with "1 Sketch Line | Length : 0.524 in" and
  prompts "Select second line or arc"; the second pick must land within 1 px of the stroke or the
  first pick is cleared.
