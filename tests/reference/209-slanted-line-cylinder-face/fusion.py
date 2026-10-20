# r10 h30 cylinder on XY. A sketch on XZ holds a slanted line from (x=-10,z=5) to (x=10,z=25);
# its Intersection Curve with the cylinder's side face is the full slanted ellipse (front and back
# halves). The Fusion API has no IntersectionCurve call, so this uses Project to Surface along the
# sketch normal (Y), which is the same curve: the line extruded along its sketch normal, cut by the
# face. A d2 pipe is run along the resulting 3D curves as a new body.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
ext = root.features.extrudeFeatures
sk = root.sketches.add(root.xYConstructionPlane)
sk.sketchCurves.sketchCircles.addByCenterRadius(P(0, 0), 1.0)
cyl = ext.addSimple(sk.profiles.item(0), adsk.core.ValueInput.createByReal(3.0),
                    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
body = cyl.bodies.item(0)
side = [f for f in body.faces if f.geometry.surfaceType == adsk.core.SurfaceTypes.CylinderSurfaceType][0]
lsk = root.sketches.add(root.xZConstructionPlane)
line = lsk.sketchCurves.sketchLines.addByTwoPoints(lsk.modelToSketchSpace(P(-10, 0, 5)),
                                                   lsk.modelToSketchSpace(P(10, 0, 25)))
isk = root.sketches.add(root.xZConstructionPlane)
res = isk.projectToSurface([side], [line], adsk.fusion.SurfaceProjectTypes.AlongVectorSurfaceProjectType,
                           root.yConstructionAxis)
curves = adsk.core.ObjectCollection.create()
for c in res:
  curves.add(c)
path = root.features.createPath(curves, True)
pipes = root.features.pipeFeatures
pi = pipes.createInput(path, adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
pi.sectionSize = adsk.core.ValueInput.createByReal(0.2)
pipes.add(pi)
