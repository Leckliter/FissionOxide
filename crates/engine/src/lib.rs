//! The CLFusion engine: an editing session over a design, with undo/redo, regeneration and a
//! JSON command interface.
//!
//! Everything the app can do is a command (`session.execute("extrude", json!({...}))`): the UI,
//! the CLI, scripts and agents all go through the same entry point, so anything you can click you
//! can also script.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

mod assembly;
mod commands;
mod drawings;
pub mod files;
pub mod hub;
pub mod scene;

use std::path::PathBuf;

use clf_model::regen::{Model, Regenerator};
use clf_model::{BodyId, Design, FeatureId};
use serde_json::Value;

pub use clf_exchange as exchange;
pub use clf_kernel as kernel;
pub use clf_model as model;
pub use clf_render as render;
pub use clf_sketch as sketch;
pub use clf_store as store;
pub use commands::{COMMANDS, CommandSpec};
pub use hub::HubLink;

/// Undo history depth.
const MAX_UNDO: usize = 200;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct CmdError(pub String);

impl CmdError {
  pub fn new(s: impl Into<String>) -> Self {
    CmdError(s.into())
  }
}

macro_rules! impl_from_err {
  ($($t:ty),*) => {$(
    impl From<$t> for CmdError {
      fn from(e: $t) -> Self {
        CmdError(e.to_string())
      }
    }
  )*};
}
impl_from_err!(
  clf_model::DesignError,
  clf_sketch::SketchError,
  clf_kernel::KernelError,
  clf_expr::ExprError,
  std::io::Error,
  serde_json::Error
);

pub type CmdResult = Result<Value, CmdError>;

/// Something selected in the viewport or browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Sel {
  Body {
    body: BodyId,
  },
  Face {
    body: BodyId,
    face: u32,
  },
  Edge {
    body: BodyId,
    edge: u32,
  },
  /// A sketch region, by index into the sketch's profile list.
  Profile {
    sketch: FeatureId,
    index: usize,
  },
  SketchPoint {
    sketch: FeatureId,
    point: clf_sketch::PointId,
  },
  SketchCurve {
    sketch: FeatureId,
    curve: clf_sketch::CurveId,
  },
  SketchConstraint {
    sketch: FeatureId,
    constraint: clf_sketch::ConstraintId,
  },
  Feature {
    feature: FeatureId,
  },
}

pub struct Session {
  pub design: Design,
  pub model: Model,
  regen: Regenerator,
  undo: Vec<Design>,
  redo: Vec<Design>,
  /// Undo depth when the open transaction began: its edits collapse into one undo step.
  txn: Option<usize>,
  /// Where the design was last opened from / saved to.
  pub path: Option<PathBuf>,
  pub dirty: bool,
  pub selection: Vec<Sel>,
  /// The sketch being edited, if any.
  pub edit_sketch: Option<FeatureId>,
  /// Bumped whenever the model is regenerated (UI caches key off it).
  pub revision: u64,
  /// The data hub directory (`None` = [`store::Store::default_root`]).
  pub store_root: Option<PathBuf>,
  /// The hub item and version the design was opened from or last saved to.
  pub hub: Option<HubLink>,
}

impl Default for Session {
  fn default() -> Self {
    Session::new(Design::new("Untitled"))
  }
}

impl Session {
  pub fn new(design: Design) -> Self {
    let mut s = Session {
      design,
      model: Model::default(),
      regen: Regenerator::default(),
      undo: Vec::new(),
      redo: Vec::new(),
      txn: None,
      path: None,
      dirty: false,
      selection: Vec::new(),
      edit_sketch: None,
      revision: 0,
      store_root: None,
      hub: None,
    };
    s.rebuild();
    s
  }

  /// Opens any readable file: a `.clf`/`.clfz` design, or a STEP, IGES, mesh, DXF ... file as a
  /// new design (see [`files`]).
  pub fn open(path: &std::path::Path) -> Result<Session, CmdError> {
    files::open(path)
  }

  pub fn hub_root(&self) -> PathBuf {
    self.store_root.clone().unwrap_or_else(store::Store::default_root)
  }

  /// Runs a command by name with JSON parameters.
  pub fn execute(&mut self, name: &str, params: &Value) -> CmdResult {
    // Last-resort guard: a panic anywhere in a command restores the design and reports an error.
    let before = self.design.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| commands::run(self, name, params)));
    match result {
      Ok(r) => r,
      Err(_) => {
        self.design = before;
        self.regen.clear();
        self.rebuild();
        Err(CmdError::new(format!("internal error while running '{name}'; the design was restored")))
      }
    }
  }

  /// Records an undo step, applies `f` to the design, and regenerates. If `f` fails, nothing
  /// changes.
  pub fn edit<T>(&mut self, f: impl FnOnce(&mut Design) -> Result<T, CmdError>) -> Result<T, CmdError> {
    let before = self.design.clone();
    match f(&mut self.design) {
      Ok(v) => {
        self.undo.push(before);
        self.trim_undo();
        self.redo.clear();
        self.dirty = true;
        self.rebuild();
        Ok(v)
      }
      Err(e) => {
        self.design = before;
        Err(e)
      }
    }
  }

  fn trim_undo(&mut self) {
    if self.txn.is_none() && self.undo.len() > MAX_UNDO {
      self.undo.drain(..self.undo.len() - MAX_UNDO);
    }
  }

  /// Starts grouping edits: until [`Session::commit`] they become a single undo step, and
  /// [`Session::rollback`] discards them all. Transactions don't nest.
  pub fn begin(&mut self) -> Result<(), CmdError> {
    if self.txn.is_some() {
      return Err(CmdError::new("a transaction is already open"));
    }
    self.txn = Some(self.undo.len());
    Ok(())
  }

  /// Ends the open transaction, keeping its edits as one undo step. Returns whether anything
  /// changed.
  pub fn commit(&mut self) -> Result<bool, CmdError> {
    let start = self.txn.take().ok_or(CmdError::new("no transaction is open"))?;
    let changed = self.undo.len() > start;
    self.undo.truncate(start + 1);
    self.trim_undo();
    Ok(changed)
  }

  /// Ends the open transaction, restoring the design as it was when it began.
  pub fn rollback(&mut self) -> Result<bool, CmdError> {
    let start = self.txn.take().ok_or(CmdError::new("no transaction is open"))?;
    if self.undo.len() <= start {
      return Ok(false);
    }
    self.design = self.undo.swap_remove(start);
    self.undo.truncate(start);
    self.redo.clear();
    self.after_history();
    Ok(true)
  }

  pub fn in_transaction(&self) -> bool {
    self.txn.is_some()
  }

  pub fn undo(&mut self) -> bool {
    // inside a transaction, only its own edits can be undone
    if self.txn.is_some_and(|start| self.undo.len() <= start) {
      return false;
    }
    match self.undo.pop() {
      Some(d) => {
        self.redo.push(std::mem::replace(&mut self.design, d));
        self.after_history();
        true
      }
      None => false,
    }
  }

  pub fn redo(&mut self) -> bool {
    match self.redo.pop() {
      Some(d) => {
        self.undo.push(std::mem::replace(&mut self.design, d));
        self.after_history();
        true
      }
      None => false,
    }
  }

  /// Forgets the redo history (used when a dialog's preview step is withdrawn).
  pub fn drop_redo(&mut self) {
    self.redo.clear();
  }

  pub fn can_undo(&self) -> bool {
    self.undo.len() > self.txn.unwrap_or(0)
  }

  pub fn can_redo(&self) -> bool {
    !self.redo.is_empty()
  }

  fn after_history(&mut self) {
    self.dirty = true;
    if let Some(s) = self.edit_sketch
      && self.design.sketch(s).is_err()
    {
      self.edit_sketch = None;
    }
    self.selection.clear();
    self.rebuild();
  }

  /// Regenerates the model from the design.
  pub fn rebuild(&mut self) {
    let before: std::collections::HashMap<BodyId, FeatureId> =
      self.model.bodies.iter().map(|b| (b.id, b.feature)).collect();
    self.model = self.regen.regenerate(&mut self.design);
    self.revision += 1;
    // Face/edge indices are only meaningful for the shape they were picked on: drop selections
    // on bodies that a feature has changed since.
    let model = &self.model;
    let same = |body: &BodyId| model.body(*body).is_some_and(|b| before.get(body) == Some(&b.feature));
    self.selection.retain(|s| match s {
      Sel::Body { body } => model.body(*body).is_some(),
      Sel::Face { body, .. } | Sel::Edge { body, .. } => same(body),
      _ => true,
    });
  }

  /// Display name of a body (user rename or the generated name).
  pub fn body_name(&self, id: BodyId) -> String {
    self
      .design
      .body_settings(id)
      .and_then(|s| s.name.clone())
      .or_else(|| self.model.body(id).map(|b| b.name.clone()))
      .unwrap_or_else(|| format!("Body {}", id.0))
  }

  pub fn body_visible(&self, id: BodyId) -> bool {
    self.design.body_settings(id).map(|s| s.visible).unwrap_or(true)
  }

  /// Saves to a file (`.clfz` writes an archive, anything else `.clf` JSON).
  pub fn save_to(&mut self, path: &std::path::Path) -> Result<(), CmdError> {
    files::save_design(self, path)?;
    self.path = Some(path.to_path_buf());
    self.dirty = false;
    Ok(())
  }
}

#[cfg(test)]
mod tests;
