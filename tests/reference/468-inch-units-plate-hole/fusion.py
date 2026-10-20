# inch DXF (INSUNITS 1): 2x1 in plate with r0.25 in hole at (0.5,0.5) in, inserted on XY at the origin; plate region extruded 5 mm.
import os, tempfile
DXF = '0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n1\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0.0\n20\n0.0\n30\n0.0\n11\n2.0\n21\n0.0\n31\n0.0\n0\nLINE\n8\n0\n10\n2.0\n20\n0.0\n30\n0.0\n11\n2.0\n21\n1.0\n31\n0.0\n0\nLINE\n8\n0\n10\n2.0\n20\n1.0\n30\n0.0\n11\n0.0\n21\n1.0\n31\n0.0\n0\nLINE\n8\n0\n10\n0.0\n20\n1.0\n30\n0.0\n11\n0.0\n21\n0.0\n31\n0.0\n0\nCIRCLE\n8\n0\n10\n0.5\n20\n0.5\n30\n0.0\n40\n0.25\n0\nENDSEC\n0\nEOF\n'
path = os.path.join(tempfile.gettempdir(), "foxprobe-inch-units-plate-hole.dxf")
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
ext.addSimple(best, adsk.core.ValueInput.createByReal(0.5), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
