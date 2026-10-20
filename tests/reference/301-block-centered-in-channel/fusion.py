# Center (midplane) assembly constraint. Grounded U-channel A (outer x 0..50, y 0..20, inner faces at
# x=10 and x=40, slot y 5..20, 10 tall) and a free 10x10x10 block B at x 60..70, y 10..20. B is
# centred between A's two inner faces, so it moves to x 20..30. The API has no Center constraint
# type, so it is expressed as its definition: B's midplane (between its x faces) mated, offset 0,
# to A's midplane (between A's inner faces). measure.py reads component bodies in their own frames,
# so the placed bodies are copied into the root and the originals hidden.
P = adsk.core.Point3D.create
CM = 0.1


def comp_with(name, pts, h):
  occ = root.occurrences.addNewComponent(adsk.core.Matrix3D.create())
  comp = occ.component
  comp.name = name
  sk = comp.sketches.add(comp.xYConstructionPlane)
  lines = sk.sketchCurves.sketchLines
  for i in range(len(pts)):
    a, b = pts[i], pts[(i + 1) % len(pts)]
    lines.addByTwoPoints(P(a[0] * CM, a[1] * CM, 0), P(b[0] * CM, b[1] * CM, 0))
  ext = comp.features.extrudeFeatures.addSimple(
    sk.profiles.item(0), adsk.core.ValueInput.createByReal(h * CM),
    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
  return occ, comp, ext.bodies.item(0)


def face_at_x(body, x):
  for f in body.faces:
    if f.geometry.surfaceType == adsk.core.SurfaceTypes.PlaneSurfaceType and \
       abs(f.pointOnFace.x - x * CM) < 1e-6 and abs(abs(f.geometry.normal.x) - 1) < 1e-9:
      return f
  raise RuntimeError("no face at x=%s" % x)


def midplane(comp, body, x0, x1):
  pi = comp.constructionPlanes.createInput()
  pi.setByTwoPlanes(face_at_x(body, x0), face_at_x(body, x1))
  return comp.constructionPlanes.add(pi)


occ_a, comp_a, body_a = comp_with("A", [(0, 0), (50, 0), (50, 20), (40, 20), (40, 5), (10, 5), (10, 20), (0, 20)], 10)
occ_a.isGrounded = True
occ_b, comp_b, body_b = comp_with("B", [(60, 10), (70, 10), (70, 20), (60, 20)], 10)

mid_a = midplane(comp_a, body_a, 10, 40).createForAssemblyContext(occ_a)
mid_b = midplane(comp_b, body_b, 60, 70).createForAssemblyContext(occ_b)

inp = root.assemblyConstraints.createInput()
inp.geometricRelationships.add(mid_b, mid_a, True, adsk.core.ValueInput.createByString("0 mm"))
root.assemblyConstraints.add(inp)

for occ, body in ((occ_a, body_a), (occ_b, body_b)):
  body.createForAssemblyContext(occ).copyToComponent(root)
  body.isLightBulbOn = False
