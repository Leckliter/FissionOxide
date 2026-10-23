//! Selection rows with an active row, as Fusion's dialogs have them: the next picks go to the
//! active (blue) row, a single-pick row hands the active row on once picked, and a click on
//! another row's chip makes it active. COMBINE (Target Body, Tool Bodies), SPLIT BODY (Body to
//! Split, Splitting Tool(s)) and the forms with [`crate::forms::Input::Pick`] rows work so. While
//! one is open the selection is kept to the rows' picks and a click picks what the active row
//! takes: whole bodies for a body row (the body lights as a whole), faces and edges otherwise.

use fox_engine::model::BodyId;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::icon_row::{self, choice};
use super::{Dialog, Kind, chip_text, is_planar, sel_chip};
use crate::App;
use crate::icons::Icon;
use crate::origin::OriginItem;
use crate::select::Priority;

/// Combine's Operation row: Join, Cut and Intersect as icon toggles.
const COMBINE_OPS: [icon_row::Choice; 3] = [
  choice("join", Icon::OpJoin, "Join"),
  choice("cut", Icon::OpCut, "Cut"),
  choice("intersect", Icon::OpIntersect, "Intersect"),
];

impl Dialog {
  /// What a click on the model picks while the dialog's active row waits (None: the dialog
  /// leaves the selection priority alone).
  pub(super) fn row_priority(&self) -> Option<Priority> {
    match self.kind {
      Kind::Combine => Some(Priority::Body),
      Kind::SplitBody if self.pick_row == 0 => Some(Priority::Body),
      Kind::SplitBody => Some(Priority::Face),
      Kind::Form(f) => self.form.row_priority(f),
      _ => None,
    }
  }

  /// Does the active selection row take a construction plane clicked in the canvas or the
  /// browser (Replace Face's Target Faces, Split Body's Splitting Tool(s))?
  pub(crate) fn row_takes_plane(&self) -> bool {
    match self.kind {
      Kind::SplitBody => self.pick_row == 1,
      Kind::Form(f) => self.editing.is_none() && self.form.row_takes_plane(f),
      _ => false,
    }
  }

  /// Does the dialog route picks to an active selection row?
  pub(crate) fn routes_picks(&self) -> bool {
    self.row_priority().is_some()
  }

  /// Runs each frame after the picks are read: the active row's priority, an origin plane or
  /// axis picked in the canvas for a row that takes one, and the selection kept to the picks.
  pub(super) fn route_picks(&mut self, app: &mut App) {
    if self.row_priority().is_none() {
      return;
    }
    if self.saved_filter.is_none() {
      self.saved_filter = Some((app.sel_priority, app.sel_filter.vertices));
    }
    if self.editing.is_none() && !(self.applied && self.picks()) {
      if let Some(item) = crate::origin::picked(app)
        && self.origin_pick(item)
      {
        crate::origin::clear(app);
        self.dirty = !self.picks();
      }
      let want = self.picked_selection();
      let s = &mut app.s;
      if want.len() != s.selection.len() || want.iter().any(|x| !s.selection.contains(x)) {
        s.selection = want;
      }
    }
    // (after the picks: a pick may have handed the active row on)
    if let Some(priority) = self.row_priority() {
      app.sel_priority = priority;
    }
  }

  /// An origin plane or axis picked for the active row; true if the row took it.
  fn origin_pick(&mut self, item: OriginItem) -> bool {
    let plane = match item {
      OriginItem::Xy => Some("xy"),
      OriginItem::Xz => Some("xz"),
      OriginItem::Yz => Some("yz"),
      _ => None,
    };
    let axis = match item {
      OriginItem::X => Some("x"),
      OriginItem::Y => Some("y"),
      OriginItem::Z => Some("z"),
      _ => None,
    };
    match self.kind {
      Kind::SplitBody if self.pick_row == 1 => {
        let Some(p) = plane else { return false };
        self.plane = json!(p);
        self.target = None;
        true
      }
      Kind::Form(f) => self.form.origin_pick(f, plane, axis),
      _ => false,
    }
  }

  /// The selection Combine's and Split Body's picks stand for (the tool of a split, if any, last).
  pub(super) fn routed_selection(&self) -> Vec<Sel> {
    let mut out: Vec<Sel> = Vec::new();
    let mut push = |x: Sel| {
      if !out.contains(&x) {
        out.push(x);
      }
    };
    if let Some(t) = self.target.filter(|_| self.kind == Kind::Combine) {
      push(Sel::Body { body: BodyId(t) });
    }
    for b in &self.bodies {
      push(Sel::Body { body: BodyId(*b) });
    }
    if self.kind == Kind::SplitBody
      && let Some(tool) = self.split_tool_sel()
    {
      push(tool);
    }
    out
  }

  /// Split Body's tool as a selection: a tool body, a planar face or a construction plane (an
  /// origin plane isn't one).
  fn split_tool_sel(&self) -> Option<Sel> {
    if let Some(b) = self.target {
      return Some(Sel::Body { body: BodyId(b) });
    }
    let get = |k: &str| self.plane.get(k).and_then(Value::as_u64).map(|x| x as u32);
    if let Some(f) = get("feature") {
      return Some(Sel::Feature { feature: fox_engine::model::FeatureId(f) });
    }
    Some(Sel::Face { body: BodyId(get("body")?), face: get("face")? })
  }

  /// Combine's picks: a body picked while Target Body is active is the target (and Tool Bodies
  /// becomes active); picked while Tool Bodies is active, it is a tool. A body unpicked leaves
  /// its row.
  pub(super) fn sync_combine(&mut self, s: &Session) {
    let held = self.routed_selection();
    let picked = |b: u32| s.selection.contains(&Sel::Body { body: BodyId(b) });
    let mut changed = false;
    if self.target.is_some_and(|t| !picked(t)) {
      self.target = None;
      self.pick_row = 0;
      changed = true;
    }
    let before = self.bodies.len();
    self.bodies.retain(|b| picked(*b));
    changed |= before != self.bodies.len();
    for x in s.selection.iter().filter(|x| !held.contains(x)) {
      let (Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. }) = *x else { continue };
      if self.target == Some(body.0) || self.bodies.contains(&body.0) {
        continue;
      }
      if self.pick_row == 0 {
        self.target = Some(body.0);
        self.pick_row = 1;
      } else {
        self.bodies.push(body.0);
      }
      changed = true;
    }
    if changed {
      self.dirty = !self.picks();
    }
  }

  /// Split Body's picks: Body to Split takes one body and hands the active row on to
  /// Splitting Tool(s), which takes a planar face or construction plane (the plane, extended)
  /// or another face (its body: a split by that body). A pick unpicked leaves its row.
  pub(super) fn sync_split_body(&mut self, s: &Session) {
    let held = self.routed_selection();
    let mut changed = false;
    if let Some(b) = self.bodies.first()
      && !s.selection.contains(&Sel::Body { body: BodyId(*b) })
    {
      self.bodies.clear();
      self.pick_row = 0;
      changed = true;
    }
    if let Some(tool) = self.split_tool_sel()
      && !s.selection.contains(&tool)
    {
      self.target = None;
      self.plane = Value::Null;
      changed = true;
    }
    for x in s.selection.iter().filter(|x| !held.contains(x)) {
      if self.pick_row == 0 {
        let (Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. }) = *x else { continue };
        self.bodies = vec![body.0];
        self.pick_row = 1;
        changed = true;
        continue;
      }
      match *x {
        Sel::Face { body, face } if is_planar(s, body, face) => {
          self.plane = json!({"body": body.0, "face": face});
          self.target = None;
        }
        Sel::Face { body, .. } | Sel::Body { body } if !self.bodies.contains(&body.0) => {
          self.target = Some(body.0);
          self.plane = Value::Null;
        }
        Sel::Feature { feature } if s.model.planes.contains_key(&feature) => {
          self.plane = json!({"feature": feature.0});
          self.target = None;
        }
        _ => continue,
      }
      changed = true;
    }
    if changed {
      self.dirty = true;
    }
  }

  /// COMBINE's rows, as Fusion lays them out: Target Body and Tool Bodies chips, Operation as
  /// icon toggles, Keep Tools with its label on the left.
  pub(super) fn combine_fields(&mut self, ui: &mut egui::Ui) {
    let target = chip_text(usize::from(self.target.is_some()), "selected");
    self.pick_row_chip(ui, "Target Body", 0, &target, self.target.is_some());
    let tools = chip_text(self.bodies.len(), "selected");
    self.pick_row_chip(ui, "Tool Bodies", 1, &tools, !self.bodies.is_empty());
    ui.label("Operation");
    if icon_row::icon_row(ui, &mut self.op, &COMBINE_OPS, "") {
      self.dirty |= self.applied;
    }
    ui.end_row();
    ui.label("Keep Tools");
    if ui.checkbox(&mut self.keep_tools, "").changed() {
      self.dirty |= self.applied;
    }
    ui.end_row();
  }

  /// SPLIT BODY's rows, as Fusion lays them out: Body to Split and Splitting Tool(s) chips, and
  /// Extend Splitting Tool(s) (ticked; a splitting plane always reaches across the body here).
  pub(super) fn split_body_fields(&mut self, ui: &mut egui::Ui) {
    let body = chip_text(self.bodies.len().min(1), "selected");
    self.pick_row_chip(ui, "Body to Split", 0, &body, !self.bodies.is_empty());
    let has = self.target.is_some() || !self.plane.is_null();
    let tool = match self.plane.as_str() {
      Some(p) if self.target.is_none() => format!("{} Plane", p.to_uppercase()),
      _ => chip_text(usize::from(has), "selected"),
    };
    self.pick_row_chip(ui, "Splitting Tool(s)", 1, &tool, has);
    ui.label("Extend Splitting Tool(s)");
    let mut on = true;
    ui.add_enabled_ui(false, |ui| ui.checkbox(&mut on, ""))
      .inner
      .on_disabled_hover_text("A splitting plane always reaches across the body in FissionOxide");
    ui.end_row();
  }

  /// A form's [`crate::forms::Input::Pick`] row: its chip, lit while the row is active or holds
  /// picks; a click on it makes the row active.
  pub(super) fn form_pick_row(&mut self, app: &App, ui: &mut egui::Ui, i: usize, label: &str, multi: bool) {
    let v = &self.form.values[i];
    let n = if multi { v.as_array().map(Vec::len).unwrap_or(0) } else { usize::from(!v.is_null()) };
    let text = match v.as_str() {
      // an origin plane or axis
      Some(o) if o.len() == 2 => format!("{} Plane", o.to_uppercase()),
      Some(o) => format!("{} Axis", o.to_uppercase()),
      None => match v.get("feature").or(v.get("axis")).and_then(Value::as_u64) {
        Some(f) => {
          app.s.design.feature(fox_engine::model::FeatureId(f as u32)).map(|f| f.name.clone()).unwrap_or_default()
        }
        None => chip_text(n, "selected"),
      },
    };
    ui.label(label);
    let active = self.form.row == i;
    let chip = sel_chip(ui, self.accent(), &text, active || n > 0);
    if !active && chip.interact(egui::Sense::click()).on_hover_text("Pick for this row").clicked() {
      self.form.row = i;
    }
    ui.end_row();
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::Session;
  use serde_json::json;

  use super::*;
  use crate::forms::{Input, REPLACE_FACE, SILHOUETTE_SPLIT, SPLIT_FACE};

  /// Two 20 mm boxes side by side, as the prober had them.
  fn two_boxes() -> App {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 20, "depth": 20, "height": 20})).unwrap();
    app.run("box", json!({"at": [40, 0], "width": 20, "depth": 20, "height": 20, "operation": "new_body"})).unwrap();
    app
  }

  /// A face of body `i` facing `dir` (a unit axis).
  fn face(app: &App, i: usize, dir: [f64; 3]) -> Sel {
    let b = &app.s.model.bodies[i];
    let f = b.faces().iter().find(|f| f.normal.dot(dir.into()) > 0.9).unwrap();
    Sel::Face { body: b.id, face: f.id }
  }

  /// A frame's pick handling: read the picks, then route them (the selection follows).
  fn frame(d: &mut Dialog, app: &mut App) {
    d.sync_selection(&app.s);
    d.route_picks(app);
  }

  /// An origin plane or axis clicked in the canvas while the dialog is open.
  fn pick_origin(d: Dialog, app: &mut App, item: OriginItem) -> Dialog {
    app.dialog = Some(d);
    crate::origin::pick(app, item);
    app.dialog.take().unwrap()
  }

  fn labels(f: &crate::forms::Form) -> Vec<&'static str> {
    f.inputs
      .iter()
      .map(|x| match x {
        Input::Pick { label, .. }
        | Input::Check { label, .. }
        | Input::CheckOn { label, .. }
        | Input::Choice { label, .. } => *label,
        _ => "",
      })
      .collect()
  }

  /// COMBINE: Target Body active with Fusion's prompt, whole bodies picked (a face click names
  /// its body), Tool Bodies active after the target, OK greyed until a tool, Join by default.
  #[test]
  fn combine_picks_a_target_body_then_tool_bodies() {
    let mut app = two_boxes();
    let (b1, b2) = (app.s.model.bodies[0].id, app.s.model.bodies[1].id);
    let mut d = Dialog::open(&mut app, Kind::Combine);
    frame(&mut d, &mut app);
    assert_eq!(app.sel_priority, Priority::Body, "clicks pick whole bodies");
    assert_eq!((d.pick_row, d.op), (0, "join"));
    assert_eq!(d.prompt(false).as_deref(), Some("Select target body"));
    assert!(!d.ok_enabled());
    app.s.selection.push(face(&app, 0, [0.0, 0.0, 1.0]));
    frame(&mut d, &mut app);
    assert_eq!((d.target, d.pick_row), (Some(b1.0), 1));
    assert_eq!(app.s.selection, [Sel::Body { body: b1 }], "the target lights as a whole body");
    assert_eq!(d.prompt(false).as_deref(), Some("Select bodies to combine with target body"));
    assert!(!d.ok_enabled());
    app.s.selection.push(Sel::Body { body: b2 });
    frame(&mut d, &mut app);
    assert_eq!(d.bodies, [b2.0]);
    assert!(d.ok_enabled());
    let (cmd, p) = d.command().unwrap();
    assert_eq!(
      (cmd, &p["target"], &p["tools"], &p["operation"]),
      ("combine", &json!(b1.0), &json!([b2.0]), &json!("join"))
    );
    // a tool unpicked leaves Tool Bodies
    app.s.selection.retain(|x| *x != Sel::Body { body: b2 });
    frame(&mut d, &mut app);
    assert!(d.bodies.is_empty() && !d.ok_enabled());
    // a click on the Target Body chip, then another body: the new target
    d.pick_row = 0;
    app.s.selection.push(Sel::Body { body: b2 });
    frame(&mut d, &mut app);
    assert_eq!((d.target, d.pick_row), (Some(b2.0), 1));
    assert_eq!(app.s.selection, [Sel::Body { body: b2 }]);
    d.leave(&mut app);
    assert_eq!(app.sel_priority, Priority::Face, "the priority is put back on close");
  }

  /// SPLIT BODY: nothing until picked (no "all bodies", no preset plane, OK greyed), Body to
  /// Split active with Fusion's prompt, then Splitting Tool(s) takes a planar face.
  #[test]
  fn split_body_picks_the_body_then_the_tool() {
    let mut app = two_boxes();
    let (b1, b2) = (app.s.model.bodies[0].id, app.s.model.bodies[1].id);
    let mut d = Dialog::open(&mut app, Kind::SplitBody);
    frame(&mut d, &mut app);
    assert!(d.bodies.is_empty() && d.plane.is_null() && d.target.is_none());
    assert!(!d.ok_enabled() && d.command().is_none());
    assert_eq!(d.prompt(false).as_deref(), Some("Select Body to split"));
    assert_eq!(app.sel_priority, Priority::Body);
    app.s.selection.push(face(&app, 0, [1.0, 0.0, 0.0]));
    frame(&mut d, &mut app);
    assert_eq!((d.bodies.clone(), d.pick_row), (vec![b1.0], 1));
    assert_eq!(app.s.selection, [Sel::Body { body: b1 }]);
    assert_eq!(app.sel_priority, Priority::Face, "the tool row picks faces");
    assert_eq!(d.prompt(false).as_deref(), Some("Select splitting tool(s)"));
    assert!(!d.ok_enabled());
    // the other box's top face: its plane is the tool (a split through the body's top: none);
    // an origin plane picked in the canvas instead cuts it in two
    let top = face(&app, 1, [0.0, 0.0, 1.0]);
    app.s.selection.push(top);
    frame(&mut d, &mut app);
    assert!(matches!(top, Sel::Face { body, .. } if d.plane["body"] == json!(body.0)));
    assert!(d.ok_enabled());
    let mut d = pick_origin(d, &mut app, OriginItem::Yz);
    frame(&mut d, &mut app);
    assert_eq!(d.plane, json!("yz"));
    assert_eq!(app.s.selection, [Sel::Body { body: b1 }]);
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, &p["bodies"]), ("split", &json!([b1.0])));
    d.preview(&mut app.s);
    assert!(d.error.is_none(), "{:?}", d.error);
    assert_eq!(app.s.model.bodies.len(), 3, "only the picked body splits");
    assert!(app.s.model.body(b2).is_some());
  }

  /// SPLIT FACE: Fusion's rows (no preset plane), Faces to Split stays active, the tool is
  /// picked once its chip is clicked; Split Type and Extend are live, and Along Vector asks for
  /// a direction.
  #[test]
  fn split_face_rows_and_tool_pick() {
    let f = &SPLIT_FACE;
    assert_eq!(
      labels(f),
      ["Faces to Split", "Splitting Tool(s)", "Chain Selection", "Split Type", "Direction", "Extend Splitting Tool"]
    );
    let mut app = two_boxes();
    let mut d = Dialog::open(&mut app, Kind::Form(f));
    frame(&mut d, &mut app);
    assert!(d.form.values[1].as_array().is_some_and(Vec::is_empty), "no preset splitting tool");
    assert_eq!((d.form.values[2].clone(), d.form.values[3].clone()), (json!(true), json!("surface")));
    assert!(crate::forms::greyed(f, "split_type").is_none() && crate::forms::greyed(f, "extend").is_none());
    assert!(!d.form.shown(f, 4), "Direction waits for Along Vector");
    assert_eq!(d.prompt(false).as_deref(), Some("Select Face to split"));
    let side = face(&app, 0, [1.0, 0.0, 0.0]);
    app.s.selection.push(side);
    frame(&mut d, &mut app);
    assert_eq!((d.form.row, d.form.values[0].as_array().map(Vec::len)), (0, Some(1)));
    assert!(!d.ok_enabled());
    d.form.row = 1;
    assert_eq!(d.prompt(false).as_deref(), Some("Select splitting tool(s)"));
    let mut d = pick_origin(d, &mut app, OriginItem::Xz);
    frame(&mut d, &mut app);
    assert_eq!(d.form.values[1], json!(["xz"]));
    assert!(d.ok_enabled());
    let faces = app.s.model.bodies[0].faces().len();
    d.preview(&mut app.s);
    assert!(d.error.is_none(), "{:?}", d.error);
    assert_eq!(app.s.model.bodies[0].faces().len(), faces + 1);
  }

  /// SPLIT FACE with sketch curves: a sketch line picked for the tool splits the face with its
  /// surface; Along Vector waits for a direction; Extend Splitting Tool off leaves a line that
  /// stops short of the edges unable to split.
  #[test]
  fn split_face_with_a_sketch_line() {
    let f = &SPLIT_FACE;
    let mut app = two_boxes();
    // a line across the first box's top, 5 mm above it, and a short one inside it
    app.run("sketch.create", json!({"plane": {"origin": [0, 0, 25], "normal": [0, 0, 1], "x": [1, 0, 0]}})).unwrap();
    let sk = app.s.design.features.last().unwrap().id;
    app.run("sketch.line", json!({"points": [[0, -30], [0, 30]]})).unwrap();
    app.run("sketch.line", json!({"points": [[5, -5], [5, 5]]})).unwrap();
    app.run("sketch.finish", json!({})).unwrap();
    let curves: Vec<_> = app.s.design.sketch(sk).unwrap().curves.iter().map(|c| c.id).collect();
    let top = face(&app, 0, [0.0, 0.0, 1.0]);
    let open = |app: &mut App, curve| {
      app.s.selection.clear();
      let mut d = Dialog::open(app, Kind::Form(f));
      app.s.selection.push(top);
      frame(&mut d, app);
      d.form.row = 1;
      app.s.selection.push(Sel::SketchCurve { sketch: sk, curve });
      frame(&mut d, app);
      d
    };
    let mut d = open(&mut app, curves[0]);
    assert_eq!(d.form.values[1], json!([{"sketch": sk.0, "curve": curves[0].0}]));
    assert!(d.ok_enabled());
    let faces = app.s.model.bodies[0].faces().len();
    d.preview(&mut app.s);
    assert!(d.error.is_none(), "{:?}", d.error);
    assert_eq!(app.s.model.bodies[0].faces().len(), faces + 1);
    // Along Vector: OK waits for the direction, then Z splits as the surface did
    d.form.values[3] = json!("vector");
    assert!(d.form.shown(f, 4) && !d.ready());
    assert_eq!(d.form.missing(f), Some("Select direction"));
    d.form.values[4] = json!("z");
    assert!(d.ready());
    assert_eq!(d.command().unwrap().1["direction"], json!("z"));
    d.cancel(&mut app.s);
    assert_eq!(app.s.model.bodies[0].faces().len(), faces);
    // the short line: extended it splits the top, as drawn it can't
    let mut d = open(&mut app, curves[1]);
    d.form.values[5] = json!(false);
    d.preview(&mut app.s);
    assert!(d.error.is_some(), "a line inside the face doesn't split it");
    d.cancel(&mut app.s);
    let mut d = open(&mut app, curves[1]);
    d.preview(&mut app.s);
    assert!(d.error.is_none(), "{:?}", d.error);
    assert_eq!(app.s.model.bodies[0].faces().len(), faces + 1);
  }

  /// REPLACE FACE: Fusion's rows, no plane drop-down; nothing previews before the target.
  #[test]
  fn replace_face_waits_for_its_target() {
    let mut app = two_boxes();
    let mut d = Dialog::open(&mut app, Kind::Form(&REPLACE_FACE));
    app.s.selection.push(face(&app, 0, [0.0, 0.0, 1.0]));
    frame(&mut d, &mut app);
    assert!(!d.ready() && d.form.row == 0);
    assert_eq!(d.prompt(false).as_deref(), Some("Select faces to replace"));
  }

  /// REPLACE FACE: with Target Faces active, a construction plane clicked in the canvas or its
  /// browser row is the target, and Source Faces keeps its pick.
  #[test]
  fn replace_face_takes_a_construction_plane_target() {
    let mut app = two_boxes();
    app.run("plane.offset", json!({"plane": "xy", "offset": 25})).unwrap();
    let up = app.s.design.features.last().unwrap().id;
    let mut d = Dialog::open(&mut app, Kind::Form(&REPLACE_FACE));
    let top = face(&app, 0, [0.0, 0.0, 1.0]);
    app.s.selection.push(top);
    frame(&mut d, &mut app);
    assert!(!d.row_takes_plane(), "Source Faces takes faces only");
    d.form.row = 2;
    assert!(d.row_takes_plane());
    app.dialog = Some(d);
    crate::panels::pick_plane(&mut app, json!({"feature": up.0}));
    let mut d = app.dialog.take().unwrap();
    frame(&mut d, &mut app);
    assert_eq!(d.form.values[2], json!({"feature": up.0}), "the plane is the target");
    assert_eq!(d.form.values[0].as_array().map(Vec::len), Some(1), "Source Faces keeps its pick");
    assert!(app.picked_plane.is_none());
    assert!(app.s.selection.contains(&top) && app.s.selection.contains(&Sel::Feature { feature: up }));
    assert_eq!(crate::construction::look(&app, up), crate::origin::Look::Selected);
    assert!(d.ready());
    d.preview(&mut app.s);
    assert!(d.error.is_none(), "{:?}", d.error);
    let m = app.s.model.bodies[0].shape.mass().unwrap();
    assert!((m.bbox_max.z - 25.0).abs() < 1e-6, "{m:?}");
  }

  /// SILHOUETTE SPLIT: View Direction then Target Body, Split Shelled Body by default; a sphere
  /// seen from above splits at its equator.
  #[test]
  fn silhouette_split_dialog_splits_a_sphere_at_its_outline() {
    let f = &SILHOUETTE_SPLIT;
    assert_eq!(labels(f), ["View Direction", "Target Body", "Operation"]);
    let mut app = App::with_session(Session::default());
    app.run("sphere", json!({"radius": 10})).unwrap();
    let b = app.s.model.bodies[0].id;
    let mut d = Dialog::open(&mut app, Kind::Form(f));
    frame(&mut d, &mut app);
    assert_eq!(d.prompt(false).as_deref(), Some("Select plane or axis for the pull direction"));
    assert_eq!(d.form.values[2], json!("shelled"));
    assert!(!d.ok_enabled());
    let mut d = pick_origin(d, &mut app, OriginItem::Xy);
    frame(&mut d, &mut app);
    assert_eq!((d.form.values[0].clone(), d.form.row), (json!("xy"), 1), "Target Body is active next");
    assert_eq!(app.sel_priority, Priority::Body);
    assert_eq!(d.prompt(false).as_deref(), Some("Select target body"));
    app.s.selection.push(Sel::Body { body: b });
    frame(&mut d, &mut app);
    assert!(d.ok_enabled());
    d.preview(&mut app.s);
    assert!(d.error.is_none(), "{:?}", d.error);
    assert_eq!(app.s.model.bodies.len(), 2);
    let tops: Vec<f64> = app.s.model.bodies.iter().map(|b| b.shape.mass().unwrap().bbox_max.z).collect();
    assert!(tops.iter().any(|z| z.abs() < 1e-3), "one half ends at the equator: {tops:?}");
  }
}
