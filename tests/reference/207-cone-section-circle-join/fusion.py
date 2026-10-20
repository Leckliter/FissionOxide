# Cone (base r10 on XY, apex at z=20) revolved from a triangle on XZ. A sketch on a construction
# plane at z=10 intersects the cone body (Intersect): an r5 circle. That circle is extruded 10 mm
# up, joined: the cone with an r5 post from z=10 to z=20.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
sk = root.sketches.add(root.xZConstructionPlane)
L = sk.sketchCurves.sketchLines
a, b, c = [sk.modelToSketchSpace(P(*w)) for w in ((0, 0, 0), (10, 0, 0), (0, 0, 20))]
l1 = L.addByTwoPoints(a, b)
l2 = L.addByTwoPoints(l1.endSketchPoint, c)
l3 = L.addByTwoPoints(l2.endSketchPoint, l1.startSketchPoint)
ri = root.features.revolveFeatures.createInput(sk.profiles.item(0), root.zConstructionAxis,
                                               adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
ri.setAngleExtent(False, adsk.core.ValueInput.createByString("360 deg"))
cone = root.features.revolveFeatures.add(ri).bodies.item(0)
pi = root.constructionPlanes.createInput()
pi.setByOffset(root.xYConstructionPlane, adsk.core.ValueInput.createByReal(1.0))
sk2 = root.sketches.add(root.constructionPlanes.add(pi))
got = sk2.intersectWithSketchPlane([cone])
if not got or sk2.profiles.count != 1:
  raise RuntimeError("expected one profile from the intersection, got %d" % sk2.profiles.count)
root.features.extrudeFeatures.addSimple(sk2.profiles.item(0), adsk.core.ValueInput.createByReal(1.0),
                                        adsk.fusion.FeatureOperations.JoinFeatureOperation)
