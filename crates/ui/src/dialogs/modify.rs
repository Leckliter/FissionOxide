//! The MODIFY dialogs Fusion lays out with their selection rows first: Draft (Type, the Pull
//! Direction and Faces picks, Tangent Chain and Draft Sides; Flip Pull Direction and Angle once
//! both are picked), Scale (Entities and Point picks and Scale Type; the factor once a body is
//! picked) and Offset Face (Faces; Distance once a face is picked).

use fox_engine::model::{BodyId, FeatureId, FeatureKind};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::icon_row::{self, Choice, choice, greyed};
use super::{Dialog, Kind, chip_text, edge_point, is_planar, sel_bodies, sel_chip, sel_faces};
use crate::icons::Icon;

const DRAFT_TYPES: [Choice; 2] = [
  choice("fixed_plane", Icon::DraftFixedPlane, "Fixed Plane"),
  greyed("parting_line", Icon::DraftPartingLine, "Parting Line"),
];
const DRAFT_SIDES: [(&str, &str); 3] = [("one_side", "One Side"), ("two_side", "Two Side"), ("symmetric", "Symmetric")];
const SCALE_TYPES: [(&str, &str); 2] = [("uniform", "Uniform"), ("non_uniform", "Non Uniform")];

/// Does `text` hold a value other than zero? A value naming a user parameter counts; a half
/// typed one (".", "-") doesn't.
pub(super) fn nonzero(text: &str, kind: fox_engine::expr::Kind) -> bool {
  match fox_engine::expr::eval(text, kind, &fox_engine::expr::Empty) {
    Ok(v) => v.abs() >= 1e-9,
    Err(_) => !text.trim().trim_matches(['.', '-', '+', ' ']).is_empty(),
  }
}

/// The picked point a Scale scales about: a body vertex or a sketch point (world mm).
fn point_of(s: &Session, sel: &Sel) -> Option<[f64; 3]> {
  match *sel {
    Sel::Vertex { body, edge, end } => edge_point(s, &json!({"body": body.0, "edge": edge}), !end),
    Sel::SketchPoint { sketch, point } => {
      let r = s.model.sketches.get(&sketch)?;
      let p = r.plane.to_world(r.sketch.pos(point)?);
      Some([p.x, p.y, p.z])
    }
    _ => None,
  }
}

/// A pull direction's name in the Draft dialog: the origin plane's, the construction plane's
/// or the body the face is on.
pub(super) fn pull_label(s: &Session, v: &Value) -> String {
  if let Some(n) = v.as_str() {
    return format!("{} plane", n.to_uppercase());
  }
  if let Some(f) = v.get("feature").and_then(Value::as_u64) {
    return s.design.feature(FeatureId(f as u32)).map(|f| f.name.clone()).unwrap_or_default();
  }
  match v.get("body").and_then(Value::as_u64) {
    Some(b) => format!("Face of {}", s.body_name(BodyId(b as u32))),
    None => String::new(),
  }
}

/// An angle typed as a bare number, with its unit ("5" reads "5 deg").
fn with_deg(angle: &str) -> String {
  let t = angle.trim();
  if t.parse::<f64>().is_ok() { format!("{t} deg") } else { angle.to_string() }
}

impl Dialog {
  /// Draft's rows, as Fusion's: Type, Pull Direction (active first) and Faces, Tangent Chain and
  /// Draft Sides; once both picks are made, Flip Pull Direction and the angle (two for Two Side).
  pub(super) fn draft_rows(&mut self, ui: &mut egui::Ui, dirty: &mut bool) {
    ui.label("Type");
    icon_row::icon_row(ui, &mut self.choice, &DRAFT_TYPES, "drafting from a parting line isn't in FissionOxide yet");
    ui.end_row();
    let pull = if self.plane.is_null() { "Select" } else { "1 selected" };
    self.pick_row_chip(ui, "Pull Direction", 0, pull, !self.plane.is_null());
    let faces = chip_text(self.faces.len(), "selected");
    self.pick_row_chip(ui, "Faces", 1, &faces, !self.faces.is_empty());
    if self.picks_fixed() {
      // the edited feature keeps the faces it was given
      Self::check_greyed(ui, "Tangent Chain", "The faces of an edited draft stay as they are");
    } else {
      Self::check(ui, "Tangent Chain", &mut self.flag, dirty);
    }
    Self::choose(ui, "Draft Sides", "draft-sides", &mut self.position, &DRAFT_SIDES, dirty);
    if self.plane.is_null() || self.faces.is_empty() {
      return;
    }
    ui.label("Flip Pull Direction");
    if crate::icons::icon_button(ui, Icon::Flip, 24.0, "Flip Pull Direction", self.flip, None).clicked() {
      self.flip = !self.flip;
      *dirty = true;
    }
    ui.end_row();
    Self::field(ui, "Angle", &mut self.angle, dirty);
    if self.position == "two_side" {
      Self::field(ui, "Angle 2", &mut self.angle2, dirty);
    }
  }

  /// The `draft` command's parameters.
  pub(super) fn draft_params(&self) -> Value {
    let mut p = json!({"faces": self.faces, "plane": self.plane, "angle": self.angle, "flip": self.flip,
      "tangent_chain": self.flag});
    match self.position {
      "symmetric" => p["symmetric"] = json!(true),
      "two_side" => p["angle2"] = json!(self.angle2),
      _ => {}
    }
    p
  }

  /// Draft: has an angle left 0.0? (OK and the preview wait for one, as in Fusion.)
  pub(super) fn draft_angle_set(&self) -> bool {
    let angle = fox_engine::expr::Kind::Angle;
    nonzero(&self.angle, angle) || (self.position == "two_side" && nonzero(&self.angle2, angle))
  }

  /// Draft: the picks, routed by the active row. With Pull Direction active, a planar face or
  /// construction plane picked becomes the pull direction and Faces becomes the active row; the
  /// other faces picked are the faces to draft. A pull face picked off again empties its row.
  pub(super) fn sync_draft(&mut self, s: &Session) {
    let picked = sel_faces(s);
    if self.pick_row == 0 {
      let face = picked.iter().rev().find(|f| {
        let get = |k: &str| f.get(k).and_then(Value::as_u64).map(|x| x as u32);
        !self.faces.contains(f)
          && **f != self.plane
          && get("body").zip(get("face")).is_some_and(|(b, x)| is_planar(s, BodyId(b), x))
      });
      let plane = face.cloned().or_else(|| {
        s.selection.iter().rev().find_map(|x| match *x {
          Sel::Feature { feature } if s.model.planes.contains_key(&feature) => {
            Some(json!({"feature": feature.0})).filter(|v| *v != self.plane)
          }
          _ => None,
        })
      });
      if let Some(v) = plane {
        self.set_pull(s, v);
      }
    }
    if self.plane.get("face").is_some() && !picked.contains(&self.plane) {
      self.plane = Value::Null;
      self.plane_label.clear();
      self.pick_row = 0;
    }
    let faces: Vec<Value> = picked.into_iter().filter(|f| *f != self.plane).collect();
    if faces != self.faces {
      self.faces = faces;
    }
  }

  fn set_pull(&mut self, s: &Session, v: Value) {
    self.plane_label = pull_label(s, &v);
    self.plane = v;
    self.pick_row = 1;
  }

  /// Is Draft waiting for its pull direction (so the viewport offers the origin and construction
  /// planes to pick)?
  pub(crate) fn wants_pull_plane(&self) -> bool {
    self.kind == Kind::Draft && !self.picks_fixed() && self.pick_row == 0
  }

  /// Draft: an origin or construction plane picked in the viewport for the pull direction.
  pub(crate) fn pick_pull_plane(&mut self, s: &mut Session, v: Value) {
    self.withdraw(s);
    self.live_reset();
    self.set_pull(s, v);
  }

  /// Draft opened on an existing feature: its values, its picks shown as made.
  pub(super) fn load_draft(&mut self, kind: &FeatureKind) {
    let FeatureKind::Draft { faces, plane, angle, flip, symmetric, angle2 } = kind else { return };
    self.choice = "fixed_plane";
    self.flag = true;
    // a bare stored angle is degrees, and the box shows the unit, as Fusion's "5 deg"
    self.angle = with_deg(angle);
    self.flip = *flip;
    self.position = match (symmetric, angle2) {
      (true, _) => "symmetric",
      (_, Some(_)) => "two_side",
      _ => "one_side",
    };
    self.angle2 = angle2.as_deref().map_or_else(|| "0.0 deg".into(), with_deg);
    self.faces = vec![Value::Null; faces.len()];
    // (the feature keeps its pull direction; the row shows it as picked)
    self.plane = serde_json::to_value(plane).unwrap_or(json!("xy"));
    self.pick_row = 1;
  }

  /// What an edited draft changes.
  pub(super) fn draft_edit_set(&self) -> Value {
    let two = self.position == "two_side";
    json!({"angle": self.angle, "flip": self.flip, "symmetric": self.position == "symmetric",
      "angle2": if two { json!(self.angle2) } else { Value::Null }})
  }

  /// Scale's rows, as Fusion's: Entities (active first) and Point picks, Scale Type, and once a
  /// body is picked the factor (one, or X, Y and Z for Non Uniform).
  pub(super) fn scale_rows(&mut self, ui: &mut egui::Ui, dirty: &mut bool) {
    let entities = chip_text(self.bodies.len(), "selected");
    self.pick_row_chip(ui, "Entities", 0, &entities, !self.bodies.is_empty());
    let point = if self.point.is_some() { "1 selected" } else { "Select" };
    self.pick_row_chip(ui, "Point", 1, point, self.point.is_some());
    Self::choose(ui, "Scale Type", "scale-type", &mut self.choice, &SCALE_TYPES, dirty);
    if self.bodies.is_empty() {
      return;
    }
    if self.choice == "non_uniform" {
      Self::field(ui, "X Scale", &mut self.a, dirty);
      Self::field(ui, "Y Scale", &mut self.b, dirty);
      Self::field(ui, "Z Scale", &mut self.c, dirty);
    } else {
      Self::field(ui, "Scale Factor", &mut self.a, dirty);
    }
  }

  /// Scale: the picks. Bodies (or the bodies of picked faces and edges) are the entities; a
  /// vertex or sketch point is the point, whichever row is active (points are never entities).
  /// The first body picked fills the point with its centre, as Fusion fills the row.
  pub(super) fn sync_scale(&mut self, s: &Session) {
    let bodies = sel_bodies(s);
    if !bodies.is_empty() && bodies != self.bodies {
      self.bodies = bodies;
      self.dirty = true;
    }
    let picked = s.selection.iter().rev().find_map(|x| Some((*x, point_of(s, x)?)));
    if let Some((sel, at)) = picked {
      if self.center != Some(sel) {
        self.center = Some(sel);
        self.point = Some(at);
        self.dirty = true;
      }
    } else if self.point.is_none()
      && let Some(b) = self.bodies.first()
    {
      self.point = super::body_centre(s, *b);
      self.dirty = true;
    }
  }

  /// Offset Face's rows, as Fusion's: Faces, and Distance once a face is picked.
  pub(super) fn offset_face_rows(&mut self, ui: &mut egui::Ui, dirty: &mut bool) {
    ui.label("Faces");
    sel_chip(ui, self.accent(), &chip_text(self.faces.len(), "selected"), !self.faces.is_empty());
    ui.end_row();
    if !self.faces.is_empty() {
      Self::field(ui, "Distance", &mut self.distance, dirty);
    }
  }
}

#[cfg(test)]
mod tests {
  use egui::{Pos2, Rect, vec2};
  use fox_engine::kernel::glam::DVec3;
  use fox_engine::model::{BodyId, FeatureKind};
  use fox_engine::{Sel, Session};
  use serde_json::{Value, json};

  use super::super::{Dialog, Kind};
  use crate::App;

  fn block() -> App {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 20, "depth": 20, "height": 20}));
    app
  }

  fn volume(s: &Session) -> f64 {
    s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
  }

  /// The face whose normal points along `dir`: as a pick and as the dialogs name it.
  fn face(s: &Session, dir: DVec3) -> (Sel, Value) {
    let b = &s.model.bodies[0];
    let f = b.faces().iter().find(|f| f.normal.dot(dir) > 0.9).unwrap();
    (Sel::Face { body: b.id, face: f.id }, json!({"body": b.id.0, "face": f.id}))
  }

  /// Draws the dialog (it syncs its picks and previews, as on screen) and returns every text it
  /// drew.
  fn frame(ctx: &egui::Context, app: &mut App, d: &mut Dialog) -> Vec<String> {
    fn walk(s: &egui::Shape, out: &mut Vec<String>) {
      match s {
        egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
        egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        _ => {}
      }
    }
    let raw = || egui::RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
      ..Default::default()
    };
    // (a new dialog's first frame only sizes it: it draws on the second)
    ctx
      .run_ui(raw(), |ui| {
        d.show(app, ui.ctx());
      })
      .textures_delta
      .clear();
    let mut out = ctx.run_ui(raw(), |ui| {
      d.show(app, ui.ctx());
    });
    out.textures_delta.clear();
    let mut all = Vec::new();
    for c in &out.shapes {
      walk(&c.shape, &mut all);
    }
    all
  }

  fn has(texts: &[String], t: &str) -> bool {
    texts.iter().any(|x| x == t)
  }

  /// Draft opens as Fusion's: Type, Pull Direction (active, "Select pull direction"), Faces,
  /// Tangent Chain, Draft Sides and a greyed OK. A planar face picked first is the pull
  /// direction and Faces takes the next picks; then Flip Pull Direction and Angle 0.0 deg
  /// appear, the prompt asks for the angle, and OK and the preview wait for one.
  #[test]
  fn draft_picks_the_pull_direction_then_faces_then_the_angle() {
    let mut app = block();
    let ctx = egui::Context::default();
    let before = volume(&app.s);
    let mut d = Dialog::open(&mut app, Kind::Draft);
    let t = frame(&ctx, &mut app, &mut d);
    for row in ["DRAFT", "Type", "Pull Direction", "Faces", "Tangent Chain", "Draft Sides", "One Side"] {
      assert!(has(&t, row), "{row}: {t:?}");
    }
    assert!(!has(&t, "Angle") && !has(&t, "Preview") && !has(&t, "XY plane"), "{t:?}");
    assert!(d.plane.is_null() && d.flag && d.wants_pull_plane());
    assert_eq!(d.prompt(false).as_deref(), Some("Select pull direction"));
    assert!(!d.ok_enabled());

    let (top, top_v) = face(&app.s, DVec3::Z);
    app.s.selection = vec![top];
    frame(&ctx, &mut app, &mut d);
    assert_eq!((&d.plane, d.pick_row), (&top_v, 1), "the planar face is the pull direction");
    assert!(d.faces.is_empty() && !d.wants_pull_plane());
    assert_eq!(d.prompt(false).as_deref(), Some("Select faces to draft"));

    let (side, side_v) = face(&app.s, -DVec3::Y);
    app.s.selection.push(side);
    let t = frame(&ctx, &mut app, &mut d);
    assert_eq!(d.faces, vec![side_v]);
    for row in ["Flip Pull Direction", "Angle", "0.0 deg"] {
      assert!(has(&t, row), "{row}: {t:?}");
    }
    assert_eq!(d.prompt(true).as_deref(), Some("Specify draft angle"), "whatever the pointer is over");
    assert!(!d.ok_enabled() && !d.applied, "0.0 deg: OK and the preview wait");
    // (the viewport brings the handles up to date every frame)
    d.refresh_manip(&app.s);
    let (at, pull) = d.draft_pull().expect("the pull direction on the top face");
    assert!((pull - DVec3::Z).length() < 1e-9 && (at.z - 20.0).abs() < 1e-6, "{at} {pull}");
    assert_eq!(d.handles(&app.s).len(), 2, "the angle disc and the pull arrow");

    d.angle = "5 deg".into();
    frame(&ctx, &mut app, &mut d);
    assert!(d.applied && d.error.is_none() && d.ok_enabled(), "{:?}", d.error);
    assert!((volume(&app.s) - before).abs() > 1.0, "{} {before}", volume(&app.s));
    assert_eq!(d.command().unwrap().1["tangent_chain"], true);
    // the pull face stays picked (blue) on the preview
    let top_now = app.s.selection.iter().any(|x| match *x {
      Sel::Face { body, face } => {
        app.s.model.body(body).unwrap().faces().iter().any(|f| f.id == face && f.normal.z > 0.9)
      }
      _ => false,
    });
    assert!(top_now, "{:?}", app.s.selection);
    // and so does the face it drafts, tilted now (both picks blue, as in Fusion)
    let tilted = app.s.selection.iter().any(|x| match *x {
      Sel::Face { body, face } => app
        .s
        .model
        .body(body)
        .unwrap()
        .faces()
        .iter()
        .any(|f| f.id == face && f.normal.y < -0.9 && f.normal.z.abs() > 1e-3),
      _ => false,
    });
    assert!(tilted && app.s.selection.len() == 2, "{:?}", app.s.selection);
    // a bare number typed in the Angle row is degrees, shown as Fusion shows it
    assert_eq!(crate::units::typed_angle("5"), "5.0 deg");
    // Flip Pull Direction turns the pull round
    d.flip = true;
    assert!((d.draft_pull().unwrap().1 + DVec3::Z).length() < 1e-9);
    d.cancel(&mut app.s);
    assert!((volume(&app.s) - before).abs() < 1e-6);
  }

  /// Draft's Pull Direction row also takes an origin plane, picked in the viewport.
  #[test]
  fn draft_takes_an_origin_plane_for_the_pull_direction() {
    let mut app = block();
    let mut d = Dialog::open(&mut app, Kind::Draft);
    assert!(d.wants_pull_plane());
    d.pick_pull_plane(&mut app.s, json!("xz"));
    assert_eq!((&d.plane, d.plane_label.as_str(), d.pick_row), (&json!("xz"), "XZ plane", 1));
    assert!(!d.wants_pull_plane());
    // a click on the Pull Direction chip makes it the active row again
    d.pick_row = 0;
    assert!(d.wants_pull_plane());
    // Draft Sides: Symmetric and Two Side reach the command
    d.position = "two_side";
    d.angle2 = "2 deg".into();
    assert_eq!(d.draft_params()["angle2"], "2 deg");
    d.position = "symmetric";
    assert_eq!(d.draft_params()["symmetric"], true);
  }

  /// Double-clicking a Draft opens EDIT FEATURE with its rows filled: both picks shown as made
  /// and its angle, which OK changes.
  #[test]
  fn draft_is_editable() {
    let mut app = block();
    let (_, top) = face(&app.s, DVec3::Z);
    let (_, side) = face(&app.s, -DVec3::Y);
    app.run("draft", json!({"faces": [side], "plane": top, "angle": "5 deg"}));
    let id = app.s.design.features.last().unwrap().id;
    crate::panels::edit_feature(&mut app, id);
    assert!(app.status.is_none(), "{:?}", app.status);
    let mut d = app.dialog.take().expect("EDIT FEATURE");
    assert_eq!((d.kind, d.editing, d.angle.as_str(), d.faces.len()), (Kind::Draft, Some(id), "5 deg", 1));
    // the picks found again on the rolled-back body: both blue, the pull arrow and angle dial on
    // the model, Tangent Chain live
    assert!(d.repicks && !d.picks_fixed());
    assert_eq!(d.faces, vec![json!({"body": side["body"], "face": side["face"]})]);
    // the pull face is selected (the drafted face, changed by the preview, is found again by
    // live.rs's drafted-face matching)
    let pull =
      Sel::Face { body: BodyId(top["body"].as_u64().unwrap() as u32), face: top["face"].as_u64().unwrap() as u32 };
    assert!(app.s.selection.contains(&pull), "{:?}", app.s.selection);
    assert_eq!(d.picked_selection().len(), 2, "both picks");
    let ctx = egui::Context::default();
    let t = frame(&ctx, &mut app, &mut d);
    assert!(has(&t, "EDIT FEATURE") && has(&t, "Angle") && has(&t, "5 deg"), "{t:?}");
    assert_eq!(t.iter().filter(|x| *x == "1 selected").count(), 2, "{t:?}");
    assert!(d.draft_pull().is_some(), "the pull arrow");
    assert_eq!(d.handles(&app.s).len(), 2, "the angle dial and the pull arrow");
    assert!(d.ok_enabled());
    d.angle = "8 deg".into();
    d.position = "symmetric";
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    let FeatureKind::Draft { angle, symmetric, .. } = &app.s.design.feature(id).unwrap().kind else { panic!() };
    assert_eq!((angle.as_str(), *symmetric), ("8 deg", true));
    // a draft whose angle was typed bare reopens with its unit
    let mut app = block();
    let (_, top) = face(&app.s, DVec3::Z);
    let (_, side) = face(&app.s, -DVec3::Y);
    app.run("draft", json!({"faces": [side], "plane": top, "angle": "5"}));
    let id = app.s.design.features.last().unwrap().id;
    crate::panels::edit_feature(&mut app, id);
    assert_eq!(app.dialog.as_ref().map(|d| d.angle.as_str()), Some("5 deg"));
  }

  /// Scale opens as Fusion's: Entities (active, "Select bodies or sketches to scale"), Point and
  /// Scale Type, OK greyed and nothing scaled. Picking a body fills Point, adds Scale Factor
  /// 1.00 with an arrow and its floating box, and enables OK.
  #[test]
  fn scale_waits_for_a_body_then_fills_the_point_and_factor() {
    let mut app = block();
    let ctx = egui::Context::default();
    let before = volume(&app.s);
    let mut d = Dialog::open(&mut app, Kind::Scale);
    let t = frame(&ctx, &mut app, &mut d);
    for row in ["SCALE", "Entities", "Point", "Scale Type", "Uniform"] {
      assert!(has(&t, row), "{row}: {t:?}");
    }
    assert!(!has(&t, "Scale Factor"), "{t:?}");
    assert!(d.bodies.is_empty() && d.point.is_none() && !d.ok_enabled() && !d.applied);
    assert_eq!(d.prompt(false).as_deref(), Some("Select bodies or sketches to scale"));
    assert!((volume(&app.s) - before).abs() < 1e-6, "no body is scaled before a pick");

    let body = app.s.model.bodies[0].id;
    app.s.selection = vec![Sel::Body { body }];
    let t = frame(&ctx, &mut app, &mut d);
    assert_eq!(d.bodies, vec![body.0]);
    assert!(d.point.is_some() && d.ok_enabled());
    assert!(has(&t, "Scale Factor") && has(&t, "1.00") && t.iter().filter(|x| *x == "1 selected").count() == 2);
    d.refresh_manip(&app.s);
    let m = d.manipulator(&app.s).expect("the scale arrow");
    assert!(m.value > 1.0);
    assert_eq!(d.value_box_field().cloned().as_deref(), Some("1.00"));
    // dragged to twice its length: a factor of 2.00
    d.set_manipulator(2.0 * m.value, 0.0);
    assert_eq!(d.a, "2.00");
    frame(&ctx, &mut app, &mut d);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert!((volume(&app.s) - 8.0 * before).abs() < 1e-3, "{}", volume(&app.s));

    // Non Uniform: a factor per axis
    d.choice = "non_uniform";
    d.a = "2".into();
    d.b = "1".into();
    d.c = "1".into();
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, &p["factors"]), ("scale", &json!(["2", "1", "1"])));
    assert!(d.manipulator(&app.s).is_none() && d.value_box_field().is_none());
    d.cancel(&mut app.s);
    assert!((volume(&app.s) - before).abs() < 1e-6);
  }

  /// Scale's point: a vertex picked is the point, whichever row is active.
  #[test]
  fn scale_takes_a_picked_vertex_as_its_point() {
    let mut app = block();
    let body = app.s.model.bodies[0].id;
    let e = app.s.model.bodies[0].edges()[0];
    app.s.selection = vec![Sel::Body { body }];
    let mut d = Dialog::open(&mut app, Kind::Scale);
    assert_eq!(d.bodies, vec![body.0]);
    app.s.selection.push(Sel::Vertex { body: BodyId(body.0), edge: e.id, end: false });
    d.sync_selection(&app.s);
    assert_eq!(d.point, Some([e.start.x, e.start.y, e.start.z]));
    assert_eq!(d.command().unwrap().1["center"], json!([e.start.x, e.start.y, e.start.z]));
  }

  /// Offset Face opens with Faces alone ("Select face(s) to offset"); a pick adds Distance 0.00
  /// with an arrow on the face and its floating box, and OK waits for a distance.
  #[test]
  fn offset_face_adds_its_distance_after_the_pick() {
    let mut app = block();
    let ctx = egui::Context::default();
    let before = volume(&app.s);
    let mut d = Dialog::open(&mut app, Kind::OffsetFace);
    let t = frame(&ctx, &mut app, &mut d);
    assert!(has(&t, "OFFSET FACE") && has(&t, "Faces"), "{t:?}");
    assert!(!has(&t, "Distance") && !has(&t, "Preview"), "{t:?}");
    assert_eq!(d.prompt(false).as_deref(), Some("Select face(s) to offset"));
    assert_eq!(d.distance, "0.00 mm");

    let (top, _) = face(&app.s, DVec3::Z);
    app.s.selection = vec![top];
    let t = frame(&ctx, &mut app, &mut d);
    assert!(has(&t, "Distance") && has(&t, "0.00 mm"), "{t:?}");
    assert!(!d.ok_enabled() && !d.applied, "0.00: OK waits for a distance");
    d.refresh_manip(&app.s);
    let m = d.manipulator(&app.s).expect("the distance arrow on the face");
    assert!(m.value.abs() < 1e-9 && (m.dir - DVec3::Z).length() < 1e-9);
    assert_eq!(d.value_box_field().cloned().as_deref(), Some("0.00 mm"));
    d.set_manipulator(3.0, 0.5);
    assert_eq!(d.distance, "3 mm");
    frame(&ctx, &mut app, &mut d);
    assert!(d.applied && d.error.is_none() && d.ok_enabled(), "{:?}", d.error);
    assert!((volume(&app.s) - before * 23.0 / 20.0).abs() < 1e-3, "{}", volume(&app.s));
  }
}
