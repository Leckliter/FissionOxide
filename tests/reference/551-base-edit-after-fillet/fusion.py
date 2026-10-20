# Create Base Feature, then edit it later: a 40x30x20 box is extruded inside a base feature, the
# base feature is finished and a parametric 3 mm fillet rounds the 4 top edges. The base feature
# is then reopened (startEdit after the fillet) and its top face pulled up 5 mm with a direct
# offset-face edit; finishing it replays the fillet on the now 25 mm tall box.
P = adsk.core.Point3D.create
CM = 0.1
bf = root.features.baseFeatures.add()
bf.startEdit()
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0, 0), P(40 * CM, 30 * CM, 0))
root.features.extrudeFeatures.addSimple(
  sk.profiles.item(0), adsk.core.ValueInput.createByReal(20 * CM),
  adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
bf.finishEdit()
body = root.bRepBodies.item(0)
edges = adsk.core.ObjectCollection.create()
for e in body.edges:
  bb = e.boundingBox
  if abs(bb.minPoint.z - 20 * CM) < 1e-6 and abs(bb.maxPoint.z - 20 * CM) < 1e-6:
    edges.add(e)
fi = root.features.filletFeatures.createInput()
fi.edgeSetInputs.addConstantRadiusEdgeSet(edges, adsk.core.ValueInput.createByReal(3 * CM), False)
root.features.filletFeatures.add(fi)
bf.startEdit()
body = bf.bodies.item(0)
top = [f for f in body.faces if abs(f.pointOnFace.z - 20 * CM) < 1e-6][0]
oi = root.features.offsetFacesFeatures.createInput([top], adsk.core.ValueInput.createByReal(5 * CM))
root.features.offsetFacesFeatures.add(oi)
bf.finishEdit()
