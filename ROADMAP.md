# CLFusion roadmap

CLFusion covers Fusion's core solid-modeling loop (sketch, constrain, dimension, extrude,
revolve, sweep, loft, holes, fillet, chamfer, shell, draft, pattern, mirror, parameters, timeline
editing), sheet metal, assemblies with joints, drawings, the common exchange formats and a
versioned data panel. The list below is what's missing compared with Fusion's Design workspace,
roughly in the order it would pay off.

## Next

| Area      | Item                                                                         |
|-----------|------------------------------------------------------------------------------|
| Sketch    | Ellipses, text, break; splines in trim/offset; spline tangency constraints   |
| Features  | Sweep with guide rails / twist; loft rails; threads (cosmetic and modeled)   |
| Features  | Press-pull of curved faces; split face; setback fillets; extrude to a curved face |
| Sheet     | Miter flange from a profile; corner relief shapes; flat pattern as a sketch  |
| Assembly  | Contact sets; motion links between joints; rigid groups                      |
| Drawing   | Broken views; ordinate/angle dimensions; GD&T; balloons                      |
| UI        | Toolbar buttons and dialogs for sheet metal, assemblies, drawings and the newer features (engine/CLI/MCP only today) |
| Selection | Window/box select, selection filters, select-other (cycle overlapping picks) |
| UI        | Drag manipulators (arrows) for extrude distance and other values             |
| UI        | Transparent preview bodies, section analysis, measure between two entities   |
| View      | GPU renderer (wgpu) for big models; the CPU one stays for the CLI            |
| Plugins   | Installable `.clfplugin` packages running sandboxed; see [docs/PLUGINS.md](docs/PLUGINS.md) |

## Files and data

| Area     | Item                                                                           |
|----------|--------------------------------------------------------------------------------|
| Formats  | ACIS SAT/SMT, Parasolid (x_t), DWG, FBX: need closed-source or licensed SDKs   |
| Formats  | DXF splines and ellipses; SVG sketch import/export                             |
| Formats  | STEP with colours, names and assembly structure (needs OCCT's XCAF toolkit)    |
| Data     | Sharing a hub between machines (sync folder or server), comments on versions   |
| Data     | Visual diff between two versions in the viewport                              |

## Later

- T-spline/form modeling, surface workspace
- Appearances/materials and a render workspace
- MCP server so AI agents can drive the command set directly
- Background regeneration so long timelines don't block the UI
