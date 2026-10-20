# 50x40x20 box centred on the origin in XY; mm DXF r6 circle at the origin inserted on the box's top face; disc cut 5 mm deep.
import os, tempfile
DXF = '0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n8\n0\n10\n0.0\n20\n0.0\n30\n0.0\n40\n6.0\n0\nENDSEC\n0\nEOF\n'
path = os.path.join(tempfile.gettempdir(), "foxprobe-on-box-top-face-cut.dxf")
with open(path, "w") as f:
  f.write(DXF)
bs = root.sketches.add(root.xYConstructionPlane)
bs.sketchCurves.sketchLines.addTwoPointRectangle(adsk.core.Point3D.create(-2.5, -2.0, 0), adsk.core.Point3D.create(2.5, 2.0, 0))
box = root.features.extrudeFeatures.addSimple(bs.profiles.item(0), adsk.core.ValueInput.createByReal(2.0), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
top = box.endFaces.item(0)
im = app.importManager
opts = im.createDXF2DImportOptions(path, top)
opts.position = adsk.core.Point2D.create(0.0, 0.0)
opts.isSingleSketchResult = True
im.importToTarget(opts, root)
sk = root.sketches.item(root.sketches.count - 1)
prof = sk.profiles.item(0)
ext = root.features.extrudeFeatures
ei = ext.createInput(prof, adsk.fusion.FeatureOperations.CutFeatureOperation)
ei.setDistanceExtent(True, adsk.core.ValueInput.createByReal(0.5))
ext.add(ei)
