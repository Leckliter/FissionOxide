//! Bounding Solid: the shape a bounding solid takes and the smallest circle around points, which
//! sizes the cylinder (see `FeatureKind::BoundingSolid`).

use serde::{Deserialize, Serialize};

/// What a bounding solid is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BoundShape {
  /// The bodies' bounding box in the frame.
  #[default]
  Box,
  /// The smallest cylinder about the frame's Z axis direction that holds the bodies.
  Cylinder,
}

/// The smallest circle holding every point: (centre, radius). Welzl's algorithm, iterative, on
/// the points in a fixed shuffled order (its expected linear time needs one).
pub fn enclosing_circle(points: &[[f64; 2]]) -> Option<([f64; 2], f64)> {
  let mut points = points.to_vec();
  let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
  for i in (1..points.len()).rev() {
    seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
    points.swap(i, ((seed >> 33) % (i as u64 + 1)) as usize);
  }
  let points = points.as_slice();
  let (first, rest) = points.split_first()?;
  let mut c = (*first, 0.0);
  let eps = 1e-9 * points.iter().map(|p| p[0].abs().max(p[1].abs())).fold(1.0, f64::max);
  let inside = |c: &([f64; 2], f64), p: &[f64; 2]| dist(c.0, *p) <= c.1 + eps;
  for (i, p) in rest.iter().enumerate() {
    if inside(&c, p) {
      continue;
    }
    c = (*p, 0.0);
    for (j, q) in points[..=i].iter().enumerate() {
      if inside(&c, q) {
        continue;
      }
      c = (mid(*p, *q), dist(*p, *q) / 2.0);
      for r in &points[..j] {
        if !inside(&c, r) {
          c = circumcircle(*p, *q, *r).unwrap_or(c);
        }
      }
    }
  }
  Some(c)
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
  (a[0] - b[0]).hypot(a[1] - b[1])
}

fn mid(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
  [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

/// The circle through three points; for (near) collinear ones, the one on the furthest pair.
fn circumcircle(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> Option<([f64; 2], f64)> {
  let (bx, by, cx, cy) = (b[0] - a[0], b[1] - a[1], c[0] - a[0], c[1] - a[1]);
  let d = 2.0 * (bx * cy - by * cx);
  let scale = (bx * bx + by * by).max(cx * cx + cy * cy);
  if d.abs() <= 1e-12 * scale {
    let pairs = [(a, b), (a, c), (b, c)];
    let (p, q) = pairs.into_iter().max_by(|x, y| dist(x.0, x.1).total_cmp(&dist(y.0, y.1)))?;
    return Some((mid(p, q), dist(p, q) / 2.0));
  }
  let (b2, c2) = (bx * bx + by * by, cx * cx + cy * cy);
  let ux = (cy * b2 - by * c2) / d;
  let uy = (bx * c2 - cx * b2) / d;
  Some(([a[0] + ux, a[1] + uy], ux.hypot(uy)))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn circle_of_a_square_and_a_triangle() {
    let (c, r) = enclosing_circle(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0], [1.0, 1.0]]).unwrap();
    assert!((c[0] - 1.0).abs() < 1e-12 && (c[1] - 1.0).abs() < 1e-12);
    assert!((r - 2f64.sqrt()).abs() < 1e-12);
    // an obtuse triangle: the circle on its longest side
    let (c, r) = enclosing_circle(&[[0.0, 0.0], [10.0, 0.0], [5.0, 1.0]]).unwrap();
    assert!((c[0] - 5.0).abs() < 1e-12 && c[1].abs() < 1e-12 && (r - 5.0).abs() < 1e-12);
    assert!(enclosing_circle(&[]).is_none());
  }
}
