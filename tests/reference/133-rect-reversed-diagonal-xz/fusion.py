# 2-point rectangle on the XZ plane picked from the upper-right corner to the lower-left:
# world (30, 0, 20) to (-10, 0, -5) (40 wide in X, 25 tall in Z), extruded symmetrically 6 mm each side.
VI = adsk.core.ValueInput.createByReal
design.fusionUnitsManager.distanceDisplayUnits = adsk.fusion.DistanceUnits.MillimeterDistanceUnits
sk = root.sketches.add(root.xZConstructionPlane)
W = lambda x, z: sk.modelToSketchSpace(adsk.core.Point3D.create(x / 10, 0, z / 10))
sk.sketchCurves.sketchLines.addTwoPointRectangle(W(30, 20), W(-10, -5))
ei = root.features.extrudeFeatures.createInput(sk.profiles.item(0), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
ei.setSymmetricExtent(VI(0.6), False)
root.features.extrudeFeatures.add(ei)
