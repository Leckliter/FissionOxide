# 40x20x10 box; on its top face one isoparametric curve in the V direction (Fusion: along Y),
# physical offset 12 mm from the face edge -> line x=12. The 12x20 region it cuts off is cut
# 4 mm down: 8000 - 960 = 7040 mm3.
# (A real-valued physical ValueInput is read in inches here: 0.6 gave 15.24 mm, so use a string.)
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

inp.isUDirection = False
inp.setSingle(True, adsk.core.ValueInput.createByString("12 mm"))
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

ext.addSimple(pick((6, 10, 10)), adsk.core.ValueInput.createByReal(-0.4),
              adsk.fusion.FeatureOperations.CutFeatureOperation)
