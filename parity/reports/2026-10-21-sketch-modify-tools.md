# Parity run, 2026-10-21 (prober 4): every sketch MODIFY tool

Fusion (inches, Sketch4 on the XY plane: a rectangle, a circle drawn across its right edge, two
short lines) and FissionOxide `main` at `feea62e` (mm, a fresh Sketch1 on XY with the same
geometry) driven in turn with desktop control, each window left of the Claude overlay. Every row
of the sketch MODIFY dropdown was hovered for its tooltip, then run with the same picks in both
apps: Fillet, Chamfer (Equal Distance, plus the submenu's tooltips), Blend Curve, Offset (on a
circle and on a line, with the Direction list), Trim by drag, Extend, Break, Sketch Scale,
Move/Copy and Change Parameters.

## Headline

| | Count |
|---|---|
| New differences recorded | 20 |
| P1 | 7 |
| P2 | 13 |
| New batches | 5 (`map-wave9.json`) |
| Match rows | 6 |
| Re-verified | 2 verified (`ui:sketch:trim-hover-color`, `ui:dialogs:offset-sketch-fields`), 1 reopened (`ui:dialogs:move-set-pivot`) |

## The shape of the gap

Fusion's sketch Fillet, Chamfer and Blend Curve are dialog-less pick-pick tools: the pointer prompt
asks for the first then the second curve, the result is applied the moment the second pick lands,
a floating value box (value selected, with a vertical-ellipsis menu) and a blue arrow manipulator
appear, and the prompt goes back to the first pick so the tool can be used again. Chamfer and Blend
Curve put their type switches in the Sketch Palette's Feature Options row. FissionOxide gives all
three a docked dialog with a Select row and a value field, prompts "Select two lines or arcs that
meet" then "Hold Ctrl to modify selection", and only previews until OK or Enter.

## What matched

- Trim: the hovered piece is dark purple in both (the `ui:sketch:trim-hover-color` fix is in),
  dragging across a curve trims it, and the tool stays active.
- Extend lengthens the line to the next curve; Break splits a curve at its crossings; both stay
  active afterwards and add coincident glyphs.
- Typed digits reach the Fillet radius and Chamfer distance and Enter commits in both apps.
- Offset shows an arrow manipulator and a floating value box with the value selected.
- The OFFSET dialog now has Curves, Chain Selection, Direction, Offset Position and Flip
  (`ui:dialogs:offset-sketch-fields` verified).

## P1

1. **Fillet flow** (`ui:sketch:fillet-flow`): no dialog in Fusion; "Select first line or arc" /
   "Select second line or arc"; applied on the second pick with the last radius, value box + arrow,
   prompt returns to the first pick. FissionOxide: SKETCH FILLET dialog, preview until Enter.
2. **Chamfer flow** (`ui:sketch:chamfer-flow`): same pattern, plus Feature Options icons in the
   palette, two driven "0.10" distance dimensions in the result and a warning toast. FissionOxide:
   SKETCH CHAMFER dialog, bare chamfer line, status left at "1 Sketch Point".
3. **Blend Curve flow** (`ui:sketch:blend-curve-flow`): Fusion blended the two top ends with a
   smooth arch, G1/G2 toggles in the palette, red preview on hover; FissionOxide blended the end
   nearest the click into a looping S-curve with 14 new degrees of freedom and no preview.
4. **Offset default and follow** (`ui:sketch:offset-default-follow`): Fusion starts at 0.00 in on
   the curve ("Modify offset distance or side settings"); FissionOxide's offset follows the pointer
   from the start (53.5 mm, then -6 mm).
5. **Value box stale selection** (`ui:sketch:value-box-stale-selection`): typing "3" after the box
   changed from "53.5 mm" to "-6 mm" left "3m", so the offset landed 3 metres away.
6. **Sketch Scale dialog** (`ui:sketch:scale-dialog`): Fusion picks a point and then shows Scale
   Factor with an arrow and value box; FissionOxide has typed Point X/Y fields and no manipulator.
7. **Move/Copy shortcut** (`ui:sketch:move-copy-shortcut`): M does nothing in FissionOxide's sketch
   and the menu row shows no hint.

## P2

- Chamfer submenu order and names ("Two Distance Chamfer" last in Fusion;
  `ui:sketch:chamfer-submenu`).
- Fillet result glyphs: Fusion's tangent glyphs sit beside the arc ends and the arc centre point is
  drawn (`ui:sketch:fillet-result-glyphs`).
- OFFSET rows: Direction has One Side / Two Sides / Symmetric with icons, Flip is an icon button,
  the label is Distance, Chain Selection is unchecked with a pre-selected curve, and the dialog
  docks left of the palette (`ui:sketch:offset-dialog-rows`).
- The selection readout is painted under the Sketch Palette when a dialog pushes it down
  (`ui:sketch:status-readout-under-palette`).
- Trim prompt "Select curve section to trim" (`ui:sketch:trim-prompt`); Extend has a light-red
  preview of the added piece (`ui:sketch:extend-preview`); Break's prompt is "Select curve to Break"
  with red x marks at the break points (`ui:sketch:break-preview`).
- "1 warning(s) / Constraints and/or dimensions were removed during operation." toast after
  Chamfer, Trim and Break (`ui:sketch:modify-warning-toast`).
- MOVE/COPY in the sketch: icon Move Object, Set Pivot, value rows only after a pick, OK disabled
  until then (`ui:sketch:move-copy-dialog`).
- PARAMETERS: toolbar icons, Automatic Compute, document level in the tree, OK closes it; the
  FissionOxide window stays open behind other commands (`ui:sketch:change-parameters-dialog`).
- Tooltip texts of all ten MODIFY rows (`ui:sketch:modify-tooltips`); the hovered row's
  vertical ellipsis renders as a missing-glyph box (`ui:menus:row-options-glyph`); menu tooltips
  are dark panels right of the menu in Fusion (`ui:menus:tooltip-style`).

## Not conclusive (no row written)

- Fusion's Trim drag removed the whole inner circle, while FissionOxide removed only the arc inside
  the rectangle. Fusion's hover had highlighted the whole circle; its document also held the offset
  circle with an offset constraint, FissionOxide's did not (the 3 m offset was undone). Re-check
  Trim on a circle crossed by a line without any offset constraint before writing a row.
- Blend Curve's end choice: one trial only. Both lines' top ends were the pair closest to each
  other; the pick on the vertical line was near its bottom. Try a pick near the top as well.
- Fusion's Chain Selection checkbox was unchecked both times, but both times a curve had been
  pre-selected before O; with no pre-selection it may default on.

## Batches

| Batch | Items | Files |
|---|---|---|
| `ui-sketch-fillet-chamfer-blend` | 5 | dialogs.rs, viewport.rs, panels.rs, menus.rs, commands.rs |
| `ui-sketch-offset-value-box` | 4 | dialogs.rs, value_box.rs, viewport.rs, panels.rs |
| `ui-sketch-trim-extend-break` | 4 | viewport.rs, commands.rs, panels.rs |
| `ui-sketch-scale-move-params` | 4 | forms.rs, dialogs.rs, menus.rs, lib.rs, params_ui.rs |
| `ui-sketch-modify-tooltips` | 3 | tips.rs, panels.rs, menus.rs |

## Not covered

- Offset with Two Sides / Symmetric, Chain Selection on a rectangle chain, the Flip button, and
  the offset dimension's result in FissionOxide (`ui:sketch:offset-dimension` stays
  `fixing-merged`: the typed value failed before it could be checked).
- Distance and Angle / Two Distance chamfers beyond their tooltips; Fillet on arcs; Scale to Size,
  Edit Text, Explode Text, Remove Duplicates, Clean Up (FissionOxide-only rows).
- Dragging the arrow manipulators; the value box's ellipsis menu; Tab between boxes.
- Edit of the results (double-click), right-click marking menus on the results, undo entries.

## Traps met this session

- The Sketch Palette covers Fusion's canvas from x = 730 (screenshot px): a circle centre placed at
  (740,470) landed on the palette's rows and nothing happened. Keep picks left of x = 720.
- The Parameters window in FissionOxide stays open and swallows canvas clicks until its x is
  clicked; Escape does not close it.
- Fusion's O with a pre-selected curve opens OFFSET with Distance 0.00 in: type the distance,
  nothing moves until you do.
