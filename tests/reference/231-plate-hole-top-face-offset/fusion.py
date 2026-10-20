# 30x20x5 plate with a r4 hole at (15,10); a sketch on a plane at z=20 projects the plate's top face
# (its outer rectangle and the hole circle land flattened on the sketch); the ring profile between
# them is extruded 3 mm as a new body: two bodies, the second a 30x20x3 plate with a r4 hole at z=20..23.
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
top = plate.endFaces.item(0)
sk2.project2([top], False)
if sk2.profiles.count != 2:
  raise RuntimeError("expected 2 profiles from the projected face, got %d" % sk2.profiles.count)
ring = max(sk2.profiles, key=lambda p: p.areaProperties().area)
ext.addSimple(ring, adsk.core.ValueInput.createByReal(0.3),
              adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
