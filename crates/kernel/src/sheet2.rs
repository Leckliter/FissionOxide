//! More sheet metal geometry (see `cpp/sheet2.cpp`).

use glam::DVec3;

use crate::{KernelError, Result, Shape, sys};

impl Shape {
  /// A smooth ruled face between two polylines with the same number of points: point i of `a`
  /// is joined to point i of `b`. Both are interpolated at parameters `u` (one per point,
  /// increasing), so faces made from the same points and parameters share their edges.
  pub fn ruled_through(a: &[DVec3], b: &[DVec3], u: &[f64]) -> Result<Shape> {
    if a.len() != b.len() || a.len() < 2 || u.len() != a.len() {
      return Err(KernelError::Input("the profiles need the same number of points".into()));
    }
    let flat = |p: &[DVec3]| p.iter().flat_map(|q| [q.x, q.y, q.z]).collect::<Vec<f64>>();
    let (fa, fb) = (flat(a), flat(b));
    if fa.iter().chain(&fb).chain(u).any(|v| !v.is_finite()) {
      return Err(KernelError::Input("non-finite profile points".into()));
    }
    Shape::wrap(sys::ruled_bspline(&fa, &fb, u)?)
  }

  /// A faceted body from triangles (double precision; see [`Shape::from_mesh`]).
  pub fn from_triangles(positions: &[DVec3], indices: &[u32], merge_coplanar: bool) -> Result<Shape> {
    if indices.len() < 3 || !indices.len().is_multiple_of(3) || indices.iter().any(|&i| i as usize >= positions.len()) {
      return Err(KernelError::Input("bad triangles".into()));
    }
    let pos: Vec<f64> = positions.iter().flat_map(|q| [q.x, q.y, q.z]).collect();
    if pos.iter().any(|v| !v.is_finite()) {
      return Err(KernelError::Input("non-finite mesh coordinates".into()));
    }
    Shape::wrap(sys::mesh_to_shape(&pos, indices, merge_coplanar)?)
  }
}
