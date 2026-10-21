# Prober progress

## Last stop: 2026-10-21, 16:15, session "Parity prober 3" (selection remainder, sketch MODIFY menu)

Finished: the selection remainder (vertex, construction planes and axes, sketch points,
in-sketch hover and selected colours, selection filters/priority/tools lists, window and crossing
boxes, Select Other, status readouts) and the sketch MODIFY dropdown list. Report
`parity/reports/2026-10-21-selection-remainder.md`; batches `map-wave8.json`
(`ui-sketch-selection-feedback`, `ui-vertex-and-construction`, `ui-select-menus-and-other`,
18 rows) plus five reopened rows. Earlier: chrome, toolbar, ViewCube and navigation bar, marking
menus, browser and timeline menus, Preferences, the basic sketch flow, the first-pass feature
dialogs, face/edge/body colours, every sketch CREATE tool (reports in `parity/reports/`).

## Next prober: start here

1. "Every MODIFY tool" under *sketch* (coverage line marked `todo; menu lists compared`): Fillet,
   Trim by drag, Extend, Break, Offset (chain and both sides), Sketch Scale, Move/Copy, Change
   Parameters, plus Fusion's Chamfer submenu. Fusion is left in Sketch4 (XY plane, FRONT view) with
   a rectangle at about (560..700, 420..520) screenshot px and a short vertical line below it;
   draw a circle over the rectangle's right edge first (press C with the pointer over empty canvas,
   then click; a click on an existing edge may not register). FissionOxide has Sketch1 (XZ, a
   dimensioned rectangle) extruded into Body1; double-click Sketch1 in the browser to edit it.
   Fusion's Fillet: "Select first line or arc", then "Select second line or arc"; the second pick
   must be within 1 px of the stroke or the first pick clears.
2. Then the CREATE leftovers (2-/3-Tangent Circle, Text after the frame, Mirror and pattern
   previews, angular and diameter dimensions) and "Every constraint".
3. Re-verify in the app any key marked `fixing-merged` once fixers have pushed
   (`parity/batches/index.jsonl`).

## Traps
- Tool calls run one after another, never in parallel: a Bash pixel sample always runs after the
  whole computer_batch before it. Sample colours in their own turn while the pointer rests
  (`scratchpad/px.ps1`: `x,y,scanh,n` / `x,y,scanv,n` prints a run of pixels), or read flat fills
  from a scale-1 zoom JPEG with `System.Drawing` (`jpg.ps1`). Fusion's rubber-band box is in an
  overlay the sampler never sees; zoom JPEGs do.
- Screen px = screenshot px x 1.3187 (1920/1456). `GetCursorPos` in the sampler confirms it.
- The Claude overlay window on the right of the screen (see `LOOP.md`, Setup 2): keep clicks
  below x = 1040 screenshot px. The overlay hides while desktop control runs, so it is not in
  screenshots, but its region still swallows clicks.
- Only one session can hold desktop control; if `request_access` says another session is using
  the computer, the user must stop that session first.
- Clicking the taskbar button of the active window minimises it. Fusion is the orange "F"
  button (497,804 screenshot), FissionOxide the one right of it (535,804).
- A fresh `fissionoxide.exe` opens with the Data Panel open and the window straddling the
  overlay: drag the title bar to the top-left, close the Data Panel with the grid icon.
- Fusion: the big SELECT icon toggles the Select tool, the "SELECT" label opens the dropdown.
  Submenus open to the right (x > 1010) under the overlay region: hover them, never click there.
  Wheel down zooms in. Body corners carry sketch points that win over vertices: hide the Sketches
  folder to pick a vertex. Origin children show only when the Origin folder's eye is also on.
- FissionOxide: clicking a sketch curve selects it with no visual change (Delete still removes
  it). Vertices need SELECT > Selection Filters > Vertices. The sketch Fillet/Trim/etc. prompts
  are in the lower-left status text.
- Fusion is in inches with Layout Grid off in 3D; FissionOxide is in mm. Ctrl+Z in sketch mode
  undoes one curve at a time in both apps.
