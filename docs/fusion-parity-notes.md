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
