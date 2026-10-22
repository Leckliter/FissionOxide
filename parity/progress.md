# Prober progress

## Last stop: 2026-10-21, 22:30, session "Parity prober 7" (seven areas this session)

Finished this session, in order: "Sketch display", "Sketch browser rows, context menus, Edit
Sketch, Redefine Sketch Plane, Save As DXF" (report
`parity/reports/2026-10-21-sketch-display-browser-rows.md`, batches `map-wave13.json`); "Tooltips
on every ribbon button, pin/unpin, customise shortcuts" for the SOLID tab (report
`2026-10-21-ribbon-tooltips.md`, `map-wave14.json`); "Data Panel" (report
`2026-10-21-data-panel.md`, `map-wave15.json`); the File menu dialogs (report
`2026-10-21-file-dialogs.md`, `map-wave16.json`); "Document Settings, units change, Named Views,
Comments panel" (report `2026-10-21-document-settings-views.md`, `map-wave17.json`). Every batch
from wave 13 on is `ready` in `index.jsonl` with no fixer yet. "Status bar readouts for every
selection kind; lower-left messages" is `in progress`. Earlier: chrome, toolbar, ViewCube and
navigation bar, marking menus, browser and timeline menus, Preferences, the basic sketch flow, the
first-pass feature dialogs, face/edge/body colours, every sketch CREATE and MODIFY tool, the
selection remainder, every constraint, the palette, the heads-up input (reports in
`parity/reports/`).

## Next prober: start here

1. Next `todo` in `coverage.md`, from the top: "Data Panel: projects, folders, upload, versions,
   search, Recent/Activity/Trash" (chrome section), then "Save, Save As, Open, Open Recent, Export,
   New Drawing, Insert into Design, Capture Image, 3D Print dialogs". Mark the area `in progress`,
   commit, probe. Fusion's Data Panel needs the cloud (Personal licence: record `blocked` where it
   refuses).
2. Re-verify in the app any key marked `fixing-merged` once fixers have pushed
   (`parity/batches/index.jsonl`). Still not re-checked: `ui:sketch:horizontal-conflict`,
   `ui:sketch:dimension-label-and-hover` (hover part), `ui:toolbar:responsive-collapse` (narrow
   FissionOxide to 920 px and count the icons), the remaining `ui-sketch-hud-prompts` keys
   (`create-tool-cursor-prompts`, `spline-accept-check`, `conic-rho-box`, `tangent-arc-snap-marker`)
   and the `ui-vertex-and-construction` canvas keys (`hover-vertex`, `vertex-filter-default`,
   `vertex-marker-style`, `construction-plane-colors`, `construction-axis-colors`).
3. Rebuild before probing: the running binary is `main` at `42dec181`; commits after it are not
   in it. Kill `fissionoxide.exe` first (the link step fails while it runs).

Where the documents stand: Fusion is in the earlier probers' document, 3D view (home orbit), Body1,
Sketch1 (consumed, hidden), Sketch2, Sketch3 (dimensions shown), Sketch4; its CREATE panel now
shows Create Sketch, Revolve, Hole, Rectangular Pattern, Create Form (Extrude was unpinned and
re-pinned but has not come back into the visible slots at 907 px; widen the window or unpin Create
Form to check). Window restored at screenshot x 0..907, y 82..733, palette floating. FissionOxide
(process started 20:56, build `42dec181`) is in a NEW document: Sketch1 (rectangle) extruded 10 mm
into Body1, Sketch2 on XY with the body projected (black) and a fully constrained 20 x 15 mm
rectangle on the origin with its dimensions shown in 3D; Extrude unpinned and pinned back (in
place); window at screenshot x 8..1010, y 63..769, Data Panel closed. Both apps are left of the
overlay; no window needs moving.

## Traps
- Fusion's ribbon tooltip needs the pointer to rest on the icon from a cold start: move onto the
  canvas for half a second between buttons. Its panel can overflow the window's right edge.
- Fusion's row options are checkboxes (Pin to Toolbar, Pin to Shortcuts); a replayed batch toggles
  them back and forth. Press "s" only after closing any open dropdown (a canvas click), or the key
  goes to the menu.
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
