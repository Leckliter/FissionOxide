# Create Base Feature: a 40x30x20 box is extruded inside a base feature and its top face is
# pulled up 5 mm with a direct offset-face edit (box now 25 tall); the base feature is finished,
# then a parametric 10 mm diameter hole is cut through all at (20, 15).
P = adsk.core.Point3D.create
CM = 0.1
bf = root.features.baseFeatures.add()
bf.startEdit()
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0, 0), P(40 * CM, 30 * CM, 0))
ext = root.features.extrudeFeatures.addSimple(
  sk.profiles.item(0), adsk.core.ValueInput.createByReal(20 * CM),
  adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
body = ext.bodies.item(0)
top = [f for f in body.faces if abs(f.pointOnFace.z - 20 * CM) < 1e-6][0]
oi = root.features.offsetFacesFeatures.createInput([top], adsk.core.ValueInput.createByReal(5 * CM))
root.features.offsetFacesFeatures.add(oi)
bf.finishEdit()
sk2 = root.sketches.add(root.xYConstructionPlane)
sk2.sketchCurves.sketchCircles.addByCenterRadius(P(20 * CM, 15 * CM, 0), 5 * CM)
ei = root.features.extrudeFeatures.createInput(
  sk2.profiles.item(0), adsk.fusion.FeatureOperations.CutFeatureOperation)
ei.setAllExtent(adsk.fusion.ExtentDirections.PositiveExtentDirection)
root.features.extrudeFeatures.add(ei)
