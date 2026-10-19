# CLFusion roadmap

CLFusion covers Fusion's core solid-modeling loop: sketch, constrain, dimension, extrude,
revolve, fillet, chamfer, shell, pattern, mirror, parameters, timeline editing, the common
exchange formats and a versioned data panel. The list below is what's missing compared with Fusion's Design workspace,
roughly in the order it would pay off.

## Next

| Area      | Item                                                                         |
|-----------|------------------------------------------------------------------------------|
| Sketch    | Trim/extend, sketch fillet/chamfer, offset, mirror and pattern in sketches   |
| Sketch    | Project/include edges from bodies; intersect curves that cross mid-span      |
| Sketch    | Splines, ellipses, text                                                      |
| Features  | Sweep and loft; hole feature (counterbore/countersink, threads as cosmetic)  |
| Features  | Extrude "to object" / "to face", taper angle, thin extrude                   |
| Features  | Press-pull and face offset; draft; split body / split face                   |
| Features  | Variable-radius and setback fillets; chamfer with two distances / angle      |
| Selection | Window/box select, selection filters, select-other (cycle overlapping picks) |
| UI        | Drag manipulators (arrows) for extrude distance and other values             |
| UI        | Transparent preview bodies, section analysis, measure between two entities   |
| View      | GPU renderer (wgpu) for big models; the CPU one stays for the CLI            |

## Files and data

| Area     | Item                                                                           |
|----------|--------------------------------------------------------------------------------|
| Formats  | ACIS SAT/SMT, Parasolid (x_t), DWG, FBX: need closed-source or licensed SDKs   |
| Formats  | DXF splines and ellipses; SVG sketch import/export                             |
| Formats  | STEP with colours, names and assembly structure (needs OCCT's XCAF toolkit)    |
| Data     | Sharing a hub between machines (sync folder or server), comments on versions   |
| Data     | Visual diff between two versions in the viewport                              |

## Later

- Components and assemblies with joints and motion
- Drawings (2D views with dimensions; DXF/PDF output)
- Sheet metal, T-spline/form modeling, surface workspace
- Appearances/materials and a render workspace
- MCP server so AI agents can drive the command set directly
- Background regeneration so long timelines don't block the UI
