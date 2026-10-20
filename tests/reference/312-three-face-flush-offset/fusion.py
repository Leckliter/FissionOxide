# Constrain Components: one constraint set of three planar face mates. Grounded A 20x20x10 at the
# origin; B 10x10x5 built beside it at x 40..50, y 0..10, z 0..5. B bottom mates A top (opposed
# normals); B's -x face is flush (flipped, aligned normals) with A's -x face 5 mm offset; B's -y
# face is flush with A's -y face. B lands on A's top in the -x,-y corner region.
P = adsk.core.Point3D.create
V = adsk.core.Vector3D.create
CM = 0.1


def comp_body(name, profile_fn, h):
  occ = root.occurrences.addNewComponent(adsk.core.Matrix3D.create())
  comp = occ.component
  comp.name = name
  sk = comp.sketches.add(comp.xYConstructionPlane)
  profile_fn(sk)
  ext = comp.features.extrudeFeatures.addSimple(
    sk.profiles.item(0), adsk.core.ValueInput.createByReal(h * CM),
    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
  return occ, ext.bodies.item(0)


def rect(x0, y0, x1, y1):
  return lambda sk: sk.sketchCurves.sketchLines.addTwoPointRectangle(
    P(x0 * CM, y0 * CM, 0), P(x1 * CM, y1 * CM, 0))


def circle(cx, cy, r):
  return lambda sk: sk.sketchCurves.sketchCircles.addByCenterRadius(P(cx * CM, cy * CM, 0), r * CM)


def plane_face(occ, body, pt, n):
  # Planar face through pt (mm) with outward normal n.
  p = P(pt[0] * CM, pt[1] * CM, pt[2] * CM)
  for f in body.faces:
    if f.geometry.surfaceType != adsk.core.SurfaceTypes.PlaneSurfaceType:
      continue
    ok, fn = f.evaluator.getNormalAtPoint(f.pointOnFace)
    if fn.isEqualTo(V(*n)) and abs(fn.dotProduct(f.pointOnFace.vectorTo(p))) < 1e-7:
      return f.createForAssemblyContext(occ)
  raise RuntimeError("no face through %s normal %s" % (pt, n))


def cyl_face(occ, body):
  for f in body.faces:
    if f.geometry.surfaceType == adsk.core.SurfaceTypes.CylinderSurfaceType:
      return f.createForAssemblyContext(occ)
  raise RuntimeError("no cylindrical face")


def vertex_at(occ, body, p):
  for v in body.vertices:
    q = v.geometry
    if abs(q.x - p[0] * CM) < 1e-6 and abs(q.y - p[1] * CM) < 1e-6 and abs(q.z - p[2] * CM) < 1e-6:
      return v.createForAssemblyContext(occ)
  raise RuntimeError("no vertex at %s" % (p,))


def val(s):
  return adsk.core.ValueInput.createByString(s)


def constrain(rels):
  # One Constrain Components set: rels = [(entity_moving, entity_fixed, is_mate, value, flipped)].
  inp = root.assemblyConstraints.createInput()
  for a, b, mate, v, flip in rels:
    g = inp.geometricRelationships.add(a, b, mate, val(v))
    if flip:
      g.isFlipped = True
  c = root.assemblyConstraints.add(inp)
  if c is None:
    raise RuntimeError("assembly constraint failed")
  return c


def bake(pairs):
  # measure.py reads component bodies in their own frames: copy the placed bodies into the root.
  for occ, body in pairs:
    body.createForAssemblyContext(occ).copyToComponent(root)
    body.isLightBulbOn = False


occ_a, body_a = comp_body("A", rect(0, 0, 20, 20), 10)
occ_a.isGrounded = True
occ_b, body_b = comp_body("B", rect(40, 0, 50, 10), 5)
constrain([
  (plane_face(occ_b, body_b, (45, 5, 0), (0, 0, -1)), plane_face(occ_a, body_a, (10, 10, 10), (0, 0, 1)), True, "0 mm", False),
  (plane_face(occ_b, body_b, (40, 5, 2), (-1, 0, 0)), plane_face(occ_a, body_a, (0, 10, 5), (-1, 0, 0)), True, "5 mm", True),
  (plane_face(occ_b, body_b, (45, 0, 2), (0, -1, 0)), plane_face(occ_a, body_a, (10, 0, 5), (0, -1, 0)), True, "0 mm", True),
])
bake(((occ_a, body_a), (occ_b, body_b)))
