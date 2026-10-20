# Motion Link from a two-motion joint: grounded base A 20x20x10 at the origin. Block B 10x10x10 at x 40..50,
# y 0..10, z 0..10 on an as-built cylindrical joint along X through (40,0,0). Arm C 30x5x5 at x 0..30, y 30..35,
# z 0..5 on an as-built revolute about Z through (0,35,0). Link: B's slide 10 mm <-> C 90 deg. B slid 10 mm, so C turns 90 deg.
import math
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
occ_b, body_b = box_comp("B", 40, 0, 50, 10, 0, 10)
occ_c, body_c = box_comp("C", 0, 30, 30, 35, 0, 5)

geo = adsk.fusion.JointGeometry.createByPoint(vertex_at(occ_b, body_b, (40, 0, 0)))
inp = root.asBuiltJoints.createInput(occ_b, occ_a, geo)
inp.setAsCylindricalJointMotion(J.XAxisJointDirection)
jb = root.asBuiltJoints.add(inp)

geo = adsk.fusion.JointGeometry.createByPoint(vertex_at(occ_c, body_c, (0, 35, 0)))
inp = root.asBuiltJoints.createInput(occ_c, occ_a, geo)
inp.setAsRevoluteJointMotion(J.ZAxisJointDirection)
jc = root.asBuiltJoints.add(inp)

# the leader's slide (not its rotation) drives the follower
mi = root.motionLinks.createInput(jb, jc)
mi.motionOne = adsk.fusion.JointMotionTypes.CylindricalJointSlideMotionType
mi.valueOne = adsk.core.ValueInput.createByString("10 mm")
mi.valueTwo = adsk.core.ValueInput.createByString("90 deg")
link = root.motionLinks.add(mi)

jb.jointMotion.slideValue = 1.0
# a driven value is transient in Fusion: capture it as a position (Fox keeps it as the joint's value)
if design.snapshots.hasPendingSnapshot:
  design.snapshots.add()
# measure.py reads component bodies in their own frames: copy the placed bodies into the root
for occ, body in ((occ_a, body_a), (occ_b, body_b), (occ_c, body_c)):
  body.createForAssemblyContext(occ).copyToComponent(root)
  body.isLightBulbOn = False
