//! Fusion's CONSTRUCT methods: "Select" chips that take picks from the canvas and the browser
//! (origin items too), OK greyed until every pick is made, the pointer naming the pick, the rows
//! that come once it is made, and Plane at Angle's rotation handle.

use fox_engine::kernel::glam::DVec3;
use fox_engine::model::FeatureKind;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::{Dialog, Kind};
use crate::forms::{self, Form, Input};

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
}

/// A form dialog synced to the session's picks.
fn open(s: &Session, f: &'static Form) -> Dialog {
  let mut d = Dialog::blank(Kind::Form(f));
  d.form.sync(f, s, None);
  d
}

fn resync(d: &mut Dialog, s: &Session) {
  if let Kind::Form(f) = d.kind {
    d.form.sync(f, s, None);
  }
}

fn apply(s: &mut Session, d: &mut Dialog) {
  assert!(d.ok_enabled(), "{:?} not ready: {:?}", d.kind, d.form.values);
  d.preview(s);
  assert!(d.applied && d.error.is_none(), "{:?}: {:?} {:?}", d.kind, d.error, d.command());
}

/// The rows the dialog draws now, by label.
fn rows(d: &Dialog) -> Vec<&'static str> {
  let Kind::Form(f) = d.kind else { return Vec::new() };
  (0..f.inputs.len())
    .filter(|&i| d.form.shown(f, i) && !forms::State::hidden(f, i))
    .map(|i| match &f.inputs[i] {
      Input::Construct { label, .. }
      | Input::Face { label, .. }
      | Input::Edge { label, .. }
      | Input::Expr { label, .. }
      | Input::Choice { label, .. }
      | Input::Icons { label, .. }
      | Input::CheckOn { label, .. } => *label,
      other => panic!("{} has a {other:?} row", f.name),
    })
    .collect()
}

/// The last construction feature's point, axis or plane.
fn last(s: &Session) -> fox_engine::model::FeatureId {
  s.design.features.last().unwrap().id
}

/// Every method opens with Fusion's rows: "Select" chips (no drop-downs preset to XY, no
/// coordinate boxes), OK greyed and the pointer naming the first pick.
#[test]
fn construct_methods_open_with_select_chips_and_ok_greyed() {
  use forms::*;
  let s = Session::default();
  let cases: [(&'static Form, &[&str], Option<&str>); 18] = [
    (&PLANE_ANGLE, &["Edge"], Some("Select an edge, axis or sketch line")),
    (&TANGENT_PLANE, &["Face", "Reference"], None),
    (&MIDPLANE, &["Plane 1", "Plane 2"], Some("Select a plane or planar face")),
    (&PERPENDICULAR_PLANE, &["Face", "Plane Edge", "Extent"], Some("Select a face or plane")),
    (&PLANE_TWO_EDGES, &["Edge 1", "Edge 2"], Some("Select an edge, axis or sketch line")),
    (&PLANE_THREE_POINTS, &["Point 1", "Point 2", "Point 3"], Some("Select a point, vertex or sketch point")),
    (&PLANE_ALONG_PATH, &["Path", "Chaining"], Some("Select an edge or sketch curve")),
    (&AXIS_FACE, &["Face"], None),
    (&AXIS_NORMAL, &["Face", "Point"], None),
    (&AXIS_TWO_PLANES, &["Plane 1", "Plane 2"], Some("Select a plane or planar face")),
    (&AXIS_TWO_POINTS, &["Point 1", "Point 2"], Some("Select a point, vertex or sketch point")),
    (&AXIS_EDGE, &["Edge"], None),
    (&POINT_VERTEX, &["Point"], Some("Select a point, vertex or sketch point")),
    (&POINT_TWO_EDGES, &["Edge 1", "Edge 2"], Some("Select an edge, axis or sketch line")),
    (&POINT_THREE_PLANES, &["Plane 1", "Plane 2", "Plane 3"], Some("Select a plane or planar face")),
    (&POINT_CENTER, &["Face/Edge"], Some("Select a circular edge, sphere or torus")),
    (&POINT_EDGE_PLANE, &["Edge", "Plane"], Some("Select an edge, axis or sketch line")),
    (&POINT_ALONG_PATH, &["Path", "Chaining"], Some("Select an edge or sketch curve")),
  ];
  for (f, want, prompt) in cases {
    let d = open(&s, f);
    assert_eq!(rows(&d), want, "{}", f.name);
    assert!(!d.ok_enabled(), "{}: OK waits for the picks", f.name);
    assert!(d.command().is_some());
    if let Some(p) = prompt {
      assert_eq!(d.prompt(false).as_deref(), Some(p), "{}", f.name);
    }
  }
  // Perpendicular Plane is its own method, not Plane Along Path
  assert_eq!(crate::menus::construct_form("Perpendicular Plane"), Some(&PERPENDICULAR_PLANE));
}

/// Plane at Angle: the Edge pick brings Extent type (To Object greyed), Angle 0.0 deg, Offset and
/// Resize, and a rotation handle whose drag turns the plane; Offset moves it along its normal.
#[test]
fn plane_at_angle_rows_and_rotation_handle_follow_the_edge_pick() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 10}));
  let b = &s.model.bodies[0];
  let e = b.edges().iter().find(|e| e.start.z > 9.9 && e.end.z > 9.9).unwrap();
  let (body, edge, dir) = (b.id, e.id, (e.end - e.start).normalize());
  let mut d = open(&s, &forms::PLANE_ANGLE);
  assert!(d.handles(&s).is_empty());
  s.selection = vec![Sel::Edge { body, edge }];
  resync(&mut d, &s);
  assert_eq!(rows(&d), ["Edge", "Extent type", "Angle", "Offset", "Resize"]);
  assert!(forms::greyed_option(&forms::PLANE_ANGLE, "extent", "to_object").is_some());
  assert!(forms::greyed_option(&forms::PLANE_ANGLE, "extent", "angle").is_none());
  apply(&mut s, &mut d);
  // at 0° the plane holds the edge and lies on a face along it
  let n0 = s.model.planes[&last(&s)].normal();
  assert!(n0.dot(dir).abs() < 1e-9 && [n0.x, n0.y, n0.z].iter().any(|c| c.abs() > 0.999), "{n0:?}");
  let h = d.handles(&s);
  assert_eq!(h.len(), 1, "the rotation handle");
  assert!(h[0].angular() && h[0].value.abs() < 1e-9 && h[0].label == "0 deg", "{:?}", h[0]);
  // dragging it to 30° writes the Angle row and turns the plane
  d.set_handle(&s, 0, 30f64.to_radians(), 1f64.to_radians());
  assert_eq!(d.form.values[2], json!("30 deg"));
  d.preview(&mut s);
  let pl = s.model.planes[&last(&s)];
  assert!((pl.normal().dot(n0) - 30f64.to_radians().cos()).abs() < 1e-9, "{:?}", pl.normal());
  assert!((d.handles(&s)[0].value - 30f64.to_radians()).abs() < 1e-9);
  // Offset slides it along its normal
  d.form.values[3] = json!("5 mm");
  let o0 = pl.origin;
  d.preview(&mut s);
  let pl = s.model.planes[&last(&s)];
  assert!(((pl.origin - o0).dot(pl.normal()) - 5.0).abs() < 1e-9);
}

/// Perpendicular Plane: a face pick, the U/V Plane Edge toggles, Extent Distance and its value.
#[test]
fn perpendicular_plane_stands_square_to_the_picked_face() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 10}));
  let b = &s.model.bodies[0];
  let top = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  let body = b.id;
  let mut d = open(&s, &forms::PERPENDICULAR_PLANE);
  s.selection = vec![Sel::Face { body, face: top }];
  resync(&mut d, &s);
  assert_eq!(rows(&d), ["Face", "Plane Edge", "Extent", "Distance"]);
  apply(&mut s, &mut d);
  let n = s.model.planes[&last(&s)].normal();
  assert!(n.z.abs() < 1e-9, "square to the top face: {n:?}");
}

/// Picks of every kind: origin items from the browser, vertices, circular edges, faces.
#[test]
fn construct_methods_take_picks_of_every_kind() {
  use forms::*;
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 10}));
  let b = &s.model.bodies[0];
  let body = b.id;
  let top = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  // three corners of the top face, as vertices
  let mut corners: Vec<(u32, bool, DVec3)> = Vec::new();
  for e in b.edges() {
    for (end, p) in [(false, e.start), (true, e.end)] {
      if p.z > 9.9 && !corners.iter().any(|c| (c.2 - p).length() < 1e-6) {
        corners.push((e.id, end, p));
      }
    }
  }
  let vertex = |i: usize| Sel::Vertex { body, edge: corners[i].0, end: corners[i].1 };

  // Point Through Three Planes from the browser's XY, XZ and YZ: the origin
  let mut d = open(&s, &POINT_THREE_PLANES);
  for (i, p) in ["xy", "xz", "yz"].iter().enumerate() {
    assert!(!d.ok_enabled(), "{i} of 3 planes picked");
    d.form.pick_origin(json!(p));
    resync(&mut d, &s);
  }
  apply(&mut s, &mut d);
  assert!(s.model.points[&last(&s)].length() < 1e-9);
  // picking an origin item again takes it out
  d.form.pick_origin(json!("yz"));
  resync(&mut d, &s);
  assert!(!d.ok_enabled());
  d.withdraw(&mut s);

  // Point At Vertex
  s.selection = vec![vertex(0)];
  let mut d = open(&s, &POINT_VERTEX);
  apply(&mut s, &mut d);
  assert!((s.model.points[&last(&s)] - corners[0].2).length() < 1e-9);
  d.applied = false;

  // Plane Through Three Points: the top face's plane
  s.selection = vec![vertex(0), vertex(1)];
  let mut d = open(&s, &PLANE_THREE_POINTS);
  assert!(!d.ok_enabled());
  s.selection.push(vertex(2));
  resync(&mut d, &s);
  apply(&mut s, &mut d);
  let pl = s.model.planes[&last(&s)];
  assert!(pl.normal().z.abs() > 0.999 && (pl.origin.z - 10.0).abs() < 1e-9, "{pl:?}");
  d.applied = false;

  // Axis Through Two Points: the origin point and a corner
  s.selection = vec![vertex(1)];
  let mut d = open(&s, &AXIS_TWO_POINTS);
  d.form.pick_origin(json!({"at": [0.0, 0.0, 0.0], "origin": true}));
  resync(&mut d, &s);
  apply(&mut s, &mut d);
  let (_, dir) = s.model.axes[&last(&s)];
  assert!(dir.normalize().cross(corners[1].2.normalize()).length() < 1e-9);
  d.applied = false;

  // Axis Perpendicular To Face through a picked corner: Face, then Point
  s.selection = vec![Sel::Face { body, face: top }];
  let mut d = open(&s, &AXIS_NORMAL);
  assert!(!d.ok_enabled(), "the Point is still to pick");
  s.selection.push(vertex(2));
  resync(&mut d, &s);
  apply(&mut s, &mut d);
  let (o, dir) = s.model.axes[&last(&s)];
  let c = corners[2].2;
  assert!(dir.normalize().z.abs() > 0.999 && (o.x - c.x).abs() < 1e-6 && (o.y - c.y).abs() < 1e-6, "{o:?}");
  d.applied = false;

  // Point At Edge And Plane: the Z axis from the browser and the top face
  s.selection = vec![Sel::Face { body, face: top }];
  let mut d = open(&s, &POINT_EDGE_PLANE);
  d.form.pick_origin(json!("z"));
  resync(&mut d, &s);
  apply(&mut s, &mut d);
  assert!((s.model.points[&last(&s)] - DVec3::new(0.0, 0.0, 10.0)).length() < 1e-9);
  d.applied = false;

  // Point At Center: a circular edge of a cylinder
  let mut s = Session::default();
  run(&mut s, "cylinder", json!({"radius": 5, "height": 20}));
  let b = &s.model.bodies[0];
  let body = b.id;
  let rim = b.edges().iter().find(|e| e.kind == fox_engine::kernel::CurveKind::Circle && e.center.z > 1.0).unwrap();
  let (rim, centre) = (rim.id, rim.center);
  let side = b.faces().iter().find(|f| f.kind == fox_engine::kernel::SurfaceKind::Cylinder).unwrap().id;
  s.selection = vec![Sel::Edge { body, edge: rim }];
  let mut d = open(&s, &POINT_CENTER);
  assert_eq!(d.command().unwrap().1["edge"], json!({"body": body.0, "edge": rim}));
  apply(&mut s, &mut d);
  assert!((s.model.points[&last(&s)] - centre).length() < 1e-9);
  d.applied = false;

  // Tangent Plane: the side face, then the XZ plane as the reference (parallel to it)
  s.selection = vec![Sel::Face { body, face: side }];
  let mut d = open(&s, &TANGENT_PLANE);
  assert_eq!(d.prompt(false).as_deref(), Some("Select a reference object."));
  d.form.pick_origin(json!("xz"));
  resync(&mut d, &s);
  apply(&mut s, &mut d);
  let pl = s.model.planes[&last(&s)];
  assert!(pl.normal().y.abs() > 0.999 && (pl.origin.y.abs() - 5.0).abs() < 1e-6, "{pl:?}");
  d.applied = false;

  // Plane Along Path: the Distance row comes with the path
  s.selection = vec![Sel::Edge { body, edge: rim }];
  let mut d = open(&s, &PLANE_ALONG_PATH);
  assert_eq!(rows(&d), ["Path", "Chaining", "Distance"]);
  apply(&mut s, &mut d);
  assert!(matches!(s.design.features.last().unwrap().kind, FeatureKind::PlaneBy { .. }));
}

/// An origin item clicked in the canvas or browser goes to an open CONSTRUCT dialog's picks
/// (drawn selected) instead of becoming the browser's picked plane.
#[test]
fn origin_items_go_to_an_open_construct_dialog() {
  use crate::origin::{self, OriginItem};
  let mut app = crate::App::with_session(Session::default());
  app.start(Kind::Form(&forms::AXIS_TWO_PLANES));
  origin::pick(&mut app, OriginItem::Xy);
  origin::pick(&mut app, OriginItem::Yz);
  assert!(app.picked_plane.is_none(), "not the browser's pick");
  assert_eq!(origin::look(&app, OriginItem::Xy), origin::Look::Selected);
  let d = app.dialog.as_mut().unwrap();
  assert_eq!(d.form.origin, [json!("xy"), json!("yz")]);
  resync(d, &app.s);
  assert!(d.ok_enabled());
  // again: taken out
  origin::pick(&mut app, OriginItem::Xy);
  assert_eq!(app.dialog.as_ref().unwrap().form.origin, [json!("yz")]);
  assert_eq!(origin::look(&app, OriginItem::Xy), origin::Look::Plain);
}
