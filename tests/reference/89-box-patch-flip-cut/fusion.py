# 40x30x20 box; a 20x20 patch surface on its top (sketch on a plane at z=20, x 10..30, y 5..25)
# thickened -5 mm (into the box) as a cut (a 20x20x5 pocket); the surface then removed.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
VI = adsk.core.ValueInput.createByReal
design.fusionUnitsManager.distanceDisplayUnits = adsk.fusion.DistanceUnits.MillimeterDistanceUnits
ext = root.features.extrudeFeatures
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0), P(40, 30))
box = ext.addSimple(sk.profiles.item(0), VI(2.0), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
pi = root.constructionPlanes.createInput()
pi.setByOffset(root.xYConstructionPlane, VI(2.0))
sk2 = root.sketches.add(root.constructionPlanes.add(pi))
sk2.sketchCurves.sketchLines.addTwoPointRectangle(P(10, 5), P(30, 25))
pin = root.features.patchFeatures.createInput(sk2.profiles.item(0), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
surf = root.features.patchFeatures.add(pin).bodies.item(0)
faces = adsk.core.ObjectCollection.create()
faces.add(surf.faces.item(0))
ti = root.features.thickenFeatures.createInput(faces, VI(-0.5), False,
                                               adsk.fusion.FeatureOperations.CutFeatureOperation, False)
root.features.thickenFeatures.add(ti)
if surf.isValid:
  root.features.removeFeatures.add(surf)
