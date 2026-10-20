#!/usr/bin/env python3
"""Builds one case in Fusion and in FissionOxide and compares them.

    py tools/parity-loop/probe_case.py CASE_DIR [--write-expected] [--png]

CASE_DIR holds the case in the tests/reference layout:

- `recipe.json`: the neutral recipe, built in Fusion by the bridge's recipe runner, or
- `fusion.py`: for features the recipe format can't express, Python run inside Fusion. It runs in
  a fresh scratch design with `app`, `ui`, `adsk`, `design` and `root` (the root component) in
  scope, and only has to build the part; this script measures it. `CASE_DIR` (the case folder's
  absolute path) is in scope too, for scripts that read files kept next to them.
- `part.jsonl`: the same part as a FissionOxide command script (`fissionoxide-cli run`).

Prints a JSON verdict and writes it to CASE_DIR/verdict.json. With `--write-expected`, writes
CASE_DIR/expected.json from Fusion's measurement (only when Fusion built the part cleanly), which
makes Fusion the answer key for `cargo test -p fox-engine --test reference_parts`.

`status` is one of match, match-geometry, mismatch, fox-unbuilt (the Fox script failed or a
command doesn't exist), fusion-unbuilt.
"""

import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "fusion-bridge"))
import client  # noqa: E402

FUSION_PY = r"""
import adsk.core, adsk.fusion, importlib, traceback
import measure
importlib.reload(measure)
doc = app.documents.add(adsk.core.DocumentTypes.FusionDesignDocumentType)
try:
  doc.name = "FoxBridge " + CASE
except Exception:
  pass
design = adsk.fusion.Design.cast(app.activeProduct)
design.designType = adsk.fusion.DesignTypes.ParametricDesignType
root = design.rootComponent
scope = dict(globals(), design=design, root=root)
error = None
try:
  exec(CODE, scope)
except Exception:
  error = traceback.format_exc(limit=4)
result = measure.design(app)
result["ok"] = bool(result.get("ok")) and error is None and not [
  t for t in result.get("timeline", []) if t.get("health") == "error"]
if error:
  result["error"] = error
if PNG:
  result["screenshot"] = measure.screenshot(app, PNG)
if not KEEP_OPEN:
  doc.close(False)
"""


def fusion_side(case_dir, png=None, keep_open=False):
  name = os.path.basename(os.path.normpath(case_dir))
  py = os.path.join(case_dir, "fusion.py")
  if os.path.exists(py):
    with open(py, encoding="utf-8") as f:
      code = f.read()
    head = f"CASE = {name!r}\nCASE_DIR = {os.path.abspath(case_dir)!r}\nCODE = {code!r}\nPNG = {png!r}\nKEEP_OPEN = {keep_open!r}\n"
    res = client.call("python", code=head + FUSION_PY, hold=keep_open)  # closes its own document
    if not res.get("ok"):
      return {"ok": False, "error": res.get("error")}
    return res["result"]
  with open(os.path.join(case_dir, "recipe.json"), encoding="utf-8") as f:
    recipe = json.load(f)
  return client.recipe_call(recipe, keep_open=keep_open, screenshot=png)


def probe(case_dir, write_expected=False, png=False):
  case_dir = os.path.abspath(case_dir)
  name = os.path.basename(case_dir)
  fusion = fusion_side(case_dir, os.path.join(case_dir, "fusion.png") if png else None)
  part = os.path.join(case_dir, "part.jsonl")
  if os.path.exists(part):
    fox = client.fox_measure(part, os.path.join(case_dir, "fox.png") if png else None)
  else:
    fox = {"ok": False, "error": "no part.jsonl"}
  fm, xm = fusion.get("measured"), fox.get("measured")
  differences = client.diff(fm, xm) if fusion.get("ok") and fox.get("ok") else None
  if not fusion.get("ok"):
    status = "fusion-unbuilt"
  elif not fox.get("ok"):
    status = "fox-unbuilt"
  elif not differences:
    status = "match"
  elif all(d.startswith("faces ") for d in differences):
    status = "match-geometry"
  else:
    status = "mismatch"
  verdict = {
    "case": name, "status": status, "fox_vs_fusion": differences,
    "fusion": {"ok": bool(fusion.get("ok")), "measured": fm, "unsupported": fusion.get("unsupported", []),
               "errors": [s for s in fusion.get("steps", []) if s.get("error")] or (
                 [fusion["error"]] if fusion.get("error") else []),
               "timeline": fusion.get("timeline")},
    "fox": {"ok": bool(fox.get("ok")), "measured": xm, "errors": fox.get("errors") or (
      [fox["error"]] if fox.get("error") else []), "failed_features": fox.get("failed_features", [])},
  }
  if write_expected and fusion.get("ok") and fm and fm.get("body_count"):
    version = client.call().get("fusion", "?")
    expected = {
      "source": f"Autodesk Fusion {version} (FoxBridge)",
      "method": "Measured in Fusion by tools/parity-loop/probe_case.py from "
                + ("fusion.py" if os.path.exists(os.path.join(case_dir, "fusion.py")) else "recipe.json"),
    }
    expected.update({k: fm[k] for k in ("body_count", "volume_mm3", "area_mm2", "center_of_mass", "bbox", "faces")
                     if k in fm})
    with open(os.path.join(case_dir, "expected.json"), "w", encoding="utf-8") as f:
      json.dump(expected, f, indent=2)
    verdict["wrote_expected"] = True
  with open(os.path.join(case_dir, "verdict.json"), "w", encoding="utf-8") as f:
    json.dump(verdict, f, indent=1)
  return verdict


def main():
  ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
  ap.add_argument("case_dir")
  ap.add_argument("--write-expected", action="store_true")
  ap.add_argument("--png", action="store_true", help="save fusion.png and fox.png in the case")
  a = ap.parse_args()
  print(json.dumps(probe(a.case_dir, a.write_expected, a.png), indent=1))


if __name__ == "__main__":
  main()
