# Prober progress

## Last stop: 2026-10-21, 21:10, session "Parity prober 7" (sketch display remainder, sketch browser rows)

Finished: "Sketch display" (projected geometry colour in FissionOxide, fully constrained colour
re-run) and "Sketch browser rows, context menus, Edit Sketch, Redefine Sketch Plane, Save As DXF",
both marked `done` in `coverage.md`. Report
`parity/reports/2026-10-21-sketch-display-browser-rows.md`; batches `map-wave13.json`
(`ui-browser-sketch-rows` 7 items, `ui-sketch-redefine-and-dxf` 2, `ui-sketch-project-colors` 3,
all `ready` in `index.jsonl`); eight match rows; twelve verified (`root-eye-and-origin-eye`,
`document-settings-expanded`, `root-unsaved-label`, `sketches-folder-after-finish`,
`folder-context-menus`, `origin-rows`, `curve-hover-color`, `curve-selected-color`,
`canvas-background`, `line-end-points`, `point-glyph-style`, `fully-constrained-color`). Earlier:
chrome, toolbar, ViewCube and navigation bar, marking menus, browser and timeline menus,
Preferences, the basic sketch flow, the first-pass feature dialogs, face/edge/body colours, every
sketch CREATE and MODIFY tool, the selection remainder, every constraint, the palette, the heads-up
input and the sketch display (reports in `parity/reports/`).

## Next prober: start here

1. Next `todo` in `coverage.md`, from the top: the chrome `todo`s, "Tooltips on every ribbon button
   (short and expanded), pin/unpin, customise shortcuts" (chrome section), then "Data Panel" and
   the Save / Save As / Open / Export dialogs. Mark the area `in progress`, commit, probe.
2. Re-verify in the app any key marked `fixing-merged` once fixers have pushed
   (`parity/batches/index.jsonl`). Still not re-checked: `ui:sketch:horizontal-conflict`,
   `ui:sketch:dimension-label-and-hover` (hover part), the remaining `ui-sketch-hud-prompts` keys
   (`create-tool-cursor-prompts`, `spline-accept-check`, `conic-rho-box`, `tangent-arc-snap-marker`)
   and the `ui-vertex-and-construction` canvas keys (`hover-vertex`, `vertex-filter-default`,
   `vertex-marker-style`, `construction-plane-colors`, `construction-axis-colors`).
3. Rebuild before probing: the running binary is `main` at `42dec181`; commits after it
   (`crates/sketch/src/place.rs` and whatever the wave-12/13 fixers push) are not in it.

Where the documents stand: Fusion is in the earlier probers' document, 3D view (home orbit), Body1,
Sketch1 (consumed, hidden), Sketch2, Sketch3 (its Slice toggled on and off again, dimensions shown),
Sketch4; window restored at screenshot x 0..907, y 82..733, palette floating. FissionOxide (process
started 20:56, build `42dec181`) is in a NEW document: Sketch1 (rectangle) extruded 10 mm into
Body1, Sketch2 on XY with the body projected (black) and a fully constrained 20 x 15 mm rectangle
on the origin with its dimensions shown in 3D; window at screenshot x 8..1010, y 63..769, Data
Panel closed. Both apps are left of the overlay; no window needs moving.

## Traps
- `request_access` can come back `user_denied` for every app several times in a row (three times
  this session) before the user sees and approves the dialog; ask the coordinator once, keep the
  turn short, and request again when the user says so. "fissionoxide.exe" is refused outright while
  the app is not running: launch first.
- Rebuilding while `fissionoxide.exe` runs fails at the link step (the exe is locked); kill it
  first (`taskkill //PID <pid> //F` from bash), build in the background, probe Fusion meanwhile.
- A fresh FissionOxide window opens at x 66..1157 (under the overlay). Dragging the title bar left
  until the right edge clears x 1040 pushes the browser off-screen; instead drag the window so its
  left edge is on screen, then narrow it from the bottom-right corner (`left_mouse_down` on the
  corner at y > 567 screenshot px, where the overlay does not reach); a drag on the right edge
  alone did not take.
- Fusion's sketch row menu opens upward when the window has no room below: its bottom edge sits at
  the pointer, so entries are at fixed offsets above the row (Edit Sketch at -299 px for a row at
  y 552 in a window whose bottom is y 733). Escape on an open dropdown inside a Fusion dialog
  cancels the whole dialog.
- Fusion maximised puts its collapsed SKETCH PALETTE tab under the Claude overlay (x 1444); restore
  the window (double-click the title bar), narrow it from its right edge at y > 567 and drag the
  title bar horizontally. A drag that ends over the desktop is refused ("Explorer ... tier click");
  use `left_mouse_down` / `mouse_move` / `left_mouse_up` and keep the end point over the window.
- Fusion's palette flyout (from the collapsed tab) closes on the first click inside it; pin it
  with the "«" at its top-right first.
- FissionOxide's heads-up boxes: a leading letter typed into the unfocused first box is swallowed
  (and "d" would be the Dimension shortcut elsewhere); Tab first when typing a parameter name. A
  Dimension pick on a rectangle line must hit the line's pixels (a click 6 px outside missed).
- Finish Sketch in Fusion hides a consumed sketch; the test sketch is re-entered from the last
  timeline icon, the first sketch (TOP view, the body's) from the first icon.
- Tool calls run one after another, never in parallel: a PowerShell pixel sample always runs after
  the whole computer_batch before it (`scratchpad/px.ps1`, quote the arguments). Screen px =
  screenshot px x 1.3187.
- The Claude overlay region (screen x >= 1372, y 20..748) swallows clicks even while hidden; keep
  clicks below screenshot x = 1040 and check `WindowFromPoint` before blaming an app.
- Clicking the taskbar button of the active window minimises it; Fusion is the orange "F" (497,804),
  FissionOxide the one right of it (535,804). Take a screenshot before trusting the switch.
- Fusion is in inches with Layout Grid off in 3D; FissionOxide is in mm.
- `git pull --rebase` refuses while another session holds uncommitted edits in `crates/`; commit
  the parity files, then rebase and push from a temporary worktree
  (`git worktree add <scratchpad>/wt origin/main`, cherry-pick, push, remove). A rebase conflict
  in `ui-features.jsonl` is almost always both sides appending rows: keep both, drop the markers,
  check for duplicate keys.
