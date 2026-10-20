# Cylinder boss r5 at (40,20), 10 tall; a 6x4x10 tab at x 52..58; axis through the boss's
# cylindrical face; circular pattern of the tab body x3 over 360 deg about that axis.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
V = lambda s: adsk.core.ValueInput.createByString(s)
NEW = adsk.fusion.FeatureOperations.NewBodyFeatureOperation
ext = root.features.extrudeFeatures
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchCircles.addByCenterRadius(P(40, 20), 0.5)
boss = ext.addSimple(sk.profiles.item(0), V("10 mm"), NEW).bodies.item(0)
sk2 = root.sketches.add(root.xYConstructionPlane)
sk2.sketchCurves.sketchLines.addTwoPointRectangle(P(52, 18), P(58, 22))
tab = ext.addSimple(sk2.profiles.item(0), V("10 mm"), NEW).bodies.item(0)
cyl = [f for f in boss.faces if f.geometry.surfaceType == adsk.core.SurfaceTypes.CylinderSurfaceType][0]
ai = root.constructionAxes.createInput()
ai.setByCircularFace(cyl)
axis = root.constructionAxes.add(ai)
ents = adsk.core.ObjectCollection.create()
ents.add(tab)
pi = root.features.circularPatternFeatures.createInput(ents, axis)
pi.quantity = adsk.core.ValueInput.createByReal(3)
pi.totalAngle = V("360 deg")
root.features.circularPatternFeatures.add(pi)
