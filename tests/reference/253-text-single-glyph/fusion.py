# Sketch text "L" in Arial, 10 mm tall, frame corner at the origin (bottom-left aligned), extruded 2 mm.
sk = root.sketches.add(root.xYConstructionPlane)
ti = sk.sketchTexts.createInput2("L", 1.0)
ti.fontName = "Arial"
ti.setAsMultiLine(adsk.core.Point3D.create(0, 0, 0), adsk.core.Point3D.create(5, 2, 0),
                  adsk.core.HorizontalAlignments.LeftHorizontalAlignment,
                  adsk.core.VerticalAlignments.BottomVerticalAlignment, 0)
t = sk.sketchTexts.add(ti)
ext = root.features.extrudeFeatures.createInput(t, adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
ext.setDistanceExtent(False, adsk.core.ValueInput.createByReal(0.2))
root.features.extrudeFeatures.add(ext)
