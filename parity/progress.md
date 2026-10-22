# Prober progress

## Last stop: 2026-10-22, 10:58, session "Parity prober 12"

Finished this session: "Configure, Construct (every plane/axis/point), Inspect, Insert, Assemble,
Select menu" (done, with a not-exercised list in `coverage.md`). Report
`parity/reports/2026-10-22-construct-inspect-insert.md`, `map-wave27.json`, batches
`ui-construct-methods` (5), `ui-inspect-measure-section` (3), `ui-inspect-analyses` (4) and
`ui-insert-new-design` (4, one P0: "+" replaces the open design), all `ready` in `index.jsonl` with no
fixer yet. Prober 11's wave-26 batches (`ui-edit-more-features`, `ui-timeline-failed-items`,
`ui-inspect-insert-assemble`) were assigned to cloud fixers by the coordinator during this session.

## Next prober: start here

1. Take the next `todo` in `coverage.md`: "SURFACE tab: every create/modify command and dialog"
   (Design workspace, SURFACE tab in both apps). Compare the tab's panels and dropdowns, then every
   command's dialog as it opens and after its first pick, the result's browser row, timeline icon and
   right-click menu.
2. Re-verify in the app the `fixing-merged` keys still not re-checked: `ui:sketch:horizontal-conflict`,
   `ui:sketch:dimension-label-and-hover` (hover part), `ui:sketch:tangent-arc-snap-marker`, plus any
   key in the SURFACE area marked `fixing-merged`.
3. Rebuild before probing: the running binary is `main` at `c4d97aa9`; kill `fissionoxide.exe` first,
   `cargo build --release -p fissionoxide` took about 2 minutes this time.

Where the documents stand: Fusion has three tabs: "Untitled*" (older), "Untitled*(1)" (Part Design,
inches: plate, cylinder with a failed Chamfer1, spheres, coil, torus, Plane1, Point1, an Analysis
folder with Section1 and CenterOfMass1) and "Untitled(2)" (an empty Assembly Design, active); window
at screenshot x 0..907, y 82..733. FissionOxide (built `c4d97aa9`) holds one new design with a Box
(the earlier design was lost to "+", see `ui:chrome:new-tab-replaces-design`); Data Panel closed,
window at x 8..1012, y 103..755. Both left of the overlay; Fusion's taskbar button at (497,803),
FissionOxide's at (572,803).

## Traps
- FissionOxide's "+" (and Ctrl+N) throws the open design away without asking; do not press it
  with work you still need.
- FissionOxide's analyses (Draft, Accessibility, Zebra, Curvature Map) stay on after Close and
  Escape and even into a new design; reopen the command and clear Show analysis before looking at
  colours.
- Fusion's INSERT entries (Decal, Canvas, SVG, Mesh) open a modal "Insert" window over the canvas;
  its Cancel is at (625,568).
- FissionOxide's Insert SVG / DXF / Mesh open the Windows file dialog and leave the INSERT dropdown
  open; the next click only closes the dropdown.
- Fusion's ASSEMBLY tab dropdowns often ignore the first click after Escape: move to the canvas,
  back onto the label, wait, click (twice if needed).
- Fusion's Tangent Plane on a sphere would not take a reference pick (browser plane, canvas plane or
  Plane1).
- Fusion's construct commands open a single "CONSTRUCTION GEOMETRY" dialog (Type icons Plane / Axis /
  Point, Method dropdown); switching Type clears the picks. Close a Method dropdown by clicking the
  field again (Escape cancels the dialog).
- Fusion's document is a Part Design: its ASSEMBLE menu holds only "Add To Assembly". Probing joints
  needs an assembly document.
- Fusion's ASSEMBLE, INSPECT and SELECT dropdowns sometimes ignore the first click after an Escape;
  move onto the label, wait half a second and click again. SELECT's dropdown overflows under the
  overlay.
- Press Pull on a Fusion primitive's end face edits the primitive (no new timeline item).
- FissionOxide's "This feature has no editable properties yet" stays at the lower left for minutes;
  do not read it as the result of a later action.
- A PowerShell pixel sample (`px.ps1`) was refused by the auto-mode classifier this session; zoom
  instead, or ask the user to allow it.
- Bash heredocs that hold many apostrophes or backticks fail to parse (twice now); write files with
  the Write tool and run python from a scratchpad file.
- A `mouse_move` in one call and `left_mouse_down` in the next pressed in Fusion (it came to the
  front over FissionOxide and started a window selection). Keep move and press in one batch.
- Scratchpad PowerShell scripts need `powershell -NoProfile -ExecutionPolicy Bypass -File ...`.
- FissionOxide's taskbar button moves (572 or 535 at y 804); take a screenshot after each switch.
- A fresh FissionOxide window opens with the Data Panel open, under the overlay: close the panel
  with the x at its top-right (272,178), drag the title bar with `left_mouse_down` / `mouse_move` /
  `left_mouse_up`, then narrow it with `left_click_drag` from the exact right-edge pixel at y 700.
- Escape on an open dropdown in a Fusion dialog cancels the dialog; close dropdowns by clicking the
  field again.
- Fusion's ribbon tooltip needs the pointer to rest on the icon from a cold start: move onto the
  canvas for half a second between buttons.
- `request_access` can come back `user_denied` several times before the user sees the dialog;
  "fissionoxide.exe" is refused outright while the app is not running: launch first.
- Rebuilding while `fissionoxide.exe` runs fails at the link step; kill it first
  (`taskkill //PID <pid> //F` from bash), build in the background, probe Fusion meanwhile.
- Fusion maximised puts its palettes under the Claude overlay; restore and narrow the window.
- FissionOxide's heads-up boxes swallow a leading letter typed into the unfocused first box; Tab
  first when typing a parameter name.
- Tool calls run one after another; screen px = screenshot px x 1.3187.
- The Claude overlay region (screen x >= 1372, y 20..748) swallows clicks even while hidden; keep
  clicks below screenshot x = 1040.
- Fusion is in inches with Layout Grid off in 3D; FissionOxide is in mm.
- `git pull --rebase` refuses while another session holds uncommitted edits in `crates/`; commit
  the parity files, then rebase and push from a temporary worktree. A rebase conflict in
  `ui-features.jsonl` is almost always both sides appending rows: keep both.
- Fusion's primitives open no dialog until a plane is picked and the base drawn; FissionOxide's open
  at once on the XY plane.
- FissionOxide's CREATE menu sometimes ignores the first click on an entry: click again.
