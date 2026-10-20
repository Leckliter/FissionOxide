# ArrangeFeatures API, set3DEnvelope with the 3D solver: two box components, A 40x30x10 at
# x 100..140 and B 20x20x10 at x 200..220, y 50..70, arranged in a 45x35x40 mm 3D envelope on the XY
# plane with 2 mm object spacing. A takes the floor; B only fits by stacking or standing. The placed
# copies under Arrange1 > Envelope1 are copied into the root and the source bodies hidden.
P = adsk.core.Point3D.create
V = adsk.core.ValueInput.createByString
CM = 0.1


def box_comp(name, x0, y0, x1, y1, h):
  occ = root.occurrences.addNewComponent(adsk.core.Matrix3D.create())
  comp = occ.component
  comp.name = name
  sk = comp.sketches.add(comp.xYConstructionPlane)
  sk.sketchCurves.sketchLines.addTwoPointRectangle(P(x0 * CM, y0 * CM, 0), P(x1 * CM, y1 * CM, 0))
  ext = comp.features.extrudeFeatures.addSimple(
    sk.profiles.item(0), adsk.core.ValueInput.createByReal(h * CM),
    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
  return occ, ext.bodies.item(0)


parts = [box_comp("A", 100, 0, 140, 30, 10), box_comp("B", 200, 50, 220, 70, 10)]

feats = root.features.arrangeFeatures
inp = feats.createInput(adsk.fusion.ArrangeSolverTypes.Arrange3DSolverType)
env = inp.set3DEnvelope(root.xYConstructionPlane, V("45 mm"), V("35 mm"), V("40 mm"))
env.objectSpacing = V("2 mm")
for occ, _ in parts:
  inp.arrangeComponents.add(occ)
feats.add(inp)

arranged = [o for o in root.allOccurrences if o.fullPathName.startswith("Arrange1")
            and o.component.bRepBodies.count]
assert len(arranged) == 2, [o.fullPathName for o in root.allOccurrences]
for o in arranged:
  o.component.bRepBodies.item(0).createForAssemblyContext(o).copyToComponent(root)
for _, body in parts:
  body.isLightBulbOn = False
