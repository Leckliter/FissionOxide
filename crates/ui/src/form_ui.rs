//! The Form workspace, as Fusion's: Create Form opens the FORM contextual tab, where primitives
//! add form bodies and the MODIFY and SYMMETRY tools edit the control cage of the form being
//! edited. The cage is drawn over the viewport (as flat faces in Box display, Alt+1; as a frame
//! over the smooth body in Control Frame display, Alt+2; faintly in Smooth display, Alt+3) and its
//! vertices, edges and faces are picked there. Edit Form shows a triad at the picked elements:
//! its arrows move them, its rings turn them and its centre scales them, with the soft
//! modification radius from the FORM panel. Finish Form returns to the timeline, where the form
//! is one feature whose body is the cage's smooth surface as exact B-spline faces.

use std::collections::BTreeSet;

use egui::{Color32, Pos2, Stroke, vec2};
use fox_engine::kernel::glam::{DMat4, DVec3};
use fox_engine::model::form::{Display, FormCage, Selection, edit};
use fox_engine::model::{FeatureId, FeatureKind};
use serde_json::{Value, json};

use crate::App;
use crate::forms::{Form, Input};
use crate::icons::Icon;
use crate::manipulators::{axis_param, ring_angle};

/// A FORM tab command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
  /// Create Form: open the FORM tab.
  Enter,
  Finish,
  Primitive(&'static Form),
  EditForm,
  InsertEdge,
  InsertPoint,
  Subdivide,
  Delete,
  Extrude,
  Bridge,
  FillHole,
  Crease,
  Uncrease,
  Pull,
  MakeUniform,
  Flip,
  Weld,
  MirrorInternal,
  CircularInternal,
  MirrorDuplicate,
  CircularDuplicate,
  ClearSymmetry,
  Display(Display),
}

/// Which cage elements a click picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
  #[default]
  All,
  Vertices,
  Edges,
  Faces,
}

/// A triad handle being dragged: the axis (0-2, cage axes) and what it does.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Grip {
  Arrow(usize),
  Ring(usize),
  Scale,
}

#[derive(Debug, Clone)]
struct Drag {
  grip: Grip,
  start: f64,
  /// The transform so far (cage coordinates).
  m: DMat4,
  /// Readout: distance, degrees or factor.
  value: f64,
  start_px: Pos2,
}

/// The FORM tab's state.
#[derive(Debug, Default)]
pub struct FormEdit {
  pub active: bool,
  pub feature: Option<FeatureId>,
  /// Form features there were when the tab opened (a new one is taken up for editing).
  known: BTreeSet<FeatureId>,
  pub sel: Selection,
  pub filter: Filter,
  pub tool: Option<Act>,
  /// Bridge's first side, kept while the second is picked.
  side_a: Vec<[u32; 2]>,
  drag: Option<Drag>,
  soft: f64,
  t: f64,
  distance: f64,
  faces: u32,
  star: bool,
  exact: bool,
  limit: bool,
  count: u32,
  plane: usize,
}

impl FormEdit {
  pub fn enter(&mut self, app_features: impl Iterator<Item = FeatureId>, feature: Option<FeatureId>) {
    *self = FormEdit {
      active: true,
      feature,
      known: app_features.collect(),
      t: 0.5,
      distance: 10.0,
      faces: 1,
      star: true,
      limit: true,
      count: 4,
      ..FormEdit::default()
    };
  }
}

fn forms_in(app: &App) -> Vec<FeatureId> {
  app.s.design.features.iter().filter(|f| matches!(f.kind, FeatureKind::Form { .. })).map(|f| f.id).collect()
}

/// Opens the FORM tab on a form feature (or with none yet, for a primitive to make one).
pub fn enter(app: &mut App, feature: Option<FeatureId>) {
  app.cancel_dialog();
  if app.s.edit_sketch.is_some() {
    app.finish_sketch();
  }
  let known = forms_in(app);
  app.form.enter(known.into_iter(), feature);
  app.tab = crate::panels::Tab::Form;
  app.hint("Form: add a primitive, or pick cage vertices, edges and faces to edit (Alt+1/2/3 change the display)");
}

pub fn info(a: Act) -> (Icon, &'static str, &'static str) {
  match a {
    Act::Enter => (Icon::CreateForm, "Create Form", "Open the Form workspace to sculpt T-spline bodies"),
    Act::Finish => {
      (Icon::Finish, "Finish Form", "Back to the timeline: the form is one feature with an exact smooth body")
    }
    Act::Primitive(f) => (f.icon, f.name, f.hint),
    Act::EditForm => (Icon::Move, "Edit Form", "Move, rotate or scale picked vertices, edges and faces with the triad"),
    Act::InsertEdge => (Icon::SplitFace, "Insert Edge", "A new edge loop across the quads through the picked edges"),
    Act::InsertPoint => (Icon::CPoint, "Insert Point", "Split the picked edges at their middles and join the points"),
    Act::Subdivide => (Icon::Grid, "Subdivide", "Split the picked faces (all if none) into quads"),
    Act::Delete => (Icon::Remove, "Delete", "Delete the picked faces, edges or vertices"),
    Act::Extrude => (Icon::Extrude, "Extrude", "Pull the picked faces out, or new faces off picked open edges"),
    Act::Bridge => (Icon::Loft, "Bridge", "Join two picked chains of open edges with faces"),
    Act::FillHole => (Icon::Patch, "Fill Hole", "Close the hole through the picked open edge"),
    Act::Crease => (Icon::Chamfer, "Crease", "Keep the picked edges sharp"),
    Act::Uncrease => (Icon::Fillet, "UnCrease", "Make creased edges smooth again"),
    Act::Pull => (Icon::ProjectSurface, "Pull", "Snap the picked vertices onto another body"),
    Act::MakeUniform => (Icon::Smooth, "Make Uniform", "Even out the control points' spacing"),
    Act::Flip => (Icon::ReverseNormal, "Reverse Normal", "Turn the form inside out"),
    Act::Weld => (Icon::Stitch, "Weld Vertices", "Merge control points that touch"),
    Act::MirrorInternal => (Icon::Mirror, "Mirror - Internal", "Keep edits mirrored in a plane"),
    Act::CircularInternal => (Icon::CircPattern, "Circular - Internal", "Keep edits copied round the Z axis"),
    Act::MirrorDuplicate => (Icon::Mirror, "Mirror - Duplicate", "Add the form's mirror image, joined to it"),
    Act::CircularDuplicate => {
      (Icon::CircPattern, "Circular - Duplicate", "Copies of the form round the Z axis, joined")
    }
    Act::ClearSymmetry => (Icon::Remove, "Clear Symmetry", "Stop keeping edits symmetric"),
    Act::Display(Display::Box) => (Icon::Box, "Box Display", "Show the control cage (Alt+1)"),
    Act::Display(Display::ControlFrame) => {
      (Icon::Display, "Control Frame Display", "The smooth body with its cage (Alt+2)")
    }
    Act::Display(Display::Smooth) => (Icon::Sphere, "Smooth Display", "The smooth body alone (Alt+3)"),
  }
}

pub fn run(app: &mut App, a: Act) {
  match a {
    Act::Enter => enter(app, None),
    Act::Finish => {
      app.form = FormEdit::default();
      app.tab = crate::panels::Tab::Solid;
      app.end_hint();
    }
    Act::Primitive(f) => {
      if !app.form.active {
        enter(app, None);
      }
      app.start(crate::dialogs::Kind::Form(f));
    }
    Act::Display(d) => {
      let mode = match d {
        Display::Box => "box",
        Display::ControlFrame => "control_frame",
        Display::Smooth => "smooth",
      };
      cmd(app, "form.display", json!({ "mode": mode }), false);
    }
    Act::Flip => cmd(app, "form.flip", json!({}), false),
    Act::MakeUniform => cmd(app, "form.make_uniform", json!({}), false),
    Act::Weld => cmd(app, "form.weld", json!({}), true),
    Act::ClearSymmetry => cmd(app, "form.symmetry", json!({ "clear": true }), false),
    other => {
      app.form.tool = Some(other);
      app.form.side_a.clear();
    }
  }
}

/// Runs a form command on the edited form; `topology` clears the picks (their indices change).
fn cmd(app: &mut App, name: &str, mut p: Value, topology: bool) {
  let Some(f) = app.form.feature else {
    app.hint("Add a form primitive first");
    return;
  };
  p["feature"] = json!(f.0);
  if app.run(name, p).is_some() && topology {
    app.form.sel = Selection::default();
  }
}

fn sel_json(sel: &Selection) -> Value {
  json!({ "vertices": sel.vertices, "edges": sel.edges, "faces": sel.faces })
}

fn with_sel(sel: &Selection, extra: Value) -> Value {
  let mut v = sel_json(sel);
  if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
    o.extend(e.clone());
  }
  v
}

/// The edited form's cage and its plane's local → world matrix.
fn cage(app: &App) -> Option<(FeatureId, FormCage, DMat4)> {
  let id = app.form.feature?;
  let FeatureKind::Form { cage, .. } = &app.s.design.feature(id)?.kind else { return None };
  let pl = app.s.model.forms.get(&id)?;
  Some((id, cage.clone(), fox_engine::model::regen::plane_matrix(pl)))
}

/// Takes up a form feature made since the tab opened (by a primitive).
fn follow_new_forms(app: &mut App) {
  let all = forms_in(app);
  if let Some(new) = all.iter().rev().find(|f| !app.form.known.contains(f)) {
    app.form.feature = Some(*new);
    app.form.sel = Selection::default();
  }
  app.form.known = all.into_iter().collect();
  if app.form.feature.is_some_and(|f| app.s.design.feature(f).is_none()) {
    app.form.feature = None;
  }
}

const CAGE: Color32 = Color32::from_rgb(70, 90, 120);
const PICKED: Color32 = Color32::from_rgb(255, 140, 20);
const AXES: [Color32; 3] =
  [Color32::from_rgb(220, 70, 60), Color32::from_rgb(60, 165, 70), Color32::from_rgb(45, 100, 230)];

/// Draws the cage and handles picking and the Edit Form triad. True if it took the pointer.
pub fn overlay(
  app: &mut App,
  resp: &egui::Response,
  painter: &egui::Painter,
  ctx: &egui::Context,
  project: &dyn Fn(DVec3) -> Option<Pos2>,
  ray: &dyn Fn(Pos2) -> (DVec3, DVec3),
) -> bool {
  if !app.form.active {
    return false;
  }
  follow_new_forms(app);
  keys(app, ctx);
  let Some((id, base, to_world)) = cage(app) else { return false };
  // the cage as the drag has it so far
  let mut shown = base.clone();
  if let Some(d) = &app.form.drag {
    let _ = edit::transform(&mut shown, &app.form.sel, d.m, app.form.soft);
  }
  let world = |v: u32| to_world.transform_point3(shown.pos(v));
  let scr: Vec<Option<Pos2>> = (0..shown.vertices.len() as u32).map(|v| project(world(v))).collect();
  let eye = app.cam.eye();
  let picked_v = app.form.sel.vertex_set(&shown);
  let picked_e: BTreeSet<[u32; 2]> = app.form.sel.edges.iter().copied().collect();
  let picked_f: BTreeSet<u32> = app.form.sel.faces.iter().copied().collect();

  // faces: solid in Box display, else just the picked ones tinted
  let mut order: Vec<usize> = (0..shown.faces.len()).collect();
  let depth = |f: usize| to_world.transform_point3(shown.centroid(f)).distance(eye);
  order.sort_by(|a, b| depth(*b).total_cmp(&depth(*a)));
  for f in order {
    let pts: Option<Vec<Pos2>> = shown.faces[f].iter().map(|&v| scr[v as usize]).collect();
    let Some(pts) = pts else { continue };
    let n = to_world.transform_vector3(shown.face_normal(f));
    let toward = (eye - to_world.transform_point3(shown.centroid(f))).normalize_or_zero();
    let lit = 0.55 + 0.45 * n.dot(toward).abs() as f32;
    let fill = if picked_f.contains(&(f as u32)) {
      PICKED.gamma_multiply(0.45)
    } else if shown.display == Display::Box {
      let g = (200.0 * lit) as u8;
      Color32::from_rgb(g, g, (g as f32 * 1.05).min(255.0) as u8)
    } else {
      continue;
    };
    painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
  }
  let faint = shown.display == Display::Smooth;
  for e in shown.edges() {
    let (Some(a), Some(b)) = (scr[e[0] as usize], scr[e[1] as usize]) else { continue };
    let creased = shown.is_creased(e[0], e[1]);
    let (w, col) = if picked_e.contains(&e) {
      (3.0, PICKED)
    } else if creased {
      (2.5, Color32::from_rgb(30, 30, 30))
    } else if faint {
      (1.0, CAGE.gamma_multiply(0.45))
    } else {
      (1.3, CAGE)
    };
    painter.line_segment([a, b], Stroke::new(w, col));
  }
  for (v, p) in scr.iter().enumerate() {
    let Some(p) = p else { continue };
    let on = picked_v.contains(&(v as u32)) && app.form.sel.vertices.contains(&(v as u32));
    let r = if on {
      4.0
    } else if faint {
      2.0
    } else {
      2.5
    };
    painter.circle_filled(*p, r, if on { PICKED } else { CAGE });
  }
  if let Some(name) = app.s.design.feature(id).map(|f| f.name.clone()) {
    let at = app.canvas.center_top() + vec2(0.0, 14.0);
    let text = format!("Editing {name} — {} faces", shown.faces.len());
    painter.text(at, egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(13.0), Color32::from_gray(60));
  }

  let mut consumed = false;
  if app.form.tool == Some(Act::EditForm) && !app.form.sel.is_empty() {
    consumed |= triad(app, resp, painter, &base, to_world, project, ray);
  }
  if !consumed
    && app.dialog.is_none()
    && resp.clicked()
    && let Some(p) = resp.interact_pointer_pos()
  {
    let hit = pick(&shown, &scr, app.form.filter, p, depth);
    let add = ctx.input(|i| i.modifiers.shift || i.modifiers.command);
    let sel = &mut app.form.sel;
    if !add {
      *sel = Selection::default();
    }
    match hit {
      Some(Hit::Vertex(v)) => toggle(&mut sel.vertices, v),
      Some(Hit::Edge(e)) => toggle(&mut sel.edges, e),
      Some(Hit::Face(f)) => toggle(&mut sel.faces, f),
      None => {}
    }
    consumed = true;
  }
  consumed
}

fn toggle<T: PartialEq>(list: &mut Vec<T>, x: T) {
  match list.iter().position(|y| *y == x) {
    Some(i) => {
      list.remove(i);
    }
    None => list.push(x),
  }
}

enum Hit {
  Vertex(u32),
  Edge([u32; 2]),
  Face(u32),
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
  let ab = b - a;
  let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
  (a + ab * t).distance(p)
}

fn inside(p: Pos2, poly: &[Pos2]) -> bool {
  let mut c = false;
  let n = poly.len();
  for i in 0..n {
    let (a, b) = (poly[i], poly[(i + n - 1) % n]);
    if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
      c = !c;
    }
  }
  c
}

/// The cage element under the pointer: vertices first, then edges, then the nearest face.
fn pick(c: &FormCage, scr: &[Option<Pos2>], filter: Filter, p: Pos2, depth: impl Fn(usize) -> f64) -> Option<Hit> {
  if matches!(filter, Filter::All | Filter::Vertices) {
    let best = scr
      .iter()
      .enumerate()
      .filter_map(|(v, q)| q.map(|q| (v, q.distance(p))))
      .filter(|(_, d)| *d < 8.0)
      .min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((v, _)) = best {
      return Some(Hit::Vertex(v as u32));
    }
  }
  if matches!(filter, Filter::All | Filter::Edges) {
    let best = c
      .edges()
      .into_iter()
      .filter_map(|e| Some((e, seg_dist(p, scr[e[0] as usize]?, scr[e[1] as usize]?))))
      .filter(|(_, d)| *d < 5.0)
      .min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((e, _)) = best {
      return Some(Hit::Edge(e));
    }
  }
  if matches!(filter, Filter::All | Filter::Faces) {
    let mut hits: Vec<usize> = (0..c.faces.len())
      .filter(|&f| {
        let pts: Option<Vec<Pos2>> = c.faces[f].iter().map(|&v| scr[v as usize]).collect();
        pts.is_some_and(|pts| inside(p, &pts))
      })
      .collect();
    hits.sort_by(|a, b| depth(*a).total_cmp(&depth(*b)));
    if let Some(f) = hits.first() {
      return Some(Hit::Face(*f as u32));
    }
  }
  None
}

/// Alt+1/2/3 display modes, Delete for the picks, Escape to drop them.
fn keys(app: &mut App, ctx: &egui::Context) {
  if ctx.egui_wants_keyboard_input() {
    return;
  }
  let pressed = |k: egui::Key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, k));
  for (k, d) in
    [(egui::Key::Num1, Display::Box), (egui::Key::Num2, Display::ControlFrame), (egui::Key::Num3, Display::Smooth)]
  {
    if pressed(k) {
      run(app, Act::Display(d));
    }
  }
  if !app.form.sel.is_empty() && app.dialog.is_none() {
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
      let p = sel_json(&app.form.sel);
      cmd(app, "form.delete", p, true);
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
      app.form.sel = Selection::default();
    }
  }
}

/// The Edit Form triad at the picks' centre, along the form's plane axes.
fn triad(
  app: &mut App,
  resp: &egui::Response,
  painter: &egui::Painter,
  base: &FormCage,
  to_world: DMat4,
  project: &dyn Fn(DVec3) -> Option<Pos2>,
  ray: &dyn Fn(Pos2) -> (DVec3, DVec3),
) -> bool {
  let pivot = edit::pivot(base, &app.form.sel);
  let moved = app.form.drag.as_ref().map_or(DMat4::IDENTITY, |d| d.m);
  let o = to_world.transform_point3(moved.transform_point3(pivot));
  let Some(c) = project(o) else { return false };
  let axes: [DVec3; 3] = [0, 1, 2].map(|i| to_world.transform_vector3(DVec3::AXES[i]).normalize_or_zero());
  // a world length that is about 90 px on screen here
  let right = (axes[0] + axes[1] * 0.37).normalize_or_zero();
  let size = project(o + right).map_or(10.0, |p| 90.0 / f64::from(p.distance(c).max(1e-3)));
  let tips: Vec<Option<Pos2>> = axes.iter().map(|a| project(o + *a * size)).collect();
  let ring = |i: usize, k: usize| {
    let (u, v) = (axes[(i + 1) % 3], axes[(i + 2) % 3]);
    let t = std::f64::consts::FRAC_PI_2 * k as f64 / 12.0;
    project(o + (u * t.cos() + v * t.sin()) * size * 0.6)
  };
  let pointer = resp.hover_pos();
  let mut lit = None;
  if let Some(p) = pointer {
    if p.distance(c) < 9.0 {
      lit = Some(Grip::Scale);
    }
    for (i, tip) in tips.iter().enumerate() {
      if let Some(t) = *tip
        && lit.is_none()
        && seg_dist(p, c, t) < 6.0
      {
        lit = Some(Grip::Arrow(i));
      }
    }
    for i in 0..3 {
      for k in 0..12 {
        if let (Some(a), Some(b)) = (ring(i, k), ring(i, k + 1))
          && lit.is_none()
          && seg_dist(p, a, b) < 6.0
        {
          lit = Some(Grip::Ring(i));
        }
      }
    }
  }
  let active = app.form.drag.as_ref().map(|d| d.grip).or(lit);
  for i in 0..3 {
    let col = if active == Some(Grip::Arrow(i)) { PICKED } else { AXES[i] };
    if let Some(t) = tips[i] {
      painter.arrow(c, t - c, Stroke::new(2.5, col));
    }
    let col = if active == Some(Grip::Ring(i)) { PICKED } else { AXES[i] };
    for k in 0..12 {
      if let (Some(a), Some(b)) = (ring(i, k), ring(i, k + 1)) {
        painter.line_segment([a, b], Stroke::new(2.0, col));
      }
    }
  }
  let col = if active == Some(Grip::Scale) { PICKED } else { Color32::WHITE };
  painter.rect(
    egui::Rect::from_center_size(c, vec2(10.0, 10.0)),
    1.0,
    col,
    Stroke::new(1.0, Color32::DARK_GRAY),
    egui::StrokeKind::Inside,
  );
  if let Some(d) = &app.form.drag {
    let text = match d.grip {
      Grip::Arrow(_) => format!("{:.2} mm", d.value),
      Grip::Ring(_) => format!("{:.1}°", d.value),
      Grip::Scale => format!("× {:.3}", d.value),
    };
    painter.text(
      c + vec2(14.0, -14.0),
      egui::Align2::LEFT_BOTTOM,
      text,
      egui::FontId::proportional(13.0),
      Color32::BLACK,
    );
  }

  // dragging
  let base_o = to_world.transform_point3(pivot);
  if resp.drag_started_by(egui::PointerButton::Primary)
    && let (Some(g), Some(p)) = (lit, resp.interact_pointer_pos())
  {
    let (ro, rd) = ray(p);
    let start = match g {
      Grip::Arrow(i) => axis_param(base_o, axes[i], ro, rd).unwrap_or(0.0),
      Grip::Ring(i) => ring_angle(base_o, axes[i], axes[(i + 1) % 3], ro, rd).unwrap_or(0.0),
      Grip::Scale => 0.0,
    };
    app.form.drag =
      Some(Drag { grip: g, start, m: DMat4::IDENTITY, value: if g == Grip::Scale { 1.0 } else { 0.0 }, start_px: p });
    return true;
  }
  if let Some(mut d) = app.form.drag.clone() {
    if resp.dragged()
      && let Some(p) = resp.interact_pointer_pos()
    {
      let (ro, rd) = ray(p);
      match d.grip {
        Grip::Arrow(i) => {
          if let Some(t) = axis_param(base_o, axes[i], ro, rd) {
            d.value = t - d.start;
            d.m = DMat4::from_translation(DVec3::AXES[i] * d.value);
          }
        }
        Grip::Ring(i) => {
          if let Some(a) = ring_angle(base_o, axes[i], axes[(i + 1) % 3], ro, rd) {
            d.value = (a - d.start).to_degrees();
            d.m = edit::compose(DVec3::ZERO, DVec3::AXES[i], d.value, DVec3::ONE, pivot);
          }
        }
        Grip::Scale => {
          d.value = (1.0 + f64::from(d.start_px.y - p.y) / 150.0).max(0.05);
          d.m = edit::compose(DVec3::ZERO, DVec3::Z, 0.0, DVec3::splat(d.value), pivot);
        }
      }
      app.form.drag = Some(d);
      return true;
    }
    if resp.drag_stopped() {
      app.form.drag = None;
      let mut extra = json!({ "soft": app.form.soft, "pivot": pivot.to_array() });
      match d.grip {
        Grip::Arrow(i) => extra["move"] = json!((DVec3::AXES[i] * d.value).to_array()),
        Grip::Ring(i) => extra["rotate"] = json!({ "axis": DVec3::AXES[i].to_array(), "angle": d.value }),
        Grip::Scale => extra["scale"] = json!(d.value),
      }
      let p = with_sel(&app.form.sel, extra);
      cmd(app, "form.edit", p, false);
      return true;
    }
  }
  lit.is_some()
}

/// The FORM panel: what's picked, the pick filter, the current tool's settings and Apply.
pub fn panel(app: &mut App, ctx: &egui::Context) {
  if !app.form.active {
    return;
  }
  let canvas = app.canvas;
  let mut apply = false;
  egui::Window::new("FORM")
    .id(egui::Id::new("form-panel"))
    .default_pos(canvas.right_top() + vec2(-250.0, 40.0))
    .default_width(230.0)
    .resizable(false)
    .collapsible(true)
    .show(ctx, |ui| {
      let f = &mut app.form;
      match f.feature {
        Some(_) => {
          let s = &f.sel;
          ui.label(format!("Picked: {} vertices, {} edges, {} faces", s.vertices.len(), s.edges.len(), s.faces.len()));
        }
        None => {
          ui.label("Add a primitive from CREATE to start a form.");
        }
      }
      ui.horizontal(|ui| {
        ui.label("Pick");
        for (v, t) in
          [(Filter::All, "All"), (Filter::Vertices, "Vertex"), (Filter::Edges, "Edge"), (Filter::Faces, "Face")]
        {
          ui.selectable_value(&mut f.filter, v, t);
        }
      });
      ui.separator();
      let Some(tool) = f.tool else {
        ui.label("Pick a MODIFY or SYMMETRY tool.");
        return;
      };
      ui.strong(info(tool).1);
      match tool {
        Act::EditForm => {
          ui.label("Drag the triad: arrows move, rings rotate, the centre scales.");
          ui.add(egui::Slider::new(&mut f.soft, 0.0..=200.0).text("Soft radius (mm)"));
        }
        Act::InsertEdge => {
          ui.add(egui::Slider::new(&mut f.t, 0.05..=0.95).text("Position"));
          apply = ui.button("Insert on picked edges").clicked();
        }
        Act::Extrude => {
          ui.add(egui::DragValue::new(&mut f.distance).speed(0.5).suffix(" mm"));
          apply = ui.button("Extrude picked faces or open edges").clicked();
        }
        Act::Bridge => {
          ui.add(egui::Slider::new(&mut f.faces, 1..=16).text("Faces across"));
          if f.side_a.is_empty() {
            if ui.button("Use picked edges as side one").clicked() {
              f.side_a = f.sel.edges.clone();
              f.sel = Selection::default();
            }
          } else {
            ui.label(format!("Side one: {} edges; now pick side two", f.side_a.len()));
            apply = ui.button("Bridge").clicked();
          }
        }
        Act::FillHole => {
          ui.checkbox(&mut f.star, "Star point (else one face)");
          apply = ui.button("Fill the hole by the picked edge").clicked();
        }
        Act::Subdivide => {
          ui.checkbox(&mut f.exact, "Exact (whole form, keeps its shape)");
          apply = ui.button("Subdivide").clicked();
        }
        Act::Pull => {
          ui.checkbox(&mut f.limit, "Pull the smooth surface (else the control points)");
          apply = ui.button("Pull picked vertices onto the nearest other body").clicked();
        }
        Act::MirrorInternal | Act::MirrorDuplicate => {
          ui.horizontal(|ui| {
            for (i, t) in ["YZ", "XZ", "XY"].iter().enumerate() {
              ui.selectable_value(&mut f.plane, i, *t);
            }
          });
          apply = ui.button("Apply").clicked();
        }
        Act::CircularInternal | Act::CircularDuplicate => {
          ui.add(egui::Slider::new(&mut f.count, 2..=24).text("Copies"));
          apply = ui.button("Apply").clicked();
        }
        _ => apply = ui.button("Apply to the picks").clicked(),
      }
    });
  if apply {
    apply_tool(app);
  }
}

fn apply_tool(app: &mut App) {
  let Some(tool) = app.form.tool else { return };
  let f = &app.form;
  let sel = f.sel.clone();
  let plane = ["yz", "xz", "xy"][f.plane.min(2)];
  let (name, p, topo) = match tool {
    Act::InsertEdge => ("form.insert_edge", json!({ "edges": sel.edges, "t": f.t }), true),
    Act::InsertPoint => {
      let pts: Vec<Value> = sel.edges.iter().map(|e| json!({ "edge": e, "t": 0.5 })).collect();
      ("form.insert_point", json!({ "points": pts }), true)
    }
    Act::Subdivide => ("form.subdivide", json!({ "faces": sel.faces, "exact": f.exact }), true),
    Act::Delete => ("form.delete", sel_json(&sel), true),
    Act::Extrude => ("form.extrude", with_sel(&sel, json!({ "distance": f.distance })), true),
    Act::Bridge => ("form.bridge", json!({ "side_a": f.side_a, "side_b": sel.edges, "faces": f.faces }), true),
    Act::FillHole => {
      let Some(e) = sel.edges.first() else {
        app.hint("Fill Hole: pick an edge of the hole");
        return;
      };
      ("form.fill_hole", json!({ "edge": e, "mode": if f.star { "star" } else { "polygon" } }), true)
    }
    Act::Crease => ("form.crease", sel_json(&sel), false),
    Act::Uncrease => ("form.uncrease", sel_json(&sel), false),
    Act::Pull => ("form.pull", with_sel(&sel, json!({ "limit": f.limit })), false),
    Act::MirrorInternal => ("form.symmetry", json!({ "mirror": plane }), false),
    Act::MirrorDuplicate => ("form.mirror_duplicate", json!({ "plane": plane }), true),
    Act::CircularInternal => ("form.symmetry", json!({ "circular": { "count": f.count } }), false),
    Act::CircularDuplicate => ("form.circular_duplicate", json!({ "count": f.count }), true),
    _ => return,
  };
  if tool == Act::Bridge {
    app.form.side_a.clear();
  }
  cmd(app, name, p, topo);
}

/// The FORM tab's tooltips: a line and a fuller explanation.
pub fn tip(label: &str) -> Option<(&'static str, &'static str)> {
  Some(match label {
    "Box" => (
      "Adds a T-spline box.",
      "A box control cage with the faces per side you choose; its smooth body is rounded, touching the cage at the middle of each side. Pull its vertices, edges and faces to shape it.",
    ),
    "Plane" => (
      "Adds a flat T-spline surface.",
      "An open grid of control faces. Extrude its open edges or pull its vertices to shape a surface body; Fill Hole and Bridge can close it up.",
    ),
    "Cylinder" => (
      "Adds a T-spline tube.",
      "A tube of control faces round the plane's normal, open at the ends unless you cap them. The smooth body has the diameter you give.",
    ),
    "Sphere" => (
      "Adds a T-spline sphere.",
      "Longitude and latitude faces with three-sided faces at the poles, sized so the smooth body has the diameter you give.",
    ),
    "Torus" => (
      "Adds a T-spline ring.",
      "Faces round the ring and round the tube; every vertex is regular, so the smooth body splits into a few large faces.",
    ),
    "Quadball" => (
      "Adds a ball made of quads.",
      "A box cage squeezed onto a sphere: all four-sided faces, with a three-edged star point at each of its eight corners. It keeps edge flow cleaner than Sphere.",
    ),
    "Face" => (
      "Adds one T-spline face.",
      "A single face through four corner points; extrude its edges to grow a surface face by face.",
    ),
    "Edit Form" => (
      "Moves, rotates and scales picked cage elements.",
      "Pick vertices, edges or faces, then drag the triad: arrows move along the form's axes, rings rotate and the centre scales. Set a soft radius in the FORM panel to drag nearby vertices part of the way.",
    ),
    "Insert Edge" => (
      "Adds an edge loop.",
      "A new loop of edges across the four-sided faces through each picked edge, at the position set in the FORM panel. More edges give finer control there.",
    ),
    "Insert Point" => (
      "Adds points on edges.",
      "Splits each picked edge at its middle and joins successive points that share a face with a new edge.",
    ),
    "Subdivide" => (
      "Splits faces into smaller ones.",
      "Each picked face (all if none) becomes four-sided faces round a new centre point; neighbours gain the new edge midpoints. Exact subdivides the whole form without changing its shape.",
    ),
    "Delete" => (
      "Deletes picked cage elements.",
      "Faces leave holes; edges join the faces either side (deleting an inserted loop gives the old faces back); vertices join the faces round them. Also on the Delete key.",
    ),
    "Extrude" => (
      "Pulls faces or open edges out.",
      "Picked faces move out along their normals with a wall of new faces round them; picked open edges grow new faces straight out from the surface.",
    ),
    "Bridge" => (
      "Joins two chains of open edges.",
      "Pick the first side's open edges and keep them, then pick the second side (the same number of edges) and Bridge: a band of faces joins them.",
    ),
    "Fill Hole" => (
      "Closes a hole.",
      "Pick an open edge of the hole: a star point in the middle joined by four-sided faces, or one many-sided face.",
    ),
    "Weld Vertices" => {
      ("Merges touching control points.", "Vertices closer than 0.01 mm become one, joining faces that met there.")
    }
    "Crease" => (
      "Makes edges sharp.",
      "Picked edges (and the edges of picked faces, or round picked vertices) stay sharp in the smooth body; they're drawn thicker.",
    ),
    "UnCrease" => (
      "Makes creased edges smooth again.",
      "Removes the crease from the picked edges, faces' edges or vertices' edges.",
    ),
    "Pull" => (
      "Snaps vertices onto another body.",
      "The smooth surface over each picked vertex (or the control point itself) lands on the nearest point of the first other body.",
    ),
    "Make Uniform" => (
      "Evens out the control points.",
      "Each free vertex slides along the surface towards the middle of its neighbours; open edges, creases and corners stay where they are.",
    ),
    "Reverse Normal" => {
      ("Turns the form inside out.", "Reverses every face of the cage, flipping which side of the surface is outside.")
    }
    "Mirror - Internal" => (
      "Keeps edits mirrored.",
      "Edits to one side of the plane (YZ, XZ or XY) are copied onto the matching elements on the other. The form must already be symmetric.",
    ),
    "Circular - Internal" => (
      "Keeps edits copied round the Z axis.",
      "Edits to one sector are copied onto the matching elements of the others. The form must already have that symmetry.",
    ),
    "Mirror - Duplicate" => (
      "Mirrors the form and joins the halves.",
      "Adds the form's mirror image, welded where they meet on the plane, and keeps the halves symmetric from then on.",
    ),
    "Circular - Duplicate" => (
      "Copies the form round the Z axis.",
      "Adds copies round the axis, welded where they meet, and keeps them matching from then on.",
    ),
    "Clear Symmetry" => (
      "Stops keeping edits symmetric.",
      "Edits apply to the picked elements only from now on; the cage keeps its shape.",
    ),
    "Box Display" => (
      "Shows the control cage.",
      "The form is drawn as its flat control cage (Alt+1), the quickest view while shaping it.",
    ),
    "Control Frame Display" => {
      ("Shows the body with its cage.", "The smooth body with the control cage drawn over it (Alt+2).")
    }
    "Smooth Display" => ("Shows the smooth body.", "The smooth body with the cage drawn faintly for picking (Alt+3)."),
    "Finish Form" => (
      "Leaves the Form workspace.",
      "The form stays in the timeline as one feature whose body is the cage's smooth surface as exact B-spline faces; double-click it to edit it again.",
    ),
    _ => return None,
  })
}

/// Form primitives' dialogs (Box is [`crate::forms::FORM_BOX`]).
pub static FORM_PLANE: Form = Form {
  title: "PLANE",
  name: "Plane",
  hint: "An open T-spline surface: a flat grid of control faces",
  icon: Icon::Plane,
  cmd: "form.plane",
  fixed: &[],
  inputs: &[
    Input::Plane { key: "plane", label: "Plane" },
    Input::Expr { key: "length", label: "Length", default: "40 mm" },
    Input::Expr { key: "width", label: "Width", default: "40 mm" },
    Input::Expr { key: "length_faces", label: "Length faces", default: "2" },
    Input::Expr { key: "width_faces", label: "Width faces", default: "2" },
  ],
  picks_model: false,
};

pub static FORM_CYLINDER: Form = Form {
  title: "CYLINDER",
  name: "Cylinder",
  hint: "A T-spline tube, open at both ends unless capped",
  icon: Icon::Cylinder,
  cmd: "form.cylinder",
  fixed: &[],
  inputs: &[
    Input::Plane { key: "plane", label: "Plane" },
    Input::Expr { key: "diameter", label: "Diameter", default: "20 mm" },
    Input::Expr { key: "height", label: "Height", default: "40 mm" },
    Input::Expr { key: "around_faces", label: "Diameter faces", default: "8" },
    Input::Expr { key: "height_faces", label: "Height faces", default: "2" },
    Input::Check { key: "capped", label: "Capped ends" },
  ],
  picks_model: false,
};

pub static FORM_SPHERE: Form = Form {
  title: "SPHERE",
  name: "Sphere",
  hint: "A T-spline sphere",
  icon: Icon::Sphere,
  cmd: "form.sphere",
  fixed: &[],
  inputs: &[
    Input::Plane { key: "plane", label: "Plane" },
    Input::Expr { key: "diameter", label: "Diameter", default: "40 mm" },
    Input::Expr { key: "around_faces", label: "Longitude faces", default: "8" },
    Input::Expr { key: "height_faces", label: "Latitude faces", default: "4" },
  ],
  picks_model: false,
};

pub static FORM_TORUS: Form = Form {
  title: "TORUS",
  name: "Torus",
  hint: "A T-spline ring",
  icon: Icon::Torus,
  cmd: "form.torus",
  fixed: &[],
  inputs: &[
    Input::Plane { key: "plane", label: "Plane" },
    Input::Expr { key: "diameter", label: "Diameter", default: "60 mm" },
    Input::Expr { key: "tube_diameter", label: "Tube diameter", default: "16 mm" },
    Input::Expr { key: "around_faces", label: "Diameter faces", default: "8" },
    Input::Expr { key: "tube_faces", label: "Tube faces", default: "4" },
  ],
  picks_model: false,
};

pub static FORM_QUADBALL: Form = Form {
  title: "QUADBALL",
  name: "Quadball",
  hint: "A ball of quads: a box cage squeezed onto a sphere",
  icon: Icon::Sphere,
  cmd: "form.quadball",
  fixed: &[],
  inputs: &[
    Input::Plane { key: "plane", label: "Plane" },
    Input::Expr { key: "diameter", label: "Diameter", default: "40 mm" },
    Input::Expr { key: "faces", label: "Span faces", default: "2" },
  ],
  picks_model: false,
};

pub static FORM_FACE: Form = Form {
  title: "FACE",
  name: "Face",
  hint: "One T-spline face through four corner points",
  icon: Icon::Patch,
  cmd: "form.face",
  fixed: &[],
  inputs: &[
    Input::Plane { key: "plane", label: "Plane" },
    Input::Vec3 { key: "p0", label: "Corner 1", default: ["0", "0", "0"] },
    Input::Vec3 { key: "p1", label: "Corner 2", default: ["40", "0", "0"] },
    Input::Vec3 { key: "p2", label: "Corner 3", default: ["40", "40", "0"] },
    Input::Vec3 { key: "p3", label: "Corner 4", default: ["0", "40", "0"] },
  ],
  picks_model: false,
};

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  #[test]
  fn create_form_opens_the_tab_and_follows_a_new_primitive() {
    let mut app = App::with_session(Session::default());
    run(&mut app, Act::Enter);
    assert!(app.form.active && app.form.feature.is_none());
    assert_eq!(app.tab, crate::panels::Tab::Form);
    app.run("form.box", json!({"length": 20, "width": 20, "height": 20}));
    follow_new_forms(&mut app);
    let id = app.form.feature.expect("the new form is taken up");
    // pick a vertex, move it with Edit Form's command, and change the display
    let (_, c, m) = cage(&app).unwrap();
    assert_eq!(m, DMat4::IDENTITY);
    app.form.sel.vertices.push(c.nearest_vertex([10.0, 10.0, 20.0]).unwrap() as u32);
    let p = with_sel(&app.form.sel, json!({ "move": [0, 0, 5] }));
    cmd(&mut app, "form.edit", p, false);
    let (_, after, _) = cage(&app).unwrap();
    assert_eq!(after.pos(app.form.sel.vertices[0]).z, 25.0);
    run(&mut app, Act::Display(Display::Box));
    assert_eq!(cage(&app).unwrap().1.display, Display::Box);
    // a topology edit drops the picks
    app.form.tool = Some(Act::Subdivide);
    apply_tool(&mut app);
    assert!(app.form.sel.is_empty());
    run(&mut app, Act::Finish);
    assert!(!app.form.active);
    // editing the feature again opens the tab on it
    crate::panels::edit_feature(&mut app, id);
    assert_eq!(app.form.feature, Some(id));
  }

  #[test]
  fn picking_prefers_vertices_then_edges_then_faces() {
    let c = fox_engine::model::form::prim::plane(20.0, 20.0, [1, 1]).unwrap();
    // a top view: x, y straight to screen
    let scr: Vec<Option<Pos2>> =
      c.vertices.iter().map(|v| Some(Pos2::new(v[0] as f32 * 10.0, -v[1] as f32 * 10.0))).collect();
    let at = |x: f32, y: f32, f| pick(&c, &scr, f, Pos2::new(x, y), |_| 0.0);
    assert!(matches!(at(-100.0, 100.0, Filter::All), Some(Hit::Vertex(_))));
    assert!(matches!(at(0.0, 100.0, Filter::All), Some(Hit::Edge(_))));
    assert!(matches!(at(0.0, 0.0, Filter::All), Some(Hit::Face(0))));
    assert!(matches!(at(-90.0, 90.0, Filter::Faces), Some(Hit::Face(0))));
    assert!(at(300.0, 0.0, Filter::All).is_none());
  }
}
