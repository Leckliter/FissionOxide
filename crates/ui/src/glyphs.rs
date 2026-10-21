//! Constraint glyphs as Fusion draws them in the canvas: small grey line-art symbols without a
//! box, beside the geometry they constrain.

use egui::{Color32, Pos2, Shape, Stroke, Vec2};
use fox_engine::sketch::ConstraintKind;

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
    // a small square round the shared point
    ConstraintKind::Coincident { .. } | ConstraintKind::PointOnCurve { .. } => {
      path(&[(-0.6, -0.6), (0.6, -0.6), (0.6, 0.6), (-0.6, 0.6)], true);
      painter.circle_filled(c, 1.3, col);
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
