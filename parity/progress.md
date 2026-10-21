# Prober progress

## Last stop: 2026-10-21, 18:10, session "Parity prober 4" (sketch MODIFY tools, CREATE leftovers)

Finished: every sketch MODIFY tool (Fillet, Chamfer and its submenu, Blend Curve, Offset on a
circle and a line with the Direction list, Trim by drag, Extend, Break, Sketch Scale, Move/Copy,
Change Parameters) with tooltips, prompts, heads-up boxes, previews and results; then the CREATE
leftovers (2-/3-Tangent Circle, angular and diameter dimensions, Text after the frame, Mirror,
Circular and Rectangular Pattern). Reports `parity/reports/2026-10-21-sketch-modify-tools.md` and
`parity/reports/2026-10-21-sketch-create-leftovers.md`; batches `map-wave9.json` (five batches,
20 rows) and `map-wave10.json` (`ui-sketch-tangent-circles-dims`, `ui-sketch-text-mirror-patterns`,
10 rows); eleven match rows; `ui:sketch:trim-hover-color`, `ui:dialogs:offset-sketch-fields` and
`ui:sketch:diameter-glyph` verified; `ui:dialogs:move-set-pivot` reopened. Earlier: chrome,
toolbar, ViewCube and navigation bar, marking menus, browser and timeline menus, Preferences, the
basic sketch flow, the first-pass feature dialogs, face/edge/body colours, every sketch CREATE
tool, the selection remainder (reports in `parity/reports/`).

## Next prober: start here

1. "Every constraint" under *sketch* (coverage line `todo`): coincident, collinear, concentric,
   midpoint, fix/unfix, parallel, perpendicular, horizontal/vertical, tangent, curvature, equal,
   symmetry, lock; glyph positions and hover; constraint deletion. Mark it `in progress` and commit
   first. Both apps are in a sketch on the XY plane with a horizontal line (300..430, y 520), a
   diagonal line below it, a short horizontal line at y 470, a 2-tangent circle between the first
   two lines, a 3-tangent circle, an angular and a diameter dimension; Fusion also holds the text
   "Fox", the earlier rectangle/arc/blend geometry to the right (under its Sketch Palette from
   x = 730) and an offset circle. FissionOxide's sketch also has a mirrored circle. Draw fresh
   geometry left of x = 450 (screenshot px) or Ctrl+Z.
2. Then "Sketch Palette: every option..." and "Heads-up input" (both `todo`), which the MODIFY and
   CREATE sessions touched only in passing: Fusion's floating value boxes have a vertical-ellipsis
   menu, select their value on appearance, and take Tab between boxes; none of that is recorded yet.
3. Two things worth a second look before writing rows (see the modify report's "Not conclusive"):
   Fusion's Trim drag removed a whole circle that a line crossed, and Blend Curve's choice of ends.
4. Re-verify in the app any key marked `fixing-merged` once fixers have pushed
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
  (`Start-Process target\release\fissionoxide.exe`), then request. `cargo build` fails with
  "Access is denied" while fissionoxide.exe runs: `Stop-Process -Name fissionoxide` first. A fresh
  instance opens with the Data Panel open and the window straddling the overlay: drag the title
  bar so the window sits at x = 0..1092 (screenshot px) and close the Data Panel with the grid
  icon at the top left; take a screenshot before clicking anything else, the window lands in a
  different place each launch. The previous instance's unsaved document is gone.
- Only one session can hold desktop control; if `request_access` says another session is using
  the computer, the user must stop that session first.
- Clicking the taskbar button of the active window minimises it (it happened once this session:
  a whole batch meant for Fusion ran in FissionOxide). Fusion is the orange "F" button (497,804
  screenshot), FissionOxide the one right of it (535,804). Check the window title in the first
  screenshot of every batch.
- Fusion: the big SELECT icon toggles the Select tool, the "SELECT" label opens the dropdown.
  Submenus open to the right under the overlay region when the menu is wide: hover them, never
  click there (CREATE > Circle at x = 260..420 and MODIFY > Chamfer at x = 330..460 are safe).
  Wheel down zooms in. Body corners carry sketch points that win over vertices: hide the Sketches
  folder to pick a vertex. Origin children show only when the Origin folder's eye is also on. The
  Sketch Palette sits at x = 730..915, y = 200..600 and eats canvas clicks under it. Single-letter
  shortcuts need the pointer over empty canvas; one Escape ends the Line tool. With a curve
  pre-selected, O opens OFFSET at 0.00 in: type the distance. Text's second frame corner is a
  click, not a drag release; typing before it fires shortcuts (F, O ...) and can rotate the view;
  recover with the ViewCube's FRONT face and the Fit button.
- FissionOxide: the sketch Fillet/Trim/etc. prompts follow the pointer; the Parameters window
  stays open behind other commands and swallows clicks until its x is clicked; the floating value
  box may keep a stale select-all range (`ui:sketch:value-box-stale-selection`), so check the
  dialog field after typing; the Mirror line pick needs the Mirror Line row clicked first; the
  Text frame needs a drag.
- Fusion is in inches with Layout Grid off in 3D; FissionOxide is in mm. Ctrl+Z in sketch mode
  undoes one curve at a time in both apps.
- `git pull --rebase` refuses while another session holds uncommitted edits in `crates/`; commit
  the parity files, then rebase and push from a temporary worktree
  (`git worktree add <scratchpad>/wt origin/main`, cherry-pick, push, remove). A rebase conflict
  in `ui-features.jsonl` is almost always both sides appending rows: keep both, drop the markers,
  check for duplicate keys.
