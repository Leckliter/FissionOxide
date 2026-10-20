#!/usr/bin/env python3
"""Talks to the FoxBridge add-in running inside Fusion, and compares Fusion with FissionOxide.

    py tools/fusion-bridge/client.py ping
    py tools/fusion-bridge/client.py catalog [-o parity/catalog.json]
    py tools/fusion-bridge/client.py recipe tests/reference/08-chamfer/recipe.json [--keep-open] [--png out.png]
    py tools/fusion-bridge/client.py compare [CASE_FILTER ...] [--png]
    py tools/fusion-bridge/client.py python "result = app.version"
    py tools/fusion-bridge/client.py close

`compare` builds each reference case in Fusion (from recipe.json) and in FissionOxide (from
part.jsonl, with target/release/fissionoxide-cli or $FOX_CLI), then writes
parity/compare/<case>.json and parity/compare/summary.md. Fusion is the answer key; the case's
expected.json is shown beside it so a wrong expected.json shows up too.

Python 3 standard library only. Fusion must be running with the FoxBridge add-in started.
"""

import argparse
import contextlib
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
REF = os.path.join(ROOT, "tests", "reference")
OUT = os.path.join(ROOT, "parity", "compare")
PORT = int(os.environ.get("FOXBRIDGE_PORT", "47800"))
REL_TOL = 1e-3


def token():
  path = os.path.join(os.environ.get("APPDATA") or os.path.expanduser("~"), "FoxBridge", "token")
  try:
    with open(path) as f:
      return f.read().strip()
  except OSError:
    sys.exit(f"no bridge token at {path}: is Fusion running with the FoxBridge add-in started?")


# ---- the Fusion lock ----------------------------------------------------------------------------
# Fusion runs bridge requests one at a time, so short self-contained ones (a recipe that closes its
# document, a compare) can interleave between clients. A client that leaves a document open
# (--keep-open) or runs `python` (which may work on the active document) holds the lock across
# calls until it runs `close`, so nobody else's document becomes active under it. Clients name
# themselves with $FOXBRIDGE_OWNER (the parity loop sets one per probe); a lock whose owner hasn't
# used Fusion for LOCK_STALE seconds is taken over.

LOCK_STALE = 600
_LOCK = os.path.join(os.environ.get("APPDATA") or os.path.expanduser("~"), "FoxBridge", "fusion.lock")


def _owner():
  return os.environ.get("FOXBRIDGE_OWNER") or f"pid-{os.getpid()}"


@contextlib.contextmanager
def _mutex():
  mx = _LOCK + ".mx"
  while True:
    try:
      os.close(os.open(mx, os.O_CREAT | os.O_EXCL | os.O_WRONLY))
      break
    except (FileExistsError, PermissionError):  # Windows: the file is being deleted
      try:
        if time.time() - os.path.getmtime(mx) > 30:
          os.unlink(mx)
      except OSError:
        pass
      time.sleep(0.05)
  try:
    yield
  finally:
    try:
      os.unlink(mx)
    except OSError:
      pass


def _read_lock():
  try:
    with open(_LOCK, encoding="utf-8") as f:
      return json.load(f)
  except (OSError, ValueError):
    return None


def fusion_acquire(hold, owner=None, wait=3600):
  """Waits until Fusion is free or ours; with hold, keeps it until fusion_release."""
  owner = owner or _owner()
  deadline, said = time.time() + wait, False
  while True:
    with _mutex():
      cur = _read_lock()
      if cur is None or cur.get("owner") == owner or time.time() - cur.get("touched", 0) > LOCK_STALE:
        held = hold or bool(cur and cur.get("owner") == owner and cur.get("hold"))
        os.makedirs(os.path.dirname(_LOCK), exist_ok=True)
        with open(_LOCK, "w", encoding="utf-8") as f:
          json.dump({"owner": owner, "hold": held, "touched": time.time()}, f)
        return held
    if time.time() > deadline:
      sys.exit(f"Fusion has been busy for {wait} s (held by {cur.get('owner')}: {_LOCK})")
    if not said:
      print(f"waiting for Fusion (held by {cur.get('owner')})", file=sys.stderr, flush=True)
      said = True
    time.sleep(0.5)


def fusion_release(owner=None, force=False):
  """Frees the lock if this owner has it (force: whoever has it)."""
  owner = owner or _owner()
  with _mutex():
    cur = _read_lock()
    if cur and (force or cur.get("owner") == owner):
      try:
        os.unlink(_LOCK)
      except OSError:
        pass


def call(op=None, timeout=900, hold=None, **req):
  """One bridge request. Every request but ping takes the Fusion lock; `hold` (default: true for
  `python` and for a kept-open recipe) keeps it after the request until `close`."""
  if op is None:
    return _call()
  if hold is None:
    hold = op == "python" or bool(req.get("keep_open"))
  held = fusion_acquire(hold)
  try:
    return _call(op, timeout, **req)
  finally:
    if op == "close_all" or not held:
      fusion_release()


def _call(op=None, timeout=900, **req):
  url = f"http://127.0.0.1:{PORT}/" + ("ping" if op is None else "run")
  data = None if op is None else json.dumps(dict(req, op=op)).encode()
  r = urllib.request.Request(url, data=data, headers={"X-Fox-Token": token(), "Content-Type": "application/json"})
  try:
    with urllib.request.urlopen(r, timeout=timeout) as resp:
      return json.loads(resp.read())
  except urllib.error.HTTPError as e:
    return json.loads(e.read() or b"{}") or {"ok": False, "error": str(e)}
  except urllib.error.URLError as e:
    sys.exit(f"can't reach FoxBridge on port {PORT}: {e.reason}")


# The runner in this checkout, loaded into Fusion through the `python` op. The add-in's own
# `recipe` op loads the copy its AddIns link points at, which is one checkout; this lets every
# worktree (the parity loop's probes and fixers) build recipes with its own runner.
LOCAL_RUNNER = r"""
import importlib.util, json, sys
def _load(name, path):
  spec = importlib.util.spec_from_file_location(name, path)
  mod = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(mod)
  return mod
_saved = sys.modules.get("measure")
sys.modules["measure"] = _load("measure", DIR + "/measure.py")
try:
  _rr = _load("foxbridge_local_runner", DIR + "/recipe_runner.py")
finally:
  if _saved is not None:
    sys.modules["measure"] = _saved
result = _rr.run(app, json.loads(RECIPE), keep_open=KEEP_OPEN, screenshot=SCREENSHOT)
"""


def recipe_call(recipe, keep_open=False, screenshot=None):
  """Builds a recipe in Fusion with this checkout's recipe_runner.py; returns the runner's result."""
  if os.environ.get("FOXBRIDGE_ADDIN_RUNNER"):
    return call("recipe", recipe=recipe, keep_open=keep_open, screenshot=screenshot)
  here = os.path.join(os.path.dirname(os.path.abspath(__file__)), "FoxBridge").replace("\\", "/")
  code = (f"DIR = {here!r}\nRECIPE = {json.dumps(json.dumps(recipe))}\nKEEP_OPEN = {bool(keep_open)!r}\n"
          f"SCREENSHOT = {screenshot!r}\n" + LOCAL_RUNNER)
  res = call("python", code=code, hold=bool(keep_open))  # the runner closes its own document
  if not res.get("ok"):
    return res
  return res.get("result") or {"ok": False, "error": "runner returned nothing"}


# ---- FissionOxide side ------------------------------------------------------------------------

def fox_cli():
  if os.environ.get("FOX_CLI"):
    return os.environ["FOX_CLI"]
  for profile in ("release", "debug"):
    p = os.path.join(ROOT, "target", profile, "fissionoxide-cli" + (".exe" if os.name == "nt" else ""))
    if os.path.exists(p):
      return p
  sys.exit("no fissionoxide-cli: run `cargo build -p fissionoxide-cli --release` or set FOX_CLI")


def fox_measure(part_jsonl, png=None):
  with open(part_jsonl, encoding="utf-8") as f:
    script = f.read()
  with tempfile.NamedTemporaryFile("w", suffix=".jsonl", delete=False, encoding="utf-8") as t:
    t.write(script.rstrip("\n") + '\n{"cmd": "info"}\n{"cmd": "inspect.center_of_mass"}\n')
  args = [fox_cli(), "run", t.name] + (["--png", png] if png else [])
  try:
    p = subprocess.run(args, capture_output=True, text=True, timeout=600)
  finally:
    os.unlink(t.name)
  lines = [json.loads(l) for l in p.stdout.splitlines() if l.strip().startswith("{")]
  errors = [l for l in lines if "error" in l]
  info = next((l["ok"] for l in lines if l.get("cmd") == "info" and "ok" in l), None)
  com = next((l["ok"] for l in lines if l.get("cmd") == "inspect.center_of_mass" and "ok" in l), None)
  if info is None:
    return {"ok": False, "error": p.stderr.strip() or "no info output", "errors": errors}
  bodies = [b for b in info["bodies"] if b.get("visible", True)]
  measured = {"body_count": len(bodies)}
  if bodies:
    measured.update(
      volume_mm3=sum(b["volume_mm3"] for b in bodies),
      area_mm2=sum(b["area_mm2"] for b in bodies),
      bbox={"min": [min(b["bbox"][0][k] for b in bodies) for k in range(3)],
            "max": [max(b["bbox"][1][k] for b in bodies) for k in range(3)]},
      faces=sum(b["faces"] for b in bodies),
    )
    if com:
      measured["center_of_mass"] = com["center"]
  failed = [t for t in info.get("timeline", []) if t.get("error")]
  return {"ok": not errors and not failed, "measured": measured, "errors": errors, "failed_features": failed}


# ---- comparison -------------------------------------------------------------------------------

def diff(a, b):
  """Differences between two measurements, in the tolerances tests/reference uses."""
  out = []
  if not a or not b:
    return ["missing measurement"]
  if a.get("body_count") != b.get("body_count"):
    out.append(f"body_count {a.get('body_count')} vs {b.get('body_count')}")
  for k in ("volume_mm3", "area_mm2"):
    x, y = a.get(k), b.get(k)
    if x is None or y is None:
      continue
    if abs(x - y) > REL_TOL * max(abs(x), abs(y), 1e-9):
      out.append(f"{k} {x:.4f} vs {y:.4f} ({(y - x) / max(abs(x), 1e-9):+.3%})")
  if a.get("bbox") and b.get("bbox"):
    lo, hi = a["bbox"]["min"], a["bbox"]["max"]
    tol = 1e-3 * sum((hi[k] - lo[k]) ** 2 for k in range(3)) ** 0.5
    for end in ("min", "max"):
      for k in range(3):
        if abs(a["bbox"][end][k] - b["bbox"][end][k]) > max(tol, 1e-6):
          out.append(f"bbox.{end}.{'xyz'[k]} {a['bbox'][end][k]:.4f} vs {b['bbox'][end][k]:.4f}")
  if a.get("center_of_mass") and b.get("center_of_mass") and a.get("bbox"):
    lo, hi = a["bbox"]["min"], a["bbox"]["max"]
    tol = 1e-3 * sum((hi[k] - lo[k]) ** 2 for k in range(3)) ** 0.5
    if any(abs(a["center_of_mass"][k] - b["center_of_mass"][k]) > max(tol, 1e-6) for k in range(3)):
      out.append(f"center_of_mass {a['center_of_mass']} vs {b['center_of_mass']}")
  if a.get("faces") is not None and b.get("faces") is not None and a["faces"] != b["faces"]:
    out.append(f"faces {a['faces']} vs {b['faces']}")
  return out


def known_differences():
  """Cases where Fusion genuinely differs from the right answer: parity/known-differences.json."""
  try:
    with open(os.path.join(ROOT, "parity", "known-differences.json"), encoding="utf-8") as f:
      return {k: v for k, v in json.load(f).items() if not k.startswith("_")}
  except OSError:
    return {}


def compare(filters, png=False):
  os.makedirs(OUT, exist_ok=True)
  known = known_differences()
  cases = sorted(d for d in os.listdir(REF) if os.path.isfile(os.path.join(REF, d, "recipe.json")))
  if filters:
    cases = [c for c in cases if any(f in c for f in filters)]
  rows = []
  for case in cases:
    d = os.path.join(REF, case)
    with open(os.path.join(d, "recipe.json"), encoding="utf-8") as f:
      recipe = json.load(f)
    shots = os.path.join(OUT, "png")
    if png:
      os.makedirs(shots, exist_ok=True)
    fusion = recipe_call(recipe, screenshot=os.path.join(shots, case + "-fusion.png") if png else None)
    part = os.path.join(d, "part.jsonl")
    fox = fox_measure(part, os.path.join(shots, case + "-fox.png") if png else None) if os.path.exists(part) else {"ok": False, "error": "no part.jsonl"}
    expected = None
    if os.path.exists(os.path.join(d, "expected.json")):
      with open(os.path.join(d, "expected.json"), encoding="utf-8") as f:
        expected = json.load(f)
    fm = fusion.get("measured")
    verdict = {
      "fusion_built": bool(fusion.get("ok")),
      "fusion_unsupported": fusion.get("unsupported", []),
      "fusion_errors": [s for s in fusion.get("steps", []) if s.get("error")] or ([fusion["error"]] if fusion.get("error") else []),
      "fox_built": bool(fox.get("ok")),
      "fox_vs_fusion": diff(fm, fox.get("measured")) if fusion.get("ok") else None,
      "expected_vs_fusion": diff(fm, expected) if fusion.get("ok") and expected else None,
    }
    fox_vs_expected = diff(expected, fox.get("measured")) if expected and fox.get("ok") else None
    geometry = [d for d in (verdict["fox_vs_fusion"] or []) if not d.startswith("faces ")]
    if not fusion.get("ok"):
      status = "fusion-unbuilt"
    elif not fox.get("ok"):
      status = "mismatch"
    elif not verdict["fox_vs_fusion"]:
      status = "match"
    elif not geometry:
      status = "match-geometry"  # the same solid, split into faces differently
    elif fox_vs_expected is not None and all(d.startswith("faces ") for d in fox_vs_expected):
      status = "disputed"  # Fox agrees with expected.json and Fusion doesn't: check the runner first
    else:
      status = "mismatch"
    if status in ("disputed", "mismatch") and case in known and fox_vs_expected is not None and not [
        d for d in geometry if d.split()[0].split(".")[0] not in known[case]["keys"]]:
      status = "fusion-differs"  # Fusion is wrong here; see parity/known-differences.json
      verdict["known_difference"] = known[case]["why"]
    verdict["status"] = status
    with open(os.path.join(OUT, case + ".json"), "w", encoding="utf-8") as f:
      json.dump({"case": case, "verdict": verdict, "fusion": fusion, "fox": fox, "expected": expected}, f, indent=1)
    rows.append((case, verdict))
    note = "; ".join((verdict["fox_vs_fusion"] or [])[:2]) or "; ".join(u["what"] for u in verdict["fusion_unsupported"][:2])
    print(f"{status:15} {case}  {note}", flush=True)

  # The summary covers every case compared so far, not only this run's filter.
  rows = []
  for name in sorted(os.listdir(OUT)):
    if name.endswith(".json"):
      with open(os.path.join(OUT, name), encoding="utf-8") as f:
        d = json.load(f)
      rows.append((d["case"], d["verdict"]))
  with open(os.path.join(OUT, "summary.md"), "w", encoding="utf-8") as f:
    counts = {}
    for _, v in rows:
      counts[v["status"]] = counts.get(v["status"], 0) + 1
    f.write("# Fusion vs FissionOxide: reference cases\n\nGenerated by `py tools/fusion-bridge/client.py compare`. "
            "Each case is built in Fusion from recipe.json and in FissionOxide from part.jsonl.\n\n"
            "- **match**: same bodies, volume, area, bounding box, centre of mass and face count.\n"
            "- **match-geometry**: the same solid, but split into a different number of faces.\n"
            "- **disputed**: FissionOxide agrees with expected.json but Fusion doesn't. Check the Fusion runner before blaming either app.\n"
            "- **mismatch**: FissionOxide differs from Fusion. This is a parity bug.\n"
            "- **fusion-differs**: Fusion genuinely differs from the right answer, and FissionOxide shouldn't copy it. See parity/known-differences.json.\n"
            "- **fusion-unbuilt**: the bridge couldn't build the recipe in Fusion (an unsupported op, or a runner error).\n\n")
    f.write(", ".join(f"**{n}** {k}" for k, n in sorted(counts.items())) + "\n\n")
    f.write("| Case | Status | Fox vs Fusion | expected.json vs Fusion | Fusion couldn't build |\n|---|---|---|---|---|\n")
    for case, v in rows:
      cell = lambda xs: "<br>".join(xs) if xs else ""
      f.write(f"| {case} | {v['status']} | {cell(v['fox_vs_fusion'])} | {cell(v['expected_vs_fusion'])} | "
              f"{cell([u['what'] for u in v['fusion_unsupported']] + [str(e.get('error', e))[:80] if isinstance(e, dict) else str(e)[:80] for e in v['fusion_errors']])} |\n")
  print(f"\nwrote {os.path.relpath(OUT, ROOT)}/summary.md")


def main():
  ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
  sub = ap.add_subparsers(dest="cmd", required=True)
  sub.add_parser("ping")
  c = sub.add_parser("catalog")
  c.add_argument("-o", "--out", default=os.path.join(ROOT, "parity", "catalog.json"))
  c.add_argument("--no-activate", action="store_true", help="don't switch Design workspaces to read their tabs")
  r = sub.add_parser("recipe")
  r.add_argument("path")
  r.add_argument("--keep-open", action="store_true")
  r.add_argument("--png")
  cmp_ = sub.add_parser("compare")
  cmp_.add_argument("filters", nargs="*")
  cmp_.add_argument("--png", action="store_true", help="save a screenshot from each app per case")
  py = sub.add_parser("python")
  py.add_argument("code", help="Python run inside Fusion with app, ui and adsk in scope; set `result`. '-' reads stdin; @FILE reads a file")
  sub.add_parser("close", help="close every scratch document the bridge opened")
  a = ap.parse_args()

  if a.cmd == "ping":
    print(json.dumps(call()))
  elif a.cmd == "catalog":
    res = call("catalog", activate_workspaces=not a.no_activate)
    if not res.get("ok"):
      sys.exit(json.dumps(res, indent=1))
    os.makedirs(os.path.dirname(a.out), exist_ok=True)
    with open(a.out, "w", encoding="utf-8") as f:
      json.dump(res, f, indent=1)
    n_tb = sum(1 for ws in res["workspaces"] for t in ws["tabs"] for p in t["panels"] for _ in p["controls"])
    print(f"{len(res['workspaces'])} workspaces, {n_tb} toolbar controls, {len(res['commands'])} command "
          f"definitions, {len(res['api']['features'])} API feature types -> {os.path.relpath(a.out, ROOT)}")
  elif a.cmd == "recipe":
    with open(a.path, encoding="utf-8") as f:
      recipe = json.load(f)
    print(json.dumps(recipe_call(recipe, keep_open=a.keep_open,
                                 screenshot=os.path.abspath(a.png) if a.png else None), indent=1))
  elif a.cmd == "compare":
    compare(a.filters, a.png)
  elif a.cmd == "python":
    code = sys.stdin.read() if a.code == "-" else open(a.code[1:], encoding="utf-8").read() if a.code.startswith("@") else a.code
    print(json.dumps(call("python", code=code), indent=1, default=str))
  elif a.cmd == "close":
    print(json.dumps(call("close_all")))


if __name__ == "__main__":
  main()
