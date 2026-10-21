# Plugins

FissionOxide has plugins so that tool suites built for other CAD apps — starting with the
LumenArt Tool Suite, a Fusion 360 add-in of about 22 sheet-metal and sign-making commands — can
live on in FissionOxide, and so that anyone can extend the app without rebuilding it.

This document describes what a plugin is, how it is installed, how it runs and the interface its
code sees. The host is the `fox-plugin` crate (`crates/plugin`); a complete example plugin is in
[examples/plugins/flange](../examples/plugins/flange).

## Add-ins and plugins

FissionOxide is free and open source, and it has two kinds of extension. Both share one API (the
JSON commands) and one manifest format (`plugin.toml` below: panels, commands, dialog inputs).

- **Plugins** are installed at run time by anyone: a sandboxed `.foxplugin` dropped on the
  window. They use the command API: new commands, dialogs, macros, exports, checks. Safe to take
  from a stranger, one file for every platform, unaffected by app updates. Sharing is "send the
  file" for now.
- **Add-ins** add real functionality and are compiled in. Each is a crate in this repository
  under `addins/`, shipped with the app and switched on or off in the add-in manager (off by
  default unless it says otherwise). Besides the command API they can add feature types that
  re-run in the timeline, workspaces, manipulators and engine commands. Anyone can write one:
  build the app with it, or send it upstream. Rust has no stable ABI and FissionOxide forbids
  `unsafe`, so native code is never loaded at run time; that is what plugins are for.

Rules that keep the two from drifting apart:

- When an add-in needs something new from the engine, it is added as a JSON command too, so
  plugins and scripts get it as well.
- General-purpose tools go into the app itself, not into an add-in. An add-in holds only what is
  specific to its trade. A plugin that outgrows the sandbox can become an add-in and keep its
  manifest.

### An add-in's toolbar tab can hold built-in commands

An add-in's manifest places its own commands **and built-in ones** in its tab, so a workflow tab
shows every step of the workflow in order, even the steps that are now part of the app. A command
entry names either the add-in's own command or a built-in one:

```toml
[[commands]]
builtin = "sketch.remove_duplicates"   # a built-in command, shown in this tab too
label = "Clean Duplicate Lines"        # optional: the tab's own label and tooltip
icon = "icons/01-line-cleaner.svg"     # optional: the add-in's own icon for it
panel = "prepare"
step = 1                               # optional: order in the workflow, drawn on the icon
```

The built-in command stays where it always was in the app's own tabs, with its own icon; the
add-in's tab only shows it again, in its own style. Switching the add-in off removes its tab and
leaves the built-in commands alone. `step` numbers the commands in the order they are used from
start to finish; the add-in's icons carry the number, and the tab is laid out in step order.

## Goals

- **One file to install.** A plugin is a single `.foxplugin` file. The user installs it from
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

A `.foxplugin` is a zip:

```
plugin.toml          manifest (below)
plugin.wasm          the code (a WebAssembly module, see "Host interface")
icons/               one PNG per command (32×32 or larger, square)
resources/           anything the plugin reads at run time (DXF parts, presets.json, …)
README.md            shown in the install dialog
```

Paths inside the zip must be relative and stay inside it; a package is at most 4096 files and
256 MB unpacked. `fissionoxide-cli plugin pack DIR OUT.foxplugin` builds one from a folder laid
out like this, checking it first.

### Manifest

```toml
id = "com.lumenart.tools"          # reverse-DNS, unique
name = "LumenArt Tools"
version = "1.0.0"                  # semver; installing a newer version updates in place
author = "LumenArt"
description = "Sheet metal tabs, slots and cut-file checks"
homepage = "https://lumenart.shop"
fissionoxide = ">=0.1"             # app versions it works with (a semver requirement)
api = 1                            # plugin API version

# What the plugin may do beyond running design commands. Shown to the user at install.
permissions = ["write-files", "render"]   # also: "read-files", "hub"

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
icon = "icons/line_cleaner.png"
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

Input types and what the plugin receives for them:

| Type | Keys | Value passed to the plugin |
|------|------|----------------------------|
| `selection` | `filter` (sketch, curve, point, profile, body, face, edge, vertex, feature; none = any), `min` (default 1), `max` | a list of picks as the `selection` command returns them (`{"type": "face", "body": 1, "face": 3}`) |
| `value` | `unit` (length, angle, unitless), `default` (number or expression), `min`, `max` | a number: mm, degrees or plain. Expressions such as `"0.5 in"` or `"width / 2"` are evaluated with the design's parameters |
| `integer` | `default`, `min`, `max`, `step` | an integer |
| `slider` | `min`, `max` (both required), `step`, `default` | a number |
| `checkbox` | `default` | `true` / `false` |
| `choice` | `options`, `default` (one of them) | the option |
| `text` | `default` | the text |
| `button` | | nothing; pressing it calls `fox_input_changed` with its id |
| `label` | `default` (the text) | nothing; the plugin can change its text |
| `group` | | nothing; inputs with `group = "<id>"` are shown under it |

Every input may have `label`, `prompt` (hover text; a selection's empty text), `group`, and
`visible_when` / `enabled_when` (`{ other_input = value, ... }`). These cover the conditional
inputs the current add-in builds by hand. A selection left out when a command is run from the
CLI is taken from the current selection. Values are checked (counts, ranges, options, unknown
inputs) before the plugin is called.

Ids are lower case (`a-z`, `0-9`, `_`, `-`); a plugin id is reverse-DNS. Unknown manifest keys
are an error, so typos don't pass silently. A command without `panel` goes in a panel named
after the plugin, on a tab named after the plugin; a panel without `tab` sits on that tab too.

### Where installed plugins live

Installing unpacks the package into the data hub's directory under
`plugins/<id>/<version>/`, and records it in `plugins/installed.json` (version, enabled flag,
granted permissions, install date, where it came from). Beside the versions sit the plugin's
`storage.json` and `log.txt`, so both survive updates; uninstalling removes them too. The folder
is an implementation detail: the UI never asks the user to find it. A `plugins/` folder next to
the executable holds bundled plugins (each unpacked in its own folder): they are enabled with
the permissions they ask for, can be turned off but not removed, and an installed copy of the
same id wins over them.

## Installing

1. The user picks File → Plugins → Install from file…, drags a `.foxplugin` onto the window, or
   picks one in the Plugins window's data hub list.
2. The app checks the package: manifest parses, ids are unique, `api` and `fissionoxide` versions
   are supported, the wasm module compiles and imports and exports only what the host has (see
   below), every icon and command input is valid.
3. The install dialog shows name, version, author, what it adds (commands with their tab and
   panel), the permissions it asks for (each can be unticked) and the README. The same id with
   a newer version shows as an update; the same or an older version asks first.
4. On Install the package is unpacked into the plugin directory and its commands appear in the
   toolbar immediately — no restart. An update keeps the enabled flag, storage and log.

The **Plugins** window (File → Plugins, the app bar's plugins button, UTILITIES → ADD-INS)
lists installed plugins with enable/disable, remove, details (commands, permissions as
checkboxes, the plugin's log) and the data hub's plugins with Install / Update. A plugin that
fails to load is shown with its error and skipped; it never stops the app or other plugins
from loading.

### The data hub

A plugin published to the data hub (`fissionoxide-cli plugin publish FILE`, or the MCP tool
`plugin_publish`) is a file item whose versions are the plugin's releases: publishing a newer
package adds a version to the item already holding that plugin id. Everyone sharing the hub
(a synced folder today) sees it in the Plugins window and can install it; `plugin update` (or
Update in the window) installs the newest release. An update grants only what the user had
granted before; new permissions stay off until granted in the Plugins window.

### Command line and MCP

```text
fissionoxide-cli plugin list | commands | hub | log ID
fissionoxide-cli plugin install FILE.foxplugin|DIR [--grant a,b] [--force]   (--force: same or older)
fissionoxide-cli plugin install --item ITEM [--version N]
fissionoxide-cli plugin update [ID]
fissionoxide-cli plugin uninstall | enable | disable ID
fissionoxide-cli plugin check FILE.foxplugin|DIR
fissionoxide-cli plugin pack DIR OUT.foxplugin
fissionoxide-cli plugin publish FILE.foxplugin|DIR [--project P] [--folder F]
fissionoxide-cli plugin run PLUGIN/COMMAND ['{inputs}'] [--open IN.fox] [--save OUT.fox] [--folder DIR] [--dev DIR]
```

`run --dev DIR` runs a plugin straight from its source folder without installing it (with the
permissions it asks for), for development. `--folder` is the run's folder (default: the current
directory). The same commands are JSON commands (`plugin.list`, `plugin.install`, `plugin.run`
...: `fissionoxide-cli commands` lists them) and MCP tools (`plugin_list`, `plugin_run` ...).

## Running plugin code

**Runtime: WebAssembly on Wasmtime.** Chosen because:

- **Sandboxed.** A plugin can only call the host functions below. No file system, network or
  process access: files go through `execute` and `write_file`, checked against the permissions
  the user granted and kept inside the folder picked for the run.
- **Can't crash the app.** Traps, panics and endless loops end the run with an error (each call
  has a fuel budget of about 20 billion instructions and a 1 GB memory cap); the design is
  restored as it was before the run.
- **One file for every platform.** No per-OS builds, no ABI break when the app is rebuilt
  (FissionOxide forbids `unsafe`, which rules out loading native libraries anyway).
- **Any language** that compiles to a plain WebAssembly module without WASI: Rust
  (`wasm32-unknown-unknown`), C/C++ (clang `--target=wasm32`), Zig, AssemblyScript, TinyGo.

Rejected alternatives: embedded Python (closest to the old code, but no sandbox and the app
would have to ship Python), Lua/Rhai (sandboxable, but a rewrite in a new language), native
dylibs (unsafe, unsandboxed, ABI-fragile). The first plan used WebAssembly *components* (WIT);
API 1 is a plain module instead, because building components still needs extra tools on top of
the compiler and the host gains nothing from them for a JSON interface. Python via
`componentize-py` would need components, so it can come with a later API version.

### Host interface (API 1)

Every string crosses as UTF-8 bytes: a (pointer, length) pair of `i32`s in the module's memory.
The host passes data in by calling the module's `fox_alloc` and writing into the block it
returns; it hands data back the same way and returns it packed in an `i64` as
`(pointer << 32) | length`, with `-1` meaning "none". The module is instantiated afresh for every
call, so nothing needs freeing; anything to keep goes in storage.

The module **exports**:

| Export | Signature | |
|--------|-----------|-|
| `memory` | memory | |
| `fox_alloc` | `(len: i32) -> i32` | a block of `len` bytes for the host to write |
| `fox_run` | `(command, inputs) -> i64` | the user pressed OK: the command's id and the dialog's values as JSON (see the input table). Returns a packed reply |
| `fox_input_changed` | `(command, input, inputs) -> i64` | optional: an input changed while the dialog is open (`input` is `""` when it opens, or a button's id). Returns a packed reply whose `ok` is the updates |

A reply is JSON: `{"ok": <anything>}` or `{"error": "message"}`. `fox_run`'s `ok` value is the
run's result (a `"message"` key in it is shown in the status bar). `fox_input_changed`'s is
`{"<input id>": {"value"?, "visible"?, "enabled"?, "text"?, "label"?, "options"?}}`: a new value,
showing, enabling, a label input's text, a row's label, a choice's options.

The module may **import** these from `fox`, nothing else:

| Import | Signature | |
|--------|-----------|-|
| `execute` | `(cmd, params) -> i64` | runs an engine command; returns `{"ok": result}` or `{"error": message}` |
| `resource` | `(path) -> i64` | the bytes of `resources/<path>` in the package, or -1 |
| `storage_get` | `(key) -> i64` | the plugin's stored string, or -1 |
| `storage_set` | `(key, value)` | stores a string (a negative value length removes the key); 16 MB in all |
| `write_file` | `(path, data) -> i64` | writes into the run's folder ("write-files"); 0, or a packed error message |
| `progress` | `(fraction: f32, message) -> i32` | 1 to go on, 0 if the user cancelled (the run is then rolled back) |
| `log` | `(level: i32, message)` | 0 error, 1 warning, 2 info, 3 debug; shown in the Plugins window |

[examples/plugins/flange/src/fox.rs](../examples/plugins/flange/src/fox.rs) wraps all of this
for Rust (`fox::execute("extrude", json!({...}))`, `fox::write_file`, ...); copy it to start a
plugin.

### What `execute` may do

- Never: `new`, `open`, `save`, `undo`, `redo`, `transaction.*`, `hub.open` and `plugin.*`
  (they would replace the design or the run's undo step).
- `hub.*` needs **hub**; `view.render` needs **render**.
- File paths in parameters are relative to the folder the user picked for the run; absolute
  paths and `..` are refused, as are symbolic links leading out of it. Commands that write
  (`export`, `drawing.export`, `cam.post`, `cut.gcode`, `sheet.flat_pattern` and `hub.download`
  with a path) need **write-files** (`view.render` only **render**). Commands that read
  (`import`, `inspect`, `sketch.insert`, `sketch.trace`, `canvas.insert`, `sheet.check_dxf`,
  font files) need **read-files**, except that `resources/...` reads the plugin's own resources.
  A command the host doesn't know that has a file-like `path`/`file`/`image`/`dir` is treated as
  a write.
- A run without a folder (the CLI defaults to the current directory; the UI asks for one when a
  plugin has file permissions) can't touch files at all.

### One run, one undo step

Each OK press runs inside an engine transaction: everything the plugin does is one undo step
named after the command, and if the plugin returns an error, traps, runs out of fuel or the user
cancels, the whole run is rolled back. `fox_input_changed` also runs in a transaction that is
always rolled back, so a plugin may build geometry to measure it there, but nothing it makes
stays; showing such geometry as a live preview is not in API 1.

## Plugin features (later)

v1 plugins are **macros**: they expand into ordinary sketches and features, so a design opens
and rebuilds anywhere, with or without the plugin. That covers every tool in the current suite.

Later, a plugin may register a **feature type**: a timeline entry that stores the plugin's
inputs and re-runs the plugin during regeneration (so an auto-tab follows edits). The design
then needs the plugin to rebuild that feature; without it the feature keeps its last result as
a frozen body and shows a warning. This is deliberately not in v1.

## Porting the LumenArt Tool Suite

What every command does, with its numbers and the bugs not to copy, is in
[LUMENART.md](LUMENART.md).

The suite splits in two (the table is in [LUMENART.md](LUMENART.md#core-features-and-the-add-in)):

- **Into the app:** the general tools (remove duplicate lines, sketch clean-up, scale to size,
  cut-service checks, cut estimates, flat patterns on one sheet, exploded views and batch
  renders, a sketch library, inspect). Every CAD user gets them.
- **The LumenArt add-in** (`addins/lumenart`, the first official add-in): the layered-sign
  system (layers at standoff heights, Quick Tab, Auto Flange, Slot Projector, Tab Builder, Auto
  Tab as live timeline features), Auto Export with LumenArt's file layout, and the LumenArt
  library pack (keyholes, studs, E12 sockets, LED strip slots). Its **LumenArt** tab lists the
  built-in tools too, in workflow order, with LumenArt's grey-and-yellow icons numbered by step.
- **Prismatic Colors** becomes the first example plugin: small, and it needs the `network`
  permission.

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
| 11 | Put DXF, `.fox` or another sketch's curves into an existing sketch | engine | 3 | `sketch.insert` | done |
| 12 | Render to a PNG file with camera control and sketches hidden | query | 4 | `view.render` | done |
| 13 | Construction plane through a point with a normal | engine | 1 | `{origin, normal, x?}` wherever a plane is taken | done |

Attributes can't be put on individual faces or edges yet: their indices change when the model
changes. Tag the feature or body instead, and find faces again with `body.topology`.

Later, needed by one tool each and not blocking the port: appearances with names and emissive
light (LED brightness), temporary exploded positions without timeline features, a combined
multi-body DXF cut file, ellipse curves, curve/surface evaluation on freeform geometry.

## Milestones

1. **Engine readiness** — the gap table above, each with tests and CLI/MCP exposure (done).
2. **Plugin host** — `fox-plugin` crate: package reader and validator, installed-plugin
   registry, Wasmtime runtime with the host interface, transactions, permissions, data hub
   publishing and updates (done).
3. **UI** — Plugins window, install dialog and drag-and-drop, manifest-driven toolbar tabs,
   panels and dialogs, result messages, plugin log (done).
4. **SDK** — the Rust bindings and example plugin in `examples/plugins/flange`,
   `fissionoxide-cli plugin pack|check|run --dev` for development (done).
5. **First ports** — Clean Duplicate Lines, then the rest of LumenArt Tools in the order above.
