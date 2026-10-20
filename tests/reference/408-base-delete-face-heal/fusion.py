# Create Base Feature: inside a base feature, a 40x30x20 box gets a 10x10x5 pocket cut into its
# top face (x 15..25, y 10..20, z 15..20), then the pocket's 5 faces are deleted with a direct
# delete-face edit, healing the box back to a plain 40x30x20 block. The base feature is finished.
P = adsk.core.Point3D.create
CM = 0.1
bf = root.features.baseFeatures.add()
bf.startEdit()
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0, 0), P(40 * CM, 30 * CM, 0))
ext = root.features.extrudeFeatures.addSimple(
  sk.profiles.item(0), adsk.core.ValueInput.createByReal(20 * CM),
  adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
pl = root.constructionPlanes.createInput()
pl.setByOffset(root.xYConstructionPlane, adsk.core.ValueInput.createByReal(20 * CM))
top = root.constructionPlanes.add(pl)
sk2 = root.sketches.add(top)
sk2.sketchCurves.sketchLines.addTwoPointRectangle(P(15 * CM, 10 * CM, 0), P(25 * CM, 20 * CM, 0))
root.features.extrudeFeatures.addSimple(
  sk2.profiles.item(0), adsk.core.ValueInput.createByReal(-5 * CM),
  adsk.fusion.FeatureOperations.CutFeatureOperation)
body = ext.bodies.item(0)
faces = adsk.core.ObjectCollection.create()
for f in body.faces:
  p = f.pointOnFace
  if 15 * CM - 1e-6 <= p.x <= 25 * CM + 1e-6 and 10 * CM - 1e-6 <= p.y <= 20 * CM + 1e-6 \
      and 15 * CM - 1e-6 <= p.z <= 20 * CM - 1e-6:
    faces.add(f)
assert faces.count == 5, faces.count
root.features.deleteFaceFeatures.add(faces)
bf.finishEdit()
