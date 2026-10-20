# Reopen a base feature that holds a direct edit: a 40x30x20 box is extruded and its top face
# pulled up 5 mm with a direct offset-face edit, both inside a base feature; it is finished and a
# parametric 3 mm fillet rounds the 4 top edges (now at z 25). The base feature is reopened
# (startEdit after the fillet), the top face pulled up another 5 mm, and finished: the fillet
# replays on the 30 mm tall box.
P = adsk.core.Point3D.create
CM = 0.1
def top_face(body, z):
  return [f for f in body.faces if abs(f.pointOnFace.z - z * CM) < 1e-6
          and abs(f.geometry.normal.z - 1) < 1e-9 if f.geometry.surfaceType == 0][0]
bf = root.features.baseFeatures.add()
bf.startEdit()
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0, 0), P(40 * CM, 30 * CM, 0))
ext = root.features.extrudeFeatures.addSimple(
  sk.profiles.item(0), adsk.core.ValueInput.createByReal(20 * CM),
  adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
oi = root.features.offsetFacesFeatures.createInput([top_face(ext.bodies.item(0), 20)],
                                                    adsk.core.ValueInput.createByReal(5 * CM))
root.features.offsetFacesFeatures.add(oi)
bf.finishEdit()
body = root.bRepBodies.item(0)
edges = adsk.core.ObjectCollection.create()
for e in body.edges:
  bb = e.boundingBox
  if abs(bb.minPoint.z - 25 * CM) < 1e-6 and abs(bb.maxPoint.z - 25 * CM) < 1e-6:
    edges.add(e)
fi = root.features.filletFeatures.createInput()
fi.edgeSetInputs.addConstantRadiusEdgeSet(edges, adsk.core.ValueInput.createByReal(3 * CM), False)
root.features.filletFeatures.add(fi)
bf.startEdit()
oi = root.features.offsetFacesFeatures.createInput([top_face(bf.bodies.item(0), 25)],
                                                    adsk.core.ValueInput.createByReal(5 * CM))
root.features.offsetFacesFeatures.add(oi)
bf.finishEdit()
