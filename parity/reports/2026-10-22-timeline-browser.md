# Timeline and browser, 2026-10-22 (prober 17)

Fusion: the "Untitled*" Part Design (inches) with Sketch1, Extrude1, Fillet1, Sketch2..4, then Plane1
(offset 0.5 in from the top). FissionOxide: rebuilt from `main` at `f1f9ed56`, one design (mm) built for
this session: Sketch1 (rectangle), Extrude1 (20 mm), Fillet1 (3 mm), Plane1 (10 mm offset), Sketch2,
Sketch3 (circles), Fillet2, RectangularPattern1 (3 bodies). Both windows 908 px wide at the left of the
screen.

## Matched
- Suppress / Unsuppress on Fillet1: rounds go and come back; the suppressed menu reads Unsuppress
  Features without Edit Feature (`ui:timeline:suppress-effect`).
- Create Group collapses at once without a name prompt; Expand Group restores the members
  (`ui:timeline:create-group-collapses`, look differs).
- Step forward / Move to end, a valid reorder drop, double-click edit.
- Browser: slow click on a selected row starts the inline rename, body eye toggles, Bodies folder menu.

## Verified (were `fixing-merged`)
`ui:timeline:hover-highlights-feature`, `ui:timeline:feature-icon-during-command`,
`ui:timeline:edit-rollback` (browser part moved to `ui:timeline:rollback-browser-rows`),
`ui:browser:sketch-row-menu-list`, `ui:browser:sketch-row-eyes`, `ui:browser:consumed-sketch-hidden`,
`ui:browser:sketches-folder-menu`, `ui:browser:sketch-row-hover`, `ui:browser:sketch-row-selected-canvas`,
`ui:browser:units-row-mass-unit`, `ui:browser:units-dialog`, `ui:browser:row-edit-pencils`,
`ui:browser:named-view-icon`.

## Differences
P1
- `ui:timeline:sketch-item-menu`: a sketch's timeline menu lacks Extrude, Edit Sketch, Redefine / Select
  Sketch Plane, Convert to DM Feature.
- `ui:timeline:multi-select`: Shift/Ctrl+click on timeline icons replaced the selection (modifiers read at
  frame end); Fusion's range gives "3 Features" and a multi-selection menu with Create Group.
- `ui:timeline:rollback-browser-rows`: rolled-back features keep (grey) browser rows; Fusion removes them.
- `ui:timeline:marker-drag-live`: Fusion recomputes while the marker is dragged and shows the left
  feature's tooltip; FissionOxide waits for the release.
- `ui:browser:body-row-menu`: the solid body row menu (overlaps `ui:surface:body-context-menu`).

P2
- `ui:timeline:single-item-menu-extras`, `ui:timeline:group-icon`, `ui:timeline:group-tooltip`,
  `ui:timeline:group-menu`, `ui:timeline:play-from-start`, `ui:timeline:settings-menu`,
  `ui:timeline:item-tooltip`, `ui:timeline:suppressed-healthy-icon` (the pink square is for failed
  suppressed items only), `ui:timeline:drag-feedback` (and the stale "Can't move there" message),
  `ui:browser:folder-eye-children-hidden`, `ui:browser:construction-folder-collapsed`.

## Batches (wave 32, all `ready`)
| batch | items | files |
|---|---|---|
| ui-timeline-menus-groups | 6 | panels.rs `timeline_entries` / `timeline_menu`, panels/timeline_groups.rs |
| ui-timeline-rollback-play | 7 | panels.rs `timeline`, `timeline_items`, `timeline_drag`; panels/sketch_rows.rs; Construction rows |
| ui-browser-body-rows | 3 | panels.rs `body_menu`, Bodies / Construction folder rows |

The two timeline batches both edit `crates/ui/src/panels.rs`'s timeline in different functions; run
them one after the other or rebase. `ui-browser-body-rows` overlaps `ui-surface-results` (fixing) in
`body_menu`.

## Retractions and caveats
- The automation tool's Shift/Ctrl+click is a quick modifier-down, click, modifier-up. Fusion took it;
  FissionOxide did not. A person holding Shift longer may get the range; the fix (read the modifiers
  from the click event) is still right.
- My first marker drag in FissionOxide grabbed Sketch3 (the marker had moved right after Expand Group),
  which produced the refused-drop message; it is a fair test of the refusal, not a marker bug.
- Fusion's group menu has a red-X entry that reads "Edit Group" with the Del hint; it was not clicked.

## Not covered
- Component rows, Activate, the Joints folder, browser drag and drop: Fusion's Part Design refuses
  components ("Save design required"), and New Component in the empty Assembly Design creates an
  external cloud design (External1 in Default Project), so it was cancelled. These belong with the
  ASSEMBLY row (needs saved designs).
- Timeline icons of every feature type side by side (16 px icons; the zoom tool does not magnify),
  Find in Window / Find in Browser results, Rename in the timeline, Delete from the timeline,
  Configure, Convert to DM Feature, Change to Direct Modeling, Fusion's group rename.
- Browser: Analysis folder, body groups (New Group), Selectable/Unselectable and Opacity Control
  effects, `ui:browser:convert-design-type` and `ui:browser:new-named-view-inline-name` (still
  `fixing-merged`).
