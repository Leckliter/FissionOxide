# Prober progress

## Last stop: 2026-10-22, 10:40, session "Parity prober 11"

Finished this session: "Edit Feature for every feature; feature errors and warnings in the
timeline" (done, with a not-exercised list) and the first part of "Configure, Construct (every
plane/axis/point), Inspect, Insert, Assemble, Select menu" (in progress). Report
`parity/reports/2026-10-22-edit-construct.md`, `map-wave26.json`, batches `ui-edit-more-features`,
`ui-timeline-failed-items`, `ui-inspect-insert-assemble`, all `ready` in `index.jsonl` with no fixer
yet. Prober 10's three wave-25 batches (`ui-extrude-manipulator`, `ui-feature-handles`,
`ui-edit-feature-errors`) were assigned to cloud fixers by the coordinator during this session.

## Next prober: start here

1. Continue "Configure, Construct (every plane/axis/point), Inspect, Insert, Assemble, Select menu"
   (Design workspace: solid). Done so far: the CONSTRUCT, CONFIGURE, INSPECT, INSERT, ASSEMBLE and
   SELECT lists (Select submenus verified), the Offset Plane and Axis Through Cylinder dialogs, Measure
   on one face. Next: each Method of Fusion's single "CONSTRUCTION GEOMETRY" dialog (Plane: At Angle,
   Tangent, Midplane, Perpendicular Plane, Through Two Edges, Through Three Points, Along Path; Axis:
   five methods; Point: six) against FissionOxide's separate dialogs (rows, previews, the result's
   browser row and timeline icon); Measure with two picks (distance, angle); Interference, Section
   Analysis (manipulator, browser Analysis folder), Curvature Comb / Map, Zebra, Draft, Isocurve,
   Accessibility, Center of Mass, Minimum Radius; the Insert Decal, Canvas, SVG, DXF, Mesh and Derive
   dialogs; Configure (the table); Assemble in an assembly document (Fusion's "Add To Assembly" or a
   new assembly design), joints, motion, contact sets. Then the next `todo` in `coverage.md` (SURFACE
   tab).
2. Re-verify in the app the `fixing-merged` keys still not re-checked:
   `ui:sketch:horizontal-conflict`, `ui:sketch:dimension-label-and-hover` (hover part),
   `ui:sketch:tangent-arc-snap-marker`, `ui:canvas:vertex-marker-style` (native-resolution crop),
   plus `ui:canvas:construction-plane-colors` and `ui:canvas:construction-axis-colors` (your area).
3. Rebuild before probing: the running binary is `main` at `ed3546c5`; kill `fissionoxide.exe`
   first, `cargo build --release -p fissionoxide` took about 5 minutes this time.

Where the documents stand: Fusion "Untitled*(1)" (active, inches, Part Design): plate with a hole,
cylinder (height 0.05 in) with a failed Chamfer1, two spheres after a Combine, a moved torus;
window at screenshot x 0..907, y 82..733. FissionOxide (built `ed3546c5`) in a new design: Box,
Cylinder, Chamfer1 (failed at 50 mm), Sketch + Hole1, Mirror1, Move1, Combine1, PressPull1; Data
Panel closed, window at x 8..1012, y 72..775. Both left of the overlay; Fusion's taskbar button at
(497,804), FissionOxide's at (572,804).

## Traps
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
