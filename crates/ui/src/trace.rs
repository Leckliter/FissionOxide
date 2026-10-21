//! Trace Image: the dialog's settings and the traced image drawn on the sketch plane under the
//! preview. The tracing itself is the engine's `sketch.trace`, previewed like any other dialog.
//! It traces a picked image file, or a canvas of the design where the canvas lies.

use std::path::{Path, PathBuf};

use egui::{Color32, Pos2, RichText};
use fox_engine::kernel::glam::DVec3;
use serde_json::{Value, json};

use crate::{App, theme};

/// Image files the tracer reads.
pub const EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "bmp", "webp"];

/// Longest side of the overlay texture (the trace itself uses the full image).
const MAX_TEXTURE: u32 = 2048;

const FITS: [(&str, &str); 3] =
  [("curves", "Lines, arcs and splines"), ("arcs", "Lines and arcs"), ("lines", "Lines only")];

pub fn is_image(p: &Path) -> bool {
  p.extension().is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

pub struct TraceState {
  pub path: Option<PathBuf>,
  /// Tracing a canvas: its image, place and size come from it.
  pub canvas: Option<fox_engine::model::FeatureId>,
  colors_mode: bool,
  /// Centre-line strokes (engraving) instead of closed outlines.
  centerline: bool,
  auto_spurs: bool,
  spurs: f64,
  /// Infer horizontal/vertical/tangent/… constraints on the result.
  constraints: bool,
  /// Kerf and minimum-feature check: the cut's width (an expression); empty = off.
  kerf: String,
  auto_threshold: bool,
  threshold: f64,
  invert: bool,
  colors: u32,
  /// Colour layers to draw; `None` = every layer but the background.
  layers: Option<Vec<usize>>,
  specks: f64,
  holes: f64,
  tolerance: f64,
  corners: f64,
  fit: &'static str,
  width: String,
  at: [String; 2],
  angle: String,
  /// The last `sketch.trace` result: size, colour layers, counts.
  pub result: Option<Value>,
  texture: Option<(PathBuf, egui::TextureHandle)>,
  show_image: bool,
}

impl Default for TraceState {
  fn default() -> Self {
    TraceState {
      path: None,
      canvas: None,
      colors_mode: false,
      centerline: false,
      auto_spurs: true,
      spurs: 10.0,
      constraints: true,
      kerf: String::new(),
      auto_threshold: true,
      threshold: 128.0,
      invert: false,
      colors: 4,
      layers: None,
      specks: 16.0,
      holes: 16.0,
      tolerance: 1.0,
      corners: 120.0,
      fit: "curves",
      width: "100 mm".into(),
      at: ["0".into(), "0".into()],
      angle: "0 deg".into(),
      result: None,
      texture: None,
      show_image: true,
    }
  }
}

impl TraceState {
  pub fn with_image(path: PathBuf) -> Self {
    TraceState { path: Some(path), ..Default::default() }
  }

  /// Tracing canvas `id`, whose image is `path`.
  pub fn with_canvas(id: fox_engine::model::FeatureId, path: PathBuf) -> Self {
    TraceState { path: Some(path), canvas: Some(id), show_image: false, ..Default::default() }
  }

  /// `sketch.trace` parameters.
  pub fn params(&self) -> Option<Value> {
    let path = self.path.as_ref()?;
    let num = |s: &str| s.trim().parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(0.0);
    let mode = if self.colors_mode {
      "colors"
    } else if self.centerline {
      "centerline"
    } else {
      "outline"
    };
    let mut p = json!({
      "image": path,
      "mode": mode,
      "constraints": self.constraints,
      "invert": self.invert,
      "specks": self.specks,
      "holes": self.holes,
      "tolerance": self.tolerance,
      "corners": self.corners,
      "fit": self.fit,
      "width": self.width,
      "at": [num(&self.at[0]), num(&self.at[1])],
      "angle": self.angle,
    });
    if self.colors_mode {
      p["colors"] = json!(self.colors);
      if let Some(l) = &self.layers {
        p["layers"] = json!(l);
      }
    } else if !self.auto_threshold {
      p["threshold"] = json!(self.threshold.round());
    }
    if !self.kerf.trim().is_empty() && !self.centerline {
      p["kerf"] = json!(self.kerf.trim());
    }
    if self.centerline && !self.auto_spurs {
      p["spurs"] = json!(self.spurs);
    }
    if let (Some(c), Some(o)) = (self.canvas, p.as_object_mut()) {
      // the canvas says where the image lies and how big it is
      for k in ["image", "width", "at", "angle"] {
        o.remove(k);
      }
      o.insert("canvas".into(), json!(c.0));
    }
    Some(p)
  }

  fn pick_file(&mut self) -> bool {
    let picked = rfd::FileDialog::new().set_title("Image to trace").add_filter("Images", &EXTENSIONS).pick_file();
    match picked {
      Some(p) => {
        self.path = Some(p);
        self.canvas = None;
        self.layers = None;
        self.result = None;
        true
      }
      None => false,
    }
  }

  /// The colour layers of the last trace: (index, "#rrggbb", is background, shapes).
  fn result_layers(&self) -> Vec<(usize, String, bool, u64)> {
    let Some(list) = self.result.as_ref().and_then(|r| r["layers"].as_array()) else { return Vec::new() };
    list
      .iter()
      .filter_map(|l| {
        Some((
          usize::try_from(l["index"].as_u64()?).ok()?,
          l["color"].as_str()?.to_string(),
          l["background"].as_bool().unwrap_or(false),
          l["shapes"].as_u64().unwrap_or(0),
        ))
      })
      .collect()
  }
}

fn color_of(hex: &str) -> Color32 {
  let h = hex.trim_start_matches('#');
  let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok()).unwrap_or(0);
  Color32::from_rgb(byte(0), byte(2), byte(4))
}

/// A slider row that re-previews when released (tracing on every drag step would stutter).
fn slider(ui: &mut egui::Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>, dirty: &mut bool) {
  slider_with(ui, label, egui::Slider::new(v, range), dirty);
}

fn slider_with(ui: &mut egui::Ui, label: &str, s: egui::Slider, dirty: &mut bool) {
  ui.label(label);
  let r = ui.add(s);
  if r.drag_stopped() || (r.changed() && !r.dragged()) {
    *dirty = true;
  }
  ui.end_row();
}

/// A text row that re-previews when the user leaves the field.
fn text(ui: &mut egui::Ui, label: &str, v: &mut String, dirty: &mut bool) {
  ui.label(label);
  if ui.add(egui::TextEdit::singleline(v).desired_width(140.0)).lost_focus() {
    *dirty = true;
  }
  ui.end_row();
}

/// The dialog's rows (inside its two-column grid).
pub fn fields(t: &mut TraceState, ui: &mut egui::Ui, dirty: &mut bool) {
  ui.label(if t.canvas.is_some() { "Canvas" } else { "Image" });
  ui.horizontal(|ui| {
    let name = t.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
    let label = name.as_deref().unwrap_or("Select…");
    let tip = if t.canvas.is_some() {
      "Tracing the canvas where it lies, at its calibrated size; click to trace another image instead"
    } else {
      "Pick a PNG, JPEG, BMP or WebP image"
    };
    if ui.button(label).on_hover_text(tip).clicked() && t.pick_file() {
      *dirty = true;
    }
  });
  ui.end_row();

  ui.label("Mode");
  ui.horizontal(|ui| {
    let outline = !t.colors_mode && !t.centerline;
    if ui.selectable_label(outline, "Outline").on_hover_text("Dark shapes on a light background").clicked() && !outline
    {
      (t.colors_mode, t.centerline) = (false, false);
      *dirty = true;
    }
    if ui.selectable_label(t.colors_mode, "Colours").on_hover_text("Each colour traced as its own shapes").clicked()
      && !t.colors_mode
    {
      (t.colors_mode, t.centerline) = (true, false);
      t.layers = None;
      *dirty = true;
    }
    if ui
      .selectable_label(t.centerline, "Centre line")
      .on_hover_text("Single lines along the middle of the strokes, for engraving and V-carving")
      .clicked()
      && !t.centerline
    {
      (t.colors_mode, t.centerline) = (false, true);
      *dirty = true;
    }
  });
  ui.end_row();

  if t.colors_mode {
    let mut n = t.colors;
    ui.label("Colours");
    let r = ui.add(egui::Slider::new(&mut n, 2..=16));
    if n != t.colors {
      t.colors = n;
      t.layers = None;
    }
    if r.drag_stopped() || (r.changed() && !r.dragged()) {
      *dirty = true;
    }
    ui.end_row();
    let layers = t.result_layers();
    if !layers.is_empty() {
      ui.label("Trace");
      ui.vertical(|ui| {
        for (i, hex, background, shapes) in layers {
          let mut on = match &t.layers {
            Some(l) => l.contains(&i),
            None => !background,
          };
          ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 2.0, color_of(&hex));
            ui.painter().rect_stroke(r, 2.0, egui::Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
            let what = if background { "background".to_string() } else { format!("{shapes} shapes") };
            if ui.checkbox(&mut on, what).changed() {
              let all = t.result_layers();
              let mut chosen: Vec<usize> = match &t.layers {
                Some(l) => l.clone(),
                None => all.iter().filter(|l| !l.2).map(|l| l.0).collect(),
              };
              chosen.retain(|x| *x != i);
              if on {
                chosen.push(i);
                chosen.sort_unstable();
              }
              t.layers = Some(chosen);
              *dirty = true;
            }
          });
        }
      });
      ui.end_row();
    }
  } else {
    ui.label("Threshold");
    ui.horizontal(|ui| {
      if ui.checkbox(&mut t.auto_threshold, "Auto").changed() {
        *dirty = true;
      }
      if !t.auto_threshold {
        let r = ui.add(egui::Slider::new(&mut t.threshold, 0.0..=255.0).integer());
        if r.drag_stopped() || (r.changed() && !r.dragged()) {
          *dirty = true;
        }
      }
    });
    ui.end_row();
    ui.label("");
    if ui.checkbox(&mut t.invert, "Invert (light shapes on dark)").changed() {
      *dirty = true;
    }
    ui.end_row();
    if t.centerline {
      ui.label("Ignore spurs");
      ui.horizontal(|ui| {
        if ui
          .checkbox(&mut t.auto_spurs, "Auto")
          .on_hover_text("Side branches shorter than 1.5 × the stroke width are thinning noise")
          .changed()
        {
          *dirty = true;
        }
        if !t.auto_spurs {
          let r = ui.add(egui::Slider::new(&mut t.spurs, 0.0..=100.0).suffix(" px"));
          if r.drag_stopped() || (r.changed() && !r.dragged()) {
            *dirty = true;
          }
        }
      });
      ui.end_row();
    }
  }

  slider_with(
    ui,
    "Ignore specks",
    egui::Slider::new(&mut t.specks, 0.0..=2000.0).logarithmic(true).suffix(" px²"),
    dirty,
  );
  slider_with(ui, "Fill holes", egui::Slider::new(&mut t.holes, 0.0..=2000.0).logarithmic(true).suffix(" px²"), dirty);
  slider_with(
    ui,
    "Detail",
    egui::Slider::new(&mut t.tolerance, 0.2..=6.0)
      .logarithmic(true)
      .suffix(" px")
      .custom_formatter(|v, _| format!("{v:.1}")),
    dirty,
  );
  slider(ui, "Corners", &mut t.corners, 60.0..=170.0, dirty);

  ui.label("Curves");
  let shown = FITS.iter().find(|(k, _)| *k == t.fit).map(|(_, l)| *l).unwrap_or(t.fit);
  egui::ComboBox::from_id_salt("trace-fit").selected_text(shown).width(150.0).show_ui(ui, |ui| {
    for (k, l) in FITS {
      if ui.selectable_label(t.fit == k, l).clicked() {
        t.fit = k;
        *dirty = true;
      }
    }
  });
  ui.end_row();

  if t.canvas.is_none() {
    text(ui, "Width", &mut t.width, dirty);
    ui.label("Centre X, Y");
    ui.horizontal(|ui| {
      for v in &mut t.at {
        if ui.add(egui::TextEdit::singleline(v).desired_width(66.0)).lost_focus() {
          *dirty = true;
        }
      }
    });
    ui.end_row();
    text(ui, "Angle", &mut t.angle, dirty);
  }

  if !t.centerline {
    ui.label("Kerf / tool");
    let r = ui
      .add(egui::TextEdit::singleline(&mut t.kerf).hint_text("off").desired_width(140.0))
      .on_hover_text("The cut's width (laser kerf, router bit): marks the details, gaps and holes it can't make");
    if r.lost_focus() {
      *dirty = true;
    }
    ui.end_row();
  }
  ui.label("");
  if ui
    .checkbox(&mut t.constraints, "Add constraints")
    .on_hover_text("Horizontal, vertical, tangent, coincident, equal and concentric, where the trace is unambiguous")
    .changed()
  {
    *dirty = true;
  }
  ui.end_row();
  ui.label("");
  ui.checkbox(&mut t.show_image, "Show image");
  ui.end_row();

  if let Some(r) = &t.result {
    let n = |k: &str| r[k].as_u64().unwrap_or(0);
    let curves = r["curves"].as_array().map(Vec::len).unwrap_or(0);
    let size = |i: usize| r["size"][i].as_f64().unwrap_or(0.0);
    ui.label("");
    let text = if t.centerline {
      if n("strokes") == 0 {
        RichText::new("Nothing traced").color(theme::WARNING)
      } else {
        RichText::new(format!(
          "{} strokes, {curves} curves, ≈ {:.2} mm wide\n{:.1} × {:.1} mm",
          n("strokes"),
          r["stroke_width"].as_f64().unwrap_or(0.0),
          size(0),
          size(1)
        ))
        .color(theme::MUTED)
      }
    } else if n("shapes") == 0 {
      RichText::new("Nothing traced").color(theme::WARNING)
    } else {
      RichText::new(format!(
        "{} shapes, {} holes, {curves} curves\n{:.1} × {:.1} mm, all closed",
        n("shapes"),
        n("holes"),
        size(0),
        size(1)
      ))
      .color(theme::MUTED)
    };
    ui.label(text);
    ui.end_row();
    if let Some(c) = r["constraint_counts"].as_object().filter(|c| !c.is_empty()) {
      let list: Vec<String> = c.iter().map(|(k, v)| format!("{} {}", v, k.replace('_', " "))).collect();
      ui.label("");
      ui.label(RichText::new(format!("Constraints: {}", list.join(", "))).color(theme::MUTED));
      ui.end_row();
    }
    if r["narrow"].as_array().is_some_and(Vec::is_empty) {
      ui.label("");
      ui.label(RichText::new("Nothing narrower than the kerf").color(theme::MUTED));
      ui.end_row();
    }
    for w in [r["warning"].as_str(), r["constraint_warning"].as_str(), r["kerf_warning"].as_str()].into_iter().flatten()
    {
      ui.label("");
      ui.label(RichText::new(w).color(theme::WARNING));
      ui.end_row();
    }
  }
}

fn load_texture(ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
  let img = image::open(path).ok()?;
  let img = if img.width().max(img.height()) > MAX_TEXTURE { img.thumbnail(MAX_TEXTURE, MAX_TEXTURE) } else { img };
  let rgba = img.to_rgba8();
  let size = [rgba.width() as usize, rgba.height() as usize];
  let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
  Some(ctx.load_texture("trace-image", color, egui::TextureOptions::LINEAR))
}

/// Draws the image being traced on the sketch plane, where the trace lands, half transparent.
/// `to_screen` projects a world point into the viewport.
pub fn overlay(app: &mut App, painter: &egui::Painter, to_screen: &dyn Fn(DVec3) -> Option<Pos2>) {
  let Some(sk) = app.s.edit_sketch else { return };
  let Some(plane) = app.s.model.sketches.get(&sk).map(|r| r.plane) else { return };
  let Some(t) = app.dialog.as_mut().and_then(|d| d.trace.as_deref_mut()) else { return };
  let (Some(path), Some(r)) = (t.path.clone(), t.result.as_ref()) else { return };
  // the kerf check's spots, ringed in red at their width (at least big enough to see)
  for n in r["narrow"].as_array().into_iter().flatten() {
    let (Some(x), Some(y)) = (n["at"][0].as_f64(), n["at"][1].as_f64()) else { continue };
    let w = n["width"].as_f64().unwrap_or(0.0);
    let (Some(c), Some(e)) = (to_screen(plane.to_world([x, y])), to_screen(plane.to_world([x + w, y]))) else {
      continue;
    };
    let radius = (c.distance(e)).max(7.0);
    painter.circle_stroke(c, radius, egui::Stroke::new(2.0, theme::ERROR));
  }
  if !t.show_image {
    return;
  }
  // where the engine put the trace
  let (Some(w), Some(h)) = (r["size"][0].as_f64(), r["size"][1].as_f64()) else { return };
  let at = [r["at"][0].as_f64().unwrap_or(0.0), r["at"][1].as_f64().unwrap_or(0.0)];
  let angle = r["angle"].as_f64().unwrap_or(0.0);
  if t.texture.as_ref().is_none_or(|(p, _)| *p != path) {
    t.texture = load_texture(painter.ctx(), &path).map(|tex| (path.clone(), tex));
  }
  let Some((_, tex)) = &t.texture else { return };
  let placed = Placed { plane, at, size: [w, h], angle };
  if let Some(mesh) = image_mesh(tex.id(), &placed, Color32::from_white_alpha(110), to_screen) {
    painter.add(mesh);
  }
}

/// An image lying on a plane: its centre `at` (plane coordinates), `size` (mm) and rotation
/// `angle` (radians) about the centre.
#[derive(Debug, Clone, Copy)]
pub struct Placed {
  pub plane: fox_engine::kernel::Plane,
  pub at: [f64; 2],
  pub size: [f64; 2],
  pub angle: f64,
}

impl Placed {
  /// Plane coordinates of a point of the image: `u`, `v` in 0..1 across it (v down, as in the
  /// file).
  pub fn local(&self, u: f64, v: f64) -> [f64; 2] {
    let (s, c) = self.angle.sin_cos();
    let (x, y) = ((u - 0.5) * self.size[0], (0.5 - v) * self.size[1]);
    [self.at[0] + x * c - y * s, self.at[1] + x * s + y * c]
  }

  /// Whether a point (plane coordinates) lies on the image.
  pub fn contains(&self, p: [f64; 2]) -> bool {
    let (s, c) = self.angle.sin_cos();
    let (dx, dy) = (p[0] - self.at[0], p[1] - self.at[1]);
    let (x, y) = (dx * c + dy * s, -dx * s + dy * c);
    x.abs() <= self.size[0] / 2.0 && y.abs() <= self.size[1] / 2.0
  }
}

/// The image `tex` drawn where `placed` puts it, tinted (`tint`'s alpha is its opacity). A grid
/// of quads, so perspective views stay close to right. None when part of it is off the view.
pub fn image_mesh(
  tex: egui::TextureId,
  placed: &Placed,
  tint: Color32,
  to_screen: &dyn Fn(DVec3) -> Option<Pos2>,
) -> Option<egui::Shape> {
  const N: usize = 8;
  let mut mesh = egui::Mesh::with_texture(tex);
  for j in 0..=N {
    for i in 0..=N {
      let (u, v) = (i as f64 / N as f64, j as f64 / N as f64);
      let p = to_screen(placed.plane.to_world(placed.local(u, v)))?;
      mesh.vertices.push(egui::epaint::Vertex { pos: p, uv: Pos2::new(u as f32, v as f32), color: tint });
    }
  }
  for j in 0..N {
    for i in 0..N {
      let k = (j * (N + 1) + i) as u32;
      let n = (N + 1) as u32;
      mesh.add_triangle(k, k + 1, k + n);
      mesh.add_triangle(k + 1, k + n + 1, k + n);
    }
  }
  Some(egui::Shape::mesh(mesh))
}

/// An image file as a texture (scaled down to at most [`MAX_TEXTURE`] across).
pub fn texture(ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
  load_texture(ctx, path)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn params_carry_mode_constraints_and_kerf() {
    let mut t = TraceState::with_image(PathBuf::from("logo.png"));
    let p = t.params().unwrap();
    assert_eq!(p["mode"], "outline");
    assert_eq!(p["constraints"], true);
    assert!(p.get("kerf").is_none(), "{p}");
    t.kerf = " 0.2 mm ".into();
    t.constraints = false;
    let p = t.params().unwrap();
    assert_eq!(p["kerf"], "0.2 mm");
    assert_eq!(p["constraints"], false);
    // centre lines have no kerf check, and take a spur length only when it isn't automatic
    (t.centerline, t.auto_spurs, t.spurs) = (true, false, 6.0);
    let p = t.params().unwrap();
    assert_eq!(p["mode"], "centerline");
    assert!(p.get("kerf").is_none(), "{p}");
    assert_eq!(p["spurs"], 6.0);
    // a canvas supplies the image and its place
    let c = TraceState::with_canvas(fox_engine::model::FeatureId(7), PathBuf::from("photo.png"));
    let p = c.params().unwrap();
    assert_eq!(p["canvas"], 7);
    assert!(p.get("image").is_none() && p.get("width").is_none() && p.get("at").is_none(), "{p}");
  }
}
