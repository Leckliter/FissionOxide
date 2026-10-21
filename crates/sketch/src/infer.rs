//! Constraint inference: horizontal, vertical, tangent, coincident, equal and concentric
//! constraints read off geometry that was drawn without them (traced images, imports), so it can
//! be dimensioned and edited parametrically.
//!
//! Only unambiguous cases are taken: a line within a small angle of horizontal or vertical, two
//! curves meeting smoothly at a point no other curve uses, points closer than the tolerance,
//! radii and lengths equal within it. Every inferred constraint is then checked by solving: those
//! the solver can't satisfy together are dropped (first the ones it names, then whole kinds,
//! weakest first), so inference never leaves the sketch failing.

use std::collections::HashMap;

use crate::solve::SolveStatus;
use crate::{ConstraintId, ConstraintKind, CurveId, CurveKind, PointId, Sketch, dist, seg_dist};

/// How close geometry must be to take a constraint.
#[derive(Debug, Clone, Copy)]
pub struct InferTolerance {
  /// Largest angle (degrees) off horizontal/vertical for a line to become horizontal/vertical.
  pub angle: f64,
  /// Largest angle (degrees) between two curves' directions where they meet to be tangent.
  pub tangent: f64,
  /// Largest distance (sketch units) between points to be coincident or centres concentric,
  /// and between lengths or radii to be equal.
  pub distance: f64,
}

impl Default for InferTolerance {
  fn default() -> InferTolerance {
    InferTolerance { angle: 1.0, tangent: 5.0, distance: 0.01 }
  }
}

/// What inference added.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Inferred {
  /// The constraints added, with their kind's name (`horizontal`, `tangent`, …).
  pub constraints: Vec<(ConstraintId, &'static str)>,
  /// Candidates dropped because the solver couldn't satisfy them with the rest.
  pub dropped: usize,
  /// Why nothing (or less) was inferred, if something stopped it.
  pub warning: Option<String>,
}

impl Inferred {
  /// How many of each kind were added, in a fixed order.
  pub fn counts(&self) -> Vec<(&'static str, usize)> {
    KINDS
      .iter()
      .map(|k| (*k, self.constraints.iter().filter(|(_, n)| n == k).count()))
      .filter(|(_, n)| *n > 0)
      .collect()
  }
}

/// Kinds in the order they are given up when the solve fails, weakest first.
const KINDS: [&str; 7] = ["equal", "concentric", "tangent", "point_on_curve", "coincident", "vertical", "horizontal"];
/// Rounds of dropping the constraints a failed solve names before whole kinds go.
const NAMED_ROUNDS: usize = 3;

/// The end points of a curve with the direction leaving each end into the curve (unit).
fn ends(sk: &Sketch, kind: &CurveKind) -> Vec<(PointId, [f64; 2])> {
  let unit = |v: [f64; 2]| {
    let l = v[0].hypot(v[1]);
    if l < 1e-12 { None } else { Some([v[0] / l, v[1] / l]) }
  };
  match *kind {
    CurveKind::Line { a, b } => {
      let (Some(pa), Some(pb)) = (sk.pos(a), sk.pos(b)) else { return Vec::new() };
      let d = [pb[0] - pa[0], pb[1] - pa[1]];
      match unit(d) {
        Some(u) => vec![(a, u), (b, [-u[0], -u[1]])],
        None => Vec::new(),
      }
    }
    CurveKind::Arc { center, start, end } => {
      let (Some(c), Some(s), Some(e)) = (sk.pos(center), sk.pos(start), sk.pos(end)) else { return Vec::new() };
      // counter-clockwise from start to end: leaving the start along the ccw tangent, leaving
      // the end against it
      let ccw = |p: [f64; 2]| unit([-(p[1] - c[1]), p[0] - c[0]]);
      match (ccw(s), ccw(e)) {
        (Some(ts), Some(te)) => vec![(start, ts), (end, [-te[0], -te[1]])],
        _ => Vec::new(),
      }
    }
    _ => Vec::new(),
  }
}

fn is_round(kind: &CurveKind) -> bool {
  matches!(kind, CurveKind::Arc { .. } | CurveKind::Circle { .. })
}

/// Centre and radius of an arc or circle.
fn round(sk: &Sketch, kind: &CurveKind) -> Option<(PointId, [f64; 2], f64)> {
  match *kind {
    CurveKind::Circle { center, radius } => Some((center, sk.pos(center)?, radius)),
    CurveKind::Arc { center, start, .. } => {
      let c = sk.pos(center)?;
      Some((center, c, dist(c, sk.pos(start)?)))
    }
    _ => None,
  }
}

/// Groups values that lie within `tol` of their neighbours and span at most `tol`: sorted
/// runs, so chains like 1.0, 1.009, 1.018 (tol 0.01) split rather than merge.
fn groups<T: Copy>(mut items: Vec<(f64, T)>, tol: f64) -> Vec<Vec<T>> {
  items.sort_by(|a, b| a.0.total_cmp(&b.0));
  let mut out: Vec<Vec<T>> = Vec::new();
  let mut start = f64::NAN;
  for (v, t) in items {
    match out.last_mut() {
      Some(g) if v - start <= tol => g.push(t),
      _ => {
        start = v;
        out.push(vec![t]);
      }
    }
  }
  out.retain(|g| g.len() > 1);
  out
}

impl Sketch {
  /// Infers constraints on `curves` (their own geometry and how they meet each other and the
  /// rest of the sketch), adds those the solver can satisfy, and solves. Construction curves
  /// and curves other than lines, arcs and circles are left alone.
  pub fn infer_constraints(&mut self, curves: &[CurveId], tol: &InferTolerance) -> Inferred {
    let mut out = Inferred::default();
    let candidates = self.candidates(curves, tol);
    if candidates.is_empty() {
      return out;
    }
    if !self.constraints.is_empty() && !self.clone().solve().ok() {
      out.warning = Some("the sketch's own constraints don't solve, so none were inferred".into());
      return out;
    }
    let mut keep: Vec<(ConstraintKind, &'static str)> = candidates;
    let mut named_rounds = 0;
    let mut kinds_dropped = 0;
    loop {
      let mut trial = self.clone();
      let mut ids = Vec::new();
      for (k, name) in &keep {
        if let Ok(id) = trial.add_constraint(k.clone()) {
          ids.push((id, *name));
        }
      }
      let report = trial.solve();
      match report.status {
        SolveStatus::Solved => {
          *self = trial;
          out.constraints = ids;
          return out;
        }
        SolveStatus::TooLarge => {
          out.dropped += keep.len();
          out.warning = Some("too much geometry to solve with inferred constraints, so none were added".into());
          return out;
        }
        SolveStatus::Failed => {}
      }
      let failing: Vec<usize> = ids
        .iter()
        .enumerate()
        .filter(|(_, (id, _))| report.failing.contains(id))
        .map(|(i, _)| i)
        .collect();
      let before = keep.len();
      if named_rounds < NAMED_ROUNDS && !failing.is_empty() {
        named_rounds += 1;
        let mut i = 0;
        keep.retain(|_| {
          i += 1;
          !failing.contains(&(i - 1))
        });
      } else if kinds_dropped < KINDS.len() {
        let kind = KINDS[kinds_dropped];
        kinds_dropped += 1;
        keep.retain(|(_, n)| *n != kind);
      } else {
        keep.clear();
      }
      out.dropped += before - keep.len();
      if keep.is_empty() {
        out.warning = Some("the inferred constraints didn't solve together, so none were added".into());
        return out;
      }
    }
  }

  /// The constraints [`Sketch::infer_constraints`] would try, with their kind's name.
  fn candidates(&self, curves: &[CurveId], tol: &InferTolerance) -> Vec<(ConstraintKind, &'static str)> {
    let mine: Vec<(CurveId, CurveKind)> = curves
      .iter()
      .filter_map(|id| self.curve(*id))
      .filter(|c| !c.construction)
      .filter(|c| matches!(c.kind, CurveKind::Line { .. } | CurveKind::Arc { .. } | CurveKind::Circle { .. }))
      .map(|c| (c.id, c.kind.clone()))
      .collect();
    let existing = |k: &ConstraintKind| self.constraints.iter().any(|c| &c.kind == k);
    let mut out: Vec<(ConstraintKind, &'static str)> = Vec::new();
    let push = |out: &mut Vec<(ConstraintKind, &'static str)>, k: ConstraintKind, name: &'static str| {
      if !existing(&k) {
        out.push((k, name));
      }
    };
    let (sin_a, cos_t) = (tol.angle.to_radians().sin(), tol.tangent.to_radians().cos());
    let d = tol.distance.max(1e-12);

    // horizontal / vertical lines
    let mut horizontal = Vec::new();
    let mut vertical = Vec::new();
    for (id, kind) in &mine {
      let CurveKind::Line { a, b } = *kind else { continue };
      let (Some(pa), Some(pb)) = (self.pos(a), self.pos(b)) else { continue };
      let len = dist(pa, pb);
      if len <= 2.0 * d {
        continue;
      }
      let (dx, dy) = ((pb[0] - pa[0]).abs(), (pb[1] - pa[1]).abs());
      if dy <= len * sin_a {
        push(&mut out, ConstraintKind::Horizontal { line: *id }, "horizontal");
        horizontal.push((len, *id));
      } else if dx <= len * sin_a {
        push(&mut out, ConstraintKind::Vertical { line: *id }, "vertical");
        vertical.push((len, *id));
      }
    }

    // where curves meet: how many (non-construction) curves of the whole sketch use each point
    let mut users: HashMap<PointId, Vec<CurveId>> = HashMap::new();
    for c in self.curves.iter().filter(|c| !c.construction) {
      for (p, _) in ends(self, &c.kind) {
        users.entry(p).or_default().push(c.id);
      }
    }
    let concentric_pair = |a: &CurveKind, b: &CurveKind| match (round(self, a), round(self, b)) {
      (Some((_, ca, _)), Some((_, cb, _))) => dist(ca, cb) <= d,
      _ => false,
    };
    // tangent: two curves (one at least round) meeting smoothly at a point only they use
    let kinds: HashMap<CurveId, &CurveKind> = mine.iter().map(|(id, k)| (*id, k)).collect();
    let mut tangent_pairs: Vec<(CurveId, CurveId)> = Vec::new();
    for (id, kind) in &mine {
      for (p, dir) in ends(self, kind) {
        let Some(us) = users.get(&p) else { continue };
        if us.len() != 2 {
          continue;
        }
        let other = if us[0] == *id { us[1] } else { us[0] };
        if other == *id || tangent_pairs.contains(&(other, *id)) || tangent_pairs.contains(&(*id, other)) {
          continue;
        }
        let Some(ok) = kinds.get(&other).copied().or_else(|| self.curve(other).map(|c| &c.kind)) else { continue };
        if !(is_round(kind) || is_round(ok)) || concentric_pair(kind, ok) {
          continue;
        }
        let Some((_, odir)) = ends(self, ok).into_iter().find(|(q, _)| *q == p) else { continue };
        // smooth: leaving the point into one is arriving along the other
        if dir[0] * odir[0] + dir[1] * odir[1] <= -cos_t {
          tangent_pairs.push((*id, other));
          push(&mut out, ConstraintKind::Tangent { a: *id, b: other }, "tangent");
        }
      }
    }

    // coincident end points that aren't shared, and free ends lying on another curve
    let my_ends: Vec<PointId> = {
      let mut v: Vec<PointId> = mine.iter().flat_map(|(_, k)| ends(self, k).into_iter().map(|(p, _)| p)).collect();
      v.sort_by_key(|p| p.0);
      v.dedup();
      v
    };
    let all_ends: Vec<(PointId, [f64; 2])> = {
      let mut v: Vec<PointId> = users.keys().copied().collect();
      v.sort_by_key(|p| p.0);
      v.into_iter().filter_map(|p| Some((p, self.pos(p)?))).collect()
    };
    let mut joined: Vec<(PointId, PointId)> = Vec::new();
    for &p in &my_ends {
      let Some(pp) = self.pos(p) else { continue };
      for &(q, pq) in &all_ends {
        if q == p || dist(pp, pq) > d || joined.contains(&(q, p)) || joined.contains(&(p, q)) {
          continue;
        }
        joined.push((p, q));
        push(&mut out, ConstraintKind::Coincident { a: p, b: q }, "coincident");
      }
    }
    for &p in &my_ends {
      let free = users.get(&p).is_some_and(|u| u.len() == 1) && !joined.iter().any(|(a, b)| *a == p || *b == p);
      let Some(pp) = self.pos(p) else { continue };
      if !free {
        continue;
      }
      for c in self.curves.iter().filter(|c| !c.construction) {
        if self.curve_points(c.id).contains(&p) {
          continue;
        }
        let on = match c.kind {
          CurveKind::Line { a, b } => match (self.pos(a), self.pos(b)) {
            // inside the line, not at its ends (that would be a coincident)
            (Some(a), Some(b)) => seg_dist(a, b, pp) <= d && dist(a, pp) > d && dist(b, pp) > d,
            _ => false,
          },
          CurveKind::Circle { .. } | CurveKind::Arc { .. } => match round(self, &c.kind) {
            Some((_, centre, r)) => {
              (dist(centre, pp) - r).abs() <= d
                && self.distance_to_curve(c.id, pp).is_some_and(|e| e <= d)
                && ends(self, &c.kind).iter().all(|(q, _)| self.pos(*q).is_none_or(|q| dist(q, pp) > d))
            }
            None => false,
          },
          _ => false,
        };
        if on {
          push(&mut out, ConstraintKind::PointOnCurve { p, curve: c.id }, "point_on_curve");
          break;
        }
      }
    }

    // concentric centres that aren't shared, and equal radii
    let rounds: Vec<(CurveId, PointId, [f64; 2], f64)> =
      mine.iter().filter_map(|(id, k)| round(self, k).map(|(p, c, r)| (*id, p, c, r))).collect();
    let mut concentric: Vec<(CurveId, CurveId)> = Vec::new();
    for (i, a) in rounds.iter().enumerate() {
      for b in &rounds[i + 1..] {
        if a.1 != b.1 && dist(a.2, b.2) <= d && !concentric.iter().any(|(_, x)| *x == b.0) {
          concentric.push((a.0, b.0));
          push(&mut out, ConstraintKind::Concentric { a: a.0, b: b.0 }, "concentric");
        }
      }
    }
    let radii: Vec<(f64, CurveId)> = rounds.iter().filter(|r| r.3 > 2.0 * d).map(|r| (r.3, r.0)).collect();
    for g in groups(radii, d).into_iter().chain(groups(horizontal, d)).chain(groups(vertical, d)) {
      for w in g.windows(2) {
        push(&mut out, ConstraintKind::Equal { a: w[0], b: w[1] }, "equal");
      }
    }
    out
  }
}

#[cfg(test)]
mod tests {
  #![allow(clippy::unwrap_used, clippy::panic)]

  use super::*;

  fn tol() -> InferTolerance {
    InferTolerance { angle: 1.0, tangent: 5.0, distance: 0.05 }
  }

  fn line(s: &mut Sketch, a: PointId, b: PointId) -> CurveId {
    s.add_curve(CurveKind::Line { a, b }).unwrap()
  }

  /// A rounded rectangle 40 × 20 with corner radius 5, slightly off (as a tracer leaves it).
  fn rounded(s: &mut Sketch) -> Vec<CurveId> {
    let r = 5.0;
    let e = 0.004;
    let p = |s: &mut Sketch, x: f64, y: f64| s.add_point(x, y);
    // straight edges' ends
    let (b0, b1) = (p(s, r, 0.0), p(s, 40.0 - r, e));
    let (r0, r1) = (p(s, 40.0, r), p(s, 40.0 + e, 20.0 - r));
    let (t0, t1) = (p(s, 40.0 - r, 20.0), p(s, r, 20.0 - e));
    let (l0, l1) = (p(s, 0.0, 20.0 - r), p(s, -e, r));
    let cs = [p(s, 40.0 - r, r), p(s, 40.0 - r, 20.0 - r), p(s, r, 20.0 - r), p(s, r, r)];
    let mut v = vec![line(s, b0, b1), line(s, r0, r1), line(s, t0, t1), line(s, l0, l1)];
    for (c, (a, b)) in cs.iter().zip([(b1, r0), (r1, t0), (t1, l0), (l1, b0)]) {
      v.push(s.add_curve(CurveKind::Arc { center: *c, start: a, end: b }).unwrap());
    }
    v
  }

  #[test]
  fn a_rounded_rectangle_gets_horizontal_vertical_tangent_and_equal() {
    let mut s = Sketch::new();
    let curves = rounded(&mut s);
    let r = s.infer_constraints(&curves, &tol());
    assert!(r.warning.is_none(), "{r:?}");
    let counts: HashMap<&str, usize> = r.counts().into_iter().collect();
    assert_eq!(counts.get("horizontal"), Some(&2), "{counts:?}");
    assert_eq!(counts.get("vertical"), Some(&2), "{counts:?}");
    assert_eq!(counts.get("tangent"), Some(&8), "{counts:?}");
    // four equal radii chain as three, the two pairs of sides as one each
    assert_eq!(counts.get("equal"), Some(&5), "{counts:?}");
    assert!(!counts.contains_key("coincident") && !counts.contains_key("concentric"), "{counts:?}");
    // solved: the edges are now exactly straight
    let report = s.solve();
    assert!(report.ok());
    for c in &curves[..4] {
      let pts = s.curve_points(*c);
      let (a, b) = (s.pos(pts[0]).unwrap(), s.pos(pts[1]).unwrap());
      assert!((a[0] - b[0]).abs() < 1e-7 || (a[1] - b[1]).abs() < 1e-7, "{a:?} {b:?}");
    }
    // a second pass finds nothing new
    let again = s.infer_constraints(&curves, &tol());
    assert!(again.constraints.is_empty(), "{again:?}");
  }

  #[test]
  fn corners_slanted_lines_and_far_points_stay_unconstrained() {
    let mut s = Sketch::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 1.0); // ~5.7°: neither horizontal nor vertical
    let c = s.add_point(10.0, 11.0);
    let l1 = line(&mut s, a, b);
    let l2 = line(&mut s, b, c);
    let r = s.infer_constraints(&[l1, l2], &tol());
    assert_eq!(r.counts(), vec![("vertical", 1)], "{r:?}");
  }

  #[test]
  fn near_points_become_coincident_and_ends_on_curves_point_on_curve() {
    let mut s = Sketch::new();
    let (a, b) = (s.add_point(0.0, 0.0), s.add_point(10.0, 0.0));
    let (c, d) = (s.add_point(10.02, 0.01), s.add_point(10.0, 10.0));
    let (e, f) = (s.add_point(5.0, 0.03), s.add_point(5.0, -8.0));
    let l1 = line(&mut s, a, b);
    let l2 = line(&mut s, c, d);
    let l3 = line(&mut s, f, e);
    let r = s.infer_constraints(&[l1, l2, l3], &tol());
    let counts: HashMap<&str, usize> = r.counts().into_iter().collect();
    assert_eq!(counts.get("coincident"), Some(&1), "{r:?}");
    assert_eq!(counts.get("point_on_curve"), Some(&1), "{r:?}");
    assert!(s.solve().ok());
    let (pb, pc) = (s.pos(b).unwrap(), s.pos(c).unwrap());
    assert!(dist(pb, pc) < 1e-7);
    assert!(s.pos(e).unwrap()[1].abs() < 1e-7);
  }

  #[test]
  fn concentric_circles_and_equal_radii() {
    let mut s = Sketch::new();
    let (c1, c2, c3) = (s.add_point(0.0, 0.0), s.add_point(0.01, -0.01), s.add_point(30.0, 0.0));
    let a = s.add_curve(CurveKind::Circle { center: c1, radius: 5.0 }).unwrap();
    let b = s.add_curve(CurveKind::Circle { center: c2, radius: 9.0 }).unwrap();
    let c = s.add_curve(CurveKind::Circle { center: c3, radius: 5.02 }).unwrap();
    let r = s.infer_constraints(&[a, b, c], &tol());
    let counts: HashMap<&str, usize> = r.counts().into_iter().collect();
    assert_eq!(counts.get("concentric"), Some(&1), "{r:?}");
    assert_eq!(counts.get("equal"), Some(&1), "{r:?}");
  }

  #[test]
  fn constraints_that_cant_solve_together_are_dropped() {
    // a slightly sheared "rectangle": horizontal and vertical sides fit, but two lengths that
    // look equal can't be once the others are snapped (the vertical is fixed in place)
    let mut s = Sketch::new();
    let p = [s.add_point(0.0, 0.0), s.add_point(10.0, 0.0), s.add_point(10.0, 10.03), s.add_point(0.0, 10.0)];
    let mut ls = Vec::new();
    for i in 0..4 {
      ls.push(line(&mut s, p[i], p[(i + 1) % 4]));
    }
    s.add_constraint(ConstraintKind::Fix { p: p[0], x: 0.0, y: 0.0 }).unwrap();
    s.add_constraint(ConstraintKind::Fix { p: p[1], x: 10.0, y: 0.0 }).unwrap();
    s.add_constraint(ConstraintKind::Fix { p: p[3], x: 0.0, y: 10.0 }).unwrap();
    s.add_constraint(ConstraintKind::Fix { p: p[2], x: 10.0, y: 10.03 }).unwrap();
    let r = s.infer_constraints(&ls, &tol());
    assert!(s.solve().ok(), "{r:?}");
    assert!(r.dropped > 0, "{r:?}");
  }
}
