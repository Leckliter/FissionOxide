# Joint (rigid), flipped and turned: grounded A 20x20x10 box; B 10x6x4 box built at x 40..50, y 0..6.
# B bottom face centre jointed to A top face centre, flipped, angle 90 deg, offset 2 mm.
import math
P = adsk.core.Point3D.create
V = adsk.core.Vector3D.create
CM = 0.1
J = adsk.fusion.JointDirections
K = adsk.fusion.JointKeyPointTypes


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


def plane_face(body, pt, n):
  p = P(pt[0] * CM, pt[1] * CM, pt[2] * CM)
  for f in body.faces:
    if f.geometry.surfaceType != adsk.core.SurfaceTypes.PlaneSurfaceType:
      continue
    ok, fn = f.evaluator.getNormalAtPoint(f.pointOnFace)
    if fn.isEqualTo(V(*n)) and abs(fn.dotProduct(f.pointOnFace.vectorTo(p))) < 1e-7:
      return f
  raise RuntimeError("no face through %s normal %s" % (pt, n))


def cyl_face(body):
  for f in body.faces:
    if f.geometry.surfaceType == adsk.core.SurfaceTypes.CylinderSurfaceType:
      return f
  raise RuntimeError("no cylindrical face")


def circle_edge(body, z):
  for e in body.edges:
    g = e.geometry
    if g.curveType == adsk.core.Curve3DTypes.Circle3DCurveType and abs(g.center.z - z * CM) < 1e-7:
      return e
  raise RuntimeError("no circular edge at z=%s" % z)


def vertex_at(body, p):
  for v in body.vertices:
    q = v.geometry
    if abs(q.x - p[0] * CM) < 1e-6 and abs(q.y - p[1] * CM) < 1e-6 and abs(q.z - p[2] * CM) < 1e-6:
      return v
  raise RuntimeError("no vertex at %s" % (p,))


def ctx(occ, ent):
  return ent.createForAssemblyContext(occ)


def val(s):
  return adsk.core.ValueInput.createByString(s)


def bake(pairs):
  # a driven value is transient in Fusion: capture it as a position (Fox keeps it as the joint's value)
  if design.snapshots.hasPendingSnapshot:
    design.snapshots.add()
  # measure.py reads component bodies in their own frames: copy the placed bodies into the root
  for occ, body in pairs:
    body.createForAssemblyContext(occ).copyToComponent(root)
    body.isLightBulbOn = False


occ_a, body_a = comp_body("A", rect(0, 0, 20, 20), 10)
occ_a.isGrounded = True
occ_b, body_b = comp_body("B", rect(40, 0, 50, 6), 4)
g_a = adsk.fusion.JointGeometry.createByPlanarFace(ctx(occ_a, plane_face(body_a, (10, 10, 10), (0, 0, 1))), None, K.CenterKeyPoint)
g_b = adsk.fusion.JointGeometry.createByPlanarFace(ctx(occ_b, plane_face(body_b, (45, 3, 0), (0, 0, -1))), None, K.CenterKeyPoint)
inp = root.joints.createInput(g_b, g_a)
inp.setAsRigidJointMotion()
inp.isFlipped = True
inp.angle = val("90 deg")
inp.offset = val("2 mm")
root.joints.add(inp)
bake(((occ_a, body_a), (occ_b, body_b)))
