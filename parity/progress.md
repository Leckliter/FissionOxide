# Prober progress

## Last stop: 2026-10-22, 11:45, session "Parity prober 14"

Finished this session: "MESH tab: every command" (done, with a not-exercised list in `coverage.md`).
Report `parity/reports/2026-10-22-mesh-tab.md`, `map-wave29.json`, batches `ui-mesh-body-and-insert`
(5), `ui-mesh-modify-dialogs` (5), `ui-mesh-new-modify` (5) and `ui-mesh-prepare-create` (5), all
`ready` in `index.jsonl` with no fixer yet. `ui-mesh-body-and-insert` (a real mesh body kind) is the
base of the other three; a coordinator may start it first or tell the others to work on the faceted
bodies until it lands. Re-verified: `ui:navbar:tooltips`, `ui:navbar:zoom-window-button`,
`ui:chrome:preferences-mouse-setup`, `ui:chrome:preferences-default-orientation`,
`ui:chrome:preferences-escape`, `ui:dialogs:primitive-plane-pick-flow`, `ui:dialogs:primitive-rows`,
`ui:dialogs:primitive-manipulators` (all verified). Still `fixing-merged` and not re-probed:
`ui:dialogs:coil-rows`, `ui:dialogs:pipe-rows`, `ui:dialogs:edit-feature-title`,
`ui:browser:redefine-sketch-plane`, `ui:browser:export-dxf-dialog`.

## Next prober: start here

1. Take the next `todo` in `coverage.md` after the ASSEMBLY row (which needs two saved designs):
   "SHEET METAL tab: rules, flange, bend, unfold, flat pattern". Compare the tab's panels and
   dropdowns, Sheet Metal Rules, every CREATE/MODIFY dialog as it opens and after its first pick
   (Fusion needs a sheet metal body: Create Sketch, a rectangle, Flange; or Convert to Sheet Metal on a
   box), Create Flat Pattern and its environment, and the results' browser rows, timeline icons and
   menus.
2. Re-verify the five keys above, and any wave-27/28/29 key a fixer has set to `fixing-merged` since
   (`python -c` over `ui-features.jsonl`, status `fixing-merged`, history entries dated after
   2026-10-22 16:45 UTC).
3. Rebuild before probing: the running binary is `main` at `25d185b8`; kill `fissionoxide.exe`
   first, `cargo build --release -p fissionoxide` took about 2 minutes.

Where the documents stand: Fusion has three tabs: "Untitled*" (older), "Untitled*(1)" (Part Design,
inches, active, on the MESH tab: the earlier plate, cylinder, spheres, coil, torus, Plane1, Point1,
Analysis folder and Body8, plus the inserted mesh "lprism" (now hidden), "lprism_Edited" (a solid
from Convert Mesh), two Direct Edit base features and GenerateFaceGroups1) and "Untitled(2)" (an empty
Assembly Design); window at screenshot x 0..907, y 82..733. FissionOxide (built `25d185b8`) holds one
design: Body1, the faceted STL (Import1), on the SOLID tab; its window was dragged to x 0..1000,
y 92..795 (Data Panel closed). Fusion's taskbar button at (497,803), FissionOxide's at (572,803).
The test meshes are in `tests/reference/464-stl-millimeter/lprism.stl`, `465-obj-inch/lprism.obj`
and `466-3mf-file/lprism.3mf`; type the full path into the file dialog's File name box.

## Traps
- Clicking a selected browser row again starts a rename in Fusion; press Escape.
- FissionOxide's "<name> isn't in FissionOxide yet" stays at the lower left and hides other lower-left
  labels (Zoom Window's).
- Dragging FissionOxide's title bar up past the top of the screen lands on Explorer (click tier) and
  stops the batch with the button held: `left_mouse_up` at once, then drag sideways only.
- FissionOxide's sketch shortcut "D" twice did nothing after the Coincident and Horizontal/Vertical
  toolbar flows (CREATE > Sketch Dimension worked); unrecorded, check whether shortcuts die after a
  constraint command.
- `left_click_drag` does not drag sketch points in FissionOxide; use `left_mouse_down`, two or three
  `mouse_move` steps and `left_mouse_up`.
- Fusion's Ctrl+Z right after Finish Sketch reopens the sketch rather than removing it.
- Win+Left on FissionOxide opens Windows snap assist over the right half (File Explorer, click tier):
  click FissionOxide's own canvas to dismiss it; a drag that ends over Explorer is refused.
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
