# CLFusion

A parametric 3D CAD app in the spirit of Autodesk Fusion 360, written in Rust on the
OpenCASCADE kernel. Sketch, constrain, dimension, extrude and fillet your way to a part, then
change a parameter and watch the whole history rebuild.

![CLFusion with a parametric L-bracket open](docs/images/clfusion-bracket.png)

## What works today

- **Parametric timeline.** Every feature is recorded; double-click one to edit it, roll the
  marker back, suppress or delete features. A failing feature turns red without stopping the
  rest of the timeline, as in Fusion.
- **User parameters and expressions.** Any value field takes `40 mm`, `width / 2 + 3`,
  `1.5 in`, `30 deg` or `sqrt(a^2 + b^2)`.
- **Constrained sketches.** Line/polyline, two-point and centre rectangles, circles, 3-point
  and centre arcs, polygons, slots and points; 20 constraint types (coincident, horizontal,
  vertical, parallel, perpendicular, tangent, equal, concentric, midpoint, collinear,
  symmetric, fix, point-on-curve); driving dimensions (distance, horizontal/vertical,
  radius, diameter, angle, point-to-line). The solver reports degrees of freedom, colours
  free geometry blue, and you can drag points.
- **Profiles.** Closed regions are found automatically, including holes and islands, so a
  rectangle with a circle in it gives you the ring and the disc.
- **Solid features.** Extrude (one side, symmetric, two sides, through all; new body, join,
  cut, intersect), revolve, fillet, chamfer, shell, box/cylinder/sphere/torus, offset
  construction planes, mirror, rectangular and circular patterns, move/rotate, combine.
- **More solid features.** Loft (smooth or ruled, open or closed), sweep along a path of lines
  and arcs, holes at sketch points (simple, counterbore, countersink, drill point, through all),
  split body by a plane, draft.
- **Sheet metal.** Sheet metal rules (thickness, bend radius, K-factor, driven by parameters);
  Flange from closed profiles, Contour Flange from an open path (corners become bends), Edge
  Flange (any angle, adjacent/inside/outside bend position, offsets), Hem, Bend along a sketch
  line, Convert to Sheet Metal for thin solids, Unfold in the timeline, and a flat pattern with
  bend lines exported as DXF (also STEP/STL). The unfolder works on the B-rep, so holes and cuts
  anywhere — even across bends — come out in the flat pattern.
- **Sketch on faces.** Sketches and features follow the faces and edges they were built on
  when the model changes: make the block taller and the hole on its top face moves up.
- **Exchange.** STEP import/export, STL and OBJ export. Designs save as readable JSON
  (`.clf`).
- **Scriptable.** Everything is a JSON command, so the GUI, the CLI and scripts (or an AI
  agent) all drive the same engine.

## Build and run

You need Rust (stable), CMake and a C++17 compiler. The first build compiles OpenCASCADE 7.8
from source into `target/OCCT` (about 20 minutes on 4 cores); later builds reuse it.

```sh
cargo run --release -p clfusion                         # the app
cargo run --release -p clfusion -- examples/bracket.clf # open a design
```

Set `CLF_OCCT_DIR` to an existing OCCT 7.8 install (`include/` and `lib/`, static libraries)
to skip the source build.

On Linux the app needs the usual X11/Wayland libraries (`libxkbcommon-x11`, Mesa/OpenGL).

## Using it

| Action                 | How                                                         |
|------------------------|-------------------------------------------------------------|
| Orbit / pan / zoom     | Right-drag / middle-drag (or Shift+right-drag) / wheel      |
| Standard views         | Click the ViewCube faces; ⌂ or Home for the home view       |
| New sketch             | Select a planar face, then Sketch, or pick XY/XZ/YZ         |
| Sketch tools           | L line, R rectangle, C circle, A arc, P point, D dimension  |
| Angle dimension        | Dimension tool: Shift-click a line, then click another line |
| Construction geometry  | Select curves, press X                                      |
| Extrude / Fillet       | E / F (select profiles or edges first)                      |
| Edit a feature         | Double-click it in the timeline                             |
| Undo / Redo / Save     | Ctrl+Z / Ctrl+Y / Ctrl+S                                    |
| Fit view               | F6                                                          |

## Sheet metal

```sh
clfusion-cli run examples/sheet_bracket.jsonl --save bracket.clf
clfusion-cli exec bracket.clf '{"cmd": "sheet.flat_pattern", "path": "bracket-flat.dxf"}'
```

Sheet metal bodies are ordinary solids that carry their rule, so every other feature (holes,
cuts, fillets, patterns) works on them. Bends are true cylinders: the flat length of a bend is
`angle × (bend radius + K × thickness)`. Edge flange height is measured from the outside of the
sheet to the tip (Fusion's default "outer" datum). The flat pattern DXF has the outline on layer
`OUTLINE` and bend centre lines on `BEND_UP` / `BEND_DOWN`.

## Headless CLI and scripting

```sh
clfusion-cli run examples/bracket.jsonl --save bracket.clf --export bracket.step --png bracket.png
clfusion-cli info bracket.clf
clfusion-cli exec bracket.clf '{"cmd": "param.set", "name": "width", "expr": "80"}' --save bracket.clf
clfusion-cli export bracket.clf bracket.stl
clfusion-cli render bracket.clf top.png --view top
clfusion-cli commands        # every command with its parameters
```

A script is one JSON command per line. Faces and edges can be named by a point on them, so
scripts don't depend on internal numbering:

```json
{"cmd": "sketch.create", "plane": {"point": [40, 0, 6], "normal": [0, 0, 1]}}
{"cmd": "sketch.circle", "center": [45, 15], "diameter": "hole"}
{"cmd": "extrude", "extent": "through_all", "flip": true, "operation": "cut"}
{"cmd": "fillet", "edges": [{"point": [6, 0, 6]}], "radius": 4}
```

## Architecture

| Crate               | Role                                                                   |
|---------------------|------------------------------------------------------------------------|
| `clf-expr`          | Unit-aware expressions and parameter evaluation                        |
| `clf-sketch`        | Sketch geometry, constraint solver, profile (region) detection         |
| `clf-kernel`        | Own cxx bridge to OpenCASCADE: modeling, meshing, STEP/STL/OBJ         |
| `clf-model`         | The design document, feature timeline, regeneration and references     |
| `clf-render`        | CPU rasterizer with depth and picking buffers (same images in the CLI) |
| `clf-engine`        | Session, undo/redo, the JSON command set, scene building               |
| `clf-ui`            | The egui desktop UI                                                    |
| `apps/clfusion`     | Desktop app                                                            |
| `apps/clfusion-cli` | Headless runner, exporter and renderer                                 |

Why OpenCASCADE: the pure-Rust B-rep kernels (truck, monstertruck) fail the coplanar booleans
that everyday modeling depends on (a boss standing on a face, a hole flush with a face).
OCCT handles them, plus fillets, shells and STEP. The bridge (`crates/kernel/cpp`) catches
every OCCT exception, so a failed fillet shows as a red timeline feature and never crashes
the app. The stock `opencascade` crate aborts the process when a fillet fails.

## Licence

CLFusion's own code is MIT OR Apache-2.0. It links OpenCASCADE, which is LGPL-2.1 with the
OCCT exception; keep that in mind if you distribute binaries.

See [ROADMAP.md](ROADMAP.md) for what's next.
