#!/usr/bin/env python3
"""Write the edit-check variants of the reference parts (tests/reference/NN-name/variants.json).

    python3 tools/reference-variants/variants.py          # every case below
    python3 tools/reference-variants/variants.py 09 box   # cases whose name contains a filter

A variant edits the finished part (the commands in "edits", run on the built model, which then
regenerates) and gives what the edited part must measure. The answers here are worked out from
closed-form geometry, independently of FissionOxide: each variant's "method" says how.

Python 3 standard library only.
"""

import json
import math
import os
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
REF = os.path.join(ROOT, "tests", "reference")
PI = math.pi


# ------------------------------------------------------------- solids as (volume, centroid) sums


class Mass:
  """A signed sum of pieces with known volume and centroid."""

  def __init__(self):
    self.v = 0.0
    self.m = [0.0, 0.0, 0.0]

  def add(self, v, c, sign=1.0):
    self.v += sign * v
    for i in range(3):
      self.m[i] += sign * v * c[i]
    return self

  def box(self, lo, hi, sign=1.0):
    v = (hi[0] - lo[0]) * (hi[1] - lo[1]) * (hi[2] - lo[2])
    return self.add(v, [(lo[i] + hi[i]) / 2 for i in range(3)], sign)

  def cyl_z(self, x, y, r, z0, z1, sign=1.0):
    return self.add(PI * r * r * (z1 - z0), [x, y, (z0 + z1) / 2], sign)

  def com(self):
    return [self.m[i] / self.v for i in range(3)]


def box_area(a, b, c):
  return 2 * (a * b + b * c + c * a)


def expected(method, volume, area, com, lo, hi, faces=None, bodies=1):
  e = {
    "method": method,
    "body_count": bodies,
    "volume_mm3": volume,
    "area_mm2": area,
    "center_of_mass": [round(x, 9) + 0.0 for x in com],
    "bbox": {"min": [float(x) for x in lo], "max": [float(x) for x in hi]},
  }
  if faces is not None:
    e["faces"] = faces
  return e


def variant(name, edits, exp, **extra):
  v = {"name": name, "edits": edits}
  v.update(extra)
  v["expected"] = exp
  return v


def extent(distance, **kw):
  """An extrude's whole `extent` (feature.edit replaces the field)."""
  e = {"kind": "one_side", "distance": str(distance)}
  e.update({k: str(v) if isinstance(v, (int, float)) else v for k, v in kw.items()})
  return e


def edit(feature, **fields):
  return {"cmd": "feature.edit", "feature": feature, "set": fields}


# ------------------------------------------------------------------------------------- the cases


def box_case(a, b, c, method):
  m = Mass().box([0, 0, 0], [a, b, c])
  return expected(method, m.v, box_area(a, b, c), m.com(), [0, 0, 0], [a, b, c], 6)


def case_01():
  return [
    variant(
      "height-35",
      [edit(2, extent=extent(35))],
      box_case(40, 30, 35, "extrude distance 20 -> 35: box 40 x 30 x 35, V = abc, A = 2(ab + bc + ca)"),
    ),
    variant(
      "width-80",
      [
        {"cmd": "sketch.edit", "sketch": 1},
        {"cmd": "sketch.drag", "point": 1, "to": [80, 0]},
        {"cmd": "sketch.drag", "point": 4, "to": [80, 30]},
        {"cmd": "sketch.finish"},
      ],
      box_case(80, 30, 20, "the sketch's right edge dragged from x = 40 to 80: box 80 x 30 x 20"),
    ),
  ]


def cylinder(r, h, method):
  return expected(method, PI * r * r * h, 2 * PI * r * r + 2 * PI * r * h, [0, 0, h / 2], [-r, -r, 0], [r, r, h], 3)


def case_02():
  return [
    variant(
      "height-80",
      [edit(2, extent=extent(80))],
      cylinder(15, 80, "extrude distance 50 -> 80: cylinder r 15 h 80, V = pi r^2 h, A = 2 pi r^2 + 2 pi r h"),
    ),
    variant(
      "radius-20",
      [
        {"cmd": "sketch.edit", "sketch": 1},
        {"cmd": "sketch.dimension", "type": "radius", "curves": [1], "value": "20 mm"},
        {"cmd": "sketch.finish"},
      ],
      cylinder(20, 50, "a radius dimension of 20 added to the circle: cylinder r 20 h 50"),
    ),
  ]


def box_hole(a, b, c, hx, hy, r, method):
  m = Mass().box([0, 0, 0], [a, b, c]).cyl_z(hx, hy, r, 0, c, -1)
  area = box_area(a, b, c) - 2 * PI * r * r + 2 * PI * r * c
  return expected(method, m.v, area, m.com(), [0, 0, 0], [a, b, c], 7)


def case_03():
  return [
    variant(
      "hole-moved",
      [{"cmd": "sketch.transform", "sketch": 3, "translate": [-15, -10]}],
      box_hole(60, 40, 20, 15, 10, 6, "hole sketch moved by (-15, -10): box 60x40x20 minus a through cylinder r 6 at (15, 10)"),
    ),
    variant(
      "hole-r10",
      [{"cmd": "sketch.transform", "sketch": 3, "scale": 10 / 6, "center": [30, 20]}],
      box_hole(60, 40, 20, 30, 20, 10, "hole circle scaled about its centre from r 6 to r 10"),
    ),
  ]


def frustum(a0, h, taper_deg, method):
  a1 = a0 + 2 * h * math.tan(math.radians(taper_deg))
  s0, s1 = a0 * a0, a1 * a1
  v = h / 3 * (s0 + s1 + math.sqrt(s0 * s1))
  zc = h * (s0 + 2 * math.sqrt(s0 * s1) + 3 * s1) / (4 * (s0 + math.sqrt(s0 * s1) + s1))
  slant = math.hypot(h, (a1 - a0) / 2)
  area = s0 + s1 + 4 * (a0 + a1) / 2 * slant
  r = max(a0, a1) / 2
  return expected(method, v, area, [0, 0, zc], [-r, -r, 0], [r, r, h], 6)


def case_05():
  return [
    variant(
      "taper-5",
      [edit(2, extent=extent(30, taper="-5 deg"))],
      frustum(40, 30, 5, "taper -10 deg -> -5 deg: square frustum, base 40, top 40 + 2*30*tan 5, V = h/3 (A0 + A1 + sqrt(A0 A1))"),
    ),
    variant(
      "height-20",
      [edit(2, extent=extent(20, taper="-10 deg"))],
      frustum(40, 20, 10, "extrude distance 30 -> 20 at taper -10 deg: square frustum, base 40, top 40 + 2*20*tan 10"),
    ),
  ]


def case_06():
  m = Mass().box([-25, -25, -10], [25, 25, 10])
  return [
    variant(
      "distance-25",
      [edit(2, extent=extent(25, kind="symmetric"))],
      expected(
        "symmetric distance 15 -> 25: box 50 x 50 x 20 centred on the origin",
        m.v,
        box_area(50, 50, 20),
        m.com(),
        [-25, -25, -10],
        [25, 25, 10],
        6,
      ),
    )
  ]


def rounded_box(a, b, c, r, method):
  # inner box grown by a ball of radius r (Steiner's formula)
  ai, bi, ci = a - 2 * r, b - 2 * r, c - 2 * r
  v = ai * bi * ci + 2 * r * (ai * bi + bi * ci + ci * ai) + PI * r * r * (ai + bi + ci) + 4 / 3 * PI * r**3
  area = 2 * (ai * bi + bi * ci + ci * ai) + 2 * PI * r * (ai + bi + ci) + 4 * PI * r * r
  return expected(method, v, area, [a / 2, b / 2, c / 2], [0, 0, 0], [a, b, c], 26)


def case_07():
  return [
    variant(
      "radius-5",
      [edit(3, radius="5")],
      rounded_box(40, 30, 20, 5, "fillet radius 3 -> 5: box 40x30x20 = inner box 30x20x10 grown by a ball r 5 (Steiner)"),
    ),
    variant(
      "height-24",
      [edit(2, extent=extent(24))],
      rounded_box(40, 30, 24, 3, "extrude 20 -> 24 under the fillet: box 40x30x24 with every edge rounded r 3"),
      note="the fillet picks its edges by points on the 20 mm box; the top edges have to follow the edit",
    ),
  ]


def chamfered_box(a, b, c, d, method):
  # lower box to c - d, then a slab whose section at depth u below the top is (a-2d+2u) x (b-2d+2u)
  def sec(u):
    return (a - 2 * d + 2 * u) * (b - 2 * d + 2 * u)

  n = 2000
  vs = ms = 0.0
  for i in range(n + 1):  # Simpson (exact: the integrands are cubic polynomials)
    u = d * i / n
    w = 1 if i in (0, n) else (4 if i % 2 else 2)
    vs += w * sec(u)
    ms += w * sec(u) * (c - u)
  vs *= d / n / 3
  ms *= d / n / 3
  m = Mass().box([0, 0, 0], [a, b, c - d])
  m.add(vs, [a / 2, b / 2, ms / vs])
  slant = d * math.sqrt(2)
  area = a * b + 2 * (a + b) * (c - d) + (a - 2 * d) * (b - 2 * d) + 2 * slant * ((2 * a - 2 * d) / 2 + (2 * b - 2 * d) / 2)
  return expected(method, m.v, area, m.com(), [0, 0, 0], [a, b, c], 10)


def case_08():
  return [
    variant(
      "distance-4",
      [edit(3, distance="4")],
      chamfered_box(40, 30, 20, 4, "chamfer 2 -> 4 on the four top edges of the 40x30x20 box (sections integrated exactly)"),
    )
  ]


def shell_box(a, b, c, t, method):
  m = Mass().box([0, 0, 0], [a, b, c]).box([t, t, t], [a - t, b - t, c], -1)
  ia, ib, ih = a - 2 * t, b - 2 * t, c - t
  area = a * b + 2 * (a + b) * c + (a * b - ia * ib) + ia * ib + 2 * (ia + ib) * ih
  return expected(method, m.v, area, m.com(), [0, 0, 0], [a, b, c], 11)


def case_09():
  return [
    variant(
      "thickness-3",
      [edit(3, thickness="3")],
      shell_box(50, 40, 30, 3, "shell 2 -> 3: 50x40x30 box minus cavity [3,47]x[3,37]x[3,30]"),
    ),
    variant(
      "height-40",
      [edit(2, extent=extent(40))],
      shell_box(50, 40, 40, 2, "extrude 30 -> 40 under the shell: 50x40x40 box shelled 2 with the top open"),
      note="the shell picks its open face by a point on the 30 mm box's top; the face has to follow the edit",
    ),
  ]


def plate_holes(a, b, t, holes, r, method, faces):
  m = Mass().box([0, 0, 0], [a, b, t])
  for x, y in holes:
    m.cyl_z(x, y, r, 0, t, -1)
  n = len(holes)
  area = box_area(a, b, t) + n * (2 * PI * r * t - 2 * PI * r * r)
  return expected(method, m.v, area, m.com(), [0, 0, 0], [a, b, t], faces)


def case_10():
  holes = [(15 + 20 * i, 15 + 20 * j) for i in range(4) for j in range(3)]
  return [
    variant(
      "thickness-8",
      [edit(2, extent=extent(8))],
      plate_holes(90, 70, 8, holes, 3, "plate 5 -> 8 thick under the 12 through holes: V = abt - 12 pi r^2 t", 18),
    )
  ]


def case_11():
  def flange(h):
    m = Mass().cyl_z(0, 0, 50, 0, h).cyl_z(0, 0, 15, 0, h, -1)
    m.v -= 6 * PI * 25 * h  # six holes on a circle: symmetric, centroid stays on the axis
    area = 2 * PI * (2500 - 225) + 2 * PI * 65 * h + 6 * (2 * PI * 5 * h - 2 * PI * 25)
    return m.v, area

  v, area = flange(16)
  return [
    variant(
      "thickness-16",
      [edit(2, extent=extent(16))],
      expected(
        "flange 10 -> 16 thick under the six through holes: V = pi(R^2 - r^2)h - 6 pi rh^2 h",
        v,
        area,
        [0, 0, 8],
        [-50, -50, 0],
        [50, 50, 16],
        10,
      ),
    )
  ]


def case_12():
  t, r, top = 14, 6, 25  # bosses still start on the z = 10 sketch plane and end at 25
  m = Mass().box([-30, 0, 0], [30, 30, t]).cyl_z(15, 15, r, t, top).cyl_z(-15, 15, r, t, top)
  area = box_area(60, 30, t) + 2 * 2 * PI * r * (top - t)
  return [
    variant(
      "block-14",
      [edit(2, extent=extent(t))],
      expected(
        "block 10 -> 14 thick; the bosses (sketched at z = 10, 15 tall) now rise 11 above it",
        m.v,
        area,
        m.com(),
        [-30, 0, 0],
        [30, 30, top],
        10,
      ),
    )
  ]


def case_13():
  r, h = 12, 30
  m = Mass().box([0, 0, 0], [40, 40, 20])
  m.add(PI * r * r / 2 * 20, [40 + 4 * r / (3 * PI), 20, 10])
  m.cyl_z(40, 20, r, 20, h)
  area = (1600 + PI * r * r / 2) + (1600 - PI * r * r / 2) + PI * r * r + 3 * 800 + (800 - 2 * r * 20)
  area += PI * r * h + PI * r * (h - 20)
  return [
    variant(
      "cylinder-30",
      [edit(4, extent=extent(h))],
      expected(
        "tool cylinder 40 -> 30 tall before the join: box + outer half cylinder z 0..20 + full cylinder z 20..30",
        m.v,
        area,
        m.com(),
        [0, 0, 0],
        [52, 40, h],
        9,
      ),
    )
  ]


def case_19():
  w = 80
  m = Mass().box([0, 0, 0], [w, 8, 20]).box([0, 8, 0], [8, 40, 20])
  s = w * 8 + 8 * 32
  per = w + 8 + (w - 8) + 32 + 8 + 40
  return [
    variant(
      "length-80",
      [{"cmd": "sketch.set_dimension", "sketch": 1, "constraint": 20, "value": "80"}],
      expected(
        "the 60 mm dimension set to 80: L (0,0)-(80,0)-(80,8)-(8,8)-(8,40)-(0,40) extruded 20",
        s * 20,
        2 * s + per * 20,
        m.com(),
        [0, 0, 0],
        [w, 40, 20],
        8,
      ),
    )
  ]


def case_20():
  r, l, h = 6, 50, 5
  s = 2 * r * l + PI * r * r
  return [
    variant(
      "length-50",
      [{"cmd": "sketch.set_dimension", "sketch": 1, "constraint": 20, "value": "50"}],
      expected(
        "slot centre distance 40 -> 50: A2 = 2rL + pi r^2, V = A2 h, A = 2 A2 + (2L + 2 pi r) h",
        s * h,
        2 * s + (2 * l + 2 * PI * r) * h,
        [l / 2, 0, h / 2],
        [-r, -r, 0],
        [l + r, r, h],
        6,
      ),
    )
  ]


def block_hole(w, d, h, hd, method):
  r = hd / 2
  m = Mass().box([0, 0, 0], [w, d, h]).cyl_z(w / 2, d / 2, r, 0, h, -1)
  area = 2 * (w * d - PI * r * r) + 2 * (w + d) * h + 2 * PI * r * h
  return expected(method, m.v, area, m.com(), [0, 0, 0], [w, d, h], 7)


def case_24():
  return [
    variant(
      "width-80",
      [{"cmd": "param.set", "name": "width", "expr": "80 mm"}],
      block_hole(80, 40, 15, 10, "width 60 -> 80: block 80 x 40 x 15 with the d10 hole still centred"),
    ),
    variant(
      "height-25-hole-20",
      [{"cmd": "param.set", "name": "height", "expr": "25 mm"}, {"cmd": "param.set", "name": "hole_d", "expr": "20 mm"}],
      block_hole(60, 40, 25, 20, "height 15 -> 25 and hole_d 10 -> 20"),
    ),
  ]


def case_43():
  # the cutting slot moved +20 in x: pieces x 0..55 and 65..80
  m = Mass().box([0, 0, 0], [55, 20, 20]).box([65, 0, 0], [80, 20, 20])
  return [
    variant(
      "cut-moved",
      [{"cmd": "sketch.transform", "sketch": 3, "translate": [20, 0]}],
      expected(
        "cut slot moved +20 in x: two boxes 55x20x20 and 15x20x20",
        m.v,
        box_area(55, 20, 20) + box_area(15, 20, 20),
        m.com(),
        [0, 0, 0],
        [80, 20, 20],
        12,
        bodies=2,
      ),
    )
  ]


def case_44():
  t = 3
  outer = Mass().box([0, 0, 0], [60, 8, 20]).box([0, 8, 0], [8, 40, 20])
  # SolidCraft's sharp-cornered shell: the cavity is the L shrunk by t, from z = t to the top
  outer.box([t, t, t], [60 - t, 8 - t, 20], -1).box([t, 8 - t, t], [8 - t, 40 - t, 20], -1)
  inner = (60 - 2 * t) * (8 - 2 * t) + (8 - 2 * t) * (32)
  inner_per = (60 - 2 * t) + (8 - 2 * t) + (60 - 8) + 32 + (8 - 2 * t) + (40 - 2 * t)
  area = 736 + 200 * 20 + (736 - inner) + inner + inner_per * (20 - t)
  return [
    variant(
      "thickness-3",
      [edit(3, thickness="3")],
      expected(
        "shell 2 -> 3: L prism minus the L shrunk by 3 from z = 3 (same sharp-corner convention as the base case)",
        outer.v,
        area,
        outer.com(),
        [0, 0, 0],
        [60, 40, 20],
        15,
      ),
    )
  ]


def case_46():
  t = 4
  m = Mass().box([0, 0, 0], [50, 40, 30]).box([t, 0, t], [50 - t, 40 - t, 30], -1)
  ia, ib, ih = 50 - 2 * t, 40 - t, 30 - t
  # outside: bottom, back, two sides; rim on top (U) and front (U); cavity: floor, back, two sides
  area = 50 * 40 + 50 * 30 + 2 * 40 * 30
  area += (50 * 40 - ia * ib) + (50 * 30 - ia * ih)
  area += ia * ib + ia * ih + 2 * ib * ih
  return [
    variant(
      "thickness-4",
      [edit(3, thickness="4")],
      expected(
        "shell 3 -> 4 with top and front open: box minus [4,46]x[0,36]x[4,30]",
        m.v,
        area,
        m.com(),
        [0, 0, 0],
        [50, 40, 30],
        10,
      ),
    )
  ]


def case_62():
  base, fin_top = 8, 30  # fins are sketched at z = 5 and extruded 25, so they end at 30
  m = Mass().box([0, 0, 0], [60, 50, base])
  for k in range(15):
    x = 2 + 4 * k
    m.box([x, 0, base], [x + 1.5, 50, fin_top])
  h = fin_top - base
  area = box_area(60, 50, base) + 15 * (2 * 50 * h + 2 * 1.5 * h)
  return [
    variant(
      "base-8",
      [edit(2, extent=extent(base))],
      expected(
        "base 5 -> 8 thick; the 15 fins (z 5..30) now stand 22 above it",
        m.v,
        area,
        m.com(),
        [0, 0, 0],
        [60, 50, fin_top],
        66,
      ),
    )
  ]


def plate(a, b, t, method):
  m = Mass().box([0, 0, 0], [a, b, t])
  return expected(method, m.v, box_area(a, b, t), m.com(), [0, 0, 0], [a, b, t], 6)


def case_67():
  return [
    variant(
      "thickness-4",
      [{"cmd": "param.set", "name": "sm_thickness", "expr": "4 mm"}],
      plate(100, 60, 4, "rule thickness 2.5 -> 4: plate 100 x 60 x 4"),
    )
  ]


def edge_flange(t, height, method):
  """A 100 long base 60 wide with a 90 deg inside flange on its y = 0 edge, bend radius = t
  (the convention of cases 68 and 76): base y 2t..60, z 0..t; bend quarter annulus t..2t about
  (y 2t, z 2t); flange y 0..t, z 2t..height."""
  rb, ro = t, 2 * t
  q = PI / 4 * (ro * ro - rb * rb)
  off = 4 * (ro**3 - rb**3) / (3 * PI * (ro * ro - rb * rb))  # quarter-annulus centroid offset
  pieces = [
    ((60 - 2 * t) * t, (2 * t + 60) / 2, t / 2),
    (q, 2 * t - off, 2 * t - off),
    (t * (height - 2 * t), t / 2, (2 * t + height) / 2),
  ]
  s = sum(p[0] for p in pieces)
  cy = sum(p[0] * p[1] for p in pieces) / s
  cz = sum(p[0] * p[2] for p in pieces) / s
  per = 2 * (60 - 2 * t) + t + PI / 2 * rb + PI / 2 * ro + 2 * (height - 2 * t) + t
  return expected(method, 100 * s, 2 * s + 100 * per, [50, cy, cz], [0, 0, 0], [100, 60, height])


def case_68():
  return [
    variant(
      "thickness-3",
      [{"cmd": "param.set", "name": "sm_thickness", "expr": "3 mm"}],
      edge_flange(3, 20, "rule thickness 2.5 -> 3 (bend radius = thickness): base, R3..6 bend, flange to z 20"),
    ),
    variant(
      "height-30",
      [edit(3, height="30")],
      edge_flange(2.5, 30, "flange height 20 -> 30 (outer face reaches z 30)"),
    ),
  ]


def case_76():
  return [
    variant(
      "thickness-2",
      [{"cmd": "param.set", "name": "sm_thickness", "expr": "2 mm"}],
      edge_flange(2, 20, "rule thickness 3 -> 2: base, R2..4 bend, flange to z 20"),
    )
  ]


def case_82():
  t_in = 2
  o, i = 42, 40 - 2 * t_in
  m = Mass().box([-21, -21, 0], [21, 21, 10]).box([-i / 2, -i / 2, 0], [i / 2, i / 2, 10], -1)
  return [
    variant(
      "inner-wall-2",
      [edit(2, extent=extent(10, thin=t_in, thin_side="inside"))],
      expected(
        "inside wall 3 -> 2: 42 x 42 less 36 x 36 frame, 10 tall",
        m.v,
        2 * (o * o - i * i) + 4 * o * 10 + 4 * i * 10,
        m.com(),
        [-21, -21, 0],
        [21, 21, 10],
        10,
      ),
    )
  ]



# ---------------------------------------------- references the position lookup would get wrong
# (cases 520-522: each edit leaves a look-alike where the referenced edge or face was)

HISTORY_COMPARE = ["body_count", "volume_mm3", "center_of_mass", "bbox"]
# the wrong rim changes the volume by a few parts in 1e5: tighter than the default 1e-3
HISTORY_TOLERANCE = {"rel": 1e-6}


def spandrel_ring(m, x, y, top, big, r):
  """Takes a convex fillet of radius r off the rim (radius big, at height top) of a round boss:
  the corner outside a quarter circle, (1 - pi/4) r^2, centroid r / (6 (1 - pi/4)) from the
  circle's centre along each axis, swept round (Pappus)."""
  k = 1 - PI / 4
  c = r / (6 * k)
  m.add(k * r * r * 2 * PI * (big - r + c), [x, y, top - r + c], -1)


def boss_rim_fillet(ax, method):
  m = Mass().box([0, 0, 0], [60, 40, 20])
  m.cyl_z(ax, 20, 4, 20, 30).cyl_z(30, 20, 5, 20, 30)
  spandrel_ring(m, ax, 20, 30, 4, 1)
  e = expected(method, m.v, 0.0, m.com(), [0, 0, 0], [60, 40, 30])
  e["compare"] = HISTORY_COMPARE
  e["tolerance"] = HISTORY_TOLERANCE
  return e


def case_520():
  return [
    variant(
      "boss-past-other-boss",
      [{"cmd": "sketch.set_dimension", "sketch": 3, "constraint": 5, "value": "50"}],
      boss_rim_fillet(50, "r4 boss moved from x 10 to 50, past the r5 boss: block + bosses - the r1 fillet ring on the r4 rim, (1 - pi/4) r^2 swept at its centroid (Pappus)"),
      note="the fillet's edge, found by position, would be the r5 boss's rim",
    )
  ]


def short_boss(h, method):
  m = Mass().box([0, 0, 0], [80, 40, 20]).cyl_z(10, 20, 4, 20, 20 + h).box([30, 5, 20], [70, 35, 30])
  m.cyl_z(10, 20, 2, 20 + h, 25 + h)
  e = expected(method, m.v, 0.0, m.com(), [0, 0, 0], [80, 40, max(30, 25 + h)])
  e["compare"] = HISTORY_COMPARE
  e["tolerance"] = HISTORY_TOLERANCE
  return e


def case_521():
  return [
    variant(
      "round-boss-4-tall",
      [edit(4, extent=extent(4))],
      short_boss(4, "round boss 10 -> 4 tall, the pin r2 x 5 still standing on it: block + bosses + pin, as boxes and cylinders"),
      note="the pin's sketch face, found by position, would be the wide boss's top",
    )
  ]


def hole_rim_chamfer(ax, method):
  d = 1
  m = Mass().box([0, 0, 0], [60, 40, 20]).cyl_z(ax, 20, 3, 0, 20, -1).cyl_z(25, 20, 4, 0, 20, -1)
  # 45 degree ring: triangle d x d at the rim, centroid d/3 out and d/3 down
  m.add(PI * d * d * (3 + d / 3), [ax, 20, 20 - d / 3], -1)
  e = expected(method, m.v, 0.0, m.com(), [0, 0, 0], [60, 40, 20])
  e["compare"] = HISTORY_COMPARE
  e["tolerance"] = HISTORY_TOLERANCE
  return e


def case_522():
  return [
    variant(
      "hole-past-other-hole",
      [{"cmd": "sketch.set_dimension", "sketch": 3, "constraint": 5, "value": "50"}],
      hole_rim_chamfer(50, "r3 hole moved from x 10 to 50, past the r4 hole: block - holes - the 1 mm chamfer ring on the r3 rim, pi d^2 (r + d/3)"),
      note="the chamfer's edge, found by position, would be the r4 hole's rim",
    )
  ]


def write_expected():
  """expected.json of cases 520-522 (the parts as built)."""
  for name, e in [
    ("520-history-boss-rim-fillet", boss_rim_fillet(10, "block + r4 and r5 bosses - the r1 fillet ring on the r4 rim (Pappus)")),
    ("521-history-sketch-on-short-boss", short_boss(10, "block + round boss + wide boss + pin, as boxes and cylinders")),
    ("522-history-hole-rim-chamfer", hole_rim_chamfer(10, "block - two holes - the 1 mm chamfer ring on the r3 rim, pi d^2 (r + d/3)")),
  ]:
    e = {"source": "analytic", **e}
    with open(os.path.join(REF, name, "expected.json"), "w") as f:
      json.dump(e, f, indent=2)
      f.write("\n")

CASES = {
  "520-history-boss-rim-fillet": case_520,
  "521-history-sketch-on-short-boss": case_521,
  "522-history-hole-rim-chamfer": case_522,
  "01-box": case_01,
  "02-cylinder": case_02,
  "03-box-through-hole": case_03,
  "05-taper-extrude": case_05,
  "06-symmetric-extrude": case_06,
  "07-fillet-all-edges": case_07,
  "08-chamfer": case_08,
  "09-shell": case_09,
  "10-rect-pattern-holes": case_10,
  "11-circular-pattern-flange": case_11,
  "12-mirror-feature": case_12,
  "13-combine-join": case_13,
  "19-l-bracket-constrained": case_19,
  "20-slot-constrained": case_20,
  "24-user-parameters": case_24,
  "43-cut-splits-body": case_43,
  "44-shell-l-bracket": case_44,
  "46-shell-two-open-faces": case_46,
  "62-heat-sink": case_62,
  "67-sm-base-flange": case_67,
  "68-sm-edge-flange": case_68,
  "76-sm-thickness-variant": case_76,
  "82-thin-extrude-frame": case_82,
}


def main():
  filters = sys.argv[1:]
  write_expected()
  for name, fn in CASES.items():
    if filters and not any(f in name for f in filters):
      continue
    doc = {
      "comment": "Edit checks, written by tools/reference-variants/variants.py; see tests/reference/README.md.",
      "variants": fn(),
    }
    with open(os.path.join(REF, name, "variants.json"), "w") as f:
      json.dump(doc, f, indent=2)
      f.write("\n")
    print(name, len(doc["variants"]))


if __name__ == "__main__":
  main()
