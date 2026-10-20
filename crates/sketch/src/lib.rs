//! 2D parametric sketches: points, lines, circles and arcs held in place by geometric constraints
//! and driving dimensions, solved numerically, with closed profiles found for 3D features.
//!
//! Every sketch carries a fixed origin point ([`ORIGIN`]) so geometry can be dimensioned from it
//! and become fully constrained, as in Fusion.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]
// `!(x > 0.0)` is deliberate: it rejects NaN as well as non-positive values.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

mod build;
mod conic;
mod curve3d;
mod edit;
mod ellipse;
mod fitspline;
mod fonts;
mod freeform;
mod profile;
mod quintic;
mod solve;
mod spun;
mod tangent;
mod text;
mod transform;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use build::{PtSpec, circumcenter};
pub use conic::{RHO_RANGE, conic_at, conic_beziers, conic_weight};
pub use curve3d::{Curve3d, Curve3dKind, CurveSource};
pub use edit::PatternDir;
pub use ellipse::{Ellipse2, ellipse_arc_beziers};
pub use fonts::{font_dirs, system_fonts};
pub use profile::{Loop, Profile, Seg, find_profiles};
pub use quintic::{QUINTIC_POINTS, quintic_at, quintic_beziers, quintic_spans};
pub use solve::{SolveReport, SolveStatus};
pub use spun::{SketchSpun, SpunAxis, SpunId, SpunSeg};
pub use text::{HAlign, SketchText, TextAlign, TextId, TextPath, TextStyle, VAlign, text_fonts};
pub use transform::{TransformReport, Xform2};

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
  /// Editable text: each keeps its string, font and placement and owns the glyph curves it
  /// generates (sketches from before text was editable have none, only exploded curves).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub texts: Vec<SketchText>,
  /// Spun profiles: outlines of bodies turned about an axis, kept up to date by the model.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub spuns: Vec<SketchSpun>,
  /// Included 3D geometry (Include 3D Geometry): curves kept at their world positions, off the
  /// sketch plane, usable as sweep paths and rails but never part of a profile.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub curves3d: Vec<Curve3d>,
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
  /// A smooth curve through fit points (Catmull-Rom tangents, cubic pieces).
  Spline {
    points: Vec<PointId>,
    /// Tangent handle at the start: the first piece's inner control point. Added when a tangency
    /// or curvature constraint uses that end, so the fit points stay free.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    start_handle: Option<PointId>,
    /// Tangent handle at the end: the last piece's inner control point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    end_handle: Option<PointId>,
  },
  /// A cubic B-spline shaped by control points (it passes through the first and last).
  ControlSpline {
    points: Vec<PointId>,
  },
  /// An ellipse: centre, the end of its major axis, and the minor radius.
  Ellipse {
    center: PointId,
    major: PointId,
    minor: f64,
  },
  /// A chain of cubic Bézier pieces: points 0, 3, 6, … lie on the curve, the two between each
  /// pair are its handles. What trimming, breaking, extending or offsetting a spline or an
  /// ellipse leaves.
  Bezier {
    points: Vec<PointId>,
  },
  /// An arc of an ellipse (centre, end of the major axis and minor radius as for `Ellipse`),
  /// counter-clockwise from `start` to `end`, both on the ellipse. What trimming or breaking an
  /// ellipse leaves.
  EllipticalArc {
    center: PointId,
    major: PointId,
    minor: f64,
    start: PointId,
    end: PointId,
  },
  /// A conic arc from `start` to `end` whose end tangents meet at `apex`, its shape set by `rho`
  /// (below 0.5 elliptical, 0.5 a parabola, above 0.5 hyperbolic): the rational quadratic Bézier
  /// with control points start, apex, end and apex weight rho / (1 - rho).
  Conic {
    start: PointId,
    end: PointId,
    apex: PointId,
    rho: f64,
  },
  /// A clamped quintic B-spline through its first and last of ten control points, its five
  /// spans even (knots 0 ×6, 0.2, 0.4, 0.6, 0.8, 1 ×6): Fusion's fitted spline through two
  /// points, as Blend Curve makes.
  QuinticSpline {
    points: Vec<PointId>,
  },
}

impl CurveKind {
  /// A fit-point spline without tangent handles.
  pub fn spline(points: Vec<PointId>) -> CurveKind {
    CurveKind::Spline { points, start_handle: None, end_handle: None }
  }

  /// Splines, ellipses and Bézier chains: curves drawn as Bézier pieces.
  pub fn is_freeform(&self) -> bool {
    matches!(
      self,
      CurveKind::Spline { .. }
        | CurveKind::ControlSpline { .. }
        | CurveKind::Ellipse { .. }
        | CurveKind::Bezier { .. }
        | CurveKind::EllipticalArc { .. }
        | CurveKind::Conic { .. }
        | CurveKind::QuinticSpline { .. }
    )
  }

  /// Ellipses and elliptical arcs.
  pub fn is_elliptic(&self) -> bool {
    matches!(self, CurveKind::Ellipse { .. } | CurveKind::EllipticalArc { .. })
  }
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
    Dim { expr: fox_expr::trim_number(value), value }
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
  /// Locks a circle's radius: fixing a circle fixes its centre and adds this.
  FixRadius {
    curve: CurveId,
    radius: f64,
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
  /// Two lines at 90°, or a line normal to a circle/arc (the extended line runs through its centre).
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
  /// `p` at the midpoint of `line`: a line, or an arc (the middle of its counter-clockwise
  /// sweep).
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
  /// Curves `a` and `b` mirror each other across `line`: two lines as infinite lines (their
  /// endpoints stay free), or two circles/arcs by their centres, with equal radii.
  SymmetricCurves {
    a: CurveId,
    b: CurveId,
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
  /// Curvature continuity (G2) where two curves meet, one of them a spline: tangent there, with
  /// equal curvature.
  Curvature {
    a: CurveId,
    b: CurveId,
  },
  /// A closed loop of lines kept a regular polygon about `center`: every side the same length and
  /// every corner the same distance from the centre. The centre is a point of its own (no curve
  /// uses it), so other constraints and dimensions can refer to it.
  Polygon {
    center: PointId,
    lines: Vec<CurveId>,
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
      ConstraintKind::Fix { .. } | ConstraintKind::FixRadius { .. } => "Fix",
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
      ConstraintKind::Symmetric { .. } | ConstraintKind::SymmetricCurves { .. } => "Symmetric",
      ConstraintKind::Distance { .. } => "Distance",
      ConstraintKind::HorizontalDistance { .. } => "Horizontal distance",
      ConstraintKind::VerticalDistance { .. } => "Vertical distance",
      ConstraintKind::PointLineDistance { .. } => "Point-line distance",
      ConstraintKind::Radius { .. } => "Radius",
      ConstraintKind::Diameter { .. } => "Diameter",
      ConstraintKind::Angle { .. } => "Angle",
      ConstraintKind::Curvature { .. } => "Curvature",
      ConstraintKind::Polygon { .. } => "Polygon",
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
      ConstraintKind::Polygon { center, .. } => vec![*center],
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
      | ConstraintKind::Curvature { a, b }
      | ConstraintKind::Angle { a, b, .. } => vec![*a, *b],
      ConstraintKind::SymmetricCurves { a, b, line } => vec![*a, *b, *line],
      ConstraintKind::PointOnCurve { curve, .. }
      | ConstraintKind::FixRadius { curve, .. }
      | ConstraintKind::Radius { curve, .. }
      | ConstraintKind::Diameter { curve, .. } => vec![*curve],
      ConstraintKind::Midpoint { line, .. }
      | ConstraintKind::Symmetric { line, .. }
      | ConstraintKind::PointLineDistance { line, .. } => vec![*line],
      ConstraintKind::Polygon { lines, .. } => lines.clone(),
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
      texts: Vec::new(),
      spuns: Vec::new(),
      curves3d: Vec::new(),
    }
  }

  pub(crate) fn alloc(&mut self) -> u32 {
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
    if let CurveKind::Spline { points, .. } | CurveKind::ControlSpline { points } = &kind
      && (points.len() < 2 || points.len() > 1000)
    {
      return Err(SketchError::InvalidValue("a spline needs 2 to 1000 points".into()));
    }
    if let CurveKind::QuinticSpline { points } = &kind
      && points.len() != QUINTIC_POINTS
    {
      return Err(SketchError::InvalidValue(format!("a quintic spline needs {QUINTIC_POINTS} control points")));
    }
    if let CurveKind::Bezier { points } = &kind
      && (points.len() < 4 || points.len() > 3001 || points.len() % 3 != 1)
    {
      return Err(SketchError::InvalidValue("a Bézier chain needs 3n + 1 points (n = 1..1000)".into()));
    }
    if let CurveKind::Ellipse { center, major, minor } | CurveKind::EllipticalArc { center, major, minor, .. } = kind {
      let (c, m) = (self.pos(center), self.pos(major));
      let a = c.zip(m).map(|(c, m)| dist(c, m)).unwrap_or(0.0);
      if !(minor.is_finite() && minor > 0.0 && a > 0.0) {
        return Err(SketchError::InvalidValue(format!("ellipse radii {a} and {minor}")));
      }
    }
    if let CurveKind::Circle { radius, .. } = kind
      && !(radius.is_finite() && radius > 0.0)
    {
      return Err(SketchError::InvalidValue(format!("circle radius {radius}")));
    }
    if let CurveKind::Conic { rho, .. } = kind
      && !RHO_RANGE.contains(&rho)
    {
      return Err(SketchError::InvalidValue(format!("conic rho {rho} (it must be 0.01 to 0.99)")));
    }
    let id = CurveId(self.alloc());
    self.curves.push(Curve { id, kind, construction: false });
    Ok(id)
  }

  /// Adds a constraint after checking it refers to geometry of the right type.
  pub fn add_constraint(&mut self, kind: ConstraintKind) -> Result<ConstraintId, SketchError> {
    self.validate(&kind)?;
    if let ConstraintKind::Tangent { a, b } | ConstraintKind::Curvature { a, b } = kind {
      self.add_spline_handle(a, b);
      self.add_spline_handle(b, a);
    }
    let id = ConstraintId(self.alloc());
    self.constraints.push(Constraint { id, kind });
    Ok(id)
  }

  /// Locks a curve's size and location (Fusion's Fix on a curve): every point that defines it
  /// gets a `Fix` and a circle's radius a `FixRadius`. Points already fixed are skipped. Returns
  /// the constraints added; deleting them unfixes the curve.
  pub fn fix_curve(&mut self, curve: CurveId) -> Result<Vec<ConstraintId>, SketchError> {
    let kind = self.curve(curve).ok_or(SketchError::NoCurve(curve))?.kind.clone();
    let mut added = Vec::new();
    for p in curve_points(&kind) {
      let Some(pt) = self.point(p).copied() else { continue };
      let pinned =
        pt.fixed || self.constraints.iter().any(|c| matches!(c.kind, ConstraintKind::Fix { p: q, .. } if q == p));
      if !pinned {
        added.push(self.add_constraint(ConstraintKind::Fix { p, x: pt.x, y: pt.y })?);
      }
    }
    if let CurveKind::Circle { radius, .. } = kind
      && !self.constraints.iter().any(|c| matches!(c.kind, ConstraintKind::FixRadius { curve: q, .. } if q == curve))
    {
      added.push(self.add_constraint(ConstraintKind::FixRadius { curve, radius })?);
    }
    Ok(added)
  }

  pub fn remove_constraint(&mut self, id: ConstraintId) -> Result<(), SketchError> {
    let removed = self.constraint(id).ok_or(SketchError::NoConstraint(id))?.clone();
    self.constraints.retain(|c| c.id != id);
    self.remove_polygon_centers(&[removed]);
    Ok(())
  }

  /// Deletes the centre points of removed polygon constraints, with the constraints on them,
  /// unless a curve or another polygon constraint still uses the point.
  fn remove_polygon_centers(&mut self, removed: &[Constraint]) {
    for c in removed {
      let ConstraintKind::Polygon { center, .. } = c.kind else { continue };
      let used = center == ORIGIN
        || self.curves.iter().any(|c| curve_points(&c.kind).contains(&center))
        || self.constraints.iter().any(|c| matches!(c.kind, ConstraintKind::Polygon { center: o, .. } if o == center));
      if !used {
        self.points.retain(|p| p.id != center);
        self.constraints.retain(|c| !c.kind.points().contains(&center));
      }
    }
  }

  /// Deletes a curve, its points that nothing else uses, and constraints that referred to them.
  /// A text's glyph curve deletes the whole text, as does the curve a text runs along.
  pub fn remove_curve(&mut self, id: CurveId) -> Result<(), SketchError> {
    if let Some(t) = self.text_of(id) {
      return self.remove_text(t);
    }
    if let Some(sp) = self.spun_of(id) {
      return self.remove_spun(sp);
    }
    if self.curve3d(id).is_some() {
      self.curves3d.retain(|c| c.id != id);
      return Ok(());
    }
    let curve = self.curve(id).ok_or(SketchError::NoCurve(id))?.clone();
    let along: Vec<TextId> =
      self.texts.iter().filter(|t| t.path.is_some_and(|p| p.curve == id)).map(|t| t.id).collect();
    for t in along {
      self.remove_text(t)?;
    }
    self.curves.retain(|c| c.id != id);
    let still_used: Vec<PointId> = self.curves.iter().flat_map(|c| curve_points(&c.kind)).collect();
    let orphans: Vec<PointId> =
      curve_points(&curve.kind).into_iter().filter(|p| *p != ORIGIN && !still_used.contains(p)).collect();
    self.points.retain(|p| !orphans.contains(&p.id));
    let (removed, kept): (Vec<Constraint>, Vec<Constraint>) = std::mem::take(&mut self.constraints)
      .into_iter()
      .partition(|c| c.kind.curves().contains(&id) || c.kind.points().iter().any(|p| orphans.contains(p)));
    self.constraints = kept;
    self.remove_polygon_centers(&removed);
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
    if let Some(t) = self.texts.iter().find(|t| t.anchor == id || t.points.contains(&id)).map(|t| t.id) {
      return self.remove_text(t);
    }
    if let Some(sp) = self.spuns.iter().find(|s| s.points.contains(&id)).map(|s| s.id) {
      return self.remove_spun(sp);
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
      CurveKind::Line { .. }
      | CurveKind::Spline { .. }
      | CurveKind::ControlSpline { .. }
      | CurveKind::Ellipse { .. }
      | CurveKind::Bezier { .. }
      | CurveKind::EllipticalArc { .. }
      | CurveKind::Conic { .. }
      | CurveKind::QuinticSpline { .. } => None,
    }
  }

  /// A spline or ellipse as cubic Bézier pieces (empty for lines, circles and arcs).
  pub fn beziers(&self, id: CurveId) -> Vec<[[f64; 2]; 4]> {
    self.curve(id).map(|c| self.kind_beziers(&c.kind)).unwrap_or_default()
  }

  /// [`Sketch::beziers`] of a curve kind at the current point positions.
  pub fn kind_beziers(&self, kind: &CurveKind) -> Vec<[[f64; 2]; 4]> {
    kind_beziers(kind, &|p| self.pos(p))
  }

  /// A spline or ellipse flattened to a polyline (empty for other curves).
  pub fn spline_points(&self, id: CurveId) -> Vec<[f64; 2]> {
    let bz = self.beziers(id);
    let mut out = Vec::new();
    for b in &bz {
      for k in 0..16 {
        out.push(bezier_at(b, k as f64 / 16.0));
      }
    }
    if let Some(l) = bz.last() {
      out.push(l[3]);
    }
    out
  }

  /// A curve as profile segments (sketch coordinates): lines, arcs, circles, ellipses and
  /// elliptical arcs exactly (a whole ellipse as two halves), splines as Bézier pieces.
  pub fn curve_segs(&self, id: CurveId) -> Vec<Seg> {
    let Some(c) = self.curve(id) else { return Vec::new() };
    match &c.kind {
      CurveKind::Line { a, b } => match (self.pos(*a), self.pos(*b)) {
        (Some(a), Some(b)) if dist(a, b) > 1e-9 => vec![Seg::Line { a, b }],
        _ => Vec::new(),
      },
      CurveKind::Circle { center, radius } => {
        self.pos(*center).map(|c| vec![Seg::Circle { center: c, radius: *radius, ccw: true }]).unwrap_or_default()
      }
      CurveKind::Arc { center, start, end } => match (self.pos(*center), self.pos(*start), self.pos(*end)) {
        (Some(c), Some(s), Some(e)) => {
          let a0 = angle_of(c, s);
          vec![Seg::Arc { center: c, radius: dist(c, s), start_angle: a0, sweep: ccw_sweep(a0, angle_of(c, e)) }]
        }
        _ => Vec::new(),
      },
      k if k.is_elliptic() => ellipse::kind_segs(k, &|p| self.pos(p)),
      CurveKind::Conic { start, end, apex, rho } => match (self.pos(*start), self.pos(*apex), self.pos(*end)) {
        (Some(s), Some(a), Some(e)) if dist(s, e) > 1e-9 => vec![Seg::Conic { p: [s, a, e], w: conic_weight(*rho) }],
        _ => Vec::new(),
      },
      CurveKind::QuinticSpline { points } => match quintic::ctrl_of(points, &|p| self.pos(p)) {
        Some(p) if dist(p[0], p[QUINTIC_POINTS - 1]) > 1e-9 => vec![Seg::Quintic { p }],
        _ => Vec::new(),
      },
      _ => self.beziers(id).into_iter().map(|p| Seg::Bezier { p }).collect(),
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
        CurveKind::Spline { .. }
        | CurveKind::ControlSpline { .. }
        | CurveKind::Bezier { .. }
        | CurveKind::QuinticSpline { .. } => {
          for p in curve_points(&c.kind) {
            if let Some(p) = self.pos(p) {
              add(p);
            }
          }
        }
        CurveKind::Ellipse { .. } | CurveKind::EllipticalArc { .. } | CurveKind::Conic { .. } => {
          for p in self.spline_points(c.id) {
            add(p);
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
      CurveKind::Spline { .. }
      | CurveKind::ControlSpline { .. }
      | CurveKind::Ellipse { .. }
      | CurveKind::Bezier { .. }
      | CurveKind::EllipticalArc { .. }
      | CurveKind::Conic { .. }
      | CurveKind::QuinticSpline { .. } => {
        let path = self.beziers(id);
        if path.is_empty() {
          return None;
        }
        dist(freeform::path_at(&path, freeform::path_param(&path, at)), at)
      }
    })
  }

  /// Evaluates every dimension expression with `scope`, storing values. Returns the ids and
  /// errors of dimensions that failed to evaluate (they keep their previous value).
  pub fn evaluate_dims(&mut self, scope: &dyn fox_expr::Scope) -> Vec<(ConstraintId, fox_expr::ExprError)> {
    let mut errors = Vec::new();
    for c in &mut self.constraints {
      let kind = if c.kind.is_angle() { fox_expr::Kind::Angle } else { fox_expr::Kind::Length };
      let id = c.id;
      if let Some(d) = c.kind.dim_mut() {
        match fox_expr::eval(&d.expr, kind, scope) {
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
    let any = |id: CurveId| -> Result<(), SketchError> {
      match self.curve(id).map(|c| &c.kind) {
        Some(k) if k.is_freeform() => {
          Err(SketchError::WrongGeometry("this constraint", "lines, circles or arcs (constrain a spline's points)"))
        }
        Some(_) => Ok(()),
        None => Err(SketchError::NoCurve(id)),
      }
    };
    let exists = |id: CurveId| self.curve(id).map(|_| ()).ok_or(SketchError::NoCurve(id));
    let free = |id: CurveId| self.curve(id).is_some_and(|c| c.kind.is_freeform());
    let ellipse = |id: CurveId| matches!(self.curve(id).map(|c| &c.kind), Some(CurveKind::Ellipse { .. }));
    match kind {
      ConstraintKind::Horizontal { line: l } => line(*l, "Horizontal"),
      ConstraintKind::Vertical { line: l } => line(*l, "Vertical"),
      ConstraintKind::Parallel { a, b } => line(*a, "Parallel").and(line(*b, "Parallel")),
      ConstraintKind::Perpendicular { a, b } => {
        // a line and a circle/arc: the line is normal to the curve, i.e. runs through its centre
        let is_round =
          |id| matches!(self.curve(id).map(|c| &c.kind), Some(CurveKind::Circle { .. } | CurveKind::Arc { .. }));
        if is_round(*a) || is_round(*b) {
          let l = if is_round(*a) { *b } else { *a };
          line(l, "Perpendicular").map_err(|e| match e {
            SketchError::WrongGeometry(..) => {
              SketchError::WrongGeometry("Perpendicular", "two lines, or a line and a circle or arc")
            }
            e => e,
          })
        } else {
          line(*a, "Perpendicular").and(line(*b, "Perpendicular"))
        }
      }
      ConstraintKind::Collinear { a, b } => line(*a, "Collinear").and(line(*b, "Collinear")),
      ConstraintKind::Angle { a, b, .. } => line(*a, "Angle").and(line(*b, "Angle")),
      ConstraintKind::Midpoint { line: l, .. } => match self.curve(*l).map(|c| &c.kind) {
        Some(CurveKind::Line { .. } | CurveKind::Arc { .. }) => Ok(()),
        Some(_) => Err(SketchError::WrongGeometry("Midpoint", "a line or an arc")),
        None => Err(SketchError::NoCurve(*l)),
      },
      ConstraintKind::Symmetric { line: l, .. } => line(*l, "Symmetric"),
      ConstraintKind::SymmetricCurves { a, b, line: l } => {
        line(*l, "Symmetric")?;
        any(*a)?;
        any(*b)?;
        let is_line = |id| matches!(self.curve(id).map(|c| &c.kind), Some(CurveKind::Line { .. }));
        let is_round =
          |id| matches!(self.curve(id).map(|c| &c.kind), Some(CurveKind::Circle { .. } | CurveKind::Arc { .. }));
        let pair = (is_line(*a) && is_line(*b)) || (is_round(*a) && is_round(*b));
        if a == b || a == l || b == l || !pair {
          Err(SketchError::WrongGeometry("Symmetric", "two points, two lines or two circles/arcs, and an axis line"))
        } else {
          Ok(())
        }
      }
      ConstraintKind::PointLineDistance { line: l, .. } => line(*l, "Point-line distance"),
      ConstraintKind::Concentric { a, b } => round(*a, "Concentric").and(round(*b, "Concentric")),
      ConstraintKind::Radius { curve, .. } => round(*curve, "Radius"),
      ConstraintKind::FixRadius { curve, radius } => {
        round(*curve, "Fix")?;
        if radius.is_finite() && *radius > 0.0 {
          Ok(())
        } else {
          Err(SketchError::InvalidValue(format!("radius {radius}")))
        }
      }
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
      ConstraintKind::Tangent { a, b } | ConstraintKind::Curvature { a, b } if free(*a) || free(*b) => {
        exists(*a)?;
        exists(*b)?;
        if a == b || ellipse(*a) || ellipse(*b) {
          Err(SketchError::WrongGeometry(kind.name(), "a spline and a curve that meets one of its ends"))
        } else {
          Ok(())
        }
      }
      ConstraintKind::Curvature { .. } => Err(SketchError::WrongGeometry("Curvature", "a spline")),
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
      ConstraintKind::PointOnCurve { curve, .. } => exists(*curve),
      ConstraintKind::Polygon { center, lines } => {
        for l in lines {
          line(*l, "Polygon")?;
        }
        match self.polygon_corners(lines) {
          Some(corners) if !corners.contains(center) => Ok(()),
          _ => Err(SketchError::WrongGeometry("Polygon", "three or more lines forming one closed loop")),
        }
      }
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
    CurveKind::Spline { points, start_handle, end_handle } => {
      points.iter().copied().chain(*start_handle).chain(*end_handle).collect()
    }
    CurveKind::ControlSpline { points } | CurveKind::Bezier { points } | CurveKind::QuinticSpline { points } => {
      points.clone()
    }
    CurveKind::Ellipse { center, major, .. } => vec![*center, *major],
    CurveKind::EllipticalArc { center, major, start, end, .. } => vec![*center, *major, *start, *end],
    CurveKind::Conic { start, end, apex, .. } => vec![*start, *apex, *end],
  }
}

/// A curve's Bézier pieces with point positions from `pos` (empty for lines, circles and arcs).
pub fn kind_beziers(kind: &CurveKind, pos: &dyn Fn(PointId) -> Option<[f64; 2]>) -> Vec<[[f64; 2]; 4]> {
  let pts = |ids: &[PointId]| -> Vec<[f64; 2]> { ids.iter().filter_map(|p| pos(*p)).collect() };
  match kind {
    CurveKind::Spline { points, start_handle, end_handle } => {
      let mut bz = spline_beziers(&pts(points));
      if let (Some(h), Some(first)) = (start_handle.and_then(pos), bz.first_mut()) {
        first[1] = h;
      }
      if let (Some(h), Some(last)) = (end_handle.and_then(pos), bz.last_mut()) {
        last[2] = h;
      }
      bz
    }
    CurveKind::ControlSpline { points } => bspline_beziers(&pts(points)),
    CurveKind::Bezier { points } => {
      let p = pts(points);
      if p.len() < 4 || p.len() % 3 != 1 {
        return Vec::new();
      }
      p.windows(4).step_by(3).map(|w| [w[0], w[1], w[2], w[3]]).collect()
    }
    CurveKind::Ellipse { center, major, minor } => match (pos(*center), pos(*major)) {
      (Some(c), Some(m)) => ellipse_beziers(c, m, *minor),
      _ => Vec::new(),
    },
    CurveKind::EllipticalArc { .. } => match ellipse::arc_of(kind, pos) {
      Some((e, t0, sweep)) => e.beziers(t0, sweep, std::f64::consts::FRAC_PI_4),
      None => Vec::new(),
    },
    CurveKind::Conic { start, end, apex, rho } => match (pos(*start), pos(*apex), pos(*end)) {
      (Some(s), Some(a), Some(e)) => conic_beziers(&[s, a, e], conic_weight(*rho)),
      _ => Vec::new(),
    },
    CurveKind::QuinticSpline { points } => {
      quintic::ctrl_of(points, pos).map(|p| quintic_beziers(&p)).unwrap_or_default()
    }
    _ => Vec::new(),
  }
}

/// Cubic Bézier pieces of a clamped uniform cubic B-spline with these control points (fewer
/// than four points make a line or a quadratic).
pub fn bspline_beziers(ctrl: &[[f64; 2]]) -> Vec<[[f64; 2]; 4]> {
  let lerp = |a: [f64; 2], b: [f64; 2], t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
  match ctrl.len() {
    0 | 1 => return Vec::new(),
    2 => return vec![[ctrl[0], lerp(ctrl[0], ctrl[1], 1.0 / 3.0), lerp(ctrl[0], ctrl[1], 2.0 / 3.0), ctrl[1]]],
    3 => return vec![[ctrl[0], lerp(ctrl[0], ctrl[1], 2.0 / 3.0), lerp(ctrl[2], ctrl[1], 2.0 / 3.0), ctrl[2]]],
    _ => {}
  }
  // knots 0,0,0,0,1,…,m,m,m,m; insert each interior knot until it has multiplicity 3 (Boehm)
  let p = 3;
  let m = ctrl.len() - 3;
  let mut knots: Vec<f64> =
    std::iter::repeat_n(0.0, 4).chain((1..m).map(|k| k as f64)).chain(std::iter::repeat_n(m as f64, 4)).collect();
  let mut pts = ctrl.to_vec();
  for u in 1..m {
    let u = u as f64;
    for _ in 0..2 {
      let Some(k) = (0..knots.len() - 1).rev().find(|&i| knots[i] <= u && u < knots[i + 1]) else { break };
      let s = knots.iter().filter(|&&x| x == u).count();
      let mut q = Vec::with_capacity(pts.len() + 1);
      for i in 0..=pts.len() {
        if i + p <= k {
          q.push(pts[i]);
        } else if i + s <= k {
          let a = (u - knots[i]) / (knots[i + p] - knots[i]);
          q.push(lerp(pts[i - 1], pts[i], a));
        } else {
          q.push(pts[i - 1]);
        }
      }
      knots.insert(k + 1, u);
      pts = q;
    }
  }
  pts.windows(4).step_by(3).map(|w| [w[0], w[1], w[2], w[3]]).collect()
}

/// An ellipse as eight cubic Bézier pieces (about 4e-6 of the radius from the true curve).
pub fn ellipse_beziers(center: [f64; 2], major: [f64; 2], minor: f64) -> Vec<[[f64; 2]; 4]> {
  let (dx, dy) = (major[0] - center[0], major[1] - center[1]);
  let a = dx.hypot(dy);
  if a <= 0.0 {
    return Vec::new();
  }
  let (c, s) = (dx / a, dy / a);
  let map = |x: f64, y: f64| [center[0] + a * x * c - minor * y * s, center[1] + a * x * s + minor * y * c];
  let n = 8;
  let step = std::f64::consts::TAU / n as f64;
  let k = 4.0 / 3.0 * (step / 4.0).tan();
  (0..n)
    .map(|i| {
      let (t0, t1) = (step * i as f64, step * (i + 1) as f64);
      let (p0, p3) = ((t0.cos(), t0.sin()), (t1.cos(), t1.sin()));
      [map(p0.0, p0.1), map(p0.0 - k * p0.1, p0.1 + k * p0.0), map(p3.0 + k * p3.1, p3.1 - k * p3.0), map(p3.0, p3.1)]
    })
    .collect()
}

/// Cubic Bézier pieces (control points) of a spline through `pts`: Catmull-Rom tangents inside,
/// natural (half-chord) tangents at the ends.
pub fn spline_beziers(pts: &[[f64; 2]]) -> Vec<[[f64; 2]; 4]> {
  let n = pts.len();
  if n < 2 {
    return Vec::new();
  }
  let tangent = |i: usize| -> [f64; 2] {
    let (a, b) = if i == 0 {
      (pts[0], pts[1])
    } else if i == n - 1 {
      (pts[n - 2], pts[n - 1])
    } else {
      (pts[i - 1], pts[i + 1])
    };
    let k = if i == 0 || i == n - 1 { 1.0 } else { 0.5 };
    [(b[0] - a[0]) * k, (b[1] - a[1]) * k]
  };
  (0..n - 1)
    .map(|i| {
      let (p0, p3) = (pts[i], pts[i + 1]);
      let (t0, t1) = (tangent(i), tangent(i + 1));
      [p0, [p0[0] + t0[0] / 3.0, p0[1] + t0[1] / 3.0], [p3[0] - t1[0] / 3.0, p3[1] - t1[1] / 3.0], p3]
    })
    .collect()
}

/// A point on a cubic Bézier.
pub fn bezier_at(b: &[[f64; 2]; 4], t: f64) -> [f64; 2] {
  let u = 1.0 - t;
  let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
  [
    w[0] * b[0][0] + w[1] * b[1][0] + w[2] * b[2][0] + w[3] * b[3][0],
    w[0] * b[0][1] + w[1] * b[1][1] + w[2] * b[2][1] + w[3] * b[3][1],
  ]
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
  fn bspline_is_clamped_and_continuous() {
    let ctrl = [[0.0, 0.0], [10.0, 10.0], [20.0, -10.0], [30.0, 10.0], [40.0, 0.0], [50.0, 5.0]];
    let bz = bspline_beziers(&ctrl);
    assert_eq!(bz.len(), 3);
    assert_eq!(bz[0][0], ctrl[0]);
    assert_eq!(bz[2][3], ctrl[5]);
    for w in bz.windows(2) {
      assert!(dist(w[0][3], w[1][0]) < 1e-12);
      // C1 at the joints: the control legs line up
      let (a, b) =
        ([w[0][3][0] - w[0][2][0], w[0][3][1] - w[0][2][1]], [w[1][1][0] - w[1][0][0], w[1][1][1] - w[1][0][1]]);
      assert!((a[0] * b[1] - a[1] * b[0]).abs() < 1e-9);
    }
  }

  #[test]
  fn ellipse_pieces_lie_on_the_ellipse() {
    let bz = ellipse_beziers([5.0, 5.0], [25.0, 5.0], 10.0);
    assert_eq!(bz.len(), 8);
    for b in &bz {
      for k in 0..=10 {
        let p = bezier_at(b, k as f64 / 10.0);
        let v = ((p[0] - 5.0) / 20.0).powi(2) + ((p[1] - 5.0) / 10.0).powi(2);
        assert!((v - 1.0).abs() < 1e-4, "{v}");
      }
    }
    assert!(dist(bz[7][3], bz[0][0]) < 1e-12, "closed");
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
