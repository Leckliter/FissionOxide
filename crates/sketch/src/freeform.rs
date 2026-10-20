//! Freeform curves (fit and control-point splines, ellipses, Bézier chains) in the modify tools:
//! Bézier math, intersections between any two curves, Break, and Trim / Extend / Offset of
//! freeform curves, plus Blend Curve.
//!
//! A freeform curve is handled as its chain of cubic Bézier pieces, parameterised 0..n (piece i
//! covers i..i+1). What a tool leaves of one is a [`CurveKind::Bezier`] chain, which keeps the
//! exact shape: Bézier pieces split exactly.

use std::f64::consts::{PI, TAU};

use crate::edit::{Geo, meet};
use crate::{ConstraintKind, CurveId, CurveKind, PointId, Sketch, SketchError, angle_of, ccw_sweep, dist};

type P = [f64; 2];
pub(crate) type Bz = [P; 4];

const EPS: f64 = 1e-9;
/// Samples per Bézier piece when looking for crossings and nearest points.
const SAMPLES: usize = 32;
/// Offset curves are refined until they are this close to the true offset (mm).
const OFFSET_TOL: f64 = 1e-5;

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

fn unit(a: P) -> Option<P> {
  let l = len(a);
  (l > 1e-15).then(|| mul(a, 1.0 / l))
}

fn left(a: P) -> P {
  [-a[1], a[0]]
}

fn lerp(a: P, b: P, t: f64) -> P {
  add(a, mul(sub(b, a), t))
}

// ---------------------------------------------------------------- Bézier pieces

pub(crate) fn bz_at(b: &Bz, t: f64) -> P {
  crate::bezier_at(b, t)
}

/// First derivative.
pub(crate) fn bz_d1(b: &Bz, t: f64) -> P {
  let u = 1.0 - t;
  let (a, c, d) = (sub(b[1], b[0]), sub(b[2], b[1]), sub(b[3], b[2]));
  add(add(mul(a, 3.0 * u * u), mul(c, 6.0 * u * t)), mul(d, 3.0 * t * t))
}

/// Second derivative.
pub(crate) fn bz_d2(b: &Bz, t: f64) -> P {
  let a = add(sub(b[2], mul(b[1], 2.0)), b[0]);
  let c = add(sub(b[3], mul(b[2], 2.0)), b[1]);
  add(mul(a, 6.0 * (1.0 - t)), mul(c, 6.0 * t))
}

/// Splits a piece at `t` (de Casteljau; `t` outside 0..1 extrapolates the cubic).
pub(crate) fn bz_split(b: &Bz, t: f64) -> (Bz, Bz) {
  let (p01, p12, p23) = (lerp(b[0], b[1], t), lerp(b[1], b[2], t), lerp(b[2], b[3], t));
  let (p012, p123) = (lerp(p01, p12, t), lerp(p12, p23, t));
  let m = lerp(p012, p123, t);
  ([b[0], p01, p012, m], [m, p123, p23, b[3]])
}

/// The part of a piece from `t0` to `t1` (either may lie outside 0..1).
pub(crate) fn bz_sub(b: &Bz, t0: f64, t1: f64) -> Bz {
  if t1.abs() >= (1.0 - t0).abs() {
    // the piece 0..t1, then its part from t0
    if t1.abs() < 1e-15 {
      return [b[0]; 4];
    }
    let (l, _) = bz_split(b, t1);
    bz_split(&l, t0 / t1).1
  } else {
    // the piece t0..1, then its part up to t1
    let (_, r) = bz_split(b, t0);
    bz_split(&r, (t1 - t0) / (1.0 - t0)).0
  }
}

fn bz_rev(b: &Bz) -> Bz {
  [b[3], b[2], b[1], b[0]]
}

fn bbox(b: &Bz) -> (P, P) {
  let mut lo = b[0];
  let mut hi = b[0];
  for p in &b[1..] {
    lo = [lo[0].min(p[0]), lo[1].min(p[1])];
    hi = [hi[0].max(p[0]), hi[1].max(p[1])];
  }
  (lo, hi)
}

fn overlap(a: (P, P), b: (P, P), pad: f64) -> bool {
  a.0[0] <= b.1[0] + pad && b.0[0] <= a.1[0] + pad && a.0[1] <= b.1[1] + pad && b.0[1] <= a.1[1] + pad
}

/// Signed curvature (positive turning left) at `t`.
pub(crate) fn bz_curvature(b: &Bz, t: f64) -> f64 {
  let (d1, d2) = (bz_d1(b, t), bz_d2(b, t));
  let l = len(d1);
  if l < 1e-12 { 0.0 } else { cross(d1, d2) / (l * l * l) }
}

// ---------------------------------------------------------------- chains of pieces

/// Piece index and local parameter of chain parameter `u`.
fn piece(n: usize, u: f64) -> (usize, f64) {
  let i = (u.floor().max(0.0) as usize).min(n.saturating_sub(1));
  (i, u - i as f64)
}

pub(crate) fn path_at(path: &[Bz], u: f64) -> P {
  let (i, t) = piece(path.len(), u);
  path.get(i).map(|b| bz_at(b, t)).unwrap_or([0.0, 0.0])
}

pub(crate) fn path_d1(path: &[Bz], u: f64) -> P {
  let (i, t) = piece(path.len(), u);
  path.get(i).map(|b| bz_d1(b, t)).unwrap_or([1.0, 0.0])
}

pub(crate) fn path_closed(path: &[Bz]) -> bool {
  match (path.first(), path.last()) {
    (Some(a), Some(b)) => dist(a[0], b[3]) < 1e-9,
    _ => false,
  }
}

fn path_rev(path: &[Bz]) -> Vec<Bz> {
  path.iter().rev().map(bz_rev).collect()
}

/// The chain between parameters `u0` < `u1`; on a closed chain `u1` may run past the end and wrap.
pub(crate) fn path_sub(path: &[Bz], u0: f64, u1: f64) -> Vec<Bz> {
  let n = path.len();
  let mut out = Vec::new();
  if n == 0 || !(u1 > u0) {
    return out;
  }
  let mut k = u0.floor().max(0.0) as usize;
  while (k as f64) < u1 - 1e-12 && out.len() <= 2 * n + 1 {
    let (t0, t1) = ((u0 - k as f64).max(0.0), (u1 - k as f64).min(1.0));
    if t1 - t0 > 1e-12 {
      let b = &path[k % n];
      out.push(if t0 <= 0.0 && t1 >= 1.0 { *b } else { bz_sub(b, t0, t1) });
    }
    k += 1;
  }
  out
}

/// Chain parameter of the point of the chain nearest `p`.
pub(crate) fn path_param(path: &[Bz], p: P) -> f64 {
  let mut best = (f64::INFINITY, 0.0);
  for (i, b) in path.iter().enumerate() {
    for k in 0..=SAMPLES {
      let t = k as f64 / SAMPLES as f64;
      let d = dist(bz_at(b, t), p);
      if d < best.0 {
        best = (d, i as f64 + t);
      }
    }
  }
  // Newton on d/dt |B(t) - p|² within the piece (and its neighbour when it runs to an end)
  let (i, t) = piece(path.len(), best.1);
  let newton = |i: usize, mut t: f64| -> (f64, f64) {
    let Some(b) = path.get(i) else { return (f64::INFINITY, 0.0) };
    for _ in 0..12 {
      let (q, d1, d2) = (sub(bz_at(b, t), p), bz_d1(b, t), bz_d2(b, t));
      let f1 = dot(q, d1);
      let f2 = dot(d1, d1) + dot(q, d2);
      if f2.abs() < 1e-15 {
        break;
      }
      let tn = (t - f1 / f2).clamp(0.0, 1.0);
      let done = (tn - t).abs() < 1e-14;
      t = tn;
      if done {
        break;
      }
    }
    (dist(bz_at(b, t), p), i as f64 + t)
  };
  let mut found = newton(i, t);
  let (_, local) = piece(path.len(), found.1);
  let n = path.len();
  let closed = path_closed(path);
  let neighbour = if local <= 1e-12 {
    if i > 0 {
      Some((i - 1, 1.0))
    } else if closed {
      Some((n - 1, 1.0))
    } else {
      None
    }
  } else if local >= 1.0 - 1e-12 {
    if i + 1 < n {
      Some((i + 1, 0.0))
    } else if closed {
      Some((0, 0.0))
    } else {
      None
    }
  } else {
    None
  };
  if let Some((j, tj)) = neighbour {
    let other = newton(j, tj);
    if other.0 < found.0 - 1e-15 {
      found = other;
    }
  }
  found.1
}

/// Signed distance from a chain to `p` (positive on the left of its direction).
pub(crate) fn path_distance(path: &[Bz], p: P) -> f64 {
  let u = path_param(path, p);
  let c = path_at(path, u);
  match unit(path_d1(path, u)) {
    Some(t) => cross(t, sub(p, c)),
    None => dist(p, c),
  }
}

/// Where chain `path` meets a line (as an infinite line) or an arc (as its whole circle):
/// (chain parameter, point).
pub(crate) fn path_meets_geo(path: &[Bz], g: &Geo) -> Vec<(f64, P)> {
  let f = |p: P| -> f64 {
    match *g {
      Geo::Line { a, b } => unit(sub(b, a)).map(|d| cross(d, sub(p, a))).unwrap_or(0.0),
      Geo::Arc { c, r, .. } => dist(c, p) - r,
    }
  };
  let cull = match *g {
    Geo::Arc { c, r, .. } => Some(([c[0] - r, c[1] - r], [c[0] + r, c[1] + r])),
    Geo::Line { .. } => None,
  };
  let mut out: Vec<(f64, P)> = Vec::new();
  // the chain's own ends count when they lie on the curve to within rounding
  let snap = |v: f64, end: bool| if end && v.abs() < 1e-10 { 0.0 } else { v };
  let n = path.len();
  for (i, b) in path.iter().enumerate() {
    if cull.is_some_and(|c| !overlap(bbox(b), c, 1e-6)) {
      continue;
    }
    let mut prev = (0.0, snap(f(b[0]), i == 0));
    for k in 1..=SAMPLES {
      let t = k as f64 / SAMPLES as f64;
      let v = snap(f(bz_at(b, t)), k == SAMPLES && i + 1 == n);
      let root = if prev.1 == 0.0 {
        Some(prev.0)
      } else if prev.1 * v < 0.0 {
        // bisection keeps the bracket; a few steps of it are exact enough
        let (mut lo, mut hi, mut flo) = (prev.0, t, prev.1);
        for _ in 0..60 {
          let m = 0.5 * (lo + hi);
          let fm = f(bz_at(b, m));
          if (fm < 0.0) == (flo < 0.0) {
            lo = m;
            flo = fm;
          } else {
            hi = m;
          }
        }
        Some(0.5 * (lo + hi))
      } else {
        None
      };
      if let Some(t) = root {
        let p = bz_at(b, t);
        if !out.iter().any(|(_, q)| dist(*q, p) < 1e-7) {
          out.push((i as f64 + t, p));
        }
      }
      prev = (t, v);
    }
    if prev.1 == 0.0 {
      let p = b[3];
      if !out.iter().any(|(_, q)| dist(*q, p) < 1e-7) {
        out.push((i as f64 + 1.0, p));
      }
    }
  }
  out
}

/// Where two chains cross: (parameter on `a`, point).
pub(crate) fn path_meets_path(a: &[Bz], b: &[Bz]) -> Vec<(f64, P)> {
  let mut out: Vec<(f64, P)> = Vec::new();
  const N: usize = 24;
  for (i, pa) in a.iter().enumerate() {
    let ba = bbox(pa);
    for pb in b {
      if !overlap(ba, bbox(pb), 1e-6) {
        continue;
      }
      let sa: Vec<P> = (0..=N).map(|k| bz_at(pa, k as f64 / N as f64)).collect();
      let sb: Vec<P> = (0..=N).map(|k| bz_at(pb, k as f64 / N as f64)).collect();
      for ka in 0..N {
        for kb in 0..N {
          let (p, r) = (sa[ka], sub(sa[ka + 1], sa[ka]));
          let (q, s) = (sb[kb], sub(sb[kb + 1], sb[kb]));
          let den = cross(r, s);
          if den.abs() < 1e-18 {
            continue;
          }
          let (t, w) = (cross(sub(q, p), s) / den, cross(sub(q, p), r) / den);
          if !(-0.05..=1.05).contains(&t) || !(-0.05..=1.05).contains(&w) {
            continue;
          }
          // refine A(t1) = B(t2) by Newton
          let (mut t1, mut t2) = ((ka as f64 + t) / N as f64, (kb as f64 + w) / N as f64);
          let mut ok = false;
          for _ in 0..30 {
            let f = sub(bz_at(pa, t1), bz_at(pb, t2));
            if len(f) < 1e-12 {
              ok = true;
              break;
            }
            let (da, db) = (bz_d1(pa, t1), mul(bz_d1(pb, t2), -1.0));
            let det = cross(da, db);
            if det.abs() < 1e-18 {
              break;
            }
            t1 -= cross(f, db) / det;
            t2 -= cross(da, f) / det;
          }
          let inside = (-1e-9..=1.0 + 1e-9).contains(&t1) && (-1e-9..=1.0 + 1e-9).contains(&t2);
          if ok && inside {
            let t1 = t1.clamp(0.0, 1.0);
            let p = bz_at(pa, t1);
            if !out.iter().any(|(_, x)| dist(*x, p) < 1e-7) {
              out.push((i as f64 + t1, p));
            }
          }
        }
      }
    }
  }
  out
}

// ---------------------------------------------------------------- any curve

/// A curve's geometry: exact lines, arcs and circles, or a Bézier chain.
#[derive(Debug, Clone)]
pub(crate) enum Shape {
  Geo(Geo),
  Free(Vec<Bz>),
}

impl Shape {
  pub(crate) fn param(&self, p: P) -> f64 {
    match self {
      Shape::Geo(g) => g.param(p),
      Shape::Free(path) => path_param(path, p),
    }
  }

  pub(crate) fn at(&self, u: f64) -> P {
    match self {
      Shape::Geo(g) => g.at(u),
      Shape::Free(path) => path_at(path, u),
    }
  }

  pub(crate) fn end_param(&self) -> f64 {
    match self {
      Shape::Geo(g) => g.end_param(),
      Shape::Free(path) => path.len() as f64,
    }
  }

  pub(crate) fn closed(&self) -> bool {
    match self {
      Shape::Geo(g) => g.full(),
      Shape::Free(path) => path_closed(path),
    }
  }
}

/// Where two curves cross within both their extents: (parameter on `g`, point).
pub(crate) fn intersect(g: &Shape, h: &Shape) -> Vec<(f64, P)> {
  match (g, h) {
    (Shape::Geo(g), Shape::Geo(h)) => {
      meet(g, h).into_iter().filter(|p| g.contains(*p) && h.contains(*p)).map(|p| (g.param(p), p)).collect()
    }
    (Shape::Free(a), Shape::Geo(h)) => path_meets_geo(a, h).into_iter().filter(|(_, p)| h.contains(*p)).collect(),
    (Shape::Geo(g), Shape::Free(b)) => {
      path_meets_geo(b, g).into_iter().filter(|(_, p)| g.contains(*p)).map(|(_, p)| (g.param(p), p)).collect()
    }
    (Shape::Free(a), Shape::Free(b)) => path_meets_path(a, b),
  }
}

impl Sketch {
  /// Any curve's geometry (None for a missing or empty curve).
  pub(crate) fn shape(&self, id: CurveId) -> Option<Shape> {
    if let Some(g) = self.geo(id) {
      return Some(Shape::Geo(g));
    }
    if let Some(path) = self.elliptic_path(id) {
      return Some(Shape::Free(path));
    }
    let bz = self.beziers(id);
    (!bz.is_empty()).then_some(Shape::Free(bz))
  }

  /// Where other curves cross curve `id`: (parameter along it, the other curve), in order.
  pub(crate) fn crossings_any(&self, id: CurveId) -> Result<Vec<(f64, CurveId)>, SketchError> {
    let g = self.shape(id).ok_or(SketchError::NoCurve(id))?;
    let mut out = Vec::new();
    for c in &self.curves {
      if c.id == id {
        continue;
      }
      let Some(h) = self.shape(c.id) else { continue };
      for (t, _) in intersect(&g, &h) {
        out.push((t, c.id));
      }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(out)
  }

  /// Where a line (as an infinite line) or an arc (as its whole circle) meets other curves within
  /// their extents.
  pub(crate) fn hits_extended(&self, g: &Geo, skip: CurveId) -> Vec<(P, CurveId)> {
    let mut out = Vec::new();
    for c in &self.curves {
      if c.id == skip {
        continue;
      }
      match self.shape(c.id) {
        Some(Shape::Geo(h)) => out.extend(meet(g, &h).into_iter().filter(|p| h.contains(*p)).map(|p| (p, c.id))),
        Some(Shape::Free(path)) => out.extend(path_meets_geo(&path, g).into_iter().map(|(_, p)| (p, c.id))),
        None => {}
      }
    }
    out
  }

  /// The two end points of an open curve (lines, arcs, splines, Bézier chains).
  pub(crate) fn ends(&self, id: CurveId) -> Option<(PointId, PointId)> {
    match &self.curve(id)?.kind {
      CurveKind::Line { a, b } => Some((*a, *b)),
      CurveKind::Arc { start, end, .. } => Some((*start, *end)),
      CurveKind::Spline { points, .. } | CurveKind::ControlSpline { points } | CurveKind::Bezier { points } => {
        Some((*points.first()?, *points.last()?))
      }
      CurveKind::EllipticalArc { start, end, .. } => Some((*start, *end)),
      CurveKind::Circle { .. } | CurveKind::Ellipse { .. } => None,
    }
  }

  /// Makes curve `id` (or a new curve when None) the Bézier chain `pieces` from point `start` to
  /// point `end`, with new points for the joints and handles between.
  pub(crate) fn put_bezier(
    &mut self,
    id: Option<CurveId>,
    pieces: &[Bz],
    start: PointId,
    end: PointId,
    construction: bool,
  ) -> Result<CurveId, SketchError> {
    if pieces.is_empty() {
      return Err(SketchError::InvalidValue("nothing is left of the curve".into()));
    }
    let mut pts = vec![start];
    for (k, b) in pieces.iter().enumerate() {
      pts.push(self.add_point(b[1][0], b[1][1]));
      pts.push(self.add_point(b[2][0], b[2][1]));
      pts.push(if k + 1 == pieces.len() { end } else { self.add_point(b[3][0], b[3][1]) });
    }
    let kind = CurveKind::Bezier { points: pts };
    match id.and_then(|i| self.curve_mut(i)) {
      Some(c) => {
        c.kind = kind;
        Ok(c.id)
      }
      None => {
        let nid = self.add_curve(kind)?;
        self.set_construction(nid, construction)?;
        Ok(nid)
      }
    }
  }

  /// Drops tangency / curvature constraints of `curve` at a joint that is no longer on it.
  pub(crate) fn drop_smooth_at(&mut self, curve: CurveId, at: P) {
    let ends_near = |s: &Sketch, other: CurveId| {
      s.ends(other).is_some_and(|(a, b)| [a, b].iter().any(|p| s.pos(*p).is_some_and(|q| dist(q, at) < 1e-6)))
    };
    let drop: Vec<crate::ConstraintId> = self
      .constraints
      .iter()
      .filter_map(|c| match c.kind {
        ConstraintKind::Tangent { a, b } | ConstraintKind::Curvature { a, b } if a == curve || b == curve => {
          let other = if a == curve { b } else { a };
          let free = [a, b].iter().any(|x| self.curve(*x).is_some_and(|c| c.kind.is_freeform()));
          (free && ends_near(self, other)).then_some(c.id)
        }
        _ => None,
      })
      .collect();
    self.constraints.retain(|c| !drop.contains(&c.id));
  }

  /// Moves tangency / curvature constraints of `old` at the joint `at` over to `new`.
  pub(crate) fn retarget_smooth(&mut self, old: CurveId, new: CurveId, at: P) {
    let near_end = |s: &Sketch, other: CurveId| {
      s.ends(other).is_some_and(|(a, b)| [a, b].iter().any(|p| s.pos(*p).is_some_and(|q| dist(q, at) < 1e-6)))
    };
    let ids: Vec<(usize, CurveId)> = self
      .constraints
      .iter()
      .enumerate()
      .filter_map(|(i, c)| match c.kind {
        ConstraintKind::Tangent { a, b } | ConstraintKind::Curvature { a, b } if a == old || b == old => {
          let other = if a == old { b } else { a };
          near_end(self, other).then_some((i, other))
        }
        _ => None,
      })
      .collect();
    for (i, _) in ids {
      if let Some(c) = self.constraints.get_mut(i) {
        match &mut c.kind {
          ConstraintKind::Tangent { a, b } | ConstraintKind::Curvature { a, b } => {
            for x in [a, b] {
              if *x == old {
                *x = new;
              }
            }
          }
          _ => {}
        }
      }
    }
  }

  // ---------------------------------------------------------------- break

  /// Splits a curve without removing anything. With `at_point` it splits at the point of the
  /// curve nearest `at` (a crossing within `snap` of it wins); otherwise, as in Fusion, at its
  /// nearest crossings with other curves on each side of `at`, or at `at` when nothing crosses it.
  /// A circle or ellipse needs two split points. Returns the pieces (the first keeps the id).
  pub fn break_curve(&mut self, curve: CurveId, at: P, at_point: bool, snap: f64) -> Result<Vec<CurveId>, SketchError> {
    self.not_text(curve)?;
    let shape = self.shape(curve).ok_or(SketchError::NoCurve(curve))?;
    let closed = shape.closed();
    let end = shape.end_param();
    let tp = shape.param(at);
    let cuts: Vec<(f64, CurveId)> =
      self.crossings_any(curve)?.into_iter().filter(|(t, _)| closed || (*t > 1e-9 && *t < end - 1e-9)).collect();
    let mut splits: Vec<(f64, Option<CurveId>)> = Vec::new();
    if at_point {
      let p = shape.at(tp);
      let snapped = cuts
        .iter()
        .filter(|(t, _)| dist(shape.at(*t), p) <= snap.max(0.0))
        .min_by(|a, b| dist(shape.at(a.0), p).total_cmp(&dist(shape.at(b.0), p)));
      splits.push(match snapped {
        Some((t, o)) => (*t, Some(*o)),
        None => (tp, None),
      });
    } else if closed {
      if cuts.len() >= 2 {
        let i = cuts.iter().rposition(|(t, _)| *t <= tp).unwrap_or(cuts.len() - 1);
        let (lo, hi) = (cuts[i], cuts[(i + 1) % cuts.len()]);
        splits.push((lo.0, Some(lo.1)));
        splits.push((hi.0, Some(hi.1)));
      }
    } else {
      let lo = cuts.iter().rev().find(|(t, _)| *t < tp).copied();
      let hi = cuts.iter().find(|(t, _)| *t > tp).copied();
      splits.extend(lo.map(|(t, o)| (t, Some(o))));
      splits.extend(hi.map(|(t, o)| (t, Some(o))));
      if splits.is_empty() {
        splits.push((tp, None));
      }
    }
    splits.sort_by(|a, b| a.0.total_cmp(&b.0));
    splits.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
    if !closed {
      splits.retain(|(t, _)| *t > 1e-9 && *t < end - 1e-9);
      if splits.is_empty() {
        return Err(SketchError::InvalidValue("that is the end of the curve".into()));
      }
    } else if splits.len() < 2 {
      return Err(SketchError::WrongGeometry("breaking a closed curve", "two points (or two crossings) on it"));
    }
    self.split_at(curve, &shape, &splits)
  }

  /// Splits `curve` at sorted parameters; split points crossing another curve stay on it.
  fn split_at(
    &mut self,
    curve: CurveId,
    shape: &Shape,
    splits: &[(f64, Option<CurveId>)],
  ) -> Result<Vec<CurveId>, SketchError> {
    let c = self.curve(curve).ok_or(SketchError::NoCurve(curve))?.clone();
    let closed = shape.closed();
    let new_pts: Vec<PointId> = splits
      .iter()
      .map(|(t, _)| {
        let p = shape.at(*t);
        self.add_point(p[0], p[1])
      })
      .collect();
    // (from, to) parameters and points of each piece
    let mut parts: Vec<(f64, f64, PointId, PointId)> = Vec::new();
    if closed {
      let n = splits.len();
      for k in 0..n {
        let (t0, t1) = (splits[k].0, splits[(k + 1) % n].0);
        let t1 = if t1 <= t0 { t1 + shape.end_param() } else { t1 };
        parts.push((t0, t1, new_pts[k], new_pts[(k + 1) % n]));
      }
    } else {
      let (first, last) = self.ends(curve).ok_or(SketchError::NoCurve(curve))?;
      let mut prev = (0.0, first);
      for (k, (t, _)) in splits.iter().enumerate() {
        parts.push((prev.0, *t, prev.1, new_pts[k]));
        prev = (*t, new_pts[k]);
      }
      parts.push((prev.0, shape.end_param(), prev.1, last));
    }
    let old_points = crate::curve_points(&c.kind);
    let end_pos = (!closed).then(|| shape.at(shape.end_param()));
    let mut out = Vec::new();
    for (k, (t0, t1, a, b)) in parts.iter().enumerate() {
      let id = if k == 0 { Some(curve) } else { None };
      let nid = match (&c.kind, shape) {
        (CurveKind::Line { .. }, _) => self.put_curve(id, CurveKind::Line { a: *a, b: *b }, c.construction)?,
        (CurveKind::Arc { center, .. } | CurveKind::Circle { center, .. }, _) => {
          self.put_curve(id, CurveKind::Arc { center: *center, start: *a, end: *b }, c.construction)?
        }
        (CurveKind::Ellipse { center, major, minor } | CurveKind::EllipticalArc { center, major, minor, .. }, _) => {
          let kind = CurveKind::EllipticalArc { center: *center, major: *major, minor: *minor, start: *a, end: *b };
          self.put_curve(id, kind, c.construction)?
        }
        (_, Shape::Free(path)) => self.put_piece(id, &c.kind, path, (*t0, *t1), (*a, *b), c.construction)?,
        _ => return Err(SketchError::NoCurve(curve)),
      };
      out.push(nid);
    }
    if let (Some(p), Some(last)) = (end_pos, out.last())
      && out.len() > 1
    {
      self.retarget_smooth(curve, *last, p);
    }
    if matches!(c.kind, CurveKind::Line { .. }) {
      for w in out.windows(2) {
        self.add_constraint(ConstraintKind::Collinear { a: w[0], b: w[1] })?;
      }
    }
    for ((_, other), p) in splits.iter().zip(&new_pts) {
      if let Some(o) = other {
        self.add_constraint(ConstraintKind::PointOnCurve { p: *p, curve: *o })?;
      }
    }
    for p in old_points {
      self.drop_if_unused(p);
    }
    Ok(out)
  }

  /// Replaces curve `id`'s kind, or adds a new curve.
  pub(crate) fn put_curve(
    &mut self,
    id: Option<CurveId>,
    kind: CurveKind,
    construction: bool,
  ) -> Result<CurveId, SketchError> {
    match id.and_then(|i| self.curve_mut(i)) {
      Some(c) => {
        c.kind = kind;
        Ok(c.id)
      }
      None => {
        let nid = self.add_curve(kind)?;
        self.set_construction(nid, construction)?;
        Ok(nid)
      }
    }
  }

  // ---------------------------------------------------------------- trim

  /// [`Sketch::trim`] for splines, ellipses and Bézier chains: what is left becomes Bézier chains.
  pub(crate) fn trim_free(&mut self, curve: CurveId, at: P) -> Result<Vec<CurveId>, SketchError> {
    let Some(Shape::Free(path)) = self.shape(curve) else { return Err(SketchError::NoCurve(curve)) };
    let c = self.curve(curve).ok_or(SketchError::NoCurve(curve))?.clone();
    let n = path.len() as f64;
    let closed = path_closed(&path);
    let tp = path_param(&path, at);
    let cuts: Vec<(f64, CurveId)> =
      self.crossings_any(curve)?.into_iter().filter(|(t, _)| closed || (*t > 1e-9 && *t < n - 1e-9)).collect();
    let old_points = crate::curve_points(&c.kind);
    let made = if closed {
      let mut cuts = cuts;
      cuts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
      if cuts.len() < 2 {
        self.remove_curve(curve)?;
        return Ok(vec![]);
      }
      let i = cuts.iter().rposition(|(t, _)| *t <= tp).unwrap_or(cuts.len() - 1);
      let (lo, hi) = (cuts[i], cuts[(i + 1) % cuts.len()]);
      // keep from the cut after the pick round to the one before it
      let u1 = if lo.0 <= hi.0 { lo.0 + n } else { lo.0 };
      let (ps, pe) = (path_at(&path, hi.0), path_at(&path, lo.0));
      let (ps, pe) = (self.add_point(ps[0], ps[1]), self.add_point(pe[0], pe[1]));
      self.put_bezier(Some(curve), &path_sub(&path, hi.0, u1), ps, pe, c.construction)?;
      self.add_constraint(ConstraintKind::PointOnCurve { p: ps, curve: hi.1 })?;
      self.add_constraint(ConstraintKind::PointOnCurve { p: pe, curve: lo.1 })?;
      vec![curve]
    } else {
      let (first, last) = self.ends(curve).ok_or(SketchError::NoCurve(curve))?;
      let lo = cuts.iter().rev().find(|(t, _)| *t < tp).copied();
      let hi = cuts.iter().find(|(t, _)| *t > tp).copied();
      let (start_pos, end_pos) = (path_at(&path, 0.0), path_at(&path, n));
      match (lo, hi) {
        (None, None) => {
          self.remove_curve(curve)?;
          return Ok(vec![]);
        }
        (None, Some((t, other))) => {
          let p = path_at(&path, t);
          let np = self.add_point(p[0], p[1]);
          self.drop_smooth_at(curve, start_pos);
          self.put_piece(Some(curve), &c.kind, &path, (t, n), (np, last), c.construction)?;
          self.add_constraint(ConstraintKind::PointOnCurve { p: np, curve: other })?;
          vec![curve]
        }
        (Some((t, other)), None) => {
          let p = path_at(&path, t);
          let np = self.add_point(p[0], p[1]);
          self.drop_smooth_at(curve, end_pos);
          self.put_piece(Some(curve), &c.kind, &path, (0.0, t), (first, np), c.construction)?;
          self.add_constraint(ConstraintKind::PointOnCurve { p: np, curve: other })?;
          vec![curve]
        }
        (Some((t0, o0)), Some((t1, o1))) => {
          let (p0, p1) = (path_at(&path, t0), path_at(&path, t1));
          let (n0, n1) = (self.add_point(p0[0], p0[1]), self.add_point(p1[0], p1[1]));
          self.put_piece(Some(curve), &c.kind, &path, (0.0, t0), (first, n0), c.construction)?;
          let second = self.put_piece(None, &c.kind, &path, (t1, n), (n1, last), c.construction)?;
          self.retarget_smooth(curve, second, end_pos);
          self.add_constraint(ConstraintKind::PointOnCurve { p: n0, curve: o0 })?;
          self.add_constraint(ConstraintKind::PointOnCurve { p: n1, curve: o1 })?;
          vec![curve, second]
        }
      }
    };
    for p in old_points {
      self.drop_if_unused(p);
    }
    Ok(made)
  }

  // ---------------------------------------------------------------- extend

  /// Lengthens an arc along its circle from the end nearest `at` to the next curve it reaches.
  pub(crate) fn extend_arc(&mut self, arc: CurveId, at: P) -> Result<(), SketchError> {
    let Some(g @ Geo::Arc { c, a0, sweep, .. }) = self.geo(arc) else {
      return Err(SketchError::WrongGeometry("extend", "a line, arc or spline"));
    };
    let Some(CurveKind::Arc { start, end, .. }) = self.curve(arc).map(|c| c.kind.clone()) else {
      return Err(SketchError::WrongGeometry("extend", "an arc"));
    };
    let from_end = dist(at, g.at(sweep)) <= dist(at, g.at(0.0));
    let mut best: Option<(f64, P, CurveId)> = None;
    for (p, other) in self.hits_extended(&g, arc) {
      let a = angle_of(c, p);
      let gain = if from_end { ccw_sweep(a0 + sweep, a) } else { ccw_sweep(a, a0) };
      if gain > 1e-9 && gain < TAU - sweep - 1e-9 && best.is_none_or(|(bg, _, _)| gain < bg) {
        best = Some((gain, p, other));
      }
    }
    let (_, p, other) = best.ok_or(SketchError::InvalidValue("nothing to extend the arc to".into()))?;
    self.move_end(arc, if from_end { end } else { start }, p, other)
  }

  /// Moves a curve's end point `end` to `p` on curve `other` (a new point when other geometry
  /// shares the end or it is fixed).
  pub(crate) fn move_end(&mut self, curve: CurveId, end: PointId, p: P, other: CurveId) -> Result<(), SketchError> {
    if self.point_users(end) > 1 || self.point(end).is_some_and(|x| x.fixed) {
      let np = self.add_point(p[0], p[1]);
      self.swap_point(curve, end, np);
      self.add_constraint(ConstraintKind::PointOnCurve { p: np, curve: other })?;
    } else {
      if let Some(q) = self.point_mut(end) {
        q.x = p[0];
        q.y = p[1];
      }
      // constraints that pinned the old end would fight the new position
      self.constraints.retain(|c| {
        !matches!(
          c.kind,
          ConstraintKind::Distance { .. }
            | ConstraintKind::HorizontalDistance { .. }
            | ConstraintKind::VerticalDistance { .. }
            | ConstraintKind::Fix { .. }
            | ConstraintKind::Coincident { .. }
        ) || !c.kind.points().contains(&end)
      });
      self.add_constraint(ConstraintKind::PointOnCurve { p: end, curve: other })?;
    }
    Ok(())
  }

  /// Lengthens an open spline or Bézier chain from the end nearest `at` to the next curve: along
  /// the end piece's own cubic, else straight on along its end tangent. The result is a Bézier
  /// chain.
  pub(crate) fn extend_free(&mut self, curve: CurveId, at: P) -> Result<(), SketchError> {
    let Some(Shape::Free(path)) = self.shape(curve) else { return Err(SketchError::NoCurve(curve)) };
    if path_closed(&path) {
      return Err(SketchError::WrongGeometry("extend", "an open curve"));
    }
    let c = self.curve(curve).ok_or(SketchError::NoCurve(curve))?.clone();
    let (first, last) = self.ends(curve).ok_or(SketchError::NoCurve(curve))?;
    let n = path.len() as f64;
    let from_end = dist(at, path_at(&path, n)) <= dist(at, path_at(&path, 0.0));
    let mut fwd = if from_end { path.clone() } else { path_rev(&path) };
    let Some(lastp) = fwd.last().copied() else { return Err(SketchError::NoCurve(curve)) };
    let size = self.bounds().map(|(a, b)| dist(a, b)).unwrap_or(100.0).max(1.0);
    let others: Vec<(CurveId, Shape)> =
      self.curves.iter().filter(|o| o.id != curve).filter_map(|o| self.shape(o.id).map(|s| (o.id, s))).collect();
    let nearest = |ext: &Shape| -> Option<(f64, P, CurveId)> {
      let mut best: Option<(f64, P, CurveId)> = None;
      for (id, h) in &others {
        for (u, p) in intersect(ext, h) {
          if u > 1e-9 && best.is_none_or(|b| u < b.0) {
            best = Some((u, p, *id));
          }
        }
      }
      best
    };
    // the end piece's cubic carried on to three times its parameter range
    const REACH: f64 = 4.0;
    let ext = bz_sub(&lastp, 1.0, REACH);
    let hit = if let Some((u, p, other)) = nearest(&Shape::Free(vec![ext])) {
      if let Some(l) = fwd.last_mut() {
        *l = bz_sub(&lastp, 0.0, 1.0 + u * (REACH - 1.0));
      }
      (p, other)
    } else {
      let e = lastp[3];
      let d = unit(bz_d1(&lastp, 1.0)).ok_or(SketchError::InvalidValue("the curve has no end direction".into()))?;
      let ray = Geo::Line { a: e, b: add(e, mul(d, 10.0 * size)) };
      let (_, p, other) =
        nearest(&Shape::Geo(ray)).ok_or(SketchError::InvalidValue("nothing to extend the curve to".into()))?;
      fwd.push([e, lerp(e, p, 1.0 / 3.0), lerp(e, p, 2.0 / 3.0), p]);
      (p, other)
    };
    if self.extend_fit(curve, &c.kind, from_end, hit)? {
      return Ok(());
    }
    let pieces = if from_end { fwd } else { path_rev(&fwd) };
    let old_points = crate::curve_points(&c.kind);
    let end = if from_end { last } else { first };
    self.put_bezier(Some(curve), &pieces, first, last, c.construction)?;
    self.move_end(curve, end, hit.0, hit.1)?;
    for p in old_points {
      self.drop_if_unused(p);
    }
    Ok(())
  }

  // ---------------------------------------------------------------- offset

  /// [`Sketch::offset`] of a chain that includes splines or ellipses. Lines stay parallel and
  /// arcs concentric; freeform curves become Bézier chains within [`OFFSET_TOL`] of the true
  /// offset. Where neighbours' offsets overlap they are cut back to where they cross; where they
  /// leave a gap (an outside corner) an arc round the corner closes it.
  pub(crate) fn offset_free(&mut self, ids: &[CurveId], distance: f64) -> Result<Vec<CurveId>, SketchError> {
    let ends = |s: &Sketch, id: CurveId| -> Option<(P, P)> {
      let sh = s.shape(id)?;
      Some((sh.at(0.0), sh.at(sh.end_param())))
    };
    let near = |p: P, q: P| dist(p, q) < 1e-6;
    // order the chain: (curve, reversed)
    let mut rest: Vec<CurveId> = ids.to_vec();
    let degree = |s: &Sketch, p: P| {
      ids
        .iter()
        .filter_map(|i| ends(s, *i))
        .map(|(a, b)| usize::from(near(a, p)) + usize::from(near(b, p)))
        .sum::<usize>()
    };
    let start = ids.iter().filter_map(|i| ends(self, *i)).flat_map(|(a, b)| [a, b]).find(|p| degree(self, *p) == 1);
    let closed = start.is_none();
    let mut at = match start {
      Some(p) => p,
      None => ends(self, ids[0]).ok_or(SketchError::NoCurve(ids[0]))?.0,
    };
    let mut chain: Vec<(CurveId, bool)> = Vec::new();
    while let Some(i) = rest.iter().position(|id| ends(self, *id).is_some_and(|(a, b)| near(a, at) || near(b, at))) {
      let id = rest.swap_remove(i);
      let (a, b) = ends(self, id).ok_or(SketchError::NoCurve(id))?;
      let rev = !near(a, at);
      at = if rev { a } else { b };
      chain.push((id, rev));
    }
    if !rest.is_empty() {
      return Err(SketchError::InvalidValue("the curves don't form one chain".into()));
    }
    // travel-ordered geometry: each piece as a Bézier chain for sampling, plus the original
    let mut travel: Vec<Vec<Bz>> = Vec::new();
    for (id, rev) in &chain {
      let path = match self.shape(*id).ok_or(SketchError::NoCurve(*id))? {
        Shape::Free(p) => p,
        Shape::Geo(g) => geo_beziers(&g),
      };
      travel.push(if *rev { path_rev(&path) } else { path });
    }
    let mut side = 1.0;
    if closed {
      let pts: Vec<P> = travel.iter().flatten().flat_map(|b| (0..8).map(move |k| bz_at(b, k as f64 / 8.0))).collect();
      let area: f64 = (0..pts.len()).map(|i| cross(pts[i], pts[(i + 1) % pts.len()])).sum::<f64>() / 2.0;
      side = if area > 0.0 { -1.0 } else { 1.0 };
    }
    let off = side * distance;
    // the offset of each piece, in travel order
    let mut pieces: Vec<Off> = Vec::new();
    for (k, (id, rev)) in chain.iter().enumerate() {
      let kind = self.curve(*id).map(|c| c.kind.clone()).ok_or(SketchError::NoCurve(*id))?;
      let (s, e) = (path_at(&travel[k], 0.0), path_at(&travel[k], travel[k].len() as f64));
      pieces.push(match (self.geo(*id), &kind) {
        (Some(Geo::Line { .. }), _) => {
          let d = unit(sub(e, s)).ok_or(SketchError::InvalidValue("zero-length line".into()))?;
          let nrm = mul(left(d), off);
          Off::Line { a: add(s, nrm), b: add(e, nrm) }
        }
        (Some(Geo::Arc { c, r, .. }), CurveKind::Arc { center, .. }) => {
          let ccw = if *rev { -1.0 } else { 1.0 };
          let r2 = r - ccw * off;
          if r2 <= EPS {
            return Err(SketchError::InvalidValue("the offset swallows an arc".into()));
          }
          let k = r2 / r;
          Off::Arc { center: *center, c, r: r2, rev: *rev, s: add(c, mul(sub(s, c), k)), e: add(c, mul(sub(e, c), k)) }
        }
        _ => Off::Free(offset_path(&travel[k], off)),
      });
    }
    // joints: shared point, or a gap closed by an arc round the original corner
    let n = pieces.len();
    let joints = if closed { n } else { n.saturating_sub(1) };
    let mut joint: Vec<Joint> = Vec::new();
    for k in 0..joints {
      let (a, b) = (&pieces[k], &pieces[(k + 1) % n]);
      let (e, s) = (a.end(), b.start());
      let corner = path_at(&travel[k], travel[k].len() as f64);
      if near(e, s) {
        joint.push(Joint::Shared(e));
        continue;
      }
      let hits: Vec<P> = match (a.extended(), b.extended()) {
        (Shape::Geo(g), Shape::Geo(h)) => meet(&g, &h),
        (Shape::Free(p), Shape::Geo(h)) | (Shape::Geo(h), Shape::Free(p)) => {
          path_meets_geo(&p, &h).into_iter().map(|x| x.1).collect()
        }
        (Shape::Free(p), Shape::Free(q)) => path_meets_path(&p, &q).into_iter().map(|x| x.1).collect(),
      };
      let x = hits.into_iter().min_by(|p, q| dist(*p, corner).total_cmp(&dist(*q, corner)));
      // a crossing on the inside of the corner; outside corners leave a gap
      let reach = 4.0 * off.abs() + 1e-6;
      let inside = x.filter(|x| dist(*x, corner) < reach && a.cuts_back(*x) && b.cuts_back(*x));
      joint.push(match inside {
        Some(x) => Joint::Shared(x),
        None => {
          let (id, rev) = chain[k];
          let (p0, p1) = self.ends(id).ok_or(SketchError::NoCurve(id))?;
          Joint::Gap { e, s, corner: if rev { p0 } else { p1 } }
        }
      });
    }
    // points where pieces start and end
    let mut out = Vec::new();
    let mut joint_pts: Vec<(PointId, PointId)> = Vec::new();
    for j in &joint {
      joint_pts.push(match j {
        Joint::Shared(p) => {
          let id = self.add_point(p[0], p[1]);
          (id, id)
        }
        Joint::Gap { e, s, .. } => (self.add_point(e[0], e[1]), self.add_point(s[0], s[1])),
      });
    }
    let open_start = (!closed).then(|| pieces[0].start());
    let open_end = (!closed).then(|| pieces[n - 1].end());
    let first_pt = open_start.map(|p| self.add_point(p[0], p[1]));
    let last_pt = open_end.map(|p| self.add_point(p[0], p[1]));
    for k in 0..n {
      let ps = match (k, first_pt) {
        (0, Some(p)) => p,
        _ => joint_pts[(n + k - 1) % n].1,
      };
      let pe = match (k + 1 == n, last_pt) {
        (true, Some(p)) => p,
        _ => joint_pts[k].0,
      };
      let (sp, ep) = (self.pos(ps).unwrap_or_default(), self.pos(pe).unwrap_or_default());
      let orig = chain[k].0;
      let nid = match &pieces[k] {
        Off::Line { .. } => {
          let nid = self.add_curve(CurveKind::Line { a: ps, b: pe })?;
          self.add_constraint(ConstraintKind::Parallel { a: orig, b: nid })?;
          nid
        }
        Off::Arc { center, rev, .. } => {
          let kind = if *rev {
            CurveKind::Arc { center: *center, start: pe, end: ps }
          } else {
            CurveKind::Arc { center: *center, start: ps, end: pe }
          };
          self.add_curve(kind)?
        }
        Off::Free(path) => {
          let (u0, u1) = (path_param(path, sp), path_param(path, ep));
          let whole = closed && n == 1;
          let part = if whole { path.clone() } else { path_sub(path, u0, u1) };
          let part = if part.is_empty() { path.clone() } else { snap_ends(part, sp, ep) };
          self.put_bezier(None, &part, ps, pe, false)?
        }
      };
      out.push(nid);
      // the gap after this piece
      if let Some(Joint::Gap { corner, .. }) = joint.get(k)
        && let (Some((ge, _)), Some((_, gs))) = (joint_pts.get(k), joint_pts.get(k))
        && let (Some(c), Some(e), Some(s)) = (self.pos(*corner), self.pos(*ge), self.pos(*gs))
      {
        let kind = if ccw_sweep(angle_of(c, e), angle_of(c, s)) <= PI {
          CurveKind::Arc { center: *corner, start: *ge, end: *gs }
        } else {
          CurveKind::Arc { center: *corner, start: *gs, end: *ge }
        };
        out.push(self.add_curve(kind)?);
      }
    }
    Ok(out)
  }

  // ---------------------------------------------------------------- blend

  /// A spline bridging an end of curve `a` to an end of curve `b`, tangent to both (G1) or also
  /// matching their curvature (G2), held there by tangent / curvature constraints. Each curve's
  /// end is the one nearest its `near` pick, else the closest pair of ends.
  pub fn blend(&mut self, a: CurveId, b: CurveId, near: Option<[P; 2]>, g2: bool) -> Result<CurveId, SketchError> {
    if a == b {
      return Err(SketchError::WrongGeometry("a blend curve", "two different curves"));
    }
    self.not_text(a)?;
    self.not_text(b)?;
    let what = || SketchError::WrongGeometry("a blend curve", "two curves with ends (lines, arcs or splines)");
    let (a0, a1) = self.ends(a).ok_or_else(what)?;
    let (b0, b1) = self.ends(b).ok_or_else(what)?;
    let pos = |p: PointId| self.pos(p).ok_or(SketchError::NoPoint(p));
    let (pa, pb) = ([pos(a0)?, pos(a1)?], [pos(b0)?, pos(b1)?]);
    let (ea, eb) = match near {
      Some([na, nb]) => (dist(pa[1], na) < dist(pa[0], na), dist(pb[1], nb) < dist(pb[0], nb)),
      None => {
        let mut best = (f64::INFINITY, true, true);
        for (i, ia) in [false, true].into_iter().enumerate() {
          for (j, ib) in [false, true].into_iter().enumerate() {
            let d = dist(pa[i], pb[j]);
            if d > 1e-9 && d < best.0 {
              best = (d, ia, ib);
            }
          }
        }
        (best.1, best.2)
      }
    };
    let ka = self.curve(a).map(|c| c.kind.clone()).ok_or(SketchError::NoCurve(a))?;
    let kb = self.curve(b).map(|c| c.kind.clone()).ok_or(SketchError::NoCurve(b))?;
    let posf = |p: PointId| self.pos(p);
    let (Some(fa), Some(fb)) = (end_frame(&ka, ea, &posf), end_frame(&kb, eb, &posf)) else { return Err(what()) };
    let l = dist(fa.0, fb.0);
    if l < 1e-9 {
      return Err(SketchError::InvalidValue("the ends already meet".into()));
    }
    let (sa, sb) = (if ea { a1 } else { a0 }, if eb { b1 } else { b0 });
    let mut ctrl = Vec::new();
    if g2 {
      // first piece of a 6-point clamped B-spline: B1 = c1, B2 = (c1 + c2) / 2, so curvature κ
      // at the start needs c2 = P0 + 3h·t + 3κh²·n
      let h = l / 5.0;
      let side = |f: &(P, P, f64)| {
        let (p, t, k) = *f;
        (add(p, mul(t, h)), add(add(p, mul(t, 3.0 * h)), mul(left(t), 3.0 * k * h * h)))
      };
      let (c1, c2) = side(&fa);
      let (c4, c3) = side(&fb);
      ctrl.extend([c1, c2, c3, c4]);
    } else {
      let h = l / 3.0;
      ctrl.extend([add(fa.0, mul(fa.1, h)), add(fb.0, mul(fb.1, h))]);
    }
    let mut pts = vec![sa];
    for c in &ctrl {
      pts.push(self.add_point(c[0], c[1]));
    }
    pts.push(sb);
    let blend = self.add_curve(CurveKind::ControlSpline { points: pts })?;
    for other in [a, b] {
      let kind = if g2 {
        ConstraintKind::Curvature { a: other, b: blend }
      } else {
        ConstraintKind::Tangent { a: other, b: blend }
      };
      self.add_constraint(kind)?;
    }
    Ok(blend)
  }

  /// Gives a fit-point spline a tangent handle at the end where it meets `other` (so tangency and
  /// curvature constraints can turn the end without dragging the fit points about), and with
  /// `curvature` a curvature handle there too, which makes that end smooth as Fusion's are.
  pub(crate) fn add_spline_handle(&mut self, spline: CurveId, other: CurveId, curvature: bool) {
    let Some(kind) = self.curve(spline).map(|c| c.kind.clone()) else { return };
    let CurveKind::Spline { points, start_handle, end_handle, start_curvature, end_curvature } = kind.clone() else {
      return;
    };
    let (Some(first), Some(last), Some((fh, lh))) = (
      points.first().and_then(|p| self.pos(*p)),
      points.last().and_then(|p| self.pos(*p)),
      crate::spline_handles(&kind, &|p| self.pos(p)),
    ) else {
      return;
    };
    let reach = |p: P| self.distance_to_curve(other, p).unwrap_or(f64::INFINITY);
    let at_end = reach(last) < reach(first);
    let (sh, eh) = if at_end {
      (start_handle, end_handle.or_else(|| Some(self.add_point(lh[0], lh[1]))))
    } else {
      (start_handle.or_else(|| Some(self.add_point(fh[0], fh[1]))), end_handle)
    };
    let (mut sk, mut ek) = (start_curvature, end_curvature);
    let slot = if at_end { &mut ek } else { &mut sk };
    if curvature && slot.is_none() {
      // asking for the end's present curvature, beside the tangent handle
      let (at, h) = if at_end { (last, lh) } else { (first, fh) };
      let frame = end_frame(&kind, at_end, &|p| self.pos(p));
      if let Some((_, dir, k)) = frame {
        let q = crate::smoothend::place_handle(at, dir, k, dist(at, h).max(1e-3));
        *slot = Some(self.add_point(q[0], q[1]));
      }
    }
    if let Some(c) = self.curve_mut(spline) {
      c.kind = CurveKind::Spline { points, start_handle: sh, end_handle: eh, start_curvature: sk, end_curvature: ek };
    }
  }
}

/// A line, arc or circle as Bézier pieces (for sampling and offsetting mixed chains).
fn geo_beziers(g: &Geo) -> Vec<Bz> {
  match *g {
    Geo::Line { a, b } => vec![[a, lerp(a, b, 1.0 / 3.0), lerp(a, b, 2.0 / 3.0), b]],
    Geo::Arc { c, r, a0, sweep } => {
      let n = (sweep / (PI / 4.0)).ceil().max(1.0) as usize;
      let step = sweep / n as f64;
      let k = 4.0 / 3.0 * (step / 4.0).tan();
      (0..n)
        .map(|i| {
          let (t0, t1) = (a0 + step * i as f64, a0 + step * (i + 1) as f64);
          let (p0, p3) = ([t0.cos(), t0.sin()], [t1.cos(), t1.sin()]);
          let m = |x: P| [c[0] + r * x[0], c[1] + r * x[1]];
          [m(p0), m(add(p0, mul(left(p0), k))), m(sub(p3, mul(left(p3), k))), m(p3)]
        })
        .collect()
    }
  }
}

/// Puts a chain's first and last points exactly on `s` and `e` (moving their handles along).
fn snap_ends(mut path: Vec<Bz>, s: P, e: P) -> Vec<Bz> {
  if let Some(f) = path.first_mut() {
    let d = sub(s, f[0]);
    f[0] = s;
    f[1] = add(f[1], d);
  }
  if let Some(l) = path.last_mut() {
    let d = sub(e, l[3]);
    l[3] = e;
    l[2] = add(l[2], d);
  }
  path
}

/// One piece of an offset chain, in travel order.
enum Off {
  Line { a: P, b: P },
  Arc { center: PointId, c: P, r: f64, rev: bool, s: P, e: P },
  Free(Vec<Bz>),
}

enum Joint {
  Shared(P),
  Gap { e: P, s: P, corner: PointId },
}

impl Off {
  fn start(&self) -> P {
    match self {
      Off::Line { a, .. } => *a,
      Off::Arc { s, .. } => *s,
      Off::Free(p) => path_at(p, 0.0),
    }
  }

  fn end(&self) -> P {
    match self {
      Off::Line { b, .. } => *b,
      Off::Arc { e, .. } => *e,
      Off::Free(p) => path_at(p, p.len() as f64),
    }
  }

  /// Lines as infinite lines and arcs as whole circles, for meeting neighbours.
  fn extended(&self) -> Shape {
    match self {
      Off::Line { a, b } => Shape::Geo(Geo::Line { a: *a, b: *b }),
      Off::Arc { c, r, .. } => Shape::Geo(Geo::Arc { c: *c, r: *r, a0: 0.0, sweep: TAU }),
      Off::Free(p) => Shape::Free(p.clone()),
    }
  }

  /// Does ending (or starting) at `x` keep a sensible piece? Lines and arcs may also grow to it.
  fn cuts_back(&self, x: P) -> bool {
    match self {
      Off::Free(p) => {
        let u = path_param(p, x);
        dist(path_at(p, u), x) < 1e-6
      }
      _ => true,
    }
  }
}

/// A Bézier chain offset by `d` to the left of its direction.
fn offset_path(path: &[Bz], d: f64) -> Vec<Bz> {
  let mut out = Vec::new();
  for b in path {
    offset_bz(b, d, 0, &mut out);
  }
  out
}

fn frame(b: &Bz, t: f64) -> (P, P, f64) {
  let d1 = bz_d1(b, t);
  let tan = unit(d1).or_else(|| unit(sub(b[3], b[0]))).unwrap_or([1.0, 0.0]);
  (bz_at(b, t), tan, bz_curvature(b, t))
}

fn offset_bz(b: &Bz, d: f64, depth: u32, out: &mut Vec<Bz>) {
  let (p0, t0, k0) = frame(b, 0.0);
  let (p3, t3, k3) = frame(b, 1.0);
  let q0 = add(p0, mul(left(t0), d));
  let q3 = add(p3, mul(left(t3), d));
  // the offset's speed is (1 - κd) times the original's
  let c1 = add(q0, mul(sub(b[1], b[0]), 1.0 - k0 * d));
  let c2 = add(q3, mul(sub(b[2], b[3]), 1.0 - k3 * d));
  let cand = [q0, c1, c2, q3];
  let mut err: f64 = 0.0;
  for t in [0.2, 0.4, 0.5, 0.6, 0.8] {
    let q = bz_at(&cand, t);
    let u = path_param(std::slice::from_ref(b), q);
    err = err.max((dist(bz_at(b, u), q) - d.abs()).abs());
  }
  if err > OFFSET_TOL && depth < 9 {
    let (l, r) = bz_split(b, 0.5);
    offset_bz(&l, d, depth + 1, out);
    offset_bz(&r, d, depth + 1, out);
  } else {
    out.push(cand);
  }
}

/// A curve's end: (position, direction pointing out of the curve, signed curvature when arriving
/// there heading that way). `at_end` picks the end over the start. None for curves without ends.
pub(crate) fn end_frame(kind: &CurveKind, at_end: bool, pos: &dyn Fn(PointId) -> Option<P>) -> Option<(P, P, f64)> {
  match kind {
    CurveKind::Line { a, b } => {
      let (pa, pb) = (pos(*a)?, pos(*b)?);
      let t = unit(sub(pb, pa))?;
      Some(if at_end { (pb, t, 0.0) } else { (pa, mul(t, -1.0), 0.0) })
    }
    CurveKind::Arc { center, start, end } => {
      let c = pos(*center)?;
      let p = pos(if at_end { *end } else { *start })?;
      let r = dist(c, p);
      let ccw = left(unit(sub(p, c))?);
      Some(if at_end { (p, ccw, 1.0 / r) } else { (p, mul(ccw, -1.0), -1.0 / r) })
    }
    CurveKind::Spline { .. } if crate::spline_end_frame(kind, at_end, pos).is_some() => {
      crate::spline_end_frame(kind, at_end, pos)
    }
    CurveKind::Spline { .. }
    | CurveKind::ControlSpline { .. }
    | CurveKind::Bezier { .. }
    | CurveKind::EllipticalArc { .. } => {
      let bz = crate::kind_beziers(kind, pos);
      if at_end {
        let b = *bz.last()?;
        let d1 = bz_d1(&b, 1.0);
        match unit(d1) {
          Some(t) => Some((b[3], t, bz_curvature(&b, 1.0))),
          None => Some((b[3], unit(sub(b[3], b[1])).or_else(|| unit(sub(b[3], b[0])))?, 0.0)),
        }
      } else {
        let b = *bz.first()?;
        let d1 = bz_d1(&b, 0.0);
        match unit(d1) {
          Some(t) => Some((b[0], mul(t, -1.0), -bz_curvature(&b, 0.0))),
          None => Some((b[0], unit(sub(b[0], b[2])).or_else(|| unit(sub(b[0], b[3])))?, 0.0)),
        }
      }
    }
    CurveKind::Circle { .. } | CurveKind::Ellipse { .. } => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{Dim, PtSpec, find_profiles};

  fn spline(s: &mut Sketch, pts: &[P]) -> CurveId {
    let ids = pts.iter().map(|p| s.add_point(p[0], p[1])).collect();
    s.add_curve(CurveKind::spline(ids)).unwrap()
  }

  /// Largest distance from points along `b` to the curve `a` described by `path`.
  fn deviation(path: &[Bz], other: &[Bz]) -> f64 {
    other
      .iter()
      .flat_map(|b| (0..=8).map(move |k| bz_at(b, k as f64 / 8.0)))
      .map(|p| dist(path_at(path, path_param(path, p)), p))
      .fold(0.0, f64::max)
  }

  #[test]
  fn bezier_split_and_sub_are_exact() {
    let b = [[0.0, 0.0], [1.0, 3.0], [4.0, 3.0], [5.0, 0.0]];
    let (l, r) = bz_split(&b, 0.3);
    assert!(dist(bz_at(&l, 0.5), bz_at(&b, 0.15)) < 1e-12);
    assert!(dist(bz_at(&r, 0.5), bz_at(&b, 0.65)) < 1e-12);
    let m = bz_sub(&b, 0.2, 0.7);
    assert!(dist(bz_at(&m, 0.5), bz_at(&b, 0.45)) < 1e-12);
    // extrapolation follows the cubic
    let e = bz_sub(&b, 1.0, 2.0);
    assert!(dist(bz_at(&e, 0.5), bz_at(&b, 1.5)) < 1e-9);
    let path = vec![b, [b[3], [6.0, -3.0], [9.0, -3.0], [10.0, 0.0]]];
    assert!((path_param(&path, bz_at(&b, 0.4)) - 0.4).abs() < 1e-9);
  }

  #[test]
  fn break_lines_arcs_circles_and_splines() {
    let mut s = Sketch::new();
    let (h, _) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(30.0, 0.0)).unwrap();
    s.add_line(PtSpec::At(10.0, -5.0), PtSpec::At(10.0, 5.0)).unwrap();
    // between crossings, the line breaks at the crossing; no crossing on the right → the point
    let parts = s.break_curve(h, [20.0, 0.0], false, 0.0).unwrap();
    assert_eq!(parts.len(), 2);
    assert!(s.solve().ok());
    let total: f64 = parts.iter().map(|c| s.distance_to_curve(*c, [10.0, 0.0]).unwrap()).sum();
    assert!(total < 1e-9, "both pieces end at the crossing");
    // at a point
    let (l, _) = s.add_line(PtSpec::At(0.0, 20.0), PtSpec::At(10.0, 20.0)).unwrap();
    let parts = s.break_curve(l, [4.0, 21.0], true, 0.0).unwrap();
    assert_eq!(parts.len(), 2);
    let shared = s.curve_points(parts[0])[1];
    assert_eq!(shared, s.curve_points(parts[1])[0], "the pieces share the break point");
    assert!(dist(s.pos(shared).unwrap(), [4.0, 20.0]) < 1e-9);
    assert!(s.solve().ok());
    // a circle needs two crossings
    let c = s.add_circle(PtSpec::At(50.0, 0.0), 5.0).unwrap();
    assert!(s.break_curve(c, [55.0, 0.0], true, 0.0).is_err());
    s.add_line(PtSpec::At(40.0, 0.0), PtSpec::At(60.0, 0.0)).unwrap();
    let arcs = s.break_curve(c, [50.0, 5.0], false, 0.0).unwrap();
    assert_eq!(arcs.len(), 2);
    assert!(s.solve().ok());
    // a fit-point spline breaks into fit-point splines, a control-point one into exact Bézier chains
    let sp = spline(&mut s, &[[0.0, 40.0], [10.0, 50.0], [20.0, 40.0], [30.0, 45.0]]);
    let before = s.beziers(sp);
    let parts = s.break_curve(sp, [15.0, 47.0], true, 0.0).unwrap();
    assert_eq!(parts.len(), 2);
    for p in &parts {
      assert!(matches!(s.curve(*p).unwrap().kind, CurveKind::Spline { .. }));
      let dev = deviation(&before, &s.beziers(*p));
      assert!(dev < 2.0, "close to the old shape: {dev}");
    }
    let ids = [[0.0, 60.0], [10.0, 70.0], [20.0, 60.0], [30.0, 65.0]].map(|p| s.add_point(p[0], p[1]));
    let cs = s.add_curve(CurveKind::ControlSpline { points: ids.to_vec() }).unwrap();
    let before = s.beziers(cs);
    let parts = s.break_curve(cs, [15.0, 65.0], true, 0.0).unwrap();
    assert_eq!(parts.len(), 2);
    for p in &parts {
      assert!(matches!(s.curve(*p).unwrap().kind, CurveKind::Bezier { .. }));
      assert!(deviation(&before, &s.beziers(*p)) < 1e-9);
    }
    assert!(s.solve().ok());
  }

  #[test]
  fn trim_splines_ellipses_and_lines_cut_by_splines() {
    let mut s = Sketch::new();
    let sp = spline(&mut s, &[[0.0, 0.0], [10.0, 10.0], [20.0, 0.0], [30.0, 10.0]]);
    let before = s.beziers(sp);
    s.add_line(PtSpec::At(10.0, -5.0), PtSpec::At(10.0, 20.0)).unwrap();
    s.add_line(PtSpec::At(20.0, -5.0), PtSpec::At(20.0, 20.0)).unwrap();
    let parts = s.trim(sp, [15.0, 5.0]).unwrap();
    assert_eq!(parts.len(), 2);
    for p in &parts {
      let dev = deviation(&before, &s.beziers(*p));
      // the kept fit points refit a minimum-energy curve, so the shape moves a little
      assert!(dev < 1.0, "the kept pieces keep close to their shape: {dev}");
    }
    assert!(s.solve().ok());
    let (min, max) = s.bounds().unwrap();
    assert!(min[0] < 1e-9 && max[0] > 29.0);
    // an ellipse cut by a line keeps the far side
    let mut s = Sketch::new();
    let c = s.add_point(0.0, 0.0);
    let m = s.add_point(20.0, 0.0);
    let e = s.add_curve(CurveKind::Ellipse { center: c, major: m, minor: 10.0 }).unwrap();
    let (l, _) = s.add_line(PtSpec::At(-30.0, 0.0), PtSpec::At(30.0, 0.0)).unwrap();
    s.trim(e, [0.0, 10.0]).unwrap();
    let bz = s.beziers(e);
    assert!(bz.iter().all(|b| (0..=8).all(|k| bz_at(b, k as f64 / 8.0)[1] <= 1e-9)), "the lower half is left");
    assert!(bz.iter().any(|b| bz_at(b, 0.5)[1] < -9.0));
    assert!(s.solve().ok());
    // the line, now crossing the half ellipse, trims against it
    let parts = s.trim(l, [0.0, 0.0]).unwrap();
    assert_eq!(parts.len(), 2);
    assert!(s.solve().ok());
    assert_eq!(find_profiles(&s).len(), 0);
    // an ellipse with one crossing goes entirely
    let mut s = Sketch::new();
    let c = s.add_point(0.0, 0.0);
    let m = s.add_point(20.0, 0.0);
    let e = s.add_curve(CurveKind::Ellipse { center: c, major: m, minor: 10.0 }).unwrap();
    s.trim(e, [0.0, 10.0]).unwrap();
    assert!(s.curves.is_empty());
  }

  #[test]
  fn extend_arcs_splines_and_lines_to_splines() {
    let mut s = Sketch::new();
    spline(&mut s, &[[20.0, -10.0], [22.0, 0.0], [20.0, 10.0]]);
    let (l, _) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(5.0, 0.0)).unwrap();
    s.extend(l, [5.0, 0.0]).unwrap();
    let Some(Geo::Line { b, .. }) = s.geo(l) else { panic!() };
    assert!(dist(b, [22.0, 0.0]) < 1e-6, "{b:?}");
    assert!(s.solve().ok());
    // a spline grows along its end until it meets the line x = 40
    let mut s = Sketch::new();
    let sp2 = spline(&mut s, &[[0.0, 0.0], [10.0, 2.0], [20.0, 0.0]]);
    s.add_line(PtSpec::At(40.0, -50.0), PtSpec::At(40.0, 50.0)).unwrap();
    let before = s.beziers(sp2);
    s.extend(sp2, [20.0, 0.0]).unwrap();
    let after = s.beziers(sp2);
    assert!(dist(after.last().unwrap()[3], [40.0, after.last().unwrap()[3][1]]) < 1e-6);
    assert!((after.last().unwrap()[3][0] - 40.0).abs() < 1e-6);
    let dev = deviation(&after, &before);
    assert!(dev < 0.1, "the old part keeps close to its shape: {dev}");
    assert!(s.solve().ok());
    // an arc runs round its circle to the next line
    let mut s = Sketch::new();
    let a = s.add_arc_center(PtSpec::At(0.0, 0.0), PtSpec::At(10.0, 0.0), [0.0, 10.0]).unwrap();
    s.add_line(PtSpec::At(-20.0, -5.0), PtSpec::At(-1.0, -5.0)).unwrap();
    s.extend(a, [0.0, 10.0]).unwrap();
    let Some(Geo::Arc { sweep, .. }) = s.geo(a) else { panic!() };
    assert!(sweep > PI && sweep < 1.5 * PI, "{sweep}");
    assert!(s.solve().ok());
  }

  #[test]
  fn offset_splines_ellipses_and_mixed_chains() {
    let mut s = Sketch::new();
    let sp = spline(&mut s, &[[0.0, 0.0], [10.0, 5.0], [20.0, 0.0], [30.0, 5.0]]);
    let src = s.beziers(sp);
    let out = s.offset(&[sp], 2.0).unwrap();
    assert_eq!(out.len(), 1);
    let off = s.beziers(out[0]);
    for b in &off {
      for k in 0..=8 {
        let p = bz_at(b, k as f64 / 8.0);
        let d = path_distance(&src, p);
        // the ends sit square to the original's ends, so they are exactly 2 away too
        assert!((d - 2.0).abs() < 1e-4, "{d}");
      }
    }
    assert!(s.solve().ok());
    // an ellipse offsets to a closed outline around it
    let mut s = Sketch::new();
    let c = s.add_point(0.0, 0.0);
    let m = s.add_point(20.0, 0.0);
    let e = s.add_curve(CurveKind::Ellipse { center: c, major: m, minor: 10.0 }).unwrap();
    let out = s.offset(&[e], 1.0).unwrap();
    let bz = s.beziers(out[0]);
    assert!(path_closed(&bz));
    let src = s.beziers(e);
    assert!(bz.iter().all(|b| (path_distance(&src, bz_at(b, 0.5)).abs() - 1.0).abs() < 1e-4));
    assert!(s.solve().ok());
    let areas: Vec<f64> = find_profiles(&s).iter().map(|p| p.area()).collect();
    assert_eq!(areas.len(), 2, "the ring and the ellipse");
    // a D shape: a spline closed by a line, offset outward
    let mut s = Sketch::new();
    let sp = spline(&mut s, &[[0.0, 0.0], [10.0, 8.0], [20.0, 0.0]]);
    let pts = s.curve_points(sp);
    let (line, _) = s.add_line(PtSpec::Point(pts[2]), PtSpec::Point(pts[0])).unwrap();
    let out = s.offset(&[sp, line], 1.0).unwrap();
    assert!(out.len() >= 2);
    assert!(s.solve().ok());
    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 2, "{:?}", profiles.iter().map(|p| p.area()).collect::<Vec<_>>());
  }

  #[test]
  fn blend_curves_are_tangent_or_curvature_continuous() {
    for g2 in [false, true] {
      let mut s = Sketch::new();
      let (l, _) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(10.0, 0.0)).unwrap();
      let a = s.add_arc_center(PtSpec::At(30.0, 15.0), PtSpec::At(30.0, 10.0), [35.0, 15.0]).unwrap();
      let b = s.blend(l, a, None, g2).unwrap();
      let r = s.solve();
      assert!(r.ok(), "{r:?}");
      let bz = s.beziers(b);
      let (first, last) = (bz[0], bz[bz.len() - 1]);
      assert!(dist(first[0], [10.0, 0.0]) < 1e-9, "starts at the line's end");
      assert!(cross(unit(bz_d1(&first, 0.0)).unwrap(), [1.0, 0.0]).abs() < 1e-6, "tangent to the line");
      // the arc starts at (30, 10) heading +x round (30, 15): the blend arrives heading +x
      assert!(dist(last[3], [30.0, 10.0]) < 1e-9);
      assert!(cross(unit(bz_d1(&last, 1.0)).unwrap(), [1.0, 0.0]).abs() < 1e-6);
      if g2 {
        assert!(bz_curvature(&first, 0.0).abs() < 1e-6, "flat like the line");
        assert!((bz_curvature(&last, 1.0) - 0.2).abs() < 1e-6, "{}", bz_curvature(&last, 1.0));
      }
      // still smooth after the arc moves
      let centre = s.curve_points(a)[0];
      assert!(s.drag(centre, [32.0, 18.0]).ok());
      let bz = s.beziers(b);
      let last = bz[bz.len() - 1];
      let ccw_at_start = {
        let CurveKind::Arc { center, start, .. } = s.curve(a).unwrap().kind.clone() else { panic!() };
        let (c, p) = (s.pos(center).unwrap(), s.pos(start).unwrap());
        left(unit(sub(p, c)).unwrap())
      };
      assert!(cross(unit(bz_d1(&last, 1.0)).unwrap(), ccw_at_start).abs() < 1e-6);
    }
  }

  #[test]
  fn spline_tangent_curvature_and_point_on_curve_constraints() {
    let mut s = Sketch::new();
    let (l, [_, e]) = s.add_line(PtSpec::At(-10.0, 0.0), PtSpec::At(0.0, 0.0)).unwrap();
    let p1 = s.add_point(10.0, 10.0);
    let p2 = s.add_point(20.0, 0.0);
    let sp = s.add_curve(CurveKind::spline(vec![e, p1, p2])).unwrap();
    s.add_constraint(ConstraintKind::Tangent { a: l, b: sp }).unwrap();
    let CurveKind::Spline { start_handle, end_handle, .. } = s.curve(sp).unwrap().kind.clone() else { panic!() };
    assert!(start_handle.is_some() && end_handle.is_none(), "a handle at the joint end");
    s.add_constraint(ConstraintKind::Horizontal { line: l }).unwrap();
    assert!(s.solve().ok());
    let bz = s.beziers(sp);
    assert!(cross(unit(bz_d1(&bz[0], 0.0)).unwrap(), [1.0, 0.0]).abs() < 1e-7);
    assert!(dist(s.pos(p1).unwrap(), [10.0, 10.0]) < 1.0, "the handle turns, the fit points hardly move");
    // curvature continuity with an arc at the other end
    let a = s.add_arc_center(PtSpec::At(25.0, 0.0), PtSpec::Point(p2), [30.0, 0.0]).unwrap();
    s.add_constraint(ConstraintKind::Curvature { a: sp, b: a }).unwrap();
    s.add_constraint(ConstraintKind::Radius { curve: a, value: Dim::new(5.0) }).unwrap();
    let r = s.solve();
    assert!(r.ok(), "{r:?}");
    // the curve's own curvature there (its cubic pieces match its tangents, not its curvature)
    let kind = s.curve(sp).unwrap().kind.clone();
    let CurveKind::Spline { end_handle, end_curvature, .. } = &kind else { panic!() };
    assert!(end_handle.is_some() && end_curvature.is_some(), "tangent and curvature handles at the joint");
    let (_, _, k) = end_frame(&kind, true, &|p| s.pos(p)).unwrap();
    assert!((k.abs() - 0.2).abs() < 1e-6, "{k}");
    let bz = s.beziers(sp);
    assert!((bz_curvature(&bz[bz.len() - 1], 1.0).abs() - 0.2).abs() < 0.05, "the pieces come close");
    // a point held on the spline
    let q = s.add_point(12.0, 3.0);
    s.add_constraint(ConstraintKind::PointOnCurve { p: q, curve: sp }).unwrap();
    assert!(s.solve().ok());
    assert!(s.distance_to_curve(sp, s.pos(q).unwrap()).unwrap() < 1e-5);
    // two lines are still not "tangent", and an ellipse has no ends to meet at
    let (l2, _) = s.add_line(PtSpec::At(0.0, 30.0), PtSpec::At(5.0, 30.0)).unwrap();
    assert!(s.add_constraint(ConstraintKind::Tangent { a: l, b: l2 }).is_err());
    assert!(s.add_constraint(ConstraintKind::Curvature { a: l, b: l2 }).is_err());
    let c = s.add_point(0.0, 60.0);
    let m = s.add_point(9.0, 60.0);
    let el = s.add_curve(CurveKind::Ellipse { center: c, major: m, minor: 3.0 }).unwrap();
    assert!(s.add_constraint(ConstraintKind::Tangent { a: el, b: sp }).is_err());
    // a spline with handles survives a save
    let back: Sketch = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back, s);
  }
}
