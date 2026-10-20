# px SVG (width 100 height 50, no viewBox): closed path with lines and a circular arc (A 15 15 sweep 1); imported on XY at x=10 mm, y=20 mm, scale 2; extruded 3 mm.
import os, tempfile
SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><path d="M 10 10 L 90 10 L 90 40 A 15 15 0 0 1 60 40 L 10 40 Z" fill="none" stroke="black"/></svg>\n'
path = os.path.join(tempfile.gettempdir(), "foxprobe-px-path-arc-offset-scale.svg")
with open(path, "w") as f:
  f.write(SVG)

sk = root.sketches.add(root.xYConstructionPlane)
if not sk.importSVG(path, 1.0, 2.0, 2.0):  # x, y in cm
  raise RuntimeError("importSVG failed")
best = max((sk.profiles.item(i) for i in range(sk.profiles.count)),
           key=lambda p: p.areaProperties().area)
ext = root.features.extrudeFeatures
ext.addSimple(best, adsk.core.ValueInput.createByReal(0.3), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
