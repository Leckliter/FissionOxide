//! The parametric design: user parameters and a timeline of features (sketches, extrudes,
//! revolves, fillets, ...) that regenerate into solid bodies.
//!
//! A [`Design`] is pure data (saved as JSON, `.clf` files). [`regen::Regenerator`] replays it
//! through the kernel into a [`regen::Model`]: bodies, solved sketches, construction planes and a
//! per-feature status, the way Fusion's timeline turns red on a failing feature without stopping
//! the features after it.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

pub mod refs;
pub mod regen;

use serde::{Deserialize, Serialize};

pub use clf_expr::Kind as ParamKind;
pub use clf_kernel as kernel;
pub use clf_sketch as sketch;
pub use refs::{EdgeRef, FaceRef};

use clf_sketch::{CurveId, Sketch};

/// Current file format version.
pub const FORMAT_VERSION: u32 = 1;
/// Hostile-file guards.
pub const MAX_FEATURES: usize = 10_000;
pub const MAX_PARAMETERS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FeatureId(pub u32);

/// Stable body identity: assigned by the feature that created the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BodyId(pub u32);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Design {
  #[serde(default = "format_version")]
  pub version: u32,
  pub name: String,
  #[serde(default)]
  pub parameters: Vec<Parameter>,
  #[serde(default)]
  pub features: Vec<Feature>,
  /// Timeline marker: only the first `marker` features are computed. `None` = all.
  #[serde(default)]
  pub marker: Option<usize>,
  /// Per-body display settings (visibility, colour), by body id.
  #[serde(default)]
  pub bodies: Vec<BodySettings>,
  #[serde(default)]
  next_id: u32,
}

fn format_version() -> u32 {
  FORMAT_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Parameter {
  pub name: String,
  pub expr: String,
  #[serde(default)]
  pub kind: ParamKind,
  #[serde(default)]
  pub comment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BodySettings {
  pub body: BodyId,
  #[serde(default)]
  pub name: Option<String>,
  #[serde(default = "yes")]
  pub visible: bool,
  #[serde(default)]
  pub color: Option<[u8; 3]>,
}

fn yes() -> bool {
  true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Feature {
  pub id: FeatureId,
  pub name: String,
  #[serde(default)]
  pub suppressed: bool,
  pub kind: FeatureKind,
}

/// The world origin planes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginPlane {
  Xy,
  Xz,
  Yz,
}

impl OriginPlane {
  pub fn plane(self) -> kernel::Plane {
    match self {
      OriginPlane::Xy => kernel::Plane::XY,
      OriginPlane::Xz => kernel::Plane::XZ,
      OriginPlane::Yz => kernel::Plane::YZ,
    }
  }
}

/// Where a sketch or construction plane sits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlaneRef {
  Origin {
    plane: OriginPlane,
  },
  /// A construction plane feature.
  Construction {
    feature: FeatureId,
  },
  /// A planar face of a body (follows the face as the model changes).
  Face {
    face: FaceRef,
  },
}

/// An axis for revolves and circular patterns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AxisRef {
  /// World X / Y / Z through the origin.
  X,
  Y,
  Z,
  /// A line in the revolve's own sketch (usually construction).
  SketchLine {
    curve: CurveId,
  },
  /// A straight edge of a body.
  Edge {
    edge: EdgeRef,
  },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
  #[default]
  NewBody,
  Join,
  Cut,
  Intersect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExtentKind {
  /// `distance` along the sketch normal (or against it when flipped).
  #[default]
  OneSide,
  /// `distance` to each side.
  Symmetric,
  /// `distance` along the normal and `distance2` against it.
  TwoSides,
  /// Through every body, in the chosen direction.
  ThroughAll,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Extent {
  #[serde(default)]
  pub kind: ExtentKind,
  #[serde(default = "zero")]
  pub distance: String,
  #[serde(default = "zero")]
  pub distance2: String,
  #[serde(default)]
  pub flip: bool,
}

fn zero() -> String {
  "0".to_string()
}

impl Extent {
  pub fn distance(d: impl Into<String>) -> Self {
    Extent { kind: ExtentKind::OneSide, distance: d.into(), distance2: zero(), flip: false }
  }
}

/// Which bodies a body-level feature acts on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BodySel {
  #[default]
  All,
  Bodies {
    ids: Vec<BodyId>,
  },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Primitive {
  Box { width: String, depth: String, height: String },
  Cylinder { radius: String, height: String },
  Sphere { radius: String },
  Torus { major: String, minor: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FeatureKind {
  Sketch {
    plane: PlaneRef,
    sketch: Sketch,
  },
  Extrude {
    sketch: FeatureId,
    /// Interior points (sketch coordinates) of the chosen regions; empty = every region.
    #[serde(default)]
    profiles: Vec<[f64; 2]>,
    extent: Extent,
    #[serde(default)]
    operation: Operation,
  },
  Revolve {
    sketch: FeatureId,
    #[serde(default)]
    profiles: Vec<[f64; 2]>,
    axis: AxisRef,
    angle: String,
    #[serde(default)]
    operation: Operation,
  },
  Fillet {
    edges: Vec<EdgeRef>,
    radius: String,
  },
  Chamfer {
    edges: Vec<EdgeRef>,
    distance: String,
  },
  Shell {
    /// Faces to open; the rest of each body becomes a wall of `thickness`.
    faces: Vec<FaceRef>,
    thickness: String,
  },
  /// Box / cylinder / sphere / torus placed on a plane at a point (plane coordinates).
  Primitive {
    shape: Primitive,
    plane: PlaneRef,
    #[serde(default)]
    at: [f64; 2],
    #[serde(default)]
    operation: Operation,
  },
  /// Plane offset from another plane.
  ConstructionPlane {
    base: PlaneRef,
    offset: String,
  },
  Mirror {
    #[serde(default)]
    bodies: BodySel,
    plane: PlaneRef,
    /// Join the mirrored copy to the original (default) or keep it as new bodies.
    #[serde(default = "yes")]
    join: bool,
  },
  RectangularPattern {
    #[serde(default)]
    bodies: BodySel,
    /// Direction (world vector).
    direction: [f64; 3],
    count: String,
    spacing: String,
    #[serde(default)]
    direction2: Option<[f64; 3]>,
    #[serde(default = "one")]
    count2: String,
    #[serde(default = "zero")]
    spacing2: String,
    #[serde(default = "yes")]
    join: bool,
  },
  CircularPattern {
    #[serde(default)]
    bodies: BodySel,
    axis: AxisRef,
    count: String,
    /// Total angle the instances span (360° = evenly around).
    #[serde(default = "full_turn")]
    angle: String,
    #[serde(default = "yes")]
    join: bool,
  },
  /// Moves bodies: translation then rotation about an axis.
  Move {
    #[serde(default)]
    bodies: BodySel,
    #[serde(default = "zeros")]
    translate: [String; 3],
    #[serde(default)]
    axis: Option<AxisRef>,
    #[serde(default = "zero")]
    angle: String,
  },
  /// Boolean between bodies: tools are joined into / cut from / intersected with the target.
  Combine {
    target: BodyId,
    tools: Vec<BodyId>,
    operation: Operation,
    #[serde(default)]
    keep_tools: bool,
  },
  /// Bodies imported from a STEP file, stored losslessly as OCCT BRep text.
  Import {
    file_name: String,
    breps: Vec<String>,
  },
  RemoveBodies {
    bodies: Vec<BodyId>,
  },
}

fn one() -> String {
  "1".to_string()
}

fn full_turn() -> String {
  "360 deg".to_string()
}

fn zeros() -> [String; 3] {
  [zero(), zero(), zero()]
}

impl FeatureKind {
  /// Base name for new features of this kind ("Extrude" → "Extrude3").
  pub fn base_name(&self) -> &'static str {
    match self {
      FeatureKind::Sketch { .. } => "Sketch",
      FeatureKind::Extrude { .. } => "Extrude",
      FeatureKind::Revolve { .. } => "Revolve",
      FeatureKind::Fillet { .. } => "Fillet",
      FeatureKind::Chamfer { .. } => "Chamfer",
      FeatureKind::Shell { .. } => "Shell",
      FeatureKind::Primitive { shape: Primitive::Box { .. }, .. } => "Box",
      FeatureKind::Primitive { shape: Primitive::Cylinder { .. }, .. } => "Cylinder",
      FeatureKind::Primitive { shape: Primitive::Sphere { .. }, .. } => "Sphere",
      FeatureKind::Primitive { shape: Primitive::Torus { .. }, .. } => "Torus",
      FeatureKind::ConstructionPlane { .. } => "Plane",
      FeatureKind::Mirror { .. } => "Mirror",
      FeatureKind::RectangularPattern { .. } => "RectangularPattern",
      FeatureKind::CircularPattern { .. } => "CircularPattern",
      FeatureKind::Move { .. } => "Move",
      FeatureKind::Combine { .. } => "Combine",
      FeatureKind::Import { .. } => "Import",
      FeatureKind::RemoveBodies { .. } => "Remove",
    }
  }

  /// Features this one depends on directly (sketches, construction planes).
  pub fn depends_on(&self) -> Vec<FeatureId> {
    let plane_dep = |p: &PlaneRef| match p {
      PlaneRef::Construction { feature } => vec![*feature],
      _ => vec![],
    };
    match self {
      FeatureKind::Sketch { plane, .. } => plane_dep(plane),
      FeatureKind::Extrude { sketch, .. } | FeatureKind::Revolve { sketch, .. } => vec![*sketch],
      FeatureKind::Primitive { plane, .. } | FeatureKind::Mirror { plane, .. } => plane_dep(plane),
      FeatureKind::ConstructionPlane { base, .. } => plane_dep(base),
      _ => vec![],
    }
  }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DesignError {
  #[error("no feature {0:?}")]
  NoFeature(FeatureId),
  #[error("feature {0:?} is not a sketch")]
  NotASketch(FeatureId),
  #[error("invalid parameter name '{0}'")]
  BadParameterName(String),
  #[error("a parameter named '{0}' already exists")]
  DuplicateParameter(String),
  #[error("no parameter '{0}'")]
  NoParameter(String),
  #[error("'{0}' is used by {1}")]
  InUse(String, String),
  #[error("too many features")]
  TooMany,
  #[error("unsupported file version {0}")]
  Version(u32),
  #[error("file error: {0}")]
  File(String),
}

impl Default for Design {
  fn default() -> Self {
    Self::new("Untitled")
  }
}

impl Design {
  pub fn new(name: impl Into<String>) -> Self {
    Design {
      version: FORMAT_VERSION,
      name: name.into(),
      parameters: Vec::new(),
      features: Vec::new(),
      marker: None,
      bodies: Vec::new(),
      next_id: 1,
    }
  }

  /// Number of features the timeline computes (the marker position).
  pub fn active_len(&self) -> usize {
    self.marker.map(|m| m.min(self.features.len())).unwrap_or(self.features.len())
  }

  pub fn feature(&self, id: FeatureId) -> Option<&Feature> {
    self.features.iter().find(|f| f.id == id)
  }

  pub fn feature_mut(&mut self, id: FeatureId) -> Option<&mut Feature> {
    self.features.iter_mut().find(|f| f.id == id)
  }

  pub fn index_of(&self, id: FeatureId) -> Option<usize> {
    self.features.iter().position(|f| f.id == id)
  }

  pub fn sketch(&self, id: FeatureId) -> Result<&Sketch, DesignError> {
    match &self.feature(id).ok_or(DesignError::NoFeature(id))?.kind {
      FeatureKind::Sketch { sketch, .. } => Ok(sketch),
      _ => Err(DesignError::NotASketch(id)),
    }
  }

  pub fn sketch_mut(&mut self, id: FeatureId) -> Result<&mut Sketch, DesignError> {
    match &mut self.feature_mut(id).ok_or(DesignError::NoFeature(id))?.kind {
      FeatureKind::Sketch { sketch, .. } => Ok(sketch),
      _ => Err(DesignError::NotASketch(id)),
    }
  }

  fn alloc(&mut self) -> u32 {
    let used = self.features.iter().map(|f| f.id.0).max().unwrap_or(0);
    self.next_id = self.next_id.max(used + 1);
    let id = self.next_id;
    self.next_id = self.next_id.saturating_add(1);
    id
  }

  /// Inserts a feature at the timeline marker (the end unless rolled back). Returns its id.
  pub fn add_feature(&mut self, kind: FeatureKind) -> Result<FeatureId, DesignError> {
    if self.features.len() >= MAX_FEATURES {
      return Err(DesignError::TooMany);
    }
    let id = FeatureId(self.alloc());
    let name = self.unique_name(kind.base_name());
    let at = self.active_len();
    self.features.insert(at, Feature { id, name, suppressed: false, kind });
    if let Some(m) = self.marker.as_mut() {
      *m += 1;
    }
    Ok(id)
  }

  fn unique_name(&self, base: &str) -> String {
    let n = self.features.iter().filter(|f| f.kind.base_name() == base).count() + 1;
    let mut k = n;
    loop {
      let name = format!("{base}{k}");
      if !self.features.iter().any(|f| f.name == name) {
        return name;
      }
      k += 1;
    }
  }

  /// Deletes a feature. Features that depend on it directly are deleted too (like Fusion's
  /// "delete dependents"); returns everything removed.
  pub fn remove_feature(&mut self, id: FeatureId) -> Result<Vec<FeatureId>, DesignError> {
    if self.feature(id).is_none() {
      return Err(DesignError::NoFeature(id));
    }
    let mut doomed = vec![id];
    loop {
      let more: Vec<FeatureId> = self
        .features
        .iter()
        .filter(|f| !doomed.contains(&f.id) && f.kind.depends_on().iter().any(|d| doomed.contains(d)))
        .map(|f| f.id)
        .collect();
      if more.is_empty() {
        break;
      }
      doomed.extend(more);
    }
    let before_marker = self.features.iter().take(self.active_len()).filter(|f| doomed.contains(&f.id)).count();
    self.features.retain(|f| !doomed.contains(&f.id));
    if let Some(m) = self.marker.as_mut() {
      *m = m.saturating_sub(before_marker);
    }
    Ok(doomed)
  }

  /// Moves a feature to a new timeline index, refusing moves that would put it before a feature
  /// it depends on (or after one that depends on it).
  pub fn move_feature(&mut self, id: FeatureId, to: usize) -> Result<(), DesignError> {
    let from = self.index_of(id).ok_or(DesignError::NoFeature(id))?;
    let to = to.min(self.features.len().saturating_sub(1));
    let f = self.features.remove(from);
    self.features.insert(to, f);
    // validate ordering
    let bad = self.features.iter().enumerate().find_map(|(i, feat)| {
      feat.kind.depends_on().iter().any(|d| self.index_of(*d).is_some_and(|di| di > i)).then(|| feat.name.clone())
    });
    if let Some(name) = bad {
      let f = self.features.remove(to);
      self.features.insert(from, f);
      return Err(DesignError::InUse(name, "a feature it would come before".into()));
    }
    Ok(())
  }

  pub fn set_marker(&mut self, at: Option<usize>) {
    self.marker = at.map(|m| m.min(self.features.len())).filter(|m| *m < self.features.len());
  }

  // ---- parameters ----

  pub fn parameter(&self, name: &str) -> Option<&Parameter> {
    self.parameters.iter().find(|p| p.name == name)
  }

  pub fn add_parameter(&mut self, name: &str, expr: &str, kind: ParamKind, comment: &str) -> Result<(), DesignError> {
    if !clf_expr::is_valid_parameter_name(name) {
      return Err(DesignError::BadParameterName(name.to_string()));
    }
    if self.parameter(name).is_some() {
      return Err(DesignError::DuplicateParameter(name.to_string()));
    }
    if self.parameters.len() >= MAX_PARAMETERS {
      return Err(DesignError::TooMany);
    }
    self.parameters.push(Parameter {
      name: name.to_string(),
      expr: expr.to_string(),
      kind,
      comment: comment.to_string(),
    });
    Ok(())
  }

  pub fn set_parameter(&mut self, name: &str, expr: &str) -> Result<(), DesignError> {
    let p = self.parameters.iter_mut().find(|p| p.name == name).ok_or(DesignError::NoParameter(name.to_string()))?;
    p.expr = expr.to_string();
    Ok(())
  }

  /// Removes a parameter unless an expression still refers to it.
  pub fn remove_parameter(&mut self, name: &str) -> Result<(), DesignError> {
    if self.parameter(name).is_none() {
      return Err(DesignError::NoParameter(name.to_string()));
    }
    for p in &self.parameters {
      if p.name != name && clf_expr::identifiers(&p.expr).iter().any(|i| i == name) {
        return Err(DesignError::InUse(name.to_string(), format!("parameter {}", p.name)));
      }
    }
    for f in &self.features {
      if expr_strings(&f.kind).iter().any(|e| clf_expr::identifiers(e).iter().any(|i| i == name)) {
        return Err(DesignError::InUse(name.to_string(), f.name.clone()));
      }
    }
    self.parameters.retain(|p| p.name != name);
    Ok(())
  }

  /// Renames a parameter and rewrites every expression that uses it.
  pub fn rename_parameter(&mut self, old: &str, new: &str) -> Result<(), DesignError> {
    if !clf_expr::is_valid_parameter_name(new) {
      return Err(DesignError::BadParameterName(new.to_string()));
    }
    if self.parameter(new).is_some() {
      return Err(DesignError::DuplicateParameter(new.to_string()));
    }
    let p = self.parameters.iter_mut().find(|p| p.name == old).ok_or(DesignError::NoParameter(old.to_string()))?;
    p.name = new.to_string();
    for p in &mut self.parameters {
      p.expr = rename_ident(&p.expr, old, new);
    }
    for f in &mut self.features {
      for_each_expr(&mut f.kind, &mut |e| *e = rename_ident(e, old, new));
    }
    Ok(())
  }

  // ---- bodies ----

  pub fn body_settings(&self, id: BodyId) -> Option<&BodySettings> {
    self.bodies.iter().find(|b| b.body == id)
  }

  pub fn body_settings_mut(&mut self, id: BodyId) -> &mut BodySettings {
    if let Some(i) = self.bodies.iter().position(|b| b.body == id) {
      &mut self.bodies[i]
    } else {
      self.bodies.push(BodySettings { body: id, name: None, visible: true, color: None });
      let last = self.bodies.len() - 1;
      &mut self.bodies[last]
    }
  }

  // ---- files ----

  pub fn to_json(&self) -> String {
    serde_json::to_string_pretty(self).unwrap_or_default()
  }

  pub fn from_json(s: &str) -> Result<Design, DesignError> {
    let d: Design = serde_json::from_str(s).map_err(|e| DesignError::File(e.to_string()))?;
    if d.version > FORMAT_VERSION {
      return Err(DesignError::Version(d.version));
    }
    if d.features.len() > MAX_FEATURES || d.parameters.len() > MAX_PARAMETERS {
      return Err(DesignError::TooMany);
    }
    Ok(d)
  }

  pub fn save(&self, path: &std::path::Path) -> Result<(), DesignError> {
    std::fs::write(path, self.to_json()).map_err(|e| DesignError::File(e.to_string()))
  }

  pub fn load(path: &std::path::Path) -> Result<Design, DesignError> {
    let meta = std::fs::metadata(path).map_err(|e| DesignError::File(e.to_string()))?;
    if meta.len() > 512 * 1024 * 1024 {
      return Err(DesignError::File("file is too large".into()));
    }
    let s = std::fs::read_to_string(path).map_err(|e| DesignError::File(e.to_string()))?;
    Design::from_json(&s)
  }
}

/// Applies `f` to every expression string inside a feature.
pub fn for_each_expr(kind: &mut FeatureKind, f: &mut dyn FnMut(&mut String)) {
  match kind {
    FeatureKind::Sketch { sketch, .. } => {
      for c in &mut sketch.constraints {
        if let Some(d) = c.kind.dim_mut() {
          f(&mut d.expr);
        }
      }
    }
    FeatureKind::Extrude { extent, .. } => {
      f(&mut extent.distance);
      f(&mut extent.distance2);
    }
    FeatureKind::Revolve { angle, .. } => f(angle),
    FeatureKind::Fillet { radius, .. } => f(radius),
    FeatureKind::Chamfer { distance, .. } => f(distance),
    FeatureKind::Shell { thickness, .. } => f(thickness),
    FeatureKind::Primitive { shape, .. } => match shape {
      Primitive::Box { width, depth, height } => {
        f(width);
        f(depth);
        f(height);
      }
      Primitive::Cylinder { radius, height } => {
        f(radius);
        f(height);
      }
      Primitive::Sphere { radius } => f(radius),
      Primitive::Torus { major, minor } => {
        f(major);
        f(minor);
      }
    },
    FeatureKind::ConstructionPlane { offset, .. } => f(offset),
    FeatureKind::RectangularPattern { count, spacing, count2, spacing2, .. } => {
      f(count);
      f(spacing);
      f(count2);
      f(spacing2);
    }
    FeatureKind::CircularPattern { count, angle, .. } => {
      f(count);
      f(angle);
    }
    FeatureKind::Move { translate, angle, .. } => {
      for t in translate.iter_mut() {
        f(t);
      }
      f(angle);
    }
    FeatureKind::Mirror { .. }
    | FeatureKind::Combine { .. }
    | FeatureKind::Import { .. }
    | FeatureKind::RemoveBodies { .. } => {}
  }
}

/// All expression strings in a feature (used for "is this parameter used?").
pub fn expr_strings(kind: &FeatureKind) -> Vec<String> {
  let mut v = kind.clone();
  let mut out = Vec::new();
  for_each_expr(&mut v, &mut |e| out.push(e.clone()));
  out
}

/// Replaces whole-word identifier `old` with `new` in an expression.
fn rename_ident(expr: &str, old: &str, new: &str) -> String {
  let mut out = String::with_capacity(expr.len());
  let chars: Vec<char> = expr.chars().collect();
  let mut i = 0;
  while i < chars.len() {
    let c = chars[i];
    if c.is_alphabetic() || c == '_' {
      let start = i;
      while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
        i += 1;
      }
      let word: String = chars[start..i].iter().collect();
      out.push_str(if word == old { new } else { &word });
    } else {
      out.push(c);
      i += 1;
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn add_remove_features_with_dependents() {
    let mut d = Design::new("t");
    let s = d
      .add_feature(FeatureKind::Sketch { plane: PlaneRef::Origin { plane: OriginPlane::Xy }, sketch: Sketch::new() })
      .unwrap();
    let e = d
      .add_feature(FeatureKind::Extrude {
        sketch: s,
        profiles: vec![],
        extent: Extent::distance("10"),
        operation: Operation::NewBody,
      })
      .unwrap();
    assert_eq!(d.feature(s).unwrap().name, "Sketch1");
    assert_eq!(d.feature(e).unwrap().name, "Extrude1");
    assert!(d.move_feature(e, 0).is_err(), "extrude can't move before its sketch");
    let removed = d.remove_feature(s).unwrap();
    assert_eq!(removed.len(), 2);
    assert!(d.features.is_empty());
  }

  #[test]
  fn marker_inserts_before_rolled_back_features() {
    let mut d = Design::new("t");
    let a = d.add_feature(FeatureKind::RemoveBodies { bodies: vec![] }).unwrap();
    let b = d.add_feature(FeatureKind::RemoveBodies { bodies: vec![] }).unwrap();
    d.set_marker(Some(1));
    let c = d.add_feature(FeatureKind::RemoveBodies { bodies: vec![] }).unwrap();
    assert_eq!(d.features.iter().map(|f| f.id).collect::<Vec<_>>(), vec![a, c, b]);
    assert_eq!(d.active_len(), 2);
    d.remove_feature(a).unwrap();
    assert_eq!(d.active_len(), 1);
  }

  #[test]
  fn parameters_rename_and_in_use() {
    let mut d = Design::new("t");
    d.add_parameter("width", "40 mm", ParamKind::Length, "").unwrap();
    d.add_parameter("half", "width / 2", ParamKind::Length, "").unwrap();
    assert!(d.add_parameter("width", "1", ParamKind::Length, "").is_err());
    assert!(d.add_parameter("mm", "1", ParamKind::Length, "").is_err());
    let s = d
      .add_feature(FeatureKind::Sketch { plane: PlaneRef::Origin { plane: OriginPlane::Xy }, sketch: Sketch::new() })
      .unwrap();
    d.add_feature(FeatureKind::Extrude {
      sketch: s,
      profiles: vec![],
      extent: Extent::distance("width * 2"),
      operation: Operation::NewBody,
    })
    .unwrap();
    assert!(matches!(d.remove_parameter("width"), Err(DesignError::InUse(..))));
    d.rename_parameter("width", "w").unwrap();
    assert_eq!(d.parameter("half").unwrap().expr, "w / 2");
    let FeatureKind::Extrude { extent, .. } = &d.features[1].kind else { panic!() };
    assert_eq!(extent.distance, "w * 2");
    d.remove_parameter("half").unwrap();
  }

  #[test]
  fn json_round_trip_and_hostile_files() {
    let mut d = Design::new("Bracket");
    d.add_parameter("t", "3", ParamKind::Length, "thickness").unwrap();
    d.add_feature(FeatureKind::Sketch { plane: PlaneRef::Origin { plane: OriginPlane::Xz }, sketch: Sketch::new() })
      .unwrap();
    let back = Design::from_json(&d.to_json()).unwrap();
    assert_eq!(back, d);
    assert!(Design::from_json("{").is_err());
    assert!(Design::from_json("{\"version\": 99, \"name\": \"x\"}").is_err());
    assert!(Design::from_json("[]").is_err());
  }

  #[test]
  fn rename_ident_whole_words_only() {
    assert_eq!(rename_ident("w + width*w2 - w", "w", "x"), "x + width*w2 - x");
  }
}
