# Boundary fill cut: a 40x40x10 plate, a separate cylinder r=8 h=20 at (20,20) and a plane at z=5;
# the cylinder and plane bound two cells; the upper one (z 5..20) cuts the plate and the tools are
# removed -> plate with a 5 mm deep d=16 pocket.
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


rect_body(0, 0, 40, 40, 10)
c = cyl_body(20, 20, 8, 20)
pl = offset_xy(5)
boundary_fill([c, pl], adsk.fusion.FeatureOperations.CutFeatureOperation, [(20, 20, 12)], True)
