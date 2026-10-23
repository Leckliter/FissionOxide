//! The mesh dialogs' own rows, drawn as Fusion draws them: "1 selected" chips, Tessellate's
//! Refinement Settings group (values with sliders, spinners on the plain numbers), Repair's Quick
//! Analysis and Detailed Analysis, the collapsed Preview groups with their own check box, and
//! Create Mesh Section Sketch's Numerical Inputs group.

use egui::{RichText, vec2};
use fox_engine::Session;
use fox_engine::model::BodyId;
use serde_json::{Value, json};

use super::{Dialog, sel_chip, spinner};
use crate::forms::Form;
use crate::{mesh_ui, theme};

/// A group's disclosure triangle at `c`: pointing down while open, right while closed.
pub(crate) fn group_triangle(p: &egui::Painter, c: egui::Pos2, open: bool) {
  let pts = if open {
    vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
  } else {
    vec![c + vec2(-2.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.0, 4.0)]
  };
  p.add(egui::Shape::convex_polygon(pts, theme::GROUP_LABEL, egui::Stroke::NONE));
}

/// A group header row in the dialog's grid: the triangle and title, clicked to open or close the
/// group (remembered per dialog). Returns whether the group is open.
fn group(ui: &mut egui::Ui, title: &str, default_open: bool) -> bool {
  let id = egui::Id::new(("mesh-dialog-group", title));
  let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(default_open);
  let font = egui::FontId::proportional(12.5);
  let tw = ui.painter().layout_no_wrap(title.to_string(), font.clone(), theme::TEXT).size().x;
  let (r, resp) = ui.allocate_exact_size(vec2(tw + 18.0, 20.0), egui::Sense::click());
  group_triangle(ui.painter(), r.left_center() + vec2(5.0, 0.0), open);
  ui.painter().text(r.left_center() + vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, title, font, theme::TEXT);
  if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
    open = !open;
    ui.data_mut(|d| d.insert_temp(id, open));
  }
  open
}

/// The number a value box holds (its leading number; units aside).
fn number(text: &str) -> f64 {
  let t = text.trim();
  let end = t.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).unwrap_or(t.len());
  t[..end].parse().unwrap_or(0.0)
}

/// The unit a value box's text ends with (" mm", " deg" or nothing).
fn unit_of(text: &str) -> &str {
  let t = text.trim();
  let end = t.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).unwrap_or(t.len());
  &t[end..]
}

impl Dialog {
  /// Draws input `i` of a mesh form; true when it was drawn here (every mesh form row is).
  pub(super) fn mesh_form_field(
    &mut self,
    f: &'static Form,
    i: usize,
    s: &Session,
    ui: &mut egui::Ui,
    dirty: &mut bool,
  ) -> bool {
    if !mesh_ui::is_mesh_form(f) {
      return false;
    }
    let key = f.inputs[i].key();
    let label = label_of(f, i);
    let accent = self.accent();
    let v = self.form.values[i].clone();
    let last = i + 1 == f.inputs.len();
    match (f.cmd, key) {
      (_, "body" | "bodies" | "faces" | "plane") => {
        let n = match &v {
          Value::Array(a) => a.len(),
          Value::Null => 0,
          _ => 1,
        };
        ui.label(label);
        sel_chip(ui, accent, &if n == 0 { "Select".to_string() } else { format!("{n} selected") }, n > 0);
        ui.end_row();
      }
      ("mesh.tessellate", "refinement") => {
        let open = group(ui, "Refinement Settings", true);
        ui.label("");
        ui.end_row();
        if open {
          let mut sel: &'static str =
            mesh_ui::REFINEMENTS.iter().map(|(k, _)| *k).find(|k| v.as_str() == Some(*k)).unwrap_or("medium");
          let before = sel;
          Self::choose(ui, label, "mesh-refinement", &mut sel, mesh_ui::REFINEMENTS, dirty);
          self.form.values[i] = json!(sel);
          if sel != before {
            mesh_ui::fill_refinement(f, &mut self.form, s);
          }
        }
      }
      ("mesh.tessellate", "surface_deviation" | "max_edge" | "normal_deviation" | "aspect_ratio") => {
        if self.group_open("Refinement Settings", true, ui) {
          let diag = self.picked_diagonal(f, s);
          let (range, spin) = match key {
            "surface_deviation" => (0.0..=(diag * 0.01).max(0.01), false),
            "max_edge" => (0.0..=diag.max(1.0), false),
            "normal_deviation" => (0.0..=90.0, true),
            _ => (1.0..=100.0, true),
          };
          if self.slider_row(i, label, range, spin, ui, dirty) {
            // a value of one's own makes the refinement Custom, as in Fusion
            if let Some(r) = f.inputs.iter().position(|x| x.key() == "refinement") {
              self.form.values[r] = json!("custom");
            }
          }
        }
      }
      ("mesh.tessellate", "quads") => {
        if self.group_open("Refinement Settings", true, ui) {
          let mut b = false;
          ui.label(label);
          ui.add_enabled_ui(false, |ui| ui.checkbox(&mut b, ""))
            .inner
            .on_disabled_hover_text("Mesh bodies are triangles in FissionOxide: quads aren't made yet");
          ui.end_row();
        }
      }
      (_, "type") => {
        let options: &'static [(&'static str, &'static str)] =
          if f.cmd == "mesh.repair" { mesh_ui::REPAIR_TYPES } else { &[("fast", "Fast"), ("accurate", "Accurate")] };
        let mut sel: &'static str =
          options.iter().map(|(k, _)| *k).find(|k| v.as_str() == Some(*k)).unwrap_or(options[0].0);
        Self::choose(ui, label, &format!("mesh-type-{}", f.cmd), &mut sel, options, dirty);
        self.form.values[i] = json!(sel);
      }
      ("mesh.face_groups", "angle") => {
        let mut t = v.as_str().unwrap_or("").to_string();
        Self::field(ui, label, &mut t, dirty);
        self.form.values[i] = json!(t);
      }
      ("mesh.face_groups", "min_size") => {
        self.slider_row(i, label, 0.0..=1.0, false, ui, dirty);
      }
      ("mesh.section_sketch", "offset") => {
        let open = group(ui, "Numerical Inputs", false);
        ui.label("");
        ui.end_row();
        if open {
          let mut t = v.as_str().unwrap_or("").to_string();
          Self::field(ui, label, &mut t, dirty);
          self.form.values[i] = json!(t);
        }
      }
      _ => return false,
    }
    if last {
      self.mesh_form_end(f, s, ui);
    }
    true
  }

  /// Whether a group drawn earlier in this dialog is open.
  fn group_open(&self, title: &str, default_open: bool, ui: &egui::Ui) -> bool {
    ui.data(|d| d.get_temp::<bool>(egui::Id::new(("mesh-dialog-group", title)))).unwrap_or(default_open)
  }

  /// The picked body's bounding diagonal (mm), for the sliders' reach.
  fn picked_diagonal(&self, f: &Form, s: &Session) -> f64 {
    f.inputs
      .iter()
      .position(|x| matches!(x.key(), "body" | "bodies"))
      .and_then(|j| self.form.values[j].as_array()?.first()?.as_u64())
      .and_then(|b| s.model.body(BodyId(b as u32)))
      .map_or(10.0, |b| b.diagonal())
  }

  /// A value box with a slider after it (and Fusion's spinner on plain numbers). True when the
  /// value changed.
  fn slider_row(
    &mut self,
    i: usize,
    label: &str,
    range: std::ops::RangeInclusive<f64>,
    spin: bool,
    ui: &mut egui::Ui,
    dirty: &mut bool,
  ) -> bool {
    let mut t = self.form.values[i].as_str().unwrap_or("").to_string();
    let mut changed = false;
    ui.label(label);
    ui.push_id(("mesh-slider-row", i), |ui| {
      ui.vertical(|ui| {
        ui.horizontal(|ui| {
          ui.spacing_mut().item_spacing.x = 2.0;
          if super::value_edit_w(ui, &mut t, if spin { 126.0 } else { 140.0 }).changed() {
            changed = true;
          }
          if spin && let Some(step) = spinner(ui) {
            t = format!("{:.2}", (number(&t) + step as f64).max(0.0));
            changed = true;
          }
        });
        let mut x = number(&t).clamp(*range.start(), *range.end());
        ui.spacing_mut().slider_width = 140.0;
        if ui.add(egui::Slider::new(&mut x, range).show_value(false)).changed() {
          let unit = unit_of(&t).to_string();
          t = format!("{x:.2}{unit}");
          changed = true;
        }
      });
    });
    ui.end_row();
    if changed {
      *dirty = true;
    }
    self.form.values[i] = json!(t);
    changed
  }

  /// The rows after a mesh form's inputs: Repair's Preview, Quick Analysis and Detailed
  /// Analysis; Tessellate's and Generate Face Groups' collapsed Preview group.
  fn mesh_form_end(&mut self, f: &'static Form, s: &Session, ui: &mut egui::Ui) {
    match f.cmd {
      "mesh.tessellate" | "mesh.face_groups" => {
        group(ui, "Preview", false);
        self.preview_check(ui, "");
      }
      "mesh.repair" => {
        ui.label("");
        self.preview_check(ui, "Preview");
        let body = f
          .inputs
          .iter()
          .position(|x| x.key() == "bodies")
          .and_then(|j| self.form.values[j].as_array()?.first()?.as_u64())
          .map(|b| BodyId(b as u32));
        let analysis = body.and_then(|b| cached_analysis(ui, s, b));
        ui.label("Quick Analysis");
        match analysis {
          Some(a) if a.clean() => {
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 8.0, theme::ICON_GREEN);
            let c = r.center();
            ui.painter().add(egui::Shape::line(
              vec![c + vec2(-4.0, 0.0), c + vec2(-1.0, 3.0), c + vec2(4.5, -3.0)],
              egui::Stroke::new(1.8, egui::Color32::WHITE),
            ));
          }
          Some(a) => {
            let n = a.open_edges + a.non_manifold_edges + a.degenerate + a.flipped;
            ui.label(RichText::new(format!("⚠ {n} problems")).color(theme::WARNING));
          }
          None => {
            ui.label("");
          }
        }
        ui.end_row();
        if group(ui, "Detailed Analysis", false) {
          ui.label("");
          ui.end_row();
          if let Some(a) = analysis {
            for (k, n) in [
              ("Triangles", a.triangles),
              ("Open Edges", a.open_edges),
              ("Holes", a.holes),
              ("Non-manifold Edges", a.non_manifold_edges),
              ("Degenerate Triangles", a.degenerate),
              ("Flipped Triangles", a.flipped),
              ("Shells", a.shells),
            ] {
              ui.label(k);
              ui.label(n.to_string());
              ui.end_row();
            }
          }
        } else {
          ui.label("");
          ui.end_row();
        }
      }
      _ => {}
    }
  }

  /// The Preview check box in the dialog's body (these forms have none in the footer).
  fn preview_check(&mut self, ui: &mut egui::Ui, label: &str) {
    if ui.checkbox(&mut self.live.on, label).changed() {
      if self.live.on {
        self.live.retry();
      } else {
        self.live.withdraw = true;
      }
    }
    ui.end_row();
  }

  /// The Preview row ending Reduce, Smooth, Remesh, Shell and Combine: its label, and the box
  /// after it as their other check boxes are drawn.
  pub(super) fn preview_row(&mut self, ui: &mut egui::Ui) {
    ui.label("Preview");
    self.preview_check(ui, "");
  }
}

/// The analysis of a body's mesh, worked out once per design revision.
fn cached_analysis(ui: &egui::Ui, s: &Session, b: BodyId) -> Option<fox_engine::model::meshprep::Analysis> {
  let id = egui::Id::new(("mesh-analysis", b.0, s.revision));
  if let Some(a) = ui.data(|d| d.get_temp::<Option<fox_engine::model::meshprep::Analysis>>(id)) {
    return a;
  }
  let a = fox_engine::mesh_prep::analysis(s, b);
  ui.data_mut(|d| d.insert_temp(id, a));
  a
}

fn label_of(f: &Form, i: usize) -> &'static str {
  use crate::forms::Input;
  match &f.inputs[i] {
    Input::Bodies { label, .. }
    | Input::Faces { label, .. }
    | Input::Plane { label, .. }
    | Input::Choice { label, .. }
    | Input::Expr { label, .. }
    | Input::Check { label, .. } => label,
    _ => "",
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::model::BodyId;
  use fox_engine::scene::Pick;
  use fox_engine::{Sel, Session};
  use serde_json::json;

  use super::super::{Dialog, Kind};
  use crate::panels::{Cmd, Tab};
  use crate::{App, mesh_ui};

  /// Draws the dialog once; the text it drew, and whether it stays open.
  fn show(app: &mut App, d: &mut Dialog) -> (Vec<String>, bool) {
    fn walk(s: &egui::Shape, out: &mut Vec<String>) {
      match s {
        egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
        egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        _ => {}
      }
    }
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))),
      ..Default::default()
    };
    // the dialog's area sizes itself on its first frame and shows on the next
    let mut keep = false;
    ctx.run_ui(raw.clone(), |ui| keep = d.show(app, ui.ctx())).drop_without_applying_deltas();
    if !keep {
      return (Vec::new(), keep);
    }
    let out = ctx.run_ui(raw, |ui| keep = d.show(app, ui.ctx()));
    let mut texts = Vec::new();
    for c in &out.shapes {
      walk(&c.shape, &mut texts);
    }
    assert!(!texts.iter().any(|t| t.contains("widget ID")), "an ID clash: {texts:?}");
    out.drop_without_applying_deltas();
    (texts, keep)
  }

  fn value(d: &Dialog, k: &str) -> serde_json::Value {
    let Kind::Form(f) = d.kind else { panic!("a form") };
    d.form.values[f.inputs.iter().position(|x| x.key() == k).unwrap()].clone()
  }

  /// An app with the reference L-shaped prism inserted (a mesh body) and a cylinder (a solid).
  fn app() -> (App, BodyId, BodyId) {
    let mut app = App::with_session(Session::default());
    let path =
      std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/reference/464-stl-millimeter/lprism.stl");
    app.run("import", json!({"path": path})).expect("the STL goes in");
    let mesh = app.s.model.bodies[0].id;
    app
      .run("cylinder", json!({"radius": 10, "height": 4, "at": [60, 0], "operation": "new_body"}))
      .expect("a cylinder");
    let solid = app.s.model.bodies.iter().map(|b| b.id).find(|b| *b != mesh).unwrap();
    (app, mesh, solid)
  }

  fn open(app: &mut App, f: &'static crate::forms::Form, sel: Vec<Sel>) -> Dialog {
    app.s.selection = sel;
    app.start(Kind::Form(f));
    app.dialog.take().expect("the dialog opens")
  }

  #[test]
  fn the_mesh_commands_are_on_the_mesh_tab() {
    let entries = crate::menus::commands(&[Tab::Mesh]);
    let cmd = |l: &str| entries.iter().find(|e| e.label == l).map(|e| e.cmd);
    assert_eq!(cmd("Tessellate"), Some(Cmd::Dialog(Kind::Form(&mesh_ui::TESSELLATE))));
    assert_eq!(cmd("Repair"), Some(Cmd::Dialog(Kind::Form(&mesh_ui::REPAIR))));
    assert_eq!(cmd("Generate Face Groups"), Some(Cmd::Dialog(Kind::Form(&mesh_ui::GENERATE_FACE_GROUPS))));
    assert_eq!(cmd("Combine Face Groups"), Some(Cmd::Dialog(Kind::Form(&mesh_ui::COMBINE_FACE_GROUPS))));
    assert_eq!(cmd("Create Mesh Section Sketch"), Some(Cmd::Dialog(Kind::Form(&mesh_ui::SECTION_SKETCH))));
    assert_eq!(cmd("Direct Edit"), Some(Cmd::MeshEdit(mesh_ui::Act::DirectEdit)));
    assert_eq!(cmd("Create Base Mesh Feature"), Some(Cmd::MeshEdit(mesh_ui::Act::BaseMesh)));
    let tip = crate::tips::tip(crate::tips::Scope::Design, "Tessellate").unwrap();
    assert_eq!(tip.line, "Converts an existing solid or surface body into a mesh body.");
    let tip = crate::tips::tip(crate::tips::Scope::Design, "Create Mesh Section Sketch").unwrap();
    assert_eq!(tip.line, "Creates a sketch that contains a section line where a plane intersects a mesh body.");
  }

  #[test]
  fn tessellate_waits_for_a_solid_and_fills_its_refinement_from_it() {
    let (mut app, mesh, solid) = app();
    // nothing picked, or only the mesh: OK grey, Medium ticked, zero lengths
    let mut d = open(&mut app, &mesh_ui::TESSELLATE, vec![Sel::Body { body: mesh }]);
    assert_eq!(value(&d, "body"), json!([]));
    assert_eq!(value(&d, "refinement"), json!("medium"));
    assert_eq!(value(&d, "max_edge"), json!("0.00 mm"));
    assert_eq!((value(&d, "normal_deviation"), value(&d, "aspect_ratio")), (json!("15.00"), json!("21.50")));
    assert!(!d.ok_enabled());
    assert_eq!(d.missing_input(), "Select a solid or surface body");
    let (texts, _) = show(&mut app, &mut d);
    for t in [
      "TESSELLATE",
      "Body",
      "Select",
      "Refinement Settings",
      "Refinement",
      "Medium",
      "Surface Deviation",
      "Normal Deviation",
      "Maximum Edge Length",
      "Aspect Ratio",
      "Create Quads",
      "Preview",
    ] {
      assert!(texts.iter().any(|x| x == t), "{t} in {texts:?}");
    }
    d.cancel(&mut app.s);
    // the solid: the maximum edge becomes half its diagonal, and OK tessellates it
    let diag = app.s.model.body(solid).unwrap().diagonal();
    let mut d = open(&mut app, &mesh_ui::TESSELLATE, vec![Sel::Body { body: solid }]);
    assert_eq!(value(&d, "max_edge"), json!(mesh_ui::length_text(&app.s, diag / 2.0)));
    assert!(d.ok_enabled());
    let (texts, _) = show(&mut app, &mut d);
    assert!(texts.iter().any(|x| x == "1 selected"), "{texts:?}");
    d.ok_requested = true;
    let (_, keep) = show(&mut app, &mut d);
    assert!(!keep, "OK closes it");
    assert_eq!(app.s.design.features.last().unwrap().name, "Tessellate1");
    assert!(mesh_ui::is_mesh(&app.s, solid));
  }

  #[test]
  fn repair_shows_its_type_and_a_clean_quick_analysis() {
    let (mut app, mesh, _) = app();
    let mut d = open(&mut app, &mesh_ui::REPAIR, vec![Sel::Body { body: mesh }]);
    assert_eq!(value(&d, "type"), json!("stitch_and_remove"));
    assert!(d.ok_enabled());
    let (texts, _) = show(&mut app, &mut d);
    for t in [
      "REPAIR",
      "Body",
      "1 selected",
      "Repair Type",
      "Stitch and Remove",
      "Preview",
      "Quick Analysis",
      "Detailed Analysis",
    ] {
      assert!(texts.iter().any(|x| x == t), "{t} in {texts:?}");
    }
    assert!(!texts.iter().any(|x| x.contains("problems")), "a clean mesh: {texts:?}");
    d.cancel(&mut app.s);
  }

  #[test]
  fn face_group_dialogs() {
    let (mut app, mesh, solid) = app();
    let mut d =
      open(&mut app, &mesh_ui::GENERATE_FACE_GROUPS, vec![Sel::Body { body: mesh }, Sel::Body { body: solid }]);
    assert_eq!(value(&d, "bodies"), json!([mesh.0]), "mesh bodies only");
    assert_eq!(
      (value(&d, "type"), value(&d, "angle"), value(&d, "min_size")),
      (json!("fast"), json!("25.0 deg"), json!("0.20"))
    );
    let (texts, _) = show(&mut app, &mut d);
    for t in [
      "GENERATE FACE GROUPS",
      "Bodies/Face Groups",
      "Type",
      "Fast",
      "Angle Threshold",
      "Minimum Face Group size",
      "Preview",
    ] {
      assert!(texts.iter().any(|x| x == t), "{t} in {texts:?}");
    }
    // while it is open the picked mesh is outlined in selection blue, not filled
    app.s.selection = vec![Sel::Body { body: mesh }];
    app.dialog = Some(d);
    assert_eq!(mesh_ui::outlined_bodies(&app), [mesh]);
    let opts = crate::viewport::scene_options(&mut app);
    let (scene, picks) = fox_engine::scene::SceneBuilder::default().build(&app.s, &opts);
    let of_mesh = |m: &&fox_engine::render::Mesh| {
      m.tri_ids.first().and_then(|id| picks.get(*id)).is_some_and(|p| crate::select::pick_body(&p) == Some(mesh))
    };
    let drawn: Vec<_> = scene.meshes.iter().filter(of_mesh).collect();
    assert!(!drawn.is_empty());
    for m in drawn {
      assert_ne!(m.color, fox_engine::scene::SELECT_COLOR, "not filled blue");
      assert_eq!(m.outline, Some(fox_engine::scene::SELECT_COLOR));
    }
    let mut d = app.dialog.take().unwrap();
    d.ok_requested = true;
    show(&mut app, &mut d);
    assert_eq!(app.s.design.features.last().unwrap().name, "GenerateFaceGroups1");
    // Combine: grey until two faces are picked
    let faces: Vec<u32> = app.s.model.body(mesh).unwrap().faces().iter().map(|f| f.id).collect();
    let mut d = open(&mut app, &mesh_ui::COMBINE_FACE_GROUPS, vec![Sel::Face { body: mesh, face: faces[0] }]);
    assert!(!d.ok_enabled());
    assert_eq!(d.missing_input(), mesh_ui::COMBINE_PROMPT);
    let (texts, _) = show(&mut app, &mut d);
    assert!(texts.iter().any(|x| x == "Face Groups"), "{texts:?}");
    // while it is open a click on the mesh picks a face, not the whole body
    app.dialog = Some(d);
    app.s.selection.clear();
    crate::select::pick(&mut app, Pick::Face { body: mesh, face: faces[1] }, false);
    assert_eq!(app.s.selection, [Sel::Face { body: mesh, face: faces[1] }]);
    let mut d = app.dialog.take().unwrap();
    d.cancel(&mut app.s);
    crate::select::pick(&mut app, Pick::Face { body: mesh, face: faces[1] }, false);
    assert_eq!(app.s.selection, [Sel::Body { body: mesh }], "otherwise the mesh is picked whole");
  }

  #[test]
  fn the_section_sketch_waits_for_a_body_and_a_plane() {
    let (mut app, mesh, _) = app();
    let mut d = open(&mut app, &mesh_ui::SECTION_SKETCH, vec![Sel::Body { body: mesh }]);
    assert!(value(&d, "plane").is_null(), "no plane until one is picked");
    assert!(!d.ok_enabled());
    assert_eq!(d.missing_input(), "Select a plane");
    let (texts, _) = show(&mut app, &mut d);
    for t in ["CREATE MESH SECTION SKETCH", "Body", "Section Plane", "Numerical Inputs"] {
      assert!(texts.iter().any(|x| x == t), "{t} in {texts:?}");
    }
    d.cancel(&mut app.s);
    // a construction plane through the prism
    let r = app.run("plane.offset", json!({"plane": "xy", "offset": "2 mm"})).unwrap();
    let plane = fox_engine::model::FeatureId(r["feature"].as_u64().unwrap() as u32);
    let mut d =
      open(&mut app, &mesh_ui::SECTION_SKETCH, vec![Sel::Body { body: mesh }, Sel::Feature { feature: plane }]);
    assert!(d.ok_enabled(), "{}", d.missing_input());
    d.ok_requested = true;
    show(&mut app, &mut d);
    assert!(matches!(app.s.design.features.last().unwrap().kind, fox_engine::model::FeatureKind::Sketch { .. }));
  }

  #[test]
  fn direct_edit_opens_the_direct_mesh_editing_tab_and_finish_leaves_an_edited_body() {
    let (mut app, mesh, solid) = app();
    // nothing selected: Direct Edit asks for a mesh
    mesh_ui::run(&mut app, mesh_ui::Act::DirectEdit);
    assert!(!app.mesh_edit.active);
    app.s.selection = vec![Sel::Body { body: mesh }];
    let n = app.s.design.features.len();
    mesh_ui::run(&mut app, mesh_ui::Act::DirectEdit);
    assert!(app.mesh_edit.active && app.mesh_edit.body == Some(mesh));
    assert_eq!(app.tab, Tab::MeshEdit);
    assert_eq!(Tab::MeshEdit.label(), "DIRECT MESH EDITING");
    assert_eq!(app.s.design.features.len(), n + 1, "a base feature joins the timeline");
    // the other bodies are ghosted
    assert_eq!(mesh_ui::ghosted(&app), vec![(solid, mesh_ui::GHOST_ALPHA)]);
    // the tab's panels, FINISH DIRECT EDITING last
    let labels: Vec<&str> = crate::menus::group_labels(Tab::MeshEdit);
    assert_eq!(
      labels,
      ["CREATE", "PREPARE", "MODIFY", "CONSTRUCT", "INSPECT", "SELECT", "EXPORT", "FINISH DIRECT EDITING"]
    );
    // the palette's Selection Filter: Faces picks a face of the mesh, Mesh Bodies the body
    let face = app.s.model.body(mesh).unwrap().faces()[0].id;
    crate::select::pick(&mut app, Pick::Face { body: mesh, face }, false);
    assert_eq!(app.s.selection, [Sel::Face { body: mesh, face }]);
    app.mesh_edit.filter = mesh_ui::SelectFilter::Bodies;
    crate::select::pick(&mut app, Pick::Face { body: mesh, face }, false);
    assert_eq!(app.s.selection, [Sel::Body { body: mesh }]);
    // the palette's counts
    app.s.selection = vec![Sel::Face { body: mesh, face }, Sel::Body { body: mesh }];
    assert_eq!(mesh_ui::counts(&app.s), (1, 0, 1));
    mesh_ui::run(&mut app, mesh_ui::Act::Finish);
    assert!(!app.mesh_edit.active);
    assert_eq!(app.tab, Tab::Mesh);
    let name = format!("{}_Edited", app.s.model.body(mesh).unwrap().name);
    assert!(app.s.model.bodies.iter().any(|b| b.name == name), "{name}");
    assert!(mesh_ui::ghosted(&app).is_empty());
    // Create Base Mesh Feature opens the tab with no body
    mesh_ui::run(&mut app, mesh_ui::Act::BaseMesh);
    assert!(app.mesh_edit.active && app.mesh_edit.body.is_none());
    mesh_ui::finish(&mut app);
  }
}
