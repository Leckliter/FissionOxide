# Align (point mode): a 10 mm cube's corner vertex is aligned to the top corner of a 20x20x10 box.
# Fusion's API has no Align feature; Align on bodies with points is a point-to-point Move, which is
# what this builds. Cube moves by (-20, 20, 10) from (40,0,0) to (20,20,10).
P = adsk.core.Point3D.create
V = adsk.core.ValueInput.createByReal

def box(x0, y0, x1, y1, h):
  sk = root.sketches.add(root.xYConstructionPlane)
  sk.sketchCurves.sketchLines.addTwoPointRectangle(P(x0 / 10, y0 / 10, 0), P(x1 / 10, y1 / 10, 0))
  ext = root.features.extrudeFeatures.addSimple(sk.profiles.item(0), V(h / 10),
    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
  return ext.bodies.item(0)

def vertex(body, x, y, z):
  for v in body.vertices:
    g = v.geometry
    if abs(g.x - x / 10) < 1e-6 and abs(g.y - y / 10) < 1e-6 and abs(g.z - z / 10) < 1e-6:
      return v
  raise Exception("no vertex at %s" % ((x, y, z),))

a = box(0, 0, 20, 20, 10)
b = box(40, 0, 50, 10, 10)
ents = adsk.core.ObjectCollection.create()
ents.add(b)
mi = root.features.moveFeatures.createInput2(ents)
mi.defineAsPointToPoint(vertex(b, 40, 0, 0), vertex(a, 20, 20, 10))
root.features.moveFeatures.add(mi)
