# Parity report: Workspaces (prober 18, 2026-10-22)

Both apps at 908 px wide, left of the Claude overlay. Fusion: the Part Design "Untitled*" (inches,
Body1, four sketches, construction planes) on the Personal licence. FissionOxide: `main` at
`2d74c694`, a new design with one CREATE > Box (34 x 20 x 10 mm). Each workspace was reached with the
DESIGN dropdown at the toolbar's left; its tabs, panels, dropdowns, the main dialog as it opens, the
browser and the canvas were compared. Fusion's raw notes per workspace are in the batch files.

## Matches
- The workspace list: the same eight entries in the same order, and "Change Workspace" on the button
  (`ui:workspaces:menu-entries`); DRAWING > From Design / From Animation (`ui:workspaces:drawing-submenu`).
- MILLING > DRILLING > Drill (`ui:manufacture:drilling-menu`).

## Blocked (Personal licence or a saved design)
- Generative Design and Simulation: greyed rows with a blue upgrade badge; a click opens "Upgrade to a
  Fusion subscription" and the workspace does not change (`ui:workspaces:generative-design`,
  `ui:workspaces:simulation`, `ui:workspaces:licence-badge`). FissionOxide opens both (with a padlock on
  the row); its Simulation toolbar is a row of text buttons, unlike every other Fusion toolbar, which
  is worth comparing if a licence ever allows it.
- Drawing > From Design on an unsaved design opens Fusion's Save dialog first (`ui:drawing:from-design`).
  Probers have not saved to the owner's cloud project; Drawing, Render's final render, the BOM and the
  ASSEMBLY row all wait on that.

## Differences
P1
- Render (`ui-render-workspace`): Fusion's SETUP has six quick icons and seven entries, an OPTICAL
  OVERRIDES panel, IN-CANVAS RENDER has Settings and Capture Image, there is no EXIT panel; the
  timeline gives way to a RENDERING GALLERY strip; entering opens nothing and leaves In-canvas Render
  off on a grey background (FissionOxide opens Appearance and turns In-Canvas Render on, a black ground
  plane fills an empty design); Scene Settings is a docked dialog with Settings / Environment Library
  tabs; In-canvas Render shows a progress bar (Excellent / Final / Infinite, elapsed time, iterations)
  and a stop button.
- Manufacture (`ui-manufacture-workspace`): six tabs against two; MILLING's and TURNING's panel and
  dropdown lists differ (no MANAGE / INSPECT / SELECT / PART HANDLING in FissionOxide, an extra
  MULTI-AXIS panel); the browser becomes a CAM Root and the timeline goes; New Setup is a tabbed dialog
  (Setup / Stock / Post Process) with WCS and Model rows, a triad and a translucent stock.
- Electronics (`ui-electronics-workspace`): Fusion opens a new document tab with a start page, then a
  schematic tab with an icon ribbon, browser, command line and PLACE COMPONENTS panel; FissionOxide
  switches the same tab to a text-button editor with a parts list. Its ELECTRONICS submenu has five
  entries against Fusion's two.
- Animation (`ui-animation-workspace`, fox-missing): FissionOxide prints "Animation isn't in
  FissionOxide yet". Fusion: ANIMATION tab (STORYBOARD, TRANSFORM, ANNOTATION, VIEW, PUBLISH), a
  Components-only browser, the ANIMATION TIMELINE with storyboards, Transform Components and New
  Storyboard dialogs.
- Extrude on a body face (`ui-extrude-body-face`): found while re-verifying. Fusion takes a body's
  planar face as the profile; FissionOxide highlights the box's face but Profiles stays "Select".

P2
- Workspace menu look (`ui-workspace-menu`): row height and weight, the blue fill on the current row,
  the rich per-workspace tooltips, and the small font of MANUFACTURE / ELECTRONICS on the button.
- Render Lights (New Light dialog) and Render's need for a saved design (in `ui-render-workspace`).

## Re-verified
- `ui:inspect:interference-dialog` and `ui:inspect:section-analysis-dialog`: verified (as opened).
- `ui:manip:*` (ui-extrude-manipulator): not re-verified; Extrude refused the box face (above).
- `ui:inspect:measure-two-picks`, the `ui-origin-display` keys, the timeline failed-item keys and the two
  browser keys from ui-document-settings-views: not reached.

## Batches (wave 33, `parity/batches/map-wave33.json`)
| Batch | Items | Files |
|---|---|---|
| ui-render-workspace | 7 | menus.rs, render_ui.rs, panels.rs (timeline) |
| ui-manufacture-workspace | 6 | menus.rs, panels.rs (tab_row, browser), forms.rs |
| ui-electronics-workspace | 5 | ecad_ui.rs, panels.rs (workspace_menu submenu) |
| ui-animation-workspace | 5 | panels.rs, menus.rs, dialogs.rs (new workspace) |
| ui-workspace-menu | 3 | panels.rs (workspace_row, button label) |
| ui-extrude-body-face | 1 | dialogs.rs, engine commands.rs |

`ui-workspace-menu`, `ui-electronics-workspace` and `ui-animation-workspace` all touch
`workspace_menu` / `switch_workspace` in `panels.rs`; `ui-render-workspace` and
`ui-manufacture-workspace` both touch `menus.rs` groups and the timeline in `panels.rs`. Run them in
two or three rounds, or tell later fixers to rebase.

## Not covered
- Fusion's Render dialog, the Rendering Gallery with images, Decal, Texture Map Controls and Optical
  Overrides dialogs; Fusion's turning, additive, inspection and fabrication setups and operation
  dialogs; the board editor, libraries and 3D PCB; Drawing from Animation; Animation's
  Auto Explode, Callout, View and Publish Video; hover states inside each workspace's ribbon.
- Keyboard shortcuts (the next `todo`).
