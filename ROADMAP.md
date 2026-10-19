# CLFusion roadmap

CLFusion covers Fusion's core solid-modeling loop (sketch, constrain, dimension, extrude,
revolve, sweep, loft, holes, fillet, chamfer, shell, draft, pattern, mirror, parameters, timeline
editing, STEP/STL export), sheet metal, assemblies with joints, and drawings. The list below is what's missing compared with Fusion's Design workspace,
roughly in the order it would pay off.

## Next

| Area      | Item                                                                         |
|-----------|------------------------------------------------------------------------------|
| Sketch    | Sketch chamfer; associative projection (follows body changes); break          |
| Sketch    | Profiles from curves that cross mid-span (without trimming first)            |
| Sketch    | Splines, ellipses, text                                                      |
| Features  | Sweep with guide rails / twist; loft rails; threads (cosmetic and modeled)   |
| Features  | Extrude "to object" / "to face", taper angle, thin extrude                   |
| Features  | Press-pull and face offset; split face; split body by a face                 |
| Sheet     | Miter flange from a profile; corner relief shapes; flat pattern as a sketch  |
| Assembly  | Component instances (copies sharing a design); contact sets; motion links    |
| Drawing   | Detail and broken views; ordinate/angle dimensions; GD&T; parts lists        |
| Features  | Variable-radius and setback fillets; chamfer with two distances / angle      |
| Selection | Window/box select, selection filters, select-other (cycle overlapping picks) |
| UI        | Drag manipulators (arrows) for extrude distance and other values             |
| UI        | Transparent preview bodies, section analysis, measure between two entities   |
| View      | GPU renderer (wgpu) for big models; the CPU one stays for the CLI            |

## Later

- T-spline/form modeling, surface workspace
- Appearances/materials and a render workspace
- MCP server so AI agents can drive the command set directly
- Background regeneration so long timelines don't block the UI
