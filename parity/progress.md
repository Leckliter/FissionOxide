# Prober progress

## Last stop: 2026-10-21, 20:05, session "Parity prober 6" (Sketch Palette remainder, heads-up input, sketch display)

Finished: "Sketch Palette" (Look At, Slice, 3D Sketch, Centerline, Feature Options for every
CREATE and MODIFY tool) and "Heads-up input" (Rectangle, Circle, 3-Point Arc, Center to Center
Slot and Polygon boxes, "1/2" and "d1*2" expressions, the dimension box's kebab menu and tooltip,
Enter on an empty box), both marked `done` in `coverage.md`. "Sketch display" is `in progress`:
grid labels, profile fill in and out of the sketch, construction and centerline styles, curve
colours and sketch points are recorded; projected geometry colour in FissionOxide and a re-run of
the fully constrained colour are left. Report `parity/reports/2026-10-21-palette-hud-display.md`;
batches `map-wave12.json` (`ui-sketch-palette-look-slice-3d`, `ui-sketch-linetype-and-colors`,
`ui-sketch-hud-expressions`, 17 new rows plus two reopened rows folded into the first two); seven
match rows; nine verified (`enter-ends-rectangle`, `line-hud-angle-precision`,
`dim-edit-param-name`, `offset-dimension`, `polygon-tool-flow`, `create-tool-heads-up-boxes`,
`palette-feature-options`, `grid-axis-labels`, `modify-menu-list`); three reopened
(`profile-color-3d`, `look-at-sketch-up`, `point-glyph-style`). Earlier: chrome, toolbar,
ViewCube and navigation bar, marking menus, browser and timeline menus, Preferences, the basic
sketch flow, the first-pass feature dialogs, face/edge/body colours, every sketch CREATE and
MODIFY tool, the selection remainder, every constraint, the palette options and the Line heads-up
(reports in `parity/reports/`).

## Next prober: start here

1. Finish "Sketch display" (`in progress`): project a body's edge in FissionOxide (make a body
   first: rectangle, Finish Sketch, Extrude, then a new sketch on the XY plane and P with the
   Bodies filter) and compare its colour with Fusion's purple (189,132,217) curves and dark-purple
   filled points; re-run the fully constrained colour (Fusion black on a fully constrained curve,
   `ui:sketch:fully-constrained-color` is `verified`); then mark the area `done`.
2. Next `todo` in `coverage.md`: "Sketch browser rows, context menus, Edit Sketch, Redefine Sketch
   Plane, Save As DXF" (sketch section), then the chrome `todo`s (tooltips on every ribbon button,
   Data Panel, Save/Open/Export dialogs).
3. Re-verify in the app any key marked `fixing-merged` once fixers have pushed
   (`parity/batches/index.jsonl`); `ui:sketch:horizontal-conflict` has still not been re-checked,
   and `ui:sketch:point-glyph-style` rides with `ui:sketch:line-end-points`.

Where the documents stand: Fusion is in its test sketch on XY (the one with the circles and the
40 degree line; a 2.00 x 0.50 in rectangle at the upper left, the body's silhouette projected in
purple, a centerline/construction test line below), window restored at screenshot x 0..907,
y 82..733, palette floating at x 717..905. FissionOxide runs the `d90d1e6` build in a NEW
document: Sketch1 on XY (seen from the BACK) with a 0.5 mm wide rectangle (dimension d1), a large
rectangle, a construction line with a 5 mm offset copy and two small rectangles; its window is at
x -50..1040 (title bar partly off-screen) with the Data Panel closed and the palette checkboxes at
x 995. Both apps are left of the overlay; no window needs moving.

## Traps
- `request_access` for "fissionoxide.exe" can be denied by a mis-click on the user's dialog; ask
  the coordinator (one message) and keep probing Fusion's side until it is granted, then request
  once more. "fissionoxide.exe" is refused outright while the app is not running: launch first.
- Fusion maximised puts its collapsed SKETCH PALETTE tab under the Claude overlay (x 1444); restore
  the window (double-click the title bar), narrow it from its right edge at y > 567 (the overlay
  ends at screen y 748) and drag the title bar horizontally. A drag that ends over the desktop is
  refused ("Explorer ... tier click"); use `left_mouse_down` / `mouse_move` / `left_mouse_up` and
  keep the end point over the window.
- Fusion's palette flyout (from the collapsed tab) closes on the first click inside it; pin it
  with the "«" at its top-right first.
- Fusion's 3D Sketch mode eats the Line tool's first click while its triad is shown; turn the
  checkbox off before drawing test geometry. Fusion's sketch Fillet picks never paired for this
  prober (three tries); pick the lines well away from axis lines and projected curves.
- FissionOxide's heads-up boxes: a leading letter typed into the unfocused first box is swallowed
  (and "d" would be the Dimension shortcut elsewhere); Tab first when typing a parameter name.
- Finish Sketch in Fusion hides a consumed sketch; the test sketch is re-entered from the last
  timeline icon (x 205, y 695 at the current window placement), the first sketch (TOP view, the
  body's) from the first icon.
- Tool calls run one after another, never in parallel: a PowerShell pixel sample always runs after
  the whole computer_batch before it (`scratchpad/px.ps1`, quote the arguments). Screen px =
  screenshot px x 1.3187. Reading a flat fill from a saved screenshot JPEG with `System.Drawing`
  works for Fusion colours after the fact (JPEG error about 1 per channel).
- The Claude overlay region (screen x >= 1372, y 20..748) swallows clicks even while hidden; keep
  clicks below screenshot x = 1040 and check `WindowFromPoint` before blaming an app.
- Clicking the taskbar button of the active window minimises it; Fusion is the orange "F" (497,804),
  FissionOxide the one right of it (535,804). A Fusion taskbar click did not raise it over
  FissionOxide once this session; take a screenshot before trusting the switch.
- Fusion is in inches with Layout Grid off in 3D; FissionOxide is in mm.
- `git pull --rebase` refuses while another session holds uncommitted edits in `crates/`; commit
  the parity files, then rebase and push from a temporary worktree
  (`git worktree add <scratchpad>/wt origin/main`, cherry-pick, push, remove). A rebase conflict
  in `ui-features.jsonl` is almost always both sides appending rows: keep both, drop the markers,
  check for duplicate keys.
