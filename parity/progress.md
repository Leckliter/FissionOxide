# Prober progress

## Last stop: 2026-10-21, 11:45, session "Parity prober 2" (every sketch CREATE tool)

Finished: chrome, toolbar, ViewCube and navigation bar, marking menus, browser and timeline
menus, Preferences layout, the basic sketch flow, Extrude/Fillet/Chamfer/Shell/Press
Pull/Move/Hole dialogs, selection and hover colours (face, edge, body, sketch curve on a face),
and every sketch CREATE tool (Midpoint Line, 3-point and center rectangles, 3-point circle,
center-point and tangent arcs, inscribed and edge polygons, ellipse, all five slots, both
splines, conic, point, text, mirror, both patterns, Project / Include, sketch dimension on a
line and an arc). Reports: `parity/reports/2026-10-21.md`, `2026-10-21-sketch-and-modify.md`,
`2026-10-21-selection-colors.md`, `2026-10-21-sketch-create-tools.md`. Batches: waves 4-7
(16 batches, 87 rows); cloud fixers run on waves 4-5 and were requested for waves 6-7.

## Next prober: start here

1. The selection remainder on the coverage line under *Design workspace: canvas*: vertex
   hover/selection, construction planes and axes, sketch points, in-sketch hover colours,
   selection filters and priority, window/crossing box colours, Select Other. Sample colours with
   the pixel trick below.
2. Then "Every MODIFY tool" under *sketch* (fillet, trim by drag, extend, break, offset chain and
   both sides, scale, move/copy, change parameters), and the CREATE leftovers: 2-/3-Tangent
   Circle (pick the lines exactly on the stroke; a miss shows "Click a curve" in FissionOxide),
   Text after the frame drag, Mirror and pattern previews, angular and diameter dimensions.
3. Re-verify in the app any key marked `fixing-merged` once fixers have pushed
   (`parity/batches/index.jsonl`).

## Traps
- The Claude overlay window on the right of the screen (see `LOOP.md`, Setup 2). Keep each app
  window inside x < 1370 screen pixels (restore and drag, or `Win+Left`).
- Only one session can hold desktop control; if `request_access` says another session is using
  the computer, the user must stop that session first. The approval dialog may be denied by
  accident: ask once, then wait.
- Clicking the taskbar button of the active window minimises it. On this taskbar Fusion is the
  orange "F" button and FissionOxide the one right of it.
- Submenu rows are 15-16 px apart in both apps; zoom the open submenu and compute each row's
  y before clicking, or the click lands on the neighbour (Project vs Intersect).
- FissionOxide's dialogs remember their last position; drag them left if they open under the
  overlay, or use Enter/Escape. In sketch mode Ctrl+Z undoes one curve at a time in both apps,
  which is the quickest way to clear the canvas between tools.
- Fusion is in inches with Layout Grid off in 3D; FissionOxide is in mm.
- To read colours, rest the pointer two seconds and sample pixels (`System.Drawing`
  `CopyFromScreen`/`GetPixel` from PowerShell); Fusion's hover lightening is invisible in a
  scaled screenshot.
- Both documents now have an unsaved Sketch3 on a vertical plane with slots and a spline (the
  Fusion one is still in sketch mode). Start a new design in each when a clean body matters.
