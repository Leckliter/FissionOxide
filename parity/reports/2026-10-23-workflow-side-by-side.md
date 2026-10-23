# 2026-10-23: incremental pass (verifier 9) and a workflow side-by-side

Built `main` at `4e1c9667` (binary already current, no build fix). Only the four uncheckable keys
were `fixing-merged` (`ui:marking-menu:flick`, `ui:chrome:comments-expand`,
`ui:sketch:vertical-on-horizontal-line`, `ui:timeline:feature-owner-component`), so nothing was
re-checked. The time went on a fresh side-by-side of the most-used workflow instead.

## Fixer batches still `fixing` (at 10:35)

No fixer has pushed to `main` since `905a4701` (05:10). On remote branches:

| batch | fixer | pushed |
|---|---|---|
| ui-reverify-mesh-1 | task_27591c2d (09:12) | nothing |
| ui-reverify-mesh-2 | task_28b09f29 (09:12) | WIP `claude/friendly-thompson-05fav0` 10:11 |
| ui-reverify-dialogs-6 | task_32bd4ad7 (09:12) | WIP `claude/sharp-newton-x1mmg1` 10:23 ("untested checkpoint") |
| ui-reverify-render-2 | task_996c0a03 (09:12) | nothing |
| ui-legacy-leftovers | task_aca51e60 (09:12) | `claude/vigilant-babbage-rxx0za` 09:57 (not on main) |
| ui-reverify-chrome-3 + ui-reverify-shortcuts-1 | task_a598c0fe (09:42) | nothing |
| ui-reverify-dialogs-7 + ui-reverify-workspaces-2 | task_e9f3d771 (09:42) | WIP `claude/gracious-maxwell-08g1v0` 10:03 ("untested") |
| wave-45 (5 batches) | task_62901c53, task_9b720f60, ... (10:34) | just started |

None has been quiet for "hours" yet: the oldest silent ones (mesh-1, render-2) were started 1 h 25 min
before this check.

## The workflow run

Fresh mm design in both apps (Fusion: Ctrl+N, Units changed to mm with Set as Default unticked,
never saved). Create Sketch on the ground plane, R, first corner lower left, 30 Tab 40 Enter,
Finish Sketch, E, 10 Enter, F on the top front edge, 2 Enter, double-click Extrude1 in the
timeline, 15 Enter.

Matched: Create Sketch's prompt and plane hover, the SKETCH tab and palette rows, the rectangle
tool ending after a typed Enter (palette drops Feature Options in both), profile area readout
"1 Profile | Area : 1200.00 mm^2", the extrude arrow / taper handle / "Hold Ctrl to modify
selection", the 10.00 dimension label in the preview, fillet edge hover black and pick bright blue,
the fillet result, the timeline tooltip "Extrude1 / (Unsaved)", EDIT FEATURE's title, rollback
(fillet greyed) and the recompute after OK, Finish Sketch leaving Sketches collapsed and nothing
selected, the consumed sketch hidden after Extrude.

Differences (all in `ui-features.jsonl`):

P1
- `ui:dialogs:extrude-auto-profile` (reopened, second time): Fusion picks the only visible profile
  automatically when E opens (one rectangle: "1 selected", focus in "0.00 mm"); two circles or no
  visible profile: Select. FissionOxide never pre-picks. The 82b2cf5 fix followed an incomplete
  Fusion text.
- `ui:dialogs:extrude-start-distance`: Fusion starts at 0.00 mm with OK greyed; FissionOxide starts
  at 10 mm with a preview body (`dialogs.rs:800`).

P2
- `ui:dialogs:title-case-labels`: "Profile Plane", "One Side", "Extent Type", "Taper Angle",
  "Radius Type", "Tangent Chain", "Tangency Weight", "Corner Type", "Rolling Ball" in Fusion;
  sentence case in FissionOxide.
- `ui:dialogs:fillet-before-pick`: empty table and no Tangency Weight until the first pick, chip
  stays "Select", zero as "0.00 mm" in Fusion.
- `ui:manip:value-box-covers-arrow` (reopened, second time): Fusion keeps the fillet box below-right
  and further out; FissionOxide moves it left of the arrow onto the body.
- `ui:sketch:rectangle-heads-up-layout` (reopened, second time): Fusion's focused height box is on
  the first corner's vertical edge (left), not beside the pointer.
- `ui:sketch:heads-up-mm-decimals`: "22.833 mm" vs "15.75 mm".
- `ui:sketch:vertical-dimension-text`: height dimension on the left with rotated text in Fusion;
  right side, horizontal text in FissionOxide.
- `ui:sketch:axis-lines-in-sketch`: red X and green Y lines across the sketch grid in Fusion; grey
  in FissionOxide.
- `ui:sketch:entry-scale-grid-steps`: 25 mm labels / 5 mm minor at about 3.5 px/mm in Fusion;
  50 mm labels / 10 mm minor at about 5 px/mm in FissionOxide.
- `ui:browser:folders-stay-collapsed`: Fusion leaves new Sketches and Bodies folders collapsed;
  FissionOxide expands them and frames Sketch1.
- `ui:chrome:status-in-command-picks`: empty on hover and "Edge" after the pick in Fusion's Fillet;
  "1 Edge | Length : 40.00 mm" on hover in FissionOxide.
- `ui:chrome:assistant-side-tab`: FissionOxide's AUTODESK ASSISTANT canvas tab has no Fusion
  counterpart in any tab seen today.

## Retractions / not recorded

- The blue body during FissionOxide's Edit Feature was the timeline-hover highlight
  (`ui:timeline:hover-highlights-feature`), not a preview colour.
- Fusion draws freshly typed sketch curves black and thick until the next click, then blue; not
  understood well enough to record.
- Fusion's ground shadow: none visible; FissionOxide's is soft and faint, within
  `ui:render:design-ground-look`.
- FissionOxide's window did not take a bottom-edge resize drag (height stayed 705 screenshot px);
  not a Fusion parity item, not chased.

## Batches (wave 46, `map-wave46.json`, all `ready`)

| batch | items |
|---|---|
| ui-workflow-dialogs-1 (P1) | extrude-auto-profile, extrude-start-distance, fillet-before-pick, title-case-labels, value-box-covers-arrow |
| ui-workflow-sketch-1 (P2) | axis-lines-in-sketch, entry-scale-grid-steps, heads-up-mm-decimals, rectangle-heads-up-layout, vertical-dimension-text |
| ui-workflow-chrome-1 (P2) | folders-stay-collapsed, status-in-command-picks, assistant-side-tab |

## Not covered

Revolve, chamfer, shell and the marking menus in this workflow; dragging the arrows; Edit Sketch
from the browser; undo names; the rest of the extrude/fillet dropdowns.
