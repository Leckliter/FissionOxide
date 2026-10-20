# Cone: right triangle (30,0,0),(40,0,0),(30,0,15) on XZ revolved about its vertical edge (x=30).
# Axis through the cone's conical face; a 5x5 square x 45..50, z 0..5 on XZ revolved 360 deg
# about that axis (a separate ring body).
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
V = lambda s: adsk.core.ValueInput.createByString(s)
NEW = adsk.fusion.FeatureOperations.NewBodyFeatureOperation
rev = root.features.revolveFeatures
sk = root.sketches.add(root.xZConstructionPlane)
m = lambda x, z: sk.modelToSketchSpace(P(x, 0, z))
L = sk.sketchCurves.sketchLines
edge = L.addByTwoPoints(m(30, 0), m(30, 15))
a = L.addByTwoPoints(edge.endSketchPoint, m(40, 0))
L.addByTwoPoints(a.endSketchPoint, edge.startSketchPoint)
ri = rev.createInput(sk.profiles.item(0), edge, NEW)
ri.setAngleExtent(False, V("360 deg"))
cone = rev.add(ri).bodies.item(0)
face = [f for f in cone.faces if f.geometry.surfaceType == adsk.core.SurfaceTypes.ConeSurfaceType][0]
ai = root.constructionAxes.createInput()
ai.setByCircularFace(face)
axis = root.constructionAxes.add(ai)
sk2 = root.sketches.add(root.xZConstructionPlane)
sk2.sketchCurves.sketchLines.addTwoPointRectangle(sk2.modelToSketchSpace(P(45, 0, 0)), sk2.modelToSketchSpace(P(50, 0, 5)))
ri2 = rev.createInput(sk2.profiles.item(0), axis, NEW)
ri2.setAngleExtent(False, V("360 deg"))
rev.add(ri2)
