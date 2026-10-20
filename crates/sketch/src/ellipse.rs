//! Ellipses and elliptical arcs: exact geometry and Bézier pieces, profile splitting, and the
//! modify tools (Trim, Break, Extend) that leave what remains of an ellipse an elliptical arc.
//!
//! An ellipse is given by its centre, the end of one axis (`major`, the point at parameter 0) and
//! the radius along that axis turned a quarter left (`minor`, which may be the larger one). The
//! point at parameter t is `c + a·cos t·u + b·sin t·v`; arcs run counter-clockwise in t.

use std::f64::consts::{FRAC_PI_4, PI, TAU};

use crate::edit::Geo;
use crate::freeform::{Bz, Shape, intersect};
use crate::{ConstraintKind, CurveId, CurveKind, PointId, Seg, Sketch, SketchError, ccw_sweep, dist};

type P = [f64; 2];

/// Largest parameter step of the Bézier pieces the modify tools work on (about 1e-11 of the
/// radius from the true ellipse).
const FINE_STEP: f64 = PI / 32.0;

/// An ellipse in the sketch plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ellipse2 {
  pub center: P,
  /// Unit direction of the `a` axis (parameter 0).
  pub dir: P,
  /// Radius along `dir`.
  pub a: f64,
  /// Radius along `dir` turned a quarter left.
  pub b: f64,
}

impl Ellipse2 {
  /// From the centre, the end of the `a` axis and the other radius. None when degenerate.
  pub fn new(center: P, major: P, minor: f64) -> Option<Ellipse2> {
    let (dx, dy) = (major[0] - center[0], major[1] - center[1]);
    let a = dx.hypot(dy);
    (a > 1e-12 && minor.is_finite() && minor > 1e-12).then(|| Ellipse2 { center, dir: [dx / a, dy / a], a, b: minor })
  }

  fn v(&self) -> P {
    [-self.dir[1], self.dir[0]]
  }

  /// The semi-axis vector at parameter 0.
  pub fn major_vec(&self) -> P {
    [self.dir[0] * self.a, self.dir[1] * self.a]
  }

  pub fn at(&self, t: f64) -> P {
    let (s, c) = t.sin_cos();
    let v = self.v();
    [
      self.center[0] + self.a * c * self.dir[0] + self.b * s * v[0],
      self.center[1] + self.a * c * self.dir[1] + self.b * s * v[1],
    ]
  }

  /// Derivative with respect to the parameter.
  pub fn d1(&self, t: f64) -> P {
    let (s, c) = t.sin_cos();
    let v = self.v();
    [-self.a * s * self.dir[0] + self.b * c * v[0], -self.a * s * self.dir[1] + self.b * c * v[1]]
  }

  /// Coordinates of `p` in the ellipse's axes.
  fn local(&self, p: P) -> P {
    let d = [p[0] - self.center[0], p[1] - self.center[1]];
    let v = self.v();
    [d[0] * self.dir[0] + d[1] * self.dir[1], d[0] * v[0] + d[1] * v[1]]
  }

  /// The parameter of a point on (or near) the ellipse, in (-π, π].
  pub fn param(&self, p: P) -> f64 {
    let q = self.local(p);
    (q[1] / self.b).atan2(q[0] / self.a)
  }

  /// Zero exactly on the ellipse, about the distance from it nearby (negative inside).
  pub fn residual(&self, p: P) -> f64 {
    let q = self.local(p);
    ((q[0] / self.a).hypot(q[1] / self.b) - 1.0) * self.a.min(self.b)
  }

  /// Distance from `p` to the nearest point of the whole ellipse.
  pub fn distance(&self, p: P) -> f64 {
    let t = self.nearest(p);
    dist(self.at(t), p)
  }

  /// Parameter of the point of the whole ellipse nearest `p`.
  pub fn nearest(&self, p: P) -> f64 {
    let mut best = (f64::INFINITY, 0.0);
    for k in 0..64 {
      let t = TAU * k as f64 / 64.0;
      let d = dist(self.at(t), p);
      if d < best.0 {
        best = (d, t);
      }
    }
    let mut t = best.1;
    for _ in 0..30 {
      let (e, d1) = (self.at(t), self.d1(t));
      let q = [e[0] - p[0], e[1] - p[1]];
      // f = (E - p)·E', f' = E'·E' + (E - p)·E'' with E'' = c - E
      let d2 = [self.center[0] - e[0], self.center[1] - e[1]];
      let f = q[0] * d1[0] + q[1] * d1[1];
      let df = d1[0] * d1[0] + d1[1] * d1[1] + q[0] * d2[0] + q[1] * d2[1];
      if df.abs() < 1e-300 {
        break;
      }
      let step = (f / df).clamp(-0.5, 0.5);
      t -= step;
      if step.abs() < 1e-15 {
        break;
      }
    }
    t
  }

  /// Cubic Bézier pieces from parameter `t0` sweeping `sweep` (either sign), each spanning at
  /// most `max_step`.
  pub fn beziers(&self, t0: f64, sweep: f64, max_step: f64) -> Vec<Bz> {
    if !(sweep.abs() > 1e-15) || !sweep.is_finite() {
      return Vec::new();
    }
    let n = (sweep.abs() / max_step).ceil().clamp(1.0, 4096.0) as usize;
    let h = sweep / n as f64;
    let k = 4.0 / 3.0 * (h / 4.0).tan();
    (0..n)
      .map(|i| {
        let (s, e) = (t0 + h * i as f64, t0 + h * (i + 1) as f64);
        let (p0, p3, d0, d3) = (self.at(s), self.at(e), self.d1(s), self.d1(e));
        [p0, [p0[0] + k * d0[0], p0[1] + k * d0[1]], [p3[0] - k * d3[0], p3[1] - k * d3[1]], p3]
      })
      .collect()
  }

  /// The profile segment from `t0` sweeping `sweep`.
  pub fn seg(&self, t0: f64, sweep: f64) -> Seg {
    Seg::EllipseArc { center: self.center, major: self.major_vec(), minor: self.b, start: t0, sweep }
  }

  /// Parameters where the whole ellipse meets a line (as an infinite line) or an arc (as its whole
  /// circle).
  fn meets(&self, g: &Geo) -> Vec<f64> {
    match *g {
      Geo::Line { a, b } => {
        // in the ellipse's unit-circle frame the line stays a line
        let (la, lb) = (self.local(a), self.local(b));
        let (pa, pb) = ([la[0] / self.a, la[1] / self.b], [lb[0] / self.a, lb[1] / self.b]);
        let d = [pb[0] - pa[0], pb[1] - pa[1]];
        let dd = d[0] * d[0] + d[1] * d[1];
        if dd < 1e-30 {
          return Vec::new();
        }
        let bq = (pa[0] * d[0] + pa[1] * d[1]) / dd;
        let c = (pa[0] * pa[0] + pa[1] * pa[1] - 1.0) / dd;
        let disc = bq * bq - c;
        if disc < -1e-12 {
          return Vec::new();
        }
        let s = disc.max(0.0).sqrt();
        let roots = if s > 1e-12 { vec![-bq - s, -bq + s] } else { vec![-bq] };
        roots.into_iter().map(|u| (pa[1] + d[1] * u).atan2(pa[0] + d[0] * u)).collect()
      }
      Geo::Arc { c, r, .. } => {
        let f = |t: f64| dist(self.at(t), c) - r;
        let n = 256;
        let mut out = Vec::new();
        let mut prev = (0.0, f(0.0));
        for k in 1..=n {
          let t = TAU * k as f64 / n as f64;
          let v = f(t);
          if prev.1 == 0.0 {
            out.push(prev.0);
          } else if prev.1 * v < 0.0 {
            let (mut lo, mut hi, mut flo) = (prev.0, t, prev.1);
            for _ in 0..60 {
              let m = 0.5 * (lo + hi);
              let fm = f(m);
              if (fm < 0.0) == (flo < 0.0) {
                lo = m;
                flo = fm;
              } else {
                hi = m;
              }
            }
            out.push(0.5 * (lo + hi));
          }
          prev = (t, v);
        }
        out
      }
    }
  }
}

/// Bézier pieces of an elliptical arc (see [`Ellipse2`]): centre, end of the `a` axis, the other
/// radius, and the parameter range.
pub fn ellipse_arc_beziers(center: P, major: P, minor: f64, t0: f64, sweep: f64) -> Vec<[[f64; 2]; 4]> {
  Ellipse2::new(center, major, minor).map(|e| e.beziers(t0, sweep, FRAC_PI_4)).unwrap_or_default()
}

/// An ellipse or elliptical arc: the ellipse, the start parameter and the counter-clockwise
/// sweep (2π for a whole ellipse).
pub(crate) fn arc_of(kind: &CurveKind, pos: &dyn Fn(PointId) -> Option<P>) -> Option<(Ellipse2, f64, f64)> {
  match *kind {
    CurveKind::Ellipse { center, major, minor } => Some((Ellipse2::new(pos(center)?, pos(major)?, minor)?, 0.0, TAU)),
    CurveKind::EllipticalArc { center, major, minor, start, end } => {
      let e = Ellipse2::new(pos(center)?, pos(major)?, minor)?;
      let t0 = e.param(pos(start)?);
      Some((e, t0, ccw_sweep(t0, e.param(pos(end)?))))
    }
    _ => None,
  }
}

/// Profile segments of an ellipse (two halves) or an elliptical arc.
pub(crate) fn kind_segs(kind: &CurveKind, pos: &dyn Fn(PointId) -> Option<P>) -> Vec<Seg> {
  let Some((e, t0, sweep)) = arc_of(kind, pos) else { return Vec::new() };
  if matches!(kind, CurveKind::Ellipse { .. }) || sweep >= TAU - 1e-12 {
    vec![e.seg(t0, PI), e.seg(t0 + PI, PI)]
  } else {
    vec![e.seg(t0, sweep)]
  }
}

/// Splits a profile's elliptical arcs where lines, arcs and circles (`geos`, the first `n_segs`
/// of them segments with ends) cross them or end on them, recording the matching cuts on those
/// curves in `cuts` (parameters along each). An arc's end lying on a curve cuts that curve too.
pub(crate) fn split_profile_arcs(
  arcs: Vec<Seg>,
  geos: &[Geo],
  n_segs: usize,
  cuts: &mut [Vec<f64>],
  tol: f64,
) -> Vec<Seg> {
  let mut out = Vec::new();
  for s in arcs {
    let Seg::EllipseArc { center, major, minor, start, sweep } = s else {
      out.push(s);
      continue;
    };
    let Some(e) = Ellipse2::new(center, [center[0] + major[0], center[1] + major[1]], minor) else { continue };
    // run counter-clockwise; the graph doesn't care which way an edge points
    let (t0, sw) = if sweep >= 0.0 { (start, sweep) } else { (start + sweep, -sweep) };
    let full = sw >= TAU - 1e-12;
    let local = |t: f64| (t - t0).rem_euclid(TAU);
    let within = |u: f64| full || u <= sw + 1e-9 || u >= TAU - 1e-9;
    let ends = if full { vec![] } else { vec![e.at(t0), e.at(t0 + sw)] };
    let mut us: Vec<f64> = Vec::new();
    for (j, g) in geos.iter().enumerate() {
      // ends of the other curve lying on the arc split the arc
      let mut on: Vec<P> = Vec::new();
      if j < n_segs {
        for q in [g.at(0.0), g.at(g.end_param())] {
          let u = local(e.param(q));
          if e.residual(q).abs() <= tol && within(u) {
            us.push(u);
            on.push(q);
          }
        }
      }
      // the arc's ends lying on the other curve split that curve
      for q in &ends {
        if g.distance(*q) <= tol && g.contains(*q) {
          cuts[j].push(g.param(*q));
          on.push(*q);
        }
      }
      for t in e.meets(g) {
        let p = e.at(t);
        let u = local(t);
        if !within(u) || !g.contains(p) || on.iter().any(|q| dist(*q, p) <= 1e-3) {
          continue;
        }
        us.push(u);
        cuts[j].push(g.param(p));
      }
    }
    let ut = tol / e.a.min(e.b).max(tol);
    let mut us: Vec<f64> = us.into_iter().map(|u| if u >= TAU - 1e-9 { 0.0 } else { u }).collect();
    us.retain(|u| full || (*u > ut && *u < sw - ut));
    us.sort_by(f64::total_cmp);
    us.dedup_by(|a, b| (*a - *b).abs() <= ut);
    if full {
      if us.len() > 1 && (us[0] + TAU - us[us.len() - 1]).abs() <= ut {
        us.pop();
      }
      if us.len() < 2 {
        out.push(e.seg(t0, PI));
        out.push(e.seg(t0 + PI, PI));
        continue;
      }
      for k in 0..us.len() {
        let (a, b) = (us[k], if k + 1 < us.len() { us[k + 1] } else { us[0] + TAU });
        out.push(e.seg(t0 + a, b - a));
      }
      continue;
    }
    let mut prev = 0.0;
    for u in us.into_iter().chain([sw]) {
      if u - prev > ut {
        out.push(e.seg(t0 + prev, u - prev));
      }
      prev = u;
    }
  }
  out
}

impl Sketch {
  /// An ellipse or elliptical arc as fine Bézier pieces (what the modify tools cut).
  pub(crate) fn elliptic_path(&self, id: CurveId) -> Option<Vec<Bz>> {
    let (e, t0, sweep) = arc_of(&self.curve(id)?.kind, &|p| self.pos(p))?;
    Some(e.beziers(t0, sweep, FINE_STEP))
  }

  /// The ellipse of an ellipse or elliptical arc, its start parameter and sweep.
  pub(crate) fn elliptic(&self, id: CurveId) -> Option<(Ellipse2, f64, f64)> {
    arc_of(&self.curve(id)?.kind, &|p| self.pos(p))
  }

  /// A new point on the ellipse at parameter `t`.
  fn point_on(&mut self, e: &Ellipse2, t: f64) -> PointId {
    let p = e.at(t);
    self.add_point(p[0], p[1])
  }

  /// [`Sketch::trim`] for ellipses and elliptical arcs: what is left stays elliptical arcs on the
  /// same ellipse (sharing its centre and axis points).
  pub(crate) fn trim_elliptic(&mut self, curve: CurveId, at: P) -> Result<Vec<CurveId>, SketchError> {
    let c = self.curve(curve).ok_or(SketchError::NoCurve(curve))?.clone();
    let (e, t0, sweep) = self.elliptic(curve).ok_or(SketchError::NoCurve(curve))?;
    let (center, major, minor) = match c.kind {
      CurveKind::Ellipse { center, major, minor } | CurveKind::EllipticalArc { center, major, minor, .. } => {
        (center, major, minor)
      }
      _ => return Err(SketchError::NoCurve(curve)),
    };
    let full = matches!(c.kind, CurveKind::Ellipse { .. });
    let shape = self.shape(curve).ok_or(SketchError::NoCurve(curve))?;
    let local = |t: f64| {
      let u = (t - t0).rem_euclid(TAU);
      if !full && u > sweep + 1e-9 { if u - sweep < TAU - u { sweep } else { 0.0 } } else { u }
    };
    let tp = local(e.param(at));
    let mut cuts: Vec<(f64, CurveId)> = self
      .crossings_any(curve)?
      .into_iter()
      .map(|(s, o)| (local(e.param(shape.at(s))), o))
      .filter(|(u, _)| full || (*u > 1e-9 && *u < sweep - 1e-9))
      .collect();
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    cuts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
    let arc = |start: PointId, end: PointId| CurveKind::EllipticalArc { center, major, minor, start, end };
    if full {
      if cuts.len() < 2 {
        self.remove_curve(curve)?;
        return Ok(vec![]);
      }
      let i = cuts.iter().rposition(|(u, _)| *u <= tp).unwrap_or(cuts.len() - 1);
      let (lo, hi) = (cuts[i], cuts[(i + 1) % cuts.len()]);
      let (ps, pe) = (self.point_on(&e, t0 + hi.0), self.point_on(&e, t0 + lo.0));
      if let Some(c) = self.curve_mut(curve) {
        c.kind = arc(ps, pe);
      }
      self.add_constraint(ConstraintKind::PointOnCurve { p: ps, curve: hi.1 })?;
      self.add_constraint(ConstraintKind::PointOnCurve { p: pe, curve: lo.1 })?;
      return Ok(vec![curve]);
    }
    let CurveKind::EllipticalArc { start, end, .. } = c.kind else { return Err(SketchError::NoCurve(curve)) };
    let lo = cuts.iter().rev().find(|(u, _)| *u < tp).copied();
    let hi = cuts.iter().find(|(u, _)| *u > tp).copied();
    let (sp, ep) = (e.at(t0), e.at(t0 + sweep));
    let made = match (lo, hi) {
      (None, None) => {
        self.remove_curve(curve)?;
        return Ok(vec![]);
      }
      (None, Some((u, other))) => {
        let np = self.point_on(&e, t0 + u);
        self.drop_smooth_at(curve, sp);
        self.swap_point(curve, start, np);
        self.add_constraint(ConstraintKind::PointOnCurve { p: np, curve: other })?;
        vec![curve]
      }
      (Some((u, other)), None) => {
        let np = self.point_on(&e, t0 + u);
        self.drop_smooth_at(curve, ep);
        self.swap_point(curve, end, np);
        self.add_constraint(ConstraintKind::PointOnCurve { p: np, curve: other })?;
        vec![curve]
      }
      (Some((u0, o0)), Some((u1, o1))) => {
        let (n0, n1) = (self.point_on(&e, t0 + u0), self.point_on(&e, t0 + u1));
        self.swap_point(curve, end, n0);
        let second = self.add_curve(arc(n1, end))?;
        self.set_construction(second, c.construction)?;
        self.retarget_smooth(curve, second, ep);
        self.add_constraint(ConstraintKind::PointOnCurve { p: n0, curve: o0 })?;
        self.add_constraint(ConstraintKind::PointOnCurve { p: n1, curve: o1 })?;
        vec![curve, second]
      }
    };
    for p in [start, end] {
      self.drop_if_unused(p);
    }
    Ok(made)
  }

  /// Lengthens an elliptical arc round its ellipse from the end nearest `at` to the next curve
  /// it reaches.
  pub(crate) fn extend_elliptic(&mut self, curve: CurveId, at: P) -> Result<(), SketchError> {
    let Some(CurveKind::EllipticalArc { start, end, .. }) = self.curve(curve).map(|c| c.kind.clone()) else {
      return Err(SketchError::WrongGeometry("extend", "a line, arc, elliptical arc or spline"));
    };
    let (e, t0, sweep) = self.elliptic(curve).ok_or(SketchError::NoCurve(curve))?;
    let from_end = dist(at, e.at(t0 + sweep)) <= dist(at, e.at(t0));
    let gap = TAU - sweep;
    // the rest of the ellipse, running away from the end being extended
    let rest = if from_end { e.beziers(t0 + sweep, gap, FINE_STEP) } else { e.beziers(t0, -gap, FINE_STEP) };
    let ext = Shape::Free(rest);
    let mut best: Option<(f64, P, CurveId)> = None;
    for o in self.curves.iter().filter(|o| o.id != curve) {
      let Some(h) = self.shape(o.id) else { continue };
      for (u, p) in intersect(&ext, &h) {
        if u > 1e-9 && best.is_none_or(|b| u < b.0) {
          best = Some((u, p, o.id));
        }
      }
    }
    let (_, p, other) = best.ok_or(SketchError::InvalidValue("nothing to extend the elliptical arc to".into()))?;
    let p = e.at(e.param(p));
    self.move_end(curve, if from_end { end } else { start }, p, other)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{PtSpec, find_profiles};

  fn ellipse(s: &mut Sketch, c: P, m: P, minor: f64) -> CurveId {
    let (c, m) = (s.add_point(c[0], c[1]), s.add_point(m[0], m[1]));
    s.add_curve(CurveKind::Ellipse { center: c, major: m, minor }).unwrap()
  }

  fn on_ellipse(s: &Sketch, id: CurveId, p: PointId) -> f64 {
    let (e, _, _) = s.elliptic(id).unwrap();
    e.residual(s.pos(p).unwrap()).abs()
  }

  #[test]
  fn geometry_round_trips() {
    let e = Ellipse2::new([1.0, 2.0], [1.0 + 6.0, 2.0 + 8.0], 4.0).unwrap();
    for t in [-3.0, -1.0, 0.0, 0.5, 2.0, 3.1] {
      let p = e.at(t);
      assert!((e.param(p) - t).abs() < 1e-12);
      assert!(e.residual(p).abs() < 1e-12);
    }
    assert!((e.distance([1.0, 2.0]) - 4.0).abs() < 1e-9, "the centre is the minor radius away");
    for b in e.beziers(0.3, 2.0, FRAC_PI_4) {
      for k in 0..=8 {
        assert!(e.residual(crate::bezier_at(&b, k as f64 / 8.0)).abs() < 1e-4);
      }
    }
    let fine = e.beziers(0.0, TAU, FINE_STEP);
    assert!(fine.iter().all(|b| e.residual(crate::bezier_at(b, 0.5)).abs() < 1e-9));
    // a line through the centre meets it twice, a far one not at all
    let ts = e.meets(&Geo::Line { a: [1.0, 2.0], b: [2.0, 2.0] });
    assert_eq!(ts.len(), 2);
    assert!(ts.iter().all(|t| (e.at(*t)[1] - 2.0).abs() < 1e-9));
    assert!(e.meets(&Geo::Line { a: [100.0, 0.0], b: [100.0, 1.0] }).is_empty());
    let ts = e.meets(&Geo::Arc { c: [1.0, 2.0], r: 6.0, a0: 0.0, sweep: TAU });
    assert_eq!(ts.len(), 4);
    assert!(ts.iter().all(|t| (dist(e.at(*t), [1.0, 2.0]) - 6.0).abs() < 1e-9));
  }

  #[test]
  fn trimming_an_ellipse_leaves_an_elliptical_arc() {
    let mut s = Sketch::new();
    let e = ellipse(&mut s, [0.0, 0.0], [20.0, 0.0], 10.0);
    let (l, _) = s.add_line(PtSpec::At(-30.0, 0.0), PtSpec::At(30.0, 0.0)).unwrap();
    s.trim(e, [0.0, 10.0]).unwrap();
    let CurveKind::EllipticalArc { start, end, .. } = s.curve(e).unwrap().kind.clone() else { panic!("an arc") };
    assert!(dist(s.pos(start).unwrap(), [-20.0, 0.0]) < 1e-9);
    assert!(dist(s.pos(end).unwrap(), [20.0, 0.0]) < 1e-9);
    let (_, _, sweep) = s.elliptic(e).unwrap();
    assert!((sweep - PI).abs() < 1e-9, "the lower half: {sweep}");
    assert!(s.solve().ok());
    // the half ellipse and the line make a D
    let p = find_profiles(&s);
    assert_eq!(p.len(), 1);
    assert!((p[0].area() - PI * 200.0 / 2.0).abs() < 0.5, "{}", p[0].area());
    assert!(p[0].outer.segs.iter().any(|g| matches!(g, Seg::EllipseArc { .. })), "an exact segment");
    // the line trims against the arc; the arc then trims again into two arcs
    let parts = s.trim(l, [0.0, 0.0]).unwrap();
    assert_eq!(parts.len(), 2);
    s.add_line(PtSpec::At(-5.0, 5.0), PtSpec::At(-5.0, -15.0)).unwrap();
    s.add_line(PtSpec::At(5.0, 5.0), PtSpec::At(5.0, -15.0)).unwrap();
    let arcs = s.trim(e, [0.0, -10.0]).unwrap();
    assert_eq!(arcs.len(), 2);
    for a in &arcs {
      assert!(matches!(s.curve(*a).unwrap().kind, CurveKind::EllipticalArc { .. }));
      for p in &s.curve_points(*a)[2..] {
        assert!(on_ellipse(&s, *a, *p) < 1e-9);
      }
    }
    assert!(s.solve().ok());
    // old files and new files both round-trip
    let back: Sketch = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back, s);
  }

  #[test]
  fn arcs_break_extend_and_stay_on_their_ellipse() {
    let mut s = Sketch::new();
    let e = ellipse(&mut s, [0.0, 0.0], [20.0, 0.0], 10.0);
    s.add_line(PtSpec::At(0.0, -20.0), PtSpec::At(0.0, 20.0)).unwrap();
    let parts = s.break_curve(e, [20.0, 0.0], false, 0.0).unwrap();
    assert_eq!(parts.len(), 2);
    assert!(parts.iter().all(|p| matches!(s.curve(*p).unwrap().kind, CurveKind::EllipticalArc { .. })));
    // delete one half, then trim the other a bit: it extends back to the line
    s.remove_curve(parts[1]).unwrap();
    let (_, _, sweep) = s.elliptic(parts[0]).unwrap();
    assert!((sweep - PI).abs() < 1e-9);
    let (cut, _) = s.add_line(PtSpec::At(-10.0, 0.0), PtSpec::At(-10.0, 20.0)).unwrap();
    s.trim(parts[0], [-5.0, 9.7]).unwrap();
    s.remove_curve(cut).unwrap();
    let (_, _, short) = s.elliptic(parts[0]).unwrap();
    assert!(short < PI - 0.1, "{short}");
    s.extend(parts[0], [-9.0, 8.9]).unwrap();
    let (e2, t0, sweep) = s.elliptic(parts[0]).unwrap();
    assert!((sweep - PI).abs() < 1e-6, "{sweep}");
    assert!(dist(e2.at(t0 + sweep), [0.0, 10.0]).min(dist(e2.at(t0), [0.0, 10.0])) < 1e-6);
    assert!(s.solve().ok());
    // the arc's ends stay on the ellipse when its centre is dragged
    let c = s.curve_points(parts[0])[0];
    assert!(s.drag(c, [1.0, 1.0]).ok());
    for p in &s.curve_points(parts[0])[2..] {
      assert!(on_ellipse(&s, parts[0], *p) < 1e-7);
    }
  }

  #[test]
  fn elliptical_arcs_take_tangent_and_point_on_curve_constraints() {
    let mut s = Sketch::new();
    let c = s.add_point(0.0, 0.0);
    let m = s.add_point(20.0, 0.0);
    let a = s.add_point(20.0, 0.0);
    let b = s.add_point(-20.0, 0.0);
    let arc = s.add_curve(CurveKind::EllipticalArc { center: c, major: m, minor: 10.0, start: a, end: b }).unwrap();
    s.add_constraint(ConstraintKind::Coincident { a: m, b: a }).unwrap();
    let (l, [l0, _]) = s.add_line(PtSpec::Point(a), PtSpec::At(21.0, -15.0)).unwrap();
    s.add_constraint(ConstraintKind::Tangent { a: arc, b: l }).unwrap();
    let q = s.add_point(0.0, 9.0);
    s.add_constraint(ConstraintKind::PointOnCurve { p: q, curve: arc }).unwrap();
    s.add_constraint(ConstraintKind::Fix { p: c, x: 0.0, y: 0.0 }).unwrap();
    let r = s.solve();
    assert!(r.ok(), "{r:?}");
    assert!(on_ellipse(&s, arc, q) < 1e-7);
    assert!(on_ellipse(&s, arc, b) < 1e-7);
    // the line leaves the arc's start along its tangent (vertical at the axis end)
    let (pl, pa) = (s.pos(s.curve_points(l)[1]).unwrap(), s.pos(l0).unwrap());
    let (e, t0, _) = s.elliptic(arc).unwrap();
    let d = e.d1(t0);
    let cr = (pl[0] - pa[0]) * d[1] - (pl[1] - pa[1]) * d[0];
    assert!(cr.abs() / dist(pl, pa) / d[0].hypot(d[1]) < 1e-6, "{cr}");
  }

  #[test]
  fn profiles_split_ellipses_where_lines_and_circles_cross_or_end_on_them() {
    // an ellipse crossed by a line: two halves
    let mut s = Sketch::new();
    ellipse(&mut s, [0.0, 0.0], [20.0, 0.0], 10.0);
    s.add_line(PtSpec::At(-30.0, 0.0), PtSpec::At(30.0, 0.0)).unwrap();
    let p = find_profiles(&s);
    assert_eq!(p.len(), 2, "two half ellipses");
    assert!(
      p.iter().all(|r| (r.area() - PI * 100.0).abs() < 0.5),
      "{:?}",
      p.iter().map(|r| r.area()).collect::<Vec<_>>()
    );
    // a line from the centre ending on the ellipse, and another: two sectors
    let mut s = Sketch::new();
    ellipse(&mut s, [0.0, 0.0], [20.0, 0.0], 10.0);
    s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(20.0, 0.0)).unwrap();
    let o = s.curve_points(s.curves[1].id)[0];
    s.add_line(PtSpec::Point(o), PtSpec::At(0.0, 10.0)).unwrap();
    let p = find_profiles(&s);
    assert_eq!(p.len(), 2, "{:?}", p.iter().map(|r| r.area()).collect::<Vec<_>>());
    let total: f64 = p.iter().map(|r| r.area()).sum();
    assert!((total - PI * 200.0).abs() < 1.0, "{total}");
    // a circle through the ellipse: lens and the rest
    let mut s = Sketch::new();
    ellipse(&mut s, [0.0, 0.0], [20.0, 0.0], 10.0);
    s.add_circle(PtSpec::At(20.0, 0.0), 6.0).unwrap();
    let p = find_profiles(&s);
    assert_eq!(p.len(), 3, "{:?}", p.iter().map(|r| r.area()).collect::<Vec<_>>());
  }
}
