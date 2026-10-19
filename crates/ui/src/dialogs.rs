//! Feature dialogs (the floating panels on the right), with live preview.
//!
//! A preview is a real command applied to the session; changing a value withdraws it (undo) and
//! applies the new one, so OK just keeps the last preview and Cancel withdraws it — exactly one
//! undo step per feature, as in Fusion.

use clf_engine::model::{AxisRef, BodyId, ExtentKind, FeatureId, FeatureKind, Operation, PlaneRef, Primitive};
use clf_engine::{Sel, Session};
use egui::RichText;
use serde_json::{Value, json};

use crate::icons::Icon;
use crate::theme;
use crate::{App, panels};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  PickPlane,
  Extrude,
  Revolve,
  Fillet,
  Chamfer,
  Shell,
  Box,
  Cylinder,
  Sphere,
  Torus,
  RectPattern,
  CircPattern,
  Mirror,
  Combine,
  Move,
  OffsetPlane,
  Inspect,
}

impl Kind {
  fn title(self) -> &'static str {
    match self {
      Kind::PickPlane => "CREATE SKETCH",
      Kind::Extrude => "EXTRUDE",
      Kind::Revolve => "REVOLVE",
      Kind::Fillet => "FILLET",
      Kind::Chamfer => "CHAMFER",
      Kind::Shell => "SHELL",
      Kind::Box => "BOX",
      Kind::Cylinder => "CYLINDER",
      Kind::Sphere => "SPHERE",
      Kind::Torus => "TORUS",
      Kind::RectPattern => "RECTANGULAR PATTERN",
      Kind::CircPattern => "CIRCULAR PATTERN",
      Kind::Mirror => "MIRROR",
      Kind::Combine => "COMBINE",
      Kind::Move => "MOVE / ROTATE",
      Kind::OffsetPlane => "OFFSET PLANE",
      Kind::Inspect => "PROPERTIES",
    }
  }

  fn icon(self) -> Icon {
    match self {
      Kind::PickPlane => Icon::Sketch,
      Kind::Extrude => Icon::Extrude,
      Kind::Revolve => Icon::Revolve,
      Kind::Fillet => Icon::Fillet,
      Kind::Chamfer => Icon::Chamfer,
      Kind::Shell => Icon::Shell,
      Kind::Box => Icon::Box,
      Kind::Cylinder => Icon::Cylinder,
      Kind::Sphere => Icon::Sphere,
      Kind::Torus => Icon::Torus,
      Kind::RectPattern => Icon::Pattern,
      Kind::CircPattern => Icon::CircPattern,
      Kind::Mirror => Icon::Mirror,
      Kind::Combine => Icon::Combine,
      Kind::Move => Icon::Move,
      Kind::OffsetPlane => Icon::Plane,
      Kind::Inspect => Icon::Measure,
    }
  }

  /// Dialogs whose input is picked edges/faces/bodies of the model: they preview on demand,
  /// because a preview changes the very geometry being picked.
  fn picks_model(self) -> bool {
    matches!(self, Kind::Fillet | Kind::Chamfer | Kind::Shell | Kind::Combine)
  }
}

const OPS: [(&str, &str); 4] = [("new_body", "New body"), ("join", "Join"), ("cut", "Cut"), ("intersect", "Intersect")];
const EXTENTS: [(&str, &str); 4] =
  [("one_side", "One side"), ("symmetric", "Symmetric"), ("two_sides", "Two sides"), ("through_all", "All")];

pub struct Dialog {
  pub kind: Kind,
  /// Feature being edited (timeline double-click); None while creating.
  pub editing: Option<FeatureId>,
  /// A preview step is applied to the session.
  applied: bool,
  error: Option<String>,
  dirty: bool,
  /// Set by a field that finished the dialog itself (e.g. picking a sketch plane).
  closed: bool,
  // captured inputs
  pub sketch: Option<FeatureId>,
  profiles: Vec<[f64; 2]>,
  edges: Vec<Value>,
  faces: Vec<Value>,
  bodies: Vec<u32>,
  plane: Value,
  plane_label: String,
  // fields
  distance: String,
  distance2: String,
  extent: &'static str,
  flip: bool,
  op: &'static str,
  angle: String,
  axis: String,
  radius: String,
  count: String,
  spacing: String,
  dir: String,
  use_dir2: bool,
  count2: String,
  spacing2: String,
  dir2: String,
  join: bool,
  a: String,
  b: String,
  c: String,
  at: [String; 2],
  translate: [String; 3],
  keep_tools: bool,
}

impl Dialog {
  fn blank(kind: Kind) -> Dialog {
    Dialog {
      kind,
      editing: None,
      applied: false,
      error: None,
      dirty: false,
      closed: false,
      sketch: None,
      profiles: Vec::new(),
      edges: Vec::new(),
      faces: Vec::new(),
      bodies: Vec::new(),
      plane: json!("xy"),
      plane_label: "XY plane".into(),
      distance: "10 mm".into(),
      distance2: "10 mm".into(),
      extent: "one_side",
      flip: false,
      op: "new_body",
      angle: "360 deg".into(),
      axis: "z".into(),
      radius: "2 mm".into(),
      count: "3".into(),
      spacing: "20 mm".into(),
      dir: "x".into(),
      use_dir2: false,
      count2: "2".into(),
      spacing2: "20 mm".into(),
      dir2: "y".into(),
      join: true,
      a: "20 mm".into(),
      b: "20 mm".into(),
      c: "20 mm".into(),
      at: ["0".into(), "0".into()],
      translate: ["0".into(), "0".into(), "0".into()],
      keep_tools: false,
    }
  }

  /// Opens a dialog for a new feature, capturing the current selection.
  pub fn open(app: &mut App, kind: Kind) -> Dialog {
    let mut d = Dialog::blank(kind);
    let s = &app.s;
    // profiles → sketch
    for sel in &s.selection {
      if let Sel::Profile { sketch, index } = sel
        && let Some(r) = s.model.sketches.get(sketch)
        && let Some(p) = r.profiles.get(*index)
      {
        d.sketch = Some(*sketch);
        d.profiles.push(p.interior_point());
      }
    }
    if matches!(kind, Kind::Extrude | Kind::Revolve) && d.sketch.is_none() {
      d.sketch = s.edit_sketch.or(app.last_sketch).or_else(|| last_unconsumed_sketch(s));
      if let Some(sk) = d.sketch
        && let Some(r) = s.model.sketches.get(&sk)
        && r.profiles.len() == 1
      {
        d.profiles = vec![r.profiles[0].interior_point()];
      }
    }
    for sel in &s.selection {
      match sel {
        Sel::Edge { body, edge } => d.edges.push(json!({"body": body.0, "edge": edge})),
        Sel::Face { body, face } => {
          d.faces.push(json!({"body": body.0, "face": face}));
          if !d.bodies.contains(&body.0) {
            d.bodies.push(body.0);
          }
        }
        Sel::Body { body } if !d.bodies.contains(&body.0) => {
          d.bodies.push(body.0);
        }
        _ => {}
      }
    }
    // a selected planar face becomes the plane for primitives / offset planes / mirror
    if let Some(Sel::Face { body, face }) = s.selection.iter().find(|x| matches!(x, Sel::Face { .. }))
      && is_planar(s, *body, *face)
    {
      d.plane = json!({"body": body.0, "face": face});
      d.plane_label = format!("Face of {}", s.body_name(*body));
    }
    match kind {
      Kind::Fillet => d.radius = "2 mm".into(),
      Kind::Chamfer => d.radius = "1 mm".into(),
      Kind::Shell => d.radius = "2 mm".into(),
      Kind::Cylinder => {
        d.a = "10 mm".into();
        d.b = "20 mm".into();
      }
      Kind::Sphere => d.a = "10 mm".into(),
      Kind::Torus => {
        d.a = "20 mm".into();
        d.b = "5 mm".into();
      }
      Kind::Revolve => d.axis = "z".into(),
      Kind::Mirror => d.join = false,
      Kind::RectPattern | Kind::CircPattern => d.join = false,
      Kind::Combine => d.op = "join",
      Kind::Extrude => {
        // extruding from a face sketch defaults to Join, like Fusion
        if let Some(sk) = d.sketch
          && let Some(FeatureKind::Sketch { plane: PlaneRef::Face { .. }, .. }) = s.design.feature(sk).map(|f| &f.kind)
        {
          d.op = "join";
        }
      }
      _ => {}
    }
    // Fusion finishes the sketch when a feature starts from it
    if matches!(kind, Kind::Extrude | Kind::Revolve) && app.s.edit_sketch.is_some() {
      app.last_sketch = app.s.edit_sketch;
      let _ = app.s.execute("sketch.finish", &json!({}));
    }
    d.dirty = !kind.picks_model();
    d
  }

  /// Opens the dialog for an existing feature (double-click in the timeline).
  pub fn edit(app: &App, id: FeatureId) -> Option<Dialog> {
    let f = app.s.design.feature(id)?;
    let mut d = match &f.kind {
      FeatureKind::Extrude { sketch, extent, operation, .. } => {
        let mut d = Dialog::blank(Kind::Extrude);
        d.sketch = Some(*sketch);
        d.distance = extent.distance.clone();
        d.distance2 = extent.distance2.clone();
        d.flip = extent.flip;
        d.extent = match extent.kind {
          ExtentKind::OneSide => "one_side",
          ExtentKind::Symmetric => "symmetric",
          ExtentKind::TwoSides => "two_sides",
          ExtentKind::ThroughAll => "through_all",
          ExtentKind::ToFace => "to_face",
        };
        d.op = op_str(*operation);
        d
      }
      FeatureKind::Revolve { sketch, angle, operation, axis, .. } => {
        let mut d = Dialog::blank(Kind::Revolve);
        d.sketch = Some(*sketch);
        d.angle = angle.clone();
        d.op = op_str(*operation);
        d.axis = match axis {
          AxisRef::X => "x".into(),
          AxisRef::Y => "y".into(),
          AxisRef::Z => "z".into(),
          AxisRef::SketchLine { curve } => format!("line:{}", curve.0),
          AxisRef::Edge { .. } => "edge".into(),
        };
        d
      }
      FeatureKind::Fillet { radius, edges, .. } => {
        let mut d = Dialog::blank(Kind::Fillet);
        d.radius = radius.clone();
        d.edges = vec![Value::Null; edges.len()];
        d
      }
      FeatureKind::Chamfer { distance, edges, .. } => {
        let mut d = Dialog::blank(Kind::Chamfer);
        d.radius = distance.clone();
        d.edges = vec![Value::Null; edges.len()];
        d
      }
      FeatureKind::Shell { thickness, faces } => {
        let mut d = Dialog::blank(Kind::Shell);
        d.radius = thickness.clone();
        d.faces = vec![Value::Null; faces.len()];
        d
      }
      FeatureKind::Primitive { shape, at, operation, .. } => {
        let (kind, a, b, c) = match shape {
          Primitive::Box { width, depth, height } => (Kind::Box, width.clone(), depth.clone(), height.clone()),
          Primitive::Cylinder { radius, height } => (Kind::Cylinder, radius.clone(), height.clone(), String::new()),
          Primitive::Sphere { radius } => (Kind::Sphere, radius.clone(), String::new(), String::new()),
          Primitive::Torus { major, minor } => (Kind::Torus, major.clone(), minor.clone(), String::new()),
        };
        let mut d = Dialog::blank(kind);
        d.a = a;
        d.b = b;
        d.c = c;
        d.at = [clf_engine::sketch::Dim::new(at[0]).expr, clf_engine::sketch::Dim::new(at[1]).expr];
        d.op = op_str(*operation);
        d
      }
      FeatureKind::ConstructionPlane { offset, .. } => {
        let mut d = Dialog::blank(Kind::OffsetPlane);
        d.distance = offset.clone();
        d
      }
      FeatureKind::RectangularPattern { count, spacing, count2, spacing2, direction2, join, .. } => {
        let mut d = Dialog::blank(Kind::RectPattern);
        d.count = count.clone();
        d.spacing = spacing.clone();
        d.count2 = count2.clone();
        d.spacing2 = spacing2.clone();
        d.use_dir2 = direction2.is_some();
        d.join = *join;
        d
      }
      FeatureKind::CircularPattern { count, angle, join, .. } => {
        let mut d = Dialog::blank(Kind::CircPattern);
        d.count = count.clone();
        d.angle = angle.clone();
        d.join = *join;
        d
      }
      FeatureKind::Move { translate, angle, .. } => {
        let mut d = Dialog::blank(Kind::Move);
        d.translate = translate.clone();
        d.angle = angle.clone();
        d
      }
      _ => return None,
    };
    d.editing = Some(id);
    Some(d)
  }

  /// The command (name, params) this dialog applies.
  fn command(&self) -> Option<(&'static str, Value)> {
    if let Some(id) = self.editing {
      return Some(("feature.edit", json!({"feature": id.0, "set": self.edit_set()?})));
    }
    let op = self.op;
    Some(match self.kind {
      Kind::PickPlane | Kind::Inspect => return None,
      Kind::Extrude => (
        "extrude",
        json!({
          "sketch": self.sketch?.0, "profiles": self.profiles, "distance": self.distance, "distance2": self.distance2,
          "extent": self.extent, "flip": self.flip, "operation": op,
        }),
      ),
      Kind::Revolve => {
        let axis = match self.axis.strip_prefix("line:") {
          Some(n) => json!({"line": n.parse::<u32>().ok()?}),
          None => json!(self.axis),
        };
        (
          "revolve",
          json!({"sketch": self.sketch?.0, "profiles": self.profiles, "axis": axis, "angle": self.angle, "operation": op}),
        )
      }
      Kind::Fillet => ("fillet", json!({"edges": self.edges, "radius": self.radius})),
      Kind::Chamfer => ("chamfer", json!({"edges": self.edges, "distance": self.radius})),
      Kind::Shell => ("shell", json!({"faces": self.faces, "thickness": self.radius})),
      Kind::Box => (
        "box",
        json!({"plane": self.plane, "at": self.at_pt()?, "width": self.a, "depth": self.b, "height": self.c, "operation": op}),
      ),
      Kind::Cylinder => (
        "cylinder",
        json!({"plane": self.plane, "at": self.at_pt()?, "radius": self.a, "height": self.b, "operation": op}),
      ),
      Kind::Sphere => ("sphere", json!({"plane": self.plane, "at": self.at_pt()?, "radius": self.a, "operation": op})),
      Kind::Torus => {
        ("torus", json!({"plane": self.plane, "at": self.at_pt()?, "major": self.a, "minor": self.b, "operation": op}))
      }
      Kind::OffsetPlane => ("plane.offset", json!({"plane": self.plane, "offset": self.distance})),
      Kind::Mirror => ("mirror", json!({"plane": self.plane, "bodies": self.bodies, "join": self.join})),
      Kind::RectPattern => {
        let mut p = json!({
          "bodies": self.bodies, "direction": axis_vec(&self.dir), "count": self.count, "spacing": self.spacing, "join": self.join,
        });
        if self.use_dir2 {
          p["direction2"] = json!(axis_vec(&self.dir2));
          p["count2"] = json!(self.count2);
          p["spacing2"] = json!(self.spacing2);
        }
        ("pattern.rectangular", p)
      }
      Kind::CircPattern => (
        "pattern.circular",
        json!({"bodies": self.bodies, "axis": self.axis, "count": self.count, "angle": self.angle, "join": self.join}),
      ),
      Kind::Combine => {
        let (target, tools) = self.bodies.split_first()?;
        ("combine", json!({"target": target, "tools": tools, "operation": op, "keep_tools": self.keep_tools}))
      }
      Kind::Move => {
        let mut p = json!({"bodies": self.bodies, "translate": self.translate});
        if !matches!(self.angle.trim(), "" | "0" | "0 deg") {
          p["axis"] = json!(self.axis);
          p["angle"] = json!(self.angle);
        }
        ("move", p)
      }
    })
  }

  /// Fields for `feature.edit` when editing an existing feature.
  fn edit_set(&self) -> Option<Value> {
    Some(match self.kind {
      Kind::Extrude => json!({
        "extent": {"kind": self.extent, "distance": self.distance, "distance2": self.distance2, "flip": self.flip},
        "operation": self.op,
      }),
      Kind::Revolve => json!({"angle": self.angle, "operation": self.op}),
      Kind::Fillet => json!({"radius": self.radius}),
      Kind::Chamfer => json!({"distance": self.radius}),
      Kind::Shell => json!({"thickness": self.radius}),
      Kind::Box => {
        json!({"shape": {"type": "box", "width": self.a, "depth": self.b, "height": self.c}, "at": self.at_pt()?, "operation": self.op})
      }
      Kind::Cylinder => {
        json!({"shape": {"type": "cylinder", "radius": self.a, "height": self.b}, "at": self.at_pt()?, "operation": self.op})
      }
      Kind::Sphere => json!({"shape": {"type": "sphere", "radius": self.a}, "at": self.at_pt()?, "operation": self.op}),
      Kind::Torus => {
        json!({"shape": {"type": "torus", "major": self.a, "minor": self.b}, "at": self.at_pt()?, "operation": self.op})
      }
      Kind::OffsetPlane => json!({"offset": self.distance}),
      Kind::RectPattern => {
        json!({"count": self.count, "spacing": self.spacing, "count2": self.count2, "spacing2": self.spacing2, "join": self.join})
      }
      Kind::CircPattern => json!({"count": self.count, "angle": self.angle, "join": self.join}),
      Kind::Move => json!({"translate": self.translate, "angle": self.angle}),
      _ => return None,
    })
  }

  fn at_pt(&self) -> Option<[f64; 2]> {
    let v = |s: &str| s.trim().parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(0.0);
    Some([v(&self.at[0]), v(&self.at[1])])
  }

  /// Withdraws the current preview (if any) and applies the dialog's command.
  pub fn preview(&mut self, s: &mut Session) {
    self.dirty = false;
    let selection = s.selection.clone();
    self.withdraw(s);
    let Some((cmd, params)) = self.command() else {
      return;
    };
    if !self.ready() {
      return;
    }
    match s.execute(cmd, &params) {
      Ok(v) => {
        self.applied = true;
        self.error = v.get("error").and_then(Value::as_str).map(str::to_string);
      }
      Err(e) => self.error = Some(e.0),
    }
    if !self.kind.picks_model() {
      s.selection = selection;
    }
  }

  /// Enough input to try?
  fn ready(&self) -> bool {
    if self.editing.is_some() {
      return true;
    }
    match self.kind {
      Kind::Extrude | Kind::Revolve => self.sketch.is_some(),
      Kind::Fillet | Kind::Chamfer => !self.edges.is_empty(),
      Kind::Shell => !self.faces.is_empty(),
      Kind::Combine => self.bodies.len() >= 2,
      _ => true,
    }
  }

  fn withdraw(&mut self, s: &mut Session) {
    if self.applied {
      s.undo();
      s.drop_redo();
      self.applied = false;
      if self.kind.picks_model() && self.editing.is_none() {
        // the picks are valid again on the un-previewed model
        s.selection = self.picked_selection();
      }
    }
  }

  fn picked_selection(&self) -> Vec<Sel> {
    let get = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
    let mut out = Vec::new();
    for e in &self.edges {
      if let (Some(b), Some(i)) = (get(e, "body"), get(e, "edge")) {
        out.push(Sel::Edge { body: BodyId(b), edge: i });
      }
    }
    for f in &self.faces {
      if let (Some(b), Some(i)) = (get(f, "body"), get(f, "face")) {
        out.push(Sel::Face { body: BodyId(b), face: i });
      }
    }
    if self.kind == Kind::Combine {
      out.extend(self.bodies.iter().map(|b| Sel::Body { body: BodyId(*b) }));
    }
    out
  }

  pub fn cancel(&mut self, s: &mut Session) {
    self.withdraw(s);
  }

  /// Is a model-picking dialog's preview showing? (Viewport clicks withdraw it first.)
  pub fn previewing_model(&self) -> bool {
    self.applied && self.kind.picks_model() && self.editing.is_none()
  }

  pub fn unpreview(&mut self, s: &mut Session) {
    self.withdraw(s);
  }

  /// Sketch whose profiles should stay visible/pickable while this dialog is open.
  pub fn profile_sketch(&self) -> Option<FeatureId> {
    matches!(self.kind, Kind::Extrude | Kind::Revolve).then_some(self.sketch).flatten()
  }

  /// Picks up selection changes made in the viewport while the dialog is open.
  fn sync_selection(&mut self, s: &Session) {
    if self.editing.is_some() {
      return;
    }
    match self.kind {
      Kind::Extrude | Kind::Revolve => {
        let mut pts = Vec::new();
        for sel in &s.selection {
          if let Sel::Profile { sketch, index } = sel
            && let Some(p) = s.model.sketches.get(sketch).and_then(|r| r.profiles.get(*index))
          {
            if self.sketch != Some(*sketch) {
              self.sketch = Some(*sketch);
            }
            pts.push(p.interior_point());
          }
        }
        if !pts.is_empty() && pts != self.profiles {
          self.profiles = pts;
          self.dirty = true;
        }
      }
      Kind::Fillet | Kind::Chamfer if !self.applied => {
        let e: Vec<Value> = s
          .selection
          .iter()
          .filter_map(
            |x| if let Sel::Edge { body, edge } = x { Some(json!({"body": body.0, "edge": edge})) } else { None },
          )
          .collect();
        if e != self.edges {
          self.edges = e;
        }
      }
      Kind::Shell if !self.applied => {
        let f: Vec<Value> = s
          .selection
          .iter()
          .filter_map(
            |x| if let Sel::Face { body, face } = x { Some(json!({"body": body.0, "face": face})) } else { None },
          )
          .collect();
        if f != self.faces {
          self.faces = f;
        }
      }
      Kind::Combine | Kind::Mirror | Kind::RectPattern | Kind::CircPattern | Kind::Move
        if !self.applied || !self.kind.picks_model() =>
      {
        let mut b: Vec<u32> = Vec::new();
        for x in &s.selection {
          let id = match x {
            Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } => body.0,
            _ => continue,
          };
          if !b.contains(&id) {
            b.push(id);
          }
        }
        if !b.is_empty() && b != self.bodies {
          self.bodies = b;
          self.dirty = !self.kind.picks_model();
        }
      }
      _ => {}
    }
  }

  /// Draws the dialog. Returns false when it closed.
  pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> bool {
    self.sync_selection(&app.s);
    if self.kind == Kind::PickPlane
      && let Some(Sel::Face { body, face }) = app.s.selection.iter().find(|x| matches!(x, Sel::Face { .. })).copied()
    {
      if is_planar(&app.s, body, face) {
        app.create_sketch(json!({"body": body.0, "face": face}));
        return false;
      }
      app.status = Some(("Pick a planar face".into(), true));
      app.s.selection.clear();
    }
    let mut keep = true;
    let mut ok = false;
    let mut cancel = false;
    let title =
      if self.editing.is_some() { format!("EDIT {}", self.kind.title()) } else { self.kind.title().to_string() };
    let width = 300.0;
    egui::Area::new(egui::Id::new("feature-dialog"))
      .default_pos(app.canvas.right_top() + egui::vec2(-34.0, 190.0))
      .pivot(egui::Align2::RIGHT_TOP)
      .constrain(true)
      .order(egui::Order::Foreground)
      .show(ctx, |ui| {
        panels::dialog_frame().show(ui, |ui| {
          ui.set_width(width);
          ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
          panels::dialog_header(ui, self.kind.icon(), &title, width);
          egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            egui::Grid::new("dlg").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
              self.fields(app, ui);
            });
            if let Some(e) = &self.error {
              ui.add_space(6.0);
              ui.label(RichText::new(format!("⚠ {e}")).color(theme::ERROR));
            }
          });
          let sep = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover()).0;
          ui.painter().rect_filled(sep, 0.0, theme::SEPARATOR);
          egui::Frame::new().fill(theme::DIALOG_BODY).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
            ui.horizontal(|ui| {
              ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
              let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 24.0), egui::Sense::hover());
              ui.painter().circle_stroke(r.center(), 7.0, egui::Stroke::new(1.2, theme::GROUP_LABEL));
              ui.painter().text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "i",
                egui::FontId::proportional(11.0),
                theme::GROUP_LABEL,
              );
              if !matches!(self.kind, Kind::PickPlane | Kind::Inspect) {
                if self.kind.picks_model() && self.editing.is_none() {
                  let mut on = self.applied;
                  if ui.checkbox(&mut on, "Preview").changed() {
                    if self.applied {
                      self.withdraw(&mut app.s);
                    } else {
                      self.preview(&mut app.s);
                    }
                  }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                  if panels::secondary_button(ui, "Cancel").clicked() {
                    cancel = true;
                  }
                  if panels::primary_button(ui, "OK", self.accent()).clicked() {
                    ok = true;
                  }
                });
              } else {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                  if panels::secondary_button(ui, if self.kind == Kind::PickPlane { "Cancel" } else { "Close" })
                    .clicked()
                  {
                    cancel = true;
                  }
                });
              }
            });
          });
        });
      });
    if self.dirty {
      self.preview(&mut app.s);
    }
    if ok {
      if !self.applied {
        self.preview(&mut app.s);
      }
      if self.applied && self.error.is_none() {
        keep = false;
        app.s.selection.clear();
        app.status = Some((format!("{} done", self.kind.title().to_lowercase()), false));
      } else if !self.applied && self.error.is_none() {
        self.error = Some(self.missing_input().into());
      } else if self.applied {
        // keep the failing feature in the timeline (red), like Fusion
        keep = false;
      }
    }
    if cancel {
      self.cancel(&mut app.s);
      keep = false;
    }
    keep && !self.closed
  }

  /// The command family's accent colour, used by this dialog's chips and OK button.
  fn accent(&self) -> egui::Color32 {
    crate::icons::accent_of(self.kind.icon())
  }

  fn missing_input(&self) -> &'static str {
    match self.kind {
      Kind::Extrude | Kind::Revolve => "select a profile",
      Kind::Fillet | Kind::Chamfer => "select edges",
      Kind::Shell => "select faces to remove",
      Kind::Combine => "select a target body and tool bodies",
      _ => "missing input",
    }
  }

  fn fields(&mut self, app: &mut App, ui: &mut egui::Ui) {
    let creating = self.editing.is_none();
    match self.kind {
      Kind::PickPlane => {
        ui.label("Plane");
        ui.vertical(|ui| {
          for (label, p) in [("XY  (Top)", "xy"), ("XZ  (Front)", "xz"), ("YZ  (Right)", "yz")] {
            if ui.button(label).clicked() {
              app.create_sketch(json!(p));
              self.closed = true;
            }
          }
          ui.label(RichText::new("…or click a planar face").small().color(theme::MUTED));
        });
        ui.end_row();
      }
      Kind::Extrude | Kind::Revolve => {
        ui.label("Profiles");
        let n = self.profiles.len();
        if self.editing.is_some() {
          ui.label("(unchanged)");
        } else if n == 0 {
          sel_chip(ui, self.accent(), "Select (all regions)", false);
        } else {
          sel_chip(ui, self.accent(), &format!("{n} selected"), true);
        }
        ui.end_row();
        if self.kind == Kind::Extrude {
          ui.label("Extent");
          self.combo(ui, "extent", &EXTENTS, true);
          ui.end_row();
          if self.extent != "through_all" {
            ui.label(if self.extent == "two_sides" { "Distance 1" } else { "Distance" });
            self.text(ui, true);
            ui.end_row();
          }
          if self.extent == "two_sides" {
            ui.label("Distance 2");
            if ui.add(egui::TextEdit::singleline(&mut self.distance2).desired_width(140.0)).changed() {
              self.dirty = true;
            }
            ui.end_row();
          }
          ui.label("Direction");
          if ui.checkbox(&mut self.flip, "Flip").changed() {
            self.dirty = true;
          }
          ui.end_row();
        } else {
          ui.label("Axis");
          let lines = self.sketch_lines(app);
          let current = self.axis.clone();
          egui::ComboBox::from_id_salt("axis").selected_text(axis_label(&current)).show_ui(ui, |ui| {
            for a in ["x", "y", "z"] {
              if ui.selectable_label(current == a, axis_label(a)).clicked() {
                self.axis = a.into();
                self.dirty = true;
              }
            }
            for (id, label) in lines {
              let key = format!("line:{id}");
              if ui.selectable_label(current == key, label).clicked() {
                self.axis = key;
                self.dirty = true;
              }
            }
          });
          ui.end_row();
          ui.label("Angle");
          if ui.add(egui::TextEdit::singleline(&mut self.angle).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
        }
        ui.label("Operation");
        self.combo(ui, "op", &OPS, false);
        ui.end_row();
      }
      Kind::Fillet | Kind::Chamfer | Kind::Shell => {
        let (what, n) =
          if self.kind == Kind::Shell { ("Faces", self.faces.len()) } else { ("Edges", self.edges.len()) };
        ui.label(what);
        if creating {
          sel_chip(ui, self.accent(), &if n == 0 { "Select".to_string() } else { format!("{n} selected") }, n > 0);
        } else {
          ui.label(format!("{n} (unchanged)"));
        }
        ui.end_row();
        ui.label(match self.kind {
          Kind::Fillet => "Radius",
          Kind::Chamfer => "Distance",
          _ => "Thickness",
        });
        if ui.add(egui::TextEdit::singleline(&mut self.radius).desired_width(140.0)).changed()
          && (!creating || self.applied)
        {
          self.dirty = true;
        }
        ui.end_row();
      }
      Kind::Box | Kind::Cylinder | Kind::Sphere | Kind::Torus => {
        ui.label("Plane");
        ui.label(&self.plane_label);
        ui.end_row();
        let labels: &[&str] = match self.kind {
          Kind::Box => &["Width (X)", "Depth (Y)", "Height"],
          Kind::Cylinder => &["Radius", "Height"],
          Kind::Sphere => &["Radius"],
          _ => &["Major radius", "Minor radius"],
        };
        for (i, l) in labels.iter().enumerate() {
          ui.label(*l);
          let f = match i {
            0 => &mut self.a,
            1 => &mut self.b,
            _ => &mut self.c,
          };
          if ui.add(egui::TextEdit::singleline(f).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
        }
        ui.label("Position X, Y");
        ui.horizontal(|ui| {
          for v in self.at.iter_mut() {
            if ui.add(egui::TextEdit::singleline(v).desired_width(64.0)).changed() {
              self.dirty = true;
            }
          }
        });
        ui.end_row();
        ui.label("Operation");
        self.combo(ui, "op", &OPS, false);
        ui.end_row();
      }
      Kind::OffsetPlane | Kind::Mirror => {
        ui.label("Plane");
        if creating {
          self.plane_choice(ui);
        } else {
          ui.label("(unchanged)");
        }
        ui.end_row();
        if self.kind == Kind::OffsetPlane {
          ui.label("Offset");
          self.text(ui, true);
          ui.end_row();
        } else {
          ui.label("Bodies");
          sel_chip(ui, self.accent(), &self.bodies_label(), !self.bodies.is_empty());
          ui.end_row();
          ui.label("");
          if ui.checkbox(&mut self.join, "Join to original").changed() {
            self.dirty = true;
          }
          ui.end_row();
        }
      }
      Kind::RectPattern | Kind::CircPattern => {
        ui.label("Bodies");
        sel_chip(ui, self.accent(), &self.bodies_label(), !self.bodies.is_empty());
        ui.end_row();
        if self.kind == Kind::RectPattern {
          ui.label("Direction");
          self.axis_combo(ui, "dir1", true);
          ui.end_row();
          ui.label("Count");
          if ui.add(egui::TextEdit::singleline(&mut self.count).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
          ui.label("Spacing");
          if ui.add(egui::TextEdit::singleline(&mut self.spacing).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
          ui.label("");
          if ui.checkbox(&mut self.use_dir2, "Second direction").changed() {
            self.dirty = true;
          }
          ui.end_row();
          if self.use_dir2 {
            ui.label("Direction 2");
            self.axis_combo(ui, "dir2", false);
            ui.end_row();
            ui.label("Count 2");
            if ui.add(egui::TextEdit::singleline(&mut self.count2).desired_width(140.0)).changed() {
              self.dirty = true;
            }
            ui.end_row();
            ui.label("Spacing 2");
            if ui.add(egui::TextEdit::singleline(&mut self.spacing2).desired_width(140.0)).changed() {
              self.dirty = true;
            }
            ui.end_row();
          }
        } else {
          ui.label("Axis");
          self.axis_combo(ui, "axis", false);
          ui.end_row();
          ui.label("Count");
          if ui.add(egui::TextEdit::singleline(&mut self.count).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
          ui.label("Total angle");
          if ui.add(egui::TextEdit::singleline(&mut self.angle).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
        }
        ui.label("");
        if ui.checkbox(&mut self.join, "Join instances").changed() {
          self.dirty = true;
        }
        ui.end_row();
      }
      Kind::Combine => {
        ui.label("Target");
        match self.bodies.first() {
          Some(b) => sel_chip(ui, self.accent(), &app.s.body_name(BodyId(*b)), true),
          None => sel_chip(ui, self.accent(), "Select", false),
        };
        ui.end_row();
        ui.label("Tools");
        ui.label(format!("{}", self.bodies.len().saturating_sub(1)));
        ui.end_row();
        ui.label("Operation");
        self.combo(ui, "op", &OPS[1..], false);
        ui.end_row();
        ui.label("");
        ui.checkbox(&mut self.keep_tools, "Keep tools");
        ui.end_row();
      }
      Kind::Move => {
        ui.label("Bodies");
        sel_chip(ui, self.accent(), &self.bodies_label(), !self.bodies.is_empty());
        ui.end_row();
        for (i, l) in ["X distance", "Y distance", "Z distance"].iter().enumerate() {
          ui.label(*l);
          if ui.add(egui::TextEdit::singleline(&mut self.translate[i]).desired_width(140.0)).changed() {
            self.dirty = true;
          }
          ui.end_row();
        }
        ui.label("Rotate about");
        self.axis_combo(ui, "axis", false);
        ui.end_row();
        ui.label("Angle");
        if ui.add(egui::TextEdit::singleline(&mut self.angle).desired_width(140.0)).changed() {
          self.dirty = true;
        }
        ui.end_row();
      }
      Kind::Inspect => inspect(app, ui),
    }
  }

  fn text(&mut self, ui: &mut egui::Ui, distance: bool) {
    let f = if distance { &mut self.distance } else { &mut self.radius };
    if ui.add(egui::TextEdit::singleline(f).desired_width(140.0)).changed() {
      self.dirty = true;
    }
  }

  fn combo(&mut self, ui: &mut egui::Ui, id: &str, items: &[(&'static str, &'static str)], extent: bool) {
    let cur = if extent { self.extent } else { self.op };
    let label = items.iter().find(|(k, _)| *k == cur).map(|(_, l)| *l).unwrap_or(cur);
    egui::ComboBox::from_id_salt(id).selected_text(label).width(140.0).show_ui(ui, |ui| {
      for (k, l) in items {
        if ui.selectable_label(cur == *k, *l).clicked() {
          if extent {
            self.extent = k;
          } else {
            self.op = k;
          }
          self.dirty = true;
        }
      }
    });
  }

  fn axis_combo(&mut self, ui: &mut egui::Ui, id: &str, first: bool) {
    let cur = if id == "dir1" {
      self.dir.clone()
    } else if id == "dir2" {
      self.dir2.clone()
    } else {
      self.axis.clone()
    };
    let _ = first;
    egui::ComboBox::from_id_salt(id).selected_text(axis_label(&cur)).width(140.0).show_ui(ui, |ui| {
      for a in ["x", "y", "z"] {
        if ui.selectable_label(cur == a, axis_label(a)).clicked() {
          match id {
            "dir1" => self.dir = a.into(),
            "dir2" => self.dir2 = a.into(),
            _ => self.axis = a.into(),
          }
          self.dirty = true;
        }
      }
    });
  }

  fn plane_choice(&mut self, ui: &mut egui::Ui) {
    let cur = self.plane_label.clone();
    egui::ComboBox::from_id_salt("plane").selected_text(cur.clone()).width(140.0).show_ui(ui, |ui| {
      for (l, p) in [("XY plane", "xy"), ("XZ plane", "xz"), ("YZ plane", "yz")] {
        if ui.selectable_label(cur == l, l).clicked() {
          self.plane = json!(p);
          self.plane_label = l.into();
          self.dirty = true;
        }
      }
    });
  }

  fn bodies_label(&self) -> String {
    if self.bodies.is_empty() { "all bodies".into() } else { format!("{} selected", self.bodies.len()) }
  }

  fn sketch_lines(&self, app: &App) -> Vec<(u32, String)> {
    let Some(sk) = self.sketch else { return vec![] };
    let Ok(s) = app.s.design.sketch(sk) else { return vec![] };
    s.curves
      .iter()
      .filter(|c| matches!(c.kind, clf_engine::sketch::CurveKind::Line { .. }))
      .map(|c| (c.id.0, format!("Sketch line {}{}", c.id.0, if c.construction { " (construction)" } else { "" })))
      .collect()
  }
}

fn inspect(app: &mut App, ui: &mut egui::Ui) {
  let s = &app.s;
  if s.selection.is_empty() {
    ui.label("Select a body, face or edge");
    ui.end_row();
    return;
  }
  for sel in s.selection.clone().iter().take(6) {
    match sel {
      Sel::Body { body } => {
        if let Some(b) = s.model.body(*body)
          && let Ok(m) = b.shape.mass()
        {
          ui.label(RichText::new(s.body_name(*body)).strong());
          ui.end_row();
          ui.label("Volume");
          ui.label(format!("{:.3} mm³", m.volume));
          ui.end_row();
          ui.label("Area");
          ui.label(format!("{:.3} mm²", m.area));
          ui.end_row();
          let d = m.bbox_max - m.bbox_min;
          ui.label("Size");
          ui.label(format!("{:.3} × {:.3} × {:.3}", d.x, d.y, d.z));
          ui.end_row();
          ui.label("Centre of mass");
          let c = m.center_of_mass;
          ui.label(format!("{:.3}, {:.3}, {:.3}", c.x, c.y, c.z));
          ui.end_row();
        }
      }
      Sel::Face { body, face } => {
        if let Some(f) = s.model.body(*body).and_then(|b| b.faces().iter().find(|f| f.id == *face).copied()) {
          ui.label(RichText::new(format!("Face ({:?})", f.kind)).strong());
          ui.end_row();
          ui.label("Area");
          ui.label(format!("{:.3} mm²", f.area));
          ui.end_row();
          if f.radius > 0.0 {
            ui.label("Radius");
            ui.label(format!("{:.3} mm", f.radius));
            ui.end_row();
          }
        }
      }
      Sel::Edge { body, edge } => {
        if let Some(e) = s.model.body(*body).and_then(|b| b.edges().iter().find(|e| e.id == *edge).copied()) {
          ui.label(RichText::new(format!("Edge ({:?})", e.kind)).strong());
          ui.end_row();
          ui.label("Length");
          ui.label(format!("{:.3} mm", e.length));
          ui.end_row();
          if e.radius > 0.0 {
            ui.label("Radius");
            ui.label(format!("{:.3} mm", e.radius));
            ui.end_row();
          }
        }
      }
      _ => {}
    }
  }
}

fn axis_label(a: &str) -> String {
  match a {
    "x" => "X axis".into(),
    "y" => "Y axis".into(),
    "z" => "Z axis".into(),
    other => other.replace("line:", "Sketch line "),
  }
}

fn axis_vec(a: &str) -> [f64; 3] {
  match a {
    "y" => [0.0, 1.0, 0.0],
    "z" => [0.0, 0.0, 1.0],
    _ => [1.0, 0.0, 0.0],
  }
}

fn op_str(o: Operation) -> &'static str {
  match o {
    Operation::NewBody => "new_body",
    Operation::Join => "join",
    Operation::Cut => "cut",
    Operation::Intersect => "intersect",
  }
}

fn is_planar(s: &Session, body: BodyId, face: u32) -> bool {
  s.model
    .body(body)
    .and_then(|b| b.faces().iter().find(|f| f.id == face).map(|f| f.kind == clf_engine::kernel::SurfaceKind::Plane))
    .unwrap_or(false)
}

fn last_unconsumed_sketch(s: &Session) -> Option<FeatureId> {
  let active = s.design.active_len();
  let feats = &s.design.features[..active];
  feats
    .iter()
    .rev()
    .find(|f| {
      matches!(f.kind, FeatureKind::Sketch { .. })
        && !feats.iter().any(|g| matches!(&g.kind, FeatureKind::Extrude { sketch, .. } | FeatureKind::Revolve { sketch, .. } if *sketch == f.id))
    })
    .map(|f| f.id)
}

/// A selection input: a white "Select" chip until something is picked, then a chip tinted with
/// the command's accent showing the count, with a ✕ beside it.
fn sel_chip(ui: &mut egui::Ui, accent: egui::Color32, text: &str, has: bool) -> egui::Response {
  let font = egui::FontId::proportional(12.0);
  let tw = ui.painter().layout_no_wrap(text.to_string(), font.clone(), theme::TEXT).size().x;
  let (rect, resp) = ui.allocate_exact_size(egui::vec2(tw + 52.0, 22.0), egui::Sense::hover());
  let chip = egui::Rect::from_min_size(rect.min, egui::vec2(tw + 32.0, 22.0));
  let p = ui.painter();
  let ir = egui::Rect::from_center_size(chip.left_center() + egui::vec2(12.0, 0.0), egui::Vec2::splat(15.0));
  let fill = if has { crate::icons::tint_of(accent, 0.84) } else { egui::Color32::WHITE };
  let stroke = if has { accent } else { theme::BORDER };
  p.rect(chip, 3.0, fill, egui::Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
  crate::icons::draw(p, ir, Icon::Select, accent);
  p.text(chip.left_center() + egui::vec2(24.0, 0.0), egui::Align2::LEFT_CENTER, text, font, theme::TEXT);
  if has {
    let x = egui::pos2(chip.right() + 10.0, chip.center().y);
    let s = egui::Stroke::new(1.4, theme::GROUP_LABEL);
    p.line_segment([x + egui::vec2(-4.0, -4.0), x + egui::vec2(4.0, 4.0)], s);
    p.line_segment([x + egui::vec2(4.0, -4.0), x + egui::vec2(-4.0, 4.0)], s);
  }
  resp
}
