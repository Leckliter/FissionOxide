# Two 20 mm cubes overlapping by 10 mm in X; Interference with "Create components from
# interferences" adds the 10x20x20 overlap as a body in a new component.
# Fusion keeps interference bodies only in a direct (non-parametric) design.
design.designType = adsk.fusion.DesignTypes.DirectDesignType
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
res.createBodies(True)
# back to parametric so the measurement can read a timeline (the bodies become base features)
design.designType = adsk.fusion.DesignTypes.ParametricDesignType
