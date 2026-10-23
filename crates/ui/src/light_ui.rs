//! The Render workspace's lights, as Fusion's: NEW LIGHT (Light Type and Target; a picked target
//! adds the Position row, draws the light's yellow wire-sphere glyph over it and lights the
//! canvas live), then EDIT LIGHT for the light just placed (Frame, Position, Rotation and Light
//! properties, with a move triad and rotation rings on the glyph), the browser's Lights folder,
//! and every placed light's glyph in the canvas.

use egui::{Color32, Pos2, Stroke, vec2};
use fox_engine::Sel;
use fox_engine::kernel::glam::{DVec3, Vec3};
use fox_engine::rendering::raytrace::Light;
use serde_json::json;

use crate::icons::{self, Icon};
use crate::manipulators::{Triad, TriadEdit};
use crate::{App, dialogs, panels, theme};

/// How far NEW LIGHT puts a light off its target to start (Fusion's "0.394 in").
pub const NEW_OFFSET_MM: f64 = 10.0;
/// A new light's intensity (cd) and colour temperature (K), as Fusion's EDIT LIGHT opens.
pub const NEW_CANDELA: f32 = 1500.0;
pub const NEW_KELVIN: f32 = 3500.0;
/// The light glyph's yellow and its radius on screen (points).
const GLYPH: Color32 = Color32::from_rgb(240, 196, 0);
const GLYPH_PX: f32 = 11.0;

/// EDIT LIGHT's Intensity unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntensityUnit {
  Candela,
  Lumen,
}

impl IntensityUnit {
  pub fn label(self) -> &'static str {
    match self {
      IntensityUnit::Candela => "Candela",
      IntensityUnit::Lumen => "Lumen",
    }
  }

  fn suffix(self) -> &'static str {
    match self {
      IntensityUnit::Candela => " cd",
      IntensityUnit::Lumen => " lm",
    }
  }
}

/// The docked light dialog: NEW LIGHT (how far off its target the light goes), or EDIT LIGHT on
/// a placed light (the light as edited and as it was).
#[derive(Clone, Debug, PartialEq)]
pub enum LightForm {
  New { offset: f64 },
  Edit(EditLight),
}

impl LightForm {
  pub fn new() -> LightForm {
    LightForm::New { offset: NEW_OFFSET_MM }
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditLight {
  /// The light's name in the design.
  pub name: String,
  pub light: Light,
  pub unit: IntensityUnit,
  /// The Position and Rotation groups' Modify pencils: the triad's arrows and rings.
  pub move_on: bool,
  pub turn_on: bool,
}

impl EditLight {
  pub fn of(l: &Light) -> EditLight {
    EditLight { name: l.name.clone(), light: l.clone(), unit: IntensityUnit::Candela, move_on: true, turn_on: true }
  }
}

/// What a light can aim at: the selected faces, edges, vertices and bodies.
pub(crate) fn light_targets(app: &App) -> usize {
  app
    .s
    .selection
    .iter()
    .filter(|s| matches!(s, Sel::Face { .. } | Sel::Edge { .. } | Sel::Vertex { .. } | Sel::Body { .. }))
    .count()
}

/// Why NEW LIGHT's OK is greyed, if it is: nothing to aim at, or the design's eight lights.
pub(crate) fn light_blocked(app: &App) -> Option<&'static str> {
  if app.s.design.render.lights.len() >= fox_engine::rendering::raytrace::light::MAX_LIGHTS {
    Some("A design takes up to eight lights")
  } else if light_targets(app) == 0 {
    Some("Select a face, edge, vertex or body for the light to aim at")
  } else {
    None
  }
}

/// The light NEW LIGHT would place now: off its target, aimed back at it, at Fusion's intensity
/// and colour temperature.
pub fn new_light(app: &App, offset: f64) -> Option<Light> {
  let at = fox_engine::rendering::light_target_off(&app.s, &app.s.selection, offset)?;
  let aim = fox_engine::rendering::light_target_off(&app.s, &app.s.selection, 0.0)
    .map(|t| Vec3::from(t) - Vec3::from(at))
    .filter(|d| d.length() > 1e-6)
    .unwrap_or(Vec3::NEG_Z);
  Some(Light {
    name: String::new(),
    position: at,
    intensity: Light::lumens_of(NEW_CANDELA),
    kelvin: Some(NEW_KELVIN),
    rotation: Light::rotation_for(aim),
    ..Light::default()
  })
}

/// The light the open dialog shows over the design's: (the placed light it stands for, if any;
/// the light as it is now).
pub fn preview(app: &App) -> Option<(Option<String>, Light)> {
  if !app.render.active {
    return None;
  }
  match app.render.light.as_ref()? {
    LightForm::New { offset } => Some((None, new_light(app, *offset)?)),
    LightForm::Edit(e) => Some((Some(e.name.clone()), e.light.clone())),
  }
}

/// The lights the canvas is lit by: the design's, with the dialog's light previewed live.
pub fn scene_lights(app: &App) -> Vec<Light> {
  let mut lights = app.s.design.render.lights.clone();
  if !app.render.lights_on {
    return Vec::new();
  }
  match preview(app) {
    Some((Some(name), l)) => {
      if let Some(x) = lights.iter_mut().find(|x| x.name == name) {
        *x = l;
      }
    }
    Some((None, l)) => lights.push(l),
    None => {}
  }
  lights
}

/// NEW LIGHT's OK: places the light, drops the target and opens EDIT LIGHT on it at once.
pub fn place(app: &mut App, offset: f64) -> bool {
  let Some(l) = new_light(app, offset) else { return false };
  let p = json!({
    "position": l.position,
    "candela": NEW_CANDELA,
    "kelvin": NEW_KELVIN,
    "rotation": l.rotation,
  });
  let Some(r) = app.run("render.light.add", p) else { return false };
  app.s.selection.clear();
  let name = r["name"].as_str().unwrap_or_default().to_string();
  if let Some(placed) = app.s.design.render.lights.iter().find(|x| x.name == name) {
    app.render.light = Some(LightForm::Edit(EditLight::of(placed)));
  } else {
    app.render.light = None;
  }
  true
}

/// EDIT LIGHT's OK: the edited light goes into the design.
pub fn apply(app: &mut App, e: &EditLight) -> bool {
  let l = &e.light;
  let p = json!({
    "name": e.name,
    "position": l.position,
    "intensity": l.intensity,
    "rotation": l.rotation,
    "kelvin": l.kelvin,
    "color": l.color,
  });
  app.run("render.light.edit", p).is_some()
}

/// Opens EDIT LIGHT on a placed light (the browser's double-click).
pub fn edit(app: &mut App, name: &str) {
  let Some(l) = app.s.design.render.lights.iter().find(|x| x.name == name).cloned() else { return };
  crate::render_tools::close_docked(app);
  app.render.light = Some(LightForm::Edit(EditLight::of(&l)));
}

// ---------------------------------------------------------------- the triad

/// EDIT LIGHT's triad on the glyph: the move arrows (Position's Modify) and rotation rings
/// (Rotation's Modify).
pub fn triad(app: &App) -> Option<Triad> {
  if !app.render.active || app.dialog.is_some() {
    return None;
  }
  let Some(LightForm::Edit(e)) = &app.render.light else { return None };
  let mut t = Triad::world(DVec3::from(e.light.position.map(f64::from)));
  t.arrows = [e.move_on; 3];
  t.planes = [e.move_on; 3];
  t.rings = [e.turn_on; 3];
  t.angles = e.light.rotation.map(|a| (a as f64).to_radians());
  Some(t)
}

pub fn set_triad(app: &mut App, edit: TriadEdit) {
  let Some(LightForm::Edit(e)) = &mut app.render.light else { return };
  match edit {
    TriadEdit::Move(p) => e.light.position = p.as_vec3().to_array(),
    TriadEdit::Rotate(i, a) => e.light.rotation[i.min(2)] = (a.to_degrees() as f32 * 10.0).round() / 10.0,
    TriadEdit::Orient(q) => {
      e.light.rotation = crate::manipulators::xyz_of(q).map(|a| a.to_degrees() as f32);
    }
  }
}

/// Flip Direction: the light turns to point the other way.
pub fn flip(l: &mut Light) {
  l.rotation = Light::rotation_for(-l.aim());
}

// ---------------------------------------------------------------- the canvas

/// The yellow wire sphere of a light at `at`: three great circles a fixed size on screen, and a
/// short line the way it points.
fn glyph(
  painter: &egui::Painter,
  app: &App,
  at: DVec3,
  aim: DVec3,
  width: f32,
  project: &dyn Fn(DVec3) -> Option<Pos2>,
) {
  let Some(c) = project(at) else { return };
  let (right, _, _) = app.cam.basis();
  let Some(edge) = project(at + right * 1e-2) else { return };
  let per_mm = (edge - c).length() / 1e-2;
  if per_mm < 1e-9 {
    return;
  }
  let r = (GLYPH_PX / per_mm) as f64;
  let stroke = Stroke::new(width, GLYPH);
  for (u, v) in [(DVec3::X, DVec3::Y), (DVec3::X, DVec3::Z), (DVec3::Y, DVec3::Z)] {
    let pts: Vec<Pos2> = (0..=32)
      .filter_map(|k| {
        let a = k as f64 / 32.0 * std::f64::consts::TAU;
        project(at + (u * a.cos() + v * a.sin()) * r)
      })
      .collect();
    painter.add(egui::Shape::line(pts, stroke));
  }
  if let Some(tip) = project(at + aim * r * 2.2) {
    painter.add(egui::Shape::dashed_line(&[c, tip], Stroke::new(1.0, GLYPH), 3.0, 2.0));
  }
}

/// Every placed light's glyph (while the browser's Lights folder shows them), and the light the
/// open dialog places or edits.
pub(crate) fn overlay(app: &App, painter: &egui::Painter, project: &dyn Fn(DVec3) -> Option<Pos2>) {
  if !app.render.active {
    return;
  }
  let shown = preview(app);
  let edited = shown.as_ref().and_then(|(n, _)| n.clone());
  if app.render.lights_shown {
    for l in app.s.design.render.lights.iter().filter(|l| Some(&l.name) != edited.as_ref()) {
      let width = if l.on { 1.2 } else { 0.6 };
      glyph(painter, app, DVec3::from(l.position.map(f64::from)), l.aim().as_dvec3(), width, project);
    }
  }
  if let Some((_, l)) = shown {
    glyph(painter, app, DVec3::from(l.position.map(f64::from)), l.aim().as_dvec3(), 1.8, project);
  }
}

// ---------------------------------------------------------------- the browser

/// The browser's Lights folder (above Origin once a light is placed): its check box switches the
/// lights' lighting on or off, its eye their glyphs; a light's own eye switches it, a
/// double-click opens EDIT LIGHT.
pub(crate) fn browser(app: &mut App, ui: &mut egui::Ui) {
  if !app.render.active || app.s.design.render.lights.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-lights");
  let ev =
    panels::tree_row(ui, id, 1, Some(false), Some(app.render.lights_shown), Icon::Folder, "Lights", false, theme::TEXT);
  if ev.eye {
    app.render.lights_shown = !app.render.lights_shown;
  }
  if let Some(r) = &ev.response {
    // the check box after the folder's name
    let galley = ui.painter().layout_no_wrap("Lights".into(), egui::FontId::proportional(13.0), theme::TEXT);
    let x = r.rect.min.x + 8.0 + 16.0 + 44.0 - 11.0 + galley.size().x + 36.0 + 8.0;
    let bx = egui::Rect::from_min_size(egui::pos2(x, r.rect.center().y - 6.5), egui::Vec2::splat(13.0));
    let cr = ui.interact(bx, id.with("check"), egui::Sense::click());
    let p = ui.painter();
    p.rect(bx, 2.0, Color32::WHITE, Stroke::new(1.0, theme::GROUP_LABEL), egui::StrokeKind::Inside);
    if app.render.lights_on {
      let pts = vec![
        bx.left_center() + vec2(2.5, 0.0),
        bx.center_bottom() + vec2(-1.0, -3.0),
        bx.right_top() + vec2(-2.5, 3.0),
      ];
      p.add(egui::Shape::line(pts, Stroke::new(1.6, theme::ACCENT)));
    }
    if cr.on_hover_text("Light the scene with these lights").clicked() {
      app.render.lights_on = !app.render.lights_on;
    }
  }
  if !ui.data(|d| d.get_temp::<bool>(id.with("open"))).unwrap_or(false) {
    return;
  }
  let lights: Vec<(String, bool)> = app.s.design.render.lights.iter().map(|l| (l.name.clone(), l.on)).collect();
  for (name, on) in lights {
    let ev = panels::tree_row(ui, id.with(&name), 2, None, Some(on), Icon::Sun, &name, false, theme::TEXT);
    if ev.eye {
      app.run("render.light.edit", json!({"name": name, "on": !on}));
    }
    if ev.double_clicked {
      edit(app, &name);
    }
    if let Some(r) = ev.response {
      r.on_hover_text("Double-click to edit this light");
    }
  }
}

// ---------------------------------------------------------------- the dialogs

/// The docked NEW LIGHT or EDIT LIGHT dialog.
pub(crate) fn light_dialog(app: &mut App, ctx: &egui::Context) {
  match app.render.light.clone() {
    None => {}
    Some(LightForm::New { offset }) => new_dialog(app, ctx, offset),
    Some(LightForm::Edit(e)) => edit_dialog(app, ctx, e),
  }
}

fn dock(app: &App, id: &str, ctx: &egui::Context, add: impl FnOnce(&mut egui::Ui)) {
  egui::Area::new(egui::Id::new(id))
    .pivot(egui::Align2::RIGHT_TOP)
    .default_pos(panels::dialog_dock(app.canvas, false))
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| panels::dialog_frame().show(ui, add));
}

/// NEW LIGHT: Light Type and Target, with Position (how far off the target) once one is picked.
fn new_dialog(app: &mut App, ctx: &egui::Context, mut offset: f64) {
  let targets = light_targets(app);
  let blocked = light_blocked(app);
  let unit = crate::units::current(ctx);
  let (mut cancel, mut ok) = (false, false);
  let width = 300.0;
  let accent = icons::accent_of(Icon::Sun);
  dock(app, "new-light", ctx, |ui| {
    ui.set_width(width);
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    panels::dialog_header(ui, Icon::Sun, "NEW LIGHT", width);
    egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
      egui::Grid::new("new-light-grid").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
        ui.label("Light Type");
        egui::ComboBox::from_id_salt("light-type").selected_text("Photometric Light").width(150.0).show_ui(ui, |ui| {
          let _ = ui.selectable_label(true, "Photometric Light");
        });
        ui.end_row();
        ui.label("Target");
        dialogs::sel_chip(ui, accent, &dialogs::chip_text(targets, "selected"), targets > 0);
        ui.end_row();
        if targets > 0 {
          ui.label("Position");
          let mut v = unit.from_mm(offset);
          let r =
            ui.add(egui::DragValue::new(&mut v).speed(0.05).fixed_decimals(3).suffix(format!(" {}", unit.symbol())));
          if r.changed() {
            offset = v * unit.mm();
          }
          ui.end_row();
        }
      });
    });
    (cancel, ok) = crate::render_panels::footer(ui, width, |ui| {
      let cancel = panels::secondary_button(ui, "Cancel").clicked();
      let resp = dialogs::ok_button(ui, accent, blocked.is_none());
      match blocked {
        None => (cancel, resp.clicked()),
        Some(why) => {
          resp.on_disabled_hover_text(why);
          (cancel, false)
        }
      }
    });
  });
  if app.render.light.is_none() {
    return;
  }
  app.render.light = Some(LightForm::New { offset });
  if ok {
    place(app, offset);
  } else if cancel {
    app.render.light = None;
  }
}

/// EDIT LIGHT: Frame, Position, Rotation and Light properties, as Fusion's.
fn edit_dialog(app: &mut App, ctx: &egui::Context, mut e: EditLight) {
  let unit = crate::units::current(ctx);
  let (mut cancel, mut ok) = (false, false);
  let width = 320.0;
  let accent = icons::accent_of(Icon::Sun);
  let deg = |ui: &mut egui::Ui, v: &mut f32| {
    ui.add(egui::DragValue::new(v).speed(0.5).fixed_decimals(1).suffix(" deg"));
  };
  dock(app, "edit-light", ctx, |ui| {
    ui.set_width(width);
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    panels::dialog_header(ui, Icon::Sun, "EDIT LIGHT", width);
    egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 8, bottom: 8 }).show(ui, |ui| {
      egui::Grid::new("edit-light-grid").num_columns(2).min_col_width(110.0).spacing([10.0, 6.0]).show(ui, |ui| {
        if dialogs::section_header(ui, egui::Id::new("edit-light-frame"), "Frame") {
          ui.label("Mode");
          egui::ComboBox::from_id_salt("light-mode").selected_text("Global").width(150.0).show_ui(ui, |ui| {
            let _ = ui.selectable_label(true, "Global");
          });
          ui.end_row();
          ui.label("Light Type");
          ui.add_enabled_ui(false, |ui| {
            egui::ComboBox::from_id_salt("edit-light-type")
              .selected_text("Photometric Light")
              .width(150.0)
              .show_ui(ui, |_| {});
          });
          ui.end_row();
        }
        if dialogs::section_header(ui, egui::Id::new("edit-light-position"), "Position") {
          ui.label("Modify");
          pencil(ui, &mut e.move_on);
          ui.end_row();
          for (k, axis) in ["X", "Y", "Z"].into_iter().enumerate() {
            ui.label(axis);
            let mut v = unit.from_mm(e.light.position[k] as f64);
            let r =
              ui.add(egui::DragValue::new(&mut v).speed(0.1).fixed_decimals(3).suffix(format!(" {}", unit.symbol())));
            if r.changed() {
              e.light.position[k] = (v * unit.mm()) as f32;
            }
            ui.end_row();
          }
        }
        if dialogs::section_header(ui, egui::Id::new("edit-light-rotation"), "Rotation") {
          ui.label("Modify");
          pencil(ui, &mut e.turn_on);
          ui.end_row();
          ui.label("Flip Direction");
          if icons::icon_button(ui, Icon::Flip, 20.0, "Flip Direction", false, None).clicked() {
            flip(&mut e.light);
          }
          ui.end_row();
          for (k, axis) in ["X", "Y", "Z"].into_iter().enumerate() {
            ui.label(axis);
            deg(ui, &mut e.light.rotation[k]);
            ui.end_row();
          }
        }
        if dialogs::section_header(ui, egui::Id::new("edit-light-props"), "Light properties") {
          ui.label("Intensity");
          ui.horizontal(|ui| {
            let mut v = match e.unit {
              IntensityUnit::Candela => e.light.candela(),
              IntensityUnit::Lumen => e.light.intensity,
            };
            let top = match e.unit {
              IntensityUnit::Candela => 10_000.0,
              IntensityUnit::Lumen => 120_000.0,
            };
            let mut changed =
              ui.add(egui::DragValue::new(&mut v).speed(5.0).fixed_decimals(3).suffix(e.unit.suffix())).changed();
            ui.spacing_mut().slider_width = 70.0;
            changed |= ui.add(egui::Slider::new(&mut v, 0.0..=top).show_value(false)).changed();
            if changed {
              e.light.intensity = match e.unit {
                IntensityUnit::Candela => Light::lumens_of(v.max(0.0)),
                IntensityUnit::Lumen => v.max(0.0),
              };
            }
          });
          ui.end_row();
          ui.label("Intensity unit");
          egui::ComboBox::from_id_salt("light-unit").selected_text(e.unit.label()).width(150.0).show_ui(ui, |ui| {
            for u in [IntensityUnit::Candela, IntensityUnit::Lumen] {
              ui.selectable_value(&mut e.unit, u, u.label());
            }
          });
          ui.end_row();
          ui.label("Distribution");
          egui::ComboBox::from_id_salt("light-dist").selected_text("Uniform").width(150.0).show_ui(ui, |ui| {
            let _ = ui.selectable_label(true, "Uniform");
          });
          ui.end_row();
          ui.label("Shape");
          egui::ComboBox::from_id_salt("light-shape").selected_text("Point").width(150.0).show_ui(ui, |ui| {
            let _ = ui.selectable_label(true, "Point");
          });
          ui.end_row();
          ui.label("Color Mode");
          let mode = if e.light.kelvin.is_some() { "Kelvin" } else { "RGB" };
          egui::ComboBox::from_id_salt("light-color-mode").selected_text(mode).width(150.0).show_ui(ui, |ui| {
            if ui.selectable_label(e.light.kelvin.is_some(), "Kelvin").clicked() && e.light.kelvin.is_none() {
              e.light.kelvin = Some(NEW_KELVIN);
            }
            if ui.selectable_label(e.light.kelvin.is_none(), "RGB").clicked() {
              e.light.kelvin = None;
            }
          });
          ui.end_row();
          if let Some(k) = e.light.kelvin.as_mut() {
            ui.label("Color Preset");
            egui::ComboBox::from_id_salt("light-preset").selected_text("Custom").width(150.0).show_ui(ui, |ui| {
              let _ = ui.selectable_label(true, "Custom");
            });
            ui.end_row();
            ui.label("Temperature");
            ui.add(egui::DragValue::new(k).speed(10.0).range(1000.0..=40000.0).fixed_decimals(2));
            ui.end_row();
            ui.label("Color");
            ui.label("None");
            ui.end_row();
          } else {
            ui.label("Color");
            let mut c = e.light.color;
            if ui.color_edit_button_srgb(&mut c).changed() {
              e.light.color = c;
            }
            ui.end_row();
          }
        }
      });
    });
    (cancel, ok) = crate::render_panels::footer(ui, width, |ui| {
      let cancel = panels::secondary_button(ui, "Cancel").clicked();
      (cancel, dialogs::ok_button(ui, accent, true).clicked())
    });
  });
  if app.render.light.is_none() {
    return;
  }
  if ok {
    apply(app, &e);
    app.render.light = None;
  } else if cancel {
    app.render.light = None;
  } else {
    app.render.light = Some(LightForm::Edit(e));
  }
}

/// A group's Modify pencil: pressed while its handles show on the glyph.
fn pencil(ui: &mut egui::Ui, on: &mut bool) {
  if icons::icon_button(ui, Icon::Pencil, 20.0, "Modify", *on, None).clicked() {
    *on = !*on;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  fn render_box() -> (App, Sel) {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 40, "depth": 40, "height": 10}));
    crate::panels::switch_workspace(&mut app, "Render");
    let b = &app.s.model.bodies[0];
    let sel = Sel::Face { body: b.id, face: b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id };
    (app, sel)
  }

  #[test]
  fn new_light_previews_over_its_target_then_opens_edit_light() {
    let (mut app, top) = render_box();
    crate::render_ui::RenderCmd::Lights.run(&mut app);
    assert_eq!(app.render.light, Some(LightForm::New { offset: NEW_OFFSET_MM }));
    assert!(preview(&app).is_none(), "nothing to preview without a target");
    app.s.selection = vec![top];
    let (_, l) = preview(&app).expect("the light previews over the face");
    assert!((l.position[2] - 20.0).abs() < 1e-3, "10 mm off the top face: {:?}", l.position);
    assert_eq!(l.rotation, [0.0, -90.0, 0.0], "aimed down at it");
    assert_eq!(scene_lights(&app).len(), 1, "it lights the canvas live");
    assert!(place(&mut app, NEW_OFFSET_MM));
    assert!(app.s.selection.is_empty(), "the target is dropped");
    let placed = app.s.design.render.lights[0].clone();
    assert!((placed.candela() - NEW_CANDELA).abs() < 0.1);
    assert_eq!(placed.kelvin, Some(NEW_KELVIN));
    let Some(LightForm::Edit(e)) = app.render.light.clone() else { panic!("EDIT LIGHT opens") };
    assert_eq!(e.name, "Light1");
    // its triad sits on the glyph; a drag moves the light, a ring turns it
    let t = triad(&app).expect("the triad");
    assert!(t.rings.iter().all(|r| *r) && t.arrows.iter().all(|a| *a));
    set_triad(&mut app, TriadEdit::Move(t.origin + DVec3::X * 5.0));
    set_triad(&mut app, TriadEdit::Rotate(2, 45f64.to_radians()));
    let Some(LightForm::Edit(mut e)) = app.render.light.clone() else { panic!() };
    assert!((e.light.position[0] - placed.position[0] - 5.0).abs() < 1e-4);
    assert_eq!(e.light.rotation[2], 45.0);
    assert_eq!(scene_lights(&app)[0].position, e.light.position, "the edit lights the canvas live");
    flip(&mut e.light);
    assert!(e.light.aim().z > 0.9, "flipped up: {:?}", e.light.rotation);
    e.light.kelvin = None;
    e.light.color = [0, 0, 255];
    assert!(apply(&mut app, &e));
    let l = &app.s.design.render.lights[0];
    assert_eq!((l.kelvin, l.color), (None, [0, 0, 255]));
    assert!((l.position[0] - e.light.position[0]).abs() < 1e-4);
  }
}
