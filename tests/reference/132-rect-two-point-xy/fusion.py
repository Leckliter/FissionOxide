# 2-point rectangle on XY from (5,-10) to (45,15) (40x25), extruded 10 mm.
P = lambda x, y: adsk.core.Point3D.create(x / 10, y / 10, 0)
VI = adsk.core.ValueInput.createByReal
design.fusionUnitsManager.distanceDisplayUnits = adsk.fusion.DistanceUnits.MillimeterDistanceUnits
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(5, -10), P(45, 15))
root.features.extrudeFeatures.addSimple(sk.profiles.item(0), VI(1.0), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
