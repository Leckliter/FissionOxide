//! Variable-radius fillets with radii at points along the edge, and fillet selection sets.

use serde::{Deserialize, Serialize};

use crate::EdgeRef;

/// A further set of filleted edges with its own radius (a row of Fusion's Fillet dialog).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilletSet {
  pub edges: Vec<EdgeRef>,
  pub radius: String,
}

/// A radius at a point along a filleted edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadiusPoint {
  /// Where along the edge: 0 at its start, 1 at its end.
  pub at: f64,
  /// The radius there (an expression); zero closes the round to the sharp edge.
  pub radius: String,
}

impl RadiusPoint {
  pub fn new(at: f64, radius: impl Into<String>) -> Self {
    RadiusPoint { at, radius: radius.into() }
  }
}
