# r5 x 20 cylinder along +X (a circle on YZ at the origin, extruded 20); a sketch on a plane at z=20
# projects the body: its silhouette seen along Z is the 20x10 rectangle x 0..20, y -5..5, whose long
# sides no edge of the cylinder traces. The rectangle is extruded 3 mm as a new body.
ext = root.features.extrudeFeatures
sk = root.sketches.add(root.yZConstructionPlane)
sk.sketchCurves.sketchCircles.addByCenterRadius(adsk.core.Point3D.create(0, 0, 0), 0.5)
cyl = ext.addSimple(sk.profiles.item(0), adsk.core.ValueInput.createByReal(2.0),
                    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
pi = root.constructionPlanes.createInput()
pi.setByOffset(root.xYConstructionPlane, adsk.core.ValueInput.createByReal(2.0))
sk2 = root.sketches.add(root.constructionPlanes.add(pi))
for c in [sk2.sketchCurves.item(i) for i in range(sk2.sketchCurves.count)]:
  c.deleteMe()
sk2.project2([cyl.bodies.item(0)], False)
if sk2.profiles.count != 1:
  raise RuntimeError("expected one silhouette profile, got %d" % sk2.profiles.count)
ext.addSimple(sk2.profiles.item(0), adsk.core.ValueInput.createByReal(0.3),
              adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
