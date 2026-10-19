# Reference parts

Parts whose measurements are known independently of CLFusion. Each directory is one case:

- `expected.json`: what the finished part must measure, and where the numbers come from.
- `recipe.json`: the part described in a neutral, CAD-independent format (sketches in
  sketch-local coordinates, features with their parameters, faces and edges picked by a
  world-space point).
- `part.jsonl`: the same part as a CLFusion command script (one JSON command per line, as
  `clfusion-cli run` reads it). Generated from `recipe.json` by `tools/reference-import`, then
  edited by hand where needed.

`cargo test -p clf-engine --test reference_parts` builds every part that has a `part.jsonl` and
compares it with `expected.json`. Each built part must also be a valid solid that survives a STEP
round trip. Set `CLF_REFERENCE_REPORT=1` to rewrite `docs/reference-parts.md`.

`baseline.txt` lists the cases that pass. The test fails when a listed case stops passing; when a
new case passes, add it to the list (the test prints the line to add).

## expected.json

| Field | Meaning |
|---|---|
| `source`, `method` | `"analytic"` (closed-form geometry; `method` gives the formulas) or a FreeCAD version (built in FreeCAD from the same recipe). |
| `body_count` | Number of solid bodies. |
| `volume_mm3`, `area_mm2` | Totals over all bodies. |
| `center_of_mass` | Volume-weighted centroid of all bodies. |
| `bbox` | Axis-aligned bounding box of all bodies, `{min, max}`. |
| `faces` | Optional total face count, where the geometry fixes it. |
| `compare` | Optional: compare only these keys. |
| `tolerance` | Optional overrides: `rel` (volume, area; default 1e-3) and `abs_mm` (centre of mass and bbox; default 1e-3 × the bbox diagonal). |

## Origin

Cases 01–83 (`recipe.json` and `expected.json`) come from the SolidCraft oracle,
<https://github.com/bherbruck/solidcraft> `tests/oracle` at commit `e1268c6`, copyright (c) 2026
SolidCraft contributors, used under the MIT licence:

> Permission is hereby granted, free of charge, to any person obtaining a copy of this software
> and associated documentation files (the "Software"), to deal in the Software without
> restriction, including without limitation the rights to use, copy, modify, merge, publish,
> distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
> Software is furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all copies or
> substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
> BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
> NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
> DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

Their expected values are analytic or were built in FreeCAD; none were measured in a commercial CAD
product. Their optional `reference.stl` surface meshes are not copied.
