# Plate 20x10 with a r2 hole at (5,5); Sketch Scale of every curve about the sketch origin by 2,
# then extrude 5. Expected: 40x20x5 plate with a r4 hole at (10,10).
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0), P(20, 10))
sk.sketchCurves.sketchCircles.addByCenterRadius(P(5, 5), 0.2)
ents = [c for c in sk.sketchCurves]
if not sk.scale(ents, P(0, 0), 2.0):
  raise RuntimeError("Sketch.scale failed")
prof = max(sk.profiles, key=lambda p: p.areaProperties().area)
root.features.extrudeFeatures.addSimple(prof, adsk.core.ValueInput.createByReal(0.5),
                                        adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
