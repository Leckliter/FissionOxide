# 40x20x10 box; on its top face 3 isoparametric curves distributed evenly in the U direction
# (Fusion: along X) -> lines y=5, 10, 15 split the face into four 40x5 strips. Strips 1 and 3
# (y 0-5 and 10-15) are extruded up 5 mm (join): 8000 + 2000 = 10000 mm3, 12 faces.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
ext = root.features.extrudeFeatures
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0), P(40, 20))
base = ext.addSimple(sk.profiles.item(0), adsk.core.ValueInput.createByReal(1.0),
                     adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
top = base.endFaces.item(0)
sk2 = root.sketches.add(top)  # the top face edges are projected automatically
iso = sk2.sketchCurves.sketchIsoparametricCurves
inp = iso.createInput(top)

inp.isUDirection = True
inp.setDistributed(adsk.core.ValueInput.createByReal(3))
iso.add(inp)
def pick(*pts):
  # profiles whose centroid is at the given model points (mm)
  col = adsk.core.ObjectCollection.create()
  for x, y, z in pts:
    q = sk2.modelToSketchSpace(P(x, y, z))
    for p in sk2.profiles:
      if p.areaProperties().centroid.distanceTo(q) < 1e-4:
        col.add(p)
  if col.count != len(pts):
    raise RuntimeError("expected %d profiles, found %d of %d" % (len(pts), col.count, sk2.profiles.count))
  return col

ext.addSimple(pick((20, 2.5, 10), (20, 12.5, 10)), adsk.core.ValueInput.createByReal(0.5),
              adsk.fusion.FeatureOperations.JoinFeatureOperation)
