# Joint (Joint Constraint): grounded A 20x20x10 at the origin; B 10x20x5 built at x 40..50,
# y 0..20, z 0..5. Rigid joint from the centre of B's bottom face to the centre of A's top face,
# offset 3 mm, angle 90 deg. Fusion keeps both face normals pointing the same way (not flipped),
# so B turns over and ends up at x 0..20, y 5..15, z 8..13.
# measure.py reads component bodies in their own frames, so the placed bodies are copied into the
# root and the originals hidden.
P = adsk.core.Point3D.create
CM = 0.1
KP = adsk.fusion.JointKeyPointTypes


def box_comp(name, x0, y0, x1, y1, h):
  occ = root.occurrences.addNewComponent(adsk.core.Matrix3D.create())
  comp = occ.component
  comp.name = name
  sk = comp.sketches.add(comp.xYConstructionPlane)
  sk.sketchCurves.sketchLines.addTwoPointRectangle(P(x0 * CM, y0 * CM, 0), P(x1 * CM, y1 * CM, 0))
  ext = comp.features.extrudeFeatures.addSimple(
    sk.profiles.item(0), adsk.core.ValueInput.createByReal(h * CM),
    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
  return occ, ext.bodies.item(0)


def face_z(occ, body, z):
  for f in body.faces:
    if f.geometry.surfaceType == adsk.core.SurfaceTypes.PlaneSurfaceType:
      bb = f.boundingBox
      if abs(bb.minPoint.z - z * CM) < 1e-6 and abs(bb.maxPoint.z - z * CM) < 1e-6:
        return f.createForAssemblyContext(occ)
  raise RuntimeError('no face at z=%g' % z)


occ_a, body_a = box_comp('A', 0, 0, 20, 20, 10)
occ_a.isGrounded = True
occ_b, body_b = box_comp('B', 40, 0, 50, 20, 5)

g1 = adsk.fusion.JointGeometry.createByPlanarFace(face_z(occ_b, body_b, 0), None, KP.CenterKeyPoint)
g2 = adsk.fusion.JointGeometry.createByPlanarFace(face_z(occ_a, body_a, 10), None, KP.CenterKeyPoint)
inp = root.joints.createInput(g1, g2)
inp.setAsRigidJointMotion()
inp.offset = adsk.core.ValueInput.createByString('3 mm')
inp.angle = adsk.core.ValueInput.createByString('90 deg')
if root.joints.add(inp) is None:
  raise RuntimeError('joint failed')

for occ, body in ((occ_a, body_a), (occ_b, body_b)):
  body.createForAssemblyContext(occ).copyToComponent(root)
  body.isLightBulbOn = False
