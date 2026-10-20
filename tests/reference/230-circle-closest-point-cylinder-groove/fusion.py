# r10 h30 cylinder on XY (axis Z). A sketch on the plane x=20 (YZ offset 20) holds an r5 circle
# centred at (y=0, z=15). Project To Surface onto the cylinder's side face, Closest Point:
# each point (20,y,z) lands on the nearest side point (10cos t, 10sin t, z), t = atan2(y, 20): one
# loop on the front only. A d2 pipe along the projected curve cuts one groove.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
ext = root.features.extrudeFeatures
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchCircles.addByCenterRadius(P(0, 0), 1.0)
cyl = ext.addSimple(sk.profiles.item(0), adsk.core.ValueInput.createByReal(3.0),
                    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
body = cyl.bodies.item(0)
side = [f for f in body.faces if f.geometry.surfaceType == adsk.core.SurfaceTypes.CylinderSurfaceType][0]
pi = root.constructionPlanes.createInput()
pi.setByOffset(root.yZConstructionPlane, adsk.core.ValueInput.createByReal(2.0))
csk = root.sketches.add(root.constructionPlanes.add(pi))
circle = csk.sketchCurves.sketchCircles.addByCenterRadius(csk.modelToSketchSpace(P(20, 0, 15)), 0.5)
psk = root.sketches.add(root.xZConstructionPlane)
res = psk.projectToSurface([side], [circle], adsk.fusion.SurfaceProjectTypes.ClosestPointSurfaceProjectType)
# The returned vector comes back empty for these closed loops; the curves are in the sketch.
res = list(psk.sketchCurves)
if len(res) == 0:
  raise RuntimeError("projectToSurface made no curves")
pipes = root.features.pipeFeatures
used = []
for c in res:
  if any(c == u for u in used):
    continue
  path = root.features.createPath(c, True)
  used += [path.item(i).entity for i in range(path.count)]
  pin = pipes.createInput(path, adsk.fusion.FeatureOperations.CutFeatureOperation)
  pin.sectionSize = adsk.core.ValueInput.createByReal(0.2)
  pipes.add(pin)
