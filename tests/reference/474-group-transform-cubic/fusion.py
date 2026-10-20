# px SVG (width 160 height 100): a group translate(20,10) holding a closed path with a cubic Bezier top edge; imported on XY at the origin, scale 1; extruded 4 mm.
import os, tempfile
SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="160" height="100"><g transform="translate(20,10)"><path d="M 0 0 C 40 -20 80 20 120 0 L 120 60 L 0 60 Z" fill="none" stroke="black"/></g></svg>\n'
path = os.path.join(tempfile.gettempdir(), "foxprobe-group-transform-cubic.svg")
with open(path, "w") as f:
  f.write(SVG)

sk = root.sketches.add(root.xYConstructionPlane)
if not sk.importSVG(path, 0.0, 0.0, 1.0):  # x, y in cm
  raise RuntimeError("importSVG failed")
best = max((sk.profiles.item(i) for i in range(sk.profiles.count)),
           key=lambda p: p.areaProperties().area)
ext = root.features.extrudeFeatures
ext.addSimple(best, adsk.core.ValueInput.createByReal(0.4), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
