# FissionOxide roadmap

FissionOxide covers Fusion's core solid-modeling loop (sketch, constrain, dimension, extrude,
revolve, sweep, loft, holes, threads, coils, ribs, fillet, chamfer, shell, draft, pattern,
mirror, parameters, timeline editing), construction geometry, surfaces, mesh tools, sheet metal,
assemblies with joints, drawings, inspection and analysis, a Manufacture workspace (milling up to
simultaneous 5-axis, and turning), the
common exchange formats, a versioned data panel and an MCP server for AI agents. The list below
is what's missing compared with Fusion, roughly in the order it would pay off.

## Next

| Area      | Item                                                                         |
|-----------|------------------------------------------------------------------------------|
| Fabrication | Langmuir FireControl and Ruida posts checked on real machines |
| Add-ins   | Native parts for add-ins: feature types that re-run in the timeline, so LumenArt's tabs and flanges follow when a layer moves (today they are macros); bending flanges with sheet metal Bend instead of leaving a bend line |

## Files and data

| Area     | Item                                                                           |
|----------|--------------------------------------------------------------------------------|
| Formats  | ACIS SAT/SMT, Parasolid (x_t), DWG, FBX: need closed-source or licensed SDKs   |

## Later

