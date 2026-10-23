# Re-verification sweep, verifier 5: toolbar, shortcuts, documents, menus, workspaces and the other tabs

2026-10-22, 20:45 to 21:20. `main` at `c6232d9f`, rebuilt (`cargo build --release -p fissionoxide`, built on
Windows without changes). FissionOxide at screenshot x 0..907 with the Data Panel closed; Fusion at x 0..920
behind it, looked at for the SOLID ribbon at 920 px, the workspace list, the CONSTRUCT list, the document tab
widths and an In-canvas Render on its empty "Untitled(2)" Assembly tab (started, stopped, back to DESIGN).
Nothing in Fusion was saved or changed apart from that.

## Counts

| Area | Re-checked | Verified | Reopened | Left |
|---|---|---|---|---|
| toolbar | 9 | 7 | 2 | 0 |
| shortcuts | 10 | 9 | 1 | 0 |
| documents | 7 | 6 | 1 | 0 |
| menus | 4 | 3 | 1 | 0 |
| workspaces | 4 | 3 | 1 | 0 |
| render | 12 of 14 | 11 | 1 | 2 (texture map controls, flatten ground) |
| manage | 2 | 1 | 1 | 0 |
| utilities | 5 | 5 | 0 | 0 |
| plastic | 1 | 1 | 0 | 0 |
| animation | 5 | 5 | 0 | 0 |
| electronics | 4 | 3 | 1 | 0 |
| manufacture | 6 | 3 | 3 | 0 |
| surface | 19 | 18 | 1 | 0 |
| sheet | 18 | 17 | 1 | 0 |
| **total** | **106** | **92** | **14** | **2** |

Plus three new rows. Not swept: mesh (21 keys; needs an STL inserted through the file dialog), the seven
dialogs keys `ui-reverify-dialogs-2` merged during this session, and the singles (marking-menu flick, manip
drag snap, timeline component owner, chrome comments, context-menus face list, status sketch-curve readout,
sketch vertical-on-horizontal, assemble convert design type). 38 keys remain `fixing-merged`.

## What still differs (wave-41 batches)

`ui-reverify-render-1`
- P1 new `ui:render:in-canvas-ground-slab`: In-canvas Render draws a black / dark grey ground slab (filling an
  empty design's canvas); Fusion's stays plain grey.
- P2 new `ui:render:design-ground-look`: with a body, the DESIGN canvas draws a large light-grey ground quad and
  a hard dark shadow; Fusion's body sits on white with no visible ground. A click on the quad does not clear
  the selection.
- `ui:render:scene-settings-dialog`: Focal Length 45 mm (Fusion 90 mm); an extra "0 deg" field by Position.

`ui-reverify-workspaces-1`
- `ui:manage:bom-palette`: no table header row or horizontal scrollbar; the status bar draws over Close.
- `ui:manufacture:cam-browser`: no CAM display icons on the navigation bar.
- `ui:manufacture:milling-panels`: Solid Holder has no submenu arrow.
- `ui:manufacture:setup-dialog`: Machine is a dropdown, not a "Select..." button; no Operation Type icon.
- `ui:electronics:new-document-tab`: the new electronics tab is "Untitled(4)" where the row says "Untitled"
  (check Fusion with other Untitled tabs open).
- new `ui:electronics:schematic-close-prompt`: closing an untouched schematic asks to save; Fusion does not.

`ui-reverify-toolbar-1`
- `ui:toolbar:responsive-collapse`: no New Component in SOLID CREATE's quick icons at 907 px.
- `ui:toolbar:workspace-menu-style`: GENERATIVE DESIGN / SIMULATION black with a padlock; Fusion greys them with
  a blue subscription badge.
- `ui:workspaces:menu-row-style`: regular weight, 24 px rows, 133 px list (Fusion bold, 26 px, about 116 px).
- `ui:menus:construct-icons`: all amber; Fusion's mix green and orange by kind.
- `ui:documents:tab-overflow-menu`: tabs shrink to about 109 screenshot px (TAB_MIN_W 150 is screen px; Fusion's
  tabs are about 147 screenshot px); grey list icons.
- `ui:shortcuts:sketch-keys-from-3d`: O gives the plane prompt but no OFFSET dialog.

`ui-reverify-dialogs-5`
- `ui:surface:sweep-loft-missing`: LOFT's Chain Selection checked, Guide Type one icon.
- `ui:sheet:flange-dialog-open`: the list's remove button draws a missing-glyph box; EDGE FLANGE's header bar
  stops short of the right edge.

## Seen on the way (not rows)
- Patch on one straight box edge shows the kernel text "patch: gp_Dir() - input vector has zero norm".
- The surface Extrude and Revolve status bar reads "1 Profile | Area ..." / "1 Edge 1 Profile" where Fusion's rows
  say "Chain" / "2 selections".
- Fusion's in-canvas progress bar has no panel behind it (labels in small chips); FissionOxide's sits in a white
  panel (noted on `ui:render:in-canvas-progress`, verified).
- A synthetic key press once went to Explorer (tier click) and was refused; clicking FissionOxide's title bar
  fixed it.

## Where things stand
FissionOxide (one instance, rebuilt at `c6232d9f`) has five tabs: "Untitled*" (Box, Sketch2/3, render light),
"Untitled(1)*" (sheet metal plate with an edge flange, flat pattern, unfold / refold), "Untitled(2)",
"Untitled(3)*" (box, SurfacePatch1, a ruled sheet, a sketch on the front face). Fusion untouched apart from the
In-canvas Render start / stop on "Untitled(2)" and the dropdowns opened.
