//! Convenience builders for common sketch shapes (what the Create menu draws).

use crate::{ConstraintKind, CurveId, CurveKind, PointId, Sketch, SketchError, angle_of, dist};

/// Where a new curve's point goes: a fresh point at a position, or an existing point (which makes
/// the two curves share it, i.e. coincident).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum PtSpec {
  Point(PointId),
  At(f64, f64),
}

impl From<[f64; 2]> for PtSpec {
  fn from(p: [f64; 2]) -> Self {
    PtSpec::At(p[0], p[1])
  }
}

impl Sketch {
  /// Resolves a spec to a point id, creating the point if needed.
  pub fn resolve(&mut self, spec: PtSpec) -> Result<PointId, SketchError> {
    match spec {
      PtSpec::Point(id) => self.point(id).map(|p| p.id).ok_or(SketchError::NoPoint(id)),
      PtSpec::At(x, y) => {
        if !(x.is_finite() && y.is_finite()) {
          return Err(SketchError::InvalidValue(format!("point ({x}, {y})")));
        }
        Ok(self.add_point(x, y))
      }
    }
  }

  fn spec_pos(&self, spec: PtSpec) -> Result<[f64; 2], SketchError> {
    match spec {
      PtSpec::Point(id) => self.pos(id).ok_or(SketchError::NoPoint(id)),
      PtSpec::At(x, y) => Ok([x, y]),
    }
  }

  pub fn add_line(&mut self, a: PtSpec, b: PtSpec) -> Result<(CurveId, [PointId; 2]), SketchError> {
    if self.spec_pos(a)? == self.spec_pos(b)? {
      return Err(SketchError::InvalidValue("zero-length line".into()));
    }
    let a = self.resolve(a)?;
    let b = self.resolve(b)?;
    let id = self.add_curve(CurveKind::Line { a, b })?;
    Ok((id, [a, b]))
  }

  pub fn add_circle(&mut self, center: PtSpec, radius: f64) -> Result<CurveId, SketchError> {
    if !(radius.is_finite() && radius > 0.0) {
      return Err(SketchError::InvalidValue(format!("radius {radius}")));
    }
    let center = self.resolve(center)?;
    self.add_curve(CurveKind::Circle { center, radius })
  }

  /// Arc around `center` from `start`, counter-clockwise to the direction of `end` (the end point
  /// is projected onto the circle through `start`).
  pub fn add_arc_center(&mut self, center: PtSpec, start: PtSpec, end: [f64; 2]) -> Result<CurveId, SketchError> {
    self.add_arc_center_dir(center, start, end, false)
  }

  /// Arc around `center` from `start` to the direction of `end`, clockwise when `clockwise` is
  /// set. Arcs are stored counter-clockwise, so a clockwise one runs from the new end point back
  /// to `start`.
  pub fn add_arc_center_dir(
    &mut self,
    center: PtSpec,
    start: PtSpec,
    end: [f64; 2],
    clockwise: bool,
  ) -> Result<CurveId, SketchError> {
    let c = self.spec_pos(center)?;
    let s = self.spec_pos(start)?;
    let r = dist(c, s);
    if !(r > 1e-9) {
      return Err(SketchError::InvalidValue("zero-radius arc".into()));
    }
    let a = angle_of(c, end);
    let e = [c[0] + r * a.cos(), c[1] + r * a.sin()];
    if dist(e, s) < 1e-9 {
      return Err(SketchError::InvalidValue("arc start and end coincide".into()));
    }
    let center = self.resolve(center)?;
    let start = self.resolve(start)?;
    let end = self.add_point(e[0], e[1]);
    let kind =
      if clockwise { CurveKind::Arc { center, start: end, end: start } } else { CurveKind::Arc { center, start, end } };
    self.add_curve(kind)
  }

  /// Arc through three points (start, a point on the arc, end).
  pub fn add_arc_3pt(&mut self, start: PtSpec, mid: [f64; 2], end: PtSpec) -> Result<CurveId, SketchError> {
    let s = self.spec_pos(start)?;
    let e = self.spec_pos(end)?;
    let c = circumcenter(s, mid, e).ok_or(SketchError::InvalidValue("the three points are collinear".into()))?;
    // Counter-clockwise from s must pass through mid; otherwise the arc runs e → s.
    let a0 = angle_of(c, s);
    let through_mid = crate::ccw_sweep(a0, angle_of(c, mid)) < crate::ccw_sweep(a0, angle_of(c, e));
    let start = self.resolve(start)?;
    let end = self.resolve(end)?;
    let center = self.add_point(c[0], c[1]);
    let kind = if through_mid {
      CurveKind::Arc { center, start, end }
    } else {
      CurveKind::Arc { center, start: end, end: start }
    };
    self.add_curve(kind)
  }

  /// Axis-aligned two-point rectangle: four lines sharing corners, horizontal/vertical constrained.
  /// Returns the lines bottom, right, top, left.
  pub fn add_rectangle(&mut self, c1: [f64; 2], c2: [f64; 2]) -> Result<[CurveId; 4], SketchError> {
    let (x0, x1) = (c1[0].min(c2[0]), c1[0].max(c2[0]));
    let (y0, y1) = (c1[1].min(c2[1]), c1[1].max(c2[1]));
    if !(x1 - x0 > 1e-9 && y1 - y0 > 1e-9) {
      return Err(SketchError::InvalidValue("rectangle has no area".into()));
    }
    let p = [self.add_point(x0, y0), self.add_point(x1, y0), self.add_point(x1, y1), self.add_point(x0, y1)];
    let mut lines = [CurveId(0); 4];
    for i in 0..4 {
      lines[i] = self.add_curve(CurveKind::Line { a: p[i], b: p[(i + 1) % 4] })?;
    }
    self.add_constraint(ConstraintKind::Horizontal { line: lines[0] })?;
    self.add_constraint(ConstraintKind::Horizontal { line: lines[2] })?;
    self.add_constraint(ConstraintKind::Vertical { line: lines[1] })?;
    self.add_constraint(ConstraintKind::Vertical { line: lines[3] })?;
    Ok(lines)
  }

  /// Rectangle from its centre and a corner, with a construction diagonal whose midpoint is the
  /// centre (so moving the centre moves the rectangle, as in Fusion).
  pub fn add_center_rectangle(&mut self, center: PtSpec, corner: [f64; 2]) -> Result<[CurveId; 4], SketchError> {
    let c = self.spec_pos(center)?;
    let other = [2.0 * c[0] - corner[0], 2.0 * c[1] - corner[1]];
    let lines = self.add_rectangle(other, corner)?;
    let center = self.resolve(center)?;
    // diagonal from the bottom-left to the top-right corner
    let bl = self.curve_points(lines[0]).first().copied();
    let tr = self.curve_points(lines[1]).get(1).copied();
    if let (Some(bl), Some(tr)) = (bl, tr) {
      let diag = self.add_curve(CurveKind::Line { a: bl, b: tr })?;
      self.set_construction(diag, true)?;
      self.add_constraint(ConstraintKind::Midpoint { p: center, line: diag })?;
    }
    Ok(lines)
  }

  /// Regular polygon inscribed in a construction circle; `first` is the first vertex.
  pub fn add_polygon(&mut self, center: PtSpec, first: [f64; 2], sides: usize) -> Result<Vec<CurveId>, SketchError> {
    if !(3..=256).contains(&sides) {
      return Err(SketchError::InvalidValue(format!("{sides} sides (3–256)")));
    }
    let c = self.spec_pos(center)?;
    let r = dist(c, first);
    if !(r > 1e-9) {
      return Err(SketchError::InvalidValue("zero-size polygon".into()));
    }
    let center = self.resolve(center)?;
    let circle = self.add_curve(CurveKind::Circle { center, radius: r })?;
    self.set_construction(circle, true)?;
    let a0 = angle_of(c, first);
    let pts: Vec<PointId> = (0..sides)
      .map(|i| {
        let a = a0 + std::f64::consts::TAU * i as f64 / sides as f64;
        self.add_point(c[0] + r * a.cos(), c[1] + r * a.sin())
      })
      .collect();
    let mut lines = Vec::with_capacity(sides);
    for i in 0..sides {
      let (a, b) = (pts[i], pts[(i + 1) % sides]);
      lines.push(self.add_curve(CurveKind::Line { a, b })?);
      self.add_constraint(ConstraintKind::PointOnCurve { p: a, curve: circle })?;
    }
    for w in lines.windows(2) {
      self.add_constraint(ConstraintKind::Equal { a: w[0], b: w[1] })?;
    }
    Ok(lines)
  }

  /// Centre-to-centre slot of overall `width`: two lines and two tangent end arcs.
  pub fn add_slot(&mut self, c1: [f64; 2], c2: [f64; 2], width: f64) -> Result<Vec<CurveId>, SketchError> {
    let len = dist(c1, c2);
    if !(len > 1e-9 && width.is_finite() && width > 1e-9) {
      return Err(SketchError::InvalidValue("degenerate slot".into()));
    }
    let r = width / 2.0;
    let d = [(c2[0] - c1[0]) / len, (c2[1] - c1[1]) / len];
    let n = [-d[1], d[0]];
    let at = |c: [f64; 2], s: f64| [c[0] + n[0] * r * s, c[1] + n[1] * r * s];
    let ce1 = self.add_point(c1[0], c1[1]);
    let ce2 = self.add_point(c2[0], c2[1]);
    let p1 = at(c1, 1.0);
    let p2 = at(c2, 1.0);
    let p3 = at(c2, -1.0);
    let p4 = at(c1, -1.0);
    let p1 = self.add_point(p1[0], p1[1]);
    let p2 = self.add_point(p2[0], p2[1]);
    let p3 = self.add_point(p3[0], p3[1]);
    let p4 = self.add_point(p4[0], p4[1]);
    let l1 = self.add_curve(CurveKind::Line { a: p4, b: p3 })?;
    let a2 = self.add_curve(CurveKind::Arc { center: ce2, start: p3, end: p2 })?;
    let l2 = self.add_curve(CurveKind::Line { a: p2, b: p1 })?;
    let a1 = self.add_curve(CurveKind::Arc { center: ce1, start: p1, end: p4 })?;
    for (l, a) in [(l1, a2), (l2, a2), (l1, a1), (l2, a1)] {
      self.add_constraint(ConstraintKind::Tangent { a: l, b: a })?;
    }
    self.add_constraint(ConstraintKind::Equal { a: a1, b: a2 })?;
    Ok(vec![l1, a2, l2, a1])
  }
}

/// Centre of the circle through three points, or None when they are (nearly) collinear.
pub fn circumcenter(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> Option<[f64; 2]> {
  let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
  let scale = dist(a, b).max(dist(b, c)).max(dist(a, c));
  if d.abs() < 1e-12 * scale * scale.max(1.0) {
    return None;
  }
  let a2 = a[0] * a[0] + a[1] * a[1];
  let b2 = b[0] * b[0] + b[1] * b[1];
  let c2 = c[0] * c[0] + c[1] * c[1];
  let x = (a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d;
  let y = (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d;
  (x.is_finite() && y.is_finite()).then_some([x, y])
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn arc_3pt_orientation() {
    let mut s = Sketch::new();
    // upper half circle from (10,0) through (0,10) to (-10,0): CCW
    let a = s.add_arc_3pt(PtSpec::At(10.0, 0.0), [0.0, 10.0], PtSpec::At(-10.0, 0.0)).unwrap();
    let CurveKind::Arc { center, start, .. } = s.curve(a).unwrap().kind.clone() else { panic!() };
    let c = s.pos(center).unwrap();
    assert!(c[0].abs() < 1e-9 && c[1].abs() < 1e-9);
    assert_eq!(s.pos(start).unwrap(), [10.0, 0.0]);
    // lower half: start/end swapped so the arc is still CCW
    let b = s.add_arc_3pt(PtSpec::At(10.0, 0.0), [0.0, -10.0], PtSpec::At(-10.0, 0.0)).unwrap();
    let CurveKind::Arc { start, .. } = s.curve(b).unwrap().kind.clone() else { panic!() };
    assert_eq!(s.pos(start).unwrap(), [-10.0, 0.0]);
    assert!(s.add_arc_3pt(PtSpec::At(0.0, 0.0), [1.0, 0.0], PtSpec::At(2.0, 0.0)).is_err());
  }

  #[test]
  fn shapes_build() {
    let mut s = Sketch::new();
    s.add_center_rectangle(PtSpec::Point(crate::ORIGIN), [10.0, 5.0]).unwrap();
    s.add_polygon(PtSpec::At(50.0, 0.0), [60.0, 0.0], 6).unwrap();
    s.add_slot([0.0, 30.0], [20.0, 30.0], 6.0).unwrap();
    assert!(s.add_polygon(PtSpec::At(0.0, 0.0), [1.0, 0.0], 2).is_err());
    assert!(s.add_slot([0.0, 0.0], [0.0, 0.0], 1.0).is_err());
    assert!(s.add_rectangle([0.0, 0.0], [0.0, 5.0]).is_err());
  }
}
