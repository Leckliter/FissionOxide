//! Edit Feature on a Hole, Mirror, Combine and Press Pull (Offset Face): the creation dialog
//! filled from what the feature stores, as Fusion's EDIT FEATURE is, and the fields
//! `feature.edit` takes back. Also what an opened edit selects in the canvas (the bodies Mirror,
//! Combine and Move act on, drawn blue as in Fusion).

use fox_engine::kernel::glam::DVec3;
use fox_engine::model::{BodyId, BodySel, FeatureKind, PlaneRef};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::{Dialog, Kind, Manipulator, op_str};

/// The bodies a body selection names now (every body for "all").
fn body_ids(s: &Session, sel: &BodySel) -> Vec<u32> {
  match sel {
    BodySel::Bodies { ids } => ids.iter().map(|b| b.0).collect(),
    BodySel::All => s.model.bodies.iter().map(|b| b.id.0).collect(),
    BodySel::Component { id } => {
      s.design.component(*id).map(|c| c.bodies.iter().map(|b| b.0).collect()).unwrap_or_default()
    }
  }
}

/// The edit dialog of a Hole, Mirror, Combine or Offset Face feature.
pub(super) fn load(s: &Session, kind: &FeatureKind) -> Option<Dialog> {
  Some(match kind {
    FeatureKind::Hole { .. } => {
      let mut d = Dialog::blank(Kind::Hole);
      d.load_hole(s, kind);
      d
    }
    FeatureKind::Mirror { bodies, plane, features, join } => {
      let mut d = Dialog::blank(Kind::Mirror);
      d.choice = if features.is_empty() { "bodies" } else { "features" };
      d.mirror_features = features.iter().map(|f| f.0).collect();
      d.bodies = if features.is_empty() { body_ids(s, bodies) } else { Vec::new() };
      d.join = *join;
      d.plane =
        super::edit_picks::plane_pick(s, plane).unwrap_or_else(|| serde_json::to_value(plane).unwrap_or_default());
      d.plane_label = "1 selected".into();
      d
    }
    FeatureKind::Combine { target, tools, operation, keep_tools } => {
      let mut d = Dialog::blank(Kind::Combine);
      d.target = Some(target.0);
      d.bodies = tools.iter().map(|b| b.0).collect();
      d.pick_row = 1;
      d.op = op_str(*operation);
      d.keep_tools = *keep_tools;
      d
    }
    FeatureKind::OffsetFaces { faces, distance } => {
      // Press Pull on a face is Fusion's Offset Face: its faces are what it stored
      let mut d = Dialog::blank(Kind::PressPull);
      d.faces = vec![Value::Null; faces.len()];
      d.distance = distance.clone();
      d.choice = "automatic";
      d
    }
    _ => return None,
  })
}

impl Dialog {
  /// Fields for `feature.edit` on a Hole, Mirror, Combine or Offset Face.
  pub(super) fn more_edit_set(&self) -> Option<Value> {
    Some(match self.kind {
      Kind::Hole => self.hole_edit_set(),
      Kind::Mirror if self.choice == "features" => json!({"features": self.mirror_features, "join": self.join}),
      Kind::Mirror => json!({"features": [], "join": self.join}),
      Kind::Combine => json!({"operation": self.op, "keep_tools": self.keep_tools}),
      Kind::PressPull => json!({"distance": self.distance}),
      _ => return None,
    })
  }

  /// What an edit selects when it opens: the bodies it moves, mirrors or combines (those that
  /// exist at this point of the timeline).
  pub(crate) fn edit_selection(&self, s: &Session) -> Option<Vec<Sel>> {
    self.editing?;
    // a hole's face is its pick (blue under the glyph), not its body
    if self.kind == Kind::Hole {
      return Some(self.hole_face(s).into_iter().collect());
    }
    let picks = match self.kind {
      Kind::Move if self.mv.object == "bodies" => &self.bodies,
      Kind::Mirror if self.choice == "bodies" => &self.bodies,
      Kind::Mirror => &Vec::new(),
      Kind::Combine => &self.target.iter().chain(&self.bodies).copied().collect(),
      _ => return None,
    };
    let mut sel: Vec<Sel> =
      picks.iter().filter(|b| s.model.body(BodyId(**b)).is_some()).map(|b| Sel::Body { body: BodyId(*b) }).collect();
    // Mirror's plane is drawn selected too (a face or construction plane; an origin plane draws
    // blue among the origin planes)
    if self.kind == Kind::Mirror {
      sel.extend(super::edit_picks::plane_sel(&self.plane).filter(|p| match p {
        Sel::Face { body, .. } => s.model.body(*body).is_some(),
        _ => true,
      }));
    }
    (!sel.is_empty()).then_some(sel)
  }
}

impl Dialog {
  /// The face an edited single hole sits on (its placement sketch's plane), on the model as it
  /// is now.
  pub(super) fn hole_face(&self, s: &Session) -> Option<Sel> {
    if !self.solid.hp.single {
      return None;
    }
    let FeatureKind::Sketch { plane: PlaneRef::Face { face, .. }, .. } = &s.design.feature(self.sketch?)?.kind else {
      return None;
    };
    Some(Sel::Face { body: face.body, face: s.model.body(face.body)?.find_face(face)? })
  }

  /// Where an edited hole sits, on the model as it is now: its (first) placement point and its
  /// sketch's normal, which the hole drills against (along it, flipped).
  pub(crate) fn hole_edit_anchor(&self, s: &Session) -> Option<(DVec3, DVec3)> {
    if self.kind != Kind::Hole {
      return None;
    }
    let FeatureKind::Hole { sketch, points, .. } = &s.design.feature(self.editing?)?.kind else { return None };
    let r = s.model.sketches.get(sketch)?;
    let used: Vec<_> = r.sketch.curves.iter().flat_map(|c| r.sketch.curve_points(c.id)).collect();
    let point = match points.first() {
      Some(p) => *p,
      None => r.sketch.points.iter().map(|p| p.id).find(|p| *p != fox_engine::sketch::ORIGIN && !used.contains(p))?,
    };
    Some((r.plane.to_world(r.sketch.pos(point)?), r.plane.normal()))
  }

  /// Where the Hole dialog draws its position glyph: a single hole's centre on its face (an
  /// edited hole's placement point).
  pub(crate) fn hole_glyph(&self, s: &Session) -> Option<DVec3> {
    if self.kind != Kind::Hole {
      return None;
    }
    if self.editing.is_some() {
      return self.hole_edit_anchor(s).map(|(c, _)| c);
    }
    if self.solid.hp.single { self.hole_centre(s) } else { None }
  }

  /// Edit Hole's depth arrow: from the hole's centre on its face, into the material, as deep as
  /// the hole (none for Through All).
  pub(super) fn hole_edit_manipulator(&self, s: &Session) -> Option<Manipulator> {
    if self.flag {
      return None;
    }
    let (origin, n) = self.hole_edit_anchor(s)?;
    let dir = if self.flip { n } else { -n };
    Some(Manipulator { origin, dir, value: self.len(s, &self.b)? })
  }

  /// Does the dialog show the origin planes (whatever the Origin folder's eyes say)? Edit Hole
  /// does, as Fusion's.
  pub(crate) fn shows_origin_planes(&self) -> bool {
    self.kind == Kind::Hole && self.editing.is_some()
  }
}

#[cfg(test)]
mod tests {
  use egui::{Pos2, Rect, vec2};
  use fox_engine::kernel::glam::DVec3;
  use fox_engine::model::FeatureId;
  use fox_engine::{Sel, Session};
  use serde_json::{Value, json};

  use super::super::Kind;
  use crate::App;

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  fn volume(s: &Session) -> f64 {
    s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
  }

  fn bbox(s: &Session, i: usize) -> (DVec3, DVec3) {
    let m = s.model.bodies[i].shape.mass().unwrap();
    (m.bbox_min, m.bbox_max)
  }

  fn close(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-4
  }

  /// Double-clicks the last timeline feature: the app with its edit dialog open.
  fn edit_last(s: Session) -> (App, FeatureId) {
    let mut app = App::with_session(s);
    let id = app.s.design.features.last().unwrap().id;
    crate::panels::edit_feature(&mut app, id);
    assert!(app.dialog.is_some(), "{:?}", app.status);
    (app, id)
  }

  /// Applies the open edit as a preview, then OK's it (the preview stays).
  fn preview(app: &mut App) {
    let mut d = app.dialog.take().unwrap();
    d.refresh_manip(&app.s);
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}: {:?} {:?}", d.kind, d.error, d.command());
    app.dialog = Some(d);
  }

  /// The text the open dialog draws.
  fn texts(app: &mut App) -> Vec<String> {
    fn walk(s: &egui::Shape, out: &mut Vec<String>) {
      match s {
        egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
        egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        _ => {}
      }
    }
    let ctx = egui::Context::default();
    let mut d = app.dialog.take().unwrap();
    let mut all = Vec::new();
    for _ in 0..2 {
      all.clear();
      let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 900.0))),
        ..Default::default()
      };
      let out = ctx.run_ui(raw, |ui| {
        d.show(app, ui.ctx());
      });
      for c in &out.shapes {
        walk(&c.shape, &mut all);
      }
      out.drop_without_applying_deltas();
    }
    app.dialog = Some(d);
    all
  }

  fn block(s: &mut Session) -> (u32, u32) {
    run(s, "sketch.create", json!({"plane": "xy"}));
    run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 50]}));
    run(s, "extrude", json!({"distance": 10}));
    let b = &s.model.bodies[0];
    (b.id.0, b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id)
  }

  #[test]
  fn hole_edits_with_its_face_references_and_shape() {
    let mut s = Session::default();
    let (body, top) = block(&mut s);
    let edge = |s: &Session, axis: usize| {
      let b = &s.model.bodies[0];
      let e = b.edges().iter().find(|e| {
        let d = e.end - e.start;
        e.start.z > 9.9 && e.end.z > 9.9 && d[axis].abs() > 1.0 && e.start[1 - axis].abs() < 1e-6
      });
      json!({"body": body, "edge": e.unwrap().id})
    };
    let (front, left) = (edge(&s, 0), edge(&s, 1));
    run(
      &mut s,
      "hole",
      json!({
        "face": {"body": body, "face": top}, "at": [20, 20, 10], "diameter": 6, "depth": 5,
        "offsets": [{"edge": front, "distance": 15}, {"edge": left, "distance": 25}], "kind": "counterbore",
        "head_diameter": 10, "head_depth": 2,
      }),
    );
    let before = volume(&s);
    let (mut app, _) = edit_last(s);
    // the face it sits on is the pick, not the whole body; the origin planes show
    let [Sel::Face { body: b, face }] = app.s.selection[..] else { panic!("{:?}", app.s.selection) };
    let normal = app.s.model.body(b).unwrap().faces().iter().find(|f| f.id == face).unwrap().normal;
    assert!(b.0 == body && normal.z > 0.9, "the top face: {normal:?}");
    assert!(crate::origin::OriginItem::PLANES.iter().all(|p| crate::origin::visible(&app, *p)));
    assert!(!crate::origin::visible(&app, crate::origin::OriginItem::X));
    // the position glyph at its centre, the depth arrow down into the block, as deep as the hole
    let d = app.dialog.as_ref().unwrap();
    let g = d.hole_glyph(&app.s).expect("the glyph");
    // (its offsets hold it 25 from the left edge and 15 from the front)
    let c = DVec3::new(25.0, 15.0, 10.0);
    assert!(close(g, c), "{g:?}");
    let m = d.manipulator(&app.s).expect("the depth arrow");
    assert!(close(m.origin, c) && close(m.dir, -DVec3::Z), "{:?} {:?}", m.origin, m.dir);
    assert!((m.value - 5.0).abs() < 1e-9);
    let d = app.dialog.as_ref().unwrap();
    assert_eq!((d.kind, d.choice, d.solid.hp.single), (Kind::Hole, "counterbore", true));
    assert_eq!((d.faces.len(), d.edges.len()), (1, 2));
    // the values come back with their unit
    assert_eq!([&d.a, &d.b, &d.c, &d.d], ["6 mm", "5 mm", "10 mm", "2 mm"]);
    assert!(!d.flag && !d.solid.hp.flat);
    let t = texts(&mut app);
    for want in ["Placement", "Face", "1 selected", "Reference 1", "Reference 2", "Shape Settings", "Hole Type"] {
      assert!(t.iter().any(|x| x == want), "no {want:?} in {t:?}");
    }
    assert!(!t.iter().any(|x| x.contains("no editable")));
    // a wider hole, moved by its offsets: one preview, one feature.edit
    let d = app.dialog.as_mut().unwrap();
    let set = d.edit_set().unwrap();
    assert_eq!(set["offsets"].as_array().map(Vec::len), Some(2), "{set}");
    d.a = "8 mm".into();
    d.solid.hp.flat = true;
    let n = app.s.design.features.len();
    preview(&mut app);
    assert_eq!(app.s.design.features.len(), n);
    assert!(volume(&app.s) < before - 1.0, "{} {before}", volume(&app.s));
    let hole = app.s.design.features.last().unwrap();
    let fox_engine::model::FeatureKind::Hole { diameter, tip_angle, .. } = &hole.kind else { panic!() };
    assert_eq!((diameter.as_str(), tip_angle.as_str()), ("8 mm", "180 deg"));
    // Cancel takes it back
    let mut d = app.dialog.take().unwrap();
    d.cancel(&mut app.s);
    assert!((volume(&app.s) - before).abs() < 1e-6);
  }

  #[test]
  fn mirror_edits_with_its_objects_plane_and_join_and_selects_the_source() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10, "at": [5, 0]}));
    let b = s.model.bodies[0].id;
    run(&mut s, "mirror", json!({"plane": "yz", "bodies": [b.0], "join": false}));
    assert_eq!(s.model.bodies.len(), 2);
    let (mut app, _) = edit_last(s);
    assert_eq!(app.s.selection, vec![Sel::Body { body: b }], "the source body is drawn selected");
    let d = app.dialog.as_ref().unwrap();
    assert_eq!((d.kind, d.choice, d.bodies.clone(), d.join), (Kind::Mirror, "bodies", vec![b.0], false));
    // the mirrored body previews as a see-through ghost; the plane is shown and drawn blue
    let ghosts = d.ghost_bodies();
    assert_eq!(ghosts.len(), 1, "{ghosts:?}");
    assert!(ghosts[0] != b);
    use crate::origin::{Look, OriginItem, look, visible};
    assert_eq!(d.mirror_origin_plane(), Some(OriginItem::Yz));
    assert!(visible(&app, OriginItem::Yz) && !visible(&app, OriginItem::Xy));
    assert_eq!(look(&app, OriginItem::Yz), Look::Selected);
    assert_eq!(crate::panels::selection_text(&app).as_deref(), Some("2 selections"));
    // Fusion's rows: Object Type, Objects, Mirror Plane, Operation
    let t = texts(&mut app);
    assert_eq!(t.iter().filter(|x| *x == "1 selected").count(), 2, "Objects and Mirror Plane chips: {t:?}");
    for want in ["EDIT FEATURE", "Object Type", "Objects", "Mirror Plane", "Operation", "New Body"] {
      assert!(t.iter().any(|x| x == want), "no {want:?} in {t:?}");
    }
    assert!(!t.iter().any(|x| x == "Join to original" || x == "Plane" || x == "Type"), "{t:?}");
    // joined to the original, the two become one body
    app.dialog.as_mut().unwrap().join = true;
    preview(&mut app);
    assert_eq!(app.s.model.bodies.len(), 1);
    assert!((volume(&app.s) - 2000.0).abs() < 1e-6);
  }

  #[test]
  fn combine_edits_its_operation_and_keep_tools() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 10}));
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 30, "operation": "new_body"}));
    let (t, tool) = (s.model.bodies[0].id.0, s.model.bodies[1].id.0);
    run(&mut s, "combine", json!({"target": t, "tools": [tool], "operation": "join"}));
    let (mut app, _) = edit_last(s);
    let d = app.dialog.as_ref().unwrap();
    assert_eq!(
      (d.kind, d.target, d.bodies.clone(), d.op, d.keep_tools),
      (Kind::Combine, Some(t), vec![tool], "join", false)
    );
    // rolled back: target and tool apart, both selected, the browser's rows as before the combine
    let both =
      vec![Sel::Body { body: fox_engine::model::BodyId(t) }, Sel::Body { body: fox_engine::model::BodyId(tool) }];
    assert_eq!(app.s.selection, both);
    assert_eq!(app.s.model.bodies.len(), 2);
    assert_eq!(crate::panels::selection_text(&app).as_deref(), Some("2 Bodies"));
    assert!(app.dialog.as_ref().unwrap().browser_bodies(&app.s).is_none(), "the browser lists the model's bodies");
    texts(&mut app);
    assert_eq!(app.s.selection, both, "a frame keeps both picks");
    let d = app.dialog.as_mut().unwrap();
    d.op = "cut";
    d.keep_tools = true;
    preview(&mut app);
    // the target with the tool's column cut out, and the tool kept
    assert_eq!(app.s.model.bodies.len(), 2);
    assert!((volume(&app.s) - (4000.0 - 1000.0 + 3000.0)).abs() < 1e-6, "{}", volume(&app.s));
  }

  #[test]
  fn press_pull_edits_as_offset_face() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
    run(&mut s, "press_pull", json!({"faces": [{"point": [0, 0, 10]}], "distance": 5}));
    let (mut app, _) = edit_last(s);
    let d = app.dialog.as_ref().unwrap();
    assert_eq!((d.kind, d.faces.len(), d.distance.as_str()), (Kind::PressPull, 1, "5 mm"));
    let t = texts(&mut app);
    for want in ["EDIT FEATURE", "Faces", "1 selected", "Offset Type", "Distance"] {
      assert!(t.iter().any(|x| x == want), "no {want:?} in {t:?}");
    }
    app.dialog.as_mut().unwrap().distance = "10 mm".into();
    preview(&mut app);
    assert!((bbox(&app.s, 0).1.z - 20.0).abs() < 1e-6);
  }

  #[test]
  fn edit_move_adds_to_the_stored_move() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
    let b = s.model.bodies[0].id;
    run(&mut s, "move", json!({"bodies": [b.0], "translate": [10, 0, 0]}));
    let (mut app, _) = edit_last(s);
    assert_eq!(app.s.selection, vec![Sel::Body { body: b }], "the moved body is drawn selected");
    let d = app.dialog.as_mut().unwrap();
    assert_eq!((d.mv.object, d.mv.mtype, d.bodies.clone()), ("bodies", "free", vec![b.0]));
    // all six boxes start at zero
    assert_eq!(d.translate, ["0 mm", "0 mm", "0 mm"]);
    assert_eq!(d.mv.rotate, ["0 deg", "0 deg", "0 deg"]);
    d.refresh_manip(&app.s);
    let t = d.triad(&app.s).expect("the triad on the body");
    assert!(close(t.origin, DVec3::new(10.0, 0.0, 5.0)), "{:?}", t.origin);
    let texts = texts(&mut app);
    for want in ["Move Object", "Selection", "1 selected", "Move Type", "Set Pivot", "X Angle", "Z Distance"] {
      assert!(texts.iter().any(|x| x == want), "no {want:?} in {texts:?}");
    }
    assert!(!texts.iter().any(|x| x == "Create Copy" || x == "Bodies"), "{texts:?}");
    // nothing added keeps the stored move as it is
    preview(&mut app);
    let d = app.dialog.as_mut().unwrap();
    assert_eq!(d.edit_set().unwrap()["translate"], json!(["10", "0", "0"]));
    assert!(close(bbox(&app.s, 0).0, DVec3::new(5.0, -5.0, 0.0)));
    // a further 5 along X and a quarter turn about Z add to it
    d.translate[0] = "5 mm".into();
    d.mv.rotate[2] = "90 deg".into();
    d.dirty = true;
    preview(&mut app);
    let (lo, hi) = bbox(&app.s, 0);
    assert!(close(lo, DVec3::new(10.0, -5.0, 0.0)) && close(hi, DVec3::new(20.0, 5.0, 10.0)), "{lo:?} {hi:?}");
    // a long box shows the turn
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 10, "height": 10}));
    let b = s.model.bodies[0].id;
    run(&mut s, "move", json!({"bodies": [b.0], "translate": [0, 0, 5]}));
    let (mut app, _) = edit_last(s);
    let d = app.dialog.as_mut().unwrap();
    d.refresh_manip(&app.s);
    d.mv.rotate[2] = "90 deg".into();
    preview(&mut app);
    let (lo, hi) = bbox(&app.s, 0);
    assert!(close(lo, DVec3::new(-5.0, -10.0, 5.0)) && close(hi, DVec3::new(5.0, 10.0, 15.0)), "{lo:?} {hi:?}");
  }

  #[test]
  fn edited_values_show_their_unit() {
    // a chamfer typed "2" in a mm design reopens as "2 mm"
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
    let (b, e) = (s.model.bodies[0].id.0, s.model.bodies[0].edges()[0].id);
    run(&mut s, "chamfer", json!({"edges": [{"body": b, "edge": e}], "distance": "2", "distance2": "3"}));
    let (mut app, _) = edit_last(s);
    let d = app.dialog.as_mut().unwrap();
    assert_eq!((d.radius.as_str(), d.solid.distance2.as_str()), ("2 mm", "3 mm"));
    assert_eq!(d.value_box_field().map(|t| t.clone()), Some("2 mm".to_string()));
    // a box's sizes too
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": "10 * 2", "height": "1 in"}));
    let (app, _) = edit_last(s);
    let d = app.dialog.as_ref().unwrap();
    assert_eq!([&d.a, &d.b, &d.c], ["20 mm", "(10 * 2) mm", "1 in"]);
  }
}
