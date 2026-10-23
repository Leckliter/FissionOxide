//! Fusion's manipulator arrow and what goes with it while a dialog shows one: the solid blue
//! arrow on the far end of the value (light grey when Tab gave the value box to another handle),
//! the dashed blue-and-white track along its axis while it is dragged, the distance dimension
//! (a thin dark line with the unitless value) along it, and the snap to model vertices and edges
//! with its "Snap To" tooltip. `viewport::manipulator` and `viewport::paint_arrow` draw with it.

use egui::{Color32, Pos2, Stroke, Vec2};
use fox_engine::Session;
use fox_engine::kernel::CurveKind;
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::{BodyId, LengthUnit};

/// Fusion's manipulator blue (the lit side of the arrow) and the side in shade.
const FILL: Color32 = Color32::from_rgb(30, 155, 215);
const SHADE: Color32 = Color32::from_rgb(20, 118, 170);
/// The arrow while another value has the box: light grey with a dark outline.
const IDLE_FILL: Color32 = Color32::from_rgb(214, 214, 214);
const IDLE_SHADE: Color32 = Color32::from_rgb(176, 176, 176);
const IDLE_OUTLINE: Color32 = Color32::from_rgb(60, 60, 60);
/// The distance dimension's line and its value's grey text.
const DIM_LINE: Color32 = Color32::from_rgb(64, 64, 64);
const DIM_VALUE: Color32 = Color32::from_rgb(110, 110, 110);

/// The arrow's shaft and head lengths and half widths (points).
const SHAFT: f32 = 18.0;
const HEAD: f32 = 14.0;
const HALF_SHAFT: f32 = 2.5;
const HALF_HEAD: f32 = 7.0;
/// Pointer distance from the arrow that grabs it (points).
const GRAB_PX: f32 = 8.0;
/// Pointer distance from a model vertex or edge that snaps the dragged value to it (points).
pub(crate) const SNAP_PX: f32 = 8.0;

/// An arrow on screen: it starts at `base` (the value's end) and points along unit `dir`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Arrow {
  pub base: Pos2,
  pub dir: Vec2,
}

impl Arrow {
  /// The arrow's point.
  pub fn tip(&self) -> Pos2 {
    self.base + self.dir * (SHAFT + HEAD)
  }

  /// Across the arrow, on the side to the right on screen (where a box reads away from it).
  fn side(&self) -> Vec2 {
    let s = Vec2::new(-self.dir.y, self.dir.x);
    if s.x < 0.0 || (s.x == 0.0 && s.y < 0.0) { -s } else { s }
  }

  /// Does a pointer at `p` grab the arrow (anywhere along its shaft and head)?
  pub fn hit(&self, p: Pos2) -> bool {
    segment_distance(p, self.base, self.tip()) <= GRAB_PX
  }

  /// Where its value box or label goes: beside the head, clear of the arrow.
  pub fn label_at(&self) -> Pos2 {
    self.base + self.dir * (SHAFT + HEAD * 0.5) + self.side() * (HALF_HEAD + 7.0)
  }

  /// Draws the arrow: Fusion's solid blue with a white outline, shaded down one side so it reads
  /// as a solid; light grey with a dark outline when another value has the box (`active` false).
  pub fn paint(&self, painter: &egui::Painter, active: bool) {
    let (fill, shade, outline) =
      if active { (FILL, SHADE, Color32::WHITE) } else { (IDLE_FILL, IDLE_SHADE, IDLE_OUTLINE) };
    let (b, d, s) = (self.base, self.dir, Vec2::new(-self.dir.y, self.dir.x));
    let neck = b + d * SHAFT;
    let tip = self.tip();
    let outline_pts = vec![
      b + s * HALF_SHAFT,
      neck + s * HALF_SHAFT,
      neck + s * HALF_HEAD,
      tip,
      neck - s * HALF_HEAD,
      neck - s * HALF_SHAFT,
      b - s * HALF_SHAFT,
    ];
    painter.add(egui::Shape::closed_line(outline_pts, Stroke::new(3.0, outline)));
    for (side, col) in [(1.0, fill), (-1.0, shade)] {
      let s = s * side;
      painter.add(egui::Shape::convex_polygon(
        vec![b, b + s * HALF_SHAFT, neck + s * HALF_SHAFT, neck],
        col,
        Stroke::NONE,
      ));
      painter.add(egui::Shape::convex_polygon(vec![neck, neck + s * HALF_HEAD, tip], col, Stroke::NONE));
    }
  }
}

/// How far `p` is from the segment `a`–`b`.
fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
  let ab = b - a;
  let t = if ab.length_sq() > 1e-6 { ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
  (a + ab * t - p).length()
}

/// The dashed blue-and-white track Fusion draws along a dragged arrow's axis, through `at`
/// along unit `dir`, across the whole view.
pub(crate) fn paint_track(painter: &egui::Painter, at: Pos2, dir: Vec2) {
  let reach = painter.clip_rect().size().length() + (at - painter.clip_rect().center()).length();
  let ends = [at - dir * reach, at + dir * reach];
  painter.line_segment(ends, Stroke::new(2.0, Color32::WHITE));
  painter.extend(egui::Shape::dashed_line(&ends, Stroke::new(2.0, FILL), 6.0, 6.0));
}

/// A length (mm) as the distance dimension reads it: in the design's unit without the unit, at
/// the document precision with trailing zeroes hidden down to two decimals ("1.30", "2.00",
/// "0.601", "25.00").
pub(crate) fn dimension_number(unit: LengthUnit, mm: f64) -> String {
  crate::units::length_number(unit, mm)
}

/// The distance dimension along an arrow's axis, from `a` (where the value starts) to `b` (its
/// end): a thin dark line with arrowheads at both ends and the value as grey text beside it.
pub(crate) fn paint_dimension(painter: &egui::Painter, a: Pos2, b: Pos2, text: &str) {
  let v = b - a;
  if v.length() < 2.0 {
    return;
  }
  let d = v.normalized();
  painter.line_segment([a, b], Stroke::new(1.0, DIM_LINE));
  for (tip, dir) in [(a, -d), (b, d)] {
    let (base, n) = (tip - dir * 8.0, dir.rot90() * 2.8);
    painter.add(egui::Shape::convex_polygon(vec![tip, base + n, base - n], DIM_LINE, Stroke::NONE));
  }
  // the value reads beside the line's middle, on the side away from the arrow's box
  let side = {
    let s = Vec2::new(-d.y, d.x);
    if s.x > 0.0 { -s } else { s }
  };
  let at = a + v * 0.5 + side * 6.0;
  let align = if side.x < -0.3 { egui::Align2::RIGHT_CENTER } else { egui::Align2::CENTER_BOTTOM };
  painter.text(at, align, text, egui::FontId::proportional(12.0), DIM_VALUE);
}

/// The model point a dragged arrow snaps to with the pointer at `pointer`: the nearest vertex
/// within reach, else the nearest point of an edge within reach, of a visible body other than
/// those in `skip` (the dialog's own preview). `project` maps world points to the screen.
pub(crate) fn snap_point(
  s: &Session,
  skip: &[BodyId],
  pointer: Pos2,
  project: &dyn Fn(DVec3) -> Option<Pos2>,
) -> Option<DVec3> {
  let mut vertex: Option<(f32, DVec3)> = None;
  let mut edge: Option<(f32, DVec3)> = None;
  let keep = |best: &mut Option<(f32, DVec3)>, d: f32, p: DVec3| {
    if d <= SNAP_PX && best.is_none_or(|(b, _)| d < b) {
      *best = Some((d, p));
    }
  };
  for body in s.model.bodies.iter().filter(|b| !skip.contains(&b.id) && s.body_visible(b.id)) {
    for e in body.edges() {
      for p in [e.start, e.end] {
        if let Some(q) = project(p) {
          keep(&mut vertex, (q - pointer).length(), p);
        }
      }
      // the edge as a polyline: lines are straight, circles and others sampled
      let pts: Vec<DVec3> = match e.kind {
        CurveKind::Line => vec![e.start, e.end],
        CurveKind::Circle if e.radius > 0.0 => {
          let zero = (e.start - e.center).try_normalize().unwrap_or_else(|| e.axis.any_orthonormal_vector());
          let y = e.axis.cross(zero);
          (0..=48)
            .map(|k| (k as f64 / 48.0) * std::f64::consts::TAU)
            .map(|t| e.center + (zero * t.cos() + y * t.sin()) * e.radius)
            .collect()
        }
        _ => vec![e.start, e.mid, e.end],
      };
      for w in pts.windows(2) {
        let (Some(a), Some(b)) = (project(w[0]), project(w[1])) else { continue };
        let ab = b - a;
        let t = if ab.length_sq() > 1e-6 { ((pointer - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
        keep(&mut edge, (a + ab * t - pointer).length(), w[0].lerp(w[1], t as f64));
      }
    }
  }
  vertex.or(edge).map(|(_, p)| p)
}

/// The value a snap reads out ("Snap To : 0.0767692"): in the design's unit without the unit,
/// to six significant digits with trailing zeros dropped.
pub(crate) fn snap_number(unit: LengthUnit, mm: f64) -> String {
  let v = unit.from_mm(mm);
  if v.abs() < 1e-12 {
    return "0".into();
  }
  let decimals = (5 - v.abs().log10().floor() as i32).clamp(0, 12) as usize;
  let s = format!("{v:.decimals$}");
  let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { &s };
  if s == "-0" { "0".into() } else { s.to_string() }
}

/// Fusion's "Snap To : value" tooltip by the pointer while a dragged arrow sits on a snap.
pub(crate) fn snap_tooltip(ctx: &egui::Context, layer: egui::LayerId, text: String) {
  egui::Tooltip::always_open(ctx.clone(), layer, egui::Id::new("manip-snap-tip"), egui::PopupAnchor::Pointer)
    .gap(14.0)
    .show(|ui| {
      ui.add(egui::Label::new(text).extend());
    });
}

/// The screen rectangle an arrow covers (for tests and hit checks).
#[cfg(test)]
fn bounds(a: &Arrow) -> egui::Rect {
  egui::Rect::from_two_pos(a.base, a.tip()).expand(HALF_HEAD)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_arrow_sits_beyond_the_value_and_is_grabbed_along_its_length() {
    let a = Arrow { base: Pos2::new(100.0, 100.0), dir: Vec2::new(0.0, -1.0) };
    assert_eq!(a.tip(), Pos2::new(100.0, 100.0 - SHAFT - HEAD));
    assert!(a.hit(Pos2::new(100.0, 100.0)) && a.hit(Pos2::new(103.0, 90.0)) && a.hit(a.tip()));
    assert!(!a.hit(Pos2::new(100.0, 120.0)), "not behind the value's end");
    assert!(!a.hit(Pos2::new(120.0, 90.0)));
    // its box reads to the right of it, clear of it
    let at = a.label_at();
    assert!(at.x > bounds(&a).right() - 1.0, "{at:?}");
    let left = Arrow { base: Pos2::new(100.0, 100.0), dir: Vec2::new(0.0, 1.0) };
    assert!(left.label_at().x > 100.0);
  }

  #[test]
  fn dimension_values_keep_two_decimals_and_drop_the_unit() {
    assert_eq!(dimension_number(LengthUnit::In, 2.0 * 25.4), "2.00");
    assert_eq!(dimension_number(LengthUnit::In, 1.3 * 25.4), "1.30");
    assert_eq!(dimension_number(LengthUnit::In, 0.601 * 25.4), "0.601");
    assert_eq!(dimension_number(LengthUnit::Mm, 25.0), "25.00");
    assert_eq!(dimension_number(LengthUnit::Mm, -3.5), "-3.50");
    assert_eq!(dimension_number(LengthUnit::Mm, -0.0001), "0.00");
    assert_eq!(snap_number(LengthUnit::In, 0.0767692 * 25.4), "0.0767692");
    assert_eq!(snap_number(LengthUnit::Mm, 20.0), "20");
    assert_eq!(snap_number(LengthUnit::Mm, -12.5), "-12.5");
  }

  /// A dragged arrow snaps to a model vertex near the pointer, else to an edge, never to the
  /// dialog's own preview.
  #[test]
  fn the_drag_snaps_to_model_vertices_and_edges() {
    use serde_json::json;
    let mut s = Session::default();
    s.execute("box", &json!({"width": 20, "depth": 20, "height": 30})).unwrap();
    let body = s.model.bodies[0].id;
    // seen from the front (down +Y): world (x, z) → screen (200 + x, 200 - z)
    let project = |p: DVec3| Some(Pos2::new(200.0 + p.x as f32, 200.0 - p.z as f32));
    // by the top corner at x = 10, z = 30
    let p = snap_point(&s, &[], Pos2::new(213.0, 168.0), &project).expect("a vertex");
    assert!((p.z - 30.0).abs() < 1e-9 && (p.x - 10.0).abs() < 1e-9, "{p:?}");
    // by the right side's vertical edge halfway up: a point on the edge
    let p = snap_point(&s, &[], Pos2::new(214.0, 185.0), &project).expect("an edge");
    assert!((p.x - 10.0).abs() < 1e-9 && (p.z - 15.0).abs() < 1e-6, "{p:?}");
    // nothing near, or the body is the preview
    assert!(snap_point(&s, &[], Pos2::new(260.0, 120.0), &project).is_none());
    assert!(snap_point(&s, &[body], Pos2::new(213.0, 168.0), &project).is_none());
  }
}
