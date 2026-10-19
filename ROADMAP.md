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
| Sketch    | Break; 2/3-tangent circles; blend curve; splines and ellipses in trim/offset; tangency and curvature constraints on splines; editable (not exploded) text |
| Features  | Sweep with guide rails / twist; loft rails; emboss; boundary fill; replace face; extrude to a curved face; setback fillets |
| Features  | Configurations (table-driven variants); base features (direct editing without history) |
| Surface   | Extend, untrim; trim with a body or sketch; offset with variable distance     |
| Sheet     | Lofted and miter flanges; corner closure; flat pattern as a sketch           |
| Assembly  | Contact sets; motion links; rigid groups; joint origins; insert a component from another design |
| Drawing   | Broken views; ordinate and angle dimensions; GD&T; balloons                  |
| Selection | Freeform and paint selection; vertex picking                                 |
| UI        | More drag manipulators (fillet radius, press-pull, move/rotate triad); transparent preview bodies |
| View      | GPU renderer (wgpu or OpenGL through egui) for big models; the CPU one stays for the CLI |
| Manufacture | 3D adaptive roughing; pencil, steep-and-shallow and contour finishing; holder and fixture collision checks; arcs (G2/G3) in G-code; turning and 4/5-axis |

## Files and data

| Area     | Item                                                                           |
|----------|--------------------------------------------------------------------------------|
| Formats  | ACIS SAT/SMT, Parasolid (x_t), DWG, FBX: need closed-source or licensed SDKs   |
| Formats  | DXF splines and ellipses as entities (exported as lines today); SVG sketch import/export |
| Formats  | STEP with colours, names and assembly structure (needs OCCT's XCAF toolkit)    |
| Data     | Sharing a hub between machines (sync folder or server), comments on versions   |
| Data     | Visual diff between two versions in the viewport                              |

## Later

- Form workspace (T-spline sculpting)
- Render workspace (materials beyond flat appearance colours, ray tracing)
- Simulation (FEA), generative design, electronics
