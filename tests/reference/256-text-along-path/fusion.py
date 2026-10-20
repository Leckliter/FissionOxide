# Sketch text "LL" in Arial, 5 mm tall, along a 20 mm radius arc (above the path, left aligned), extruded 2 mm.
sk = root.sketches.add(root.xYConstructionPlane)
arc = sk.sketchCurves.sketchArcs.addByCenterStartSweep(adsk.core.Point3D.create(0, 0, 0),
                                                      adsk.core.Point3D.create(2, 0, 0), 3.14159265358979 / 2)
arc.isConstruction = True
ti = sk.sketchTexts.createInput2("LL", 0.5)
ti.fontName = "Arial"
ti.setAsAlongPath(arc, True, adsk.core.HorizontalAlignments.LeftHorizontalAlignment, 0)
t = sk.sketchTexts.add(ti)
ext = root.features.extrudeFeatures.createInput(t, adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
ext.setDistanceExtent(False, adsk.core.ValueInput.createByReal(0.2))
root.features.extrudeFeatures.add(ext)
