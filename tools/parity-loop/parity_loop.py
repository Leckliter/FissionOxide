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
import json
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "fusion-bridge"))

import agents  # noqa: E402
import ledger_io as L  # noqa: E402
from ledger_io import Ledger, now, slug  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
REF = os.path.join(ROOT, "tests", "reference")
LOOP = os.path.join(ROOT, "parity", "loop")
PROBES = os.path.join(ROOT, "parity", "probes")
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
    os.replace(self.path + ".tmp", self.path)
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


def probe_cases(case_dirs):
  import probe_case  # imported late: it needs FOX_CLI set
  return {os.path.basename(d): probe_case.probe(d, write_expected=True) for d in case_dirs}


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
    self.fusion_lock = asyncio.Lock()  # one Fusion: probes, verifies and UI probes take turns
    self.sem = asyncio.Semaphore(a.fixers)
    self.slots = list(range(1, a.fixers + 1))
    self.fixing = set()
    os.environ["FOX_CLI"] = fox_cli()
    os.makedirs(os.path.join(LOOP, "logs"), exist_ok=True)

  def log(self, kind, key):
    return os.path.join(LOOP, "logs", f"{kind}-{slug(key)}.log")

  def commit(self, paths, message):
    if not self.a.commit:
      return
    sh(["git", "add", "--"] + paths)
    if sh(["git", "diff", "--cached", "--quiet"], check=False).returncode:
      sh(["git", "commit", "-m", message + "\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"])

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
    async with self.fusion_lock:
      report, meta = await agents.run_agent(
        prompt, ROOT, agents.PROBE_SCHEMA, self.budget.agent_cap(self.a.probe_usd), self.a.probe_turns,
        log_path=self.log("probe", key))
      self.budget.record("probe", key, meta)
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
      verdicts = await asyncio.to_thread(probe_cases, dirs) if dirs else {}
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
      slot = self.slots.pop(0)
      try:
        await self._fix(key, slot)
      except Exception as ex:  # one failed fixer mustn't stop the loop
        self.ledger.update(key, note=f"fixer error: {ex}"[:1500])
        print(f"  fixer {key} failed: {ex}", flush=True)
      finally:
        self.slots.append(slot)

  async def _fix(self, key, slot):
    entry = self.ledger.get(key)
    fix = dict(entry.get("fix") or {})
    if fix.get("state") == "pushed":
      return await self.open_pr(key)
    if not L.needs_fix(entry) or not self.budget.ok():
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
    fix.update(branch=branch, worktree=wt, state="agent", attempts=fix.get("attempts", 0) + 1, slot=slot,
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
    report = report or {"done": False, "summary": "", "title": "", "blocker": "; ".join(meta.get("errors") or [])}

    # Check the agent's work the same way CI will, before anything leaves this machine.
    print(f"  checking {branch}: cargo test -p fox-engine --test reference_parts", flush=True)
    test = await asyncio.to_thread(
      sh, ["cargo", "test", "-p", "fox-engine", "--test", "reference_parts"], wt, False, env, 7200)
    tail = (test.stdout + test.stderr)[-3000:]
    with open(self.log("fix", key), "a", encoding="utf-8") as f:
      f.write("\n---- driver: cargo test -p fox-engine --test reference_parts ----\n" + tail)
    baseline = open(os.path.join(wt, "tests", "reference", "baseline.txt"), encoding="utf-8").read().split()
    unlisted = [c for c in cases if c not in baseline]
    if sh(["git", "status", "--porcelain"], cwd=wt).stdout.strip():
      sh(["git", "add", "-A"], cwd=wt)
      sh(["git", "commit", "-m", (report.get("title") or f"Parity fix: {entry['name']}")
          + "\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"], cwd=wt)
    ahead = int(sh(["git", "rev-list", "--count", f"{self.a.base}..HEAD"], cwd=wt).stdout.strip() or 0)
    passed = test.returncode == 0 and not unlisted and ahead > 0
    fix.update(done=bool(report.get("done")), test_passed=test.returncode == 0, cases_unlisted=unlisted,
               summary=report.get("summary", ""), title=report.get("title", ""), blocker=report.get("blocker", ""),
               finished=now())
    if not passed:
      fix["state"] = "failed"
      why = ("the reference test failed" if test.returncode else
             f"cases not passing: {', '.join(unlisted)}" if unlisted else "no commits")
      self.ledger.update(key, fix=fix, note=f"fixer {fix['attempts']} didn't land: {why}. "
                         f"{report.get('blocker', '')}"[:1500])
      print(f"  -> not fixed ({why})", flush=True)
      return
    sh(["git", "push", "-u", "origin", branch], cwd=wt)
    fix["state"] = "pushed"
    self.ledger.update(key, fix=fix, note=f"fixer passed the reference test; pushed {branch}")
    await self.open_pr(key)

  async def open_pr(self, key):
    entry = self.ledger.get(key)
    fix = dict(entry["fix"])
    env = gh_env(self.a.gh_token_from_git)
    if env is None:
      self.ledger.update(key, note=f"{fix['branch']} is pushed but gh isn't logged in, so no PR yet "
                         "(run `gh auth login`, then `parity_loop.py run` again)")
      print("  gh isn't logged in: branch pushed, PR not opened", flush=True)
      return
    cases = entry.get("cases", [])
    body = (f"{fix.get('summary') or ''}\n\n"
            f"**Fusion feature:** {entry['name']} (`{entry['key']}`, {entry['where']})\n\n"
            f"**Probe:** {(entry.get('probe') or {}).get('summary', '')}\n\n"
            + (f"**New reference cases** (expected.json measured in Fusion): {', '.join(f'`{c}`' for c in cases)}\n\n"
               if cases else "")
            + "`cargo test -p fox-engine --test reference_parts` passes on this branch, with the new cases in "
            "baseline.txt.\n\nOpened by the parity loop (`tools/parity-loop`). Not auto-merged.\n\n"
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
      async with self.fusion_lock:
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
    async with self.fusion_lock:
      report, meta = await agents.run_agent(
        agents.ui_prompt(entry, ROOT, fox_exe), ROOT, agents.UI_SCHEMA, self.budget.agent_cap(self.a.ui_usd),
        self.a.ui_turns, mcp_servers={"desktop": desktop.server()}, tools=["Bash", "Read"],
        log_path=self.log("ui", key))
    self.budget.record("ui", key, meta)
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
      if (keys is None or e["key"] in keys) and (L.needs_fix(e) or (e.get("fix") or {}).get("state") == "pushed"):
        start_fix(e["key"])
    probed = 0
    while probed < self.a.max_features and self.budget.ok():
      e = L.next_untested(self.ledger.read(), keys)
      if e is None:
        break
      await self.probe(e)
      probed += 1
      e = self.ledger.get(e["key"])
      if L.needs_fix(e):
        start_fix(e["key"])
    if self.a.ui:
      ui = [e for e in self.ledger.read() if e.get("needs_ui") and (keys is None or e["key"] in keys)]
      for e in ui[: self.a.max_features]:
        if not self.budget.ok():
          break
        await self.ui_probe(e)
    if tasks:
      print(f"waiting for {len(tasks)} fixer(s)", flush=True)
      await asyncio.gather(*tasks)
    if not self.budget.ok():
      print(f"stopped: budget reached (${self.budget.spent_usd:.2f}, {self.budget.spent_tokens:,} tokens)")


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
  loop = Loop(a)
  if a.cmd == "run":
    asyncio.run(loop.run())
  elif a.cmd == "probe":
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
