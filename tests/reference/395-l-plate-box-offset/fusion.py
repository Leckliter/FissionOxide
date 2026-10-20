# Bounding Solid (StockModelCommand), box shape, uniform 2 mm offset, around an L plate.
# Fusion's API has no bounding-solid feature (no adsk.fusion.*BoundingSolid* / StockModel input),
# so this script reproduces what the command makes: the L plate (x 0..40, y 0..30, z 0..8, legs
# 10 wide) plus a separate box body = the plate's axis-aligned bounding box grown 2 mm on every
# side (x -2..42, y -2..32, z -2..10). Both bodies stay visible, as after the command.
P = adsk.core.Point3D.create
CM = 0.1

sk = root.sketches.add(root.xYConstructionPlane)
pts = [(0, 0), (40, 0), (40, 10), (10, 10), (10, 30), (0, 30)]
lines = sk.sketchCurves.sketchLines
for i in range(len(pts)):
  a, b = pts[i], pts[(i + 1) % len(pts)]
  lines.addByTwoPoints(P(a[0] * CM, a[1] * CM, 0), P(b[0] * CM, b[1] * CM, 0))
ext = root.features.extrudeFeatures.addSimple(
  sk.profiles.item(0), adsk.core.ValueInput.createByReal(8 * CM),
  adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
plate = ext.bodies.item(0)

off = 2 * CM
bb = plate.boundingBox
mn, mx = bb.minPoint, bb.maxPoint
tb = adsk.fusion.TemporaryBRepManager.get()
c = P((mn.x + mx.x) / 2, (mn.y + mx.y) / 2, (mn.z + mx.z) / 2)
obb = adsk.core.OrientedBoundingBox3D.create(
  c, adsk.core.Vector3D.create(1, 0, 0), adsk.core.Vector3D.create(0, 1, 0),
  mx.x - mn.x + 2 * off, mx.y - mn.y + 2 * off, mx.z - mn.z + 2 * off)
box = tb.createBox(obb)
if design.designType == adsk.fusion.DesignTypes.ParametricDesignType:
  bf = root.features.baseFeatures.add()
  bf.startEdit()
  root.bRepBodies.add(box, bf)
  bf.finishEdit()
else:
  root.bRepBodies.add(box)
