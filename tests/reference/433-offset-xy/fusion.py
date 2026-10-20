# Joint origin on a vertex offset along its own X and Y. Grounded block A 20x20x10 at the origin;
# block B 10x6x4 at x 40..50, y 0..6. B gets a joint origin on its corner (40,0,0) offset 4 mm in X
# and 2 mm in Y (so at (44,2,0)). A rigid joint puts that origin on A's top corner (20,20,10).
# measure.py reads component bodies in their own frames, so the placed bodies are copied into the
# root and the originals hidden.
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


def vertex_at(body, p):
  for v in body.vertices:
    q = v.geometry
    if abs(q.x - p[0] * CM) < 1e-6 and abs(q.y - p[1] * CM) < 1e-6 and abs(q.z - p[2] * CM) < 1e-6:
      return v
  raise RuntimeError("no vertex at %s" % (p,))


def face_at(body, z):
  for f in body.faces:
    if abs(f.pointOnFace.z - z * CM) < 1e-6 and f.geometry.surfaceType == adsk.core.SurfaceTypes.PlaneSurfaceType:
      return f
  raise RuntimeError("no face at z=%s" % z)


def finish(pairs):
  for occ, body in pairs:
    body.createForAssemblyContext(occ).copyToComponent(root)
    body.isLightBulbOn = False


occ_a, body_a = box_comp("A", 0, 0, 20, 20, 10)
occ_a.isGrounded = True
occ_b, body_b = box_comp("B", 40, 0, 50, 6, 4)

oi = occ_b.component.jointOrigins.createInput(adsk.fusion.JointGeometry.createByPoint(vertex_at(body_b, (40, 0, 0))))
oi.offsetX = adsk.core.ValueInput.createByString("4 mm")
oi.offsetY = adsk.core.ValueInput.createByString("2 mm")
jo = occ_b.component.jointOrigins.add(oi)

geo = adsk.fusion.JointGeometry.createByPoint(vertex_at(body_a, (20, 20, 10)).createForAssemblyContext(occ_a))
ji = root.joints.createInput(jo.createForAssemblyContext(occ_b), geo)
ji.setAsRigidJointMotion()
root.joints.add(ji)

finish(((occ_a, body_a), (occ_b, body_b)))
