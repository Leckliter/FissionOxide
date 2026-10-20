import math
# As-built slider constraint (as-built joint): grounded base A 20x20x10 at the origin, arm B 30x5x5
# on top of it at x 0..30, y 0..5, z 10..15. Slider along X at A's corner (20,0,10), driven 12 mm, so
# B moves to x 12..42. measure.py reads component bodies in their own frames, so the placed bodies
# are copied into the root and the originals hidden.
PIVOT = (20, 0, 10)
P = adsk.core.Point3D.create
CM = 0.1
J = adsk.fusion.JointDirections


def box_comp(name, x0, y0, x1, y1, z0, h):
  occ = root.occurrences.addNewComponent(adsk.core.Matrix3D.create())
  comp = occ.component
  comp.name = name
  plane = comp.xYConstructionPlane
  if z0:
    pi = comp.constructionPlanes.createInput()
    pi.setByOffset(plane, adsk.core.ValueInput.createByReal(z0 * CM))
    plane = comp.constructionPlanes.add(pi)
  sk = comp.sketches.add(plane)
  sk.sketchCurves.sketchLines.addTwoPointRectangle(P(x0 * CM, y0 * CM, 0), P(x1 * CM, y1 * CM, 0))
  ext = comp.features.extrudeFeatures.addSimple(
    sk.profiles.item(0), adsk.core.ValueInput.createByReal(h * CM),
    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
  return occ, ext.bodies.item(0)


def vertex_at(occ, body, p):
  for v in body.vertices:
    q = v.geometry
    if abs(q.x - p[0] * CM) < 1e-6 and abs(q.y - p[1] * CM) < 1e-6 and abs(q.z - p[2] * CM) < 1e-6:
      return v.createForAssemblyContext(occ)
  raise RuntimeError("no vertex at %s" % (p,))


occ_a, body_a = box_comp("A", 0, 0, 20, 20, 0, 10)
occ_a.isGrounded = True
occ_b, body_b = box_comp("B", 0, 0, 30, 5, 10, 5)

geo = adsk.fusion.JointGeometry.createByPoint(vertex_at(occ_a, body_a, PIVOT))
inp = root.asBuiltJoints.createInput(occ_b, occ_a, geo)
inp.setAsSliderJointMotion(J.XAxisJointDirection)
j = root.asBuiltJoints.add(inp)
j.jointMotion.slideValue = 12 * CM
# a driven value is transient in Fusion: capture it as a position (Fox keeps it as the joint's value)
if design.snapshots.hasPendingSnapshot:
  design.snapshots.add()

for occ, body in ((occ_a, body_a), (occ_b, body_b)):
  body.createForAssemblyContext(occ).copyToComponent(root)
  body.isLightBulbOn = False
