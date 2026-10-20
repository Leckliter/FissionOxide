"""Reads and writes parity/ledger.jsonl, one entry at a time, so a crash never loses a step.

Every write rewrites the whole file to a temporary file and renames it over the ledger, under a
lock file, so two driver processes (or a probe and a fixer) can't interleave half-written lines.
"""

import contextlib
import datetime
import json
import os
import time

# Statuses the loop moves an entry through (see tools/fusion-bridge/README.md).
UNTESTED, PROBING, MATCH, MISMATCH, FOX_MISSING, BLOCKED, FIXING, VERIFIED = (
  "untested", "probing", "match", "mismatch", "fox-missing", "blocked", "fixing", "verified")
PROBE_RESULTS = (MATCH, MISMATCH, FOX_MISSING, BLOCKED)


def replace(src, dst, tries=40):
  """os.replace, retried: on Windows it fails while another process (git, OneDrive, an agent's
  reader) has the target open."""
  for i in range(tries):
    try:
      os.replace(src, dst)
      return
    except PermissionError:
      if i == tries - 1:
        raise
      time.sleep(0.05 * (i + 1))


def now():
  return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


class Ledger:
  def __init__(self, path):
    self.path = path
    self.lock_path = path + ".lock"

  @contextlib.contextmanager
  def locked(self, timeout=60):
    deadline = time.time() + timeout
    while True:
      try:
        fd = os.open(self.lock_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        os.write(fd, str(os.getpid()).encode())
        os.close(fd)
        break
      except (FileExistsError, PermissionError):  # Windows: the file is being deleted
        # A lock older than two minutes belongs to a process that died mid-write.
        try:
          if time.time() - os.path.getmtime(self.lock_path) > 120:
            os.unlink(self.lock_path)
            continue
        except OSError:
          continue
        if time.time() > deadline:
          raise TimeoutError(f"ledger locked: {self.lock_path}")
        time.sleep(0.05)
    try:
      yield
    finally:
      with contextlib.suppress(OSError):
        os.unlink(self.lock_path)

  def read(self):
    with open(self.path, encoding="utf-8") as f:
      return [json.loads(l) for l in f if l.strip()]

  def _write(self, entries):
    tmp = self.path + ".tmp"
    with open(tmp, "w", encoding="utf-8", newline="\n") as f:
      for e in entries:
        f.write(json.dumps(e, ensure_ascii=False) + "\n")
    replace(tmp, self.path)

  def get(self, key):
    for e in self.read():
      if e["key"] == key:
        return e
    raise KeyError(key)

  def update(self, key, note=None, **fields):
    """Sets fields on one entry and appends a history note; returns the updated entry."""
    with self.locked():
      entries = self.read()
      for e in entries:
        if e["key"] == key:
          e.update(fields)
          if note or "status" in fields:
            h = {"at": now(), "by": "parity-loop"}
            if "status" in fields:
              h["status"] = fields["status"]
            if note:
              h["note"] = note
            e.setdefault("history", []).append(h)
          self._write(entries)
          return e
    raise KeyError(key)

  def claim_next(self, keys=None, busy=()):
    """Claims the next entry to probe under the lock, marking it probing for this process; None
    when there's nothing left. Skips entries a live process is probing (this one included)."""
    with self.locked():
      entries = self.read()
      free = [e for e in entries if e["key"] not in busy and not (
        e["status"] == PROBING and e.get("probe_pid") not in (None, os.getpid()) and pid_alive(e.get("probe_pid")))]
      e = next_untested(free, keys)
      if e is None:
        return None
      e["probe_pid"] = os.getpid()
      if e["status"] != PROBING:
        e["status"] = PROBING
        e.setdefault("history", []).append({"at": now(), "by": "parity-loop", "status": PROBING, "note": "claimed"})
      self._write(entries)
      return e

  def mutate(self, key, fn, note=None):
    """Applies fn(entry) under the lock (for list fields such as prs and cases)."""
    with self.locked():
      entries = self.read()
      for e in entries:
        if e["key"] == key:
          fn(e)
          if note:
            e.setdefault("history", []).append({"at": now(), "by": "parity-loop", "note": note})
          self._write(entries)
          return e
    raise KeyError(key)


def next_untested(entries, keys=None):
  """The design-scope entry to probe next: a probe that was interrupted first, then the lowest
  priority untested entry, in ledger order."""
  pool = [e for e in entries if e.get("scope") == "design" and (keys is None or e["key"] in keys)]
  for e in pool:
    if e["status"] == PROBING:
      return e
  todo = [e for e in pool if e["status"] == UNTESTED]
  todo.sort(key=lambda e: e.get("priority", 9))  # stable: ledger order within a priority
  return todo[0] if todo else None


MAX_FIX_ATTEMPTS = 2


def needs_fix(e):
  """A probed gap with no fixer PR waiting on review, and fixer attempts left."""
  fix = e.get("fix") or {}
  return (e.get("scope") == "design" and e["status"] in (MISMATCH, FOX_MISSING)
          and fix.get("state") not in ("pr-open", "pushed") and fix.get("attempts", 0) < MAX_FIX_ATTEMPTS
          and not e.get("no_fix"))


def slug(key):
  s = key.split(":", 1)[-1]
  out = "".join(c.lower() if c.isalnum() else "-" for c in s)
  while "--" in out:
    out = out.replace("--", "-")
  return out.strip("-")[:48]


def pid_alive(pid):
  """Whether a process is still running. (On Windows, os.kill(pid, 0) would terminate it.)"""
  if not pid:
    return False
  if os.name == "nt":
    import ctypes

    k = ctypes.windll.kernel32
    h = k.OpenProcess(0x1000, False, int(pid))  # PROCESS_QUERY_LIMITED_INFORMATION
    if not h:
      return False
    code = ctypes.c_ulong()
    ok = k.GetExitCodeProcess(h, ctypes.byref(code))
    k.CloseHandle(h)
    return bool(ok) and code.value == 259  # STILL_ACTIVE
  try:
    os.kill(int(pid), 0)
    return True
  except OSError:
    return False


def fixer_running_elsewhere(e):
  """A fixer another driver process is running right now for this entry."""
  fix = e.get("fix") or {}
  return fix.get("state") == "agent" and fix.get("pid") not in (None, os.getpid()) and pid_alive(fix.get("pid"))
