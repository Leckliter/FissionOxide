# Two 20 mm cubes overlapping by 10 mm in X; Interference reports one 4000 mm3 overlap
# and, without "create components", leaves the two bodies alone.
def box(x0):
  sk = root.sketches.add(root.xYConstructionPlane)
  P = adsk.core.Point3D.create
  sk.sketchCurves.sketchLines.addTwoPointRectangle(P(x0 / 10, 0, 0), P((x0 + 20) / 10, 2, 0))
  ext = root.features.extrudeFeatures.addSimple(
    sk.profiles.item(0), adsk.core.ValueInput.createByString("20 mm"),
    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
  return ext.bodies.item(0)
a = box(0)
b = box(10)
coll = adsk.core.ObjectCollection.create()
coll.add(a)
coll.add(b)
inp = design.createInterferenceInput(coll)
res = design.analyzeInterference(inp)
assert res.count == 1, res.count
v = res.item(0).interferenceBody.volume * 1000.0
assert abs(v - 4000.0) < 1e-3, v
