# Align (assembly) constraint: flush. Two components: a grounded 40x30x10 plate A and a 10x10x10
# block B sitting beside it on XY (x 50..60, y 0..10). B's bottom face is mated (opposed normals)
# to A's top face with a 5 mm offset, so B rises to z 15..25. measure.py reads component bodies
# in their own frames, so the placed bodies are copied into the root and the originals hidden.
P = adsk.core.Point3D.create
CM = 0.1


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


def face_at(occ, body, z):
  for f in body.faces:
    pt = f.pointOnFace
    if abs(pt.z - z * CM) < 1e-6 and f.geometry.surfaceType == adsk.core.SurfaceTypes.PlaneSurfaceType:
      return f.createForAssemblyContext(occ)
  raise RuntimeError("no face at z=%s" % z)


occ_a, body_a = box_comp("A", 0, 0, 40, 30, 10)
occ_a.isGrounded = True
occ_b, body_b = box_comp("B", 50, 0, 60, 10, 10)

inp = root.assemblyConstraints.createInput()
inp.geometricRelationships.add(face_at(occ_b, body_b, 0), face_at(occ_a, body_a, 10), True,
                               adsk.core.ValueInput.createByString("5 mm"))
root.assemblyConstraints.add(inp)

for occ, body in ((occ_a, body_a), (occ_b, body_b)):
  body.createForAssemblyContext(occ).copyToComponent(root)
  body.isLightBulbOn = False
