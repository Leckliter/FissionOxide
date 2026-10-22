//! Placing geometry for a constraint that is not yet satisfied, the way Fusion does when one is
//! added: the first-picked entity stays where it is, the second-picked one moves rigidly (same
//! size and shape) onto it, and nothing else in the sketch moves.
//!
//! The numerical solver ([`Sketch::solve`]) finds the minimum-norm change over every free
//! coordinate, which spreads a new constraint over both entities: a Coincident pulls both points
//! to their midpoint, a Perpendicular turns and stretches both lines. Fusion instead moves the
//! geometry picked last (or the less constrained one) as a rigid body, so this pass runs before
//! the solve and puts that geometry where Fusion would, leaving the solver nothing to do when the
//! constraint can be met that way. When it cannot (the moving geometry is fixed, shared with
//! something that would have to move too, or held by constraints a rigid move would break), the
//! solver takes it from the drawn positions as before.
//!
//! A constraint's `a` is the first pick and `b` the second (the engine's `constrain` keeps the
//! order). The pass tries moving `b`; if `b` cannot move it tries `a`, as Fusion moves the less
//! constrained of the two.
//!
//! Only sketches with [`PLACE_SECOND_PICK`] take the pass; a legacy sketch solves from the drawn
//! positions as it always did.

use std::collections::HashSet;

use crate::behaviour::PLACE_SECOND_PICK;
use crate::{Constraint, ConstraintId, ConstraintKind, CurveId, CurveKind, PointId, Sketch, dist};

/// Below this a constraint counts as already satisfied and is left alone.
const SATISFIED: f64 = 1e-9;

/// A rigid motion of part of a sketch.
#[derive(Debug, Clone, Copy)]
enum Motion {
  Translate([f64; 2]),
  /// Turn about a point by an angle, then shift.
  Rotate {
    about: [f64; 2],
    angle: f64,
    shift: [f64; 2],
  },
}

impl Motion {
  fn apply(&self, p: [f64; 2]) -> [f64; 2] {
    match *self {
      Motion::Translate(t) => [p[0] + t[0], p[1] + t[1]],
      Motion::Rotate { about, angle, shift } => {
        let (s, c) = angle.sin_cos();
        let d = [p[0] - about[0], p[1] - about[1]];
        [about[0] + c * d[0] - s * d[1] + shift[0], about[1] + s * d[0] + c * d[1] + shift[1]]
      }
    }
  }

  fn is_translation(&self) -> bool {
    matches!(self, Motion::Translate(_))
  }
}

/// What moves, as one rigid body.
struct Body {
  points: Vec<PointId>,
  motion: Motion,
}

impl Sketch {
  /// Before a solve, moves the second-picked entity of each unsatisfied Coincident, Parallel,
  /// Perpendicular, Collinear, Concentric or line/circle Tangent rigidly onto the first (see the
  /// module docs). Satisfied constraints, and ones whose geometry a rigid move cannot place
  /// without disturbing something else, are left to the solver.
  pub(crate) fn place_unsatisfied_constraints(&mut self) {
    if !self.behaviours.has(PLACE_SECOND_PICK) {
      return;
    }
    let ids: Vec<ConstraintId> = self.constraints.iter().map(|c| c.id).collect();
    for id in ids {
      let Some(c) = self.constraint(id).cloned() else { continue };
      if let Some(body) = self.placement(&c) {
        for p in &body.points {
          let Some(pos) = self.pos(*p) else { continue };
          let to = body.motion.apply(pos);
          if let Some(pt) = self.point_mut(*p) {
            pt.x = to[0];
            pt.y = to[1];
          }
        }
      }
    }
  }

  /// The rigid move that satisfies `c`: of its second entity if that may move, else its first.
  fn placement(&self, c: &Constraint) -> Option<Body> {
    match c.kind {
      ConstraintKind::Coincident { a, b } => {
        let (pa, pb) = (self.pos(a)?, self.pos(b)?);
        if dist(pa, pb) < SATISFIED {
          return None;
        }
        self
          .body_from_point(c.id, b, &[a], Motion::Translate([pa[0] - pb[0], pa[1] - pb[1]]))
          .or_else(|| self.body_from_point(c.id, a, &[b], Motion::Translate([pb[0] - pa[0], pb[1] - pa[1]])))
      }
      ConstraintKind::Concentric { a, b } => {
        let (ca, cb) = (self.centre_of(a)?, self.centre_of(b)?);
        if dist(ca, cb) < SATISFIED {
          return None;
        }
        self
          .body_from_curve(c.id, b, &[], a, Motion::Translate([ca[0] - cb[0], ca[1] - cb[1]]))
          .or_else(|| self.body_from_curve(c.id, a, &[], b, Motion::Translate([cb[0] - ca[0], cb[1] - ca[1]])))
      }
      ConstraintKind::Parallel { a, b }
      | ConstraintKind::Perpendicular { a, b }
      | ConstraintKind::Collinear { a, b } => {
        self.place_line_to_line(c, a, b).or_else(|| self.place_line_to_line(c, b, a))
      }
      ConstraintKind::Tangent { a, b } => self.place_tangent(c, a, b).or_else(|| self.place_tangent(c, b, a)),
      _ => None,
    }
  }

  /// Turns line `mover` onto `stay`'s direction (Parallel, Collinear), or square to it
  /// (Perpendicular): about the point the two lines share, when they share one, otherwise about
  /// its own midpoint, which Collinear then slides onto `stay`'s line.
  fn place_line_to_line(&self, c: &Constraint, stay: CurveId, mover: CurveId) -> Option<Body> {
    let (Some(CurveKind::Line { a: s0, b: s1 }), Some(CurveKind::Line { a: m0, b: m1 })) =
      (self.curve(stay).map(|k| &k.kind), self.curve(mover).map(|k| &k.kind))
    else {
      return None;
    };
    let (s0, s1, m0, m1) = (self.pos(*s0)?, self.pos(*s1)?, self.pos(*m0)?, self.pos(*m1)?);
    let u = unit(sub(s1, s0))?;
    let v = unit(sub(m1, m0))?;
    let collinear = matches!(c.kind, ConstraintKind::Collinear { .. });
    // the direction the mover should have, the way round nearest to how it lies now
    let mut target = if matches!(c.kind, ConstraintKind::Perpendicular { .. }) { [-u[1], u[0]] } else { u };
    if dot(target, v) < 0.0 {
      target = [-target[0], -target[1]];
    }
    let shared = self.shared_point(stay, mover);
    let turned = cross(v, target).abs() > SATISFIED || dot(v, target) < 0.0;
    let off_line = collinear && (cross(u, sub(m0, s0)).abs() > SATISFIED || cross(u, sub(m1, s0)).abs() > SATISFIED);
    if !turned && !off_line {
      return None;
    }
    let angle = cross(v, target).atan2(dot(v, target));
    if let Some(pivot) = shared {
      // the shared end stays; the other end swings round it (Collinear lands on the line as the
      // pivot is already on it)
      let about = self.pos(pivot)?;
      return self.body_from_curve(c.id, mover, &[pivot], stay, Motion::Rotate { about, angle, shift: [0.0, 0.0] });
    }
    // no shared point: turn about the midpoint, which Collinear then slides onto the line
    let mid = [(m0[0] + m1[0]) / 2.0, (m0[1] + m1[1]) / 2.0];
    let d = if collinear { cross(u, sub(mid, s0)) } else { 0.0 };
    let shift = [u[1] * d, -u[0] * d];
    self.body_from_curve(c.id, mover, &[], stay, Motion::Rotate { about: mid, angle, shift })
  }

  /// Moves `mover` (a line, or a circle or arc) until the line touches the round curve: a line
  /// with one end held pivots about that end so its extension is tangent; a line with both ends
  /// free, or the circle, slides along the normal.
  fn place_tangent(&self, c: &Constraint, stay: CurveId, mover: CurveId) -> Option<Body> {
    let (line, round) = match (self.curve(stay).map(|k| &k.kind), self.curve(mover).map(|k| &k.kind)) {
      (Some(CurveKind::Line { .. }), Some(CurveKind::Circle { .. } | CurveKind::Arc { .. })) => (stay, mover),
      (Some(CurveKind::Circle { .. } | CurveKind::Arc { .. }), Some(CurveKind::Line { .. })) => (mover, stay),
      _ => return None,
    };
    let Some(CurveKind::Line { a: l0, b: l1 }) = self.curve(line).map(|k| &k.kind) else { return None };
    let (l0, l1) = (*l0, *l1);
    let (p0, p1) = (self.pos(l0)?, self.pos(l1)?);
    let (centre, radius) = (self.centre_of(round)?, self.round_radius(round)?);
    let u = unit(sub(p1, p0))?;
    let n = [-u[1], u[0]];
    // signed distance of the centre from the line; tangent when it is ±radius
    let s = cross(u, sub(centre, p0));
    let side = if s < 0.0 { -1.0 } else { 1.0 };
    if (s.abs() - radius).abs() < SATISFIED {
      return None;
    }
    if mover == round {
      let k = side * radius - s;
      return self.body_from_curve(c.id, round, &[], line, Motion::Translate([n[0] * k, n[1] * k]));
    }
    // the line moves: about an end something else holds, if exactly one is held
    let held = |p: PointId| self.point_held_by_other(p, line);
    let pivot = match (held(l0), held(l1)) {
      (true, false) => Some((l0, p0, p1)),
      (false, true) => Some((l1, p1, p0)),
      (false, false) => None,
      // both ends held: no rigid move of the line alone
      (true, true) => return None,
    };
    let Some((pivot, at, free)) = pivot else {
      // both ends free: slide the line along its normal onto the circle
      let k = s - side * radius;
      return self.body_from_curve(c.id, line, &[], round, Motion::Translate([n[0] * k, n[1] * k]));
    };
    let v = sub(centre, at);
    let l = v[0].hypot(v[1]);
    if l <= radius {
      // the held end is inside the circle: no tangent through it
      return None;
    }
    let phi = (radius / l).asin();
    let vhat = [v[0] / l, v[1] / l];
    let now = unit(sub(free, at))?;
    // the four directions through the pivot whose line touches the circle; the nearest one
    let mut best: Option<([f64; 2], f64)> = None;
    for sign in [1.0, -1.0] {
      let (sp, cp) = (sign * phi).sin_cos();
      let d = [cp * vhat[0] - sp * vhat[1], sp * vhat[0] + cp * vhat[1]];
      for d in [d, [-d[0], -d[1]]] {
        let score = dot(d, now);
        if best.is_none_or(|(_, b)| score > b) {
          best = Some((d, score));
        }
      }
    }
    let (d, _) = best?;
    let angle = cross(now, d).atan2(dot(now, d));
    self.body_from_curve(c.id, line, &[pivot], round, Motion::Rotate { about: at, angle, shift: [0.0, 0.0] })
  }

  /// Everything joined to `seed`, as a body under `motion`, if it may move (see [`Sketch::may_move`])
  /// and none of the `stay` points is part of it.
  fn body_from_point(&self, skip: ConstraintId, seed: PointId, stay: &[PointId], motion: Motion) -> Option<Body> {
    let (points, curves) = self.connected(&[seed], &[], &[]);
    self.body(skip, points, curves, stay, motion)
  }

  /// Everything joined to curve `seed` (not through the `still` points, which stay put), as a
  /// body under `motion`, if it may move and no point of curve `stay` is part of it.
  fn body_from_curve(
    &self,
    skip: ConstraintId,
    seed: CurveId,
    still: &[PointId],
    stay: CurveId,
    motion: Motion,
  ) -> Option<Body> {
    let (points, curves) = self.connected(&[], &[seed], still);
    self.body(skip, points, curves, &self.curve_points(stay), motion)
  }

  fn body(
    &self,
    skip: ConstraintId,
    points: Vec<PointId>,
    curves: Vec<CurveId>,
    stay: &[PointId],
    motion: Motion,
  ) -> Option<Body> {
    if stay.iter().any(|p| points.contains(p)) {
      return None;
    }
    self.may_move(skip, &points, &curves, motion).then_some(Body { points, motion })
  }

  /// The points and curves reachable from the seeds through shared points, never crossing a
  /// `still` point (which is left out).
  fn connected(&self, points: &[PointId], curves: &[CurveId], still: &[PointId]) -> (Vec<PointId>, Vec<CurveId>) {
    let mut seen_p: HashSet<PointId> = HashSet::new();
    let mut seen_c: HashSet<CurveId> = HashSet::new();
    let mut todo_p: Vec<PointId> = points.iter().copied().filter(|p| !still.contains(p)).collect();
    let mut todo_c: Vec<CurveId> = curves.to_vec();
    let mut out_p = Vec::new();
    let mut out_c = Vec::new();
    while !todo_p.is_empty() || !todo_c.is_empty() {
      if let Some(p) = todo_p.pop() {
        if !seen_p.insert(p) {
          continue;
        }
        out_p.push(p);
        for c in &self.curves {
          if crate::curve_points(&c.kind).contains(&p) && !seen_c.contains(&c.id) {
            todo_c.push(c.id);
          }
        }
      }
      if let Some(c) = todo_c.pop() {
        if !seen_c.insert(c) {
          continue;
        }
        out_c.push(c);
        if let Some(curve) = self.curve(c) {
          for p in crate::curve_points(&curve.kind) {
            if !still.contains(&p) && !seen_p.contains(&p) {
              todo_p.push(p);
            }
          }
        }
      }
    }
    (out_p, out_c)
  }

  /// Whether the body may move rigidly without disturbing anything else: none of its points is
  /// fixed, and every constraint but `skip` that names any of it either names nothing outside it
  /// (so it moves along) or only measures a size a rigid move keeps. Horizontal and Vertical
  /// survive a translation only.
  fn may_move(&self, skip: ConstraintId, points: &[PointId], curves: &[CurveId], motion: Motion) -> bool {
    if points.iter().any(|p| self.point(*p).is_none_or(|pt| pt.fixed)) {
      return false;
    }
    if points.is_empty() {
      return false;
    }
    self.constraints.iter().filter(|c| c.id != skip).all(|c| {
      let (cp, cc) = (c.kind.points(), c.kind.curves());
      let names = cp.iter().any(|p| points.contains(p)) || cc.iter().any(|k| curves.contains(k));
      if !names {
        return true;
      }
      let all_in = cp.iter().all(|p| points.contains(p)) && cc.iter().all(|k| curves.contains(k));
      match c.kind {
        ConstraintKind::Fix { .. } => false,
        ConstraintKind::Radius { .. }
        | ConstraintKind::Diameter { .. }
        | ConstraintKind::FixRadius { .. }
        | ConstraintKind::Equal { .. } => true,
        ConstraintKind::Horizontal { .. }
        | ConstraintKind::Vertical { .. }
        | ConstraintKind::HorizontalPoints { .. }
        | ConstraintKind::VerticalPoints { .. }
        | ConstraintKind::HorizontalDistance { .. }
        | ConstraintKind::VerticalDistance { .. } => all_in && motion.is_translation(),
        _ => all_in,
      }
    })
  }

  /// A point of both curves, if they share one.
  fn shared_point(&self, a: CurveId, b: CurveId) -> Option<PointId> {
    let pb = self.curve_points(b);
    self.curve_points(a).into_iter().find(|p| pb.contains(p))
  }

  /// Whether anything but `curve` holds point `p`: it is fixed, a constraint names it, or another
  /// curve uses it.
  fn point_held_by_other(&self, p: PointId, curve: CurveId) -> bool {
    self.point(p).is_none_or(|pt| pt.fixed)
      || self.constraints.iter().any(|c| c.kind.points().contains(&p))
      || self.curves.iter().any(|c| c.id != curve && crate::curve_points(&c.kind).contains(&p))
  }

  /// Centre of a circle, arc, ellipse or elliptical arc.
  fn centre_of(&self, id: CurveId) -> Option<[f64; 2]> {
    match &self.curve(id)?.kind {
      CurveKind::Circle { center, .. }
      | CurveKind::Arc { center, .. }
      | CurveKind::Ellipse { center, .. }
      | CurveKind::EllipticalArc { center, .. } => self.pos(*center),
      _ => None,
    }
  }

  /// Radius of a circle or arc.
  fn round_radius(&self, id: CurveId) -> Option<f64> {
    match &self.curve(id)?.kind {
      CurveKind::Circle { radius, .. } => Some(*radius),
      CurveKind::Arc { center, start, .. } => Some(dist(self.pos(*center)?, self.pos(*start)?)),
      _ => None,
    }
  }
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
  [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
  a[0] * b[0] + a[1] * b[1]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
  a[0] * b[1] - a[1] * b[0]
}

/// `a` scaled to unit length; None for a zero-length line.
fn unit(a: [f64; 2]) -> Option<[f64; 2]> {
  let l = a[0].hypot(a[1]);
  (l > 1e-12).then(|| [a[0] / l, a[1] / l])
}

#[cfg(test)]
mod tests {
  use super::{cross, dot, sub};
  use crate::*;

  fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    dist(a, b) < 1e-9
  }

  fn ends(s: &Sketch, l: CurveId) -> ([f64; 2], [f64; 2]) {
    let p = s.curve_points(l);
    (s.pos(p[0]).unwrap(), s.pos(p[1]).unwrap())
  }

  fn unit(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    super::unit(sub(b, a)).unwrap()
  }

  /// Every point's position.
  fn positions(s: &Sketch) -> Vec<[f64; 2]> {
    s.points.iter().map(|p| [p.x, p.y]).collect()
  }

  /// The prober's sketch: two separate lines, the first about 70 long, the second about 95.
  fn two_lines() -> (Sketch, CurveId, [PointId; 2], CurveId, [PointId; 2]) {
    let mut s = Sketch::new();
    let (l1, p1) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(60.0, 35.0)).unwrap();
    let (l2, p2) = s.add_line(PtSpec::At(100.0, 10.0), PtSpec::At(170.0, 75.0)).unwrap();
    assert!(s.solve().ok());
    (s, l1, p1, l2, p2)
  }

  /// `ui:sketch:coincident-motion`: the first pick stays, the second line comes over rigidly.
  #[test]
  fn coincident_moves_the_second_pick_rigidly_onto_the_first() {
    let (mut s, l1, [a0, a1], l2, [b0, b1]) = two_lines();
    let (q0, q1) = ends(&s, l2);
    let (p0, p1) = ends(&s, l1);
    s.add_constraint(ConstraintKind::Coincident { a: a1, b: b0 }).unwrap();
    let r = s.solve();
    assert!(r.ok(), "{r:?}");
    assert_eq!(ends(&s, l1), (p0, p1), "line 1 is untouched");
    assert!(near(s.pos(b0).unwrap(), p1), "{:?}", s.pos(b0));
    let shift = sub(p1, q0);
    assert!(near(s.pos(b1).unwrap(), [q1[0] + shift[0], q1[1] + shift[1]]), "line 2 keeps its length and angle");
    assert_eq!(r.dof, 6, "{r:?}");
    // the solve is a fixed point: nothing moves on the next one
    let before = positions(&s);
    assert!(s.solve().ok());
    assert_eq!(positions(&s), before);

    // picked the other way round, line 1 moves instead
    let (mut s, l1, [a0b, a1b], l2, [b0b, _]) = two_lines();
    let (q0, _) = ends(&s, l2);
    let (p0, p1) = ends(&s, l1);
    s.add_constraint(ConstraintKind::Coincident { a: b0b, b: a1b }).unwrap();
    assert!(s.solve().ok());
    assert_eq!(ends(&s, l2), (q0, ends(&s, l2).1));
    assert!(near(s.pos(a1b).unwrap(), q0));
    assert!(near(s.pos(a0b).unwrap(), [p0[0] + q0[0] - p1[0], p0[1] + q0[1] - p1[1]]));
    let _ = (a0, b1, l1);
  }

  #[test]
  fn coincident_moves_the_first_pick_when_the_second_is_held() {
    // the second line is fixed: the first comes to it
    let (mut s, l1, [a0, a1], l2, [b0, _]) = two_lines();
    s.fix_curve(l2).unwrap();
    let (q0, q1) = ends(&s, l2);
    let (p0, p1) = ends(&s, l1);
    s.add_constraint(ConstraintKind::Coincident { a: a1, b: b0 }).unwrap();
    assert!(s.solve().ok());
    assert_eq!(ends(&s, l2), (q0, q1));
    assert!(near(s.pos(a1).unwrap(), q0));
    assert!(near(s.pos(a0).unwrap(), [p0[0] + q0[0] - p1[0], p0[1] + q0[1] - p1[1]]));
    let _ = l1;
    // both held: nothing is placed and the solver reports the conflict as before
    let (mut s, l1, [_, a1], _, [b0, _]) = two_lines();
    s.fix_curve(l1).unwrap();
    let q0 = s.pos(b0).unwrap();
    s.add_constraint(ConstraintKind::Fix { p: b0, x: q0[0], y: q0[1] }).unwrap();
    assert!(s.solve().ok());
    s.add_constraint(ConstraintKind::Coincident { a: a1, b: b0 }).unwrap();
    assert!(!s.solve().ok(), "a fixed point can't meet another fixed point");
  }

  /// A line joined to the end of another: the second line shares the first's end point.
  fn joined_lines() -> (Sketch, CurveId, CurveId, PointId, PointId) {
    let mut s = Sketch::new();
    let (l1, [_, shared]) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(60.0, 30.0)).unwrap();
    let (l2, [_, free]) = s.add_line(PtSpec::Point(shared), PtSpec::At(120.0, 100.0)).unwrap();
    assert!(s.solve().ok());
    (s, l1, l2, shared, free)
  }

  /// `ui:sketch:parallel-perpendicular-motion`: the second line turns about the shared point.
  #[test]
  fn parallel_and_perpendicular_turn_the_second_line_about_the_shared_point() {
    for perpendicular in [false, true] {
      let (mut s, l1, l2, shared, free) = joined_lines();
      let before = ends(&s, l1);
      let pivot = s.pos(shared).unwrap();
      let len = dist(pivot, s.pos(free).unwrap());
      let kind = if perpendicular {
        ConstraintKind::Perpendicular { a: l1, b: l2 }
      } else {
        ConstraintKind::Parallel { a: l1, b: l2 }
      };
      s.add_constraint(kind).unwrap();
      let r = s.solve();
      assert!(r.ok(), "{r:?}");
      assert_eq!(ends(&s, l1), before, "the first line stays");
      assert_eq!(s.pos(shared).unwrap(), pivot);
      let e = s.pos(free).unwrap();
      assert!((dist(pivot, e) - len).abs() < 1e-9, "the second keeps its length");
      let u = unit(before.0, before.1);
      let v = unit(pivot, e);
      if perpendicular {
        assert!(dot(u, v).abs() < 1e-9, "{v:?}");
        // the way round nearest to how it lay: up and to the left
        assert!(v[1] > 0.0 && v[0] < 0.0, "{v:?}");
      } else {
        assert!(cross(u, v).abs() < 1e-9 && dot(u, v) > 0.0, "{v:?}");
      }
      assert_eq!(r.dof, 5, "{r:?}");
    }
  }

  #[test]
  fn parallel_between_separate_lines_turns_the_second_about_its_middle() {
    let (mut s, l1, _, l2, [b0, b1]) = two_lines();
    let before = ends(&s, l1);
    let (q0, q1) = ends(&s, l2);
    let mid = [(q0[0] + q1[0]) / 2.0, (q0[1] + q1[1]) / 2.0];
    s.add_constraint(ConstraintKind::Parallel { a: l1, b: l2 }).unwrap();
    assert!(s.solve().ok());
    assert_eq!(ends(&s, l1), before);
    let (r0, r1) = (s.pos(b0).unwrap(), s.pos(b1).unwrap());
    assert!(near([(r0[0] + r1[0]) / 2.0, (r0[1] + r1[1]) / 2.0], mid));
    assert!((dist(r0, r1) - dist(q0, q1)).abs() < 1e-9);
    assert!(cross(unit(before.0, before.1), unit(r0, r1)).abs() < 1e-9);
  }

  /// `ui:sketch:collinear-motion`: the first line keeps its direction, the second turns onto it.
  #[test]
  fn collinear_turns_the_second_line_onto_the_first() {
    let (mut s, l1, l2, shared, free) = joined_lines();
    let before = ends(&s, l1);
    let pivot = s.pos(shared).unwrap();
    let len = dist(pivot, s.pos(free).unwrap());
    s.add_constraint(ConstraintKind::Collinear { a: l1, b: l2 }).unwrap();
    let r = s.solve();
    assert!(r.ok(), "{r:?}");
    assert_eq!(ends(&s, l1), before, "the first line stays");
    let e = s.pos(free).unwrap();
    assert!((dist(pivot, e) - len).abs() < 1e-9);
    let u = unit(before.0, before.1);
    assert!(cross(u, sub(e, before.0)).abs() < 1e-9, "on the first line's extension: {e:?}");
    assert!(dot(u, sub(e, pivot)) > 0.0, "running on past the shared end");
    let _ = l2;

    // separate lines: the second turns about its middle and slides onto the first's line
    let (mut s, l1, _, l2, [b0, b1]) = two_lines();
    let before = ends(&s, l1);
    let (q0, q1) = ends(&s, l2);
    s.add_constraint(ConstraintKind::Collinear { a: l1, b: l2 }).unwrap();
    assert!(s.solve().ok());
    assert_eq!(ends(&s, l1), before);
    let (r0, r1) = (s.pos(b0).unwrap(), s.pos(b1).unwrap());
    let u = unit(before.0, before.1);
    assert!(cross(u, sub(r0, before.0)).abs() < 1e-9 && cross(u, sub(r1, before.0)).abs() < 1e-9);
    assert!((dist(r0, r1) - dist(q0, q1)).abs() < 1e-9);
    // slid straight across: the midpoint moved along the normal only
    let mid = [(q0[0] + q1[0]) / 2.0, (q0[1] + q1[1]) / 2.0];
    let mid2 = [(r0[0] + r1[0]) / 2.0, (r0[1] + r1[1]) / 2.0];
    assert!(dot(u, sub(mid2, mid)).abs() < 1e-9);
  }

  /// `ui:sketch:concentric-motion`: only the free circle moves, whichever was picked first.
  #[test]
  fn concentric_moves_only_the_free_circle() {
    for free_first in [false, true] {
      let mut s = Sketch::new();
      // a circle held by two tangent lines, with a neighbour, and a free circle off to the side
      let held = s.add_circle(PtSpec::At(30.0, 30.0), 10.0).unwrap();
      let (t1, _) = s.add_line(PtSpec::At(0.0, 40.0), PtSpec::At(60.0, 40.0)).unwrap();
      let (t2, _) = s.add_line(PtSpec::At(40.0, 0.0), PtSpec::At(40.0, 60.0)).unwrap();
      s.add_constraint(ConstraintKind::Tangent { a: held, b: t1 }).unwrap();
      s.add_constraint(ConstraintKind::Tangent { a: held, b: t2 }).unwrap();
      let neighbour = s.add_circle(PtSpec::At(80.0, 30.0), 8.0).unwrap();
      let free = s.add_circle(PtSpec::At(55.0, 75.0), 6.0).unwrap();
      assert!(s.solve().ok());
      let before = positions(&s);
      let kind = if free_first {
        ConstraintKind::Concentric { a: free, b: held }
      } else {
        ConstraintKind::Concentric { a: held, b: free }
      };
      s.add_constraint(kind).unwrap();
      let r = s.solve();
      assert!(r.ok(), "{r:?}");
      let after = positions(&s);
      let fc = s.curve_points(free)[0];
      for (i, p) in s.points.iter().enumerate() {
        if p.id == fc {
          assert_eq!(after[i], [30.0, 30.0], "the free circle's centre lands on the held one");
        } else {
          assert_eq!(after[i], before[i], "nothing else moves (free first: {free_first})");
        }
      }
      assert!((s.radius(held).unwrap() - 10.0).abs() < 1e-9);
      assert!((s.radius(free).unwrap() - 6.0).abs() < 1e-9, "the free circle keeps its size");
      assert!((s.radius(neighbour).unwrap() - 8.0).abs() < 1e-9);
    }
  }

  /// `ui:sketch:tangent-line-circle-motion`: the second-picked line pivots about its held end.
  #[test]
  fn tangent_pivots_a_joined_line_about_the_shared_point_to_touch_the_circle() {
    let mut s = Sketch::new();
    let (l1, [_, shared]) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(40.0, 0.0)).unwrap();
    let (l2, [_, free]) = s.add_line(PtSpec::Point(shared), PtSpec::At(80.0, 30.0)).unwrap();
    let circle = s.add_circle(PtSpec::At(120.0, -20.0), 15.0).unwrap();
    assert!(s.solve().ok());
    let before = ends(&s, l1);
    let pivot = s.pos(shared).unwrap();
    let len = dist(pivot, s.pos(free).unwrap());
    s.add_constraint(ConstraintKind::Tangent { a: circle, b: l2 }).unwrap();
    let r = s.solve();
    assert!(r.ok(), "{r:?}");
    assert_eq!(ends(&s, l1), before, "the other line stays");
    assert_eq!(s.pos(s.curve_points(circle)[0]).unwrap(), [120.0, -20.0], "the circle stays");
    assert!((s.radius(circle).unwrap() - 15.0).abs() < 1e-9);
    let e = s.pos(free).unwrap();
    assert!((dist(pivot, e) - len).abs() < 1e-9, "the line keeps its length");
    let u = unit(pivot, e);
    assert!((cross(u, sub([120.0, -20.0], pivot)).abs() - 15.0).abs() < 1e-9, "its extension touches the circle");
    // the nearest tangent: still running right and down a little, not the one the far side
    assert!(u[0] > 0.9 && u[1] < 0.0, "{u:?}");

    // a free circle picked second slides onto the line's extension instead
    let mut s = Sketch::new();
    let (l1, [_, shared]) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(40.0, 0.0)).unwrap();
    let (l2, _) = s.add_line(PtSpec::Point(shared), PtSpec::At(80.0, 30.0)).unwrap();
    let circle = s.add_circle(PtSpec::At(120.0, -20.0), 15.0).unwrap();
    let (before1, before2) = (ends(&s, l1), ends(&s, l2));
    s.add_constraint(ConstraintKind::Tangent { a: l2, b: circle }).unwrap();
    assert!(s.solve().ok());
    assert_eq!(ends(&s, l1), before1);
    assert_eq!(ends(&s, l2), before2);
    let c = s.pos(s.curve_points(circle)[0]).unwrap();
    let u = unit(before2.0, before2.1);
    assert!((cross(u, sub(c, before2.0)).abs() - 15.0).abs() < 1e-9, "{c:?}");
    assert!(dot(u, sub(c, [120.0, -20.0])).abs() < 1e-9, "moved along the normal only");
    assert!((s.radius(circle).unwrap() - 15.0).abs() < 1e-9);
  }

  #[test]
  fn a_held_second_pick_leaves_the_placement_to_the_first() {
    // the line's ends are both fixed, so the circle picked first slides onto it instead
    let mut s = Sketch::new();
    let (l, [a, b]) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(40.0, 0.0)).unwrap();
    s.add_constraint(ConstraintKind::Fix { p: a, x: 0.0, y: 0.0 }).unwrap();
    s.add_constraint(ConstraintKind::Fix { p: b, x: 40.0, y: 0.0 }).unwrap();
    let circle = s.add_circle(PtSpec::At(20.0, 30.0), 10.0).unwrap();
    s.add_constraint(ConstraintKind::Tangent { a: circle, b: l }).unwrap();
    let r = s.solve();
    assert!(r.ok(), "{r:?}");
    assert_eq!(s.pos(a).unwrap(), [0.0, 0.0]);
    assert_eq!(s.pos(b).unwrap(), [40.0, 0.0]);
    assert!(near(s.pos(s.curve_points(circle)[0]).unwrap(), [20.0, 10.0]));
    assert!((s.radius(circle).unwrap() - 10.0).abs() < 1e-9);
    // both fixed: nothing can be placed and the solver reports the conflict
    s.fix_curve(circle).unwrap();
    let (l2, _) = s.add_line(PtSpec::At(0.0, 50.0), PtSpec::At(40.0, 50.0)).unwrap();
    s.fix_curve(l2).unwrap();
    s.add_constraint(ConstraintKind::Tangent { a: circle, b: l2 }).unwrap();
    assert!(!s.solve().ok());
  }

  #[test]
  fn closing_a_chain_is_left_to_the_solver() {
    // the two ends of one open polyline: moving either end rigidly would carry the other along
    let mut s = Sketch::new();
    let (_, [a, m]) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(40.0, 0.0)).unwrap();
    let (_, [_, b]) = s.add_line(PtSpec::Point(m), PtSpec::At(40.0, 30.0)).unwrap();
    s.add_constraint(ConstraintKind::Coincident { a, b }).unwrap();
    assert!(s.solve().ok());
    let met = s.pos(a).unwrap();
    assert!(near(met, s.pos(b).unwrap()));
    assert!(dist(met, [20.0, 15.0]) < 1e-6, "met at the midpoint, nothing shifted: {met:?}");
    assert!(dist(s.pos(m).unwrap(), [40.0, 0.0]) < 1e-6);
  }

  #[test]
  fn a_legacy_sketch_solves_from_the_drawn_positions() {
    // without the behaviour, Coincident still pulls both points to their midpoint
    let (mut s, l1, [_, a1], l2, [b0, _]) = two_lines();
    s.behaviours = Behaviours::legacy();
    let (p1, q0) = (s.pos(a1).unwrap(), s.pos(b0).unwrap());
    s.add_constraint(ConstraintKind::Coincident { a: a1, b: b0 }).unwrap();
    assert!(s.solve().ok());
    let met = s.pos(a1).unwrap();
    assert!(near(met, s.pos(b0).unwrap()));
    assert!(dist(met, [(p1[0] + q0[0]) / 2.0, (p1[1] + q0[1]) / 2.0]) < 1e-6, "{met:?}");
    let _ = (l1, l2);
  }

  #[test]
  fn placement_leaves_a_line_with_a_horizontal_to_the_solver_unless_it_only_slides() {
    // Parallel on a horizontal second line would have to turn it: the solver places it
    let mut s = Sketch::new();
    let (l1, _) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(60.0, 30.0)).unwrap();
    let (l2, _) = s.add_line(PtSpec::At(100.0, 10.0), PtSpec::At(170.0, 10.0)).unwrap();
    s.add_constraint(ConstraintKind::Horizontal { line: l2 }).unwrap();
    assert!(s.solve().ok());
    s.add_constraint(ConstraintKind::Parallel { a: l2, b: l1 }).unwrap();
    assert!(s.solve().ok());
    let (q0, q1) = ends(&s, l2);
    assert!((q0[1] - q1[1]).abs() < 1e-9, "horizontal still holds");
    // Coincident only slides it, so Horizontal survives and the first pick stays
    let mut s = Sketch::new();
    let (l1, [_, a1]) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(60.0, 30.0)).unwrap();
    let (l2, [b0, b1]) = s.add_line(PtSpec::At(100.0, 10.0), PtSpec::At(170.0, 10.0)).unwrap();
    s.add_constraint(ConstraintKind::Horizontal { line: l2 }).unwrap();
    assert!(s.solve().ok());
    let before = ends(&s, l1);
    s.add_constraint(ConstraintKind::Coincident { a: a1, b: b0 }).unwrap();
    assert!(s.solve().ok());
    assert_eq!(ends(&s, l1), before);
    assert!(near(s.pos(b0).unwrap(), [60.0, 30.0]) && near(s.pos(b1).unwrap(), [130.0, 30.0]));
  }
}
