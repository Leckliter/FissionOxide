"""Lists everything Fusion can do, from its own API rather than from the screen.

Three views, which overlap on purpose:

- `toolbar`: every workspace -> tab -> panel -> control, as a user sees the ribbon (including the
  contextual tabs such as Sketch, which exist while hidden).
- `commands`: every command definition Fusion has registered, including ones not on any toolbar
  (marking menu, context menus, shortcuts).
- `api`: every feature collection, its input class and the input's settable properties, every
  sketch curve / constraint / dimension creation method, and every enum in adsk.fusion. This is
  the option inventory a probe works through (an extrude's extent types, a fillet's modes, ...).
"""

import inspect
import time

import adsk.core
import adsk.fusion

DESIGN_PRODUCT = "DesignProductType"


def _safe(f, default=None):
  try:
    return f()
  except Exception:
    return default


def _command(defn):
  if defn is None:
    return None
  return {
    "id": defn.id,
    "name": _safe(lambda: defn.name, ""),
    "tooltip": _safe(lambda: defn.tooltip, ""),
    "type": _safe(lambda: defn.controlDefinition.objectType.split("::")[-1], ""),
    "native": _safe(lambda: defn.isNative, None),
  }


def _controls(controls):
  out = []
  for i in range(controls.count):
    c = controls.item(i)
    kind = c.objectType.split("::")[-1]
    if kind == "SeparatorControl":
      continue
    entry = {"control": kind, "id": c.id, "visible": _safe(lambda: c.isVisible, None),
             "promoted": _safe(lambda: c.isPromoted, None)}
    if kind == "CommandControl":
      entry["command"] = _command(_safe(lambda: c.commandDefinition))
    elif kind == "DropDownControl":
      entry["name"] = _safe(lambda: c.name, "")
      entry["children"] = _controls(c.controls)
    elif kind == "SplitButtonControl":
      entry["command"] = _command(_safe(lambda: c.defaultCommandDefinition))
      extra = _safe(lambda: c.additionalDefinitions, []) or []
      entry["children"] = [{"control": "CommandControl", "command": _command(d)} for d in extra]
    out.append(entry)
  return out


def _workspace(ws):
  tabs = []
  for t in range(_safe(lambda: ws.toolbarTabs.count, 0)):
    tab = ws.toolbarTabs.item(t)
    panels = []
    for p in range(tab.toolbarPanels.count):
      panel = tab.toolbarPanels.item(p)
      panels.append({"id": panel.id, "name": panel.name, "visible": _safe(lambda: panel.isVisible, None),
                     "controls": _controls(panel.controls)})
    tabs.append({"id": tab.id, "name": tab.name, "visible": _safe(lambda: tab.isVisible, None),
                 "panels": panels})
  return tabs


def _api_inventory():
  features = []
  for name, _ in inspect.getmembers(adsk.fusion.Features):
    if not name.endswith("Features") or name.startswith("_"):
      continue
    singular = name[0].upper() + name[1:-1]  # extrudeFeatures -> ExtrudeFeature
    input_cls = getattr(adsk.fusion, singular + "Input", None)
    coll_cls = getattr(adsk.fusion, singular + "s", None)
    features.append({
      "collection": name,
      "feature": singular,
      "create_methods": sorted(m for m in dir(coll_cls) if m.startswith(("create", "add"))) if coll_cls else [],
      "input_members": sorted(m for m in dir(input_cls) if not m.startswith("_") and m not in ("cast", "classType", "objectType", "isValid", "this", "thisown")) if input_cls else [],
    })

  def methods(cls, prefix):
    return sorted(m for m in dir(cls) if m.startswith(prefix))

  sketch = {
    "curves": {n: methods(getattr(adsk.fusion, n), "add") for n in (
      "SketchLines", "SketchCircles", "SketchArcs", "SketchEllipses", "SketchEllipticalArcs",
      "SketchFittedSplines", "SketchControlPointSplines", "SketchConicCurves", "SketchPoints",
      "SketchTexts") if hasattr(adsk.fusion, n)},
    "sketch_methods": sorted(m for m in dir(adsk.fusion.Sketch) if not m.startswith("_")),
    "constraints": methods(adsk.fusion.GeometricConstraints, "add"),
    "dimensions": methods(adsk.fusion.SketchDimensions, "add"),
  }
  construction = {n: methods(getattr(adsk.fusion, n), "setBy") for n in (
    "ConstructionPlaneInput", "ConstructionAxisInput", "ConstructionPointInput")}

  enums = {}
  for name, cls in inspect.getmembers(adsk.fusion, inspect.isclass):
    # SWIG enums are classes whose public attributes (besides `thisown`) are all ints.
    public = {k: v for k, v in vars(cls).items() if not k.startswith("_") and k != "thisown"}
    members = {k: v for k, v in public.items() if isinstance(v, int) and not isinstance(v, bool)}
    if members and len(members) == len(public):
      enums[name] = sorted(members)
  return {"features": features, "sketch": sketch, "construction": construction, "enums": enums}


def _ensure_design(app):
  if app.activeProduct and app.activeProduct.objectType == adsk.fusion.Design.classType():
    return None
  return app.documents.add(adsk.core.DocumentTypes.FusionDesignDocumentType)


def harvest(app, activate=True):
  ui = app.userInterface
  t0 = time.time()
  scratch = _ensure_design(app)
  start_ws = _safe(lambda: ui.activeWorkspace.id)

  workspaces = []
  for i in range(ui.workspaces.count):
    ws = ui.workspaces.item(i)
    product = _safe(lambda: ws.productType, "")
    entry = {"id": ws.id, "name": ws.name, "product": product,
             "scope": "design" if product == DESIGN_PRODUCT else "deferred"}
    if activate and product == DESIGN_PRODUCT and not ws.isActive:
      _safe(lambda: ws.activate())
    entry["tabs"] = _safe(lambda: _workspace(ws), [])
    workspaces.append(entry)
  if start_ws:
    _safe(lambda: ui.workspaces.itemById(start_ws).activate())

  toolbars = []
  for i in range(ui.toolbars.count):
    tb = ui.toolbars.item(i)
    toolbars.append({"id": tb.id, "controls": _safe(lambda: _controls(tb.controls), [])})

  commands = []
  for i in range(ui.commandDefinitions.count):
    c = _safe(lambda: _command(ui.commandDefinitions.item(i)))
    if c:
      commands.append(c)

  if scratch:
    _safe(lambda: scratch.close(False))
  return {
    "ok": True,
    "fusion": app.version,
    "harvested_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
    "seconds": round(time.time() - t0, 1),
    "workspaces": workspaces,
    "toolbars": toolbars,
    "commands": commands,
    "api": _api_inventory(),
  }
