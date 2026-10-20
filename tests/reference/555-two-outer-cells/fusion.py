# Boundary fill: two 20 mm cubes overlapping 10 mm in X bound three cells; keep the two outer cells
# (x 0..10 and 20..30), not the middle, as new bodies; tools removed -> two 10x20x20 blocks.
P = adsk.core.Point3D.create
CM = 0.1
NEW = adsk.fusion.FeatureOperations.NewBodyFeatureOperation


def rect_body(x0, y0, x1, y1, h):
  sk = root.sketches.add(root.xYConstructionPlane)
  sk.sketchCurves.sketchLines.addTwoPointRectangle(P(x0 * CM, y0 * CM, 0), P(x1 * CM, y1 * CM, 0))
  return root.features.extrudeFeatures.addSimple(
    sk.profiles.item(0), adsk.core.ValueInput.createByReal(h * CM), NEW).bodies.item(0)


def cyl_body(cx, cy, r, h):
  sk = root.sketches.add(root.xYConstructionPlane)
  sk.sketchCurves.sketchCircles.addByCenterRadius(P(cx * CM, cy * CM, 0), r * CM)
  return root.features.extrudeFeatures.addSimple(
    sk.profiles.item(0), adsk.core.ValueInput.createByReal(h * CM), NEW).bodies.item(0)


def offset_xy(z):
  pi = root.constructionPlanes.createInput()
  pi.setByOffset(root.xYConstructionPlane, adsk.core.ValueInput.createByReal(z * CM))
  return root.constructionPlanes.add(pi)


def boundary_fill(tools, op, keep, remove):
  coll = adsk.core.ObjectCollection.create()
  for t in tools:
    coll.add(t)
  bf = root.features.boundaryFillFeatures
  inp = bf.createInput(coll, op)
  cells = inp.bRepCells
  for i in range(cells.count):
    c = cells.item(i)
    if keep is None:
      c.isSelected = True
    else:
      c.isSelected = any(
        c.cellBody.pointContainment(P(x * CM, y * CM, z * CM)) == adsk.fusion.PointContainment.PointInsidePointContainment
        for (x, y, z) in keep)
  inp.isRemoveTools = remove
  return bf.add(inp)


a = rect_body(0, 0, 20, 20, 20)
b = rect_body(10, 0, 30, 20, 20)
boundary_fill([a, b], NEW, [(5, 10, 10), (25, 10, 10)], True)
