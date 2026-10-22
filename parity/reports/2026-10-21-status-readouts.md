# Parity probe: status bar readouts and lower-left messages (2026-10-21, prober 7)

Same session as the earlier reports of the day; FissionOxide `main` at `42dec181`. Browser rows
(origin point, axis, plane, body, sketch), the timeline feature, faces, an edge, sketch points and
a sketch dimension were selected in each app and the lower-right status text read after each.

## Method notes
- In Fusion the sketches on the body's faces take the picks: with the Sketches folder shown, a
  click on the top face selected a sketch point and a Ctrl+click a sketch line. Hide the Sketches
  folder before picking faces and edges.
- In FissionOxide the consumed Sketch1 is still pickable through the body (a side-face Ctrl+click
  picked its line, the bottom corner click picked its point); see
  `ui:browser:consumed-sketch-hidden`.
- Fusion's lower left shows nothing in normal use; its status text sits at the lower right only.

## What matched
`ui:chrome:status-browser-row-readouts`, `status-face-readout`, `status-sketch-point-readout`,
`status-empty-click`. Fusion extras recorded for later: "1 Sketch Dimension : d10 | Radius :
0.228 in", "1 Sketch Point 1 Sketch Line | Min Distance : 1.196 in".

## Differences (all P2)
- `ui:chrome:status-readout-on-hover`: FissionOxide prints the face readout on hover; Fusion only
  after a selection. `crates/ui/src/panels.rs:2954`.
- `ui:chrome:status-mixed-selection`: Fusion "1 Face 1 Edge | Min Distance : 0.10 in"; a
  Ctrl+click on a face after an edge reads "1 Face | Area ..." in FissionOxide.
- `ui:chrome:status-two-faces-angle`: Fusion "2 Faces | Angle : 90.0 deg"; FissionOxide not
  re-observed (ledger says "2 Faces").
- `ui:chrome:escape-clears-feature-selection`: Escape leaves the timeline feature selected (body
  blue, "Extrude1") in FissionOxide.

## Retractions
None.

## Batches
| batch | items | keys |
|---|---|---|
| `ui-status-readouts` | 4 | status-readout-on-hover, status-mixed-selection, status-two-faces-angle, escape-clears-feature-selection |

## Not covered
- An edge alone and a vertex in Fusion (the vertex click missed), a vertex and two faces in
  FissionOxide, construction geometry picked in the canvas, components, a sketch dimension in
  FissionOxide, lower-left messages during commands (`ui:chrome:status-dev-messages` stays
  fixing-merged).
