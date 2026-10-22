# 2026-10-22: SHEET METAL tab (prober 15)

Setup: Fusion in a new Part Design tab "Untitled(3)" made with "+" (millimeter, gram); FissionOxide
built from `main` at `8b29a812` in its one design (mm). In both: a 100 x 60 mm rectangle on XY, a base
flange, an edge flange on the front edge; in Fusion also Create Flat Pattern (finished), Unfold and
Refold. FissionOxide's window was moved to x 27..990 (screenshot px) so its right edge sits left of
the Claude overlay.

## Matched
- The tab's panels (CREATE, MODIFY, CONFIGURE, CONSTRUCT, INSPECT, INSERT, ASSEMBLE, SELECT) and the
  CREATE > Pattern submenu (`ui:sheet:tab-panels`).
- Re-verified from earlier waves: `ui:dialogs:coil-rows`, `ui:dialogs:pipe-rows` (as opened),
  `ui:dialogs:edit-feature-title`, `ui:browser:redefine-sketch-plane`, `ui:browser:export-dxf-dialog`.

## Differences (17 rows, wave 30)
P1:
- Sheet Metal Rules is a library palette in Fusion ("In this design" + 19 library rules with
  expandable values, pencil/copy on hover, Edit Rule / New Rule / Set as Default / Delete), a command
  dialog with one "Default" rule in FissionOxide, which also dirties the document (`ui:sheet:rules-palette`).
- Fusion's modal Edit Rule window with expression defaults and 2-/3-bend corner conditions
  (`ui:sheet:edit-rule-dialog`); the default rule is ".063 Aluminum (Default)" (`ui:sheet:default-rule`).
- Fusion adds a "Rule: <name>" browser row with a Switch Rule button and sheet metal icons on the root
  and the body (`ui:sheet:browser-rule-row`).
- Fusion's single Flange dialog with Type icons (Flange, Hem, Lofted flange) that opens empty; FissionOxide
  auto-picks the only profile (even a consumed one), and then its Type dropdown and chip x stop
  responding (`ui:sheet:flange-dialog-open`).
- BASE FLANGE rows (Orientation Side 1 / Side 2 / Center, rule dropdown, blue preview, "BaseFlange1")
  (`ui:sheet:base-flange-rows`); EDGE FLANGE rows, manipulator, value box, upward fold
  (FissionOxide folds down, no manipulator) (`ui:sheet:edge-flange-rows`).
- No sheet metal feature can be edited in FissionOxide ("This feature has no editable properties yet")
  (`ui:sheet:edit-feature`).
- Create Flat Pattern: Fusion picks a stationary face and enters the FLAT PATTERN SOLID / SURFACE
  environment with Bend Lines and FINISH FLAT PATTERN; FissionOxide shows an info panel
  (`ui:sheet:flat-pattern-environment`), Export Flat Pattern dialog (`ui:sheet:export-flat-pattern`),
  Flat Pattern browser row and its menu (`ui:sheet:flat-pattern-row`).
- Unfold rows (Reference, Bends, Unfold all bends) and the temporary REFOLD ribbon panel whose Refold
  runs without a dialog (`ui:sheet:unfold-refold`).

P2:
- Ribbon quick buttons and the CREATE/MODIFY lists (extra FissionOxide entries, Join by bend in place of
  Sheet Metal Chamfer, one extra separator) (`ui:sheet:ribbon-lists`).
- Fold, Convert to Sheet Metal, Rip, Corner Closure dialogs as opened, and Hem rows after the pick
  (`ui:sheet:fold-dialog`, `ui:sheet:convert-dialog`, `ui:sheet:rip-corner-dialogs`, `ui:sheet:hem-rows`).

Blocked: Join by bend, Sheet Metal Fillet, Sheet Metal Chamfer show Fusion's subscription card
(`ui:sheet:subscription-commands`).

## Batches
| Batch | Items | Keys |
|---|---|---|
| ui-sheet-rules-library | 4 | rules-palette, edit-rule-dialog, default-rule, browser-rule-row |
| ui-sheet-flange-dialog | 4 | flange-dialog-open, base-flange-rows, edge-flange-rows, edit-feature |
| ui-sheet-flat-pattern-env | 4 | flat-pattern-environment, export-flat-pattern, flat-pattern-row, unfold-refold |
| ui-sheet-menus-dialogs | 5 | ribbon-lists, fold-dialog, convert-dialog, rip-corner-dialogs, hem-rows |

## Not covered
Contour flange and Lofted Flange after a pick, Fold / Convert / Rip / Corner Closure after their picks,
the flat pattern canvas itself (Fusion's view in the environment stayed a beige fill at every zoom,
even after Fit and Home), FLAT PATTERN SURFACE tab and the environment's CREATE/MODIFY lists, New Rule,
Set as Default, Delete rule, Switch Rule completed, timeline right-click menus of sheet metal items,
Derive, Bill of Materials on sheet metal.

## Traps
- Fusion's Sheet Metal Rules is a palette: close it with its Close button; an Edit Rule window is
  modal over the canvas.
- Fusion's flat pattern environment opened at a zoom where the canvas showed only beige; Finish Flat
  Pattern still works from the green button's dropdown.
- FissionOxide's Flange dialog re-picks the consumed profile; pick an edge before starting Flange to
  get an edge flange.
