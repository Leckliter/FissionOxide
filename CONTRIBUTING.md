# Contributing to FissionOxide

## Build

You need Rust (stable, 1.90 or newer), CMake and a C++17 compiler. On Linux the app also needs
the X11/Wayland and OpenGL development libraries; on Debian/Ubuntu:

```sh
sudo apt-get install cmake g++ pkg-config libxkbcommon-x11-dev libxkbcommon-dev libx11-dev \
  libxcursor-dev libxrandr-dev libxi-dev libwayland-dev libgl1-mesa-dev libegl1-mesa-dev
```

The first build compiles OpenCASCADE 7.8 from source into `target/OCCT` (about 20 minutes on
4 cores). Every profile shares that directory, so `cargo build`, `cargo test`, `cargo clippy`
and `--release` all reuse it, and only `cargo clean` throws it away.

To skip the OCCT build altogether, point `FOX_OCCT_DIR` at an existing install (`include/` and
`lib/` with static libraries), for example one you kept from an earlier build:

```sh
mkdir -p ~/occt && cp -r target/OCCT/include target/OCCT/lib ~/occt/
export FOX_OCCT_DIR=~/occt
```

CI does exactly this with a cached install. A `FOX_OCCT_DIR` without `include/` and `lib/` is
an error rather than a silent rebuild.

```sh
cargo run --release -p fissionoxide        # the app
cargo run --release -p fissionoxide-cli -- commands   # every JSON command
```

### Claude Code cloud sessions

`.claude/settings.json` runs `.claude/hooks/session-start.sh` when a cloud session starts. It
installs the packages above and starts `cargo build --workspace --all-targets` followed by
`cargo clippy --workspace --all-targets` in the background; follow it with
`tail -f target/warm-build.log`. Cargo commands you run meanwhile wait for the build lock and then
reuse its output.

## Test

Before opening a pull request:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The crates that don't link OpenCASCADE build and test in seconds, so iterate on them alone:

```sh
cargo test -p fox-expr -p fox-sketch -p fox-exchange -p fox-store -p fox-render -p fox-trace -p fox-brand -p fox-desktop
```

CI (`.github/workflows/ci.yml`) uses a pinned Rust (`RUST_TOOLCHAIN`, currently 1.97.0) so a new
stable's clippy lints don't break `main` unannounced; bump it in its own pull request, fixing the
new lints there. It runs two jobs on every push and pull request to `main`:

- **fmt + pure crates**: `cargo fmt --check`, then clippy and tests for the crates above.
  Results in a few minutes.
- **clippy + tests (workspace)**: the full clippy and test run against a cached OCCT install
  (keyed on the `occt-sys` version in `Cargo.lock` and `crates/kernel/build.rs`). A cache miss
  builds OCCT once and saves it before anything else can fail.

Add tests for what you add: kernel operations in `crates/kernel/src/tests.rs`, regeneration in
`crates/model/src/regen/tests.rs`, commands in `crates/engine/src/tests.rs` (or `tests_more.rs`).

## Style

- `rustfmt.toml` sets 2-space indents and a 120-column limit; `cargo fmt` is the authority.
- Match the comment density of the code around you: a doc comment on public items and on
  anything non-obvious, not a narration of each line.
- `unsafe` is forbidden outside the cxx bridge in `fox-kernel`.
- Every OCCT call in `crates/kernel/cpp/kernel.cpp` sits between `FOX_TRY` and `FOX_CATCH`,
  so a failing operation becomes an `Err` (a red timeline feature), never a crash.

## Shared files

Many changes touch the same few files. To keep merges painless, in these files add new code in
new blocks at the **end** of the relevant list or section, never reorder, rename or reformat
existing code, and put sizeable new logic in a new file or module (as `engine/src/features.rs`,
`model/src/regen/extra.rs` and `ui/src/forms.rs` already do):

- `crates/engine/src/commands.rs`
- `FeatureKind` in `crates/model/src/lib.rs`
- `crates/model/src/regen.rs`
- `crates/kernel/cpp/kernel.cpp` and `crates/kernel/cpp/kernel.hxx`
- `crates/kernel/src/ffi.rs`
- `crates/ui/src/dialogs.rs`, `crates/ui/src/menus.rs`, `crates/ui/src/icons.rs`
- `README.md`, `ROADMAP.md`

## Colours and the logo

Colours come from the brand kit, not literals: `crates/ui/src/theme.rs` and the icon accents
read `fox-brand`, which `brand/build.py` generates from `brand/tokens.json`. To change a colour,
edit `tokens.json`, run `python3 brand/build.py` and commit what it writes (see
[`brand/README.md`](brand/README.md)).

## Adding a feature end to end: chamfer as the example

A feature runs through five layers. Chamfer is a good one to copy because it is small and
touches all of them.

```
UI dialog ──JSON command──▶ engine command ──FeatureKind──▶ model regen ──▶ kernel (Rust) ──cxx──▶ kernel.cpp (OCCT)
```

### 1. Kernel: the geometric operation

- **C++** (`crates/kernel/cpp/kernel.cpp`, declared in `kernel.hxx`): `chamfer(s, edges, dist)`
  and `chamfer2(s, edges, d1, d2, angle)` build a `BRepFilletAPI_MakeChamfer`, add the 1-based
  edges, throw `std::runtime_error` with a readable message when OCCT fails, and wrap the body in
  `FOX_TRY` … `FOX_CATCH("chamfer")`.
- **Bridge** (`crates/kernel/src/ffi.rs`): the matching `fn chamfer(...) -> Result<UniquePtr<Shape>>;`
  lines in the `unsafe extern "C++"` block.
- **Safe API** (`crates/kernel/src/lib.rs`): `Shape::chamfer` and `Shape::chamfer_two` check
  their inputs (`KernelError::Input` for a non-positive distance) and wrap the result.
- **Test** (`crates/kernel/src/tests.rs`, `fillet_chamfer_shell`): chamfer a box, check the
  volume.

### 2. Model: the timeline feature

- **`FeatureKind::Chamfer`** (`crates/model/src/lib.rs`): the persistent description. Store
  references (`EdgeRef`), not kernel ids, and values as expression strings (`distance: String`)
  so parameters drive them. New optional fields get `#[serde(default)]` so older `.fox` files
  still load.
- **`FeatureKind::base_name`**: `"Chamfer"`, the stem of new feature names (`Chamfer3`).
- **`for_each_expr`**: visit every expression field, so parameter renames and dependency
  tracking see them.
- **Regeneration** (`crates/model/src/regen.rs`, the `FeatureKind::Chamfer` arm of the feature
  match): evaluate the expressions (`self.len(..)`, `self.angle(..)`), then hand the kernel call
  to `edge_op`, which resolves the edge references on each body (re-finding them after upstream
  changes) and replaces the body with the result. Return `Err(..)` with a short message for a
  bad input; the feature turns red and the rest of the timeline keeps going.
- **Test** in `crates/model/src/regen/tests.rs`: build a design, add the feature, check the
  result, change a parameter and check it again.

### 3. Engine: the JSON command

- **Listing** (`cmds!` at the top of `crates/engine/src/commands.rs`):
  `"chamfer" => "help text", "{edges: [{body, edge}], distance, distance2? | angle?}";`. This
  list drives `fissionoxide-cli`, the MCP server's tool list and the `every_listed_command_is_handled`
  test.
- **Handler**: the `"chamfer"` arm parses the parameters with the helpers there (`edge_refs`,
  `req_expr`, `opt_expr`) and calls `add_feature(s, FeatureKind::Chamfer { .. })`, which records
  an undo step, regenerates and returns the feature status. A group of new commands belongs in its
  own module with a `run(s, name, p) -> Option<CmdResult>` hooked in next to `features::run`.
- Editing needs nothing extra: `feature.edit` merges JSON fields into the stored `FeatureKind`.
- **Test** in `crates/engine/src/tests.rs`: drive it the way a user or script would, through
  `Session::execute` (see `extrude_options_press_pull_chamfers_and_split_by_body`).

### 4. UI: the dialog

In `crates/ui/src/dialogs.rs`, each `Kind::Chamfer` match arm does one job:

| Where            | What it does for chamfer                                                  |
|------------------|---------------------------------------------------------------------------|
| `enum Kind`      | the `Chamfer` variant                                                      |
| `title`, `icon`  | `"CHAMFER"`, `Icon::Chamfer`                                               |
| `picks_model`    | it picks edges of the bodies it changes (preview on demand)                |
| `Dialog::open`   | defaults: `d.radius = "1 mm"`                                              |
| `Dialog::edit`   | fills the dialog from an existing `FeatureKind::Chamfer` (double-click in the timeline) |
| `command`        | `("chamfer", json!({"edges": .., "distance": ..}))`                        |
| `edit_set`       | the fields `feature.edit` changes: `{"distance": ..}`                      |
| `ready`, `missing_input` | needs at least one edge; otherwise shows "select edges"           |
| `fields`         | the edge selector and the Distance field                                   |

The preview is the command itself: the dialog runs it on every change and withdraws it on
Cancel, so there is no separate preview code. A dialog that only picks selections and asks for
values doesn't need any of the above: declare a `Form` in `crates/ui/src/forms.rs` (see
`forms::RIB`) and open it as `Kind::Form(&forms::RIB)`.

Then make it reachable:

- **Toolbar menus** (`crates/ui/src/menus.rs`): `c("Chamfer", Icon::Chamfer, D(Kind::Chamfer))`
  in the Solid → Modify list (and other workspaces that have it).
- **Icon** (`crates/ui/src/icons.rs`): an `Icon::Chamfer` variant and its drawing arm.
- **Marking menu** (`crates/ui/src/marking.rs`): offered when edges are selected.
- **Timeline icon** (`crates/ui/src/panels.rs`): `FeatureKind::Chamfer { .. } => Icon::Chamfer`.

### 5. Docs

Add a line to "What works today" in `README.md` and remove the item from `ROADMAP.md` if it
was there.

## Releasing

A release is a tag. Bump `version` under `[workspace.package]` in `Cargo.toml` (every crate and
the installer take it from there), merge that to `main`, then tag it:

```sh
git tag v0.2.0 && git push origin v0.2.0
```

`.github/workflows/release.yml` then builds the release executables on Windows, packs them into
`FissionOxide-0.2.0-x64-setup.exe` with Inno Setup (`installer/windows/fissionoxide.iss`), writes
its SHA-256 next to it, installs and uninstalls it once as a smoke test, and publishes both as the
GitHub release `FissionOxide 0.2.0`. Installed copies find it through the releases API and update
themselves (`crates/desktop/src/update.rs`). A tag with a pre-release (`v0.3.0-beta.1`) becomes a
GitHub pre-release, which installed copies don't pick up. Run the workflow by hand (Actions →
Release → Run workflow) to get the installer as an artifact without publishing.

Rules that keep updates working:

- Never change the `AppId` in `fissionoxide.iss`: it is how Windows and later installers find the
  installed copy to replace.
- Keep the asset names (`FissionOxide-<version>-x64-setup.exe` and `….exe.sha256`); the updater
  looks for them.
- The release feed must be readable without signing in, so the repository (or at least its
  releases) has to be public. `FOX_UPDATE_URL` points a build at another feed with the same JSON as
  `GET /repos/{owner}/{repo}/releases/latest`, for testing or a mirror.
- Sign the executables and installer so Windows SmartScreen trusts them: add the repository
  secrets `WINDOWS_CERTIFICATE` (a `.pfx`, base64) and `WINDOWS_CERTIFICATE_PASSWORD` and the
  workflow signs everything, the uninstaller included.

To build the installer locally on Windows: `cargo build --release -p fissionoxide -p
fissionoxide-cli`, then `iscc /DAppVersion=0.2.0 installer\windows\fissionoxide.iss` (Inno Setup
6.4 or later); it lands in `dist\`. The executables' icon, version and manifest come from
`apps/fissionoxide/build.rs`; the icon and wizard images from `brand/windows/`, which
`brand/build.py` generates.

## Pull requests

Keep a pull request to one area, merge `origin/main` into your branch before opening it, and
re-run the checks above. CI must be green before merging.
