//! NURBS curves in the plane (DXF SPLINE entities) and their conversion to cubic Bézier pieces.

/// A planar NURBS curve: what a DXF SPLINE stores.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spline {
  pub degree: usize,
  /// `control.len() + degree + 1` non-decreasing values.
  pub knots: Vec<f64>,
  pub control: Vec<[f64; 2]>,
  /// One weight per control point; empty when the curve isn't rational.
  pub weights: Vec<f64>,
  /// The points the curve was fitted through, if the writer kept them.
  pub fit: Vec<[f64; 2]>,
}

type Bez = [[f64; 2]; 4];

fn lerp(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
  [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

impl Spline {
  /// A clamped B-spline with uniform interior knots (0, …, 0, 1, 2, …, m, …, m).
  pub fn clamped_uniform(degree: usize, control: Vec<[f64; 2]>) -> Spline {
    let spans = control.len().saturating_sub(degree).max(1);
    let knots = std::iter::repeat_n(0.0, degree + 1)
      .chain((1..spans).map(|k| k as f64))
      .chain(std::iter::repeat_n(spans as f64, degree + 1))
      .collect();
    Spline { degree, knots, control, weights: Vec::new(), fit: Vec::new() }
  }

  /// A chain of cubic Bézier pieces as one cubic B-spline (each joint a triple knot), so it
  /// reads back as exactly the same pieces.
  pub fn from_beziers(pieces: &[Bez]) -> Spline {
    let mut control = Vec::with_capacity(pieces.len() * 3 + 1);
    for (i, p) in pieces.iter().enumerate() {
      if i == 0 {
        control.push(p[0]);
      }
      control.extend_from_slice(&p[1..]);
    }
    let n = pieces.len();
    let mut knots = vec![0.0; 4];
    for k in 1..n {
      knots.extend([k as f64; 3]);
    }
    knots.extend([n as f64; 4]);
    Spline { degree: 3, knots, control, weights: Vec::new(), fit: Vec::new() }
  }

  /// Rational with weights that actually differ.
  pub fn is_rational(&self) -> bool {
    self.weights.len() == self.control.len()
      && self.weights.iter().any(|w| (w - self.weights[0]).abs() > 1e-12 * self.weights[0].abs().max(1.0))
  }

  /// Degree, knot and control counts that make a curve.
  pub fn is_valid(&self) -> bool {
    let p = self.degree;
    (1..=25).contains(&p)
      && self.control.len() > p
      && self.knots.len() == self.control.len() + p + 1
      && self.knots.windows(2).all(|w| w[0] <= w[1])
      && self.knots[p] < self.knots[self.control.len()]
      && self.control.iter().flatten().chain(&self.knots).all(|v| v.is_finite())
      && (self.weights.is_empty()
        || (self.weights.len() == self.control.len() && self.weights.iter().all(|w| *w > 0.0)))
  }

  /// The first and last `degree + 1` knots are equal: the curve starts and ends on its end
  /// control points.
  pub fn is_clamped(&self) -> bool {
    let (p, k) = (self.degree, &self.knots);
    k.len() > 2 * p + 1
      && k[..=p].iter().all(|v| (v - k[0]).abs() < 1e-12)
      && k[k.len() - p - 1..].iter().all(|v| (v - k[k.len() - 1]).abs() < 1e-12)
  }

  /// Clamped with evenly spaced simple interior knots: the sketch's control-point spline.
  pub fn is_uniform(&self) -> bool {
    if !self.is_clamped() {
      return false;
    }
    let p = self.degree;
    let inner = &self.knots[p..self.knots.len() - p];
    let step = inner[1] - inner[0];
    step > 0.0 && inner.windows(2).all(|w| ((w[1] - w[0]) - step).abs() < 1e-9 * step.max(1.0))
  }

  /// The parameter range the curve is defined on.
  pub fn domain(&self) -> (f64, f64) {
    (self.knots[self.degree], self.knots[self.control.len()])
  }

  /// The point at parameter `u` (de Boor, in homogeneous coordinates for rational curves).
  pub fn point(&self, u: f64) -> [f64; 2] {
    let p = self.degree;
    let n = self.control.len();
    let (a, b) = self.domain();
    let u = u.clamp(a, b);
    let k = (p..n).rev().find(|&i| self.knots[i] <= u && (u < self.knots[i + 1] || i == n - 1)).unwrap_or(p);
    let w = |i: usize| if self.weights.len() == n { self.weights[i] } else { 1.0 };
    let mut d: Vec<[f64; 3]> = (0..=p)
      .map(|j| {
        let i = j + k - p;
        let wi = w(i);
        [self.control[i][0] * wi, self.control[i][1] * wi, wi]
      })
      .collect();
    for r in 1..=p {
      for j in (r..=p).rev() {
        let i = j + k - p;
        let den = self.knots[i + p + 1 - r] - self.knots[i];
        let al = if den.abs() < 1e-300 { 0.0 } else { (u - self.knots[i]) / den };
        let prev = d[j - 1];
        for (c, v) in d[j].iter_mut().enumerate() {
          *v = (1.0 - al) * prev[c] + al * *v;
        }
      }
    }
    let h = d[p];
    if h[2].abs() < 1e-300 { [h[0], h[1]] } else { [h[0] / h[2], h[1] / h[2]] }
  }

  /// The curve as cubic Bézier pieces: exact for non-rational clamped curves up to degree 3,
  /// fitted (several pieces per knot span, matching end points and tangents) otherwise. A curve
  /// with only fit points goes through them with Catmull-Rom tangents.
  pub fn beziers(&self) -> Vec<Bez> {
    if self.control.is_empty() {
      return catmull_rom(&self.fit);
    }
    if !self.is_valid() {
      return Vec::new();
    }
    if self.degree <= 3 && !self.is_rational() && self.is_clamped() {
      return self.exact_beziers();
    }
    self.fitted_beziers()
  }

  /// Bézier extraction by knot insertion (Boehm), then degree elevation to cubic.
  fn exact_beziers(&self) -> Vec<Bez> {
    let p = self.degree;
    let mut knots = self.knots.clone();
    let mut pts = self.control.clone();
    let (a, b) = self.domain();
    let mut inner: Vec<f64> = knots.iter().copied().filter(|&u| u > a && u < b).collect();
    inner.dedup();
    for u in inner {
      loop {
        let s = knots.iter().filter(|&&x| x == u).count();
        if s >= p {
          break;
        }
        let Some(k) = (0..knots.len() - 1).rev().find(|&i| knots[i] <= u && u < knots[i + 1]) else { break };
        let mut q = Vec::with_capacity(pts.len() + 1);
        for i in 0..=pts.len() {
          if i + p <= k {
            q.push(pts[i]);
          } else if i + s <= k {
            let al = (u - knots[i]) / (knots[i + p] - knots[i]);
            q.push(lerp(pts[i - 1], pts[i], al));
          } else {
            q.push(pts[i - 1]);
          }
        }
        pts = q;
        knots.insert(k + 1, u);
      }
    }
    pts
      .windows(p + 1)
      .step_by(p)
      .map(|w| match p {
        1 => [w[0], lerp(w[0], w[1], 1.0 / 3.0), lerp(w[0], w[1], 2.0 / 3.0), w[1]],
        2 => [w[0], lerp(w[0], w[1], 2.0 / 3.0), lerp(w[2], w[1], 2.0 / 3.0), w[2]],
        _ => [w[0], w[1], w[2], w[3]],
      })
      .collect()
  }

  /// Hermite cubics through points and finite-difference tangents, each knot span halved until
  /// the pieces stay within a millionth of the curve's size of it.
  fn fitted_beziers(&self) -> Vec<Bez> {
    let (a, b) = self.domain();
    let size = self.control.iter().fold(0.0f64, |m, p| m.max(p[0].abs()).max(p[1].abs())).max(1e-3);
    let tol = size * 1e-6;
    let mut breaks: Vec<f64> = self.knots.iter().copied().filter(|&u| u >= a && u <= b).collect();
    breaks.dedup();
    let mut out = Vec::new();
    for w in breaks.windows(2) {
      self.fit_span(w[0], w[1], tol, 0, &mut out);
    }
    out
  }

  fn fit_span(&self, s0: f64, s1: f64, tol: f64, depth: u32, out: &mut Vec<Bez>) {
    let h = s1 - s0;
    // one-sided differences inside the span, so knots with reduced continuity stay sharp
    let e = h * 1e-5;
    let (p0, p3) = (self.point(s0), self.point(s1));
    let (q0, q1) = (self.point(s0 + e), self.point(s1 - e));
    let d0 = [(q0[0] - p0[0]) / e, (q0[1] - p0[1]) / e];
    let d1 = [(p3[0] - q1[0]) / e, (p3[1] - q1[1]) / e];
    let bz =
      [p0, [p0[0] + d0[0] * h / 3.0, p0[1] + d0[1] * h / 3.0], [p3[0] - d1[0] * h / 3.0, p3[1] - d1[1] * h / 3.0], p3];
    let err = [0.25, 0.5, 0.75]
      .iter()
      .map(|&t| {
        let (x, y) = (bezier_at(&bz, t), self.point(s0 + h * t));
        (x[0] - y[0]).hypot(x[1] - y[1])
      })
      .fold(0.0, f64::max);
    if err > tol && depth < 10 {
      let m = s0 + h / 2.0;
      self.fit_span(s0, m, tol, depth + 1, out);
      self.fit_span(m, s1, tol, depth + 1, out);
    } else {
      out.push(bz);
    }
  }
}

fn bezier_at(p: &Bez, t: f64) -> [f64; 2] {
  let u = 1.0 - t;
  let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
  [0, 1].map(|k| w[0] * p[0][k] + w[1] * p[1][k] + w[2] * p[2][k] + w[3] * p[3][k])
}

/// Cubic pieces through `pts` with Catmull-Rom tangents (end tangents along the end chords).
pub fn catmull_rom(pts: &[[f64; 2]]) -> Vec<Bez> {
  let n = pts.len();
  if n < 2 {
    return Vec::new();
  }
  let tangent = |i: usize| {
    let (a, b) = (pts[i.saturating_sub(1)], pts[(i + 1).min(n - 1)]);
    let k = if i == 0 || i == n - 1 { 1.0 } else { 0.5 };
    [(b[0] - a[0]) * k, (b[1] - a[1]) * k]
  };
  (0..n - 1)
    .map(|i| {
      let (t0, t1) = (tangent(i), tangent(i + 1));
      let (a, b) = (pts[i], pts[i + 1]);
      [a, [a[0] + t0[0] / 3.0, a[1] + t0[1] / 3.0], [b[0] - t1[0] / 3.0, b[1] - t1[1] / 3.0], b]
    })
    .collect()
}

/// An elliptical arc as cubic Bézier pieces (at most a quarter turn each): centre, major axis
/// vector, minor/major ratio, counter-clockwise from parameter `start` to `end`.
pub fn ellipse_beziers(center: [f64; 2], major: [f64; 2], ratio: f64, start: f64, end: f64) -> Vec<Bez> {
  use std::f64::consts::{FRAC_PI_2, TAU};
  let mut sweep = (end - start).rem_euclid(TAU);
  if sweep < 1e-12 {
    sweep = TAU;
  }
  let minor = [-major[1] * ratio, major[0] * ratio];
  let at =
    |t: f64| [center[0] + major[0] * t.cos() + minor[0] * t.sin(), center[1] + major[1] * t.cos() + minor[1] * t.sin()];
  let deriv = |t: f64| [-major[0] * t.sin() + minor[0] * t.cos(), -major[1] * t.sin() + minor[1] * t.cos()];
  let n = (sweep / FRAC_PI_2 - 1e-9).ceil().max(1.0) as usize;
  let h = sweep / n as f64;
  let k = 4.0 / 3.0 * (h / 4.0).tan();
  (0..n)
    .map(|i| {
      let (t0, t1) = (start + h * i as f64, start + h * (i + 1) as f64);
      let (p0, p3, d0, d1) = (at(t0), at(t1), deriv(t0), deriv(t1));
      [p0, [p0[0] + d0[0] * k, p0[1] + d0[1] * k], [p3[0] - d1[0] * k, p3[1] - d1[1] * k], p3]
    })
    .collect()
}
