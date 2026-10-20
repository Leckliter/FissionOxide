//! Variable-radius fillets with radii at points along the edge.

use serde::{Deserialize, Serialize};

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
