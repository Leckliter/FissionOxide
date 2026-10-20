# Create Base Feature: a 40x30x20 box is extruded inside a base feature (history-free), the base
# feature is finished, then a parametric 3 mm fillet rounds the 4 top edges of the base body.
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
