# Prober progress

## Last stop: 2026-10-21, 19:05, session "Parity prober 5" (sketch constraints, palette options, Line heads-up)

Finished: every CONSTRAINTS entry (Horizontal/Vertical, Coincident, Tangent, Equal, Parallel,
Perpendicular, Fix/UnFix, MidPoint, Concentric, Collinear, Symmetry, Curvature, Polygon) with the
menu tooltips and icons, prompts, picks, solver motion, glyph placement, hover, selection, Delete
and the glyph's right-click menu in Fusion; the Sketch Palette's Options rows (hover, every
checkbox, Options collapse, the chevron); the Line tool's heads-up boxes with Tab, typed values and
Enter. Report `parity/reports/2026-10-21-sketch-constraints.md`; batches `map-wave11.json`
(`ui-sketch-constraint-flow`, `ui-sketch-constraint-solver`, `ui-sketch-constraint-glyphs`,
`ui-sketch-palette-options`, `ui-sketch-line-heads-up`, 21 rows); five match rows; three verified
(`ui:sketch:heads-up-units-and-lock`, `ui:sketch:glyph-style-create-tools`,
`ui:sketch:palette-linetype`); two reopened (`ui:sketch:constraint-tool-flow`,
`ui:sketch:constraint-glyphs`). Earlier: chrome, toolbar, ViewCube and navigation bar, marking
menus, browser and timeline menus, Preferences, the basic sketch flow, the first-pass feature
dialogs, face/edge/body colours, every sketch CREATE and MODIFY tool, the selection remainder
(reports in `parity/reports/`).

## Next prober: start here

1. Finish "Sketch Palette" (coverage line `in progress`): Look At's effect in both apps (Fusion
   turns the camera to the sketch plane and the button shows a pressed state when already looking
   at it), Slice, 3D Sketch, Linetype's Centerline toggle on a selected line, and Feature Options per
   tool (start each CREATE/MODIFY tool and compare the palette's Feature Options section; earlier
   probers saw it only for Circle and Blend Curve). Then mark it `done`.
2. Finish "Heads-up input" (`in progress`): Rectangle, Circle, Arc, Polygon, Slot, Fillet, Offset
   and Dimension boxes; expressions ("40/2", "d1*2") and parameter names in a box; the
   vertical-ellipsis menu on Fusion's dimension edit box; what Enter does with an empty box.
3. Then "Sketch display" (`todo`): grid labels, profile shading colours in and out of the sketch,
   fully constrained colour (Fusion turns fully constrained curves black; `ui:sketch:fix-unfix-flow`
   has the green of fixed curves), construction and centerline styles, projected geometry colour,
   sketch points.
4. Re-verify in the app any key marked `fixing-merged` once fixers have pushed
   (`parity/batches/index.jsonl`); `ui:sketch:horizontal-conflict` and `ui:sketch:offset-dimension`
   have not been re-checked.

Where the documents stand: both apps are in a sketch on the XY plane. Fusion's test geometry is at
screenshot x 380..640, y 500..610 (a horizontal line, a vertical line joined to it, a circle on the
line's midpoint, a 1.00 x 40 degree line with a dashed reference); its Sketch Palette floats at
x 713..900, y 230..540 after being docked and undocked. FissionOxide's window was dragged 120 px
left (x -120..972) so its palette checkboxes at x 926 are clear of the overlay; its test geometry is
at x 600..810, y 540..630 plus a 30 mm line at 40 degrees near (520,300)..(600,250). Draw fresh
geometry below y 620 or Ctrl+Z.

## Traps
- Tool calls run one after another, never in parallel: a Bash pixel sample always runs after the
  whole computer_batch before it. Sample colours in their own turn while the pointer rests
  (`scratchpad/px.ps1`: quote the arguments, `"x,y"` or `"x,y,scanh,n"` / `"x,y,scanv,n"`), or read
  flat fills from a scale-1 zoom JPEG with `System.Drawing` (`jpg.ps1`). Fusion's rubber-band box
  is in an overlay the sampler never sees; zoom JPEGs do.
- Screen px = screenshot px x 1.3187 (1920/1456). `GetCursorPos` in the sampler confirms it.
- Before calling a Fusion pick "missed", scan for the curve: `px.ps1 "x,y0,scanv,150"` at the
  screen x and look for the blue (7,151,214)/(32,138,252) rows. Three "missed" picks this session
  were lines 20 px away from where the screenshot suggested.
- The Claude overlay window on the right of the screen (see `LOOP.md`, Setup 2): keep clicks
  below x = 1040 screenshot px. The overlay hides while desktop control runs, so it is not in
  screenshots, but its region still swallows clicks. FissionOxide's Sketch Palette checkboxes sit at
  x = 1046 when the window is at x = 0..1092: drag the window left first (it is there now).
- `request_access` refuses "fissionoxide.exe" while the app is not running: build, launch
  (`Start-Process target\release\fissionoxide.exe`), then request. `cargo build` fails with
  "Access is denied" while fissionoxide.exe runs: `Stop-Process -Name fissionoxide` first. A fresh
  instance opens maximised over the overlay: double-click the title bar to restore, then drag.
  The previous instance's unsaved document is gone.
- Only one session can hold desktop control; if `request_access` says another session is using
  the computer, the user must stop that session first.
- Clicking the taskbar button of the active window minimises it. Fusion is the orange "F" button
  (497,804 screenshot), FissionOxide the one right of it (535,804). Check the window title in the
  first screenshot of every batch. After a taskbar switch Fusion's single-letter shortcuts need a
  click on empty canvas first (an "l" straight after the switch did nothing).
- FissionOxide multi-select: a modifier sent in the same frame as the click is missed, so
  Shift+click from the tool never adds; use a window box (left-to-right, selects enclosed curves
  AND their points) and remember the engine takes `pts[0]` for MidPoint/Symmetry.
- Fusion: the big SELECT icon toggles the Select tool, the "SELECT" label opens the dropdown.
  Submenus open to the right under the overlay region when the menu is wide: hover them, never
  click there. Wheel down zooms in. Body corners carry sketch points that win over vertices.
  The Sketch Palette eats canvas clicks under it. Single-letter shortcuts need the pointer over
  empty canvas; one Escape ends the Line tool. X toggles construction on the selection. The
  Linetype construction toggle stays on between sessions: check the palette before drawing.
- Fusion constraint commands stay active after applying; press Escape before Ctrl+Z, and never
  Ctrl+Z after a pick that may not have landed (it undoes the previous command instead).
- FissionOxide: the red error line at the bottom-left is stale after the next success; the
  Parameters window stays open behind other commands; the floating value box may keep a stale
  select-all range; the Mirror line pick needs the Mirror Line row clicked first; the Text frame
  needs a drag.
- Fusion is in inches with Layout Grid off in 3D; FissionOxide is in mm. Ctrl+Z in sketch mode
  undoes one curve or one constraint at a time in both apps.
- `git pull --rebase` refuses while another session holds uncommitted edits in `crates/`; commit
  the parity files, then rebase and push from a temporary worktree
  (`git worktree add <scratchpad>/wt origin/main`, cherry-pick, push, remove). A rebase conflict
  in `ui-features.jsonl` is almost always both sides appending rows: keep both, drop the markers,
  check for duplicate keys.
