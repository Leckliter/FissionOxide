# r10 cylinder along Z, z=-20..20 (extruded symmetric from XY). A sketch on a plane through the
# origin tilted 30 deg from XY about the X axis intersects the cylinder body (Intersect): an
# ellipse, semi-axes 10 and 10/cos30. That ellipse is extruded 5 mm along the plane's normal as a
# new body (an elliptic prism overlapping the cylinder): two bodies.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
ext = root.features.extrudeFeatures
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchCircles.addByCenterRadius(P(0, 0), 1.0)
ei = ext.createInput(sk.profiles.item(0), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
ei.setSymmetricExtent(adsk.core.ValueInput.createByReal(2.0), False)
cyl = ext.add(ei).bodies.item(0)
pi = root.constructionPlanes.createInput()
pi.setByAngle(root.xConstructionAxis, adsk.core.ValueInput.createByString("30 deg"),
              root.xYConstructionPlane)
sk2 = root.sketches.add(root.constructionPlanes.add(pi))
got = sk2.intersectWithSketchPlane([cyl])
if not got or sk2.profiles.count != 1:
  raise RuntimeError("expected one profile from the intersection, got %d" % sk2.profiles.count)
ext.addSimple(sk2.profiles.item(0), adsk.core.ValueInput.createByReal(0.5),
              adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
