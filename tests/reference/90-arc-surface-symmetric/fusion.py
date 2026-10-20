# Quarter-circle arc r=20 about the origin on XY, extruded 30 mm as a surface; thickened 2 mm
# symmetrically (Fusion: 2 mm each side) into a new body (quarter tube r 18..22), the surface then removed.
P = lambda x, y, z=0: adsk.core.Point3D.create(x / 10, y / 10, z / 10)
VI = adsk.core.ValueInput.createByReal
design.fusionUnitsManager.distanceDisplayUnits = adsk.fusion.DistanceUnits.MillimeterDistanceUnits
ext = root.features.extrudeFeatures
sk = root.sketches.add(root.xYConstructionPlane)
arc = sk.sketchCurves.sketchArcs.addByThreePoints(P(20, 0), P(14.142135623730951, 14.142135623730951), P(0, 20))
inp = ext.createInput(root.createOpenProfile(arc, False), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
inp.isSolid = False
inp.setDistanceExtent(False, VI(3.0))
surf = ext.add(inp).bodies.item(0)
faces = adsk.core.ObjectCollection.create()
for f in surf.faces:
  faces.add(f)
ti = root.features.thickenFeatures.createInput(faces, adsk.core.ValueInput.createByString("2 mm"), True,
                                               adsk.fusion.FeatureOperations.NewBodyFeatureOperation, False)
tf = root.features.thickenFeatures.add(ti)
# the API doubles a symmetric thickness given to createInput (2 mm in, a 4.00 mm parameter out);
# set the parameter the user types in the dialog: Thickness 2 mm, Direction Symmetric (2 mm each side)
tf.thickness.expression = "2 mm"
root.features.removeFeatures.add(surf)
