//! The right-click marking menu, as in Fusion: eight commands arranged around the cursor (pick
//! one by clicking it, or by clicking anywhere in its direction) and an overflow list of
//! commands for the current selection directly underneath. The ring changes with what is
//! selected (faces, edges, bodies, sketches), and items marked ▾ / ▸ open a sub-list.
//!
//! Also here: the context-menu helpers the browser and timeline share with the menu (Find in
//! Browser / Window, Show/Hide, the hover highlight they put on the viewport).

use egui::{Color32, Pos2, Rect, Stroke, Vec2, pos2, vec2};
use fox_engine::Sel;
use fox_engine::kernel::SurfaceKind;
use fox_engine::kernel::glam::DVec3;
use fox_engine::model::{BodyId, FeatureId, FeatureKind};
use serde_json::json;

use crate::dialogs::Kind;
use crate::icons::{self, Icon};
use crate::panels::{self, Cmd};
use crate::viewport::NavMode;
use crate::{App, Tool, theme};

/// Where the item ring sits from the centre (the vertical spacing is squeezed, as in Fusion).
const RADIUS: f32 = 112.0;
/// Clicks within this distance of the centre dismiss the menu.
const DEAD_ZONE: f32 = 22.0;
/// Directional picks reach this far; clicks further out dismiss the menu.
const REACH: f32 = 260.0;
/// Rows in the overflow list and sub-lists.
const ROW_H: f32 = 24.0;
const SEP_H: f32 = 7.0;
/// Recent commands listed at the bottom of the overflow list.
const RECENT: usize = 4;

pub struct MarkingMenu {
  center: Pos2,
  /// The open sub-list: a ring slot's (opened by clicking it) or an overflow row's (by hovering).
  sub: Option<Sub>,
}

#[derive(Clone, Copy, PartialEq)]
enum Sub {
  Ring(usize),
  Row(usize),
}

/// Context-menu state kept between frames.
#[derive(Default)]
pub(crate) struct MenuState {
  /// Bodies drawn highlighted (the browser or timeline row under the pointer) and the name the
  /// status bar shows for it.
  pub(crate) peek: Vec<BodyId>,
  pub(crate) peek_label: Option<String>,
  next_peek: Vec<BodyId>,
  next_label: Option<String>,
  /// Faces and sketches drawn highlighted: what the timeline feature under the pointer made.
  pub(crate) peek_faces: Vec<(BodyId, u32)>,
  pub(crate) peek_sketches: Vec<FeatureId>,
  next_faces: Vec<(BodyId, u32)>,
  next_sketches: Vec<FeatureId>,
  /// The browser node Find in Browser scrolls to and selects.
  pub(crate) reveal: Option<Sel>,
  /// Isolate: only these bodies are drawn (a view state; nothing in the design changes).
  pub(crate) isolate: Option<Vec<BodyId>>,
  /// Bodies drawn see-through, (body, alpha) (Opacity Control; a view state).
  pub(crate) opacity: Vec<(BodyId, u8)>,
  /// Bodies the canvas can't pick (Selectable/Unselectable); the browser still selects them.
  pub(crate) unselectable: Vec<BodyId>,
}

#[derive(Clone)]
enum Action {
  Cmd(Cmd),
  Repeat(Cmd),
  EditFeature(FeatureId),
  /// Calibrate a canvas: pick two points on it, type their real distance.
  Calibrate(FeatureId),
  /// Trace a canvas's image into sketch curves where it lies.
  TraceCanvas(FeatureId),
  EditSketch(FeatureId),
  /// Create a sketch on the selected planar face and start a sketch tool in it.
  SketchOnFace(Tool),
  Nav(NavMode),
  Ok,
  Cancel,
  ShowHide,
  ShowAll,
  HideAll,
  Isolate,
  Unisolate,
  FindInBrowser(Sel),
  FindInWindow,
  /// Selectable/Unselectable: stop the canvas picking the body (or let it again).
  ToggleSelectable(BodyId),
  Workspace(&'static str),
  /// A row that only opens its sub-list.
  Open,
}

#[derive(Clone)]
struct Item {
  action: Action,
  icon: Icon,
  label: String,
  /// Shortcut key, drawn on the right of overflow rows.
  hint: Option<String>,
  children: Vec<Item>,
  /// A greyed row: listed as Fusion lists it, but it can't run here.
  enabled: bool,
}

impl Item {
  fn new(action: Action, icon: Icon, label: &str) -> Item {
    Item { action, icon, label: label.into(), hint: None, children: Vec::new(), enabled: true }
  }

  /// A row Fusion lists that FissionOxide can't run yet, greyed out.
  fn greyed(icon: Icon, label: &str) -> Item {
    Item { enabled: false, ..Item::new(Action::Open, icon, label) }
  }

  fn cmd(c: Cmd) -> Item {
    let (icon, label, _) = c.info();
    Item::new(Action::Cmd(c), icon, label)
  }

  fn with_label(c: Cmd, label: &str) -> Item {
    Item { label: label.into(), ..Item::cmd(c) }
  }
}

/// Puts the key that runs each item (and each of its children) on it, as the app binds it now:
/// the user's keys over the menus' defaults (see [`App::key_for`]).
fn with_keys(app: &App, mut it: Item) -> Item {
  if it.hint.is_none() {
    it.hint = match it.action {
      Action::Cmd(c) => app.key_for(c),
      Action::ShowHide => app.key_for(Cmd::ToggleVisibility),
      // a sketch tool started on a face: its key inside the sketch
      Action::SketchOnFace(t) => {
        let c = Cmd::SketchTool(t);
        crate::menus::commands(&[crate::panels::Tab::Sketch])
          .into_iter()
          .find(|e| e.cmd == c)
          .map(|e| crate::menus::key_of(&app.prefs, crate::tips::Scope::Sketch, e.label))
          .filter(|k| !k.is_empty())
          .map(str::to_string)
      }
      _ => None,
    };
  }
  it.children = std::mem::take(&mut it.children).into_iter().map(|c| with_keys(app, c)).collect();
  it
}

const APPEARANCE: Cmd = Cmd::Dialog(Kind::Form(&crate::forms::APPEARANCE));

/// Opens the menu at `pos`. A right-click on unselected geometry selects it first, so the
/// menu offers commands for what was clicked.
pub fn open(app: &mut App, pos: Pos2) {
  if let Some(h) = app.hover {
    let sel = h.to_sel();
    if !app.s.selection.contains(&sel) {
      app.s.selection = vec![sel];
    }
  }
  app.nav_mode = None;
  app.marking = Some(MarkingMenu { center: pos, sub: None });
}

// ---------------------------------------------------------------- what is selected

/// What the selection is, for the context-aware ring and list.
struct Context {
  face: Option<(BodyId, u32)>,
  planar_face: bool,
  edge: bool,
  /// Bodies of the selected faces, edges and bodies.
  bodies: Vec<BodyId>,
  /// Whole bodies selected.
  body_sel: bool,
  feature: Option<FeatureId>,
  sketch: Option<FeatureId>,
  profile: bool,
}

fn context(app: &App) -> Context {
  let sel = &app.s.selection;
  let face = sel.iter().find_map(|s| if let Sel::Face { body, face } = s { Some((*body, *face)) } else { None });
  let planar_face = sel.iter().any(|s| match s {
    Sel::Face { body, face } => face_kind(app, *body, *face) == Some(SurfaceKind::Plane),
    _ => false,
  });
  let feature = sel.iter().find_map(|s| if let Sel::Feature { feature } = s { Some(*feature) } else { None });
  let sketch =
    feature.filter(|f| app.s.design.feature(*f).is_some_and(|f| matches!(f.kind, FeatureKind::Sketch { .. })));
  Context {
    face,
    planar_face,
    edge: sel.iter().any(|s| matches!(s, Sel::Edge { .. })),
    bodies: selected_bodies(app),
    body_sel: sel.iter().any(|s| matches!(s, Sel::Body { .. })),
    feature,
    sketch,
    profile: sel.iter().any(|s| matches!(s, Sel::Profile { .. })),
  }
}

fn face_kind(app: &App, body: BodyId, face: u32) -> Option<SurfaceKind> {
  app.s.model.body(body)?.faces().iter().find(|f| f.id == face).map(|f| f.kind)
}

/// The bodies of everything selected (faces and edges count for their body).
pub(crate) fn selected_bodies(app: &App) -> Vec<BodyId> {
  let mut v: Vec<BodyId> = Vec::new();
  for s in &app.s.selection {
    let b = match s {
      Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } | Sel::Vertex { body, .. } => *body,
      _ => continue,
    };
    if !v.contains(&b) {
      v.push(b);
    }
  }
  v
}

/// The feature that last changed a body (FissionOxide doesn't track which feature made each
/// face, so a face's "feature" is its body's).
pub(crate) fn body_feature(app: &App, body: BodyId) -> Option<FeatureId> {
  app.s.model.body(body).map(|b| b.feature)
}

/// The bodies a feature produced or last changed.
pub(crate) fn feature_bodies(app: &App, f: FeatureId) -> Vec<BodyId> {
  app.s.model.bodies.iter().filter(|b| b.feature == f).map(|b| b.id).collect()
}

/// The sketch a feature's profiles come from.
pub(crate) fn profile_sketch(app: &App, f: FeatureId) -> Option<FeatureId> {
  let d = &app.s.design;
  d.feature(f)?
    .kind
    .depends_on()
    .into_iter()
    .find(|s| d.feature(*s).is_some_and(|s| matches!(s.kind, FeatureKind::Sketch { .. })))
}

fn feature_name(app: &App, f: FeatureId) -> String {
  app.s.design.feature(f).map(|f| f.name.clone()).unwrap_or_default()
}

/// Whether a thing has a node in the browser (bodies, sketches, construction planes, joints).
pub(crate) fn in_browser(app: &App, sel: &Sel) -> bool {
  match sel {
    Sel::Body { .. } => true,
    Sel::Feature { feature } => app.s.design.feature(*feature).is_some_and(|f| {
      matches!(f.kind, FeatureKind::Sketch { .. } | FeatureKind::ConstructionPlane { .. } | FeatureKind::Joint { .. })
    }),
    _ => false,
  }
}

// ---------------------------------------------------------------- shared actions

/// Opens the browser at a node and selects it (the browser scrolls to it when it draws).
pub(crate) fn find_in_browser(app: &mut App, sel: Sel) {
  app.browser_open = true;
  app.s.selection = vec![sel];
  app.ctx_menu.reveal = Some(sel);
}

/// Fits the view to bodies, or to a sketch when there are none.
pub(crate) fn find_in_window(app: &mut App, bodies: &[BodyId], sketch: Option<FeatureId>) {
  let mut lo = DVec3::splat(f64::INFINITY);
  let mut hi = DVec3::splat(f64::NEG_INFINITY);
  for b in bodies {
    if let Some(m) = app.s.model.body(*b).and_then(|b| b.shape.mass().ok()) {
      lo = lo.min(m.bbox_min);
      hi = hi.max(m.bbox_max);
    }
  }
  if bodies.is_empty()
    && let Some(r) = sketch.and_then(|s| app.s.model.sketches.get(&s))
  {
    for p in &r.sketch.points {
      let w = r.plane.to_world([p.x, p.y]);
      lo = lo.min(w);
      hi = hi.max(w);
    }
  }
  fit_box(app, lo, hi);
}

/// Find in Window for a plane: fits the view to the square the viewport draws it as.
pub(crate) fn find_plane_in_window(app: &mut App, plane: &fox_engine::kernel::Plane) {
  let h = crate::viewport::plane_half_size(app);
  let mut lo = DVec3::splat(f64::INFINITY);
  let mut hi = DVec3::splat(f64::NEG_INFINITY);
  for c in [[-h, -h], [h, -h], [h, h], [-h, h]] {
    let w = plane.to_world(c);
    lo = lo.min(w);
    hi = hi.max(w);
  }
  fit_box(app, lo, hi);
}

/// Fits the view to a world box.
fn fit_box(app: &mut App, lo: DVec3, hi: DVec3) {
  if !lo.is_finite() || !hi.is_finite() {
    app.status = Some(("Nothing to show in the window".into(), false));
    return;
  }
  let aspect = app.frame.as_ref().map(|f| f.width as f64 / f.height.max(1) as f64).unwrap_or(1.6);
  app.cam.fit(lo, hi, aspect);
}

/// Shows the bodies if any is hidden, else hides them.
pub(crate) fn show_hide(app: &mut App, bodies: &[BodyId]) {
  if bodies.is_empty() {
    app.hint("Show/Hide: select bodies, faces or edges first");
    return;
  }
  let show = bodies.iter().any(|b| !app.s.body_visible(*b));
  for b in bodies {
    app.run("body.visible", json!({"body": b.0, "visible": show}));
  }
  if !show {
    app.s.selection.retain(|s| !matches!(s, Sel::Body { .. } | Sel::Face { .. } | Sel::Edge { .. }));
  }
}

/// Save As Mesh: one body to an STL, OBJ or 3MF file.
pub(crate) fn save_as_mesh(app: &mut App, body: BodyId) {
  let name = app.s.body_name(body);
  let Some(p) = rfd::FileDialog::new()
    .add_filter("STL", &["stl"])
    .add_filter("OBJ", &["obj"])
    .add_filter("3MF", &["3mf"])
    .set_file_name(format!("{name}.stl"))
    .save_file()
  else {
    return;
  };
  if app.run("export", json!({"path": p, "bodies": [body.0]})).is_some() {
    app.status = Some((format!("Saved {name} as {}", p.display()), false));
  }
}

fn show_all(app: &mut App) {
  set_all_visible(app, true);
}

/// Shows or hides every body.
fn set_all_visible(app: &mut App, visible: bool) {
  let change: Vec<BodyId> =
    app.s.model.bodies.iter().map(|b| b.id).filter(|b| app.s.body_visible(*b) != visible).collect();
  for b in change {
    app.run("body.visible", json!({"body": b.0, "visible": visible}));
  }
  if !visible {
    app.s.selection.retain(|s| !matches!(s, Sel::Body { .. } | Sel::Face { .. } | Sel::Edge { .. }));
  }
}

/// Highlights bodies in the viewport, and names them in the status bar, while the pointer is
/// over a browser or timeline row (call it every frame the row is hovered).
pub(crate) fn peek(app: &mut App, bodies: Vec<BodyId>, label: String) {
  app.ctx_menu.next_peek = bodies;
  app.ctx_menu.next_label = Some(label);
}

/// Highlights what a timeline feature made, as Fusion does when its icon is hovered: a sketch's
/// curves, the bodies a feature created, or just the faces it added (a fillet's rounds, a cut's
/// walls); the bodies the feature last changed when nothing finer is known.
pub(crate) fn peek_feature(app: &mut App, f: FeatureId, label: String) {
  let sketch = app.s.design.feature(f).is_some_and(|x| matches!(x.kind, FeatureKind::Sketch { .. }));
  if sketch {
    app.ctx_menu.next_sketches = vec![f];
    app.ctx_menu.next_label = Some(label);
    return;
  }
  let made = app.s.feature_made(f);
  if made.is_empty() {
    peek(app, feature_bodies(app, f), label);
    return;
  }
  app.ctx_menu.next_peek = made.bodies;
  app.ctx_menu.next_faces =
    made.faces.into_iter().flat_map(|(b, faces)| faces.into_iter().map(move |f| (b, f))).collect();
  app.ctx_menu.next_label = Some(label);
}

/// After the panels: takes this frame's hover highlight and notes the last command run.
pub fn end_frame(app: &mut App, ctx: &egui::Context) {
  let m = &mut app.ctx_menu;
  let bodies = std::mem::take(&mut m.next_peek);
  let faces = std::mem::take(&mut m.next_faces);
  let sketches = std::mem::take(&mut m.next_sketches);
  let label = m.next_label.take();
  if m.peek != bodies || m.peek_faces != faces || m.peek_sketches != sketches || m.peek_label != label {
    m.peek = bodies;
    m.peek_faces = faces;
    m.peek_sketches = sketches;
    m.peek_label = label;
    ctx.request_repaint();
  }
}

// ---------------------------------------------------------------- contents

/// Sketch ▾: a sketch, or (on a planar face) a sketch with a drawing tool already running.
fn sketch_item(cx: &Context) -> Item {
  let mut it = Item::new(Action::Open, Icon::Sketch, "Sketch");
  it.children.push(Item::cmd(Cmd::NewSketch));
  if cx.planar_face {
    for t in [Tool::Line, Tool::Rect, Tool::Circle, Tool::Arc3, Tool::Point] {
      let (icon, label, _) = Cmd::SketchTool(t).info();
      it.children.push(Item::new(Action::SketchOnFace(t), icon, label));
    }
  }
  it
}

/// The eight ring items, clockwise from north, with their keys. `None` leaves a direction empty.
fn ring(app: &App, cx: &Context) -> [Option<Item>; 8] {
  ring_items(app, cx).map(|i| i.map(|i| with_keys(app, i)))
}

fn ring_items(app: &App, cx: &Context) -> [Option<Item>; 8] {
  let repeat = app.last_cmd.map(|c| {
    let (icon, name, _) = c.info();
    Item::new(Action::Repeat(c), icon, &format!("Repeat {name}"))
  });
  if app.s.edit_sketch.is_some() {
    return [
      repeat,
      Some(Item::cmd(Cmd::Delete)),
      Some(Item::cmd(Cmd::Redo)),
      Some(Item::with_label(Cmd::SketchTool(Tool::Circle), "Circle")),
      Some(Item::cmd(Cmd::SketchTool(Tool::Dimension))),
      Some(Item::with_label(Cmd::SketchTool(Tool::Rect), "Rectangle")),
      Some(Item::cmd(Cmd::Undo)),
      Some(Item::cmd(Cmd::SketchTool(Tool::Line))),
    ];
  }
  // Fusion's ring: Repeat, Delete, Press Pull, Redo, Hole, Sketch ▾, Move/Copy, Undo; the east
  // and south slots follow the selection
  let (east, south) = if cx.edge {
    (Item::cmd(Cmd::Dialog(Kind::Fillet)), Item::cmd(Cmd::Dialog(Kind::Chamfer)))
  } else if cx.body_sel {
    (Item::cmd(Cmd::Dialog(Kind::Combine)), Item::cmd(APPEARANCE))
  } else if cx.sketch.is_some() || cx.profile {
    (Item::cmd(Cmd::Dialog(Kind::Extrude)), Item::cmd(Cmd::Dialog(Kind::Revolve)))
  } else {
    (Item::cmd(Cmd::Dialog(Kind::PressPull)), Item::cmd(Cmd::Dialog(Kind::Hole)))
  };
  let south_west = match cx.sketch {
    Some(f) => Item::new(Action::EditSketch(f), Icon::Sketch, "Edit Sketch"),
    None => sketch_item(cx),
  };
  [
    repeat,
    Some(Item::cmd(Cmd::Delete)),
    Some(east),
    Some(Item::cmd(Cmd::Redo)),
    Some(south),
    Some(south_west),
    Some(Item::cmd(Cmd::Dialog(Kind::Move))),
    Some(Item::cmd(Cmd::Undo)),
  ]
}

/// The overflow list: OK/Cancel, commands for the selection, navigation, display, workspaces
/// and recent commands (`None` is a separator).
fn overflow(app: &App, cx: &Context) -> Vec<Option<Item>> {
  overflow_items(app, cx).into_iter().map(|i| i.map(|i| with_keys(app, i))).collect()
}

fn overflow_items(app: &App, cx: &Context) -> Vec<Option<Item>> {
  let mut v: Vec<Option<Item>> = Vec::new();
  let sep = |v: &mut Vec<Option<Item>>| {
    if v.last().is_some_and(Option::is_some) {
      v.push(None);
    }
  };
  if let Some(d) = &app.dialog {
    if d.kind != Kind::PickPlane && !d.kind.reports() {
      v.push(Some(Item::new(Action::Ok, Icon::Finish, "OK")));
    }
    v.push(Some(Item::new(Action::Cancel, Icon::Close, "Cancel")));
    sep(&mut v);
  }
  if app.s.edit_sketch.is_some() {
    v.push(Some(Item::cmd(Cmd::FinishSketch)));
    sep(&mut v);
    // Fusion's commands for picked sketch geometry
    let curve = app.s.selection.iter().any(|s| matches!(s, Sel::SketchCurve { .. }));
    let point = app.s.selection.iter().any(|s| matches!(s, Sel::SketchPoint { .. }));
    if curve {
      v.push(Some(Item::with_label(Cmd::Construction, "Normal/Construction")));
    }
    if curve || point {
      v.push(Some(Item::cmd(Cmd::Constraint("fix"))));
    }
    if curve {
      v.push(Some(Item::with_label(Cmd::SketchTool(Tool::Dimension), "Dimension")));
      v.push(Some(Item::cmd(Cmd::SketchTool(Tool::Trim))));
      sep(&mut v);
    }
    v.push(Some(Item::cmd(Cmd::LookAt)));
    v.push(Some(Item::cmd(Cmd::Fit)));
    sep(&mut v);
    v.push(Some(Item::cmd(Cmd::Dialog(Kind::Inspect))));
    return v;
  }
  // the selection's commands
  let edit_feature = |v: &mut Vec<Option<Item>>, f: FeatureId| {
    v.push(Some(Item::new(Action::EditFeature(f), Icon::Gear, &format!("Edit Feature ({})", feature_name(app, f)))));
    if let Some(sk) = profile_sketch(app, f) {
      v.push(Some(Item::new(Action::EditSketch(sk), Icon::Sketch, "Edit Profile Sketch")));
    }
  };
  let find = |v: &mut Vec<Option<Item>>, sel: Option<Sel>| {
    if let Some(s) = sel {
      v.push(Some(Item::new(Action::FindInBrowser(s), Icon::Folder, "Find in Browser")));
    }
    v.push(Some(Item::new(Action::FindInWindow, Icon::Fit, "Find in Window")));
  };
  let properties = || Item::with_label(Cmd::Dialog(Kind::Inspect), "Properties");
  let show_hide = || Item::new(Action::ShowHide, Icon::Eye, "Show/Hide");
  if let Some((body, _)) = cx.face {
    // Fusion's face list, and only it: no navigation section and no recent commands
    v.push(Some(Item::greyed(Icon::Select, "Create Selection Set")));
    if cx.planar_face {
      v.push(Some(Item::cmd(Cmd::NewSketch)));
      // Fusion's Extrude on a face extrudes the face: a press pull here, under Extrude's key
      let mut extrude = Item::new(Action::Cmd(Cmd::Dialog(Kind::PressPull)), Icon::Extrude, "Extrude");
      extrude.hint = app.key_for(Cmd::Dialog(Kind::Extrude));
      v.push(Some(extrude));
      v.push(Some(Item::cmd(Cmd::Dialog(Kind::OffsetPlane))));
    }
    v.push(Some(Item::cmd(Cmd::Dialog(Kind::Shell))));
    sep(&mut v);
    match body_feature(app, body) {
      Some(f) => {
        v.push(Some(Item::new(
          Action::EditFeature(f),
          Icon::Gear,
          &format!("Edit Feature ({})", feature_name(app, f)),
        )));
        match profile_sketch(app, f) {
          Some(sk) => v.push(Some(Item::new(Action::EditSketch(sk), Icon::Sketch, "Edit Profile Sketch"))),
          None => v.push(Some(Item::greyed(Icon::Sketch, "Edit Profile Sketch"))),
        }
      }
      None => {
        v.push(Some(Item::greyed(Icon::Gear, "Edit Feature")));
        v.push(Some(Item::greyed(Icon::Sketch, "Edit Profile Sketch")));
      }
    }
    sep(&mut v);
    v.push(Some(Item::cmd(APPEARANCE)));
    v.push(Some(Item::greyed(Icon::SurfaceTexture, "Texture Map Controls")));
    v.push(Some(properties()));
    sep(&mut v);
    v.push(Some(show_hide()));
    v.push(Some(Item::new(Action::ToggleSelectable(body), Icon::Select, "Selectable/Unselectable")));
    find(&mut v, Some(Sel::Body { body }));
    return v;
  } else if cx.edge {
    v.push(Some(Item::cmd(Cmd::Dialog(Kind::Fillet))));
    v.push(Some(Item::cmd(Cmd::Dialog(Kind::Chamfer))));
    sep(&mut v);
    if let Some(f) = cx.bodies.first().and_then(|b| body_feature(app, *b)) {
      edit_feature(&mut v, f);
      sep(&mut v);
    }
    v.push(Some(properties()));
    v.push(Some(show_hide()));
    find(&mut v, cx.bodies.first().map(|b| Sel::Body { body: *b }));
  } else if cx.body_sel {
    v.push(Some(Item::cmd(Cmd::Dialog(Kind::Move))));
    sep(&mut v);
    v.push(Some(Item::cmd(APPEARANCE)));
    if cx.bodies.iter().any(|b| app.s.design.component_of(*b).is_some()) {
      v.push(Some(Item::cmd(Cmd::Dialog(Kind::Material))));
    }
    v.push(Some(properties()));
    sep(&mut v);
    // whole bodies: Show/Hide toggles each one
    v.push(Some(Item::cmd(Cmd::ToggleVisibility)));
    v.push(Some(Item::cmd(Cmd::RemoveBodies)));
    v.push(Some(Item::new(Action::Isolate, Icon::Body, "Isolate")));
    find(&mut v, (cx.bodies.len() == 1).then(|| Sel::Body { body: cx.bodies[0] }));
  } else if let Some(f) = cx.feature {
    if cx.sketch.is_some() {
      v.push(Some(Item::cmd(Cmd::Dialog(Kind::Extrude))));
      v.push(Some(Item::cmd(Cmd::Dialog(Kind::Revolve))));
      sep(&mut v);
    }
    if app.s.design.feature(f).is_some_and(|x| matches!(x.kind, FeatureKind::Canvas { .. })) {
      // Fusion's canvas menu: Edit Canvas (opacity, size, position) and Calibrate
      v.push(Some(Item::new(Action::EditFeature(f), Icon::Canvas, "Edit Canvas")));
      v.push(Some(Item::new(Action::Calibrate(f), Icon::Measure, "Calibrate")));
      v.push(Some(Item::new(Action::TraceCanvas(f), Icon::TraceImage, "Trace Canvas")));
    } else {
      v.push(Some(Item::new(Action::EditFeature(f), Icon::Gear, &format!("Edit {}", feature_name(app, f)))));
    }
    let sel = Sel::Feature { feature: f };
    find(&mut v, in_browser(app, &sel).then_some(sel));
  } else if cx.profile {
    v.push(Some(Item::cmd(Cmd::Dialog(Kind::Extrude))));
    v.push(Some(Item::cmd(Cmd::Dialog(Kind::Revolve))));
  }
  sep(&mut v);
  // navigation
  v.push(Some(Item::new(Action::Nav(NavMode::Pan), Icon::Pan, "Pan")));
  v.push(Some(Item::new(Action::Nav(NavMode::Zoom), Icon::Zoom, "Zoom")));
  v.push(Some(Item::new(Action::Nav(NavMode::Orbit), Icon::Orbit, "Constrained Orbit")));
  v.push(Some(Item::cmd(Cmd::Fit)));
  v.push(Some(Item::cmd(Cmd::Home)));
  v.push(None);
  // display
  if !cx.body_sel && !cx.bodies.is_empty() {
    v.push(Some(Item::cmd(Cmd::RemoveBodies)));
  }
  // Fusion always lists Show All; Hide All Bodies is its Show/Hide for every body
  if !app.s.model.bodies.is_empty() {
    v.push(Some(Item::new(Action::ShowAll, Icon::Eye, "Show All")));
  }
  if app.s.model.bodies.iter().any(|b| app.s.body_visible(b.id)) {
    v.push(Some(Item::new(Action::HideAll, Icon::EyeOff, "Hide All Bodies")));
  }
  if app.ctx_menu.isolate.is_some() {
    v.push(Some(Item::new(Action::Unisolate, Icon::Body, "Unisolate")));
  }
  v.push(Some(Item::cmd(Cmd::Dialog(Kind::Inspect))));
  let mut ws = Item::new(Action::Open, Icon::Component, "Workspace");
  for (w, icon) in [
    ("Design", Icon::Component),
    ("Generative Design", Icon::Gauge),
    ("Render", Icon::Appearance),
    ("Manufacture", Icon::MotionStudy),
    ("Drawing", Icon::NewDrawing),
  ] {
    let current = match w {
      "Manufacture" => app.manufacture,
      "Generative Design" => app.generative,
      "Render" => app.render.active,
      _ => w == "Design" && !app.manufacture && !app.generative && !app.render.active,
    };
    let label = if current { format!("{w}  ✔") } else { w.to_string() };
    ws.children.push(Item::new(Action::Workspace(w), icon, &label));
  }
  v.push(Some(ws));
  // recent commands (the newest is the ring's Repeat)
  let recent: Vec<Cmd> = app.recent.iter().copied().filter(|c| Some(*c) != app.last_cmd).take(RECENT).collect();
  if !recent.is_empty() {
    v.push(None);
    for c in recent {
      v.push(Some(Item::cmd(c)));
    }
  }
  v
}

fn run(app: &mut App, action: Action) {
  match action {
    Action::Cmd(c) => c.run(app),
    // repeating a modelling command from inside a sketch finishes the sketch first
    Action::Repeat(c) => app.launch(c),
    Action::EditFeature(f) => panels::edit_feature(app, f),
    Action::Calibrate(f) => crate::canvas_ui::start_calibrate(app, f),
    Action::TraceCanvas(f) => app.trace_canvas(f),
    Action::EditSketch(f) => app.edit_sketch(f),
    Action::SketchOnFace(t) => {
      app.new_sketch();
      if app.s.edit_sketch.is_some() {
        Cmd::SketchTool(t).run(app);
      }
    }
    Action::Nav(m) => app.nav_mode = Some(m),
    Action::Ok => {
      if let Some(d) = app.dialog.as_mut() {
        d.ok_requested = true;
      }
    }
    Action::Cancel => {
      app.cancel_dialog();
      app.s.selection.clear();
    }
    Action::ShowHide => {
      let bodies = selected_bodies(app);
      show_hide(app, &bodies);
    }
    Action::ShowAll => show_all(app),
    Action::HideAll => set_all_visible(app, false),
    Action::Isolate => app.ctx_menu.isolate = Some(selected_bodies(app)),
    Action::Unisolate => app.ctx_menu.isolate = None,
    Action::FindInBrowser(s) => find_in_browser(app, s),
    Action::FindInWindow => {
      let bodies = selected_bodies(app);
      let feature =
        app.s.selection.iter().find_map(|s| if let Sel::Feature { feature } = s { Some(*feature) } else { None });
      match feature {
        Some(f) if bodies.is_empty() => {
          let bodies = feature_bodies(app, f);
          find_in_window(app, &bodies, Some(f));
        }
        _ => find_in_window(app, &bodies, None),
      }
    }
    Action::ToggleSelectable(body) => {
      let m = &mut app.ctx_menu;
      if m.unselectable.contains(&body) {
        m.unselectable.retain(|b| *b != body);
      } else {
        m.unselectable.push(body);
        app.s.selection.retain(
          |s| !matches!(s, Sel::Body { body: b } | Sel::Face { body: b, .. } | Sel::Edge { body: b, .. } if *b == body),
        );
      }
    }
    Action::Workspace(w) => panels::switch_workspace(app, w),
    Action::Open => {}
  }
}

// ---------------------------------------------------------------- layout and drawing

/// Unit direction of ring slot `i` (0 = north, clockwise).
fn dir(i: usize) -> Vec2 {
  let a = i as f32 * std::f32::consts::FRAC_PI_4;
  vec2(a.sin(), -a.cos())
}

/// The box of ring item `i`: left-aligned on the right side, right-aligned on the left side,
/// centred at north and south.
fn item_rect(center: Pos2, i: usize, size: Vec2) -> Rect {
  let d = dir(i);
  let anchor = center + vec2(d.x * RADIUS, d.y * RADIUS * 0.72);
  let min_x = if d.x > 0.1 {
    anchor.x - 14.0
  } else if d.x < -0.1 {
    anchor.x - size.x + 14.0
  } else {
    anchor.x - size.x / 2.0
  };
  Rect::from_min_size(pos2(min_x, anchor.y - size.y / 2.0), size)
}

/// The ring slot a pointer at `p` points to, if it's outside the dead zone and within reach.
fn slot_at(center: Pos2, p: Pos2) -> Option<usize> {
  let v = p - center;
  let len = v.length();
  if !(DEAD_ZONE..=REACH).contains(&len) {
    return None;
  }
  let a = v.x.atan2(-v.y).rem_euclid(std::f32::consts::TAU);
  Some(((a / std::f32::consts::FRAC_PI_4).round() as usize) % 8)
}

fn list_width(painter: &egui::Painter, font: &egui::FontId, items: &[Option<Item>]) -> f32 {
  items
    .iter()
    .flatten()
    .map(|it| {
      let w = painter.layout_no_wrap(it.label.clone(), font.clone(), theme::TEXT).size().x;
      let hint =
        it.hint.as_ref().map(|h| painter.layout_no_wrap(h.clone(), font.clone(), theme::MUTED).size().x + 18.0);
      let more = if it.children.is_empty() { 0.0 } else { 16.0 };
      w + 48.0 + hint.unwrap_or(0.0) + more
    })
    .fold(180.0f32, f32::max)
}

fn list_height(items: &[Option<Item>]) -> f32 {
  items.iter().map(|it| if it.is_some() { ROW_H } else { SEP_H }).sum::<f32>() + 6.0
}

/// The row of a list under `p`.
fn row_at(rect: Rect, items: &[Option<Item>], p: Pos2) -> Option<usize> {
  if !rect.contains(p) {
    return None;
  }
  let mut y = rect.top() + 3.0;
  for (k, it) in items.iter().enumerate() {
    let h = if it.is_some() { ROW_H } else { SEP_H };
    if it.is_some() && p.y >= y && p.y < y + h {
      return Some(k);
    }
    y += h;
  }
  None
}

fn row_top(rect: Rect, items: &[Option<Item>], k: usize) -> f32 {
  rect.top() + 3.0 + items[..k].iter().map(|it| if it.is_some() { ROW_H } else { SEP_H }).sum::<f32>()
}

/// Where the menu goes for a right-click at `center`: the ring centre and the overflow list's
/// box. Like Fusion, the ring stays centred on the click (moved only as far as its own boxes
/// need to be inside the canvas; the slot directions are what the hand remembers) and the list
/// never covers it: it sits under the ring, flips above it when it won't fit below, and opens
/// beside it when it fits neither way. Near an edge the list slides sideways; the ring doesn't.
fn layout(canvas: Rect, center: Pos2, ring: &[Option<Vec2>; 8], list: Vec2) -> (Pos2, Rect) {
  const GAP: f32 = 8.0;
  let inner = canvas.shrink(4.0);
  let bounds = |c: Pos2| {
    ring
      .iter()
      .enumerate()
      .filter_map(|(i, s)| s.map(|s| item_rect(c, i, s)))
      .fold(Rect::from_center_size(c, Vec2::splat(2.0 * DEAD_ZONE)), |a, r| a.union(r))
  };
  // the ring itself inside the canvas, where it fits
  let b = bounds(center);
  let shift = |lo: f32, hi: f32, min: f32, max: f32| {
    if hi - lo > max - min {
      0.0
    } else if lo < min {
      min - lo
    } else if hi > max {
      max - hi
    } else {
      0.0
    }
  };
  let mut center = center + vec2(shift(b.left(), b.right(), inner.left(), inner.right()), 0.0);
  center.y += shift(b.top(), b.bottom(), inner.top(), inner.bottom());
  let b = bounds(center);

  let centred = center.x - list.x / 2.0;
  // under the ring
  if b.bottom() + GAP + list.y <= inner.bottom() {
    return (center, clamp(canvas, pos2(centred, b.bottom() + GAP), list));
  }
  // above it
  if b.top() - GAP - list.y >= inner.top() {
    return (center, clamp(canvas, pos2(centred, b.top() - GAP - list.y), list));
  }
  // beside it, right then left, its top level with the ring's
  if b.right() + GAP + list.x <= inner.right() {
    return (center, clamp(canvas, pos2(b.right() + GAP, b.top()), list));
  }
  if b.left() - GAP - list.x >= inner.left() {
    return (center, clamp(canvas, pos2(b.left() - GAP - list.x, b.top()), list));
  }
  // no room anywhere: under the ring, kept on the canvas
  (center, clamp(canvas, pos2(centred, b.bottom() + GAP), list))
}

/// Keeps a list's box inside the canvas.
fn clamp(canvas: Rect, min: Pos2, size: Vec2) -> Rect {
  let x = min.x.clamp(canvas.left() + 4.0, (canvas.right() - size.x - 4.0).max(canvas.left() + 4.0));
  let y = min.y.clamp(canvas.top() + 4.0, (canvas.bottom() - size.y - 4.0).max(canvas.top() + 4.0));
  Rect::from_min_size(pos2(x, y), size)
}

fn draw_list(painter: &egui::Painter, font: &egui::FontId, rect: Rect, items: &[Option<Item>], hot: Option<usize>) {
  painter.rect(
    rect.translate(vec2(0.0, 2.0)).expand(1.0),
    2.0,
    Color32::from_black_alpha(25),
    Stroke::NONE,
    egui::StrokeKind::Outside,
  );
  painter.rect(rect, 2.0, Color32::WHITE, Stroke::new(1.0, Color32::from_gray(165)), egui::StrokeKind::Inside);
  let mut y = rect.top() + 3.0;
  for (k, it) in items.iter().enumerate() {
    match it {
      Some(it) => {
        let row = Rect::from_min_size(pos2(rect.left() + 1.0, y), vec2(rect.width() - 2.0, ROW_H));
        if hot == Some(k) && it.enabled {
          painter.rect_filled(row, 0.0, theme::HOVER);
        }
        let icon_rect = Rect::from_center_size(row.left_center() + vec2(16.0, 0.0), Vec2::splat(16.0));
        if it.enabled {
          icons::draw_color(painter, icon_rect, it.icon);
        } else {
          icons::draw(painter, icon_rect, it.icon, theme::MUTED);
        }
        painter.text(
          row.left_center() + vec2(32.0, 0.0),
          egui::Align2::LEFT_CENTER,
          &it.label,
          font.clone(),
          if it.enabled { theme::TEXT } else { theme::MUTED },
        );
        let mut right = row.right() - 10.0;
        if !it.children.is_empty() {
          let c = pos2(right - 3.0, row.center().y);
          painter.add(egui::Shape::convex_polygon(
            vec![c + vec2(-3.0, -4.0), c + vec2(2.0, 0.0), c + vec2(-3.0, 4.0)],
            theme::GROUP_LABEL,
            Stroke::NONE,
          ));
          right -= 14.0;
        }
        if let Some(h) = &it.hint {
          painter.text(pos2(right, row.center().y), egui::Align2::RIGHT_CENTER, h, font.clone(), theme::MUTED);
        }
        y += ROW_H;
      }
      None => {
        painter.line_segment(
          [pos2(rect.left() + 8.0, y + 3.5), pos2(rect.right() - 8.0, y + 3.5)],
          Stroke::new(1.0, theme::SEPARATOR),
        );
        y += SEP_H;
      }
    }
  }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
  let Some((center, mut sub)) = app.marking.as_ref().map(|m| (m.center, m.sub)) else { return };
  let cx = context(app);
  let ring = ring(app, &cx);
  let overflow = overflow(app, &cx);
  let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("marking-menu")));
  let font = egui::FontId::proportional(12.5);
  let (pointer, clicked, secondary) =
    ctx.input(|i| (i.pointer.hover_pos(), i.pointer.primary_clicked(), i.pointer.secondary_clicked()));
  let canvas = app.canvas;

  // layout: the ring, the overflow list under it (or beside it) without covering it
  let ring_sizes: [Option<Vec2>; 8] = std::array::from_fn(|i| {
    ring[i].as_ref().map(|it| {
      let tw = painter.layout_no_wrap(it.label.clone(), font.clone(), theme::TEXT).size().x;
      let more = if it.children.is_empty() { 0.0 } else { 14.0 };
      vec2(tw + 40.0 + more, 26.0)
    })
  });
  let list_size = vec2(list_width(&painter, &font, &overflow), list_height(&overflow));
  let (center, list_rect) = layout(canvas, center, &ring_sizes, list_size);
  let ring_rects: Vec<Option<Rect>> =
    ring_sizes.iter().enumerate().map(|(i, s)| s.map(|s| item_rect(center, i, s))).collect();

  // an overflow row with children opens its sub-list on hover
  if let Some(p) = pointer
    && let Some(k) = row_at(list_rect, &overflow, p)
  {
    sub = overflow[k].as_ref().filter(|it| !it.children.is_empty()).map(|_| Sub::Row(k)).or(match sub {
      Some(Sub::Ring(i)) => Some(Sub::Ring(i)),
      _ => None,
    });
  }
  let sub_items: Vec<Option<Item>> = match sub {
    Some(Sub::Ring(i)) => ring[i].as_ref().map(|it| it.children.iter().cloned().map(Some).collect()),
    Some(Sub::Row(k)) => overflow[k].as_ref().map(|it| it.children.iter().cloned().map(Some).collect()),
    None => None,
  }
  .unwrap_or_default();
  let sub_rect = (!sub_items.is_empty()).then(|| {
    let size = vec2(list_width(&painter, &font, &sub_items), list_height(&sub_items));
    let at = match sub {
      Some(Sub::Ring(i)) => ring_rects[i].map(|r| r.left_bottom() + vec2(0.0, 2.0)).unwrap_or(center),
      Some(Sub::Row(k)) => pos2(list_rect.right() - 2.0, row_top(list_rect, &overflow, k) - 3.0),
      None => center,
    };
    clamp(canvas, at, size)
  });

  // what the pointer points at: a sub-list row, an item box, a list row, then a direction
  let mut hot_slot: Option<usize> = None;
  let mut hot_row: Option<usize> = None;
  let mut hot_sub: Option<usize> = None;
  if let Some(p) = pointer {
    hot_sub = sub_rect.and_then(|r| row_at(r, &sub_items, p));
    let in_sub = sub_rect.is_some_and(|r| r.contains(p));
    if !in_sub {
      hot_slot = ring_rects.iter().position(|r| r.is_some_and(|r| r.contains(p)));
      if hot_slot.is_none() {
        hot_row = row_at(list_rect, &overflow, p);
        if hot_row.is_none() && !list_rect.contains(p) {
          hot_slot = slot_at(center, p).filter(|s| ring[*s].is_some());
        }
      }
    }
  }

  // centre: a small hub with the pointed-at direction lit
  painter.circle(center, DEAD_ZONE, Color32::from_white_alpha(170), Stroke::new(1.0, Color32::from_gray(150)));
  painter.circle_filled(center, 3.0, Color32::from_gray(110));
  for (i, it) in ring.iter().enumerate() {
    if it.is_none() {
      continue;
    }
    let d = dir(i);
    let col = if hot_slot == Some(i) { theme::ACCENT } else { Color32::from_gray(185) };
    let w = if hot_slot == Some(i) { 4.0 } else { 2.0 };
    let a = d.y.atan2(d.x);
    let arc: Vec<Pos2> = (0..=8)
      .map(|k| {
        let t = a - 0.3 + 0.6 * k as f32 / 8.0;
        center + vec2(t.cos(), t.sin()) * (DEAD_ZONE + 3.0)
      })
      .collect();
    painter.add(egui::Shape::line(arc, Stroke::new(w, col)));
  }
  if let Some(i) = hot_slot
    && let Some(r) = ring_rects[i]
  {
    let from = center + dir(i) * (DEAD_ZONE + 5.0);
    let to = pos2(center.x.clamp(r.left(), r.right()), center.y.clamp(r.top(), r.bottom()));
    painter.line_segment([from, to], Stroke::new(1.0, theme::ACCENT));
  }

  // ring items
  for (i, it) in ring.iter().enumerate() {
    let (Some(it), Some(r)) = (it, ring_rects[i]) else { continue };
    let hot = hot_slot == Some(i) || sub == Some(Sub::Ring(i));
    let (fill, stroke, text) = if hot {
      (theme::ACCENT, theme::ACCENT_DARK, Color32::WHITE)
    } else {
      (Color32::WHITE, Color32::from_gray(165), theme::TEXT)
    };
    painter.rect(
      r.translate(vec2(0.0, 1.0)),
      2.0,
      Color32::from_black_alpha(30),
      Stroke::NONE,
      egui::StrokeKind::Inside,
    );
    painter.rect(r, 2.0, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    let ir = Rect::from_center_size(r.left_center() + vec2(15.0, 0.0), Vec2::splat(17.0));
    if hot {
      painter.rect_filled(ir.expand(2.0), 2.0, Color32::WHITE);
    }
    icons::draw_color(&painter, ir, it.icon);
    painter.text(r.left_center() + vec2(30.0, 0.0), egui::Align2::LEFT_CENTER, &it.label, font.clone(), text);
    if !it.children.is_empty() {
      // ▾: the item opens a list
      let c = pos2(r.right() - 10.0, r.center().y);
      painter.add(egui::Shape::convex_polygon(
        vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)],
        text,
        Stroke::NONE,
      ));
    }
  }

  draw_list(
    &painter,
    &font,
    list_rect,
    &overflow,
    hot_row.or(match sub {
      Some(Sub::Row(k)) => Some(k),
      _ => None,
    }),
  );
  if let Some(r) = sub_rect {
    draw_list(&painter, &font, r, &sub_items, hot_sub);
  }

  if let Some(m) = app.marking.as_mut() {
    // the menu stays where the layout moved it
    m.center = center;
    m.sub = sub;
  }

  // input
  if secondary {
    // a second right-click moves the menu (handled by the viewport) or dismisses it here
    if !pointer.is_some_and(|p| canvas.contains(p)) {
      app.marking = None;
    }
    return;
  }
  if !clicked {
    return;
  }
  let picked: Option<&Item> = if !pointer.is_some_and(|p| canvas.contains(p)) {
    None
  } else if let Some(k) = hot_sub {
    if sub_items[k].as_ref().is_some_and(|it| !it.enabled) {
      return;
    }
    sub_items[k].as_ref()
  } else if pointer.is_some_and(|p| sub_rect.is_some_and(|r| r.contains(p))) {
    // a click between a sub-list's rows keeps the menu open
    return;
  } else if let Some(k) = hot_row {
    if overflow[k].as_ref().is_some_and(|it| !it.enabled) {
      // a greyed row does nothing and keeps the menu open
      return;
    }
    overflow[k].as_ref()
  } else {
    hot_slot.and_then(|i| ring[i].as_ref())
  };
  if let Some(it) = picked
    && !it.children.is_empty()
  {
    // an item with a list opens it (clicking it again closes it)
    let slot = hot_slot.filter(|_| hot_row.is_none() && hot_sub.is_none());
    if let (Some(m), Some(i)) = (app.marking.as_mut(), slot) {
      m.sub = if m.sub == Some(Sub::Ring(i)) { None } else { Some(Sub::Ring(i)) };
    }
    return;
  }
  let action = picked.map(|it| it.action.clone());
  app.marking = None;
  if let Some(a) = action {
    run(app, a);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  fn labels(items: &[Option<Item>]) -> Vec<String> {
    items.iter().flatten().map(|i| i.label.clone()).collect()
  }

  fn boxed() -> App {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    assert_eq!(app.s.model.bodies.len(), 1, "no box");
    app
  }

  fn ring_labels(app: &App) -> Vec<String> {
    let cx = context(app);
    ring(app, &cx).iter().map(|i| i.as_ref().map(|i| i.label.clone()).unwrap_or_default()).collect()
  }

  /// The ring's boxes for a layout, with every slot filled.
  fn full_ring() -> [Option<Vec2>; 8] {
    std::array::from_fn(|i| Some(vec2(if i % 4 == 0 { 110.0 } else { 140.0 }, 26.0)))
  }

  fn ring_clear(canvas: Rect, center: Pos2, sizes: &[Option<Vec2>; 8], list: Rect) {
    for (i, s) in sizes.iter().enumerate() {
      let r = item_rect(center, i, s.unwrap());
      assert!(!r.intersects(list), "list {list:?} covers ring slot {i} {r:?}");
      assert!(canvas.contains_rect(r), "ring slot {i} {r:?} is off the canvas {canvas:?}");
    }
    assert!(!Rect::from_center_size(center, Vec2::splat(2.0 * DEAD_ZONE)).intersects(list), "list covers the hub");
    assert!(canvas.contains_rect(list), "list {list:?} is off the canvas");
  }

  #[test]
  fn list_goes_under_the_ring_when_it_fits() {
    let canvas = Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 1000.0));
    let at = pos2(700.0, 300.0);
    let (c, list) = layout(canvas, at, &full_ring(), vec2(220.0, 460.0));
    assert_eq!(c, at, "the menu opens where it was clicked");
    assert!((list.center().x - at.x).abs() < 0.5 && list.top() > at.y, "{list:?}");
    ring_clear(canvas, c, &full_ring(), list);
  }

  #[test]
  fn list_flips_above_the_ring_when_it_wont_fit_below() {
    // Fusion: a right-click at y = 600 of a 790 px window keeps the ring there, the list above
    let canvas = Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 790.0));
    let at = pos2(900.0, 600.0);
    let (c, list) = layout(canvas, at, &full_ring(), vec2(220.0, 460.0));
    assert_eq!(c, at, "the ring stays on the click");
    assert!(list.bottom() < c.y, "the list opens above the ring: {list:?}");
    assert!((list.center().x - at.x).abs() < 0.5, "{list:?}");
    ring_clear(canvas, c, &full_ring(), list);
    // where it fits neither below nor above, it opens beside the ring; the ring still never moves
    for at in [pos2(650.0, 300.0), pos2(717.0, 430.0)] {
      let (c, list) = layout(canvas, at, &full_ring(), vec2(220.0, 460.0));
      assert_eq!(c, at, "the ring stays on the click");
      assert!(list.left() > c.x, "the list opens beside the ring: {list:?}");
      ring_clear(canvas, c, &full_ring(), list);
    }
  }

  #[test]
  fn list_slides_sideways_near_the_right_edge_and_the_ring_stays() {
    let canvas = Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 1000.0));
    // the ring's own boxes fit here, so the click is the centre; a wide list shifts left to fit
    let at = pos2(1150.0, 300.0);
    let (c, list) = layout(canvas, at, &full_ring(), vec2(600.0, 460.0));
    assert_eq!(c, at);
    assert!(list.center().x < at.x && list.top() > at.y, "{list:?}");
    ring_clear(canvas, c, &full_ring(), list);
    // the ring moves only as far as its own boxes need, never for the list
    let at = pos2(1390.0, 300.0);
    let (c, _) = layout(canvas, at, &full_ring(), vec2(220.0, 460.0));
    let (c_small, _) = layout(canvas, at, &full_ring(), vec2(220.0, 40.0));
    assert_eq!(c, c_small, "the list's size never moves the ring");
    assert!(c.x < at.x && c.y == at.y, "{c:?}");
  }

  #[test]
  fn list_opens_beside_the_ring_on_a_short_canvas() {
    let canvas = Rect::from_min_size(pos2(40.0, 90.0), vec2(1200.0, 520.0));
    // (clicks whose ring boxes fit the canvas, so the ring clamp has nothing to do)
    for x in [300.0, 640.0, 990.0] {
      let at = pos2(x, 350.0);
      let (c, list) = layout(canvas, at, &full_ring(), vec2(220.0, 460.0));
      assert_eq!(c, at, "the ring stays on the click");
      assert!(list.left() > c.x || list.right() < c.x, "the list opens beside the ring: {list:?}");
      ring_clear(canvas, c, &full_ring(), list);
    }
    // off the bottom: the ring is lifted only so its own boxes are inside, however long the list
    let at = pos2(640.0, 590.0);
    let (c, list) = layout(canvas, at, &full_ring(), vec2(220.0, 460.0));
    let (c_small, _) = layout(canvas, at, &full_ring(), vec2(220.0, 40.0));
    assert!(c.y < at.y && c == c_small, "{c:?} vs {c_small:?}");
    ring_clear(canvas, c, &full_ring(), list);
  }

  #[test]
  fn empty_space_ring_is_fusions() {
    let app = App::with_session(Session::default());
    let r = ring_labels(&app);
    assert_eq!(r[1..], ["Delete", "Press Pull", "Redo", "Hole", "Sketch", "Move/Copy", "Undo"]);
    let cx = context(&app);
    let list = labels(&overflow(&app, &cx));
    for want in ["Pan", "Zoom", "Constrained Orbit", "Workspace"] {
      assert!(list.iter().any(|l| l == want), "{want} missing from {list:?}");
    }
  }

  #[test]
  fn ring_and_list_follow_the_selection() {
    let mut app = boxed();
    let body = app.s.model.bodies[0].id;
    let face = app.s.model.bodies[0].faces().iter().find(|f| f.kind == SurfaceKind::Plane).unwrap().id;
    app.s.selection = vec![Sel::Face { body, face }];
    let cx = context(&app);
    let items = overflow(&app, &cx);
    let list = labels(&items);
    // Fusion's face list, and nothing after it: no navigation section, no recent commands
    assert_eq!(
      list,
      [
        "Create Selection Set",
        "Create Sketch",
        "Extrude",
        "Offset Plane",
        "Shell",
        "Edit Feature (Box1)",
        "Edit Profile Sketch",
        "Appearance",
        "Texture Map Controls",
        "Properties",
        "Show/Hide",
        "Selectable/Unselectable",
        "Find in Browser",
        "Find in Window",
      ]
    );
    let row = |l: &str| items.iter().flatten().find(|i| i.label == l).unwrap();
    assert_eq!(row("Extrude").hint.as_deref(), Some("E"), "Extrude under Extrude's key, not Press Pull's");
    assert_eq!(row("Show/Hide").hint.as_deref(), Some("V"));
    // rows FissionOxide can't run yet are listed greyed; a box has no profile sketch
    for l in ["Create Selection Set", "Edit Profile Sketch", "Texture Map Controls"] {
      assert!(!row(l).enabled, "{l} is greyed");
    }
    assert!(row("Edit Feature (Box1)").enabled && row("Selectable/Unselectable").enabled);

    let edge = app.s.model.bodies[0].edges()[0].id;
    app.s.selection = vec![Sel::Edge { body, edge }];
    let r = ring_labels(&app);
    assert_eq!((r[2].as_str(), r[4].as_str()), ("Fillet", "Chamfer"));

    app.s.selection = vec![Sel::Body { body }];
    let r = ring_labels(&app);
    assert_eq!(r[2], "Combine");
    let cx = context(&app);
    let list = labels(&overflow(&app, &cx));
    assert!(list.iter().any(|l| l == "Remove") && list.iter().any(|l| l == "Move/Copy"), "{list:?}");
  }

  #[test]
  fn timeline_hover_highlights_what_the_feature_made() {
    use fox_engine::scene::{self, SceneBuilder, ViewOptions};
    let mut app = boxed();
    let body = app.s.model.bodies[0].id;
    let box_feature = app.s.design.features[0].id;
    let edge = app.s.model.bodies[0].edges()[0].id;
    app.run("fillet", json!({"edges": [{"body": body.0, "edge": edge}], "radius": 1}));
    let fillet = app.s.design.features[1].id;
    assert_eq!(app.s.model.bodies[0].faces().len(), 7, "the box has its round");
    let ctx = egui::Context::default();
    // the box made its body
    peek_feature(&mut app, box_feature, "Box1".into());
    end_frame(&mut app, &ctx);
    assert_eq!(app.ctx_menu.peek, vec![body]);
    assert!(app.ctx_menu.peek_faces.is_empty());
    // the fillet made one face of it, not the body
    peek_feature(&mut app, fillet, "Fillet1".into());
    end_frame(&mut app, &ctx);
    assert!(app.ctx_menu.peek.is_empty(), "the fillet doesn't tint the whole body");
    assert_eq!(app.ctx_menu.peek_faces.len(), 1, "{:?}", app.ctx_menu.peek_faces);
    assert_eq!(app.ctx_menu.peek_faces[0].0, body);
    assert_eq!(panels::selection_text(&app).as_deref(), Some("Fillet1"));
    // the scene draws that face in the hover colour and the rest in the body's
    let opts = ViewOptions { peek_faces: app.ctx_menu.peek_faces.clone(), ..Default::default() };
    let (sc, _) = SceneBuilder::default().build(&app.s, &opts);
    let colors: Vec<[u8; 4]> = sc.meshes.iter().map(|m| m.color).collect();
    let hovered = scene::mix(scene::BODY_COLOR, scene::HOVER_COLOR, 0.6);
    assert!(colors.contains(&hovered) && colors.contains(&scene::BODY_COLOR), "{colors:?}");
    // a sketch highlights its curves, shown even when hidden
    app.create_sketch(json!("xy"));
    let sk = app.s.edit_sketch.unwrap();
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [5, 5]}));
    app.finish_sketch();
    app.run("sketch.visible", json!({"sketch": sk.0, "visible": false}));
    peek_feature(&mut app, sk, "Sketch1".into());
    end_frame(&mut app, &ctx);
    assert_eq!(app.ctx_menu.peek_sketches, vec![sk]);
    assert!(app.ctx_menu.peek.is_empty() && app.ctx_menu.peek_faces.is_empty());
    let opts = ViewOptions { peek_sketches: vec![sk], ..Default::default() };
    let (sc, _) = SceneBuilder::default().build(&app.s, &opts);
    assert!(sc.lines.iter().any(|l| l.color == scene::HOVER_COLOR), "the hidden sketch's curves show highlighted");
    let (sc, _) = SceneBuilder::default().build(&app.s, &ViewOptions::default());
    assert!(!sc.lines.iter().any(|l| l.color == scene::HOVER_COLOR));
    end_frame(&mut app, &ctx);
    assert!(app.ctx_menu.peek_sketches.is_empty(), "the highlight goes when the pointer leaves");
  }

  #[test]
  fn face_list_toggles_the_body_selectable() {
    let mut app = boxed();
    let body = app.s.model.bodies[0].id;
    let face = app.s.model.bodies[0].faces()[0].id;
    app.s.selection = vec![Sel::Face { body, face }];
    run(&mut app, Action::ToggleSelectable(body));
    assert_eq!(app.ctx_menu.unselectable, vec![body]);
    assert!(app.s.selection.is_empty(), "an unselectable body's face is dropped from the selection");
    run(&mut app, Action::ToggleSelectable(body));
    assert!(app.ctx_menu.unselectable.is_empty());
  }

  #[test]
  fn extruded_face_lists_edit_profile_sketch() {
    let mut app = App::with_session(Session::default());
    let sk = app.run("sketch.create", json!({"plane": "xy"})).unwrap()["sketch"].as_u64().unwrap();
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    app.run("extrude", json!({"sketch": sk, "profiles": [0], "distance": 5}));
    assert_eq!(app.s.model.bodies.len(), 1, "no extrusion");
    let body = app.s.model.bodies[0].id;
    let face = app.s.model.bodies[0].faces()[0].id;
    app.s.selection = vec![Sel::Face { body, face }];
    let cx = context(&app);
    let items = overflow(&app, &cx);
    let row = items.iter().flatten().find(|i| i.label == "Edit Profile Sketch").expect("Edit Profile Sketch");
    assert!(row.enabled && matches!(row.action, Action::EditSketch(f) if f.0 == sk as u32));
  }

  #[test]
  fn dialog_adds_ok_and_cancel() {
    let mut app = boxed();
    app.start(Kind::Fillet);
    let cx = context(&app);
    let list = labels(&overflow(&app, &cx));
    assert_eq!(list[..2], ["OK", "Cancel"]);
    run(&mut app, Action::Cancel);
    assert!(app.dialog.is_none() && app.s.selection.is_empty());
  }

  #[test]
  fn status_bar_reads_out_area_and_length() {
    let mut app = boxed();
    let body = app.s.model.bodies[0].id;
    let face = app.s.model.bodies[0].faces()[0].id;
    app.s.selection = vec![Sel::Face { body, face }];
    assert_eq!(panels::selection_text(&app).as_deref(), Some("1 Face | Area : 100.00 mm^2"));
    let edges: Vec<u32> = app.s.model.bodies[0].edges().iter().take(2).map(|e| e.id).collect();
    app.s.selection = edges.iter().map(|e| Sel::Edge { body, edge: *e }).collect();
    assert_eq!(panels::selection_text(&app).as_deref(), Some("2 Edges | Length : 20.00 mm"));
    // in the design's unit
    app.s.design.units = fox_engine::model::LengthUnit::Cm;
    assert_eq!(panels::selection_text(&app).as_deref(), Some("2 Edges | Length : 2.00 cm"));
    app.s.selection = vec![Sel::Face { body, face }];
    assert_eq!(panels::selection_text(&app).as_deref(), Some("1 Face | Area : 1.00 cm^2"));
    app.s.design.units = fox_engine::model::LengthUnit::Mm;
    app.s.selection = vec![Sel::Body { body }];
    assert_eq!(panels::selection_text(&app), Some(app.s.body_name(body)));
    // a hovered timeline or browser row names itself
    peek(&mut app, vec![body], "Box1".into());
    end_frame(&mut app, &egui::Context::default());
    assert_eq!(panels::selection_text(&app).as_deref(), Some("Box1"));
    assert_eq!(app.ctx_menu.peek, vec![body]);
    end_frame(&mut app, &egui::Context::default());
    assert!(app.ctx_menu.peek.is_empty(), "the highlight goes when the pointer leaves");
  }

  /// In a sketch the readout names the curve's kind with its length as Fusion spaces it
  /// ("1 Sketch Line | Length : 1.575 in"), and a point with its world coordinates.
  #[test]
  fn status_bar_reads_out_sketch_lines_and_points() {
    let mut app = App::with_session(Session::default());
    app.run("design.units", json!({"unit": "in"}));
    let sk = app.run("sketch.create", json!({"plane": "xy"})).unwrap()["sketch"].as_u64().unwrap();
    let sketch = fox_engine::model::FeatureId(sk as u32);
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]}));
    let circle = app.run("sketch.circle", json!({"center": [60, 10], "radius": 5})).unwrap()["curves"][0].clone();
    let circle = fox_engine::sketch::CurveId(circle.as_u64().unwrap() as u32);
    let s = app.s.design.sketch(sketch).unwrap().clone();
    let line_of = |len: f64| {
      s.curves
        .iter()
        .find(|c| {
          let p = fox_engine::sketch::curve_points(&c.kind);
          let (a, b) = (s.pos(p[0]).unwrap(), s.pos(p[1]).unwrap());
          matches!(c.kind, fox_engine::sketch::CurveKind::Line { .. })
            && ((b[0] - a[0]).hypot(b[1] - a[1]) - len).abs() < 1e-9
        })
        .unwrap()
        .id
    };
    let (long, short) = (line_of(40.0), line_of(20.0));
    app.s.selection = vec![Sel::SketchCurve { sketch, curve: long }];
    assert_eq!(panels::selection_text(&app).as_deref(), Some("1 Sketch Line | Length : 1.575 in"));
    // two curves read how close they come (ui-sketch-tangent-circles-dims), not their length
    app.s.selection = vec![Sel::SketchCurve { sketch, curve: long }, Sel::SketchCurve { sketch, curve: short }];
    assert_eq!(panels::selection_text(&app).as_deref(), Some("2 Sketch Lines | Min Distance : 0.000 in"));
    app.s.selection = vec![Sel::SketchCurve { sketch, curve: circle }];
    assert_eq!(panels::selection_text(&app).as_deref(), Some("1 Sketch Circle | Length : 1.237 in"));
    app.s.selection = vec![Sel::SketchCurve { sketch, curve: circle }, Sel::SketchCurve { sketch, curve: short }];
    assert_eq!(panels::selection_text(&app).as_deref(), Some("Multiple selections | 2"));
    // the far corner of the rectangle, in world coordinates
    let corner = s.points.iter().find(|p| (p.x - 40.0).abs() < 1e-9 && (p.y - 20.0).abs() < 1e-9).unwrap().id;
    app.s.selection = vec![Sel::SketchPoint { sketch, point: corner }];
    assert_eq!(panels::selection_text(&app).as_deref(), Some("Sketch Point | X: 1.575 Y: 0.787 Z: 0.00 in"));
    // hovering reads out the same
    app.s.selection.clear();
    app.hover = Some(fox_engine::scene::Pick::SketchCurve { sketch, curve: short });
    assert_eq!(panels::selection_text(&app).as_deref(), Some("1 Sketch Line | Length : 0.787 in"));
  }

  #[test]
  fn status_bar_profile_area_in_inches() {
    let mut app = App::with_session(Session::default());
    app.run("design.units", json!({"unit": "in"}));
    let sk = app.run("sketch.create", json!({"plane": "xy"})).unwrap()["sketch"].as_u64().unwrap();
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]}));
    let sketch = fox_engine::model::FeatureId(sk as u32);
    app.s.selection = vec![Sel::Profile { sketch, index: 0 }];
    // 800 mm² in an inch design: Fusion reads "1 Profile | Area : 1.24 in^2"
    assert_eq!(panels::selection_text(&app).as_deref(), Some("1 Profile | Area : 1.24 in^2"));
    app.run("design.units", json!({"unit": "mm"}));
    assert_eq!(panels::selection_text(&app).as_deref(), Some("1 Profile | Area : 800.00 mm^2"));
  }

  #[test]
  fn show_hide_and_show_all() {
    let mut app = boxed();
    let body = app.s.model.bodies[0].id;
    app.s.selection = vec![Sel::Body { body }];
    run(&mut app, Action::ShowHide);
    assert!(!app.s.body_visible(body));
    let cx = context(&app);
    assert!(labels(&overflow(&app, &cx)).iter().any(|l| l == "Show All"));
    run(&mut app, Action::ShowAll);
    assert!(app.s.body_visible(body));
  }

  #[test]
  fn empty_canvas_lists_show_all_hide_all_and_unisolate() {
    let mut app = boxed();
    let body = app.s.model.bodies[0].id;
    let list = |app: &App| {
      let cx = context(app);
      labels(&overflow(app, &cx))
    };
    let l = list(&app);
    assert!(l.iter().any(|l| l == "Show All") && l.iter().any(|l| l == "Hide All Bodies"), "{l:?}");
    assert!(!l.iter().any(|l| l == "Unisolate"), "{l:?}");
    run(&mut app, Action::HideAll);
    assert!(!app.s.body_visible(body));
    let l = list(&app);
    assert!(l.iter().any(|l| l == "Show All") && !l.iter().any(|l| l == "Hide All Bodies"), "{l:?}");
    run(&mut app, Action::ShowAll);
    assert!(app.s.body_visible(body));
    app.ctx_menu.isolate = Some(vec![body]);
    let l = list(&app);
    assert!(l.iter().any(|l| l == "Unisolate"), "{l:?}");
  }

  #[test]
  fn sketch_curve_lists_fix_unfix_dimension_and_trim() {
    let mut app = App::with_session(Session::default());
    app.create_sketch(json!("xy"));
    let sk = app.s.edit_sketch.expect("sketching");
    app.run("sketch.line", json!({"points": [[5, 5], [15, 5]]}));
    let curve = app.s.design.sketch(sk).unwrap().curves[0].id;
    app.s.selection = vec![Sel::SketchCurve { sketch: sk, curve }];
    let cx = context(&app);
    let list = labels(&overflow(&app, &cx));
    for want in ["Normal/Construction", "Fix/UnFix", "Dimension", "Trim"] {
      assert!(list.iter().any(|l| l == want), "{want} missing from {list:?}");
    }
    let fixes = |app: &App| {
      let s = app.s.design.sketch(sk).unwrap();
      s.constraints.iter().filter(|c| matches!(c.kind, fox_engine::sketch::ConstraintKind::Fix { .. })).count()
    };
    run(&mut app, Action::Cmd(Cmd::Constraint("fix")));
    assert_eq!(fixes(&app), 2, "both ends fixed");
    app.s.selection = vec![Sel::SketchCurve { sketch: sk, curve }];
    run(&mut app, Action::Cmd(Cmd::Constraint("fix")));
    assert_eq!(fixes(&app), 0, "a fixed line is unfixed");
  }

  #[test]
  fn recent_commands_are_listed_with_their_keys() {
    let mut app = App::with_session(Session::default());
    for c in [Cmd::Dialog(Kind::Fillet), Cmd::Dialog(Kind::Extrude)] {
      app.note_cmd(c);
    }
    let cx = context(&app);
    let list = overflow(&app, &cx);
    let fillet = list.iter().flatten().find(|i| i.label == "Fillet").expect("recent Fillet");
    assert_eq!(fillet.hint.as_deref(), Some("F"));
    // the user's key shows instead of the menus' default
    app.prefs.keys.set("design.Fillet", "G", "F");
    let list = overflow(&app, &cx);
    let fillet = list.iter().flatten().find(|i| i.label == "Fillet").expect("recent Fillet");
    assert_eq!(fillet.hint.as_deref(), Some("G"));
    // keys the app keeps for itself, and keys by workspace: A is Appearance outside a sketch
    let ring = ring(&app, &cx);
    let undo = ring.iter().flatten().find(|i| matches!(i.action, Action::Cmd(Cmd::Undo))).expect("Undo");
    assert_eq!(undo.hint.as_deref(), Some("Ctrl+Z"));
    assert_eq!(app.key_for(APPEARANCE).as_deref(), Some("A"));
    app.run("sketch.create", serde_json::json!({"plane": "xy"})).expect("a sketch");
    // 3-Point Arc has no shortcut in Fusion
    assert_eq!(app.key_for(Cmd::SketchTool(Tool::Arc3)), None);
    assert_eq!(app.key_for(Cmd::Dialog(Kind::Project)).as_deref(), Some("P"));
    assert_eq!(app.key_for(Cmd::Construction).as_deref(), Some("X"));
  }
}
