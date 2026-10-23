//! The MESH tab's Tessellate, Repair, Generate and Combine Face Groups and Create Mesh Section
//! Sketch dialogs, and the DIRECT MESH EDITING environment that Direct Edit and Create Base Mesh
//! Feature open: its contextual tab, the docked MESH SELECTION PALETTE and the other bodies
//! ghosted while it is open.
//!
//! Mesh bodies are asked for through [`is_mesh`] (see `fox_model::regen::BodyKind`).

use egui::{Color32, RichText, vec2};
use fox_engine::Session;
use fox_engine::model::{BodyId, FeatureId};
use serde_json::{Value, json};

use crate::forms::{Form, Input, State};
use crate::icons::{self, Icon};
use crate::{App, panels, theme};

/// Is `body` a mesh body?
pub(crate) fn is_mesh(s: &Session, body: BodyId) -> bool {
  fox_engine::mesh_prep::is_mesh_body(s, body)
}

pub static TESSELLATE: Form = Form {
  title: "TESSELLATE",
  name: "Tessellate",
  hint: "Converts an existing solid or surface body into a mesh body.",
  icon: Icon::Tessellate,
  cmd: "mesh.tessellate",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "body", label: "Body", required: true },
    Input::Choice { key: "refinement", label: "Refinement", options: REFINEMENTS },
    Input::Expr { key: "surface_deviation", label: "Surface Deviation", default: "0.00 mm" },
    Input::Expr { key: "normal_deviation", label: "Normal Deviation", default: "15.00" },
    Input::Expr { key: "max_edge", label: "Maximum Edge Length", default: "0.00 mm" },
    Input::Expr { key: "aspect_ratio", label: "Aspect Ratio", default: "21.50" },
    Input::Check { key: "quads", label: "Create Quads" },
  ],
  picks_model: true,
};

/// Tessellate's Refinement drop-down, as Fusion lists it (Medium is the default).
pub const REFINEMENTS: &[(&str, &str)] =
  &[("high", "High"), ("medium", "Medium"), ("low", "Low"), ("custom", "Custom")];

pub static REPAIR: Form = Form {
  title: "REPAIR",
  name: "Repair",
  hint: "Fixes holes, flipped triangles and other problems in a mesh.",
  icon: Icon::Repair,
  cmd: "mesh.repair",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Body", required: true },
    Input::Choice { key: "type", label: "Repair Type", options: REPAIR_TYPES },
  ],
  picks_model: true,
};

/// Repair's Repair Type drop-down (Stitch and Remove is the default).
pub const REPAIR_TYPES: &[(&str, &str)] = &[
  ("close_holes", "Close Holes"),
  ("stitch_and_remove", "Stitch and Remove"),
  ("wrap", "Wrap"),
  ("one_touch_fix", "One Touch Fix"),
  ("rebuild", "Rebuild"),
];

pub static GENERATE_FACE_GROUPS: Form = Form {
  title: "GENERATE FACE GROUPS",
  name: "Generate Face Groups",
  hint: "Divides a mesh into groups of faces that belong together.",
  icon: Icon::GenFaceGroups,
  cmd: "mesh.face_groups",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Bodies/Face Groups", required: true },
    Input::Choice { key: "type", label: "Type", options: &[("fast", "Fast"), ("accurate", "Accurate")] },
    Input::Expr { key: "angle", label: "Angle Threshold", default: "25.0 deg" },
    Input::Expr { key: "min_size", label: "Minimum Face Group size", default: "0.20" },
  ],
  picks_model: true,
};

pub static COMBINE_FACE_GROUPS: Form = Form {
  title: "COMBINE FACE GROUPS",
  name: "Combine Face Groups",
  hint: "Merges mesh face groups into one.",
  icon: Icon::CombineFaceGroups,
  cmd: "mesh.combine_face_groups",
  fixed: &[],
  inputs: &[Input::Faces { key: "faces", label: "Face Groups" }],
  picks_model: true,
};

pub static SECTION_SKETCH: Form = Form {
  title: "CREATE MESH SECTION SKETCH",
  name: "Create Mesh Section Sketch",
  hint: "Creates a sketch that contains a section line where a plane intersects a mesh body.",
  icon: Icon::MeshSection,
  cmd: "mesh.section_sketch",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "body", label: "Body", required: true },
    Input::Plane { key: "plane", label: "Section Plane" },
    Input::Expr { key: "offset", label: "Distance", default: "0.00 mm" },
  ],
  picks_model: true,
};

/// The bodies drawn outlined in selection blue rather than filled: GENERATE FACE GROUPS' picked
/// meshes, as Fusion shows them while it is open.
pub(crate) fn outlined_bodies(app: &App) -> Vec<BodyId> {
  let Some(d) = app.dialog.as_ref().filter(|d| d.kind == crate::dialogs::Kind::Form(&GENERATE_FACE_GROUPS)) else {
    return Vec::new();
  };
  let picked = d.form_value("bodies").and_then(Value::as_array).cloned().unwrap_or_default();
  picked.iter().filter_map(|b| Some(BodyId(b.as_u64()? as u32))).collect()
}

/// The mesh forms.
pub(crate) fn is_mesh_form(form: &Form) -> bool {
  [&TESSELLATE, &REPAIR, &GENERATE_FACE_GROUPS, &COMBINE_FACE_GROUPS, &SECTION_SKETCH]
    .iter()
    .any(|f| std::ptr::eq(*f, form))
}

fn slot(form: &Form, k: &str) -> Option<usize> {
  form.inputs.iter().position(|x| x.key() == k)
}

/// A length (mm) as the dialog shows it, in the design's unit.
pub(crate) fn length_text(s: &Session, mm: f64) -> String {
  s.design.units.fixed(mm, if mm.abs() < 0.0005 { 2 } else { 3 })
}

/// The refinement's values for the picked body (zero lengths until one is picked, as Fusion's).
pub(crate) fn refinement_values(s: &Session, level: &str, body: Option<BodyId>) -> [String; 4] {
  let diag = body.and_then(|b| s.model.body(b)).map(|b| b.diagonal());
  let r = fox_engine::model::meshprep::refinement(level, diag.unwrap_or(0.0));
  let len = |v: f64| length_text(s, if diag.is_some() { v } else { 0.0 });
  [len(r.surface_deviation), format!("{:.2}", r.normal_deviation), len(r.max_edge), format!("{:.2}", r.aspect_ratio)]
}

/// Puts the refinement's values in Tessellate's rows (unless it is Custom).
pub(crate) fn fill_refinement(form: &Form, state: &mut State, s: &Session) {
  let level = slot(form, "refinement").and_then(|i| state.values[i].as_str()).unwrap_or("medium").to_string();
  if level == "custom" {
    return;
  }
  let body = picked_body(form, state);
  let values = refinement_values(s, &level, body);
  for (k, v) in ["surface_deviation", "normal_deviation", "max_edge", "aspect_ratio"].iter().zip(values) {
    if let Some(i) = slot(form, k) {
      state.values[i] = json!(v);
    }
  }
}

fn picked_body(form: &Form, state: &State) -> Option<BodyId> {
  let i = slot(form, "body").or_else(|| slot(form, "bodies"))?;
  state.values[i].as_array()?.first()?.as_u64().map(|b| BodyId(b as u32))
}

/// Defaults Fusion's dialogs open with that aren't a list's first entry.
pub(crate) fn prefill(form: &Form, state: &mut State, s: &Session) {
  let set = |state: &mut State, k: &str, v: &str| {
    if let Some(i) = slot(form, k) {
      state.values[i] = json!(v);
    }
  };
  if std::ptr::eq(form, &TESSELLATE) {
    set(state, "refinement", "medium");
    fill_refinement(form, state, s);
  } else if std::ptr::eq(form, &REPAIR) {
    set(state, "type", "stitch_and_remove");
  }
}

/// After the picks are read: Tessellate takes solid and surface bodies, the mesh commands mesh
/// bodies, and only one body each where Fusion's row takes one; Create Mesh Section Sketch's
/// plane must be picked (no origin plane by default).
pub(crate) fn sync(form: &Form, state: &mut State, before: &[Value], s: &Session) {
  let only = |state: &mut State, k: &str, keep: &dyn Fn(BodyId) -> bool, one: bool| {
    if let Some(i) = slot(form, k)
      && let Some(a) = state.values[i].as_array()
    {
      let mut kept: Vec<Value> =
        a.iter().filter(|v| v.as_u64().is_some_and(|b| keep(BodyId(b as u32)))).cloned().collect();
      if one {
        kept.truncate(1);
      }
      state.values[i] = json!(kept);
    }
  };
  match form.cmd {
    "mesh.tessellate" => {
      only(state, "body", &|b| !is_mesh(s, b), true);
      let changed = |state: &State| slot(form, "body").map(|i| state.values[i] != before[i]).unwrap_or(false);
      if changed(state) {
        fill_refinement(form, state, s);
      }
    }
    "mesh.repair" => only(state, "bodies", &|b| is_mesh(s, b), true),
    "mesh.face_groups" => only(state, "bodies", &|b| is_mesh(s, b), false),
    "mesh.section_sketch" => {
      only(state, "body", &|b| is_mesh(s, b), true);
      if let Some(i) = slot(form, "plane")
        && state.values[i] == json!("xy")
        && state.chosen[i].is_none()
      {
        state.values[i] = Value::Null;
      }
    }
    _ => {}
  }
}

/// What the mesh forms still need, as their cursor prompts say it.
pub(crate) fn missing(form: &Form, state: &State) -> Option<&'static str> {
  let empty = |k: &str| {
    slot(form, k).is_none_or(|i| state.values[i].is_null() || state.values[i].as_array().is_some_and(Vec::is_empty))
  };
  match form.cmd {
    "mesh.combine_face_groups" => {
      let n = slot(form, "faces").and_then(|i| state.values[i].as_array().map(Vec::len)).unwrap_or(0);
      (n < 2).then_some(COMBINE_PROMPT)
    }
    "mesh.section_sketch" if empty("body") => Some("Select a mesh body"),
    "mesh.section_sketch" if empty("plane") => Some("Select a plane"),
    "mesh.tessellate" if empty("body") => Some("Select a solid or surface body"),
    "mesh.repair" if empty("bodies") => Some("Select a mesh body"),
    "mesh.face_groups" if empty("bodies") => Some("Select mesh bodies"),
    _ => None,
  }
}

/// Combine Face Groups' prompt.
pub(crate) const COMBINE_PROMPT: &str = "Select two or more adjacent face groups";

/// The command's parameters: a single Body row hands the command the body, not a list.
pub(crate) fn params(form: &Form, p: &mut Value) {
  if matches!(form.cmd, "mesh.tessellate" | "mesh.section_sketch")
    && let Some(b) = p.get("body").and_then(Value::as_array).and_then(|a| a.first()).cloned()
  {
    p["body"] = b;
  }
}

// ---------------------------------------------------------------- DIRECT MESH EDITING

/// The Direct Mesh Editing environment's commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
  /// MODIFY > Direct Edit: edit the selected mesh body.
  DirectEdit,
  /// CREATE > Create Base Mesh Feature: a base feature for mesh bodies made inside it.
  BaseMesh,
  /// FINISH DIRECT EDITING.
  Finish,
}

pub fn info(a: Act) -> (Icon, &'static str, &'static str) {
  match a {
    Act::DirectEdit => (Icon::DirectEdit, "Direct Edit", "Edits a mesh's triangles directly."),
    Act::BaseMesh => {
      (Icon::BaseMesh, "Create Base Mesh Feature", "Starts a block of mesh edits that aren't recorded in history.")
    }
    Act::Finish => (Icon::Finish, "Finish Direct Editing", "Finish editing: the edited mesh is kept as a new body"),
  }
}

/// The MESH SELECTION PALETTE's Selection Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectType {
  #[default]
  Paint,
  Window,
  Freeform,
}

/// The MESH SELECTION PALETTE's Selection Filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectFilter {
  #[default]
  Faces,
  FaceGroups,
  Bodies,
}

/// The DIRECT MESH EDITING tab, while open.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshEdit {
  pub active: bool,
  /// The mesh body being edited (Direct Edit); none for Create Base Mesh Feature.
  pub body: Option<BodyId>,
  /// The base feature the edits go into.
  pub feature: Option<FeatureId>,
  pub select_type: SelectType,
  pub filter: SelectFilter,
  /// Brush Size, as a fraction of the slider.
  pub brush: f32,
  pub back_faces: bool,
}

/// Opens the DIRECT MESH EDITING tab: a base feature joins the timeline.
pub fn enter(app: &mut App, body: Option<BodyId>) {
  app.cancel_dialog();
  if app.s.edit_sketch.is_some() {
    app.finish_sketch();
  }
  let feature = app.run("mesh.edit.start", json!({})).and_then(|r| r["feature"].as_u64()).map(|f| FeatureId(f as u32));
  app.mesh_edit = MeshEdit { active: true, body, feature, brush: 0.25, ..MeshEdit::default() };
  app.tab = panels::Tab::MeshEdit;
  app.hint("Select faces to edit");
}

pub fn run(app: &mut App, a: Act) {
  match a {
    Act::DirectEdit => {
      let body = app.s.selection.iter().find_map(|x| match x {
        fox_engine::Sel::Body { body } | fox_engine::Sel::Face { body, .. } | fox_engine::Sel::Edge { body, .. } => {
          Some(*body)
        }
        _ => None,
      });
      match body {
        Some(b) if is_mesh(&app.s, b) => enter(app, Some(b)),
        _ => app.hint("Select a mesh body to edit"),
      }
    }
    Act::BaseMesh => enter(app, None),
    Act::Finish => finish(app),
  }
}

/// FINISH DIRECT EDITING: the edited body is kept as "<name>_Edited" and the MESH tab returns.
pub fn finish(app: &mut App) {
  if !app.mesh_edit.active {
    return;
  }
  app.cancel_dialog();
  let mut p = json!({});
  if let Some(f) = app.mesh_edit.feature {
    p["feature"] = json!(f.0);
  }
  if let Some(b) = app.mesh_edit.body {
    p["body"] = json!(b.0);
  }
  app.run("mesh.edit.finish", p);
  app.mesh_edit = MeshEdit::default();
  app.tab = panels::Tab::Mesh;
  app.end_hint();
}

/// While editing, every other body is drawn ghosted (white and see-through), as in Fusion.
pub(crate) fn ghosted(app: &App) -> Vec<(BodyId, u8)> {
  if !app.mesh_edit.active {
    return Vec::new();
  }
  app.s.model.bodies.iter().map(|b| b.id).filter(|b| Some(*b) != app.mesh_edit.body).map(|b| (b, GHOST_ALPHA)).collect()
}

/// How opaque a ghosted body is.
pub(crate) const GHOST_ALPHA: u8 = 60;

/// What a click on a mesh body's face picks while faces of mesh bodies are being picked (they
/// are otherwise picked whole): Combine Face Groups' faces, and in Direct Mesh Editing the face
/// (Selection Filter Faces) or its whole face group (Face Groups). None otherwise.
pub(crate) fn face_pick(app: &App, p: fox_engine::scene::Pick) -> Option<Vec<fox_engine::Sel>> {
  use fox_engine::Sel;
  let fox_engine::scene::Pick::Face { body, face } = p else { return None };
  if !is_mesh(&app.s, body) {
    return None;
  }
  let combining = app
    .dialog
    .as_ref()
    .is_some_and(|d| matches!(d.kind, crate::dialogs::Kind::Form(f) if std::ptr::eq(f, &COMBINE_FACE_GROUPS)));
  let filter = if combining {
    SelectFilter::Faces
  } else if app.mesh_edit.active {
    app.mesh_edit.filter
  } else {
    return None;
  };
  match filter {
    SelectFilter::Faces => Some(vec![Sel::Face { body, face }]),
    SelectFilter::FaceGroups => {
      let group = app.s.model.face_group(body, face);
      let b = app.s.model.body(body)?;
      Some(
        b.faces()
          .iter()
          .map(|f| f.id)
          .filter(|f| *f == face || group.is_some() && app.s.model.face_group(body, *f) == group)
          .map(|face| Sel::Face { body, face })
          .collect(),
      )
    }
    SelectFilter::Bodies => None,
  }
}

/// The selection counts the palette shows: faces, face groups and mesh bodies.
pub(crate) fn counts(s: &Session) -> (usize, usize, usize) {
  let mut faces = 0;
  let mut groups: Vec<(BodyId, u32)> = Vec::new();
  let mut bodies: Vec<BodyId> = Vec::new();
  for x in &s.selection {
    match x {
      fox_engine::Sel::Face { body, face } => {
        faces += 1;
        if let Some(g) = s.model.face_group(*body, *face)
          && !groups.contains(&(*body, g))
        {
          groups.push((*body, g));
        }
      }
      fox_engine::Sel::Body { body } if is_mesh(s, *body) && !bodies.contains(body) => bodies.push(*body),
      _ => {}
    }
  }
  (faces, groups.len(), bodies.len())
}

/// The MESH SELECTION PALETTE, docked on the right of the canvas while the tab is open.
pub fn palette(app: &mut App, ctx: &egui::Context) {
  if !app.mesh_edit.active {
    return;
  }
  let at = panels::palette_pos(
    app.canvas,
    app.dialog.is_some().then(|| ctx.memory(|m| m.area_rect(egui::Id::new("feature-dialog")))).flatten(),
  );
  egui::Area::new(egui::Id::new("mesh-selection-palette"))
    .fixed_pos(at)
    .pivot(egui::Align2::RIGHT_TOP)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      panels::dialog_frame().show(ui, |ui| {
        let width = 270.0;
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        panels::dialog_header(ui, Icon::Select, "MESH SELECTION PALETTE", width);
        egui::Frame::new().inner_margin(egui::Margin { left: 10, right: 14, top: 6, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
          palette_rows(app, ui);
        });
        let sep = ui.allocate_exact_size(vec2(width, 1.0), egui::Sense::hover()).0;
        ui.painter().rect_filled(sep, 0.0, Color32::from_gray(215));
        egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
          ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if panels::primary_button(ui, "Finish Direct Editing", icons::accent_of(Icon::Finish)).clicked() {
              finish(app);
            }
          });
        });
      });
    });
}

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
  ui.horizontal(|ui| {
    ui.set_height(27.0);
    ui.add_space(4.0);
    ui.label(label);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
  });
}

fn section(ui: &mut egui::Ui, title: &str) {
  let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), egui::Sense::hover());
  crate::dialogs::group_triangle(ui.painter(), r.left_center() + vec2(5.0, 0.0), true);
  ui.painter().text(
    r.left_center() + vec2(16.0, 0.0),
    egui::Align2::LEFT_CENTER,
    title,
    egui::FontId::proportional(13.0),
    theme::TEXT,
  );
}

/// A small push button drawn with a glyph (Modify Selection's pairs).
fn glyph_button(ui: &mut egui::Ui, glyph: &str, tip: &str) -> egui::Response {
  ui.add(egui::Button::new(RichText::new(glyph).size(13.0)).min_size(vec2(24.0, 22.0))).on_hover_text(tip)
}

fn palette_rows(app: &mut App, ui: &mut egui::Ui) {
  let me = &mut app.mesh_edit;
  // laid out right to left, so the last listed is the leftmost
  row(ui, "Selection Type", |ui| {
    for (t, icon, tip) in [
      (SelectType::Freeform, Icon::FreeformSel, "Freeform Selection"),
      (SelectType::Window, Icon::WindowSel, "Window Selection"),
      (SelectType::Paint, Icon::PaintSel, "Paint Selection"),
    ] {
      if icons::icon_button(ui, icon, 22.0, tip, me.select_type == t, None).clicked() {
        me.select_type = t;
      }
    }
  });
  row(ui, "Selection Filter", |ui| {
    for (t, icon, tip) in [
      (SelectFilter::Bodies, Icon::InsertMesh, "Mesh Bodies"),
      (SelectFilter::FaceGroups, Icon::FaceGroups, "Face Groups"),
      (SelectFilter::Faces, Icon::Tessellate, "Faces"),
    ] {
      if icons::icon_button(ui, icon, 22.0, tip, me.filter == t, None).clicked() {
        me.filter = t;
      }
    }
  });
  row(ui, "Brush Size", |ui| {
    ui.spacing_mut().slider_width = 110.0;
    ui.add(egui::Slider::new(&mut me.brush, 0.0..=1.0).show_value(false));
  });
  row(ui, "Select Back Faces", |ui| {
    ui.checkbox(&mut me.back_faces, "");
  });
  ui.add_space(2.0);
  section(ui, "Modify Selection");
  let mut act: Option<&str> = None;
  for (label, pair) in [
    ("Expand", [("+", "Expand Selection"), ("−", "Shrink Selection")]),
    ("Grow/Shrink", [("⊞", "Grow Selection"), ("⊟", "Shrink Selection")]),
    ("Invert", [("🔀", "Invert Selection"), ("⊗", "Clear Selection")]),
  ] {
    row(ui, label, |ui| {
      for (g, tip) in pair.iter().rev() {
        if glyph_button(ui, g, tip).clicked() {
          act = Some(tip);
        }
      }
    });
  }
  if let Some(a) = act {
    modify_selection(app, a);
  }
  ui.add_space(2.0);
  let (faces, groups, bodies) = counts(&app.s);
  for (label, n) in [("Selected Faces", faces), ("Selected Face Groups", groups), ("Selected Mesh Bodies", bodies)] {
    row(ui, label, |ui| {
      ui.label(RichText::new(n.to_string()).color(theme::TEXT));
    });
  }
}

/// Modify Selection's buttons, on the selected faces of the edited body.
fn modify_selection(app: &mut App, act: &str) {
  use fox_engine::Sel;
  match act {
    "Clear Selection" => app.s.selection.clear(),
    "Invert Selection" => {
      let Some(body) = app.mesh_edit.body else { return };
      let Some(b) = app.s.model.body(body) else { return };
      let picked: Vec<u32> = app
        .s
        .selection
        .iter()
        .filter_map(|x| if let Sel::Face { body: bb, face } = x { (*bb == body).then_some(*face) } else { None })
        .collect();
      app.s.selection =
        b.faces().iter().map(|f| f.id).filter(|f| !picked.contains(f)).map(|face| Sel::Face { body, face }).collect();
    }
    "Expand Selection" | "Grow Selection" => {
      let Some(body) = app.mesh_edit.body else { return };
      let grown = neighbours(&app.s, body, &app.s.selection);
      for face in grown {
        let sel = Sel::Face { body, face };
        if !app.s.selection.contains(&sel) {
          app.s.selection.push(sel);
        }
      }
    }
    "Shrink Selection" => {
      let Some(body) = app.mesh_edit.body else { return };
      // the faces on the selection's border (with an unselected neighbour) drop out
      let picked: Vec<u32> = app
        .s
        .selection
        .iter()
        .filter_map(|x| if let Sel::Face { body: bb, face } = x { (*bb == body).then_some(*face) } else { None })
        .collect();
      let border: Vec<u32> = picked
        .iter()
        .copied()
        .filter(|f| neighbours(&app.s, body, &[Sel::Face { body, face: *f }]).iter().any(|n| !picked.contains(n)))
        .collect();
      app.s.selection.retain(|x| !matches!(x, Sel::Face { body: bb, face } if *bb == body && border.contains(face)));
    }
    _ => {}
  }
}

/// Faces of `body` sharing an edge with the selected ones (and not selected).
fn neighbours(s: &Session, body: BodyId, sel: &[fox_engine::Sel]) -> Vec<u32> {
  let Some(b) = s.model.body(body) else { return Vec::new() };
  let picked: Vec<u32> = sel
    .iter()
    .filter_map(
      |x| if let fox_engine::Sel::Face { body: bb, face } = x { (*bb == body).then_some(*face) } else { None },
    )
    .collect();
  let mut out = Vec::new();
  for e in b.edges() {
    let [a, c] = e.faces;
    if a == 0 || c == 0 {
      continue;
    }
    for (x, y) in [(a, c), (c, a)] {
      if picked.contains(&x) && !picked.contains(&y) && !out.contains(&y) {
        out.push(y);
      }
    }
  }
  out
}
