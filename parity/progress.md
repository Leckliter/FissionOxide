# Prober progress

## Last stop: 2026-10-22, 16:45, session "Re-verification sweep, verifier 1"

Swept the sketch area of the re-verification line in `coverage.md` (kept `in progress`). On the rebuilt
binary (`main` at `ef887079`) 85 sketch keys were `fixing-merged`: 74 now `verified`, 7 reopened
(`mismatch`), 4 not re-checked and still `fixing-merged`: `ui:sketch:intersect-missing` (the body did not
cross the sketch plane, so no curves to compare), `ui:sketch:look-at-sketch-up` (needs an XZ and a YZ
sketch side by side with Fusion), `ui:sketch:tangent-line-circle-motion` (my test line was also
collinear-constrained, so it could not pivot: redo with a line joined to another line only) and
`ui:sketch:slice-section-look` (the body sat behind the sketch plane). Of the seven open rows that sat in
done batches, five are `verified` (superseded by verified keys, or seen) and `ui:canvas:select-other` and
`ui:dialogs:combine-fields` are `match` with a note naming their superseding keys
(`ui:canvas:select-other-popup`, `ui:dialogs:combine-rows`, both still `fixing-merged`).
`ui:canvas:box-select-colors` was verified on the way. New wave-37 batches, both `ready` with no fixer:
`ui-reverify-sketch-1` (6: one P1, `ui:sketch:window-delete-error`, plus three reopened and two new P2s)
and `ui-reverify-sketch-2` (4 reopened P2s). `map-wave37.json` holds the keys.

Remaining `fixing-merged` after this session: 271 (dialogs 56, chrome 26, mesh 20, surface 19, canvas 18,
timeline 18, sheet 17, manip 11, browser 9, navbar 8, toolbar 8, render 7, inspect 6, manufacture 6,
construct 5, animation 5, utilities 5, sketch 4, menus 4, workspaces 4, electronics 4, shortcuts 4,
documents 3, manage 2, viewcube 1, plastic 1).

## Next verifier: start here

1. Keep the sweep line `in progress` and take the dialogs area next (56 keys; list them with
   `python -c "import json;[print(r['key']) for r in map(json.loads,open('parity/ui-features.jsonl',encoding='utf-8')) if r['area']=='dialogs' and r['status']=='fixing-merged']"`),
   then chrome, canvas and the rest in `coverage.md`'s order. Most dialog keys are "open the dialog and
   read its rows", so a body (one extrude) and a sketch cover most of them in one pass.
2. Pick up the four sketch keys above when a body crosses a sketch plane (draw on XY, extrude, sketch on
   XZ through the body for Intersect and Slice; Look At on XZ and YZ in both apps).
3. Rebuild first (kill `fissionoxide.exe`, `cargo build --release -p fissionoxide`, about 3 min).
4. Work from the rows' `fusion` text and look at Fusion only where the text is unclear; one sketch per
   group of keys (this session: a rectangle with typed values and expressions, a line with typed angle,
   circles, polygons, slots, then a clean second sketch for fillet / chamfer / offset / trim / constraints).

Where the documents stand: Fusion untouched this session (four tabs as prober 20 left them).
FissionOxide: one "Untitled*" mm Part Design with Sketch1 (hexagon extruded into Body1, the rest of the
test shapes), Sketch2 (rectangle, circles, lines, a blend curve) and Sketch3 on a vertical origin plane
(projected Body1 outline), open for edit; window at screenshot x 0..908, Data Panel closed.

## Traps
- Synthetic clicks on FissionOxide's canvas right after a key press or a menu click are sometimes dropped
  (the first corner of a rectangle, the first Mirror pick, a palette checkbox right after another click):
  `mouse_move` to the target, wait 0.3 s, then click, and look before assuming the click landed.
- A window box over a whole FissionOxide sketch then Delete fails ("no point Point(d2) in this sketch")
  until `ui-reverify-sketch-1` lands; delete with a crossing box or start a new sketch instead.
- After a Collinear constraint the second line shifts a few px; re-read positions before the next pick.
- The Circular Pattern Center Point would not take the centre shared by two concentric circles; pick a
  plain sketch point.
- Fusion's Help > Check for Updates puts an "A new version of Fusion has landed" card with Update Now /
  Update Later in the Job Status panel; never press Update Now (Fusion restarts and the unsaved tabs go).
- Fusion's window X with unsaved tabs opens the Save dialog for the active tab; Cancel at (635,500) keeps
  Fusion open with every tab.
- Fusion's house icon (16,114) opens the Home page and shows the Text Commands panel; click a document tab to
  go back.
- Fusion's Preferences reopens on the page last shown; its tree scrolls once Design, Manufacture and Graphics
  are expanded, so take a zoom of the tree before clicking a lower node. Cancel at (787,605).
- FissionOxide's window X quits at once even with unsaved changes (until `ui-documents-window-tabs` lands).
- A relaunched FissionOxide opens at screenshot (105..1196, 100..800) with the Data Panel open: close the
  panel (x at its top right), click the canvas, Win+Left, click the canvas again to dismiss snap assist, then
  drag the right edge from x 727 to 907 with `left_mouse_down` / `mouse_move` / `left_mouse_up` at y 600.
  Dragging the title bar upward is refused once the pointer passes over the desktop.
- Body corners in Fusion's "Untitled*" carry sketch points (the sketches sit on the body), so a right-click
  there is a sketch point; use the sheet metal flange in "Untitled*(3)" for a vertex.
- Bash heredocs with apostrophes fail; write notes with the Write / Edit tools.
- Closing a Fusion tab with changes opens the Save dialog (Name, Location, Don't Save / Cancel / Save);
  Cancel at (632,500). Do not press Don't Save on the owner's tabs.
- Until `ui-shortcut-sketch-scope` lands, A, E, F, H and Q in a FissionOxide sketch finish the sketch;
  double-click the sketch's browser row to edit it again. Fusion keeps the sketch open.
- Clicking a row in Fusion's browser gives the browser the keyboard: Home and the arrows then move in the
  tree. Click the canvas before testing keys.
- FissionOxide's V hides a selected body and drops the selection; show it again from its browser eye.
- `zoom` does not magnify: a 90 px region comes back about 90 px; icons that small cannot be told apart.
- Fusion's Render (teapot), Drawing > From Design and the BOM all need a saved design; From Design jumps
  back to DESIGN and opens the Save dialog (Cancel at (634,501)). Probers have not saved to the owner's
  Default Project; ask the owner before doing so.
- Fusion's Generative Design and Simulation open an upgrade card at the upper left (x at (247,201)).
- Fusion's New Electronics Design opens a new document tab; New Schematic another. The schematic tab
  closes without asking; the electronics design tab asks Save / Don't Save / Cancel (Don't Save at
  (563,498)).
- Fusion's In-canvas Render keeps rendering until its (now stop) button is pressed again.
- Fusion's docked dialogs at the right (New Setup, Scene Settings) put OK/Cancel near y 678 / 549; a
  Cancel click that lands on a tab label leaves the dialog open: take a screenshot after cancelling.
- FissionOxide's DESIGN > RENDER turns In-Canvas Render on and opens Appearance; Finish Render or the
  dropdown leaves it. FissionOxide's CREATE > Box now waits for a plane pick like Fusion's.
- Shift+click and Ctrl+click from the automation tool did not extend FissionOxide's timeline selection
  (recorded as `ui:timeline:multi-select`); Fusion took them. A Ctrl+click once made Explorer the
  frontmost app for the tool (the next right-click was refused at tier "click"): click FissionOxide's
  title bar first.
- After Expand Group in FissionOxide the open-folder icon takes a slot and the marker moves right: take a
  screenshot before grabbing the marker, or you drag the last feature instead.
- Fusion's Part Design refuses New Component ("Save design required", Cancel at (597,445)); in the empty
  Assembly Design "Untitled(2)" New Component creates an external cloud design (External1 in Default
  Project): Cancel it.
- `zoom` returns the region at native pixels (no magnification): a 150 px region comes back about 200 px
  wide. Timeline icons stay 16 px; read names from tooltips instead.
- Win+Left is the quickest way to place a window left of the overlay; dismiss the snap assist by clicking the
  app's own canvas. FissionOxide snaps to 728 px wide: drag its right edge (x 727) to 907 with
  `left_mouse_down` / `mouse_move` / `left_mouse_up` to match Fusion's 908 px. `left_click_drag` on the right
  edge did nothing twice this session.
- Double-clicking a snapped window's title bar restores it to its old size rather than maximising it.
- Fusion's PLASTIC tab on this licence is SOLID's menus; do not hunt for Boss or Snap Fit again.
- Fusion's Bill of Materials needs a saved design for rows.
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
- FissionOxide's "+" opens the "What do you want to design?" chooser and Ctrl+N a new tab; the old
  design stays in its tab (verified 2026-10-22). Fusion's Ctrl+N opens a Part Design tab at once; an
  untouched new tab closes from its x without asking.
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
