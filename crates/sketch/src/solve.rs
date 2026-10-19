//! Numerical constraint solver.
//!
//! Unknowns are free point coordinates and circle radii. Each constraint contributes residual
//! equations that are zero when it is satisfied. Steps are damped minimum-norm Gauss–Newton
//! (Levenberg–Marquardt on `J Jᵀ`), so an under-constrained sketch moves as little as possible —
//! dragging one point doesn't fling the rest of the sketch around.
//!
//! After solving, the rank of the Jacobian gives the remaining degrees of freedom and which
//! points are still free to move (drawn blue in the UI; fully constrained geometry is drawn black).

use std::collections::HashMap;

use crate::{ConstraintId, ConstraintKind, CurveId, CurveKind, PointId, Sketch};

const MAX_ITERS: usize = 100;
const TOL: f64 = 1e-9;
/// Dense solve guard: more equations than this is refused rather than allocating gigabytes.
const MAX_EQUATIONS: usize = 4000;
const MAX_VARS: usize = 8000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveStatus {
  /// Every constraint is satisfied.
  Solved,
  /// The constraints conflict (or the solver couldn't find a solution from here).
  Failed,
  /// The sketch is too large for the dense solver.
  TooLarge,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SolveReport {
  pub status: SolveStatus,
  pub iterations: usize,
  /// Largest remaining residual.
  pub residual: f64,
  /// Remaining degrees of freedom (0 = fully constrained).
  pub dof: usize,
  /// Points that can still move.
  pub free_points: Vec<PointId>,
  /// Circles whose radius is still free.
  pub free_curves: Vec<CurveId>,
  /// Constraints left unsatisfied when solving failed.
  pub failing: Vec<ConstraintId>,
}

impl SolveReport {
  pub fn ok(&self) -> bool {
    self.status == SolveStatus::Solved
  }

  pub fn fully_constrained(&self) -> bool {
    self.ok() && self.dof == 0
  }
}

impl Sketch {
  /// Solves all constraints, moving geometry as little as possible. Positions are written back
  /// when the solve succeeds; on failure the best attempt is kept only if `keep_on_failure`.
  pub fn solve(&mut self) -> SolveReport {
    self.solve_inner(None, true)
  }

  /// Moves `point` towards `to` and re-solves with it held there; if that conflicts with the
  /// constraints, solves with the point free instead (so it snaps back to the nearest valid place).
  /// The sketch is left unchanged if neither works.
  pub fn drag(&mut self, point: PointId, to: [f64; 2]) -> SolveReport {
    let before = self.clone();
    let mut fixed_pos = false;
    if let Some(p) = self.point_mut(point)
      && !p.fixed
    {
      p.x = to[0];
      p.y = to[1];
      fixed_pos = true;
    }
    if fixed_pos {
      let r = self.solve_inner(Some(point), false);
      if r.ok() {
        // DOF/free-point info must describe the sketch without the drag hold.
        return self.solve_inner(None, false);
      }
      *self = before.clone();
      if let Some(p) = self.point_mut(point) {
        p.x = to[0];
        p.y = to[1];
      }
    }
    let r = self.solve_inner(None, false);
    if !r.ok() {
      *self = before;
    }
    r
  }

  fn solve_inner(&mut self, hold: Option<PointId>, keep_on_failure: bool) -> SolveReport {
    let sys = System::new(self, hold);
    let n = sys.n_vars();
    let m = sys.n_rows;
    if m > MAX_EQUATIONS || n > MAX_VARS {
      return SolveReport {
        status: SolveStatus::TooLarge,
        iterations: 0,
        residual: f64::INFINITY,
        dof: n,
        free_points: Vec::new(),
        free_curves: Vec::new(),
        failing: Vec::new(),
      };
    }
    let mut x = sys.x0.clone();
    let mut r = sys.residuals(&x);
    let mut f = norm2(&r);
    let mut lambda = 1e-9;
    let mut iterations = 0;
    while iterations < MAX_ITERS && max_abs(&r) > TOL && n > 0 {
      iterations += 1;
      let j = sys.jacobian(&x);
      let jjt = mul_aat(&j, m, n);
      let scale = (0..m).map(|i| jjt[i * m + i]).fold(0.0, f64::max).max(1e-12);
      let mut improved = false;
      while lambda < 1e8 {
        let mut a = jjt.clone();
        for i in 0..m {
          a[i * m + i] += lambda * scale + 1e-14;
        }
        let Some(y) = cholesky_solve(&mut a, &r, m) else {
          lambda *= 10.0;
          continue;
        };
        // dx = -Jᵀ y
        let mut xn = x.clone();
        for (k, xk) in xn.iter_mut().enumerate() {
          let mut s = 0.0;
          for i in 0..m {
            s += j[i * n + k] * y[i];
          }
          *xk -= s;
        }
        let rn = sys.residuals(&xn);
        let fnew = norm2(&rn);
        if fnew.is_finite() && fnew < f {
          x = xn;
          r = rn;
          f = fnew;
          lambda = (lambda / 10.0).max(1e-12);
          improved = true;
          break;
        }
        lambda *= 10.0;
      }
      if !improved {
        break;
      }
    }
    let residual = max_abs(&r);
    let solved = residual <= TOL * 1e3;
    let failing = if solved {
      Vec::new()
    } else {
      let mut bad = Vec::new();
      for (row, eq) in sys.rows_of.iter().zip(&sys.eqns) {
        if let Eqn::User(ci) = eq
          && (row.0..row.1).any(|i| r.get(i).is_some_and(|v| v.abs() > 1e-6))
          && let Some(c) = sys.sketch.constraints.get(*ci)
        {
          bad.push(c.id);
        }
      }
      bad
    };
    let (rank, free) = if n > 0 && m > 0 { analyze(&sys.jacobian(&x), m, n) } else { (0, vec![true; n]) };
    let is_free = |v: Option<usize>| v.is_some_and(|v| free.get(v).copied().unwrap_or(false));
    let mut free_points = Vec::new();
    let mut free_curves = Vec::new();
    for (pi, p) in sys.sketch.points.iter().enumerate() {
      if is_free(sys.px.get(pi).copied().flatten()) || is_free(sys.py.get(pi).copied().flatten()) {
        free_points.push(p.id);
      }
    }
    for (ci, c) in sys.sketch.curves.iter().enumerate() {
      if is_free(sys.cr.get(ci).copied().flatten()) {
        free_curves.push(c.id);
      }
    }
    let vars = VarMap { px: sys.px.clone(), py: sys.py.clone(), cr: sys.cr.clone() };
    drop(sys);
    if solved || keep_on_failure {
      vars.write_back(self, &x);
      self.place_texts();
    }
    SolveReport {
      status: if solved { SolveStatus::Solved } else { SolveStatus::Failed },
      iterations,
      residual,
      dof: n.saturating_sub(rank),
      free_points,
      free_curves,
      failing,
    }
  }
}

/// Where each point coordinate / radius lives in the unknown vector.
struct VarMap {
  px: Vec<Option<usize>>,
  py: Vec<Option<usize>>,
  cr: Vec<Option<usize>>,
}

impl VarMap {
  fn write_back(&self, sketch: &mut Sketch, x: &[f64]) {
    for (i, p) in sketch.points.iter_mut().enumerate() {
      if let Some(Some(v)) = self.px.get(i) {
        p.x = x.get(*v).copied().unwrap_or(p.x);
      }
      if let Some(Some(v)) = self.py.get(i) {
        p.y = x.get(*v).copied().unwrap_or(p.y);
      }
    }
    for (i, c) in sketch.curves.iter_mut().enumerate() {
      if let (Some(Some(v)), CurveKind::Circle { radius, .. }) = (self.cr.get(i), &mut c.kind)
        && let Some(r) = x.get(*v)
      {
        *radius = r.abs().max(1e-9);
      }
    }
  }
}

#[derive(Debug, Clone, Copy)]
enum Eqn {
  /// A user constraint (index into `sketch.constraints`).
  User(usize),
  /// Implicit: an arc's end lies on the circle through its start (index into `sketch.curves`).
  ArcRadius(usize),
}

/// The equation system for one sketch, with variable mapping and per-constraint branch choices
/// frozen at the start (which side of a line a point stays on, internal vs external tangency).
struct System<'a> {
  sketch: &'a Sketch,
  pidx: HashMap<PointId, usize>,
  cidx: HashMap<CurveId, usize>,
  /// Variable index of each point's x / y (None = fixed).
  px: Vec<Option<usize>>,
  py: Vec<Option<usize>>,
  /// Variable index of each circle's radius.
  cr: Vec<Option<usize>>,
  x0: Vec<f64>,
  eqns: Vec<Eqn>,
  /// Residual row range of each equation.
  rows_of: Vec<(usize, usize)>,
  /// Variables each equation depends on.
  deps: Vec<Vec<usize>>,
  /// Sign / branch chosen per equation at the start.
  sign: Vec<f64>,
  /// Which ends meet, per spline tangency / curvature equation, chosen at the start.
  joint: Vec<Joint>,
  n_rows: usize,
}

/// Where two curves meet for a spline tangency or curvature constraint.
#[derive(Debug, Clone, Copy, Default)]
struct Joint {
  /// Use the end (not the start) of curve a / b.
  ends: [bool; 2],
  /// Length that makes the curvature residual comparable to the others.
  scale: f64,
}

impl<'a> System<'a> {
  fn new(sketch: &'a Sketch, hold: Option<PointId>) -> Self {
    let pidx = sketch.point_index();
    let cidx = sketch.curve_index();
    let mut x0 = Vec::new();
    let mut px = Vec::with_capacity(sketch.points.len());
    let mut py = Vec::with_capacity(sketch.points.len());
    for p in &sketch.points {
      if p.fixed || Some(p.id) == hold {
        px.push(None);
        py.push(None);
      } else {
        px.push(Some(x0.len()));
        x0.push(p.x);
        py.push(Some(x0.len()));
        x0.push(p.y);
      }
    }
    let mut cr = Vec::with_capacity(sketch.curves.len());
    for c in &sketch.curves {
      if let CurveKind::Circle { radius, .. } = c.kind {
        cr.push(Some(x0.len()));
        x0.push(radius);
      } else {
        cr.push(None);
      }
    }
    let mut eqns: Vec<Eqn> = (0..sketch.constraints.len()).map(Eqn::User).collect();
    for (i, c) in sketch.curves.iter().enumerate() {
      if matches!(c.kind, CurveKind::Arc { .. }) {
        eqns.push(Eqn::ArcRadius(i));
      }
    }
    let mut sys = System {
      sketch,
      pidx,
      cidx,
      px,
      py,
      cr,
      x0,
      eqns,
      rows_of: Vec::new(),
      deps: Vec::new(),
      sign: Vec::new(),
      joint: Vec::new(),
      n_rows: 0,
    };
    let x0 = sys.x0.clone();
    let mut rows = 0;
    for k in 0..sys.eqns.len() {
      let eq = sys.eqns[k];
      let sign = sys.branch(eq, &x0);
      sys.sign.push(sign);
      let joint = sys.joint_of(eq, &x0);
      sys.joint.push(joint);
      let n = sys.eval_at(k, &x0).len();
      sys.rows_of.push((rows, rows + n));
      rows += n;
      let d = sys.eq_deps(eq);
      sys.deps.push(d);
    }
    sys.n_rows = rows;
    sys
  }

  fn n_vars(&self) -> usize {
    self.x0.len()
  }

  fn residuals(&self, x: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(self.n_rows);
    for k in 0..self.eqns.len() {
      out.extend(self.eval_at(k, x));
    }
    out
  }

  /// Residuals of equation `k` with its frozen branch choices.
  fn eval_at(&self, k: usize, x: &[f64]) -> Vec<f64> {
    let Some(eq) = self.eqns.get(k) else { return vec![] };
    let sign = self.sign.get(k).copied().unwrap_or(1.0);
    let joint = self.joint.get(k).copied().unwrap_or_default();
    match eq {
      Eqn::User(ci) => match self.sketch.constraints.get(*ci).map(|c| &c.kind) {
        Some(ConstraintKind::Tangent { a, b }) if self.is_free(*a) || self.is_free(*b) => self.smooth(x, *a, *b, joint, false),
        Some(ConstraintKind::Curvature { a, b }) => self.smooth(x, *a, *b, joint, true),
        Some(ConstraintKind::PointOnCurve { p, curve }) if self.is_free(*curve) => {
          let path = self.free_path(x, *curve);
          if path.is_empty() { vec![0.0] } else { vec![crate::freeform::path_distance(&path, self.p(x, *p))] }
        }
        _ => self.eval(*eq, sign, x),
      },
      Eqn::ArcRadius(_) => self.eval(*eq, sign, x),
    }
  }

  /// Sparse central-difference Jacobian, dense row-major m×n.
  fn jacobian(&self, x: &[f64]) -> Vec<f64> {
    let n = self.n_vars();
    let mut j = vec![0.0; self.n_rows * n];
    let mut xp = x.to_vec();
    for k in 0..self.eqns.len() {
      let (r0, _) = self.rows_of[k];
      for &v in &self.deps[k] {
        let orig = xp[v];
        let h = 1e-7 * (1.0 + orig.abs());
        xp[v] = orig + h;
        let rp = self.eval_at(k, &xp);
        xp[v] = orig - h;
        let rm = self.eval_at(k, &xp);
        xp[v] = orig;
        for (i, (a, b)) in rp.iter().zip(&rm).enumerate() {
          j[(r0 + i) * n + v] = (a - b) / (2.0 * h);
        }
      }
    }
    j
  }

  fn p(&self, x: &[f64], id: PointId) -> [f64; 2] {
    let Some(&i) = self.pidx.get(&id) else { return [0.0, 0.0] };
    let Some(pt) = self.sketch.points.get(i) else { return [0.0, 0.0] };
    let vx = self.px.get(i).copied().flatten().and_then(|v| x.get(v).copied()).unwrap_or(pt.x);
    let vy = self.py.get(i).copied().flatten().and_then(|v| x.get(v).copied()).unwrap_or(pt.y);
    [vx, vy]
  }

  fn curve(&self, id: CurveId) -> Option<(usize, &CurveKind)> {
    let i = *self.cidx.get(&id)?;
    Some((i, &self.sketch.curves.get(i)?.kind))
  }

  fn line(&self, x: &[f64], id: CurveId) -> ([f64; 2], [f64; 2]) {
    match self.curve(id) {
      Some((_, CurveKind::Line { a, b })) => (self.p(x, *a), self.p(x, *b)),
      _ => ([0.0, 0.0], [1.0, 0.0]),
    }
  }

  /// Centre and radius of a circle or arc.
  fn round(&self, x: &[f64], id: CurveId) -> ([f64; 2], f64) {
    match self.curve(id) {
      Some((i, CurveKind::Circle { center, radius })) => {
        let r = self.cr.get(i).copied().flatten().and_then(|v| x.get(v).copied()).unwrap_or(*radius);
        (self.p(x, *center), r)
      }
      Some((_, CurveKind::Arc { center, start, .. })) => {
        let c = self.p(x, *center);
        (c, crate::dist(c, self.p(x, *start)))
      }
      _ => ([0.0, 0.0], 0.0),
    }
  }

  fn is_line(&self, id: CurveId) -> bool {
    matches!(self.curve(id), Some((_, CurveKind::Line { .. })))
  }

  fn is_free(&self, id: CurveId) -> bool {
    self.curve(id).is_some_and(|(_, k)| k.is_freeform())
  }

  /// A freeform curve's Bézier pieces at `x`.
  fn free_path(&self, x: &[f64], id: CurveId) -> Vec<[[f64; 2]; 4]> {
    match self.curve(id) {
      Some((_, kind)) => crate::kind_beziers(kind, &|p| Some(self.p(x, p))),
      None => Vec::new(),
    }
  }

  /// (position, outward direction, curvature) of a curve at the joint end, or for a circle at the
  /// point nearest `toward`.
  fn frame(&self, x: &[f64], id: CurveId, at_end: bool, toward: Option<([f64; 2], [f64; 2])>) -> Option<([f64; 2], [f64; 2], f64)> {
    let (_, kind) = self.curve(id)?;
    if let CurveKind::Circle { .. } = kind {
      let (q, tq) = toward?;
      let (c, r) = self.round(x, id);
      let u = unit(sub(q, c));
      let ccw = [-u[1], u[0]];
      let p = [c[0] + u[0] * r, c[1] + u[1] * r];
      return Some(if dot(ccw, tq) <= 0.0 { (p, ccw, 1.0 / r) } else { (p, [-ccw[0], -ccw[1]], -1.0 / r) });
    }
    crate::freeform::end_frame(kind, at_end, &|p| Some(self.p(x, p)))
  }

  /// Tangency (and with `curvature`, equal curvature) where a spline meets another curve.
  fn smooth(&self, x: &[f64], a: CurveId, b: CurveId, joint: Joint, curvature: bool) -> Vec<f64> {
    let is_circle = |id| matches!(self.curve(id), Some((_, CurveKind::Circle { .. })));
    // a circle has no ends: it is met where the other curve's end is
    let (fa, fb) = if is_circle(a) {
      let fb = self.frame(x, b, joint.ends[1], None);
      (fb.and_then(|f| self.frame(x, a, false, Some((f.0, f.1)))), fb)
    } else {
      let fa = self.frame(x, a, joint.ends[0], None);
      (fa, fa.and_then(|f| self.frame(x, b, joint.ends[1], Some((f.0, f.1)))))
    };
    let (Some((_, ta, ka)), Some((_, tb, kb))) = (fa, fb) else {
      return if curvature { vec![0.0, 0.0] } else { vec![0.0] };
    };
    let mut r = vec![cross(ta, tb)];
    if curvature {
      // both curvatures are taken arriving at the joint, so a smooth joint has them opposite
      r.push((ka + kb) * joint.scale);
    }
    r
  }

  /// Picks the ends that meet for spline tangency / curvature (the closest pair).
  fn joint_of(&self, eq: Eqn, x: &[f64]) -> Joint {
    let Eqn::User(ci) = eq else { return Joint::default() };
    let (a, b) = match self.sketch.constraints.get(ci).map(|c| &c.kind) {
      Some(ConstraintKind::Tangent { a, b }) if self.is_free(*a) || self.is_free(*b) => (*a, *b),
      Some(ConstraintKind::Curvature { a, b }) => (*a, *b),
      _ => return Joint::default(),
    };
    let ends = |id: CurveId| -> Vec<(bool, [f64; 2])> {
      match self.curve(id) {
        Some((_, CurveKind::Circle { .. })) | None => vec![],
        Some((_, kind)) => [false, true]
          .into_iter()
          .filter_map(|e| crate::freeform::end_frame(kind, e, &|p| Some(self.p(x, p))).map(|f| (e, f.0)))
          .collect(),
      }
    };
    let (ea, eb) = (ends(a), ends(b));
    let span = |v: &[(bool, [f64; 2])]| if v.len() == 2 { crate::dist(v[0].1, v[1].1) } else { 0.0 };
    let scale = span(&ea).max(span(&eb)).max(1e-3);
    let mut best = (f64::INFINITY, [true, true]);
    if ea.is_empty() || eb.is_empty() {
      // one is a circle: the other's end nearest it
      let (circle, other, flip) = if ea.is_empty() { (a, &eb, true) } else { (b, &ea, false) };
      let (c, r) = self.round(x, circle);
      for (e, p) in other {
        let d = (crate::dist(c, *p) - r).abs();
        if d < best.0 {
          best = (d, if flip { [false, *e] } else { [*e, false] });
        }
      }
    } else {
      for (e1, p1) in &ea {
        for (e2, p2) in &eb {
          let d = crate::dist(*p1, *p2);
          if d < best.0 - 1e-12 {
            best = (d, [*e1, *e2]);
          }
        }
      }
    }
    Joint { ends: best.1, scale }
  }

  fn curve_vars(&self, id: CurveId, out: &mut Vec<usize>) {
    if let Some((i, kind)) = self.curve(id) {
      for p in crate::curve_points(kind) {
        self.point_vars(p, out);
      }
      if let Some(Some(v)) = self.cr.get(i) {
        out.push(*v);
      }
    }
  }

  fn point_vars(&self, id: PointId, out: &mut Vec<usize>) {
    if let Some(&i) = self.pidx.get(&id) {
      out.extend(self.px.get(i).copied().flatten());
      out.extend(self.py.get(i).copied().flatten());
    }
  }

  fn eq_deps(&self, eq: Eqn) -> Vec<usize> {
    let mut out = Vec::new();
    match eq {
      Eqn::User(ci) => {
        if let Some(c) = self.sketch.constraints.get(ci) {
          for p in c.kind.points() {
            self.point_vars(p, &mut out);
          }
          for cv in c.kind.curves() {
            self.curve_vars(cv, &mut out);
          }
        }
      }
      Eqn::ArcRadius(i) => {
        if let Some(c) = self.sketch.curves.get(i) {
          self.curve_vars(c.id, &mut out);
        }
      }
    }
    out.sort_unstable();
    out.dedup();
    out
  }

  /// Picks the branch (sign) of sign-dependent constraints from the starting configuration.
  fn branch(&self, eq: Eqn, x: &[f64]) -> f64 {
    let Eqn::User(ci) = eq else { return 1.0 };
    let Some(c) = self.sketch.constraints.get(ci) else { return 1.0 };
    let sgn = |v: f64| if v < 0.0 { -1.0 } else { 1.0 };
    match &c.kind {
      ConstraintKind::HorizontalDistance { a, b, .. } => sgn(self.p(x, *b)[0] - self.p(x, *a)[0]),
      ConstraintKind::VerticalDistance { a, b, .. } => sgn(self.p(x, *b)[1] - self.p(x, *a)[1]),
      ConstraintKind::PointLineDistance { p, line, .. } => {
        let (a, b) = self.line(x, *line);
        sgn(cross(unit(sub(b, a)), sub(self.p(x, *p), a)))
      }
      ConstraintKind::Tangent { a, b } => {
        if self.is_line(*a) || self.is_line(*b) {
          let (l, r) = if self.is_line(*a) { (*a, *b) } else { (*b, *a) };
          let (la, lb) = self.line(x, l);
          let (c, _) = self.round(x, r);
          sgn(cross(unit(sub(lb, la)), sub(c, la)))
        } else {
          // +1 = external tangency, -1 = internal (one circle inside the other)
          let (c1, r1) = self.round(x, *a);
          let (c2, r2) = self.round(x, *b);
          if crate::dist(c1, c2) < r1.max(r2) { -1.0 } else { 1.0 }
        }
      }
      _ => 1.0,
    }
  }

  fn eval(&self, eq: Eqn, sign: f64, x: &[f64]) -> Vec<f64> {
    match eq {
      Eqn::ArcRadius(i) => match self.sketch.curves.get(i).map(|c| &c.kind) {
        Some(CurveKind::Arc { center, start, end }) => {
          let c = self.p(x, *center);
          vec![crate::dist(c, self.p(x, *end)) - crate::dist(c, self.p(x, *start))]
        }
        _ => vec![],
      },
      Eqn::User(ci) => match self.sketch.constraints.get(ci) {
        Some(c) => self.eval_user(&c.kind, sign, x),
        None => vec![],
      },
    }
  }

  fn eval_user(&self, kind: &ConstraintKind, sign: f64, x: &[f64]) -> Vec<f64> {
    use ConstraintKind as K;
    match kind {
      K::Coincident { a, b } => {
        let (a, b) = (self.p(x, *a), self.p(x, *b));
        vec![a[0] - b[0], a[1] - b[1]]
      }
      K::Fix { p, x: fx, y: fy } => {
        let p = self.p(x, *p);
        vec![p[0] - fx, p[1] - fy]
      }
      K::Horizontal { line } => {
        let (a, b) = self.line(x, *line);
        vec![a[1] - b[1]]
      }
      K::Vertical { line } => {
        let (a, b) = self.line(x, *line);
        vec![a[0] - b[0]]
      }
      K::HorizontalPoints { a, b } => vec![self.p(x, *a)[1] - self.p(x, *b)[1]],
      K::VerticalPoints { a, b } => vec![self.p(x, *a)[0] - self.p(x, *b)[0]],
      K::Parallel { a, b } => {
        let (u, v) = (self.dir(x, *a), self.dir(x, *b));
        vec![cross(u, v)]
      }
      K::Perpendicular { a, b } => {
        let (u, v) = (self.dir(x, *a), self.dir(x, *b));
        vec![dot(u, v)]
      }
      K::Collinear { a, b } => {
        let (a0, a1) = self.line(x, *a);
        let (b0, b1) = self.line(x, *b);
        let u = unit(sub(a1, a0));
        vec![cross(u, sub(b0, a0)), cross(u, sub(b1, a0))]
      }
      K::Equal { a, b } => {
        if self.is_line(*a) {
          let (a0, a1) = self.line(x, *a);
          let (b0, b1) = self.line(x, *b);
          vec![crate::dist(a0, a1) - crate::dist(b0, b1)]
        } else {
          vec![self.round(x, *a).1 - self.round(x, *b).1]
        }
      }
      K::Tangent { a, b } => {
        if self.is_line(*a) || self.is_line(*b) {
          let (l, r) = if self.is_line(*a) { (*a, *b) } else { (*b, *a) };
          let (la, lb) = self.line(x, l);
          let (c, rad) = self.round(x, r);
          vec![sign * cross(unit(sub(lb, la)), sub(c, la)) - rad]
        } else {
          let (c1, r1) = self.round(x, *a);
          let (c2, r2) = self.round(x, *b);
          let d = crate::dist(c1, c2);
          if sign > 0.0 { vec![d - (r1 + r2)] } else { vec![d - (r1 - r2).abs()] }
        }
      }
      K::Concentric { a, b } => {
        let (c1, _) = self.round(x, *a);
        let (c2, _) = self.round(x, *b);
        vec![c1[0] - c2[0], c1[1] - c2[1]]
      }
      K::PointOnCurve { p, curve } => {
        let p = self.p(x, *p);
        if self.is_line(*curve) {
          let (a, b) = self.line(x, *curve);
          vec![cross(unit(sub(b, a)), sub(p, a))]
        } else {
          let (c, r) = self.round(x, *curve);
          vec![crate::dist(c, p) - r]
        }
      }
      K::Midpoint { p, line } => {
        let p = self.p(x, *p);
        let (a, b) = self.line(x, *line);
        vec![p[0] - (a[0] + b[0]) / 2.0, p[1] - (a[1] + b[1]) / 2.0]
      }
      K::Symmetric { a, b, line } => {
        let (pa, pb) = (self.p(x, *a), self.p(x, *b));
        let (la, lb) = self.line(x, *line);
        let u = unit(sub(lb, la));
        let m = [(pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0];
        vec![cross(u, sub(m, la)), dot(u, sub(pb, pa))]
      }
      K::Distance { a, b, value } => vec![crate::dist(self.p(x, *a), self.p(x, *b)) - value.value],
      K::HorizontalDistance { a, b, value } => vec![sign * (self.p(x, *b)[0] - self.p(x, *a)[0]) - value.value],
      K::VerticalDistance { a, b, value } => vec![sign * (self.p(x, *b)[1] - self.p(x, *a)[1]) - value.value],
      K::PointLineDistance { p, line, value } => {
        let (a, b) = self.line(x, *line);
        vec![sign * cross(unit(sub(b, a)), sub(self.p(x, *p), a)) - value.value]
      }
      K::Radius { curve, value } => vec![self.round(x, *curve).1 - value.value],
      K::Diameter { curve, value } => vec![2.0 * self.round(x, *curve).1 - value.value],
      // spline joints are evaluated in `eval_at`
      K::Curvature { .. } => vec![0.0, 0.0],
      K::Angle { a, b, value } => {
        let (u, v) = (self.dir(x, *a), self.dir(x, *b));
        // sin(φ − θ) where φ is the angle from a to b
        let (s, c) = value.value.sin_cos();
        vec![cross(u, v) * c - dot(u, v) * s]
      }
    }
  }

  fn dir(&self, x: &[f64], line: CurveId) -> [f64; 2] {
    let (a, b) = self.line(x, line);
    unit(sub(b, a))
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

fn unit(a: [f64; 2]) -> [f64; 2] {
  let l = a[0].hypot(a[1]);
  if l > 1e-15 { [a[0] / l, a[1] / l] } else { [1.0, 0.0] }
}

fn norm2(v: &[f64]) -> f64 {
  v.iter().map(|x| x * x).sum()
}

fn max_abs(v: &[f64]) -> f64 {
  v.iter().fold(0.0, |m, x| if x.is_nan() { f64::INFINITY } else { m.max(x.abs()) })
}

/// A Aᵀ for row-major m×n A.
fn mul_aat(a: &[f64], m: usize, n: usize) -> Vec<f64> {
  let mut out = vec![0.0; m * m];
  for i in 0..m {
    let ri = &a[i * n..(i + 1) * n];
    for k in i..m {
      let rk = &a[k * n..(k + 1) * n];
      let s: f64 = ri.iter().zip(rk).map(|(p, q)| p * q).sum();
      out[i * m + k] = s;
      out[k * m + i] = s;
    }
  }
  out
}

/// Solves A y = b for symmetric positive definite A (destroys A). None if not SPD.
fn cholesky_solve(a: &mut [f64], b: &[f64], m: usize) -> Option<Vec<f64>> {
  for j in 0..m {
    let mut d = a[j * m + j];
    for k in 0..j {
      d -= a[j * m + k] * a[j * m + k];
    }
    if !(d > 0.0) || !d.is_finite() {
      return None;
    }
    let d = d.sqrt();
    a[j * m + j] = d;
    for i in (j + 1)..m {
      let mut s = a[i * m + j];
      for k in 0..j {
        s -= a[i * m + k] * a[j * m + k];
      }
      a[i * m + j] = s / d;
    }
  }
  let mut y = b.to_vec();
  for i in 0..m {
    let mut s = y[i];
    for k in 0..i {
      s -= a[i * m + k] * y[k];
    }
    y[i] = s / a[i * m + i];
  }
  for i in (0..m).rev() {
    let mut s = y[i];
    for k in (i + 1)..m {
      s -= a[k * m + i] * y[k];
    }
    y[i] = s / a[i * m + i];
  }
  Some(y)
}

/// Rank of J and, per variable, whether it can still move (has a component in the null space).
fn analyze(j: &[f64], m: usize, n: usize) -> (usize, Vec<bool>) {
  let mut a = j.to_vec();
  let scale = a.iter().fold(0.0f64, |s, v| s.max(v.abs())).max(1e-12);
  let tol = 1e-7 * scale;
  let mut pivots: Vec<usize> = Vec::new(); // pivot column per row
  let mut row = 0;
  for col in 0..n {
    if row >= m {
      break;
    }
    let (best, val) =
      (row..m).map(|r| (r, a[r * n + col].abs())).fold((row, 0.0), |b, c| if c.1 > b.1 { c } else { b });
    if val <= tol {
      continue;
    }
    if best != row {
      for k in 0..n {
        a.swap(best * n + k, row * n + k);
      }
    }
    let p = a[row * n + col];
    for k in 0..n {
      a[row * n + k] /= p;
    }
    for r in 0..m {
      if r != row {
        let f = a[r * n + col];
        if f != 0.0 {
          for k in 0..n {
            a[r * n + k] -= f * a[row * n + k];
          }
        }
      }
    }
    pivots.push(col);
    row += 1;
  }
  let rank = pivots.len();
  let mut is_pivot = vec![false; n];
  for &c in &pivots {
    is_pivot[c] = true;
  }
  let mut free = vec![false; n];
  // Null-space basis: one vector per free column f: x_f = 1, x_pivot(r) = -a[r][f].
  for f in 0..n {
    if is_pivot[f] {
      continue;
    }
    free[f] = true;
    for (r, &pc) in pivots.iter().enumerate() {
      if a[r * n + f].abs() > 1e-6 {
        free[pc] = true;
      }
    }
  }
  (rank, free)
}

#[cfg(test)]
mod tests {
  use crate::*;

  fn rect_fully_constrained() -> (Sketch, [CurveId; 4]) {
    let mut s = Sketch::new();
    let lines = s.add_rectangle([1.0, 1.0], [21.0, 11.0]).unwrap();
    let bl = s.curve_points(lines[0])[0];
    s.add_constraint(ConstraintKind::Coincident { a: bl, b: ORIGIN }).unwrap();
    let [p0, p1] = [s.curve_points(lines[0])[0], s.curve_points(lines[0])[1]];
    s.add_constraint(ConstraintKind::Distance { a: p0, b: p1, value: Dim::new(40.0) }).unwrap();
    let [q0, q1] = [s.curve_points(lines[1])[0], s.curve_points(lines[1])[1]];
    s.add_constraint(ConstraintKind::Distance { a: q0, b: q1, value: Dim::new(25.0) }).unwrap();
    (s, lines)
  }

  #[test]
  fn rectangle_solves_and_is_fully_constrained() {
    let (mut s, lines) = rect_fully_constrained();
    let r = s.solve();
    assert!(r.ok(), "{r:?}");
    assert_eq!(r.dof, 0, "{r:?}");
    assert!(r.free_points.is_empty());
    let tr = s.curve_points(lines[1])[1];
    let p = s.pos(tr).unwrap();
    assert!((p[0] - 40.0).abs() < 1e-6 && (p[1] - 25.0).abs() < 1e-6, "{p:?}");
  }

  #[test]
  fn dof_counts() {
    let mut s = Sketch::new();
    s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(10.0, 0.0)).unwrap();
    let r = s.solve();
    assert_eq!(r.dof, 4);
    let mut s = Sketch::new();
    s.add_rectangle([0.0, 0.0], [10.0, 5.0]).unwrap();
    assert_eq!(s.solve().dof, 4); // x, y, width, height
    let mut s = Sketch::new();
    let c = s.add_circle(PtSpec::Point(ORIGIN), 5.0).unwrap();
    assert_eq!(s.solve().dof, 1);
    s.add_constraint(ConstraintKind::Diameter { curve: c, value: Dim::new(20.0) }).unwrap();
    let r = s.solve();
    assert!(r.fully_constrained());
    assert!((s.radius(c).unwrap() - 10.0).abs() < 1e-9);
  }

  #[test]
  fn changing_a_dimension_moves_geometry() {
    let (mut s, lines) = rect_fully_constrained();
    s.solve();
    let id = s.constraints.iter().find(|c| matches!(c.kind, ConstraintKind::Distance { .. })).unwrap().id;
    s.set_dim(id, Dim::new(60.0)).unwrap();
    assert!(s.solve().ok());
    let br = s.curve_points(lines[0])[1];
    assert!((s.pos(br).unwrap()[0] - 60.0).abs() < 1e-6);
  }

  #[test]
  fn conflicting_constraints_fail() {
    let mut s = Sketch::new();
    let (l, [a, b]) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(10.0, 1.0)).unwrap();
    s.add_constraint(ConstraintKind::Horizontal { line: l }).unwrap();
    s.add_constraint(ConstraintKind::Vertical { line: l }).unwrap();
    s.add_constraint(ConstraintKind::Distance { a, b, value: Dim::new(10.0) }).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Failed);
    assert!(!r.failing.is_empty());
  }

  #[test]
  fn tangent_angle_parallel_perpendicular() {
    let mut s = Sketch::new();
    let (l1, _) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(10.0, 1.0)).unwrap();
    let (l2, _) = s.add_line(PtSpec::At(0.0, 5.0), PtSpec::At(10.0, 4.0)).unwrap();
    let (l3, _) = s.add_line(PtSpec::At(20.0, 0.0), PtSpec::At(21.0, 10.0)).unwrap();
    let (l4, _) = s.add_line(PtSpec::At(30.0, 0.0), PtSpec::At(40.0, 3.0)).unwrap();
    let c = s.add_circle(PtSpec::At(5.0, 12.0), 3.0).unwrap();
    s.add_constraint(ConstraintKind::Parallel { a: l1, b: l2 }).unwrap();
    s.add_constraint(ConstraintKind::Perpendicular { a: l1, b: l3 }).unwrap();
    s.add_constraint(ConstraintKind::Angle { a: l1, b: l4, value: Dim::new(0.5) }).unwrap();
    s.add_constraint(ConstraintKind::Tangent { a: l2, b: c }).unwrap();
    let r = s.solve();
    assert!(r.ok(), "{r:?}");
    let d = |s: &Sketch, l| {
      let p = s.curve_points(l);
      let (a, b) = (s.pos(p[0]).unwrap(), s.pos(p[1]).unwrap());
      (b[1] - a[1]).atan2(b[0] - a[0])
    };
    let a1 = d(&s, l1);
    assert!(((d(&s, l2) - a1).sin()).abs() < 1e-6);
    assert!(((d(&s, l3) - a1).cos()).abs() < 1e-6);
    assert!(((d(&s, l4) - a1) - 0.5).sin().abs() < 1e-6);
    let cc = s.pos(s.curve_points(c)[0]).unwrap();
    assert!((s.distance_to_curve(l2, cc).unwrap() - s.radius(c).unwrap()).abs() < 1e-6);
  }

  #[test]
  fn slot_and_polygon_solve() {
    let mut s = Sketch::new();
    s.add_slot([0.0, 0.0], [30.0, 0.0], 10.0).unwrap();
    s.add_polygon(PtSpec::At(0.0, 40.0), [10.0, 40.0], 6).unwrap();
    let r = s.solve();
    assert!(r.ok(), "{r:?}");
  }

  #[test]
  fn drag_moves_free_geometry_only() {
    let mut s = Sketch::new();
    let lines = s.add_rectangle([0.0, 0.0], [10.0, 5.0]).unwrap();
    let tr = s.curve_points(lines[1])[1];
    let r = s.drag(tr, [20.0, 8.0]);
    assert!(r.ok());
    let p = s.pos(tr).unwrap();
    assert!((p[0] - 20.0).abs() < 1e-6 && (p[1] - 8.0).abs() < 1e-6);
    // still a rectangle
    let br = s.curve_points(lines[0])[1];
    assert!((s.pos(br).unwrap()[0] - 20.0).abs() < 1e-6);
    // the origin can't be dragged
    s.drag(ORIGIN, [5.0, 5.0]);
    assert_eq!(s.pos(ORIGIN).unwrap(), [0.0, 0.0]);
  }

  #[test]
  fn arc_keeps_its_radius() {
    let mut s = Sketch::new();
    let a = s.add_arc_center(PtSpec::At(0.0, 0.0), PtSpec::At(10.0, 0.0), [0.0, 10.0]).unwrap();
    let pts = s.curve_points(a);
    s.drag(pts[2], [0.0, 15.0]);
    let c = s.pos(pts[0]).unwrap();
    let r1 = super::super::dist(c, s.pos(pts[1]).unwrap());
    let r2 = super::super::dist(c, s.pos(pts[2]).unwrap());
    assert!((r1 - r2).abs() < 1e-6);
  }
}
