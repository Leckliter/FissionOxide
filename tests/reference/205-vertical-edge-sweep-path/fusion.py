# 20x20x10 box; a sketch on XY includes the box's vertical edge at x=20,y=20 (it stays a 3D line
# from z=0 to z=10, off the sketch plane); a r3 circle at (20,20) on XY is swept along it, joined.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
ext = root.features.extrudeFeatures
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0), P(20, 20))
box = ext.addSimple(sk.profiles.item(0), adsk.core.ValueInput.createByReal(1.0),
                    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
body = box.bodies.item(0)
edge = None
for e in body.edges:
  a, b = e.startVertex.geometry, e.endVertex.geometry
  if all(abs(v.x - 2) < 1e-6 and abs(v.y - 2) < 1e-6 for v in (a, b)):
    edge = e
path_sk = root.sketches.add(root.xYConstructionPlane)
for c in [path_sk.sketchCurves.item(i) for i in range(path_sk.sketchCurves.count)]:
  c.deleteMe()
inc = path_sk.include(edge)
prof_sk = root.sketches.add(root.xYConstructionPlane)
prof_sk.sketchCurves.sketchCircles.addByCenterRadius(P(20, 20), 0.3)
path = root.features.createPath(inc.item(0), False)
si = root.features.sweepFeatures.createInput(prof_sk.profiles.item(0), path,
                                             adsk.fusion.FeatureOperations.JoinFeatureOperation)
root.features.sweepFeatures.add(si)
