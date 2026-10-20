# Revert Position (Design > SOLID > Position > Revert Position): a 40x30x20 box in the root and a
# 10x10x30 pin in a component, its occurrence overlapping the box. The occurrence is moved
# (translate 100 mm in X and rotate 90 deg about Z) without capturing the position, so the design
# holds a pending snapshot; Revert Position (Snapshots.revertPendingSnapshot) puts the pin back.
# The pin's world position is then made visible in the measurement by cutting it from the box
# (keep tools): reverted -> a 10x10x20 pocket through the box; not reverted -> no cut at all.
CM = 0.1
V = adsk.core.Point3D.create

def box(comp, x0, y0, x1, y1, h):
  sk = comp.sketches.add(comp.xYConstructionPlane)
  sk.sketchCurves.sketchLines.addTwoPointRectangle(V(x0 * CM, y0 * CM, 0), V(x1 * CM, y1 * CM, 0))
  ext = comp.features.extrudeFeatures.addSimple(
    sk.profiles.item(0), adsk.core.ValueInput.createByReal(h * CM),
    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
  return ext.bodies.item(0)

design.designType = adsk.fusion.DesignTypes.ParametricDesignType
target = box(root, 0, 0, 40, 30, 20)
occ = root.occurrences.addNewComponent(adsk.core.Matrix3D.create())
occ.component.name = "Pin"
pin = box(occ.component, 10, 10, 20, 20, 30)

m = adsk.core.Matrix3D.create()
m.setToRotation(1.5707963267948966, adsk.core.Vector3D.create(0, 0, 1), V(0, 0, 0))
m.translation = adsk.core.Vector3D.create(100 * CM, 0, 0)
occ.transform2 = m
snaps = design.snapshots
if not snaps.hasPendingSnapshot:
  raise RuntimeError("moving the occurrence left no pending snapshot")
if not snaps.revertPendingSnapshot():
  raise RuntimeError("revertPendingSnapshot failed")
if snaps.hasPendingSnapshot:
  raise RuntimeError("pending snapshot still there after revert")

tools = adsk.core.ObjectCollection.create()
tools.add(pin.createForAssemblyContext(occ))
ci = root.features.combineFeatures.createInput(target, tools)
ci.operation = adsk.fusion.FeatureOperations.CutFeatureOperation
ci.isKeepToolBodies = True
root.features.combineFeatures.add(ci)
