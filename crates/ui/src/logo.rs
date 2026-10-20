//! The FissionOxide fox, painted from its facets in `fox-brand` so it stays sharp at any size,
//! and its two animations: the start-up [`Splash`] and the [`ripple`] loading indicator. Both play
//! `brand/logo/motion/`'s keyframes through `fox_brand::animate`.

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Painter, Pos2, Rect, Response, Sense, Shape, Stroke, TextureHandle, Ui, pos2, vec2};
use fox_brand::animate::{self, Pose};
use fox_brand::logo::{FACETS, SIZE};
use fox_brand::motion::{NUCLEUS, Stop, ripple as ripple_data, splash as splash_data};

use crate::theme::color;

/// Paints the fox centred in `rect`, as large as fits.
pub fn paint(painter: &Painter, rect: Rect) {
  let s = (rect.width() / SIZE.0).min(rect.height() / SIZE.1);
  let origin = rect.center() - egui::vec2(SIZE.0, SIZE.1) * s / 2.0;
  for f in FACETS {
    let pts: Vec<Pos2> = f.points.iter().map(|&(x, y)| pos2(origin.x + x * s, origin.y + y * s)).collect();
    let c = color(f.color);
    // a hairline in the facet's colour hides the anti-aliasing seams between facets
    painter.add(Shape::convex_polygon(pts, c, Stroke::new(0.6, c)));
  }
}

/// A canvas of `size` units fitted and centred in `rect`: maps canvas points to the screen.
#[derive(Clone, Copy)]
struct Fit {
  origin: Pos2,
  scale: f32,
}

impl Fit {
  fn new(rect: Rect, size: (f32, f32)) -> Fit {
    let scale = (rect.width() / size.0).min(rect.height() / size.1);
    Fit { origin: rect.center() - vec2(size.0, size.1) * scale / 2.0, scale }
  }

  fn pos(self, (x, y): (f32, f32)) -> Pos2 {
    self.origin + vec2(x, y) * self.scale
  }
}

/// The fox's facets, each moved and faded by its pose.
fn paint_facets(painter: &Painter, fit: Fit, poses: &[Pose]) {
  for (f, pose) in FACETS.iter().zip(poses) {
    if pose.opacity <= 0.0 {
      continue;
    }
    let pts: Vec<Pos2> = f.points.iter().map(|&p| fit.pos(pose.transform.apply(p))).collect();
    let c = color(f.color).gamma_multiply(pose.opacity.min(1.0));
    // the seam-hiding hairline would show as an outline on a see-through facet
    let stroke = if pose.opacity >= 0.99 { Stroke::new(0.6, c) } else { Stroke::NONE };
    painter.add(Shape::convex_polygon(pts, c, stroke));
  }
}

/// A radial gradient disc: `stops` from the centre out to `radius`, all faded by `opacity`.
fn paint_glow(painter: &Painter, centre: Pos2, radius: f32, stops: &[Stop], opacity: f32) {
  if opacity <= 0.0 || radius <= 0.0 {
    return;
  }
  const SIDES: u32 = 48;
  let mut mesh = Mesh::default();
  let shade = |&(_, c, a): &Stop| color(c).gamma_multiply(a * opacity);
  mesh.vertices.push(Vertex { pos: centre, uv: WHITE_UV, color: shade(&stops[0]) });
  for (ring, stop) in stops.iter().enumerate().skip(usize::from(stops[0].0 == 0.0)) {
    for i in 0..SIDES {
      let a = std::f32::consts::TAU * i as f32 / SIDES as f32;
      let pos = centre + vec2(a.cos(), a.sin()) * radius * stop.0;
      mesh.vertices.push(Vertex { pos, uv: WHITE_UV, color: shade(stop) });
    }
    let ring = ring as u32 - u32::from(stops[0].0 == 0.0);
    let base = 1 + ring * SIDES;
    for i in 0..SIDES {
      let j = (i + 1) % SIDES;
      if ring == 0 {
        mesh.add_triangle(0, base + i, base + j);
      } else {
        let inner = base - SIDES;
        mesh.add_triangle(inner + i, base + i, base + j);
        mesh.add_triangle(inner + i, base + j, inner + j);
      }
    }
  }
  painter.add(Shape::mesh(mesh));
}

/// Paints the ripple loader `t` seconds in, fitted and centred in `rect`.
pub fn paint_ripple(painter: &Painter, rect: Rect, t: f32) {
  let fit = Fit::new(rect, animate::ripple_size());
  let frame = animate::ripple(t);
  paint_facets(painter, fit, &frame.facets);
  let r = ripple_data::NOSE_RADIUS * frame.nose.scale * fit.scale;
  paint_glow(painter, fit.pos(animate::ripple_nucleus()), r, ripple_data::GLOW, frame.nose.opacity);
}

/// The ripple loader as a widget `height` points tall: FissionOxide's busy indicator.
pub fn ripple(ui: &mut Ui, height: f32) -> Response {
  let (w, h) = animate::ripple_size();
  let (rect, response) = ui.allocate_exact_size(vec2(height * w / h, height), Sense::hover());
  if ui.is_rect_visible(rect) {
    paint_ripple(ui.painter(), rect, ui.input(|i| i.time) as f32);
    ui.ctx().request_repaint();
  }
  response
}

/// The start-up splash: the fox bursts in and "FOX" opens out to FissionOxide, then the finished
/// logo holds until the app is ready and fades to reveal it. Clicking or pressing a key skips to
/// the finished logo.
#[derive(Default)]
pub struct Splash {
  /// When the animation started (egui time).
  start: Option<f64>,
  /// When it started fading out.
  fade: Option<f64>,
  /// Frames the app has drawn beneath the finished logo.
  warm: u32,
  letters: Option<TextureHandle>,
}

/// What the window should show this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplashPhase {
  /// Only the splash: the logo is still forming.
  Playing,
  /// The app beneath, still hidden by the finished logo (it loads and draws its first frames).
  Holding,
  /// The app beneath, the logo fading off it.
  Fading,
  /// The app alone.
  Done,
}

const FADE: f64 = 0.35;

impl Splash {
  /// Where the splash is. `loading` is true while the app is still getting ready (opening the
  /// design it was started with).
  pub fn phase(&mut self, ctx: &egui::Context, loading: bool) -> SplashPhase {
    let now = ctx.input(|i| i.time);
    let start = *self.start.get_or_insert(now);
    if let Some(f) = self.fade {
      return if now - f >= FADE { SplashPhase::Done } else { SplashPhase::Fading };
    }
    if (now - start) < f64::from(animate::splash_duration()) {
      return SplashPhase::Playing;
    }
    // a couple of frames of the app beneath so its first, slowest frames happen out of sight
    if !loading && self.warm >= 2 {
      self.fade = Some(now);
      return SplashPhase::Fading;
    }
    SplashPhase::Holding
  }

  /// Paints the splash over the whole window. Call after drawing the app (if it is drawn).
  pub fn show(&mut self, ctx: &egui::Context, phase: SplashPhase, note: Option<&str>) {
    let now = ctx.input(|i| i.time);
    let start = *self.start.get_or_insert(now);
    if phase == SplashPhase::Holding {
      self.warm += 1;
    }
    if phase == SplashPhase::Playing
      && ctx.input(|i| {
        i.pointer.any_pressed() || i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
      })
    {
      // skip to the finished logo
      self.start = Some(now - f64::from(animate::splash_duration()));
    }
    let t = (now - self.start.unwrap_or(start)) as f32;
    let alpha = match self.fade {
      Some(f) => (1.0 - (now - f) / FADE).clamp(0.0, 1.0) as f32,
      None => 1.0,
    };
    let screen = ctx.content_rect();
    // an input-blocking cover over the app, and the splash on the topmost layer
    egui::Area::new(egui::Id::new("splash-cover")).fixed_pos(screen.min).order(egui::Order::Foreground).show(
      ctx,
      |ui| {
        ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
      },
    );
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Debug, egui::Id::new("splash")));
    painter.rect_filled(screen, 0.0, color(fox_brand::light::BACKGROUND).gamma_multiply(alpha));
    let width =
      (screen.width() * 0.5).min(screen.height() * 0.5 * splash_data::SIZE.0 / splash_data::SIZE.1).min(720.0);
    let area = Rect::from_center_size(screen.center(), vec2(width, width * splash_data::SIZE.1 / splash_data::SIZE.0));
    self.paint(ctx, &painter, area, t, alpha);
    if let Some(note) = note
      && phase == SplashPhase::Holding
      && t > animate::splash_duration() + 0.6
    {
      let ink = color(fox_brand::light::TEXT_MUTED).gamma_multiply(alpha);
      painter.text(
        area.center_bottom() + vec2(0.0, 24.0),
        egui::Align2::CENTER_TOP,
        note,
        egui::FontId::proportional(14.0),
        ink,
      );
    }
    if phase != SplashPhase::Done {
      ctx.request_repaint();
    }
  }

  fn paint(&mut self, ctx: &egui::Context, painter: &Painter, area: Rect, t: f32, alpha: f32) {
    let fit = Fit::new(area, splash_data::SIZE);
    let frame = animate::splash(t);
    let fade = |p: &Pose| Pose { opacity: p.opacity * alpha, ..*p };
    let n = (NUCLEUS.0, NUCLEUS.1);
    let r = splash_data::RING_RADIUS * frame.ring.scale * fit.scale;
    if frame.ring.opacity > 0.0 && r > 0.0 {
      let c = color(splash_data::RING).gamma_multiply(frame.ring.opacity * alpha);
      painter.circle_stroke(fit.pos(n), r, Stroke::new(splash_data::RING_WIDTH * frame.ring.scale * fit.scale, c));
    }
    let r = splash_data::FLASH_RADIUS * frame.flash.scale * fit.scale;
    paint_glow(painter, fit.pos(n), r, splash_data::FLASH, frame.flash.opacity * alpha);
    let facets: Vec<Pose> = frame.facets.iter().map(fade).collect();
    paint_facets(painter, fit, &facets);

    let Some(tex) = self.letters(ctx) else { return };
    let size = tex.size_vec2();
    let ink = color(fox_brand::light::WORDMARK);
    let mut mesh = Mesh::with_texture(tex.id());
    for (g, pose) in splash_data::GLYPHS.iter().zip(&frame.glyphs) {
      if pose.opacity <= 0.0 {
        continue;
      }
      let (x0, y0, x1, y1) = g.cell;
      let uv =
        Rect::from_min_max(pos2(g.atlas_x / size.x, 0.0), pos2((g.atlas_x + x1 - x0) / size.x, (y1 - y0) / size.y));
      let c = ink.gamma_multiply((pose.opacity * alpha).min(1.0));
      let at = |p: (f32, f32)| fit.pos(pose.transform.apply(p));
      let base = mesh.vertices.len() as u32;
      for (p, u) in [
        ((x0, y0), uv.left_top()),
        ((x1, y0), uv.right_top()),
        ((x1, y1), uv.right_bottom()),
        ((x0, y1), uv.left_bottom()),
      ] {
        mesh.vertices.push(Vertex { pos: at(p), uv: u, color: c });
      }
      mesh.add_triangle(base, base + 1, base + 2);
      mesh.add_triangle(base, base + 2, base + 3);
    }
    painter.add(Shape::mesh(mesh));
  }

  /// The name's letters, loaded once.
  fn letters(&mut self, ctx: &egui::Context) -> Option<&TextureHandle> {
    if self.letters.is_none() {
      let img = image::load_from_memory(splash_data::GLYPHS_PNG)
        .map_err(|e| log::warn!("splash letters: {e}"))
        .ok()?
        .to_rgba8();
      let size = [img.width() as usize, img.height() as usize];
      let img = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
      let options =
        egui::TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..egui::TextureOptions::LINEAR };
      self.letters = Some(ctx.load_texture("splash-letters", img, options));
    }
    self.letters.as_ref()
  }
}
