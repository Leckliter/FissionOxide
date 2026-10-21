# Prober progress

## Last stop: 2026-10-21, 09:00, session "parity hands-on" (the first prober)

Finished: chrome, toolbar at full and narrow width, ViewCube and navigation bar, marking menus
on canvas/face/edge, browser rows and their menus, timeline menu, Preferences layout, the basic
sketch flow (rectangle, circle, line, trim, offset, polygon, dimension edit, Horizontal), and
the Extrude, Fillet, Chamfer, Shell, Press Pull, Move and Hole dialogs. Reports:
`parity/reports/2026-10-21.md` and `parity/reports/2026-10-21-sketch-and-modify.md`.
Batches: waves 4 and 5 (`map-wave4.json`, `map-wave5.json`), 12 batches, 56 rows.

## Next prober: start here

1. Take "Selection: hover/selected colours ..." under *Design workspace: canvas* in
   `parity/coverage.md`. The owner asked for hover features, colour changes on selection,
   hover and action states to be covered as carefully as commands. Sample the colours on screen
   (zoom, read the pixel values) for face, edge, vertex, body, sketch curve, sketch point and
   construction plane in both apps, hovered and selected, and in a command's selection mode.
2. Then "Every CREATE tool" under *sketch*.
3. Re-verify in the app any key in `ui-features.jsonl` marked `fixing-merged` when a fixer
   has pushed (check `parity/batches/index.jsonl`).

## Traps
- The Claude overlay window on the right of the screen (see `LOOP.md`, Setup 2).
- Clicking the taskbar button of the active window minimises it.
- FissionOxide's dialogs remember their last position; drag them left if they open under the
  overlay, or use Enter/Escape.
- Fusion in this account is in inches with Layout Grid off in 3D; FissionOxide is in mm.
- Fusion's running document has a chamfer-less test body with Sketch1-3; FissionOxide's has
  its own. Make a new design in each when a clean start matters (Ctrl+N in FissionOxide, the
  `+` tab in Fusion).
