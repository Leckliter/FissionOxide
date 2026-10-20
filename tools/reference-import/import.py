#!/usr/bin/env python3
"""Turn the neutral reference recipes (tests/reference/NN-name/recipe.json) into FissionOxide
command scripts (part.jsonl next to each recipe).

    python3 tools/reference-import/import.py            # every case
    python3 tools/reference-import/import.py 03 box     # cases whose name contains a filter

A part.jsonl containing a line "# hand-edited" is left alone. Recipe operations FissionOxide cannot
express are written as "# UNSUPPORTED: ..." comment lines (the case then fails, which is the point).

Conventions (see a recipe's "conventions"): sketches are in sketch-local 2D coordinates on a frame
(origin, x_dir, y_dir, normal); faces and edges are picked by a world point. Every sketch is placed
on a free plane {origin, normal, x}, which reproduces the recipe frame exactly (y = normal x x).
Feature patterns and feature mirrors are replayed by repeating the feature on a moved copy of its
sketch (FissionOxide patterns and mirrors work on bodies).

Constrained sketches are solved by target/debug/fissionoxide-cli (build it first, or set FOX_CLI)
so that profile points fall inside the solved regions.

Python 3 standard library only.
"""

import json
import math
import os
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
REF = os.path.join(ROOT, "tests", "reference")
# The command-line app, used to solve constrained sketches (FOX_CLI overrides).
CLI = os.environ.get("FOX_CLI") or os.path.join(ROOT, "target", "debug", "fissionoxide-cli")

SNAP = 1e-6
BODIES_PER_FEATURE = 1000  # FissionOxide body ids: feature id * 1000 + k


# ---------------------------------------------------------------- vectors


def add(a, b):
  return [a[i] + b[i] for i in range(len(a))]


def sub(a, b):
  return [a[i] - b[i] for i in range(len(a))]


def mul(a, k):
  return [x * k for x in a]


def dot(a, b):
  return sum(a[i] * b[i] for i in range(len(a)))


def cross(a, b):
  return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def norm(a):
  return math.sqrt(dot(a, a))


def unit(a):
  n = norm(a)
  return [x / n for x in a] if n > 0 else a


def clean(v, nd=9):
  """Round floats for the script (and kill -0.0)."""
  if isinstance(v, list):
    return [clean(x, nd) for x in v]
  if isinstance(v, float):
    r = round(v, nd)
    if r == 0:
      r = 0.0
    if r == int(r) and abs(r) < 1e15:
      return int(r)
    return r
  return v


# ---------------------------------------------------------------- rigid transforms
# A transform is (R, t): world point p -> R p + t, R a 3x3 row-major matrix (orthogonal).

IDENTITY = ([[1, 0, 0], [0, 1, 0], [0, 0, 1]], [0, 0, 0])


def t_apply(T, p):
  R, t = T
  return [dot(R[i], p) + t[i] for i in range(3)]


def t_dir(T, v):
  R, _ = T
  return [dot(R[i], v) for i in range(3)]


def t_det(T):
  R, _ = T
  return dot(R[0], cross(R[1], R[2]))


def t_is_identity(T):
  R, t = T
  return all(abs(R[i][j] - (1 if i == j else 0)) < 1e-12 for i in range(3) for j in range(3)) and norm(t) < 1e-12


def rotation(origin, axis, ang):
  """Rotation by ang (radians, right hand about axis) about the line through origin."""
  k = unit(axis)
  c, s = math.cos(ang), math.sin(ang)
  x, y, z = k
  R = [
    [c + x * x * (1 - c), x * y * (1 - c) - z * s, x * z * (1 - c) + y * s],
    [y * x * (1 - c) + z * s, c + y * y * (1 - c), y * z * (1 - c) - x * s],
    [z * x * (1 - c) - y * s, z * y * (1 - c) + x * s, c + z * z * (1 - c)],
  ]
  t = sub(origin, [dot(R[i], origin) for i in range(3)])
  return (R, t)


def translation(v):
  return ([[1, 0, 0], [0, 1, 0], [0, 0, 1]], list(v))


def reflection(origin, normal):
  n = unit(normal)
  R = [[(1 if i == j else 0) - 2 * n[i] * n[j] for j in range(3)] for i in range(3)]
  t = sub(origin, [dot(R[i], origin) for i in range(3)])
  return (R, t)


# ---------------------------------------------------------------- 2D sketch geometry


def circumcenter(a, b, c):
  d = 2 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]))
  if abs(d) < 1e-15:
    return None
  a2, b2, c2 = a[0] ** 2 + a[1] ** 2, b[0] ** 2 + b[1] ** 2, c[0] ** 2 + c[1] ** 2
  ux = (a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d
  uy = (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d
  return [ux, uy]


def ccw_sweep(a0, a1):
  s = (a1 - a0) % (2 * math.pi)
  return s


def arc_polyline(c, r, a0, sweep, n=None):
  n = n or max(8, int(abs(sweep) / (2 * math.pi) * 128))
  return [[c[0] + r * math.cos(a0 + sweep * i / n), c[1] + r * math.sin(a0 + sweep * i / n)] for i in range(n + 1)]


def seg_dist(p, a, b):
  ab = sub(b, a)
  l2 = dot(ab, ab)
  t = 0 if l2 == 0 else max(0, min(1, dot(sub(p, a), ab) / l2))
  return norm(sub(add(a, mul(ab, t)), p))


def crossings(p, polys):
  """Ray-casting parity against a set of polylines (they need not be chained)."""
  inside = False
  for poly in polys:
    for i in range(len(poly) - 1):
      a, b = poly[i], poly[i + 1]
      if (a[1] > p[1]) != (b[1] > p[1]):
        x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1])
        if x > p[0]:
          inside = not inside
  return inside


RADIUS_SAMPLES = 32  # points a B-spline radius law is sampled at along the edge


def bspline(ctrl, knots, u):
  """A clamped 1-D B-spline (degree len(knots) - len(ctrl) - 1) at u, by de Boor."""
  p = len(knots) - len(ctrl) - 1
  k = p
  while k + 1 < len(ctrl) and knots[k + 1] <= u:
    k += 1
  d = [ctrl[j + k - p] for j in range(p + 1)]
  for r in range(1, p + 1):
    for j in range(p, r - 1, -1):
      a = (u - knots[j + k - p]) / (knots[j + 1 + k - r] - knots[j + k - p])
      d[j] = (1 - a) * d[j - 1] + a * d[j]
  return d[p]


def edge_point(e):
  """A point on a recipe edge. A full circle's given point is its start, which is also on the seam
  of the face it bounds; take the point a quarter turn on instead."""
  if e.get("type") == "circle":
    c, ax = e["center"], unit(e["axis"])
    return add(c, cross(ax, sub(e["point"], c)))
  return e["point"]


class Unsupported(Exception):
  pass


# ---------------------------------------------------------------- the converter


class Sketch:
  """A recipe sketch as emitted into FissionOxide: feature id, entity -> curve/point ids, geometry."""

  def __init__(self, fid, constrained):
    self.fid = fid
    self.constrained = constrained
    self.points = [(0, 0.0, 0.0)]  # (id, x, y); id 0 is the sketch origin
    self.next_id = 1
    self.curve = {}  # recipe entity id -> FissionOxide curve id
    self.pt = {"origin": 0}  # "<entity>.<start|end|center>" or point entity id -> point id
    self.pos = {"origin": [0.0, 0.0]}
    self.geom = {}  # recipe entity id -> polyline (local, as drawn)
    self.kind = {}  # recipe entity id -> type
    self.hv = set()  # curve ids FissionOxide constrained horizontal/vertical on its own
    self.construction = set()

  def alloc(self):
    i = self.next_id
    self.next_id += 1
    return i

  def snap(self, at):
    best = None
    for pid, x, y in self.points:
      d = math.hypot(x - at[0], y - at[1])
      if d <= SNAP and (best is None or d < best[1]):
        best = (pid, d)
    return best[0] if best else None

  def resolve(self, spec, at):
    if spec is not None:
      return spec
    pid = self.alloc()
    self.points.append((pid, at[0], at[1]))
    return pid

  def new_point(self, at):
    pid = self.alloc()
    self.points.append((pid, at[0], at[1]))
    return pid


class Converter:
  def __init__(self, recipe):
    self.r = recipe
    self.out = []
    self.fid = 0  # last FissionOxide feature id
    self.sketch_defs = {}  # recipe sketch name -> recipe feature
    self.sketches = {}  # recipe sketch name -> Sketch (emitted at identity)
    self.bodies = {}  # recipe body name -> FissionOxide body id
    self.planes = {}  # construction plane name -> (origin, normal)
    self.features = {}  # recipe feature name -> recipe feature
    self.feature_bodies = {}
    self.sheet_rule = None
    self.unsupported = []
    self.params = {p["name"] for p in recipe.get("parameters", [])}

  # ---- output

  def cmd(self, _cmd, _feature=False, **params):
    line = {"cmd": _cmd}
    line.update({k: clean(v) for k, v in params.items()})
    self.out.append(json.dumps(line))
    if _feature:
      self.fid += 1
      return self.fid
    return None

  def comment(self, text):
    self.out.append("# " + text)

  def unsupported_op(self, text):
    self.unsupported.append(text)
    self.comment("UNSUPPORTED: " + text)

  def value(self, v):
    """A recipe value: a number, or an expression string ("height", "60 mm")."""
    return v

  # ---- sketches

  def frame(self, plane, T):
    o, x, y, n = plane["origin"], plane["x_dir"], plane["y_dir"], plane["normal"]
    # the recipe frame must be right-handed (y = n x x); otherwise mirror the sketch's y
    flip = dot(cross(n, x), y) < 0
    o, x, y, n = t_apply(T, o), t_dir(T, x), t_dir(T, y), t_dir(T, n)
    if t_det(T) < 0:
      flip = not flip
    return o, x, n, flip

  def emit_sketch(self, name, T=IDENTITY):
    rs = self.sketch_defs[name]
    o, x, n, flip = self.frame(rs["plane"], T)
    constrained = bool(rs.get("constraints") or rs.get("dimensions"))
    tag = "" if t_is_identity(T) else " (moved copy)"
    self.comment(f"sketch {name}{tag}: {rs['plane'].get('ref', '')}")
    fid = self.cmd("sketch.create", True, plane={"origin": o, "normal": n, "x": x})
    sk = Sketch(fid, constrained)
    sk.flip = flip
    fy = -1.0 if flip else 1.0

    def loc(p):
      return [float(p[0]), float(p[1]) * fy]

    def ptspec(spec):
      """(position, ref key or None) of a recipe point spec."""
      ref = spec.get("ref")
      if ref is not None and ref in sk.pos:
        return sk.pos[ref], ref
      return loc(spec["at"]), None

    for e in rs["entities"]:
      t, eid = e["type"], e["id"]
      sk.kind[eid] = t
      if t == "line":
        (a, _), (b, _) = ptspec(e["start"]), ptspec(e["end"])
        sa, sb = sk.snap(a), sk.snap(b)
        self.cmd("sketch.line", points=[a, b])
        pa, pb = sk.resolve(sa, a), sk.resolve(sb, b)
        cid = sk.alloc()
        if abs(a[1] - b[1]) < 1e-9 or abs(a[0] - b[0]) < 1e-9:
          sk.alloc()  # the automatic horizontal/vertical constraint
          sk.hv.add(cid)
        sk.curve[eid] = cid
        sk.pt[eid + ".start"], sk.pt[eid + ".end"] = pa, pb
        sk.pos[eid + ".start"], sk.pos[eid + ".end"] = a, b
        sk.geom[eid] = [a, b]
      elif t == "circle":
        c, _ = ptspec(e["center"])
        r = float(e["radius"])
        sc = sk.snap(c)
        self.cmd("sketch.circle", center=c, radius=r)
        pc = sk.resolve(sc, c)
        sk.curve[eid] = sk.alloc()
        sk.pt[eid + ".center"] = pc
        sk.pos[eid + ".center"] = c
        sk.geom[eid] = arc_polyline(c, r, 0, 2 * math.pi)
        sk.radius = getattr(sk, "radius", {})
        sk.radius[eid] = r
      elif t in ("arc_three_point", "arc_center_start_sweep"):
        if t == "arc_three_point":
          (s, _), (en, _) = ptspec(e["start"]), ptspec(e["end"])
          m = loc(e["mid"])
          c = circumcenter(s, m, en)
          if c is None:
            raise Unsupported(f"collinear arc {eid}")
        else:
          c, _ = ptspec(e["center"])
          s, _ = ptspec(e["start"])
          sw = math.radians(float(e["sweep_deg"])) * fy
          r = math.hypot(s[0] - c[0], s[1] - c[1])
          a0 = math.atan2(s[1] - c[1], s[0] - c[0])
          en = [c[0] + r * math.cos(a0 + sw), c[1] + r * math.sin(a0 + sw)]
          m = [c[0] + r * math.cos(a0 + sw / 2), c[1] + r * math.sin(a0 + sw / 2)]
          c = circumcenter(s, m, en) or c
        ss, se = sk.snap(s), sk.snap(en)
        self.cmd("sketch.arc", start=s, mid=m, end=en)
        ps, pe = sk.resolve(ss, s), sk.resolve(se, en)
        pcen = sk.new_point(c)
        sk.curve[eid] = sk.alloc()
        sk.pt[eid + ".start"], sk.pt[eid + ".end"], sk.pt[eid + ".center"] = ps, pe, pcen
        sk.pos[eid + ".start"], sk.pos[eid + ".end"], sk.pos[eid + ".center"] = s, en, c
        r = math.hypot(s[0] - c[0], s[1] - c[1])
        a0 = math.atan2(s[1] - c[1], s[0] - c[0])
        am = math.atan2(m[1] - c[1], m[0] - c[0])
        a1 = math.atan2(en[1] - c[1], en[0] - c[0])
        if ccw_sweep(a0, am) < ccw_sweep(a0, a1):
          sk.geom[eid] = arc_polyline(c, r, a0, ccw_sweep(a0, a1))
        else:
          sk.geom[eid] = arc_polyline(c, r, a1, ccw_sweep(a1, a0))
      elif t == "point":
        p = loc(e["at"])
        self.cmd("sketch.point", at=p)
        sk.pt[eid] = sk.new_point(p)
        sk.pos[eid] = p
      elif t == "fitted_spline":
        sk.geom[eid] = None
        sk.spline3d = getattr(sk, "spline3d", {})
        sk.spline3d[eid] = e["fit_points"]
        self.comment(f"fitted 3D spline {eid} not drawn (see the feature using it)")
      else:
        raise Unsupported(f"sketch entity type {t}")
      if e.get("construction") and eid in sk.curve:
        self.cmd("sketch.construction", curves=[sk.curve[eid]])
        sk.construction.add(eid)

    for c in rs.get("constraints") or []:
      self.constraint(sk, c)
    for d in rs.get("dimensions") or []:
      self.dimension(sk, d)
    if constrained:
      self.use_solved_geometry(sk)
    self.cmd("sketch.finish")
    return sk

  def use_solved_geometry(self, sk):
    """Replaces the sketch's drawn geometry with where the solver puts it, so profile points land
    inside the solved regions (recipes draw constrained sketches roughly on purpose)."""
    if not os.path.exists(CLI):
      raise Unsupported(f"constrained sketch needs {CLI} (cargo build -p fissionoxide-cli) to solve it")
    script = "\n".join(self.out + [json.dumps({"cmd": "sketch.info", "sketch": sk.fid})]) + "\n"
    run = subprocess.run([CLI, "run", "-"], input=script, capture_output=True, text=True)
    lines = [json.loads(l) for l in run.stdout.splitlines() if l.startswith("{")]
    info = lines[-1].get("ok") if lines else None
    if run.returncode != 0 or not info or not info.get("curves"):
      raise Unsupported(f"solving the sketch failed: {run.stderr.strip() or run.stdout[-300:]}")
    pts = {p["id"]: p["at"] for p in info["points"]}
    curves = {c["id"]: c["kind"] for c in info["curves"]}
    for eid, cid in sk.curve.items():
      k = curves.get(cid)
      if k is None or sk.geom.get(eid) is None:
        continue
      if k["type"] == "line":
        sk.geom[eid] = [pts[k["a"]], pts[k["b"]]]
      elif k["type"] == "circle":
        sk.geom[eid] = arc_polyline(pts[k["center"]], k["radius"], 0, 2 * math.pi)
      elif k["type"] == "arc":
        c, a, b = pts[k["center"]], pts[k["start"]], pts[k["end"]]
        a0, a1 = math.atan2(a[1] - c[1], a[0] - c[0]), math.atan2(b[1] - c[1], b[0] - c[0])
        sk.geom[eid] = arc_polyline(c, math.hypot(a[0] - c[0], a[1] - c[1]), a0, ccw_sweep(a0, a1))

  def point_id(self, sk, ref):
    if ref not in sk.pt:
      raise Unsupported(f"unknown sketch point {ref}")
    return sk.pt[ref]

  def constraint(self, sk, c):
    t, refs = c["type"], c["refs"]
    if t == "coincident":
      a, b = refs
      if "." not in b and b != "origin" and b in sk.curve:
        self.cmd("sketch.constrain", type="point_on_curve", points=[self.point_id(sk, a)], curves=[sk.curve[b]])
      elif "." not in a and a != "origin" and a in sk.curve:
        self.cmd("sketch.constrain", type="point_on_curve", points=[self.point_id(sk, b)], curves=[sk.curve[a]])
      else:
        pa, pb = self.point_id(sk, a), self.point_id(sk, b)
        if pa == pb:
          self.comment(f"coincident {a} {b}: already the same point")
          return
        self.cmd("sketch.constrain", type="coincident", points=[pa, pb])
    elif t in ("horizontal", "vertical"):
      cid = sk.curve[refs[0]]
      if cid in sk.hv:
        self.comment(f"{t} {refs[0]}: already constrained when drawn")
        return
      self.cmd("sketch.constrain", type=t, curves=[cid])
    elif t in ("horizontal_points", "vertical_points"):
      ty = t.split("_")[0]
      self.cmd("sketch.constrain", type=ty, points=[self.point_id(sk, refs[0]), self.point_id(sk, refs[1])])
    elif t in ("tangent", "equal", "parallel", "perpendicular", "concentric", "collinear"):
      self.cmd("sketch.constrain", type=t, curves=[sk.curve[refs[0]], sk.curve[refs[1]]])
    elif t == "midpoint":
      p, line = (refs[0], refs[1]) if refs[1] in sk.curve else (refs[1], refs[0])
      self.cmd("sketch.constrain", type="midpoint", points=[self.point_id(sk, p)], curves=[sk.curve[line]])
    elif t == "symmetric":
      a, b, line = refs
      if sk.kind.get(a) == "circle" and sk.kind.get(b) == "circle":
        self.cmd("sketch.constrain", type="symmetric", points=[sk.pt[a + ".center"], sk.pt[b + ".center"]],
                 curves=[sk.curve[line]])
        # symmetric circles are the same size
        self.cmd("sketch.constrain", type="equal", curves=[sk.curve[a], sk.curve[b]])
        sk.alloc()
      else:
        self.cmd("sketch.constrain", type="symmetric", points=[self.point_id(sk, a), self.point_id(sk, b)],
                 curves=[sk.curve[line]])
    else:
      raise Unsupported(f"sketch constraint {t}")
    sk.alloc()

  def dimension(self, sk, d):
    t, refs, v = d["type"], d["refs"], self.value(d["value"])
    if t == "distance":
      o = d.get("orientation", "aligned")
      ty = {"horizontal": "horizontal", "vertical": "vertical"}.get(o, "distance")
      self.cmd("sketch.dimension", type=ty, points=[self.point_id(sk, refs[0]), self.point_id(sk, refs[1])],
               value=v)
    elif t in ("radius", "diameter"):
      self.cmd("sketch.dimension", type=t, curves=[sk.curve[refs[0]]], value=v)
    elif t == "angle":
      # the recipe's angle is unsigned; FissionOxide's runs counter-clockwise from the first line to the
      # second (modulo 180 deg), so take the sign the drawn lines have
      if not isinstance(v, str):
        ga, gb = sk.geom[refs[0]], sk.geom[refs[1]]
        phi = math.degrees(math.atan2(gb[1][1] - gb[0][1], gb[1][0] - gb[0][0]) -
                           math.atan2(ga[1][1] - ga[0][1], ga[1][0] - ga[0][0]))

        def off(th):
          d = (phi - th) % 180
          return min(d, 180 - d)

        if off(-v) < off(v):
          v = -v
      val = v if isinstance(v, str) else f"{v} deg"
      self.cmd("sketch.dimension", type="angle", curves=[sk.curve[refs[0]], sk.curve[refs[1]]], value=val)
    elif t == "offset":
      line, p = (refs[0], refs[1]) if refs[0] in sk.curve else (refs[1], refs[0])
      self.cmd("sketch.dimension", type="point_line", points=[self.point_id(sk, p)], curves=[sk.curve[line]],
               value=v)
    else:
      raise Unsupported(f"sketch dimension {t}")
    sk.alloc()

  def sketch(self, name, T=IDENTITY):
    if t_is_identity(T):
      if name not in self.sketches:
        self.sketches[name] = self.emit_sketch(name)
      return self.sketches[name]
    return self.emit_sketch(name, T)

  # ---- profiles

  def profile_points(self, sk, profiles):
    """Points inside the regions the recipe's profiles cover, or None for "every region"."""
    used, holes = set(), False
    for prof in profiles:
      for lp in prof["loops"]:
        used.update(lp["curves"])
        holes = holes or not lp.get("outer", True)
    closed = {e for e, g in sk.geom.items() if g is not None and e not in sk.construction}
    if not holes and used >= closed:
      return None
    pts = []
    for prof in profiles:
      outer = [sk.geom[c] for lp in prof["loops"] if lp.get("outer", True) for c in dict.fromkeys(lp["curves"])]
      inner = [[sk.geom[c] for c in dict.fromkeys(lp["curves"])] for lp in prof["loops"] if not lp.get("outer", True)]
      allsegs = [g for e, g in sk.geom.items() if g is not None and e not in sk.construction]
      xs = [p[0] for poly in outer for p in poly]
      ys = [p[1] for poly in outer for p in poly]
      size = max(max(xs) - min(xs), max(ys) - min(ys))
      margin = min(0.5, 0.02 * size)

      def ok(p, margin=margin):
        if not crossings(p, outer) or any(crossings(p, h) for h in inner):
          return False
        return all(seg_dist(p, poly[i], poly[i + 1]) > margin for poly in allsegs for i in range(len(poly) - 1))

      cand = []
      # just inside the middle of each outer boundary curve (stable when a constrained sketch moves)
      for poly in outer:
        k = len(poly) // 2
        a, b = (poly[k - 1], poly[k]) if len(poly) > 2 else (poly[0], poly[1])
        mid = mul(add(a, b), 0.5)
        tn = unit(sub(b, a))
        nn = [-tn[1], tn[0]]
        for s in (1, -1):
          cand.append(add(mid, mul(nn, s * 2 * margin)))
      if not sk.constrained:
        n = 30
        for i in range(n):
          for j in range(n):
            cand.append([min(xs) + (i + 0.5) / n * (max(xs) - min(xs)), min(ys) + (j + 0.5) / n * (max(ys) - min(ys))])
      got = [p for p in cand if ok(p)]
      if not got:
        raise Unsupported("could not find a point inside a profile")
      pts.extend(got)
    # thin the list: keep points at least margin apart
    out = []
    for p in pts:
      if all(math.hypot(p[0] - q[0], p[1] - q[1]) > 1.0 for q in out):
        out.append(p)
    return out

  def profile_sketch(self, profiles):
    names = {p["sketch"] for p in profiles}
    if len(names) != 1:
      raise Unsupported("profiles from several sketches")
    return names.pop()

  # ---- bodies

  def new_bodies(self, fid, names):
    for k, n in enumerate(names or []):
      if n not in self.bodies:
        self.bodies[n] = fid * BODIES_PER_FEATURE + k

  def targets(self, names):
    return [self.bodies[n] for n in names or [] if n in self.bodies]

  # ---- features

  def run(self):
    r = self.r
    self.comment(f"{r['id']}: {r['description']}")
    self.comment("generated by tools/reference-import/import.py from recipe.json")
    for p in r.get("parameters", []):
      self.cmd("param.add", name=p["name"], expr=p["expression"])
    for f in r["features"]:
      if "name" in f:
        self.features[f["name"]] = f
      op = f["op"]
      if op == "sketch":
        self.sketch_defs[f["name"]] = f
        continue
      try:
        getattr(self, "op_" + op)(f, IDENTITY)
      except Unsupported as e:
        self.unsupported_op(f"{op} {f.get('name', '')}: {e}")
      except AttributeError as e:
        if f"op_{op}" in str(e):
          self.unsupported_op(f"operation '{op}' has no FissionOxide mapping in the importer")
        else:
          raise
    return self.out

  def op_extrude(self, f, T):
    op = {"new": "new_body", "join": "join", "cut": "cut", "intersect": "intersect"}[f["operation"]]
    if "thin" in f:
      th = f["thin"]
      name = th["sketch"]
      sk = self.sketch(name, T)
      side = th.get("side")
      t1, t2 = th["thickness"], th.get("thickness2") or 0
      if th.get("curves") and set(th["curves"]) != set(e for e in sk.curve if e not in sk.construction):
        raise Unsupported("thin extrude of some of a sketch's curves")
      if side == "two_sides" and t2 and t1 != t2:
        # two walls, inside and outside the closed curves, joined: FissionOxide's thin extrude has one
        # thickness on one side (or centred)
        self.comment(f"thin wall {t1} inside and {t2} outside the curves: two thin extrudes, joined")
        thin = [{"thin": t1, "thin_side": "inside"}, {"thin": t2, "thin_side": "outside"}]
      elif side == "two_sides":
        thin = [{"thin": 2 * t1, "thin_side": "center"}]
      else:
        thin = [{"thin": t1, "thin_side": side}]
      params = {}
    else:
      name = self.profile_sketch(f["profiles"])
      sk = self.sketch(name, T)
      params = {}
      pp = self.profile_points(sk, f["profiles"])
      if pp is not None:
        params["profiles"] = pp
    ext = f["extent"]
    et = ext["type"]
    if et == "distance":
      params["distance"] = self.value(ext["distance"])
      if ext.get("direction") == "negative":
        params["flip"] = True
    elif et == "symmetric":
      d = ext["distance"]
      if ext.get("is_full_length"):
        d = d / 2 if not isinstance(d, str) else f"({d}) / 2"
      params["extent"] = "symmetric"
      params["distance"] = d
    elif et == "two_sides":
      params["extent"] = "two_sides"
      params["distance"] = ext["side1"]
      params["distance2"] = ext["side2_opposite"]
    elif et == "through_all":
      params["extent"] = "through_all"
      if ext.get("direction") == "negative":
        params["flip"] = True
      elif ext.get("direction") not in (None, "positive"):
        raise Unsupported(f"through-all direction {ext.get('direction')}")
    else:
      raise Unsupported(f"extent {et}")
    if ext.get("taper_deg"):
      # recipe: positive grows outward; FissionOxide: positive tapers inward
      params["taper"] = f"{-ext['taper_deg']} deg"
    params["operation"] = op
    if op != "new_body":
      tg = self.targets(f.get("result_bodies"))
      if tg and t_is_identity(T):
        params["targets"] = tg
      elif tg:
        params["targets"] = tg
    if "thin" in f:
      fid = self.cmd("extrude", True, sketch=sk.fid, **params, **thin[0])
      for extra in thin[1:]:
        p2 = dict(params, operation="join" if op == "new_body" else op)
        if op == "new_body":
          p2["targets"] = [fid * BODIES_PER_FEATURE]
        self.cmd("extrude", True, sketch=sk.fid, **p2, **extra)
    else:
      fid = self.cmd("extrude", True, sketch=sk.fid, **params)
    if op == "new_body" and t_is_identity(T):
      self.new_bodies(fid, f.get("result_bodies"))

  def axis(self, ax):
    o, d = ax.get("origin", [0, 0, 0]), unit(ax["dir"])
    if norm(o) > 1e-9:
      raise Unsupported(f"axis through {o}")
    for name, v in (("x", [1, 0, 0]), ("y", [0, 1, 0]), ("z", [0, 0, 1])):
      if norm(sub(d, v)) < 1e-9:
        return name, 1
      if norm(add(d, v)) < 1e-9:
        return name, -1
    raise Unsupported(f"axis direction {d}")

  def op_revolve(self, f, T):
    prof = f["profile"]
    sk = self.sketch(prof["sketch"], T)
    params = {}
    pp = self.profile_points(sk, [prof])
    if pp is not None:
      params["profiles"] = pp
    axis, sgn = self.axis(f["axis"])
    ang = f["angle"] * sgn
    op = {"new": "new_body", "join": "join", "cut": "cut", "intersect": "intersect"}[f["operation"]]
    if op != "new_body":
      params["targets"] = self.targets(f.get("result_bodies"))
    fid = self.cmd("revolve", True, sketch=sk.fid, axis=axis, angle=f"{ang} deg", operation=op, **params)
    if op == "new_body":
      self.new_bodies(fid, f.get("result_bodies"))

  def op_construction_plane(self, f, T):
    if f["method"] == "offset":
      base = {"XY": [0, 0, 1], "XZ": [0, 1, 0], "YZ": [1, 0, 0]}[f["base"]]
      self.planes[f["name"]] = (mul(base, f["offset"]), base)
    elif "result_origin" in f:
      self.planes[f["name"]] = (f["result_origin"], f["result_normal"])
    else:
      raise Unsupported(f"construction plane method {f['method']}")
    self.comment(f"construction plane {f['name']}: sketches on it use its frame directly")

  def op_loft(self, f, T):
    secs = []
    for s in f["sections"]:
      sk = self.sketch(s["sketch"], T)
      if "loops" in s:
        pp = self.profile_points(sk, [s])
        secs.append({"sketch": sk.fid, "profile": pp[0]} if pp else sk.fid)
      else:
        secs.append(sk.fid)  # a point section
    if f.get("rails"):
      raise Unsupported("loft rails")
    fid = self.cmd("loft", True, sections=secs, operation="new_body")
    self.new_bodies(fid, f.get("result_bodies"))

  def op_sweep(self, f, T):
    path = f["path"]
    psk = self.sketch(path["sketch"], T)
    spl = getattr(psk, "spline3d", {})
    if any(c in spl for c in path["curves"]):
      return self.helix_sweep(f, spl[path["curves"][0]])
    prof = f["profile"]
    sk = self.sketch(prof["sketch"], T)
    params = {}
    pp = self.profile_points(sk, [prof])
    if pp is not None:
      params["profiles"] = pp
    fid = self.cmd("sweep", True, sketch=sk.fid, path=psk.fid, path_curves=[psk.curve[c] for c in path["curves"]],
                   operation="new_body", **params)
    self.new_bodies(fid, f.get("result_bodies"))

  def helix_sweep(self, f, pts):
    """A circle swept along a fitted helix through the Z axis: FissionOxide's coil."""
    rs = [math.hypot(p[0], p[1]) for p in pts]
    R = sum(rs) / len(rs)
    if max(rs) - min(rs) > 1e-6 * R:
      raise Unsupported("fitted 3D spline path that is not a helix about Z")
    angs = [math.atan2(p[1], p[0]) for p in pts]
    turn = 0.0
    for i in range(1, len(angs)):
      turn += (angs[i] - angs[i - 1] + math.pi) % (2 * math.pi) - math.pi
    revs = turn / (2 * math.pi)
    height = pts[-1][2] - pts[0][2]
    pitch = height / revs
    prof = f["profile"]
    rs_ = self.sketch_defs[prof["sketch"]]
    circ = [e for e in rs_["entities"] if e["id"] in prof["loops"][0]["curves"]][0]
    self.comment(f"sweep along a fitted helix (R {R:.6g}, pitch {pitch:.6g}, {revs:.6g} turns) as a coil")
    fid = self.cmd("coil", True, plane="xy", diameter=2 * R, pitch=round(pitch, 9), revolutions=round(abs(revs), 9),
                   section="circular", section_size=2 * circ["radius"], left=revs < 0)
    self.new_bodies(fid, f.get("result_bodies"))

  def face_normal(self, face, at=None):
    if face["type"] == "plane":
      return face.get("plane_normal_outward") or face["normal_at_point_on_face"]
    if face["type"] == "cylinder" and at is not None:
      ax, o = unit(face["axis"]), face["axis_origin"]
      v = sub(at, o)
      v = sub(v, mul(ax, dot(v, ax)))
      n = unit(v)
      if dot(n, face["normal_at_point_on_face"]) < 0 and norm(sub(at, face["point"])) < 1e-9:
        n = mul(n, -1)
      return n
    return face["normal_at_point_on_face"]

  def op_hole(self, f, T):
    kind = f["hole_type"]
    params = {"diameter": f["diameter"]}
    if kind == "counterbore":
      params.update(kind="counterbore", head_diameter=f["counterbore_diameter"], head_depth=f["counterbore_depth"])
    elif kind == "countersink":
      params.update(kind="countersink", head_diameter=f["countersink_diameter"],
                    head_angle=f"{f['countersink_angle']} deg")
    elif kind != "simple":
      raise Unsupported(f"hole type {kind}")
    if f["extent"] == "through_all":
      params["through_all"] = True
    else:
      params["depth"] = f["depth"]
      if "tip_angle" in f:
        params["tip_angle"] = f"{f['tip_angle']} deg"
    if "positions_from" in f:
      name = f["positions_from"].split()[1]
      sk = self.sketch(name, T)
      self.cmd("hole", True, sketch=sk.fid, **params)
      return
    pos = t_apply(T, f["position"])
    n = t_dir(T, self.face_normal(f["face"], f["position"]))
    x = cross([0, 0, 1], n) if abs(n[2]) < 0.9 else cross([0, 1, 0], n)
    self.comment(f"hole {f['name']} at {clean(pos, 6)}: a sketch point on the face plane there")
    fid = self.cmd("sketch.create", True, plane={"origin": pos, "normal": n, "x": unit(x)})
    self.cmd("sketch.point", at=[0, 0])
    self.cmd("hole", True, sketch=fid, **params)

  def edges(self, f, T, key="edges"):
    return [{"point": t_apply(T, edge_point(e))} for e in f[key]]

  def op_fillet(self, f, T):
    t = f.get("type", "constant_radius")
    if "fit" in f:
      raise Unsupported(f"fillet fit '{f['fit']}' (radius following the room available)")
    if t == "constant_radius":
      self.cmd("fillet", True, edges=self.edges(f, T), radius=f["radius"])
    elif t == "variable_radius":
      if len(f["edges"]) != 1:
        raise Unsupported("variable fillet on several edges")
      e = f["edges"][0]
      r1, r2 = f["radius"], f["radius2"]
      self.comment(f"variable radius {r1} at {e['start']} to {r2} at {e['end']} (recipe start {f.get('start')})")
      if f.get("start") is not None and norm(sub(f["start"], e["start"])) > 1e-6:
        r1, r2 = r2, r1
      self.cmd("fillet", True, edges=self.edges(f, T), radius=r1, radius2=r2)
    elif t == "chord_length":
      self.cmd("fillet", True, edges=self.edges(f, T), radius=f["radius"], chord=True)
    elif t == "spline_radius":
      if len(f["edges"]) != 1:
        raise Unsupported("spline-radius fillet on several edges")
      e = f["edges"][0]
      law = [f["radius"]] + f["radii"]
      flip = f.get("start") is not None and norm(sub(f["start"], e["start"])) > 1e-6
      self.comment(f"B-spline radius law {law} on knots {f['knots']}, sampled at {RADIUS_SAMPLES} points")
      pts = []
      for i in range(RADIUS_SAMPLES + 1):
        u = i / RADIUS_SAMPLES
        pts.append([1 - u if flip else u, clean(bspline(law, f["knots"], u), 9)])
      pts.sort()
      self.cmd("fillet", True, edges=self.edges(f, T), radius=pts[0][1], radius2=pts[-1][1], radii=pts[1:-1])
    else:
      raise Unsupported(f"fillet type '{t}'")

  def op_chamfer(self, f, T):
    if f["type"] != "equal_distance":
      raise Unsupported(f"chamfer type {f['type']}")
    self.cmd("chamfer", True, edges=self.edges(f, T), distance=f["distance"])

  def op_shell(self, f, T):
    if f.get("outside_thickness"):
      raise Unsupported("shell with outside thickness")
    faces = [{"point": t_apply(T, x["point"]), "normal": t_dir(T, self.face_normal(x))} for x in f["faces_removed"]]
    self.cmd("shell", True, faces=faces, thickness=f["inside_thickness"])

  def op_draft(self, f, T):
    faces = [{"point": x["point"]} for x in f["faces"]]
    npl = f["neutral_plane"]
    plane = {"origin": npl["point"], "normal": self.face_normal(npl)}
    params = {}
    if f.get("flipped"):
      params["flip"] = True
    self.cmd("draft", True, faces=faces, plane=plane, angle=f"{f['angle']} deg", **params)

  def op_combine(self, f, T):
    op = {"join": "join", "cut": "cut", "intersect": "intersect"}[f["operation"]]
    params = {}
    if f.get("keep_tools"):
      params["keep_tools"] = True
    self.cmd("combine", True, target=self.bodies[f["target_body"]],
             tools=[self.bodies[b] for b in f["tool_bodies"]], operation=op, **params)

  def replay(self, names, T):
    for n in names:
      base = n
      if base not in self.features and base.endswith(")") and " (" in base:
        base = base[: base.rindex(" (")]
      if base not in self.features:
        raise Unsupported(f"patterned feature {n} not found")
      g = self.features[base]
      fn = getattr(self, "op_" + g["op"], None)
      if fn is None or g["op"] not in ("extrude", "revolve", "hole", "fillet", "chamfer"):
        raise Unsupported(f"patterning a {g['op']} feature")
      fn(g, T)

  def op_circular_pattern(self, f, T):
    if f["pattern_of"] != "features":
      raise Unsupported("body pattern")
    ax = f["axis"]
    n, total = f["count"], f["total_angle"]
    if f.get("symmetric"):
      raise Unsupported("symmetric circular pattern")
    step = total / n if abs(total - 360) < 1e-9 else total / (n - 1)
    self.comment(f"circular pattern of {f['features']}: {n} x {step} deg, replayed feature by feature")
    for k in range(1, n):
      self.replay(f["features"], rotation(ax["origin"], ax["dir"], math.radians(step * k)))

  def op_rectangular_pattern(self, f, T):
    if f["pattern_of"] != "features":
      raise Unsupported("body pattern")
    d1, d2 = f["direction1"], f.get("direction2") or {"axis": [0, 0, 0], "count": 1, "spacing": 0}
    self.comment(f"rectangular pattern of {f['features']}, replayed feature by feature")
    for i in range(d1["count"]):
      for j in range(d2["count"]):
        if i == 0 and j == 0:
          continue
        v = add(mul(unit(d1["axis"]), i * d1["spacing"]), mul(unit(d2["axis"]) if j else [0, 0, 0], j * d2["spacing"]))
        self.replay(f["features"], translation(v))

  def op_path_pattern(self, f, T):
    path = f["path"]
    rs = self.sketch_defs[path["sketch"]]
    if len(path["curves"]) != 1:
      raise Unsupported("path pattern along several curves")
    e = [x for x in rs["entities"] if x["id"] == path["curves"][0]][0]
    if e["type"] != "arc_three_point" or f.get("orientation", "").split()[0] != "identical":
      raise Unsupported("path pattern along a non-arc path, or with rotation")
    pl = rs["plane"]
    s, m, en = e["start"]["at"], e["mid"], e["end"]["at"]
    c = circumcenter(s, m, en)
    r = math.hypot(s[0] - c[0], s[1] - c[1])
    a0, am, a1 = (math.atan2(p[1] - c[1], p[0] - c[0]) for p in (s, m, en))
    sgn = 1 if ccw_sweep(a0, am) < ccw_sweep(a0, a1) else -1

    def world(p2):
      return add(pl["origin"], add(mul(pl["x_dir"], p2[0]), mul(pl["y_dir"], p2[1])))

    self.comment(f"path pattern of {f['features']}: {f['count']} along arc {e['id']} every {f['spacing']} mm")
    p0 = world(s)
    for k in range(1, f["count"]):
      a = a0 + sgn * k * f["spacing"] / r
      pk = world([c[0] + r * math.cos(a), c[1] + r * math.sin(a)])
      self.replay(f["features"], translation(sub(pk, p0)))

  def op_mirror(self, f, T):
    pl = f["plane"]
    if f["mirror_of"] == "features":
      self.comment(f"mirror of {f['features']}, replayed on mirrored sketches")
      self.replay(f["features"], reflection(pl["origin"], pl["normal"]))
    else:
      bodies = [self.bodies[b] for b in f["bodies"]]
      self.cmd("mirror", True, plane={"origin": pl["origin"], "normal": pl["normal"]}, bodies=bodies,
               join=bool(f.get("combine")))

  def op_split_body(self, f, T):
    tool = f["tool"]
    if tool not in self.planes:
      raise Unsupported(f"split tool {tool}")
    o, n = self.planes[tool]
    self.cmd("split", True, plane={"origin": o, "normal": n}, bodies=[self.bodies[f["body"]]])

  def op_thread(self, f, T):
    if f.get("modeled"):
      raise Unsupported("modeled thread")
    face = f["face"]
    params = {}
    des = f.get("designation", "")
    if "x" in des:
      params["pitch"] = float(des.split("x")[1])
    self.comment(f"cosmetic thread {des} {f.get('class', '')}: no change to the solid")
    self.cmd("thread", True, faces=[{"point": face["point"]}], **params)

  # ---- sheet metal

  def ensure_rule(self, name):
    if self.sheet_rule is None:
      self.sheet_rule = name
      self.comment(f"sheet metal rule '{name}': thickness 2.5, bend radius = thickness, K 0.44")
      self.cmd("param.add", name="sm_thickness", expr="2.5 mm")
      self.cmd("param.add", name="sm_bend_radius", expr="sm_thickness")
      self.cmd("sheet.rule", name=name, thickness="sm_thickness", bend_radius="sm_bend_radius", k_factor=0.44)

  def op_sm_flange(self, f, T):
    t = f["type"]
    rule = f.get("rule", self.sheet_rule or "Steel (mm)")
    self.ensure_rule(rule)
    if t == "base":
      prof = f["profile"]
      sk = self.sketch(prof["sketch"], T)
      params = {}
      pp = self.profile_points(sk, [prof])
      if pp is not None:
        params["profiles"] = pp
      fid = self.cmd("sheet.flange", True, sketch=sk.fid, rule=rule, **params)
      self.new_bodies(fid, f.get("result_bodies") or ["Body1"])
    elif t == "edge":
      if f.get("bend_radius", "rule").split()[0] != "rule":
        raise Unsupported(f"edge flange bend radius {f['bend_radius']}")
      pos = f.get("bend_position", "Inside").split()[0].lower()
      params = {"position": pos}
      if f.get("flip"):
        params["flip"] = True
      if len(f.get("edges") or []) > 1:
        self.comment("the source's corners: walls run on, cut square, until their inside corners are the rule gap (the thickness) apart")
        self.cmd("sheet.rule", name=rule, gap="sm_thickness")
        params["corners"] = "butt"
      self.cmd("sheet.edge_flange", True, edges=self.edges(f, T), height=f["height"], angle=f"{f['angle']} deg",
               **params)
    elif t == "contour":
      self.comment("the recipe puts the material right of the sketch path's travel; FissionOxide's default is left: flip")
      sk = self.sketch(f["profile_sketch"], T)
      self.cmd("sheet.contour_flange", True, sketch=sk.fid, curves=[sk.curve[c] for c in f["profile_curves"]],
               distance=f["distance"], rule=rule, flip=True)
    else:
      raise Unsupported(f"sheet metal flange type {t}")

  def op_sheet_metal_rule_edit(self, f, T):
    self.ensure_rule(f["rule"])
    changes = dict(f.get("set") or {})
    for s in f.get("sequence") or []:
      k, v = s.split(" ", 1)
      changes[k] = v
    for k, v in changes.items():
      if k == "thickness":
        self.cmd("param.set", name="sm_thickness", expr=v)
      elif k == "bendRadius":
        self.cmd("param.set", name="sm_bend_radius", expr=v)
      elif k == "kFactor":
        self.cmd("sheet.rule", name=f["rule"], k_factor=float(v))
      else:
        raise Unsupported(f"sheet metal rule field {k}")

  def op_sm_hem(self, f, T):
    if f.get("type") != "flat":
      raise Unsupported(f"hem type {f.get('type')}")
    # the recipe's hem folds over the flange's inner face; FissionOxide's default folds it outward
    params = {} if f.get("flipped") else {"flip": True}
    self.comment("hem gap 0.01 mm: the source's rule hem gap; folded over the flange's inner face")
    self.cmd("sheet.hem", True, edges=[{"point": f["edge"]["point"]}], length=f["length"], gap=0.01, **params)

  def op_sm_unfold(self, f, T):
    if not f.get("all_bends", True):
      raise Unsupported("unfolding some bends only")
    sf = f["stationary_face"]
    self.cmd("sheet.unfold", True, face={"point": sf["point"], "normal": self.face_normal(sf)})

  def op_sm_refold(self, f, T):
    self.cmd("sheet.refold", True)

  # ---- surfaces

  def op_boundary_surface(self, f, T):
    sk = self.sketch(f["sketch"], T)
    if set(f["curves"]) != set(e for e in sk.curve if e not in sk.construction):
      raise Unsupported("boundary surface from some of a sketch's curves")
    fid = self.cmd("surface.patch", True, sketch=sk.fid)
    self.new_bodies(fid, f.get("result_bodies"))

  def op_thicken(self, f, T):
    params = {}
    if f.get("symmetric"):
      params["symmetric"] = True
    fid = self.cmd("thicken", True, bodies=[self.bodies[b] for b in f["bodies"]], thickness=f["thickness"],
                   operation="new_body", **params)
    self.new_bodies(fid, f.get("result_bodies") or [f.get("name", "Thicken")])

  def op_remove(self, f, T):
    self.cmd("body.remove", True, bodies=[self.bodies[b] for b in f["bodies"]])


def convert(case_dir):
  recipe = json.load(open(os.path.join(case_dir, "recipe.json")))
  conv = Converter(recipe)
  lines = conv.run()
  return "\n".join(lines) + "\n", conv.unsupported


def main(argv):
  filters = argv[1:]
  n = 0
  for name in sorted(os.listdir(REF)):
    d = os.path.join(REF, name)
    if not os.path.isfile(os.path.join(d, "recipe.json")):
      continue
    if filters and not any(f in name for f in filters):
      continue
    out = os.path.join(d, "part.jsonl")
    if os.path.exists(out) and any(l.strip() == "# hand-edited" for l in open(out)):
      print(f"{name}: hand-edited, skipped")
      continue
    try:
      text, unsupported = convert(d)
    except Unsupported as e:
      print(f"{name}: FAILED {e}")
      continue
    open(out, "w").write(text)
    n += 1
    if unsupported:
      print(f"{name}: unsupported: {'; '.join(unsupported)}")
  print(f"wrote {n} scripts")


if __name__ == "__main__":
  main(sys.argv)
