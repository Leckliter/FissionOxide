# User Coordinate System: a 40x30x10 box; a UCS snapped to the box's top corner vertex (40,30,10),
# offset +5 mm in X and rotated 30 deg about Z. A 10x6 rectangle is sketched on the UCS XY plane
# from the UCS origin along its X/Y axes and extruded 4 mm (+UCS Z) as a new body.
P = adsk.core.Point3D.create
V = adsk.core.ValueInput.createByString
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0, 0), P(4, 3, 0))
ext = root.features.extrudeFeatures
inp = ext.createInput(sk.profiles.item(0), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal(1))
body = ext.add(inp).bodies.item(0)
vtx = [v for v in body.vertices if v.geometry.isEqualTo(P(4, 3, 1))][0]
ucss = root.userCoordinateSystems
ui = ucss.createInput(adsk.fusion.UserCoordinateSystemGeometry.createByPoint(vtx))
ui.offsetX = V('5 mm')
ui.angleZ = V('30 deg')
ucs = ucss.add(ui)
T = ucs.transform
sk2 = root.sketches.add(ucs.xYConstructionPlane)
def W(x, y):  # UCS-local mm -> sketch space
  p = P(x / 10, y / 10, 0)
  p.transformBy(T)
  return sk2.modelToSketchSpace(p)
sk2.sketchCurves.sketchLines.addTwoPointRectangle(W(0, 0), W(10, 6))
inp = ext.createInput(sk2.profiles.item(0), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal(0.4))
ext.add(inp)
