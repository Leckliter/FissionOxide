# ArrangeFeatures API, definition.globalRotation: one box component (40x15x8 at x 100..140) arranged
# by the rectangular solver on a 60x50 mm envelope on the XY plane, 2 mm spacing, no frame, with the
# 2D definition's global rotation set to "90 and 270 only" (the component's own rotation is left at
# its default, which follows the global). The placed copy under Arrange1 > Envelope1 is copied into
# the root (measure.py reads component bodies in their own frames) and the source body hidden.
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

feats = root.features.arrangeFeatures
inp = feats.createInput(adsk.fusion.ArrangeSolverTypes.Arrange2DRectangularSolverType)
env = inp.setPlaneEnvelope(root.xYConstructionPlane, V("60 mm"), V("50 mm"))
env.objectSpacing = V("2 mm")
env.frameWidth = V("0 mm")
inp.definition.globalRotation = adsk.fusion.ArrangeRotationTypes.Only90And270ArrangeRotationType
inp.arrangeComponents.add(occ)
feats.add(inp)

arranged = [o for o in root.allOccurrences if o.fullPathName.startswith("Arrange1")
            and o.component.bRepBodies.count]
assert len(arranged) == 1, [o.fullPathName for o in root.allOccurrences]
for o in arranged:
  o.component.bRepBodies.item(0).createForAssemblyContext(o).copyToComponent(root)
body.isLightBulbOn = False
