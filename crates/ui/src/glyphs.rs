//! Constraint glyphs as Fusion draws them in the canvas: small grey line-art symbols without a
//! box, beside the geometry they constrain, and where each constraint's glyphs go.

use egui::{Color32, Pos2, Shape, Stroke, Vec2};
use fox_engine::sketch::ConstraintKind;
use fox_engine::sketch::{CurveId, PointId, Sketch};

/// Half the side of the square a glyph is drawn in (points).
pub const HALF: f32 = 5.5;

/// Paints the glyph of `k` centred on `c` in `col`.
pub fn paint(painter: &egui::Painter, c: Pos2, k: &ConstraintKind, col: Color32) {
  use std::f32::consts::{PI, TAU};
  let h = HALF;
  let stroke = Stroke::new(1.2, col);
  let at = |x: f32, y: f32| c + Vec2::new(x * h, y * h);
  let line = |a: (f32, f32), b: (f32, f32)| painter.line_segment([at(a.0, a.1), at(b.0, b.1)], stroke);
  let path = |pts: &[(f32, f32)], closed: bool| {
    let v: Vec<Pos2> = pts.iter().map(|p| at(p.0, p.1)).collect();
    if closed {
      painter.add(Shape::closed_line(v, stroke));
    } else {
      painter.add(Shape::line(v, stroke));
    }
  };
  let arc = |cx: f32, cy: f32, r: f32, a0: f32, a1: f32| {
    let n = 16;
    let v: Vec<Pos2> =
      (0..=n).map(|i| a0 + (a1 - a0) * i as f32 / n as f32).map(|a| at(cx + r * a.cos(), cy - r * a.sin())).collect();
    painter.add(Shape::line(v, stroke));
  };
  match k {
    // a triangle pointing at the line's middle
    ConstraintKind::Midpoint { .. } => path(&[(0.0, -0.9), (0.9, 0.8), (-0.9, 0.8)], true),
    // an "L" corner with its right-angle mark
    ConstraintKind::Perpendicular { .. } => {
      line((-0.8, 0.9), (0.9, 0.9));
      line((-0.8, 0.9), (-0.8, -0.8));
      path(&[(-0.8, 0.2), (-0.1, 0.2), (-0.1, 0.9)], false);
    }
    // a circle with a bar touching it
    ConstraintKind::Tangent { .. } => {
      arc(0.0, 0.25, 0.65, 0.0, TAU);
      line((-0.95, -0.4), (0.95, -0.4));
    }
    ConstraintKind::Equal { .. } => {
      line((-0.8, -0.35), (0.8, -0.35));
      line((-0.8, 0.35), (0.8, 0.35));
    }
    ConstraintKind::Parallel { .. } => {
      line((-0.7, 0.9), (0.1, -0.9));
      line((-0.1, 0.9), (0.7, -0.9));
    }
    ConstraintKind::Polygon { .. } => {
      let pts: Vec<(f32, f32)> = (0..5)
        .map(|i| {
          let a = PI / 2.0 + TAU * i as f32 / 5.0;
          (0.9 * a.cos(), -0.9 * a.sin())
        })
        .collect();
      path(&pts, true);
    }
    // a bar with end ticks, along the way the line is held
    ConstraintKind::Horizontal { .. } | ConstraintKind::HorizontalPoints { .. } => {
      line((-0.9, 0.0), (0.9, 0.0));
      line((-0.9, -0.4), (-0.9, 0.4));
      line((0.9, -0.4), (0.9, 0.4));
    }
    ConstraintKind::Vertical { .. } | ConstraintKind::VerticalPoints { .. } => {
      line((0.0, -0.9), (0.0, 0.9));
      line((-0.4, -0.9), (0.4, -0.9));
      line((-0.4, 0.9), (0.4, 0.9));
    }
    // an "L" corner with a point in it, shown while the merged point is hovered
    ConstraintKind::Coincident { .. } | ConstraintKind::PointOnCurve { .. } => {
      path(&[(-0.8, -0.9), (-0.8, 0.9), (0.9, 0.9)], false);
      painter.circle_filled(at(0.2, 0.0), 1.6, col);
    }
    // a padlock
    ConstraintKind::Fix { .. } => {
      path(&[(-0.8, -0.1), (0.8, -0.1), (0.8, 0.95), (-0.8, 0.95)], true);
      arc(0.0, -0.1, 0.5, 0.0, PI);
    }
    ConstraintKind::Concentric { .. } => {
      arc(0.0, 0.0, 0.9, 0.0, TAU);
      arc(0.0, 0.0, 0.4, 0.0, TAU);
    }
    // two dashes on one line
    ConstraintKind::Collinear { .. } => {
      line((-0.95, 0.0), (-0.2, 0.0));
      line((0.2, 0.0), (0.95, 0.0));
    }
    // a dashed mirror line with a mark each side
    ConstraintKind::Symmetric { .. } | ConstraintKind::SymmetricCurves { .. } => {
      for i in 0..3 {
        let y = -0.9 + i as f32 * 0.7;
        line((0.0, y), (0.0, y + 0.35));
      }
      line((-0.8, -0.3), (-0.8, 0.3));
      line((0.8, -0.3), (0.8, 0.3));
    }
    // an arc running smoothly into a line
    ConstraintKind::Curvature { .. } => {
      arc(-0.3, 0.3, 0.6, -PI / 2.0, 0.0);
      line((0.3, 0.3), (0.3, -0.9));
    }
    _ => arc(0.0, 0.0, 0.7, 0.0, TAU),
  }
}

/// Where one of a constraint's glyphs goes on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
  /// The glyph's centre.
  pub at: Pos2,
  /// The host curve's direction on screen there (x for a point): the next glyph sharing the
  /// host steps this way.
  pub dir: Vec2,
  /// The host curve (or point, flagged with the top bit), so glyphs sharing it line up.
  pub key: u32,
}

/// How far a glyph sits from the geometry it marks (points).
pub const GAP: f32 = 12.0;

/// The glyphs of `k`, placed as Fusion places them: Horizontal and Vertical just below (right
/// of) the line's middle; Parallel and Collinear one under the middle of each line; Equal one on
/// the middle of each curve; Perpendicular one inside the corner the lines share; MidPoint above
/// the point; Symmetry above each mirrored item and one on the axis; Concentric beside the shared
/// centre; Tangent and Curvature at the tangency (or the end nearest it); Coincident nothing
/// until its point is hovered (`hovered`), then below-left of it; Fix and Polygon above-right of
/// their point. `to_screen` maps sketch coordinates to the screen.
pub fn places(
  s: &Sketch,
  k: &ConstraintKind,
  hovered: Option<PointId>,
  to_screen: &dyn Fn([f64; 2]) -> Option<Pos2>,
) -> Vec<Place> {
  let point_key = |p: PointId| p.0 | 0x8000_0000;
  let pt = |p: PointId| s.pos(p).and_then(to_screen);
  let on_point =
    |p: PointId, off: Vec2| -> Option<Place> { Some(Place { at: pt(p)? + off, dir: Vec2::X, key: point_key(p) }) };
  let curve_screen = |c: CurveId| -> Vec<Pos2> {
    s.curve(c)
      .map(|c| fox_engine::scene::curve_polyline(s, &c.kind))
      .unwrap_or_default()
      .iter()
      .filter_map(|p| to_screen(*p))
      .collect()
  };
  // a curve's middle on screen and its direction there
  let mid = |c: CurveId| -> Option<(Pos2, Vec2)> {
    let pts = curve_screen(c);
    let n = pts.len();
    if n < 2 {
      return None;
    }
    if n == 2 {
      return Some((pts[0] + (pts[1] - pts[0]) / 2.0, unit(pts[1] - pts[0])));
    }
    let i = n / 2;
    Some((pts[i], unit(pts[(i + 1).min(n - 1)] - pts[i - 1])))
  };
  let on_curve = |c: CurveId, off: &dyn Fn(Vec2) -> Vec2| -> Option<Place> {
    let (m, dir) = mid(c)?;
    Some(Place { at: m + off(dir), dir, key: c.0 })
  };
  let beside_mid = |c: CurveId| on_curve(c, &|d| beside(d) * GAP);
  let above_mid = |c: CurveId| on_curve(c, &|_| Vec2::new(0.0, -GAP));
  let on_mid = |c: CurveId| on_curve(c, &|_| Vec2::ZERO);
  let above = Vec2::new(0.0, -GAP);
  let corner = Vec2::new(10.0, -10.0);
  // the direction a curve leaves the point `p` (one of its ends) in
  let away = |c: CurveId, p: Pos2| -> Vec2 {
    let pts = curve_screen(c);
    match pts.len() {
      0 | 1 => Vec2::X,
      _ if pts[0].distance(p) <= pts[pts.len() - 1].distance(p) => unit(pts[1] - pts[0]),
      n => unit(pts[n - 2] - pts[n - 1]),
    }
  };
  // the point two curves share, if any
  let shared = |a: CurveId, b: CurveId| -> Option<PointId> {
    let bp = s.curve_points(b);
    s.curve_points(a).into_iter().find(|p| bp.contains(p))
  };
  let v: Vec<Option<Place>> = match k {
    ConstraintKind::Horizontal { line }
    | ConstraintKind::Vertical { line }
    | ConstraintKind::FixRadius { curve: line, .. } => {
      vec![beside_mid(*line)]
    }
    ConstraintKind::HorizontalPoints { a, b } | ConstraintKind::VerticalPoints { a, b } => match (pt(*a), pt(*b)) {
      (Some(pa), Some(pb)) => {
        let dir = unit(pb - pa);
        vec![Some(Place { at: pa + (pb - pa) / 2.0 + beside(dir) * GAP, dir, key: point_key(*a) })]
      }
      _ => vec![],
    },
    ConstraintKind::Parallel { a, b } | ConstraintKind::Collinear { a, b } => vec![beside_mid(*a), beside_mid(*b)],
    ConstraintKind::Equal { a, b } => vec![on_mid(*a), on_mid(*b)],
    ConstraintKind::Perpendicular { a, b } => match shared(*a, *b).and_then(pt) {
      Some(p) => {
        let bis = unit(away(*a, p) + away(*b, p));
        let bis = if bis == Vec2::ZERO { beside(away(*a, p)) } else { bis };
        vec![Some(Place { at: p + bis * GAP, dir: Vec2::X, key: a.0 })]
      }
      None => vec![beside_mid(*a), beside_mid(*b)],
    },
    // a sketch fillet's tangent glyph sits on the line just past the arc's end, outside the
    // shape; otherwise the glyph sits by the point the curves share, or where the curves touch
    // (for a line and a circle the point of the line nearest the centre, or the line's end
    // nearest the touch when only its extension is tangent), in a row of its own
    ConstraintKind::Tangent { a, b } | ConstraintKind::Curvature { a, b } => {
      let fillet = fillet_nudge(s, k).and_then(|(j, along, out)| {
        let sj = to_screen(j)?;
        let sa = unit(to_screen([j[0] + along[0], j[1] + along[1]])? - sj);
        let so = unit(to_screen([j[0] + out[0], j[1] + out[1]])? - sj);
        (sa != Vec2::ZERO && so != Vec2::ZERO).then(|| sj + sa * 12.0 + so * 9.0)
      });
      match (fillet, shared(*a, *b)) {
        (Some(at), p) => vec![Some(Place { at, dir: Vec2::X, key: p.map(point_key).unwrap_or(a.0) })],
        (None, Some(p)) => vec![pt(p).map(|at| Place { at: at + corner, dir: Vec2::X, key: point_key(p) })],
        (None, None) => match tangent_touch(s, *a, *b).or_else(|| touch_point(s, *a, *b)).and_then(to_screen) {
          Some(at) => {
            vec![Some(Place { at: at + corner, dir: Vec2::X, key: 0x4000_0000 | (a.0 ^ b.0.rotate_left(16)) })]
          }
          None => vec![beside_mid(*a)],
        },
      }
    }
    ConstraintKind::Midpoint { p, .. } => vec![on_point(*p, above)],
    ConstraintKind::Symmetric { a, b, line } => vec![on_point(*a, above), on_point(*b, above), on_mid(*line)],
    ConstraintKind::SymmetricCurves { a, b, line } => vec![above_mid(*a), above_mid(*b), on_mid(*line)],
    ConstraintKind::Concentric { a, .. } => {
      vec![s.curve_points(*a).first().and_then(|c| on_point(*c, Vec2::new(GAP, 0.0)))]
    }
    ConstraintKind::Coincident { a, b } => match hovered {
      Some(h) if h == *a || h == *b => vec![on_point(h, Vec2::new(-GAP, GAP))],
      _ => vec![],
    },
    ConstraintKind::PointOnCurve { p, .. } => match hovered {
      Some(h) if h == *p => vec![on_point(h, Vec2::new(-GAP, GAP))],
      _ => vec![],
    },
    ConstraintKind::Fix { p, .. } => vec![on_point(*p, corner)],
    ConstraintKind::Polygon { center, .. } => vec![on_point(*center, corner)],
    _ => match k.curves().first() {
      Some(c) => vec![beside_mid(*c)],
      None => vec![k.points().first().and_then(|p| on_point(*p, corner))],
    },
  };
  v.into_iter().flatten().collect()
}

fn unit(v: Vec2) -> Vec2 {
  if v.length() < 1e-6 { Vec2::ZERO } else { v.normalized() }
}

/// The side of a curve running along `dir` that Fusion hangs a glyph on: below it, or to the
/// right of a vertical one.
fn beside(dir: Vec2) -> Vec2 {
  let n = Vec2::new(-dir.y, dir.x);
  if n == Vec2::ZERO {
    return Vec2::new(0.0, 1.0);
  }
  let down = if n.y.abs() < 1e-3 { n.x > 0.0 } else { n.y > 0.0 };
  if down { n } else { -n }
}

/// The point of curve `a` nearest curve `b` (where a tangent pair touches): the best of samples
/// along `a`, refined on the stretch around it.
fn touch_point(s: &Sketch, a: CurveId, b: CurveId) -> Option<[f64; 2]> {
  let pts = fox_engine::scene::curve_polyline(s, &s.curve(a)?.kind);
  let lerp = |p: [f64; 2], q: [f64; 2], t: f64| [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
  let d = |p: [f64; 2]| s.distance_to_curve(b, p).unwrap_or(f64::INFINITY);
  let mut best: Option<(f64, usize, f64)> = None;
  for (i, w) in pts.windows(2).enumerate() {
    for k in 0..=16 {
      let t = k as f64 / 16.0;
      let dist = d(lerp(w[0], w[1], t));
      if best.is_none_or(|(bd, _, _)| dist < bd) {
        best = Some((dist, i, t));
      }
    }
  }
  let (dist, i, t) = best?;
  if !dist.is_finite() {
    return None;
  }
  let (p, q) = (pts[i], pts[i + 1]);
  // ternary search on the sixteenth around the best sample
  let (mut lo, mut hi) = ((t - 1.0 / 16.0).max(0.0), (t + 1.0 / 16.0).min(1.0));
  for _ in 0..40 {
    let (m1, m2) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
    if d(lerp(p, q, m1)) < d(lerp(p, q, m2)) { hi = m2 } else { lo = m1 }
  }
  Some(lerp(p, q, (lo + hi) / 2.0))
}

/// For a tangent constraint between a line and an arc that share an end (a sketch fillet): the
/// shared point, the unit direction along the line away from the arc, and the direction out of
/// the shape (away from the arc's centre), all in sketch coordinates; None for other glyphs.
fn fillet_nudge(s: &Sketch, k: &ConstraintKind) -> Option<([f64; 2], [f64; 2], [f64; 2])> {
  use fox_engine::sketch::CurveKind;
  let ConstraintKind::Tangent { a, b } = k else { return None };
  let (line, arc) = match (s.curve(*a).map(|c| &c.kind), s.curve(*b).map(|c| &c.kind)) {
    (Some(CurveKind::Line { .. }), Some(CurveKind::Arc { .. })) => (*a, *b),
    (Some(CurveKind::Arc { .. }), Some(CurveKind::Line { .. })) => (*b, *a),
    _ => return None,
  };
  let lp = s.curve_points(line);
  let ap = s.curve_points(arc);
  let shared = *lp.iter().find(|p| ap[1..].contains(p))?;
  let other = *lp.iter().find(|p| **p != shared)?;
  let (j, o, c) = (s.pos(shared)?, s.pos(other)?, s.pos(*ap.first()?)?);
  let unit = |v: [f64; 2]| {
    let l = v[0].hypot(v[1]);
    (l > 1e-9).then(|| [v[0] / l, v[1] / l])
  };
  let along = unit([o[0] - j[0], o[1] - j[1]])?;
  let out = unit([j[0] - c[0], j[1] - c[1]])?;
  Some((j, along, out))
}

/// For a Tangent between a line and a circle or arc: the point of the line's segment nearest the
/// circle's centre (the touch point when the segment reaches it, otherwise the nearer end).
fn tangent_touch(s: &Sketch, a: CurveId, b: CurveId) -> Option<[f64; 2]> {
  use fox_engine::sketch::CurveKind;
  let kind = |id: CurveId| s.curve(id).map(|c| &c.kind);
  let (line, round) = match (kind(a)?, kind(b)?) {
    (CurveKind::Line { .. }, CurveKind::Circle { .. } | CurveKind::Arc { .. }) => (a, b),
    (CurveKind::Circle { .. } | CurveKind::Arc { .. }, CurveKind::Line { .. }) => (b, a),
    _ => return None,
  };
  let pts = s.curve_points(line);
  let (p0, p1) = (s.pos(pts[0])?, s.pos(pts[1])?);
  let c = s.pos(s.curve_points(round)[0])?;
  let d = [p1[0] - p0[0], p1[1] - p0[1]];
  let len2 = d[0] * d[0] + d[1] * d[1];
  if len2 < 1e-18 {
    return None;
  }
  let t = (((c[0] - p0[0]) * d[0] + (c[1] - p0[1]) * d[1]) / len2).clamp(0.0, 1.0);
  Some([p0[0] + t * d[0], p0[1] + t * d[1]])
}
