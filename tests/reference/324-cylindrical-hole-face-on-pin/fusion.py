import math
# Joint (Joint Constraint): grounded pin A, a r5 h20 cylinder at the origin; plate B 40x12x4 built
# at x 30..70, y 0..12, z 0..4 with a r5 hole at (36,6). Cylindrical joint from the middle of the
# hole's cylindrical face to the middle of the pin's cylindrical face, then slid 5 mm along the
# axis and turned 90 deg: the plate rides the pin at z 13..17 with its long end along +Y.
# measure.py reads component bodies in their own frames, so the placed bodies are copied into the
# root and the originals hidden.
P = adsk.core.Point3D.create
CM = 0.1
KP = adsk.fusion.JointKeyPointTypes


def new_comp(name):
  occ = root.occurrences.addNewComponent(adsk.core.Matrix3D.create())
  occ.component.name = name
  return occ, occ.component


def extrude(comp, prof, h):
  return comp.features.extrudeFeatures.addSimple(
    prof, adsk.core.ValueInput.createByReal(h * CM),
    adsk.fusion.FeatureOperations.NewBodyFeatureOperation).bodies.item(0)


def cyl_face(occ, body):
  for f in body.faces:
    if f.geometry.surfaceType == adsk.core.SurfaceTypes.CylinderSurfaceType:
      return f.createForAssemblyContext(occ)
  raise RuntimeError('no cylindrical face')


occ_a, comp_a = new_comp('A')
sk = comp_a.sketches.add(comp_a.xYConstructionPlane)
sk.sketchCurves.sketchCircles.addByCenterRadius(P(0, 0, 0), 5 * CM)
body_a = extrude(comp_a, sk.profiles.item(0), 20)
occ_a.isGrounded = True

occ_b, comp_b = new_comp('B')
sk = comp_b.sketches.add(comp_b.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(30 * CM, 0, 0), P(70 * CM, 12 * CM, 0))
sk.sketchCurves.sketchCircles.addByCenterRadius(P(36 * CM, 6 * CM, 0), 5 * CM)
plate = [p for p in sk.profiles if p.profileLoops.count == 2][0]
body_b = extrude(comp_b, plate, 4)

g1 = adsk.fusion.JointGeometry.createByNonPlanarFace(cyl_face(occ_b, body_b), KP.MiddleKeyPoint)
g2 = adsk.fusion.JointGeometry.createByNonPlanarFace(cyl_face(occ_a, body_a), KP.MiddleKeyPoint)
inp = root.joints.createInput(g1, g2)
inp.setAsCylindricalJointMotion(adsk.fusion.JointDirections.ZAxisJointDirection)
j = root.joints.add(inp)
if j is None:
  raise RuntimeError('joint failed')
j.jointMotion.slideValue = 0.5
j.jointMotion.rotationValue = math.pi / 2
# a driven value is transient in Fusion: capture it as a position (Fox keeps it as the joint's value)
if design.snapshots.hasPendingSnapshot:
  design.snapshots.add()

for occ, body in ((occ_a, body_a), (occ_b, body_b)):
  body.createForAssemblyContext(occ).copyToComponent(root)
  body.isLightBulbOn = False
