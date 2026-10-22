# Prober progress

## Last stop: 2026-10-22, 12:40, session "Parity prober 16"

Finished this session: "PLASTIC tab" and "MANAGE (BOM) and UTILITIES tabs" (both done, with
not-exercised lists in `coverage.md`). Report `parity/reports/2026-10-22-plastic-manage-utilities.md`,
`map-wave31.json`, batches `ui-manage-bom-palette` (2), `ui-utilities-addins-make` (4) and
`ui-utilities-tips-status` (4, two of them reopened keys), all `ready` in `index.jsonl` with no fixer yet.
The three batches touch different files (dialogs.rs Kind::Bom; plugins_ui.rs / panels.rs / customize.rs;
tips.rs / material_ui.rs / lib.rs) and can run in parallel; `ui-utilities-addins-make` and
`ui-modify-menu-tooltips` (fixing) both touch `menus.rs`, rebase if needed.
Fusion's plastic commands (Plastic Rules, Boss, Snap Fit, Rest, Lip/Groove, Rule Fillet) are not offered on
this Personal licence: `ui:plastic:plastic-features` is `blocked`.
Verified: `ui:toolbar:tooltip-style`, `ui:toolbar:tooltip-expanded-illustration`, `ui:toolbar:tooltip-texts-solid`,
`ui:dialogs:rib-rows`, `ui:dialogs:web-rows`, `ui:chrome:3d-print-dialog`. Reopened: `ui:dialogs:manage-materials`
(library tree overlaps on one line), `ui:chrome:status-dev-messages` (Compute All writes "Computed in 0.0 s").
About 180 other keys are still `fixing-merged` from earlier waves; re-verify those that fall inside the
area you probe.

## Next prober: start here

1. Take the next `todo` in `coverage.md` (the ASSEMBLY row still needs two saved designs; leave it):
   "Timeline: every item type icon, groups, suppress, rollback, play, context menus, drag, errors". Fusion's
   "Untitled*" document has a timeline of sketches, an extrude and fillets; FissionOxide's design is empty,
   so build a few features first (sketch, extrude, fillet, a pattern, a construction plane) in both.
2. Re-verify the `fixing-merged` timeline keys first (`ui:timeline:*`, two of them) and any wave-30/31 key
   a fixer has set to `fixing-merged` since this stop (history entries dated after 2026-10-22 12:40 local).
3. Rebuild before probing: the running binary is `main` at `c94d76ac`; kill `fissionoxide.exe` first,
   `cargo build --release -p fissionoxide` took about 2.5 minutes.

Where the documents stand: Fusion has four tabs: "Untitled*" (active, inches, Body1, Sketch1..4, UTILITIES
tab), "Untitled*(1)" (the MESH-tab design), "Untitled(2)" (an empty Assembly Design) and "Untitled*(3)"
(sheet metal). Both windows were snapped with Win+Left and sit at screenshot x 0..908: Fusion at y 80..730,
FissionOxide at y 0..785 (one empty design, mm, UTILITIES tab, Data Panel closed). Fusion's taskbar button
at (497,803), FissionOxide's at (572,803).

## Traps
- Win+Left is the quickest way to place a window left of the overlay; dismiss the snap assist by clicking the
  app's own canvas. FissionOxide snaps to 728 px wide: drag its right edge (x 727) to 907 with
  `left_mouse_down` / `mouse_move` / `left_mouse_up` to match Fusion's 908 px. `left_click_drag` on the right
  edge did nothing twice this session.
- Double-clicking a snapped window's title bar restores it to its old size rather than maximising it.
- Fusion's PLASTIC tab on this licence is SOLID's menus; do not hunt for Boss or Snap Fit again.
- Fusion's Bill of Materials needs a saved design for rows; FissionOxide's "+" is still unsafe
  (`ui-insert-new-design` is `fixing`), so do not start a new design there to get a clean one.
- A fresh FissionOxide window can reach past screenshot x 1040, where its right edge is under the
  Claude overlay and cannot be grabbed: drag the title bar left first (part of the window off screen),
  narrow the right edge, then drag it back.
- Fusion's Sheet Metal Rules is a palette (Close button); its Edit Rule window is modal. Fusion's
  flat pattern environment opened with a canvas that stayed a beige fill at every zoom; Finish Flat
  Pattern still works from the green button's dropdown.
- FissionOxide's Flange dialog re-picks the consumed profile, and after that its Type dropdown and the
  chip's x do nothing: select an edge first, then start Flange, to get an edge flange.
- Fusion's Join by bend, Sheet Metal Fillet and Sheet Metal Chamfer open a subscription card (close it
  with its x at the card's top right).
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
