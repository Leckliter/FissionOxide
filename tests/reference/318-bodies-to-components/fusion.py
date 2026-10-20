# New Component "from bodies": two root bodies (40x30x10 plate at x 0..40, y 0..30, and a
# 10x10x20 block at x 50..60, y 0..10) each turned into its own component (one per body).
P = adsk.core.Point3D.create
CM = 0.1


def box(x0, y0, x1, y1, h):
  sk = root.sketches.add(root.xYConstructionPlane)
  sk.sketchCurves.sketchLines.addTwoPointRectangle(P(x0 * CM, y0 * CM, 0), P(x1 * CM, y1 * CM, 0))
  ext = root.features.extrudeFeatures.addSimple(
    sk.profiles.item(0), adsk.core.ValueInput.createByReal(h * CM),
    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
  return ext.bodies.item(0)


plate = box(0, 0, 40, 30, 10)
block = box(50, 0, 60, 10, 20)
for b in (plate, block):
  b.createComponent()
if root.bRepBodies.count != 0 or root.occurrences.count != 2:
  raise RuntimeError("expected 2 components and no root bodies, got %d bodies, %d occurrences"
                     % (root.bRepBodies.count, root.occurrences.count))
