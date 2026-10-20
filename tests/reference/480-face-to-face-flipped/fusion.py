# Align (plane mode, faces facing): a 10x10x30 bar standing at x 40..50 has its +X side face
# (centre (50,5,15)) aligned onto the top face of a 20x20x10 box (centre (10,10,10)), normals
# opposed, so the bar lies down on the box. Fusion's API has no Align feature; this builds the same
# rigid transform Align makes (align the face's frame onto the target face's flipped frame) as a Move.
P = adsk.core.Point3D.create
D = adsk.core.Vector3D.create
V = adsk.core.ValueInput.createByReal

def box(x0, y0, x1, y1, h):
  sk = root.sketches.add(root.xYConstructionPlane)
  sk.sketchCurves.sketchLines.addTwoPointRectangle(P(x0 / 10, y0 / 10, 0), P(x1 / 10, y1 / 10, 0))
  ext = root.features.extrudeFeatures.addSimple(sk.profiles.item(0), V(h / 10),
    adsk.fusion.FeatureOperations.NewBodyFeatureOperation)
  return ext.bodies.item(0)

def face_at(body, x, y, z):
  # the planar face whose bounding-box centre is at the given point (mm)
  def centre(f):
    lo, hi = f.boundingBox.minPoint, f.boundingBox.maxPoint
    return P((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, (lo.z + hi.z) / 2)
  return min(body.faces, key=lambda f: centre(f).distanceTo(P(x / 10, y / 10, z / 10)))

a = box(0, 0, 20, 20, 10)
b = box(40, 0, 50, 10, 30)
src = face_at(b, 50, 5, 15)
dst = face_at(a, 10, 10, 10)
sn = src.geometry.normal
dn = dst.geometry.normal
dn.scaleBy(-1)  # faces facing each other
sx = D(0, 1, 0)
dx = D(0, 1, 0)
m = adsk.core.Matrix3D.create()
m.setToAlignCoordinateSystems(P(5.0, 0.5, 1.5), sx, sn.crossProduct(sx), sn,
                              P(1.0, 1.0, 1.0), dx, dn.crossProduct(dx), dn)
ents = adsk.core.ObjectCollection.create()
ents.add(b)
mi = root.features.moveFeatures.createInput2(ents)
mi.defineAsFreeMove(m)
root.features.moveFeatures.add(mi)
