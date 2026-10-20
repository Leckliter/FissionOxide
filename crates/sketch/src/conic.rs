//! Conic curves: the rational quadratic Bézier from a start point to an end point, pulled
//! towards an apex (where the end tangents meet) by its shape value rho, as Fusion's conic curve.
//! Rho below 0.5 makes an elliptical arc, 0.5 a parabola and above 0.5 a hyperbola.

/// Lowest and highest rho a conic accepts.
pub const RHO_RANGE: std::ops::RangeInclusive<f64> = 0.01..=0.99;

/// The apex weight of the rational quadratic Bézier with this rho (the end weights are 1): the
/// curve's middle point lies `rho` of the way from the chord's middle to the apex.
pub fn conic_weight(rho: f64) -> f64 {
  rho / (1.0 - rho)
}

/// The point at parameter `t` (0 to 1) of the rational quadratic Bézier `p` with apex weight `w`.
pub fn conic_at(p: &[[f64; 2]; 3], w: f64, t: f64) -> [f64; 2] {
  let (b0, b1, b2) = ((1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t) * w, t * t);
  let d = b0 + b1 + b2;
  [(b0 * p[0][0] + b1 * p[1][0] + b2 * p[2][0]) / d, (b0 * p[0][1] + b1 * p[1][1] + b2 * p[2][1]) / d]
}

/// The two halves of a conic split at its middle, each again with end weights 1.
fn halves(p: &[[f64; 2]; 3], w: f64) -> ([[f64; 2]; 3], [[f64; 2]; 3], f64) {
  let mix = |a: [f64; 2], b: [f64; 2], wa: f64, wb: f64| {
    [(wa * a[0] + wb * b[0]) / (wa + wb), (wa * a[1] + wb * b[1]) / (wa + wb)]
  };
  let m = conic_at(p, w, 0.5);
  let (l, r) = (mix(p[0], p[1], 1.0, w), mix(p[1], p[2], w, 1.0));
  ([p[0], l, m], [m, r, p[2]], ((1.0 + w) / 2.0).sqrt())
}

/// A conic as cubic Bézier pieces: split exactly into eight conic pieces, each close to a
/// parabola, then each as the cubic with matching end tangents and middle point.
pub fn conic_beziers(p: &[[f64; 2]; 3], w: f64) -> Vec<[[f64; 2]; 4]> {
  if !(w > 0.0) || !w.is_finite() {
    return Vec::new();
  }
  let mut pieces = vec![(*p, w)];
  for _ in 0..3 {
    pieces = pieces
      .into_iter()
      .flat_map(|(q, w)| {
        let (a, b, w2) = halves(&q, w);
        [(a, w2), (b, w2)]
      })
      .collect();
  }
  pieces
    .into_iter()
    .map(|(q, w)| {
      let k = 4.0 * w / (3.0 * (1.0 + w));
      let lerp = |a: [f64; 2], b: [f64; 2]| [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k];
      [q[0], lerp(q[0], q[1]), lerp(q[2], q[1]), q[2]]
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn middle_point_sits_rho_towards_the_apex() {
    let p = [[0.0, 0.0], [20.0, 20.0], [40.0, 0.0]];
    for rho in [0.1, 0.3, 0.5, 0.8, 0.95] {
      let m = conic_at(&p, conic_weight(rho), 0.5);
      assert!((m[0] - 20.0).abs() < 1e-12 && (m[1] - 20.0 * rho).abs() < 1e-12, "{rho}: {m:?}");
    }
  }

  #[test]
  fn cubic_pieces_follow_the_conic() {
    let p = [[0.0, 0.0], [10.0, 25.0], [40.0, 0.0]];
    for rho in [0.05, 0.3, 0.5, 0.8, 0.97] {
      let w = conic_weight(rho);
      let bz = conic_beziers(&p, w);
      assert_eq!(bz.len(), 8);
      assert_eq!(bz[0][0], p[0]);
      assert_eq!(bz[7][3], p[2]);
      // every cubic point lies on the conic's implicit curve: check against dense samples
      let dense: Vec<[f64; 2]> = (0..=4000).map(|i| conic_at(&p, w, i as f64 / 4000.0)).collect();
      for b in &bz {
        for k in 0..=10 {
          let q = crate::bezier_at(b, k as f64 / 10.0);
          let d = dense.iter().map(|s| (s[0] - q[0]).hypot(s[1] - q[1])).fold(f64::INFINITY, f64::min);
          assert!(d < 0.05, "{rho}: {q:?} is {d} off");
        }
      }
    }
  }
}
