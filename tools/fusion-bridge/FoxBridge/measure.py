"""Measures the active Fusion design in the units and shape of tests/reference expected.json.

Fusion works in cm internally; everything here is converted to mm, mm2 and mm3.
"""

import adsk.core
import adsk.fusion

CM = 10.0


def _bbox(app, body):
  # BRepBody.boundingBox can be loose on curved bodies; the measure manager's oriented box along
  # the world axes is tight.
  try:
    x = adsk.core.Vector3D.create(1, 0, 0)
    y = adsk.core.Vector3D.create(0, 1, 0)
    obb = app.measureManager.getOrientedBoundingBox(body, x, y)
    c = obb.centerPoint
    h = [obb.length / 2, obb.width / 2, obb.height / 2]
    lo = [c.x - h[0], c.y - h[1], c.z - h[2]]
    hi = [c.x + h[0], c.y + h[1], c.z + h[2]]
  except Exception:
    b = body.boundingBox
    lo = [b.minPoint.x, b.minPoint.y, b.minPoint.z]
    hi = [b.maxPoint.x, b.maxPoint.y, b.maxPoint.z]
  return [v * CM for v in lo], [v * CM for v in hi]


def _timeline(design):
  out = []
  tl = design.timeline
  for i in range(tl.count):
    item = tl.item(i)
    entity = item.entity
    health = getattr(item, "healthState", None)
    out.append({
      "name": item.name,
      "type": entity.objectType.split("::")[-1] if entity else None,
      "health": {0: "healthy", 1: "warning", 2: "error", 3: "unknown"}.get(health, health),
      "message": getattr(item, "errorOrWarningMessage", "") or "",
      "suppressed": item.isSuppressed,
    })
  return out


def bodies(app, design):
  out = []
  comps = [design.rootComponent] + [o.component for o in design.rootComponent.allOccurrences]
  seen = set()
  for comp in comps:
    if comp.entityToken in seen:
      continue
    seen.add(comp.entityToken)
    for i in range(comp.bRepBodies.count):
      b = comp.bRepBodies.item(i)
      if not b.isVisible and not b.isLightBulbOn:
        continue
      props = b.physicalProperties
      com = props.centerOfMass
      lo, hi = _bbox(app, b)
      out.append({
        "name": b.name,
        "component": comp.name,
        "solid": b.isSolid,
        "volume_mm3": b.volume * CM ** 3 if b.isSolid else 0.0,
        "area_mm2": b.area * CM ** 2,
        "center_of_mass": [com.x * CM, com.y * CM, com.z * CM],
        "bbox": {"min": lo, "max": hi},
        "faces": b.faces.count,
        "edges": b.edges.count,
      })
  return out


def summarize(bs):
  solids = [b for b in bs if b["solid"]]
  if not bs:
    return {"body_count": 0}
  vol = sum(b["volume_mm3"] for b in solids)
  com = [0.0, 0.0, 0.0]
  if vol > 0:
    for b in solids:
      for k in range(3):
        com[k] += b["center_of_mass"][k] * b["volume_mm3"] / vol
  return {
    "body_count": len(solids),
    "surface_body_count": len(bs) - len(solids),
    "volume_mm3": vol,
    "area_mm2": sum(b["area_mm2"] for b in bs),
    "center_of_mass": com,
    "bbox": {"min": [min(b["bbox"]["min"][k] for b in bs) for k in range(3)],
             "max": [max(b["bbox"]["max"][k] for b in bs) for k in range(3)]},
    "faces": sum(b["faces"] for b in bs),
  }


def design(app):
  d = adsk.fusion.Design.cast(app.activeProduct)
  if d is None:
    return {"ok": False, "error": "the active document is not a Fusion design"}
  bs = bodies(app, d)
  return {"ok": True, "measured": summarize(bs), "bodies": bs, "timeline": _timeline(d)}


def screenshot(app, path, width=1200, height=800, fit=True):
  vp = app.activeViewport
  if fit:
    vp.fit()
  ok = vp.saveAsImageFile(path, width, height)
  return {"ok": bool(ok), "path": path}
