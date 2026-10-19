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
  // full toolbar menus
  PressPull,
  Draft,
  Scale,
  OffsetFace,
  ReplaceFace,
  SplitFace,
  SplitBody,
  SilhouetteSplit,
  Align,
  RemoveBody,
  PhysMaterial,
  Appearance,
  ManageMaterials,
  ComputeAll,
  Bom,
  CreateForm,
  Derive,
  Sweep,
  Loft,
  Rib,
  Web,
  Emboss,
  Hole,
  Thread,
  Coil,
  Pipe,
  Thicken,
  BoundaryFill,
  BaseFeature,
  JointOrigin,
  Configure,
  ConfigTable,
  ConfigRules,
  Ucs,
  PlaneAlt,
  Axis,
  CPoint,
  Interference,
  Section,
  CurvComb,
  CurvMap,
  DraftAnalysis,
  EnvMap,
  Isocurve,
  Zebra,
  Accessibility,
  CenterOfMass,
  MinRadius,
  Navigator,
  ComponentColors,
  FaceGroups,
  History,
  Sustainability,
  Decal,
  Canvas,
  InsertSvg,
  InsertDxf,
  InsertMesh,
  InsertComponent,
  Fastener,
  DuplicateJoints,
  McMaster,
  Manufacturer,
  AddToAssembly,
  WindowSel,
  FreeformSel,
  PaintSel,
  Patch,
  Ruled,
  SurfOffset,
  Trim,
  Untrim,
  Extend,
  Stitch,
  Unstitch,
  ReverseNormal,
  Tessellate,
  MeshSection,
  BaseMesh,
  Repair,
  GenFaceGroups,
  CombineFaceGroups,
  DirectEdit,
  Remesh,
  Reduce,
  PlaneCut,
  Smooth,
  Separate,
  ScaleMesh,
  ConvertMesh,
  Flange,
  Hem,
  LoftedFlange,
  Fold,
  ConvertSheet,
  FlatPattern,
  Unfold,
  SmRules,
  CornerClosure,
  Rip,
  Print3d,
  Scripts,
  Marketplace,
  Assistant,
  Gauge,
  Clock,
}

#[derive(Clone, Copy)]
enum Tone {
  Ink,
  Fill,
  Light,
  Dark,
  Blue,
  BlueLight,
  BlueDark,
  Orange,
  Green,
  Red,
  White,
  Purple,
  PurpleLight,
}

/// Colour family of an icon's solids: blue for solid modelling, orange for surfaces and gold
/// for meshes, as in Fusion's toolbars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tint {
  Blue,
  Orange,
  Gold,
}

struct Pen<'a> {
  p: &'a Painter,
  r: Rect,
  mono: Option<Color32>,
  tint: Tint,
  w: f32,
}

impl Pen<'_> {
  fn u(&self, x: f32, y: f32) -> Pos2 {
    pos2(self.r.min.x + x * self.r.width(), self.r.min.y + y * self.r.height())
  }

  fn col(&self, t: Tone) -> Color32 {
    match self.mono {
      Some(c) => match t {
        Tone::Fill | Tone::Dark | Tone::Blue | Tone::BlueDark | Tone::Orange | Tone::Green => c.gamma_multiply(0.35),
        Tone::BlueLight => c.gamma_multiply(0.2),
        Tone::Light => c.gamma_multiply(0.15),
        Tone::White => Color32::from_white_alpha(200),
        Tone::Purple => c.gamma_multiply(0.35),
        Tone::PurpleLight => c.gamma_multiply(0.2),
        Tone::Ink | Tone::Red => c,
      },
      None => match t {
        Tone::Ink => theme::ICON_INK,
        Tone::Fill => theme::ICON_FILL,
        Tone::Light => theme::ICON_FILL_LIGHT,
        Tone::Dark => Color32::from_rgb(150, 154, 160),
        Tone::Blue => match self.tint {
          Tint::Blue => theme::ICON_BLUE,
          Tint::Orange => Color32::from_rgb(240, 138, 48),
          Tint::Gold => Color32::from_rgb(222, 178, 64),
        },
        Tone::BlueLight => match self.tint {
          Tint::Blue => theme::ICON_BLUE_LIGHT,
          Tint::Orange => Color32::from_rgb(250, 192, 128),
          Tint::Gold => Color32::from_rgb(242, 216, 136),
        },
        Tone::BlueDark => match self.tint {
          Tint::Blue => theme::ICON_BLUE_DARK,
          Tint::Orange => Color32::from_rgb(196, 98, 24),
          Tint::Gold => Color32::from_rgb(168, 126, 36),
        },
        Tone::Orange => theme::ICON_ORANGE,
        Tone::Green => theme::ICON_GREEN,
        Tone::Red => theme::ICON_RED,
        Tone::White => Color32::WHITE,
        Tone::Purple => Color32::from_rgb(150, 96, 214),
        Tone::PurpleLight => Color32::from_rgb(204, 172, 240),
      },
    }
  }

  /// Ink colour for outlines drawn over a tone (blue parts keep a blue outline).
  fn edge(&self, t: Tone) -> Color32 {
    match (self.mono, t) {
      (Some(c), _) => c,
      (None, Tone::Blue | Tone::BlueLight | Tone::BlueDark) => match self.tint {
        Tint::Blue => Color32::from_rgb(16, 84, 160),
        Tint::Orange => Color32::from_rgb(170, 84, 20),
        Tint::Gold => Color32::from_rgb(140, 104, 30),
      },
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
  paint(p, r, icon, Some(c), Tint::Blue);
}

/// Draws `icon` in colour with its solids in the `tint` family.
pub fn draw_tinted(p: &Painter, r: Rect, icon: Icon, tint: Tint) {
  paint(p, r, icon, None, tint);
}

/// Draws `icon` into `r` in Fusion's colour style.
pub fn draw_color(p: &Painter, r: Rect, icon: Icon) {
  paint(p, r, icon, None, Tint::Blue);
}

fn paint(p: &Painter, r: Rect, icon: Icon, mono: Option<Color32>, tint: Tint) {
  use Tone::*;
  use std::f32::consts::{FRAC_PI_2, PI, TAU};
  let w = (r.width() / 16.0).clamp(1.0, 1.8);
  let k = Pen { p, r, mono, tint, w };
  match icon {
    Icon::Sketch => {
      k.solid(&[(0.08, 0.14), (0.78, 0.14), (0.78, 0.78), (0.08, 0.78)], White);
      for (x, y) in [(0.08, 0.14), (0.78, 0.14), (0.08, 0.78)] {
        k.fill(&[(x - 0.07, y - 0.07), (x + 0.07, y - 0.07), (x + 0.07, y + 0.07), (x - 0.07, y + 0.07)], Ink);
      }
      k.thick((0.78, 0.58), (0.78, 0.98), Green, 2.2);
      k.thick((0.58, 0.78), (0.98, 0.78), Green, 2.2);
    }
    Icon::Extrude => {
      k.block(0.12, 0.34, 0.62, 0.94, 0.26, BlueLight, Blue, BlueDark);
      k.thick((0.37, 0.3), (0.37, 0.04), Ink, 1.0);
      k.arrow_head((0.37, 0.0), (0.0, -1.0), 0.13, Ink);
    }
    Icon::Revolve => {
      k.p.add(Shape::line(k.arc_pts((0.5, 0.56), 0.36, 0.3, -2.9, 0.1), Stroke::new(w * 5.0, k.col(Blue))));
      k.p.add(Shape::line(k.arc_pts((0.5, 0.44), 0.36, 0.3, -2.9, 0.1), Stroke::new(w * 3.0, k.col(BlueLight))));
      k.dashed((0.5, 0.02), (0.5, 0.98), Ink);
    }
    Icon::Box => k.block(0.1, 0.38, 0.66, 0.9, 0.26, BlueLight, Blue, BlueDark),
    Icon::Cylinder => {
      k.ellipse((0.5, 0.78), 0.32, 0.12, Some(Blue), Blue);
      k.fill(&[(0.18, 0.25), (0.82, 0.25), (0.82, 0.78), (0.18, 0.78)], Blue);
      k.line((0.18, 0.25), (0.18, 0.78), BlueDark);
      k.line((0.82, 0.25), (0.82, 0.78), BlueDark);
      k.ellipse((0.5, 0.25), 0.32, 0.12, Some(BlueLight), Blue);
    }
    Icon::Sphere => {
      k.circle((0.5, 0.5), 0.4, Some(Blue), Some(Blue));
      k.circle((0.38, 0.38), 0.13, Some(BlueLight), None);
    }
    Icon::Torus => {
      k.ellipse((0.5, 0.5), 0.44, 0.28, Some(Blue), Blue);
      k.ellipse((0.5, 0.47), 0.18, 0.08, Some(White), Blue);
    }
    Icon::Pattern => {
      for (i, (x, y)) in [(0.06, 0.08), (0.56, 0.08), (0.06, 0.58), (0.56, 0.58)].into_iter().enumerate() {
        k.solid(&[(x, y), (x + 0.36, y), (x + 0.36, y + 0.34), (x, y + 0.34)], if i == 0 { BlueDark } else { Blue });
      }
    }
    Icon::CircPattern => {
      k.circle((0.5, 0.5), 0.3, None, Some(Light));
      for i in 0..6 {
        let t = TAU * i as f32 / 6.0 - FRAC_PI_2;
        let c = (0.5 + t.cos() * 0.32, 0.5 + t.sin() * 0.32);
        k.circle(c, 0.11, Some(if i == 0 { BlueDark } else { Blue }), Some(Blue));
      }
    }
    Icon::Mirror => {
      k.solid(&[(0.06, 0.85), (0.42, 0.85), (0.42, 0.2)], BlueLight);
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
      k.block(0.06, 0.4, 0.5, 0.88, 0.2, BlueLight, Blue, BlueDark);
      k.block(0.36, 0.22, 0.78, 0.66, 0.2, BlueLight, Blue, BlueDark);
    }
    Icon::Move => {
      for (tip, dir) in
        [((0.5, 0.02), (0.0, -1.0)), ((0.5, 0.98), (0.0, 1.0)), ((0.02, 0.5), (-1.0, 0.0)), ((0.98, 0.5), (1.0, 0.0))]
      {
        k.thick((0.5, 0.5), (tip.0 - dir.0 * 0.12, tip.1 - dir.1 * 0.12), Ink, 1.6);
        k.arrow_head(tip, dir, 0.2, Ink);
      }
    }
    Icon::Plane => {
      k.solid(&[(0.06, 0.82), (0.06, 0.28), (0.4, 0.08), (0.4, 0.62)], Green);
      k.solid(&[(0.5, 0.92), (0.5, 0.38), (0.86, 0.18), (0.86, 0.72)], Orange);
    }
    Icon::Measure => {
      k.solid(&[(0.04, 0.44), (0.96, 0.44), (0.96, 0.72), (0.04, 0.72)], Orange);
      for i in 1..8 {
        let x = 0.04 + i as f32 * 0.115;
        k.line((x, 0.44), (x, if i % 2 == 0 { 0.6 } else { 0.54 }), Ink);
      }
      k.line((0.04, 0.24), (0.96, 0.24), Blue);
      k.line((0.04, 0.14), (0.04, 0.34), Blue);
      k.line((0.96, 0.14), (0.96, 0.34), Blue);
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
      for (a, b) in [
        ((0.06, 0.06), (0.78, 0.06)),
        ((0.06, 0.06), (0.06, 0.7)),
        ((0.78, 0.06), (0.78, 0.4)),
        ((0.06, 0.7), (0.4, 0.7)),
      ] {
        k.dashed(a, b, Green);
      }
      k.solid(&[(0.46, 0.34), (0.46, 0.98), (0.62, 0.84), (0.82, 0.84)], White);
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
      k.circle((0.5, 0.5), 0.12, Some(Red), Some(Red));
    }
    Icon::Horizontal => {
      k.line((0.1, 0.62), (0.9, 0.62), Ink);
      k.dot((0.1, 0.62), Red);
      k.dot((0.9, 0.62), Red);
      k.text((0.5, 0.28), "H", 0.4, Red);
    }
    Icon::Vertical => {
      k.line((0.62, 0.1), (0.62, 0.9), Ink);
      k.dot((0.62, 0.1), Red);
      k.dot((0.62, 0.9), Red);
      k.text((0.28, 0.5), "V", 0.4, Red);
    }
    Icon::Parallel => {
      k.line((0.12, 0.8), (0.52, 0.12), Ink);
      k.line((0.48, 0.88), (0.88, 0.2), Red);
    }
    Icon::Perpendicular => {
      k.line((0.1, 0.85), (0.9, 0.85), Ink);
      k.line((0.5, 0.85), (0.5, 0.12), Red);
      k.closed(&[(0.5, 0.85), (0.5, 0.68), (0.67, 0.68), (0.67, 0.85)], Ink);
    }
    Icon::Tangent => {
      k.circle((0.5, 0.62), 0.28, None, Some(Ink));
      k.line((0.04, 0.34), (0.96, 0.34), Red);
    }
    Icon::Equal => {
      k.thick((0.15, 0.38), (0.85, 0.38), Red, 1.4);
      k.thick((0.15, 0.62), (0.85, 0.62), Red, 1.4);
    }
    Icon::Concentric => {
      k.circle((0.5, 0.5), 0.4, None, Some(Ink));
      k.circle((0.5, 0.5), 0.2, None, Some(Red));
      k.dot((0.5, 0.5), Ink);
    }
    Icon::Midpoint => {
      k.line((0.08, 0.72), (0.92, 0.28), Ink);
      k.fill(&[(0.5, 0.33), (0.62, 0.57), (0.38, 0.57)], Red);
    }
    Icon::Collinear => {
      k.line((0.05, 0.78), (0.4, 0.57), Ink);
      k.line((0.6, 0.45), (0.95, 0.24), Red);
      k.dashed((0.4, 0.57), (0.6, 0.45), Ink);
    }
    Icon::Symmetric => {
      k.dashed((0.5, 0.04), (0.5, 0.96), Ink);
      k.dot((0.18, 0.5), Red);
      k.dot((0.82, 0.5), Red);
      k.line((0.24, 0.5), (0.38, 0.5), Ink);
      k.line((0.62, 0.5), (0.76, 0.5), Ink);
    }
    Icon::Fix => {
      k.arc((0.5, 0.45), 0.2, 0.0, PI, Ink);
      k.solid(&[(0.2, 0.45), (0.8, 0.45), (0.8, 0.9), (0.2, 0.9)], Orange);
    }
    Icon::Finish => {
      k.circle((0.5, 0.5), 0.46, Some(Green), None);
      k.p.add(Shape::line(k.pts(&[(0.27, 0.52), (0.44, 0.68), (0.74, 0.34)]), Stroke::new(w * 2.2, k.col(White))));
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
    Icon::PressPull => {
      k.block(0.08, 0.36, 0.5, 0.92, 0.22, BlueLight, Blue, BlueDark);
      k.thick((0.62, 0.62), (0.88, 0.62), Ink, 1.2);
      k.arrow_head((0.98, 0.62), (1.0, 0.0), 0.16, Ink);
    }
    Icon::Draft => {
      k.solid(&[(0.24, 0.1), (0.76, 0.1), (0.92, 0.9), (0.08, 0.9)], Fill);
      k.solid(&[(0.24, 0.1), (0.4, 0.1), (0.3, 0.9), (0.08, 0.9)], Blue);
    }
    Icon::Scale => {
      k.solid(&[(0.06, 0.06), (0.94, 0.06), (0.94, 0.94), (0.06, 0.94)], White);
      k.solid(&[(0.06, 0.06), (0.52, 0.06), (0.52, 0.52), (0.06, 0.52)], Blue);
    }
    Icon::OffsetFace => {
      k.solid(&[(0.06, 0.06), (0.94, 0.06), (0.94, 0.94), (0.06, 0.94)], White);
      k.solid(&[(0.24, 0.24), (0.76, 0.24), (0.76, 0.76), (0.24, 0.76)], BlueLight);
    }
    Icon::ReplaceFace => {
      k.block(0.1, 0.42, 0.66, 0.92, 0.24, Blue, White, Light);
      k.thick((0.42, 0.62), (0.42, 0.22), Ink, 1.2);
      k.arrow_head((0.42, 0.12), (0.0, -1.0), 0.16, Ink);
    }
    Icon::SplitFace => {
      k.solid(&[(0.08, 0.1), (0.92, 0.1), (0.92, 0.44), (0.08, 0.44)], Blue);
      k.solid(&[(0.08, 0.54), (0.92, 0.54), (0.92, 0.9), (0.08, 0.9)], White);
    }
    Icon::SplitBody => {
      k.block(0.08, 0.62, 0.74, 0.92, 0.2, Light, Fill, Dark);
      k.block(0.08, 0.26, 0.74, 0.5, 0.2, BlueLight, Blue, BlueDark);
    }
    Icon::SilhouetteSplit => {
      k.circle((0.5, 0.5), 0.42, Some(Blue), None);
      k.p.add(Shape::convex_polygon(k.arc_pts((0.5, 0.5), 0.42, 0.42, PI, TAU), k.col(Light), Stroke::NONE));
      k.ellipse((0.5, 0.5), 0.42, 0.12, None, Ink);
    }
    Icon::Align => {
      k.thick((0.1, 0.06), (0.1, 0.94), Ink, 1.2);
      k.solid(&[(0.14, 0.18), (0.6, 0.18), (0.6, 0.42), (0.14, 0.42)], Blue);
      k.solid(&[(0.14, 0.56), (0.9, 0.56), (0.9, 0.8), (0.14, 0.8)], BlueLight);
    }
    Icon::RemoveBody => {
      k.circle((0.76, 0.5), 0.17, Some(Blue), None);
      k.thick((0.6, 0.5), (0.2, 0.5), Blue, 2.4);
      k.arrow_head((0.04, 0.5), (-1.0, 0.0), 0.2, Blue);
    }
    Icon::PhysMaterial => {
      k.circle((0.5, 0.5), 0.42, Some(White), Some(Ink));
      for (x, y) in [(0.32, 0.32), (0.68, 0.32), (0.5, 0.5), (0.32, 0.68), (0.68, 0.68)] {
        k.fill(&[(x - 0.09, y - 0.09), (x + 0.09, y - 0.09), (x + 0.09, y + 0.09), (x - 0.09, y + 0.09)], Ink);
      }
    }
    Icon::Appearance => {
      let cols = [(230, 60, 50), (245, 160, 40), (240, 220, 60), (80, 180, 70), (50, 130, 220), (150, 90, 200)];
      for (i, c) in cols.iter().enumerate() {
        let a0 = TAU * i as f32 / 6.0;
        let mut v = vec![k.u(0.5, 0.5)];
        v.extend(k.arc_pts((0.5, 0.5), 0.44, 0.44, a0, a0 + TAU / 6.0));
        let col = match mono {
          Some(m) => m.gamma_multiply(0.3 + 0.1 * i as f32),
          None => Color32::from_rgb(c.0, c.1, c.2),
        };
        k.p.add(Shape::convex_polygon(v, col, Stroke::NONE));
      }
    }
    Icon::ManageMaterials | Icon::Bom | Icon::ConfigTable => {
      k.solid(&[(0.08, 0.12), (0.92, 0.12), (0.92, 0.9), (0.08, 0.9)], White);
      for y in [0.32, 0.51, 0.7] {
        k.line((0.08, y), (0.92, y), Ink);
      }
      k.line((0.36, 0.12), (0.36, 0.9), Ink);
      if icon == Icon::Bom {
        k.fill(&[(0.0, 0.0), (0.22, 0.0), (0.22, 0.2), (0.0, 0.2)], Blue);
      }
      if icon == Icon::ConfigTable {
        k.circle((0.76, 0.24), 0.2, Some(Blue), None);
        k.path(&[(0.8, 0.1), (0.72, 0.26), (0.8, 0.26), (0.72, 0.4)], White);
      }
    }
    Icon::ComputeAll => {
      k.solid(&[(0.3, 0.04), (0.7, 0.04), (0.7, 0.96), (0.3, 0.96)], Ink);
      k.circle((0.5, 0.22), 0.1, Some(Red), None);
      k.circle((0.5, 0.5), 0.1, Some(Orange), None);
      k.circle((0.5, 0.78), 0.1, Some(Green), None);
    }
    Icon::CreateForm => {
      k.block(0.06, 0.36, 0.6, 0.9, 0.24, PurpleLight, Purple, Purple);
      k.thick((0.78, 0.56), (0.78, 0.98), Green, 2.2);
      k.thick((0.57, 0.77), (0.99, 0.77), Green, 2.2);
    }
    Icon::Derive => {
      k.block(0.22, 0.36, 0.78, 0.92, 0.18, White, Light, Fill);
      k.thick((0.02, 0.64), (0.4, 0.64), Ink, 1.4);
      k.arrow_head((0.52, 0.64), (1.0, 0.0), 0.16, Ink);
    }
    Icon::Sweep => {
      k.p.add(Shape::line(k.arc_pts((0.12, 0.9), 0.7, 0.7, 0.0, FRAC_PI_2 * 0.9), Stroke::new(w * 5.0, k.col(Blue))));
      k.solid(&[(0.04, 0.08), (0.3, 0.08), (0.3, 0.34), (0.04, 0.34)], White);
    }
    Icon::Loft => {
      k.solid(&[(0.3, 0.2), (0.7, 0.2), (0.92, 0.86), (0.08, 0.86)], Blue);
      k.ellipse((0.5, 0.2), 0.2, 0.08, Some(BlueLight), Blue);
      k.circle((0.24, 0.8), 0.12, Some(White), Some(Ink));
    }
    Icon::Rib => {
      k.path(&[(0.1, 0.06), (0.1, 0.9), (0.94, 0.9)], Ink);
      k.solid(&[(0.14, 0.2), (0.14, 0.86), (0.8, 0.86)], Blue);
    }
    Icon::Web => {
      k.solid(&[(0.06, 0.06), (0.94, 0.06), (0.94, 0.94), (0.06, 0.94)], White);
      k.thick((0.5, 0.1), (0.5, 0.9), Blue, 1.8);
      k.thick((0.1, 0.5), (0.9, 0.5), Blue, 1.8);
    }
    Icon::Emboss => {
      k.solid(&[(0.04, 0.3), (0.96, 0.3), (0.96, 0.92), (0.04, 0.92)], Light);
      k.text((0.5, 0.56), "A", 0.62, Blue);
    }
    Icon::Hole => {
      k.block(0.1, 0.4, 0.66, 0.92, 0.24, Light, Fill, Dark);
      k.ellipse((0.47, 0.31), 0.13, 0.06, Some(White), Ink);
      k.circle((0.38, 0.66), 0.13, Some(BlueLight), Some(Blue));
    }
    Icon::Thread => {
      k.solid(&[(0.24, 0.08), (0.76, 0.08), (0.76, 0.92), (0.24, 0.92)], Light);
      for i in 0..5 {
        let y = 0.16 + i as f32 * 0.16;
        k.thick((0.2, y + 0.08), (0.8, y), Blue, 1.2);
      }
    }
    Icon::Coil => {
      let mut pts = Vec::new();
      for i in 0..=8 {
        let y = 0.08 + i as f32 * 0.105;
        pts.push((if i % 2 == 0 { 0.2 } else { 0.8 }, y));
      }
      k.p.add(Shape::line(k.pts(&pts), Stroke::new(w * 2.0, k.col(Blue))));
    }
    Icon::Pipe => {
      k.thick((0.12, 0.88), (0.88, 0.12), Blue, 5.0);
      k.thick((0.18, 0.82), (0.82, 0.18), BlueLight, 1.6);
    }
    Icon::Thicken => {
      k.solid(&[(0.04, 0.6), (0.4, 0.2), (0.96, 0.2), (0.6, 0.6)], BlueLight);
      k.solid(&[(0.04, 0.6), (0.6, 0.6), (0.6, 0.8), (0.04, 0.8)], Blue);
      k.solid(&[(0.6, 0.6), (0.96, 0.2), (0.96, 0.4), (0.6, 0.8)], BlueDark);
    }
    Icon::BoundaryFill => {
      k.solid(&[(0.06, 0.3), (0.6, 0.3), (0.6, 0.94), (0.06, 0.94)], Orange);
      k.solid(&[(0.4, 0.06), (0.94, 0.06), (0.94, 0.6), (0.4, 0.6)], Blue);
    }
    Icon::BaseFeature => {
      k.block(0.06, 0.36, 0.6, 0.9, 0.24, White, Light, Fill);
      k.thick((0.78, 0.56), (0.78, 0.98), Green, 2.2);
      k.thick((0.57, 0.77), (0.99, 0.77), Green, 2.2);
    }
    Icon::JointOrigin => {
      k.circle((0.5, 0.5), 0.36, Some(Light), Some(Ink));
      k.circle((0.5, 0.5), 0.12, Some(Blue), None);
      k.p.add(Shape::line(k.arc_pts((0.5, 0.5), 0.46, 0.46, 0.3, 1.5), Stroke::new(w * 1.2, k.col(Ink))));
    }
    Icon::Configure | Icon::ConfigRules => {
      if icon == Icon::ConfigRules {
        k.solid(&[(0.04, 0.3), (0.84, 0.3), (0.84, 0.92), (0.04, 0.92)], BlueLight);
      }
      let (c, rad) = if icon == Icon::Configure { ((0.5, 0.5), 0.44) } else { ((0.74, 0.26), 0.24) };
      k.circle(c, rad, Some(Blue), None);
      let s = rad / 0.44;
      k.p.add(Shape::line(
        k.pts(&[
          (c.0 + 0.08 * s, c.1 - 0.3 * s),
          (c.0 - 0.1 * s, c.1 + 0.04 * s),
          (c.0 + 0.1 * s, c.1 + 0.0),
          (c.0 - 0.06 * s, c.1 + 0.3 * s),
        ]),
        Stroke::new(w * 1.6, k.col(White)),
      ));
    }
    Icon::Ucs => {
      k.thick((0.2, 0.8), (0.95, 0.8), Red, 1.2);
      k.thick((0.2, 0.8), (0.2, 0.05), Blue, 1.2);
      k.thick((0.2, 0.8), (0.55, 0.45), Green, 1.2);
    }
    Icon::PlaneAlt => {
      k.solid(&[(0.04, 0.78), (0.3, 0.26), (0.96, 0.26), (0.7, 0.78)], Orange);
      k.thick((0.18, 0.92), (0.88, 0.1), Ink, 0.9);
    }
    Icon::Axis => {
      k.solid(&[(0.3, 0.3), (0.7, 0.3), (0.7, 0.9), (0.3, 0.9)], Light);
      k.thick((0.08, 0.96), (0.92, 0.04), Orange, 1.8);
    }
    Icon::CPoint => {
      k.line((0.5, 0.06), (0.5, 0.94), Ink);
      k.line((0.06, 0.5), (0.94, 0.5), Ink);
      k.circle((0.5, 0.5), 0.2, Some(Orange), Some(Orange));
    }
    Icon::Interference => {
      k.solid(&[(0.06, 0.06), (0.62, 0.06), (0.62, 0.62), (0.06, 0.62)], White);
      k.solid(&[(0.38, 0.38), (0.94, 0.38), (0.94, 0.94), (0.38, 0.94)], White);
      k.fill(&[(0.38, 0.38), (0.62, 0.38), (0.62, 0.62), (0.38, 0.62)], Red);
    }
    Icon::Section => {
      k.solid(&[(0.06, 0.06), (0.94, 0.06), (0.94, 0.94), (0.06, 0.94)], White);
      k.fill(&[(0.06, 0.06), (0.5, 0.06), (0.5, 0.94), (0.06, 0.94)], BlueLight);
      k.thick((0.5, 0.02), (0.5, 0.98), Blue, 1.4);
    }
    Icon::CurvComb => {
      k.arc((0.5, 0.95), 0.42, 0.3, PI - 0.3, Ink);
      for i in 0..7 {
        let a = 0.4 + i as f32 * 0.39;
        let (c, sn) = (a.cos(), a.sin());
        k.line((0.5 + c * 0.42, 0.95 - sn * 0.42), (0.5 + c * 0.62, 0.95 - sn * 0.62), Blue);
      }
    }
    Icon::CurvMap | Icon::DraftAnalysis | Icon::FaceGroups | Icon::GenFaceGroups | Icon::CombineFaceGroups => {
      let bands = [Red, Orange, Green, Blue];
      if icon == Icon::CurvMap {
        for (i, t) in bands.iter().enumerate() {
          let y = 0.08 + i as f32 * 0.21;
          k.fill(&[(0.08, y), (0.92, y), (0.92, y + 0.21), (0.08, y + 0.21)], *t);
        }
      } else if icon == Icon::DraftAnalysis {
        for (i, t) in bands.iter().enumerate() {
          let y = 0.1 + i as f32 * 0.2;
          let half = (0.42f32.powi(2) - (y + 0.1 - 0.5).powi(2)).max(0.02).sqrt();
          k.fill(&[(0.5 - half, y), (0.5 + half, y), (0.5 + half, y + 0.2), (0.5 - half, y + 0.2)], *t);
        }
      } else {
        k.block(0.08, 0.38, 0.62, 0.92, 0.26, Green, Red, Blue);
        if icon == Icon::CombineFaceGroups {
          k.fill(&[(0.08, 0.38), (0.62, 0.38), (0.62, 0.92), (0.08, 0.92)], Blue);
        }
      }
    }
    Icon::EnvMap => {
      k.circle((0.5, 0.5), 0.42, Some(Blue), None);
      k.circle((0.38, 0.36), 0.14, Some(BlueLight), None);
    }
    Icon::Isocurve => {
      k.solid(&[(0.06, 0.06), (0.94, 0.06), (0.94, 0.94), (0.06, 0.94)], White);
      for i in 1..4 {
        let t = i as f32 * 0.22 + 0.06;
        k.line((t, 0.06), (t + 0.06, 0.94), Ink);
        k.line((0.06, t), (0.94, t + 0.04), Ink);
      }
    }
    Icon::Zebra => {
      for i in 0..3 {
        k.p.add(Shape::line(
          k.arc_pts((0.0, 0.5), 0.3 + i as f32 * 0.24, 0.42, -1.2, 1.2),
          Stroke::new(w * 2.2, k.col(Ink)),
        ));
      }
    }
    Icon::Accessibility => {
      k.circle((0.4, 0.62), 0.32, Some(Green), None);
      k.circle((0.74, 0.28), 0.16, Some(Red), None);
    }
    Icon::CenterOfMass => {
      k.circle((0.5, 0.5), 0.42, Some(White), Some(Ink));
      let mut v = vec![k.u(0.5, 0.5)];
      v.extend(k.arc_pts((0.5, 0.5), 0.42, 0.42, 0.0, FRAC_PI_2));
      k.p.add(Shape::convex_polygon(v, k.col(Ink), Stroke::NONE));
      let mut v = vec![k.u(0.5, 0.5)];
      v.extend(k.arc_pts((0.5, 0.5), 0.42, 0.42, PI, PI + FRAC_PI_2));
      k.p.add(Shape::convex_polygon(v, k.col(Ink), Stroke::NONE));
    }
    Icon::MinRadius => {
      k.solid(&[(0.08, 0.08), (0.92, 0.08), (0.92, 0.92), (0.08, 0.92)], Light);
      k.arc((0.92, 0.08), 0.44, PI, PI + FRAC_PI_2, Red);
    }
    Icon::Navigator => {
      for (x, y) in [(0.08, 0.08), (0.6, 0.08), (0.08, 0.6), (0.6, 0.6)] {
        k.closed(&[(x, y), (x + 0.3, y), (x + 0.3, y + 0.3), (x, y + 0.3)], Ink);
      }
      k.line((0.38, 0.23), (0.6, 0.23), Ink);
      k.line((0.23, 0.38), (0.23, 0.6), Ink);
    }
    Icon::ComponentColors => {
      k.circle((0.34, 0.34), 0.22, Some(Purple), None);
      k.circle((0.68, 0.4), 0.2, Some(Blue), None);
      k.circle((0.44, 0.7), 0.22, Some(Orange), None);
    }
    Icon::History => {
      k.solid(&[(0.14, 0.06), (0.74, 0.06), (0.74, 0.94), (0.14, 0.94)], White);
      k.circle((0.66, 0.7), 0.24, Some(White), Some(Blue));
      k.path(&[(0.66, 0.56), (0.66, 0.7), (0.76, 0.76)], Blue);
    }
    Icon::Sustainability => {
      k.circle((0.5, 0.5), 0.42, Some(White), Some(Ink));
      k.text((0.5, 0.5), "CO2", 0.3, Ink);
    }
    Icon::Decal | Icon::Canvas => {
      if icon == Icon::Decal {
        k.solid(&[(0.04, 0.04), (0.64, 0.04), (0.64, 0.64), (0.04, 0.64)], Blue);
        k.solid(&[(0.36, 0.5), (0.96, 0.5), (0.96, 0.96), (0.36, 0.96)], White);
      } else {
        k.solid(&[(0.04, 0.12), (0.96, 0.12), (0.96, 0.88), (0.04, 0.88)], BlueLight);
        k.fill(&[(0.04, 0.88), (0.38, 0.42), (0.7, 0.88)], White);
        k.fill(&[(0.42, 0.88), (0.66, 0.56), (0.96, 0.88)], White);
        k.circle((0.74, 0.32), 0.08, Some(White), None);
      }
    }
    Icon::InsertSvg | Icon::InsertDxf => {
      k.solid(&[(0.16, 0.04), (0.84, 0.04), (0.84, 0.96), (0.16, 0.96)], White);
      k.fill(&[(0.16, 0.62), (0.84, 0.62), (0.84, 0.96), (0.16, 0.96)], Ink);
      k.text((0.5, 0.79), if icon == Icon::InsertSvg { "SVG" } else { "DXF" }, 0.26, White);
      k.thick((0.0, 0.3), (0.42, 0.3), Blue, 1.4);
      k.arrow_head((0.54, 0.3), (1.0, 0.0), 0.16, Blue);
    }
    Icon::InsertMesh
    | Icon::Tessellate
    | Icon::BaseMesh
    | Icon::DirectEdit
    | Icon::Repair
    | Icon::Remesh
    | Icon::Reduce
    | Icon::PlaneCut
    | Icon::Smooth
    | Icon::ScaleMesh
    | Icon::MeshSection => {
      k.block(0.08, 0.38, 0.62, 0.92, 0.28, BlueLight, Blue, BlueDark);
      k.line((0.08, 0.38), (0.62, 0.92), BlueDark);
      k.line((0.62, 0.38), (0.08, 0.92), BlueDark);
      match icon {
        Icon::InsertMesh => {
          k.thick((0.0, 0.12), (0.34, 0.12), Ink, 1.4);
          k.arrow_head((0.46, 0.12), (1.0, 0.0), 0.16, Ink);
        }
        Icon::Tessellate | Icon::ScaleMesh => k.solid(&[(0.66, 0.04), (0.96, 0.04), (0.96, 0.34), (0.66, 0.34)], Light),
        Icon::BaseMesh | Icon::DirectEdit => {
          for i in 0..4 {
            let a = i as f32 * FRAC_PI_2 * 0.5;
            k.thick(
              (0.8 - a.cos() * 0.16, 0.2 - a.sin() * 0.16),
              (0.8 + a.cos() * 0.16, 0.2 + a.sin() * 0.16),
              Orange,
              1.4,
            );
          }
        }
        Icon::Repair => k.solid(&[(0.04, 0.72), (0.66, 0.72), (0.66, 0.86), (0.04, 0.86)], White),
        Icon::Remesh | Icon::Reduce => k.line((0.35, 0.38), (0.35, 0.92), BlueDark),
        Icon::PlaneCut => k.solid(&[(0.0, 0.7), (0.3, 0.2), (0.98, 0.2), (0.68, 0.7)], Orange),
        Icon::Smooth => k.circle((0.74, 0.24), 0.18, Some(BlueLight), Some(Blue)),
        _ => k.ellipse((0.36, 0.65), 0.3, 0.1, None, Blue),
      }
    }
    Icon::InsertComponent | Icon::Manufacturer | Icon::McMaster => {
      k.block(0.18, 0.36, 0.72, 0.92, 0.24, Light, Fill, Dark);
      if icon == Icon::McMaster {
        k.text((0.45, 0.65), "M", 0.4, Ink);
      }
    }
    Icon::Fastener => {
      k.solid(&[(0.1, 0.1), (0.9, 0.1), (0.9, 0.32), (0.1, 0.32)], Fill);
      k.solid(&[(0.38, 0.32), (0.62, 0.32), (0.62, 0.94), (0.38, 0.94)], Fill);
    }
    Icon::DuplicateJoints => {
      for x in [0.08, 0.38, 0.68] {
        k.solid(&[(x, 0.2), (x + 0.22, 0.2), (x + 0.22, 0.86), (x, 0.86)], Fill);
      }
    }
    Icon::AddToAssembly => {
      k.block(0.06, 0.2, 0.36, 0.46, 0.12, White, Light, Fill);
      k.block(0.44, 0.56, 0.84, 0.94, 0.12, Light, Fill, Dark);
      k.thick((0.5, 0.14), (0.62, 0.36), Ink, 1.0);
      k.arrow_head((0.66, 0.44), (0.4, 1.0), 0.14, Ink);
    }
    Icon::WindowSel | Icon::FreeformSel | Icon::PaintSel => {
      match icon {
        Icon::WindowSel => {
          for (a, b) in [
            ((0.06, 0.06), (0.78, 0.06)),
            ((0.06, 0.06), (0.06, 0.7)),
            ((0.78, 0.06), (0.78, 0.4)),
            ((0.06, 0.7), (0.4, 0.7)),
          ] {
            k.dashed(a, b, Ink);
          }
        }
        Icon::FreeformSel => {
          k.p.add(Shape::line(k.arc_pts((0.42, 0.4), 0.36, 0.32, 0.4, TAU - 0.6), Stroke::new(w, k.col(Green))));
        }
        _ => {
          k.block(0.06, 0.4, 0.5, 0.8, 0.16, Green, White, Light);
          k.thick((0.6, 0.62), (0.94, 0.2), Orange, 2.0);
        }
      }
      if icon != Icon::PaintSel {
        k.solid(&[(0.46, 0.34), (0.46, 0.98), (0.62, 0.84), (0.82, 0.84)], White);
      }
    }
    Icon::Patch => {
      k.solid(&[(0.08, 0.2), (0.56, 0.2), (0.56, 0.56), (0.08, 0.56)], Blue);
      k.solid(&[(0.56, 0.2), (0.92, 0.5), (0.92, 0.9), (0.56, 0.56)], BlueDark);
    }
    Icon::Ruled => {
      k.solid(&[(0.06, 0.66), (0.46, 0.2), (0.94, 0.2), (0.54, 0.66)], BlueLight);
      k.solid(&[(0.06, 0.66), (0.54, 0.66), (0.54, 0.86), (0.06, 0.86)], Blue);
    }
    Icon::SurfOffset => {
      k.p.add(Shape::line(k.arc_pts((0.5, 1.1), 0.5, 0.5, 0.5, PI - 0.5), Stroke::new(w * 3.0, k.col(BlueLight))));
      k.p.add(Shape::line(k.arc_pts((0.5, 1.1), 0.8, 0.8, 0.6, PI - 0.6), Stroke::new(w * 3.0, k.col(Blue))));
    }
    Icon::Trim => {
      k.circle((0.24, 0.78), 0.14, None, Some(Ink));
      k.circle((0.76, 0.78), 0.14, None, Some(Ink));
      k.thick((0.32, 0.66), (0.78, 0.06), Ink, 1.3);
      k.thick((0.68, 0.66), (0.22, 0.06), Ink, 1.3);
    }
    Icon::Untrim | Icon::Unstitch => {
      if icon == Icon::Untrim {
        k.solid(&[(0.08, 0.2), (0.7, 0.2), (0.7, 0.9), (0.08, 0.9)], Blue);
      } else {
        k.solid(&[(0.14, 0.1), (0.62, 0.1), (0.62, 0.9), (0.14, 0.9)], Blue);
        for i in 0..5 {
          let y = 0.2 + i as f32 * 0.15;
          k.line((0.3, y), (0.46, y), White);
        }
      }
      k.thick((0.62, 0.62), (0.96, 0.96), Red, 1.6);
      k.thick((0.96, 0.62), (0.62, 0.96), Red, 1.6);
    }
    Icon::Extend => {
      k.solid(&[(0.08, 0.5), (0.92, 0.5), (0.92, 0.94), (0.08, 0.94)], Blue);
      k.thick((0.5, 0.46), (0.5, 0.16), Ink, 1.4);
      k.arrow_head((0.5, 0.04), (0.0, -1.0), 0.16, Ink);
    }
    Icon::Stitch => {
      k.solid(&[(0.14, 0.06), (0.86, 0.06), (0.86, 0.94), (0.14, 0.94)], Blue);
      k.line((0.5, 0.06), (0.5, 0.94), Ink);
      for i in 0..5 {
        let y = 0.16 + i as f32 * 0.17;
        k.line((0.36, y), (0.64, y), Ink);
      }
    }
    Icon::ReverseNormal => {
      k.solid(&[(0.06, 0.4), (0.94, 0.4), (0.94, 0.94), (0.06, 0.94)], Blue);
      k.thick((0.5, 0.36), (0.5, 0.12), Ink, 1.2);
      k.arrow_head((0.5, 0.02), (0.0, -1.0), 0.14, Ink);
      k.text((0.5, 0.7), "↔", 0.4, White);
    }
    Icon::Separate => {
      k.block(0.02, 0.48, 0.38, 0.92, 0.14, BlueLight, Blue, BlueDark);
      k.block(0.5, 0.3, 0.84, 0.72, 0.14, BlueLight, Blue, BlueDark);
    }
    Icon::Flange | Icon::LoftedFlange => {
      let pts = [(0.22, 0.94), (0.22, 0.42), (0.3, 0.24), (0.46, 0.16), (0.92, 0.16)];
      k.p.add(Shape::line(k.pts(&pts), Stroke::new(w * 5.0, k.col(Blue))));
      k.p.add(Shape::line(k.pts(&pts), Stroke::new(w * 1.6, k.col(BlueLight))));
      if icon == Icon::LoftedFlange {
        k.ellipse((0.7, 0.72), 0.22, 0.12, Some(BlueLight), Blue);
      }
    }
    Icon::Hem => {
      k.thick((0.06, 0.82), (0.82, 0.34), Blue, 4.0);
      k.thick((0.82, 0.34), (0.5, 0.18), BlueDark, 4.0);
    }
    Icon::Fold | Icon::Unfold => {
      k.solid(&[(0.06, 0.56), (0.5, 0.56), (0.5, 0.9), (0.06, 0.9)], Blue);
      if icon == Icon::Fold {
        k.solid(&[(0.5, 0.56), (0.9, 0.16), (0.9, 0.5), (0.5, 0.9)], BlueLight);
        k.arc((0.5, 0.56), 0.3, 0.2, 1.4, Ink);
      } else {
        k.solid(&[(0.5, 0.56), (0.94, 0.56), (0.94, 0.9), (0.5, 0.9)], BlueLight);
        k.dashed((0.5, 0.56), (0.9, 0.16), Ink);
      }
    }
    Icon::ConvertSheet => {
      k.block(0.06, 0.4, 0.5, 0.92, 0.2, Light, Fill, Dark);
      k.thick((0.62, 0.9), (0.62, 0.3), Blue, 3.0);
      k.thick((0.62, 0.3), (0.94, 0.3), Blue, 3.0);
    }
    Icon::FlatPattern => {
      k.solid(&[(0.06, 0.06), (0.8, 0.06), (0.8, 0.8), (0.06, 0.8)], White);
      k.line((0.06, 0.32), (0.8, 0.32), Ink);
      k.line((0.32, 0.06), (0.32, 0.8), Ink);
      k.circle((0.82, 0.84), 0.14, Some(Blue), None);
    }
    Icon::SmRules => {
      k.solid(&[(0.04, 0.06), (0.74, 0.06), (0.74, 0.84), (0.04, 0.84)], White);
      for y in [0.3, 0.52] {
        k.line((0.04, y), (0.74, y), Ink);
      }
      k.block(0.5, 0.62, 0.86, 0.96, 0.12, Light, Fill, Dark);
    }
    Icon::CornerClosure => {
      k.solid(&[(0.06, 0.2), (0.46, 0.5), (0.46, 0.94), (0.06, 0.64)], Blue);
      k.solid(&[(0.94, 0.2), (0.54, 0.5), (0.54, 0.94), (0.94, 0.64)], BlueLight);
    }
    Icon::Rip => {
      k.solid(&[(0.14, 0.06), (0.86, 0.06), (0.86, 0.94), (0.14, 0.94)], Blue);
      k.path(&[(0.5, 0.06), (0.4, 0.24), (0.6, 0.42), (0.4, 0.6), (0.6, 0.78), (0.5, 0.94)], White);
    }
    Icon::Scripts => {
      k.solid(&[(0.04, 0.08), (0.8, 0.08), (0.8, 0.7), (0.04, 0.7)], White);
      k.fill(&[(0.04, 0.08), (0.8, 0.08), (0.8, 0.2), (0.04, 0.2)], Ink);
      k.path(&[(0.14, 0.32), (0.26, 0.42), (0.14, 0.52)], Ink);
      k.circle((0.76, 0.76), 0.16, Some(Fill), Some(Ink));
      k.circle((0.76, 0.76), 0.06, Some(White), None);
    }
    Icon::Marketplace => {
      k.solid(&[(0.24, 0.42), (0.7, 0.42), (0.7, 0.94), (0.24, 0.94)], Ink);
      k.solid(&[(0.36, 0.16), (0.58, 0.16), (0.58, 0.42), (0.36, 0.42)], Ink);
      k.thick((0.6, 0.18), (0.86, 0.04), Orange, 2.4);
    }
    Icon::Assistant => {
      for i in 0..8 {
        let a = i as f32 * TAU / 8.0;
        let (c, s) = (a.cos(), a.sin());
        k.thick((0.5 + c * 0.14, 0.5 + s * 0.14), (0.5 + c * 0.42, 0.5 + s * 0.42), Ink, 1.4);
      }
    }
    Icon::Gauge => {
      k.circle((0.5, 0.5), 0.42, None, Some(Ink));
      k.thick((0.5, 0.5), (0.74, 0.3), Ink, 1.4);
      k.dot((0.5, 0.5), Ink);
    }
    Icon::Clock => {
      k.circle((0.5, 0.5), 0.42, None, Some(Ink));
      k.path(&[(0.5, 0.22), (0.5, 0.5), (0.7, 0.62)], Ink);
    }
    Icon::Print3d => {
      k.solid(&[(0.06, 0.12), (0.94, 0.12), (0.94, 0.9), (0.06, 0.9)], Dark);
      k.solid(&[(0.2, 0.34), (0.8, 0.34), (0.8, 0.78), (0.2, 0.78)], White);
      k.fill(&[(0.3, 0.78), (0.5, 0.48), (0.7, 0.78)], Orange);
    }
    Icon::ConvertMesh => {
      k.ellipse((0.34, 0.78), 0.22, 0.08, Some(Fill), Ink);
      k.fill(&[(0.12, 0.3), (0.56, 0.3), (0.56, 0.78), (0.12, 0.78)], Fill);
      k.ellipse((0.34, 0.3), 0.22, 0.08, Some(Light), Ink);
      k.block(0.6, 0.62, 0.86, 0.94, 0.1, BlueLight, Blue, BlueDark);
    }
  }
}

/// A large toolbar icon (Fusion shows toolbar commands as icons only; the name is the tooltip).
pub fn tool_button(ui: &mut egui::Ui, icon: Icon, label: &str, hint: &str, active: bool) -> egui::Response {
  tool_button_tinted(ui, icon, Tint::Blue, label, hint, active)
}

/// [`tool_button`] with the icon's solids in the `tint` family.
pub fn tool_button_tinted(
  ui: &mut egui::Ui,
  icon: Icon,
  tint: Tint,
  label: &str,
  hint: &str,
  active: bool,
) -> egui::Response {
  let size = vec2(38.0, 38.0);
  let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
  let ic = Rect::from_center_size(rect.center(), Vec2::splat(28.0));
  if active {
    // Fusion shows the running tool as a white icon on solid blue
    ui.painter().rect_filled(rect, 2.0, theme::ACCENT);
    draw(ui.painter(), ic, icon, Color32::WHITE);
  } else {
    if resp.hovered() {
      ui.painter().rect(rect, 2.0, theme::HOVER, Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
    }
    draw_tinted(ui.painter(), ic, icon, tint);
  }
  resp.on_hover_ui(|ui| {
    ui.label(egui::RichText::new(label).strong());
    if !hint.is_empty() {
      ui.label(egui::RichText::new(hint).color(theme::MUTED));
    }
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
