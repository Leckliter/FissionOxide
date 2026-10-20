# Reference parts

Parts whose measurements are known independently of FissionOxide. Each directory is one case:

- `expected.json`: what the finished part must measure, and where the numbers come from.
- `recipe.json`: the part described in a neutral, CAD-independent format (sketches in
  sketch-local coordinates, features with their parameters, faces and edges picked by a
  world-space point).
- `part.jsonl`: the same part as a FissionOxide command script (one JSON command per line, as
  `fissionoxide-cli run` reads it). Generated from `recipe.json` by `tools/reference-import`, then
  edited by hand where needed. File paths in it (an SVG to insert, say) are relative to the
  repository root.

`cargo test -p fox-engine --test reference_parts` builds every part that has a `part.jsonl` and
compares it with `expected.json`. Each built part must also be a valid solid that survives a STEP
round trip. Set `FOX_REFERENCE_REPORT=1` to rewrite `docs/reference-parts.md`.

Every passing part is also edited: its first extrusion's distance grows by 15% in place and the
timeline rebuilds. The result must match building the script with that distance from scratch or,
where the script's later picks (points on faces and edges) no longer land on the bigger part, the
edit must keep every face and editing it back must restore the part exactly. A failure means a
later feature lost track of the face or edge it was on. `edit-baseline.txt` lists the parts that
survive; one that stops surviving fails the test.
Parts can also carry hand-written edits with their own answers; see `variants.json` below.

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

## variants.json (edit checks)

Optional. Each variant edits the finished part and says what the edited part must measure, so
editing an early feature is tested as well as building:

```json
{"variants": [
  {"name": "height-35",
   "edits": [{"cmd": "feature.edit", "feature": 2, "set": {"extent": {"kind": "one_side", "distance": "35"}}}],
   "expected": {"method": "...", "volume_mm3": 42000.0, "faces": 6}}
]}
```

| Field | Meaning |
|---|---|
| `name` | Short name; the variant is reported as `case/name`. |
| `edits` | Commands in the `part.jsonl` form, run on the built part (`param.set`, `feature.edit`, `sketch.set_dimension`, `sketch.drag`, `sketch.transform`, ...). |
| `expected` | As `expected.json`, for the edited part. |
| `note` | Optional: what the variant exercises. |

`cargo test -p fox-engine --test reference_parts reference_variants` builds each case once, then
for each variant applies the edits in a transaction (the model regenerates incrementally, as in the
app) and checks that the edited part is a valid solid matching `expected`, that regenerating the
edited design from scratch gives the same volume, area and face count, and that rolling the edits
back gives the original part again. `variant-baseline.txt` ratchets like `baseline.txt`, with
`case/name` lines. `FOX_REFERENCE_REPORT=1` also writes `docs/reference-variants.md`.

The variants and their answers are written by `tools/reference-variants/variants.py` (closed-form
geometry, independent of FissionOxide); edit the script and rerun it rather than the JSON.

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
