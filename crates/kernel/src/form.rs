//! Form (T-spline) geometry (see `cpp/form.cpp`).

use glam::DVec3;

use crate::{KernelError, Result, Shape, sys};

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
