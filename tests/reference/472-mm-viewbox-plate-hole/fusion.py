# SVG sized 40mm x 30mm with viewBox 0 0 40 30: rect 40x30 user units and r5 circle at (10,10); imported on XY at the origin, scale 1; plate-with-hole region extruded 5 mm.
import os, tempfile
SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="30mm" viewBox="0 0 40 30"><rect x="0" y="0" width="40" height="30" fill="none" stroke="black"/><circle cx="10" cy="10" r="5" fill="none" stroke="black"/></svg>\n'
path = os.path.join(tempfile.gettempdir(), "foxprobe-mm-viewbox-plate-hole.svg")
with open(path, "w") as f:
  f.write(SVG)

sk = root.sketches.add(root.xYConstructionPlane)
if not sk.importSVG(path, 0.0, 0.0, 1.0):  # x, y in cm
  raise RuntimeError("importSVG failed")
best = max((sk.profiles.item(i) for i in range(sk.profiles.count)),
           key=lambda p: p.areaProperties().area)
ext = root.features.extrudeFeatures
ext.addSimple(best, adsk.core.ValueInput.createByReal(0.5), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
