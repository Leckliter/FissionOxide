# CLFusion roadmap

CLFusion covers Fusion's core solid-modeling loop (sketch, constrain, dimension, extrude,
revolve, sweep, loft, holes, threads, coils, ribs, fillet, chamfer, shell, draft, pattern,
mirror, parameters, timeline editing), construction geometry, surfaces, mesh tools, sheet metal,
assemblies with joints, drawings, inspection and analysis, a 2.5-axis Manufacture workspace, the
common exchange formats, a versioned data panel and an MCP server for AI agents. The list below
is what's missing compared with Fusion, roughly in the order it would pay off.

## Next

| Area      | Item                                                                         |
|-----------|------------------------------------------------------------------------------|
| Sketch    | Elliptical arcs as their own curve and fit-point splines that stay fit-point after trim/extend (both become Bézier chains today); fonts from disk for text |
| Features  | Sweep with two guide rails or a guide surface; replace face with curved source faces; extrude to a vertex |
| Features  | Configuration rules (driven by other columns); configured components inside assemblies; move face for curved faces and rotations in direct edits |
| Surface   | Extend with corner closing between adjacent edges; untrim with an extension amount; trim by picking pieces to remove in the viewport |
| Sheet     | Rounded bends between press-brake lofted flange facets; later cuts in a lofted flange's flat pattern; a flat pattern sketch that follows the model |
| Assembly  | Pick saved joint origins and joints in the viewport (dialogs name them today); screw and cam motion; contact sets during viewport drags; nested sub-assemblies from inserted designs |
| Drawing   | Baseline and chain dimensions; GD&T composite frames and datum targets; weld symbols; revision table; multiple sheets |
| Selection | Freeform and paint selection; vertex picking                                 |
| UI        | More drag manipulators (fillet radius, press-pull, move/rotate triad); transparent preview bodies |
| View      | GPU renderer (wgpu or OpenGL through egui) for big models; the CPU one stays for the CLI |
| Manufacture | 3D adaptive roughing; pencil, steep-and-shallow and contour finishing; holder and fixture collision checks; arcs (G2/G3) in G-code; turning and 4/5-axis |
| Plugins   | Installable `.clfplugin` packages running sandboxed; see [docs/PLUGINS.md](docs/PLUGINS.md) |
| Tracing   | Canvas (an image kept on a plane, calibrated); centre-line tracing for engraving; constraints on traced geometry; see [docs/TRACING.md](docs/TRACING.md) |

## Files and data

| Area     | Item                                                                           |
|----------|--------------------------------------------------------------------------------|
| Formats  | ACIS SAT/SMT, Parasolid (x_t), DWG, FBX: need closed-source or licensed SDKs   |
| Formats  | DXF splines and ellipses as entities (exported as lines today)               |
| Formats  | STEP with colours, names and assembly structure (needs OCCT's XCAF toolkit)    |
| Data     | Sharing a hub between machines (sync folder or server), comments on versions   |
| Data     | Visual diff between two versions in the viewport                              |

## Later

- Form workspace (T-spline sculpting)
- Render workspace (materials beyond flat appearance colours, ray tracing)
- Simulation (FEA), generative design, electronics
