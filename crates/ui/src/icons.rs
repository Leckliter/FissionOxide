//! Toolbar and timeline icons, drawn in code (no image assets, no font glyph gaps).

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
  Sketch,
  Extrude,
  Revolve,
  Box,
  Cylinder,
  Sphere,
  Torus,
  Pattern,
  CircPattern,
  Mirror,
  Fillet,
  Chamfer,
  Shell,
  Combine,
  Move,
  Plane,
  Measure,
  Import,
  Remove,
  Line,
  Rect,
  CenterRect,
  Circle,
  Arc3,
  CenterArc,
  Polygon,
  Slot,
  Point,
  Dimension,
  Construction,
  Coincident,
  Horizontal,
  Vertical,
  Parallel,
  Perpendicular,
  Tangent,
  Equal,
  Concentric,
  Midpoint,
  Collinear,
  Symmetric,
  Fix,
  Finish,
}

/// Draws `icon` into `r` (square-ish) with `c`.
pub fn draw(p: &Painter, r: Rect, icon: Icon, c: Color32) {
  let s = Stroke::new(1.5, c);
  let thin = Stroke::new(1.0, c);
  // unit square → rect
  let u = |x: f32, y: f32| -> Pos2 { pos2(r.min.x + x * r.width(), r.min.y + y * r.height()) };
  let line = |a: Pos2, b: Pos2| {
    p.line_segment([a, b], s);
  };
  let poly = |pts: &[Pos2], closed: bool| {
    if closed {
      p.add(Shape::closed_line(pts.to_vec(), s));
    } else {
      p.add(Shape::line(pts.to_vec(), s));
    }
  };
  let circle = |c0: Pos2, rad: f32| {
    p.circle_stroke(c0, rad, s);
  };
  let arc = |c0: Pos2, rad: f32, a0: f32, a1: f32| {
    let pts: Vec<Pos2> = (0..=16)
      .map(|i| {
        let t = a0 + (a1 - a0) * i as f32 / 16.0;
        c0 + vec2(t.cos(), -t.sin()) * rad
      })
      .collect();
    p.add(Shape::line(pts, s));
  };
  let ell = |c0: Pos2, rx: f32, ry: f32| {
    let pts: Vec<Pos2> = (0..32)
      .map(|i| {
        let t = std::f32::consts::TAU * i as f32 / 32.0;
        c0 + vec2(t.cos() * rx, t.sin() * ry)
      })
      .collect();
    p.add(Shape::closed_line(pts, thin));
  };
  let dot = |q: Pos2| {
    p.circle_filled(q, 2.0, c);
  };
  let w = r.width();
  let cen = r.center();
  match icon {
    Icon::Sketch => {
      poly(&[u(0.1, 0.35), u(0.65, 0.35), u(0.65, 0.9), u(0.1, 0.9)], true);
      line(u(0.45, 0.6), u(0.9, 0.1));
      line(u(0.4, 0.68), u(0.45, 0.6));
    }
    Icon::Extrude => {
      poly(&[u(0.1, 0.75), u(0.55, 0.75), u(0.9, 0.55), u(0.45, 0.55)], true);
      line(u(0.5, 0.62), u(0.5, 0.1));
      poly(&[u(0.35, 0.25), u(0.5, 0.08), u(0.65, 0.25)], false);
    }
    Icon::Revolve => {
      arc(cen, w * 0.36, 0.6, 5.4);
      poly(&[u(0.72, 0.12), u(0.8, 0.3), u(0.6, 0.32)], false);
      line(u(0.5, 0.05), u(0.5, 0.95));
    }
    Icon::Box => {
      poly(&[u(0.15, 0.35), u(0.6, 0.35), u(0.6, 0.85), u(0.15, 0.85)], true);
      poly(&[u(0.15, 0.35), u(0.4, 0.15), u(0.85, 0.15), u(0.6, 0.35)], false);
      poly(&[u(0.85, 0.15), u(0.85, 0.65), u(0.6, 0.85)], false);
    }
    Icon::Cylinder => {
      ell(u(0.5, 0.22), w * 0.32, w * 0.1);
      line(u(0.18, 0.22), u(0.18, 0.78));
      line(u(0.82, 0.22), u(0.82, 0.78));
      arc(u(0.5, 0.78), w * 0.32, std::f32::consts::PI, std::f32::consts::TAU);
    }
    Icon::Sphere => {
      circle(cen, w * 0.38);
      ell(cen, w * 0.38, w * 0.12);
    }
    Icon::Torus => {
      ell(cen, w * 0.42, w * 0.24);
      ell(cen, w * 0.16, w * 0.07);
    }
    Icon::Pattern => {
      for (x, y) in [(0.1, 0.1), (0.55, 0.1), (0.1, 0.55), (0.55, 0.55)] {
        poly(&[u(x, y), u(x + 0.32, y), u(x + 0.32, y + 0.32), u(x, y + 0.32)], true);
      }
    }
    Icon::CircPattern => {
      for k in 0..6 {
        let t = std::f32::consts::TAU * k as f32 / 6.0;
        p.circle_stroke(cen + vec2(t.cos(), t.sin()) * w * 0.32, w * 0.09, thin);
      }
    }
    Icon::Mirror => {
      poly(&[u(0.1, 0.8), u(0.42, 0.8), u(0.42, 0.25)], true);
      poly(&[u(0.9, 0.8), u(0.58, 0.8), u(0.58, 0.25)], true);
      dashed(p, u(0.5, 0.05), u(0.5, 0.95), thin);
    }
    Icon::Fillet => {
      poly(&[u(0.15, 0.9), u(0.15, 0.45)], false);
      arc(u(0.55, 0.45), w * 0.4, std::f32::consts::FRAC_PI_2, std::f32::consts::PI);
      poly(&[u(0.55, 0.05), u(0.9, 0.05)], false);
    }
    Icon::Chamfer => {
      poly(&[u(0.15, 0.9), u(0.15, 0.45), u(0.55, 0.1), u(0.9, 0.1)], false);
    }
    Icon::Shell => {
      poly(&[u(0.1, 0.2), u(0.1, 0.9), u(0.9, 0.9), u(0.9, 0.2)], false);
      poly(&[u(0.25, 0.2), u(0.25, 0.75), u(0.75, 0.75), u(0.75, 0.2)], false);
    }
    Icon::Combine => {
      circle(u(0.38, 0.5), w * 0.27);
      circle(u(0.62, 0.5), w * 0.27);
    }
    Icon::Move => {
      line(u(0.5, 0.08), u(0.5, 0.92));
      line(u(0.08, 0.5), u(0.92, 0.5));
      for (a, b, d) in [
        ((0.5, 0.08), (0.0, 1.0), 1.0),
        ((0.5, 0.92), (0.0, -1.0), 1.0),
        ((0.08, 0.5), (1.0, 0.0), 1.0),
        ((0.92, 0.5), (-1.0, 0.0), 1.0),
      ] {
        let tip = u(a.0, a.1);
        let dir = vec2(b.0, b.1) * d;
        let side = vec2(-dir.y, dir.x);
        poly(&[tip + (dir + side) * w * 0.14, tip, tip + (dir - side) * w * 0.14], false);
      }
    }
    Icon::Plane => {
      poly(&[u(0.05, 0.75), u(0.3, 0.3), u(0.95, 0.3), u(0.7, 0.75)], true);
    }
    Icon::Measure => {
      poly(&[u(0.05, 0.6), u(0.6, 0.05), u(0.95, 0.4), u(0.4, 0.95)], true);
      for k in 1..4 {
        let t = k as f32 * 0.14;
        line(u(0.05 + t, 0.6 - t), u(0.13 + t, 0.68 - t));
      }
    }
    Icon::Import => {
      line(u(0.5, 0.05), u(0.5, 0.6));
      poly(&[u(0.32, 0.42), u(0.5, 0.6), u(0.68, 0.42)], false);
      poly(&[u(0.1, 0.6), u(0.1, 0.9), u(0.9, 0.9), u(0.9, 0.6)], false);
    }
    Icon::Remove => {
      line(u(0.2, 0.2), u(0.8, 0.8));
      line(u(0.8, 0.2), u(0.2, 0.8));
    }
    Icon::Line => {
      line(u(0.15, 0.85), u(0.85, 0.15));
      dot(u(0.15, 0.85));
      dot(u(0.85, 0.15));
    }
    Icon::Rect => {
      poly(&[u(0.1, 0.25), u(0.9, 0.25), u(0.9, 0.8), u(0.1, 0.8)], true);
      dot(u(0.1, 0.8));
      dot(u(0.9, 0.25));
    }
    Icon::CenterRect => {
      poly(&[u(0.1, 0.25), u(0.9, 0.25), u(0.9, 0.8), u(0.1, 0.8)], true);
      dot(cen + vec2(0.0, w * 0.025));
      dot(u(0.9, 0.25));
    }
    Icon::Circle => {
      circle(cen, w * 0.38);
      dot(cen);
    }
    Icon::Arc3 => {
      arc(u(0.5, 0.8), w * 0.4, 0.0, std::f32::consts::PI);
      dot(u(0.1, 0.8));
      dot(u(0.9, 0.8));
      dot(u(0.5, 0.4));
    }
    Icon::CenterArc => {
      arc(u(0.3, 0.7), w * 0.55, 0.0, std::f32::consts::FRAC_PI_2);
      dot(u(0.3, 0.7));
      thin_line(p, u(0.3, 0.7), u(0.85, 0.7), thin);
    }
    Icon::Polygon => {
      let pts: Vec<Pos2> = (0..6)
        .map(|k| {
          let t = std::f32::consts::TAU * k as f32 / 6.0;
          cen + vec2(t.cos(), t.sin()) * w * 0.4
        })
        .collect();
      poly(&pts, true);
    }
    Icon::Slot => {
      arc(u(0.3, 0.5), w * 0.22, std::f32::consts::FRAC_PI_2, 1.5 * std::f32::consts::PI);
      arc(u(0.7, 0.5), w * 0.22, -std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
      line(u(0.3, 0.28), u(0.7, 0.28));
      line(u(0.3, 0.72), u(0.7, 0.72));
    }
    Icon::Point => {
      p.circle_filled(cen, w * 0.12, c);
    }
    Icon::Dimension => {
      line(u(0.1, 0.25), u(0.1, 0.75));
      line(u(0.9, 0.25), u(0.9, 0.75));
      line(u(0.15, 0.5), u(0.85, 0.5));
      poly(&[u(0.28, 0.38), u(0.13, 0.5), u(0.28, 0.62)], false);
      poly(&[u(0.72, 0.38), u(0.87, 0.5), u(0.72, 0.62)], false);
    }
    Icon::Construction => dashed(p, u(0.1, 0.85), u(0.9, 0.15), s),
    Icon::Coincident => {
      line(u(0.1, 0.8), u(0.5, 0.5));
      line(u(0.5, 0.5), u(0.9, 0.75));
      p.circle_filled(u(0.5, 0.5), w * 0.1, c);
    }
    Icon::Horizontal => {
      line(u(0.1, 0.5), u(0.9, 0.5));
      p.text(u(0.5, 0.22), egui::Align2::CENTER_CENTER, "H", egui::FontId::proportional(w * 0.45), c);
    }
    Icon::Vertical => {
      line(u(0.5, 0.1), u(0.5, 0.9));
      p.text(u(0.25, 0.5), egui::Align2::CENTER_CENTER, "V", egui::FontId::proportional(w * 0.45), c);
    }
    Icon::Parallel => {
      line(u(0.15, 0.8), u(0.55, 0.15));
      line(u(0.45, 0.85), u(0.85, 0.2));
    }
    Icon::Perpendicular => {
      line(u(0.1, 0.85), u(0.9, 0.85));
      line(u(0.5, 0.85), u(0.5, 0.12));
    }
    Icon::Tangent => {
      circle(u(0.5, 0.6), w * 0.28);
      line(u(0.05, 0.32), u(0.95, 0.32));
    }
    Icon::Equal => {
      line(u(0.15, 0.38), u(0.85, 0.38));
      line(u(0.15, 0.62), u(0.85, 0.62));
    }
    Icon::Concentric => {
      circle(cen, w * 0.4);
      circle(cen, w * 0.2);
    }
    Icon::Midpoint => {
      line(u(0.1, 0.7), u(0.9, 0.3));
      p.add(Shape::convex_polygon(
        vec![cen + vec2(0.0, -w * 0.14), cen + vec2(w * 0.12, w * 0.07), cen + vec2(-w * 0.12, w * 0.07)],
        c,
        Stroke::NONE,
      ));
    }
    Icon::Collinear => {
      line(u(0.05, 0.75), u(0.4, 0.55));
      line(u(0.6, 0.45), u(0.95, 0.25));
    }
    Icon::Symmetric => {
      dashed(p, u(0.5, 0.05), u(0.5, 0.95), thin);
      dot(u(0.2, 0.5));
      dot(u(0.8, 0.5));
      line(u(0.2, 0.5), u(0.35, 0.5));
      line(u(0.65, 0.5), u(0.8, 0.5));
    }
    Icon::Fix => {
      poly(&[u(0.2, 0.45), u(0.8, 0.45), u(0.8, 0.9), u(0.2, 0.9)], true);
      arc(u(0.5, 0.45), w * 0.2, 0.0, std::f32::consts::PI);
    }
    Icon::Finish => {
      poly(&[u(0.12, 0.5), u(0.4, 0.8), u(0.9, 0.2)], false);
    }
  }
}

fn dashed(p: &Painter, a: Pos2, b: Pos2, s: Stroke) {
  let n = 6;
  for i in 0..n {
    if i % 2 == 0 {
      let t0 = i as f32 / n as f32;
      let t1 = (i + 1) as f32 / n as f32;
      p.line_segment([a + (b - a) * t0, a + (b - a) * t1], s);
    }
  }
}

fn thin_line(p: &Painter, a: Pos2, b: Pos2, s: Stroke) {
  p.line_segment([a, b], s);
}

/// A toolbar button: icon above a short label (Fusion style). Returns the response.
pub fn tool_button(ui: &mut egui::Ui, icon: Icon, label: &str, hint: &str, active: bool) -> egui::Response {
  let font = egui::FontId::proportional(11.0);
  let text_w = ui.painter().layout_no_wrap(label.to_string(), font.clone(), Color32::BLACK).size().x;
  let size = vec2(text_w.max(30.0) + 10.0, 46.0);
  let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
  let vis = ui.style().interact_selectable(&resp, active);
  if active || resp.hovered() {
    ui.painter().rect_filled(
      rect,
      4.0,
      if active { Color32::from_rgb(205, 225, 248) } else { Color32::from_rgb(228, 232, 238) },
    );
  }
  let ic = Rect::from_center_size(pos2(rect.center().x, rect.min.y + 15.0), Vec2::splat(20.0));
  let col = if active { crate::theme::ACCENT } else { Color32::from_rgb(45, 52, 64) };
  draw(ui.painter(), ic, icon, col);
  ui.painter().text(
    pos2(rect.center().x, rect.max.y - 9.0),
    egui::Align2::CENTER_CENTER,
    label,
    font,
    vis.text_color(),
  );
  resp.on_hover_text(hint)
}

/// A small square icon-only button (constraints, timeline).
pub fn icon_button(
  ui: &mut egui::Ui,
  icon: Icon,
  size: f32,
  hint: &str,
  active: bool,
  color: Color32,
) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
  if active || resp.hovered() {
    ui.painter().rect_filled(
      rect,
      3.0,
      if active { Color32::from_rgb(205, 225, 248) } else { Color32::from_rgb(228, 232, 238) },
    );
  }
  draw(ui.painter(), rect.shrink(size * 0.18), icon, color);
  resp.on_hover_text(hint)
}
