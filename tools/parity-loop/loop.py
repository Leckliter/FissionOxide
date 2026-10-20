#!/usr/bin/env python3
"""State for the parity loop: which Fusion feature to probe next, what was found, and the fix
batches handed to agents. Works on both ledgers:

    parity/ui-features.jsonl   interaction behaviours (seed_ui.py, plus discovery)
    parity/ledger.jsonl        Fusion commands and API features (fusion-bridge/ledger.py)

    py tools/parity-loop/loop.py next [--n 5]           # next untested entries, UI first
    py tools/parity-loop/loop.py record KEY STATUS [--note TEXT] [--shot PATH] [--severity P0|P1|P2] [--files a,b]
    py tools/parity-loop/loop.py add AREA SLUG "what Fusion does"
    py tools/parity-loop/loop.py summary
    py tools/parity-loop/loop.py batches [--max 8]      # group gaps by area -> parity/batches/*.md
    py tools/parity-loop/loop.py mark-batch NAME STATUS [--pr URL]

STATUS is one of: probing match mismatch fox-missing blocked fixing verified.
"""

import argparse
import datetime
import json
import os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
UI = os.path.join(ROOT, "parity", "ui-features.jsonl")
CMD = os.path.join(ROOT, "parity", "ledger.jsonl")
BATCHES = os.path.join(ROOT, "parity", "batches")
STATUSES = {"untested", "probing", "match", "mismatch", "fox-missing", "blocked", "fixing", "verified"}
GAPS = {"mismatch", "fox-missing"}


def load(path):
  if not os.path.exists(path):
    return []
  with open(path, encoding="utf-8") as f:
    return [json.loads(l) for l in f if l.strip()]


def save(path, entries):
  tmp = path + ".tmp"
  with open(tmp, "w", encoding="utf-8", newline="\n") as f:
    for e in entries:
      f.write(json.dumps(e) + "\n")
  os.replace(tmp, path)


def both():
  return [(UI, load(UI)), (CMD, load(CMD))]


def area(e):
  return e.get("area") or (e.get("where") or "other").split(" > ")[-1].lower().replace(" ", "-")


def now():
  return datetime.datetime.now().astimezone().isoformat(timespec="seconds")


def cmd_next(a):
  out = []
  for path, entries in both():
    pool = [e for e in entries if e["status"] in ("untested", "probing") and e.get("scope", "design") == "design"]
    pool.sort(key=lambda e: (e["status"] != "probing", e.get("priority", 0)))
    out += pool
  for e in out[: a.n]:
    print(json.dumps({k: e[k] for k in ("key", "name", "status") if k in e} | {"fusion": e.get("fusion") or e.get("tooltip", "")[:200], "where": e.get("where", e.get("area"))}))


def cmd_record(a):
  if a.status not in STATUSES:
    raise SystemExit(f"status must be one of {sorted(STATUSES)}")
  for path, entries in both():
    for e in entries:
      if e["key"] == a.key:
        h = {"at": now(), "status": a.status}
        for k in ("note", "shot", "severity", "files"):
          if getattr(a, k):
            h[k] = getattr(a, k)
        e.setdefault("history", []).append(h)
        e["status"] = a.status
        if a.note:
          e["notes"] = a.note
        if a.severity:
          e["severity"] = a.severity
        if a.files:
          e["files"] = a.files.split(",")
        save(path, entries)
        print(f"{a.key}: {a.status}")
        return
  raise SystemExit(f"no entry {a.key}")


def cmd_add(a):
  entries = load(UI)
  key = f"ui:{a.area}:{a.slug}"
  if any(e["key"] == key for e in entries):
    print(f"{key} exists")
    return
  entries.append({"key": key, "area": a.area, "name": a.slug.replace("-", " "), "fusion": a.fusion, "status": "untested", "history": [], "found": now()})
  save(UI, entries)
  print(f"added {key}")


def cmd_summary(_):
  for path, entries in both():
    by = {}
    for e in entries:
      if e.get("scope", "design") == "design":
        by[e["status"]] = by.get(e["status"], 0) + 1
    print(f"{os.path.relpath(path, ROOT)}: " + ", ".join(f"{k}={v}" for k, v in sorted(by.items())))


def cmd_batches(a):
  gaps = {}
  ledgers = both()
  # --map groups entries by the code they touch ({key: batch}); unmapped gaps wait
  groups = json.load(open(a.map, encoding="utf-8")) if a.map else None
  for _, entries in ledgers:
    for e in entries:
      if e["status"] in GAPS and not e.get("batch"):
        g = groups.get(e["key"]) if groups is not None else area(e)
        if g:
          gaps.setdefault(g, []).append(e)
  os.makedirs(BATCHES, exist_ok=True)
  made = []
  for ar, items in sorted(gaps.items()):
    items.sort(key=lambda e: e.get("severity", "P9"))
    for i in range(0, len(items), a.max):
      chunk = items[i : i + a.max]
      name = ar if groups is not None else f"{ar}-{i // a.max + 1}"
      lines = [f"# Parity batch: {name}", "", "Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion",
               "does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests",
               "where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.", ""]
      for e in chunk:
        lines += [f"## {e['name']} ({e.get('severity', 'P2')})", f"- key: `{e['key']}`",
                  f"- Fusion: {e.get('fusion') or e.get('tooltip', '')}", f"- FissionOxide: {e.get('notes', '')}"]
        if e.get("files"):
          lines.append("- Start here: " + ", ".join(f"`{f}`" for f in e["files"]))
        lines.append("")
      with open(os.path.join(BATCHES, name + ".md"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines))
      for e in chunk:
        e["batch"] = name
      made.append((name, len(chunk)))
  for path, entries in ledgers:
    save(path, entries)
  idx = load(os.path.join(BATCHES, "index.jsonl"))
  known = {b["name"] for b in idx}
  idx += [{"name": n, "items": c, "status": "pending", "pr": None} for n, c in made if n not in known]
  save(os.path.join(BATCHES, "index.jsonl"), idx)
  for n, c in made:
    print(f"{n}: {c} items")


def cmd_mark_batch(a):
  path = os.path.join(BATCHES, "index.jsonl")
  idx = load(path)
  for b in idx:
    if b["name"] == a.name:
      b["status"] = a.status
      if a.pr:
        b["pr"] = a.pr
  save(path, idx)
  if a.status in ("fixing", "verified"):
    for p, entries in both():
      for e in entries:
        if e.get("batch") == a.name:
          e["status"] = a.status
          if a.pr:
            e.setdefault("prs", []).append(a.pr)
      save(p, entries)
  print(f"{a.name}: {a.status}")


def main():
  p = argparse.ArgumentParser()
  s = p.add_subparsers(dest="cmd", required=True)
  n = s.add_parser("next"); n.add_argument("--n", type=int, default=5)
  r = s.add_parser("record"); r.add_argument("key"); r.add_argument("status")
  for k in ("note", "shot", "severity", "files"):
    r.add_argument("--" + k)
  ad = s.add_parser("add"); ad.add_argument("area"); ad.add_argument("slug"); ad.add_argument("fusion")
  s.add_parser("summary")
  b = s.add_parser("batches"); b.add_argument("--max", type=int, default=8); b.add_argument("--map")
  m = s.add_parser("mark-batch"); m.add_argument("name"); m.add_argument("status"); m.add_argument("--pr")
  a = p.parse_args()
  {"next": cmd_next, "record": cmd_record, "add": cmd_add, "summary": cmd_summary, "batches": cmd_batches, "mark-batch": cmd_mark_batch}[a.cmd](a)


if __name__ == "__main__":
  main()
