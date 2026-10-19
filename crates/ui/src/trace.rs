//! Trace Image: the dialog's settings and the traced image drawn on the sketch plane under the
//! preview. The tracing itself is the engine's `sketch.trace`, previewed like any other dialog.

use std::path::{Path, PathBuf};

use clf_engine::kernel::glam::DVec3;
use egui::{Color32, Pos2, RichText};
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
  colors_mode: bool,
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
      colors_mode: false,
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

  /// `sketch.trace` parameters.
  pub fn params(&self) -> Option<Value> {
    let path = self.path.as_ref()?;
    let num = |s: &str| s.trim().parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(0.0);
    let mut p = json!({
      "image": path,
      "mode": if self.colors_mode { "colors" } else { "outline" },
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
    Some(p)
  }

  fn pick_file(&mut self) -> bool {
    let picked = rfd::FileDialog::new().set_title("Image to trace").add_filter("Images", &EXTENSIONS).pick_file();
    match picked {
      Some(p) => {
        self.path = Some(p);
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
  ui.label("Image");
  ui.horizontal(|ui| {
    let name = t.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
    let label = name.as_deref().unwrap_or("Select…");
    if ui.button(label).on_hover_text("Pick a PNG, JPEG, BMP or WebP image").clicked() && t.pick_file() {
      *dirty = true;
    }
  });
  ui.end_row();

  ui.label("Mode");
  ui.horizontal(|ui| {
    if ui.selectable_label(!t.colors_mode, "Outline").on_hover_text("Dark shapes on a light background").clicked()
      && t.colors_mode
    {
      t.colors_mode = false;
      *dirty = true;
    }
    if ui.selectable_label(t.colors_mode, "Colours").on_hover_text("Each colour traced as its own shapes").clicked()
      && !t.colors_mode
    {
      t.colors_mode = true;
      t.layers = None;
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

  ui.label("");
  ui.checkbox(&mut t.show_image, "Show image");
  ui.end_row();

  if let Some(r) = &t.result {
    let n = |k: &str| r[k].as_u64().unwrap_or(0);
    let curves = r["curves"].as_array().map(Vec::len).unwrap_or(0);
    let size = |i: usize| r["size"][i].as_f64().unwrap_or(0.0);
    ui.label("");
    let text = if n("shapes") == 0 {
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
    if let Some(w) = r["warning"].as_str() {
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
  let (s, c) = angle.sin_cos();
  // u, v in 0..1 across the image (v down, as in the file) → world
  let world = |u: f64, v: f64| {
    let (x, y) = ((u - 0.5) * w, (0.5 - v) * h);
    plane.to_world([at[0] + x * c - y * s, at[1] + x * s + y * c])
  };
  // a grid of quads, so perspective views stay close to right
  const N: usize = 8;
  let mut mesh = egui::Mesh::with_texture(tex.id());
  let tint = Color32::from_white_alpha(110);
  for j in 0..=N {
    for i in 0..=N {
      let (u, v) = (i as f64 / N as f64, j as f64 / N as f64);
      let Some(p) = to_screen(world(u, v)) else { return };
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
  painter.add(egui::Shape::mesh(mesh));
}
