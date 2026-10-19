//! Base features: non-parametric geometry and direct edits in the timeline (Fusion's Base
//! Feature and "Do not capture design history" mode). See `docs/CONFIGURATIONS.md`.

use serde::{Deserialize, Serialize};

use crate::{BodyId, Design, FaceRef, FeatureId, FeatureKind};

/// A body frozen into a base feature, keeping its id and name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseBody {
  pub id: BodyId,
  #[serde(default)]
  pub name: String,
  /// OCCT BRep text.
  pub brep: String,
}

/// A direct edit inside a base feature. Values are millimetres, not expressions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DirectEdit {
  /// Moves flat faces by a vector (the part along each face's normal).
  MoveFaces { faces: Vec<FaceRef>, translate: [f64; 3] },
  /// Pushes (−) or pulls (+) faces along their normals; curved faces are offset.
  PressPull { faces: Vec<FaceRef>, distance: f64 },
  /// Deletes faces and heals the body around them.
  DeleteFaces { faces: Vec<FaceRef> },
}

impl DirectEdit {
  pub fn label(&self) -> &'static str {
    match self {
      DirectEdit::MoveFaces { .. } => "Move Face",
      DirectEdit::PressPull { .. } => "Press Pull",
      DirectEdit::DeleteFaces { .. } => "Delete Face",
    }
  }
}

impl Design {
  /// The open base feature: the one just before the timeline marker, if that is a base feature.
  pub fn open_base_feature(&self) -> Option<FeatureId> {
    let i = self.active_len().checked_sub(1)?;
    let f = self.features.get(i)?;
    matches!(f.kind, FeatureKind::BaseFeature { .. }).then_some(f.id)
  }

  /// Appends a direct edit to the open base feature, adding one at the marker if none is open.
  /// Returns the base feature's id.
  pub fn add_direct_edit(&mut self, edit: DirectEdit) -> Result<FeatureId, crate::DesignError> {
    let id = match self.open_base_feature() {
      Some(id) => id,
      None => self.add_feature(FeatureKind::BaseFeature { bodies: Vec::new(), edits: Vec::new() })?,
    };
    if let Some(FeatureKind::BaseFeature { edits, .. }) = self.feature_mut(id).map(|f| &mut f.kind) {
      edits.push(edit);
    }
    Ok(id)
  }

  /// Collapses the timeline into one base feature holding `bodies` (the bodies as regenerated
  /// now), as turning design history off does. Components, body settings, drawings and the
  /// rest of the design are kept; the features (sketches, joints, ...) are removed.
  pub fn collapse_history(&mut self, bodies: Vec<BaseBody>) -> FeatureId {
    // features are renumbered from here on, so configuration rows naming them go too
    let removed: Vec<FeatureId> = self.features.iter().map(|f| f.id).collect();
    for c in &mut self.configurations.list {
      c.overrides.retain(|o| match o {
        crate::config::Override::Suppress { feature, .. } => !removed.contains(feature),
        crate::config::Override::Joint { joint, .. } => !removed.contains(joint),
        _ => true,
      });
    }
    self.attributes.retain(|a| a.target.feature().is_none());
    // keep ids growing past the removed features (their body ids stay in use)
    self.alloc();
    self.features.clear();
    self.marker = None;
    match self.add_feature(FeatureKind::BaseFeature { bodies, edits: Vec::new() }) {
      Ok(id) => id,
      Err(_) => FeatureId(0),
    }
  }
}
