# ArrangeFeatures API, per-component rotationType "180 only" with the true-shape solver: an L-plate
# component (50x40 outline with a 30x25 notch cut from one corner, 6 mm thick, at x 300..350) and a
# second instance of it 100 mm up in Y, arranged on a 110x60 mm envelope on the XY plane, 3 mm
# spacing, no frame. The placed copies under Arrange1 > Envelope1 are copied into the root and the
# source body hidden.
P = adsk.core.Point3D.create
V = adsk.core.ValueInput.createByString
CM = 0.1

occ = root.occurrences.addNewComponent(adsk.core.Matrix3D.create())
comp = occ.component
comp.name = "L"
sk = comp.sketches.add(comp.xYConstructionPlane)
pts = [(300, 0), (350, 0), (350, 15), (320, 15), (320, 40), (300, 40)]
lines = sk.sketchCurves.sketchLines
for i in range(len(pts)):
  a, b = pts[i], pts[(i + 1) % len(pts)]
  lines.addByTwoPoints(P(a[0] * CM, a[1] * CM, 0), P(b[0] * CM, b[1] * CM, 0))
ext = comp.features.extrudeFeatures.addSimple(
  sk.profiles.item(0), adsk.core.ValueInput.createByReal(6 * CM),
  adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
body = ext.bodies.item(0)

feats = root.features.arrangeFeatures
inp = feats.createInput(adsk.fusion.ArrangeSolverTypes.Arrange2DTrueShapeSolverType)
env = inp.setPlaneEnvelope(root.xYConstructionPlane, V("110 mm"), V("60 mm"))
env.objectSpacing = V("3 mm")
env.frameWidth = V("0 mm")
m = adsk.core.Matrix3D.create()
m.translation = adsk.core.Vector3D.create(0, 100 * CM, 0)
occ2 = root.occurrences.addExistingComponent(comp, m)
for o in (occ, occ2):
  ac = inp.arrangeComponents.add(o)
  ac.rotationType = adsk.fusion.ArrangeRotationTypes.Only180ArrangeRotationType
feats.add(inp)

arranged = [o for o in root.allOccurrences if o.fullPathName.startswith("Arrange1")
            and o.component.bRepBodies.count]
assert len(arranged) == 2, [o.fullPathName for o in root.allOccurrences]
for o in arranged:
  o.component.bRepBodies.item(0).createForAssemblyContext(o).copyToComponent(root)
body.isLightBulbOn = False
