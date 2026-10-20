//! Sketch modify tools, as in Fusion's sketch Modify and Create menus: fillet, offset, trim,
//! extend, mirror, and rectangular / circular patterns.

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

use crate::{
  ConstraintKind, CurveId, CurveKind, Dim, PointId, Sketch, SketchError, angle_of, ccw_sweep, curve_points, dist,
};

type P = [f64; 2];

const EPS: f64 = 1e-9;

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

fn unit(a: P) -> Option<P> {
  let l = a[0].hypot(a[1]);
  (l > EPS).then(|| mul(a, 1.0 / l))
}

fn polar(c: P, r: f64, a: f64) -> P {
  [c[0] + r * a.cos(), c[1] + r * a.sin()]
}

/// One direction of a sketch rectangular pattern, as Fusion's sketch Rectangular Pattern sets it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternDir {
  pub direction: P,
  /// Instances along this direction, the original included.
  pub count: usize,
  /// The spacing between neighbours, or the whole pattern's size when `extent` is set.
  pub distance: f64,
  pub extent: bool,
  /// Centre the instances on the original (`count` in total) instead of running one way.
  pub symmetric: bool,
}

impl PatternDir {
  /// `count` instances along `direction` every `spacing`, one way from the original.
  pub fn spacing(direction: P, count: usize, spacing: f64) -> Self {
    Self { direction, count, distance: spacing, extent: false, symmetric: false }
  }

  /// Index of the original among `offsets`. A symmetric pattern with an even count puts the
  /// extra instance on the positive side.
  fn original(&self) -> usize {
    if self.symmetric { (self.count.max(1) - 1) / 2 } else { 0 }
  }

  /// Offsets of the instances along the direction, in order; the original's is zero.
  fn offsets(&self) -> Result<Vec<f64>, SketchError> {
    if !self.distance.is_finite() {
      return Err(SketchError::InvalidValue("pattern distance must be a number".into()));
    }
    let step = match (self.extent, self.count) {
      (false, _) => self.distance,
      (true, n) if n > 1 => self.distance / (n - 1) as f64,
      (true, _) => 0.0,
    };
    let first = self.original() as f64;
    Ok((0..self.count).map(|k| step * (k as f64 - first)).collect())
  }
}

/// A curve's current geometry.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Geo {
  Line {
    a: P,
    b: P,
  },
  /// Counter-clockwise from `a0` by `sweep` (a full circle has sweep 2π).
  Arc {
    c: P,
    r: f64,
    a0: f64,
    sweep: f64,
  },
}

impl Geo {
  /// A profile segment's geometry (circles start at angle 0).
  pub(crate) fn of_seg(s: &crate::Seg) -> Option<Geo> {
    Some(match *s {
      crate::Seg::Line { a, b } => Geo::Line { a, b },
      crate::Seg::Arc { center, radius, start_angle, sweep } => {
        if sweep >= 0.0 {
          Geo::Arc { c: center, r: radius, a0: start_angle, sweep }
        } else {
          Geo::Arc { c: center, r: radius, a0: start_angle + sweep, sweep: -sweep }
        }
      }
      crate::Seg::Circle { center, radius, .. } => Geo::Arc { c: center, r: radius, a0: 0.0, sweep: TAU },
      crate::Seg::Bezier { .. } | crate::Seg::EllipseArc { .. } => return None,
    })
  }

  /// Parameter of a point on the (extended) curve: line 0..1, arc 0..sweep.
  pub(crate) fn param(&self, p: P) -> f64 {
    match *self {
      Geo::Line { a, b } => {
        let d = sub(b, a);
        dot(sub(p, a), d) / dot(d, d).max(EPS)
      }
      Geo::Arc { c, a0, .. } => (angle_of(c, p) - a0).rem_euclid(TAU),
    }
  }

  pub(crate) fn at(&self, t: f64) -> P {
    match *self {
      Geo::Line { a, b } => add(a, mul(sub(b, a), t)),
      Geo::Arc { c, r, a0, .. } => polar(c, r, a0 + t),
    }
  }

  pub(crate) fn end_param(&self) -> f64 {
    match *self {
      Geo::Line { .. } => 1.0,
      Geo::Arc { sweep, .. } => sweep,
    }
  }

  pub(crate) fn full(&self) -> bool {
    matches!(*self, Geo::Arc { sweep, .. } if sweep >= TAU - 1e-12)
  }

  /// Distance from a point to the extended curve (infinite line, full circle).
  pub(crate) fn distance(&self, p: P) -> f64 {
    match *self {
      Geo::Line { a, b } => match unit(sub(b, a)) {
        Some(d) => cross(d, sub(p, a)).abs(),
        None => dist(a, p),
      },
      Geo::Arc { c, r, .. } => (dist(c, p) - r).abs(),
    }
  }

  /// Is a point of the (extended) curve within its extent?
  pub(crate) fn contains(&self, p: P) -> bool {
    let t = self.param(p);
    match *self {
      Geo::Line { a, b } => {
        let tol = 1e-9 / dist(a, b).max(EPS);
        (-tol..=1.0 + tol).contains(&t)
      }
      Geo::Arc { sweep, .. } => self.full() || t <= sweep + 1e-9 || t >= TAU - 1e-9,
    }
  }
}

/// Points where the extended curves meet (lines as infinite lines, arcs as full circles).
pub(crate) fn meet(g: &Geo, h: &Geo) -> Vec<P> {
  match (*g, *h) {
    (Geo::Line { a, b }, Geo::Line { a: c, b: d }) => {
      let (r, s) = (sub(b, a), sub(d, c));
      let den = cross(r, s);
      if den.abs() < EPS * r[0].hypot(r[1]) * s[0].hypot(s[1]) {
        return vec![];
      }
      let t = cross(sub(c, a), s) / den;
      vec![add(a, mul(r, t))]
    }
    (Geo::Line { a, b }, Geo::Arc { c, r, .. }) | (Geo::Arc { c, r, .. }, Geo::Line { a, b }) => {
      let Some(d) = unit(sub(b, a)) else { return vec![] };
      let f = sub(a, c);
      let bq = dot(f, d);
      let disc = bq * bq - (dot(f, f) - r * r);
      if disc < -1e-12 {
        return vec![];
      }
      let s = disc.max(0.0).sqrt();
      let mut v = vec![add(a, mul(d, -bq - s))];
      if s > 1e-12 {
        v.push(add(a, mul(d, -bq + s)));
      }
      v
    }
    (Geo::Arc { c: c1, r: r1, .. }, Geo::Arc { c: c2, r: r2, .. }) => {
      let dd = dist(c1, c2);
      if dd < EPS || dd > r1 + r2 + 1e-12 || dd < (r1 - r2).abs() - 1e-12 {
        return vec![];
      }
      let a = (r1 * r1 - r2 * r2 + dd * dd) / (2.0 * dd);
      let h = (r1 * r1 - a * a).max(0.0).sqrt();
      let u = mul(sub(c2, c1), 1.0 / dd);
      let m = add(c1, mul(u, a));
      let n = [-u[1], u[0]];
      let mut v = vec![add(m, mul(n, h))];
      if h > 1e-12 {
        v.push(add(m, mul(n, -h)));
      }
      v
    }
  }
}

impl Sketch {
  pub(crate) fn geo(&self, id: CurveId) -> Option<Geo> {
    Some(match self.curve(id)?.kind {
      CurveKind::Line { a, b } => Geo::Line { a: self.pos(a)?, b: self.pos(b)? },
      CurveKind::Circle { center, radius } => Geo::Arc { c: self.pos(center)?, r: radius, a0: 0.0, sweep: TAU },
      CurveKind::Arc { center, start, end } => {
        let (c, s, e) = (self.pos(center)?, self.pos(start)?, self.pos(end)?);
        let a0 = angle_of(c, s);
        Geo::Arc { c, r: dist(c, s), a0, sweep: ccw_sweep(a0, angle_of(c, e)) }
      }
      CurveKind::Spline { .. }
      | CurveKind::ControlSpline { .. }
      | CurveKind::Ellipse { .. }
      | CurveKind::Bezier { .. }
      | CurveKind::EllipticalArc { .. } => {
        return None;
      }
    })
  }

  /// Where other curves (splines too) cross curve `id`: (parameter along it, the other curve).
  fn crossings(&self, id: CurveId) -> Result<Vec<(f64, CurveId)>, SketchError> {
    self.crossings_any(id)
  }

  /// Uses of a point by curves.
  pub(crate) fn point_users(&self, p: PointId) -> usize {
    self.curves.iter().filter(|c| curve_points(&c.kind).contains(&p)).count()
  }

  /// Replaces point `old` by `new` in curve `id`.
  pub(crate) fn swap_point(&mut self, id: CurveId, old: PointId, new: PointId) {
    if let Some(c) = self.curve_mut(id) {
      match &mut c.kind {
        CurveKind::Line { a, b } => {
          for q in [a, b] {
            if *q == old {
              *q = new;
            }
          }
        }
        CurveKind::Arc { start, end, .. } => {
          for q in [start, end] {
            if *q == old {
              *q = new;
            }
          }
        }
        CurveKind::Spline { points, start_handle, end_handle } => {
          for q in points.iter_mut().chain(start_handle.iter_mut()).chain(end_handle.iter_mut()) {
            if *q == old {
              *q = new;
            }
          }
        }
        CurveKind::ControlSpline { points } | CurveKind::Bezier { points } => {
          for q in points {
            if *q == old {
              *q = new;
            }
          }
        }
        CurveKind::Ellipse { center, major, .. } => {
          for q in [center, major] {
            if *q == old {
              *q = new;
            }
          }
        }
        CurveKind::EllipticalArc { center, major, start, end, .. } => {
          for q in [center, major, start, end] {
            if *q == old {
              *q = new;
            }
          }
        }
        CurveKind::Circle { .. } => {}
      }
    }
  }

  /// Deletes a point no curve uses any more, with its constraints.
  pub(crate) fn drop_if_unused(&mut self, p: PointId) {
    if p != crate::ORIGIN && self.point_users(p) == 0 {
      self.points.retain(|x| x.id != p);
      self.constraints.retain(|c| !c.kind.points().contains(&p));
    }
  }

  // ---------------------------------------------------------------- fillet

  /// Rounds the corner where two lines meet with an arc of `radius`, tangent to both (the lines
  /// are trimmed back). The radius becomes a driving dimension.
  pub fn fillet(&mut self, l1: CurveId, l2: CurveId, radius: f64) -> Result<CurveId, SketchError> {
    self.not_text(l1)?;
    self.not_text(l2)?;
    if !(radius > EPS && radius.is_finite()) {
      return Err(SketchError::InvalidValue(format!("fillet radius {radius}")));
    }
    let ends = |s: &Sketch, id: CurveId| match s.curve(id).map(|c| &c.kind) {
      Some(CurveKind::Line { a, b }) => Ok((*a, *b)),
      _ => Err(SketchError::WrongGeometry("a sketch fillet", "two lines")),
    };
    let (a1, b1) = ends(self, l1)?;
    let (a2, b2) = ends(self, l2)?;
    // the shared corner (by id, else by position)
    let near =
      |s: &Sketch, p: PointId, q: PointId| p == q || s.pos(p).zip(s.pos(q)).is_some_and(|(x, y)| dist(x, y) < 1e-6);
    let (c1, f1, c2, f2) = [(a1, b1, a2, b2), (a1, b1, b2, a2), (b1, a1, a2, b2), (b1, a1, b2, a2)]
      .into_iter()
      .find(|(p, _, q, _)| near(self, *p, *q))
      .ok_or(SketchError::WrongGeometry("a sketch fillet", "two lines that meet at an end"))?;
    let pos = |id| self.pos(id).ok_or(SketchError::NoPoint(id));
    let (p, q1, q2) = (pos(c1)?, pos(f1)?, pos(f2)?);
    let (Some(u), Some(v)) = (unit(sub(q1, p)), unit(sub(q2, p))) else {
      return Err(SketchError::InvalidValue("zero-length line".into()));
    };
    let theta = dot(u, v).clamp(-1.0, 1.0).acos();
    if !(theta > 1e-6 && theta < PI - 1e-6) {
      return Err(SketchError::InvalidValue("the lines are parallel".into()));
    }
    let d = radius / (theta / 2.0).tan();
    if d >= dist(p, q1) - 1e-9 || d >= dist(p, q2) - 1e-9 {
      return Err(SketchError::InvalidValue(format!("a {radius} fillet doesn't fit on these lines")));
    }
    let (t1, t2) = (add(p, mul(u, d)), add(p, mul(v, d)));
    let w = unit(add(u, v)).ok_or(SketchError::InvalidValue("bad corner".into()))?;
    let c = add(p, mul(w, radius / (theta / 2.0).sin()));
    let (n1, n2, nc) = (self.add_point(t1[0], t1[1]), self.add_point(t2[0], t2[1]), self.add_point(c[0], c[1]));
    self.swap_point(l1, c1, n1);
    self.swap_point(l2, c2, n2);
    let kind = if cross(sub(t1, c), sub(t2, c)) > 0.0 {
      CurveKind::Arc { center: nc, start: n1, end: n2 }
    } else {
      CurveKind::Arc { center: nc, start: n2, end: n1 }
    };
    let arc = self.add_curve(kind)?;
    self.drop_if_unused(c1);
    self.drop_if_unused(c2);
    self.add_constraint(ConstraintKind::Tangent { a: l1, b: arc })?;
    self.add_constraint(ConstraintKind::Tangent { a: l2, b: arc })?;
    self.add_constraint(ConstraintKind::Radius { curve: arc, value: Dim::new(radius) })?;
    Ok(arc)
  }

  /// Bevels the corner where two lines meet: the lines are cut back `d1` and `d2` from the corner
  /// and joined by a new line.
  pub fn chamfer(&mut self, l1: CurveId, l2: CurveId, d1: f64, d2: f64) -> Result<CurveId, SketchError> {
    self.not_text(l1)?;
    self.not_text(l2)?;
    if !(d1 > EPS && d2 > EPS && d1.is_finite() && d2.is_finite()) {
      return Err(SketchError::InvalidValue("chamfer distances must be positive".into()));
    }
    let ends = |s: &Sketch, id: CurveId| match s.curve(id).map(|c| &c.kind) {
      Some(CurveKind::Line { a, b }) => Ok((*a, *b)),
      _ => Err(SketchError::WrongGeometry("a sketch chamfer", "two lines")),
    };
    let (a1, b1) = ends(self, l1)?;
    let (a2, b2) = ends(self, l2)?;
    let near =
      |s: &Sketch, p: PointId, q: PointId| p == q || s.pos(p).zip(s.pos(q)).is_some_and(|(x, y)| dist(x, y) < 1e-6);
    let (c1, f1, c2, f2) = [(a1, b1, a2, b2), (a1, b1, b2, a2), (b1, a1, a2, b2), (b1, a1, b2, a2)]
      .into_iter()
      .find(|(p, _, q, _)| near(self, *p, *q))
      .ok_or(SketchError::WrongGeometry("a sketch chamfer", "two lines that meet at an end"))?;
    let pos = |id| self.pos(id).ok_or(SketchError::NoPoint(id));
    let (p, q1, q2) = (pos(c1)?, pos(f1)?, pos(f2)?);
    let (Some(u), Some(v)) = (unit(sub(q1, p)), unit(sub(q2, p))) else {
      return Err(SketchError::InvalidValue("zero-length line".into()));
    };
    if d1 >= dist(p, q1) - 1e-9 || d2 >= dist(p, q2) - 1e-9 {
      return Err(SketchError::InvalidValue("the chamfer doesn't fit on these lines".into()));
    }
    let (t1, t2) = (add(p, mul(u, d1)), add(p, mul(v, d2)));
    let (n1, n2) = (self.add_point(t1[0], t1[1]), self.add_point(t2[0], t2[1]));
    self.swap_point(l1, c1, n1);
    self.swap_point(l2, c2, n2);
    let line = self.add_curve(CurveKind::Line { a: n1, b: n2 })?;
    self.drop_if_unused(c1);
    self.drop_if_unused(c2);
    Ok(line)
  }

  // ---------------------------------------------------------------- trim / extend

  /// Removes the piece of `curve` around `at` between its nearest crossings with other curves
  /// (the whole curve if nothing crosses it). New ends stay on the curves they were cut at.
  /// Ellipses leave elliptical arcs, fit-point splines fit-point splines, and other splines
  /// Bézier chains.
  pub fn trim(&mut self, curve: CurveId, at: P) -> Result<Vec<CurveId>, SketchError> {
    self.not_text(curve)?;
    if self.curve(curve).is_some_and(|c| c.kind.is_elliptic()) {
      return self.trim_elliptic(curve, at);
    }
    if self.curve(curve).is_some_and(|c| c.kind.is_freeform()) {
      return self.trim_free(curve, at);
    }
    let g = self.geo(curve).ok_or(SketchError::NoCurve(curve))?;
    let tp = g.param(at);
    let end = g.end_param();
    // a circle has no ends, so a crossing at angle 0 counts
    let cuts: Vec<(f64, CurveId)> =
      self.crossings(curve)?.into_iter().filter(|(t, _)| g.full() || (*t > 1e-9 && *t < end - 1e-9)).collect();
    if g.full() {
      // a circle needs two cuts; it becomes the arc that remains
      if cuts.len() < 2 {
        self.remove_curve(curve)?;
        return Ok(vec![]);
      }
      let i = cuts.iter().rposition(|(t, _)| *t <= tp).unwrap_or(cuts.len() - 1);
      let (lo, hi) = (cuts[i], cuts[(i + 1) % cuts.len()]);
      let Some(CurveKind::Circle { center, .. }) = self.curve(curve).map(|c| c.kind.clone()) else {
        return Err(SketchError::NoCurve(curve));
      };
      let (s, e) = (g.at(hi.0), g.at(lo.0));
      let (ps, pe) = (self.add_point(s[0], s[1]), self.add_point(e[0], e[1]));
      if let Some(c) = self.curve_mut(curve) {
        c.kind = CurveKind::Arc { center, start: ps, end: pe };
      }
      self.add_constraint(ConstraintKind::PointOnCurve { p: ps, curve: hi.1 })?;
      self.add_constraint(ConstraintKind::PointOnCurve { p: pe, curve: lo.1 })?;
      return Ok(vec![curve]);
    }
    let lo = cuts.iter().rev().find(|(t, _)| *t < tp).copied();
    let hi = cuts.iter().find(|(t, _)| *t > tp).copied();
    let (first, last) = match self.curve(curve).map(|c| &c.kind) {
      Some(CurveKind::Line { a, b }) => (*a, *b),
      Some(CurveKind::Arc { start, end, .. }) => (*start, *end),
      _ => return Err(SketchError::NoCurve(curve)),
    };
    match (lo, hi) {
      (None, None) => {
        self.remove_curve(curve)?;
        Ok(vec![])
      }
      // keep the far part: move the near end to the cut
      (None, Some((t, other))) => {
        let p = g.at(t);
        let np = self.add_point(p[0], p[1]);
        self.detach(curve, first, np);
        self.add_constraint(ConstraintKind::PointOnCurve { p: np, curve: other })?;
        Ok(vec![curve])
      }
      (Some((t, other)), None) => {
        let p = g.at(t);
        let np = self.add_point(p[0], p[1]);
        self.detach(curve, last, np);
        self.add_constraint(ConstraintKind::PointOnCurve { p: np, curve: other })?;
        Ok(vec![curve])
      }
      // split in two around the gap
      (Some((t0, o0)), Some((t1, o1))) => {
        let (p0, p1) = (g.at(t0), g.at(t1));
        let (n0, n1) = (self.add_point(p0[0], p0[1]), self.add_point(p1[0], p1[1]));
        let kind = match self.curve(curve).map(|c| c.kind.clone()) {
          Some(CurveKind::Line { b, .. }) => CurveKind::Line { a: n1, b },
          Some(CurveKind::Arc { center, end, .. }) => CurveKind::Arc { center, start: n1, end },
          _ => return Err(SketchError::NoCurve(curve)),
        };
        self.swap_point(curve, last, n0);
        let second = self.add_curve(kind)?;
        self.add_constraint(ConstraintKind::PointOnCurve { p: n0, curve: o0 })?;
        self.add_constraint(ConstraintKind::PointOnCurve { p: n1, curve: o1 })?;
        let link = if matches!(g, Geo::Line { .. }) {
          ConstraintKind::Collinear { a: curve, b: second }
        } else {
          ConstraintKind::Equal { a: curve, b: second }
        };
        self.add_constraint(link)?;
        Ok(vec![curve, second])
      }
    }
  }

  /// Moves curve `id`'s end `old` to point `new`, dropping constraints that tied `old` to it.
  fn detach(&mut self, id: CurveId, old: PointId, new: PointId) {
    self.swap_point(id, old, new);
    self.drop_if_unused(old);
  }

  /// Lengthens a line, arc or open spline from its end nearest `at` to the nearest curve it
  /// would reach (lines straight on, arcs round their circle, splines along their end piece).
  pub fn extend(&mut self, line: CurveId, at: P) -> Result<(), SketchError> {
    self.not_text(line)?;
    match self.curve(line).map(|c| &c.kind) {
      Some(CurveKind::Arc { .. }) => return self.extend_arc(line, at),
      Some(CurveKind::EllipticalArc { .. }) => return self.extend_elliptic(line, at),
      Some(k) if k.is_freeform() => return self.extend_free(line, at),
      _ => {}
    }
    let Some(CurveKind::Line { a, b }) = self.curve(line).map(|c| c.kind.clone()) else {
      return Err(SketchError::WrongGeometry("extend", "a line, arc or spline"));
    };
    let g = self.geo(line).ok_or(SketchError::NoCurve(line))?;
    let (pa, pb) = (self.pos(a).ok_or(SketchError::NoPoint(a))?, self.pos(b).ok_or(SketchError::NoPoint(b))?);
    let from_b = dist(at, pb) <= dist(at, pa);
    let mut best: Option<(f64, P, CurveId)> = None;
    for (p, other) in self.hits_extended(&g, line) {
      let t = g.param(p);
      let gain = if from_b { t - 1.0 } else { -t };
      if gain > 1e-9 && best.is_none_or(|(bg, _, _)| gain < bg) {
        best = Some((gain, p, other));
      }
    }
    let (_, p, other) = best.ok_or(SketchError::InvalidValue("nothing to extend the line to".into()))?;
    self.move_end(line, if from_b { b } else { a }, p, other)
  }

  // ---------------------------------------------------------------- copies

  /// Copies curves through a point map `f`. Points shared between the copied curves stay
  /// shared; points `f` leaves in place are reused. `flip` reverses arcs (mirrors).
  fn copy_curves(
    &mut self,
    curves: &[CurveId],
    f: &dyn Fn(P) -> P,
    flip: bool,
  ) -> Result<(Vec<CurveId>, HashMap<PointId, PointId>), SketchError> {
    let mut map: HashMap<PointId, PointId> = HashMap::new();
    let mut out = Vec::new();
    for id in curves {
      let c = self.curve(*id).ok_or(SketchError::NoCurve(*id))?.clone();
      let mut get = |s: &mut Sketch, p: PointId| -> Result<PointId, SketchError> {
        if let Some(q) = map.get(&p) {
          return Ok(*q);
        }
        let pos = s.pos(p).ok_or(SketchError::NoPoint(p))?;
        let np = f(pos);
        let q = if dist(np, pos) < 1e-9 { p } else { s.add_point(np[0], np[1]) };
        map.insert(p, q);
        Ok(q)
      };
      let kind = match c.kind {
        CurveKind::Line { a, b } => CurveKind::Line { a: get(self, a)?, b: get(self, b)? },
        CurveKind::Circle { center, radius } => CurveKind::Circle { center: get(self, center)?, radius },
        CurveKind::Arc { center, start, end } => {
          let (center, s, e) = (get(self, center)?, get(self, start)?, get(self, end)?);
          if flip { CurveKind::Arc { center, start: e, end: s } } else { CurveKind::Arc { center, start: s, end: e } }
        }
        CurveKind::Spline { points, start_handle, end_handle } => CurveKind::Spline {
          points: points.iter().map(|p| get(self, *p)).collect::<Result<_, _>>()?,
          start_handle: start_handle.map(|p| get(self, p)).transpose()?,
          end_handle: end_handle.map(|p| get(self, p)).transpose()?,
        },
        CurveKind::ControlSpline { points } => {
          CurveKind::ControlSpline { points: points.iter().map(|p| get(self, *p)).collect::<Result<_, _>>()? }
        }
        CurveKind::Bezier { points } => {
          CurveKind::Bezier { points: points.iter().map(|p| get(self, *p)).collect::<Result<_, _>>()? }
        }
        CurveKind::Ellipse { center, major, minor } => {
          CurveKind::Ellipse { center: get(self, center)?, major: get(self, major)?, minor }
        }
        CurveKind::EllipticalArc { center, major, minor, start, end } => {
          let (center, major, s, e) = (get(self, center)?, get(self, major)?, get(self, start)?, get(self, end)?);
          let (start, end) = if flip { (e, s) } else { (s, e) };
          CurveKind::EllipticalArc { center, major, minor, start, end }
        }
      };
      let nid = self.add_curve(kind)?;
      if c.construction {
        self.set_construction(nid, true)?;
      }
      out.push(nid);
    }
    Ok((out, map))
  }

  /// Mirrors curves across a line; mirrored points are tied to the originals with symmetry
  /// constraints, so the copy follows edits.
  pub fn mirror(&mut self, curves: &[CurveId], axis: CurveId) -> Result<Vec<CurveId>, SketchError> {
    let Some(Geo::Line { a, b }) = self.geo(axis) else {
      return Err(SketchError::WrongGeometry("mirror", "a line to mirror across"));
    };
    let d = unit(sub(b, a)).ok_or(SketchError::InvalidValue("zero-length mirror line".into()))?;
    let f = move |p: P| {
      let v = sub(p, a);
      let along = mul(d, dot(v, d));
      add(a, sub(mul(along, 2.0), v))
    };
    let list: Vec<CurveId> = curves.iter().copied().filter(|c| *c != axis).collect();
    let (out, map) = self.copy_curves(&list, &f, true)?;
    for (p, q) in map {
      if p != q {
        self.add_constraint(ConstraintKind::Symmetric { a: p, b: q, line: axis })?;
      }
    }
    for (o, n) in list.iter().zip(&out) {
      if matches!(self.curve(*o).map(|c| &c.kind), Some(CurveKind::Circle { .. })) {
        self.add_constraint(ConstraintKind::Equal { a: *o, b: *n })?;
      }
    }
    Ok(out)
  }

  /// The direction of a sketch line, from its start to its end (the pattern direction Fusion
  /// takes from a picked line).
  pub fn line_direction(&self, line: CurveId) -> Result<P, SketchError> {
    let Some(Geo::Line { a, b }) = self.geo(line) else {
      return Err(SketchError::WrongGeometry("pattern direction", "a line"));
    };
    unit(sub(b, a)).ok_or(SketchError::InvalidValue("zero-length direction line".into()))
  }

  /// Copies of curves on a grid, as Fusion's sketch Rectangular Pattern: `a` sets the first
  /// direction and `b` the second (a zero `b.direction` is 90° counter-clockwise from `a`'s).
  pub fn pattern_rect(
    &mut self,
    curves: &[CurveId],
    a: PatternDir,
    b: PatternDir,
  ) -> Result<Vec<CurveId>, SketchError> {
    if a.count * b.count > 2000 || a.count == 0 || b.count == 0 {
      return Err(SketchError::InvalidValue("pattern counts must be 1..2000 in total".into()));
    }
    let u1 = unit(a.direction).ok_or(SketchError::InvalidValue("zero pattern direction".into()))?;
    let u2 = unit(b.direction).unwrap_or([-u1[1], u1[0]]);
    let (o1, o2) = (a.offsets()?, b.offsets()?);
    let mut out = Vec::new();
    for (i, &t1) in o1.iter().enumerate() {
      for (j, &t2) in o2.iter().enumerate() {
        if i == a.original() && j == b.original() {
          continue;
        }
        let off = add(mul(u1, t1), mul(u2, t2));
        out.extend(self.copy_curves(curves, &move |p| add(p, off), false)?.0);
      }
    }
    Ok(out)
  }

  /// `n` copies of curves around `center` spread over `angle` (radians; a full turn spaces them
  /// evenly).
  pub fn pattern_circ(
    &mut self,
    curves: &[CurveId],
    center: P,
    n: usize,
    angle: f64,
  ) -> Result<Vec<CurveId>, SketchError> {
    if !(1..=2000).contains(&n) {
      return Err(SketchError::InvalidValue("pattern count must be 1..2000".into()));
    }
    let full = (angle.abs() - TAU).abs() < 1e-9;
    let step = if full || n < 2 { angle / n as f64 } else { angle / (n - 1) as f64 };
    let mut out = Vec::new();
    for k in 1..n {
      let (s, c) = (step * k as f64).sin_cos();
      let rot = move |p: P| {
        let v = sub(p, center);
        add(center, [c * v[0] - s * v[1], s * v[0] + c * v[1]])
      };
      out.extend(self.copy_curves(curves, &rot, false)?.0);
    }
    Ok(out)
  }

  // ---------------------------------------------------------------- offset

  /// Offsets a chain of lines, arcs and splines (or circles and ellipses) by `distance`: positive
  /// is outward for a closed chain, to the left for an open one. Corners are re-met sharply; arcs
  /// stay concentric with the originals (sharing their centres) and lines parallel.
  pub fn offset(&mut self, curves: &[CurveId], distance: f64) -> Result<Vec<CurveId>, SketchError> {
    if !(distance.is_finite() && distance.abs() > EPS) {
      return Err(SketchError::InvalidValue("offset distance must not be zero".into()));
    }
    let mut out = Vec::new();
    let mut chain_ids = Vec::new();
    for id in curves {
      match self.curve(*id).map(|c| c.kind.clone()) {
        Some(CurveKind::Circle { center, radius }) => {
          let r = radius + distance;
          if r <= EPS {
            return Err(SketchError::InvalidValue("the offset swallows a circle".into()));
          }
          out.push(self.add_curve(CurveKind::Circle { center, radius: r })?);
        }
        Some(_) => chain_ids.push(*id),
        None => return Err(SketchError::NoCurve(*id)),
      }
    }
    if chain_ids.is_empty() {
      return Ok(out);
    }
    if chain_ids.iter().any(|c| self.curve(*c).is_some_and(|c| c.kind.is_freeform())) {
      out.extend(self.offset_free(&chain_ids, distance)?);
      return Ok(out);
    }
    // order the chain: (curve, reversed)
    let ends = |s: &Sketch, id: CurveId| -> Option<(P, P)> {
      match s.geo(id)? {
        Geo::Line { a, b } => Some((a, b)),
        g @ Geo::Arc { .. } => Some((g.at(0.0), g.at(g.end_param()))),
      }
    };
    let mut left: Vec<CurveId> = chain_ids.clone();
    let near = |p: P, q: P| dist(p, q) < 1e-6;
    let degree = |s: &Sketch, p: P, ids: &[CurveId]| {
      ids
        .iter()
        .filter_map(|i| ends(s, *i))
        .map(|(a, b)| usize::from(near(a, p)) + usize::from(near(b, p)))
        .sum::<usize>()
    };
    let start = chain_ids
      .iter()
      .filter_map(|i| ends(self, *i))
      .flat_map(|(a, b)| [a, b])
      .find(|p| degree(self, *p, &chain_ids) == 1);
    let closed = start.is_none();
    let mut at = match start {
      Some(p) => p,
      None => ends(self, chain_ids[0]).ok_or(SketchError::NoCurve(chain_ids[0]))?.0,
    };
    let mut chain: Vec<(CurveId, bool)> = Vec::new();
    while let Some(i) = left.iter().position(|id| ends(self, *id).is_some_and(|(a, b)| near(a, at) || near(b, at))) {
      let id = left.swap_remove(i);
      let (a, b) = ends(self, id).ok_or(SketchError::NoCurve(id))?;
      let rev = !near(a, at);
      at = if rev { a } else { b };
      chain.push((id, rev));
    }
    if !left.is_empty() {
      return Err(SketchError::InvalidValue("the curves don't form one chain".into()));
    }
    // side: + left of travel; outward for closed chains
    let mut side = 1.0;
    if closed {
      let mut pts = Vec::new();
      for (id, rev) in &chain {
        let g = self.geo(*id).ok_or(SketchError::NoCurve(*id))?;
        for k in 0..16 {
          let t = g.end_param() * k as f64 / 16.0;
          pts.push(g.at(if *rev { g.end_param() - t } else { t }));
        }
      }
      let area: f64 = (0..pts.len()).map(|i| cross(pts[i], pts[(i + 1) % pts.len()])).sum::<f64>() / 2.0;
      side = if area > 0.0 { -1.0 } else { 1.0 };
    }
    let off = side * distance;
    // offset pieces in travel order, as extended geometry plus their travel ends
    let mut pieces: Vec<(Geo, P, P, CurveId, bool)> = Vec::new();
    for (id, rev) in &chain {
      let g = self.geo(*id).ok_or(SketchError::NoCurve(*id))?;
      let (s, e) = if *rev { (g.at(g.end_param()), g.at(0.0)) } else { (g.at(0.0), g.at(g.end_param())) };
      match g {
        Geo::Line { .. } => {
          let d = unit(sub(e, s)).ok_or(SketchError::InvalidValue("zero-length line".into()))?;
          let n = mul([-d[1], d[0]], off);
          let (s2, e2) = (add(s, n), add(e, n));
          pieces.push((Geo::Line { a: s2, b: e2 }, s2, e2, *id, *rev));
        }
        Geo::Arc { c, r, .. } => {
          // travelling counter-clockwise the centre is on the left
          let ccw = if *rev { -1.0 } else { 1.0 };
          let r2 = r - ccw * off;
          if r2 <= EPS {
            return Err(SketchError::InvalidValue("the offset swallows an arc".into()));
          }
          let k = r2 / r;
          let (s2, e2) = (add(c, mul(sub(s, c), k)), add(c, mul(sub(e, c), k)));
          pieces.push((Geo::Arc { c, r: r2, a0: 0.0, sweep: TAU }, s2, e2, *id, *rev));
        }
      }
    }
    // meet neighbours at corners
    let n = pieces.len();
    let joints = if closed { n } else { n.saturating_sub(1) };
    let mut corner: Vec<P> = Vec::new();
    for k in 0..joints {
      let (g1, _, e1, _, _) = pieces[k];
      let (g2, s2, _, _, _) = pieces[(k + 1) % n];
      let guess = mul(add(e1, s2), 0.5);
      let p = if near(e1, s2) {
        e1
      } else {
        meet(&g1, &g2).into_iter().min_by(|a, b| dist(*a, guess).total_cmp(&dist(*b, guess))).unwrap_or(guess)
      };
      corner.push(p);
    }
    // the new chain's points
    let mut pts: Vec<PointId> = Vec::new();
    let first = if closed { corner[n - 1] } else { pieces[0].1 };
    pts.push(self.add_point(first[0], first[1]));
    for k in 0..n {
      if closed && k == n - 1 {
        pts.push(pts[0]);
      } else {
        let p = if k < corner.len() { corner[k] } else { pieces[k].2 };
        pts.push(self.add_point(p[0], p[1]));
      }
    }
    for (k, (g, _, _, orig, rev)) in pieces.iter().enumerate() {
      let (ps, pe) = (pts[k], pts[k + 1]);
      let kind = match (g, self.curve(*orig).map(|c| c.kind.clone())) {
        (Geo::Line { .. }, _) => CurveKind::Line { a: ps, b: pe },
        (Geo::Arc { .. }, Some(CurveKind::Arc { center, .. })) => {
          if *rev {
            CurveKind::Arc { center, start: pe, end: ps }
          } else {
            CurveKind::Arc { center, start: ps, end: pe }
          }
        }
        _ => return Err(SketchError::NoCurve(*orig)),
      };
      let nid = self.add_curve(kind)?;
      if matches!(g, Geo::Line { .. }) {
        self.add_constraint(ConstraintKind::Parallel { a: *orig, b: nid })?;
      }
      out.push(nid);
    }
    Ok(out)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{PtSpec, find_profiles};

  #[test]
  fn fillet_rounds_a_rectangle_corner() {
    let mut s = Sketch::new();
    let r = s.add_rectangle([0.0, 0.0], [40.0, 20.0]).unwrap();
    let arc = s.fillet(r[0], r[1], 5.0).unwrap();
    assert!(s.solve().ok());
    assert!((s.radius(arc).unwrap() - 5.0).abs() < 1e-9);
    let p = find_profiles(&s);
    assert_eq!(p.len(), 1);
    let expect = 800.0 - (25.0 - std::f64::consts::PI * 25.0 / 4.0);
    assert!((p[0].area() - expect).abs() < 0.05, "{}", p[0].area());
  }

  #[test]
  fn trim_line_between_crossings_and_circle_to_arc() {
    let mut s = Sketch::new();
    let (h, _) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(30.0, 0.0)).unwrap();
    s.add_line(PtSpec::At(10.0, -5.0), PtSpec::At(10.0, 5.0)).unwrap();
    s.add_line(PtSpec::At(20.0, -5.0), PtSpec::At(20.0, 5.0)).unwrap();
    let parts = s.trim(h, [15.0, 0.0]).unwrap();
    assert_eq!(parts.len(), 2);
    let len: f64 = parts
      .iter()
      .map(|c| {
        s.geo(*c)
          .map(|g| match g {
            Geo::Line { a, b } => dist(a, b),
            _ => 0.0,
          })
          .unwrap()
      })
      .sum();
    assert!((len - 20.0).abs() < 1e-9);
    assert!(s.solve().ok());
    // a circle cut by a line keeps the side away from the pick
    let mut s = Sketch::new();
    let c = s.add_circle(PtSpec::At(0.0, 0.0), 10.0).unwrap();
    s.add_line(PtSpec::At(-20.0, 0.0), PtSpec::At(20.0, 0.0)).unwrap();
    s.trim(c, [0.0, 10.0]).unwrap();
    let Some(Geo::Arc { sweep, a0, .. }) = s.geo(c) else { panic!() };
    assert!((sweep - PI).abs() < 1e-9 && (a0 - PI).abs() < 1e-9, "lower half remains");
  }

  #[test]
  fn extend_reaches_the_next_curve() {
    let mut s = Sketch::new();
    let (l, _) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(5.0, 0.0)).unwrap();
    s.add_circle(PtSpec::At(20.0, 0.0), 3.0).unwrap();
    s.extend(l, [5.0, 0.0]).unwrap();
    let Some(Geo::Line { b, .. }) = s.geo(l) else { panic!() };
    assert!((b[0] - 17.0).abs() < 1e-9);
    assert!(s.solve().ok());
  }

  #[test]
  fn offset_rectangle_outward_and_open_chain() {
    let mut s = Sketch::new();
    let r = s.add_rectangle([0.0, 0.0], [40.0, 20.0]).unwrap();
    let out = s.offset(&r, 2.0).unwrap();
    assert_eq!(out.len(), 4);
    assert!(s.solve().ok());
    let areas: Vec<f64> = find_profiles(&s).iter().map(|p| p.area()).collect();
    assert!(
      areas.iter().any(|a| (a - 44.0 * 24.0).abs() < 1e-6)
        || areas.iter().any(|a| (a - (44.0 * 24.0 - 800.0)).abs() < 1e-6),
      "{areas:?}"
    );
    let mut s = Sketch::new();
    let (l1, [_, m]) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(10.0, 0.0)).unwrap();
    let (l2, _) = s.add_line(PtSpec::Point(m), PtSpec::At(10.0, 10.0)).unwrap();
    let out = s.offset(&[l1, l2], 1.0).unwrap();
    let Some(Geo::Line { b, .. }) = s.geo(out[0]) else { panic!() };
    assert!(dist(b, [9.0, 1.0]) < 1e-9, "inner corner {b:?}");
  }

  #[test]
  fn mirror_and_patterns() {
    let mut s = Sketch::new();
    let (axis, _) = s.add_line(PtSpec::At(0.0, -10.0), PtSpec::At(0.0, 10.0)).unwrap();
    s.set_construction(axis, true).unwrap();
    let c = s.add_circle(PtSpec::At(5.0, 2.0), 1.0).unwrap();
    let m = s.mirror(&[c], axis).unwrap();
    assert!(s.solve().ok());
    let Some(Geo::Arc { c: mc, .. }) = s.geo(m[0]) else { panic!() };
    assert!(dist(mc, [-5.0, 2.0]) < 1e-9);
    let (a, b) = (PatternDir::spacing([1.0, 0.0], 3, 4.0), PatternDir::spacing([0.0, 1.0], 2, 5.0));
    assert_eq!(s.pattern_rect(&[c], a, b).unwrap().len(), 5);
    let centres = |s: &Sketch, made: &[CurveId]| -> Vec<P> {
      made.iter().map(|id| if let Some(Geo::Arc { c, .. }) = s.geo(*id) { c } else { [f64::NAN; 2] }).collect()
    };
    // Extent: 4 instances spread over 12 are 4 apart; symmetric: 3 instances centred on the original.
    let ext = PatternDir { extent: true, ..PatternDir::spacing([1.0, 0.0], 4, 12.0) };
    let made = s.pattern_rect(&[c], ext, PatternDir::spacing([0.0, 0.0], 1, 0.0)).unwrap();
    let want = [[9.0, 2.0], [13.0, 2.0], [17.0, 2.0]];
    assert!(centres(&s, &made).iter().zip(want).all(|(p, q)| dist(*p, q) < 1e-9), "{:?}", centres(&s, &made));
    let sym = PatternDir { symmetric: true, ..PatternDir::spacing([0.0, 1.0], 3, 6.0) };
    let made = s.pattern_rect(&[c], sym, PatternDir::spacing([0.0, 0.0], 1, 0.0)).unwrap();
    let want = [[5.0, -4.0], [5.0, 8.0]];
    assert!(centres(&s, &made).iter().zip(want).all(|(p, q)| dist(*p, q) < 1e-9), "{:?}", centres(&s, &made));
    let (a, b) = (PatternDir::spacing([1.0, 0.0], 2, 4.0), PatternDir::spacing([0.0, 0.0], 2, 5.0));
    let made = s.pattern_rect(&[c], a, b).unwrap();
    assert!(dist(centres(&s, &made)[0], [5.0, 7.0]) < 1e-9, "second direction defaults to 90° from the first");
    let circ = s.pattern_circ(&[c], [0.0, 0.0], 4, TAU).unwrap();
    let Some(Geo::Arc { c: pc, .. }) = s.geo(circ[0]) else { panic!() };
    assert!(dist(pc, [-2.0, 5.0]) < 1e-9);
  }
}
