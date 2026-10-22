//! The application bar's icons, as Fusion draws them: solid dark grey glyphs (a filled house,
//! grid, page, floppy, bell, ...) with the details cut out in the bar's colour, not the
//! coloured toolbar style. One ink for all of them, so the bar reads as one row.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use std::f32::consts::{PI, TAU};

/// Fusion's application bar ink: a flat mid grey (#808080, measured on Fusion's bar).
pub const INK: Color32 = Color32::from_rgb(128, 128, 128);
/// A disabled bar icon (Redo with nothing to redo).
pub const INK_OFF: Color32 = Color32::from_rgb(184, 184, 184);

/// An application bar glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
  Home,
  DataPanel,
  File,
  Save,
  Undo,
  Redo,
  Plus,
  Close,
  /// Fusion's blue Extensions button (FissionOxide's plugins): a white bolt on a blue disc.
  Plugins,
  Assistant,
  JobStatus,
  Bell,
  Help,
}

/// Unit-square coordinates in `r` to the screen.
struct Box(Rect);

impl Box {
  fn u(&self, x: f32, y: f32) -> Pos2 {
    pos2(self.0.min.x + x * self.0.width(), self.0.min.y + y * self.0.height())
  }

  fn pts(&self, pts: &[(f32, f32)]) -> Vec<Pos2> {
    pts.iter().map(|&(x, y)| self.u(x, y)).collect()
  }

  fn len(&self, k: f32) -> f32 {
    k * self.0.width()
  }

  fn poly(&self, p: &Painter, pts: &[(f32, f32)], c: Color32) {
    p.add(Shape::convex_polygon(self.pts(pts), c, Stroke::NONE));
  }

  fn rect(&self, p: &Painter, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
    p.rect_filled(Rect::from_min_max(self.u(x0, y0), self.u(x1, y1)), 0.0, c);
  }

  fn arc(&self, c: (f32, f32), r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    (0..=20).map(|i| a0 + (a1 - a0) * i as f32 / 20.0).map(|t| self.u(c.0 + t.cos() * r, c.1 - t.sin() * r)).collect()
  }
}

/// Draws `g` filling `r` in `ink`; `bg` is the colour under it, used for the cut-out details.
pub fn draw(p: &Painter, r: Rect, g: Glyph, ink: Color32, bg: Color32) {
  let b = Box(r);
  match g {
    Glyph::Home => {
      b.poly(p, &[(0.5, 0.06), (0.97, 0.5), (0.03, 0.5)], ink);
      b.rect(p, 0.17, 0.44, 0.83, 0.92, ink);
      b.rect(p, 0.41, 0.62, 0.59, 0.92, bg);
    }
    Glyph::DataPanel => {
      for i in 0..3 {
        for j in 0..3 {
          let (x, y) = (0.08 + i as f32 * 0.31, 0.08 + j as f32 * 0.31);
          b.rect(p, x, y, x + 0.22, y + 0.22, ink);
        }
      }
    }
    Glyph::File => {
      b.poly(p, &[(0.16, 0.04), (0.6, 0.04), (0.84, 0.28), (0.84, 0.96), (0.16, 0.96)], ink);
      // the folded corner, lighter
      b.poly(p, &[(0.6, 0.04), (0.84, 0.28), (0.6, 0.28)], mix(ink, bg, 0.55));
    }
    Glyph::Save => {
      b.poly(p, &[(0.08, 0.08), (0.74, 0.08), (0.92, 0.26), (0.92, 0.92), (0.08, 0.92)], ink);
      b.rect(p, 0.26, 0.08, 0.66, 0.34, bg);
      b.rect(p, 0.5, 0.13, 0.6, 0.29, ink);
      b.rect(p, 0.22, 0.54, 0.78, 0.92, bg);
      b.rect(p, 0.3, 0.62, 0.7, 0.66, ink);
      b.rect(p, 0.3, 0.73, 0.7, 0.77, ink);
    }
    Glyph::Undo | Glyph::Redo => {
      let flip = |x: f32| if g == Glyph::Redo { 1.0 - x } else { x };
      // a thick hook from the arrow head round to the right, as Fusion's
      let pts: Vec<Pos2> = b
        .arc((0.54, 0.6), 0.3, PI * 0.82, -PI * 0.3)
        .into_iter()
        .map(|q| pos2(r.min.x + flip((q.x - r.min.x) / r.width()) * r.width(), q.y))
        .collect();
      p.add(Shape::line(pts, Stroke::new(b.len(0.15), ink)));
      let tip = (flip(0.04), 0.42);
      b.poly(p, &[tip, (flip(0.42), 0.18), (flip(0.42), 0.66)], ink);
    }
    Glyph::Plus => {
      let w = b.len(0.12);
      p.line_segment([b.u(0.5, 0.1), b.u(0.5, 0.9)], Stroke::new(w, ink));
      p.line_segment([b.u(0.1, 0.5), b.u(0.9, 0.5)], Stroke::new(w, ink));
    }
    Glyph::Close => {
      let w = b.len(0.1);
      p.line_segment([b.u(0.2, 0.2), b.u(0.8, 0.8)], Stroke::new(w, ink));
      p.line_segment([b.u(0.8, 0.2), b.u(0.2, 0.8)], Stroke::new(w, ink));
    }
    Glyph::Plugins => {
      p.circle_filled(b.u(0.5, 0.5), b.len(0.48), crate::theme::ACCENT);
      b.poly(p, &[(0.56, 0.14), (0.3, 0.56), (0.5, 0.56)], Color32::WHITE);
      b.poly(p, &[(0.5, 0.44), (0.7, 0.44), (0.44, 0.86)], Color32::WHITE);
    }
    Glyph::Assistant => {
      // Fusion's sparkle: four long rays and four short ones
      for i in 0..8 {
        let a = i as f32 * TAU / 8.0;
        let len = if i % 2 == 0 { 0.46 } else { 0.32 };
        let (c, s) = (a.cos(), a.sin());
        p.line_segment(
          [b.u(0.5 + c * 0.12, 0.5 + s * 0.12), b.u(0.5 + c * len, 0.5 + s * len)],
          Stroke::new(b.len(0.11), ink),
        );
      }
    }
    Glyph::JobStatus => {
      p.circle_filled(b.u(0.5, 0.5), b.len(0.46), ink);
      p.circle_filled(b.u(0.5, 0.5), b.len(0.3), bg);
      p.add(Shape::convex_polygon(b.pts(&[(0.5, 0.5), (0.5, 0.2), (0.8, 0.5)]), ink, Stroke::NONE));
    }
    Glyph::Bell => {
      let mut dome = b.arc((0.5, 0.42), 0.28, 0.0, PI);
      dome.push(b.u(0.22, 0.68));
      dome.push(b.u(0.78, 0.68));
      p.add(Shape::convex_polygon(dome, ink, Stroke::NONE));
      b.poly(p, &[(0.22, 0.6), (0.78, 0.6), (0.94, 0.8), (0.06, 0.8)], ink);
      p.circle_filled(b.u(0.5, 0.86), b.len(0.1), ink);
    }
    Glyph::Help => {
      p.circle_filled(b.u(0.5, 0.5), b.len(0.47), ink);
      p.text(b.u(0.5, 0.53), egui::Align2::CENTER_CENTER, "?", egui::FontId::proportional(b.len(0.74)), bg);
    }
  }
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
  let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
  Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// A square application bar button `size` across: no frame until hovered (Fusion's light hover
/// box), the glyph in the middle. Disabled buttons draw greyed and show the not-allowed cursor.
pub fn button(ui: &mut egui::Ui, g: Glyph, size: f32, hint: &str, enabled: bool) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(size, size), egui::Sense::click());
  let bg = hover_fill(ui, rect, &resp, enabled);
  let ink = if enabled { INK } else { INK_OFF };
  draw(ui.painter(), Rect::from_center_size(rect.center(), egui::Vec2::splat(GLYPH)), g, ink, bg);
  let resp = if hint.is_empty() { resp } else { resp.on_hover_text(hint) };
  if enabled { resp } else { resp.on_hover_cursor(egui::CursorIcon::NotAllowed) }
}

/// The glyph size on the bar (Fusion's 16 px icons).
pub const GLYPH: f32 = 16.0;

/// Paints the hover box behind a bar button and returns the colour under the glyph.
pub(crate) fn hover_fill(ui: &egui::Ui, rect: Rect, resp: &egui::Response, enabled: bool) -> Color32 {
  if enabled && resp.hovered() {
    ui.painter().rect_filled(rect, 3.0, HOVER);
    HOVER
  } else {
    crate::theme::APP_BAR
  }
}

/// A bar button under the pointer: a darker grey box, as Fusion's.
pub const HOVER: Color32 = Color32::from_rgb(196, 196, 196);
