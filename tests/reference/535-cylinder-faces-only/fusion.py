# cylinder-faces-only: cylinder r10 along Y (30 long, centred), silhouette split faces only (one body), view set by the XY plane
U = 0.1  # mm -> cm
sk = root.sketches.add(root.xZConstructionPlane)
sk.sketchCurves.sketchCircles.addByCenterRadius(adsk.core.Point3D.create(0, 0, 0), 10 * U)
ext = root.features.extrudeFeatures
inp = ext.createInput(sk.profiles.item(0), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
inp.setSymmetricExtent(adsk.core.ValueInput.createByReal(15 * U), False)
cyl = ext.add(inp)
body = cyl.bodies.item(0)
ss = root.features.silhouetteSplitFeatures
si = ss.createInput(root.xYConstructionPlane, body, adsk.fusion.SilhouetteSplitOperations.SilhouetteSplitFacesOnlyOperation)
ss.add(si)
