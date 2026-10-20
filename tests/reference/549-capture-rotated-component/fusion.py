# Capture Position: a 20x10x5 box in its own component; the occurrence is turned 90 deg about Z
# (through the origin) and then moved (30, 0, 0), and the position captured as a timeline
# snapshot. The placed body is copied into the root and the original hidden, since measure.py
# reads component bodies in their own frames. Expect x 20..30, y 0..20, z 0..5.
import math
P = adsk.core.Point3D.create
CM = 0.1
occ = root.occurrences.addNewComponent(adsk.core.Matrix3D.create())
comp = occ.component
comp.name = "Box"
sk = comp.sketches.add(comp.xYConstructionPlane)
sk.sketchCurves.sketchLines.addTwoPointRectangle(P(0, 0, 0), P(20 * CM, 10 * CM, 0))
ext = comp.features.extrudeFeatures.addSimple(
  sk.profiles.item(0), adsk.core.ValueInput.createByReal(5 * CM),
  adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
body = ext.bodies.item(0)
m = adsk.core.Matrix3D.create()
m.setToRotation(math.pi / 2, adsk.core.Vector3D.create(0, 0, 1), P(0, 0, 0))
m.translation = adsk.core.Vector3D.create(30 * CM, 0, 0)
occ.isGroundToParent = False  # a lone occurrence is grounded to its parent and snaps back
occ.transform2 = m
design.snapshots.add()
body.createForAssemblyContext(occ).copyToComponent(root)
body.isLightBulbOn = False
