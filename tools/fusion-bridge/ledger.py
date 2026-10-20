#!/usr/bin/env python3
"""Turns the Fusion catalog into the parity ledger: one line per Fusion feature to test.

    py tools/fusion-bridge/ledger.py                    # parity/catalog.json -> parity/ledger.jsonl
    py tools/fusion-bridge/ledger.py --summary          # counts by scope, tab and status

Each line:

    {"key": "toolbar:ExtrudeCmd", "kind": "toolbar", "scope": "design", "name": "Extrude",
     "tooltip": "...", "where": "Design > SOLID > CREATE", "paths": [...],
     "fox_candidates": [{"cmd": "extrude", "score": 4}], "status": "untested", "history": []}

`kind` is `toolbar` (a ribbon command) or `api` (an API feature type, carrying its input's
options in `options`). `scope` is `design` for the Design workspace's tabs and `deferred` for the
other products (Manufacture, Simulation, Drawing, ...) and the PCB/Package tabs, which the loop
skips for now. `priority` (1 = Sketch and Solid first) orders the work.

Re-running keeps every existing entry's status, history and notes, adds new entries and marks
entries whose command is gone as `status: "removed"`. Statuses the loop moves through:
untested -> probing -> match | mismatch | fox-missing | blocked -> fixing -> verified.

`fox_candidates` is a guess from names, for the probe agent to start from, not a mapping.
"""

import argparse
import json
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
CATALOG = os.path.join(ROOT, "parity", "catalog.json")
LEDGER = os.path.join(ROOT, "parity", "ledger.jsonl")
STOP = {"create", "the", "a", "an", "to", "of", "and", "or", "by", "on", "in", "with", "from", "for", "cmd", "command"}


def words(s):
  s = re.sub(r"([a-z])([A-Z])", r"\1 \2", s or "")
  return {w for w in re.split(r"[^a-z0-9]+", s.lower()) if w and w not in STOP}


def fox_commands():
  sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
  from client import fox_cli  # noqa: E402

  p = subprocess.run([fox_cli(), "commands"], capture_output=True, text=True, check=True)
  return [json.loads(l) for l in p.stdout.splitlines() if l.strip()]


def candidates(name, tooltip, fox):
  nw, tw = words(name), words(tooltip)
  scored = []
  for c in fox:
    idw, hw = words(c["cmd"].replace(".", " ").replace("_", " ")), words(c["help"] + " " + c.get("params", ""))
    score = 3 * len(nw & idw) + len(nw & hw) + (len(tw & idw) if nw & (idw | hw) else 0)
    if score:
      scored.append({"cmd": c["cmd"], "score": score})
  scored.sort(key=lambda x: -x["score"])
  return scored[:4]


def walk(controls, path, out):
  for c in controls:
    cmd = c.get("command")
    if cmd and cmd.get("id"):
      out.append((cmd, path + ([c["name"]] if c.get("name") else []), c))
    if c.get("children"):
      walk(c["children"], path + ([c["name"]] if c.get("name") else []), out)


# The Design workspace's tabs, in the order the loop works through them. Every other workspace
# (and the Design workspace's PCB/Package tabs) is deferred. Commands from add-ins are skipped.
DESIGN_WS = "FusionSolidEnvironment"
TAB_PRIORITY = {
  "SketchTab": 1, "SolidTab": 1, "SurfaceTab": 2, "SheetMetalTab": 2, "AssemblyTab": 3, "ToolsTab": 3,
  "BasefeatureSolidTab": 4, "BasefeatureSurfaceTab": 4, "EditSnapshotTab": 4, "ManageTab": 4,
  "ParaMeshOuterTab": 5, "ParaMeshBaseFeatureTab": 5, "FormTab": 5, "PlasticTab": 5,
}


def build(catalog, fox):
  entries = {}
  for ws in catalog["workspaces"]:
    for tab in ws.get("tabs", []):
      prio = TAB_PRIORITY.get(tab["id"]) if ws["id"] == DESIGN_WS else None
      scope = "design" if prio else "deferred"
      for panel in tab["panels"]:
        found = []
        walk(panel["controls"], [ws["name"], tab["name"], panel["name"]], found)
        for cmd, path, ctl in found:
          if cmd.get("native") is False:
            continue
          key = "toolbar:" + cmd["id"]
          where = " > ".join(p for p in path if p)
          e = entries.get(key)
          if e is None:
            e = entries[key] = {
              "key": key, "kind": "toolbar", "scope": scope, "priority": prio or 9, "name": cmd["name"],
              "tooltip": cmd["tooltip"], "where": where, "paths": [],
              "fox_candidates": candidates(cmd["name"], cmd["tooltip"], fox),
            }
          if where not in e["paths"]:
            e["paths"].append(where)
          if prio and prio < e["priority"]:
            e.update(scope="design", priority=prio, where=where)
  for feat in catalog["api"]["features"]:
    key = "api:" + feat["collection"]
    entries[key] = {
      "key": key, "kind": "api", "scope": "design", "priority": 1, "name": feat["feature"],
      "tooltip": "", "where": "API adsk.fusion.Features." + feat["collection"],
      "options": feat["input_members"], "create_methods": feat["create_methods"],
      "fox_candidates": candidates(feat["feature"].replace("Feature", ""), "", fox),
    }
  return entries


def merge(new, path):
  old = {}
  if os.path.exists(path):
    with open(path, encoding="utf-8") as f:
      for line in f:
        if line.strip():
          e = json.loads(line)
          old[e["key"]] = e
  out = []
  for key, e in new.items():
    prev = old.pop(key, None)
    e["status"] = prev.get("status", "untested") if prev else "untested"
    e["history"] = prev.get("history", []) if prev else []
    for k in ("notes", "cases", "prs"):
      if prev and k in prev:
        e[k] = prev[k]
    out.append(e)
  for e in old.values():
    if e.get("status") != "removed":
      e["status"] = "removed"
    out.append(e)
  order = {"design": 0, "deferred": 1}
  out.sort(key=lambda e: (order.get(e["scope"], 2), e.get("priority", 9), e["kind"] != "toolbar", e["where"], e["name"]))
  return out


def summary(entries):
  by = {}
  for e in entries:
    k = (e["scope"], e["where"].split(" > ")[1] if e["kind"] == "toolbar" and " > " in e["where"] else e["kind"])
    by.setdefault(k, {}).setdefault(e["status"], 0)
    by[k][e["status"]] += 1
  for (scope, tab), st in sorted(by.items()):
    print(f"{scope:9} {tab:28} " + "  ".join(f"{s}={n}" for s, n in sorted(st.items())))
  print(f"total {len(entries)}")


def main():
  ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
  ap.add_argument("--catalog", default=CATALOG)
  ap.add_argument("--ledger", default=LEDGER)
  ap.add_argument("--summary", action="store_true")
  a = ap.parse_args()
  if a.summary:
    with open(a.ledger, encoding="utf-8") as f:
      summary([json.loads(l) for l in f if l.strip()])
    return
  with open(a.catalog, encoding="utf-8") as f:
    catalog = json.load(f)
  entries = merge(build(catalog, fox_commands()), a.ledger)
  with open(a.ledger, "w", encoding="utf-8") as f:
    for e in entries:
      f.write(json.dumps(e, ensure_ascii=False) + "\n")
  summary(entries)


if __name__ == "__main__":
  main()
