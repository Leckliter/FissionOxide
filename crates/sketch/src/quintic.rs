//! Quintic splines: how Fusion stores a fitted spline through two points, as its Blend Curve
//! makes. The curve is a clamped degree-5 B-spline with ten control points over five even spans
//! (knots 0 ×6, 0.2, 0.4, 0.6, 0.8, 1 ×6). It passes through its first and last control points.
//!
//! [`blend_points`] shapes a blend the way Fusion's sketch solver does. These rules were measured
//! by building blends in Fusion and reading back their control points; they reproduce Fusion's to
//! about 1e-7 of the chord.
//! - Between its end conditions the curve is the one of least energy
//!   ∫|C′|² + 5/128 ∫|C″|² + 1/125 ∫|C‴|² over the parameter 0..1.
//! - Each end's speed |C′| is the chord times s(θ) = (3 − cos θ) / 2, where θ is the angle
//!   between the end's direction of travel and the chord.
//! - G1 applies the first curve's tangency, then the second's. The second rescales both ends by
//!   the mean of the chord and the free end's |C′| / s just before it.
//! - G2 sets C″ at each end to the curvature it continues, with no part along the tangent.
//!   Each end's speed depends on that curvature (see `g2_speed`).

type P = [f64; 2];

/// Number of control points of a quintic spline.
pub const QUINTIC_POINTS: usize = 10;
const DEGREE: usize = 5;
const SPANS: usize = 5;
/// Parameter length of one span.
const H: f64 = 1.0 / SPANS as f64;
/// |C′| at an end is this times the first control leg, C″ needs `C2_LEG` (see `end_derivs`).
const C1_LEG: f64 = DEGREE as f64 / H;
/// Fusion's fairing weights on ∫|C″|² and ∫|C‴|², relative to ∫|C′|².
const W2: f64 = 5.0 / 128.0;
const W3: f64 = 1.0 / 125.0;

fn sub(a: P, b: P) -> P {
  [a[0] - b[0], a[1] - b[1]]
}

fn add(a: P, b: P) -> P {
  [a[0] + b[0], a[1] + b[1]]
}

fn mul(a: P, k: f64) -> P {
  [a[0] * k, a[1] * k]
}

fn dot(a: P, b: P) -> f64 {
  a[0] * b[0] + a[1] * b[1]
}

fn cross(a: P, b: P) -> f64 {
  a[0] * b[1] - a[1] * b[0]
}

fn len(a: P) -> f64 {
  a[0].hypot(a[1])
}

fn left(a: P) -> P {
  [-a[1], a[0]]
}

fn unit(a: P) -> Option<P> {
  let l = len(a);
  (l > 1e-12).then(|| [a[0] / l, a[1] / l])
}

/// The control point positions of a quintic spline's point ids (None unless there are ten).
pub(crate) fn ctrl_of(
  points: &[crate::PointId],
  pos: &dyn Fn(crate::PointId) -> Option<P>,
) -> Option<[P; QUINTIC_POINTS]> {
  if points.len() != QUINTIC_POINTS {
    return None;
  }
  let mut out = [[0.0; 2]; QUINTIC_POINTS];
  for (o, p) in out.iter_mut().zip(points) {
    *o = pos(*p)?;
  }
  Some(out)
}

/// The clamped knot vector.
fn knots() -> Vec<f64> {
  let mut t = vec![0.0; DEGREE + 1];
  t.extend((1..SPANS).map(|k| k as f64 * H));
  t.extend([1.0; DEGREE + 1]);
  t
}

/// Inserts knot `u` once (Boehm), for control values of any kind combined by `mix`.
fn insert<T: Copy>(t: &mut Vec<f64>, c: &mut Vec<T>, u: f64, mix: &impl Fn(T, T, f64) -> T) {
  let p = DEGREE;
  let Some(k) = (p..t.len() - p - 1).rev().find(|&k| t[k] <= u) else { return };
  let mut out = Vec::with_capacity(c.len() + 1);
  for i in 0..=c.len() {
    out.push(if i + p <= k {
      c[i]
    } else if i > k {
      c[i - 1]
    } else {
      let a = (u - t[i]) / (t[i + p] - t[i]);
      mix(c[i - 1], c[i], a)
    });
  }
  t.insert(k + 1, u);
  *c = out;
}

/// The quintic Bézier pieces of the spline, one per span.
fn spans_of<T: Copy + Default>(c: &[T; QUINTIC_POINTS], mix: impl Fn(T, T, f64) -> T) -> [[T; 6]; SPANS] {
  let mut t = knots();
  let mut q = c.to_vec();
  for k in 1..SPANS {
    for _ in 1..DEGREE {
      insert(&mut t, &mut q, k as f64 * H, &mix);
    }
  }
  let mut out = [[T::default(); 6]; SPANS];
  for (s, span) in out.iter_mut().enumerate() {
    for (i, v) in span.iter_mut().enumerate() {
      *v = q.get(s * DEGREE + i).copied().unwrap_or_default();
    }
  }
  out
}

/// The spline's quintic Bézier pieces (exact), one per span.
pub fn quintic_spans(p: &[P; QUINTIC_POINTS]) -> [[P; 6]; SPANS] {
  spans_of(p, |a, b, k| add(mul(a, 1.0 - k), mul(b, k)))
}

/// Value and first three derivatives at `s` (0 to 1) of a quintic Bézier with scalar values.
fn bez5_derivs(b: &[f64; 6], s: f64) -> [f64; 4] {
  let casteljau = |v: &[f64]| {
    let mut v = v.to_vec();
    for r in (1..v.len()).rev() {
      for i in 0..r {
        v[i] += (v[i + 1] - v[i]) * s;
      }
    }
    v.first().copied().unwrap_or(0.0)
  };
  let mut out = [0.0; 4];
  let mut v = b.to_vec();
  for (r, o) in out.iter_mut().enumerate() {
    *o = casteljau(&v);
    let n = (DEGREE - r) as f64;
    v = v.windows(2).map(|w| n * (w[1] - w[0])).collect();
  }
  out
}

fn bez5_at(b: &[P; 6], s: f64) -> (P, P) {
  let x = bez5_derivs(&b.map(|q| q[0]), s);
  let y = bez5_derivs(&b.map(|q| q[1]), s);
  ([x[0], y[0]], [x[1], y[1]])
}

/// The point at parameter `u` (0 to 1).
pub fn quintic_at(p: &[P; QUINTIC_POINTS], u: f64) -> P {
  let u = u.clamp(0.0, 1.0);
  let k = ((u / H) as usize).min(SPANS - 1);
  bez5_at(&quintic_spans(p)[k], u / H - k as f64).0
}

/// The spline as cubic Bézier pieces for drawing, picking and offsetting: each span split in
/// eight, each piece the cubic with its ends and end tangents (within about 1e-5 of the chord).
pub fn quintic_beziers(p: &[P; QUINTIC_POINTS]) -> Vec<[P; 4]> {
  const SPLIT: usize = 8;
  let mut out = Vec::with_capacity(SPANS * SPLIT);
  for b in quintic_spans(p) {
    for j in 0..SPLIT {
      let (s0, s1) = (j as f64 / SPLIT as f64, (j + 1) as f64 / SPLIT as f64);
      let ((a, da), (e, de)) = (bez5_at(&b, s0), bez5_at(&b, s1));
      let k = (s1 - s0) / 3.0;
      out.push([a, add(a, mul(da, k)), sub(e, mul(de, k)), e]);
    }
  }
  out
}

/// C′ and C″ leaving the start (at_end false) or arriving at the end.
fn end_derivs(p: &[P; QUINTIC_POINTS], at_end: bool) -> (P, P) {
  // C″(0) = p(p−1)/H · ((p2 − p1)/2H − (p1 − p0)/H)
  let c2 = (DEGREE * (DEGREE - 1)) as f64 / H;
  let (a, b, c) = if at_end { (p[9], p[8], p[7]) } else { (p[0], p[1], p[2]) };
  let d1 = mul(sub(b, a), C1_LEG);
  let d2 = sub(mul(sub(c, b), c2 / (2.0 * H)), mul(sub(b, a), c2 / H));
  (if at_end { mul(d1, -1.0) } else { d1 }, d2)
}

/// (position, outward direction, curvature) at an end, as `freeform::end_frame`: the direction
/// leads away from the curve and the curvature is signed along it (positive turning left).
pub(crate) fn quintic_end(p: &[P; QUINTIC_POINTS], at_end: bool) -> Option<(P, P, f64)> {
  let (d1, d2) = end_derivs(p, at_end);
  let l = len(d1);
  if l < 1e-12 {
    let q = if at_end { p[0] } else { p[9] };
    let o = if at_end { p[9] } else { p[0] };
    return Some((o, unit(sub(o, q))?, 0.0));
  }
  let k = cross(d1, d2) / (l * l * l);
  Some(if at_end { (p[9], mul(d1, 1.0 / l), k) } else { (p[0], mul(d1, -1.0 / l), -k) })
}

/// The 10 × 10 matrix of the fairing energy on each coordinate's control values.
fn energy_matrix() -> [[f64; QUINTIC_POINTS]; QUINTIC_POINTS] {
  // 5-point Gauss-Legendre: exact for the degree-8 products of derivatives
  const GAUSS: [(f64, f64); 5] = [
    (-0.906_179_845_938_664, 0.236_926_885_056_189_1),
    (-0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (0.0, 0.568_888_888_888_888_9),
    (0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (0.906_179_845_938_664, 0.236_926_885_056_189_1),
  ];
  let basis: Vec<[[f64; 6]; SPANS]> = (0..QUINTIC_POINTS)
    .map(|i| {
      let mut c = [0.0; QUINTIC_POINTS];
      c[i] = 1.0;
      spans_of(&c, |a, b, k| a * (1.0 - k) + b * k)
    })
    .collect();
  let mut m = [[0.0; QUINTIC_POINTS]; QUINTIC_POINTS];
  for s in 0..SPANS {
    for (x, w) in GAUSS {
      let at = (x + 1.0) / 2.0;
      let wt = w / 2.0 * H;
      let d: Vec<[f64; 4]> = basis.iter().map(|b| bez5_derivs(&b[s], at)).collect();
      for i in 0..QUINTIC_POINTS {
        for j in 0..QUINTIC_POINTS {
          let (a, b) = (d[i], d[j]);
          m[i][j] += wt * (a[1] * b[1] / (H * H) + W2 * a[2] * b[2] / H.powi(4) + W3 * a[3] * b[3] / H.powi(6));
        }
      }
    }
  }
  m
}

/// Moves the control points not in `fixed` to where the fairing energy is least.
fn fair(c: &mut [P; QUINTIC_POINTS], fixed: &[usize]) -> Option<()> {
  let m = energy_matrix();
  let free: Vec<usize> = (0..QUINTIC_POINTS).filter(|i| !fixed.contains(i)).collect();
  let n = free.len();
  // rows: M_ff x = −M_fF c_F, one right-hand side per coordinate
  let mut a: Vec<Vec<f64>> = free
    .iter()
    .map(|&i| {
      let mut row: Vec<f64> = free.iter().map(|&j| m[i][j]).collect();
      let rhs = fixed.iter().fold([0.0, 0.0], |acc, &j| sub(acc, mul(c[j], m[i][j])));
      row.extend(rhs);
      row
    })
    .collect();
  for col in 0..n {
    let piv = (col..n).max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))?;
    if a[piv][col].abs() < 1e-300 {
      return None;
    }
    a.swap(col, piv);
    let pivot = a[col].clone();
    for (r, row) in a.iter_mut().enumerate() {
      if r != col {
        let f = row[col] / pivot[col];
        for (v, p) in row.iter_mut().zip(&pivot).skip(col) {
          *v -= f * p;
        }
      }
    }
  }
  for (r, &i) in free.iter().enumerate() {
    c[i] = [a[r][n] / a[r][r], a[r][n + 1] / a[r][r]];
  }
  Some(())
}

/// The straight spline from `a` to `b`, evenly parameterised (its control points at the knots'
/// Greville abscissae).
fn straight(a: P, b: P) -> [P; QUINTIC_POINTS] {
  let t = knots();
  std::array::from_fn(|i| {
    let g = t[i + 1..i + 1 + DEGREE].iter().sum::<f64>() / DEGREE as f64;
    add(a, mul(sub(b, a), g))
  })
}

/// Fusion's end speed factor (3 − cos θ) / 2 for direction `d` against the chord direction `e`.
fn turn(d: P, e: P) -> f64 {
  (3.0 - dot(d, e) / len(d).max(1e-300)) / 2.0
}

/// A G2 end's speed over the chord, for travel direction `t`, chord direction `e` and
/// x = |curvature| × chord. Fusion uses 1.44 s(θ) at a straight end. With curvature it uses the
/// cube-root law (c/x)^(1/3), where c = s·max(s², 2|sin θ|)/2. Below the point where that law
/// reaches 1.08 s, it uses the law's tangent line, which meets 1.44 s at x = 0.
fn g2_speed(t: P, e: P, x: f64) -> f64 {
  let s = turn(t, e);
  if x < 1e-12 {
    return 1.44 * s;
  }
  let sin = cross(t, e).abs() / len(t).max(1e-300);
  let c = 0.5 * s * (s * s).max(2.0 * sin);
  let g = 1.08 * s;
  let xt = c / (g * g * g);
  if x >= xt { (c / x).cbrt() } else { g * (4.0 - x / xt) / 3.0 }
}

/// Control points of Fusion's blend curve from an end of one curve to an end of another, given
/// each end as (position, outward direction, curvature signed along it), as
/// `freeform::end_frame` gives them. G1 matches tangents, G2 tangents and curvature.
pub(crate) fn blend_points(a: (P, P, f64), b: (P, P, f64), g2: bool) -> Option<[P; QUINTIC_POINTS]> {
  let (p0, p9) = (a.0, b.0);
  let chord = len(sub(p9, p0));
  let e = unit(sub(p9, p0))?;
  // the blend leaves along the first curve and arrives running into the second
  let (t0, k0) = (unit(a.1)?, a.2);
  let (t1, k1) = (mul(unit(b.1)?, -1.0), -b.2);
  let mut c = straight(p0, p9);
  if g2 {
    let (m0, m1) = (g2_speed(t0, e, k0.abs() * chord) * chord, g2_speed(t1, e, k1.abs() * chord) * chord);
    // C″ = κ|C′|² across the tangent: p2 = p0 + 3(p1 − p0) + κ|C′|² H² / (p(p−1)/2) · n
    let bend = 2.0 * H * H / (DEGREE * (DEGREE - 1)) as f64;
    c[1] = add(p0, mul(t0, m0 / C1_LEG));
    c[2] = add(add(p0, mul(sub(c[1], p0), 3.0)), mul(left(t0), bend * k0 * m0 * m0));
    c[8] = sub(p9, mul(t1, m1 / C1_LEG));
    c[7] = add(add(p9, mul(sub(c[8], p9), 3.0)), mul(left(t1), bend * k1 * m1 * m1));
    fair(&mut c, &[0, 1, 2, 7, 8, 9])?;
  } else {
    // the first tangency alone, the far end free
    c[1] = add(p0, mul(t0, chord * turn(t0, e) / C1_LEG));
    fair(&mut c, &[0, 1, 9])?;
    let free = mul(sub(c[9], c[8]), C1_LEG);
    let scale = if len(free) > 1e-12 { (chord + len(free) / turn(free, e)) / 2.0 } else { chord };
    c[1] = add(p0, mul(t0, scale * turn(t0, e) / C1_LEG));
    c[8] = sub(p9, mul(t1, scale * turn(t1, e) / C1_LEG));
    fair(&mut c, &[0, 1, 8, 9])?;
  }
  c.iter().all(|q| q[0].is_finite() && q[1].is_finite()).then_some(c)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn close(a: P, b: P, tol: f64) -> bool {
    len(sub(a, b)) < tol
  }

  #[test]
  fn spans_reproduce_the_b_spline() {
    // a straight evenly parameterised spline stays on its line at the right parameter
    let c = straight([1.0, 2.0], [11.0, 7.0]);
    for k in 0..=20 {
      let u = k as f64 / 20.0;
      assert!(close(quintic_at(&c, u), [1.0 + 10.0 * u, 2.0 + 5.0 * u], 1e-12), "{u}");
    }
    // the cubic pieces join and follow the spans
    let mut c = straight([0.0, 0.0], [20.0, 0.0]);
    c[3][1] = 4.0;
    c[6][1] = -3.0;
    let bz = quintic_beziers(&c);
    for w in bz.windows(2) {
      assert!(close(w[0][3], w[1][0], 1e-12));
    }
    for (i, b) in bz.iter().enumerate() {
      let mid = crate::bezier_at(b, 0.5);
      let exact = quintic_at(&c, (i as f64 + 0.5) / bz.len() as f64);
      assert!(close(mid, exact, 1e-4), "{mid:?} {exact:?}");
    }
  }

  /// Fusion's blend curves (addBlendCurve), read back as NURBS control points.
  #[test]
  fn blends_match_fusion() {
    // lines (0,0)→(30,0) and (30,20)→(0,20) joined at their right ends
    let a = ([30.0, 0.0], [1.0, 0.0], 0.0);
    let b = ([30.0, 20.0], [1.0, 0.0], 0.0);
    let g1 = blend_points(a, b, false).unwrap();
    let fusion_g1 = [
      [30.0, 0.0],
      [31.598826, 0.0],
      [34.430006, 0.408406],
      [37.578258, 2.244963],
      [39.799471, 6.659848],
      [39.799471, 13.340152],
      [37.578258, 17.755037],
      [34.430006, 19.591594],
      [31.598826, 20.0],
      [30.0, 20.0],
    ];
    let g2 = blend_points(a, b, true).unwrap();
    let fusion_g2 = [
      [30.0, 0.0],
      [31.728, 0.0],
      [35.184, 0.0],
      [39.834981, 1.012957],
      [43.717064, 5.766559],
      [43.717064, 14.233441],
      [39.834981, 18.987043],
      [35.184, 20.0],
      [31.728, 20.0],
      [30.0, 20.0],
    ];
    // a quarter arc about (0,20) ending at (10,20) heading +y, and a line down from (40,40)
    let arc = ([10.0, 20.0], [0.0, 1.0], 0.1);
    let line = ([40.0, 40.0], [0.0, 1.0], 0.0);
    let arch_g1 = blend_points(arc, line, false).unwrap();
    let fusion_arch_g1 = [
      [10.0, 20.0],
      [10.0, 21.979783],
      [10.612608, 26.020294],
      [13.367444, 32.32327],
      [19.989772, 40.794557],
      [30.010227, 48.979519],
      [36.632555, 50.702017],
      [39.387391, 47.439549],
      [40.0, 42.877985],
      [40.0, 40.0],
    ];
    let arch_g2 = blend_points(arc, line, true).unwrap();
    let fusion_arch_g2 = [
      [10.0, 20.0],
      [10.0, 20.945929],
      [9.776305, 22.837786],
      [10.717662, 27.711369],
      [17.522497, 40.339456],
      [30.712098, 56.470577],
      [38.353594, 58.681012],
      [40.0, 51.073589],
      [40.0, 43.691196],
      [40.0, 40.0],
    ];
    for (got, want) in [(g1, fusion_g1), (g2, fusion_g2), (arch_g1, fusion_arch_g1), (arch_g2, fusion_arch_g2)] {
      for (g, w) in got.iter().zip(&want) {
        assert!(close(*g, *w, 1e-5), "{g:?} vs Fusion's {w:?}");
      }
    }
  }

  #[test]
  fn blend_ends_are_tangent_and_curvature_continuous() {
    // an arc end (curving right as it leaves) to a line end, at an angle
    let a = ([0.0, 0.0], [0.6, 0.8], -0.07);
    let b = ([25.0, 5.0], [0.0, -1.0], 0.0);
    for g2 in [false, true] {
      let c = blend_points(a, b, g2).unwrap();
      let (s, e) = (quintic_end(&c, false).unwrap(), quintic_end(&c, true).unwrap());
      assert!(close(s.0, a.0, 1e-12) && close(e.0, b.0, 1e-12));
      // the blend leaves where the first curve heads and arrives running into the second
      assert!(close(s.1, [-0.6, -0.8], 1e-9), "{:?}", s.1);
      assert!(close(e.1, [0.0, 1.0], 1e-9), "{:?}", e.1);
      if g2 {
        // arriving at the joint both curvatures are taken toward it: equal and opposite
        assert!((s.2 - 0.07).abs() < 1e-9, "{}", s.2);
        assert!(e.2.abs() < 1e-9, "{}", e.2);
      }
    }
  }
}
