# Angle Constraint (assembly): component B, a 20x20x4 plate, hinged by an edge-to-edge mate on the
# top x=40 edge of grounded component A (a 40x20x10 block), then tilted 30 deg between B's top face
# and A's top face. The posed bodies are copied into the root (world position) and the component
# bodies hidden so the measurement sees the assembly as placed.
import math
P = adsk.core.Point3D.create
M = adsk.core.Matrix3D.create

def box(comp, x0, y0, x1, y1, h):
  sk = comp.sketches.add(comp.xYConstructionPlane)
  sk.sketchCurves.sketchLines.addTwoPointRectangle(P(x0 / 10, y0 / 10, 0), P(x1 / 10, y1 / 10, 0))
  ext = comp.features.extrudeFeatures
  inp = ext.createInput(sk.profiles.item(0), adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
  inp.setDistanceExtent(False, adsk.core.ValueInput.createByReal(h / 10))
  return ext.add(inp).bodies.item(0)

def pick_face(body, pt):
  for f in body.faces:
    if f.boundingBox.contains(pt) and f.geometry.objectType == adsk.core.Plane.classType():
      _, n = f.evaluator.getNormalAtPoint(pt)
      if f.geometry.isCoPlanarTo(adsk.core.Plane.create(pt, n)) if hasattr(f.geometry, 'isCoPlanarTo') else True:
        return f
  raise RuntimeError('no face at %s' % pt.asArray())

def pick_edge(body, pt):
  best, bd = None, 1e9
  for e in body.edges:
    r = e.evaluator.getEndPoints()
    a, b = r[1], r[2]
    mid = P((a.x + b.x) / 2, (a.y + b.y) / 2, (a.z + b.z) / 2)
    d = mid.distanceTo(pt)
    if d < bd:
      best, bd = e, d
  return best

occA = root.occurrences.addNewComponent(M())
occA.component.name = 'A'
occB = root.occurrences.addNewComponent(M())
occB.component.name = 'B'
bodyA = box(occA.component, 0, 0, 40, 20, 10)
bodyB = box(occB.component, 40, 0, 60, 20, 4)
occA.isGrounded = True

pA = bodyA.createForAssemblyContext(occA)
pB = bodyB.createForAssemblyContext(occB)
faceA = pick_face(pA, P(2.0, 1.0, 1.0))     # A top, z=10
faceB = pick_face(pB, P(5.0, 1.0, 0.4))     # B top, z=4
edgeA = pick_edge(pA, P(4.0, 1.0, 1.0))     # A top edge at x=40
edgeB = pick_edge(pB, P(4.0, 1.0, 0.0))     # B bottom edge at x=40

cons = root.assemblyConstraints
inp = cons.createInput()
rels = inp.geometricRelationships
rels.add(edgeB, edgeA, True, adsk.core.ValueInput.createByReal(0))
rels.add(faceB, faceA, False, adsk.core.ValueInput.createByString('30 deg'))
c = cons.add(inp)
if c is None:
  raise RuntimeError('assembly constraint failed')

for occ, body in ((occA, bodyA), (occB, bodyB)):
  proxy = body.createForAssemblyContext(occ)
  proxy.copyToComponent(root)
  body.isLightBulbOn = False
