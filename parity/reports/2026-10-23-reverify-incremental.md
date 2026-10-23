# 2026-10-23: incremental re-verification (verifier 7)

Built `main` at `905a4701` on Windows as is (no build fix needed) and re-checked every key the wave-38..42
fixers had marked `fixing-merged` since verifier 6, area by area in the running app, from each row's
`fusion` text, with Fusion side by side wherever the text left room for doubt. Both windows were snapped
left; Fusion will not go narrower than 1215 px (about 920 screenshot px), so FissionOxide was widened to
the same width for the toolbar and tab comparisons. Nothing was saved in either app.

36 keys checked: 29 verified, 7 reopened, 1 new row. Four keys stay `fixing-merged` because they cannot be
checked here (`ui:marking-menu:flick`, `ui:chrome:comments-expand`,
`ui:sketch:vertical-on-horizontal-line`, `ui:timeline:feature-owner-component`; see `progress.md`).

## Verified (29)

- chrome: Preferences General defaults, window size (759 x 468 vs Fusion's 748 x 447) and General
  expanded, Unit and Value Display's 0.123, the narrow profile menu (113 vs 108 px), no Unsaved banner on Home.
- toolbar and menus: the SOLID row at Fusion's width (CREATE ends with New Component in both; this Fusion
  has Extrude unpinned), the workspace list's row height and width, CONSTRUCT icons in orange and green.
- canvas and sketch: plane hover and grid preview everywhere during plane picks, Fillet / Extrude hover and
  pick colours, Distance and Angle chamfer's distance dimension, Project / Include list and its Spun
  Profile / Isoparametric Curve dialogs, Circular Pattern's centre and status, 3D Sketch's off-plane start
  with its triad.
- solid: Box's temporary sketch, typed base and grey preview, Coil's taper handle (12 deg), Draft's drag to
  a failing angle (responsive, Cancel works), the Error window's icon slots, Measure's two-pick layout and
  status, Replace Face keeping a two-lump body whole.
- other tabs and workspaces: Flange dialog, BOM palette's empty state, the electronics tab and both close
  prompts, Manufacture's SETUP list and Solid Holder, CAM browser, SETUP dialog, Scene Settings (90 mm,
  Position icon), in-canvas render without the dark slab, Design's faint ground shadow.

## Reopened (7) and new (1)

| Key | What still differs | Batch |
|---|---|---|
| `ui:toolbar:workspace-menu-style` | subscription badge: Fusion's is a blue circle with a white up arrow, right-aligned in one column; FissionOxide draws a plain dot after the name | ui-reverify-chrome-3 |
| `ui:documents:tab-overflow-menu` | four tabs fit Fusion's 1215 px window (about 142 screenshot px each); FissionOxide's 196 px minimum scrolls the fourth and shows the 'v' | ui-reverify-chrome-3 |
| `ui:shortcuts:sketch-keys-from-3d` | after L and a plane pick the sketch opens with no tool running | ui-reverify-shortcuts-1 |
| `ui:shortcuts:offset-from-3d-dialog` | the waiting OFFSET has an extra 'Distance 0 mm' row | ui-reverify-shortcuts-1 |
| `ui:dialogs:replace-face-plane-target` | a construction plane picks from its browser row but not from the canvas while Target Faces is active | ui-reverify-dialogs-7 |
| `ui:surface:sweep-loft-missing` | surface Loft's Rails / Centerline chip is greyed (no rails support) | ui-reverify-dialogs-7 |
| `ui:manufacture:navbar-cam-icons` | Fusion adds six CAM icons (Toolpath, Synchronize active setup, Stock, Machine, Tool, Turning profile visibility) and a narrow check-box Tool fly-out; FissionOxide adds Tool Visibility only, its fly-out 450 px wide | ui-reverify-workspaces-2 |
| `ui:render:in-canvas-body-shading` (new, P1) | In-canvas Render shades default bodies near black with hard long shadows; Fusion's are bright reflective steel with faint soft shadows | ui-reverify-workspaces-2 |

Keys in `parity/batches/map-wave44.json`; the four batches are `ready` in `index.jsonl`.

## Noted, not recorded

- In RENDER (before In-canvas Render) Fusion already draws reflective steel with long environment shadows,
  where FissionOxide keeps the plain shaded view; `ui:render:entry-state` was verified earlier against
  "keeps the ordinary shaded view", so worth a second look by the next prober in RENDER.
- The Error window's message is FissionOxide's own ('draft: the draft failed'), not Fusion's wording.

## Where things stand

Fusion: untouched apart from O / OFFSET cancelled, the CONSTRUCT list opened, Manufacture and RENDER (with
In-canvas Render started and stopped) visited and back to DESIGN on 'Untitled*(1)', and the window snapped
left (x 0..1215). FissionOxide (`905a4701`, one instance) at x 0..1215 (widened from the snap), tabs
'Untitled', 'Untitled(1)', 'Untitled(2)', 'Untitled(3)*' (two boxes joined into Body1, a boss, Plane1,
three sketches); the electronics tab was closed with Don't Save.
