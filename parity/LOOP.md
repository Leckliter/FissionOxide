# The parity loop

Goal, as the owner set it on 2026-10-21: FissionOxide becomes a complete copy of Autodesk Fusion,
with every feature working as it should and every UI behaviour matching, down to hover
highlights, selection colours, cursor prompts, tooltips, animations and dialog layouts. Until
then, two kinds of agents run without anyone restarting them:

- **Probers** drive Fusion and FissionOxide side by side with desktop control, record every
  difference in `parity/ui-features.jsonl` and `parity/ledger.jsonl`, and write assignable
  batches in `parity/batches/`.
- **Fixers** take one batch each, make FissionOxide match, verify, push to `main`, and archive
  themselves.

Every prober hands off to the next prober and starts fixers for the batches it found. This file
is the whole protocol; a new agent needs nothing else from the conversation that started it.

## Files

| File | What it is |
|---|---|
| `parity/LOOP.md` | this protocol |
| `parity/coverage.md` | the checklist of Fusion areas to probe, with status; probers pick the next `todo` area and mark it |
| `parity/progress.md` | where the last prober stopped and what the next one should do first |
| `parity/ui-features.jsonl` | one row per UI behaviour (`key`, `area`, `fusion`, `status`, `notes`, `severity`, `files`, `batch`) |
| `parity/ledger.jsonl` | one row per Fusion command/API feature (the command-parity driver's ledger) |
| `parity/batches/<name>.md` | one batch of related items for one fixer, in the format below |
| `parity/batches/map-wave<N>.json` | key → batch for each wave of batches |
| `parity/batches/index.jsonl` | batch name, item count, status (`ready`, `fixing`, `done`), PR or commit |
| `parity/batches/FIXER.md` | the brief every fixer follows |
| `parity/reports/<date>-<topic>.md` | a narrative report per probing session |
| `docs/fusion-parity-notes.md` | the running history; append a short section per session |

## Prober protocol

### Setup
1. Pull `main`, build the app in release (`cargo build --release -p fissionoxide -p fissionoxide-cli`),
   close any running `fissionoxide.exe`, and launch `target/release/fissionoxide.exe`. Launch
   Fusion from `C:\Users\Leckliter\AppData\Local\Autodesk\webdeploy\production\736d2b4c5e6b95e77c11721da222cf21c6a1fc1f\Fusion360.exe`
   if it is not running. Use `request_access` for "Autodesk Fusion", "fusion360.exe",
   "fissionoxide.exe" and "File Explorer" (taskbar clicks).
2. **The Claude desktop app keeps a topmost window over the right of the screen** (seen at
   1372..1908 x 20..748 on the 1920x1080 display). Clicks under it never reach the app and look
   like app bugs. Place each app window left of it (restore and drag, or `Win+Left`), and before
   calling any click failure a bug, check with `WindowFromPoint` (PowerShell `Add-Type` with
   `user32!WindowFromPoint`, see `parity/reports/2026-10-21.md`, "Retractions").
3. Swap between the apps with their taskbar buttons. Clicking the button of the active window
   minimises it, so click it again. Use `mouse_move` and a short wait before each click, and
   `zoom` for anything small.
4. Read `parity/coverage.md` and `parity/progress.md`, pick the first area marked `todo`, set it
   to `in progress` with your session's date, and commit that so a second prober does not take
   the same area.

### Probing
For the chosen area, do the same actions in both apps and compare, in this order, for every
command, panel or control:
- the ribbon/menu entry: label, icon, shortcut hint, tooltip (hover 1 s, then 3 s for the
  expanded tooltip), enabled state;
- opening it: dialog title, every row and its control type (icon toggles vs text), default
  values with units, which rows appear only after a pick, OK/Cancel enabled state;
- the canvas during it: cursor prompt text, selection filter, hover highlight colour, selected
  colour, preview colour, manipulators (arrows, rings, floating value boxes and what has focus),
  status-bar readout, timeline change, browser change;
- keyboard: typed digits, Tab between boxes, Enter, Escape, Space repeat, the command's letter;
- after OK: the result, the timeline icon, the browser rows, the undo list entry, status text;
- edit: double-click the timeline icon and compare the edit dialog;
- right-click: the marking menu ring and list for the result (face, edge, body, sketch, feature);
- hover states on every control you touch (buttons, rows, tabs, palette rows, ViewCube spots,
  browser rows with their canvas highlight), and colour changes on hover, selection, press.
Fusion is the reference even where it seems odd; record "Fusion does X, FissionOxide does Y".
If Fusion refuses a feature on this account (Personal licence), record `blocked`.

### Recording
- One row per difference in `parity/ui-features.jsonl` (append; keys are `ui:<area>:<slug>`,
  unique; `status` is `mismatch`, `fox-missing`, `match`, `verified` or `blocked`; `severity`
  P0 blocks the basic workflow, P1 a Fusion user hits in minutes, P2 polish; `files` are the
  code pointers; `batch` the batch name). Also record matches, so coverage is visible.
- Code pointers: `grep` the relevant strings in `crates/ui/src` (`panels.rs` chrome, browser,
  timeline, palette; `dialogs.rs` command dialogs; `viewport.rs` canvas, sketch tools, HUD;
  `marking.rs` marking menus; `menus.rs` ribbon lists; `navbar.rs`, `viewcube.rs`,
  `preferences.rs`; `crates/engine/src/scene.rs` colours and grid; `crates/engine/src/commands.rs`
  commands). Give `file:line`.
- Batches: group 2-8 related items that touch the same files into `parity/batches/<name>.md`
  in the existing format (header paragraph, then `## <name> (P1)` with `- key:`, `- Fusion:`,
  `- FissionOxide:`, `- Start here:`). One batch per fixer. Add the keys to a new
  `map-wave<N>.json` and a `{"name","items","status":"ready","pr":null}` line to `index.jsonl`.
  The script in `parity/reports/2026-10-21.md`'s session (python, reads the batch files and
  appends the ledger rows) is the easiest way: see `parity/batches/ledger_from_batches.py`.
- Report: `parity/reports/<date>-<topic>.md` with what matched, the differences by priority with
  pointers, retractions, the batch table, and what is not covered. Append a short section to
  `docs/fusion-parity-notes.md`.
- Commit the parity files only (`git add parity docs/fusion-parity-notes.md`), never other
  sessions' edits, with a message that names the area and the counts, and push to `main`
  (`git pull --rebase origin main` first).

### Handing off
Hand off when the context is long (after about two areas, or 60-80 tool calls of probing),
when an area is finished, or when anything blocks you. Never stop without handing off.
1. Update `parity/coverage.md` (area `done` or `in progress` with a note) and
   `parity/progress.md` (what you finished, what the next prober should start with, any traps).
   Commit and push.
2. For every batch you wrote this session (and any `ready` batch in `index.jsonl` with no
   fixer yet), start a cloud fixer with `start_session`: `initiation: "user_asked"`,
   `context: "fresh"`, `target: "cloud"`, title `Fix parity batch <name>`, prompt =
   "Follow `parity/batches/FIXER.md` for the batch `parity/batches/<name>.md`." plus the batch's
   items in one line each, background = the goal paragraph at the top of this file. Set the
   batch's `index.jsonl` line to `fixing` with the session link once it starts. The owner
   wants these on Opus: the cloud environment's model setting decides that, so say so in the
   handoff message if it looks wrong.
3. Start the next prober with `start_session`: `initiation: "user_asked"`, `context: "fresh"`,
   `target: "local"`, `use_worktree: false`, title `Parity prober: <next area>`, prompt =
   "Read `parity/LOOP.md` and follow the prober protocol, starting with `parity/progress.md`."
   plus two sentences on where you stopped, background = the goal paragraph. The user approves
   desktop control again in that session; say so.
4. Write a one-paragraph note to the user: area covered, counts, batches started, next prober's
   link. Then end the turn (archive this session only if the owner asked for self-archiving
   probers; by default leave it for review).

## Fixer protocol

See `parity/batches/FIXER.md`. In short: one batch, make FissionOxide match Fusion for each
item, add tests, `cargo test --workspace` and `cargo clippy --workspace`, `git pull --rebase
origin main`, push to `main`, set the batch to `done` in `index.jsonl` and each key to
`fixing-merged` in `ui-features.jsonl` (the next prober verifies it in the app and sets
`verified`), then archive the session.

## Coverage rules

- An area is `done` only when every control in it has a ledger row (match or not) and every
  mismatch has a batch.
- Re-verify: when a prober's area includes keys marked `fixing-merged`, re-probe them first and
  set `verified` or reopen them with a new note.
- Never mark Fusion behaviour from memory; look at it on screen in the session.
