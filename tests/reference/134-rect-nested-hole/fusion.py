# Two 2-point rectangles on XY: outer (-20,-15)-(20,15), inner picked (8,6)-(-4,-2) (12x8, off-centre).
# The region between them (the frame) extruded 5 mm: a plate with a rectangular hole.
P = lambda x, y: adsk.core.Point3D.create(x / 10, y / 10, 0)
VI = adsk.core.ValueInput.createByReal
design.fusionUnitsManager.distanceDisplayUnits = adsk.fusion.DistanceUnits.MillimeterDistanceUnits
sk = root.sketches.add(root.xYConstructionPlane)
L = sk.sketchCurves.sketchLines
L.addTwoPointRectangle(P(-20, -15), P(20, 15))
L.addTwoPointRectangle(P(8, 6), P(-4, -2))
prof = max((sk.profiles.item(i) for i in range(sk.profiles.count)),
           key=lambda p: p.areaProperties().area)
assert sk.profiles.count == 2
root.features.extrudeFeatures.addSimple(prof, VI(0.5), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
