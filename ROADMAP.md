# FissionOxide roadmap

FissionOxide covers Fusion's core solid-modeling loop (sketch, constrain, dimension, extrude,
revolve, sweep, loft, holes, threads, coils, ribs, fillet, chamfer, shell, draft, pattern,
mirror, parameters, timeline editing), construction geometry, surfaces, mesh tools, sheet metal,
assemblies with joints, drawings, inspection and analysis, a 2.5-axis Manufacture workspace, the
common exchange formats, a versioned data panel and an MCP server for AI agents. The list below
is what's missing compared with Fusion, roughly in the order it would pay off.

## Next

| Area      | Item                                                                         |
|-----------|------------------------------------------------------------------------------|
| Features  | Sweep with two guide rails or a guide surface; replace face with curved source faces; extrude to a vertex |
| Features  | Configuration rules (driven by other columns); configured components inside assemblies; move face for curved faces and rotations in direct edits |
| Surface   | Extend with corner closing between adjacent edges; untrim with an extension amount; trim by picking pieces to remove in the viewport |
| Sheet     | A flat pattern sketch that follows the model; rounded bends where press-brake folds converge on a corner (they stay sharp); splines in flat pattern DXFs |
| Assembly  | Contact sets during viewport drags; motion links and cams inside flexible sub-assemblies; roller (curved-face) cam followers and cams pushing along a curve |
| Drawing   | Weld symbol contours, finishes and back/melt-through welds; composite frames in the Feature Control Frame dialog; datum target lines; picking dimension points on the sheet |
| UI        | Manipulators for more commands (shell, draft, patterns); a triad that turns about any axis |
| View      | Silhouette edges and hidden-line (dashed) display styles; GPU frustum culling and level of detail for huge assemblies |
| Manufacture | 3D adaptive roughing; pencil, steep-and-shallow and contour finishing; holder and fixture collision checks; arcs (G2/G3) in G-code; turning and 4/5-axis |
| Plugins   | Installable `.foxplugin` packages running sandboxed; see [docs/PLUGINS.md](docs/PLUGINS.md) |
| Tracing   | Canvas (an image kept on a plane, calibrated); centre-line tracing for engraving; constraints on traced geometry; see [docs/TRACING.md](docs/TRACING.md) |

## Files and data

| Area     | Item                                                                           |
|----------|--------------------------------------------------------------------------------|
| Formats  | ACIS SAT/SMT, Parasolid (x_t), DWG, FBX: need closed-source or licensed SDKs   |
| Formats  | STEP: nested sub-assemblies and instanced parts kept as such (flattened into components today); colours per face |
| Data     | A hub server (sharing works through sync folders today); real-time presence and locking of designs being edited |

## Later

- Form workspace (T-spline sculpting)
- Render workspace (materials beyond flat appearance colours, ray tracing)
- Simulation (FEA), generative design, electronics
