//! `plugin.toml`: what a plugin is, what it adds to the toolbar and what it may do.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The plugin API this app implements (`api` in the manifest).
pub const API_VERSION: u32 = 1;

/// The app version manifests are checked against (`fissionoxide` in the manifest).
pub fn app_version() -> semver::Version {
  semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is semver")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
  /// Reverse-DNS id, unique among plugins (`com.example.tools`).
  pub id: String,
  pub name: String,
  /// Semver; installing a newer version updates in place.
  pub version: String,
  #[serde(default)]
  pub author: String,
  #[serde(default)]
  pub description: String,
  #[serde(default)]
  pub homepage: String,
  /// App versions it works with, as a semver requirement (`">=0.1"`).
  #[serde(default = "any_version")]
  pub fissionoxide: String,
  /// Plugin API version; must be [`API_VERSION`].
  pub api: u32,
  /// What the plugin may do beyond running design commands; shown at install.
  #[serde(default)]
  pub permissions: Vec<Permission>,
  #[serde(default)]
  pub panels: Vec<Panel>,
  #[serde(default)]
  pub commands: Vec<Command>,
}

fn any_version() -> String {
  "*".into()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
  /// Read files from the folder picked for the run (`import`, `sketch.insert`, `inspect` ...).
  ReadFiles,
  /// Write files into the folder picked for the run (`export`, `write_file`, `cut.gcode` ...).
  WriteFiles,
  /// Render images of the model into the run's folder (`view.render`).
  Render,
  /// Use the data hub (`hub.*` commands): list, upload, download, comment.
  Hub,
}

impl Permission {
  pub const ALL: [Permission; 4] = [Permission::ReadFiles, Permission::WriteFiles, Permission::Render, Permission::Hub];

  pub fn id(self) -> &'static str {
    match self {
      Permission::ReadFiles => "read-files",
      Permission::WriteFiles => "write-files",
      Permission::Render => "render",
      Permission::Hub => "hub",
    }
  }

  pub fn parse(s: &str) -> Option<Permission> {
    Permission::ALL.into_iter().find(|p| p.id() == s)
  }

  /// What granting it means, for the install dialog.
  pub fn describe(self) -> &'static str {
    match self {
      Permission::ReadFiles => "Read files in the folder you pick when running a command",
      Permission::WriteFiles => "Write files into the folder you pick when running a command",
      Permission::Render => "Render images of the model into that folder",
      Permission::Hub => "Read and change projects and designs in your data hub",
    }
  }
}

/// A toolbar panel; panels are created in manifest order, grouped into tabs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Panel {
  pub id: String,
  pub label: String,
  /// The toolbar tab it sits on (default: the plugin's name).
  #[serde(default)]
  pub tab: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Command {
  /// Unique within the plugin; the full id is `<plugin id>/<command id>`. Empty for a
  /// built-in command shown on the plugin's tab.
  #[serde(default)]
  pub id: String,
  /// A built-in engine command shown on the plugin's tab too (`"sketch.remove_duplicates"`),
  /// instead of one of the plugin's own; it keeps its place elsewhere in the app. Its inputs, if
  /// any, become its parameters.
  #[serde(default)]
  pub builtin: Option<String>,
  /// Default for a built-in command: its id.
  #[serde(default)]
  pub label: String,
  /// Where it comes in the workflow: the tab is laid out in step order and the icon shows it.
  #[serde(default)]
  pub step: Option<u32>,
  #[serde(default)]
  pub tooltip: String,
  /// Path of a PNG in the package (32×32 or larger, square).
  #[serde(default)]
  pub icon: Option<String>,
  /// The panel it sits in (default: one panel named after the plugin).
  #[serde(default)]
  pub panel: Option<String>,
  /// Shown in the panel's main row, not only in its menu.
  #[serde(default)]
  pub promoted: bool,
  /// The dialog stays open for another run after OK.
  #[serde(default)]
  pub repeatable: bool,
  #[serde(default)]
  pub inputs: Vec<Input>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
  Selection,
  Value,
  Integer,
  Slider,
  Checkbox,
  Choice,
  Text,
  Button,
  Label,
  Group,
}

impl InputKind {
  /// Whether the input carries a value passed to the plugin.
  pub fn has_value(self) -> bool {
    !matches!(self, InputKind::Button | InputKind::Label | InputKind::Group)
  }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
  #[default]
  Length,
  Angle,
  Unitless,
}

/// What a selection input accepts.
pub const SELECTION_FILTERS: [&str; 9] =
  ["sketch", "curve", "point", "profile", "body", "face", "edge", "vertex", "feature"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Input {
  pub id: String,
  #[serde(rename = "type")]
  pub kind: InputKind,
  /// Shown beside the input (default: the id).
  #[serde(default)]
  pub label: Option<String>,
  /// Hint text (selection prompt, tooltip).
  #[serde(default)]
  pub prompt: String,
  /// Initial value: number or expression (value), integer, bool, option, text; the text of a label.
  #[serde(default)]
  pub default: Option<Value>,
  /// Selection: what may be picked ([`SELECTION_FILTERS`]; empty = anything).
  #[serde(default)]
  pub filter: Vec<String>,
  /// Selection: how many (default 1 and unlimited); integer and slider: the range.
  #[serde(default)]
  pub min: Option<f64>,
  #[serde(default)]
  pub max: Option<f64>,
  /// Slider and integer step.
  #[serde(default)]
  pub step: Option<f64>,
  /// Value: the quantity (length in mm, angle in degrees, or a plain number).
  #[serde(default)]
  pub unit: Option<Unit>,
  /// Choice: the options.
  #[serde(default)]
  pub options: Vec<String>,
  /// The group input this one sits in.
  #[serde(default)]
  pub group: Option<String>,
  /// Shown only while the other inputs have these values.
  #[serde(default)]
  pub visible_when: Option<Map<String, Value>>,
  /// Editable only while the other inputs have these values.
  #[serde(default)]
  pub enabled_when: Option<Map<String, Value>>,
}

impl Command {
  /// Its id within the plugin: its own id, or the built-in command's.
  pub fn key(&self) -> &str {
    self.builtin.as_deref().unwrap_or(&self.id)
  }

  pub fn label(&self) -> &str {
    if self.label.is_empty() { self.key() } else { &self.label }
  }
}

impl Input {
  pub fn label(&self) -> &str {
    self.label.as_deref().unwrap_or(&self.id)
  }

  pub fn min_count(&self) -> usize {
    self.min.map_or(1, |m| m.max(0.0) as usize)
  }

  pub fn max_count(&self) -> Option<usize> {
    self.max.map(|m| m.max(0.0) as usize)
  }
}

/// Whether `values` satisfy a `visible_when` / `enabled_when` condition.
pub fn condition_holds(cond: Option<&Map<String, Value>>, values: &Map<String, Value>) -> bool {
  cond.is_none_or(|c| c.iter().all(|(k, want)| values.get(k).is_some_and(|v| same_value(v, want))))
}

fn same_value(a: &Value, b: &Value) -> bool {
  match (a.as_f64(), b.as_f64()) {
    (Some(x), Some(y)) => (x - y).abs() < 1e-9,
    _ => a == b,
  }
}

fn is_id(s: &str) -> bool {
  !s.is_empty()
    && s.len() <= 64
    && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

impl Manifest {
  pub fn parse(text: &str) -> Result<Manifest, String> {
    let m: Manifest = toml::from_str(text).map_err(|e| format!("plugin.toml: {}", e.message()))?;
    m.validate()?;
    Ok(m)
  }

  pub fn version(&self) -> semver::Version {
    semver::Version::parse(&self.version).unwrap_or_else(|_| semver::Version::new(0, 0, 0))
  }

  /// The tab and panel a command sits in, by label.
  pub fn placement(&self, cmd: &Command) -> (String, String) {
    match cmd.panel.as_deref().and_then(|p| self.panels.iter().find(|q| q.id == p)) {
      Some(p) => (p.tab.clone().unwrap_or_else(|| self.name.clone()), p.label.clone()),
      None => (self.name.clone(), self.name.to_uppercase()),
    }
  }

  pub fn command(&self, id: &str) -> Option<&Command> {
    self.commands.iter().find(|c| c.key() == id)
  }

  /// Checks everything that doesn't need the package's other files.
  pub fn validate(&self) -> Result<(), String> {
    let parts: Vec<&str> = self.id.split('.').collect();
    if parts.len() < 2 || !parts.iter().all(|p| is_id(p)) || self.id.len() > 128 {
      return Err(format!("id '{}' must be reverse-DNS, like com.example.tools (lower case)", self.id));
    }
    if self.name.trim().is_empty() {
      return Err("name is empty".into());
    }
    semver::Version::parse(&self.version).map_err(|e| format!("version '{}': {e}", self.version))?;
    let req = semver::VersionReq::parse(&self.fissionoxide)
      .map_err(|e| format!("fissionoxide = '{}': {e}", self.fissionoxide))?;
    let app = app_version();
    if !req.matches(&app) {
      return Err(format!("needs FissionOxide {}, this is {app}", self.fissionoxide));
    }
    if self.api != API_VERSION {
      return Err(format!("needs plugin API {}, this app has API {API_VERSION}", self.api));
    }
    let mut seen = HashSet::new();
    for p in &self.panels {
      if !is_id(&p.id) || !seen.insert(p.id.as_str()) {
        return Err(format!("panel id '{}' is invalid or repeated", p.id));
      }
    }
    let mut seen = HashSet::new();
    for c in &self.commands {
      match &c.builtin {
        Some(b) => {
          let ok = !b.is_empty() && b.split('.').all(is_id) && c.id.is_empty();
          if !ok || !seen.insert(b.as_str()) {
            return Err(format!("builtin command '{b}' is invalid or repeated (and takes no id)"));
          }
        }
        None => {
          if !is_id(&c.id) || !seen.insert(c.id.as_str()) {
            return Err(format!("command id '{}' is invalid or repeated", c.id));
          }
          if c.label.trim().is_empty() {
            return Err(format!("command '{}' has no label", c.id));
          }
        }
      }
      if let Some(p) = &c.panel
        && !self.panels.iter().any(|q| &q.id == p)
      {
        return Err(format!("command '{}': no panel '{p}'", c.key()));
      }
      validate_inputs(c).map_err(|e| format!("command '{}': {e}", c.key()))?;
    }
    Ok(())
  }
}

fn validate_inputs(c: &Command) -> Result<(), String> {
  let mut seen = HashSet::new();
  for i in &c.inputs {
    if !is_id(&i.id) || !seen.insert(i.id.as_str()) {
      return Err(format!("input id '{}' is invalid or repeated", i.id));
    }
  }
  let kind_of = |id: &str| c.inputs.iter().find(|i| i.id == id).map(|i| i.kind);
  for i in &c.inputs {
    let bad = |what: &str| Err(format!("input '{}': {what}", i.id));
    match i.kind {
      InputKind::Selection => {
        if let Some(f) = i.filter.iter().find(|f| !SELECTION_FILTERS.contains(&f.as_str())) {
          return bad(&format!("unknown filter '{f}' (one of {})", SELECTION_FILTERS.join(", ")));
        }
        if i.max_count().is_some_and(|m| m < i.min_count().max(1)) {
          return bad("max is below min");
        }
      }
      InputKind::Choice => {
        if i.options.is_empty() {
          return bad("a choice needs options");
        }
        if let Some(d) = &i.default
          && !d.as_str().is_some_and(|d| i.options.iter().any(|o| o == d))
        {
          return bad("the default is not one of the options");
        }
      }
      InputKind::Slider if i.min.is_none() || i.max.is_none() => return bad("a slider needs min and max"),
      InputKind::Value if i.default.as_ref().is_some_and(|d| !d.is_number() && !d.is_string()) => {
        return bad("the default must be a number or an expression");
      }
      InputKind::Integer | InputKind::Slider if i.default.as_ref().is_some_and(|d| !d.is_number()) => {
        return bad("the default must be a number");
      }
      InputKind::Checkbox if i.default.as_ref().is_some_and(|d| !d.is_boolean()) => {
        return bad("the default must be true or false");
      }
      _ => {}
    }
    if let (Some(lo), Some(hi)) = (i.min, i.max)
      && lo > hi
    {
      return bad("min is above max");
    }
    if let Some(g) = &i.group
      && kind_of(g) != Some(InputKind::Group)
    {
      return bad(&format!("no group input '{g}'"));
    }
    for cond in [&i.visible_when, &i.enabled_when].into_iter().flatten() {
      if let Some(k) = cond.keys().find(|k| kind_of(k).is_none_or(|k| !k.has_value())) {
        return bad(&format!("condition on '{k}', which is not a value input"));
      }
    }
  }
  Ok(())
}
