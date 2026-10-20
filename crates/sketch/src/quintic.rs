//! The curve of a fit-point spline, as Fusion fits it: one quintic B-spline through the fit
//! points that bends as little as it can.
//!
//! Measured against Fusion's `NurbsCurve3D.getData()`: the curve has degree 5, five uniform knot
//! spans per gap between fit points (clamped ends, or periodic when closed), and minimises
//! `∫ |C'|² + 5/128 |C''|² + 1/125 |C'''|² dt` over `t` in [0, 1] while passing through each fit
//! point. Fit point `i` sits at the parameter where the running sum of `√(chord / 1 cm + 0.22 /
//! gaps)` reaches it (chords in Fusion's internal centimetres, so the curve depends on its size).
//! The solve is a banded KKT system (plus a small dense border for the periodic wrap), so it
//! stays linear in the number of points.
//!
//! The rest of the sketch works on cubic Bézier pieces, so the curve is handed out as one cubic
//! per knot span (also split at the fit points): Hermite pieces matching the quintic's position
//! and tangent at both ends, within about 1e-3 of a millimetre of it.

type P = [f64; 2];

const DEG: usize = 5;
/// Knot spans per gap between fit points.
pub(crate) const SPANS_PER_GAP: usize = 5;
/// Weights of the second and third derivative terms of the energy (the first has weight 1).
const W2: f64 = 5.0 / 128.0;
const W3: f64 = 1.0 / 125.0;
/// Fit parameters closer than this to a knot start no piece of their own.
const SNAP: f64 = 1e-9;

/// Five-point Gauss-Legendre nodes on [-1, 1] and their weights (exact to degree 9).
const GAUSS: [(f64, f64); 5] = [
  (0.0, 0.568_888_888_888_888_9),
  (-0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
  (0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
  (-0.906_179_845_938_664, 0.236_926_885_056_189_08),
  (0.906_179_845_938_664, 0.236_926_885_056_189_08),
];

/// A fitted quintic: `spans` uniform knot spans over `t` in [0, 1].
#[derive(Debug, Clone)]
pub(crate) struct Quintic {
  poles: Vec<P>,
  spans: usize,
  closed: bool,
  /// The fit points and their parameters.
  fit: Vec<(f64, P)>,
}

impl Quintic {
  /// The minimum-energy quintic through `pts` (the start point not repeated at the end when
  /// `closed`). `handles` are the tangent handles of an open curve: where the inner control point
  /// of a cubic Bézier spanning the first (last) gap would be, which fixes the end's tangent.
  /// None for fewer than two points or a singular system.
  pub(crate) fn fit(pts: &[P], closed: bool, handles: (Option<P>, Option<P>)) -> Option<Quintic> {
    let n = pts.len();
    if n < 2 || pts.iter().flatten().any(|v| !v.is_finite()) {
      return None;
    }
    let gaps = if closed { n } else { n - 1 };
    let spans = gaps * SPANS_PER_GAP;
    let chord = |i: usize| crate::dist(pts[i], pts[(i + 1) % n]);
    let w: Vec<f64> = (0..gaps).map(|i| (chord(i) / 10.0 + 0.22 / gaps as f64).sqrt()).collect();
    let total: f64 = w.iter().sum();
    let mut params = Vec::with_capacity(n);
    let mut acc = 0.0;
    for (i, wi) in w.iter().enumerate() {
      params.push(if i == 0 { 0.0 } else { acc / total });
      acc += wi;
    }
    if !closed {
      params.push(1.0);
    }
    let basis = Basis { spans, closed };
    let poles_n = basis.poles();
    // unknowns: poles 0..poles_n, then one multiplier per constraint
    let mut rows: Vec<Vec<(usize, f64)>> = Vec::new();
    let mut rhs: Vec<P> = Vec::new();
    for (&t, &q) in params.iter().zip(pts) {
      let (first, d) = basis.ders(t * spans as f64, 0);
      rows.push((0..=DEG).map(|k| (basis.pole(first + k), d[0][k])).collect());
      rhs.push(q);
    }
    if !closed {
      let scale = spans as f64;
      let mut tangent = |at_end: bool, h: P, rows: &mut Vec<Vec<(usize, f64)>>| {
        let (q, gap) = if at_end { (pts[n - 1], 1.0 - params[n - 2]) } else { (pts[0], params[1]) };
        let s = if at_end { 3.0 / gap } else { -3.0 / gap };
        // dC/dt at the end = 3 (q - h) / gap at the end, 3 (h - q) / gap at the start
        let (first, d) = basis.ders(if at_end { spans as f64 } else { 0.0 }, 1);
        rows.push((0..=DEG).map(|k| (basis.pole(first + k), d[1][k] * scale)).collect());
        rhs.push([s * (q[0] - h[0]), s * (q[1] - h[1])]);
      };
      if let Some(h) = handles.0 {
        tangent(false, h, &mut rows);
      }
      if let Some(h) = handles.1 {
        tangent(true, h, &mut rows);
      }
    }
    let m = rows.len();
    let size = poles_n + m;
    // energy, scaled so its largest weight is about 1 (the minimiser does not change)
    let ns = spans as f64;
    let norm = 1.0 / (ns + W2 * ns.powi(3) + W3 * ns.powi(5));
    let wk = [0.0, ns * norm, W2 * ns.powi(3) * norm, W3 * ns.powi(5) * norm];
    let mut trip: Vec<(usize, usize, f64)> = Vec::new();
    for j in 0..spans {
      let mut local = [[0.0; DEG + 1]; DEG + 1];
      for (x, gw) in GAUSS {
        let u = j as f64 + 0.5 + 0.5 * x;
        let (_, d) = basis.ders_in(j, u, 3);
        for (k, wkk) in wk.iter().enumerate().skip(1) {
          for a in 0..=DEG {
            for b in 0..=DEG {
              local[a][b] += 0.5 * gw * wkk * d[k][a] * d[k][b];
            }
          }
        }
      }
      let first = j;
      for (a, row) in local.iter().enumerate() {
        for (b, v) in row.iter().enumerate() {
          trip.push((basis.pole(first + a), basis.pole(first + b), *v));
        }
      }
    }
    for (r, row) in rows.iter().enumerate() {
      for &(c, v) in row {
        trip.push((poles_n + r, c, v));
        trip.push((c, poles_n + r, v));
      }
    }
    let mut b = vec![[0.0; 2]; size];
    b[poles_n..].copy_from_slice(&rhs);
    // order: poles with each multiplier beside the poles it touches. A periodic curve's first
    // five poles (which meet the last ones) go last, as a dense border, with the multipliers of
    // the fit points that touch them (so the band left is the system of the other poles alone).
    let wrap = if closed { DEG } else { 0 };
    let size_f = size as f64;
    let mut slot: Vec<(f64, usize)> = (0..poles_n)
      .map(|p| (if p < wrap { size_f + p as f64 } else { p as f64 }, p))
      .chain(rows.iter().enumerate().map(|(r, row)| {
        let at = if row.iter().any(|e| e.0 < wrap && e.1 != 0.0) {
          2.0 * size_f + r as f64
        } else {
          row.iter().map(|e| e.0).max().unwrap_or(0) as f64 - 2.5
        };
        (at, poles_n + r)
      }))
      .collect();
    slot.sort_by(|a, b| a.0.total_cmp(&b.0));
    let border = slot.iter().filter(|s| s.0 >= size_f).count();
    let order: Vec<usize> = slot.into_iter().map(|s| s.1).collect();
    let x = solve_bordered(size, &trip, &b, &order, border)?;
    let poles = x[..poles_n].to_vec();
    if poles.iter().flatten().any(|v| !v.is_finite()) {
      return None;
    }
    let fit = params.into_iter().zip(pts.iter().copied()).collect();
    Some(Quintic { poles, spans, closed, fit })
  }

  /// Position and derivative (by `t`) at `t` in [0, 1].
  pub(crate) fn eval(&self, t: f64) -> (P, P) {
    let basis = Basis { spans: self.spans, closed: self.closed };
    let (first, d) = basis.ders(t * self.spans as f64, 1);
    let mut p = [0.0; 2];
    let mut v = [0.0; 2];
    for (k, (b0, b1)) in d[0].iter().zip(&d[1]).enumerate() {
      let q = self.poles[basis.pole(first + k)];
      for i in 0..2 {
        p[i] += b0 * q[i];
        v[i] += b1 * q[i] * self.spans as f64;
      }
    }
    (p, v)
  }

  /// The tangent handles (see [`Quintic::fit`]) that give an open curve its present end tangents.
  pub(crate) fn handles(&self) -> Option<(P, P)> {
    let n = self.fit.len();
    if self.closed || n < 2 {
      return None;
    }
    let ((_, d0), (_, d1)) = (self.eval(0.0), self.eval(1.0));
    let (g0, g1) = (self.fit[1].0 / 3.0, (1.0 - self.fit[n - 2].0) / 3.0);
    let (q0, q1) = (self.fit[0].1, self.fit[n - 1].1);
    Some(([q0[0] + d0[0] * g0, q0[1] + d0[1] * g0], [q1[0] - d1[0] * g1, q1[1] - d1[1] * g1]))
  }

  /// The curve as cubic Bézier pieces, one per knot span and split at the fit points, with the
  /// index of the piece each fit point starts (the last open fit point: the piece count).
  pub(crate) fn beziers(&self) -> (Vec<[P; 4]>, Vec<usize>) {
    let ns = self.spans as f64;
    // breaks: (t, exact point when a fit point lies there)
    let mut breaks: Vec<(f64, Option<P>)> = (0..=self.spans).map(|j| (j as f64 / ns, None)).collect();
    for &(t, q) in &self.fit {
      match breaks.iter_mut().find(|b| (b.0 - t).abs() <= SNAP) {
        Some(b) => b.1 = Some(q),
        None => breaks.push((t, Some(q))),
      }
    }
    breaks.sort_by(|a, b| a.0.total_cmp(&b.0));
    if self.closed
      && let (Some(first), Some(last)) = (breaks.first().and_then(|b| b.1), breaks.last_mut())
    {
      last.1 = Some(first);
    }
    let at: Vec<(P, P)> = breaks
      .iter()
      .map(|&(t, q)| {
        let (p, d) = self.eval(t);
        (q.unwrap_or(p), d)
      })
      .collect();
    let pieces: Vec<[P; 4]> = (0..breaks.len() - 1)
      .map(|i| {
        let h = (breaks[i + 1].0 - breaks[i].0) / 3.0;
        let ((p0, d0), (p3, d3)) = (at[i], at[i + 1]);
        [p0, [p0[0] + d0[0] * h, p0[1] + d0[1] * h], [p3[0] - d3[0] * h, p3[1] - d3[1] * h], p3]
      })
      .collect();
    let starts =
      self.fit.iter().map(|&(t, _)| breaks.iter().position(|b| (b.0 - t).abs() <= SNAP).unwrap_or(0)).collect();
    (pieces, starts)
  }
}

/// The B-spline basis: clamped uniform knots 0..spans (open), or uniform knots repeating with
/// period `spans` (closed).
struct Basis {
  spans: usize,
  closed: bool,
}

impl Basis {
  fn poles(&self) -> usize {
    if self.closed { self.spans } else { self.spans + DEG }
  }

  /// Pole index of unwrapped index `k`.
  fn pole(&self, k: usize) -> usize {
    if self.closed { k % self.spans } else { k }
  }

  /// Knot `i` of the (unwrapped) knot vector; span `j` runs from knot `j + DEG` to `j + DEG + 1`.
  fn knot(&self, i: usize) -> f64 {
    let u = i as f64 - DEG as f64;
    if self.closed { u } else { u.clamp(0.0, self.spans as f64) }
  }

  /// The (unwrapped) index of the first pole in play and the basis functions' derivatives 0..=`nd` at `u` in [0, spans].
  fn ders(&self, u: f64, nd: usize) -> (usize, [[f64; DEG + 1]; 4]) {
    let j = (u.floor().max(0.0) as usize).min(self.spans - 1);
    self.ders_in(j, u, nd)
  }

  /// [`Basis::ders`] in span `j` (The NURBS Book, A2.3).
  fn ders_in(&self, j: usize, u: f64, nd: usize) -> (usize, [[f64; DEG + 1]; 4]) {
    let span = j + DEG;
    let mut ndu = [[0.0; DEG + 1]; DEG + 1];
    let mut left = [0.0; DEG + 1];
    let mut right = [0.0; DEG + 1];
    ndu[0][0] = 1.0;
    for jj in 1..=DEG {
      left[jj] = u - self.knot(span + 1 - jj);
      right[jj] = self.knot(span + jj) - u;
      let mut saved = 0.0;
      for r in 0..jj {
        ndu[jj][r] = right[r + 1] + left[jj - r];
        let temp = ndu[r][jj - 1] / ndu[jj][r];
        ndu[r][jj] = saved + right[r + 1] * temp;
        saved = left[jj - r] * temp;
      }
      ndu[jj][jj] = saved;
    }
    let mut ders = [[0.0; DEG + 1]; 4];
    for r in 0..=DEG {
      ders[0][r] = ndu[r][DEG];
    }
    let mut a = [[0.0; DEG + 1]; 2];
    for r in 0..=DEG {
      let (mut s1, mut s2) = (0usize, 1usize);
      a[0][0] = 1.0;
      for k in 1..=nd.min(3) {
        let mut d = 0.0;
        let rk = r as isize - k as isize;
        let pk = DEG - k;
        if rk >= 0 {
          a[s2][0] = a[s1][0] / ndu[pk + 1][rk as usize];
          d = a[s2][0] * ndu[rk as usize][pk];
        }
        let j1 = if rk >= -1 { 1 } else { (-rk) as usize };
        let j2 = if (r as isize - 1) <= pk as isize { k - 1 } else { DEG - r };
        for jj in j1..=j2 {
          let idx = (rk + jj as isize) as usize;
          a[s2][jj] = (a[s1][jj] - a[s1][jj - 1]) / ndu[pk + 1][idx];
          d += a[s2][jj] * ndu[idx][pk];
        }
        if r <= pk {
          a[s2][k] = -a[s1][k - 1] / ndu[pk + 1][r];
          d += a[s2][k] * ndu[r][pk];
        }
        ders[k][r] = d;
        std::mem::swap(&mut s1, &mut s2);
      }
    }
    let mut f = DEG as f64;
    for (k, row) in ders.iter_mut().enumerate().take(nd.min(3) + 1).skip(1) {
      for v in row.iter_mut() {
        *v *= f;
      }
      f *= (DEG - k) as f64;
    }
    (j, ders)
  }
}

/// Solves the symmetric system `trip` (row, column, value; repeats add) for two right-hand sides.
/// `order` lists the unknowns in elimination order; the last `border` of them may couple with
/// any other, the rest only with neighbours in that order (a band).
fn solve_bordered(
  size: usize,
  trip: &[(usize, usize, f64)],
  rhs: &[P],
  order: &[usize],
  border: usize,
) -> Option<Vec<P>> {
  let mut pos = vec![0usize; size];
  for (i, &u) in order.iter().enumerate() {
    pos[u] = i;
  }
  let nb = size - border;
  let (mut kl, mut ku) = (0usize, 0usize);
  for &(r, c, _) in trip {
    let (r, c) = (pos[r], pos[c]);
    if r < nb && c < nb {
      if r > c {
        kl = kl.max(r - c);
      } else {
        ku = ku.max(c - r);
      }
    }
  }
  let w = 2 * kl + ku + 1;
  // band rows: row i holds columns i - kl .. i - kl + w
  let mut band = vec![0.0; nb * w];
  let col = |i: usize, c: usize| i * w + (c + kl - i);
  // right-hand sides of the band solve: the border columns, then x and y
  let nr = border + 2;
  let mut rb = vec![0.0; nb * nr];
  let mut cmat = vec![0.0; border * nb];
  let mut dmat = vec![0.0; border * border];
  let mut r2 = vec![[0.0; 2]; border];
  for &(r, c, v) in trip {
    let (r, c) = (pos[r], pos[c]);
    match (r < nb, c < nb) {
      (true, true) => band[col(r, c)] += v,
      (true, false) => rb[r * nr + (c - nb)] += v,
      (false, true) => cmat[(r - nb) * nb + c] += v,
      (false, false) => dmat[(r - nb) * border + (c - nb)] += v,
    }
  }
  for (u, q) in rhs.iter().enumerate() {
    let i = pos[u];
    if i < nb {
      rb[i * nr + border] = q[0];
      rb[i * nr + border + 1] = q[1];
    } else {
      r2[i - nb] = *q;
    }
  }
  // banded LU with partial pivoting, applied to the right-hand sides as it goes
  for i in 0..nb {
    let last = (i + kl).min(nb - 1);
    let piv = (i..=last).max_by(|&a, &b| band[col(a, i)].abs().total_cmp(&band[col(b, i)].abs()))?;
    if band[col(piv, i)].abs() < 1e-300 {
      return None;
    }
    let hi = (i + ku + kl).min(nb - 1);
    if piv != i {
      for c in i..=hi {
        band.swap(col(i, c), col(piv, c));
      }
      for k in 0..nr {
        rb.swap(i * nr + k, piv * nr + k);
      }
    }
    let d = band[col(i, i)];
    for r in i + 1..=last {
      let m = band[col(r, i)] / d;
      if m == 0.0 {
        continue;
      }
      for c in i..=hi {
        band[col(r, c)] -= m * band[col(i, c)];
      }
      for k in 0..nr {
        rb[r * nr + k] -= m * rb[i * nr + k];
      }
    }
  }
  for i in (0..nb).rev() {
    let hi = (i + ku + kl).min(nb - 1);
    for k in 0..nr {
      let mut s = rb[i * nr + k];
      for c in i + 1..=hi {
        s -= band[col(i, c)] * rb[c * nr + k];
      }
      rb[i * nr + k] = s / band[col(i, i)];
    }
  }
  // border: (D - C A⁻¹B) x₂ = r₂ - C A⁻¹r₁
  let mut x2 = vec![[0.0; 2]; border];
  if border > 0 {
    let k = border;
    let mut s = vec![0.0; k * (k + 2)];
    for a in 0..k {
      for b in 0..k + 2 {
        let mut v = if b < k { dmat[a * k + b] } else { r2[a][b - k] };
        for c in 0..nb {
          v -= cmat[a * nb + c] * rb[c * nr + b];
        }
        s[a * (k + 2) + b] = v;
      }
    }
    let cols = k + 2;
    for i in 0..k {
      let piv = (i..k).max_by(|&a, &b| s[a * cols + i].abs().total_cmp(&s[b * cols + i].abs()))?;
      if s[piv * cols + i].abs() < 1e-300 {
        return None;
      }
      for c in 0..cols {
        s.swap(i * cols + c, piv * cols + c);
      }
      for r in 0..k {
        if r != i {
          let m = s[r * cols + i] / s[i * cols + i];
          for c in 0..cols {
            s[r * cols + c] -= m * s[i * cols + c];
          }
        }
      }
    }
    for (i, x) in x2.iter_mut().enumerate() {
      let d = s[i * cols + i];
      *x = [s[i * cols + k] / d, s[i * cols + k + 1] / d];
    }
  }
  let mut out = vec![[0.0; 2]; size];
  for (u, o) in out.iter_mut().enumerate() {
    let i = pos[u];
    *o = if i < nb {
      let mut v = [rb[i * nr + border], rb[i * nr + border + 1]];
      for (b, xb) in x2.iter().enumerate() {
        v[0] -= rb[i * nr + b] * xb[0];
        v[1] -= rb[i * nr + b] * xb[1];
      }
      v
    } else {
      x2[i - nb]
    };
  }
  Some(out)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn close(a: P, b: P, tol: f64) -> bool {
    crate::dist(a, b) < tol
  }

  /// Poles Fusion 2705 reports for a fitted spline through (0,0), (20,20), (40,0) (in mm).
  #[test]
  fn matches_fusions_three_point_arch() {
    let q = Quintic::fit(&[[0.0, 0.0], [20.0, 20.0], [40.0, 0.0]], false, (None, None)).unwrap();
    let fusion = [
      [0.0, 0.0],
      [0.8, 1.516_952_183_893],
      [2.4, 4.418_381_840_337],
      [4.8, 8.373_114_370_462],
      [8.0, 12.838_085_524_429],
      [12.0, 16.981_227_788_987],
      [16.0, 19.557_779_278_795],
      [20.0, 20.439_894_271_585],
    ];
    for (a, b) in q.poles.iter().zip(fusion) {
      assert!(close(*a, b, 1e-6), "{a:?} vs {b:?}");
    }
    assert_eq!(q.poles.len(), 15);
  }

  /// Fit point parameters follow Fusion's √(chord + 0.22 / gaps) rule (chords in cm).
  #[test]
  fn fit_parameters_match_fusion() {
    let pts = [[0.0, 0.0], [10.0, 12.0], [20.0, 4.0], [30.0, 14.0], [40.0, 6.0]];
    let q = Quintic::fit(&pts, false, (None, None)).unwrap();
    let want = [0.265_192_629_087, 0.506_206_463_471, 0.758_986_165_138];
    for (f, w) in q.fit[1..4].iter().zip(want) {
      assert!((f.0 - w).abs() < 1e-8, "{} vs {w}", f.0);
      assert!(close(q.eval(f.0).0, f.1, 1e-9), "passes through the fit point");
    }
    // Fusion's poles 5 and 12 of this curve
    assert!(close(q.poles[5], [5.420_627_126_505, 12.488_358_992_896], 1e-6), "{:?}", q.poles[5]);
    assert!(close(q.poles[12], [19.756_439_141_373, 3.665_485_036_146], 1e-6), "{:?}", q.poles[12]);
  }

  #[test]
  fn closed_curves_are_periodic() {
    let pts = [[0.0, 0.0], [30.0, -5.0], [35.0, 20.0], [5.0, 25.0]];
    let q = Quintic::fit(&pts, true, (None, None)).unwrap();
    assert_eq!(q.poles.len(), 20);
    let (a, da) = q.eval(0.0);
    let (b, db) = q.eval(1.0 - 1e-12);
    assert!(close(a, b, 1e-6) && close(da, db, 1e-3), "{da:?} {db:?}");
    // Fusion's derivative at the seam (mm per unit parameter)
    assert!(close(da, [70.846_209_41, -98.094_649_92], 1e-5), "{da:?}");
    let (pieces, starts) = q.beziers();
    assert_eq!(starts.len(), 4);
    assert_eq!(pieces.first().unwrap()[0], pieces.last().unwrap()[3]);
    for (s, p) in starts.iter().zip(pts) {
      assert_eq!(pieces[*s][0], p);
    }
  }

  #[test]
  fn handles_set_the_end_tangents() {
    let pts = [[0.0, 0.0], [10.0, 5.0], [20.0, 0.0]];
    let q = Quintic::fit(&pts, false, (Some([0.0, 4.0]), Some([20.0, 3.0]))).unwrap();
    let (_, d0) = q.eval(0.0);
    let (_, d1) = q.eval(1.0);
    assert!(d0[0].abs() < 1e-9 && d0[1] > 0.0, "{d0:?}");
    assert!(d1[0].abs() < 1e-9 && d1[1] < 0.0, "{d1:?}");
    // the handle is the inner control point of a cubic over the first gap
    let gap = q.fit[1].0;
    assert!((d0[1] * gap / 3.0 - 4.0).abs() < 1e-9);
    let (h0, h1) = q.handles().unwrap();
    assert!(close(h0, [0.0, 4.0], 1e-9) && close(h1, [20.0, 3.0], 1e-9), "{h0:?} {h1:?}");
    // a curve fitted with its own handles is the same curve
    let free = Quintic::fit(&pts, false, (None, None)).unwrap();
    let same = Quintic::fit(&pts, false, (free.handles().map(|h| h.0), free.handles().map(|h| h.1))).unwrap();
    for t in [0.1, 0.4, 0.75] {
      assert!(close(free.eval(t).0, same.eval(t).0, 1e-9));
    }
  }

  #[test]
  fn many_points_solve_quickly() {
    let pts: Vec<P> = (0..1000).map(|i| [i as f64, (i as f64 * 0.3).sin() * 5.0]).collect();
    let q = Quintic::fit(&pts, false, (None, None)).unwrap();
    for f in q.fit.iter().step_by(97) {
      assert!(close(q.eval(f.0).0, f.1, 1e-6));
    }
    let q = Quintic::fit(&pts[..300], true, (None, None)).unwrap();
    for f in q.fit.iter().step_by(37) {
      assert!(close(q.eval(f.0).0, f.1, 1e-6));
    }
  }
}
