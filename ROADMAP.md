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
| UI        | Manipulators for more commands (shell, draft, patterns); a triad that turns about any axis |
| View      | Silhouette edges and hidden-line (dashed) display styles; GPU frustum culling and level of detail for huge assemblies |
| Manufacture | Holder and fixture collision checks; turning and 4/5-axis; rest roughing after a larger tool in 3D; more pencil passes either side of a corner |
| Plugins   | Installable `.foxplugin` packages running sandboxed; see [docs/PLUGINS.md](docs/PLUGINS.md) |
| Fabrication | Nesting on sheets (`jagua-rs`); 2D cuts kept in the design with a viewport preview; Langmuir FireControl and Ruida posts checked on real machines; per-contour lead positions |
| Add-ins   | Compiled-in add-ins under `addins/` with an add-in manager; an add-in's tab can list built-in commands in workflow order; LumenArt as the first (see [docs/LUMENART.md](docs/LUMENART.md)) |
| Sketch    | Remove duplicate/overlapping curves; Clean Up (break links, delete constraints, splines to arcs/lines); Scale to Size and Fit in Box; a sketch library placed where you click |
| Fabrication | Cut-service checks with shop profiles (SendCutSend, OSH Cut, Ponoko); cut estimate (area, cut length, pierces); every part's flat pattern on one sheet / DXF |
| View      | Exploded views; batch render one image per component |
| Features  | New component option on Extrude |

## Files and data

| Area     | Item                                                                           |
|----------|--------------------------------------------------------------------------------|
| Formats  | ACIS SAT/SMT, Parasolid (x_t), DWG, FBX: need closed-source or licensed SDKs   |
| Formats  | STEP: nested sub-assemblies and instanced parts kept as such (flattened into components today); colours per face |

## Later

- Form workspace (T-spline sculpting)
- Render workspace (materials beyond flat appearance colours, ray tracing)
- Simulation (FEA), generative design
