# Sketch text "IO" in Arial, 10 mm tall, frame corner at the origin, extruded 2 mm: two glyph profiles, the O with a hole.
sk = root.sketches.add(root.xYConstructionPlane)
ti = sk.sketchTexts.createInput2("IO", 1.0)
ti.fontName = "Arial"
ti.setAsMultiLine(adsk.core.Point3D.create(0, 0, 0), adsk.core.Point3D.create(5, 2, 0),
                  adsk.core.HorizontalAlignments.LeftHorizontalAlignment,
                  adsk.core.VerticalAlignments.BottomVerticalAlignment, 0)
t = sk.sketchTexts.add(ti)
ext = root.features.extrudeFeatures.createInput(t, adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
ext.setDistanceExtent(False, adsk.core.ValueInput.createByReal(0.2))
root.features.extrudeFeatures.add(ext)
