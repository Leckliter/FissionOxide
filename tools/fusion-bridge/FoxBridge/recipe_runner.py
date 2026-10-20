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
      elif t == "circle":
        c = sk.sketchCurves.sketchCircles.addByCenterRadius(pt(e["center"]), e["radius"] * MM)
        keep(eid, c, center=c.centerSketchPoint)
      elif t == "arc_three_point":
        c = sk.sketchCurves.sketchArcs.addByThreePoints(pt(e["start"]), pt(e["mid"]), pt(e["end"]))
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
      elif t == "point":
        c = sk.sketchPoints.add(pt(e["at"]))
        rec["entities"][eid] = c
        rec["points"][eid] = c
      elif t == "fitted_spline":
        pts = collection([sk.modelToSketchSpace(P(world(p))) for p in e["fit_points"]])
        c = sk.sketchCurves.sketchFittedSplines.add(pts)
        keep(eid, c, start=c.startSketchPoint, end=c.endSketchPoint)
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
        elif t in two and two[t]:
          a, b = refs
          is_point = lambda e: e.objectType.endswith("SketchPoint")
          if t in ("coincident", "midpoint") and not is_point(a) and is_point(b):
            a, b = b, a  # Fusion wants the point first
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
    if "thin" in f:
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
                            adsk.fusion.DistanceExtentDefinition.create(V(e["side2_opposite"])))
    else:
      raise Unsupported(f"extrude extent {t}")
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
    inp.setAngleExtent(False, V(f.get("angle", 360), "deg"))
    feat = self.root.features.revolveFeatures.add(inp)
    self.name_bodies(feat, f.get("result_bodies"))
    return feat

  def op_fillet(self, f):
    fillets = self.root.features.filletFeatures
    inp = fillets.createInput()
    edges = collection(self.edges(f["edges"]))
    chain = bool(f.get("tangent_chain", False))
    t = f.get("type", "constant_radius")
    if f.get("fit") and f["fit"] != "fixed":
      # Fusion has no fit option: building the plain fillet instead would measure a different part.
      raise Unsupported(f"fillet fit: {f['fit']} (Fusion has no taper-to-fit fillet)")
    sets = getattr(inp, "edgeSetInputs", None)
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
    return fillets.add(inp)

  def op_chamfer(self, f):
    chamfers = self.root.features.chamferFeatures
    edges = collection(self.edges(f["edges"]))
    chain = bool(f.get("tangent_chain", False))
    t = f.get("type", "equal_distance")
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
      inp.setByAngle(self.axis(f["axis"]), V(f["angle"], "deg"), self.plane(f["base_plane"]))
    else:
      raise Unsupported(f"construction plane method {m}")
    p = planes.add(inp)
    p.name = f["name"]
    self.planes[f["name"]] = p
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
    pats = self.root.features.rectangularPatternFeatures
    d1 = f["direction1"]
    a1, s1 = self.linear(d1["axis"])
    types = adsk.fusion.PatternDistanceType
    extent = str(f.get("distance_type", "spacing")).startswith("extent")
    kind = types.ExtentPatternDistanceType if extent else types.SpacingPatternDistanceType
    inp = pats.createInput(self.pattern_entities(f), a1, V(d1["count"], ""), V(d1["spacing"] * s1), kind)
    inp.isSymmetricInDirectionOne = bool(d1.get("symmetric", False))
    if f.get("direction2"):
      d2 = f["direction2"]
      a2, s2 = self.linear(d2["axis"])
      inp.setDirectionTwo(a2, V(d2["count"], ""), V(d2["spacing"] * s2))
      inp.isSymmetricInDirectionTwo = bool(d2.get("symmetric", False))
    else:
      # otherwise Fusion keeps its default second-direction quantity and stacks extra copies
      inp.quantityTwo = V(1, "")
    return pats.add(inp)

  def op_circular_pattern(self, f):
    pats = self.root.features.circularPatternFeatures
    inp = pats.createInput(self.pattern_entities(f), self.axis(f["axis"]))
    inp.quantity = V(f["count"], "")
    inp.totalAngle = V(f.get("total_angle", 360), "deg")
    inp.isSymmetric = bool(f.get("symmetric", False))
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
    else:
      inp.setDistanceExtent(V(f["depth"]))
    return holes.add(inp)

  def op_draft(self, f):
    drafts = self.root.features.draftFeatures
    inp = drafts.createInput(self.faces(f["faces"]), self.find(f["neutral_plane"]["point"], "face", f["neutral_plane"]),
                             bool(f.get("tangent_chain", False)))
    inp.setSingleAngle(False, V(f["angle"], "deg"))
    if f.get("flipped"):
      inp.isDirectionFlipped = True
    return drafts.add(inp)

  def op_sweep(self, f):
    path = self.root.features.createPath(collection(self.curves(f["path"])), bool(f["path"].get("chained", True)))
    sweeps = self.root.features.sweepFeatures
    inp = sweeps.createInput(self.profile_input(f["profile"]), path, OPERATIONS[f.get("operation", "new")])
    if f.get("orientation", "perpendicular") == "perpendicular":
      inp.orientation = adsk.fusion.SweepOrientationTypes.PerpendicularOrientationType
    else:
      inp.orientation = adsk.fusion.SweepOrientationTypes.ParallelOrientationType
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
    if f.get("rails"):
      raise Unsupported("loft rails")
    lofts = self.root.features.loftFeatures
    inp = lofts.createInput(OPERATIONS[f.get("operation", "new")])
    for s in f["sections"]:
      if "point" in s:
        inp.loftSections.add(self.sketches[s["sketch"]]["points"][s["point"]])
      else:
        inp.loftSections.add(self.profile_input(s))
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

  def op_split_body(self, f):
    splits = self.root.features.splitBodyFeatures
    inp = splits.createInput(self.body(f["body"]), self.plane(f["tool"]), bool(f.get("extend_tool", True)))
    feat = splits.add(inp)
    self.name_bodies(feat, f.get("result_bodies"))
    return feat

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
