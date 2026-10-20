//! Moving, rotating and scaling sketch geometry, and copying curves between sketches.

use std::collections::HashMap;

use crate::{ConstraintId, ConstraintKind, CurveId, CurveKind, Dim, PointId, Sketch, SketchError, curve_points};

/// Scale and rotate about `center`, then translate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Xform2 {
  pub scale: f64,
  /// Radians, counter-clockwise.
  pub angle: f64,
  pub center: [f64; 2],
  pub translate: [f64; 2],
}

impl Default for Xform2 {
  fn default() -> Self {
    Xform2 { scale: 1.0, angle: 0.0, center: [0.0, 0.0], translate: [0.0, 0.0] }
  }
}

impl Xform2 {
  pub fn apply(&self, p: [f64; 2]) -> [f64; 2] {
    let (s, c) = self.angle.sin_cos();
    let (dx, dy) = ((p[0] - self.center[0]) * self.scale, (p[1] - self.center[1]) * self.scale);
    [self.center[0] + dx * c - dy * s + self.translate[0], self.center[1] + dx * s + dy * c + self.translate[1]]
  }

  fn check(&self) -> Result<(), SketchError> {
    let finite = [self.scale, self.angle, self.center[0], self.center[1], self.translate[0], self.translate[1]];
    if !finite.iter().all(|v| v.is_finite()) || !(self.scale > 1e-9) {
      return Err(SketchError::InvalidValue("the scale must be a positive number".into()));
    }
    Ok(())
  }

  /// Whether horizontal/vertical directions survive (no rotation, or a half turn).
  fn keeps_axes(&self) -> bool {
    let half_turns = self.angle / std::f64::consts::PI;
    (half_turns - half_turns.round()).abs() < 1e-9
  }
}

/// What [`Sketch::transform`] had to change besides positions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransformReport {
  /// Horizontal/vertical constraints dropped because the geometry was rotated.
  pub removed: Vec<ConstraintId>,
  /// Dimensions whose expression was replaced by the scaled number.
  pub unlinked: Vec<ConstraintId>,
}

impl Sketch {
  /// Moves the points of `curves` by `t`, scaling circle and ellipse radii and the dimensions measured
  /// entirely within the moved geometry. Fixed points (the origin, projected geometry) stay.
  /// Constraints to geometry that didn't move are kept; solve afterwards.
  pub fn transform(&mut self, curves: &[CurveId], t: &Xform2) -> Result<TransformReport, SketchError> {
    t.check()?;
    let mut moved: Vec<PointId> = Vec::new();
    for c in curves {
      let curve = self.curve(*c).ok_or(SketchError::NoCurve(*c))?;
      for p in curve_points(&curve.kind) {
        if !moved.contains(&p) && self.point(p).is_some_and(|q| !q.fixed) {
          moved.push(p);
        }
      }
    }
    for p in self.points.iter_mut().filter(|p| moved.contains(&p.id)) {
      [p.x, p.y] = t.apply([p.x, p.y]);
    }
    for c in self.curves.iter_mut().filter(|c| curves.contains(&c.id)) {
      match &mut c.kind {
        CurveKind::Circle { radius, .. } => *radius *= t.scale,
        CurveKind::Ellipse { minor, .. } | CurveKind::EllipticalArc { minor, .. } => *minor *= t.scale,
        _ => {}
      }
    }
    let line_moved =
      |sk: &Sketch, l: CurveId| curves.contains(&l) && sk.curve_points(l).iter().all(|p| moved.contains(p));
    let mut report = TransformReport::default();
    let mut scaled: Vec<ConstraintId> = Vec::new();
    for con in &self.constraints {
      let inside = match &con.kind {
        ConstraintKind::Distance { a, b, .. }
        | ConstraintKind::HorizontalDistance { a, b, .. }
        | ConstraintKind::VerticalDistance { a, b, .. } => moved.contains(a) && moved.contains(b),
        ConstraintKind::PointLineDistance { p, line, .. } => moved.contains(p) && line_moved(self, *line),
        ConstraintKind::Radius { curve, .. } | ConstraintKind::Diameter { curve, .. } => curves.contains(curve),
        _ => false,
      };
      if inside {
        scaled.push(con.id);
      }
      if !t.keeps_axes() {
        let axis = match &con.kind {
          ConstraintKind::Horizontal { line } | ConstraintKind::Vertical { line } => line_moved(self, *line),
          ConstraintKind::HorizontalPoints { a, b }
          | ConstraintKind::VerticalPoints { a, b }
          | ConstraintKind::HorizontalDistance { a, b, .. }
          | ConstraintKind::VerticalDistance { a, b, .. } => moved.contains(a) && moved.contains(b),
          _ => false,
        };
        if axis {
          report.removed.push(con.id);
        }
      }
    }
    self.constraints.retain(|c| !report.removed.contains(&c.id));
    self.transform_texts(curves, t, &moved)?;
    for con in self.constraints.iter_mut() {
      if let ConstraintKind::Fix { p, x, y } = &mut con.kind
        && moved.contains(p)
      {
        [*x, *y] = t.apply([*x, *y]);
      }
      if let ConstraintKind::FixRadius { curve, radius } = &mut con.kind
        && curves.contains(curve)
      {
        *radius *= t.scale;
      }
      if (t.scale - 1.0).abs() > 1e-12
        && scaled.contains(&con.id)
        && let Some(d) = con.kind.dim_mut()
      {
        let numeric = d.expr.trim().parse::<f64>().is_ok();
        *d = Dim::new(d.value * t.scale);
        if !numeric {
          report.unlinked.push(con.id);
        }
      }
    }
    Ok(report)
  }

  /// Copies `curves` of `src` (all of them if empty) into this sketch, transformed by `t`.
  /// Points shared between the copied curves stay shared; construction flags are kept, and
  /// constraints are not copied. Returns the new curves, in the order of `src`.
  pub fn insert_from(&mut self, src: &Sketch, curves: &[CurveId], t: &Xform2) -> Result<Vec<CurveId>, SketchError> {
    t.check()?;
    for c in curves {
      src.curve(*c).ok_or(SketchError::NoCurve(*c))?;
    }
    let mut map: HashMap<PointId, PointId> = HashMap::new();
    let mut out = Vec::new();
    for c in src.curves.iter().filter(|c| curves.is_empty() || curves.contains(&c.id)) {
      let mut pt = |sk: &mut Sketch, p: PointId| -> Result<PointId, SketchError> {
        if let Some(q) = map.get(&p) {
          return Ok(*q);
        }
        let at = t.apply(src.pos(p).ok_or(SketchError::NoPoint(p))?);
        let q = sk.add_point(at[0], at[1]);
        map.insert(p, q);
        Ok(q)
      };
      let kind = match &c.kind {
        CurveKind::Line { a, b } => CurveKind::Line { a: pt(self, *a)?, b: pt(self, *b)? },
        CurveKind::Circle { center, radius } => {
          CurveKind::Circle { center: pt(self, *center)?, radius: radius * t.scale }
        }
        CurveKind::Arc { center, start, end } => {
          CurveKind::Arc { center: pt(self, *center)?, start: pt(self, *start)?, end: pt(self, *end)? }
        }
        CurveKind::Spline { points, start_handle, end_handle } => CurveKind::Spline {
          points: points.iter().map(|p| pt(self, *p)).collect::<Result<_, _>>()?,
          start_handle: start_handle.map(|p| pt(self, p)).transpose()?,
          end_handle: end_handle.map(|p| pt(self, p)).transpose()?,
        },
        CurveKind::Bezier { points } => {
          CurveKind::Bezier { points: points.iter().map(|p| pt(self, *p)).collect::<Result<_, _>>()? }
        }
        CurveKind::ControlSpline { points } => {
          CurveKind::ControlSpline { points: points.iter().map(|p| pt(self, *p)).collect::<Result<_, _>>()? }
        }
        CurveKind::Ellipse { center, major, minor } => {
          CurveKind::Ellipse { center: pt(self, *center)?, major: pt(self, *major)?, minor: minor * t.scale }
        }
        CurveKind::EllipticalArc { center, major, minor, start, end } => CurveKind::EllipticalArc {
          center: pt(self, *center)?,
          major: pt(self, *major)?,
          minor: minor * t.scale,
          start: pt(self, *start)?,
          end: pt(self, *end)?,
        },
        // a rational curve's weights survive affine maps: rho stays
        CurveKind::Conic { start, end, apex, rho } => {
          CurveKind::Conic { start: pt(self, *start)?, end: pt(self, *end)?, apex: pt(self, *apex)?, rho: *rho }
        }
      };
      let id = self.add_curve(kind)?;
      if c.construction {
        self.set_construction(id, true)?;
      }
      out.push(id);
    }
    Ok(out)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ConstraintKind;

  fn square(sk: &mut Sketch) -> Vec<CurveId> {
    let p: Vec<PointId> = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]
      .iter()
      .map(|q| sk.add_point(q[0] + 5.0, q[1] + 5.0))
      .collect();
    (0..4).map(|i| sk.add_curve(CurveKind::Line { a: p[i], b: p[(i + 1) % 4] }).unwrap_or(CurveId(0))).collect()
  }

  #[test]
  fn scale_and_rotate_keep_the_shape_consistent() {
    let mut sk = Sketch::new();
    let lines = square(&mut sk);
    let [a, b] = [sk.curve_points(lines[0])[0], sk.curve_points(lines[0])[1]];
    sk.add_constraint(ConstraintKind::Horizontal { line: lines[0] }).unwrap();
    let d = sk.add_constraint(ConstraintKind::Distance { a, b, value: Dim::expr("w", 10.0) }).unwrap();
    let t = Xform2 { scale: 2.0, center: [5.0, 5.0], ..Default::default() };
    let r = sk.transform(&lines, &t).unwrap();
    assert_eq!(r.unlinked, vec![d]);
    assert!(r.removed.is_empty());
    assert_eq!(sk.pos(b), Some([25.0, 5.0]));
    assert!(sk.solve().ok());
    assert_eq!(sk.constraint(d).and_then(|c| c.kind.dim()).map(|d| d.value), Some(20.0));

    let quarter = Xform2 { angle: std::f64::consts::FRAC_PI_2, center: [5.0, 5.0], ..Default::default() };
    let r = sk.transform(&lines, &quarter).unwrap();
    assert_eq!(r.removed.len(), 1, "the horizontal constraint no longer holds");
    let q = sk.pos(b).unwrap();
    assert!((q[0] - 5.0).abs() < 1e-9 && (q[1] - 25.0).abs() < 1e-9, "{q:?}");
    assert!(sk.solve().ok());
  }

  #[test]
  fn insert_copies_shared_points_once() {
    let mut src = Sketch::new();
    let lines = square(&mut src);
    let c = src.add_point(10.0, 10.0);
    let circle = src.add_curve(CurveKind::Circle { center: c, radius: 2.0 }).unwrap();
    src.set_construction(circle, true).unwrap();
    let mut dst = Sketch::new();
    let t = Xform2 { scale: 0.5, translate: [100.0, 0.0], ..Default::default() };
    let new = dst.insert_from(&src, &[], &t).unwrap();
    assert_eq!(new.len(), 5);
    assert_eq!(dst.points.len(), 1 + 4 + 1, "origin, four shared corners, the centre");
    assert_eq!(dst.radius(new[4]), Some(1.0));
    assert!(dst.curve(new[4]).is_some_and(|c| c.construction));
    let some = dst.insert_from(&src, &lines[..1], &Xform2::default()).unwrap();
    assert_eq!(some.len(), 1);
    assert!(dst.insert_from(&src, &[CurveId(999)], &Xform2::default()).is_err());
    assert!(dst.transform(&new, &Xform2 { scale: 0.0, ..Default::default() }).is_err());
  }
}
