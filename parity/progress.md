# Prober progress

## Last stop: 2026-10-22, 09:55, session "Parity prober 9"

Finished this session: "Modify: Draft, Scale, Combine, ..." (Design workspace: solid; report
`parity/reports/2026-10-22-solid-modify.md`, `map-wave24.json`, batches
`ui-modify-menu-tooltips`, `ui-draft-scale-dialogs`, `ui-combine-split-dialogs`,
`ui-align-remove-simplify`, `ui-material-appearance`, `ui-dialog-selection-state`). All six are
`ready` in `index.jsonl` with no fixer yet. Prober 8's six wave-22/23 batches have fixers. Not
started: "Manipulators for every feature".

## Next prober: start here

1. Next `todo` in `coverage.md`, from the top: "Manipulators for every feature: arrows, rings,
   floating value boxes, drag behaviour, snapping" (Design workspace: solid). Mark it
   `in progress`, commit, probe. Fusion shows a manipulator and a focused floating box once a
   dialog's picks are complete (Extrude, Draft's arrow plus angle ring, Scale's arrow, Offset
   Face's arrow, the primitives); FissionOxide has few. Drag each, type into the box, Tab, and
   compare snapping and the value readout. Then "Edit Feature for every feature".
2. Re-verify in the app the `fixing-merged` keys of your area. Still not re-checked from the
   older list: `ui:sketch:horizontal-conflict`, `ui:sketch:dimension-label-and-hover` (hover
   part), `ui:sketch:tangent-arc-snap-marker`, `ui:canvas:vertex-marker-style` (use a
   native-resolution crop; `zoom` is too small). Fixers of waves 22-24 will add more.
3. Rebuild before probing: the running binary is `main` at `ffa348e2`; kill `fissionoxide.exe`
   first, `cargo build --release -p fissionoxide` takes about 2 minutes.

Where the documents stand: Fusion "Untitled(1)*" (active) as prober 8 left it plus a Draft (5 deg
on the box's +X face) at the end of the timeline; window at screenshot x 0..907, y 82..733.
FissionOxide (built `ffa348e2`) is in a new design: Body1 (20 mm box at the origin), Body2 (20 mm
box at X 40), Draft1 (3 deg on Body1's +X face), Data Panel closed, window at screenshot
x 8..915, y 82..785 (narrowed to Fusion's width for the ribbon re-check). Both are left of the
overlay.

## Traps
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
