# Rectangle (0,0)-(20,10) with a 20 mm horizontal and 10 mm vertical dimension. Sketch Scale of all
# its curves about the origin by 1.5, then extrude 5. Fusion is the answer key for whether the
# driving dimensions follow the scale (30x15) or pull the geometry back (20x10).
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
sk = root.sketches.add(root.xYConstructionPlane)
r = sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0), P(20, 10))
dims = sk.sketchDimensions
H = adsk.fusion.DimensionOrientations.HorizontalDimensionOrientation
V = adsk.fusion.DimensionOrientations.VerticalDimensionOrientation
dims.addDistanceDimension(r.item(0).startSketchPoint, r.item(0).endSketchPoint, H, P(10, -5))
dims.addDistanceDimension(r.item(1).startSketchPoint, r.item(1).endSketchPoint, V, P(25, 5))
ents = [r.item(i) for i in range(r.count)]
if not sk.scale(ents, P(0, 0), 1.5):
  raise RuntimeError("Sketch.scale failed")
prof = max(sk.profiles, key=lambda p: p.areaProperties().area)
root.features.extrudeFeatures.addSimple(prof, adsk.core.ValueInput.createByReal(0.5),
                                        adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
