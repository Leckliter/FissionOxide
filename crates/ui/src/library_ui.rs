//! Insert > Sketch Library: a picker of the library's parts (`sketch.library.list`), then clicks in
//! the sketch place the chosen part there (`sketch.library.insert`), as often as you like, turned
//! by the angle in the picker. Folders listed in `settings.ini` under `[Library]` are library roots
//! too; Add folder writes them there.

use egui::{Color32, Pos2, Rect, RichText, Stroke, vec2};
use fox_desktop::settings::Settings;
use fox_engine::kernel::glam::DVec3;
use fox_engine::render::ray_plane;
use fox_engine::scene::Pick;
use serde_json::{Value, json};

use crate::{App, theme};

const SECTION: &str = "Library";
const TILE: f32 = 76.0;

/// The picker's state.
#[derive(Debug, Clone)]
pub struct LibraryUi {
  pub open: bool,
  parts: Vec<Value>,
  roots: Vec<Value>,
  packs: Vec<String>,
  /// Shown pack (`None`: all of them) and a name filter.
  pack: Option<String>,
  filter: String,
  /// The part being placed: clicks in the sketch drop it.
  chosen: Option<usize>,
  /// Degrees, counter-clockwise; and the scale factor.
  angle: String,
  scale: String,
  placed: usize,
}

impl Default for LibraryUi {
  fn default() -> Self {
    LibraryUi {
      open: false,
      parts: Vec::new(),
      roots: Vec::new(),
      packs: Vec::new(),
      pack: None,
      filter: String::new(),
      chosen: None,
      angle: "0".into(),
      scale: "1".into(),
      placed: 0,
    }
  }
}

impl LibraryUi {
  /// Esc: stop placing, then close the picker. False when it had nothing to stop.
  pub fn cancel(&mut self) -> bool {
    if self.chosen.take().is_some() {
      return true;
    }
    std::mem::replace(&mut self.open, false)
  }

  fn part(&self) -> Option<&Value> {
    self.parts.get(self.chosen?)
  }

  fn angle_deg(&self) -> f64 {
    self.angle.trim().parse().unwrap_or(0.0)
  }

  fn scale_factor(&self) -> f64 {
    self.scale.trim().parse().ok().filter(|s: &f64| *s > 0.0).unwrap_or(1.0)
  }
}

/// The `[Library]` folders of `settings.ini`.
fn settings_folders(settings: &Settings) -> Vec<String> {
  settings.section(SECTION).into_iter().filter(|(k, _)| k.starts_with("Folder")).map(|(_, v)| v).collect()
}

fn save_folders(folders: &[String]) {
  let mut settings = Settings::load();
  let entries: Vec<(String, String)> =
    folders.iter().enumerate().map(|(i, f)| (format!("Folder{}", i + 1), f.clone())).collect();
  settings.set_section(SECTION, &entries);
  let _ = settings.save();
}

/// Reads the library again (the settings' folders are added as roots first).
fn refresh(app: &mut App) {
  for f in settings_folders(&Settings::load()) {
    fox_engine::library::add_root(f);
  }
  match app.s.execute("sketch.library.list", &json!({})) {
    Ok(v) => {
      let lib = &mut app.library_ui;
      lib.parts = v["parts"].as_array().cloned().unwrap_or_default();
      lib.roots = v["roots"].as_array().cloned().unwrap_or_default();
      lib.packs =
        v["packs"].as_array().into_iter().flatten().filter_map(|p| p["name"].as_str().map(String::from)).collect();
      lib.chosen = None;
    }
    Err(e) => app.status = Some((e.0, true)),
  }
}

/// Insert > Sketch Library.
pub fn open(app: &mut App) {
  app.cancel_dialog();
  app.library_ui.open = true;
  app.library_ui.placed = 0;
  refresh(app);
  if app.s.edit_sketch.is_none() {
    app.status = Some(("Sketch Library: edit a sketch to place parts in it".into(), false));
  }
}

/// A part's outline polylines.
fn outline(part: &Value) -> Vec<Vec<[f64; 2]>> {
  part["outline"]
    .as_array()
    .into_iter()
    .flatten()
    .map(|l| {
      l.as_array().into_iter().flatten().filter_map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?])).collect()
    })
    .collect()
}

/// Draws a part's outline fitted into `rect`.
fn thumbnail(painter: &egui::Painter, rect: Rect, part: &Value, color: Color32) {
  let lines = outline(part);
  let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
  for p in lines.iter().flatten() {
    for i in 0..2 {
      lo[i] = lo[i].min(p[i]);
      hi[i] = hi[i].max(p[i]);
    }
  }
  if lo[0] > hi[0] {
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, "?", egui::FontId::proportional(20.0), theme::MUTED);
    return;
  }
  let r = rect.shrink(6.0);
  let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-9);
  let k = f64::from(r.width().min(r.height())) / span;
  let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
  let to = |p: &[f64; 2]| r.center() + vec2(((p[0] - mid[0]) * k) as f32, (-(p[1] - mid[1]) * k) as f32);
  for l in &lines {
    painter.add(egui::Shape::line(l.iter().map(to).collect(), Stroke::new(1.2, color)));
  }
}

/// The library window.
pub fn window(app: &mut App, ctx: &egui::Context) {
  if !app.library_ui.open {
    return;
  }
  let (mut close, mut reload, mut add_folder, mut remove_folder) = (false, false, false, None);
  let mut lib = std::mem::take(&mut app.library_ui);
  let in_sketch = app.s.edit_sketch.is_some();
  let saved = settings_folders(&Settings::load());
  egui::Window::new("SKETCH LIBRARY")
    .id(egui::Id::new("sketch-library"))
    .title_bar(false)
    .resizable(true)
    .default_pos(app.canvas.right_top() + vec2(-380.0, 60.0))
    .default_width(350.0)
    .show(ctx, |ui| {
      ui.label(RichText::new("SKETCH LIBRARY").strong().color(theme::TEXT));
      ui.separator();
      ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("library-pack")
          .selected_text(lib.pack.clone().unwrap_or_else(|| "All packs".into()))
          .width(130.0)
          .show_ui(ui, |ui| {
            ui.selectable_value(&mut lib.pack, None, "All packs");
            for p in lib.packs.clone() {
              ui.selectable_value(&mut lib.pack, Some(p.clone()), p);
            }
          });
        ui.add(egui::TextEdit::singleline(&mut lib.filter).hint_text("Filter").desired_width(120.0));
        reload = ui.button("⟳").on_hover_text("Read the library folders again").clicked();
      });
      let filter = lib.filter.to_lowercase();
      let shown: Vec<usize> = (0..lib.parts.len())
        .filter(|&i| {
          let p = &lib.parts[i];
          lib.pack.as_deref().is_none_or(|k| p["pack"].as_str() == Some(k))
            && (filter.is_empty() || p["id"].as_str().is_some_and(|n| n.to_lowercase().contains(&filter)))
        })
        .collect();
      if lib.parts.is_empty() {
        ui.label(
          RichText::new("No parts yet. Put DXF, SVG or .fox files in a folder of the hub's library folder (one folder per pack), or add a folder below.")
            .color(theme::MUTED),
        );
      }
      egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
          for i in shown {
            let part = &lib.parts[i];
            let (rect, resp) = ui.allocate_exact_size(vec2(TILE, TILE + 14.0), egui::Sense::click());
            let tile = Rect::from_min_size(rect.min, vec2(TILE, TILE));
            let chosen = lib.chosen == Some(i);
            let (fill, stroke) = if chosen {
              (theme::SELECTED, theme::ACCENT)
            } else if resp.hovered() {
              (theme::HOVER, theme::SELECTED)
            } else {
              (Color32::WHITE, theme::MUTED.gamma_multiply(0.4))
            };
            ui.painter().rect(tile, 2.0, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
            thumbnail(ui.painter(), tile, part, theme::TEXT);
            let name = part["name"].as_str().unwrap_or("?");
            let short: String = name.rsplit('/').next().unwrap_or(name).chars().take(12).collect();
            ui.painter().text(
              Pos2::new(rect.center().x, rect.max.y - 6.0),
              egui::Align2::CENTER_CENTER,
              short,
              egui::FontId::proportional(10.0),
              theme::TEXT,
            );
            let mut tip = part["id"].as_str().unwrap_or("").to_string();
            if let Some(s) = part["size"].as_array() {
              let n = |v: &Value| v.as_f64().unwrap_or(0.0);
              tip += &format!("\n{:.2} x {:.2} mm", n(&s[0]), n(&s[1]));
            }
            if let Some(e) = part["error"].as_str() {
              tip += &format!("\n{e}");
            }
            if resp.on_hover_text(tip).clicked() {
              lib.chosen = if chosen { None } else { Some(i) };
            }
          }
        });
      });
      ui.separator();
      egui::Grid::new("library-place").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Angle (deg)");
        ui.add(egui::TextEdit::singleline(&mut lib.angle).desired_width(50.0));
        if ui.button("Rotate 90°").clicked() {
          lib.angle = fox_engine::expr::trim_number((lib.angle_deg() + 90.0) % 360.0);
        }
        ui.end_row();
        ui.label("Scale");
        ui.add(egui::TextEdit::singleline(&mut lib.scale).desired_width(50.0));
        ui.end_row();
      });
      let note = match (lib.part(), in_sketch) {
        (Some(p), true) => format!(
          "Click in the sketch to place {} (placed {}). Esc stops.",
          p["name"].as_str().unwrap_or("the part"),
          lib.placed
        ),
        (Some(_), false) => "Edit a sketch to place the part in it.".into(),
        (None, _) => "Pick a part, then click in the sketch to place it.".into(),
      };
      ui.label(RichText::new(note).color(theme::MUTED));
      egui::CollapsingHeader::new("Library folders").id_salt("library-roots").show(ui, |ui| {
        for r in &lib.roots {
          let path = r["path"].as_str().unwrap_or("");
          ui.horizontal(|ui| {
            let col = if r["exists"].as_bool() == Some(true) { theme::TEXT } else { theme::MUTED };
            ui.label(RichText::new(path).color(col).small());
            if saved.iter().any(|f| f == path) && ui.small_button("✕").on_hover_text("Remove from settings").clicked() {
              remove_folder = Some(path.to_string());
            }
          });
        }
        add_folder = ui.button("Add folder…").on_hover_text("A pack folder, or a folder of packs").clicked();
      });
      ui.horizontal(|ui| {
        close = ui.button("Close").clicked();
      });
    });
  app.library_ui = lib;
  if add_folder && let Some(dir) = rfd::FileDialog::new().set_title("Library folder").pick_folder() {
    let mut folders = saved.clone();
    let dir = dir.to_string_lossy().into_owned();
    if !folders.contains(&dir) {
      folders.push(dir);
      save_folders(&folders);
    }
    reload = true;
  }
  if let Some(f) = remove_folder {
    save_folders(&saved.iter().filter(|x| **x != f).cloned().collect::<Vec<_>>());
    fox_engine::library::remove_root(std::path::Path::new(&f));
    reload = true;
  }
  if reload {
    refresh(app);
  }
  if close {
    app.library_ui.open = false;
    app.library_ui.chosen = None;
  }
}

/// Where the part would go: the hovered sketch point, else the cursor on the sketch plane.
fn place_at(app: &App, resp: &egui::Response, ray: &dyn Fn(Pos2) -> (DVec3, DVec3)) -> Option<[f64; 2]> {
  let sk = app.s.edit_sketch?;
  if let Some(Pick::SketchPoint { sketch, point }) = app.hover
    && sketch == sk
    && let Some(p) = app.s.model.sketches.get(&sk).and_then(|r| r.sketch.pos(point))
  {
    return Some(p);
  }
  let plane = app.s.model.sketches.get(&sk)?.plane;
  let (o, d) = ray(resp.hover_pos()?);
  Some(plane.to_local(ray_plane(o, d, plane.origin, plane.normal())?))
}

/// The chosen part follows the cursor; a click places it. True while placing (the clicks are
/// the library's).
pub fn place_clicks(
  app: &mut App,
  resp: &egui::Response,
  painter: &egui::Painter,
  to_screen: &dyn Fn(DVec3) -> Option<Pos2>,
  ray: &dyn Fn(Pos2) -> (DVec3, DVec3),
) -> bool {
  let lib = &app.library_ui;
  if !lib.open || app.dialog.is_some() {
    return false;
  }
  let (Some(part), Some(sk)) = (lib.part(), app.s.edit_sketch) else { return false };
  let Some(at) = place_at(app, resp, ray) else { return false };
  let Some(plane) = app.s.model.sketches.get(&sk).map(|r| r.plane) else { return false };
  let (s, c) = lib.angle_deg().to_radians().sin_cos();
  let k = lib.scale_factor();
  let place = |p: &[f64; 2]| {
    let (x, y) = (p[0] * k, p[1] * k);
    to_screen(plane.to_world([at[0] + x * c - y * s, at[1] + x * s + y * c]))
  };
  let ghost = Stroke::new(1.5, theme::ACCENT);
  for l in outline(part) {
    let pts: Option<Vec<Pos2>> = l.iter().map(place).collect();
    if let Some(pts) = pts {
      painter.add(egui::Shape::line(pts, ghost));
    }
  }
  resp.clone().on_hover_cursor(egui::CursorIcon::Crosshair);
  if resp.clicked() {
    let id = part["id"].as_str().unwrap_or_default().to_string();
    let params = json!({"sketch": sk.0, "part": id, "at": at, "angle": lib.angle_deg(), "scale": lib.scale_factor()});
    if app.run("sketch.library.insert", params).is_some() {
      app.library_ui.placed += 1;
    }
  }
  true
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn esc_stops_placing_then_closes() {
    let mut lib = LibraryUi { open: true, chosen: Some(0), ..LibraryUi::default() };
    assert!(lib.cancel() && lib.open && lib.chosen.is_none());
    assert!(lib.cancel() && !lib.open);
    assert!(!lib.cancel());
  }

  #[test]
  fn folders_come_from_the_library_section() {
    let s = Settings::parse("[Library]\nFolder1=/a\nFolder2=/b\nOther=x\n");
    assert_eq!(settings_folders(&s), ["/a", "/b"]);
  }

  #[test]
  fn placing_a_part_inserts_it_where_you_click() {
    let hub = fox_engine::store::Store::scratch_root();
    let dir = hub.join("library").join("Pack");
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = fox_engine::Session::default();
    s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
    s.execute("sketch.circle", &json!({"center": [0, 0], "radius": 3})).unwrap();
    let sk = s.edit_sketch.unwrap().0;
    s.execute("export", &json!({"path": dir.join("dot.dxf"), "sketch": sk})).unwrap();

    let mut app = App::with_session(fox_engine::Session::default());
    app.s.store_root = Some(hub);
    app.s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
    open(&mut app);
    assert_eq!(app.library_ui.parts.len(), 1, "{:?}", app.library_ui.parts);
    assert!(!outline(&app.library_ui.parts[0]).is_empty());
    app.library_ui.chosen = Some(0);
    let id = app.library_ui.parts[0]["id"].as_str().unwrap().to_string();
    let r = app.run("sketch.library.insert", json!({"part": id, "at": [40, 10]})).unwrap();
    assert_eq!(r["curves"].as_array().unwrap().len(), 1);
    let sk = app.s.edit_sketch.unwrap();
    let (lo, hi) = app.s.model.sketches[&sk].sketch.bounds().unwrap();
    assert!((lo[0] - 37.0).abs() < 1e-6 && (hi[1] - 13.0).abs() < 1e-6, "{lo:?} {hi:?}");
  }
}
