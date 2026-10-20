# New Component, internal and empty, with a parent: an empty "Assy" under the root, then an empty
# "Part" whose parent is Assy; a 20x20x15 box with a 5 mm radius through hole at its centre is
# extruded inside Part (x 0..20, y 0..20).
P = adsk.core.Point3D.create
CM = 0.1

assy_occ = root.occurrences.addNewComponent(adsk.core.Matrix3D.create())
assy_occ.component.name = "Assy"
part_occ = assy_occ.component.occurrences.addNewComponent(adsk.core.Matrix3D.create())
part = part_occ.component
part.name = "Part"
sk = part.sketches.add(part.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0, 0), P(20 * CM, 20 * CM, 0))
sk.sketchCurves.sketchCircles.addByCenterRadius(P(10 * CM, 10 * CM, 0), 5 * CM)
prof = None
for i in range(sk.profiles.count):
  p = sk.profiles.item(i)
  if p.profileLoops.count == 2:
    prof = p
part.features.extrudeFeatures.addSimple(
  prof, adsk.core.ValueInput.createByReal(15 * CM),
  adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
if root.bRepBodies.count != 0 or part.bRepBodies.count != 1 or root.allOccurrences.count != 2:
  raise RuntimeError("unexpected structure")
