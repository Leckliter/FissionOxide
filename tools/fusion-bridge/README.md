# Fusion bridge

Tools for the Fusion parity loop. They list everything Autodesk Fusion can do, build test parts
in Fusion and FissionOxide side by side, and track which Fusion features FissionOxide matches.

- `FoxBridge/`: a Fusion add-in. It serves `127.0.0.1:47800` so scripts can drive Fusion's API.
  Requests need the token the add-in writes to `%APPDATA%/FoxBridge/token`.
- `client.py`: talks to the add-in, and runs comparisons.
- `ledger.py`: turns the Fusion catalog into `parity/ledger.jsonl`, with one line per feature to
  test.
- `install.py`: links `FoxBridge/` into Fusion's AddIns folder.

Python 3 standard library only. Run these with `py` on Windows.

## Setup

```
py tools/fusion-bridge/install.py
```

Then in Fusion, press Shift+S. On the Add-Ins list, turn on **FoxBridge** and tick **Run on
Startup**. Check that it answers:

```
py tools/fusion-bridge/client.py ping
```

`catalog.py`, `measure.py` and `recipe_runner.py` reload on every request, so edits to them take
effect without restarting Fusion. Edits to `FoxBridge.py` need the add-in stopped and started
again.

## Commands

```
py tools/fusion-bridge/client.py catalog            # parity/catalog.json (about 2 s)
py tools/fusion-bridge/ledger.py                    # parity/ledger.jsonl, keeps existing statuses
py tools/fusion-bridge/ledger.py --summary
py tools/fusion-bridge/client.py compare [FILTER]   # every tests/reference case, both apps
py tools/fusion-bridge/client.py recipe tests/reference/08-chamfer/recipe.json --keep-open
py tools/fusion-bridge/client.py python "result = app.version"
py tools/fusion-bridge/client.py close              # close the scratch documents left open
```

**`catalog`** reads Fusion's own API, not the screen. It records:
- every workspace, tab, panel and command,
- all 3,000+ command definitions,
- every API feature type with its input options and creation methods,
- every enum in `adsk.fusion`.

**`compare`** builds each `tests/reference/*/recipe.json` in Fusion, and its `part.jsonl` in
FissionOxide with `target/release/fissionoxide-cli` (or `$FOX_CLI`). It then compares body count,
volume, area, bounding box, centre of mass and face count, using the tolerances from
`tests/reference`. It writes `parity/compare/<case>.json` and
[`parity/compare/summary.md`](../../parity/compare/summary.md). Pass `--png` to also save a
screenshot from each app.

`recipe_runner.py` builds a recipe in Fusion:
- It places sketches using the recipe's frame.
- It picks faces and edges by point, with `findBRepUsingPoint`.
- It applies the recipe's constraints and dimensions, so Fusion's solver places constrained
  sketches.
- It reports ops it can't build under `unsupported` instead of guessing. These are sheet metal,
  path patterns, threads, boundary surfaces, thicken, spline-radius fillets, and asymmetric thin
  walls.

## The ledger

`parity/ledger.jsonl` holds one entry per Fusion command or API feature type:

- `scope` is `design` (the Design workspace's tabs, in `priority` order: Sketch and Solid first)
  or `deferred` (Manufacture, Simulation, Drawing, Electronics, and the PCB and Package tabs).
- `fox_candidates` are name-based guesses at the matching FissionOxide commands. They're a
  starting point for a probe, not a mapping.
- `status` moves through `untested -> probing -> match | mismatch | fox-missing | blocked ->
  fixing -> verified`.
- `history`, `notes`, `cases` and `prs` are kept when `ledger.py` runs again. Entries whose
  command disappears from Fusion are marked `removed`.

The parity loop (`tools/parity-loop/`, next) picks the next entry, probes it in both apps, and
starts a fixer agent for each mismatch.
