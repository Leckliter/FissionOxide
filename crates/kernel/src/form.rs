//! B-spline faces for form (T-spline / subdivision) bodies and for Insert T-Spline (C++ in
//! `cpp/form.cpp`).

use glam::DVec3;

use crate::ffi::ffi as sys;
use crate::{KernelError, Result, Shape};

/// A non-rational tensor-product B-spline surface: `poles[i][j]` (u index i, v index j) and, per
/// direction, the distinct knot values with their multiplicities.
#[derive(Debug, Clone, PartialEq)]
pub struct BSplineSurface {
  pub degree: u32,
  pub poles: Vec<Vec<DVec3>>,
  pub u_knots: Vec<f64>,
  pub u_mults: Vec<u32>,
  pub v_knots: Vec<f64>,
  pub v_mults: Vec<u32>,
}

impl Shape {
  /// One face spanning the whole of a B-spline surface.
  pub fn bspline_face(s: &BSplineSurface) -> Result<Shape> {
    let nu = s.poles.len();
    let nv = s.poles.first().map_or(0, Vec::len);
    if nu == 0 || nv == 0 || s.poles.iter().any(|row| row.len() != nv) {
      return Err(KernelError::Input("the poles must form a grid".into()));
    }
    let poles: Vec<f64> = s.poles.iter().flatten().flat_map(|p| [p.x, p.y, p.z]).collect();
    if poles.iter().chain(&s.u_knots).chain(&s.v_knots).any(|v| !v.is_finite()) {
      return Err(KernelError::Input("non-finite surface data".into()));
    }
    let (nu, nv) = (
      u32::try_from(nu).map_err(|_| KernelError::Input("too many poles".into()))?,
      u32::try_from(nv).map_err(|_| KernelError::Input("too many poles".into()))?,
    );
    Shape::wrap(sys::bspline_face(s.degree, nu, nv, &poles, &s.u_knots, &s.u_mults, &s.v_knots, &s.v_mults)?)
  }
}

impl Shape {
  /// A face on the bicubic B-spline surface through a grid of `rows × cols` points (row by row)
  /// at uniform parameters. A face's edges depend only on its boundary points, so faces whose
  /// grids share a side sew together exactly.
  pub fn through_grid(points: &[DVec3], rows: usize, cols: usize) -> Result<Shape> {
    if rows < 2 || cols < 2 || points.len() != rows * cols {
      return Err(KernelError::Input("the grid needs at least 2 × 2 points".into()));
    }
    let flat: Vec<f64> = points.iter().flat_map(|p| [p.x, p.y, p.z]).collect();
    if flat.iter().any(|v| !v.is_finite()) {
      return Err(KernelError::Input("non-finite grid points".into()));
    }
    Shape::wrap(sys::grid_face(&flat, rows as u32, cols as u32)?)
  }
}
