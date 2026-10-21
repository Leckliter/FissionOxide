# Parity run, 2026-10-21 (third pass): selection and hover colours

Both apps at their defaults, each window placed left of the Claude overlay, colours read from
the screen with a pixel sampler (`System.Drawing` `CopyFromScreen` + `GetPixel`) after the
pointer rested on the target for two seconds. Fusion's hover effects are subtle enough that a
screenshot misses them; the sampler does not.

## Colour table (sRGB of a lit face or a line pixel)

| State | Fusion | FissionOxide |
|---|---|---|
| Face, plain (front / top) | (162,162,155) / (165,173,165) | (142,145,150) / (177,181,186) |
| Face, hovered | lightened: (190,190,185) / (192,197,193) | blue tint: (98,145,182) |
| Face, selected | (81,132,205) / (83,138,210) | (28,109,168) |
| Face, selected and hovered | (133,169,220) | (98,145,182), same as hover |
| Edge, plain | 1 px light grey silhouette (179,178,173) | dark (35,38,44) |
| Edge, hovered | 2 px black (10,10,10) | 2 px light blue (128,190,238) |
| Edge, selected | (10,131,252) with halo (214,235,255) | (36,142,220) |
| Body selected from the browser | every face as a selected face | lighter blue-grey (91,128,158) / (113,160,197) |
| Sketch curve on a face, hovered | curve thickens, status "Sketch Arc" | the face tints instead; curve not pickable |
| Fillet: hovered edge / picked edge | black / bright blue with halo, body unchanged | light blue / (30,110,235), whole body tinted (116,132,154) |
| Extrude: hovered profile / picked | black outline, fill lightened (192,197,193) / fill (83,138,210) with red outline | (96,148,196) / (39,118,185), no outline change |

## What matched
- Click selects, Escape and an empty click clear, the status bar names the pick with area or
  length, and the browser hover highlights the body in both.
- Edge and face hit-testing find the same entities at the same spots.

## Differences
Eight rows, all in the new batch `ui-selection-colors` (`map-wave6.json`): face hover (P1,
lighten instead of tint), edge hover (P1, black instead of blue), sketch-curve priority over
faces (P1), face/edge/body selected colours, selected-and-hovered state, command-mode pick
colours and the preview tint, and Extrude opening with every region preselected.

## Not covered
Vertex hover and selection (the pick at the body's bottom corner returned the 40 mm edge in
FissionOxide; Fusion's vertex was not hit either), construction planes and axes, sketch points,
hover in a sketch, selection filters and priority settings, window and crossing selection
colours, Select Other. These stay in the coverage checklist.
