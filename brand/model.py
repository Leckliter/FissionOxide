#!/usr/bin/env python3
"""Builds FissionOxide.fox, the fox mark as a 3D model, from the same sources as the logo.

Sources:
  brand/tokens.json     the colour families and their shades
  brand/logo/fox.json   the fox mark's facets

Outputs (never edit by hand; re-run this script):
  brand/model/FissionOxide.jsonl   the fissionoxide-cli script that models the fox
  brand/model/FissionOxide.fox     the design it makes: preloaded into every installation's
                                   data hub and shipped next to the app
  brand/model/FissionOxide.png     a render of it

Every facet of the mark becomes its own body in the facet's colour, standing up on the XZ plane
and extruded both ways from it, so the fox reads the same from the front and the back. Each part
of the face stands proud of the plane by a different amount (the muzzle and nose furthest, the
ears least), so the head has relief, and every facet's sides are drafted in by the same small
bevel, which the viewport's lighting picks out as a lit or shaded rim around each facet.

Needs Python 3.9+ and a built fissionoxide-cli (cargo build -p fissionoxide-cli; set FOX_CLI to
use another one). Run from anywhere: python3 brand/model.py
"""

import json
import math
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
OUT = os.path.join(HERE, "model")

sys.dont_write_bytecode = True  # importing build.py leaves no __pycache__ in brand/
sys.path.insert(0, HERE)
from build import FOX, COLOR, SHADES, W, H  # noqa: E402

NAME = "FissionOxide"
# mm per unit of the mark's 600 x 680 grid: the fox is 120 mm wide and 136 mm tall
SCALE = 0.2
# how far each part stands out of the middle plane on each side (mm); the centre facets
# (forehead and nose) are listed under their own names
RELIEF = {
  "ear": 4.0,
  "face": 6.0,
  "eye": 5.0,
  "jaw": 5.0,
  "muzzle": 8.0,
  "forehead": 7.0,
  "chin": 7.5,
  "forehead-centre": 7.5,
  "nose": 9.0,
}
# the lighter shades stand a little prouder, so the facets also step within a part
SHADE_STEP = 0.4
# how far each facet's sides lean in, measured across its face (mm)
BEVEL = 0.6


def model_facets():
  """Every facet as (name, part, hex colour, shade index, [(x, y)] on the mark's grid)."""
  out = []
  n = {}

  def name(part, side):
    n[(part, side)] = n.get((part, side), 0) + 1
    return f"{side} {part} {n[(part, side)]}".strip().capitalize()

  for f in FOX["left"]:
    out.append((name(f["part"], "left"), f["part"], COLOR[f["left"]][SHADES[f["shade"]]], f["shade"], f["points"]))
    s = min(f["shade"] + 1, 3)
    right = [(W - x, y) for x, y in reversed(f["points"])]
    out.append((name(f["part"], "right"), f["part"], COLOR[f["right"]][SHADES[s]], f["shade"], right))
  for f in FOX["centre"]:
    part = "nose" if f["part"] == "chin" else f"{f['part']}-centre"
    label = "Nose" if part == "nose" else "Forehead centre"
    out.append((label, part, COLOR[f["family"]][SHADES[f["shade"]]], f["shade"], f["points"]))
  return out


def to_mm(p):
  """The mark's grid (y down, centre line x = 300) to sketch millimetres (y up, centred)."""
  x, y = p
  return [round((x - W / 2) * SCALE, 3), round((H - y) * SCALE, 3)]


def script():
  facets = model_facets()
  lines = [
    "# The FissionOxide fox in 3D. Written by brand/model.py from brand/logo/fox.json and",
    "# brand/tokens.json; don't edit it by hand. Every facet is a body in its own colour,",
    "# extruded both ways from the XZ plane so the fox looks the same from the front and back.",
    json.dumps({"cmd": "new", "name": NAME}),
  ]
  for i, (name, part, colour, shade, pts) in enumerate(facets):
    half = RELIEF[part] + SHADE_STEP * (3 - shade)
    mm = [to_mm(p) for p in pts]
    taper = math.degrees(math.atan(BEVEL / half))
    lines.append(f"# {name}")
    sketch = 3 * i + 1
    body = (sketch + 1) * 1000  # a new body is numbered after the feature that made it
    lines.append(json.dumps({"cmd": "sketch.create", "plane": "xz"}))
    lines.append(json.dumps({"cmd": "sketch.line", "points": mm, "closed": True}))
    lines.append(json.dumps({"cmd": "sketch.finish"}))
    # taper needs a one-sided extent: out of the plane one way, then back the other way, joined
    for flip in (False, True):
      ext = {"cmd": "extrude", "sketch": sketch, "distance": f"{half:g} mm", "taper": f"{taper:.3f} deg"}
      ext.update({"flip": True, "operation": "join", "targets": [body]} if flip else {"operation": "new_body"})
      lines.append(json.dumps(ext))
    lines.append(json.dumps({"cmd": "body.appearance", "body": body, "color": colour}))
    lines.append(json.dumps({"cmd": "body.rename", "body": body, "name": name}))
  lines.append(json.dumps({"cmd": "info"}))
  return "\n".join(lines) + "\n"


def cli():
  exe = os.environ.get("FOX_CLI")
  if exe:
    return exe
  for profile in ("release", "debug"):
    p = os.path.join(ROOT, "target", profile, "fissionoxide-cli" + (".exe" if os.name == "nt" else ""))
    if os.path.exists(p):
      return p
  return shutil.which("fissionoxide-cli") or sys.exit("no fissionoxide-cli: cargo build -p fissionoxide-cli")


def main():
  os.makedirs(OUT, exist_ok=True)
  jsonl = os.path.join(OUT, f"{NAME}.jsonl")
  with open(jsonl, "w") as f:
    f.write(script())
  print("wrote", os.path.relpath(jsonl, ROOT))
  fox = os.path.join(OUT, f"{NAME}.fox")
  png = os.path.join(OUT, f"{NAME}.png")
  with tempfile.TemporaryDirectory() as hub:
    # a throwaway hub: building the model must not touch the user's own
    env = dict(os.environ, FOX_DATA_DIR=hub)
    subprocess.run(
      [cli(), "run", jsonl, "--save", fox], check=True, env=env, stdout=subprocess.DEVNULL
    )
    # three-quarters from the front, a little above, to show the relief
    render = {"cmd": "view.render", "path": png, "width": 1200, "height": 1200, "yaw": -30, "pitch": 15}
    subprocess.run([cli(), "exec", fox, json.dumps(render)], check=True, env=env, stdout=subprocess.DEVNULL)
  print("wrote", os.path.relpath(fox, ROOT))
  print("wrote", os.path.relpath(png, ROOT))


if __name__ == "__main__":
  main()
