# A sketch on XZ holds a line from world (0,0,0) to (0,0,30); a sketch on XY includes that sketch
# line (Include 3D Geometry of a sketch curve) and a r5 circle at the origin; the circle is swept
# along the included line: a r5 x 30 cylinder.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
sk1 = root.sketches.add(root.xZConstructionPlane)
ln = sk1.sketchCurves.sketchLines.addByTwoPoints(sk1.modelToSketchSpace(P(0, 0, 0)),
                                                  sk1.modelToSketchSpace(P(0, 0, 30)))
sk2 = root.sketches.add(root.xYConstructionPlane)
inc = sk2.include(ln)
sk2.sketchCurves.sketchCircles.addByCenterRadius(P(0, 0), 0.5)
prof = None
for p in sk2.profiles:
  prof = p
path = root.features.createPath(inc.item(0), False)
si = root.features.sweepFeatures.createInput(prof, path,
                                             adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
root.features.sweepFeatures.add(si)
