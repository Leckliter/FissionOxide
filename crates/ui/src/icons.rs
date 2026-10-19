//! Toolbar, browser, timeline and chrome icons, drawn in code (no image assets, no font glyph
//! gaps). Icons come in Fusion's colour style (grey solids, a blue highlighted face, orange
//! sketch accents) or in one flat colour for disabled, rolled-back and failing states.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};

use crate::theme;

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
  Params,
  Select,
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
  // application bar
  DataPanel,
  File,
  Save,
  Undo,
  Redo,
  Plus,
  Close,
  Extensions,
  JobStatus,
  Bell,
  Help,
  // browser
  Folder,
  Gear,
  Component,
  Body,
  SketchSmall,
  Eye,
  EyeOff,
  Origin,
  Views,
  // navigation bar
  Orbit,
  LookAt,
  Pan,
  Zoom,
  Fit,
  Display,
  Grid,
  Viewports,
  // timeline
  ToStart,
  StepBack,
  Play,
  StepForward,
  ToEnd,
}

#[derive(Clone, Copy)]
enum Tone {
  Ink,
  Fill,
  Light,
  Dark,
  Blue,
  Orange,
  Green,
  Red,
  White,
}

struct Pen<'a> {
  p: &'a Painter,
  r: Rect,
  mono: Option<Color32>,
  w: f32,
}

impl Pen<'_> {
  fn u(&self, x: f32, y: f32) -> Pos2 {
    pos2(self.r.min.x + x * self.r.width(), self.r.min.y + y * self.r.height())
  }

  fn col(&self, t: Tone) -> Color32 {
    match self.mono {
      Some(c) => match t {
        Tone::Fill | Tone::Dark | Tone::Blue | Tone::Orange | Tone::Green => c.gamma_multiply(0.35),
        Tone::Light => c.gamma_multiply(0.15),
        Tone::White => Color32::from_white_alpha(200),
        Tone::Ink | Tone::Red => c,
      },
      None => match t {
        Tone::Ink => theme::ICON_INK,
        Tone::Fill => theme::ICON_FILL,
        Tone::Light => theme::ICON_FILL_LIGHT,
        Tone::Dark => Color32::from_rgb(150, 154, 160),
        Tone::Blue => theme::ICON_BLUE,
        Tone::Orange => theme::ICON_ORANGE,
        Tone::Green => theme::ICON_GREEN,
        Tone::Red => theme::ERROR,
        Tone::White => Color32::WHITE,
      },
    }
  }

  /// Ink colour for outlines drawn over a tone (blue parts keep a blue outline).
  fn edge(&self, t: Tone) -> Color32 {
    match (self.mono, t) {
      (Some(c), _) => c,
      (None, Tone::Blue) => Color32::from_rgb(20, 100, 175),
      (None, Tone::Orange) => Color32::from_rgb(190, 105, 20),
      (None, Tone::Green) => Color32::from_rgb(40, 125, 40),
      _ => theme::ICON_INK,
    }
  }

  fn pts(&self, pts: &[(f32, f32)]) -> Vec<Pos2> {
    pts.iter().map(|&(x, y)| self.u(x, y)).collect()
  }

  /// A filled convex shape with an outline.
  fn solid(&self, pts: &[(f32, f32)], t: Tone) {
    let v = self.pts(pts);
    self.p.add(Shape::convex_polygon(v.clone(), self.col(t), Stroke::NONE));
    self.p.add(Shape::closed_line(v, Stroke::new(self.w * 0.8, self.edge(t))));
  }

  fn fill(&self, pts: &[(f32, f32)], t: Tone) {
    self.p.add(Shape::convex_polygon(self.pts(pts), self.col(t), Stroke::NONE));
  }

  fn path(&self, pts: &[(f32, f32)], t: Tone) {
    self.p.add(Shape::line(self.pts(pts), Stroke::new(self.w, self.col(t))));
  }

  fn closed(&self, pts: &[(f32, f32)], t: Tone) {
    self.p.add(Shape::closed_line(self.pts(pts), Stroke::new(self.w, self.col(t))));
  }

  fn line(&self, a: (f32, f32), b: (f32, f32), t: Tone) {
    self.p.line_segment([self.u(a.0, a.1), self.u(b.0, b.1)], Stroke::new(self.w, self.col(t)));
  }

  fn thick(&self, a: (f32, f32), b: (f32, f32), t: Tone, k: f32) {
    self.p.line_segment([self.u(a.0, a.1), self.u(b.0, b.1)], Stroke::new(self.w * k, self.col(t)));
  }

  fn dashed(&self, a: (f32, f32), b: (f32, f32), t: Tone) {
    let (a, b) = (self.u(a.0, a.1), self.u(b.0, b.1));
    let n = 7;
    for i in (0..n).step_by(2) {
      let t0 = i as f32 / n as f32;
      let t1 = (i + 1) as f32 / n as f32;
      self.p.line_segment([a + (b - a) * t0, a + (b - a) * t1], Stroke::new(self.w * 0.8, self.col(t)));
    }
  }

  fn circle(&self, c: (f32, f32), rad: f32, fill: Option<Tone>, stroke: Option<Tone>) {
    let c = self.u(c.0, c.1);
    let rad = rad * self.r.width();
    if let Some(f) = fill {
      self.p.circle_filled(c, rad, self.col(f));
    }
    if let Some(s) = stroke {
      let col = if fill.is_some() { self.edge(s) } else { self.col(s) };
      self.p.circle_stroke(c, rad, Stroke::new(if fill.is_some() { self.w * 0.8 } else { self.w }, col));
    }
  }

  fn dot(&self, c: (f32, f32), t: Tone) {
    let rad = (self.r.width() * 0.07).max(1.6);
    self.p.circle_filled(self.u(c.0, c.1), rad, self.col(t));
  }

  /// Arc in unit coordinates, angles counter-clockwise from +x (screen y up).
  fn arc_pts(&self, c: (f32, f32), rx: f32, ry: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    let n = 20;
    (0..=n)
      .map(|i| {
        let t = a0 + (a1 - a0) * i as f32 / n as f32;
        self.u(c.0 + t.cos() * rx, c.1 - t.sin() * ry)
      })
      .collect()
  }

  fn arc(&self, c: (f32, f32), rad: f32, a0: f32, a1: f32, t: Tone) {
    self.p.add(Shape::line(self.arc_pts(c, rad, rad, a0, a1), Stroke::new(self.w, self.col(t))));
  }

  fn ellipse(&self, c: (f32, f32), rx: f32, ry: f32, fill: Option<Tone>, stroke: Tone) {
    let pts = self.arc_pts(c, rx, ry, 0.0, std::f32::consts::TAU);
    if let Some(f) = fill {
      self.p.add(Shape::convex_polygon(pts.clone(), self.col(f), Stroke::NONE));
    }
    self.p.add(Shape::closed_line(pts, Stroke::new(self.w * 0.8, self.edge(stroke))));
  }

  fn arrow_head(&self, tip: (f32, f32), dir: (f32, f32), size: f32, t: Tone) {
    let d = vec2(dir.0, dir.1).normalized();
    let s = vec2(-d.y, d.x);
    let tip = self.u(tip.0, tip.1);
    let k = size * self.r.width();
    self.p.add(Shape::convex_polygon(
      vec![tip, tip - d * k + s * k * 0.6, tip - d * k - s * k * 0.6],
      self.col(t),
      Stroke::NONE,
    ));
  }

  fn text(&self, at: (f32, f32), s: &str, size: f32, t: Tone) {
    self.p.text(
      self.u(at.0, at.1),
      egui::Align2::CENTER_CENTER,
      s,
      egui::FontId::proportional(size * self.r.width()),
      self.col(t),
    );
  }

  /// An isometric block: front, top and side faces.
  fn block(&self, x0: f32, y0: f32, x1: f32, y1: f32, d: f32, top: Tone, front: Tone, side: Tone) {
    self.solid(&[(x0, y0), (x0 + d, y0 - d * 0.7), (x1 + d, y0 - d * 0.7), (x1, y0)], top);
    self.solid(&[(x1, y0), (x1 + d, y0 - d * 0.7), (x1 + d, y1 - d * 0.7), (x1, y1)], side);
    self.solid(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], front);
  }
}

/// Draws `icon` into `r` in one flat colour.
pub fn draw(p: &Painter, r: Rect, icon: Icon, c: Color32) {
  paint(p, r, icon, Some(c));
}

/// Draws `icon` into `r` in Fusion's colour style.
pub fn draw_color(p: &Painter, r: Rect, icon: Icon) {
  paint(p, r, icon, None);
}

fn paint(p: &Painter, r: Rect, icon: Icon, mono: Option<Color32>) {
  use Tone::*;
  use std::f32::consts::{FRAC_PI_2, PI, TAU};
  let w = (r.width() / 16.0).clamp(1.0, 1.8);
  let k = Pen { p, r, mono, w };
  match icon {
    Icon::Sketch => {
      k.solid(&[(0.08, 0.3), (0.72, 0.3), (0.72, 0.92), (0.08, 0.92)], Light);
      k.path(&[(0.2, 0.8), (0.2, 0.48), (0.52, 0.48)], Ink);
      // pencil
      k.p.line_segment([k.u(0.42, 0.7), k.u(0.9, 0.1)], Stroke::new(w * 3.2, k.col(Orange)));
      k.p.line_segment([k.u(0.84, 0.17), k.u(0.9, 0.1)], Stroke::new(w * 3.2, k.col(Ink)));
      k.line((0.42, 0.7), (0.37, 0.78), Ink);
    }
    Icon::Extrude => {
      k.block(0.1, 0.55, 0.68, 0.9, 0.22, Blue, Fill, Dark);
      k.line((0.49, 0.45), (0.49, 0.12), Ink);
      k.arrow_head((0.49, 0.04), (0.0, -1.0), 0.16, Ink);
    }
    Icon::Revolve => {
      k.dashed((0.5, 0.02), (0.5, 0.98), Ink);
      k.solid(&[(0.55, 0.3), (0.75, 0.3), (0.75, 0.7), (0.55, 0.7)], Fill);
      k.p.add(Shape::line(k.arc_pts((0.5, 0.5), 0.42, 0.16, -2.6, 0.2), Stroke::new(w * 1.4, k.col(Blue))));
      k.arrow_head((0.92, 0.47), (0.2, -1.0), 0.14, Blue);
    }
    Icon::Box => k.block(0.1, 0.38, 0.66, 0.9, 0.26, Light, Fill, Dark),
    Icon::Cylinder => {
      k.fill(&[(0.18, 0.25), (0.82, 0.25), (0.82, 0.78), (0.18, 0.78)], Fill);
      k.ellipse((0.5, 0.78), 0.32, 0.12, Some(Fill), Ink);
      k.fill(&[(0.18, 0.25), (0.82, 0.25), (0.82, 0.78), (0.18, 0.78)], Fill);
      k.line((0.18, 0.25), (0.18, 0.78), Ink);
      k.line((0.82, 0.25), (0.82, 0.78), Ink);
      k.ellipse((0.5, 0.25), 0.32, 0.12, Some(Light), Ink);
    }
    Icon::Sphere => {
      k.circle((0.5, 0.5), 0.4, Some(Fill), Some(Ink));
      k.circle((0.38, 0.38), 0.12, Some(Light), None);
      k.p.add(Shape::line(k.arc_pts((0.5, 0.5), 0.4, 0.12, PI, TAU), Stroke::new(w * 0.7, k.col(Ink))));
    }
    Icon::Torus => {
      k.ellipse((0.5, 0.5), 0.44, 0.28, Some(Fill), Ink);
      k.ellipse((0.5, 0.47), 0.18, 0.08, Some(White), Ink);
    }
    Icon::Pattern => {
      for (i, (x, y)) in [(0.06, 0.08), (0.56, 0.08), (0.06, 0.58), (0.56, 0.58)].into_iter().enumerate() {
        k.solid(&[(x, y), (x + 0.36, y), (x + 0.36, y + 0.34), (x, y + 0.34)], if i == 0 { Blue } else { Fill });
      }
    }
    Icon::CircPattern => {
      k.circle((0.5, 0.5), 0.3, None, Some(Light));
      for i in 0..6 {
        let t = TAU * i as f32 / 6.0 - FRAC_PI_2;
        let c = (0.5 + t.cos() * 0.32, 0.5 + t.sin() * 0.32);
        k.circle(c, 0.11, Some(if i == 0 { Blue } else { Fill }), Some(Ink));
      }
    }
    Icon::Mirror => {
      k.solid(&[(0.06, 0.85), (0.42, 0.85), (0.42, 0.2)], Fill);
      k.solid(&[(0.94, 0.85), (0.58, 0.85), (0.58, 0.2)], Blue);
      k.dashed((0.5, 0.04), (0.5, 0.96), Ink);
    }
    Icon::Fillet => {
      let mut v = k.pts(&[(0.92, 0.92), (0.08, 0.92), (0.08, 0.08), (0.5, 0.08)]);
      v.extend(k.arc_pts((0.5, 0.5), 0.42, 0.42, FRAC_PI_2, 0.0));
      k.p.add(Shape::convex_polygon(v.clone(), k.col(Fill), Stroke::NONE));
      k.p.add(Shape::closed_line(v, Stroke::new(w * 0.8, k.col(Ink))));
      k.p.add(Shape::line(k.arc_pts((0.5, 0.5), 0.36, 0.36, FRAC_PI_2, 0.0), Stroke::new(w * 2.6, k.col(Blue))));
    }
    Icon::Chamfer => {
      k.solid(&[(0.08, 0.92), (0.08, 0.45), (0.45, 0.08), (0.92, 0.08), (0.92, 0.92)], Fill);
      k.solid(&[(0.08, 0.45), (0.45, 0.08), (0.58, 0.2), (0.2, 0.58)], Blue);
    }
    Icon::Shell => {
      k.block(0.08, 0.38, 0.7, 0.92, 0.22, Light, Fill, Dark);
      k.solid(&[(0.18, 0.36), (0.28, 0.27), (0.76, 0.27), (0.66, 0.36)], Blue);
    }
    Icon::Combine => {
      k.circle((0.36, 0.5), 0.3, Some(Fill), Some(Ink));
      k.circle((0.64, 0.5), 0.3, Some(Blue), Some(Blue));
      k.p.add(Shape::line(k.arc_pts((0.36, 0.5), 0.3, 0.3, -1.1, 1.1), Stroke::new(w * 0.8, k.col(Ink))));
    }
    Icon::Move => {
      k.solid(&[(0.3, 0.3), (0.7, 0.3), (0.7, 0.7), (0.3, 0.7)], Fill);
      for (tip, dir) in
        [((0.5, 0.02), (0.0, -1.0)), ((0.5, 0.98), (0.0, 1.0)), ((0.02, 0.5), (-1.0, 0.0)), ((0.98, 0.5), (1.0, 0.0))]
      {
        k.line((0.5, 0.5), (tip.0 - dir.0 * 0.1, tip.1 - dir.1 * 0.1), Blue);
        k.arrow_head(tip, dir, 0.16, Blue);
      }
    }
    Icon::Plane => {
      k.solid(&[(0.04, 0.78), (0.3, 0.28), (0.96, 0.28), (0.7, 0.78)], Orange);
      k.line((0.5, 0.53), (0.5, 0.05), Ink);
      k.arrow_head((0.5, 0.02), (0.0, -1.0), 0.12, Ink);
    }
    Icon::Measure => {
      k.solid(&[(0.04, 0.62), (0.62, 0.04), (0.96, 0.38), (0.38, 0.96)], Light);
      for i in 1..6 {
        let t = i as f32 * 0.1;
        let l = if i % 2 == 0 { 0.16 } else { 0.1 };
        k.line((0.04 + t, 0.62 - t), (0.04 + t + l, 0.62 - t + l), Ink);
      }
    }
    Icon::Import => {
      k.solid(&[(0.08, 0.6), (0.92, 0.6), (0.92, 0.92), (0.08, 0.92)], Fill);
      k.thick((0.5, 0.04), (0.5, 0.52), Blue, 1.6);
      k.arrow_head((0.5, 0.68), (0.0, 1.0), 0.2, Blue);
    }
    Icon::Remove => {
      k.thick((0.2, 0.2), (0.8, 0.8), Red, 1.4);
      k.thick((0.8, 0.2), (0.2, 0.8), Red, 1.4);
    }
    Icon::Params => {
      k.solid(&[(0.06, 0.14), (0.94, 0.14), (0.94, 0.86), (0.06, 0.86)], Light);
      k.text((0.5, 0.5), "ƒx", 0.55, Blue);
    }
    Icon::Select => {
      k.solid(&[(0.22, 0.06), (0.22, 0.8), (0.4, 0.64), (0.62, 0.64)], White);
      k.thick((0.36, 0.62), (0.52, 0.94), Ink, 1.6);
      k.closed(&[(0.22, 0.06), (0.22, 0.8), (0.4, 0.64), (0.62, 0.64)], Ink);
    }
    Icon::Line => {
      k.line((0.15, 0.85), (0.85, 0.15), Ink);
      k.dot((0.15, 0.85), Blue);
      k.dot((0.85, 0.15), Blue);
    }
    Icon::Rect => {
      k.closed(&[(0.1, 0.25), (0.9, 0.25), (0.9, 0.8), (0.1, 0.8)], Ink);
      k.dot((0.1, 0.8), Blue);
      k.dot((0.9, 0.25), Blue);
    }
    Icon::CenterRect => {
      k.closed(&[(0.1, 0.25), (0.9, 0.25), (0.9, 0.8), (0.1, 0.8)], Ink);
      k.dot((0.5, 0.525), Blue);
      k.dot((0.9, 0.25), Blue);
    }
    Icon::Circle => {
      k.circle((0.5, 0.5), 0.38, None, Some(Ink));
      k.line((0.5, 0.5), (0.77, 0.23), Ink);
      k.dot((0.5, 0.5), Blue);
    }
    Icon::Arc3 => {
      k.arc((0.5, 0.78), 0.4, 0.0, PI, Ink);
      k.dot((0.1, 0.78), Blue);
      k.dot((0.9, 0.78), Blue);
      k.dot((0.5, 0.38), Blue);
    }
    Icon::CenterArc => {
      k.arc((0.25, 0.75), 0.6, 0.0, FRAC_PI_2, Ink);
      k.dashed((0.25, 0.75), (0.85, 0.75), Ink);
      k.dot((0.25, 0.75), Blue);
    }
    Icon::Polygon => {
      let pts: Vec<(f32, f32)> = (0..6)
        .map(|i| {
          let t = TAU * i as f32 / 6.0;
          (0.5 + t.cos() * 0.4, 0.5 + t.sin() * 0.4)
        })
        .collect();
      k.closed(&pts, Ink);
      k.dot((0.5, 0.5), Blue);
    }
    Icon::Slot => {
      k.arc((0.3, 0.5), 0.22, FRAC_PI_2, 1.5 * PI, Ink);
      k.arc((0.7, 0.5), 0.22, -FRAC_PI_2, FRAC_PI_2, Ink);
      k.line((0.3, 0.28), (0.7, 0.28), Ink);
      k.line((0.3, 0.72), (0.7, 0.72), Ink);
      k.dot((0.3, 0.5), Blue);
      k.dot((0.7, 0.5), Blue);
    }
    Icon::Point => {
      k.circle((0.5, 0.5), 0.14, Some(Blue), Some(Blue));
      k.line((0.5, 0.15), (0.5, 0.3), Ink);
      k.line((0.5, 0.7), (0.5, 0.85), Ink);
      k.line((0.15, 0.5), (0.3, 0.5), Ink);
      k.line((0.7, 0.5), (0.85, 0.5), Ink);
    }
    Icon::Dimension => {
      k.line((0.1, 0.55), (0.1, 0.92), Ink);
      k.line((0.9, 0.55), (0.9, 0.92), Ink);
      k.line((0.14, 0.75), (0.86, 0.75), Ink);
      k.arrow_head((0.11, 0.75), (-1.0, 0.0), 0.14, Ink);
      k.arrow_head((0.89, 0.75), (1.0, 0.0), 0.14, Ink);
      k.text((0.5, 0.36), "10", 0.42, Blue);
    }
    Icon::Construction => {
      k.dashed((0.1, 0.85), (0.9, 0.15), Orange);
      k.dot((0.1, 0.85), Ink);
      k.dot((0.9, 0.15), Ink);
    }
    Icon::Coincident => {
      k.line((0.08, 0.82), (0.5, 0.5), Ink);
      k.line((0.5, 0.5), (0.92, 0.78), Ink);
      k.circle((0.5, 0.5), 0.12, Some(Blue), Some(Blue));
    }
    Icon::Horizontal => {
      k.line((0.1, 0.62), (0.9, 0.62), Ink);
      k.dot((0.1, 0.62), Blue);
      k.dot((0.9, 0.62), Blue);
      k.text((0.5, 0.28), "H", 0.4, Blue);
    }
    Icon::Vertical => {
      k.line((0.62, 0.1), (0.62, 0.9), Ink);
      k.dot((0.62, 0.1), Blue);
      k.dot((0.62, 0.9), Blue);
      k.text((0.28, 0.5), "V", 0.4, Blue);
    }
    Icon::Parallel => {
      k.line((0.12, 0.8), (0.52, 0.12), Ink);
      k.line((0.48, 0.88), (0.88, 0.2), Blue);
    }
    Icon::Perpendicular => {
      k.line((0.1, 0.85), (0.9, 0.85), Ink);
      k.line((0.5, 0.85), (0.5, 0.12), Blue);
      k.closed(&[(0.5, 0.85), (0.5, 0.68), (0.67, 0.68), (0.67, 0.85)], Ink);
    }
    Icon::Tangent => {
      k.circle((0.5, 0.62), 0.28, None, Some(Ink));
      k.line((0.04, 0.34), (0.96, 0.34), Blue);
    }
    Icon::Equal => {
      k.thick((0.15, 0.38), (0.85, 0.38), Blue, 1.4);
      k.thick((0.15, 0.62), (0.85, 0.62), Blue, 1.4);
    }
    Icon::Concentric => {
      k.circle((0.5, 0.5), 0.4, None, Some(Ink));
      k.circle((0.5, 0.5), 0.2, None, Some(Blue));
      k.dot((0.5, 0.5), Ink);
    }
    Icon::Midpoint => {
      k.line((0.08, 0.72), (0.92, 0.28), Ink);
      k.fill(&[(0.5, 0.33), (0.62, 0.57), (0.38, 0.57)], Blue);
    }
    Icon::Collinear => {
      k.line((0.05, 0.78), (0.4, 0.57), Ink);
      k.line((0.6, 0.45), (0.95, 0.24), Blue);
      k.dashed((0.4, 0.57), (0.6, 0.45), Ink);
    }
    Icon::Symmetric => {
      k.dashed((0.5, 0.04), (0.5, 0.96), Ink);
      k.dot((0.18, 0.5), Blue);
      k.dot((0.82, 0.5), Blue);
      k.line((0.24, 0.5), (0.38, 0.5), Ink);
      k.line((0.62, 0.5), (0.76, 0.5), Ink);
    }
    Icon::Fix => {
      k.arc((0.5, 0.45), 0.2, 0.0, PI, Ink);
      k.solid(&[(0.2, 0.45), (0.8, 0.45), (0.8, 0.9), (0.2, 0.9)], Orange);
    }
    Icon::Finish => {
      k.p.add(Shape::line(k.pts(&[(0.1, 0.52), (0.38, 0.8), (0.92, 0.2)]), Stroke::new(w * 2.4, k.col(Green))));
    }
    Icon::DataPanel => {
      for i in 0..3 {
        for j in 0..3 {
          let (x, y) = (0.12 + i as f32 * 0.29, 0.12 + j as f32 * 0.29);
          k.fill(&[(x, y), (x + 0.18, y), (x + 0.18, y + 0.18), (x, y + 0.18)], Ink);
        }
      }
    }
    Icon::File => {
      k.solid(&[(0.18, 0.06), (0.62, 0.06), (0.82, 0.26), (0.82, 0.94), (0.18, 0.94)], White);
      k.path(&[(0.62, 0.06), (0.62, 0.26), (0.82, 0.26)], Ink);
      k.line((0.3, 0.5), (0.7, 0.5), Ink);
      k.line((0.3, 0.66), (0.7, 0.66), Ink);
      k.line((0.3, 0.82), (0.58, 0.82), Ink);
    }
    Icon::Save => {
      k.solid(&[(0.1, 0.1), (0.76, 0.1), (0.9, 0.24), (0.9, 0.9), (0.1, 0.9)], Ink);
      k.fill(&[(0.26, 0.1), (0.66, 0.1), (0.66, 0.36), (0.26, 0.36)], White);
      k.fill(&[(0.24, 0.56), (0.76, 0.56), (0.76, 0.9), (0.24, 0.9)], White);
    }
    Icon::Undo => {
      k.p
        .add(Shape::line(k.arc_pts((0.52, 0.58), 0.32, 0.32, PI * 0.95, -PI * 0.25), Stroke::new(w * 1.2, k.col(Ink))));
      k.arrow_head((0.18, 0.62), (-0.1, 1.0), 0.2, Ink);
    }
    Icon::Redo => {
      k.p.add(Shape::line(k.arc_pts((0.48, 0.58), 0.32, 0.32, PI * 0.05, PI * 1.25), Stroke::new(w * 1.2, k.col(Ink))));
      k.arrow_head((0.82, 0.62), (0.1, 1.0), 0.2, Ink);
    }
    Icon::Plus => {
      k.thick((0.5, 0.15), (0.5, 0.85), Ink, 1.2);
      k.thick((0.15, 0.5), (0.85, 0.5), Ink, 1.2);
    }
    Icon::Close => {
      k.line((0.25, 0.25), (0.75, 0.75), Ink);
      k.line((0.75, 0.25), (0.25, 0.75), Ink);
    }
    Icon::Extensions => {
      k.solid(&[(0.12, 0.3), (0.72, 0.3), (0.72, 0.9), (0.12, 0.9)], White);
      k.circle((0.42, 0.24), 0.11, Some(White), Some(Ink));
      k.circle((0.8, 0.6), 0.11, Some(White), Some(Ink));
      k.fill(&[(0.32, 0.31), (0.52, 0.31), (0.52, 0.36), (0.32, 0.36)], White);
      k.fill(&[(0.66, 0.5), (0.71, 0.5), (0.71, 0.7), (0.66, 0.7)], White);
    }
    Icon::JobStatus => {
      k.circle((0.5, 0.5), 0.4, None, Some(Ink));
      k.path(&[(0.3, 0.52), (0.45, 0.66), (0.72, 0.36)], Ink);
    }
    Icon::Bell => {
      k.p.add(Shape::line(k.arc_pts((0.5, 0.45), 0.26, 0.26, 0.0, PI), Stroke::new(w, k.col(Ink))));
      k.path(&[(0.24, 0.45), (0.24, 0.66), (0.12, 0.78), (0.88, 0.78), (0.76, 0.66), (0.76, 0.45)], Ink);
      k.arc((0.5, 0.82), 0.1, PI, TAU, Ink);
    }
    Icon::Help => {
      k.circle((0.5, 0.5), 0.42, None, Some(Ink));
      k.text((0.5, 0.52), "?", 0.6, Ink);
    }
    Icon::Folder => {
      let (back, front) = if mono.is_some() { (Fill, Light) } else { (Dark, Fill) };
      k.solid(&[(0.06, 0.2), (0.4, 0.2), (0.48, 0.3), (0.94, 0.3), (0.94, 0.86), (0.06, 0.86)], back);
      k.solid(&[(0.06, 0.4), (0.94, 0.4), (0.94, 0.86), (0.06, 0.86)], front);
    }
    Icon::Gear => {
      for i in 0..8 {
        let t = TAU * i as f32 / 8.0;
        k.thick((0.5 + t.cos() * 0.26, 0.5 + t.sin() * 0.26), (0.5 + t.cos() * 0.44, 0.5 + t.sin() * 0.44), Ink, 2.2);
      }
      k.circle((0.5, 0.5), 0.3, Some(Fill), Some(Ink));
      k.circle((0.5, 0.5), 0.12, Some(White), Some(Ink));
    }
    Icon::Component => {
      k.block(0.1, 0.38, 0.66, 0.9, 0.26, White, White, Light);
      k.circle((0.38, 0.64), 0.1, Some(Blue), None);
    }
    Icon::Body => k.block(0.1, 0.38, 0.66, 0.9, 0.26, Light, Fill, Dark),
    Icon::SketchSmall => {
      k.solid(&[(0.08, 0.2), (0.92, 0.2), (0.92, 0.84), (0.08, 0.84)], White);
      k.path(&[(0.22, 0.7), (0.22, 0.36), (0.54, 0.36)], Red);
      k.circle((0.64, 0.6), 0.12, None, Some(Red));
    }
    Icon::Eye => {
      k.ellipse((0.5, 0.5), 0.46, 0.28, Some(White), Ink);
      k.circle((0.5, 0.5), 0.16, Some(Ink), None);
    }
    Icon::EyeOff => {
      k.ellipse((0.5, 0.5), 0.46, 0.28, None, Fill);
      k.line((0.14, 0.86), (0.86, 0.14), Ink);
    }
    Icon::Origin => {
      k.line((0.3, 0.75), (0.95, 0.75), Red);
      k.line((0.3, 0.75), (0.3, 0.08), Blue);
      k.line((0.3, 0.75), (0.05, 0.95), Green);
      k.dot((0.3, 0.75), Ink);
    }
    Icon::Views => {
      k.solid(&[(0.08, 0.24), (0.7, 0.24), (0.7, 0.8), (0.08, 0.8)], White);
      k.fill(&[(0.7, 0.42), (0.94, 0.26), (0.94, 0.78), (0.7, 0.62)], Ink);
    }
    Icon::Orbit => {
      k.ellipse((0.5, 0.5), 0.42, 0.18, None, Ink);
      k.p.add(Shape::line(k.arc_pts((0.5, 0.5), 0.18, 0.42, 0.2, PI - 0.2), Stroke::new(w, k.col(Ink))));
      k.circle((0.5, 0.5), 0.12, Some(Blue), None);
      k.arrow_head((0.9, 0.42), (0.2, -1.0), 0.14, Ink);
    }
    Icon::LookAt => {
      k.solid(&[(0.1, 0.62), (0.32, 0.36), (0.92, 0.36), (0.7, 0.62)], Light);
      k.thick((0.5, 0.06), (0.5, 0.4), Blue, 1.2);
      k.arrow_head((0.5, 0.5), (0.0, 1.0), 0.16, Blue);
    }
    Icon::Pan => {
      for (tip, dir) in
        [((0.5, 0.04), (0.0, -1.0)), ((0.5, 0.96), (0.0, 1.0)), ((0.04, 0.5), (-1.0, 0.0)), ((0.96, 0.5), (1.0, 0.0))]
      {
        k.line((0.5, 0.5), (tip.0 - dir.0 * 0.1, tip.1 - dir.1 * 0.1), Ink);
        k.arrow_head(tip, dir, 0.16, Ink);
      }
    }
    Icon::Zoom => {
      k.circle((0.42, 0.42), 0.3, Some(White), Some(Ink));
      k.thick((0.64, 0.64), (0.92, 0.92), Ink, 2.0);
      k.line((0.28, 0.42), (0.56, 0.42), Ink);
      k.line((0.42, 0.28), (0.42, 0.56), Ink);
    }
    Icon::Fit => {
      k.circle((0.42, 0.42), 0.3, Some(White), Some(Ink));
      k.thick((0.64, 0.64), (0.92, 0.92), Ink, 2.0);
      k.solid(&[(0.3, 0.3), (0.54, 0.3), (0.54, 0.54), (0.3, 0.54)], Blue);
    }
    Icon::Display => {
      k.solid(&[(0.06, 0.12), (0.94, 0.12), (0.94, 0.72), (0.06, 0.72)], White);
      k.line((0.5, 0.72), (0.5, 0.88), Ink);
      k.line((0.3, 0.9), (0.7, 0.9), Ink);
      k.block(0.32, 0.4, 0.58, 0.64, 0.12, Light, Blue, Fill);
    }
    Icon::Grid => {
      for i in 0..4 {
        let t = 0.1 + i as f32 * 0.267;
        k.line((t, 0.1), (t, 0.9), Ink);
        k.line((0.1, t), (0.9, t), Ink);
      }
    }
    Icon::Viewports => {
      k.solid(&[(0.08, 0.12), (0.92, 0.12), (0.92, 0.88), (0.08, 0.88)], White);
      k.line((0.5, 0.12), (0.5, 0.88), Ink);
      k.line((0.08, 0.5), (0.92, 0.5), Ink);
    }
    Icon::ToStart => {
      k.thick((0.22, 0.2), (0.22, 0.8), Ink, 1.4);
      k.fill(&[(0.3, 0.5), (0.78, 0.2), (0.78, 0.8)], Ink);
    }
    Icon::StepBack => {
      k.fill(&[(0.22, 0.5), (0.72, 0.2), (0.72, 0.8)], Ink);
    }
    Icon::Play => {
      k.fill(&[(0.28, 0.2), (0.28, 0.8), (0.8, 0.5)], Ink);
    }
    Icon::StepForward => {
      k.fill(&[(0.28, 0.2), (0.28, 0.8), (0.78, 0.5)], Ink);
    }
    Icon::ToEnd => {
      k.fill(&[(0.22, 0.2), (0.22, 0.8), (0.7, 0.5)], Ink);
      k.thick((0.78, 0.2), (0.78, 0.8), Ink, 1.4);
    }
  }
}

/// A large toolbar icon (Fusion shows toolbar commands as icons only; the name is the tooltip).
pub fn tool_button(ui: &mut egui::Ui, icon: Icon, label: &str, hint: &str, active: bool) -> egui::Response {
  let size = vec2(36.0, 36.0);
  let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
  if active || resp.hovered() {
    let (fill, stroke) = if active { (theme::SELECTED, theme::ACCENT) } else { (theme::HOVER, theme::SELECTED) };
    ui.painter().rect(rect, 2.0, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
  }
  let ic = Rect::from_center_size(rect.center(), Vec2::splat(26.0));
  draw_color(ui.painter(), ic, icon);
  resp.on_hover_ui(|ui| {
    ui.label(egui::RichText::new(label).strong());
    ui.label(egui::RichText::new(hint).color(theme::MUTED));
  })
}

/// A small square icon-only button (constraints, timeline, chrome). `color` None draws the
/// icon in colour.
pub fn icon_button(
  ui: &mut egui::Ui,
  icon: Icon,
  size: f32,
  hint: &str,
  active: bool,
  color: Option<Color32>,
) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
  if active || resp.hovered() {
    let (fill, stroke) = if active { (theme::SELECTED, theme::ACCENT) } else { (theme::HOVER, theme::SELECTED) };
    ui.painter().rect(rect, 2.0, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
  }
  let ir = rect.shrink(size * 0.18);
  match color {
    Some(c) => draw(ui.painter(), ir, icon, c),
    None => draw_color(ui.painter(), ir, icon),
  }
  if hint.is_empty() { resp } else { resp.on_hover_text(hint) }
}

/// A row in a drop-down menu: small icon then the command name.
pub fn menu_item(ui: &mut egui::Ui, icon: Icon, label: &str) -> egui::Response {
  let w = ui.available_width().max(190.0);
  let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), egui::Sense::click());
  if resp.hovered() {
    ui.painter().rect_filled(rect, 0.0, theme::HOVER);
  }
  draw_color(ui.painter(), Rect::from_min_size(rect.min + vec2(6.0, 4.0), Vec2::splat(18.0)), icon);
  ui.painter().text(
    rect.left_center() + vec2(32.0, 0.0),
    egui::Align2::LEFT_CENTER,
    label,
    egui::FontId::proportional(12.5),
    theme::TEXT,
  );
  resp
}

/// A small downward chevron.
pub fn chevron(p: &Painter, c: Pos2, color: Color32) {
  p.add(Shape::convex_polygon(
    vec![c + vec2(-3.5, -1.5), c + vec2(3.5, -1.5), c + vec2(0.0, 2.5)],
    color,
    Stroke::NONE,
  ));
}
