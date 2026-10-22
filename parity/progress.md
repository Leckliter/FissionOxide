# Prober progress

## Last stop: 2026-10-22, 10:20, session "Parity prober 10"

Finished this session: "Manipulators for every feature" (done, with a not-exercised list) and the
first part of "Edit Feature for every feature; feature errors and warnings in the timeline" (in
progress). Report `parity/reports/2026-10-22-manipulators.md`, `map-wave25.json`, batches
`ui-extrude-manipulator`, `ui-feature-handles`, `ui-edit-feature-errors`, all `ready` in
`index.jsonl` with no fixer yet. Prober 9's six wave-24 batches were also `ready` with no fixer when
this session started.

## Next prober: start here

1. Continue "Edit Feature for every feature; feature errors and warnings in the timeline"
   (Design workspace: solid). Done so far: Edit on Extrude, Fillet (both apps) and Box (Fusion),
   a failing fillet value in the edit dialog and an upstream failure (`ui-edit-feature-errors`).
   Next: double-click every other feature type (Revolve, Chamfer, Shell, Press Pull / Offset Face,
   Draft, Scale, Combine, Move, Hole, patterns, Mirror, sketches) and compare the edit dialog,
   its handles and the rollback; then a yellow warning (Fusion's warning toast and icon), the
   error toast's More Info, and Suppress / Roll to here on a failed item. Then the next `todo` in
   `coverage.md` (Configure, Construct, Inspect, Insert, Assemble, Select menu).
2. Re-verify in the app the `fixing-merged` keys of your area, plus the older ones still not
   re-checked: `ui:sketch:horizontal-conflict`, `ui:sketch:dimension-label-and-hover` (hover part),
   `ui:sketch:tangent-arc-snap-marker`, `ui:canvas:vertex-marker-style` (native-resolution crop).
3. Rebuild before probing: the running binary is `main` at `2a6b280b`; kill `fissionoxide.exe`
   first, `cargo build --release -p fissionoxide` takes about 2 minutes.

Where the documents stand: Fusion "Untitled(1)*" (active): the box is 0.05 in high with a
0.01 in Fillet1 at the end of the timeline; window at screenshot x 0..907, y 82..733. FissionOxide
(built `2a6b280b`) is in a new design: Sketch1, Extrude1 (25 mm, -3 deg taper), Fillet1 failed at
100 mm radius (red in the timeline); Data Panel closed, window at x 8..915, y 80..782. Both are left
of the overlay; FissionOxide's taskbar button is at (571,804), Fusion's at (497,804).

## Traps
- A `mouse_move` in one call and `left_mouse_down` in the next pressed in Fusion (it came to the
  front over FissionOxide and started a window selection). Keep move and press in one batch.
- Scratchpad PowerShell scripts need `powershell -NoProfile -ExecutionPolicy Bypass -File ...`.
- FissionOxide's window narrows only from its invisible resize border, 2 px right of the last
  window pixel (x 1100 when the window ended at 1098).
- A Bash heredoc holding many apostrophes and backticks failed to parse once; write batch files
  with the Write tool.
- FissionOxide's taskbar button moves: this session it was at (572,804) on first launch
  and (535,804) afterwards, next to Fusion's (497,804). A click on the wrong one brought another
  app forward; take a screenshot after each switch.
- A right-edge drag on FissionOxide took only with `left_click_drag` from the exact edge pixel
  (x 1100, y 650); `left_mouse_down` / `mouse_move` on the edge or corner did nothing.
- Escape on an open dropdown in a Fusion dialog cancels the dialog (Scale); close dropdowns by
  clicking the field again.
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
- Screenshots taken with `scale` < 1 still use the full 1456 x 819 frame for clicks; divide the
  scaled image's pixels by the scale. `zoom` output is too small to read dash patterns; a
  native-resolution crop (`CopyFromScreen` of the region, upscaled 4x with nearest-neighbour) reads
  them, and is only a capture, not input.
- A fresh FissionOxide window opened at x 145..1228 this time, with the Data Panel open: drag the
  title bar left, then drag the right edge at y 700 (below the overlay) to x 1012. The Data
  Panel's x at its top-right closes it; the grid button at (45,118) opened INSPECT instead once.
- Fusion's primitives (Box, Cylinder, Sphere, Torus, Coil) open no dialog until a plane is picked
  and the base drawn; Create Sketch's chooser picks infinite planes, so any canvas click picks one.
- FissionOxide's CREATE menu sometimes ignores the first click on an entry (Coil): click again.
  Its sketch palette grows when a tool's Feature Options show, which moves the Finish Sketch
  button down; use the ribbon's FINISH SKETCH.
- Escape cancels Fusion's Create dialogs (when no dropdown is open) and FissionOxide's.
