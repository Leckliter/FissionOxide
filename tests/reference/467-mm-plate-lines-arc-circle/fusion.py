# mm DXF (INSUNITS 4): 40x30 plate with R5 rounded corner and r6 hole at (12,15), inserted on XY at the origin; plate region extruded 8 mm.
import os, tempfile
DXF = '0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0.0\n20\n0.0\n30\n0.0\n11\n40.0\n21\n0.0\n31\n0.0\n0\nLINE\n8\n0\n10\n40.0\n20\n0.0\n30\n0.0\n11\n40.0\n21\n25.0\n31\n0.0\n0\nARC\n8\n0\n10\n35.0\n20\n25.0\n30\n0.0\n40\n5.0\n50\n0.0\n51\n90.0\n0\nLINE\n8\n0\n10\n35.0\n20\n30.0\n30\n0.0\n11\n0.0\n21\n30.0\n31\n0.0\n0\nLINE\n8\n0\n10\n0.0\n20\n30.0\n30\n0.0\n11\n0.0\n21\n0.0\n31\n0.0\n0\nCIRCLE\n8\n0\n10\n12.0\n20\n15.0\n30\n0.0\n40\n6.0\n0\nENDSEC\n0\nEOF\n'
path = os.path.join(tempfile.gettempdir(), "foxprobe-mm-plate-lines-arc-circle.dxf")
with open(path, "w") as f:
  f.write(DXF)

im = app.importManager
opts = im.createDXF2DImportOptions(path, root.xYConstructionPlane)
opts.position = adsk.core.Point2D.create(0.0, 0.0)
opts.isSingleSketchResult = True
im.importToTarget(opts, root)
sk = root.sketches.item(root.sketches.count - 1)
best = max((sk.profiles.item(i) for i in range(sk.profiles.count)),
           key=lambda p: p.areaProperties().area)
ext = root.features.extrudeFeatures
ext.addSimple(best, adsk.core.ValueInput.createByReal(0.8), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
