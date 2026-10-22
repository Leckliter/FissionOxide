# Prober progress

## Last stop: 2026-10-22, 09:10, session "Parity prober 8" (two areas)

Finished this session: "Origin planes/axes visibility, construction geometry display, sketch
visibility" (report `parity/reports/2026-10-22-origin-construction.md`, `map-wave22.json`, batches
`ui-origin-display` and `ui-construction-display`) and "Create: New Component, Revolve, Sweep,
Loft, ..." (report `parity/reports/2026-10-22-solid-create.md`, `map-wave23.json`, batches
`ui-primitive-dialogs`, `ui-sweep-loft-revolve`, `ui-rib-emboss-thread`,
`ui-pattern-mirror-misc`). All six are `ready` in `index.jsonl` with no fixer yet. Earlier probers
covered the chrome, canvas, sketch areas and the first-pass feature dialogs (reports in
`parity/reports/`); waves 13 to 21 have fixers.

## Next prober: start here

1. Next `todo` in `coverage.md`, from the top: "Modify: Draft, Scale, Combine, Offset Face,
   Replace Face, Split Face, Split Body, Silhouette Split, Align, Delete, Remove, Simplify, Physical
   Material, Appearance, Manage Materials, Change Parameters, Compute All, Direct Edit" (Design
   workspace: solid). Mark it `in progress`, commit, probe. Expect the same pattern as CREATE:
   Fusion dialogs open with selection chips and add rows after the picks (see the solid-create
   report's "How Fusion's Create dialogs behave").
2. Re-verify in the app the keys marked `fixing-merged` that belong to your area (141 rows carry
   that status; most are sketch keys from fixed batches). Still not re-checked from the last list:
   `ui:sketch:horizontal-conflict`, `ui:sketch:dimension-label-and-hover` (hover part),
   `ui:sketch:tangent-arc-snap-marker`, `ui:toolbar:responsive-collapse` (narrow FissionOxide to
   920 px and count the icons), `ui:canvas:vertex-marker-style` (in Fusion the pointer landed on a
   construction plane; hide Plane1 first).
3. Rebuild before probing: the running binary is `main` at `0c453dd1`; commits after it are not in
   it. Kill `fissionoxide.exe` first (the link step fails while it runs); the build takes about
   3 minutes.

Where the documents stand: Fusion has two tabs. "Untitled*" is the earlier probers' design;
"Untitled(1)*" (active) is this session's: Origin hidden (expanded in the browser), Plane1
(offset 1 in from XZ), Sketch1 (a rectangle on XY), Bodies from Box, Cylinder, Sphere, Torus and
Coil, and an empty base feature at the end of the timeline. Window restored at screenshot x
0..907, y 82..733. FissionOxide (built `0c453dd1`) is in a new design: Origin on, Plane1 (25 mm
over XY), Body1 (20 mm box at the origin), Body2 (cylinder at X 45), Sketch1 (empty, on Plane1),
Sketch2 (a rectangle on XZ), Sketch3 (empty, on XY). Window at screenshot x 8..1011, y 82..785,
Data Panel closed. Both apps are left of the overlay.

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
