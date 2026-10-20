# Parity loop

Makes FissionOxide behave like Autodesk Fusion, one Fusion feature at a time. It works through
[`parity/ledger.jsonl`](../../parity/ledger.jsonl), which `tools/fusion-bridge/ledger.py` builds
from Fusion's own command catalog. For each feature it:

1. **Probes** it. A probe agent writes small neutral test cases that cover the feature's options. It
   builds each case in Fusion, through the FoxBridge add-in, and in FissionOxide, then compares the
   two. The driver re-runs every case itself. Cases that Fusion built go into `tests/reference/`
   as new numbered folders, with `expected.json` measured in Fusion. The entry becomes `match`,
   `mismatch`, `fox-missing` or `blocked`.
2. **Fixes** it. For each `mismatch` or `fox-missing` entry, a fixer agent works in its own git
   worktree off `main` and changes FissionOxide until the new cases pass. The driver then runs
   `cargo test -p fox-engine --test reference_parts` itself, plus `cargo test -p <crate> --lib` for every crate the branch changes (fox-sketch and fox-engine together if either changed). If those pass, it pushes the branch and
   opens a PR with `gh`, and the entry becomes `fixing`. Nothing merges automatically.
3. **Verifies** it. Once a PR has merged, `verify` builds `main` and re-probes the entry's cases. The
   entry becomes `verified`, or goes back to `mismatch`.

The agents run on the Claude Agent SDK with `claude-opus-5-5` (set `PARITY_MODEL` to change it).

## Setup

- Fusion is running with the FoxBridge add-in (see [`../fusion-bridge/README.md`](../fusion-bridge/README.md)):
  `py tools/fusion-bridge/client.py ping` answers.
- `fissionoxide-cli` is built: `cargo build -p fissionoxide-cli --release`. The driver looks in
  this checkout's `target/release`, then the main checkout's, or uses `$FOX_CLI`.
- `py -m pip install claude-agent-sdk pillow`. Pillow is only needed for the UI probe. Claude Code
  is installed and logged in; the SDK runs it.
- `gh` is installed and logged in (`gh auth login`), so fixers can open PRs. Without it, a fixer
  pushes its branch and records that the PR is still to open; the next `run` opens it. As an
  alternative, `--gh-token-from-git` gives `gh` the GitHub token that git already pushes with,
  for that run only.

## Commands

```
py tools/parity-loop/parity_loop.py status
py tools/parity-loop/parity_loop.py run --max-features 3 --budget-usd 40 --commit
py tools/parity-loop/parity_loop.py run --only toolbar:PrimitiveCoil api:ribFeatures
py tools/parity-loop/parity_loop.py probe toolbar:ConstraintCoincident
py tools/parity-loop/parity_loop.py fix toolbar:PrimitiveCoil
py tools/parity-loop/parity_loop.py verify
py tools/parity-loop/parity_loop.py ui-probe [KEY] [--max-features N]
py tools/parity-loop/parity_loop.py reset KEY
py tools/parity-loop/probe_case.py CASE_DIR [--write-expected] [--png]
```

**`run`** first restarts unfinished work: fixers for entries that still need one, and PRs for
branches that were pushed without one. It then probes up to `--max-features` entries (default
3). It picks `scope: design` entries with `status: untested`, lowest `priority` first, after any
entry a crash left in `probing`. As each probe finds a gap, `run` starts a fixer for it. Up to
`--probes` probe agents run at once (default 1), and each claims its entry under the ledger lock,
so no two probes (even in two driver processes) take the same one. Fixers run `--fixers` at a
time (default 2). Each fixer claims its own cargo target directory (`target/parity-1`,
`target/parity-2`, ...), and they all share the main checkout's OpenCASCADE build through
`FOX_OCCT_DIR`.

The probes share one Fusion. Fusion runs bridge requests one at a time, so short self-contained
requests (a recipe that closes its document, `compare`, `probe_case.py`) interleave freely.
`client.py` takes a cross-process lock (`%APPDATA%/FoxBridge/fusion.lock`) for every request. A
`python` call, or a `recipe --keep-open`, keeps the lock until that client runs `close`, so no
other probe's document becomes active under it. Each probe agent gets its own owner name
(`$FOXBRIDGE_OWNER`), and when the agent ends the driver closes anything it left open and frees
the lock. A lock idle for 10 minutes is taken over.

| Flag | Default | |
|---|---|---|
| `--max-features N` | 3 | Probe at most N entries this run. |
| `--budget-usd X` | 25 (or `$PARITY_BUDGET_USD`) | Start no new agent once this run has spent X dollars. Each agent is also capped at what's left. |
| `--budget-tokens T` | off | Also stop at T tokens (input, output and cache). |
| `--probe-usd` / `--fix-usd` / `--ui-usd` | 4 / 20 / 6 | Cap per agent. |
| `--probes N` | 1 | Probe agents at once. |
| `--fixers N` | 2 | Fixers at once. |
| `--only KEY ...` | | Limit the run to these ledger keys. |
| `--no-fix` | | Probe only. |
| `--base BRANCH` | main | The branch fixers start from and open PRs against. |
| `--commit` | | Commit the ledger and new cases on the current branch after each step. |
| `--ui` | | Also UI-probe the entries that need one (see below). |
| `--until-done` | | Keep running batches until the Design scope is done (below). No dollar cap. |

### Running until done

`run --until-done --probes 3 --fixers 4 --commit` runs batch after batch. It stops when no Design
entry is `untested` or `probing`, and when every `mismatch` and `fox-missing` entry has a PR or has
used up its fixer attempts. A usage limit doesn't end it. The driver logs the wait, sleeps until
the limit resets, then resumes from the ledger. It uses the reset time the agent's run reported,
or else the next 2:30 AM US Central (the user's Max reset), plus 5 minutes of margin. `STOP`
ends it, waits included. UI probes stay off unless you add `--ui`.

`--probe-until 02:55` (US Central; `'2026-10-20 02:55'` pins a date) hands Fusion back at a set
time. From the cutoff on, no new probe starts, including after a usage-limit resume. Probes
still running 3 minutes later are stopped, and their entries go back to `untested`. Then the
FoxBridge scratch documents are closed and the Fusion lock is freed. Fixers carry on and open PRs
as usual, and `verify`, which uses Fusion, refuses to run. Once the fixers have nothing left, the
run ends. Probing stays off until the loop is started again without the cutoff. A plain `HH:MM`
means the next such time, so restarting with the same flag after the cutoff would allow probing
again the next night; pin the date to avoid that.

While any `run` is going, waits included, the driver asks Windows not to idle-sleep
(`SetThreadExecutionState`), and it clears that when it exits. This prevents idle sleep only and
changes no power settings: closing the laptop lid or choosing Sleep still stops the run. The
next run resumes it.

Spend per agent run, and the running totals, go to `parity/loop/state.json`. Each agent's
transcript goes to `parity/loop/logs/`. `status` prints both.

**`status`** counts entries by scope and status, and by Design tab. It lists every probed entry
with its fix state and PR, names the next entry to probe, and prints the total agent spend.

## Stopping and resuming

To stop a run cleanly, create `parity/loop/STOP`. No new probe or fixer starts, and the running
ones finish. Delete the file before the next run. When the Max plan's usage limit stops an agent,
the run stops the same way and prints when the limit resets.

Every step writes to the ledger before and after it runs. Writes go to a temporary file that's
renamed over the ledger, under `parity/ledger.jsonl.lock`. Stop the driver at any point (Ctrl+C,
a crash, the budget) and run it again:

- An entry left `probing` is probed again before anything new. The agent is pointed at the cases
  it already wrote in `parity/probes/<entry>/`.
- A fixer whose process died is resumed without using up an attempt. A second driver skips an
  entry whose fixer is still running in another live process.
- An entry whose fixer stopped still has its worktree, `.claude/worktrees/parity-<entry>`, and
  its branch, `parity/<entry>`. The next fixer carries on there. Each entry gets two fixer
  attempts; set `no_fix: true` on an entry to stop fixers for it.
- A branch that was pushed but has no PR (`fix.state: pushed`) gets its PR on the next `run`.

## Ledger fields the loop writes

On top of `status` and `history` (one line per step, with a time and a note), the loop writes:

- `cases`: the `tests/reference/` folders the probe added.
- `probe`: the probe's summary, the Fox commands involved, a fix hint, each case's verdict, and
  what the probe cost.
- `needs_ui`: the feature can only be tested by driving the apps' windows.
- `fix`: the branch, the worktree, `state` (`agent`, `failed`, `pushed`, `pr-open`, `pr-closed`,
  `merged` or `verify-failed`), the attempt count, and the test result.
- `prs`: the PRs opened for the entry.
- `verify`: the verdicts from re-probing on `main` after the merge.
- `ui`: a UI probe's report.

## Cases

A probe case is a folder in the `tests/reference/` layout:

- `recipe.json`, the neutral recipe, built in Fusion by
  `tools/fusion-bridge/FoxBridge/recipe_runner.py`. A feature the recipe format can't express can
  use `fusion.py` instead. That's Python run inside a fresh Fusion design, with `design` and `root`
  in scope; it only builds the part.
- `part.jsonl`, the FissionOxide script.
- `expected.json`, written by `probe_case.py --write-expected` from Fusion's measurement. Fusion is
  the answer key.

The reference test doesn't fail on a case that isn't listed in `baseline.txt`, so a probe can add
a case Fox gets wrong. The fixer makes that case pass and lists it.

`parity/known-differences.json` records the reference cases where Fusion itself is wrong, so Fox
shouldn't copy it. `client.py compare` reports those cases as `fusion-differs`.

## UI probe

Some features don't change a part, such as dialogs, heads-up inputs, shortcuts and viewport
tools. The probe marks these `blocked` with `needs_ui: true`. `ui-probe`, or `run --ui`, tests them
by driving both apps on the desktop: screenshots, mouse and keyboard, through an in-process MCP
server (`desktop.py`, Windows only). This takes over your mouse, so:

- it's opt-in: it asks first unless you pass `--yes`;
- it runs one feature at a time;
- don't use the PC while it runs.

It launches FissionOxide from `target/release/fissionoxide.exe` if it isn't already running, and
records what each app does, plus the differences, in the entry's `ui` field. A UI `mismatch`
gets a fixer like any other gap. With no reference cases, that fixer works from the UI report.

## Files

| File | |
|---|---|
| `parity_loop.py` | The driver: `run`, `probe`, `fix`, `verify`, `ui-probe`, `status`, `reset`. |
| `agents.py` | The probe, fixer and UI-probe prompts, their structured-output schemas, and the SDK wrapper. |
| `ledger_io.py` | Locked, atomic ledger reads and writes; picks the next entry. |
| `probe_case.py` | Builds one case in both apps and prints the verdict. Agents and the driver both use it. |
| `desktop.py` | Screen, mouse and keyboard tools for the UI probe. |
