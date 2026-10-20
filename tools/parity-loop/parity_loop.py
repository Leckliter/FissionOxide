#!/usr/bin/env python3
"""The Fusion parity loop: probes Fusion features one at a time, and starts fixer agents that open
FissionOxide PRs for every gap, all tracked in parity/ledger.jsonl.

    py tools/parity-loop/parity_loop.py status
    py tools/parity-loop/parity_loop.py run --max-features 3 --budget-usd 40
    py tools/parity-loop/parity_loop.py probe toolbar:ConstraintCoincident
    py tools/parity-loop/parity_loop.py fix toolbar:ConstraintCoincident
    py tools/parity-loop/parity_loop.py verify
    py tools/parity-loop/parity_loop.py ui-probe [KEY] --yes        # drives the mouse: opt-in
    py tools/parity-loop/parity_loop.py reset KEY                   # back to untested

Every step writes to the ledger before and after it runs, so after a crash or Ctrl+C the next
`run` picks up where it stopped: an entry left `probing` is probed again first, a fixer reuses
its worktree, and a pushed branch without a PR gets its PR. See README.md.
"""

import argparse
import asyncio
import contextlib
import json
import os
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "fusion-bridge"))

import agents  # noqa: E402
import client  # noqa: E402
import ledger_io as L  # noqa: E402
from ledger_io import Ledger, now, slug  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
REF = os.path.join(ROOT, "tests", "reference")
LOOP = os.path.join(ROOT, "parity", "loop")
PROBES = os.path.join(ROOT, "parity", "probes")
STOP_FILE = os.path.join(LOOP, "STOP")  # create it to stop a run cleanly: running agents finish, nothing new starts
EXE = ".exe" if os.name == "nt" else ""
CASE_FILES = ("recipe.json", "fusion.py", "part.jsonl", "expected.json")


def sh(args, cwd=ROOT, check=True, env=None, timeout=None):
  p = subprocess.run(args, cwd=cwd, capture_output=True, text=True, encoding="utf-8", errors="replace",
                     env=env, timeout=timeout)
  if check and p.returncode:
    raise RuntimeError(f"{' '.join(args)} failed ({p.returncode}): {(p.stderr or p.stdout).strip()[-2000:]}")
  return p


def common_root():
  """The main checkout (worktrees share its .git, its target/ and its OCCT build)."""
  d = sh(["git", "rev-parse", "--path-format=absolute", "--git-common-dir"]).stdout.strip()
  return os.path.dirname(os.path.normpath(d))


COMMON = common_root()


def fox_cli():
  if os.environ.get("FOX_CLI"):
    return os.environ["FOX_CLI"]
  for base in (ROOT, COMMON):
    p = os.path.join(base, "target", "release", "fissionoxide-cli" + EXE)
    if os.path.exists(p):
      return p
  sys.exit("no fissionoxide-cli: run `cargo build -p fissionoxide-cli --release` or set FOX_CLI")


def cargo_env(slot):
  env = dict(os.environ, CARGO_TARGET_DIR=os.path.join(COMMON, "target", f"parity-{slot}"))
  occt = os.path.join(COMMON, "target", "OCCT")
  if os.path.isdir(os.path.join(occt, "include")) and os.path.isdir(os.path.join(occt, "lib")):
    env["FOX_OCCT_DIR"] = occt
  return env


# ---- budget ------------------------------------------------------------------------------------

def tests_passed(p):
  """Whether a cargo test run passed. On Windows, fox-kernel's test binary (on main too) crashes
  with STATUS_ACCESS_VIOLATION while exiting, after every test has passed; that's not a failure of
  the branch, so a run counts by its test results when the only problem is that exit crash."""
  if p.returncode == 0:
    return True
  out = p.stdout + p.stderr
  return ("STATUS_ACCESS_VIOLATION" in out and "test result: ok" in out and "test result: FAILED" not in out
          and "could not compile" not in out and "panicked" not in out)


def touched_packages(wt, base):
  """The crates a branch changes (by their Cargo package names), for their unit tests. Changes in
  crates/sketch or crates/engine always bring in both: the engine's tests exercise the sketcher."""
  files = sh(["git", "diff", "--name-only", f"{base}...HEAD"], cwd=wt).stdout.split()
  pkgs = []
  for f in files:
    parts = f.split("/")
    if len(parts) < 3 or parts[0] not in ("crates", "apps"):
      continue
    crate = os.path.join(wt, parts[0], parts[1])
    if not os.path.exists(os.path.join(crate, "src", "lib.rs")):
      continue
    with open(os.path.join(crate, "Cargo.toml"), encoding="utf-8") as fh:
      name = next((l.split("=", 1)[1].strip().strip('"') for l in fh if l.startswith("name")), None)
    if name and name not in pkgs:
      pkgs.append(name)
  if any(p in pkgs for p in ("fox-sketch", "fox-engine")):
    pkgs += [p for p in ("fox-sketch", "fox-engine") if p not in pkgs]
  return pkgs


class Budget:
  """Caps what this run spends on agents; parity/loop/state.json keeps the running totals."""

  def __init__(self, usd, tokens):
    self.usd, self.tokens = usd, tokens
    self.spent_usd, self.spent_tokens = 0.0, 0
    self.path = os.path.join(LOOP, "state.json")
    os.makedirs(LOOP, exist_ok=True)

  def remaining_usd(self):
    return self.usd - self.spent_usd

  def ok(self):
    return self.spent_usd < self.usd and (not self.tokens or self.spent_tokens < self.tokens)

  def agent_cap(self, cap):
    return max(0.5, min(cap, self.remaining_usd()))

  def record(self, kind, key, meta):
    self.spent_usd += meta.get("cost_usd") or 0.0
    self.spent_tokens += meta.get("tokens") or 0
    try:
      with open(self.path, encoding="utf-8") as f:
        state = json.load(f)
    except (OSError, ValueError):
      state = {"total_usd": 0.0, "total_tokens": 0, "runs": []}
    state["total_usd"] += meta.get("cost_usd") or 0.0
    state["total_tokens"] += meta.get("tokens") or 0
    state["runs"].append({"at": now(), "kind": kind, "key": key, **{k: meta.get(k) for k in (
      "cost_usd", "tokens", "turns", "subtype", "is_error", "session_id")}})
    state["runs"] = state["runs"][-500:]
    with open(self.path + ".tmp", "w", encoding="utf-8") as f:
      json.dump(state, f, indent=1)
    L.replace(self.path + ".tmp", self.path)
    print(f"  [{kind}] {key}: ${meta.get('cost_usd', 0):.2f}, {meta.get('tokens', 0):,} tokens "
          f"(run total ${self.spent_usd:.2f} of ${self.usd:.2f})", flush=True)


# ---- cases -------------------------------------------------------------------------------------

def next_case_number():
  nums = [int(d.split("-", 1)[0]) for d in os.listdir(REF) if d[:2].isdigit() and "-" in d]
  return max(nums, default=0) + 1


def promote(scratch_case, name_hint):
  """Copies a scratch case into tests/reference/ under the next free number; returns its name."""
  name = f"{next_case_number():02d}-{name_hint}"
  dest = os.path.join(REF, name)
  os.makedirs(dest)
  for f in CASE_FILES:
    if os.path.exists(os.path.join(scratch_case, f)):
      shutil.copy2(os.path.join(scratch_case, f), dest)
  # A part.jsonl names its case on the first line, as the imported cases do.
  part = os.path.join(dest, "part.jsonl")
  if os.path.exists(part):
    with open(part, encoding="utf-8") as fh:
      text = fh.read()
    if not text.startswith(f"# {name}"):
      with open(part, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(f"# {name}: added by the parity loop from a Fusion probe\n" + text)
  return name


class FusionCutoff(Exception):
  """The --probe-until hard stop came while a probe still needed Fusion."""


def probe_cases(case_dirs, deadline=None):
  import probe_case  # imported late: it needs FOX_CLI set
  out = {}
  for d in case_dirs:
    if deadline and time.time() > deadline:
      raise FusionCutoff()
    out[os.path.basename(d)] = probe_case.probe(d, write_expected=True)
  return out


def short(v):
  return {"status": v["status"], "fox_vs_fusion": v.get("fox_vs_fusion"),
          "fusion_errors": [str(e)[:300] for e in v["fusion"].get("errors", [])][:3],
          "fusion_unsupported": v["fusion"].get("unsupported", [])[:3],
          "fox_errors": [str(e)[:300] for e in v["fox"].get("errors", [])][:3],
          "fox_failed_features": v["fox"].get("failed_features", [])[:3]}


# ---- the loop ----------------------------------------------------------------------------------

class Loop:
  def __init__(self, a):
    self.a = a
    self.ledger = Ledger(a.ledger)
    self.budget = Budget(a.budget_usd, a.budget_tokens)
    self.probing = set()  # entries this process is probing now
    self.sem = asyncio.Semaphore(a.fixers)
    self.fixing = set()
    self.halt = None  # set when the subscription's usage limit stops an agent
    self.halt_reset = None  # Unix time it resets, when the agent reported one
    self.fusion_down = None  # set when FoxBridge stops answering mid-run
    self.started = 0  # probes and fixers this run started
    self.claimed = set()  # cargo target slots this process holds
    os.environ["FOX_CLI"] = fox_cli()
    os.makedirs(os.path.join(LOOP, "logs"), exist_ok=True)

  def log(self, kind, key):
    return os.path.join(LOOP, "logs", f"{kind}-{slug(key)}.log")

  def commit(self, paths, message):
    if not self.a.commit:
      return
    # Another driver process may be committing on this branch too; retry past its index.lock.
    for attempt in range(10):
      try:
        sh(["git", "add", "--"] + paths)
        if sh(["git", "diff", "--cached", "--quiet"], check=False).returncode:
          sh(["git", "commit", "-m", message + "\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"])
        return
      except RuntimeError as ex:
        if "index.lock" not in str(ex) or attempt == 9:
          print(f"  commit skipped: {ex}"[:400], flush=True)
          return
        time.sleep(1 + attempt)

  def past_cutoff(self):
    """No new probes (or other Fusion work) from --probe-until on."""
    return bool(self.a.probe_cutoff) and time.time() >= self.a.probe_cutoff

  def past_hard_stop(self):
    return bool(self.a.hard_stop) and time.time() >= self.a.hard_stop

  def put_back(self, key, why):
    self.ledger.update(key, status=L.UNTESTED, probe_pid=None, note=f"put back to untested: {why}")
    print(f"  {key}: put back to untested ({why})", flush=True)

  async def cutoff_watchdog(self, probers):
    """At the hard stop, cancels probes still running, puts their entries back and clears Fusion."""
    await asyncio.sleep(max(0, self.a.hard_stop - time.time()))
    running = list(self.probing)
    for t in probers:
      t.cancel()
    await asyncio.gather(*probers, return_exceptions=True)
    try:
      for key in running:
        try:
          self.put_back(key, "still probing at the Fusion cutoff")
        except Exception as ex:  # the entry stays probing and resumes; Fusion must still be cleared
          print(f"  couldn't put {key} back: {ex}", flush=True)
    finally:
      await asyncio.to_thread(clear_fusion)
    print(f"[{stamp()}] Fusion cutoff: probes stopped, scratch documents closed; fixers continue", flush=True)

  def stop_requested(self):
    return os.path.exists(STOP_FILE)

  def usage_stop(self, kind, key, meta):
    """Stops the batch cleanly when the subscription's usage limit is hit. Returns True if it was."""
    if not meta.get("usage_limited"):
      return False
    resets = agents.limit_reset(meta)
    self.halt = f"usage limit hit during {kind} {key}; resets {resets}"
    self.halt_reset = agents.limit_reset_epoch(meta)
    self.ledger.update(key, note=f"{kind} stopped by the usage limit (resets {resets}); the next run resumes it")
    print(f"  STOP: {self.halt}", flush=True)
    return True

  def claim_slot(self):
    """A cargo target directory no other fixer (in any driver process) is using."""
    n = 1
    while True:
      claim = os.path.join(COMMON, "target", f"parity-{n}.claim")
      try:
        with open(claim, encoding="utf-8") as f:
          owner = int(f.read().strip() or 0)
      except (OSError, ValueError):
        owner = 0
      taken = n in self.claimed or (owner != os.getpid() and L.pid_alive(owner))
      if not taken:
        os.makedirs(os.path.dirname(claim), exist_ok=True)
        with open(claim, "w", encoding="utf-8") as f:
          f.write(str(os.getpid()))
        self.claimed.add(n)
        return n
      n += 1

  def release_slot(self, n):
    self.claimed.discard(n)
    with contextlib.suppress(OSError):
      os.unlink(os.path.join(COMMON, "target", f"parity-{n}.claim"))

  # ---- probe ---------------------------------------------------------------------------------

  async def probe(self, entry):
    key = entry["key"]
    scratch = os.path.join(PROBES, slug(key))
    resumed = entry["status"] == L.PROBING and os.path.isdir(scratch)
    os.makedirs(scratch, exist_ok=True)
    attempts = entry.get("probe_attempts", 0) + 1
    self.ledger.update(key, status=L.PROBING, probe_attempts=attempts,
                       note=f"probe {attempts} started" + (" (resuming earlier scratch work)" if resumed else ""))
    print(f"probe {key} ({entry['name']}, {entry['where']})", flush=True)
    prompt = agents.probe_prompt(entry, ROOT, scratch.replace("\\", "/"), fox_cli())
    if resumed:
      prompt += f"\nAn earlier probe of this entry stopped part way. Its cases are in {scratch}: reuse them.\n"
    owner = f"probe-{os.getpid()}-{slug(key)}"
    try:
      report, meta = await agents.run_agent(
        prompt, ROOT, agents.PROBE_SCHEMA, self.budget.agent_cap(self.a.probe_usd), self.a.probe_turns,
        env={"FOXBRIDGE_OWNER": owner}, log_path=self.log("probe", key))
    finally:
      await asyncio.to_thread(release_fusion, owner)
    self.budget.record("probe", key, meta)
    # If Fusion died while the agent worked, its report (often "blocked") says nothing about the
    # feature: this ping raises SystemExit, and the prober puts the entry back.
    await asyncio.to_thread(client.call)
    if self.usage_stop("probe", key, meta):
      self.ledger.update(key, probe_attempts=attempts - 1)  # stays probing; resumes next run
      return
    if not report:
      why = "; ".join(meta.get("errors") or []) or meta.get("subtype") or "no report"
      if attempts >= 2:
        self.ledger.update(key, status=L.BLOCKED, note=f"probe agent failed twice: {why}")
      else:
        self.ledger.update(key, note=f"probe agent ended without a report ({why}); stays probing to resume")
      return
    # Re-run every case here: the ledger records what the driver measured, not what the agent said.
    dirs = [os.path.join(scratch, c["dir"]) for c in report.get("cases", [])
            if os.path.isdir(os.path.join(scratch, c.get("dir", "")))]
    if self.past_hard_stop():
      return self.put_back(key, "the Fusion cutoff came before its cases were re-run")
    try:
      verdicts = await asyncio.to_thread(probe_cases, dirs, self.a.hard_stop) if dirs else {}
    except FusionCutoff:
      return self.put_back(key, "the Fusion cutoff came while its cases were being re-run")
    built = {n: v for n, v in verdicts.items() if v["fusion"]["ok"]}
    if not built:
      status = L.BLOCKED
    elif all(v["status"] in ("match", "match-geometry") for v in built.values()):
      status = L.MATCH
    elif report["status"] in (L.MISMATCH, L.FOX_MISSING):
      status = report["status"]
    else:
      status = L.MISMATCH
    names = []
    for n, v in built.items():
      if os.path.exists(os.path.join(scratch, n, "expected.json")):
        names.append(promote(os.path.join(scratch, n), n))
    summary = {n: short(v) for n, v in verdicts.items()}
    disagree = "" if status == report["status"] else f" (agent said {report['status']})"
    self.ledger.mutate(key, lambda e: e.setdefault("cases", []).extend(names))
    self.ledger.update(
      key, status=status, needs_ui=bool(report.get("needs_ui")),
      probe={"at": now(), "summary": report.get("summary"), "fox_commands": report.get("fox_commands", []),
             "fix_hint": report.get("fix_hint", ""), "verdicts": summary, "cost_usd": round(meta["cost_usd"], 3)},
      note=f"{report.get('summary', '')}{disagree} Cases: {', '.join(names) or 'none'}.")
    print(f"  -> {status}{disagree}; cases {names}", flush=True)
    self.commit([os.path.join(REF, n) for n in names] + [self.a.ledger],
                f"Parity probe: {entry['name']} ({status})")

  # ---- fix -----------------------------------------------------------------------------------

  async def fix(self, key):
    async with self.sem:
      if self.halt or self.stop_requested():
        return
      self.started += 1
      slot = self.claim_slot()
      try:
        await self._fix(key, slot)
      except Exception as ex:  # one failed fixer mustn't stop the loop
        self.ledger.update(key, note=f"fixer error: {ex}"[:1500])
        print(f"  fixer {key} failed: {ex}", flush=True)
      finally:
        self.release_slot(slot)

  async def _fix(self, key, slot):
    entry = self.ledger.get(key)
    fix = dict(entry.get("fix") or {})
    if fix.get("state") == "pushed":
      return await self.open_pr(key)
    if not L.needs_fix(entry) or L.fixer_running_elsewhere(entry) or not self.budget.ok() or self.halt:
      return
    s = slug(key)
    branch = fix.get("branch") or f"parity/{s}"
    wt = fix.get("worktree") or os.path.join(COMMON, ".claude", "worktrees", f"parity-{s}")
    if not os.path.isdir(wt):
      exists = sh(["git", "rev-parse", "--verify", "--quiet", f"refs/heads/{branch}"], check=False).returncode == 0
      sh(["git", "worktree", "add"] + ([wt, branch] if exists else ["-b", branch, wt, self.a.base]), cwd=COMMON)
    cases = entry.get("cases", [])
    for c in cases:
      dst = os.path.join(wt, "tests", "reference", c)
      if not os.path.isdir(dst):
        shutil.copytree(os.path.join(REF, c), dst)
    interrupted = fix.get("state") in ("agent", "interrupted")
    fix.update(branch=branch, worktree=wt, state="agent", slot=slot, pid=os.getpid(),
               attempts=fix.get("attempts", 0) + (0 if interrupted and fix.get("attempts") else 1),
               started=now())
    self.ledger.update(key, fix=fix, note=f"fixer {fix['attempts']} started on {branch} in {wt}")
    print(f"fix {key} on {branch} (slot {slot})", flush=True)
    env = cargo_env(slot)
    verdicts = (entry.get("probe") or {}).get("verdicts") or {}
    report, meta = await agents.run_agent(
      agents.fix_prompt(entry, wt, cases, verdicts, entry.get("probe") or {}), wt, agents.FIX_SCHEMA,
      self.budget.agent_cap(self.a.fix_usd), self.a.fix_turns, env={k: env[k] for k in (
        "CARGO_TARGET_DIR", "FOX_OCCT_DIR") if k in env}, log_path=self.log("fix", key))
    self.budget.record("fix", key, meta)
    if self.usage_stop("fixer", key, meta):
      fix.update(state="interrupted", attempts=fix["attempts"] - 1)  # not the fixer's fault: retry next run
      self.ledger.update(key, fix=fix)
      return
    report = report or {"done": False, "summary": "", "title": "", "blocker": "; ".join(meta.get("errors") or [])}

    if sh(["git", "status", "--porcelain"], cwd=wt).stdout.strip():
      sh(["git", "add", "-A"], cwd=wt)
      sh(["git", "commit", "-m", (report.get("title") or f"Parity fix: {entry['name']}")
          + "\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"], cwd=wt)
    # Check the agent's work the way CI will, before anything leaves this machine: the reference
    # test, plus the unit tests of every crate the branch touches.
    checks = [["cargo", "test", "-p", "fox-engine", "--test", "reference_parts"]]
    checks += [["cargo", "test", "-p", pkg, "--lib", "--", "--skip", "fuzz_found_bugs"]
               for pkg in touched_packages(wt, self.a.base)]
    failed = None
    for cmd in checks:
      label = " ".join(cmd[:5] if "--lib" in cmd else cmd)
      print(f"  checking {branch}: {label}", flush=True)
      test = await asyncio.to_thread(sh, cmd, wt, False, env, 7200)
      with open(self.log("fix", key), "a", encoding="utf-8") as f:
        f.write(f"\n---- driver: {label} (exit {test.returncode}) ----\n" + (test.stdout + test.stderr)[-3000:])
      if not tests_passed(test):
        failed = label
        break
    baseline = open(os.path.join(wt, "tests", "reference", "baseline.txt"), encoding="utf-8").read().split()
    unlisted = [c for c in cases if c not in baseline]
    ahead = int(sh(["git", "rev-list", "--count", f"{self.a.base}..HEAD"], cwd=wt).stdout.strip() or 0)
    passed = not failed and not unlisted and ahead > 0
    fix.update(done=bool(report.get("done")), test_passed=not failed, failed_check=failed, cases_unlisted=unlisted,
               summary=report.get("summary", ""), title=report.get("title", ""), blocker=report.get("blocker", ""),
               finished=now())
    if not passed:
      fix["state"] = "failed"
      why = (f"`{failed}` failed" if failed else
             f"cases not passing: {', '.join(unlisted)}" if unlisted else "no commits")
      self.ledger.update(key, fix=fix, note=f"fixer {fix['attempts']} didn't land: {why}. "
                         f"{report.get('blocker', '')}"[:1500])
      print(f"  -> not fixed ({why})", flush=True)
      return
    sh(["git", "push", "-u", "origin", branch], cwd=wt)
    fix["state"] = "pushed"
    self.ledger.update(key, fix=fix, note=f"fixer passed the reference test and unit tests; pushed {branch}")
    await self.open_pr(key)

  async def open_pr(self, key):
    entry = self.ledger.get(key)
    fix = dict(entry["fix"])
    env = gh_env(self.a.gh_token_from_git)
    if env is None:
      self.ledger.update(key, note=f"PR pending: gh not authenticated. {fix['branch']} is pushed; "
                         "run `gh auth login`, then `parity_loop.py run` again to open the PR")
      print("  gh isn't logged in: branch pushed, PR not opened", flush=True)
      return
    cases = entry.get("cases", [])
    body = (f"{fix.get('summary') or ''}\n\n"
            f"**Fusion feature:** {entry['name']} (`{entry['key']}`, {entry['where']})\n\n"
            f"**Probe:** {(entry.get('probe') or {}).get('summary', '')}\n\n"
            + (f"**New reference cases** (expected.json measured in Fusion): {', '.join(f'`{c}`' for c in cases)}\n\n"
               if cases else "")
            + "`cargo test -p fox-engine --test reference_parts` and the unit tests (`--lib`) of every crate this "
            "branch changes pass on it, with the new cases in baseline.txt.\n\nOpened by the parity loop (`tools/parity-loop`). Not auto-merged.\n\n"
            "🤖 Generated with [Claude Code](https://claude.com/claude-code)")
    title = fix.get("title") or f"Match Fusion's {entry['name']}"
    p = sh([gh_exe(), "pr", "create", "--base", self.a.base, "--head", fix["branch"], "--title", title,
            "--body", body], cwd=fix["worktree"], env=env, check=False)
    url = (p.stdout.strip().splitlines() or [""])[-1]
    if p.returncode or not url.startswith("http"):
      # Already open (a rerun after a crash): look it up instead.
      v = sh([gh_exe(), "pr", "view", fix["branch"], "--json", "url"], cwd=fix["worktree"], env=env, check=False)
      if v.returncode:
        self.ledger.update(key, note=f"gh pr create failed: {(p.stderr or p.stdout).strip()[:500]}")
        return
      url = json.loads(v.stdout)["url"]
    fix["state"] = "pr-open"

    def add(e):
      e["fix"] = fix
      if url not in [p.get("url") for p in e.setdefault("prs", [])]:
        e["prs"].append({"url": url, "branch": fix["branch"], "opened": now()})

    self.ledger.mutate(key, add)
    self.ledger.update(key, status=L.FIXING, note=f"PR opened: {url}")
    print(f"  -> PR {url}", flush=True)
    self.commit([self.a.ledger], f"Parity fix PR for {entry['name']}")

  # ---- verify --------------------------------------------------------------------------------

  async def verify(self):
    env = gh_env(self.a.gh_token_from_git)
    if env is None:
      sys.exit("verify needs gh: run `gh auth login`")
    todo = [e for e in self.ledger.read() if e["status"] == L.FIXING and e.get("prs")]
    if self.past_cutoff():
      sys.exit("verify uses Fusion, which is off-limits after --probe-until; run it once probing is back on")
    merged = []
    for e in todo:
      pr = e["prs"][-1]
      v = sh([gh_exe(), "pr", "view", pr["url"], "--json", "state,mergedAt"], env=env, check=False)
      if v.returncode:
        continue
      state = json.loads(v.stdout)["state"]
      if state == "MERGED":
        merged.append(e)
      elif state == "CLOSED":
        fix = dict(e.get("fix") or {}, state="pr-closed")
        self.ledger.update(e["key"], status=L.MISMATCH, fix=fix, note=f"PR closed without merging: {pr['url']}")
    print(f"{len(todo)} fixing, {len(merged)} merged", flush=True)
    if not merged:
      return
    # Build main as it is now, in its own worktree, and re-probe each merged entry's cases with it.
    wt = os.path.join(COMMON, ".claude", "worktrees", "parity-verify")
    sh(["git", "fetch", "origin", self.a.base], cwd=COMMON)
    if not os.path.isdir(wt):
      sh(["git", "worktree", "add", "--detach", wt, f"origin/{self.a.base}"], cwd=COMMON)
    sh(["git", "checkout", "--detach", f"origin/{self.a.base}"], cwd=wt)
    env = cargo_env("verify")
    print("building fissionoxide-cli from main for verify", flush=True)
    await asyncio.to_thread(sh, ["cargo", "build", "-p", "fissionoxide-cli", "--release"], wt, True, env, 7200)
    os.environ["FOX_CLI"] = os.path.join(env["CARGO_TARGET_DIR"], "release", "fissionoxide-cli" + EXE)
    for e in merged:
      dirs = [os.path.join(wt, "tests", "reference", c) for c in e.get("cases", [])]
      dirs = [d for d in dirs if os.path.isdir(d)]
      verdicts = await asyncio.to_thread(self._reprobe, dirs)
      ok = dirs and all(v["status"] in ("match", "match-geometry") for v in verdicts.values())
      if not e.get("cases"):
        ok = True  # a UI fix has no cases; the merged PR is the evidence
      self.ledger.update(e["key"], status=L.VERIFIED if ok else L.MISMATCH,
                         verify={"at": now(), "verdicts": {n: short(v) for n, v in verdicts.items()}},
                         fix=dict(e.get("fix") or {}, state="merged" if ok else "verify-failed"),
                         note="verified on main after the PR merged" if ok else
                         "PR merged but the cases still differ from Fusion on main")
      print(f"  {e['key']}: {'verified' if ok else 'still differs'}", flush=True)

  def _reprobe(self, dirs):
    import probe_case
    # Re-measure Fox against the committed expected.json-producing Fusion run, without rewriting it.
    return {os.path.basename(d): probe_case.probe(d, write_expected=False) for d in dirs}

  # ---- UI probe ------------------------------------------------------------------------------

  async def ui_probe(self, entry):
    import desktop

    key = entry["key"]
    fox_exe = os.path.join(os.path.dirname(fox_cli()), "fissionoxide" + EXE)
    self.ledger.update(key, note="UI probe started")
    print(f"ui-probe {key} ({entry['name']})", flush=True)
    # The UI probe works Fusion's windows, so it holds Fusion for its whole run.
    owner = f"ui-{os.getpid()}"
    await asyncio.to_thread(client.fusion_acquire, True, owner)
    try:
      report, meta = await agents.run_agent(
        agents.ui_prompt(entry, ROOT, fox_exe), ROOT, agents.UI_SCHEMA, self.budget.agent_cap(self.a.ui_usd),
        self.a.ui_turns, mcp_servers={"desktop": desktop.server()}, tools=["Bash", "Read"],
        env={"FOXBRIDGE_OWNER": owner}, log_path=self.log("ui", key))
    finally:
      await asyncio.to_thread(client.fusion_release, owner)
    self.budget.record("ui", key, meta)
    if self.usage_stop("UI probe", key, meta):
      return
    if not report:
      self.ledger.update(key, note=f"UI probe ended without a report: {meta.get('errors')}")
      return
    self.ledger.update(key, status=report["status"], needs_ui=False, ui={"at": now(), **report},
                       note=f"UI probe: {report['summary']}")
    print(f"  -> {report['status']}", flush=True)
    self.commit([self.a.ledger], f"Parity UI probe: {entry['name']} ({report['status']})")

  # ---- run -----------------------------------------------------------------------------------

  async def run(self):
    tasks = []

    def start_fix(key):
      if key not in self.fixing and not self.a.no_fix:
        self.fixing.add(key)
        tasks.append(asyncio.create_task(self.fix(key)))

    keys = set(self.a.only) if self.a.only else None
    # Resume fixers first: gaps found earlier, and branches pushed without a PR.
    for e in self.ledger.read():
      if L.fixer_running_elsewhere(e):
        print(f"skip {e['key']}: another driver process is fixing it", flush=True)
        continue
      if (keys is None or e["key"] in keys) and (L.needs_fix(e) or (e.get("fix") or {}).get("state") == "pushed"):
        start_fix(e["key"])
    claimed = [0]

    async def prober():
      # Up to --probes of these run at once. Each claims its entry under the ledger lock, so two
      # probes (in this process or another) never take the same one.
      while (claimed[0] < self.a.max_features and self.budget.ok() and not self.halt and not self.stop_requested()
             and not self.past_cutoff() and not self.fusion_down):
        e = self.ledger.claim_next(keys, self.probing)
        if e is None:
          return
        claimed[0] += 1
        self.started += 1
        self.probing.add(e["key"])
        try:
          await self.probe(e)
        except SystemExit as ex:  # client.py exits when FoxBridge is unreachable: Fusion is gone
          self.fusion_down = str(ex)
          self.put_back(e["key"], f"Fusion went away mid-probe ({ex})")
          print(f"[{stamp()}] Fusion is unreachable; no more probes this run, fixers continue", flush=True)
        except Exception as ex:  # one probe's failure mustn't stop the others; the entry resumes
          self.ledger.update(e["key"], note=f"probe error (stays probing): {ex}"[:1500])
          print(f"  probe {e['key']} failed: {ex}", flush=True)
        finally:
          self.probing.discard(e["key"])
        e = self.ledger.get(e["key"])
        if L.needs_fix(e) and not self.halt and not self.stop_requested():
          start_fix(e["key"])

    probers = [asyncio.create_task(prober()) for _ in range(max(1, self.a.probes))]
    watchdog = None
    if self.a.hard_stop and not self.past_hard_stop():
      watchdog = asyncio.create_task(self.cutoff_watchdog(probers))
    await asyncio.gather(*probers, return_exceptions=True)
    if watchdog:
      if self.past_hard_stop():
        await watchdog  # it fired: let it put entries back and clear Fusion
      else:
        watchdog.cancel()
    if self.past_cutoff():
      print(f"[{stamp()}] past the Fusion cutoff: no probes; fixers only", flush=True)
    if self.a.ui and not self.past_cutoff():
      ui = [e for e in self.ledger.read() if e.get("needs_ui") and (keys is None or e["key"] in keys)]
      for e in ui[: self.a.max_features]:
        if not self.budget.ok() or self.halt:
          break
        await self.ui_probe(e)
    if tasks:
      print(f"waiting for {len(tasks)} fixer(s)", flush=True)
      await asyncio.gather(*tasks)
    if self.stop_requested():
      print(f"stopped: {STOP_FILE} exists (delete it before the next run). Unfinished entries resume next run.")
    if self.halt:
      print(f"stopped: {self.halt}. Progress is in the ledger; run the same command again after the reset.")
    if not self.budget.ok():
      print(f"stopped: budget reached (${self.budget.spent_usd:.2f}, {self.budget.spent_tokens:,} tokens)")


def clear_fusion():
  """Closes every FoxBridge scratch document and frees the Fusion lock, whoever holds it."""
  try:
    client._call("close_all", 120)
  except SystemExit:
    pass
  client.fusion_release(force=True)


def release_fusion(owner):
  """After a probe agent ends: close any document it left open and free Fusion if it held it."""
  cur = client._read_lock()
  if cur and cur.get("owner") == owner:
    if cur.get("hold"):
      try:
        client._call("close_all", 120)
      except SystemExit:
        pass
    client.fusion_release(owner)


# ---- until done ----------------------------------------------------------------------------------

RESET_TZ = "America/Chicago"
RESET_AT = (2, 30)  # the user's Max usage resets at 2:30 AM US Central
RESET_MARGIN = 5 * 60


def next_reset(reported=None):
  """When to resume after a usage limit: the reset the agent reported, else the next 2:30 AM
  Central; plus a margin either way."""
  import datetime
  import zoneinfo

  if reported and reported > time.time():
    return reported + RESET_MARGIN
  tz = zoneinfo.ZoneInfo(RESET_TZ)
  now_c = datetime.datetime.now(tz)
  at = now_c.replace(hour=RESET_AT[0], minute=RESET_AT[1], second=0, microsecond=0)
  if at <= now_c:
    at = (now_c + datetime.timedelta(days=1)).replace(hour=RESET_AT[0], minute=RESET_AT[1], second=0,
                                                       microsecond=0)
  return at.timestamp() + RESET_MARGIN


def design_done(entries):
  """True when every Design entry is probed, and every gap has a PR or has used its fixer attempts."""
  for e in entries:
    if e.get("scope") != "design":
      continue
    if e["status"] in (L.UNTESTED, L.PROBING):
      return False
    if L.needs_fix(e) or (e.get("fix") or {}).get("state") in ("pushed", "agent") or L.fixer_running_elsewhere(e):
      return False
  return True


def stamp(t=None):
  return time.strftime("%Y-%m-%d %H:%M %Z", time.localtime(t))


def wait_until(t, why):
  """Sleeps until t, waking each minute to honour the STOP file. Returns False if stopped."""
  print(f"[{stamp()}] waiting: {why}; resuming at {stamp(t)}", flush=True)
  while time.time() < t:
    if os.path.exists(STOP_FILE):
      return False
    time.sleep(min(60, max(1, t - time.time())))
  print(f"[{stamp()}] resuming", flush=True)
  return True


def parse_cutoff(text):
  """--probe-until: 'HH:MM' (the next such time, US Central) or 'YYYY-MM-DD HH:MM' (US Central)."""
  import datetime
  import zoneinfo

  tz = zoneinfo.ZoneInfo(RESET_TZ)
  text = text.strip().replace("T", " ")
  if " " in text:
    at = datetime.datetime.strptime(text, "%Y-%m-%d %H:%M").replace(tzinfo=tz)
  else:
    h, m = (int(x) for x in text.split(":"))
    now_c = datetime.datetime.now(tz)
    at = now_c.replace(hour=h, minute=m, second=0, microsecond=0)
    if at <= now_c:
      at += datetime.timedelta(days=1)
  return at.timestamp()


HARD_STOP_AFTER = 3 * 60  # probes still running this long after the cutoff are stopped


def run_until_done(a):
  """Runs batches until the Design scope is done, waiting out usage limits."""
  a.budget_usd = float("inf")  # the usage limit is the real stop
  a.max_features = 10 ** 9
  batch = 0
  while True:
    batch += 1
    print(f"[{stamp()}] batch {batch}", flush=True)
    loop = Loop(a)
    try:
      asyncio.run(loop.run())
    except Exception as ex:  # an overnight run outlives a bad batch: the ledger has the progress
      import traceback

      traceback.print_exc()
      print(f"[{stamp()}] batch {batch} failed: {ex}; retrying in a minute", flush=True)
      if not wait_until(time.time() + 60, "after a failed batch"):
        return
      continue
    if os.path.exists(STOP_FILE):
      print(f"[{stamp()}] stopped: {STOP_FILE} exists", flush=True)
      return
    if design_done(Ledger(a.ledger).read()):
      print(f"[{stamp()}] done: every Design entry is probed, and every gap has a PR or used its attempts", flush=True)
      return
    if loop.halt:
      if not wait_until(next_reset(loop.halt_reset), loop.halt):
        return
    elif loop.started == 0 and loop.past_cutoff():
      print(f"[{stamp()}] past the Fusion cutoff and no fixer work left: stopping. Probing stays off until "
            "the user restarts the loop.", flush=True)
      return
    elif loop.started == 0:
      # Nothing to start, but not done: another process is probing or fixing what's left.
      if not wait_until(time.time() + 600, "the remaining entries are busy in another driver process"):
        return


def keep_awake(on):
  """Stops Windows idle sleep while the driver runs (a closed lid or a manual Sleep still wins)."""
  if os.name != "nt":
    return
  import ctypes

  ES_CONTINUOUS, ES_SYSTEM_REQUIRED = 0x80000000, 0x00000001
  ctypes.windll.kernel32.SetThreadExecutionState(ES_CONTINUOUS | (ES_SYSTEM_REQUIRED if on else 0))


# ---- gh ----------------------------------------------------------------------------------------

def gh_exe():
  found = shutil.which("gh")
  if found:
    return found
  p = os.path.join(os.environ.get("ProgramFiles", r"C:\Program Files"), "GitHub CLI", "gh.exe")
  return p if os.path.exists(p) else "gh"


def gh_env(from_git=False):
  """Environment for gh, or None when it can't authenticate."""
  env = dict(os.environ)
  if env.get("GH_TOKEN") or env.get("GITHUB_TOKEN"):
    return env
  try:
    if sh([gh_exe(), "auth", "status"], check=False).returncode == 0:
      return env
  except OSError:
    return None
  if from_git:
    # The token git already pushes with, from the credential manager, used for this run only.
    p = subprocess.run(["git", "credential", "fill"], input="protocol=https\nhost=github.com\n\n",
                       capture_output=True, text=True, cwd=ROOT)
    for line in p.stdout.splitlines():
      if line.startswith("password="):
        env["GH_TOKEN"] = line.split("=", 1)[1]
        return env
  return None


# ---- status ------------------------------------------------------------------------------------

def status(a):
  entries = Ledger(a.ledger).read()
  by_scope = {}
  for e in entries:
    by_scope.setdefault(e["scope"], {}).setdefault(e["status"], 0)
    by_scope[e["scope"]][e["status"]] += 1
  print("Ledger: " + os.path.relpath(a.ledger, ROOT))
  for scope, st in sorted(by_scope.items()):
    print(f"  {scope:9} {sum(st.values()):5}  " + "  ".join(f"{k}={v}" for k, v in sorted(st.items())))
  design = [e for e in entries if e["scope"] == "design"]
  tabs = {}
  for e in design:
    tab = e["where"].split(" > ")[1] if e["kind"] == "toolbar" and " > " in e["where"] else "API features"
    tabs.setdefault((e.get("priority", 9), tab), {}).setdefault(e["status"], 0)
    tabs[(e.get("priority", 9), tab)][e["status"]] += 1
  print("\nDesign workspace, by tab (priority order):")
  for (prio, tab), st in sorted(tabs.items()):
    print(f"  p{prio} {tab:24} " + "  ".join(f"{k}={v}" for k, v in sorted(st.items())))
  active = [e for e in design if e["status"] not in (L.UNTESTED,)]
  if active:
    print("\nProbed entries:")
    for e in active:
      fix = e.get("fix") or {}
      pr = (e.get("prs") or [{}])[-1].get("url", "")
      extra = f" fix={fix.get('state')}" if fix else ""
      print(f"  {e['status']:11} {e['key']:44} {e['name'][:28]:28}{extra} {pr}")
  nxt = L.next_untested(entries)
  if nxt:
    print(f"\nNext to probe: {nxt['key']} ({nxt['name']}, {nxt['where']})")
  ui = [e["key"] for e in design if e.get("needs_ui")]
  if ui:
    print(f"Waiting for a UI probe: {len(ui)} ({', '.join(ui[:5])}{', ...' if len(ui) > 5 else ''})")
  try:
    with open(os.path.join(LOOP, "state.json"), encoding="utf-8") as f:
      st = json.load(f)
    print(f"Agent spend so far: ${st['total_usd']:.2f}, {st['total_tokens']:,} tokens over {len(st['runs'])} agent runs")
  except (OSError, ValueError):
    pass


# ---- CLI ---------------------------------------------------------------------------------------

def main():
  for stream in (sys.stdout, sys.stderr):
    stream.reconfigure(encoding="utf-8", errors="replace")  # agents' summaries aren't cp1252
  ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
  ap.add_argument("--ledger", default=os.path.join(ROOT, "parity", "ledger.jsonl"))
  sub = ap.add_subparsers(dest="cmd", required=True)
  sub.add_parser("status", help="summarize the ledger")
  common = argparse.ArgumentParser(add_help=False)
  common.add_argument("--budget-usd", type=float, default=float(os.environ.get("PARITY_BUDGET_USD", 25)),
                      help="stop starting agents once this run has spent this much (default 25)")
  common.add_argument("--budget-tokens", type=int, default=0, help="also stop at this many tokens (0: no cap)")
  common.add_argument("--fixers", type=int, default=2, help="fixer agents at once (default 2)")
  common.add_argument("--base", default="main", help="branch fixers start from and PR into")
  common.add_argument("--probe-usd", type=float, default=4.0, help="cap per probe agent")
  common.add_argument("--fix-usd", type=float, default=20.0, help="cap per fixer agent")
  common.add_argument("--ui-usd", type=float, default=6.0, help="cap per UI probe agent")
  common.add_argument("--probe-turns", type=int, default=120)
  common.add_argument("--fix-turns", type=int, default=300)
  common.add_argument("--ui-turns", type=int, default=150)
  common.add_argument("--commit", action="store_true",
                      help="commit the ledger and new reference cases on the current branch after each step")
  common.add_argument("--gh-token-from-git", action="store_true",
                      help="when gh isn't logged in, authenticate it with the token git pushes with")
  r = sub.add_parser("run", parents=[common], help="probe untested entries and fix what differs")
  r.add_argument("--max-features", type=int, default=3, help="probe at most this many entries (default 3)")
  r.add_argument("--only", nargs="*", help="limit the run to these ledger keys")
  r.add_argument("--no-fix", action="store_true", help="probe only; start no fixers")
  r.add_argument("--probe-until", help="stop using Fusion at this US Central time ('02:55' or "
                 "'2026-10-20 02:55'): no new probes, running ones stopped 3 minutes later; fixers continue")
  r.add_argument("--until-done", action="store_true", help="run batches until every Design entry is done; wait "
                 "out usage limits (resets 2:30 AM Central) instead of exiting; no dollar cap")
  r.add_argument("--probes", type=int, default=1, help="probe agents at once (default 1); they share Fusion "
                 "through the bridge's lock")
  r.add_argument("--ui", action="store_true", help="also UI-probe entries that need it (takes over the mouse)")
  r.add_argument("--yes", action="store_true", help="don't ask before a UI probe takes over the mouse")
  p = sub.add_parser("probe", parents=[common], help="probe one entry")
  p.add_argument("key")
  f = sub.add_parser("fix", parents=[common], help="start a fixer for one entry")
  f.add_argument("key")
  sub.add_parser("verify", parents=[common], help="re-probe entries whose fixer PR has merged")
  u = sub.add_parser("ui-probe", parents=[common], help="probe interaction-only entries by driving the apps")
  u.add_argument("key", nargs="?")
  u.add_argument("--max-features", type=int, default=1)
  u.add_argument("--yes", action="store_true")
  rs = sub.add_parser("reset", help="set an entry back to untested")
  rs.add_argument("key")
  a = ap.parse_args()

  if a.cmd == "status":
    return status(a)
  if a.cmd == "reset":
    Ledger(a.ledger).update(a.key, status=L.UNTESTED, note="reset by hand")
    return print(f"{a.key} -> untested")
  if (a.cmd == "ui-probe" or getattr(a, "ui", False)) and not a.yes:
    if input("A UI probe moves your mouse and types into Fusion and FissionOxide. Continue? [y/N] ").lower() != "y":
      return
  a.probe_cutoff = a.hard_stop = None
  if getattr(a, "probe_until", None):
    a.probe_cutoff = parse_cutoff(a.probe_until)
    a.hard_stop = a.probe_cutoff + HARD_STOP_AFTER
    print(f"probes stop at {stamp(a.probe_cutoff)}; any still running are stopped at {stamp(a.hard_stop)}", flush=True)
  if a.cmd == "run":
    keep_awake(True)
    try:
      if a.until_done:
        return run_until_done(a)
      loop = Loop(a)
      asyncio.run(loop.run())
    finally:
      keep_awake(False)
    return print(f"spent ${loop.budget.spent_usd:.2f}, {loop.budget.spent_tokens:,} tokens")
  loop = Loop(a)
  if a.cmd == "probe":
    asyncio.run(loop.probe(loop.ledger.get(a.key)))
  elif a.cmd == "fix":
    loop.ledger.update(a.key, note="fixer requested by hand")
    asyncio.run(loop.fix(a.key))
  elif a.cmd == "verify":
    asyncio.run(loop.verify())
  elif a.cmd == "ui-probe":
    async def go():
      entries = [loop.ledger.get(a.key)] if a.key else [e for e in loop.ledger.read() if e.get("needs_ui")]
      for e in entries[: a.max_features]:
        if loop.budget.ok():
          await loop.ui_probe(e)
    asyncio.run(go())
  status_line = f"spent ${loop.budget.spent_usd:.2f}, {loop.budget.spent_tokens:,} tokens"
  print(status_line)


if __name__ == "__main__":
  main()
