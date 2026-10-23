//! Measure with two picks, Interference and Section Analysis, driven headlessly.

use fox_engine::model::BodyId;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::{Dialog, Kind, MeasureFmt, measure_sections, shown_section};
use crate::App;

fn show_once(app: &mut App, d: &mut Dialog) -> bool {
  let ctx = egui::Context::default();
  let raw = egui::RawInput {
    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))),
    ..Default::default()
  };
  let mut keep = false;
  let mut out = ctx.run_ui(raw, |ui| keep = d.show(app, ui.ctx()));
  out.textures_delta.clear();
  keep
}

/// A 10 mm cube at the origin, a box overlapping it by half and one touching its other side.
fn three_boxes() -> (App, [BodyId; 3]) {
  let mut app = App::with_session(Session::default());
  for at in [[0, 0], [5, 0], [-10, 0]] {
    let p = json!({"width": 10, "depth": 10, "height": 10, "at": at, "operation": "new_body"});
    app.s.execute("box", &p).unwrap();
  }
  let ids = [0, 1, 2].map(|i| app.s.model.bodies[i].id);
  (app, ids)
}

fn labels(rows: &[super::Readout]) -> Vec<(&str, &str)> {
  rows.iter().map(|r| (r.label.as_str(), r.value.as_str())).collect()
}

#[test]
fn measure_two_picks_show_the_distance_and_a_section_per_pick() {
  let mut app = App::with_session(Session::default());
  app.s.execute("box", &json!({"width": 10, "depth": 20, "height": 30})).unwrap();
  let b = app.s.model.bodies[0].id;
  let body = app.s.model.body(b).unwrap();
  // the 10 × 20 bottom face and a vertex on top
  let face = body.faces().iter().find(|f| (f.area - 200.0).abs() < 1e-6 && f.center.z < 1.0).unwrap().id;
  let edge = body.edges().iter().find(|e| (e.length - 30.0).abs() < 1e-6).unwrap().id;
  let top = body.edges().iter().find(|e| e.id == edge).unwrap().end.z > 1.0;
  app.s.selection = vec![Sel::Face { body: b, face }, Sel::Vertex { body: b, edge, end: top }];
  let mut d = Dialog::open(&mut app, Kind::Inspect);
  assert!(show_once(&mut app, &mut d));
  assert!(app.measure.is_some(), "the distance line shows");

  let r = app.s.execute("measure", &json!({"a": {"type": "face", "body": b.0, "face": face}, "b": {"type": "vertex", "body": b.0, "edge": edge, "end": top}})).unwrap();
  let mm = MeasureFmt::default();
  // Results first, only the distance (no ΔX, ΔY, ΔZ); then Selection 1: the face, its area and
  // loop length; Selection 2: the vertex's position
  let secs = measure_sections(&r, &mm);
  let titles: Vec<&str> = secs.iter().map(|x| x.title.as_str()).collect();
  assert_eq!(titles, ["Results", "Selection 1", "Selection 2"]);
  assert_eq!(labels(&secs[0].rows), [("Distance", "30.000 mm")]);
  assert_eq!(secs[1].note, "Face selected");
  assert_eq!(labels(&secs[1].rows), [("Area", "200.000 mm^2"), ("Loop Length", "60.000 mm")]);
  assert_eq!(secs[2].note, "Vertex selected");
  assert_eq!(
    secs[2].rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
    ["X Position", "Y Position", "Z Position"]
  );
  assert_eq!(secs[2].rows[2].value, "30.000 mm");

  // the status bar names both picks and the distance, as Fusion's
  app.dialog = Some(d);
  assert_eq!(crate::panels::selection_text(&app).as_deref(), Some("1 Face 1 Vertex | Min Distance : 30.00 mm"));
}

/// Measure keeps the selection to its picks (the newest two it can measure), so the status bar
/// counts what it measures; a sketch point is a pick of its own with its Selection section and
/// the Results row.
#[test]
fn measure_takes_a_sketch_point_and_keeps_the_selection_to_its_picks() {
  let mut app = App::with_session(Session::default());
  app.s.execute("box", &json!({"width": 10, "depth": 20, "height": 30})).unwrap();
  let b = app.s.model.bodies[0].id;
  app.s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
  let sk = app.s.edit_sketch.unwrap();
  app.s.execute("sketch.point", &json!({"at": [3, 4]})).unwrap();
  app.s.execute("sketch.finish", &json!({})).unwrap();
  let point = app.s.design.sketch(sk).unwrap().points.last().unwrap().id;
  let body = app.s.model.body(b).unwrap();
  let (top, bottom) = (
    body.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id,
    body.faces().iter().find(|f| f.normal.z < -0.9).unwrap().id,
  );
  let pt = Sel::SketchPoint { sketch: sk, point };
  // a sketch point picked earlier, then two faces: the two faces are measured and counted
  app.s.selection = vec![pt, Sel::Face { body: b, face: top }, Sel::Face { body: b, face: bottom }];
  let mut d = Dialog::open(&mut app, Kind::Inspect);
  assert!(show_once(&mut app, &mut d));
  assert_eq!(app.s.selection, [Sel::Face { body: b, face: top }, Sel::Face { body: b, face: bottom }]);
  app.dialog = Some(d);
  let status = crate::panels::selection_text(&app).unwrap();
  assert!(status.starts_with("2 Faces"), "{status}");
  let mut d = app.dialog.take().unwrap();
  // a face, then the sketch point: Results and a section each
  app.s.selection = vec![Sel::Face { body: b, face: top }, pt];
  app.measure = None;
  assert!(show_once(&mut app, &mut d));
  assert_eq!(app.s.selection.len(), 2);
  assert!(app.measure.is_some(), "the distance line shows");
  let a = serde_json::to_value(Sel::Face { body: b, face: top }).unwrap();
  let r = app.s.execute("measure", &json!({"a": a, "b": serde_json::to_value(pt).unwrap()})).unwrap();
  let secs = measure_sections(&r, &MeasureFmt::default());
  let titles: Vec<&str> = secs.iter().map(|x| x.title.as_str()).collect();
  assert_eq!(titles, ["Results", "Selection 1", "Selection 2"]);
  assert_eq!(labels(&secs[0].rows), [("Distance", "30.000 mm")]);
  assert_eq!(secs[2].note, "Sketch Point selected");
  assert_eq!(secs[2].rows.iter().map(|r| r.value.as_str()).collect::<Vec<_>>(), ["3.000 mm", "4.000 mm", "0.000 mm"]);
  app.dialog = Some(d);
  assert_eq!(crate::panels::selection_text(&app).as_deref(), Some("1 Face 1 Sketch Point | Min Distance : 30.00 mm"));
}

#[test]
fn interference_computes_the_picked_bodies_and_ok_waits_for_it() {
  let (mut app, [a, b, c]) = three_boxes();
  let mut d = Dialog::open(&mut app, Kind::Interference);
  assert!(!d.kind.reports(), "OK and Cancel, not Close");
  assert!(show_once(&mut app, &mut d));
  assert!(d.report.is_none(), "nothing is computed until Compute");
  assert!(!d.ok_enabled(), "OK waits for Compute");
  assert_eq!(app.sel_priority, crate::select::Priority::Body, "Select picks bodies");

  app.s.selection = [a, b, c].iter().map(|x| Sel::Body { body: *x }).collect();
  assert!(show_once(&mut app, &mut d));
  assert_eq!(d.bodies.len(), 3);
  d.compute_interference(&mut app.s);
  let rows = d.report.as_ref().and_then(Value::as_array).unwrap().clone();
  assert_eq!(rows.len(), 1, "only the overlap: {rows:?}");
  assert!((rows[0]["volume_mm3"].as_f64().unwrap() - 500.0).abs() < 1e-3);
  assert!(d.ok_enabled());

  // Include Coincident Faces: the touching pair too, without a volume; a new pick drops the result
  d.flag2 = true;
  d.compute_interference(&mut app.s);
  let rows = d.report.as_ref().and_then(Value::as_array).unwrap().clone();
  assert_eq!(rows.len(), 2, "{rows:?}");
  assert!(rows.iter().any(|r| r["coincident"] == json!(true)));
  app.s.selection = vec![Sel::Body { body: a }, Sel::Body { body: b }];
  assert!(show_once(&mut app, &mut d));
  assert!(d.report.is_none() && !d.ok_enabled());

  // OK closes once computed
  d.compute_interference(&mut app.s);
  d.ok_requested = true;
  assert!(!show_once(&mut app, &mut d));
  d.leave(&mut app);
  assert_eq!(app.sel_priority, crate::select::Priority::Face, "the pick filter is put back");
}

#[test]
fn section_analysis_waits_for_a_cut_plane_then_keeps_its_cut_in_the_browser() {
  let mut app = App::with_session(Session::default());
  app.s.execute("box", &json!({"width": 10, "depth": 10, "height": 10})).unwrap();
  let mut d = Dialog::open(&mut app, Kind::Section);
  assert!(!d.kind.reports(), "OK and Cancel");
  assert!(show_once(&mut app, &mut d));
  // only Cut Plane: no cut, OK greyed, Fusion's prompt, the planes offered to click
  assert!(app.section.is_none() && !d.ok_enabled() && d.wants_cut_plane());
  assert_eq!(d.prompt(false).as_deref(), Some("Select a planar object to define the cut plane"));
  assert!(d.manipulator(&app.s).is_none());

  d.set_cut_plane(&app.s, json!("xz"));
  assert!(show_once(&mut app, &mut d));
  let sec = app.section.clone().expect("the cut shows");
  assert_eq!(sec["plane"], json!("xz"));
  assert_eq!(
    (sec["offset"].as_f64(), sec["hatch"].as_bool(), sec["color"].as_str()),
    (Some(0.0), Some(true), Some("component"))
  );
  assert!(d.ok_enabled());
  // the distance arrow and the two angle dials; dragging them sets Distance and the angles
  let m = d.manipulator(&app.s).expect("distance arrow");
  assert!((m.origin - fox_engine::kernel::glam::DVec3::new(0.0, 0.0, 5.0)).length() < 1e-6, "{:?}", m.origin);
  assert_eq!(d.handles(&app.s).len(), 2);
  d.set_manipulator(2.0, 0.0);
  d.set_handle(&app.s, 0, 30f64.to_radians(), 0.0);
  assert!(show_once(&mut app, &mut d));
  let sec = app.section.clone().unwrap();
  assert!((sec["offset"].as_f64().unwrap() - 2.0).abs() < 1e-9);
  assert!((sec["angle1"].as_f64().unwrap() - 30.0).abs() < 1e-9);
  let pl = fox_engine::scene::view_options(&app.s, &json!({"section": sec})).unwrap().section.unwrap();
  let n0 = fox_engine::model::OriginPlane::Xz.plane(app.s.design.frames()).normal();
  assert!((pl.normal().dot(n0) - 30f64.to_radians().cos()).abs() < 1e-9, "Angle 1 tilts the cut");

  // OK keeps it as Section1 in the Analysis folder; the folder's eye hides it
  d.ok_requested = true;
  assert!(!show_once(&mut app, &mut d));
  d.leave(&mut app);
  assert!(app.section.is_none());
  assert_eq!(app.sections.iter().map(|k| k.name.as_str()).collect::<Vec<_>>(), ["Section1"]);
  assert!(shown_section(&app).is_some());
  app.analysis_hidden = true;
  assert!(shown_section(&app).is_none(), "hiding the folder restores the full model");
  app.analysis_hidden = false;

  // Cancel leaves nothing behind; the next kept one is Section2
  let mut d = Dialog::open(&mut app, Kind::Section);
  d.set_cut_plane(&app.s, json!("xy"));
  assert!(show_once(&mut app, &mut d));
  assert!(app.section.is_some());
  app.dialog = Some(d);
  app.cancel_dialog();
  assert!(app.section.is_none() && app.sections.len() == 1);
  assert_eq!(super::section::next_section_name(&app), "Section2");
}

#[test]
fn section_analysis_takes_a_planar_face_picked_before_it_opens() {
  let mut app = App::with_session(Session::default());
  app.s.execute("box", &json!({"width": 10, "depth": 10, "height": 10})).unwrap();
  let b = app.s.model.bodies[0].id;
  let top = app.s.model.body(b).unwrap().faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  app.s.selection = vec![Sel::Face { body: b, face: top }];
  let d = Dialog::open(&mut app, Kind::Section);
  assert_eq!(d.plane, json!({"body": b.0, "face": top}));
  assert!(d.flip, "the cut keeps the body behind the face");
}
