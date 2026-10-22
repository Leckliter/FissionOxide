//! Canvases in the UI: Insert > Canvas, the images drawn on their planes, right-click Edit Canvas
//! (opacity, size, position) and Calibrate (pick two points on the image, type the real distance
//! between them, and the image rescales). The canvas itself is the engine's `canvas.*` commands.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Color32, Pos2, RichText, Stroke};
use fox_engine::Sel;
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::{FeatureId, FeatureKind};
use fox_engine::render::ray_plane;
use serde_json::{Value, json};

use crate::trace::Placed;
use crate::{App, theme};

#[derive(Default)]
pub struct CanvasUi {
  insert: Option<InsertForm>,
  edit: Option<EditForm>,
  calibrate: Option<Calibrate>,
  /// Image textures by file (None: the file couldn't be read).
  textures: HashMap<String, Option<egui::TextureHandle>>,
}

struct InsertForm {
  path: Option<PathBuf>,
  /// Index into [`plane_choices`].
  plane: usize,
  width: String,
  opacity: f32,
}

struct EditForm {
  feature: FeatureId,
  opacity: f32,
  width: String,
  at: [String; 2],
  angle: String,
}

/// Calibrate in progress: the points picked so far (plane coordinates) and the distance typed.
struct Calibrate {
  feature: FeatureId,
  from: Option<[f64; 2]>,
  to: Option<[f64; 2]>,
  distance: String,
}

/// A canvas of the design as it lies now.
#[derive(Debug, Clone)]
pub struct Shown {
  pub id: FeatureId,
  pub image: String,
  pub placed: Placed,
  pub opacity: f64,
}

impl CanvasUi {
  /// Esc: stops Calibrate or closes a canvas window. False when none was open.
  pub fn cancel(&mut self) -> bool {
    let open = self.insert.is_some() || self.edit.is_some() || self.calibrate.is_some();
    self.insert = None;
    self.edit = None;
    self.calibrate = None;
    open
  }
}

/// The canvases to draw: not suppressed, regenerated, on a plane.
pub fn shown(app: &App) -> Vec<Shown> {
  app
    .s
    .design
    .features
    .iter()
    .filter(|f| !f.suppressed && f.visible != Some(false))
    .filter_map(|f| {
      let FeatureKind::Canvas { image, at, width, height, angle, opacity, .. } = &f.kind else { return None };
      let plane = *app.s.model.canvases.get(&f.id)?;
      let placed = Placed { plane, at: *at, size: [*width, *height], angle: angle.to_radians() };
      Some(Shown { id: f.id, image: image.clone(), placed, opacity: *opacity })
    })
    .collect()
}

/// Insert > Canvas, once the Insert window picked the image: the form, with the selected planar face or
/// construction plane as the plane.
pub fn start_insert_with(app: &mut App, path: Option<PathBuf>) {
  app.cancel_dialog();
  let choices = plane_choices(app);
  let plane = choices.iter().position(|(_, p)| p.get("face").is_some() || p.get("feature").is_some()).unwrap_or(0);
  app.canvas_ui.calibrate = None;
  app.canvas_ui.edit = None;
  app.canvas_ui.insert = Some(InsertForm { path, plane, width: "100".into(), opacity: 0.5 });
}

/// Right-click > Edit Canvas.
pub fn start_edit(app: &mut App, id: FeatureId) {
  let Some(FeatureKind::Canvas { at, width, angle, opacity, .. }) = app.s.design.feature(id).map(|f| &f.kind) else {
    return;
  };
  let num = |v: f64| fox_engine::expr::trim_number(v);
  let form = EditForm {
    feature: id,
    opacity: *opacity as f32,
    width: num(*width),
    at: [num(at[0]), num(at[1])],
    angle: num(*angle),
  };
  app.canvas_ui.insert = None;
  app.canvas_ui.calibrate = None;
  app.canvas_ui.edit = Some(form);
}

/// Right-click > Calibrate: pick two points on the image, then type their distance.
pub fn start_calibrate(app: &mut App, id: FeatureId) {
  app.cancel_dialog();
  app.canvas_ui.insert = None;
  app.canvas_ui.edit = None;
  app.canvas_ui.calibrate = Some(Calibrate { feature: id, from: None, to: None, distance: String::new() });
  app.status = Some(("Calibrate: click the first point on the canvas".into(), false));
}

/// Origin planes, construction planes and the selected face, as (label, `canvas.insert` plane).
fn plane_choices(app: &App) -> Vec<(String, Value)> {
  let mut v: Vec<(String, Value)> =
    vec![("XY plane".into(), json!("xy")), ("XZ plane".into(), json!("xz")), ("YZ plane".into(), json!("yz"))];
  for id in app.s.model.planes.keys() {
    let name = app.s.design.feature(*id).map(|f| f.name.clone()).unwrap_or_default();
    v.push((name, json!({"feature": id.0})));
  }
  for s in &app.s.selection {
    if let Sel::Face { body, face } = s {
      v.push((format!("Selected face ({}:{face})", body.0), json!({"body": body.0, "face": face})));
    }
  }
  v
}

fn texture<'a>(
  textures: &'a mut HashMap<String, Option<egui::TextureHandle>>,
  ctx: &egui::Context,
  image: &str,
) -> Option<&'a egui::TextureHandle> {
  textures.entry(image.to_string()).or_insert_with(|| crate::trace::texture(ctx, Path::new(image))).as_ref()
}

/// Draws every canvas on its plane, at its opacity (Edit Canvas's slider shows live), and
/// Calibrate's picked points.
pub fn overlay(app: &mut App, painter: &egui::Painter, to_screen: &dyn Fn(DVec3) -> Option<Pos2>) {
  let canvases = shown(app);
  let editing = app.canvas_ui.edit.as_ref().map(|e| (e.feature, e.opacity));
  for c in &canvases {
    let opacity = match editing {
      Some((f, o)) if f == c.id => f64::from(o),
      _ => c.opacity,
    };
    let Some(tex) = texture(&mut app.canvas_ui.textures, painter.ctx(), &c.image) else { continue };
    let tint = Color32::from_white_alpha((opacity.clamp(0.0, 1.0) * 255.0).round() as u8);
    if let Some(mesh) = crate::trace::image_mesh(tex.id(), &c.placed, tint, to_screen) {
      painter.add(mesh);
    }
  }
  // Calibrate: the picked points and the line between them
  let Some(cal) = &app.canvas_ui.calibrate else { return };
  let Some(c) = canvases.iter().find(|c| c.id == cal.feature) else { return };
  let screen = |p: [f64; 2]| to_screen(c.placed.plane.to_world(p));
  let ink = Color32::from_rgb(40, 120, 230);
  if let (Some(a), Some(b)) = (cal.from.and_then(screen), cal.to.and_then(screen)) {
    painter.line_segment([a, b], Stroke::new(2.0, ink));
  }
  for p in [cal.from, cal.to].into_iter().flatten().filter_map(screen) {
    painter.circle(p, 4.5, Color32::WHITE, Stroke::new(2.0, ink));
  }
}

/// The canvas under a screen ray (nearest first), with the hit in its plane coordinates.
pub fn canvas_at(app: &App, origin: DVec3, dir: DVec3) -> Option<(FeatureId, [f64; 2])> {
  let mut best: Option<(f64, FeatureId, [f64; 2])> = None;
  for c in shown(app) {
    let pl = c.placed.plane;
    let Some(hit) = ray_plane(origin, dir, pl.origin, pl.normal()) else { continue };
    let t = (hit - origin).dot(dir);
    let local = pl.to_local(hit);
    if t > 0.0 && c.placed.contains(local) && best.as_ref().is_none_or(|b| t < b.0) {
      best = Some((t, c.id, local));
    }
  }
  best.map(|(_, id, p)| (id, p))
}

/// Calibrate's picks in the viewport. Returns true when it took the click.
pub fn calibrate_clicks(app: &mut App, resp: &egui::Response, ray: &dyn Fn(Pos2) -> (DVec3, DVec3)) -> bool {
  let Some(cal) = &app.canvas_ui.calibrate else { return false };
  if cal.to.is_some() {
    return false;
  }
  let feature = cal.feature;
  let hit = resp.hover_pos().and_then(|p| {
    let (o, d) = ray(p);
    let c = shown(app).into_iter().find(|c| c.id == feature)?;
    let pl = c.placed.plane;
    Some(pl.to_local(ray_plane(o, d, pl.origin, pl.normal())?))
  });
  if hit.is_some() {
    resp.clone().on_hover_cursor(egui::CursorIcon::Crosshair);
  }
  if !resp.clicked() {
    return hit.is_some();
  }
  let Some(p) = hit else { return false };
  let Some(cal) = app.canvas_ui.calibrate.as_mut() else { return false };
  if cal.from.is_none() {
    cal.from = Some(p);
    app.status = Some(("Calibrate: click the second point".into(), false));
  } else {
    cal.to = Some(p);
    app.status = Some(("Calibrate: type the real distance between the points".into(), false));
  }
  true
}

/// The Canvas, Edit Canvas and Calibrate windows.
pub fn windows(app: &mut App, ctx: &egui::Context) {
  insert_window(app, ctx);
  edit_window(app, ctx);
  calibrate_window(app, ctx);
}

fn header(ui: &mut egui::Ui, title: &str) {
  ui.label(RichText::new(title).strong().color(theme::TEXT));
  ui.separator();
}

fn insert_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut form) = app.canvas_ui.insert.take() else { return };
  let choices = plane_choices(app);
  form.plane = form.plane.min(choices.len() - 1);
  let (mut ok, mut cancel) = (false, false);
  egui::Window::new("CANVAS")
    .id(egui::Id::new("canvas-insert"))
    .title_bar(false)
    .resizable(false)
    .default_pos(app.canvas.right_top() + egui::vec2(-300.0, 60.0))
    .show(ctx, |ui| {
      ui.set_width(260.0);
      header(ui, "CANVAS");
      egui::Grid::new("canvas-insert-grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("Image");
        let name = form.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
        if ui.button(name.as_deref().unwrap_or("Select…")).on_hover_text("A PNG, JPEG, BMP or WebP image").clicked()
          && let Some(p) =
            rfd::FileDialog::new().set_title("Canvas image").add_filter("Images", &crate::trace::EXTENSIONS).pick_file()
        {
          form.path = Some(p);
        }
        ui.end_row();
        ui.label("Plane");
        egui::ComboBox::from_id_salt("canvas-plane").selected_text(choices[form.plane].0.clone()).width(150.0).show_ui(
          ui,
          |ui| {
            for (i, (label, _)) in choices.iter().enumerate() {
              ui.selectable_value(&mut form.plane, i, label);
            }
          },
        );
        ui.end_row();
        ui.label("Width (mm)");
        ui.add(egui::TextEdit::singleline(&mut form.width).desired_width(80.0));
        ui.end_row();
        ui.label("Opacity");
        ui.add(egui::Slider::new(&mut form.opacity, 0.0..=1.0).fixed_decimals(2));
        ui.end_row();
      });
      ui.label(RichText::new("Right-click the canvas to Calibrate its size").color(theme::MUTED));
      ui.separator();
      ui.horizontal(|ui| {
        ok = ui.add_enabled(form.path.is_some(), egui::Button::new("OK")).clicked();
        cancel = ui.button("Cancel").clicked();
      });
    });
  if cancel {
    return;
  }
  if ok && let Some(path) = &form.path {
    let mut p = json!({"image": path, "plane": choices[form.plane].1, "opacity": form.opacity});
    if let Ok(w) = form.width.trim().parse::<f64>() {
      p["width"] = json!(w);
    }
    if app.run("canvas.insert", p).is_some() {
      return;
    }
  }
  app.canvas_ui.insert = Some(form);
}

fn edit_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut form) = app.canvas_ui.edit.take() else { return };
  let name = app.s.design.feature(form.feature).map(|f| f.name.clone());
  let Some(name) = name else { return };
  let (mut ok, mut cancel, mut calibrate) = (false, false, false);
  egui::Window::new("EDIT CANVAS")
    .id(egui::Id::new("canvas-edit"))
    .title_bar(false)
    .resizable(false)
    .default_pos(app.canvas.right_top() + egui::vec2(-300.0, 60.0))
    .show(ctx, |ui| {
      ui.set_width(260.0);
      header(ui, &format!("EDIT CANVAS — {name}"));
      egui::Grid::new("canvas-edit-grid").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("Opacity");
        ui.add(egui::Slider::new(&mut form.opacity, 0.0..=1.0).fixed_decimals(2));
        ui.end_row();
        ui.label("Width (mm)");
        ui.add(egui::TextEdit::singleline(&mut form.width).desired_width(80.0));
        ui.end_row();
        ui.label("Centre X, Y");
        ui.horizontal(|ui| {
          for v in &mut form.at {
            ui.add(egui::TextEdit::singleline(v).desired_width(56.0));
          }
        });
        ui.end_row();
        ui.label("Angle (deg)");
        ui.add(egui::TextEdit::singleline(&mut form.angle).desired_width(80.0));
        ui.end_row();
      });
      calibrate = ui.button("Calibrate…").on_hover_text("Scale the image from a known distance").clicked();
      ui.separator();
      ui.horizontal(|ui| {
        ok = ui.button("OK").clicked();
        cancel = ui.button("Cancel").clicked();
      });
    });
  if cancel {
    return;
  }
  if calibrate {
    start_calibrate(app, form.feature);
    return;
  }
  if ok {
    let num = |s: &str| s.trim().parse::<f64>().ok().filter(|v| v.is_finite());
    let mut p = json!({"feature": form.feature.0, "opacity": form.opacity});
    if let Some(w) = num(&form.width) {
      p["width"] = json!(w);
    }
    if let (Some(x), Some(y)) = (num(&form.at[0]), num(&form.at[1])) {
      p["at"] = json!([x, y]);
    }
    if let Some(a) = num(&form.angle) {
      p["angle"] = json!(a);
    }
    if app.run("canvas.edit", p).is_some() {
      return;
    }
  }
  app.canvas_ui.edit = Some(form);
}

fn calibrate_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut cal) = app.canvas_ui.calibrate.take() else { return };
  let (mut ok, mut cancel) = (false, false);
  egui::Window::new("CALIBRATE")
    .id(egui::Id::new("canvas-calibrate"))
    .title_bar(false)
    .resizable(false)
    .default_pos(app.canvas.right_top() + egui::vec2(-300.0, 60.0))
    .show(ctx, |ui| {
      ui.set_width(240.0);
      header(ui, "CALIBRATE");
      let step = match (cal.from, cal.to) {
        (None, _) => "Click the first point on the canvas",
        (Some(_), None) => "Click the second point",
        _ => "Type the real distance between the points",
      };
      ui.label(RichText::new(step).color(theme::MUTED));
      if let (Some(a), Some(b)) = (cal.from, cal.to) {
        let picked = (b[0] - a[0]).hypot(b[1] - a[1]);
        ui.label(RichText::new(format!("Now {picked:.2} mm apart")).color(theme::MUTED));
        ui.horizontal(|ui| {
          ui.label("Distance (mm)");
          let r = ui.add(egui::TextEdit::singleline(&mut cal.distance).desired_width(80.0));
          if cal.distance.is_empty() {
            r.request_focus();
          }
          ok |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        });
      }
      ui.separator();
      ui.horizontal(|ui| {
        let ready = cal.to.is_some() && cal.distance.trim().parse::<f64>().is_ok_and(|d| d > 0.0);
        ok = (ui.add_enabled(ready, egui::Button::new("OK")).clicked() || ok) && ready;
        cancel = ui.button("Cancel").clicked();
      });
    });
  if cancel {
    app.status = None;
    return;
  }
  if ok
    && let (Some(from), Some(to), Ok(d)) = (cal.from, cal.to, cal.distance.trim().parse::<f64>())
    && app.run("canvas.calibrate", json!({"feature": cal.feature.0, "from": from, "to": to, "distance": d})).is_some()
  {
    app.status = Some(("Canvas calibrated".into(), false));
    return;
  }
  app.canvas_ui.calibrate = Some(cal);
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  fn png(dir: &Path) -> PathBuf {
    let p = dir.join("canvas.png");
    image::RgbaImage::from_pixel(40, 20, image::Rgba([10, 20, 30, 255])).save(&p).unwrap();
    p
  }

  fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fox-ui-canvas-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
  }

  #[test]
  fn calibrate_picks_two_points_and_rescales() {
    let dir = temp_dir("calibrate");
    let mut app = App::with_session(Session::default());
    let r = app.run("canvas.insert", json!({"image": png(&dir), "plane": "xy", "width": 100})).unwrap();
    let id = FeatureId(r["feature"].as_u64().unwrap() as u32);
    // the canvas is 100 × 50 on XY; a ray straight down hits it
    let down = DVec3::new(0.0, 0.0, -1.0);
    assert_eq!(canvas_at(&app, DVec3::new(10.0, 5.0, 50.0), down).map(|h| h.0), Some(id));
    assert_eq!(canvas_at(&app, DVec3::new(10.0, 30.0, 50.0), down), None);

    start_calibrate(&mut app, id);
    let cal = app.canvas_ui.calibrate.as_mut().unwrap();
    cal.from = Some([0.0, 0.0]);
    cal.to = Some([20.0, 0.0]);
    cal.distance = "40".into();
    let (from, to) = (cal.from.unwrap(), cal.to.unwrap());
    app.run("canvas.calibrate", json!({"feature": id.0, "from": from, "to": to, "distance": 40}));
    let c = shown(&app).into_iter().find(|c| c.id == id).unwrap();
    assert!((c.placed.size[0] - 200.0).abs() < 1e-9 && (c.placed.size[1] - 100.0).abs() < 1e-9, "{c:?}");
    // Esc ends it
    assert!(app.canvas_ui.cancel());
    assert!(!app.canvas_ui.cancel());
    let _ = std::fs::remove_dir_all(dir);
  }

  #[test]
  fn hidden_canvases_are_not_drawn_and_canvases_trace_where_they_lie() {
    let dir = temp_dir("trace");
    let mut app = App::with_session(Session::default());
    let r = app.run("canvas.insert", json!({"image": png(&dir), "plane": "xz", "width": 80})).unwrap();
    let id = FeatureId(r["feature"].as_u64().unwrap() as u32);
    app.run("canvas.edit", json!({"feature": id.0, "visible": false}));
    assert!(shown(&app).is_empty());
    app.run("canvas.edit", json!({"feature": id.0, "visible": true}));
    assert_eq!(shown(&app).len(), 1);
    // Trace Canvas with no sketch open starts one on the canvas's plane and previews the trace
    app.trace_canvas(id);
    let sk = app.s.edit_sketch.expect("a sketch on the canvas's plane");
    match app.s.design.feature(sk).map(|f| &f.kind) {
      Some(FeatureKind::Sketch { plane, .. }) => {
        assert!(matches!(plane, fox_engine::model::PlaneRef::Origin { plane: fox_engine::model::OriginPlane::Xz }))
      }
      k => panic!("{k:?}"),
    }
    let t = app.dialog.as_ref().and_then(|d| d.trace.as_ref()).unwrap();
    let p = t.params().unwrap();
    assert_eq!(p["canvas"], json!(id.0), "{p}");
    assert!(p.get("width").is_none() && p.get("image").is_none(), "{p}");
    // the dark image traces to one 80 × 40 rectangle
    let r = t.result.as_ref().expect("previewed");
    assert_eq!(r["shapes"], 1, "{r}");
    assert_eq!(r["size"], json!([80.0, 40.0]), "{r}");
    let _ = std::fs::remove_dir_all(dir);
  }

  #[test]
  fn placed_images_map_corners_and_rotation() {
    let placed =
      Placed { plane: fox_engine::kernel::Plane::XY, at: [10.0, 0.0], size: [20.0, 10.0], angle: 90f64.to_radians() };
    // the image's top-left corner, turned a quarter turn about the centre
    let tl = placed.local(0.0, 0.0);
    assert!((tl[0] - 5.0).abs() < 1e-9 && (tl[1] + 10.0).abs() < 1e-9, "{tl:?}");
    assert!(placed.contains([10.0, 9.0]) && !placed.contains([19.0, 0.0]));
  }
}
