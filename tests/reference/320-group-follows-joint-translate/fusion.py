# Rigid Group: grounded base A 60x20x5 at the origin. Blocks B (x 0..10, y 0..10, z 20..30) and
# C (x 15..25, y 0..10, z 20..30) float above it and are rigid-grouped. A rigid joint then mates
# B's bottom centre (5,5,20) to (30,10,5) on A's top: B moves by (25,5,-15) and C, grouped with
# it, moves the same way (to x 40..50, y 5..15, z 5..15). Without the group C would stay put.
# measure.py reads component bodies in their own frames, so the placed bodies are copied into the
# root and the originals hidden.
P = adsk.core.Point3D.create
CM = 0.1
ANGLE = 0
JOINT_ON = 'B'


def plane_at(comp, z):
  if z == 0:
    return comp.xYConstructionPlane
  pi = comp.constructionPlanes.createInput()
  pi.setByOffset(comp.xYConstructionPlane, adsk.core.ValueInput.createByReal(z * CM))
  return comp.constructionPlanes.add(pi)


def make_comp(name, x0, y0, x1, y1, z0, h):
  occ = root.occurrences.addNewComponent(adsk.core.Matrix3D.create())
  comp = occ.component
  comp.name = name
  sk = comp.sketches.add(plane_at(comp, z0))
  sk.sketchCurves.sketchLines.addTwoPointRectangle(P(x0 * CM, y0 * CM, 0), P(x1 * CM, y1 * CM, 0))
  comp.features.extrudeFeatures.addSimple(
    sk.profiles.item(0), adsk.core.ValueInput.createByReal(h * CM),
    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
  return occ, comp


def point(comp, z, x, y):
  sk = comp.sketches.add(plane_at(comp, z))
  return sk.sketchPoints.add(P(x * CM, y * CM, 0))


occ_a, comp_a = make_comp('A', 0, 0, 60, 20, 0, 5)
occ_a.isGrounded = True
snap = point(comp_a, 5, 30, 10)
occ_b, comp_b = make_comp('B', 0, 0, 10, 10, 20, 10)
occ_c, comp_c = make_comp('C', 15, 0, 25, 10, 20, 10)

occs = adsk.core.ObjectCollection.create()
occs.add(occ_b)
occs.add(occ_c)
if root.rigidGroups.add(occs, True) is None:
  raise RuntimeError('rigid group failed')

if JOINT_ON == 'B':
  mover, origin = occ_b, point(comp_b, 20, 5, 5)
else:
  mover, origin = occ_c, point(comp_c, 20, 20, 5)
g1 = adsk.fusion.JointGeometry.createByPoint(origin.createForAssemblyContext(mover))
g2 = adsk.fusion.JointGeometry.createByPoint(snap.createForAssemblyContext(occ_a))
inp = root.joints.createInput(g1, g2)
inp.setAsRigidJointMotion()
if ANGLE:
  inp.angle = adsk.core.ValueInput.createByString('%g deg' % ANGLE)
if root.joints.add(inp) is None:
  raise RuntimeError('joint failed')

for occ in (occ_a, occ_b, occ_c):
  for body in occ.component.bRepBodies:
    body.createForAssemblyContext(occ).copyToComponent(root)
for comp in (comp_a, comp_b, comp_c):
  for body in comp.bRepBodies:
    body.isLightBulbOn = False
