//! More sheet metal geometry (see `cpp/sheet2.cpp`).

/// An edge's exact curve as a NURBS (see [`Shape::edge_nurbs`]).
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeNurbs {
  pub degree: usize,
  /// Knots with their multiplicities written out (`poles.len() + degree + 1` values).
  pub knots: Vec<f64>,
  pub poles: Vec<DVec3>,
  /// One weight per pole; empty when the curve isn't rational.
  pub weights: Vec<f64>,
}

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

  /// Edge `edge` (1-based, as in [`Shape::edges`]) as a NURBS over the edge's own range: lines,
  /// arcs, ellipses and splines alike (OCCT's exact conversion).
  pub fn edge_nurbs(&self, edge: u32) -> Result<EdgeNurbs> {
    let v = sys::edge_nurbs(self.get()?, edge)?;
    let bad = || KernelError::Kernel("bad NURBS data".into());
    let (&deg, &rational, &nk) = (v.first().ok_or_else(bad)?, v.get(1).ok_or_else(bad)?, v.get(2).ok_or_else(bad)?);
    let nk = nk as usize;
    let knots = v.get(3..3 + nk).ok_or_else(bad)?.to_vec();
    let np = *v.get(3 + nk).ok_or_else(bad)? as usize;
    let rest = v.get(4 + nk..4 + nk + 4 * np).ok_or_else(bad)?;
    let poles = rest.chunks(4).map(|c| DVec3::new(c[0], c[1], c[2])).collect();
    let weights = if rational > 0.5 { rest.chunks(4).map(|c| c[3]).collect() } else { Vec::new() };
    Ok(EdgeNurbs { degree: deg as usize, knots, poles, weights })
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
