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
  assert!(!d.live.on);
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

const LPRISM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/reference/464-stl-millimeter/lprism.stl");

/// A body's volume and bounding box.
fn size(app: &App, b: BodyId) -> (f64, fox_engine::kernel::glam::DVec3, fox_engine::kernel::glam::DVec3) {
  let m = app.s.model.body(b).unwrap().shape.mass().unwrap();
  (m.volume, m.bbox_min, m.bbox_max)
}

/// `ui:mesh:{reduce,smooth,remesh,shell,combine}-dialog` (verifier 6 on an inserted
/// `lprism.stl`): the dialogs open without previewing, as Fusion's do with Preview unticked
/// (the model and the timeline stay as they were), and end with the Preview row; ticking it
/// previews, unticking it withdraws the preview. Remesh at its defaults keeps the prism's shape.
#[test]
fn mesh_dialogs_preview_only_once_preview_is_ticked() {
  use crate::forms::{MESH_COMBINE, MESH_REMESH, MESH_SHELL};
  let mut app = App::with_session(Session::default());
  app.run("import", json!({"path": LPRISM})).unwrap();
  let b = app.s.model.bodies[0].id;
  let tris = |app: &App| app.s.model.body(b).unwrap().mesh().unwrap().indices.len() / 3;
  let (v0, lo0, hi0) = size(&app, b);
  let t0 = tris(&app);
  let n0 = app.s.design.features.len();
  for f in [&MESH_REDUCE, &MESH_SMOOTH, &MESH_REMESH, &MESH_SHELL, &MESH_COMBINE] {
    assert!(forms::preview_row(f), "{} ends with Preview", f.name);
    app.s.selection = vec![Sel::Body { body: b }];
    app.start(Kind::Form(f));
    let d = app.dialog.as_mut().unwrap();
    assert!(!d.live.on && !d.applied && !d.has_preview_box(), "{}: Preview unticked, in the rows", f.name);
    d.live_check();
    assert!(!d.dirty, "{}: nothing previews while Preview is unticked", f.name);
    assert_eq!((app.s.design.features.len(), tris(&app)), (n0, t0), "{}: the model is as it was", f.name);
    assert!((size(&app, b).0 - v0).abs() < 1e-9, "{}", f.name);
    app.cancel_dialog();
  }
  // Preview ticked: Remesh previews, refining the prism and keeping its shape
  app.s.selection = vec![Sel::Body { body: b }];
  app.start(Kind::Form(&MESH_REMESH));
  let d = app.dialog.as_mut().unwrap();
  d.live.on = true;
  d.live.retry();
  d.live_check();
  assert!(d.dirty);
  d.preview(&mut app.s);
  assert!(d.applied && d.error.is_none(), "{:?}", d.error);
  let (v, lo, hi) = size(&app, b);
  assert!(tris(&app) > t0, "{t0} → {}", tris(&app));
  assert!((v - v0).abs() < 1e-3 * v0, "the volume stays: {v0} → {v}");
  assert!((lo - lo0).length() < 1e-4 && (hi - hi0).length() < 1e-4, "the box stays: {lo0}..{hi0} → {lo}..{hi}");
  // unticked again: the preview goes
  let d = app.dialog.as_mut().unwrap();
  d.live.on = false;
  d.withdraw(&mut app.s);
  assert_eq!((app.s.design.features.len(), tris(&app)), (n0, t0));
}

/// `ui:mesh:smooth-dialog`: opening Smooth on the prism froze the app for seconds. It
/// previewed on opening (with Preview unticked), and the preview subdivided the 20 triangles
/// into 5120 planar faces, which took the kernel about two seconds to build into a body before
/// the dialog drew. It now opens without computing anything, and a preview stays a few hundred
/// faces.
#[test]
fn smooth_opens_at_once_and_previews_a_light_mesh() {
  let mut app = App::with_session(Session::default());
  app.run("import", json!({"path": LPRISM})).unwrap();
  let b = app.s.model.bodies[0].id;
  let tris = |app: &App| app.s.model.body(b).unwrap().mesh().unwrap().indices.len() / 3;
  let (v0, lo0, hi0) = size(&app, b);
  app.s.selection = vec![Sel::Body { body: b }];
  let revision = app.s.revision;
  app.start(Kind::Form(&MESH_SMOOTH));
  assert_eq!(app.s.revision, revision, "nothing regenerates as Smooth opens");
  assert_eq!(tris(&app), 20);
  let d = app.dialog.as_mut().unwrap();
  d.live.on = true;
  d.live.retry();
  d.live_check();
  d.preview(&mut app.s);
  assert!(d.applied && d.error.is_none(), "{:?}", d.error);
  let n = tris(&app);
  assert!(n > 20 && n <= fox_engine::model::meshtools::SMOOTH_SUBDIVIDE_MAX, "{n} triangles");
  // rounded off, about the size it was
  let (v, lo, hi) = size(&app, b);
  assert!((v - v0).abs() < 0.02 * v0, "{v0} → {v}");
  let (d, d0) = ((hi - lo).length(), (hi0 - lo0).length());
  assert!((d - d0).abs() < 0.2 * d0, "{lo0}..{hi0} → {lo}..{hi}");
}
