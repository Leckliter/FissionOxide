# Parity probe: SOLID > CREATE dialogs (2026-10-22, prober 8)

Same session and documents as `2026-10-22-origin-construction.md`; FissionOxide `main` at
`0c453dd1`. Every entry of SOLID > CREATE except Extrude and Hole (first pass done earlier), Create
Form (a workspace), Derive (needs a saved design) and Create PCB was opened in both apps. In
Fusion, Box, Cylinder, Sphere, Torus and Coil were completed and Box was edited from the timeline;
Pipe, Revolve, Thread, Mirror and Circular Pattern were driven to their preview. In FissionOxide,
Box was completed and edited, a cylinder made for Thread, a sketch made for Revolve.

## Re-verified (from the progress list)
Verified: `ui:canvas:hover-vertex`, `ui:canvas:vertex-filter-default`,
`ui:sketch:create-tool-cursor-prompts` (rectangle, center rectangle, spline, conic prompts),
`ui:sketch:center-rect-construction`, `ui:sketch:spline-accept-check`, `ui:sketch:conic-rho-box`.
Not re-checked: `ui:sketch:horizontal-conflict`, `ui:sketch:dimension-label-and-hover`,
`ui:sketch:tangent-arc-snap-marker`, `ui:toolbar:responsive-collapse`,
`ui:canvas:vertex-marker-style`.

## What matched
`ui:toolbar:solid-create-menu-list` (every entry, order, separator, shortcut hint and submenu),
`ui:dialogs:revolve-single-profile-auto`, `ui:dialogs:escape-cancels-create-dialogs`.

## How Fusion's Create dialogs behave (the pattern behind most rows)
Fusion dialogs open nearly empty: selection chips ("Select", OK greyed) and icon toggles, with the
value rows appearing only after the picks; the prompt follows the cursor; values get a floating
box and a manipulator; Operation switches to Cut when the preview starts inside a body. Most
FissionOxide dialogs open with every row at once, use sketch dropdowns and "(empty = ...)" text
fields where Fusion uses chips and dropdowns, preset a plane or axis, enable OK with a red error
line, and add a Preview checkbox Fusion does not have.

## Differences
P1: `ui:dialogs:primitive-plane-pick-flow` (Fusion picks a plane, draws the base, then opens the
dialog), `ui:dialogs:primitive-rows` (Length/Width/Height, Diameter, Inner/Torus Diameter and
Position against Width (X)/Depth (Y), Radius, Major/Minor radius and Position X, Y),
`ui:dialogs:pipe-rows`, `ui:dialogs:revolve-fields` (reopened into a new batch: Project Axis,
Extent Type "Partial", no preset axis, angle handle), `ui:dialogs:sweep-rows`,
`ui:dialogs:loft-rows`, `ui:dialogs:operation-auto-cut`, `ui:dialogs:thread-rows`,
`ui:dialogs:mirror-rows`, `ui:dialogs:pattern-type-switch` (one pattern dialog with a Type row;
FissionOxide's Circular Pattern patterns "all bodies" about a preset Z axis as soon as it opens).

P2: `ui:dialogs:primitive-manipulators`, `ui:dialogs:coil-rows`, `ui:dialogs:edit-feature-title`
("EDIT FEATURE" against "EDIT BOX"), `ui:dialogs:rib-rows`, `ui:dialogs:web-rows` (FissionOxide's
Web reuses Rib's error text), `ui:dialogs:emboss-rows`, `ui:dialogs:thicken-rows`,
`ui:dialogs:boundary-fill-rows`, `ui:dialogs:path-pattern-rows`, `ui:dialogs:copy-ghost-preview`,
`ui:dialogs:base-feature-mode` (a ribbon mode in Fusion, an empty dialog in FissionOxide),
`ui:dialogs:joint-origin-rows`.

Pointers are in each batch item. Fusion's Coil, Rib and Web work on this account; the command
ledger rows that say `blocked` mean the API cannot script them, and now carry a note.

## Retractions
None.

## Batches (wave 23)
| batch | items | keys |
|---|---|---|
| `ui-primitive-dialogs` | 6 | primitive-plane-pick-flow, primitive-rows, primitive-manipulators, coil-rows, pipe-rows, edit-feature-title |
| `ui-sweep-loft-revolve` | 4 | revolve-fields, sweep-rows, loft-rows, operation-auto-cut |
| `ui-rib-emboss-thread` | 6 | rib-rows, web-rows, emboss-rows, thread-rows, thicken-rows, boundary-fill-rows |
| `ui-pattern-mirror-misc` | 6 | mirror-rows, pattern-type-switch, path-pattern-rows, copy-ghost-preview, base-feature-mode, joint-origin-rows |

## Not covered
The rows Fusion shows only after picks for Sweep, Loft, Rib, Web, Emboss, Thicken, Boundary Fill,
Joint Origin and Pattern on Path (no suitable geometry was built); the results, timeline icons and
undo names of each command; right-click menus of the results; Create Form, Derive, Create PCB; New
Component is under ASSEMBLE in Fusion (already verified as `ui:assemble:new-component`). The
`ui:dialogs:revolve-all-regions` row (several profiles) was not re-probed and still has no batch.
