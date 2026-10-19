# Plugin system plan

CLFusion needs plugins so that tool suites built for other CAD apps — starting with the LumenArt
Tool Suite, a Fusion 360 add-in of about 22 sheet-metal and sign-making commands — can live on in
CLFusion, and so that anyone can extend the app without rebuilding it.

This document is the plan: what a plugin is, how it is installed, how it runs, and what the
engine has to grow before the first real plugin can be ported.

## Goals

- **One file to install.** A plugin is a single `.clfplugin` file. The user installs it from
  File → Plugins → Install… or by dropping it on the window; they never touch a folder.
- **Same API as everything else.** Plugins drive the engine through the same JSON commands as
  the GUI, the CLI and the MCP server (`Session::execute`). There is no second API to keep in
  sync; every command added for plugins also helps scripts and AI agents.
- **Safe to install from someone else.** Plugin code runs sandboxed and can only do what its
  manifest asked for and the user approved.
- **Designs never depend on a plugin to open.** What a plugin builds lands in the timeline as
  ordinary features (see [Plugin features](#plugin-features-later)).
- **Native look.** The app draws plugin dialogs from a declaration, so plugins match the rest of
  the UI and don't need GUI code.

## The package

A `.clfplugin` is a zip:

```
plugin.toml          manifest (below)
plugin.wasm          the code (a WebAssembly component)
icons/               one SVG or PNG per command
resources/           anything the plugin reads at run time (DXF parts, presets.json, …)
README.md            shown in the plugin manager
```

### Manifest

```toml
id = "com.lumenart.tools"          # reverse-DNS, unique
name = "LumenArt Tools"
version = "1.0.0"                  # semver; installing a newer version updates in place
author = "LumenArt"
description = "Sheet metal tabs, slots and cut-file checks"
homepage = "https://lumenart.shop"
clfusion = ">=0.2"                 # app versions it works with
api = 1                            # plugin API version

# What the plugin may do beyond running design commands. Shown to the user at install.
permissions = ["write-files", "render"]   # also: "network", "read-files"

# Toolbar placement. Panels are created on demand in this order; tabs group panels.
[[panels]]
id = "design"
label = "1 · DESIGN"
tab = "LumenArt"

[[panels]]
id = "validate"
label = "3 · VALIDATE"
tab = "LumenArt"

[[commands]]
id = "line_cleaner"                # full id becomes com.lumenart.tools/line_cleaner
label = "Clean Duplicate Lines"
tooltip = "Remove overlapping lines, arcs and circles from a sketch"
icon = "icons/line_cleaner.svg"
panel = "design"
promoted = true                    # shown in the panel's main row, not only its menu
repeatable = true                  # stays open for another run after OK

  [[commands.inputs]]
  id = "sketch"
  type = "selection"
  filter = ["sketch"]
  min = 1
  max = 1
  prompt = "Sketch to clean"

  [[commands.inputs]]
  id = "tolerance"
  type = "choice"
  options = ["Exact", "Tight", "Standard", "Loose", "Custom"]
  default = "Standard"

  [[commands.inputs]]
  id = "custom"
  type = "value"
  unit = "length"
  default = "0.001 in"
  visible_when = { tolerance = "Custom" }
```

Input types: `selection` (filters: sketch, curve, point, profile, body, face, edge, component;
min/max; returns ids and the picked point), `value` (with unit: length, angle, unitless),
`integer`, `slider`, `checkbox`, `choice`, `text`, `button`, `label` (read-only text the plugin
can update) and `group`. `visible_when` / `enabled_when` cover the conditional inputs the
current add-in builds by hand.

### Where installed plugins live

Installing copies the package into the app's data directory (next to the data hub), unpacked,
under `plugins/<id>/<version>/`, and records it in `plugins/installed.json` (enabled flag,
granted permissions, install date). The folder is an implementation detail: the UI never asks
the user to find it. A bundled-plugins folder next to the executable is read too, for plugins
shipped with the app.

## Installing ("upload")

1. The user picks File → Plugins → Install… or drags a `.clfplugin` onto the window.
2. The app checks the package: manifest parses, ids are unique, `api` and `clfusion` versions
   are supported, the wasm module compiles, every icon and command input is valid.
3. The install dialog shows name, version, author, what it adds (commands and panels) and the
   permissions it asks for. Same id with a newer version shows as an update; older asks first.
4. On Install the package is unpacked into the plugin directory and its commands appear in the
   toolbar immediately — no restart.

The **Plugins** panel (File → Plugins) lists installed plugins with enable/disable, update
(install a newer file), remove, permissions and the plugin's log. A plugin that fails to load
is shown with its error and skipped; it never stops the app or other plugins from loading.

The same package and install path will work for a future download page or a shared hub server;
"upload" there just ends in the same install step.

## Running plugin code

**Runtime: WebAssembly components on Wasmtime.** Chosen because:

- **Sandboxed.** A plugin can only call the host functions it is given. No file system, network
  or process access unless the matching permission was granted, and then only through narrow
  host functions (e.g. write into a folder the user picked).
- **Can't crash the app.** Traps, panics and infinite loops (fuel/epoch limits) end the plugin
  run with an error; the design is restored as it was before the run.
- **One file for every platform.** No per-OS builds, no ABI break when the app is rebuilt
  (CLFusion forbids `unsafe`, which rules out loading native libraries anyway).
- **Any language.** Plugins can be written in Rust, or in **Python via `componentize-py`**,
  which keeps porting the existing Python add-in close to the original code. JavaScript, Go and
  C also compile to components.

Rejected alternatives: embedded Python (closest to the old code, but no sandbox and the app
would have to ship Python), Lua/Rhai (sandboxable, but a rewrite in a new language), native
dylibs (unsafe, unsandboxed, ABI-fragile).

### Host interface (WIT)

```wit
package clfusion:plugin@1.0.0;

interface host {
  /// Run any engine command; returns its JSON result or an error message.
  execute: func(command: string, params: string) -> result<string, string>;
  /// Read a file bundled in the plugin's resources/ folder.
  resource: func(path: string) -> result<list<u8>, string>;
  /// Plugin-private key/value storage that survives restarts (user libraries, presets).
  storage-get: func(key: string) -> option<string>;
  storage-set: func(key: string, value: string);
  /// Write a file under the output folder the user chose for this run ("write-files").
  write-file: func(relative-path: string, data: list<u8>) -> result<_, string>;
  /// Dialog feedback while running.
  progress: func(fraction: f32, message: string) -> bool;   // false = user cancelled
  log: func(level: u8, message: string);
}

world plugin {
  import host;
  /// Called when the user presses OK. `inputs` is the dialog's values as JSON.
  export run: func(command: string, inputs: string) -> result<string, string>;
  /// Called when an input changes while the dialog is open (live update, previews,
  /// enabling inputs, filling a selection). Returns input updates as JSON.
  export input-changed: func(command: string, input: string, inputs: string) -> result<string, string>;
}
```

Rendering and file export go through `execute` (`view.render`, `export`) with paths relative to
the chosen output folder; the host rewrites and checks every path a plugin passes, so a plugin
can't write outside that folder.

### One run, one undo step

Each OK press runs inside an engine transaction: everything the plugin does is one undo step
named after the command, and if the plugin returns an error, traps or the user cancels, the
whole run is rolled back. Previews while the dialog is open run in a scratch transaction that
is discarded on every change and on Cancel.

## Plugin features (later)

v1 plugins are **macros**: they expand into ordinary sketches and features, so a design opens
and rebuilds anywhere, with or without the plugin. That covers every tool in the current suite.

Later, a plugin may register a **feature type**: a timeline entry that stores the plugin's
inputs and re-runs the plugin during regeneration (so an auto-tab follows edits). The design
then needs the plugin to rebuild that feature; without it the feature keeps its last result as
a frozen body and shows a warning. This is deliberately not in v1.

## Porting the LumenArt Tool Suite

The suite splits into two plugins, replacing its current folder-detecting loader:

- **LumenArt Tools** (the commercial, public commands): Clean Duplicate Lines, Optimize Sketch,
  Create Assembly Tabs, Create Matching Slots, Analyze Cut File, Extract Component Dimensions,
  Inspect Entities, Validate for SendCutSend, Render Product View / Individual Parts / Exploded
  Assembly.
- **LumenArt Pro** (the private commands), installed only on LumenArt machines.

Its DXF library ships in `resources/`. Its `.f3d` library parts can't be read by CLFusion and
need converting to `.dxf` or `.clf` once. Signing and license keys for selling the public plugin
can come later without changing the format.

Port order (each step proves more of the API): Clean Duplicate Lines (selection + choice inputs,
sketch read/delete) → Analyze Cut File (file output) → Validate for SendCutSend → Create
Matching Slots / Create Assembly Tabs (B-rep topology, resources, attributes) → render tools →
the rest.

## Gaps to close first

Every tool was checked against the current command set. Host gaps (dialog inputs, progress,
file output, resources, storage) are the plugin system itself, above. These are the **engine
and query gaps**, in the order they unblock tools:

| # | Gap | Kind | Tools blocked | Command | Status |
|---|-----|------|---------------|---------|--------|
| 1 | Undo grouping: many commands as one undo step, with rollback | engine | all | `transaction.begin/commit/rollback` | done |
| 2 | List a body's faces and edges with geometry and adjacency (kind, area, centre, normal, edge endpoints, length, faces per edge) | query | 7 | `body.topology` | done |
| 3 | Which bodies a feature created or changed | query | 4 | `result_bodies` in feature results | done |
| 4 | Join/cut only chosen bodies (today it hits every overlapping body) | engine | 4 | `targets` on extrude/revolve/loft/sweep/primitives/flange | done |
| 5 | Custom attributes on bodies, components, features and sketch curves/points | engine | 5 | `attribute.set/get/find/delete` | done |
| 6 | Sketch frame (origin, axes) and curve lengths/samples, profile bounding boxes and their curves, curves outside every profile | query | 6 | richer `sketch.info` | done |
| 7 | Read the current selection | query | 2 | `selection` | done |
| 8 | Sketch visibility | engine | 4 | `sketch.visible`, shown in `info` | done |
| 9 | Body colour | query | 3 | `body.appearance` takes `"#rrggbb"`; colour in `info` | done |
| 10 | Move/rotate/scale sketch geometry | engine | 3 | `sketch.transform` | done |
| 11 | Put DXF, `.clf` or another sketch's curves into an existing sketch | engine | 3 | `sketch.insert` | done |
| 12 | Render to a PNG file with camera control and sketches hidden | query | 4 | `view.render` | done |
| 13 | Construction plane through a point with a normal | engine | 1 | `{origin, normal, x?}` wherever a plane is taken | done |

Attributes can't be put on individual faces or edges yet: their indices change when the model
changes. Tag the feature or body instead, and find faces again with `body.topology`.

Later, needed by one tool each and not blocking the port: appearances with names and emissive
light (LED brightness), temporary exploded positions without timeline features, a combined
multi-body DXF cut file, ellipse curves, curve/surface evaluation on freeform geometry.

## Milestones

1. **Engine readiness** — the gap table above, each with tests and CLI/MCP exposure (done).
2. **Plugin host** — `clf-plugin` crate: package reader and validator, installed-plugin
   registry, Wasmtime runtime with the WIT host interface, transactions, permissions.
3. **UI** — Plugins panel, install dialog and drag-and-drop, manifest-driven toolbar panels and
   dialogs, progress and result messages, plugin log.
4. **SDK** — WIT file, a Rust and a Python template, `clfusion-cli plugin pack|check|run` for
   development (run a plugin headless against a design).
5. **First ports** — Clean Duplicate Lines, then the rest of LumenArt Tools in the order above.
