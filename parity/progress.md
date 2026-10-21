# Prober progress

## Last stop: 2026-10-21, 10:30, session "parity hands-on" (first prober, third pass)

Finished: chrome, toolbar, ViewCube and navigation bar, marking menus, browser and timeline
menus, Preferences layout, the basic sketch flow, Extrude/Fillet/Chamfer/Shell/Press
Pull/Move/Hole dialogs, and the selection and hover colours (face, edge, body, sketch curve on
a face, Fillet and Extrude pick states). Reports: `parity/reports/2026-10-21.md`,
`2026-10-21-sketch-and-modify.md`, `2026-10-21-selection-colors.md`. Batches: waves 4, 5 and 6
(13 batches, 64 rows); twelve cloud fixers are running on waves 4-5 and one is requested for
wave 6.

## Next prober: start here

1. "Every CREATE tool" under *Design workspace: sketch* in `parity/coverage.md`: each tool's
   heads-up boxes, cursor prompt, preview, glyphs, Feature Options and the created geometry's
   dimensions, in both apps.
2. Then the selection remainder listed on the coverage line: vertex hover/selection,
   construction planes and axes, sketch points, in-sketch hover colours, selection filters,
   window/crossing box colours, Select Other.
3. Re-verify in the app any key marked `fixing-merged` once fixers have pushed
   (`parity/batches/index.jsonl`).

## Traps
- The Claude overlay window on the right of the screen (see `LOOP.md`, Setup 2). Keep each app
  window inside x < 1370 screen pixels (restore and drag, or `Win+Left`).
- Clicking the taskbar button of the active window minimises it.
- FissionOxide's dialogs remember their last position; drag them left if they open under the
  overlay, or use Enter/Escape.
- Fusion is in inches with Layout Grid off in 3D; FissionOxide is in mm.
- To read colours, rest the pointer two seconds and sample pixels (`System.Drawing`
  `CopyFromScreen`/`GetPixel` from PowerShell); Fusion's hover lightening is invisible in a
  scaled screenshot.
- Fusion's current document has Sketch1-3 and a body with fillets; FissionOxide's has a box with
  a circle/offset/line sketch on top. Start a new design in each when a clean body matters.
