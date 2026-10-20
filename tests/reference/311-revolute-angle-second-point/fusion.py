import math
# Duplicate With Joints: grounded base A 60x20x5; peg B 30x6x10 built at the origin
# (x 0..30, y 0..6, z 0..10). A revolute joint (angle 30 deg) mates B's bottom end
# (0,3,0) to point (15,10,5) on A's top. Duplicate With Joints copies B and its joint, the copy
# snapped to (45,10,5). The Fusion API has no Duplicate With Joints, so this does what it does: a
# second occurrence of B's component and the same joint to the new snap point.
# measure.py reads component bodies in their own frames, so the placed bodies are copied into the
# root and the originals hidden.
P = adsk.core.Point3D.create
CM = 0.1
KIND = 'revolute'
ANGLE = 30


def top_plane(comp, z):
  pi = comp.constructionPlanes.createInput()
  pi.setByOffset(comp.xYConstructionPlane, adsk.core.ValueInput.createByReal(z * CM))
  return comp.constructionPlanes.add(pi)


def make_comp(name, x1, y1, h):
  occ = root.occurrences.addNewComponent(adsk.core.Matrix3D.create())
  comp = occ.component
  comp.name = name
  sk = comp.sketches.add(comp.xYConstructionPlane)
  sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0, 0), P(x1 * CM, y1 * CM, 0))
  ext = comp.features.extrudeFeatures.addSimple(
    sk.profiles.item(0), adsk.core.ValueInput.createByReal(h * CM),
    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
  return occ, comp, ext.bodies.item(0)


def points(comp, z, xys):
  plane = comp.xYConstructionPlane if z == 0 else top_plane(comp, z)
  sk = comp.sketches.add(plane)
  return [sk.sketchPoints.add(P(x * CM, y * CM, 0)) for (x, y) in xys]


def mate(moving_occ, moving_pt, fixed_occ, fixed_pt):
  g1 = adsk.fusion.JointGeometry.createByPoint(moving_pt.createForAssemblyContext(moving_occ))
  g2 = adsk.fusion.JointGeometry.createByPoint(fixed_pt.createForAssemblyContext(fixed_occ))
  inp = root.joints.createInput(g1, g2)
  if KIND == 'revolute':
    inp.setAsRevoluteJointMotion(adsk.fusion.JointDirections.ZAxisJointDirection)
  else:
    inp.setAsRigidJointMotion()
  if ANGLE:
    inp.angle = adsk.core.ValueInput.createByString('%g deg' % ANGLE)
  j = root.joints.add(inp)
  if j is None:
    raise RuntimeError('joint failed')
  return j


occ_a, comp_a, body_a = make_comp('A', 60, 20, 5)
occ_a.isGrounded = True
snap1, snap2 = points(comp_a, 5, [(15, 10), (45, 10)])
occ_b, comp_b, body_b = make_comp('B', 30, 6, 10)
(b_origin,) = points(comp_b, 0, [(0, 3)])
mate(occ_b, b_origin, occ_a, snap1)

# Duplicate With Joints: another instance of B, with B's joint re-made to the second snap point
occ_c = root.occurrences.addExistingComponent(comp_b, adsk.core.Matrix3D.create())
mate(occ_c, b_origin, occ_a, snap2)

for occ in (occ_a, occ_b, occ_c):
  for body in occ.component.bRepBodies:
    body.createForAssemblyContext(occ).copyToComponent(root)
for body in list(comp_a.bRepBodies) + list(comp_b.bRepBodies):
  body.isLightBulbOn = False
