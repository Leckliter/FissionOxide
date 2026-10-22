# Fusion vs FissionOxide: hands-on comparison

Tested 2026-10-19 by driving both apps side by side on Windows 11: Autodesk Fusion (Personal) and
the installed FissionOxide build from 2026-10-19 20:44, which matches `main` at 4d4917f. The
same session ran in both: create a sketch on the ground plane, draw a dimensioned rectangle,
finish, extrude, fillet an edge, then open the right-click marking menu, the S key, the timeline
menu, the face menu and the browser body menu.

Each item gives what Fusion does, what FissionOxide does, and where to start in the code. Items
are in priority order: P0 blocks the basic workflow, P1 is something a Fusion user hits within
minutes, P2 is polish.

---

## P0: input bugs that block the basic workflow

### 1. Clicks in the top-right of the canvas are ignored
- **Repro:** Create Sketch. The CREATE SKETCH dialog opens at its default spot under the
  ViewCube. Its `XY (Top)` / `XZ` / `YZ` buttons and `Cancel` do nothing, and don't highlight on
  hover either. Escape still closes it. Drag the dialog by its header to the middle of the canvas
  and the same buttons work.
- The ViewCube's Home button and clicks on ViewCube faces also did nothing during the session.
  Dragging the cube didn't orbit.
- Hovering over the ViewCube showed a tooltip containing a **source file path**,
  `C:\Users\colli\...\crates\ui\src\viewport.rs`. That looks like egui's widget debug info
  appearing in a release build, and it points to a widget allocated in `viewport.rs` that covers
  this region.
- **Start here:** `viewport::viewcube` ([viewport.rs:1269](../crates/ui/src/viewport.rs)) reads raw
  `pointer.primary_clicked()` instead of a `Response`, so it isn't layered with the dialogs.
  Also check `assistant_tab` ([panels.rs:1643](../crates/ui/src/panels.rs)) and the
  `feature-dialog` Area ([dialogs.rs:1726](../crates/ui/src/dialogs.rs)). Find what owns the hit
  area, and make sure the debug tooltip can't show in release builds.

### 2. Some clicks are dropped unless the pointer rests on the target first
- Finish Sketch on the ribbon failed on several clicks in a row. The Sketch Palette's Finish Sketch
  worked once the pointer had hovered over it for about 0.4 s. Dialog OK buttons behaved the same
  way.
- This looks like reactive repaint: egui only learns the hover position on a frame, so a press
  arriving in the same frame as the move misses the widget. Real mice usually move first, so users
  may see it rarely, but tablets, remote desktop (RustDesk is installed here) and automation will
  hit it.
- **Start here:** the eframe run loop and repaint settings in `apps/fissionoxide/src/main.rs` /
  `crates/ui/src/lib.rs`. Request a repaint on pointer move, or don't gate clicks on the previous
  frame's hover.

### 3. Launching the app again opens a second instance
- Starting FissionOxide from the Start menu while it's already running opened a second
  window, with its splash screen. Fusion brings the running window forward instead.
- **Start here:** a single-instance guard (named mutex plus forwarding the file argument) in
  `crates/desktop` / `apps/fissionoxide/src/main.rs`.

---

## P1: workflow differences a Fusion user hits right away

### 4. No heads-up value input while drawing or running a feature
- **Fusion:** after the first rectangle corner, two dimension boxes follow the cursor. Typing sets
  the first value, Tab moves to the second, Enter commits. Each typed value becomes a locked
  driving dimension, with extension lines and arrows. Extrude and Fillet show a floating value box
  next to the manipulator, and typing goes straight into it. Enter is OK.
- **FissionOxide:** the rectangle shows only a coordinate readout (`5.15, 13.35`), and typed
  digits are ignored. Extrude and Fillet ignore typed digits too: you must click the Distance or
  Radius field and select its text first.
- **Start here:** the sketch tool state in `viewport.rs` (`tstate`, `sketch_overlay`) and
  `dim_editor`. Reuse the dim-editor Area as the heads-up box. For features, route typed digits to
  the dialog's primary value field (`dialogs.rs`, `fields`).

### 5. The dimension editor isn't focused or pre-selected
- **Fusion:** a new dimension's value box opens with the text selected, so you just type.
- **FissionOxide:** the box opens showing `58.4` with ✓/✗ buttons, but it has no keyboard focus.
  Typing `50` + Enter kept `58.45`. Double-clicking an existing dimension does the same unless you
  press Ctrl+A first. Fusion has no ✓/✗ buttons: Enter commits and Esc cancels.
- **Start here:** `dim_editor` ([viewport.rs:853](../crates/ui/src/viewport.rs)). Call
  `request_focus()` and select all on open.

### 6. Dimensions have no leader graphics
- **Fusion:** a linear dimension is drawn with extension lines, a dimension line with arrowheads
  and the value, in black, offset from the geometry.
- **FissionOxide:** only the number floats near the line (`58.45`), and it overlaps the
  horizontal-constraint glyph.

### 7. Picking a sketch plane
- **Fusion:** Create Sketch shows the three origin planes as translucent orange squares in the
  viewport. They highlight on hover, with the tooltip "Select a plane or planar face", and you
  click one. There's no dialog.
- **FissionOxide:** a CREATE SKETCH dialog with XY/XZ/YZ buttons (blocked by bug 1), and no
  origin planes are drawn in the viewport. Selecting XY in the browser and then running Create
  Sketch also didn't start a sketch. Fusion accepts a preselected plane.
- **Start here:** `App::create_sketch` / `start_sketch` ([lib.rs:775](../crates/ui/src/lib.rs)),
  `Kind::PickPlane` in `dialogs.rs`, and preview overlays in `preview.rs`.

### 8. Camera behaviour around sketches
- **Fusion:** entering a sketch animates to look at the plane. Finish Sketch animates back to the
  previous 3D view, and the sketch hides once a feature consumes its profile.
- **FissionOxide:** entering looks at the plane, but **Finish Sketch leaves the camera in Top
  view**, so Extrude starts with no visible depth. The Home button didn't respond (bug 1). The
  sketch hiding after Extrude does work.

### 9. Shortcuts and the S toolbox
- **Fusion:** `S` opens the "Design Shortcuts" box (recent commands plus a search field; typing
  `sh` lists Shell, Sheet Metal Rules, …). `V` toggles visibility. In a sketch, `E` finishes the
  sketch and starts Extrude.
- **FissionOxide:** `S` does nothing. There's no command search anywhere. `E`/`F`/`M` are
  ignored while sketching (the `else` branch in `App::shortcuts`,
  [lib.rs:645](../crates/ui/src/lib.rs)). No `V`.
- **Have:** L R C A P D T X O in sketches; E F M I H Q J A and 1/2/3 select modes; F6; Home; Ctrl+Z/Y/S/O/N.

### 10. "Repeat" in the marking menu shows the wrong command
- After running Fillet with `F`, the marking menu offered **Repeat Create Sketch**. Fusion offered
  Repeat Fillet.
- **Cause:** `last_cmd` is only set in `Cmd::run` ([panels.rs:529](../crates/ui/src/panels.rs)).
  Shortcuts call `App::start` directly, so they never update it. Set it in `App::start`, or route
  shortcuts through `Cmd::run`.

### 11. Live preview is off by default
- **Fusion:** Extrude and Fillet preview the result live as soon as there's a selection (the
  fillet rounds the edge while you type).
- **FissionOxide:** the Fillet dialog has a `Preview` checkbox that starts unchecked. Extrude
  previews, but it creates `Body1` in the browser while the dialog is still open.

### 12. Fillet can't take faces
- **Fusion:** clicking a face fillets all of its edges. The dialog lists rows (`1 Face | 0.2 in`),
  with a Tangent Chain checkbox, Corner Type (Rolling Ball / Setback) and a Type dropdown (Fillet /
  Rule Fillet / Full Round Fillet).
- **FissionOxide:** the selector is Edges only. Clicking a face highlights it but leaves the
  dialog at "Select". There's no per-row radius list, no tangent chain and no corner type.

### 13. Extrude dialog fields
- **Fusion:** Type (solid/thin), Profiles, Start (Profile Plane / Offset / Object), Direction (One
  Side / Two Sides / Symmetric), Extent Type, Distance, Taper Angle, Operation. The status bar shows
  `1 Profile | Area: 2.00 in^2`.
- **FissionOxide:** Profiles, Extent, Distance, Direction (Flip), Operation. Taper and thin exist
  in the engine (see README) but weren't shown in the default dialog. There's no Start row, and the
  status bar doesn't show area.

---

## P1: context menus

### 14. Right-click marking menu
| | Fusion | FissionOxide |
|---|---|---|
| Ring (8 slots) | Repeat ‹last›, Delete, Press Pull, Redo, Hole, Sketch ▾, Move/Copy, Undo | Repeat ‹wrong›, Delete, Redo, Fillet, Create Sketch, Extrude, Undo, Move/Copy |
| Overflow list | OK / Cancel, Pan, Zoom, Constrained Orbit, Set / Reset Orbit Center, Remove, Show All, Unisolate, Workspace ▸, recent commands with shortcut keys | Measure, Change Parameters, Fit, Home View |
| Overflow position | Directly under the ring | Off to the right of the ring (empty space) / under the ring (face) |

The ring is context-aware in Fusion: its slots change for faces, edges, bodies and sketches. Right
**drag** in Fusion is a marking-menu flick gesture, but FissionOxide uses right drag to orbit
(`navigate`, [viewport.rs:243](../crates/ui/src/viewport.rs)), so flick gestures can't be added
without changing that.

### 15. Face right-click
- **Fusion:** Create Selection Set, Create Sketch, Extrude, Offset Plane, Shell, Edit Feature, Edit
  Profile Sketch, Appearance (A), Texture Map Controls, Properties, Show/Hide (V),
  Selectable/Unselectable, Find in Browser, Find in Window. The status bar shows
  `1 Face | Area: 1.60 in^2`.
- **FissionOxide:** Create Sketch, Look At, Shell, Measure, Change Parameters, Fit, Home View.
  The status bar shows just `Face`.

### 16. Timeline item right-click
- **Fusion:** Create Selection Set, Edit Feature, Configure, Delete (Del), Rename, Roll Timeline
  Marker Here, Convert to DM Feature, Suppress Features, Find in Browser, Find in Window, with
  icons and shortcut hints. Hovering an item highlights its faces in the viewport and shows its
  name (`Fillet1`) in the status bar.
- **FissionOxide:** Edit Feature, Rename, Suppress Features, Roll History Marker Here, Delete.
  There are no icons, no Find in Browser/Window, and no hover highlight in the viewport.
  "History" should read "Timeline".

### 17. Browser body right-click
- **Fusion:** Move/Copy (M), Move to Group, Create Selection Set, Configure, Physical Material,
  Appearance (A), Texture Map Controls, Properties, Save As Mesh, Copy (Ctrl+C), Cut (Ctrl+X),
  Delete (Del), Remove, Rename, Display Detail Control, Show/Hide (V), Selectable/Unselectable,
  Opacity Control ▸, Isolate, Find in Window.
- **FissionOxide:** Rename, Properties, Hide, Remove.

---

## P2: shell and look

### 18. Selection and Escape
- Escape didn't clear a face selection after cancelling a dialog. The face stayed blue through a
  right-click and a browser menu. In Fusion, Esc clears the selection, and clicking empty space
  does too.

### 19. Browser
- **Fusion:** every node has an eye toggle. The root shows `(Unsaved)` until it's saved. Document
  Settings expands to `Units: in, ouncemass` and `Part Design`. Origin holds O, X, Y, Z axes and
  the XY/XZ/YZ planes. Hovering a body in the browser highlights it in the viewport.
- **FissionOxide:** eye toggles only on Bodies. The root shows `Untitled v1` before saving. Origin
  has O, XY, XZ, YZ but no axes. There's no hover highlight from the browser.

### 20. Viewport
- FissionOxide uses Z-up and its ViewCube is larger. Fusion here was Y-up, with a smaller cube in the
  corner. That's a preference in Fusion; add a Y-up / Z-up setting.
- FissionOxide's sketch grid is clipped to a square around the origin (visible in Top view).
  Fusion's grid fades out toward the horizon.
- FissionOxide shows `Camera: Orthographic` as a label in the canvas. Fusion doesn't.
- FissionOxide shades closed profiles pale yellow. Fusion shades them pale blue.
- Entering a sketch in FissionOxide highlights the Line tool on the ribbon. Fusion starts with no
  tool active.

### 21. Panels and chrome
- Fusion opens with the Data Panel closed; FissionOxide opens with it open.
- Fusion has a TEXT COMMANDS panel under the timeline (Txt / Py / TS). FissionOxide has none.
- Fusion has no EXPORT group on the SOLID ribbon. FissionOxide adds one, which is fine to keep.
- Status messages in FissionOxide are lowercase and dev-flavoured (`extrude done`,
  `fillet done`), and the sketch hint (`Sketching — draw with the Create tools…`) stayed after
  leaving the sketch. Fusion uses the status bar for the selection readout (`1 Profile | Area …`,
  `Body1`).

---

## Suggested agent split

These groups touch mostly separate code and can run in parallel:

1. **Input bugs (P0 items 1–2):** `viewport.rs` (viewcube), `dialogs.rs` Area, run loop.
   Do this first, because it makes everything else testable.
2. **Heads-up input and dimensions (items 4–6):** `viewport.rs` sketch tools, `dim_editor`, and
   the dimension drawing.
3. **Sketch entry and exit (items 7–8):** origin planes in the viewport, camera on finish, and a
   preselected plane.
4. **Shortcuts, S toolbox, Repeat (items 9–10):** `lib.rs` shortcuts, `marking.rs`, `panels.rs`
   `Cmd::run`, and a new command-search popup that reuses the `Cmd::info()` table.
5. **Context menus (items 14–17):** `marking.rs`, `panels.rs` (browser and timeline menus),
   `select.rs`.
6. **Feature dialogs (items 11–13):** live preview on by default, Fillet faces/rows/tangent chain,
   Extrude Start/Direction/Taper rows.
7. **Single instance (item 3):** `crates/desktop`, `apps/fissionoxide`.

## Not covered yet

Assemblies and joints, drawings, sheet metal, surface, mesh, CAM, the data panel and versioning,
file import/export, parameters, and the rest of the sketch tools (constraints, trim, offset,
patterns). Testing them the same way is a good next step.

---

## 2026-10-21 session

A second side-by-side run on `main` at `1f7c4f8`, with the same session (sketch, dimensioned
rectangle, finish, extrude, fillet) and then every tab, menu, context menu, the S key, browser,
timeline, ViewCube, navigation bar and Preferences. The full report with code pointers is
[parity/reports/2026-10-21.md](../parity/reports/2026-10-21.md); the 41 new ledger rows are in
`parity/ui-features.jsonl` and the nine batches in `parity/batches/` (`map-wave4.json`).

What changed since the 10-19 list above: items 1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12 (faces), 14
(overflow), 16, 17, 18 and 19 (hover highlight) are fixed and were re-checked. Still open from
this list: 3 is not a bug (see below), 13's Taper/Start rows exist but the Flip row is extra,
15's face list is now padded with navigation items, 20's grid and 21's dev status messages and
sketch hint remain.

**Test-rig warning.** During desktop control the Claude desktop app keeps a topmost window over
the right side of the screen (1372..1908 x 20..748 on this display). Clicks there never reach the
app under test and the OS caption tooltips show instead. Item 1 above ("clicks in the top-right
are ignored") and this run's first hour of right-side failures were that window, not
FissionOxide. Snap the app under test to the left half before probing.

### Second pass, 2026-10-21 (sketch tools and Modify commands)

On the rebuilt `main` (`2356d5a`): a sketch on the body's top face (circle, diameter edit, line,
Trim, Offset, Horizontal, polygon) and Chamfer, Shell, Press Pull, Move and Hole. 15 more rows
(2 P1: the polygon tool creates nothing, and Horizontal refuses a line touching a circle) in
three batches (`map-wave5.json`). Report:
[parity/reports/2026-10-21-sketch-and-modify.md](../parity/reports/2026-10-21-sketch-and-modify.md).

### Third pass, 2026-10-21 (selection and hover colours)

Pixel-sampled hover, selected and command-pick colours for faces, edges, bodies and sketch
curves in both apps: Fusion lightens a hovered face and draws a hovered edge black, FissionOxide
tints both blue; sketch curves on a face are not pickable over the face. Eight rows in
`ui-selection-colors` (`map-wave6.json`). Report:
[parity/reports/2026-10-21-selection-colors.md](../parity/reports/2026-10-21-selection-colors.md).

### Fourth pass, 2026-10-21 (every sketch CREATE tool)

Every entry of the sketch CREATE menu driven with the same clicks in both apps on a vertical
origin plane. Two geometry bugs (Three Point Arc Slot builds a three-quarter ring; Edge Polygon
ignores the side pick), no cursor prompts or heads-up boxes on any create tool, no spline
check-mark, Sketch Dimension without a placement step, Text opening as a full form, Intersect
inert, and the construction geometry, glyphs and dialog rows Fusion's tools leave behind.
23 rows in three batches (`map-wave7.json`), eight match rows. Report:
[parity/reports/2026-10-21-sketch-create-tools.md](../parity/reports/2026-10-21-sketch-create-tools.md).

## 2026-10-21, prober 3: selection remainder and the sketch MODIFY menu

Probed vertex, construction plane/axis/origin point, sketch points, in-sketch hover and selected
colours, dimension hover, window and crossing boxes, Select Other, the SELECT submenus and the
sketch MODIFY list in both apps (report `parity/reports/2026-10-21-selection-remainder.md`).
Biggest gaps: a selected sketch curve is invisible in FissionOxide and the status bar never names
it; vertices are not pickable with the default filters; the Origin folder lacks axis rows and eyes;
construction planes have no hover/selected colours; the Selection Filters list has 7 entries to
Fusion's 27. Three batches in `parity/batches/map-wave8.json` (18 rows); five earlier `match` rows
reopened. Fusion's box colours: window peach (253,223,185) with a solid purple-grey border, crossing
pale yellow (255,253,202) with a dashed dark border.

## 2026-10-21, prober 4: every sketch MODIFY tool

Fillet, Chamfer (and its submenu), Blend Curve, Offset, Trim by drag, Extend, Break, Sketch
Scale, Move/Copy and Change Parameters run with the same picks in both apps (report
`parity/reports/2026-10-21-sketch-modify-tools.md`). Fusion's Fillet, Chamfer and Blend Curve
have no dialog: first pick, second pick, applied at once with a floating value box and an arrow,
prompt back to the first pick; FissionOxide gives each a docked dialog and a preview. Offset in
FissionOxide follows the pointer from the start and its value box kept a stale select-all range
("3" became "3m", a 3 m offset); M does nothing in the sketch; Sketch Scale has typed point
fields instead of a point pick and manipulator. 20 rows in five batches
(`parity/batches/map-wave9.json`), six match rows, Trim's hover colour and the OFFSET rows
verified, Move's Set Pivot reopened for the sketch case.

### Second part, 2026-10-21 (sketch CREATE leftovers)

2-Tangent and 3-Tangent Circle, angular and diameter dimensions, Text after the frame, Mirror and
both patterns, on FissionOxide rebuilt at `ef41107` (report
`parity/reports/2026-10-21-sketch-create-leftovers.md`). Text needs a drag and loses typed text;
Mirror's line pick lands in Objects; tangent circles get an unasked radius dimension and no
heads-up box; the diameter dimension ignores its placement click. 10 rows in two batches
(`parity/batches/map-wave10.json`), five match rows, the diameter glyph verified.

## 2026-10-21, prober 5: sketch constraints, Sketch Palette options, Line heads-up

Every CONSTRAINTS entry run in both apps on fresh test geometry (report
`parity/reports/2026-10-21-sketch-constraints.md`). The big one: Fusion's constraints are pick
commands with prompts and a lit ribbon icon, FissionOxide's are one-shot actions on a
pre-selection that print "'coincident' needs 2 point(s)" in red. The solver moves the wrong
things (both coincident points to their midpoint, both lines stretched for Perpendicular, both
dropped to horizontal for Collinear, the constrained circles dragged for Concentric), glyphs are
one per constraint above-right instead of Fusion's per-line placement, Fix paints black with
padlocks where Fusion paints green, six palette rows are greyed, the palette chevron is inert, and
the Line heads-up draws two plain boxes and leaves only the length dimension where Fusion draws a
dimension-style length box, an angle arc from a dashed reference, and both dimensions. 21 rows
in five batches (`parity/batches/map-wave11.json`), five match rows, three verified
(`heads-up-units-and-lock`, `glyph-style-create-tools`, `palette-linetype`), two reopened
(`constraint-tool-flow`, `constraint-glyphs`). Trap: the Claude overlay swallowed FissionOxide's
palette checkbox clicks at x >= 1040 until the window was dragged left.

## 2026-10-21, prober 6: Sketch Palette remainder, heads-up input, sketch display

Look At, Slice, 3D Sketch, Centerline and Feature Options for every CREATE and MODIFY tool, the
Rectangle / Circle / Arc / Slot / Polygon heads-up boxes with expressions and a parameter name,
the dimension box's kebab menu and Enter on an empty box, and the sketch display colours (report
`parity/reports/2026-10-21-palette-hud-display.md`). FissionOxide enters an XY sketch from the
back (BACK, +X left) where Fusion shows FRONT; a "d1*2" heads-up expression is ignored and a
leading letter is swallowed; the kebab menu lists Change Parameters / Delete where Fusion lists
recent values and Measure; centerlines and construction draw grey even dashes where Fusion draws
tan dash-dot / dashes; profile fills are nearly white inside the sketch and steel blue outside
where Fusion uses (230,245,250) both ways; Fusion's Slice hatches the section and 3D Sketch adds a
triad to the Line tool. 17 rows in three batches (`parity/batches/map-wave12.json`), seven match
rows, nine verified, three reopened (`profile-color-3d`, `look-at-sketch-up`,
`point-glyph-style`). Desktop control of fissionoxide.exe was denied twice before the user
approved it; Fusion's side was recorded meanwhile.

## 2026-10-21, prober 7: sketch display remainder, sketch browser rows and context menus

Projected geometry and the fully constrained colour in FissionOxide, then every sketch browser row
behaviour in both apps: hover, click, eye, the row menu's twenty Fusion entries one by one,
Redefine Sketch Plane, Export DXF, the Sketches folder menu, double-click (report
`parity/reports/2026-10-21-sketch-display-browser-rows.md`). FissionOxide draws projected curves
black like fully constrained geometry where Fusion draws purple with filled points; its sketch
rows have no eyes, no folder menu, an 11-entry menu against Fusion's 20, Redefine Sketch Plane is
greyed, and Save As DXF goes straight to the Windows file dialog where Fusion asks for units and
visible geometry first. 12 rows in three batches (`parity/batches/map-wave13.json`), eight match
rows, twelve verified (the wave-9/10 browser rows and the sketch selection feedback colours).
Desktop control was denied three times before the user approved it.

## 2026-10-21, prober 7 (continued): SOLID ribbon tooltips, pin/unpin, shortcut customisation

Every SOLID ribbon button hovered for 1 s and 4 s in both apps, then the CREATE > Extrude row's
options, pin/unpin, Change Keyboard Shortcut and the S toolbox (report
`parity/reports/2026-10-21-ribbon-tooltips.md`). Fusion's tooltip is a dark panel with the
shortcut in the title, two paragraphs at once and a large illustration plus "Press Ctrl+/ for
more help." after 3 s; FissionOxide's is a light panel with one line, a "Shortcut:" row and its own
icon, and every text differs (Fusion's 20 texts are in the batch). Fusion's row options are two
checkboxes and a dialog entry; its shortcut dialog is a modal with hints and Reset to default.
6 rows in two batches (`parity/batches/map-wave14.json`), five match rows, two verified
(`row-options-glyph`, `tooltip-style`).

## 2026-10-21, prober 7 (continued): Data Panel

Fusion's Data Panel against FissionOxide's local hub (report
`parity/reports/2026-10-21-data-panel.md`). Fusion opens on a project list with filters, a New
Project button, described cards and a LIBRARIES section; a project has Data | People tabs, a blue
Upload, inline New Folder, a Settings gear, "Open on the Web" and a 12-entry design menu, and its
Upload is a window with Select Files and Drag and Drop. FissionOxide has a project dropdown, four
Files / Recent / Activity / Trash tabs Fusion lacks, text buttons, floating prompts that ignore
Escape, a 6-entry card menu and a native upload dialog. 8 rows in one batch
(`parity/batches/map-wave15.json`), four match rows.

## 2026-10-21, prober 7 (continued): File menu dialogs

Save, Open, Export, 3D Print, Capture Image and the View submenu opened from the File menu in both
apps (report `parity/reports/2026-10-21-file-dialogs.md`). Fusion opens its own Open window (project
tree, Samples, "Open from my computer"), an Export window with a 14-format Type dropdown and a
wide Save modal; FissionOxide goes to the Windows dialogs for Open and each Export format, its Save
window carries Description and Milestone rows, its 3D Print and Capture Image windows are large and
sparse, and none of its floating windows close on Escape. 7 new rows plus the reopened
`file-menu-items` in one batch (`parity/batches/map-wave16.json`), three match rows.

## 2026-10-21, prober 7 (continued): Document Settings, units, Named Views, Comments

The Units and Part Design rows, the Named Views folder and the COMMENTS header in both apps
(report `parity/reports/2026-10-21-document-settings-views.md`). Fusion's Change Active Units is a
docked dialog with a unit-system dropdown, its Part Design row has a pencil and a Convert Design
Type dialog, New Named View names the row inline, and COMMENTS expands a docked panel;
FissionOxide has a radio popup, the pencil on the wrong row, no design-type menu, "Named View 1"
with a status line and a floating Comments window. 5 rows plus the reopened `comments-expand` in
one batch (`parity/batches/map-wave17.json`), four match rows.

## 2026-10-21, prober 7 (continued): status bar readouts

Every selection kind reachable on a one-body design read in both apps (report
`parity/reports/2026-10-21-status-readouts.md`). The browser rows, a face, a sketch point and the
timeline feature read alike; FissionOxide prints the face readout already on hover, loses the
first kind of a mixed selection, and keeps a timeline feature selected after Escape. 4 rows in one
batch (`parity/batches/map-wave18.json`), four match rows.

## 2026-10-21, prober 7 (continued): camera and navigation bar

Navigation bar tooltips and tools, the wheel, Display Settings > Camera and the Preferences mouse
rows in both apps (report `parity/reports/2026-10-21-camera-navbar.md`). The wheel, the tool
drags, the Camera submenu and the orbit-type row match; FissionOxide's navbar tooltips are
sentences, its fifth button is Fit where Fusion's is Zoom Window, its mouse presets lack
PowerMill and Tinkercad, its default orientation is Z up, and Escape closes its whole Preferences
window. 5 rows in one batch (`parity/batches/map-wave19.json`), four match rows, one verified
(`display-settings-2`).

## 2026-10-21, prober 7 (continued): display settings

Every Display Settings submenu in both apps (report
`parity/reports/2026-10-21-display-settings.md`). The menu structure, the Camera rows and full
screen match; FissionOxide's Visual Style has eight unordered rows without the Ctrl+4..9 keys,
its Environment is a flat list of sixteen names without Fusion's Theme row and headers, its
Effects has three unchecked boxes against Fusion's seven with a preset row, Object Visibility has
five rows against fourteen, and Ground Plane Offset is a large window without the Adaptive
checkbox. 5 rows in one batch (`parity/batches/map-wave20.json`), two match rows.

## 2026-10-21, prober 7 (continued): grid and snaps

The Grid and Snaps menu, Grid Settings and Set Increments in both apps (report
`parity/reports/2026-10-21-grid-snaps.md`). The entries match; Fusion's 3D view hides the grid
until Layout Grid is on, greys the lock until then, keeps Incremental Move on, and opens docked
Adaptive / Fixed dialogs where FissionOxide shows the grid by default and uses submenus with
values. 3 rows in one batch (`parity/batches/map-wave21.json`), one match row.

## 2026-10-22, prober 8: origin and construction display

The browser's Origin folder, an Offset Plane and Create Sketch's plane chooser in both apps
(report `parity/reports/2026-10-22-origin-construction.md`). Hover and selection colours of the
origin planes and axes are verified. Fusion draws each origin plane in the positive quadrant of
its axes, see-through and depth-tested with the axes on top, keeps them in an open sketch, and
picks infinite planes with a grid preview in Create Sketch; FissionOxide centres near-opaque
squares over everything and hides them in a sketch. Construction planes are an invisible dashed
outline in FissionOxide, without eyes, V or the full row menu; Fusion's Offset Plane now opens a
CONSTRUCTION GEOMETRY dialog. 11 rows in two batches (`parity/batches/map-wave22.json`), two match
rows, three verified.

## 2026-10-22, prober 8 (continued): SOLID > CREATE dialogs

Every CREATE entry's dialog in both apps, several to their preview (report
`parity/reports/2026-10-22-solid-create.md`). The CREATE list matches. Fusion's dialogs open with
selection chips and grow rows after the picks, its primitives start with a plane pick and a drawn
base, and Operation turns to Cut over a body; FissionOxide's open with every row, preset planes
and axes, sketch dropdowns and "(empty = ...)" fields, and preview at once. 22 rows in four batches
(`parity/batches/map-wave23.json`), three match rows, six sketch and vertex keys verified.

## 2026-10-22, prober 9: SOLID > MODIFY

Every MODIFY entry in both apps, Draft completed and edited (report
`parity/reports/2026-10-22-solid-modify.md`). The entries, order and shortcut hints match; Fusion
has no Direct Edit submenu, fewer separators and Simplify in a different order. As in CREATE,
Fusion's dialogs open with chips and grow value rows after the picks; FissionOxide presets planes
and non-zero values, enables OK early, picks faces where Fusion picks bodies, keeps its picks
selected after Cancel, cannot edit a Draft, and has no Silhouette Split, Replace with Primitives
or Manage Materials. 24 rows in six batches (`parity/batches/map-wave24.json`), including the
reopened `ui:toolbar:responsive-collapse` (MODIFY keeps 6 icons at 907 px against 4); one match row.

## 2026-10-22, prober 10: feature manipulators, Edit Feature, feature errors

Extrude, Fillet, Shell, Offset Face and Draft manipulators in both apps, read from
native-resolution crops while hovering, dragging and tabbing, then Edit Feature on Extrude,
Fillet and Box and a failing fillet (report `parity/reports/2026-10-22-manipulators.md`). The
floating value box, live preview while typing, Enter as OK and the failed timeline icon match.
Fusion draws 3D blue arrows with a dashed drag track, a separate disc-on-arc taper and draft handle,
a grey dimension in the preview, moves the box with Tab, keeps the box clear of its arrow, rolls the
timeline back while editing, greys OK on a failing value and shows a "1 error(s)" toast with a
status-bar badge. 16 rows in three batches (`parity/batches/map-wave25.json`), four match rows,
five keys verified.

## 2026-10-22, prober 11: Edit Feature finished; Construct, Inspect, Insert, Assemble, Select started

Chamfer, Mirror, Hole, Move, Combine and Press Pull made and double-clicked in both apps, a chamfer
made to fail, then its toast, Error window, right-click menu, Suppress, Unsuppress and Roll here;
then the CONSTRUCT, CONFIGURE, INSPECT, INSERT, ASSEMBLE and SELECT lists, Offset Plane, Axis Through
Cylinder and Measure (report `parity/reports/2026-10-22-edit-construct.md`). Fusion edits every
feature in its creation dialog titled "EDIT FEATURE"; FissionOxide cannot edit Hole, Mirror, Combine
or Press Pull (and, from the code, most other kinds), has no Error window or Review warning, and
lists every assembly command in a Part Design document where Fusion offers only "Add To Assembly".
18 rows in three batches (`parity/batches/map-wave26.json`), three match rows, three Select menu keys
verified.

## 2026-10-22, prober 12: Construct methods, Inspect, Insert, Configure, Assemble

Every Method of Fusion's CONSTRUCTION GEOMETRY dialog against FissionOxide's separate dialogs, a
construction point made in both, Measure with two picks, every Inspect dialog, every Insert entry,
Configure, and Fusion's new-design chooser and Assembly Design document (report
`parity/reports/2026-10-22-construct-inspect-insert.md`). Fusion picks everything in the canvas or
browser with OK disabled until done, keeps analyses in a browser Analysis folder and opens its Insert
window for files; FissionOxide uses plane dropdowns and coordinate boxes, opens Plane Along Path for
Perpendicular Plane, lists no construction points or axes in the browser, leaves analysis colours on
after Close, lacks four analyses, Decal and Derive, and its "+" silently replaces the open design.
16 rows in four batches (`parity/batches/map-wave27.json`), two match rows, Configure blocked by the
licence, one key verified. The area is done; joints need an assembly with components (new coverage
row).

## 2026-10-22, prober 13: SURFACE tab

The SURFACE tab's ribbon, tooltips, dropdowns and every create/modify dialog as it opens (and after
the first pick in Fusion), a Patch made in both apps and a surface Extrude in FissionOxide (report
`parity/reports/2026-10-22-surface-tab.md`). The shared panels and the MODIFY order match; Fusion's
surface dialogs are their own (no Operation row, rows that appear after a pick, manipulators,
continuity tables) where FissionOxide uses the generic form dialog, its Ruled is a different command,
surface Sweep and Loft are missing, surface bodies and features get solid icons, the menus carry four
rows Fusion does not, and Finish Sketch drops back to SOLID. 20 rows in four batches
(`parity/batches/map-wave28.json`), three match rows, three sketch keys verified (the dimension
tooltip's light box split out). The area is done.

## 2026-10-22, prober 14: MESH tab

The MESH tab's dropdowns, tooltips and every command's dialog in Fusion with an inserted STL mesh,
against FissionOxide's MESH tab with the same file inserted (report
`parity/reports/2026-10-22-mesh-tab.md`). The dropdown lists and the tooltip text match. FissionOxide
has no mesh body: the STL becomes a faceted solid, so the browser row, menus, picking and Convert
Mesh result differ, Insert Mesh skips Fusion's INSERT MESH placement dialog, Plane Cut opens Split
Body, Reduce and Smooth have other rows (and their previews collapse or shrink the body), and twelve
entries (Tessellate, Remesh, Repair, face groups, Direct Edit and others) are stubs. 20 rows in four
batches (`parity/batches/map-wave29.json`), two match rows, 18 command-ledger rows set, eight
fixing-merged keys verified. The area is done; SHEET METAL is next.

## 2026-10-22, prober 15: SHEET METAL tab

The SHEET METAL tab in a new millimeter Part Design in each app (report
`parity/reports/2026-10-22-sheet-metal-tab.md`): a base flange and an edge flange made in both, and in
Fusion Create Flat Pattern, its environment, Unfold and Refold. The tab's panels match. Fusion keeps
sheet metal rules in a library palette with a modal Edit Rule window and shows the design's rule in
the browser; FissionOxide has one "Default" rule edited in a command dialog. Fusion's single Flange
dialog changes with the pick (Base / Edge, Type icons for Hem and Lofted Flange, Orientation, rule
list, manipulator); FissionOxide auto-picks the profile, folds edge flanges the other way and cannot
edit any sheet metal feature. Create Flat Pattern is a face pick and an environment in Fusion, an info
panel in FissionOxide; Fusion's Unfold opens a REFOLD ribbon panel. 17 rows in four batches
(`parity/batches/map-wave30.json`), one match row, one blocked row (subscription commands), five
fixing-merged keys verified. The area is done; PLASTIC is next.
