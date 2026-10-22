# Parity batch: ui-status-readouts

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 7) on a one-body design in each app (FissionOxide `main` at
`42dec181`), reading the status text at the lower right of the canvas after each selection. What
already matches: browser rows read "Origin Point | X: 0.00 Y: 0.00 Z: 0.00 in", "X Axis",
"XY Plane", "Body1", "Sketch2"; a timeline feature reads "Extrude1"; a face reads
"1 Face | Area : 1.44 in^2"; a sketch point "Sketch Point | X: -2.669 Y: 0.50 Z: 0.444 in"; an
empty click clears the text. The readout code is `crates/ui/src/panels.rs:2954`.

## status readout on hover (P2)
- key: `ui:chrome:status-readout-on-hover`
- Fusion: Hovering a face, edge or browser row prints nothing; the readout appears only once something is selected.
- FissionOxide: Hovering the body's top face already prints "1 Face | Area : 473.77 mm^2" before any click.
- Start here: `crates/ui/src/panels.rs:2954` (the readout takes the hover candidate as well as the selection; print only the selection).

## status mixed selection (P2)
- key: `ui:chrome:status-mixed-selection`
- Fusion: A face plus an edge (Ctrl+click) reads "1 Face 1 Edge | Min Distance : 0.10 in"; a sketch point plus a sketch line reads "1 Sketch Point 1 Sketch Line | Min Distance : 1.196 in" (the kinds listed in order, then the least distance between them).
- FissionOxide: After selecting an edge ("1 Edge | Length : 10.00 mm"), Ctrl+click on a face reads "1 Face | Area : 473.77 mm^2": either the face replaced the edge or the readout prints one kind only.
- Start here: `crates/ui/src/panels.rs:2954` (mixed-kind selections: count each kind, then "Min Distance" between the two entities), `crates/ui/src/viewport.rs:3436` (the least-distance helper already used for two curves).

## status two faces angle (P2)
- key: `ui:chrome:status-two-faces-angle`
- Fusion: Two perpendicular faces read "2 Faces | Angle : 90.0 deg" (two parallel faces read a distance instead).
- FissionOxide: Not re-observed this session (the Ctrl+click on the side face picked a consumed sketch's line instead, see `ui:browser:consumed-sketch-hidden`); the ledger row `ui:canvas:multi-select-status` says two faces read "2 Faces" with no measure.
- Start here: `crates/ui/src/panels.rs:2954` (two planar faces: "Angle : <deg>" between their normals, or the distance for parallel faces).

## escape clears feature selection (P2)
- key: `ui:chrome:escape-clears-feature-selection`
- Fusion: Escape after clicking a timeline feature clears the selection: the readout empties and nothing stays highlighted.
- FissionOxide: After clicking the Extrude1 timeline icon, Escape leaves the body drawn blue and the readout "Extrude1"; an empty canvas click clears it.
- Start here: `crates/ui/src/lib.rs` (the Escape handler clears `selection` but not the timeline's selected feature), `crates/ui/src/panels.rs` (timeline selection state).
