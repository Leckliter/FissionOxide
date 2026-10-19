//! Configurations: named variants of a design that override parameters, feature suppression,
//! body appearances and joint values (Fusion's configuration table). See
//! `docs/CONFIGURATIONS.md`.
//!
//! A configuration is a sparse row of [`Override`]s; whatever it doesn't override keeps the
//! design's own value. Overrides never change the stored design: regeneration runs on
//! [`effective`], a copy with the active row applied.

use serde::{Deserialize, Serialize};

use crate::assembly::MotionAxis;
use crate::{BodyId, Design, FeatureId, FeatureKind};

/// Configurations per design (hostile-file guard, like the feature cap).
pub const MAX_CONFIGURATIONS: usize = 1000;

/// The configuration table of a design.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigTable {
  /// The active configuration's name; `None` = the design as modelled.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub active: Option<String>,
  #[serde(default)]
  pub list: Vec<Configuration>,
}

/// One row of the table.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Configuration {
  pub name: String,
  #[serde(default)]
  pub overrides: Vec<Override>,
}

/// One cell of a row: a value that replaces the design's own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Override {
  /// A user parameter's expression.
  Parameter { name: String, expr: String },
  /// Whether a feature is suppressed.
  Suppress { feature: FeatureId, suppressed: bool },
  /// A body's appearance colour (`None` = no appearance).
  Appearance {
    body: BodyId,
    #[serde(default)]
    color: Option<[u8; 3]>,
  },
  /// A joint's motion value (an expression: an angle for rotation and pitch, a length for slides).
  Joint {
    joint: FeatureId,
    #[serde(default)]
    motion: MotionAxis,
    value: String,
  },
}

/// A column of the table: what an override changes, without its value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Column {
  Parameter { name: String },
  Suppress { feature: FeatureId },
  Appearance { body: BodyId },
  Joint { joint: FeatureId, motion: MotionAxis },
}

impl Override {
  pub fn column(&self) -> Column {
    match self {
      Override::Parameter { name, .. } => Column::Parameter { name: name.clone() },
      Override::Suppress { feature, .. } => Column::Suppress { feature: *feature },
      Override::Appearance { body, .. } => Column::Appearance { body: *body },
      Override::Joint { joint, motion, .. } => Column::Joint { joint: *joint, motion: *motion },
    }
  }

  /// The cell's value as text (the table dialog's cell contents).
  pub fn value_text(&self) -> String {
    match self {
      Override::Parameter { expr, .. } => expr.clone(),
      Override::Suppress { suppressed, .. } => if *suppressed { "suppressed" } else { "active" }.into(),
      Override::Appearance { color: Some(c), .. } => format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]),
      Override::Appearance { color: None, .. } => "none".into(),
      Override::Joint { value, .. } => value.clone(),
    }
  }
}

impl Column {
  /// A readable heading for the column (`design` names features and bodies).
  pub fn label(&self, design: &Design) -> String {
    let feature = |id: FeatureId| design.feature(id).map(|f| f.name.clone()).unwrap_or(format!("feature {}", id.0));
    match self {
      Column::Parameter { name } => name.clone(),
      Column::Suppress { feature: f } => format!("{} suppressed", feature(*f)),
      Column::Appearance { body } => {
        let name = design.body_settings(*body).and_then(|b| b.name.clone()).unwrap_or(format!("body {}", body.0));
        format!("{name} appearance")
      }
      Column::Joint { joint, motion } => format!("{} {}", feature(*joint), motion.name()),
    }
  }
}

impl ConfigTable {
  pub fn is_empty(&self) -> bool {
    self.list.is_empty() && self.active.is_none()
  }

  pub fn get(&self, name: &str) -> Option<&Configuration> {
    self.list.iter().find(|c| c.name == name)
  }

  pub fn get_mut(&mut self, name: &str) -> Option<&mut Configuration> {
    self.list.iter_mut().find(|c| c.name == name)
  }

  /// The active configuration, if one is set and exists.
  pub fn active(&self) -> Option<&Configuration> {
    self.active.as_deref().and_then(|n| self.get(n))
  }

  /// Every column any row overrides, in a stable order.
  pub fn columns(&self) -> Vec<Column> {
    let mut cols: Vec<Column> = self.list.iter().flat_map(|c| c.overrides.iter().map(Override::column)).collect();
    cols.sort();
    cols.dedup();
    cols
  }

  /// Renames a parameter in every row: its own cells and the expressions that use it.
  pub fn rename_parameter(&mut self, old: &str, new: &str) {
    for o in self.list.iter_mut().flat_map(|c| c.overrides.iter_mut()) {
      match o {
        Override::Parameter { name, expr } => {
          if name == old {
            *name = new.to_string();
          }
          *expr = crate::rename_ident(expr, old, new);
        }
        Override::Joint { value, .. } => *value = crate::rename_ident(value, old, new),
        _ => {}
      }
    }
  }

  /// Does any override expression use the parameter `name`? (Returns the row's name.)
  pub fn uses_parameter(&self, name: &str) -> Option<String> {
    self.list.iter().find_map(|c| {
      c.overrides
        .iter()
        .any(|o| match o {
          Override::Parameter { name: n, expr } => n != name && fox_expr::identifiers(expr).iter().any(|i| i == name),
          Override::Joint { value, .. } => fox_expr::identifiers(value).iter().any(|i| i == name),
          _ => false,
        })
        .then(|| c.name.clone())
    })
  }
}

impl Configuration {
  /// The row's cell in `col`, if it overrides it.
  pub fn cell(&self, col: &Column) -> Option<&Override> {
    self.overrides.iter().find(|o| o.column() == *col)
  }

  /// Sets a cell (replacing the row's override of the same column).
  pub fn set(&mut self, o: Override) {
    let col = o.column();
    match self.overrides.iter_mut().find(|x| x.column() == col) {
      Some(x) => *x = o,
      None => self.overrides.push(o),
    }
  }

  /// Clears a cell back to the default; returns whether it was set.
  pub fn clear(&mut self, col: &Column) -> bool {
    let n = self.overrides.len();
    self.overrides.retain(|o| o.column() != *col);
    self.overrides.len() != n
  }
}

impl Design {
  /// A body's colour as shown: the active configuration's appearance, else the body's own.
  pub fn body_color(&self, body: BodyId) -> Option<[u8; 3]> {
    if let Some(c) = self.configurations.active() {
      for o in &c.overrides {
        if let Override::Appearance { body: b, color } = o
          && *b == body
        {
          return *color;
        }
      }
    }
    self.body_settings(body).and_then(|b| b.color)
  }

  /// The design with configuration `name` active (`None` = none); the design is unchanged.
  pub fn with_configuration(&self, name: Option<&str>) -> Result<Design, String> {
    if let Some(n) = name
      && self.configurations.get(n).is_none()
    {
      return Err(format!("there is no configuration '{n}'"));
    }
    let mut d = self.clone();
    d.configurations.active = name.map(str::to_string);
    Ok(d)
  }
}

/// The design with the active configuration applied, ready to regenerate (no configuration is
/// active in the copy). `None` when nothing is overridden: regenerate the design itself.
pub fn effective(design: &Design) -> Option<Design> {
  let c = design.configurations.active()?;
  if !c.overrides.iter().any(|o| !matches!(o, Override::Appearance { .. })) {
    return None;
  }
  let mut d = design.clone();
  d.configurations.active = None;
  for o in &c.overrides {
    match o {
      Override::Parameter { name, expr } => {
        if let Some(p) = d.parameters.iter_mut().find(|p| p.name == *name) {
          p.expr = expr.clone();
        }
      }
      Override::Suppress { feature, suppressed } => {
        if let Some(f) = d.feature_mut(*feature) {
          f.suppressed = *suppressed;
        }
      }
      Override::Appearance { .. } => {}
      Override::Joint { joint, motion, value } => {
        if let Some(field) = d.feature_mut(*joint).and_then(|f| joint_field(&mut f.kind, *motion)) {
          *field = value.clone();
        }
      }
    }
  }
  Some(d)
}

/// Copies what regenerating `eff` (from [`effective`]) wrote into its features (solved
/// sketches, refreshed references) back into `design`, keeping the joint values the
/// configuration overrode.
pub fn write_back(design: &mut Design, eff: &Design) {
  let overridden: Vec<(FeatureId, MotionAxis)> = design
    .configurations
    .active()
    .map(|c| {
      c.overrides
        .iter()
        .filter_map(|o| match o {
          Override::Joint { joint, motion, .. } => Some((*joint, *motion)),
          _ => None,
        })
        .collect()
    })
    .unwrap_or_default();
  for f in &eff.features {
    let Some(orig) = design.feature_mut(f.id) else { continue };
    let mut kind = f.kind.clone();
    for (joint, motion) in &overridden {
      if *joint == f.id
        && let (Some(dst), Some(src)) = (joint_field(&mut kind, *motion), joint_field(&mut orig.kind, *motion))
      {
        *dst = src.clone();
      }
    }
    orig.kind = kind;
  }
}

/// The expression field of a joint's motion.
pub fn joint_field(kind: &mut FeatureKind, motion: MotionAxis) -> Option<&mut String> {
  match kind {
    FeatureKind::Joint { rotation, slide, slide2, pitch, .. } => Some(match motion {
      MotionAxis::Rotation => rotation,
      MotionAxis::Slide => slide,
      MotionAxis::Slide2 => slide2,
      MotionAxis::Pitch => pitch,
    }),
    _ => None,
  }
}
