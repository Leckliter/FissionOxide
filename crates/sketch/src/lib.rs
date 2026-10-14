//! 2D parametric sketches: points, lines, circles and arcs held in place by geometric constraints
//! and driving dimensions, solved numerically, with closed profiles found for 3D features.
//!
//! Every sketch carries a fixed origin point ([`ORIGIN`]) so geometry can be dimensioned from it
//! and become fully constrained, as in Fusion.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

mod build;
mod profile;
mod solve;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use build::PtSpec;
pub use profile::{Loop, Profile, Seg, find_profiles};
pub use solve::{SolveReport, SolveStatus};

/// The sketch origin: always present, always fixed at (0, 0).
pub const ORIGIN: PointId = PointId(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PointId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CurveId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConstraintId(pub u32);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sketch {
  pub points: Vec<Point>,
  pub curves: Vec<Curve>,
  pub constraints: Vec<Constraint>,
  next_id: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Point {
  pub id: PointId,
  pub x: f64,
  pub y: f64,
  /// Never moved by the solver (the origin, projected geometry).
  #[serde(default)]
  pub fixed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Curve {
  pub id: CurveId,
  pub kind: CurveKind,
  /// Construction geometry guides the sketch but never forms profiles.
  #[serde(default)]
  pub construction: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CurveKind {
  Line {
    a: PointId,
    b: PointId,
  },
  Circle {
    center: PointId,
    radius: f64,
  },
  /// Counter-clockwise from `start` to `end` around `center`.
  Arc {
    center: PointId,
    start: PointId,
    end: PointId,
  },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Constraint {
  pub id: ConstraintId,
  pub kind: ConstraintKind,
}

/// A driving dimension: the expression the user typed and the value it last evaluated to
/// (millimetres or radians).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Dim {
  pub expr: String,
  pub value: f64,
}

impl Dim {
  pub fn new(value: f64) -> Self {
    Dim { expr: clf_expr::trim_number(value), value }
  }

  pub fn expr(expr: impl Into<String>, value: f64) -> Self {
    Dim { expr: expr.into(), value }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConstraintKind {
  Coincident {
    a: PointId,
    b: PointId,
  },
  /// Locks a point where it is.
  Fix {
    p: PointId,
    x: f64,
    y: f64,
  },
  Horizontal {
    line: CurveId,
  },
  Vertical {
    line: CurveId,
  },
  /// Two points on a horizontal line.
  HorizontalPoints {
    a: PointId,
    b: PointId,
  },
  /// Two points on a vertical line.
  VerticalPoints {
    a: PointId,
    b: PointId,
  },
  Parallel {
    a: CurveId,
    b: CurveId,
  },
  Perpendicular {
    a: CurveId,
    b: CurveId,
  },
  Collinear {
    a: CurveId,
    b: CurveId,
  },
  /// Equal length (lines) or equal radius (circles/arcs).
  Equal {
    a: CurveId,
    b: CurveId,
  },
  Tangent {
    a: CurveId,
    b: CurveId,
  },
  Concentric {
    a: CurveId,
    b: CurveId,
  },
  PointOnCurve {
    p: PointId,
    curve: CurveId,
  },
  Midpoint {
    p: PointId,
    line: CurveId,
  },
  /// `a` and `b` mirror each other across `line`.
  Symmetric {
    a: PointId,
    b: PointId,
    line: CurveId,
  },
  Distance {
    a: PointId,
    b: PointId,
    value: Dim,
  },
  HorizontalDistance {
    a: PointId,
    b: PointId,
    value: Dim,
  },
  VerticalDistance {
    a: PointId,
    b: PointId,
    value: Dim,
  },
  PointLineDistance {
    p: PointId,
    line: CurveId,
    value: Dim,
  },
  Radius {
    curve: CurveId,
    value: Dim,
  },
  Diameter {
    curve: CurveId,
    value: Dim,
  },
  /// Angle measured counter-clockwise from line `a` to line `b`.
  Angle {
    a: CurveId,
    b: CurveId,
    value: Dim,
  },
}

impl ConstraintKind {
  /// The driving dimension, if this is a dimensional constraint.
  pub fn dim(&self) -> Option<&Dim> {
    match self {
      ConstraintKind::Distance { value, .. }
      | ConstraintKind::HorizontalDistance { value, .. }
      | ConstraintKind::VerticalDistance { value, .. }
      | ConstraintKind::PointLineDistance { value, .. }
      | ConstraintKind::Radius { value, .. }
      | ConstraintKind::Diameter { value, .. }
      | ConstraintKind::Angle { value, .. } => Some(value),
      _ => None,
    }
  }

  pub fn dim_mut(&mut self) -> Option<&mut Dim> {
    match self {
      ConstraintKind::Distance { value, .. }
      | ConstraintKind::HorizontalDistance { value, .. }
      | ConstraintKind::VerticalDistance { value, .. }
      | ConstraintKind::PointLineDistance { value, .. }
      | ConstraintKind::Radius { value, .. }
      | ConstraintKind::Diameter { value, .. }
      | ConstraintKind::Angle { value, .. } => Some(value),
      _ => None,
    }
  }

  /// Is the dimension an angle (radians) rather than a length (mm)?
  pub fn is_angle(&self) -> bool {
    matches!(self, ConstraintKind::Angle { .. })
  }

  /// Short human name (for lists and errors).
  pub fn name(&self) -> &'static str {
    match self {
      ConstraintKind::Coincident { .. } => "Coincident",
      ConstraintKind::Fix { .. } => "Fix",
      ConstraintKind::Horizontal { .. } | ConstraintKind::HorizontalPoints { .. } => "Horizontal",
      ConstraintKind::Vertical { .. } | ConstraintKind::VerticalPoints { .. } => "Vertical",
      ConstraintKind::Parallel { .. } => "Parallel",
      ConstraintKind::Perpendicular { .. } => "Perpendicular",
      ConstraintKind::Collinear { .. } => "Collinear",
      ConstraintKind::Equal { .. } => "Equal",
      ConstraintKind::Tangent { .. } => "Tangent",
      ConstraintKind::Concentric { .. } => "Concentric",
      ConstraintKind::PointOnCurve { .. } => "Point on curve",
      ConstraintKind::Midpoint { .. } => "Midpoint",
      ConstraintKind::Symmetric { .. } => "Symmetric",
      ConstraintKind::Distance { .. } => "Distance",
      ConstraintKind::HorizontalDistance { .. } => "Horizontal distance",
      ConstraintKind::VerticalDistance { .. } => "Vertical distance",
      ConstraintKind::PointLineDistance { .. } => "Point-line distance",
      ConstraintKind::Radius { .. } => "Radius",
      ConstraintKind::Diameter { .. } => "Diameter",
      ConstraintKind::Angle { .. } => "Angle",
    }
  }

  /// Points this constraint refers to directly.
  pub fn points(&self) -> Vec<PointId> {
    match self {
      ConstraintKind::Coincident { a, b }
      | ConstraintKind::HorizontalPoints { a, b }
      | ConstraintKind::VerticalPoints { a, b }
      | ConstraintKind::Distance { a, b, .. }
      | ConstraintKind::HorizontalDistance { a, b, .. }
      | ConstraintKind::VerticalDistance { a, b, .. }
      | ConstraintKind::Symmetric { a, b, .. } => vec![*a, *b],
      ConstraintKind::Fix { p, .. }
      | ConstraintKind::PointOnCurve { p, .. }
      | ConstraintKind::Midpoint { p, .. }
      | ConstraintKind::PointLineDistance { p, .. } => vec![*p],
      _ => vec![],
    }
  }

  /// Curves this constraint refers to directly.
  pub fn curves(&self) -> Vec<CurveId> {
    match self {
      ConstraintKind::Horizontal { line } | ConstraintKind::Vertical { line } => vec![*line],
      ConstraintKind::Parallel { a, b }
      | ConstraintKind::Perpendicular { a, b }
      | ConstraintKind::Collinear { a, b }
      | ConstraintKind::Equal { a, b }
      | ConstraintKind::Tangent { a, b }
      | ConstraintKind::Concentric { a, b }
      | ConstraintKind::Angle { a, b, .. } => vec![*a, *b],
      ConstraintKind::PointOnCurve { curve, .. }
      | ConstraintKind::Radius { curve, .. }
      | ConstraintKind::Diameter { curve, .. } => vec![*curve],
      ConstraintKind::Midpoint { line, .. }
      | ConstraintKind::Symmetric { line, .. }
      | ConstraintKind::PointLineDistance { line, .. } => vec![*line],
      _ => vec![],
    }
  }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SketchError {
  #[error("no point {0:?} in this sketch")]
  NoPoint(PointId),
  #[error("no curve {0:?} in this sketch")]
  NoCurve(CurveId),
  #[error("no constraint {0:?} in this sketch")]
  NoConstraint(ConstraintId),
  #[error("{0} needs {1}")]
  WrongGeometry(&'static str, &'static str),
  #[error("the origin can't be deleted")]
  Origin,
  #[error("invalid value: {0}")]
  InvalidValue(String),
}

impl Default for Sketch {
  fn default() -> Self {
    Self::new()
  }
}

impl Sketch {
  pub fn new() -> Self {
    Sketch {
      points: vec![Point { id: ORIGIN, x: 0.0, y: 0.0, fixed: true }],
      curves: Vec::new(),
      constraints: Vec::new(),
      next_id: 1,
    }
  }

  fn alloc(&mut self) -> u32 {
    let id = self.next_id;
    self.next_id = self.next_id.saturating_add(1);
    id
  }

  pub fn point(&self, id: PointId) -> Option<&Point> {
    self.points.iter().find(|p| p.id == id)
  }

  pub fn point_mut(&mut self, id: PointId) -> Option<&mut Point> {
    self.points.iter_mut().find(|p| p.id == id)
  }

  pub fn pos(&self, id: PointId) -> Option<[f64; 2]> {
    self.point(id).map(|p| [p.x, p.y])
  }

  pub fn curve(&self, id: CurveId) -> Option<&Curve> {
    self.curves.iter().find(|c| c.id == id)
  }

  pub fn curve_mut(&mut self, id: CurveId) -> Option<&mut Curve> {
    self.curves.iter_mut().find(|c| c.id == id)
  }

  pub fn constraint(&self, id: ConstraintId) -> Option<&Constraint> {
    self.constraints.iter().find(|c| c.id == id)
  }

  pub fn add_point(&mut self, x: f64, y: f64) -> PointId {
    let id = PointId(self.alloc());
    self.points.push(Point { id, x, y, fixed: false });
    id
  }

  pub fn add_curve(&mut self, kind: CurveKind) -> Result<CurveId, SketchError> {
    for p in curve_points(&kind) {
      if self.point(p).is_none() {
        return Err(SketchError::NoPoint(p));
      }
    }
    if let CurveKind::Circle { radius, .. } = kind
      && !(radius.is_finite() && radius > 0.0)
    {
      return Err(SketchError::InvalidValue(format!("circle radius {radius}")));
    }
    let id = CurveId(self.alloc());
    self.curves.push(Curve { id, kind, construction: false });
    Ok(id)
  }

  /// Adds a constraint after checking it refers to geometry of the right type.
  pub fn add_constraint(&mut self, kind: ConstraintKind) -> Result<ConstraintId, SketchError> {
    self.validate(&kind)?;
    let id = ConstraintId(self.alloc());
    self.constraints.push(Constraint { id, kind });
    Ok(id)
  }

  pub fn remove_constraint(&mut self, id: ConstraintId) -> Result<(), SketchError> {
    let before = self.constraints.len();
    self.constraints.retain(|c| c.id != id);
    if self.constraints.len() == before { Err(SketchError::NoConstraint(id)) } else { Ok(()) }
  }

  /// Deletes a curve, its points that nothing else uses, and constraints that referred to them.
  pub fn remove_curve(&mut self, id: CurveId) -> Result<(), SketchError> {
    let curve = self.curve(id).ok_or(SketchError::NoCurve(id))?.clone();
    self.curves.retain(|c| c.id != id);
    let still_used: Vec<PointId> = self.curves.iter().flat_map(|c| curve_points(&c.kind)).collect();
    let orphans: Vec<PointId> =
      curve_points(&curve.kind).into_iter().filter(|p| *p != ORIGIN && !still_used.contains(p)).collect();
    self.points.retain(|p| !orphans.contains(&p.id));
    self.constraints.retain(|c| !c.kind.curves().contains(&id) && !c.kind.points().iter().any(|p| orphans.contains(p)));
    Ok(())
  }

  /// Deletes a free point (one no curve uses) and its constraints.
  pub fn remove_point(&mut self, id: PointId) -> Result<(), SketchError> {
    if id == ORIGIN {
      return Err(SketchError::Origin);
    }
    if self.point(id).is_none() {
      return Err(SketchError::NoPoint(id));
    }
    let users: Vec<CurveId> =
      self.curves.iter().filter(|c| curve_points(&c.kind).contains(&id)).map(|c| c.id).collect();
    for c in users {
      self.remove_curve(c)?;
    }
    self.points.retain(|p| p.id != id);
    self.constraints.retain(|c| !c.kind.points().contains(&id));
    Ok(())
  }

  pub fn set_construction(&mut self, id: CurveId, construction: bool) -> Result<(), SketchError> {
    self.curve_mut(id).ok_or(SketchError::NoCurve(id))?.construction = construction;
    Ok(())
  }

  /// Sets a dimension's expression and value (caller evaluated it) — does not re-solve.
  pub fn set_dim(&mut self, id: ConstraintId, dim: Dim) -> Result<(), SketchError> {
    let c = self.constraints.iter_mut().find(|c| c.id == id).ok_or(SketchError::NoConstraint(id))?;
    match c.kind.dim_mut() {
      Some(d) => {
        *d = dim;
        Ok(())
      }
      None => Err(SketchError::WrongGeometry("this constraint", "a dimension")),
    }
  }

  /// The radius of a circle or arc at the current positions.
  pub fn radius(&self, id: CurveId) -> Option<f64> {
    match &self.curve(id)?.kind {
      CurveKind::Circle { radius, .. } => Some(*radius),
      CurveKind::Arc { center, start, .. } => {
        let c = self.pos(*center)?;
        let s = self.pos(*start)?;
        Some(dist(c, s))
      }
      CurveKind::Line { .. } => None,
    }
  }

  /// Ids of the points defining a curve.
  pub fn curve_points(&self, id: CurveId) -> Vec<PointId> {
    self.curve(id).map(|c| curve_points(&c.kind)).unwrap_or_default()
  }

  /// Axis-aligned bounds of all geometry (min, max), or None for an empty sketch.
  pub fn bounds(&self) -> Option<([f64; 2], [f64; 2])> {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    let mut any = false;
    let mut add = |p: [f64; 2]| {
      any = true;
      for i in 0..2 {
        min[i] = min[i].min(p[i]);
        max[i] = max[i].max(p[i]);
      }
    };
    for c in &self.curves {
      match &c.kind {
        CurveKind::Line { a, b } => {
          for p in [a, b] {
            if let Some(p) = self.pos(*p) {
              add(p);
            }
          }
        }
        CurveKind::Circle { center, radius } => {
          if let Some(p) = self.pos(*center) {
            add([p[0] - radius, p[1] - radius]);
            add([p[0] + radius, p[1] + radius]);
          }
        }
        CurveKind::Arc { center, .. } => {
          if let (Some(p), Some(r)) = (self.pos(*center), self.radius(c.id)) {
            add([p[0] - r, p[1] - r]);
            add([p[0] + r, p[1] + r]);
          }
        }
      }
    }
    any.then_some((min, max))
  }

  /// Nearest point to `at` within `tol`, ignoring `exclude`.
  pub fn nearest_point(&self, at: [f64; 2], tol: f64, exclude: &[PointId]) -> Option<PointId> {
    self
      .points
      .iter()
      .filter(|p| !exclude.contains(&p.id))
      .map(|p| (p.id, dist([p.x, p.y], at)))
      .filter(|(_, d)| *d <= tol)
      .min_by(|a, b| a.1.total_cmp(&b.1))
      .map(|(id, _)| id)
  }

  /// Nearest curve to `at` within `tol`.
  pub fn nearest_curve(&self, at: [f64; 2], tol: f64) -> Option<CurveId> {
    self
      .curves
      .iter()
      .filter_map(|c| self.distance_to_curve(c.id, at).map(|d| (c.id, d)))
      .filter(|(_, d)| *d <= tol)
      .min_by(|a, b| a.1.total_cmp(&b.1))
      .map(|(id, _)| id)
  }

  /// Distance from a point to a curve (segment / circle / arc).
  pub fn distance_to_curve(&self, id: CurveId, at: [f64; 2]) -> Option<f64> {
    let c = self.curve(id)?;
    Some(match &c.kind {
      CurveKind::Line { a, b } => seg_dist(self.pos(*a)?, self.pos(*b)?, at),
      CurveKind::Circle { center, radius } => (dist(self.pos(*center)?, at) - radius).abs(),
      CurveKind::Arc { center, start, end } => {
        let (cp, sp, ep) = (self.pos(*center)?, self.pos(*start)?, self.pos(*end)?);
        let r = dist(cp, sp);
        let a0 = angle_of(cp, sp);
        let sweep = ccw_sweep(a0, angle_of(cp, ep));
        let a = angle_of(cp, at);
        if ccw_sweep(a0, a) <= sweep { (dist(cp, at) - r).abs() } else { dist(at, sp).min(dist(at, ep)) }
      }
    })
  }

  /// Evaluates every dimension expression with `scope`, storing values. Returns the ids and
  /// errors of dimensions that failed to evaluate (they keep their previous value).
  pub fn evaluate_dims(&mut self, scope: &dyn clf_expr::Scope) -> Vec<(ConstraintId, clf_expr::ExprError)> {
    let mut errors = Vec::new();
    for c in &mut self.constraints {
      let kind = if c.kind.is_angle() { clf_expr::Kind::Angle } else { clf_expr::Kind::Length };
      let id = c.id;
      if let Some(d) = c.kind.dim_mut() {
        match clf_expr::eval(&d.expr, kind, scope) {
          Ok(v) => d.value = v,
          Err(e) => errors.push((id, e)),
        }
      }
    }
    errors
  }

  /// Number of non-origin points; handy for tests and limits.
  pub fn len(&self) -> usize {
    self.curves.len()
  }

  pub fn is_empty(&self) -> bool {
    self.curves.is_empty()
  }

  fn validate(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
    for p in kind.points() {
      if self.point(p).is_none() {
        return Err(SketchError::NoPoint(p));
      }
    }
    let line = |id: CurveId, what: &'static str| -> Result<(), SketchError> {
      match self.curve(id).map(|c| &c.kind) {
        Some(CurveKind::Line { .. }) => Ok(()),
        Some(_) => Err(SketchError::WrongGeometry(what, "lines")),
        None => Err(SketchError::NoCurve(id)),
      }
    };
    let round = |id: CurveId, what: &'static str| -> Result<(), SketchError> {
      match self.curve(id).map(|c| &c.kind) {
        Some(CurveKind::Circle { .. } | CurveKind::Arc { .. }) => Ok(()),
        Some(_) => Err(SketchError::WrongGeometry(what, "circles or arcs")),
        None => Err(SketchError::NoCurve(id)),
      }
    };
    let any = |id: CurveId| -> Result<(), SketchError> { self.curve(id).map(|_| ()).ok_or(SketchError::NoCurve(id)) };
    match kind {
      ConstraintKind::Horizontal { line: l } => line(*l, "Horizontal"),
      ConstraintKind::Vertical { line: l } => line(*l, "Vertical"),
      ConstraintKind::Parallel { a, b } => line(*a, "Parallel").and(line(*b, "Parallel")),
      ConstraintKind::Perpendicular { a, b } => line(*a, "Perpendicular").and(line(*b, "Perpendicular")),
      ConstraintKind::Collinear { a, b } => line(*a, "Collinear").and(line(*b, "Collinear")),
      ConstraintKind::Angle { a, b, .. } => line(*a, "Angle").and(line(*b, "Angle")),
      ConstraintKind::Midpoint { line: l, .. } => line(*l, "Midpoint"),
      ConstraintKind::Symmetric { line: l, .. } => line(*l, "Symmetric"),
      ConstraintKind::PointLineDistance { line: l, .. } => line(*l, "Point-line distance"),
      ConstraintKind::Concentric { a, b } => round(*a, "Concentric").and(round(*b, "Concentric")),
      ConstraintKind::Radius { curve, .. } => round(*curve, "Radius"),
      ConstraintKind::Diameter { curve, .. } => round(*curve, "Diameter"),
      ConstraintKind::Equal { a, b } => {
        any(*a)?;
        any(*b)?;
        let is_line = |id| matches!(self.curve(id).map(|c| &c.kind), Some(CurveKind::Line { .. }));
        if is_line(*a) == is_line(*b) {
          Ok(())
        } else {
          Err(SketchError::WrongGeometry("Equal", "two lines or two circles/arcs"))
        }
      }
      ConstraintKind::Tangent { a, b } => {
        any(*a)?;
        any(*b)?;
        let is_line = |id| matches!(self.curve(id).map(|c| &c.kind), Some(CurveKind::Line { .. }));
        if is_line(*a) && is_line(*b) {
          Err(SketchError::WrongGeometry("Tangent", "at least one circle or arc"))
        } else {
          Ok(())
        }
      }
      ConstraintKind::PointOnCurve { curve, .. } => any(*curve),
      _ => {
        if let Some(d) = kind.dim()
          && !d.value.is_finite()
        {
          return Err(SketchError::InvalidValue(d.expr.clone()));
        }
        Ok(())
      }
    }
  }

  /// Point id → index lookup.
  pub(crate) fn point_index(&self) -> HashMap<PointId, usize> {
    self.points.iter().enumerate().map(|(i, p)| (p.id, i)).collect()
  }

  pub(crate) fn curve_index(&self) -> HashMap<CurveId, usize> {
    self.curves.iter().enumerate().map(|(i, c)| (c.id, i)).collect()
  }
}

/// Points that define a curve.
pub fn curve_points(kind: &CurveKind) -> Vec<PointId> {
  match kind {
    CurveKind::Line { a, b } => vec![*a, *b],
    CurveKind::Circle { center, .. } => vec![*center],
    CurveKind::Arc { center, start, end } => vec![*center, *start, *end],
  }
}

pub(crate) fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
  (a[0] - b[0]).hypot(a[1] - b[1])
}

pub(crate) fn angle_of(c: [f64; 2], p: [f64; 2]) -> f64 {
  (p[1] - c[1]).atan2(p[0] - c[0])
}

/// Counter-clockwise sweep from angle `a0` to `a1`, in (0, 2π].
pub(crate) fn ccw_sweep(a0: f64, a1: f64) -> f64 {
  let tau = std::f64::consts::TAU;
  let s = (a1 - a0).rem_euclid(tau);
  if s <= 1e-12 { tau } else { s }
}

pub(crate) fn seg_dist(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
  let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
  let l2 = dx * dx + dy * dy;
  if l2 <= 0.0 {
    return dist(a, p);
  }
  let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0);
  dist([a[0] + t * dx, a[1] + t * dy], p)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn remove_curve_drops_orphans_and_constraints() {
    let mut s = Sketch::new();
    let (l1, _) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(10.0, 0.0)).unwrap();
    let b = s.curve_points(l1)[1];
    let (l2, _) = s.add_line(PtSpec::Point(b), PtSpec::At(10.0, 10.0)).unwrap();
    s.add_constraint(ConstraintKind::Horizontal { line: l1 }).unwrap();
    s.add_constraint(ConstraintKind::Perpendicular { a: l1, b: l2 }).unwrap();
    s.remove_curve(l1).unwrap();
    assert_eq!(s.curves.len(), 1);
    assert!(s.point(b).is_some(), "shared point survives");
    assert!(s.constraints.is_empty());
    assert_eq!(s.points.len(), 3); // origin + l2's two points
  }

  #[test]
  fn constraint_validation() {
    let mut s = Sketch::new();
    let c = s.add_circle(PtSpec::At(0.0, 0.0), 5.0).unwrap();
    assert!(s.add_constraint(ConstraintKind::Horizontal { line: c }).is_err());
    assert!(s.add_constraint(ConstraintKind::Radius { curve: c, value: Dim::new(3.0) }).is_ok());
    assert!(s.add_constraint(ConstraintKind::Horizontal { line: CurveId(999) }).is_err());
  }

  #[test]
  fn serde_round_trip() {
    let mut s = Sketch::new();
    s.add_rectangle([0.0, 0.0], [20.0, 10.0]).unwrap();
    let json = serde_json::to_string(&s).unwrap();
    let back: Sketch = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
  }
}
