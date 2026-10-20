# Torus: circle r5 centred (40,0,10) on XZ revolved about the vertical line x=20 (R20, r5).
# A 6x6x6 box x 50..56, y -3..3 on XY; axis through the torus face; circular pattern of the box
# body x4 over 360 deg about that axis.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
V = lambda s: adsk.core.ValueInput.createByString(s)
NEW = adsk.fusion.FeatureOperations.NewBodyFeatureOperation
rev = root.features.revolveFeatures
sk = root.sketches.add(root.xZConstructionPlane)
m = lambda x, z: sk.modelToSketchSpace(P(x, 0, z))
line = sk.sketchCurves.sketchLines.addByTwoPoints(m(20, 0), m(20, 20))
line.isConstruction = True
sk.sketchCurves.sketchCircles.addByCenterRadius(m(40, 10), 0.5)
ri = rev.createInput(sk.profiles.item(0), line, NEW)
ri.setAngleExtent(False, V("360 deg"))
torus = rev.add(ri).bodies.item(0)
face = [f for f in torus.faces if f.geometry.surfaceType == adsk.core.SurfaceTypes.TorusSurfaceType][0]
sk2 = root.sketches.add(root.xYConstructionPlane)
sk2.sketchCurves.sketchLines.addTwoPointRectangle(P(50, -3), P(56, 3))
box = root.features.extrudeFeatures.addSimple(sk2.profiles.item(0), V("6 mm"), NEW).bodies.item(0)
ai = root.constructionAxes.createInput()
ai.setByCircularFace(face)
axis = root.constructionAxes.add(ai)
ents = adsk.core.ObjectCollection.create()
ents.add(box)
pi = root.features.circularPatternFeatures.createInput(ents, axis)
pi.quantity = adsk.core.ValueInput.createByReal(4)
pi.totalAngle = V("360 deg")
root.features.circularPatternFeatures.add(pi)
