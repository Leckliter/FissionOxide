# Parity loop runbook

The steps an unattended Claude session follows to compare FissionOxide against Autodesk Fusion,
one feature at a time, and get every gap fixed. It is written for the scheduled 3:00 AM run, but
any session can follow it.

The state lives in two files, both handled through `tools/parity-loop/loop.py`:

- `parity/ui-features.jsonl` lists interaction behaviours: hover highlights, ViewCube hot spots,
  menus and drag gestures. `seed_ui.py` writes the first set, and discovery adds more.
- `parity/ledger.jsonl` lists Fusion's commands and API features, from `tools/fusion-bridge`.

## Ground rules

- Control only Autodesk Fusion (`fusion360.exe`) and FissionOxide. If anything else takes focus,
  such as a Windows dialog, an update prompt or a sign-in page, stop that probe and record it as
  `blocked`.
- In Fusion, work only in a new untitled design. Never save, upload, share, sign out, open existing
  projects, or accept any terms, trial or extension offer. Close scratch documents without saving.
- In FissionOxide, work in a new document and never overwrite a file.
- Record every finding with `loop.py record` straight away, and commit the ledgers every 20 or so
  probes. The session may stop at any point, and nothing that was recorded should be lost.
- Take a screenshot before every click. The windows move.

## Phase 0: preflight

1. `py tools/fusion-bridge/client.py ping` should answer. If it doesn't, start Fusion, then turn
   FoxBridge on with Shift+S.
2. Take a screenshot. Put Fusion and FissionOxide side by side on the primary display, or switch
   between them, and open a new empty design in each.
3. `py tools/parity-loop/loop.py summary`.
4. Work on the `parity-loop-bridge` branch. Commit ledger updates there.

## Phase 1: discovery

Walk Fusion's UI looking for behaviours the catalog lacks. Cover every toolbar tab and panel
dropdown, every right-click menu (canvas, face, edge, body, sketch, browser node, timeline icon),
the ViewCube and its menu, the navigation bar flyouts, the File menu, Preferences (read only), and
the S toolbox. Add each new behaviour with:

    py tools/parity-loop/loop.py add AREA SLUG "what Fusion does, precisely"

Allow about 45 minutes, and keep going later if new areas turn up.

## Phase 2: probe loop

Repeat until nothing is left or the run reaches 9:00 AM:

1. `py tools/parity-loop/loop.py next --n 1`, then record the entry as `probing`.
2. **Fusion:** perform the behaviour and note exactly what happens: hover state, highlight
   colour, animation, menu contents, dialog fields. Zoom in to check details.
3. **FissionOxide:** do the same thing.
4. Record the result:
   - `match`: it behaves the same, near enough that a Fusion user wouldn't notice.
   - `mismatch`: it exists but behaves differently. Say how in `--note`.
   - `fox-missing`: FissionOxide doesn't have it.
   - `blocked`: it can't be probed (needs a cloud service, sign-in, or paid extension).

   Give `--severity` (P0 breaks the basic workflow, P1 is something a Fusion user hits within
   minutes, P2 is polish). Give `--files` with the FissionOxide source to start from, found by
   grepping `crates/`, and `--shot` with a saved screenshot if it helps.
5. Go UI entries first (the `next` order), then ledger entries by priority. The 451 Design-scope
   commands won't all fit in one night, so get through priorities 1 and 2.

Before each probe, read `docs/fusion-parity-notes.md` and the merged commits on `main`. Many of
those items were fixed on 2026-10-19, so re-probe them rather than assuming.

## Phase 3: fix batches

Once the UI entries are done, or by 6:00 AM, whichever comes first:

1. `py tools/parity-loop/loop.py batches` writes `parity/batches/<area>-N.md`, with up to 8 gaps
   in each.
2. Commit, and push `parity-loop-bridge` so the batch files are on GitHub.
3. Start one fixer per batch. Each fixer works from `main`, reads its batch file, makes the fix,
   runs `cargo test` and `cargo clippy`, and opens a PR against `main` titled
   `Parity: <batch name>`. Try these in order:
   1. `start_session` with `target: "cloud"`, one session per batch.
   2. The Agent tool with `isolation: "remote"`.
   3. The Agent tool with `isolation: "worktree"`, running locally, at most 6 at a time.
4. `loop.py mark-batch NAME fixing --pr URL` once each PR exists.
5. Keep probing (phase 2) while the fixers work, then batch again for a second wave.

## Phase 4: merge, verify, archive

1. For each PR: check it out, run `cargo test`, then `gh pr merge --merge`. If it conflicts with
   something merged earlier, rebase it, fix the conflicts and test again. Don't merge a PR whose
   tests fail. Leave it open and note it in the report.
2. Merge `parity-loop-bridge` into `main` too.
3. Run `cargo build --release` on `main`. Close FissionOxide, copy `target/release/fissionoxide.exe`
   and `fissionoxide-cli.exe` over the copies in `C:\Program Files\FissionOxide\`, and reopen it.
4. Re-probe every `fixing` entry. Record `verified`, or `mismatch` with a note for a later wave.
5. Archive the fixer sessions and delete the merged remote branches.
6. Write `parity/reports/<date>.md`: counts before and after, what was merged, what's still open,
   and anything blocked that needs a human. Commit it to `main` and push.
