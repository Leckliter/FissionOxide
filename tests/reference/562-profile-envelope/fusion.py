# ArrangeFeatures API, setProfileOrFaceEnvelope: a box component (40x15x8 at x 100..140) arranged by
# the rectangular solver into a sketch profile used as the envelope (a 50x30 rectangle on the root
# XY plane at x -20..30, y 30..60), 2 mm spacing, no frame. The placed copy under Arrange1 >
# Envelope1 is copied into the root and the source body hidden.
P = adsk.core.Point3D.create
V = adsk.core.ValueInput.createByString
CM = 0.1

occ = root.occurrences.addNewComponent(adsk.core.Matrix3D.create())
comp = occ.component
comp.name = "A"
sk = comp.sketches.add(comp.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(100 * CM, 0, 0), P(140 * CM, 15 * CM, 0))
ext = comp.features.extrudeFeatures.addSimple(
  sk.profiles.item(0), adsk.core.ValueInput.createByReal(8 * CM),
  adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
body = ext.bodies.item(0)

env_sk = root.sketches.add(root.xYConstructionPlane)
env_sk.sketchCurves.sketchLines.addTwoPointRectangle(P(-20 * CM, 30 * CM, 0), P(30 * CM, 60 * CM, 0))

feats = root.features.arrangeFeatures
inp = feats.createInput(adsk.fusion.ArrangeSolverTypes.Arrange2DRectangularSolverType)
env = inp.setProfileOrFaceEnvelope([env_sk.profiles.item(0)])
env.objectSpacing = V("2 mm")
env.frameWidth = V("0 mm")
inp.arrangeComponents.add(occ)
feats.add(inp)

arranged = [o for o in root.allOccurrences if o.fullPathName.startswith("Arrange1")
            and o.component.bRepBodies.count]
assert len(arranged) == 1, [o.fullPathName for o in root.allOccurrences]
for o in arranged:
  o.component.bRepBodies.item(0).createForAssemblyContext(o).copyToComponent(root)
body.isLightBulbOn = False
