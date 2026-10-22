//! Plugins: installable `.foxplugin` packages whose WebAssembly code drives the engine through
//! the same JSON commands as the UI, the CLI and the MCP server, sandboxed to what its manifest
//! asked for and the user granted. See docs/PLUGINS.md.
//!
//! Installed plugins live unpacked under `<data hub>/plugins/<id>/<version>/`, recorded in
//! `plugins/installed.json`; each plugin's storage and log sit beside its versions, so they
//! survive updates. A `plugins/` folder next to the executable holds bundled plugins.

pub mod commands;
pub mod hub;
pub mod inputs;
pub mod manifest;
pub mod package;
pub mod runtime;
pub mod sandbox;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use fox_engine::Session;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use manifest::{Command, Input, InputKind, Manifest, Permission};
pub use package::Package;
pub use runtime::LogLine;

/// The record of one installed plugin in `installed.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Record {
  pub id: String,
  pub version: String,
  pub enabled: bool,
  /// Permissions the user granted (a subset of what the manifest asks for).
  #[serde(default)]
  pub granted: Vec<Permission>,
  /// When it was installed or last updated (seconds since 1970).
  #[serde(default)]
  pub installed: u64,
  /// Where it came from: a file path or `hub:<item>/<version>`.
  #[serde(default)]
  pub source: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct State {
  #[serde(default)]
  plugins: Vec<Record>,
  /// Bundled plugins the user turned off.
  #[serde(default)]
  disabled_bundled: Vec<String>,
  /// Bundled plugins that start off (`enabled_by_default = false`) and the user turned on.
  #[serde(default)]
  enabled_bundled: Vec<String>,
}

/// A plugin as loaded: its record, and its manifest or why it failed to load.
#[derive(Debug, Clone)]
pub struct Installed {
  pub record: Record,
  pub dir: PathBuf,
  pub manifest: Option<Manifest>,
  /// Why it can't be used (a broken or incompatible package); it is skipped, nothing else is.
  pub error: Option<String>,
  /// Shipped with the app (can be turned off, not removed).
  pub bundled: bool,
  /// It starts with the app (`record.enabled` is whether it runs now: the two differ after
  /// [`Plugins::set_running`]).
  pub startup: bool,
}

impl Installed {
  pub fn usable(&self) -> bool {
    self.record.enabled && self.manifest.is_some() && self.error.is_none()
  }

  pub fn name(&self) -> &str {
    self.manifest.as_ref().map_or(&self.record.id, |m| &m.name)
  }

  pub fn to_json(&self) -> Value {
    let m = self.manifest.as_ref();
    json!({
      "id": self.record.id,
      "name": self.name(),
      "version": self.record.version,
      "enabled": self.record.enabled,
      "bundled": self.bundled,
      "author": m.map(|m| m.author.as_str()),
      "description": m.map(|m| m.description.as_str()),
      "homepage": m.map(|m| m.homepage.as_str()),
      "permissions": m.map(|m| &m.permissions),
      "granted": self.record.granted,
      "installed": self.record.installed,
      "source": self.record.source,
      "commands": m.map(|m| m.commands.iter().map(|c| json!({
        "id": format!("{}/{}", m.id, c.key()),
        "label": c.label(),
        "builtin": c.builtin,
        "step": c.step,
        "tooltip": c.tooltip,
        "inputs": c.inputs,
      })).collect::<Vec<_>>()),
      "error": self.error,
      "dir": self.dir,
    })
  }
}

/// What installing a package would do.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Plan {
  Install,
  /// A newer version of an installed plugin.
  Update {
    from: String,
  },
  /// The same version again.
  Reinstall,
  /// An older version than the installed one: only with `allow_downgrade`.
  Downgrade {
    from: String,
  },
}

#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
  /// Permissions to grant (`None`: everything the manifest asks for).
  pub grant: Option<Vec<Permission>>,
  /// Allow installing the same or an older version over the installed one.
  pub allow_downgrade: bool,
  pub source: String,
}

/// A plugin command, for toolbars and menus.
#[derive(Debug, Clone)]
pub struct CommandRef {
  /// `<plugin id>/<command id>`.
  pub full_id: String,
  pub plugin: String,
  pub plugin_name: String,
  pub command: Command,
  pub tab: String,
  pub panel: String,
  /// The icon file on disk.
  pub icon: Option<PathBuf>,
  /// The plugin may read or write files, so a run needs a folder.
  pub uses_files: bool,
}

/// How a run goes.
#[derive(Default)]
pub struct RunOptions {
  /// The folder the plugin reads files from and writes them to.
  pub folder: Option<PathBuf>,
  /// Called on `progress`; returning false cancels the run.
  pub progress: Option<runtime::Progress>,
  /// Instruction budget (default [`runtime::DEFAULT_FUEL`]).
  pub fuel: Option<u64>,
}

/// What a run did.
#[derive(Debug, Clone, Serialize)]
pub struct RunResult {
  /// What the plugin returned.
  pub result: Value,
  /// Whether the design changed (one undo step named after the command).
  pub changed: bool,
  /// Engine commands the plugin ran.
  pub commands: usize,
  pub log: Vec<LogLine>,
  /// Files it wrote.
  pub files: Vec<PathBuf>,
}

/// The installed plugins.
pub struct Plugins {
  root: PathBuf,
  bundled_dirs: Vec<PathBuf>,
  state: State,
  pub list: Vec<Installed>,
  modules: HashMap<PathBuf, wasmtime::Module>,
  /// Library folders of usable plugins, added to the sketch library.
  libraries: Vec<PathBuf>,
  /// Plugins started or stopped for this session only, by id (whether they run).
  running: HashMap<String, bool>,
}

fn now() -> u64 {
  fox_engine::store::now()
}

impl Plugins {
  /// The plugins of a data hub (`<hub>/plugins`), with the bundled ones next to the executable
  /// and, in a build from a checkout, the add-ins in the repository's `addins/`.
  pub fn for_hub(hub_root: &Path) -> Plugins {
    let mut bundled: Vec<PathBuf> =
      std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("plugins"))).into_iter().collect();
    let addins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../addins");
    if addins.is_dir() {
      bundled.push(addins);
    }
    Plugins::open(&hub_root.join("plugins"), bundled)
  }

  /// The plugins installed under `root`, plus unpacked plugins in each of `bundled_dirs`.
  pub fn open(root: &Path, bundled_dirs: Vec<PathBuf>) -> Plugins {
    let mut p = Plugins {
      root: root.to_path_buf(),
      bundled_dirs,
      state: State::default(),
      list: Vec::new(),
      modules: HashMap::new(),
      libraries: Vec::new(),
      running: HashMap::new(),
    };
    p.reload();
    p
  }

  pub fn root(&self) -> &Path {
    &self.root
  }

  fn state_path(&self) -> PathBuf {
    self.root.join("installed.json")
  }

  /// Re-reads the registry and every plugin's manifest.
  pub fn reload(&mut self) {
    self.state =
      std::fs::read_to_string(self.state_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    self.modules.clear();
    let mut list = Vec::new();
    for r in &self.state.plugins {
      let dir = self.root.join(&r.id).join(&r.version);
      let (manifest, error) = load_manifest(&dir);
      list.push(Installed { record: r.clone(), dir, manifest, error, bundled: false, startup: r.enabled });
    }
    for bdir in &self.bundled_dirs {
      let Ok(entries) = std::fs::read_dir(bdir) else { continue };
      let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
      dirs.sort();
      for dir in dirs {
        let (manifest, error) = load_manifest(&dir);
        let Some(m) = &manifest else {
          log::warn!("bundled plugin {}: {}", dir.display(), error.unwrap_or_default());
          continue;
        };
        // a version the user installed (or an earlier bundled folder) wins over this one
        if list.iter().any(|i: &Installed| i.record.id == m.id) {
          continue;
        }
        let enabled = if m.enabled_by_default {
          !self.state.disabled_bundled.contains(&m.id)
        } else {
          self.state.enabled_bundled.contains(&m.id)
        };
        let record = Record {
          id: m.id.clone(),
          version: m.version.clone(),
          enabled,
          granted: m.permissions.clone(),
          installed: 0,
          source: dir.to_string_lossy().into_owned(),
        };
        list.push(Installed { record, dir, manifest, error, bundled: true, startup: enabled });
      }
    }
    for p in &mut list {
      if let Some(on) = self.running.get(&p.record.id) {
        p.record.enabled = *on;
      }
    }
    self.list = list;
    self.update_libraries();
  }

  /// Offers the library packs of the plugins that are on in the sketch library, and withdraws
  /// those of plugins turned off or removed.
  fn update_libraries(&mut self) {
    let now: Vec<PathBuf> = self
      .list
      .iter()
      .filter(|p| p.usable())
      .filter_map(|p| p.manifest.as_ref()?.library.as_ref().map(|l| p.dir.join(l)))
      .collect();
    for old in self.libraries.iter().filter(|l| !now.contains(l)) {
      fox_engine::library::remove_root(old);
    }
    for l in &now {
      fox_engine::library::add_root(l.clone());
    }
    self.libraries = now;
  }

  fn save(&self) -> Result<(), String> {
    std::fs::create_dir_all(&self.root).map_err(|e| format!("{}: {e}", self.root.display()))?;
    let text = serde_json::to_string_pretty(&self.state).map_err(|e| e.to_string())?;
    let tmp = self.root.join("installed.json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, self.state_path()).map_err(|e| e.to_string())
  }

  pub fn get(&self, id: &str) -> Option<&Installed> {
    self.list.iter().find(|p| p.record.id == id)
  }

  fn find(&self, id: &str) -> Result<&Installed, String> {
    self.get(id).ok_or_else(|| format!("no plugin '{id}' is installed"))
  }

  /// What installing `pkg` would do.
  pub fn plan(&self, pkg: &Package) -> Plan {
    match self.get(&pkg.manifest.id).filter(|p| !p.bundled) {
      None => Plan::Install,
      Some(cur) => {
        let from = cur.record.version.clone();
        let old = semver::Version::parse(&from).unwrap_or_else(|_| semver::Version::new(0, 0, 0));
        match pkg.manifest.version().cmp(&old) {
          std::cmp::Ordering::Greater => Plan::Update { from },
          std::cmp::Ordering::Equal => Plan::Reinstall,
          std::cmp::Ordering::Less => Plan::Downgrade { from },
        }
      }
    }
  }

  /// Installs (or updates) a checked package; its commands are usable straight away.
  pub fn install(&mut self, pkg: &Package, opts: InstallOptions) -> Result<Plan, String> {
    let plan = self.plan(pkg);
    if matches!(plan, Plan::Downgrade { .. } | Plan::Reinstall) && !opts.allow_downgrade {
      let what = if plan == Plan::Reinstall { "is already installed" } else { "is older than the installed one" };
      return Err(format!("{} {} {what}", pkg.manifest.name, pkg.manifest.version));
    }
    let m = &pkg.manifest;
    let base = self.root.join(&m.id);
    let staging = base.join(format!(".installing-{}", m.version));
    let _ = std::fs::remove_dir_all(&staging);
    pkg.unpack(&staging)?;
    let dest = base.join(&m.version);
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&staging, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let old = self.state.plugins.iter().position(|r| r.id == m.id).map(|i| self.state.plugins.remove(i));
    if let Some(old) = &old
      && old.version != m.version
    {
      let _ = std::fs::remove_dir_all(base.join(&old.version));
    }
    let granted: Vec<Permission> = match opts.grant {
      Some(g) => m.permissions.iter().copied().filter(|p| g.contains(p)).collect(),
      None => m.permissions.clone(),
    };
    self.state.plugins.push(Record {
      id: m.id.clone(),
      version: m.version.clone(),
      enabled: old.as_ref().is_none_or(|o| o.enabled),
      granted,
      installed: now(),
      source: opts.source,
    });
    self.state.plugins.sort_by(|a, b| a.id.cmp(&b.id));
    self.save()?;
    self.running.remove(&m.id);
    self.append_log(&m.id, &format!("installed version {} ({:?})", m.version, plan));
    self.reload();
    Ok(plan)
  }

  pub fn install_file(&mut self, path: &Path, mut opts: InstallOptions) -> Result<(Manifest, Plan), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let pkg = Package::read(&bytes)?;
    if opts.source.is_empty() {
      opts.source = path.to_string_lossy().into_owned();
    }
    let plan = self.install(&pkg, opts)?;
    Ok((pkg.manifest, plan))
  }

  /// Removes an installed plugin with its storage and log.
  pub fn uninstall(&mut self, id: &str) -> Result<(), String> {
    if self.find(id)?.bundled {
      return Err(format!("'{id}' comes with FissionOxide: it can be turned off, not removed"));
    }
    self.state.plugins.retain(|r| r.id != id);
    self.save()?;
    self.running.remove(id);
    let _ = std::fs::remove_dir_all(self.root.join(id));
    self.reload();
    Ok(())
  }

  pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
    self.running.remove(id);
    self.set_startup(id, enabled)
  }

  /// Starts or stops a plugin for this session only (Fusion's Run switch); whether it starts
  /// with the app stays as it is.
  pub fn set_running(&mut self, id: &str, on: bool) -> Result<(), String> {
    self.find(id)?;
    self.running.insert(id.to_string(), on);
    self.reload();
    Ok(())
  }

  /// Whether a plugin starts with the app (Fusion's Run on Startup); whether it runs now stays
  /// as it is.
  pub fn set_run_on_startup(&mut self, id: &str, on: bool) -> Result<(), String> {
    let now = self.find(id)?.record.enabled;
    self.running.insert(id.to_string(), now);
    self.set_startup(id, on)
  }

  fn set_startup(&mut self, id: &str, enabled: bool) -> Result<(), String> {
    if self.find(id)?.bundled {
      self.state.disabled_bundled.retain(|d| d != id);
      self.state.enabled_bundled.retain(|d| d != id);
      if enabled {
        self.state.enabled_bundled.push(id.to_string());
      } else {
        self.state.disabled_bundled.push(id.to_string());
      }
    } else if let Some(r) = self.state.plugins.iter_mut().find(|r| r.id == id) {
      r.enabled = enabled;
    }
    self.save()?;
    self.reload();
    Ok(())
  }

  /// Grants exactly these permissions (limited to what the plugin asks for).
  pub fn set_granted(&mut self, id: &str, granted: &[Permission]) -> Result<(), String> {
    let p = self.find(id)?;
    if p.bundled {
      return Err(format!("'{id}' comes with FissionOxide; its permissions are fixed"));
    }
    let asked = p.manifest.as_ref().map(|m| m.permissions.clone()).unwrap_or_default();
    if let Some(r) = self.state.plugins.iter_mut().find(|r| r.id == id) {
      r.granted = asked.into_iter().filter(|a| granted.contains(a)).collect();
    }
    self.save()?;
    self.reload();
    Ok(())
  }

  fn log_path(&self, id: &str) -> PathBuf {
    self.root.join(id).join("log.txt")
  }

  /// The plugin's log (newest last).
  pub fn log(&self, id: &str) -> String {
    std::fs::read_to_string(self.log_path(id)).unwrap_or_default()
  }

  pub fn clear_log(&self, id: &str) {
    let _ = std::fs::remove_file(self.log_path(id));
  }

  fn append_log(&self, id: &str, text: &str) {
    use std::io::Write;
    let path = self.log_path(id);
    if let Some(d) = path.parent() {
      let _ = std::fs::create_dir_all(d);
    }
    // keep the log small: start again past 256 KB
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 256 << 10) {
      let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
      let time = fox_engine::store::format_time(now());
      for line in text.lines() {
        let _ = writeln!(f, "{time}  {line}");
      }
    }
  }

  /// Commands of every usable plugin, in manifest order.
  pub fn commands(&self) -> Vec<CommandRef> {
    let mut out = Vec::new();
    for p in self.list.iter().filter(|p| p.usable()) {
      let m = p.manifest.as_ref().expect("usable plugins have a manifest");
      let uses_files =
        m.permissions.iter().any(|p| matches!(p, Permission::ReadFiles | Permission::WriteFiles | Permission::Render));
      // the tab is laid out in workflow order: numbered steps first
      let mut cmds: Vec<&Command> = m.commands.iter().collect();
      cmds.sort_by_key(|c| c.step.unwrap_or(u32::MAX));
      for c in cmds {
        let (tab, panel) = m.placement(c);
        out.push(CommandRef {
          full_id: format!("{}/{}", m.id, c.key()),
          plugin: m.id.clone(),
          plugin_name: m.name.clone(),
          command: c.clone(),
          tab,
          panel,
          icon: c.icon.as_ref().map(|i| p.dir.join(i)),
          uses_files,
        });
      }
    }
    out
  }

  pub fn command(&self, full_id: &str) -> Result<CommandRef, String> {
    let (plugin, _) =
      full_id.split_once('/').ok_or_else(|| format!("'{full_id}': plugin commands are <plugin id>/<command id>"))?;
    let p = self.find(plugin)?;
    if let Some(e) = &p.error {
      return Err(format!("{}: {e}", p.name()));
    }
    if !p.record.enabled {
      return Err(format!("{} is turned off", p.name()));
    }
    self
      .commands()
      .into_iter()
      .find(|c| c.full_id == full_id)
      .ok_or_else(|| format!("{} has no command '{full_id}'", p.name()))
  }

  fn module(&mut self, dir: &Path) -> Result<wasmtime::Module, String> {
    if let Some(m) = self.modules.get(dir) {
      return Ok(m.clone());
    }
    let wasm = std::fs::read(dir.join(package::MODULE)).map_err(|e| format!("plugin.wasm: {e}"))?;
    let m = runtime::compile(&wasm)?;
    self.modules.insert(dir.to_path_buf(), m.clone());
    Ok(m)
  }

  fn host(&self, session: &mut Session, plugin: &str, folder: Option<PathBuf>) -> Result<runtime::Host, String> {
    let p = self.find(plugin)?;
    let ctx = sandbox::Context { granted: p.record.granted.clone(), folder, resources: p.dir.join("resources") };
    Ok(runtime::Host::new(std::mem::take(session), ctx, self.root.join(plugin).join("storage.json")))
  }

  /// Runs a plugin command (the user pressed OK): its inputs are checked and filled in, and
  /// everything it does is one undo step named after it, rolled back if it fails or is cancelled.
  pub fn run(
    &mut self,
    session: &mut Session,
    full_id: &str,
    given: &Value,
    opts: RunOptions,
  ) -> Result<RunResult, String> {
    let cmd = self.command(full_id)?;
    let values = inputs::resolve(&cmd.command, given, session)?;
    if let Some(b) = &cmd.command.builtin {
      return self.run_builtin(session, &cmd, b, &values, opts.folder);
    }
    let dir = self.find(&cmd.plugin)?.dir.clone();
    let module = self.module(&dir)?;
    session.begin_transaction().map_err(|e| e.0)?;
    let mut host = self.host(session, &cmd.plugin, opts.folder)?;
    host.progress = opts.progress;
    let entry = runtime::Entry::Run { command: &cmd.command.id, inputs: &values };
    let (host, result) = runtime::invoke(&module, host, entry, opts.fuel.unwrap_or(runtime::DEFAULT_FUEL));
    *session = host.session;
    let mut text: Vec<String> = host
      .log
      .iter()
      .map(|l| format!("[{}] {}", ["error", "warn", "info", "debug"][l.level as usize], l.message))
      .collect();
    let out = match result {
      Ok(result) => {
        let changed = session.commit_transaction_named(cmd.command.label()).map_err(|e| e.0)?;
        text.push(format!("{}: done ({} commands)", cmd.command.label(), host.commands));
        Ok(RunResult { result, changed, commands: host.commands, log: host.log, files: host.written })
      }
      Err(e) => {
        session.rollback_transaction().map_err(|e| e.0)?;
        text.push(format!("{}: failed: {e}", cmd.command.label()));
        Err(e)
      }
    };
    self.append_log(&cmd.plugin, &text.join("\n"));
    out
  }

  /// A built-in command on a plugin's tab: run with its inputs as parameters (single picks as
  /// ids), under the same rules as the plugin's own `execute` calls.
  fn run_builtin(
    &mut self,
    session: &mut Session,
    cmd: &CommandRef,
    builtin: &str,
    values: &Value,
    folder: Option<PathBuf>,
  ) -> Result<RunResult, String> {
    let p = self.find(&cmd.plugin)?;
    let ctx = sandbox::Context { granted: p.record.granted.clone(), folder, resources: p.dir.join("resources") };
    let params = ctx.prepare(builtin, &inputs::as_params(&cmd.command, values))?;
    session.begin_transaction().map_err(|e| e.0)?;
    let result =
      session.execute(builtin, &params).map_err(|e| e.0).and_then(|v| match v.get("error").and_then(Value::as_str) {
        Some(e) => Err(e.to_string()),
        None => Ok(v),
      });
    match result {
      Ok(result) => {
        let changed = session.commit_transaction_named(cmd.command.label()).map_err(|e| e.0)?;
        Ok(RunResult { result, changed, commands: 1, log: Vec::new(), files: Vec::new() })
      }
      Err(e) => {
        session.rollback_transaction().map_err(|e| e.0)?;
        self.append_log(&cmd.plugin, &format!("{}: failed: {e}", cmd.command.label()));
        Err(e)
      }
    }
  }

  /// An input changed while the dialog is open: the plugin returns updates for other inputs
  /// (`{"<input>": {"value"?, "visible"?, "enabled"?, "text"?, "options"?}}`). Anything it does
  /// to the design is rolled back.
  pub fn input_changed(
    &mut self,
    session: &mut Session,
    full_id: &str,
    input: &str,
    given: &Value,
  ) -> Result<Value, String> {
    let cmd = self.command(full_id)?;
    if cmd.command.builtin.is_some() {
      return Ok(json!({}));
    }
    let values = inputs::resolve_partial(&cmd.command, given, session);
    let dir = self.find(&cmd.plugin)?.dir.clone();
    let module = self.module(&dir)?;
    session.begin_transaction().map_err(|e| e.0)?;
    let host = self.host(session, &cmd.plugin, None)?;
    let entry = runtime::Entry::InputChanged { command: &cmd.command.id, input, inputs: &values };
    let (host, result) = runtime::invoke(&module, host, entry, runtime::DEFAULT_FUEL / 10);
    *session = host.session;
    session.rollback_transaction().map_err(|e| e.0)?;
    if !host.log.is_empty() {
      self.append_log(&cmd.plugin, &host.log.iter().map(|l| l.message.as_str()).collect::<Vec<_>>().join("\n"));
    }
    result.map(|v| if v.is_object() { v } else { json!({}) })
  }
}

fn load_manifest(dir: &Path) -> (Option<Manifest>, Option<String>) {
  match std::fs::read_to_string(dir.join(package::MANIFEST)) {
    Err(e) => (None, Some(format!("plugin.toml: {e}"))),
    Ok(text) => match Manifest::parse(&text) {
      Ok(m) => {
        let error = if dir.join(package::MODULE).is_file() { None } else { Some("plugin.wasm is missing".into()) };
        (Some(m), error)
      }
      Err(e) => (None, Some(e)),
    },
  }
}
