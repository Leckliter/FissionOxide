//! Sub-assemblies: a design inserted from another one keeps its own components (as children of
//! the inserted component) and the joints between them.
//!
//! A rigid sub-assembly is the source's bodies where its joints had put them. A flexible one
//! starts from the source's bodies before any joint moved them and solves its joints again in
//! the host, so its motions can be driven there (`component.drive`). Either way the source's
//! expressions are frozen to plain values, since its parameters don't exist in the host.

use serde::{Deserialize, Serialize};

use crate::{ComponentId, FeatureKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SubAssembly {
  /// The source's components, in its order.
  #[serde(default)]
  pub components: Vec<SubComponent>,
  /// The source's bodies before its joints moved them (same order as the insert's `breps`);
  /// empty when the source has no joints.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub free_breps: Vec<String>,
  /// The source's joint origins and joints, in timeline order, with components, bodies and
  /// joint origins renamed to the host's (joint origins get the ids [`SubAssembly::origin_id`]).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub mates: Vec<SubMate>,
  /// Solve the joints in the host (true) or keep the source's solved positions (false).
  #[serde(default)]
  pub flexible: bool,
  /// The inserted component the sub-assembly hangs below.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub top: Option<ComponentId>,
}

/// One of the source's components and the host component standing in for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubComponent {
  /// Its id in the source.
  pub source: ComponentId,
  /// Its id in the host.
  pub id: ComponentId,
  pub name: String,
}

/// A joint origin or joint of the source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubMate {
  pub name: String,
  pub kind: FeatureKind,
}

impl SubAssembly {
  /// The id a sub-assembly's joint origin (its `k`th mate) has in the host: like inserted
  /// bodies, numbered from the insert feature's id.
  pub fn origin_id(insert: crate::FeatureId, k: usize) -> crate::FeatureId {
    crate::FeatureId(insert.0.saturating_mul(1000).saturating_add(k as u32))
  }

  /// Can it be flexible (does it have joints to solve)?
  pub fn has_joints(&self) -> bool {
    !self.free_breps.is_empty() && self.mates.iter().any(|m| matches!(m.kind, FeatureKind::Joint { .. }))
  }
}
