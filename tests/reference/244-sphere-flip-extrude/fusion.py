# Sphere r 5 centred (15, 0, 5). Spun Profile around the Z axis onto an XZ sketch with flipResult
# (other side of the axis), centerline not added: the silhouette circle r 5 at x=-15, z=5.
# Extruding it 2 mm symmetric as a new body gives a disc pi*25*2 = 157.08 mm3 beside the sphere.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
sk = root.sketches.add(root.xZConstructionPlane)
c = sk.modelToSketchSpace(P(15, 0, 5))
a0 = sk.modelToSketchSpace(P(15, 0, 0)); a1 = sk.modelToSketchSpace(P(15, 0, 10))
arc = sk.sketchCurves.sketchArcs.addByThreePoints(a0, sk.modelToSketchSpace(P(20, 0, 5)), a1)
axis = sk.sketchCurves.sketchLines.addByTwoPoints(arc.startSketchPoint, arc.endSketchPoint)
ri = root.features.revolveFeatures.createInput(sk.profiles.item(0), axis,
                                               adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
ri.setAngleExtent(False, adsk.core.ValueInput.createByString("360 deg"))
ball = root.features.revolveFeatures.add(ri).bodies.item(0)
sk2 = root.sketches.add(root.xZConstructionPlane)
si = sk2.createSpunProfileInput([ball], root.zConstructionAxis)
si.flipResult = True
si.isCenterlineAdded = False
sk2.createSpunProfile(si)
if sk2.profiles.count != 1:
  raise RuntimeError("expected one spun profile, got %d" % sk2.profiles.count)
ei = root.features.extrudeFeatures.createInput(sk2.profiles.item(0),
                                               adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
ei.setSymmetricExtent(adsk.core.ValueInput.createByReal(0.2), True)
root.features.extrudeFeatures.add(ei)
