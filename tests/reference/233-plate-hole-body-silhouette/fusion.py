# 30x20x5 plate with a r4 hole at (15,10); a sketch on a plane at z=20 projects the whole body
# (its silhouette seen along Z); the largest profile of the projection (the one holding the corner
# at (2,2)) is extruded 3 mm as a new body.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
ext = root.features.extrudeFeatures
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0), P(30, 20))
sk.sketchCurves.sketchCircles.addByCenterRadius(P(15, 10), 0.4)
prof = max(sk.profiles, key=lambda p: p.areaProperties().area)
plate = ext.addSimple(prof, adsk.core.ValueInput.createByReal(0.5),
                      adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
pi = root.constructionPlanes.createInput()
pi.setByOffset(root.xYConstructionPlane, adsk.core.ValueInput.createByReal(2.0))
sk2 = root.sketches.add(root.constructionPlanes.add(pi))
for c in [sk2.sketchCurves.item(i) for i in range(sk2.sketchCurves.count)]:
  c.deleteMe()
sk2.project2([plate.bodies.item(0)], False)
if sk2.profiles.count < 1:
  raise RuntimeError("the body silhouette made no profile")
prof2 = max(sk2.profiles, key=lambda p: p.areaProperties().area)
ext.addSimple(prof2, adsk.core.ValueInput.createByReal(0.3),
              adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
