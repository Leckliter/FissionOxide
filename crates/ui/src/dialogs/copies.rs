//! Mirror and the solid patterns, laid out as Fusion's: Mirror's Object Type, Objects and Mirror
//! Plane picks (the Operation once both are in); the patterns' shared Type row, which switches
//! the one pattern dialog between Rectangular, Circular and Pattern on Path; Circular Pattern's
//! Object Type, Objects and Axis picks, with Distribution, Suppress and Quantity after them; and
//! the copies a preview adds, which the viewport draws as ghosts until OK.

use fox_engine::model::BodyId;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::icon_row::{Choice, choice, icon_row};
use super::{Dialog, Kind, is_planar};
use crate::App;
use crate::icons::Icon;
use crate::origin::OriginItem;

/// The patterns' Type row: Fusion's one pattern dialog and its three kinds.
const PATTERN_KINDS: [Choice; 3] = [
  choice("rectangular", Icon::Pattern, "Rectangular Pattern"),
  choice("circular", Icon::CircPattern, "Circular Pattern"),
  choice("path", Icon::PathPattern, "Pattern on Path"),
];

/// Mirror's Object Type (what FissionOxide can mirror: bodies, or features' cuts and joins).
const MIRROR_TYPES: [(&str, Icon, &str); 2] =
  [("bodies", Icon::Body, "Bodies"), ("features", Icon::History, "Features")];

/// Circular Pattern's Object Type (bodies are what FissionOxide patterns about an axis).
const CIRC_TYPES: [(&str, Icon, &str); 1] = [("bodies", Icon::Body, "Bodies")];

/// Mirror's Operation once both picks are in.
const MIRROR_OPS: [(&str, &str); 2] = [("new_body", "New Body"), ("join", "Join")];

/// Circular Pattern's Distribution: a whole turn, the typed total angle, or that angle shared
/// either side of the original.
const DISTRIBUTIONS: [(&str, Icon, &str); 3] = [
  ("full", Icon::DistFull, "Full"),
  ("angle", Icon::DistAngle, "Angle"),
  ("symmetric", Icon::DistSymmetric, "Symmetric"),
];

/// The dialog kind behind a Type row key, and back.
fn pattern_kind(key: &str) -> Kind {
  match key {
    "circular" => Kind::CircPattern,
    "path" => Kind::Form(&crate::forms::PATH_PATTERN),
    _ => Kind::RectPattern,
  }
}

fn pattern_key(kind: Kind) -> Option<&'static str> {
  match kind {
    Kind::RectPattern => Some("rectangular"),
    Kind::CircPattern => Some("circular"),
    Kind::Form(f) if std::ptr::eq(f, &crate::forms::PATH_PATTERN) => Some("path"),
    _ => None,
  }
}

/// The name of a mirror plane or pattern axis a pick made.
fn plane_pick(s: &Session, x: &Sel, faces: bool) -> Option<Value> {
  match x {
    Sel::Feature { feature } if s.model.planes.contains_key(feature) => Some(json!({"feature": feature.0})),
    Sel::Face { body, face } if faces && is_planar(s, *body, *face) => Some(json!({"body": body.0, "face": face})),
    _ => None,
  }
}

impl Dialog {
  /// Does the dialog make copies of bodies (Mirror, the patterns), drawn as ghosts while
  /// previewed?
  fn makes_copies(&self) -> bool {
    matches!(self.kind, Kind::Mirror | Kind::RectPattern | Kind::CircPattern) || pattern_key(self.kind) == Some("path")
  }

  /// Notes the bodies a preview added (those not in `before`) when it made copies.
  pub(super) fn note_copies(&mut self, s: &Session, before: &[BodyId], ok: bool) {
    self.ghosts = if ok && self.makes_copies() {
      s.model.bodies.iter().map(|b| b.id).filter(|b| !before.contains(b)).collect()
    } else {
      Vec::new()
    };
  }

  /// The previewed copies, drawn as Fusion's translucent grey ghosts with dark edges.
  pub(crate) fn ghost_bodies(&self) -> Vec<BodyId> {
    if self.applied && self.editing.is_none() { self.ghosts.clone() } else { Vec::new() }
  }

  // ---- the patterns' Type row ----

  /// Fusion's Type row: three icon toggles switching the pattern dialog between its kinds.
  pub(super) fn pattern_type_row(&mut self, ui: &mut egui::Ui) {
    let Some(mut cur) = pattern_key(self.kind) else { return };
    ui.label("Type");
    if icon_row(ui, &mut cur, &PATTERN_KINDS, "") {
      self.switch = Some(pattern_kind(cur));
    }
    ui.end_row();
  }

  /// Pattern on Path's Type input changed: the dialog becomes the pattern it names.
  pub(super) fn path_type_switch(&mut self, f: &'static crate::forms::Form) {
    if !std::ptr::eq(f, &crate::forms::PATH_PATTERN) {
      return;
    }
    let i = f.inputs.iter().position(|x| x.key() == "pattern_type");
    let cur = i.and_then(|i| self.form.values.get(i)).and_then(Value::as_str).unwrap_or("path");
    if cur != "path" {
      self.switch = Some(pattern_kind(cur));
    }
  }

  /// The objects picked so far, as the selection the next pattern dialog takes them from.
  fn objects_selection(&self) -> Vec<Sel> {
    let bodies: Vec<u32> = match self.kind {
      Kind::Form(f) => f
        .inputs
        .iter()
        .position(|x| x.key() == "bodies")
        .and_then(|i| self.form.values[i].as_array())
        .map(|a| a.iter().filter_map(Value::as_u64).map(|b| b as u32).collect())
        .unwrap_or_default(),
      _ => self.bodies.clone(),
    };
    bodies.into_iter().map(|b| Sel::Body { body: BodyId(b) }).collect()
  }

  /// Becomes the pattern dialog `kind`, keeping the picked objects and where the dialog sits.
  pub(super) fn switch_pattern(&mut self, app: &mut App, kind: Kind) {
    let objects = self.objects_selection();
    self.cancel(&mut app.s);
    self.leave(app);
    app.s.selection = objects;
    let docked = self.docked;
    let mut d = Dialog::open(app, kind);
    d.docked = docked;
    d.refresh_manip(&app.s);
    *self = d;
  }

  // ---- mirror ----

  /// Fusion's MIRROR: Object Type, Objects and Mirror Plane (no default); the Operation once
  /// both are picked.
  pub(super) fn mirror_rows(&mut self, app: &mut App, ui: &mut egui::Ui) {
    let mut dirty = false;
    let mut ty = self.choice;
    Self::icon_choose(ui, "Object Type", "mirror_type", &mut ty, &MIRROR_TYPES, &mut dirty);
    if ty != self.choice {
      self.choice = ty;
      self.pick_row = 0;
    }
    if self.choice == "features" {
      self.mirror_feature_rows(app, ui, &mut dirty);
    } else {
      let objects = super::chip_text(self.bodies.len(), "selected");
      self.pick_row_chip(ui, "Objects", 0, &objects, !self.bodies.is_empty());
    }
    let plane = if self.plane.is_null() { "Select" } else { "1 selected" };
    let has = !self.plane.is_null();
    self.pick_row_chip(ui, "Mirror Plane", 1, plane, has);
    if self.choice != "features" && !self.bodies.is_empty() && has {
      let mut op: &'static str = if self.join { "join" } else { "new_body" };
      Self::choose(ui, "Operation", "mirror_op", &mut op, &MIRROR_OPS, &mut dirty);
      self.join = op == "join";
    }
    self.dirty |= dirty;
  }

  /// Mirror's Objects with Object Type Features: the timeline features that add or remove a
  /// solid (their tool is what gets mirrored).
  fn mirror_feature_rows(&mut self, app: &App, ui: &mut egui::Ui, dirty: &mut bool) {
    use fox_engine::model::FeatureKind;
    let tools: Vec<(u32, String)> = app
      .s
      .design
      .features
      .iter()
      .filter(|f| {
        matches!(
          f.kind,
          FeatureKind::Extrude { .. }
            | FeatureKind::Revolve { .. }
            | FeatureKind::Primitive { .. }
            | FeatureKind::Loft { .. }
            | FeatureKind::Sweep { .. }
            | FeatureKind::Hole { .. }
        )
      })
      .map(|f| (f.id.0, f.name.clone()))
      .collect();
    ui.label("Objects");
    ui.vertical(|ui| {
      for (id, name) in tools {
        let mut on = self.mirror_features.contains(&id);
        if ui.checkbox(&mut on, name).changed() {
          self.mirror_features.retain(|f| *f != id);
          if on {
            self.mirror_features.push(id);
          }
          *dirty = true;
        }
      }
    });
    ui.end_row();
  }

  /// Mirror's picks: a construction plane picked (or a planar face while the Mirror Plane row
  /// is active) is the plane; picked bodies (and the bodies of other picked faces and edges)
  /// are the objects.
  pub(super) fn mirror_sync(&mut self, s: &Session) {
    // only a new plane pick changes the plane (one from the viewport's planes may be newer)
    let picked = s.selection.iter().rev().find_map(|x| plane_pick(s, x, self.pick_row == 1));
    if let Some(p) = picked
      && p != self.last_pick
    {
      self.last_pick = p.clone();
      if p != self.plane {
        self.plane = p;
        self.dirty = true;
      }
    }
    if self.choice == "features" {
      return;
    }
    let mut b: Vec<u32> = Vec::new();
    for x in &s.selection {
      let id = match x {
        Sel::Body { body } => body.0,
        Sel::Face { body, face } if json!({"body": body.0, "face": face}) != self.plane => body.0,
        Sel::Edge { body, .. } => body.0,
        _ => continue,
      };
      if !b.contains(&id) {
        b.push(id);
      }
    }
    if !b.is_empty() && b != self.bodies {
      self.bodies = b;
      self.dirty = true;
    }
  }

  /// Does the dialog take a plane from the viewport's origin and construction planes (drawn for
  /// it as Create Sketch draws them)? Mirror does, as Fusion shows the origin planes for it.
  pub(crate) fn wants_plane(&self) -> bool {
    self.kind == Kind::Mirror && self.editing.is_none()
  }

  /// A plane clicked in the viewport (`"xy"` or `{"feature"}`): Mirror's plane.
  pub(crate) fn take_plane(&mut self, plane: Value) {
    if self.wants_plane() && plane != self.plane {
      self.plane = plane;
      self.dirty = true;
    }
  }

  // ---- circular pattern ----

  /// Fusion's CIRCULAR PATTERN starts with nothing picked: Objects and Axis say Select, and
  /// once both are picked three instances go all the way round (Distribution Full).
  pub(super) fn circ_defaults(&mut self, s: &Session) {
    self.join = false;
    self.bodies = super::sel_only_bodies(s);
    self.axis = String::new();
    self.solid.axis_edge = Value::Null;
    self.count = "3".into();
    self.angle = "360 deg".into();
    self.choice = "full";
  }

  /// The `axis` the command takes: an origin axis, a construction axis or a picked edge.
  pub(super) fn circ_axis(&self) -> Option<Value> {
    if self.axis.is_empty() { None } else { self.revolve_axis() }
  }

  /// The `pattern.circular` command's parameters.
  pub(super) fn circ_params(&self) -> Option<Value> {
    if self.editing.is_some() {
      return Some(json!({
        "bodies": self.bodies, "axis": self.axis, "count": self.count, "angle": self.angle,
        "symmetric": self.symmetric[0], "join": self.join,
      }));
    }
    let angle =
      if self.choice == "angle" || self.choice == "symmetric" { self.angle.clone() } else { "360 deg".into() };
    Some(json!({
      "bodies": self.bodies, "axis": self.circ_axis()?, "count": self.count, "angle": angle,
      "symmetric": self.choice == "symmetric", "join": self.join,
    }))
  }

  /// Fusion's rows: Type, Object Type, Objects and Axis; Distribution, Suppress and Quantity
  /// (with the Total Angle for Angle and Symmetric) once both are picked.
  pub(super) fn circ_rows(&mut self, ui: &mut egui::Ui) {
    let mut dirty = false;
    self.pattern_type_row(ui);
    let mut ty: &'static str = "bodies";
    Self::icon_choose(ui, "Object Type", "circ_type", &mut ty, &CIRC_TYPES, &mut dirty);
    let objects = super::chip_text(self.bodies.len(), "selected");
    self.pick_row_chip(ui, "Objects", 0, &objects, !self.bodies.is_empty());
    let has = self.circ_axis().is_some();
    self.pick_row_chip(ui, "Axis", 1, if has { "1 selected" } else { "Select" }, has);
    if !self.bodies.is_empty() && has {
      if self.choice.is_empty() {
        self.choice = "full";
      }
      Self::icon_choose(ui, "Distribution", "circ-dist", &mut self.choice, &DISTRIBUTIONS, &mut dirty);
      Self::check_greyed(ui, "Suppress", "Suppressing pattern instances isn't in FissionOxide yet");
      Self::count_field(ui, "Quantity", &mut self.count, &mut dirty);
      if self.choice != "full" {
        Self::field(ui, "Total Angle", &mut self.angle, &mut dirty);
      }
    }
    self.dirty |= dirty;
  }

  /// Circular Pattern's picks: a straight or circular edge picked while the Axis row is active,
  /// or a construction axis, is the axis; picked bodies (and the bodies of other picked faces
  /// and edges) are the objects.
  pub(super) fn circ_sync(&mut self, s: &Session) {
    let row = self.pick_row;
    let picked = s.selection.iter().rev().find_map(|x| match x {
      Sel::Feature { feature } if s.model.axes.contains_key(feature) => {
        Some((format!("axis:{}", feature.0), Value::Null))
      }
      Sel::Edge { body, edge } if row == 1 => Some(("edge".to_string(), json!({"body": body.0, "edge": edge}))),
      _ => None,
    });
    if let Some((axis, edge)) = picked {
      let key = json!([axis.clone(), edge.clone()]);
      if key != self.last_pick {
        self.last_pick = key;
        self.axis = axis;
        self.solid.axis_edge = edge;
        self.dirty = true;
      }
    }
    let mut b: Vec<u32> = Vec::new();
    for x in &s.selection {
      let id = match x {
        Sel::Body { body } | Sel::Face { body, .. } => body.0,
        // (an edge picked as the axis is no object, even once an origin axis replaced it)
        Sel::Edge { body, edge } => {
          let e = json!({"body": body.0, "edge": edge});
          if e == self.solid.axis_edge || e == self.last_pick[1] {
            continue;
          }
          body.0
        }
        _ => continue,
      };
      if !b.contains(&id) {
        b.push(id);
      }
    }
    if !b.is_empty() && b != self.bodies {
      self.bodies = b;
      self.dirty = true;
    }
  }

  /// Does a click on a visible origin item go to the dialog: Mirror's plane, or Circular
  /// Pattern's axis while its Axis row is active?
  pub(crate) fn wants_origin(&self) -> bool {
    self.editing.is_none() && (self.kind == Kind::Mirror || self.kind == Kind::CircPattern && self.pick_row == 1)
  }

  /// An origin item clicked in the canvas or browser while the dialog is open; false when the
  /// dialog has no use for it.
  pub(crate) fn take_origin(&mut self, item: OriginItem) -> bool {
    if !self.wants_origin() {
      return false;
    }
    match (self.kind, item.sketch_plane(), item) {
      (Kind::Mirror, Some(p), _) => self.take_plane(p),
      (Kind::CircPattern, _, OriginItem::X | OriginItem::Y | OriginItem::Z) => {
        let a = item.label().to_lowercase();
        if a != self.axis {
          self.axis = a;
          self.solid.axis_edge = Value::Null;
          self.dirty = true;
        }
      }
      _ => return false,
    }
    true
  }
}

#[cfg(test)]
mod tests {
  use egui::{Pos2, Rect, vec2};
  use fox_engine::model::BodyId;
  use fox_engine::{Sel, Session};
  use serde_json::{Value, json};

  use super::*;

  /// Draws the dialog for two frames (it previews when ready, as on screen; a new window
  /// draws nothing on its first); the text drawn on the second.
  fn frame(ctx: &egui::Context, app: &mut App, d: &mut Dialog) -> Vec<String> {
    frame1(ctx, app, d);
    frame1(ctx, app, d)
  }

  fn frame1(ctx: &egui::Context, app: &mut App, d: &mut Dialog) -> Vec<String> {
    fn walk(s: &egui::Shape, out: &mut Vec<String>) {
      match s {
        egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
        egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        _ => {}
      }
    }
    let raw =
      egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))), ..Default::default() };
    let out = ctx.run_ui(raw, |ui| {
      d.show(app, ui.ctx());
    });
    let mut texts = Vec::new();
    for c in &out.shapes {
      walk(&c.shape, &mut texts);
    }
    out.drop_without_applying_deltas();
    texts
  }

  /// A 10 mm cube at (20, 0) and a cylinder at (-30, 0) standing on XY.
  fn parts(s: &mut Session) -> (BodyId, BodyId) {
    s.execute("box", &json!({"width": 10, "depth": 10, "height": 10, "at": [20, 0]})).unwrap();
    s.execute("cylinder", &json!({"radius": 5, "height": 20, "at": [-30, 0]})).unwrap();
    (s.model.bodies[0].id, s.model.bodies[1].id)
  }

  fn row(texts: &[String], label: &str) -> bool {
    texts.iter().any(|t| t == label)
  }

  /// Fusion's MIRROR: Object Type, Objects and Mirror Plane with no default, OK greyed and
  /// "Select bodies to mirror"; the Operation once both are picked, the copy previewed as a
  /// ghost and the prompt naming Ctrl.
  #[test]
  fn mirror_waits_for_objects_and_a_plane() {
    let mut app = App::with_session(Session::default());
    let (cube, _) = parts(&mut app.s);
    let ctx = egui::Context::default();
    let mut d = Dialog::open(&mut app, Kind::Mirror);
    assert!(d.plane.is_null() && d.bodies.is_empty(), "nothing preset");
    let t = frame(&ctx, &mut app, &mut d);
    for label in ["Object Type", "Bodies", "Objects", "Mirror Plane"] {
      assert!(row(&t, label), "{label} in {t:?}");
    }
    assert!(!row(&t, "Plane") && !row(&t, "Join to original") && !row(&t, "Operation"), "{t:?}");
    assert!(!d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Select bodies to mirror"));
    assert!(d.wants_plane(), "the origin planes show for it");
    // the cube picked: the plane is still to pick
    app.s.selection = vec![Sel::Body { body: cube }];
    frame(&ctx, &mut app, &mut d);
    assert_eq!(d.bodies, vec![cube.0]);
    assert!(!d.applied && !d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Select mirror plane"));
    // the YZ plane clicked in the canvas
    d.take_plane(json!("yz"));
    let t = frame(&ctx, &mut app, &mut d);
    frame(&ctx, &mut app, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?} {:?}", d.error, d.command());
    let t2 = frame(&ctx, &mut app, &mut d);
    assert!(row(&t2, "Operation") && row(&t2, "New Body"), "{t:?} {t2:?}");
    assert_eq!(app.s.model.bodies.len(), 3, "a new body");
    let ghosts = d.ghost_bodies();
    assert_eq!(ghosts.len(), 1);
    assert!(ghosts[0] != cube, "the copy is the ghost, not the original");
    assert_eq!(app.s.selection, vec![Sel::Body { body: cube }], "the original stays picked (blue)");
    assert_eq!(d.prompt(false).as_deref(), Some("Hold Ctrl to modify object selection or mirror plane"));
    let x = app.s.model.body(ghosts[0]).unwrap().shape.mass().unwrap().center_of_mass.x;
    assert!((x + 20.0).abs() < 1e-6, "mirrored across YZ: {x}");
    // a construction plane picked while the Mirror Plane row is active replaces it
    d.pick_row = 1;
    let top = app.s.model.body(cube).unwrap().faces().iter().find(|f| f.normal.x > 0.9).unwrap().id;
    app.s.selection.push(Sel::Face { body: cube, face: top });
    frame(&ctx, &mut app, &mut d);
    assert_eq!(d.plane, json!({"body": cube.0, "face": top}));
    assert_eq!(d.bodies, vec![cube.0], "the plane's face doesn't add an object");
    d.cancel(&mut app.s);
    assert_eq!(app.s.model.bodies.len(), 2);
  }

  /// Rectangular, Circular and Pattern on Path share Fusion's Type row: picking another type
  /// turns the dialog into that pattern, keeping the picked objects.
  #[test]
  fn the_pattern_type_row_switches_the_dialog() {
    let mut app = App::with_session(Session::default());
    let (cube, _) = parts(&mut app.s);
    let ctx = egui::Context::default();
    app.s.selection = vec![Sel::Body { body: cube }];
    let mut d = Dialog::open(&mut app, Kind::RectPattern);
    let t = frame(&ctx, &mut app, &mut d);
    assert!(row(&t, "Type"), "{t:?}");
    d.switch = Some(pattern_kind("circular"));
    frame(&ctx, &mut app, &mut d);
    assert_eq!(d.kind, Kind::CircPattern);
    assert_eq!(d.bodies, vec![cube.0], "the objects carry over");
    let t = frame(&ctx, &mut app, &mut d);
    assert!(row(&t, "Type") && row(&t, "Axis"), "{t:?}");
    d.switch = Some(pattern_kind("path"));
    frame(&ctx, &mut app, &mut d);
    let f = &crate::forms::PATH_PATTERN;
    assert_eq!(d.kind, Kind::Form(f));
    let at = |k: &str| f.inputs.iter().position(|x| x.key() == k).unwrap();
    assert_eq!(d.form.values[at("pattern_type")], json!("path"));
    assert_eq!(d.form.values[at("bodies")], json!([cube.0]));
    // the path form's Type row back to rectangular
    d.form.values[at("pattern_type")] = json!("rectangular");
    d.path_type_switch(f);
    assert_eq!(d.switch, Some(Kind::RectPattern));
  }

  /// Fusion's CIRCULAR PATTERN starts empty (no "all bodies", no preset axis), OK greyed and
  /// its prompt asking for bodies; Distribution, Suppress and Quantity come after both picks,
  /// and the copies preview as ghosts.
  #[test]
  fn circular_pattern_waits_for_objects_and_an_axis() {
    let mut app = App::with_session(Session::default());
    let (cube, cyl) = parts(&mut app.s);
    let ctx = egui::Context::default();
    let mut d = Dialog::open(&mut app, Kind::CircPattern);
    assert!(d.bodies.is_empty() && d.circ_axis().is_none());
    let t = frame(&ctx, &mut app, &mut d);
    for label in ["Type", "Object Type", "Objects", "Axis", "Select"] {
      assert!(row(&t, label), "{label} in {t:?}");
    }
    assert!(!row(&t, "all bodies") && !row(&t, "Quantity") && !row(&t, "Join instances"), "{t:?}");
    assert!(!d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Select solid or surface bodies in the canvas or browser to pattern"));
    app.s.selection = vec![Sel::Body { body: cube }];
    frame(&ctx, &mut app, &mut d);
    assert_eq!(d.bodies, vec![cube.0]);
    assert_eq!(d.prompt(false).as_deref(), Some("Select axis"));
    // the Axis row active: a circular edge of the cylinder is the axis, its body no object
    d.pick_row = 1;
    assert!(d.wants_origin());
    let rim =
      app.s.model.body(cyl).unwrap().edges().iter().find(|e| e.kind != fox_engine::kernel::CurveKind::Line).unwrap().id;
    app.s.selection.push(Sel::Edge { body: cyl, edge: rim });
    frame(&ctx, &mut app, &mut d);
    assert_eq!(d.bodies, vec![cube.0]);
    assert_eq!(d.circ_axis(), Some(json!({"body": cyl.0, "edge": rim})));
    // or the origin Z axis picked in the browser
    assert!(d.take_origin(OriginItem::Z));
    assert_eq!(d.circ_axis(), Some(json!("z")));
    let t = frame(&ctx, &mut app, &mut d);
    frame(&ctx, &mut app, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?} {:?}", d.error, d.command());
    let t2 = frame(&ctx, &mut app, &mut d);
    for label in ["Distribution", "Full", "Suppress", "Quantity"] {
      assert!(row(&t2, label), "{label} in {t:?} {t2:?}");
    }
    assert!(!row(&t2, "Total Angle"));
    assert_eq!(app.s.model.bodies.len(), 4, "three instances: two copies");
    assert_eq!(d.ghost_bodies().len(), 2);
    assert_eq!(d.command().unwrap().1["angle"], json!("360 deg"));
    d.choice = "symmetric";
    d.angle = "90 deg".into();
    let p = d.command().unwrap().1;
    assert_eq!((p["angle"].clone(), p["symmetric"].clone()), (json!("90 deg"), Value::Bool(true)));
  }

  /// Pattern on Path: Type, Object Type, Objects and Path first; Distribution, Quantity,
  /// Distance and Orientation once both are picked, and OK waits for a path.
  #[test]
  fn pattern_on_path_rows_follow_the_picks() {
    use crate::forms::State;
    let f = &crate::forms::PATH_PATTERN;
    let mut st = State::new(f);
    let keys = |st: &State| -> Vec<&str> {
      (0..f.inputs.len()).filter(|&i| st.shown(f, i) && !State::hidden(f, i)).map(|i| f.inputs[i].key()).collect()
    };
    assert_eq!(keys(&st), ["pattern_type", "object_type", "bodies", "path_curves"]);
    let at = |k: &str| f.inputs.iter().position(|x| x.key() == k).unwrap();
    assert_eq!(st.missing(f), Some("Objects"));
    st.values[at("bodies")] = json!([1]);
    assert_eq!(st.missing(f), Some("Path"));
    st.values[at("path_curves")] = json!([2]);
    st.values[at("path")] = json!(3);
    assert_eq!(st.missing(f), None);
    assert_eq!(
      keys(&st),
      [
        "pattern_type",
        "object_type",
        "bodies",
        "path_curves",
        "distance_type",
        "count",
        "distance",
        "orientation",
        "flip",
        "symmetric"
      ]
    );
    let p = st.params(f);
    assert_eq!((p["path"].clone(), p["distance_type"].clone()), (json!(3), json!("extent")));
  }

  /// A preview of copies draws them as ghosts: see-through grey with dark edges.
  #[test]
  fn ghosts_draw_see_through_grey() {
    use fox_engine::scene::{GHOST_COLOR, GHOST_EDGE_COLOR, SceneBuilder, ViewOptions};
    let mut s = Session::default();
    let (cube, cyl) = parts(&mut s);
    let opts = ViewOptions { ghosts: vec![cyl], ..Default::default() };
    let (scene, _) = SceneBuilder::default().build(&s, &opts);
    assert!(scene.meshes.iter().any(|m| m.color == GHOST_COLOR));
    assert!(scene.lines.iter().any(|l| l.color == GHOST_EDGE_COLOR));
    let plain = SceneBuilder::default().build(&s, &ViewOptions::default()).0;
    assert!(!plain.meshes.iter().any(|m| m.color == GHOST_COLOR));
    let _ = cube;
  }
}
