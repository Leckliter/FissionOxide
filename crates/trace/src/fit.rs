//! Fitting lines, arcs, circles and cubic Béziers to traced boundaries.
//!
//! A contour is sampled at its vertices and at the midpoints of its edges: sample `2i` is vertex
//! `i`, sample `2i + 1` the midpoint after it. Fit errors are measured at the midpoints, which
//! lie on the pixel outline but skip the half-pixel staircase of slanted edges. Every piece runs
//! from one sample to another, so neighbouring segments share bit-identical end points.
//!
//! Per contour: no corners and round → one circle. Otherwise the outline is cut at its corners
//! and each run is fitted: long straight stretches become lines (so lines meet arcs near their
//! tangent points), the rest is tried as a line, an arc through its ends, then a least-squares
//! cubic (Schneider), and split at the worst point until it fits. Neighbouring pieces are then
//! merged where one segment still fits both.

use std::f64::consts::{PI, TAU};

use crate::contour::{self, Raw};
use crate::{Fit, Options, P, Seg};

/// Shortest straight stretch picked out of a curvy run: samples to start with, and length
/// once trimmed (px).
const STRAIGHT_MIN: usize = 32;
const STRAIGHT_LEN: f64 = 15.0;
/// Fewest vertices a contour needs to be considered as a circle.
const CIRCLE_MIN: usize = 16;
/// How far a straight stretch may stray from its chord, px (at most the tolerance).
const STRAIGHT_TOL: f64 = 0.5;
/// How far the ends of a straight stretch may sit off its fitted line, px.
const TRIM: f64 = 0.3;
/// How far two lines meeting at a corner may stray from one line and still be merged, px.
const CORNER_FLAT: f64 = 0.1;
/// Most pieces merged into one segment at a time.
const MERGE_MAX: usize = 5;
/// Least bulge (px) of an arc that takes in a straight edge.
const ARC_MIN_SAGITTA: f64 = 1.0;
/// Samples either side used to estimate a tangent.
const TANGENT_REACH: usize = 4;

pub(crate) fn sub(a: P, b: P) -> P {
  [a[0] - b[0], a[1] - b[1]]
}

pub(crate) fn add(a: P, b: P) -> P {
  [a[0] + b[0], a[1] + b[1]]
}

pub(crate) fn mul(a: P, k: f64) -> P {
  [a[0] * k, a[1] * k]
}

pub(crate) fn dot(a: P, b: P) -> f64 {
  a[0] * b[0] + a[1] * b[1]
}

pub(crate) fn cross(a: P, b: P) -> f64 {
  a[0] * b[1] - a[1] * b[0]
}

pub(crate) fn dist(a: P, b: P) -> f64 {
  (a[0] - b[0]).hypot(a[1] - b[1])
}

fn angle(v: P) -> f64 {
  v[1].atan2(v[0])
}

fn unit(v: P) -> Option<P> {
  let l = v[0].hypot(v[1]);
  (l > 1e-12).then(|| [v[0] / l, v[1] / l])
}

/// Distance from `p` to the segment `a`–`b`.
fn seg_dist(p: P, a: P, b: P) -> f64 {
  let d = sub(b, a);
  let l2 = dot(d, d);
  if l2 == 0.0 {
    return dist(p, a);
  }
  let t = (dot(sub(p, a), d) / l2).clamp(0.0, 1.0);
  dist(p, add(a, mul(d, t)))
}

/// Swept angle (0, 2π) of an arc from `a` to `b` about `center`.
pub(crate) fn arc_sweep(a: P, b: P, center: P, ccw: bool) -> f64 {
  let full = (angle(sub(b, center)) - angle(sub(a, center))).rem_euclid(TAU);
  if ccw { full } else { TAU - full }
}

/// Largest distance of an arc from its chord.
fn sagitta(seg: &Seg) -> f64 {
  match *seg {
    Seg::Arc { a, b, center, ccw } => dist(a, center) * (1.0 - (arc_sweep(a, b, center, ccw) / 2.0).cos()),
    _ => 0.0,
  }
}

pub(crate) fn bezier(c: &[P; 4], u: f64) -> P {
  let v = 1.0 - u;
  let (b0, b1, b2, b3) = (v * v * v, 3.0 * v * v * u, 3.0 * v * u * u, u * u * u);
  [b0 * c[0][0] + b1 * c[1][0] + b2 * c[2][0] + b3 * c[3][0], b0 * c[0][1] + b1 * c[1][1] + b2 * c[2][1] + b3 * c[3][1]]
}

fn bezier_d1(c: &[P; 4], u: f64) -> P {
  let v = 1.0 - u;
  let (k0, k1, k2) = (3.0 * v * v, 6.0 * v * u, 3.0 * u * u);
  let (d0, d1, d2) = (sub(c[1], c[0]), sub(c[2], c[1]), sub(c[3], c[2]));
  [k0 * d0[0] + k1 * d1[0] + k2 * d2[0], k0 * d0[1] + k1 * d1[1] + k2 * d2[1]]
}

fn bezier_d2(c: &[P; 4], u: f64) -> P {
  let a = add(sub(c[2], mul(c[1], 2.0)), c[0]);
  let b = add(sub(c[3], mul(c[2], 2.0)), c[1]);
  [6.0 * ((1.0 - u) * a[0] + u * b[0]), 6.0 * ((1.0 - u) * a[1] + u * b[1])]
}

/// A contour ready for validation: its samples, corner flags and fitted pieces.
pub(crate) struct Prepared {
  pub s: Vec<P>,
  pub corner: Vec<bool>,
  pub hole: bool,
  pub pieces: Vec<Piece>,
}

/// One fitted segment covering samples `i..=j` (indices taken modulo the sample count, `j > i`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Piece {
  pub i: usize,
  pub j: usize,
  pub seg: Seg,
  /// A line found as a straight stretch: never absorbed into a curve.
  pub straight: bool,
}

pub(crate) fn prepare(raw: &Raw, options: &Options) -> Prepared {
  let n = raw.v.len();
  let mut s = Vec::with_capacity(2 * n);
  for i in 0..n {
    let (a, b) = (raw.v[i], raw.v[(i + 1) % n]);
    s.push(a);
    s.push([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]);
  }
  let cv = contour::corners(raw, options.corner_angle);
  let corner: Vec<bool> = (0..2 * n).map(|k| k % 2 == 0 && cv[k / 2]).collect();
  let ctx = Ctx { s: &s, corner: &corner, tol: options.tolerance, fit: options.fit };
  let pieces = ctx.fit_contour(n);
  Prepared { s, corner, hole: raw.hole, pieces }
}

pub(crate) struct Ctx<'a> {
  pub s: &'a [P],
  pub corner: &'a [bool],
  pub tol: f64,
  pub fit: Fit,
}

impl Ctx<'_> {
  fn m(&self) -> usize {
    self.s.len()
  }

  pub(crate) fn at(&self, k: usize) -> P {
    self.s[k % self.s.len()]
  }

  fn is_corner(&self, k: usize) -> bool {
    self.corner[k % self.s.len()]
  }

  fn line(&self, i: usize, j: usize) -> Seg {
    Seg::Line { a: self.at(i), b: self.at(j) }
  }

  /// Midpoint samples strictly between `i` and `j`.
  fn mids(&self, i: usize, j: usize) -> impl Iterator<Item = (usize, P)> + '_ {
    let first = if i.is_multiple_of(2) { i + 1 } else { i + 2 };
    (first..j).step_by(2).map(move |k| (k, self.at(k)))
  }

  /// Largest distance of the midpoints in `(i, j)` from the chord, and where (`i` if none).
  fn line_err(&self, i: usize, j: usize) -> (f64, usize) {
    let (a, b) = (self.at(i), self.at(j));
    let mut worst = (0.0, i);
    for (k, p) in self.mids(i, j) {
      let d = seg_dist(p, a, b);
      if d > worst.0 {
        worst = (d, k);
      }
    }
    worst
  }

  fn fit_contour(&self, vertices: usize) -> Vec<Piece> {
    let m = self.m();
    let cs: Vec<usize> = (0..m).filter(|&k| self.corner[k]).collect();
    let mut pieces = Vec::new();
    if cs.is_empty() {
      if self.fit != Fit::Lines
        && vertices >= CIRCLE_MIN
        && let Some(seg) = self.circle()
      {
        return vec![Piece { i: 0, j: m, seg, straight: false }];
      }
      self.fit_run(0, m, &mut pieces);
    } else {
      for (a, &c) in cs.iter().enumerate() {
        let next = cs.get(a + 1).copied().unwrap_or(cs[0] + m);
        self.fit_run(c, next, &mut pieces);
      }
    }
    self.merge(&mut pieces);
    pieces
  }

  /// Douglas–Peucker on `i..=j` within `tol`, as sample ranges. A whole loop is cut in three
  /// first so it never collapses to a point.
  pub(crate) fn dp_ranges(&self, i: usize, j: usize, tol: f64) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut stack = Vec::new();
    if j - i >= self.m() {
      let third = (j - i) / 3;
      stack.extend([(i + 2 * third, j), (i + third, i + 2 * third), (i, i + third)]);
    } else {
      stack.push((i, j));
    }
    while let Some((i, j)) = stack.pop() {
      let (e, w) = self.line_err(i, j);
      if e <= tol || w == i {
        out.push((i, j));
      } else {
        stack.push((w, j));
        stack.push((i, w));
      }
    }
    out
  }

  /// The exact pixel outline of `i..=j` as lines, collinear points dropped.
  pub(crate) fn raw(&self, i: usize, j: usize) -> Vec<Seg> {
    let first = if i.is_multiple_of(2) { i + 2 } else { i + 1 };
    let mut pts: Vec<P> = vec![self.at(i)];
    for p in (first..j).step_by(2).map(|k| self.at(k)).chain([self.at(j)]) {
      while let [.., a, b] = pts[..] {
        let (u, v) = (sub(b, a), sub(p, b));
        if cross(u, v) == 0.0 && dot(u, v) > 0.0 {
          pts.pop();
        } else {
          break;
        }
      }
      pts.push(p);
    }
    pts.windows(2).map(|w| Seg::Line { a: w[0], b: w[1] }).collect()
  }

  /// Fits the run between two corners (or a whole corner-less loop).
  fn fit_run(&self, i: usize, j: usize, out: &mut Vec<Piece>) {
    let m = self.m();
    if self.fit == Fit::Lines {
      out.extend(self.dp_ranges(i, j, self.tol).into_iter().map(|(a, b)| Piece {
        i: a,
        j: b,
        seg: self.line(a, b),
        straight: false,
      }));
      return;
    }
    if j - i < m && self.line_err(i, j).0 <= self.tol {
      out.push(Piece { i, j, seg: self.line(i, j), straight: true });
      return;
    }
    // long straight stretches become lines; what lies between is curvy
    let (mut cur, mut k) = (i, i);
    while k + STRAIGHT_MIN <= j {
      match self.straight(k, j) {
        Some((a, e)) => {
          if a > cur {
            self.curvy(cur, a, out);
          }
          out.push(Piece { i: a, j: e, seg: self.line(a, e), straight: true });
          (cur, k) = (e, e);
        }
        None => k += 1,
      }
    }
    if cur < j {
      self.curvy(cur, j, out);
    }
  }

  /// A straight stretch starting near `k` (before `j`): grown while within [`STRAIGHT_TOL`] of
  /// its chord, trimmed so its ends lie on its least-squares line, and rejected if an arc fits
  /// it much better (a gentle curve rather than a straight edge).
  fn straight(&self, k: usize, j: usize) -> Option<(usize, usize)> {
    let e = self.grow(k, j, self.tol.min(STRAIGHT_TOL))?;
    let (a, e, ls_rms) = self.trim(k, e);
    if dist(self.at(a), self.at(e)) < STRAIGHT_LEN {
      return None;
    }
    if ls_rms > 0.1
      && let Some((Seg::Arc { center, .. }, _)) = self.arc(a, e)
    {
      let r = dist(self.at(a), center);
      let n = self.mids(a, e).count().max(1) as f64;
      let arc_rms = (self.mids(a, e).map(|(_, p)| (dist(p, center) - r).powi(2)).sum::<f64>() / n).sqrt();
      if arc_rms < 0.6 * ls_rms {
        return None;
      }
    }
    Some((a, e))
  }

  /// The furthest end of a stretch from `k` within `ts` of its chord, at least
  /// [`STRAIGHT_MIN`] samples long and short of a whole loop.
  fn grow(&self, k: usize, j: usize, ts: f64) -> Option<usize> {
    let lim = j.min(k + self.m() - 1);
    let ok = |e: usize| self.line_err(k, e).0 <= ts;
    let mut lo = k + STRAIGHT_MIN;
    if lo > lim || !ok(lo) {
      return None;
    }
    let mut step = STRAIGHT_MIN;
    let mut hi = None;
    while lo < lim {
      let t = (lo + step).min(lim);
      if ok(t) {
        lo = t;
        step *= 2;
      } else {
        hi = Some(t);
        break;
      }
    }
    if let Some(mut hi) = hi {
      while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if ok(mid) { lo = mid } else { hi = mid }
      }
    }
    Some(lo)
  }

  /// Shrinks a straight stretch until both ends lie on its least-squares line (within
  /// [`TRIM`] px), so a side running into a rounded corner isn't tilted by the corner. Also
  /// returns the RMS distance of the midpoints from that line.
  fn trim(&self, mut a: usize, mut e: usize) -> (usize, usize, f64) {
    let pts: Vec<P> = self.mids(a, e).map(|(_, p)| p).collect();
    if pts.len() < 2 {
      return (a, e, 0.0);
    }
    let n = pts.len() as f64;
    let c = pts.iter().fold([0.0, 0.0], |acc, p| add(acc, mul(*p, 1.0 / n)));
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for p in &pts {
      let d = sub(*p, c);
      sxx += d[0] * d[0];
      sxy += d[0] * d[1];
      syy += d[1] * d[1];
    }
    let th = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let nrm = [-th.sin(), th.cos()];
    let off = |k: usize| dot(nrm, sub(self.at(k), c)).abs();
    while e > a && off(a) > TRIM && !self.is_corner(a) {
      a += 1;
    }
    while e > a && off(e) > TRIM && !self.is_corner(e) {
      e -= 1;
    }
    let rms = (pts.iter().map(|p| dot(nrm, sub(*p, c)).powi(2)).sum::<f64>() / n).sqrt();
    (a, e, rms)
  }

  fn curvy(&self, i: usize, j: usize, out: &mut Vec<Piece>) {
    if j - i >= self.m() {
      let third = (j - i) / 3;
      self.split_fit(i, i + third, out);
      self.split_fit(i + third, i + 2 * third, out);
      self.split_fit(i + 2 * third, j, out);
    } else {
      self.split_fit(i, j, out);
    }
  }

  /// Line, else arc, else cubic; split at the worst point until each piece fits.
  fn split_fit(&self, i: usize, j: usize, out: &mut Vec<Piece>) {
    let mut stack = vec![(i, j)];
    while let Some((i, j)) = stack.pop() {
      match self.fit_one(i, j) {
        Ok(seg) => out.push(Piece { i, j, seg, straight: false }),
        Err(w) => {
          stack.push((w, j));
          stack.push((i, w));
        }
      }
    }
  }

  /// One segment for `i..=j` within tolerance, or where to split.
  fn fit_one(&self, i: usize, j: usize) -> Result<Seg, usize> {
    let (le, lw) = self.line_err(i, j);
    if le <= self.tol || j - i <= 2 {
      return Ok(self.line(i, j));
    }
    if let Some((seg, e)) = self.arc(i, j)
      && e <= self.tol
    {
      return Ok(seg);
    }
    let mut w = lw;
    if self.fit == Fit::Curves {
      let (seg, e, cw) = self.cubic(i, j);
      if e <= self.tol {
        return Ok(seg);
      }
      w = cw;
    }
    let q = ((j - i) / 4).max(1);
    Err(w.clamp(i + q, j - q))
  }

  /// An arc through the samples `i` and `j` fitted to the midpoints between: the centre lies
  /// on the chord's bisector, and the algebraic error is linear in its offset there.
  fn arc(&self, i: usize, j: usize) -> Option<(Seg, f64)> {
    let (a, b) = (self.at(i), self.at(j));
    let half = mul(sub(b, a), 0.5);
    let h2 = dot(half, half);
    if h2 < 1e-12 {
      return None;
    }
    let (mid, h) = (add(a, half), h2.sqrt());
    let nrm = [-half[1] / h, half[0] / h];
    let (mut num, mut den, mut count) = (0.0, 0.0, 0);
    for (_, p) in self.mids(i, j) {
      let d = sub(p, mid);
      let g = dot(nrm, d);
      num += (dot(d, d) - h2) * g;
      den += g * g;
      count += 1;
    }
    if count == 0 || den < 1e-12 {
      return None;
    }
    let t = num / (2.0 * den);
    if !t.is_finite() || t.abs() > 1e6 {
      return None;
    }
    let center = add(mid, mul(nrm, t));
    let r = (h2 + t * t).sqrt();
    let a0 = angle(sub(a, center));
    let full = (angle(sub(b, center)) - a0).rem_euclid(TAU);
    let probe = self.at(i + (j - i) / 2);
    let ccw = (angle(sub(probe, center)) - a0).rem_euclid(TAU) < full;
    let sweep = if ccw { full } else { TAU - full };
    if !(sweep > 1e-6) || sweep > 1.9 * PI {
      return None;
    }
    let mut err = 0.0f64;
    for (_, p) in self.mids(i, j) {
      let rel = sub(p, center);
      let ang = if ccw { (angle(rel) - a0).rem_euclid(TAU) } else { (a0 - angle(rel)).rem_euclid(TAU) };
      // a point beyond the arc's ends is as far off as its distance to the nearer end
      let e = if ang <= sweep { (rel[0].hypot(rel[1]) - r).abs() } else { dist(p, a).min(dist(p, b)) };
      err = err.max(e);
    }
    Some((Seg::Arc { a, b, center, ccw }, err))
  }

  /// A least-squares circle (Kåsa) through the midpoints of the whole contour.
  fn circle(&self) -> Option<Seg> {
    let pts: Vec<P> = (1..self.m()).step_by(2).map(|k| self.s[k]).collect();
    let n = pts.len() as f64;
    let c0 = pts.iter().fold([0.0, 0.0], |acc, p| add(acc, mul(*p, 1.0 / n)));
    let (mut sxx, mut sxy, mut syy, mut sxz, mut syz) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for p in &pts {
      let (x, y) = (p[0] - c0[0], p[1] - c0[1]);
      let z = x * x + y * y;
      sxx += x * x;
      sxy += x * y;
      syy += y * y;
      sxz += x * z;
      syz += y * z;
    }
    let det = sxx * syy - sxy * sxy;
    if !(det.abs() > 1e-12) {
      return None;
    }
    let d = -(sxz * syy - syz * sxy) / det;
    let e = -(sxx * syz - sxy * sxz) / det;
    let center = [c0[0] - d / 2.0, c0[1] - e / 2.0];
    let radius = pts.iter().map(|p| dist(*p, center)).sum::<f64>() / n;
    let err = pts.iter().map(|p| (dist(*p, center) - radius).abs()).fold(0.0, f64::max);
    (radius.is_finite() && err <= self.tol).then_some(Seg::Circle { center, radius })
  }

  /// Unit tangent at `i` pointing into the run `i..j`.
  fn tangent_out(&self, i: usize, j: usize) -> P {
    let p = self.at(i);
    let d = if self.is_corner(i) {
      (1..=TANGENT_REACH.min(j - i)).fold([0.0, 0.0], |acc, q| add(acc, sub(self.at(i + q), p)))
    } else {
      sub(self.at(i + TANGENT_REACH), self.at(i + self.m() - TANGENT_REACH))
    };
    unit(d).or_else(|| unit(sub(self.at(j), p))).unwrap_or([1.0, 0.0])
  }

  /// Unit tangent at `j` pointing back into the run `i..j`.
  fn tangent_in(&self, i: usize, j: usize) -> P {
    let p = self.at(j);
    let d = if self.is_corner(j) {
      (1..=TANGENT_REACH.min(j - i)).fold([0.0, 0.0], |acc, q| add(acc, sub(self.at(j - q), p)))
    } else {
      sub(self.at(j + self.m() - TANGENT_REACH), self.at(j + TANGENT_REACH))
    };
    unit(d).or_else(|| unit(sub(self.at(i), p))).unwrap_or([-1.0, 0.0])
  }

  /// A cubic for `i..=j`, its error and the sample where it is worst.
  fn cubic(&self, i: usize, j: usize) -> (Seg, f64, usize) {
    let mut q = vec![self.at(i)];
    let mut idx = vec![i];
    for (k, p) in self.mids(i, j) {
      q.push(p);
      idx.push(k);
    }
    q.push(self.at(j));
    idx.push(j);
    let (c, err, w) = schneider(&q, self.tangent_out(i, j), self.tangent_in(i, j), self.tol);
    (Seg::Cubic { a: c[0], c1: c[1], c2: c[2], b: c[3] }, err, idx.get(w).copied().unwrap_or(i))
  }

  /// Merges neighbouring pieces (two at a time, else up to [`MERGE_MAX`]) where one segment
  /// fits them all:
  /// lines with lines anywhere, otherwise only where they meet away from a corner.
  fn merge(&self, pieces: &mut Vec<Piece>) {
    let m = self.m();
    loop {
      let mut changed = false;
      let mut k = 0;
      while k < pieces.len() {
        let mut merged = false;
        for n in 2..=MERGE_MAX {
          if pieces.len() <= n {
            break;
          }
          let group: Vec<usize> = (k..k + n).map(|g| g % pieces.len()).collect();
          let run: Vec<Piece> = group.iter().map(|&g| pieces[g]).collect();
          let i = run[0].i;
          let j = i + run.iter().map(|p| p.j - p.i).sum::<usize>();
          if j - i >= m {
            continue;
          }
          if let Some(seg) = self.union(i, j, &run) {
            let straight = matches!(seg, Seg::Line { .. }) && run.iter().any(|p| p.straight);
            pieces[k] = Piece { i, j, seg, straight };
            let mut rest: Vec<usize> = group[1..].to_vec();
            rest.sort_unstable_by(|a, b| b.cmp(a));
            for g in rest {
              pieces.remove(g);
            }
            (merged, changed) = (true, true);
            break;
          }
        }
        if !merged {
          k += 1;
        }
      }
      if !changed {
        break;
      }
    }
  }

  /// One segment for the consecutive pieces `run`, covering samples `i..=j`, if it fits.
  fn union(&self, i: usize, j: usize, run: &[Piece]) -> Option<Seg> {
    let is_line = |p: &Piece| matches!(p.seg, Seg::Line { .. });
    let corner = run[..run.len() - 1].iter().any(|p| self.is_corner(p.j));
    let edge = run.iter().any(|p| is_line(p) && p.straight);
    if run.iter().all(is_line) {
      // across a corner only if the corner is all but flat (keeps diagonal-touch chamfers);
      // a straight edge only takes in what keeps it straight, so it doesn't tilt
      let tol = if corner {
        self.tol.min(CORNER_FLAT)
      } else if edge {
        self.tol.min(TRIM)
      } else {
        self.tol
      };
      if self.line_err(i, j).0 <= tol {
        return Some(self.line(i, j));
      }
    }
    if corner || self.fit == Fit::Lines {
      return None;
    }
    if run.iter().all(|p| matches!(p.seg, Seg::Line { .. } | Seg::Arc { .. }))
      && let Some((seg, e)) = self.arc(i, j)
    {
      // a straight edge joins an arc only where the arc bends clearly more than it strays
      // (chains of lines on a large circle), never to bend a side into a corner
      let ok = e <= self.tol && (!edge || sagitta(&seg) >= ARC_MIN_SAGITTA.max(2.0 * e));
      if ok {
        return Some(seg);
      }
    }
    if self.fit == Fit::Curves && run.iter().all(|p| matches!(p.seg, Seg::Cubic { .. })) {
      let (seg, e, _) = self.cubic(i, j);
      if e <= self.tol {
        return Some(seg);
      }
    }
    None
  }
}

/// Least-squares cubic from `q[0]` to its last point with the given end tangents (`t2` points
/// back from the end), refined by Newton reparameterisation. Returns the control points, the
/// largest distance of a point from the curve at its parameter, and that point's index.
pub(crate) fn schneider(q: &[P], t1: P, t2: P, tol: f64) -> ([P; 4], f64, usize) {
  let (a, b) = (q[0], q[q.len() - 1]);
  let mut u: Vec<f64> = Vec::with_capacity(q.len());
  let mut acc = 0.0;
  u.push(0.0);
  for w in q.windows(2) {
    acc += dist(w[0], w[1]);
    u.push(acc);
  }
  if acc > 0.0 {
    u.iter_mut().for_each(|v| *v /= acc);
  }
  let seg = dist(a, b).max(acc / 3.0);
  let mut best = ([a, a, b, b], f64::INFINITY, 0);
  for _ in 0..5 {
    let c = solve_alphas(q, &u, t1, t2, seg);
    let (mut err, mut worst) = (0.0, 0);
    for (k, (p, &uk)) in q.iter().zip(&u).enumerate() {
      let e = dist(bezier(&c, uk), *p);
      if e > err {
        (err, worst) = (e, k);
      }
    }
    if err < best.1 {
      best = (c, err, worst);
    }
    if err <= tol {
      break;
    }
    for (p, uk) in q.iter().zip(u.iter_mut()) {
      let d = sub(bezier(&c, *uk), *p);
      let (d1, d2) = (bezier_d1(&c, *uk), bezier_d2(&c, *uk));
      let den = dot(d1, d1) + dot(d, d2);
      if den.abs() > 1e-12 {
        *uk = (*uk - dot(d, d1) / den).clamp(0.0, 1.0);
      }
    }
  }
  best
}

fn solve_alphas(q: &[P], u: &[f64], t1: P, t2: P, seg: f64) -> [P; 4] {
  let (a, b) = (q[0], q[q.len() - 1]);
  let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
  for (p, &uk) in q.iter().zip(u) {
    let v = 1.0 - uk;
    let (b0, b1, b2, b3) = (v * v * v, 3.0 * v * v * uk, 3.0 * v * uk * uk, uk * uk * uk);
    let (a1, a2) = (mul(t1, b1), mul(t2, b2));
    let tmp = sub(*p, add(mul(a, b0 + b1), mul(b, b2 + b3)));
    c00 += dot(a1, a1);
    c01 += dot(a1, a2);
    c11 += dot(a2, a2);
    x0 += dot(a1, tmp);
    x1 += dot(a2, tmp);
  }
  let det = c00 * c11 - c01 * c01;
  let (mut al1, mut al2) = (seg / 3.0, seg / 3.0);
  if det.abs() > 1e-12 {
    let (s1, s2) = ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det);
    let eps = 1e-6 * seg;
    if s1 > eps && s2 > eps && s1 < 3.0 * seg && s2 < 3.0 * seg {
      (al1, al2) = (s1, s2);
    }
  }
  [a, add(a, mul(t1, al1)), add(b, mul(t2, al2)), b]
}
