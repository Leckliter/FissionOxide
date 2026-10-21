# Prober progress

## Last stop: 2026-10-21, 18:00, session "Parity prober 4" (every sketch MODIFY tool)

Finished: every sketch MODIFY tool (Fillet, Chamfer and its submenu, Blend Curve, Offset on a
circle and a line with the Direction list, Trim by drag, Extend, Break, Sketch Scale, Move/Copy,
Change Parameters) with tooltips, prompts, heads-up boxes, previews and results. Report
`parity/reports/2026-10-21-sketch-modify-tools.md`; batches `map-wave9.json`
(`ui-sketch-fillet-chamfer-blend`, `ui-sketch-offset-value-box`, `ui-sketch-trim-extend-break`,
`ui-sketch-scale-move-params`, `ui-sketch-modify-tooltips`, 20 rows), six match rows, two keys
verified and one reopened. Earlier: chrome, toolbar, ViewCube and navigation bar, marking menus,
browser and timeline menus, Preferences, the basic sketch flow, the first-pass feature dialogs,
face/edge/body colours, every sketch CREATE tool, the selection remainder (reports in
`parity/reports/`).

## Next prober: start here

1. The CREATE leftovers (coverage line under *sketch*, "Every CREATE tool"): 2-Tangent and
   3-Tangent Circle, Text after the frame is placed, Mirror and Circular/Rectangular Pattern
   previews, angular and diameter dimensions. Then "Every constraint" (mark it `in progress`
   first). Both apps are in a sketch with a rectangle (filleted top-left, chamfered bottom-left),
   an arc on the right edge, a blend spline and two short lines below, and a horizontal line into
   the left edge; Fusion also has an offset circle. Draw fresh geometry left of x = 450 (screenshot
   px) or press Ctrl+Z a few times; the Sketch Palette covers Fusion's canvas from x = 730.
2. Two things worth a second look before writing rows (see the report's "Not conclusive"):
   Fusion's Trim drag removed a whole circle that a line crossed (was it the offset constraint?),
   and Blend Curve's choice of ends (try picking the vertical line near its top).
3. Re-verify in the app any key marked `fixing-merged` once fixers have pushed
   (`parity/batches/index.jsonl`); `ui:sketch:offset-dimension` could not be checked this time.

## Traps
- Tool calls run one after another, never in parallel: a Bash pixel sample always runs after the
  whole computer_batch before it. Sample colours in their own turn while the pointer rests
  (`scratchpad/px.ps1`: quote the arguments, `"x,y"` or `"x,y,scanh,n"`), or read flat fills
  from a scale-1 zoom JPEG with `System.Drawing` (`jpg.ps1`). Fusion's rubber-band box is in an
  overlay the sampler never sees; zoom JPEGs do.
- Screen px = screenshot px x 1.3187 (1920/1456). `GetCursorPos` in the sampler confirms it.
- The Claude overlay window on the right of the screen (see `LOOP.md`, Setup 2): keep clicks
  below x = 1040 screenshot px. The overlay hides while desktop control runs, so it is not in
  screenshots, but its region still swallows clicks.
- `request_access` refuses "fissionoxide.exe" while the app is not running: build, launch
  (`Start-Process target\release\fissionoxide.exe`), then request. A fresh instance opens with the
  Data Panel open and the window straddling the overlay: drag the title bar left by 65 px and
  close the Data Panel with the grid icon. The previous instance's unsaved document is gone.
- Only one session can hold desktop control; if `request_access` says another session is using
  the computer, the user must stop that session first.
- Clicking the taskbar button of the active window minimises it. Fusion is the orange "F"
  button (497,804 screenshot), FissionOxide the one right of it (535,804).
- Fusion: the big SELECT icon toggles the Select tool, the "SELECT" label opens the dropdown.
  Submenus open to the right under the overlay region when the menu is wide: hover them, never
  click there (the MODIFY > Chamfer submenu at x = 330..460 is safe). Wheel down zooms in. Body
  corners carry sketch points that win over vertices: hide the Sketches folder to pick a vertex.
  Origin children show only when the Origin folder's eye is also on. The Sketch Palette sits at
  x = 730..915, y = 200..600 and eats canvas clicks under it. Single-letter shortcuts need the
  pointer over empty canvas. With a curve pre-selected, O opens OFFSET at 0.00 in: type the
  distance.
- FissionOxide: the sketch Fillet/Trim/etc. prompts follow the pointer; the Parameters window
  stays open behind other commands and swallows clicks until its x is clicked; the floating value
  box may keep a stale select-all range (`ui:sketch:value-box-stale-selection`), so check the
  dialog field after typing.
- Fusion is in inches with Layout Grid off in 3D; FissionOxide is in mm. Ctrl+Z in sketch mode
  undoes one curve at a time in both apps.
- `git pull --rebase` refuses while another session holds uncommitted edits in `crates/`; commit
  the parity files, then rebase and push from a temporary worktree
  (`git worktree add <scratchpad>/wt origin/main`, cherry-pick, push, remove).
