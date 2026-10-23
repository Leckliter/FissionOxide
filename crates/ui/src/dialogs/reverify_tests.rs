//! Batch ui-reverify-dialogs-1: no Preview check box in Fusion's command dialogs, Chamfer's
//! Corner Type and wording, Hole's chip, prompt, red preview and depth arrow, Move/Copy's
//! Selection row, prompt and units, and Extrude from a body face at 0.00.

use egui::{Pos2, RawInput, Rect, vec2};
use fox_engine::kernel::glam::DVec3;
use fox_engine::{Sel, Session};
use serde_json::json;

use super::*;

/// A 20 mm box at the origin's corner (0..20 on each axis).
fn boxed() -> App {
  let mut app = App::with_session(Session::default());
  app.run("box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]})).unwrap();
  app.s.selection.clear();
  app
}

/// The box's face whose normal is `n`.
fn face(app: &App, n: DVec3) -> Sel {
  let b = &app.s.model.bodies[0];
  Sel::Face { body: b.id, face: b.faces().iter().find(|f| f.normal.dot(n) > 0.99).unwrap().id }
}

/// Draws the whole app for two frames (the dialog sizes itself, then draws), returning the
/// texts of the second.
fn texts(app: &mut App) -> Vec<String> {
  fn walk(s: &egui::Shape, out: &mut Vec<String>) {
    match s {
      egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
      egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
      _ => {}
    }
  }
  let ctx = egui::Context::default();
  let mut all = Vec::new();
  for i in 0..3 {
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
      time: Some(f64::from(i) / 60.0),
      ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| app.draw(ui));
    all.clear();
    for c in &out.shapes {
      walk(&c.shape, &mut all);
    }
    out.drop_without_applying_deltas();
  }
  all
}

/// A text drawn, as shown (a drop-down's icon sits in spaces before its value).
fn has(t: &[String], s: &str) -> bool {
  t.iter().any(|x| x.trim() == s)
}

/// `ui:dialogs:preview-checkbox-extra`: Chamfer, Hole, Combine and Extrude on a body face end
/// with the (i) and OK / Cancel, no Preview check box.
#[test]
fn command_dialogs_have_no_preview_box() {
  let edge = |app: &App| {
    let b = &app.s.model.bodies[0];
    Sel::Edge { body: b.id, edge: b.edges()[0].id }
  };
  for (kind, pick) in [
    (Kind::Chamfer, edge as fn(&App) -> Sel),
    (Kind::Hole, |a: &App| face(a, DVec3::Z)),
    (Kind::Extrude, |a: &App| face(a, DVec3::Z)),
    (Kind::Combine, |a: &App| Sel::Body { body: a.s.model.bodies[0].id }),
  ] {
    let mut app = boxed();
    app.s.selection = vec![pick(&app)];
    app.start(kind);
    let t = texts(&mut app);
    assert!(has(&t, "OK") && !has(&t, "Preview"), "{kind:?}: {t:?}");
  }
}

/// `ui:dialogs:chamfer-fields-2`: Type reads "Equal Distance" (with its icon), Corner Type is
/// live (Chamfer / Miter / Blend) and reaches the command and Edit Feature, and after the pick
/// the prompt asks for the distance, whatever the pointer is over.
#[test]
fn chamfer_corner_type_and_prompt() {
  let mut app = boxed();
  let b = &app.s.model.bodies[0];
  app.s.selection = vec![Sel::Edge { body: b.id, edge: b.edges()[0].id }];
  app.start(Kind::Chamfer);
  let t = texts(&mut app);
  for row in ["Equal Distance", "Corner Type", "Chamfer"] {
    assert!(has(&t, row), "{row}: {t:?}");
  }
  let d = app.dialog.as_mut().unwrap();
  for hovering in [false, true] {
    assert_eq!(d.prompt(hovering).as_deref(), Some("Specify chamfer distance"));
  }
  assert!(d.command().unwrap().1.get("corner").is_none(), "Chamfer is the default");
  d.solid.chamfer_corner = "miter";
  d.dirty = true;
  assert_eq!(d.command().unwrap().1["corner"], "miter");
  let mut d = app.dialog.take().unwrap();
  d.preview(&mut app.s);
  d.end_edit(&mut app.s, true);
  let id = app.s.design.features.last().unwrap().id;
  let e = Dialog::edit(&app, id).expect("Edit Feature");
  assert_eq!(e.solid.chamfer_corner, "miter");
  assert_eq!(e.command().unwrap().1["set"]["corner"], "miter", "{:?}", e.command());
}

/// `ui:dialogs:hole-fields-2`: a hole placed on a face shows Face "1 selected", asks for its
/// reference edges, previews its cut in red and has a depth arrow into the body.
#[test]
fn a_placed_hole_previews_red_with_a_depth_arrow() {
  let mut app = boxed();
  app.s.selection = vec![face(&app, DVec3::Z)];
  app.start(Kind::Hole);
  let t = texts(&mut app);
  assert!(has(&t, "1 selected") && !t.iter().any(|x| x.starts_with("Face of")), "{t:?}");
  let d = app.dialog.as_mut().unwrap();
  assert_eq!(d.prompt(false).as_deref(), Some(HOLE_REFERENCE_PROMPT));
  assert!(d.applied && d.cut_ghost_key() > 0, "the red ghost shows with the preview");
  let m = d.manipulator(&app.s).expect("the depth arrow");
  assert!(m.dir.z < -0.999 && (m.value - 10.0).abs() < 1e-9 && (m.origin.z - 20.0).abs() < 1e-6, "{m:?}");
  d.set_manipulator(6.0, 0.1);
  assert_eq!(d.b, "6 mm");
}

/// `ui:dialogs:move-set-pivot`: MOVE/COPY's second row is Selection, the prompt "Select bodies
/// to move" and the distances 0.00 in the document unit. (The whole-body pick is
/// ui:dialogs:body-pick-highlight's, in body_picks.rs.)
#[test]
fn move_selection_row_prompt_and_units() {
  let mut app = boxed();
  app.start(Kind::Move);
  let t = texts(&mut app);
  assert!(has(&t, "Selection"), "{t:?}");
  let d = app.dialog.as_ref().unwrap();
  assert_eq!(d.prompt(false).as_deref(), Some("Select bodies to move"));
  assert_eq!(d.translate, ["0.00 mm", "0.00 mm", "0.00 mm"]);
  app.cancel_dialog();
  // in inches: 0.00 in
  app.s.design.units = fox_engine::model::LengthUnit::In;
  app.start(Kind::Move);
  assert_eq!(app.dialog.as_ref().unwrap().translate[0], "0.00 in");
}

/// `ui:dialogs:extrude-body-face-pick`: a body face's extrude starts at 0.00 with no preview, OK
/// greyed and the operation as it was; a distance out of the body previews a Join.
#[test]
fn a_body_face_extrude_starts_at_zero() {
  let mut app = boxed();
  app.s.selection = vec![face(&app, DVec3::Z)];
  app.start(Kind::Extrude);
  texts(&mut app);
  let d = app.dialog.as_mut().unwrap();
  assert_eq!((d.distance.as_str(), d.op), ("0.00 mm", "new_body"));
  assert!(!d.applied && !d.ok_enabled(), "nothing previews at 0.00");
  assert!(d.manipulator(&app.s).is_some(), "the arrow is on the face");
  d.distance = "5 mm".into();
  d.dirty = true;
  texts(&mut app);
  let d = app.dialog.as_ref().unwrap();
  assert!(d.applied && d.ok_enabled() && d.op == "join", "{:?} {}", d.error, d.op);
}
