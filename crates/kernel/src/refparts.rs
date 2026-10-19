//! Fillets whose radius follows a law along each edge (Fusion's variable-radius fillet with
//! radii at points along the edge), and merging hole walls that a tangent neighbour split in two.
//! The C++ side is `cpp/refparts.cpp`.

use crate::ffi::ffi as sys;
use crate::{KernelError, Result, Shape};

impl Shape {
  /// Rounds edges (1-based) with a radius that passes through `(at, radius)` points: `at` is the
  /// fraction along each edge (0 at its start, 1 at its end) and the radius runs smoothly between
  /// the points, holding the end values beyond the first and last. A radius may be zero (the round
  /// closes to the sharp edge there), but not every one.
  pub fn fillet_law(&self, edges: &[u32], points: &[(f64, f64)]) -> Result<Shape> {
    if points.len() < 2 {
      return Err(KernelError::Input("a variable fillet needs two or more radius points".into()));
    }
    if points.iter().any(|(t, r)| !t.is_finite() || !r.is_finite() || *r < 0.0) {
      return Err(KernelError::Input("fillet radii must be zero or more".into()));
    }
    if !points.iter().any(|(_, r)| *r > 1e-9) {
      return Err(KernelError::Input("fillet radii can't all be zero".into()));
    }
    let (at, radii): (Vec<f64>, Vec<f64>) = points.iter().copied().unzip();
    Shape::wrap(sys::fillet_law(self.get()?, edges, &at, &radii)?)
  }

  /// Merges faces that lie on one surface across edges that more than two faces share (where two
  /// holes are exactly tangent, the boolean splits each wall at the line they touch along). A
  /// shape without such edges, or that the merge would leave invalid, comes back unchanged.
  pub fn unify_internal(&self) -> Result<Shape> {
    Shape::wrap(sys::unify_faces_internal(self.get()?)?)
  }
}
