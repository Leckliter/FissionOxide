# mm DXF plate (as mm-plate-lines-arc-circle) inserted on the XZ plane (sketch y = -Z) positioned at sketch (10,5) mm; plate region extruded 6 mm along +Y.
import os, tempfile
DXF = '0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0.0\n20\n0.0\n30\n0.0\n11\n40.0\n21\n0.0\n31\n0.0\n0\nLINE\n8\n0\n10\n40.0\n20\n0.0\n30\n0.0\n11\n40.0\n21\n25.0\n31\n0.0\n0\nARC\n8\n0\n10\n35.0\n20\n25.0\n30\n0.0\n40\n5.0\n50\n0.0\n51\n90.0\n0\nLINE\n8\n0\n10\n35.0\n20\n30.0\n30\n0.0\n11\n0.0\n21\n30.0\n31\n0.0\n0\nLINE\n8\n0\n10\n0.0\n20\n30.0\n30\n0.0\n11\n0.0\n21\n0.0\n31\n0.0\n0\nCIRCLE\n8\n0\n10\n12.0\n20\n15.0\n30\n0.0\n40\n6.0\n0\nENDSEC\n0\nEOF\n'
path = os.path.join(tempfile.gettempdir(), "foxprobe-xz-plane-position-offset.dxf")
with open(path, "w") as f:
  f.write(DXF)

im = app.importManager
opts = im.createDXF2DImportOptions(path, root.xZConstructionPlane)
opts.position = adsk.core.Point2D.create(1.0, 0.5)
opts.isSingleSketchResult = True
im.importToTarget(opts, root)
sk = root.sketches.item(root.sketches.count - 1)
best = max((sk.profiles.item(i) for i in range(sk.profiles.count)),
           key=lambda p: p.areaProperties().area)
ext = root.features.extrudeFeatures
ext.addSimple(best, adsk.core.ValueInput.createByReal(0.6), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
