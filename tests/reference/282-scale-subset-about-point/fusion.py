# Plate 40x20 with a square (2,4)-(10,16) and a r6 circle at (30,10) inside it. Sketch Scale of only
# the square and the circle about (20,10) by 0.5 (the plate outline stays), then extrude the plate
# region 5. Expected holes: square (11,7)-(15,13) and r3 circle at (25,10).
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
sk = root.sketches.add(root.xYConstructionPlane)
lines = sk.sketchCurves.sketchLines
lines.addTwoPointRectangle(P(0, 0), P(40, 20))
sq = lines.addTwoPointRectangle(P(2, 4), P(10, 16))
circ = sk.sketchCurves.sketchCircles.addByCenterRadius(P(30, 10), 0.6)
ents = [sq.item(i) for i in range(sq.count)] + [circ]
if not sk.scale(ents, P(20, 10), 0.5):
  raise RuntimeError("Sketch.scale failed")
prof = max(sk.profiles, key=lambda p: p.areaProperties().area)
root.features.extrudeFeatures.addSimple(prof, adsk.core.ValueInput.createByReal(0.5),
                                        adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
