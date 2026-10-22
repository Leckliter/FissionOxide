//! The mesh dialogs as Fusion's: Reduce, Smooth and Convert Mesh forms, and Plane Cut.

use fox_engine::model::BodyId;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::{Dialog, Kind};
use crate::App;
use crate::forms::{self, MESH_CONVERT, MESH_REDUCE, MESH_SMOOTH};

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
}

/// A 10 mm cube standing on the XY plane.
fn cube() -> (Session, BodyId) {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
  run(&mut s, "extrude", json!({"distance": 10}));
  let b = s.model.bodies[0].id;
  (s, b)
}

fn value(d: &Dialog, f: &forms::Form, key: &str) -> Value {
  d.form.values[f.inputs.iter().position(|x| x.key() == key).unwrap()].clone()
}

#[test]
fn reduce_and_smooth_open_as_fusions() {
  let (mut s, b) = cube();
  let f = &MESH_REDUCE;
  let mut d = Dialog::blank(Kind::Form(f));
  assert!(!d.live.on, "Preview starts unticked");
  assert_eq!(f.inputs[0], forms::Input::Bodies { key: "bodies", label: "Body/Face Groups", required: true });
  assert_eq!((value(&d, f, "type"), value(&d, f, "remesh")), (json!("proportion"), json!("adaptive")));
  assert_eq!(value(&d, f, "proportion"), json!("25.00"));
  s.selection = vec![Sel::Body { body: b }];
  d.form.sync(f, &s, None);
  // only the Type's own value goes to the command
  let p = d.form.params(f);
  assert_eq!(p, json!({"bodies": [b.0], "type": "proportion", "remesh": "adaptive", "proportion": "25.00"}));
  assert!(!d.form.shown(f, 3) && d.form.shown(f, 4) && !d.form.shown(f, 5));
  // the prompt names the settings, over the model or not
  for hovering in [false, true] {
    assert_eq!(d.prompt(hovering).as_deref(), Some("Adjust reduce type and settings"));
  }
  d.preview(&mut s);
  assert!(d.applied && d.error.is_none(), "{:?}", d.error);
  assert_eq!(s.design.features.last().unwrap().name, "Reduce1");
  assert!((s.model.bodies[0].shape.mass().unwrap().volume - 1000.0).abs() < 1e-6, "a box stays whole");

  let f = &MESH_SMOOTH;
  let mut d = Dialog::blank(Kind::Form(f));
  assert!(!d.live.on);
  assert_eq!(value(&d, f, "smoothness"), json!("1.00"));
  d.form.sync(f, &s, None);
  assert_eq!(d.prompt(true).as_deref(), Some("Adjust smoothness"));
}

#[test]
fn convert_mesh_has_no_preview_and_makes_a_new_body() {
  let (mut s, b) = cube();
  let f = &MESH_CONVERT;
  let mut d = Dialog::blank(Kind::Form(f));
  assert!(forms::no_preview(f) && !d.live.on);
  assert_eq!((value(&d, f, "operation"), value(&d, f, "method")), (json!("parametric"), json!("faceted")));
  assert!(forms::greyed_option(f, "method", "organic").is_some());
  assert!(forms::greyed_option(f, "method", "prismatic").is_none());
  s.selection = vec![Sel::Body { body: b }];
  d.form.sync(f, &s, None);
  assert_eq!(d.prompt(true).as_deref(), Some("Adjust conversion settings"));
  d.preview(&mut s);
  assert!(d.applied && d.error.is_none(), "{:?}", d.error);
  assert_eq!(s.design.features.last().unwrap().name, "Convert Mesh1");
  assert_eq!(s.model.bodies.len(), 2);
  assert!(!s.body_visible(b));
}

#[test]
fn plane_cut_waits_for_its_plane_then_trims() {
  let (s, b) = cube();
  let mut app = App::with_session(s);
  app.s.selection = vec![Sel::Body { body: b }];
  let mut d = Dialog::open(&mut app, Kind::PlaneCut);
  assert_eq!(d.kind.title(), "PLANE CUT");
  assert!(!d.live.on, "Preview starts unticked");
  assert_eq!(d.bodies, vec![b.0]);
  d.plane_cut_sync(&app);
  assert!(!d.ready() && !d.ok_enabled());
  assert_eq!(d.prompt(false).as_deref(), Some(super::plane_cut::HINT));
  assert!(d.handles(&app.s).is_empty(), "no handles before the plane");
  // a plane picked in the browser or canvas
  app.picked_plane = Some(json!("xy"));
  d.plane_cut_sync(&app);
  assert!(d.ready() && d.ok_enabled());
  assert_eq!(d.plane_cut.distance, "0.00 mm");
  assert_eq!(d.handles(&app.s).len(), 3, "an arrow and two dials");
  d.set_handle(&app.s, 0, 4.0, 0.0);
  assert_eq!(d.len(&app.s, &d.plane_cut.distance), Some(4.0));
  // Trim tints red what goes: the part above the plane
  let (p, removed) = d.plane_cut_removed(&app.s).unwrap();
  assert!(!removed.is_empty() && removed.iter().all(|&i| p[i as usize].z >= 4.0 - 1e-9));
  d.preview(&mut app.s);
  assert!(d.applied && d.error.is_none(), "{:?}", d.error);
  assert_eq!(app.s.design.features.last().unwrap().name, "MeshPlaneCut1");
  assert!((app.s.model.bodies[0].shape.mass().unwrap().volume - 400.0).abs() < 1e-3);
  // Split Body leaves two bodies
  d.plane_cut.cut = "split_body";
  d.preview(&mut app.s);
  assert!(d.error.is_none(), "{:?}", d.error);
  assert_eq!(app.s.model.bodies.len(), 2);
}
