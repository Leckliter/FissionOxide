"""Builds a neutral recipe (tests/reference/*/recipe.json) in Fusion and measures the result.

The recipe format is documented in tests/reference/README.md. In short: sketches carry their
world frame (origin, x_dir, y_dir, normal) and sketch-local 2D points; faces and edges are picked
by a world point; values are mm and degrees unless they are expressions.

Sketch constraints and dimensions are not applied: the recipe already gives every point's solved
position, so the geometry is the same without them, and applying them risks Fusion re-solving the
sketch somewhere else. Operations this runner can't build yet are reported under `unsupported`
and the measurement is still returned, so a caller can tell "Fusion disagrees" from "not built".
"""

import math
import traceback

import adsk.core
import adsk.fusion

import measure

MM = 0.1  # Fusion's internal length unit is cm
SCRATCH_PREFIX = "FoxBridge "

OPERATIONS = {
  "new": adsk.fusion.FeatureOperations.NewBodyFeatureOperation,
  "join": adsk.fusion.FeatureOperations.JoinFeatureOperation,
  "cut": adsk.fusion.FeatureOperations.CutFeatureOperation,
  "intersect": adsk.fusion.FeatureOperations.IntersectFeatureOperation,
  "new_component": adsk.fusion.FeatureOperations.NewComponentFeatureOperation,
}


class Unsupported(Exception):
  pass


def V(expr, unit="mm"):
  """A ValueInput from a number (in `unit`) or an expression string."""
  if isinstance(expr, str):
    return adsk.core.ValueInput.createByString(expr)
  return adsk.core.ValueInput.createByString(f"{expr} {unit}")


def P(v):
  return adsk.core.Point3D.create(v[0] * MM, v[1] * MM, (v[2] if len(v) > 2 else 0.0) * MM)


def add(a, b):
  return [a[i] + b[i] for i in range(3)]


def scale(a, s):
  return [a[i] * s for i in range(3)]


def collection(items):
  c = adsk.core.ObjectCollection.create()
  for i in items:
    c.add(i)
  return c


class Runner:
  def __init__(self, app, recipe):
    self.app = app
    self.recipe = recipe
    self.doc = app.documents.add(adsk.core.DocumentTypes.FusionDesignDocumentType)
    try:
      self.doc.name = SCRATCH_PREFIX + recipe.get("id", "recipe")
    except Exception:
      pass
    self.design = adsk.fusion.Design.cast(app.activeProduct)
    self.design.designType = adsk.fusion.DesignTypes.ParametricDesignType
    self.root = self.design.rootComponent
    self.sketches = {}  # name -> {"sketch", "frame", "entities": {id: curve}, "points": {ref: SketchPoint}}
    self.planes = {"XY": self.root.xYConstructionPlane, "XZ": self.root.xZConstructionPlane,
                   "YZ": self.root.yZConstructionPlane}
    self.axes = {"X": self.root.xConstructionAxis, "Y": self.root.yConstructionAxis,
                 "Z": self.root.zConstructionAxis}
    self.points = {}  # name -> ConstructionPoint
    self.features = {}
    self.bodies = {}
    self.steps = []
    self.unsupported = []

  # ---- picking --------------------------------------------------------------------------------

  def find(self, point, kind, hint=None):
    """The face or edge at a world point. A point on an edge between two faces hits both, so a
    face pick prefers the hit whose surface type, normal and radius match the recipe's ref."""
    etype = {"face": adsk.fusion.BRepEntityTypes.BRepFaceEntityType,
             "edge": adsk.fusion.BRepEntityTypes.BRepEdgeEntityType}[kind]
    for tol in (1e-4, 1e-3, 1e-2):
      hits = self.root.findBRepUsingPoint(P(point), etype, tol, False)
      if hits and hits.count:
        found = [hits.item(i) for i in range(hits.count)]
        if kind == "face" and hint and len(found) > 1:
          found.sort(key=lambda f: -self.face_score(f, point, hint))
        return found[0]
    raise RuntimeError(f"no {kind} at {point}")

  def face_score(self, face, point, hint):
    score = 0.0
    g = face.geometry
    if hint.get("type") and g.objectType.split("::")[-1].lower() == str(hint["type"]).lower():
      score += 1
    if hint.get("radius") is not None and hasattr(g, "radius") and abs(g.radius / MM - hint["radius"]) < 1e-3:
      score += 1
    n = hint.get("normal_at_point_on_face") or hint.get("normal")
    if n:
      try:
        ok, fn = face.evaluator.getNormalAtPoint(P(point))
        if ok:
          length = math.sqrt(sum(x * x for x in n)) or 1.0
          score += 2 * abs(fn.x * n[0] + fn.y * n[1] + fn.z * n[2]) / length
      except Exception:
        pass
    return score

  def faces(self, refs):
    return [self.find(r["point"], "face", r) for r in refs]

  def edges(self, refs):
    return [self.find(r["point"], "edge") for r in refs]

  def body(self, name):
    if name in self.bodies:
      return self.bodies[name]
    for i in range(self.root.bRepBodies.count):
      b = self.root.bRepBodies.item(i)
      if b.name == name:
        return b
    raise RuntimeError(f"no body named {name!r}")

  def axis(self, a):
    """A linear entity for {ref, origin, dir}, a world axis name, or a sketch line {sketch, curve}."""
    if isinstance(a, str):
      a = {"ref": a}
    if a.get("ref") in self.axes:
      return self.axes[a["ref"]]
    ref = (a.get("ref") or "").lower()
    for k in ("x", "y", "z"):
      if ref.startswith(f"world {k} axis") or ref == k:
        return self.axes[k.upper()]
    if "sketch" in a and "curve" in a:
      return self.sketches[a["sketch"]]["entities"][a["curve"]]
    o, d = a.get("origin", [0, 0, 0]), a.get("dir")
    if d is not None:
      unit = [abs(x) for x in d]
      if max(o, key=abs) == 0 and sorted(unit) == [0, 0, 1]:
        return self.axes["XYZ"[unit.index(1)]]
      # A free axis: a construction line in a helper sketch (3D sketch geometry is allowed).
      sk = self.root.sketches.add(self.planes["XY"])
      sk.name = "FoxBridge axis"
      line = sk.sketchCurves.sketchLines.addByTwoPoints(sk.modelToSketchSpace(P(o)),
                                           sk.modelToSketchSpace(P(add(o, scale(d, 10)))))
      line.isConstruction = True
      return line
    raise RuntimeError(f"can't resolve axis {a}")

  def plane(self, p):
    ref = p.get("ref", "") if isinstance(p, dict) else p
    if ref in self.planes:
      return self.planes[ref]
    if isinstance(p, dict) and "face" in p:
      return self.find(p["face"]["point"], "face", p["face"])
    if ref.startswith("construction plane "):
      name = ref.split()[2]
      if name in self.planes:
        return self.planes[name]
    if ref.startswith("face") and isinstance(p, dict) and "origin" in p:
      return self.find(p["origin"], "face")
    if isinstance(p, dict) and "origin" in p and "normal" in p:
      return self.free_plane(p["origin"], p["normal"])
    raise RuntimeError(f"can't resolve plane {p}")

  def free_plane(self, origin, normal):
    n = [abs(x) for x in normal]
    k = n.index(max(n))
    if sorted(n) != [0, 0, 1]:
      raise Unsupported(f"oblique free plane normal {normal}")
    base = self.planes[["YZ", "XZ", "XY"][k]]
    inp = self.root.constructionPlanes.createInput()
    inp.setByOffset(base, V(origin[k]))
    return self.root.constructionPlanes.add(inp)

  # ---- sketches -------------------------------------------------------------------------------

  def op_sketch(self, f):
    pl = f["plane"]
    sk = self.root.sketches.add(self.plane(pl))
    sk.name = f.get("name", sk.name)
    # A sketch on a face starts with the face's edges projected into it; the recipe's sketches
    # hold only their own curves, so drop them.
    for c in [sk.sketchCurves.item(i) for i in range(sk.sketchCurves.count)]:
      c.deleteMe()
    o, xd, yd = pl.get("origin", [0, 0, 0]), pl.get("x_dir", [1, 0, 0]), pl.get("y_dir", [0, 1, 0])
    nd = pl.get("normal", [0, 0, 1])
    rec = {"sketch": sk, "entities": {}, "points": {}}
    o_world = o

    def world(uv):
      w = add(o, add(scale(xd, uv[0]), scale(yd, uv[1])))
      return add(w, scale(nd, uv[2])) if len(uv) > 2 else w

    def pt(spec):
      if isinstance(spec, dict) and spec.get("ref") == "origin":
        return origin_point()
      if isinstance(spec, dict) and spec.get("ref") in rec["points"]:
        return rec["points"][spec["ref"]]
      uv = spec["at"] if isinstance(spec, dict) else spec
      return sk.modelToSketchSpace(P(world(uv)))

    def origin_point():
      # The recipe's "origin" is the frame origin: Fusion's sketch origin when they coincide,
      # otherwise a fixed point there.
      if "origin" not in rec["points"]:
        o = sk.originPoint
        frame_origin = P(o_world)
        if o.worldGeometry.distanceTo(frame_origin) > 1e-6:
          o = sk.sketchPoints.add(sk.modelToSketchSpace(frame_origin))
          o.isFixed = True
        rec["points"]["origin"] = o
      return rec["points"]["origin"]

    def arc_ends(c, start_spec):
      # Fusion stores arcs counter-clockwise about the sketch normal, so the recipe's start can be
      # Fusion's end; name the endpoints by where they are.
      want = pt(start_spec)
      want = want.geometry if hasattr(want, "geometry") else want
      s0, s1 = c.startSketchPoint, c.endSketchPoint
      if s0.geometry.distanceTo(want) > s1.geometry.distanceTo(want):
        s0, s1 = s1, s0
      return {"start": s0, "end": s1, "center": c.centerSketchPoint}

    def keep(eid, curve, **points):
      rec["entities"][eid] = curve
      for k, v in points.items():
        rec["points"][f"{eid}.{k}"] = v

    for e in f.get("entities", []):
      t, eid = e["type"], e.get("id")
      if t == "line":
        c = sk.sketchCurves.sketchLines.addByTwoPoints(pt(e["start"]), pt(e["end"]))
        keep(eid, c, start=c.startSketchPoint, end=c.endSketchPoint)
      elif t == "line_midpoint":
        # Midpoint Line: "mid" is the midpoint, "end" one end; Fusion mirrors it for the other end
        # and holds the midpoint on the line with a midpoint constraint. '<id>.end' is the end
        # nearest "end", '<id>.start' the mirrored one, '<id>.mid' the midpoint.
        mid = pt(e["mid"])
        c = sk.sketchCurves.sketchLines.addByMidpoint(mid, pt(e["end"]))
        if not c:
          raise RuntimeError("addByMidpoint returned null")
        want = pt(e["end"])
        want = want.geometry if hasattr(want, "geometry") else want
        s0, s1 = c.startSketchPoint, c.endSketchPoint
        if s1.geometry.distanceTo(want) > s0.geometry.distanceTo(want):
          s0, s1 = s1, s0
        mids = {}
        gcs = sk.geometricConstraints
        for i in range(gcs.count):
          g = adsk.fusion.MidPointConstraint.cast(gcs.item(i))
          if g and g.midPointCurve == c:
            mids["mid"] = g.point
        if not mids and hasattr(mid, "geometry"):
          mids["mid"] = mid
        keep(eid, c, start=s0, end=s1, **mids)
      elif t == "circle":
        c = sk.sketchCurves.sketchCircles.addByCenterRadius(pt(e["center"]), e["radius"] * MM)
        keep(eid, c, center=c.centerSketchPoint)
      elif t == "circle_two_point":
        # A diameter's ends ("through": [a, b]). Fusion takes plain points, so a referenced sketch
        # point is held on the circle with a coincident constraint, as clicking it in the UI does.
        ends = [pt(s) for s in e["through"]]
        c = sk.sketchCurves.sketchCircles.addByTwoPoints(
            *[p.geometry if hasattr(p, "geometry") else p for p in ends])
        for p in ends:
          if hasattr(p, "geometry"):
            sk.geometricConstraints.addCoincident(p, c)
        keep(eid, c, center=c.centerSketchPoint)
      elif t == "circle_three_point":
        # 3-Point Circle: through three points on the circumference ("through": [a, b, c]); a
        # referenced sketch point is held on the circle with a coincident constraint, as above.
        pts = [pt(s) for s in e["through"]]
        c = sk.sketchCurves.sketchCircles.addByThreePoints(
            *[p.geometry if hasattr(p, "geometry") else p for p in pts])
        if not c:
          raise RuntimeError(f"addByThreePoints failed for {eid}")
        for p in pts:
          if hasattr(p, "geometry"):
            sk.geometricConstraints.addCoincident(p, c)
        keep(eid, c, center=c.centerSketchPoint)
      elif t == "circle_two_tangent":
        # 2-Tangent Circle: tangent to two earlier lines ("tangents": [id, id]) with a radius; the
        # hint point ("near") picks the quadrant the lines (as infinite lines) make.
        a, b = [rec["entities"][r] for r in e["tangents"]]
        hint = pt(e["near"])
        c = sk.sketchCurves.sketchCircles.addByTwoTangents(
            a, b, e["radius"] * MM, hint.geometry if hasattr(hint, "geometry") else hint)
        if not c:
          raise RuntimeError(f"addByTwoTangents failed for {eid}")
        keep(eid, c, center=c.centerSketchPoint)
      elif t == "circle_three_tangent":
        # 3-Tangent Circle: tangent to three earlier lines ("tangents": [id, id, id]); the hint
        # point ("near") picks the region the lines (as infinite lines) make.
        a, b, d = [rec["entities"][r] for r in e["tangents"]]
        hint = pt(e["near"])
        c = sk.sketchCurves.sketchCircles.addByThreeTangents(
            a, b, d, hint.geometry if hasattr(hint, "geometry") else hint)
        if not c:
          raise RuntimeError(f"addByThreeTangents failed for {eid}")
        keep(eid, c, center=c.centerSketchPoint)
      elif t == "arc_three_point":
        c = sk.sketchCurves.sketchArcs.addByThreePoints(pt(e["start"]), pt(e["mid"]), pt(e["end"]))
        keep(eid, c, **arc_ends(c, e["start"]))
      elif t == "arc_tangent":
        # Tangent Arc: starts at an end of an earlier line or arc ("tangent": id; "start" refs
        # that end) carrying on its direction, and ends at "end". The API has no tangent arc, so
        # the arc is solved here (centre on the start's normal, equidistant from both ends), made
        # through three points and held tangent with a constraint, as the UI tool does.
        prev = rec["entities"][e["tangent"]]
        sq = pt(e["start"])
        s = sq.geometry if hasattr(sq, "geometry") else sq
        eq = pt(e["end"])
        en = eq.geometry if hasattr(eq, "geometry") else eq
        ln = adsk.fusion.SketchLine.cast(prev)
        ar = adsk.fusion.SketchArc.cast(prev)
        if ln:
          a, b = ln.startSketchPoint.geometry, ln.endSketchPoint.geometry
          if b.distanceTo(s) > a.distanceTo(s):
            a, b = b, a
          d = (b.x - a.x, b.y - a.y)
        elif ar:
          cg = ar.centerSketchPoint.geometry
          ccw = (-(s.y - cg.y), s.x - cg.x)
          at_end = ar.endSketchPoint.geometry.distanceTo(s) < ar.startSketchPoint.geometry.distanceTo(s)
          d = ccw if at_end else (-ccw[0], -ccw[1])
        else:
          raise RuntimeError(f"arc_tangent {eid}: 'tangent' must be a line or arc")
        dl = math.hypot(*d)
        n = (-d[1] / dl, d[0] / dl)
        ch = (en.x - s.x, en.y - s.y)
        den = 2 * (ch[0] * n[0] + ch[1] * n[1])
        if abs(den) < 1e-12:
          raise RuntimeError(f"arc_tangent {eid}: end is straight ahead")
        tt = (ch[0] ** 2 + ch[1] ** 2) / den
        cx, cy = s.x + n[0] * tt, s.y + n[1] * tt
        r = abs(tt)
        a0 = math.atan2(s.y - cy, s.x - cx)
        a1 = math.atan2(en.y - cy, en.x - cx)
        if tt > 0:
          am = a0 + ((a1 - a0) % (2 * math.pi)) / 2
        else:
          am = a0 - ((a0 - a1) % (2 * math.pi)) / 2
        mid = adsk.core.Point3D.create(cx + r * math.cos(am), cy + r * math.sin(am), 0)  # sketch space, cm
        c = sk.sketchCurves.sketchArcs.addByThreePoints(sq, mid, eq)
        if not c:
          raise RuntimeError(f"addByThreePoints failed for {eid}")
        sk.geometricConstraints.addTangent(prev, c)
        keep(eid, c, **arc_ends(c, e["start"]))
      elif t == "arc_center_start_sweep":
        # Sketch-local sweep is counter-clockwise about the frame normal; Fusion's sweep is about
        # the sketch normal, which can point the other way on a face, so flip when it does.
        sweep = math.radians(e["sweep_deg"])
        sn = sk.transform.getAsCoordinateSystem()[-1]
        if sn.dotProduct(adsk.core.Vector3D.create(*nd)) < 0:
          sweep = -sweep
        c = sk.sketchCurves.sketchArcs.addByCenterStartSweep(pt(e["center"]), pt(e["start"]), sweep)
        keep(eid, c, **arc_ends(c, e["start"]))
      elif t == "ellipse":
        # Centre, the end of the major axis (sketch-local) and the minor radius, as Fox's ellipse;
        # or "point": a point the ellipse passes through, as Fusion's Ellipse tool's third pick.
        cx, cy = e["center"]["at"] if isinstance(e["center"], dict) else e["center"]
        mx, my = e["major"]
        ux, uy = mx - cx, my - cy
        n = math.hypot(ux, uy)
        if "point" in e:
          side = e["point"]
        else:
          side = [cx - uy / n * e["minor"], cy + ux / n * e["minor"]]
        c = sk.sketchCurves.sketchEllipses.add(pt(e["center"]), pt(e["major"]), pt(side))
        keep(eid, c, center=c.centerSketchPoint)
      elif t == "rectangle_three_point":
        # 3-Point Rectangle: "points": [a, b, c]; a-b is one side, c sets the height across it.
        # Its four lines are '<id>.l0'..'<id>.l3' (l0 runs from a) for profile loops.
        lines = sk.sketchCurves.sketchLines.addThreePointRectangle(*[pt(s) for s in e["points"]])
        if not lines or lines.count != 4:
          raise RuntimeError(f"addThreePointRectangle failed for {eid}")
        c = None
        for i in range(4):
          ln = lines.item(i)
          keep(f"{eid}.l{i}", ln, start=ln.startSketchPoint, end=ln.endSketchPoint)
          if e.get("construction"):
            ln.isConstruction = True
      elif t == "rectangle_center_point":
        # Center Rectangle: "center" and one "corner"; sides along the sketch axes. Its four
        # lines are '<id>.l0'..'<id>.l3' for profile loops (any construction diagonals are skipped).
        # The API takes Point3Ds only, so a referenced point (e.g. the origin) gives its position.
        def p3(spec):
          q = pt(spec)
          return q.geometry if hasattr(q, "geometry") else q
        got = sk.sketchCurves.sketchLines.addCenterPointRectangle(p3(e["center"]), p3(e["corner"]))
        lines = [got.item(i) for i in range(got.count)] if got else []
        lines = [ln for ln in lines if not ln.isConstruction]
        if len(lines) != 4:
          raise RuntimeError(f"addCenterPointRectangle failed for {eid}")
        c = None
        for i, ln in enumerate(lines):
          keep(f"{eid}.l{i}", ln, start=ln.startSketchPoint, end=ln.endSketchPoint)
          if e.get("construction"):
            ln.isConstruction = True
      elif t == "polygon_scribed":
        # Inscribed / Circumscribed Polygon: "center", "sides" and one "point" that is a corner
        # ("inscribed": true) or the midpoint of an edge (circumscribed, the default). Its lines
        # are '<id>.l0'..'<id>.l<n-1>' for profile loops. The angle and radius are measured in
        # Fusion's sketch space, so the sketch's own axes don't matter.
        cq, pq = pt(e["center"]), pt(e["point"])
        cg = cq.geometry if hasattr(cq, "geometry") else cq
        pg = pq.geometry if hasattr(pq, "geometry") else pq
        ang = math.atan2(pg.y - cg.y, pg.x - cg.x)
        got = sk.sketchCurves.sketchLines.addScribedPolygon(
            cq, int(e["sides"]), ang, cg.distanceTo(pg), bool(e.get("inscribed", False)))
        lines = [got[i] for i in range(len(got))] if got else []
        lines = [ln for ln in lines if not ln.isConstruction]
        if len(lines) != int(e["sides"]):
          raise RuntimeError(f"addScribedPolygon failed for {eid}")
        c = None
        for i, ln in enumerate(lines):
          keep(f"{eid}.l{i}", ln, start=ln.startSketchPoint, end=ln.endSketchPoint)
          if e.get("construction"):
            ln.isConstruction = True
      elif t == "polygon_edge":
        # Edge Polygon: one edge "start" -> "end" and "sides"; the polygon lies to the left of
        # the edge (looking from start to end) unless "right": true. Its lines are
        # '<id>.l0'..'<id>.l<n-1>' for profile loops.
        got = sk.sketchCurves.sketchLines.addEdgePolygon(
            pt(e["start"]), pt(e["end"]), bool(e.get("right", False)), int(e["sides"]))
        lines = [got[i] for i in range(len(got))] if got else []
        lines = [ln for ln in lines if not ln.isConstruction]
        if len(lines) != int(e["sides"]):
          raise RuntimeError(f"addEdgePolygon failed for {eid}")
        c = None
        for i, ln in enumerate(lines):
          keep(f"{eid}.l{i}", ln, start=ln.startSketchPoint, end=ln.endSketchPoint)
          if e.get("construction"):
            ln.isConstruction = True
      elif t == "arc_slot_center":
        # Center Point Arc Slot: the centre-line arc's centre, start and end ("center", "start",
        # "end") and the slot width. Its end caps are '<id>.e0'/'<id>.e1', its sides
        # '<id>.s0'/'<id>.s1' (for profile loops) and the construction centre-line '<id>'.
        got = sk.addCenterPointArcSlot(pt(e["center"]), pt(e["start"]), pt(e["end"]),
                                       adsk.core.ValueInput.createByReal(e["width"] * MM))
        if not got or len(got) < 5:
          raise RuntimeError(f"addCenterPointArcSlot failed for {eid}")
        for k, a in zip(["e0", "e1", "s0", "s1"], got[:4]):
          keep(f"{eid}.{k}", a, center=a.centerSketchPoint)
        c = got[4]
        keep(eid, c, center=c.centerSketchPoint)
      elif t == "arc_slot_three_point":
        # Three Point Arc Slot: the centre-line arc's "start" and "end" and a third point on it
        # ("mid", any point on the arc; the API wants a Point3D, so a referenced point gives its
        # position) and the slot width. Its end caps are '<id>.e0'/'<id>.e1', its sides
        # '<id>.s0'/'<id>.s1' (for profile loops) and the construction centre-line '<id>'.
        mq = pt(e["mid"])
        got = sk.addThreePointArcSlot(pt(e["start"]), pt(e["end"]),
                                      mq.geometry if hasattr(mq, "geometry") else mq,
                                      adsk.core.ValueInput.createByReal(e["width"] * MM))
        if not got or len(got) < 5:
          raise RuntimeError(f"addThreePointArcSlot failed for {eid}")
        for k, a in zip(["e0", "e1", "s0", "s1"], got[:4]):
          keep(f"{eid}.{k}", a, center=a.centerSketchPoint)
        c = got[4]
        keep(eid, c, center=c.centerSketchPoint)
      elif t == "slot_center_point":
        # Center Point Slot: the slot's centre ("center"), the centre of one end arc ("end"; the
        # other is mirrored through the centre) and the slot width. Its end arcs are
        # '<id>.e0'/'<id>.e1', its sides '<id>.s0'/'<id>.s1' (for profile loops) and the
        # construction centre-line '<id>'.
        got = sk.addCenterPointSlot(pt(e["center"]), pt(e["end"]),
                                    adsk.core.ValueInput.createByReal(e["width"] * MM))
        if not got or len(got) < 5:
          raise RuntimeError(f"addCenterPointSlot failed for {eid}")
        for k, a in zip(["e0", "e1"], got[:2]):
          keep(f"{eid}.{k}", a, center=a.centerSketchPoint)
        for k, ln in zip(["s0", "s1"], got[2:4]):
          keep(f"{eid}.{k}", ln, start=ln.startSketchPoint, end=ln.endSketchPoint)
        c = got[4]
        keep(eid, c, start=c.startSketchPoint, end=c.endSketchPoint)
      elif t == "slot_center_to_center":
        # Center to Center Slot: the centres of the two end arcs ("start", "end") and the slot
        # width. Optional "length" (mm) and "angle" (deg) override the end point, which then only
        # sets the direction. Its end arcs are '<id>.e0'/'<id>.e1', its sides '<id>.s0'/'<id>.s1'
        # (for profile loops) and the construction centre-line '<id>'.
        length = adsk.core.ValueInput.createByReal(e["length"] * MM) if "length" in e else None
        angle = (adsk.core.ValueInput.createByReal(math.radians(e["angle"]))
                 if "angle" in e else None)
        got = sk.addCenterToCenterSlot(pt(e["start"]), pt(e["end"]),
                                       adsk.core.ValueInput.createByReal(e["width"] * MM),
                                       False, length, angle)
        if not got or len(got) < 5:
          raise RuntimeError(f"addCenterToCenterSlot failed for {eid}")
        for k, a in zip(["e0", "e1"], got[:2]):
          keep(f"{eid}.{k}", a, center=a.centerSketchPoint)
        for k, ln in zip(["s0", "s1"], got[2:4]):
          keep(f"{eid}.{k}", ln, start=ln.startSketchPoint, end=ln.endSketchPoint)
        c = got[4]
        keep(eid, c, start=c.startSketchPoint, end=c.endSketchPoint)
      elif t == "slot_overall":
        # Overall Slot: the two extreme ends of the slot on its centre line ("start", "end") and
        # the slot width. Optional "length" (mm, overall) and "angle" (deg) override the end
        # point, which then only sets the direction. Its end arcs are '<id>.e0'/'<id>.e1', its
        # sides '<id>.s0'/'<id>.s1' (for profile loops) and the construction centre-line '<id>'.
        length = adsk.core.ValueInput.createByReal(e["length"] * MM) if "length" in e else None
        angle = (adsk.core.ValueInput.createByReal(math.radians(e["angle"]))
                 if "angle" in e else None)
        got = sk.addOverallSlot(pt(e["start"]), pt(e["end"]),
                                adsk.core.ValueInput.createByReal(e["width"] * MM),
                                False, length, angle)
        if not got or len(got) < 5:
          raise RuntimeError(f"addOverallSlot failed for {eid}")
        for k, a in zip(["e0", "e1"], got[:2]):
          keep(f"{eid}.{k}", a, center=a.centerSketchPoint)
        for k, ln in zip(["s0", "s1"], got[2:4]):
          keep(f"{eid}.{k}", ln, start=ln.startSketchPoint, end=ln.endSketchPoint)
        c = got[4]
        keep(eid, c, start=c.startSketchPoint, end=c.endSketchPoint)
      elif t == "point":
        c = sk.sketchPoints.add(pt(e["at"]))
        rec["entities"][eid] = c
        rec["points"][eid] = c
      elif t == "circular_pattern":
        # Sketch Circular Pattern: copies of earlier curves ("curves": [id]) about "center" (a
        # point ref or "at"), "count" instances in all over "angle_deg" (default 360), optionally
        # "symmetric" about the originals. The copies are '<id>.0', '<id>.1', ... in Fusion's order.
        ctr = pt(e["center"])
        if not hasattr(ctr, "geometry"):
          ctr = sk.sketchPoints.add(ctr)
        gc = sk.geometricConstraints
        inp = gc.createCircularPatternInput([rec["entities"][r] for r in e["curves"]], ctr)
        inp.quantity = V(e["count"], "")
        inp.totalAngle = V(e.get("angle_deg", 360), "deg")
        inp.isSymmetric = bool(e.get("symmetric", False))
        c = gc.addCircularPattern(inp)
        if not c:
          raise RuntimeError(f"addCircularPattern failed for {eid}")
        made = list(c.createdEntities)
        made = [m for m in made if not m.objectType.endswith("SketchPoint")]
        for i, m in enumerate(made):
          rec["entities"][f"{eid}.{i}"] = m
        c = None
      elif t == "rectangular_pattern":
        # Sketch Rectangular Pattern: copies of earlier curves ("curves": [id]) in rows and
        # columns. "count"/"distance" (mm) along "direction" (an earlier sketch line, default the
        # sketch X axis) and optionally "count2"/"distance2" along "direction2" (default 90 deg to
        # the first). "distance_type": "spacing" (default; between instances) or "extent" (the
        # whole pattern); "symmetric"/"symmetric2" spread it both ways about the originals. The
        # copies are '<id>.0', '<id>.1', ... in Fusion's order.
        gc = sk.geometricConstraints
        dt = {"spacing": adsk.fusion.PatternDistanceType.SpacingPatternDistanceType,
              "extent": adsk.fusion.PatternDistanceType.ExtentPatternDistanceType
              }.get(e.get("distance_type", "spacing"))
        if dt is None:
          raise Unsupported(f"rectangular_pattern distance_type {e.get('distance_type')}")
        inp = gc.createRectangularPatternInput([rec["entities"][r] for r in e["curves"]], dt)
        d1 = rec["entities"][e["direction"]] if e.get("direction") else None
        d2 = rec["entities"][e["direction2"]] if e.get("direction2") else None
        # setDirectionOne/Two reject a null line, so the default directions set the properties.
        if d1:
          inp.setDirectionOne(d1, V(e["count"], ""), V(e["distance"]))
        else:
          inp.quantityOne, inp.distanceOne = V(e["count"], ""), V(e["distance"])
        if d2:
          inp.setDirectionTwo(d2, V(e.get("count2", 1), ""), V(e.get("distance2", 0)))
        else:
          inp.quantityTwo, inp.distanceTwo = V(e.get("count2", 1), ""), V(e.get("distance2", 0))
        inp.isSymmetricInDirectionOne = bool(e.get("symmetric", False))
        inp.isSymmetricInDirectionTwo = bool(e.get("symmetric2", False))
        c = gc.addRectangularPattern(inp)
        if not c:
          raise RuntimeError(f"addRectangularPattern failed for {eid}")
        made = [m for m in c.createdEntities if not m.objectType.endswith("SketchPoint")]
        for i, m in enumerate(made):
          rec["entities"][f"{eid}.{i}"] = m
        c = None
      elif t == "mirror":
        # Sketch Mirror: copies of earlier curves ("curves": [id]) mirrored about an earlier sketch
        # line ("line": id), tied to the originals by symmetry constraints (Sketch.mirror, as the
        # interactive Mirror command). The copies are '<id>.0', '<id>.1', ... in input order: the
        # first entries Fusion returns (its new points and spline handle lines follow them).
        got = sk.mirror([rec["entities"][r] for r in e["curves"]], rec["entities"][e["line"]])
        made = list(got or [])[:len(e["curves"])]
        if len(made) != len(e["curves"]) or any(m.objectType.endswith("SketchPoint") for m in made):
          raise RuntimeError(f"Sketch.mirror failed for {eid}")
        for i, m in enumerate(made):
          rec["entities"][f"{eid}.{i}"] = m
        c = None
      elif t == "offset":
        # Sketch Offset: copies of a chain of earlier connected curves ("curves": [id], in chain
        # order) "distance" (mm) away, tied to them by an offset constraint, as MODIFY > Offset
        # does. Positive is outward for a closed chain and to the left of travel for an open one
        # (travel runs from the first curve to the last). "both_sides" also makes the copy on the
        # other side (addTwoSidesOffset, linked); "topology_matched" (default true, the API's
        # default) makes Fusion refuse an offset that drops or adds curves. The copies are
        # '<id>.0', '<id>.1', ... in Fusion's order (for both sides: the first side, then the
        # other).
        src = [rec["entities"][r] for r in e["curves"]]
        d = float(e["distance"])  # mm; V() converts
        closed_chain = len(src) == 1 and not hasattr(src[0], "startSketchPoint")
        if not closed_chain:
          ends = [(c.startSketchPoint.geometry, c.endSketchPoint.geometry) for c in src
                  if hasattr(c, "startSketchPoint")]
          pts = [p for ab in ends for p in ab]
          closed_chain = all(sum(1 for q in pts if q.distanceTo(p) < 1e-7) % 2 == 0 for p in pts)

        def bbox_diag(cs):
          b = None
          for cv in cs:
            cb = cv.boundingBox
            if b is None:
              b = adsk.core.BoundingBox3D.create(cb.minPoint, cb.maxPoint)
            else:
              b.combine(cb)
          return b.minPoint.distanceTo(b.maxPoint)

        def make(v):
          inp = sk.geometricConstraints.createOffsetInput(src, V(v))
          if inp is None:
            raise RuntimeError(f"createOffsetInput failed for {eid}")
          inp.isTopologyMatched = bool(e.get("topology_matched", True))
          if e.get("both_sides"):
            ocs = list(sk.geometricConstraints.addTwoSidesOffset(inp, True) or [])
          else:
            oc = sk.geometricConstraints.addOffset2(inp)
            ocs = [oc] if oc else []
          if not ocs:
            raise RuntimeError(f"offset failed for {eid}")
          made = [cv for oc in ocs for cv in oc.childCurves]
          return ocs, made

        # Fusion's positive offset is to the right of travel (outward for a lone closed curve).
        ocs, made = make(-d if not (len(src) == 1 and closed_chain) else d)
        if closed_chain and not e.get("both_sides"):
          grew = bbox_diag(made) > bbox_diag(src)
          if grew != (d > 0):
            for cv in made:
              cv.deleteMe()
            for oc in ocs:
              if oc.isValid:
                oc.deleteMe()
            ocs, made = make(d if not len(src) == 1 else -d)
        for i, m in enumerate(made):
          keep(f"{eid}.{i}", m, **({"start": m.startSketchPoint, "end": m.endSketchPoint}
                                   if hasattr(m, "startSketchPoint") else {}))
        c = None
      elif t == "conic":
        # Conic Curve: "start" and "end" (point refs or "at"), the "apex" where the end tangents
        # meet, and "rho" (0..1; 0.5 is a parabola, below elliptical, above hyperbolic).
        def p3(spec):
          q = pt(spec)
          return q.geometry if hasattr(q, "geometry") else q
        a, b = pt(e["start"]), pt(e["end"])
        c = sk.sketchCurves.sketchConicCurves.add(a, b, p3(e["apex"]), float(e["rho"]))
        if not c:
          raise RuntimeError(f"sketchConicCurves.add failed for {eid}")
        keep(eid, c, start=c.startSketchPoint, end=c.endSketchPoint)
      elif t == "fitted_spline":
        pts = collection([sk.modelToSketchSpace(P(world(p))) for p in e["fit_points"]])
        c = sk.sketchCurves.sketchFittedSplines.add(pts)
        if not c:
          raise RuntimeError(f"sketchFittedSplines.add failed for {eid}")
        # "closed": true makes a periodic spline back through the first fit point (Fusion's
        # Fit Point Spline closed by clicking its start).
        if e.get("closed"):
          c.isClosed = True
          if not c.isClosed:
            raise RuntimeError(f"could not close fitted spline {eid}")
        # '<id>.fit<i>' names each fit point (fit0 is the start), so constraints can reach them.
        fits = {f"fit{i}": c.fitPoints.item(i) for i in range(c.fitPoints.count)}
        keep(eid, c, start=c.startSketchPoint, end=c.endSketchPoint, **fits)
      elif t == "control_spline":
        # Control Point Spline: "control_points" (the first and last are on the curve) and
        # "degree" (3, the default, or 5: the only degrees Fusion creates). '<id>.cp<i>' names
        # each control point.
        pts = [sk.modelToSketchSpace(P(world(p))) for p in e["control_points"]]
        deg = {3: adsk.fusion.SplineDegrees.SplineDegreeThree,
               5: adsk.fusion.SplineDegrees.SplineDegreeFive}.get(int(e.get("degree", 3)))
        if deg is None:
          raise Unsupported(f"control_spline degree {e.get('degree')}")
        c = sk.sketchCurves.sketchControlPointSplines.add(pts, deg)
        if not c:
          raise RuntimeError(f"sketchControlPointSplines.add failed for {eid}")
        cps = {f"cp{i}": c.controlPoints[i] for i in range(len(c.controlPoints))}
        keep(eid, c, start=c.startSketchPoint, end=c.endSketchPoint, **cps)
      elif t == "blend":
        # Blend Curve: a spline joining an end of each of two earlier open curves ("curves":
        # [id, id]); "near": [[x, y], [x, y]] picks each curve's end (the end nearest the point).
        # "continuity": "g1" (tangent, the API's default) or "g2" (curvature).
        a, b = [rec["entities"][r] for r in e["curves"]]
        na, nb = [sk.modelToSketchSpace(P(world(p))) for p in e["near"]]
        cont = e.get("continuity", "g1").lower()
        if cont not in ("g1", "g2"):
          raise Unsupported(f"blend continuity {cont}")
        c = sk.sketchCurves.sketchFittedSplines.addBlendCurve(a, na, b, nb, cont == "g2")
        if not c:
          raise RuntimeError(f"addBlendCurve failed for {eid}")
        keep(eid, c, start=c.startSketchPoint, end=c.endSketchPoint)
      elif t in ("chamfer_distance_angle", "chamfer_distance"):
        # Sketch Chamfer between two earlier lines ("lines": [first, second]) at their shared
        # vertex, crossing or apparent intersection: "distance" from the intersection along the
        # first line and "angle_deg" from the first line (Distance and Angle), or "distance" and
        # "distance2" along the second line (Distance). "keep" ([spec, spec], optional) are points
        # on the side of each line to keep; by default each line's end farther from the other line.
        a, b = [adsk.fusion.SketchLine.cast(rec["entities"][r]) for r in e["lines"]]
        if not a or not b:
          raise RuntimeError(f"{t} {eid} needs two sketch lines")

        def far_end(ln, other):
          ends = [ln.startSketchPoint.geometry, ln.endSketchPoint.geometry]
          ref = [other.startSketchPoint.geometry, other.endSketchPoint.geometry]
          return max(ends, key=lambda q: min(q.distanceTo(r) for r in ref))

        if "keep" in e:
          ka, kb = [pt(s) for s in e["keep"]]
          ka, kb = [k.geometry if hasattr(k, "geometry") else k for k in (ka, kb)]
        else:
          ka, kb = far_end(a, b), far_end(b, a)
        lines = sk.sketchCurves.sketchLines
        if t == "chamfer_distance_angle":
          c = lines.addAngleChamfer(a, ka, b, kb, e["distance"] * MM, math.radians(e["angle_deg"]))
        else:
          c = lines.addDistanceChamfer(a, ka, b, kb, e["distance"] * MM, e["distance2"] * MM)
        if not c:
          raise RuntimeError(f"{t} failed for {eid}")
        keep(eid, c, start=c.startSketchPoint, end=c.endSketchPoint)
      elif t == "fillet":
        # Sketch Fillet: an arc of "radius" tangent to two earlier lines or arcs ("curves":
        # [first, second]) at their shared vertex or crossing, trimming both, as MODIFY > Fillet
        # does. "near" ([spec, spec], optional) are the pick points on each curve, which choose the
        # corner to round; by default each curve's midpoint.
        a, b = [rec["entities"][r] for r in e["curves"]]

        def mid(cv):
          ev = cv.geometry.evaluator
          ok, t0, t1 = ev.getParameterExtents()
          ok, q = ev.getPointAtParameter((t0 + t1) / 2)
          return q

        if "near" in e:
          na, nb = [pt(s) for s in e["near"]]
          na, nb = [k.geometry if hasattr(k, "geometry") else k for k in (na, nb)]
        else:
          na, nb = mid(a), mid(b)
        c = sk.sketchCurves.sketchArcs.addFillet(a, na, b, nb, e["radius"] * MM)
        if not c:
          raise RuntimeError(f"addFillet failed for {eid}")
        keep(eid, c, start=c.startSketchPoint, end=c.endSketchPoint, center=c.centerSketchPoint)
      elif t == "break":
        # Break: splits an earlier curve ("curve": id) at its crossings with the other curves
        # nearest "at" (a point on the piece to split off), as the MODIFY > Break tool does.
        # "pieces" names the resulting curves by a point on each ({name: [x, y]}, nearest piece);
        # "delete" lists piece names to delete afterwards (so the break shows in the profiles).
        def piece_distance(m, q):
          ev = m.geometry.evaluator
          ok, t0, t1 = ev.getParameterExtents()
          ok, ps = ev.getPointsAtParameters([t0 + (t1 - t0) * i / 40 for i in range(41)])
          return min(p.distanceTo(q) for p in ps)

        src = rec["entities"][e["curve"]]
        got = src.breakCurve(pt(e["at"]), True)
        made = [got.item(i) for i in range(got.count)] if got else []
        if not made:
          raise RuntimeError(f"breakCurve found no crossings for {eid or e['curve']}")
        if not src.isValid:
          # Fusion replaces some curves (a circle, a line broken in three) with new pieces.
          rec["entities"].pop(e["curve"])
        for name, where in e.get("pieces", {}).items():
          q = pt(where)
          q = q.geometry if hasattr(q, "geometry") else q
          best = min(made, key=lambda m: piece_distance(m, q))
          rec["entities"][name] = best
        for name in e.get("delete", []):
          rec["entities"].pop(name).deleteMe()
        continue
      elif t == "trim":
        # Trim: removes the piece of an earlier curve ("curve": id) around "at" between its
        # nearest crossings with other curves (the whole curve if nothing crosses it), as the
        # MODIFY > Trim tool does. The curve keeps its id when Fusion shortens it in place or
        # hands back one piece; "pieces" names what's left by a point on each ({name: [x, y]},
        # nearest piece) when the trim splits it in two.
        def trim_distance(m, q):
          ev = m.geometry.evaluator
          ok, t0, t1 = ev.getParameterExtents()
          ok, ps = ev.getPointsAtParameters([t0 + (t1 - t0) * i / 40 for i in range(41)])
          return min(p.distanceTo(q) for p in ps)

        src = rec["entities"][e["curve"]]
        got = src.trim(pt(e["at"]), True)
        made = [got.item(i) for i in range(got.count)] if got else []
        left = [m for m in made if m.isValid] + ([src] if src.isValid else [])
        if not src.isValid:
          rec["entities"].pop(e["curve"])
          if len(left) == 1 and "pieces" not in e:
            rec["entities"][e["curve"]] = left[0]
        for name, where in e.get("pieces", {}).items():
          q = pt(where)
          q = q.geometry if hasattr(q, "geometry") else q
          if not left:
            raise RuntimeError(f"trim left nothing of {e['curve']}")
          rec["entities"][name] = min(left, key=lambda m: trim_distance(m, q))
        continue
      elif t == "extend":
        # Extend: lengthens an earlier open curve ("curve": id) from its end nearest "at" to the
        # nearest curve it reaches, as the MODIFY > Extend tool does. The curve keeps its id
        # (Fusion may hand back a new curve in its place).
        # (Lines and arcs are lengthened in place and the returned collection is empty.)
        src = rec["entities"][e["curve"]]
        before = src.length
        got = src.extend(pt(e["at"]), True)
        made = [got.item(i) for i in range(got.count)] if got else []
        if not src.isValid:
          if not made:
            raise RuntimeError(f"extend lost curve {e['curve']}")
          rec["entities"][e["curve"]] = made[0]
        elif src.length <= before + 1e-9:
          raise RuntimeError(f"extend found nothing to reach for {e['curve']}")
        continue
      else:
        raise Unsupported(f"sketch entity {t}")
      if e.get("construction") and hasattr(c, "isConstruction"):
        c.isConstruction = True
    self.sketches[sk.name] = rec
    if f.get("constraints") or f.get("dimensions"):
      self.constrain(sk, rec, f, pt, origin_point)
    return sk

  def constrain(self, sk, rec, f, pt, origin_point):
    """Applies the recipe's constraints and driving dimensions; the entity positions are only the
    starting guess for Fusion's solver (constrained recipes rely on this)."""
    def ref(r):
      if r == "origin":
        return origin_point()
      if r in rec["points"]:
        return rec["points"][r]
      return rec["entities"][r]

    gc = sk.geometricConstraints
    two = {"coincident": gc.addCoincident, "tangent": gc.addTangent, "equal": gc.addEqual,
           "concentric": gc.addConcentric, "perpendicular": gc.addPerpendicular,
           "parallel": gc.addParallel, "collinear": gc.addCollinear,
           "horizontal_points": gc.addHorizontalPoints, "vertical_points": gc.addVerticalPoints,
           "midpoint": gc.addMidPoint, "smooth": getattr(gc, "addSmooth", None)}
    for c in f.get("constraints", []):
      t, refs = c["type"], [ref(r) for r in c["refs"]]
      try:
        if t == "horizontal":
          gc.addHorizontal(refs[0])
        elif t == "vertical":
          gc.addVertical(refs[0])
        elif t == "fix":
          refs[0].isFixed = True
        elif t == "symmetric":
          gc.addSymmetry(refs[0], refs[1], refs[2])
        elif t == "polygon":
          # The lines must already form a regular polygon; the constraint keeps them one.
          # '<id>.center' names the centre point Fusion adds, so later refs can reach it.
          pc = gc.addPolygon(refs)
          if not pc:
            raise RuntimeError("addPolygon returned null")
          if c.get("id"):
            rec["points"][f"{c['id']}.center"] = pc.centerPoint
        elif t in two and two[t]:
          a, b = refs
          is_point = lambda e: e.objectType.endswith("SketchPoint")
          if t in ("coincident", "midpoint") and not is_point(a) and is_point(b):
            a, b = b, a  # Fusion wants the point first
          is_line = lambda e: e.objectType.endswith("SketchLine")
          if t == "perpendicular" and not (is_line(a) and is_line(b)):
            # a line perpendicular to a circle or arc (runs through its centre): line first
            if not is_line(a):
              a, b = b, a
            gc.addPerpendicular2(a, b)
          else:
            two[t](a, b)
        else:
          raise Unsupported(f"constraint {t}")
      except Unsupported:
        raise
      except Exception as e:
        raise RuntimeError(f"constraint {c}: {e}")

    dims = sk.sketchDimensions
    orient = adsk.fusion.DimensionOrientations

    def text_point(c, refs):
      if c.get("text_at"):
        return pt(c["text_at"])
      g = refs[0].geometry if refs[0].objectType.endswith("SketchPoint") else refs[0].boundingBox.maxPoint
      return adsk.core.Point3D.create(g.x + 0.5, g.y + 0.5, 0)

    for c in f.get("dimensions", []):
      t, refs = c["type"], [ref(r) for r in c["refs"]]
      tp = text_point(c, refs)
      try:
        if t == "distance":
          o = {"horizontal": orient.HorizontalDimensionOrientation, "vertical": orient.VerticalDimensionOrientation
               }.get(c.get("orientation"), orient.AlignedDimensionOrientation)
          if all(r.objectType.endswith("SketchPoint") for r in refs):
            d = dims.addDistanceDimension(refs[0], refs[1], o, tp)
          else:
            d = dims.addOffsetDimension(refs[0], refs[1], tp)
        elif t == "offset":
          d = dims.addOffsetDimension(refs[0], refs[1], tp)
        elif t == "radius":
          d = dims.addRadialDimension(refs[0], tp)
        elif t == "diameter":
          d = dims.addDiameterDimension(refs[0], tp)
        elif t == "angle":
          d = dims.addAngularDimension(refs[0], refs[1], tp)
        else:
          raise Unsupported(f"dimension {t}")
        v = c["value"]
        d.parameter.expression = v if isinstance(v, str) else f"{v} {c.get('unit', 'mm')}"
      except Unsupported:
        raise
      except Exception as e:
        raise RuntimeError(f"dimension {c}: {e}")

  def profiles(self, specs):
    """Fusion profiles matching recipe loops: [{sketch, loops: [{outer, curves}]}]."""
    found = []
    for spec in specs if isinstance(specs, list) else [specs]:
      rec = self.sketches[spec["sketch"]]
      wanted_outer = [set(l["curves"]) for l in spec["loops"] if l.get("outer", True)]
      wanted_inner = [set(l["curves"]) for l in spec["loops"] if not l.get("outer", True)]
      names = {id(c): k for k, c in rec["entities"].items()}
      by_token = {getattr(c, "entityToken", None): k for k, c in rec["entities"].items()}

      def curve_ids(loop):
        out = set()
        for j in range(loop.profileCurves.count):
          ent = loop.profileCurves.item(j).sketchEntity
          out.add(by_token.get(ent.entityToken, names.get(id(ent), ent.entityToken)))
        return out

      sk = rec["sketch"]
      loops = []  # (profile, outer curve ids, [inner curve ids])
      for i in range(sk.profiles.count):
        prof = sk.profiles.item(i)
        outer, inner = set(), []
        for j in range(prof.profileLoops.count):
          loop = prof.profileLoops.item(j)
          if loop.isOuter:
            outer = curve_ids(loop)
          else:
            inner.append(curve_ids(loop))
        loops.append((prof, outer, inner))

      # A profile's curves are the sketch curves (Fusion doesn't split them, it only bounds
      # regions with pieces of them), so a recipe region is the profile whose outer loop lies on
      # a wanted outer loop and whose holes are exactly wanted inner loops.
      matched = [p for p, o, inn in loops
                 if o and any(o <= w for w in wanted_outer) and all(l in wanted_inner for l in inn)]
      if not matched and not wanted_inner:
        # The recipe wants a whole outer region but the sketch also has closed curves inside it:
        # take the region with holes plus the regions that fill those holes.
        outers = [(p, inn) for p, o, inn in loops if o and any(o <= w for w in wanted_outer)]
        holes = [h for _, inn in outers for h in inn]
        matched = [p for p, _ in outers] + [p for p, o, _ in loops if o in holes]
      found.extend(matched)
    if not found:
      raise RuntimeError(f"no Fusion profile matches {specs}")
    return found

  def profile_input(self, specs):
    profs = self.profiles(specs)
    return profs[0] if len(profs) == 1 else collection(profs)

  def curves(self, spec):
    rec = self.sketches[spec["sketch"]]
    return [rec["entities"][c] for c in spec["curves"]]

  # ---- features -------------------------------------------------------------------------------

  def name_bodies(self, feature, names):
    if not names:
      return
    for i in range(min(feature.bodies.count, len(names))):
      b = feature.bodies.item(i)
      b.name = names[i]
      self.bodies[names[i]] = b

  def op_extrude(self, f):
    op = OPERATIONS[f.get("operation", "new")]
    ext = self.root.features.extrudeFeatures
    if "thin" in f and "curves" in f["thin"]:
      th = f["thin"]
      prof = self.root.createOpenProfile(collection(self.curves(th)), False)
    else:
      prof = self.profile_input(f["profiles"])
    inp = ext.createInput(prof, op)
    e = f["extent"]
    t = e["type"]
    direction = (adsk.fusion.ExtentDirections.NegativeExtentDirection if e.get("direction") == "negative"
                 else adsk.fusion.ExtentDirections.PositiveExtentDirection)
    taper = V(e.get("taper_deg", 0), "deg")
    if t == "distance":
      inp.setOneSideExtent(adsk.fusion.DistanceExtentDefinition.create(V(e["distance"])), direction, taper)
    elif t == "through_all":
      inp.setOneSideExtent(adsk.fusion.ThroughAllExtentDefinition.create(), direction, taper)
    elif t == "symmetric":
      inp.setSymmetricExtent(V(e["distance"]), bool(e.get("is_full_length", False)), taper)
    elif t == "two_sides":
      inp.setTwoSidesExtent(adsk.fusion.DistanceExtentDefinition.create(V(e["side1"])),
                            adsk.fusion.DistanceExtentDefinition.create(V(e["side2_opposite"])),
                            V(e.get("taper_deg", 0), "deg"), V(e.get("taper2_deg", e.get("taper_deg", 0)), "deg"))
    elif t == "to_face":
      inp.setOneSideExtent(adsk.fusion.ToEntityExtentDefinition.create(self.faces([e["face"]])[0], False),
                           direction, taper)
    else:
      raise Unsupported(f"extrude extent {t}")
    if e.get("start_offset"):
      inp.startExtent = adsk.fusion.OffsetStartDefinition.create(V(e["start_offset"]))
    if "thin" in f:
      th = f["thin"]
      loc = adsk.fusion.ThinExtrudeWallLocation
      side = th.get("side", "side1")
      if side == "two_sides":
        if th.get("thickness2", th["thickness"]) != th["thickness"]:
          raise Unsupported("thin wall with different thicknesses each side (Fusion: side 1, side 2 or centred)")
        inp.setThinExtrude(loc.Center, V(2 * th["thickness"]))
      else:
        where = {"side1": loc.Side1, "side2": loc.Side2, "center": loc.Center}[side]
        inp.setThinExtrude(where, V(th["thickness"]))
    feat = ext.add(inp)
    self.name_bodies(feat, f.get("result_bodies"))
    return feat

  def op_revolve(self, f):
    inp = self.root.features.revolveFeatures.createInput(
      self.profile_input(f["profile"]), self.axis(f["axis"]), OPERATIONS[f.get("operation", "new")])
    if "angle2" in f:
      # two sides: 'angle' one way round the axis, 'angle2' the other way
      inp.setTwoSideAngleExtent(V(f.get("angle", 360), "deg"), V(f["angle2"], "deg"))
    else:
      inp.setAngleExtent(bool(f.get("symmetric", False)), V(f.get("angle", 360), "deg"))
    feat = self.root.features.revolveFeatures.add(inp)
    self.name_bodies(feat, f.get("result_bodies"))
    return feat

  def op_fillet(self, f):
    fillets = self.root.features.filletFeatures
    inp = fillets.createInput()
    if "sets" in f:
      # several edge sets in one feature, each with its own radius (and type)
      for s in f["sets"]:
        self.fillet_set(inp, dict(s, tangent_chain=s.get("tangent_chain", f.get("tangent_chain", False))))
    else:
      self.fillet_set(inp, f)
    if "rolling_ball_corner" in f:
      inp.isRollingBallCorner = bool(f["rolling_ball_corner"])
    if f.get("g2"):
      inp.isG2 = True
    return fillets.add(inp)

  def fillet_set(self, inp, f):
    # 'edges' and/or 'faces' (a face fillets all its edges)
    edges = collection(self.edges(f.get("edges", [])) + self.faces(f.get("faces", [])))
    chain = bool(f.get("tangent_chain", False))
    t = f.get("type", "constant_radius")
    if f.get("fit") and f["fit"] != "fixed":
      # Fusion has no fit option: building the plain fillet instead would measure a different part.
      raise Unsupported(f"fillet fit: {f['fit']} (Fusion has no taper-to-fit fillet)")
    sets = getattr(inp, "edgeSetInputs", None)
    if t == "asymmetric":
      if sets is None:
        raise Unsupported("asymmetric fillet (needs edgeSetInputs)")
      s = sets.addAsymmetricRadiusEdgeSet(edges, V(f["distance"]), V(f["distance2"]), chain)
      if f.get("flipped"):
        s.isFlipped = True
      return
    if t == "constant_radius":
      if sets is not None:
        sets.addConstantRadiusEdgeSet(edges, V(f["radius"]), chain)
      else:
        inp.addConstantRadiusEdgeSet(edges, V(f["radius"]), chain)
    elif t == "variable_radius":
      if sets is not None:
        sets.addVariableRadiusEdgeSet(edges, V(f["radius"]), V(f["radius2"]), chain)
      else:
        inp.addVariableRadiusEdgeSet(edges, V(f["radius"]), V(f["radius2"]), chain)
    elif t == "chord_length" and sets is not None:
      sets.addChordLengthEdgeSet(edges, V(f["radius"]), chain)
    else:
      raise Unsupported(f"fillet type {t}")

  def op_full_round_fillet(self, f):
    # round the centre face over into the two side faces with one tangent fillet
    fillets = self.root.features.filletFeatures
    if not hasattr(fillets, "createFullRoundFilletInput"):
      raise Unsupported("full round fillet (needs createFullRoundFilletInput)")
    inp = fillets.createFullRoundFilletInput()
    for s in f["face_sets"]:
      centre = self.faces([s["center"]])[0]
      inp.faceSets.add(centre, self.faces(s["side_one"]), self.faces(s["side_two"]), True)
    return fillets.addFullRoundFillet(inp)

  def op_chamfer(self, f):
    chamfers = self.root.features.chamferFeatures
    edges = collection(self.edges(f["edges"]))
    chain = bool(f.get("tangent_chain", False))
    t = f.get("type", "equal_distance")
    flipped = bool(f.get("flipped", False))
    if t in ("two_distance", "distance_angle"):
      if not hasattr(chamfers, "createInput2"):
        raise Unsupported(f"chamfer type {t} (needs createInput2)")
      inp = chamfers.createInput2()
      sets = inp.chamferEdgeSets
      if t == "two_distance":
        sets.addTwoDistancesChamferEdgeSet(edges, V(f["distance"]), V(f["distance2"]), flipped, chain)
      else:
        sets.addDistanceAndAngleChamferEdgeSet(edges, V(f["distance"]), V(f["angle"], "deg"), flipped, chain)
      return chamfers.add(inp)
    if t != "equal_distance":
      raise Unsupported(f"chamfer type {t}")
    if hasattr(chamfers, "createInput2"):
      inp = chamfers.createInput2()
      inp.chamferEdgeSets.addEqualDistanceChamferEdgeSet(edges, V(f["distance"]), chain)
    else:
      inp = chamfers.createInput(edges, chain)
      inp.setToEqualDistance(V(f["distance"]))
    return chamfers.add(inp)

  def op_shell(self, f):
    shells = self.root.features.shellFeatures
    inp = shells.createInput(collection(self.faces(f["faces_removed"])), bool(f.get("tangent_chain", False)))
    if f.get("inside_thickness"):
      inp.insideThickness = V(f["inside_thickness"])
    if f.get("outside_thickness"):
      inp.outsideThickness = V(f["outside_thickness"])
    return shells.add(inp)

  def op_emboss(self, f):
    # Fusion's emboss wraps the profiles onto the faces; positive depth raises, negative (or
    # "deboss": true) recesses. "horizontal"/"vertical" offset and "rotation_deg" move the
    # profiles on the faces.
    embosses = self.root.features.embossFeatures
    depth = float(f["depth"])
    if f.get("deboss"):
      depth = -abs(depth)
    inp = embosses.createInput(self.profiles(f["profiles"]), self.faces(f["faces"]), V(depth))
    inp.isTangentChain = bool(f.get("tangent_chain", True))
    if f.get("horizontal"):
      inp.horizontalDistance = V(f["horizontal"])
    if f.get("vertical"):
      inp.verticalDistance = V(f["vertical"])
    if f.get("rotation_deg"):
      inp.rotationAngle = V(f["rotation_deg"], "deg")
    return embosses.add(inp)

  def op_combine(self, f):
    comb = self.root.features.combineFeatures
    inp = comb.createInput(self.body(f["target_body"]), collection([self.body(b) for b in f["tool_bodies"]]))
    inp.operation = OPERATIONS[f.get("operation", "join")]
    inp.isKeepToolBodies = bool(f.get("keep_tools", False))
    return comb.add(inp)

  def op_construction_plane(self, f):
    planes = self.root.constructionPlanes
    inp = planes.createInput()
    m = f["method"]
    if m == "offset":
      inp.setByOffset(self.plane(f["base"]), V(f["offset"]))
    elif m == "at_angle":
      # "axis": a world axis name, {"ref"}, a sketch line {"sketch", "curve"}, or a linear body
      # edge {"point"}; "base_plane": a plane name or a planar face {"face": {"point"}}.
      a = f["axis"]
      ax = self.find(a["point"], "edge") if isinstance(a, dict) and "point" in a else self.axis(a)
      if not inp.setByAngle(ax, V(f["angle"], "deg"), self.plane(f["base_plane"])):
        raise RuntimeError("setByAngle failed")
    elif m == "midplane":
      inp.setByTwoPlanes(self.plane(f["a"]), self.plane(f["b"]))
    elif m == "perpendicular":
      # Perpendicular to a plane or face; optionally through a point (vertex/sketch point) or a
      # plane, shifted by distance, aligned with the face's U (default) or V direction.
      ref = f.get("reference")
      if ref is None:
        ref_ent = None
      elif isinstance(ref, dict) and ("vertex" in ref or "sketch" in ref):
        ref_ent = self.point_entity(ref)
      else:
        ref_ent = self.plane(ref)
      dist = V(f["distance"]) if f.get("distance") is not None else None
      if not inp.setByPerpendicularToPlane(self.plane(f["base"]), dist, ref_ent,
                                           bool(f.get("u_direction", True))):
        raise RuntimeError("setByPerpendicularToPlane failed")
    elif m == "three_points":
      # Through three points: each {"vertex": [x,y,z]} or {"sketch", "point"}.
      a, b, c = (self.point_entity(f[k]) for k in ("a", "b", "c"))
      if not inp.setByThreePoints(a, b, c):
        raise RuntimeError("setByThreePoints failed")
    elif m == "two_edges":
      # Plane Through Two Edges: "a" and "b" are each a linear body edge {"point"}, or an axis /
      # sketch line as axis() takes them (world axis name, {"ref"}, {"sketch", "curve"}).
      def lin(e):
        return self.find(e["point"], "edge") if isinstance(e, dict) and "point" in e else self.axis(e)
      if not inp.setByTwoEdges(lin(f["a"]), lin(f["b"])):
        raise RuntimeError("setByTwoEdges failed")
    elif m == "tangent":
      # Tangent Plane: tangent to "face" ({"point", "type"?, "radius"?}). With "point" (a point
      # entity as point_entity takes it) the plane touches the face nearest that point
      # (setByTangentAtPoint); otherwise it is rotated "angle" deg (default 0) about the face
      # from "reference" (a plane name or planar face; default XY) (setByTangent).
      face = self.find(f["face"]["point"], "face", f["face"])
      if f.get("point") is not None:
        if not inp.setByTangentAtPoint(face, self.point_entity(f["point"])):
          raise RuntimeError("setByTangentAtPoint failed")
      else:
        ref = self.plane(f.get("reference", "XY"))
        if not inp.setByTangent(face, V(f.get("angle", 0), "deg"), ref):
          raise RuntimeError("setByTangent failed")
    elif m == "along_path":
      # Plane Along Path: normal to "path" (a body edge {"point"} or a sketch curve {"sketch",
      # "curve"}) at fraction "t" (0..1, default 0.5) of its length from its start.
      e = f["path"]
      if "point" in e:
        ent = self.find(e["point"], "edge")
      else:
        ent = self.sketches[e["sketch"]]["entities"][e["curve"]]
      if not inp.setByDistanceOnPath(ent, adsk.core.ValueInput.createByReal(float(f.get("t", 0.5)))):
        raise RuntimeError("setByDistanceOnPath failed")
    else:
      raise Unsupported(f"construction plane method {m}")
    p = planes.add(inp)
    p.name = f["name"]
    self.planes[f["name"]] = p
    return p

  def point_entity(self, spec):
    """A point for a construction feature: {"vertex": [x,y,z]} (the body vertex there) or
    {"sketch", "point"} (a sketch point id, or a '<id>.start'-style ref)."""
    if "vertex" in spec:
      vt = adsk.fusion.BRepEntityTypes.BRepVertexEntityType
      for tol in (1e-4, 1e-3, 1e-2):
        hits = self.root.findBRepUsingPoint(P(spec["vertex"]), vt, tol, False)
        if hits and hits.count:
          return hits.item(0)
      raise RuntimeError(f"no vertex at {spec['vertex']}")
    if "sketch" in spec:
      return self.sketches[spec["sketch"]]["points"][spec["point"]]
    if spec.get("ref") in self.points:
      return self.points[spec["ref"]]
    raise RuntimeError(f"can't resolve point {spec}")

  def op_construction_axis(self, f):
    axes = self.root.constructionAxes
    inp = axes.createInput()
    m = f["method"]
    if m == "normal_to_face":
      # Axis Perpendicular To Face: normal to "face" through "point" (a vertex or sketch point).
      face = self.find(f["face"]["point"], "face", f["face"])
      if not inp.setByNormalToFaceAtPoint(face, self.point_entity(f["point"])):
        raise RuntimeError("setByNormalToFaceAtPoint failed")
    elif m == "edge":
      # Axis Through Edge: along a linear body edge {"point"}, a sketch line {"sketch", "curve"}
      # or another axis {"ref"}.
      e = f["edge"]
      ent = self.find(e["point"], "edge") if "point" in e else self.axis(e)
      if not inp.setByEdge(ent):
        raise RuntimeError("setByEdge failed")
    elif m == "two_planes":
      # Axis Through Two Planes: the intersection of "a" and "b" (origin/construction planes or
      # planar faces {"face": {"point", "normal"}}, as plane() takes them).
      if not inp.setByTwoPlanes(self.plane(f["a"]), self.plane(f["b"])):
        raise RuntimeError("setByTwoPlanes failed")
    elif m == "two_points":
      # Axis Through Two Points: through "a" and "b" (vertices or sketch points, as
      # point_entity() takes them).
      if not inp.setByTwoPoints(self.point_entity(f["a"]), self.point_entity(f["b"])):
        raise RuntimeError("setByTwoPoints failed")
    else:
      raise Unsupported(f"construction axis method {m}")
    a = axes.add(inp)
    a.name = f["name"]
    self.axes[f["name"]] = a
    return a

  def op_construction_point(self, f):
    pts = self.root.constructionPoints
    inp = pts.createInput()
    m = f["method"]
    if m == "vertex":
      # Point At Vertex: at "point", a body vertex {"vertex": [x,y,z]} or a sketch point
      # {"sketch", "point"} (as point_entity() takes them).
      if not inp.setByPoint(self.point_entity(f["point"])):
        raise RuntimeError("setByPoint failed")
    elif m == "along_path":
      # Point Along Path: on "path" (a body edge {"point"} or a sketch curve {"sketch", "curve"}),
      # "t" (0..1, default 0.5) of its length from its start, or with "distance_type": "physical",
      # "distance" mm along it.
      e = f["path"]
      ent = self.find(e["point"], "edge") if "point" in e else           self.sketches[e["sketch"]]["entities"][e["curve"]]
      path = adsk.fusion.Path.create(ent, adsk.fusion.ChainedCurveOptions.noChainedCurves)
      if f.get("distance_type", "proportional") == "physical":
        kind, d = adsk.fusion.PathDistanceTypes.PhysicalPathDistanceType, V(f["distance"])
      else:
        kind = adsk.fusion.PathDistanceTypes.ProportionalPathDistanceType
        d = adsk.core.ValueInput.createByReal(float(f.get("t", 0.5)))
      if not inp.setByPath(path, kind, d):
        raise RuntimeError("setByPath failed")
    elif m == "edge_plane":
      # Point At Edge And Plane: where "edge" (a linear body edge {"point"}, a sketch line
      # {"sketch", "curve"} or an axis as axis() takes it) meets "plane" (as plane() takes it).
      e = f["edge"]
      ent = self.find(e["point"], "edge") if isinstance(e, dict) and "point" in e else self.axis(e)
      if not inp.setByEdgePlane(ent, self.plane(f["plane"])):
        raise RuntimeError("setByEdgePlane failed")
    elif m == "center":
      # Point At Center Of Circle/Sphere/Torus: of "face" (a circular planar, spherical or
      # toroidal face {"point", ...}), "edge" (a circular body edge {"point"}) or a sketch
      # circle/arc ({"sketch", "curve"}).
      if "face" in f:
        ent = self.find(f["face"]["point"], "face", f["face"])
      elif "edge" in f:
        ent = self.find(f["edge"]["point"], "edge")
      else:
        ent = self.sketches[f["sketch"]]["entities"][f["curve"]]
      if not inp.setByCenter(ent):
        raise RuntimeError("setByCenter failed")
    elif m == "three_planes":
      # Point Through Three Planes: where "a", "b" and "c" (origin/construction planes or planar
      # faces {"face": {"point", "normal"}}, as plane() takes them), extended, meet.
      if not inp.setByThreePlanes(self.plane(f["a"]), self.plane(f["b"]), self.plane(f["c"])):
        raise RuntimeError("setByThreePlanes failed")
    elif m == "two_edges":
      # Point Through Two Edges: where "a" and "b" (linear body edges {"point"}, or axes / sketch
      # lines as axis() takes them: world axis name, {"ref"}, {"sketch", "curve"}), extended, meet.
      def lin(e):
        return self.find(e["point"], "edge") if isinstance(e, dict) and "point" in e else self.axis(e)
      if not inp.setByTwoEdges(lin(f["a"]), lin(f["b"])):
        raise RuntimeError("setByTwoEdges failed")
    else:
      raise Unsupported(f"construction point method {m}")
    p = pts.add(inp)
    p.name = f["name"]
    self.points[f["name"]] = p
    return p

  def feature(self, name):
    if name in self.features:
      return self.features[name]
    base = name.rsplit(" (", 1)[0]
    if base in self.features:
      return self.features[base]
    raise RuntimeError(f"no feature named {name!r}")

  def pattern_entities(self, f):
    if f.get("pattern_of", "features") == "features":
      return collection([self.feature(n) for n in f["features"]])
    return collection([self.body(n) for n in f["bodies"]])

  def linear(self, axis_vec):
    unit = [abs(x) for x in axis_vec]
    if sorted(unit) != [0, 0, 1]:
      raise Unsupported(f"pattern direction {axis_vec}")
    k = unit.index(1)
    return self.axes["XYZ"[k]], (1 if axis_vec[k] > 0 else -1)

  def op_rectangular_pattern(self, f):
    """"distance_type" "spacing ..." (default; "spacing" is between instances) or "extent ..."
    ("spacing" is then the whole pattern's length). "symmetric": true in a direction spreads it
    both ways about the originals."""
    pats = self.root.features.rectangularPatternFeatures
    d1 = f["direction1"]
    a1, s1 = self.linear(d1["axis"])
    kind = str(f.get("distance_type", "spacing"))
    if kind.startswith("spacing"):
      dt = adsk.fusion.PatternDistanceType.SpacingPatternDistanceType
    elif kind.startswith("extent"):
      dt = adsk.fusion.PatternDistanceType.ExtentPatternDistanceType
    else:
      raise Unsupported(f"rectangular_pattern distance_type {kind}")
    inp = pats.createInput(self.pattern_entities(f), a1, V(d1["count"], ""), V(d1["spacing"] * s1), dt)
    inp.isSymmetricInDirectionOne = bool(d1.get("symmetric", False))
    if f.get("direction2"):
      d2 = f["direction2"]
      a2, s2 = self.linear(d2["axis"])
      inp.setDirectionTwo(a2, V(d2["count"], ""), V(d2["spacing"] * s2))
      inp.isSymmetricInDirectionTwo = bool(d2.get("symmetric", False))
    else:
      # Without a second direction Fusion keeps its default quantity there (stacked copies).
      inp.quantityTwo = V(1, "")
    return pats.add(inp)

  def op_circular_pattern(self, f):
    pats = self.root.features.circularPatternFeatures
    inp = pats.createInput(self.pattern_entities(f), self.axis(f["axis"]))
    inp.quantity = V(f["count"], "")
    inp.totalAngle = V(f.get("total_angle", 360), "deg")
    inp.isSymmetric = bool(f.get("symmetric", False))
    return pats.add(inp)

  def op_path_pattern(self, f):
    """Pattern along a sketch path. `spacing` is the arc length between instances; or `distance`
    with `distance_type` "extent" spreads `count` instances over that length. `orientation` is
    "identical" (translated copies) or "along_path" (rotated with the path's tangent).
    `start_point` (0..1 of the path) is where the distance is counted from; `flip` runs toward
    the path start; `symmetric` runs both ways."""
    # The pattern stores its distance in the design's display units rounded to their precision
    # ("15 mm" becomes "0.591 in" = 15.011 mm), so work in mm.
    self.design.fusionUnitsManager.distanceDisplayUnits = adsk.fusion.DistanceUnits.MillimeterDistanceUnits
    pats = self.root.features.pathPatternFeatures
    path = self.root.features.createPath(collection(self.curves(f["path"])), bool(f["path"].get("chained", True)))
    types = adsk.fusion.PatternDistanceType
    if f.get("spacing") is not None:
      dist, dtype = f["spacing"], types.SpacingPatternDistanceType
    elif str(f.get("distance_type", "extent")).startswith("extent"):
      dist, dtype = f["distance"], types.ExtentPatternDistanceType
    else:
      dist, dtype = f["distance"], types.SpacingPatternDistanceType
    inp = pats.createInput(self.pattern_entities(f), path, V(f["count"], ""), V(dist), dtype)
    if f.get("start_point") is not None:
      inp.startPoint = float(f["start_point"])  # 0 = path start, 1 = path end
    inp.isFlipDirection = bool(f.get("flip", False))
    inp.isSymmetric = bool(f.get("symmetric", False))
    orient = str(f.get("orientation", "identical"))
    if orient not in ("identical", "along_path") and not orient.startswith("identical"):
      raise Unsupported(f"path pattern orientation {orient}")
    inp.isOrientationAlongPath = orient == "along_path"
    return pats.add(inp)

  def op_mirror(self, f):
    mirrors = self.root.features.mirrorFeatures
    if f.get("mirror_of") == "bodies":
      ents = collection([self.body(b) for b in f["bodies"]])
    else:
      ents = collection([self.feature(n) for n in f["features"]])
    inp = mirrors.createInput(ents, self.plane(f["plane"]))
    if f.get("mirror_of") == "bodies" and f.get("combine"):
      inp.isCombine = True
    return mirrors.add(inp)

  def op_hole(self, f):
    holes = self.root.features.holeFeatures
    t = f.get("hole_type", "simple")
    if t == "simple":
      inp = holes.createSimpleInput(V(f["diameter"]))
    elif t == "counterbore":
      inp = holes.createCounterboreInput(V(f["diameter"]), V(f["counterbore_diameter"]), V(f["counterbore_depth"]))
    elif t == "countersink":
      inp = holes.createCountersinkInput(V(f["diameter"]), V(f["countersink_diameter"]), V(f["countersink_angle"], "deg"))
    else:
      raise Unsupported(f"hole type {t}")
    if "tip_angle" in f:
      inp.tipAngle = V(f["tip_angle"], "deg")
    if f.get("positions_from", "").startswith("sketch "):
      rec = self.sketches[f["positions_from"].split()[1]]
      pts = [e for e in rec["entities"].values() if e.objectType.endswith("SketchPoint")]
      inp.setPositionBySketchPoints(collection(pts))
    else:
      inp.setPositionByPoint(self.find(f["face"]["point"], "face", f["face"]), P(f["position"]))
    ext = f.get("extent", "distance")
    if ext == "through_all":
      inp.setAllExtent(adsk.fusion.ExtentDirections.PositiveExtentDirection)
    elif ext == "to":
      # "to": a construction plane name or {"face": {"point"}}; the hole stops on it.
      if not inp.setOneSideToExtent(self.plane(f["to"]), bool(f.get("match_shape", False))):
        raise RuntimeError("setOneSideToExtent failed")
    else:
      inp.setDistanceExtent(V(f["depth"]))
    if f.get("flip"):
      inp.isDefaultDirection = False
    return holes.add(inp)

  def op_draft(self, f):
    # "neutral_plane": a planar face {"point"} or any plane the sketch op accepts (a construction
    # plane name, {"origin", "normal"}). "angle2" drafts the two sides of the plane independently
    # (setTwoAngles); "symmetric" splits the faces and drafts both sides by "angle".
    drafts = self.root.features.draftFeatures
    np = f["neutral_plane"]
    plane = self.find(np["point"], "face", np) if isinstance(np, dict) and "point" in np else self.plane(np)
    inp = drafts.createInput(self.faces(f["faces"]), plane, bool(f.get("tangent_chain", False)))
    if f.get("angle2") is not None:
      inp.setTwoAngles(V(f["angle"], "deg"), V(f["angle2"], "deg"))
    else:
      inp.setSingleAngle(bool(f.get("symmetric", False)), V(f["angle"], "deg"))
    if f.get("flipped"):
      inp.isDirectionFlipped = True
    return drafts.add(inp)

  def op_offset_faces(self, f):
    # Offset Face (Modify > Offset Face): moves "faces" along their normals by "distance"
    # (positive grows the body).
    offsets = self.root.features.offsetFacesFeatures
    inp = offsets.createInput(self.faces(f["faces"]), V(f["distance"]))
    return offsets.add(inp)

  def op_sweep(self, f):
    path = self.root.features.createPath(collection(self.curves(f["path"])), bool(f["path"].get("chained", True)))
    sweeps = self.root.features.sweepFeatures
    inp = sweeps.createInput(self.profile_input(f["profile"]), path, OPERATIONS[f.get("operation", "new")])
    if f.get("orientation", "perpendicular") == "perpendicular":
      inp.orientation = adsk.fusion.SweepOrientationTypes.PerpendicularOrientationType
    else:
      inp.orientation = adsk.fusion.SweepOrientationTypes.ParallelOrientationType
    # Optional: "rail": {"sketch", "curves", "chained"?} guide rail with "profile_scaling"
    # (scale|stretch|none); "twist"/"taper" (deg); "distance_one"/"distance_two" (0..1 of the path).
    if f.get("rail"):
      r = f["rail"]
      inp.guideRail = self.root.features.createPath(collection(self.curves(r)), bool(r.get("chained", True)))
      scaling = {
        "scale": adsk.fusion.SweepProfileScalingOptions.SweepProfileScaleOption,
        "stretch": adsk.fusion.SweepProfileScalingOptions.SweepProfileStretchOption,
        "none": adsk.fusion.SweepProfileScalingOptions.SweepProfileNoScalingOption,
      }
      if f.get("profile_scaling", "scale") not in scaling:
        raise Unsupported(f"sweep profile_scaling {f.get('profile_scaling')}")
      inp.profileScaling = scaling[f.get("profile_scaling", "scale")]
    if f.get("twist") is not None:
      inp.twistAngle = V(f["twist"], "deg")
    if f.get("taper") is not None:
      inp.taperAngle = V(f["taper"], "deg")
    if f.get("distance_one") is not None:
      inp.distanceOne = V(f["distance_one"], "")
    if f.get("distance_two") is not None:
      inp.distanceTwo = V(f["distance_two"], "")
    feat = sweeps.add(inp)
    self.name_bodies(feat, f.get("result_bodies"))
    return feat

  def op_pipe(self, f):
    path = self.root.features.createPath(collection(self.curves(f["path"])), bool(f["path"].get("chained", True)))
    pipes = self.root.features.pipeFeatures
    inp = pipes.createInput(path, OPERATIONS[f.get("operation", "new")])
    sections = {
      "circular": adsk.fusion.PipeSectionTypes.CircularPipeSectionType,
      "square": adsk.fusion.PipeSectionTypes.SquarePipeSectionType,
      "triangular": adsk.fusion.PipeSectionTypes.TriangularPipeSectionType,
    }
    if f.get("section", "circular") not in sections:
      raise Unsupported(f"pipe section {f.get('section')}")
    inp.sectionType = sections[f.get("section", "circular")]
    inp.sectionSize = V(f["size"])
    if f.get("wall") is not None:
      inp.isHollow = True
      inp.sectionThickness = V(f["wall"])
    if f.get("distance_one") is not None:
      inp.distanceOne = V(f["distance_one"], "")
    if f.get("distance_two") is not None:
      inp.distanceTwo = V(f["distance_two"], "")
    feat = pipes.add(inp)
    self.name_bodies(feat, f.get("result_bodies"))
    return feat

  def op_loft(self, f):
    # "rails": [{"sketch", "curves", "chained"?}] or "centerline": {"sketch", "curves"}; "closed";
    # per section "end_condition": {"type": free|direction|point_sharp|point_tangent, "angle"?
    # (deg, direction), "weight"?}.
    lofts = self.root.features.loftFeatures
    inp = lofts.createInput(OPERATIONS[f.get("operation", "new")])
    for s in f["sections"]:
      if "point" in s:
        sec = inp.loftSections.add(self.sketches[s["sketch"]]["points"][s["point"]])
      else:
        sec = inp.loftSections.add(self.profile_input(s))
      ec = s.get("end_condition")
      if ec:
        w = adsk.core.ValueInput.createByReal(float(ec.get("weight", 1.0)))
        t = ec["type"]
        if t == "free":
          ok = sec.setFreeEndCondition()
        elif t == "direction":
          ok = sec.setDirectionEndCondition(V(ec.get("angle", 0), "deg"), w)
        elif t == "point_sharp":
          ok = sec.setPointSharpEndCondition()
        elif t == "point_tangent":
          ok = sec.setPointTangentEndCondition(w)
        else:
          raise Unsupported(f"loft end condition {t}")
        if not ok:
          raise RuntimeError(f"loft end condition {t} failed")
    guides = inp.centerLineOrRails
    for r in f.get("rails", []):
      guides.addRail(self.root.features.createPath(collection(self.curves(r)), bool(r.get("chained", True))))
    if f.get("centerline"):
      c = f["centerline"]
      guides.addCenterLine(self.root.features.createPath(collection(self.curves(c)), bool(c.get("chained", True))))
    if f.get("closed"):
      inp.isClosed = True
    inp.isSolid = bool(f.get("solid", True))
    feat = lofts.add(inp)
    self.name_bodies(feat, f.get("result_bodies"))
    return feat

  def op_thread(self, f):
    threads = self.root.features.threadFeatures
    info = threads.createThreadInfo(bool(f.get("internal", False)), f.get("thread_type", "ISO Metric profile"),
                                    f["designation"], f["class"])
    faces = self.faces(f["faces"]) if "faces" in f else [self.find(f["face"]["point"], "face", f["face"])]
    inp = threads.createInput(collection(faces), info)
    inp.isModeled = bool(f.get("modeled", False))
    inp.isRightHanded = bool(f.get("right_handed", True))
    inp.isFullLength = bool(f.get("full_length", True))
    if not inp.isFullLength:
      inp.threadLength = V(f["length"])
      inp.threadOffset = V(f.get("offset", 0))
      loc = f.get("location")
      if loc == "high_end":
        inp.threadLocation = adsk.fusion.ThreadLocations.HighEndThreadLocation
      elif loc == "low_end":
        inp.threadLocation = adsk.fusion.ThreadLocations.LowEndThreadLocation
      elif loc is not None:
        raise Unsupported(f"thread location {loc}")
    return threads.add(inp)

  def op_move(self, f):
    """Move/Copy (parametric Move feature) of "bodies" (names) or "faces" ([{"point"}]): by
    "translate" [x, y, z] mm, then rotated "angle" deg about "axis" (as axis() takes it). Both
    together are one free move (translation first, then the rotation)."""
    if f.get("copy"):
      raise Unsupported("move with create copy")
    if f.get("faces"):
      ents = collection([self.find(x["point"], "face", x) for x in f["faces"]])
    else:
      ents = collection([self.body(b) for b in f["bodies"]])
    moves = self.root.features.moveFeatures
    inp = moves.createInput2(ents)
    t = [float(x) for x in f.get("translate", [0, 0, 0])]
    angle = float(f.get("angle", 0))
    if angle and any(t):
      m = adsk.core.Matrix3D.create()
      m.translation = adsk.core.Vector3D.create(*[x * MM for x in t])
      ax = self.axis(f["axis"])
      line = ax.geometry if hasattr(ax, "geometry") else ax.worldGeometry
      if hasattr(line, "startPoint"):
        o, d = line.startPoint, line.startPoint.vectorTo(line.endPoint)
      else:
        o, d = line.origin, line.direction
      r = adsk.core.Matrix3D.create()
      r.setToRotation(math.radians(angle), d, o)
      m.transformBy(r)
      inp.defineAsFreeMove(m)
    elif angle:
      inp.defineAsRotate(self.axis(f["axis"]), V(angle, "deg"))
    else:
      inp.defineAsTranslateXYZ(V(t[0]), V(t[1]), V(t[2]), True)
    return moves.add(inp)

  def op_split_body(self, f):
    splits = self.root.features.splitBodyFeatures
    inp = splits.createInput(self.body(f["body"]), self.plane(f["tool"]), bool(f.get("extend_tool", True)))
    feat = splits.add(inp)
    self.name_bodies(feat, f.get("result_bodies"))
    return feat

  def op_set_parameter(self, f):
    """Change Parameters: set an existing user parameter's expression after the features that use
    it are built, so the timeline recomputes."""
    p = self.design.userParameters.itemByName(f["name"])
    if p is None:
      raise RuntimeError(f"no user parameter {f['name']}")
    p.expression = f["expression"]
    return None

  def op_delete(self, f):
    """Delete (not Remove): permanently delete a named feature or sketch ("feature"), or a body
    ("body"), with no timeline feature. Deleting a body that is the only output of its feature
    deletes that feature; deleting a consumed sketch leaves its features on cached geometry."""
    if "body" in f:
      target = self.body(f["body"])
    elif "feature" in f:
      name = f["feature"]
      target = self.features.get(name)
      if target is None and name in self.sketches:
        target = self.sketches[name]["sketch"]
      if target is None:
        raise RuntimeError(f"no feature or sketch named {name!r}")
    else:
      raise Unsupported("delete needs a 'feature' or 'body'")
    if not target.deleteMe():
      raise RuntimeError(f"Fusion refused to delete {f}")
    return None

  def op_compute_all(self, f):
    """Compute All: recompute every parameter, feature and assembly relationship in the design."""
    self.design.computeAll()
    return None

  # ---- driver ---------------------------------------------------------------------------------

  def run(self):
    for p in self.recipe.get("parameters", []):
      self.design.userParameters.add(p["name"], adsk.core.ValueInput.createByString(p["expression"]),
                                     p.get("unit", "mm"), p.get("comment", ""))
    for i, f in enumerate(self.recipe.get("features", [])):
      op = f.get("op")
      step = {"index": i, "op": op, "name": f.get("name")}
      handler = getattr(self, "op_" + str(op), None)
      try:
        if handler is None:
          raise Unsupported(f"op {op}")
        made = handler(f)
        if f.get("name") and made is not None:
          self.features[f["name"]] = made
          try:
            if op != "sketch":
              made.name = f["name"]
          except Exception:
            pass
        step["ok"] = True
      except Unsupported as e:
        step.update(ok=False, unsupported=str(e))
        self.unsupported.append({"index": i, "op": op, "what": str(e)})
      except Exception as e:
        step.update(ok=False, error=str(e), trace=traceback.format_exc(limit=3))
      self.steps.append(step)
    return self


def run(app, recipe, keep_open=False, screenshot=None):
  r = Runner(app, recipe).run()
  result = measure.design(app)
  result.update(ok=all(s["ok"] for s in r.steps), recipe=recipe.get("id"), steps=r.steps,
                unsupported=r.unsupported)
  if screenshot:
    result["screenshot"] = measure.screenshot(app, screenshot)
  if not keep_open:
    r.doc.close(False)
  return result


def close_scratch_documents(app):
  closed = 0
  for i in reversed(range(app.documents.count)):
    d = app.documents.item(i)
    if d.name.startswith(SCRATCH_PREFIX):
      d.close(False)
      closed += 1
  return {"ok": True, "closed": closed}
