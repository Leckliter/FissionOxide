# Box x 10..20, y -5..5, z 0..10. Spun Profile of the box around the Z axis onto an XZ sketch
# (defaults: not flipped, axis projected, centerline added) gives the rectangle r 10..sqrt(425), z 0..10.
# Revolving that profile 360 deg about Z with Join gives an annulus: pi*(425-100)*10 = 10210.2 mm3.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(10, -5), P(20, 5))
box = root.features.extrudeFeatures.addSimple(sk.profiles.item(0), adsk.core.ValueInput.createByReal(1.0),
      adsk.fusion.FeatureOperations.NewBodyFeatureOperation).bodies.item(0)
sk2 = root.sketches.add(root.xZConstructionPlane)
sk2.createSpunProfile(sk2.createSpunProfileInput([box], root.zConstructionAxis))
if sk2.profiles.count != 1:
  raise RuntimeError("expected one spun profile, got %d" % sk2.profiles.count)
ri = root.features.revolveFeatures.createInput(sk2.profiles.item(0), root.zConstructionAxis,
                                               adsk.fusion.FeatureOperations.JoinFeatureOperation)
ri.setAngleExtent(False, adsk.core.ValueInput.createByString("360 deg"))
root.features.revolveFeatures.add(ri)
