# A wedge: the XZ profile (0,0)-(30,0)-(30,5)-(10,15)-(0,15) extruded 20 along +Y. Its slanted face
# runs from x=10,z=15 down to x=30,z=5; a sketch on a plane at z=30 projects that face, which lands
# foreshortened as the 20x20 rectangle x 10..30, y 0..20, extruded 5 mm as a new body.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
ext = root.features.extrudeFeatures
sk = root.sketches.add(root.xZConstructionPlane)
pts = [sk.modelToSketchSpace(P(x, 0, z)) for x, z in [(0, 0), (30, 0), (30, 5), (10, 15), (0, 15)]]
for i in range(len(pts)):
  sk.sketchCurves.sketchLines.addByTwoPoints(pts[i], pts[(i + 1) % len(pts)])
direction = adsk.fusion.ExtentDirections.PositiveExtentDirection
if sk.xDirection.crossProduct(sk.yDirection).y < 0:
  direction = adsk.fusion.ExtentDirections.NegativeExtentDirection
ei = ext.createInput(sk.profiles.item(0), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
ei.setOneSideExtent(adsk.fusion.DistanceExtentDefinition.create(adsk.core.ValueInput.createByReal(2.0)),
                    direction)
body = ext.add(ei).bodies.item(0)
slant = None
for f in body.faces:
  ok, n = f.evaluator.getNormalAtPoint(f.pointOnFace)
  if abs(n.x) > 0.1 and abs(n.z) > 0.1:
    slant = f
pi = root.constructionPlanes.createInput()
pi.setByOffset(root.xYConstructionPlane, adsk.core.ValueInput.createByReal(3.0))
sk2 = root.sketches.add(root.constructionPlanes.add(pi))
for c in [sk2.sketchCurves.item(i) for i in range(sk2.sketchCurves.count)]:
  c.deleteMe()
sk2.project2([slant], False)
if sk2.profiles.count != 1:
  raise RuntimeError("expected one profile from the projected face, got %d" % sk2.profiles.count)
ext.addSimple(sk2.profiles.item(0), adsk.core.ValueInput.createByReal(0.5),
              adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
