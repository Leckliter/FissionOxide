"""Append ledger rows and index lines for a wave of batches.

Usage, from the repository root:

    python parity/batches/ledger_from_batches.py parity/batches/map-wave6.json

Reads every batch named in the map, parses its `## name (P1)` sections (with `- key:`,
`- Fusion:`, `- FissionOxide:`, `- Start here:` lines), appends one row per item to
`parity/ui-features.jsonl` (refusing duplicate keys) and one `ready` line per batch to
`parity/batches/index.jsonl`.
"""

import datetime
import json
import pathlib
import re
import sys


def main(map_path: str) -> None:
  root = pathlib.Path("parity")
  mapping = json.loads(pathlib.Path(map_path).read_text(encoding="utf-8"))
  ledger = root / "ui-features.jsonl"
  existing = {json.loads(l)["key"] for l in ledger.read_text(encoding="utf-8").splitlines() if l.strip()}
  now = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
  rows, counts = [], {}
  for batch in sorted(set(mapping.values())):
    text = (root / "batches" / f"{batch}.md").read_text(encoding="utf-8")
    for sec in re.split(r"\n## ", text)[1:]:
      head, body = sec.split("\n", 1)
      name, sev = re.match(r"(.*) \((P\d)\)", head).groups()

      def field(label: str) -> str:
        return re.search(rf"^- {label}: (.*)$", body, re.M).group(1).strip()

      key = field("key").strip("`")
      if mapping.get(key) != batch:
        raise SystemExit(f"{key} is not mapped to {batch}")
      if key in existing:
        raise SystemExit(f"{key} is already in the ledger")
      files = sorted(set(re.findall(r"(crates/[\w/.-]+\.rs)", field("Start here"))))
      note = field("FissionOxide")
      rows.append({
        "key": key, "area": key.split(":")[1], "name": name, "fusion": field("Fusion"),
        "status": "mismatch",
        "history": [{"at": now, "status": "mismatch", "note": note, "severity": sev, "files": ",".join(files)}],
        "notes": note, "severity": sev, "files": files, "batch": batch,
      })
      counts[batch] = counts.get(batch, 0) + 1
  with ledger.open("a", encoding="utf-8") as fh:
    for r in rows:
      fh.write(json.dumps(r, ensure_ascii=False) + "\n")
  with (root / "batches" / "index.jsonl").open("a", encoding="utf-8") as fh:
    for b, n in sorted(counts.items()):
      fh.write(json.dumps({"name": b, "items": n, "status": "ready", "pr": None}) + "\n")
  print(len(rows), "rows added;", counts)


if __name__ == "__main__":
  main(sys.argv[1])
