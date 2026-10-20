"""The probe, fixer and UI-probe agents: their prompts, and one wrapper that runs any of them with
the Claude Agent SDK and returns the agent's structured report plus what it cost."""

import json
import os

from claude_agent_sdk import (AssistantMessage, ClaudeAgentOptions, ClaudeSDKError, ResultMessage, TextBlock,
                              ToolUseBlock, query)

MODEL = os.environ.get("PARITY_MODEL", "claude-opus-5-5")
TOOLS = ["Read", "Write", "Edit", "Bash", "Glob", "Grep"]

PROBE_SCHEMA = {
  "type": "object",
  "properties": {
    "status": {"type": "string", "enum": ["match", "mismatch", "fox-missing", "blocked"]},
    "summary": {"type": "string", "description": "Two or three sentences: what was tested and what differs."},
    "cases": {"type": "array", "items": {"type": "object", "properties": {
      "dir": {"type": "string", "description": "Case folder under the scratch directory, e.g. coincident-line-point"},
      "covers": {"type": "string", "description": "Which options of the feature this case exercises"},
      "status": {"type": "string"},
    }, "required": ["dir", "covers", "status"]}},
    "fox_commands": {"type": "array", "items": {"type": "string"},
                     "description": "FissionOxide commands that implement this feature (empty if none)"},
    "needs_ui": {"type": "boolean", "description": "True when the feature is interaction-only (a dialog, a heads-up "
                 "input, a shortcut, a viewport tool) and only a UI probe can test it"},
    "fix_hint": {"type": "string", "description": "For mismatch or fox-missing: where in FissionOxide the gap is "
                 "and what a fix needs to do"},
  },
  "required": ["status", "summary", "cases", "fox_commands", "needs_ui"],
}

FIX_SCHEMA = {
  "type": "object",
  "properties": {
    "done": {"type": "boolean", "description": "True when every new case passes and the reference test passes"},
    "summary": {"type": "string", "description": "What changed and why, for the PR description"},
    "title": {"type": "string", "description": "PR title, under 70 characters, sentence case, no period"},
    "cases_passing": {"type": "array", "items": {"type": "string"}},
    "cases_failing": {"type": "array", "items": {"type": "string"}},
    "blocker": {"type": "string", "description": "When not done: what stops the fix"},
  },
  "required": ["done", "summary", "title", "cases_passing", "cases_failing"],
}

UI_SCHEMA = {
  "type": "object",
  "properties": {
    "status": {"type": "string", "enum": ["match", "mismatch", "fox-missing", "blocked"]},
    "summary": {"type": "string"},
    "fusion_behaviour": {"type": "string", "description": "Step by step: what Fusion does"},
    "fox_behaviour": {"type": "string", "description": "Step by step: what FissionOxide does"},
    "differences": {"type": "array", "items": {"type": "string"}},
    "fix_hint": {"type": "string"},
  },
  "required": ["status", "summary", "fusion_behaviour", "fox_behaviour", "differences"],
}


def entry_brief(entry):
  keep = ("key", "kind", "name", "tooltip", "where", "paths", "options", "create_methods", "fox_candidates",
          "notes", "cases", "ui", "history")
  e = {k: entry[k] for k in keep if k in entry}
  e["paths"] = e.get("paths", [])[:4]
  e["history"] = e.get("history", [])[-6:]
  return json.dumps(e, indent=1, ensure_ascii=False)


def probe_prompt(entry, root, scratch, fox_cli):
  return f"""You are probing one Autodesk Fusion feature to see whether FissionOxide (Fox), a Rust and
OpenCASCADE parametric CAD app in this repository, does the same thing. Fusion is running on this PC
with the FoxBridge add-in, which serves Fusion's API to scripts.

The feature, from parity/ledger.jsonl:
{entry_brief(entry)}

`fox_candidates` are guesses from names. Check them; don't trust them.

Your job:
1. Work out what the feature does in Fusion and which of its options change the resulting geometry.
   For an `api:` entry, `options` lists the input object's members. For a `toolbar:` entry, the
   tooltip says what it does. You can read Fusion's API from inside Fusion with
   `py tools/fusion-bridge/client.py python "result = dir(adsk.fusion.ExtrudeFeatureInput)"` (or `@file.py`).
2. Find the Fox side: `"{fox_cli}" commands` lists every Fox command, one JSON object per line
   (grep it rather than reading all of it). The Rust source is under crates/ (engine, model, sketch, kernel, ui).
3. Write 1 to 4 small neutral test cases that between them cover the feature's options that change
   geometry. Put each case in its own folder under `{scratch}`, named in kebab-case without a number
   (the driver numbers them when it adds them to tests/reference/). Each folder holds:
   - `recipe.json` in the tests/reference format (read tests/reference/README.md and copy the shape
     of a similar case, e.g. tests/reference/08-chamfer/recipe.json; the ops the Fusion runner
     supports are the `op_*` methods in tools/fusion-bridge/FoxBridge/recipe_runner.py), OR
     `fusion.py` when the recipe format can't express the feature: plain Python run inside Fusion
     in a fresh design with `app`, `adsk`, `design` and `root` (the root component) in scope; it
     only builds the part. Prefer recipe.json. You may extend recipe_runner.py with a new op when
     that is straightforward; keep its style and report anything it can't build as Unsupported.
   - `part.jsonl`: the same part as a Fox command script, one JSON command per line. Copy the
     style of tests/reference/08-chamfer/part.jsonl. If Fox has no command for the feature, write
     the closest script you can, or none, and report fox-missing.
   Cases must produce a solid (or bodies) whose volume, area, bounding box and face count show
   whether the feature worked. Keep them small: one sketch, one or two features.
4. Run each case: `py tools/parity-loop/probe_case.py {scratch}/<case> --write-expected`. It builds the
   part in Fusion and in Fox, compares them with the tests/reference tolerances, writes
   expected.json from Fusion's measurement, and prints a verdict. Fusion is the answer key. If
   Fusion fails to build a case, fix the case (or the runner) and run it again. If a Fox script
   fails because you wrote it wrong, fix the script; if it fails because Fox lacks or mis-builds
   the feature, that's the finding: don't work around it.
5. Report with the structured output:
   - match: every case Fusion built also matches in Fox (match or match-geometry).
   - mismatch: Fox has the feature but at least one case measures differently or fails to build.
   - fox-missing: Fox has no command for the feature.
   - blocked: the feature can't be tested by building geometry (it doesn't change a part: a
     view, a dialog, an inspection or a data command), or Fusion can't build it from a script.
     Set needs_ui true when a UI probe (driving both apps' windows) could test it.
   For mismatch and fox-missing, put in fix_hint where in Fox the gap is.

Rules:
- Probes run one at a time against a single Fusion. Only use the bridge through client.py and
  probe_case.py. Close any document you open with --keep-open (`py tools/fusion-bridge/client.py close`).
- Don't change FissionOxide's Rust code, tests/reference/ or the ledger: the driver does that. Only
  write under `{scratch}` (and tools/fusion-bridge/FoxBridge/recipe_runner.py if you extend it).
- Run Python with `py`. The repository is {root}. Keep going without asking questions.
"""


def cases_text(cases, verdicts):
  if not cases:
    return ("There are no reference cases: this gap was found by a UI probe (see `ui` and the history in "
            "the entry). Make Fox behave as the probe describes Fusion behaving, test it with the crate's "
            "own tests (add one), and ignore the reference-case steps below except that the reference test "
            "must still pass.")
  return (f"It added these reference cases to tests/reference/ (they're already in this worktree): "
          f"{', '.join(cases)}. Each has recipe.json (the neutral recipe) or fusion.py (the Fusion script), "
          "part.jsonl (the Fox script) and expected.json, which was measured in Fusion and is the answer "
          "key. Their last verdicts, Fusion vs Fox:\n" + json.dumps(verdicts, indent=1, ensure_ascii=False)[:6000])


def fix_prompt(entry, worktree, cases, verdicts, probe_report):
  return f"""You are fixing a gap between FissionOxide (Fox), the Rust and OpenCASCADE parametric CAD
app in this repository, and Autodesk Fusion. You're in a git worktree at {worktree}, on its own
branch off main.

The Fusion feature, from parity/ledger.jsonl:
{entry_brief(entry)}

The probe agent reported:
{json.dumps(probe_report, indent=1, ensure_ascii=False)}

{cases_text(cases, verdicts)}

Your job: change FissionOxide so these cases build and match expected.json, the way Fusion does.
- Read tests/reference/README.md and CONTRIBUTING.md first. Look at how similar features are
  implemented (crates/engine, crates/model, crates/kernel, crates/sketch) and follow that code's style.
- If Fox lacks the feature, add it as a command the script can call, with the options the cases use.
  Edit part.jsonl only to call the new command or to fix a script that misstates the recipe;
  never edit expected.json or recipe.json.
- Check with `cargo test -p fox-engine --test reference_parts` (filter while iterating:
  `cargo test -p fox-engine --test reference_parts -- --nocapture` prints each case). It must
  pass: no case listed in tests/reference/baseline.txt or edit-baseline.txt may regress. Add
  each new case that now passes to baseline.txt (and edit-baseline.txt when the test says so).
  `cargo build -p fissionoxide-cli --release` then `target/release/fissionoxide-cli run
  tests/reference/<case>/part.jsonl` runs one script.
- Run `cargo fmt --all` and `cargo clippy -p <crates you changed> --all-targets -- -D warnings`
  before you finish.
- Commit your work on this branch with a clear message (end it with the line
  "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"). Don't push, don't open a PR, don't
  switch branches: the driver pushes and opens the PR after it re-runs the test.
- The first build of a fresh target directory takes several minutes; the OpenCASCADE build is
  shared through FOX_OCCT_DIR, which is already set. Run long cargo commands in the foreground
  with a long timeout.
- Keep going without asking questions. If you can't make every case pass, commit what helps,
  report done false and say what blocks the rest.
"""


def ui_prompt(entry, root, fox_exe):
  return f"""You are comparing one interaction-only feature of Autodesk Fusion with FissionOxide (Fox),
the CAD app in this repository, by driving both apps on this Windows desktop. Use the `desktop`
tools (screenshot, click, type, key, scroll, focus_window) to work the real windows. The user
opted in to this and isn't using the mouse while it runs.

The feature, from parity/ledger.jsonl:
{entry_brief(entry)}

1. Bring Fusion to the front (focus_window "Fusion"). Start from a new design if needed. Use the
   feature the way a user would: its toolbar button, shortcut, dialog, heads-up inputs, and the
   options that change its behaviour. Note each step and what Fusion shows.
2. Do the same in Fox. Start it with the Bash tool if it isn't running: `"{fox_exe}"` (run it in
   the background). Find the matching command (`fox_candidates` are guesses).
3. Compare what a Fusion user would notice: where the command lives, how it's started, the dialog's
   inputs and defaults, the heads-up inputs, selection behaviour, previews, keyboard shortcuts,
   and the result.
4. Leave both apps as you found them: cancel dialogs, close documents you created without saving.
5. Report with the structured output. match: a Fusion user would find nothing different. mismatch:
   Fox has the feature but behaves differently (list each difference). fox-missing: Fox doesn't
   have it. blocked: you couldn't run it in one of the apps.

Don't edit any files. The repository is {root}. Take a screenshot after each action to check it
worked before going on. Keep going without asking questions.
"""


async def run_agent(prompt, cwd, schema, budget_usd, max_turns, env=None, mcp_servers=None, tools=None,
                    log_path=None, effort="high"):
  """Runs one agent to completion. Returns (report or None, ResultMessage fields as a dict)."""
  opts = ClaudeAgentOptions(
    model=MODEL,
    cwd=cwd,
    tools=tools or TOOLS,
    allowed_tools=(tools or TOOLS) + ([f"mcp__{n}" for n in (mcp_servers or {})]),
    permission_mode="bypassPermissions",
    max_turns=max_turns,
    max_budget_usd=budget_usd,
    effort=effort,
    # Only the MCP servers given here: the user's claude.ai connectors would add a tool list of
    # hundreds of thousands of tokens to every turn.
    env={"ENABLE_CLAUDEAI_MCP_SERVERS": "false", **(env or {})},
    mcp_servers=mcp_servers or {},
    strict_mcp_config=True,
    output_format={"type": "json_schema", "schema": schema},
    setting_sources=["project"],
  )
  result, texts, structured, failure = None, [], None, None
  log = open(log_path, "a", encoding="utf-8") if log_path else None
  try:
    async for msg in query(prompt=prompt, options=opts):
      if isinstance(msg, AssistantMessage):
        for b in msg.content:
          if isinstance(b, TextBlock):
            texts.append(b.text)
            if log:
              log.write(b.text.rstrip() + "\n\n")
              log.flush()
          elif isinstance(b, ToolUseBlock):
            if b.name == "StructuredOutput":
              structured = b.input  # kept in case the run then fails (budget, turns)
            if log:
              log.write(f"> {b.name} {json.dumps(b.input, ensure_ascii=False)[:400]}\n")
              log.flush()
      elif isinstance(msg, ResultMessage):
        result = msg
  except ClaudeSDKError as ex:
    failure = str(ex)
    if log:
      log.write(f"\n!! agent stopped: {failure}\n")
  finally:
    if log:
      log.close()
  if result is None:
    # The SDK raises on a budget stop instead of returning the result; the cap was spent.
    cost = budget_usd if failure and "maximum budget" in failure else 0.0
    return structured, {"is_error": True, "errors": [failure or "agent produced no result"], "cost_usd": cost,
                        "tokens": 0, "subtype": "error"}
  usage = result.usage or {}
  tokens = sum(int(usage.get(k) or 0) for k in (
    "input_tokens", "output_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"))
  meta = {"is_error": result.is_error, "subtype": result.subtype, "cost_usd": result.total_cost_usd or 0.0,
          "tokens": tokens, "turns": result.num_turns, "session_id": result.session_id,
          "errors": result.errors or []}
  report = result.structured_output or structured
  if report is None:
    report = last_json(result.result or "\n".join(texts))
  return report, meta


def last_json(text):
  """The last JSON object in free text, for a run that ended without structured output."""
  dec = json.JSONDecoder()
  found = None
  i = text.find("{")
  while i != -1:
    try:
      obj, end = dec.raw_decode(text[i:])
      if isinstance(obj, dict):
        found = obj
      i = text.find("{", i + max(end, 1))
    except ValueError:
      i = text.find("{", i + 1)
  return found
