use super::*;
use crate::*;
use clf_sketch::{ConstraintKind, Dim, ORIGIN, PtSpec, Sketch};
use std::f64::consts::PI;

const XY: PlaneRef = PlaneRef::Origin { plane: OriginPlane::Xy };

/// A width×depth rectangle anchored at the origin with its dimensions driven by expressions.
fn rect_sketch(w: &str, d: &str) -> Sketch {
  let mut s = Sketch::new();
  let lines = s.add_rectangle([0.0, 0.0], [10.0, 10.0]).unwrap();
  let bl = s.curve_points(lines[0])[0];
  s.add_constraint(ConstraintKind::Coincident { a: bl, b: ORIGIN }).unwrap();
  let b = s.curve_points(lines[0]);
  s.add_constraint(ConstraintKind::Distance { a: b[0], b: b[1], value: Dim::expr(w, 10.0) }).unwrap();
  let r = s.curve_points(lines[1]);
  s.add_constraint(ConstraintKind::Distance { a: r[0], b: r[1], value: Dim::expr(d, 10.0) }).unwrap();
  s
}

fn block_design() -> (Design, FeatureId, FeatureId) {
  let mut d = Design::new("block");
  d.add_parameter("w", "40 mm", ParamKind::Length, "").unwrap();
  d.add_parameter("h", "10 mm", ParamKind::Length, "").unwrap();
  let s = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: rect_sketch("w", "20") }).unwrap();
  let e = d
    .add_feature(FeatureKind::Extrude {
      sketch: s,
      profiles: vec![],
      extent: Extent::distance("h"),
      operation: Operation::NewBody,
    })
    .unwrap();
  (d, s, e)
}

fn volume(m: &Model) -> f64 {
  m.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
}

fn ok(m: &Model) {
  assert!(m.errors().is_empty(), "{:?}", m.errors());
}

#[test]
fn parametric_block() {
  let (mut d, s, _) = block_design();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 1);
  assert_eq!(m.bodies[0].name, "Body1");
  assert!((volume(&m) - 8000.0).abs() < 1e-6);
  assert!(m.sketches[&s].report.fully_constrained());
  d.set_parameter("w", "60").unwrap();
  d.set_parameter("h", "w / 4").unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - 60.0 * 20.0 * 15.0).abs() < 1e-6);
}

#[test]
fn hole_on_top_face_follows_height_changes() {
  let (mut d, _, _) = block_design();
  let m = regenerate(&mut d);
  let body = &m.bodies[0];
  let top = body.faces().iter().find(|f| f.normal.z > 0.9).unwrap();
  let face = FaceRef::from_face(body.id, top, top.center);
  let mut hole = Sketch::new();
  hole.add_circle(PtSpec::At(20.0, 10.0), 3.0).unwrap();
  let hs = d.add_feature(FeatureKind::Sketch { plane: PlaneRef::Face { face }, sketch: hole }).unwrap();
  d.add_feature(FeatureKind::Extrude {
    sketch: hs,
    profiles: vec![],
    extent: Extent { kind: ExtentKind::ThroughAll, flip: true, ..Extent::distance("0") },
    operation: Operation::Cut,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let expected = |h: f64| 40.0 * 20.0 * h - PI * 9.0 * h;
  assert!((volume(&m) - expected(10.0)).abs() < 1e-3, "{}", volume(&m));
  d.set_parameter("h", "25").unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - expected(25.0)).abs() < 1e-3, "{}", volume(&m));
  assert!((m.sketches[&hs].plane.origin.z - 25.0).abs() < 1e-9);
}

#[test]
fn fillet_survives_parameter_change() {
  let (mut d, _, _) = block_design();
  let m = regenerate(&mut d);
  let body = &m.bodies[0];
  let edges: Vec<EdgeRef> = body
    .edges()
    .iter()
    .filter(|e| (e.mid.z - 10.0).abs() < 1e-6)
    .map(|e| EdgeRef::from_edge(body.id, e, body.faces()))
    .collect();
  assert_eq!(edges.len(), 4);
  d.add_feature(FeatureKind::Fillet { edges, radius: "2".into() }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let v1 = volume(&m);
  // a quarter-round removed along the 120 mm top perimeter
  let removed = (4.0 - PI) * 120.0;
  assert!((v1 - (8000.0 - removed)).abs() < 5.0, "{v1}");
  d.set_parameter("h", "30").unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - (24000.0 - removed)).abs() < 5.0, "fillet stays on the top edges: {}", volume(&m));
}

#[test]
fn join_cut_and_split_bodies() {
  let (mut d, _, _) = block_design();
  // a second, overlapping block joined in
  let mut s2 = Sketch::new();
  s2.add_rectangle([30.0, 0.0], [50.0, 20.0]).unwrap();
  let s2 = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: s2 }).unwrap();
  d.add_feature(FeatureKind::Extrude {
    sketch: s2,
    profiles: vec![],
    extent: Extent::distance("10"),
    operation: Operation::Join,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 1);
  assert!((volume(&m) - 50.0 * 20.0 * 10.0).abs() < 1e-6);
  // a slot cut right through the middle splits the body in two
  let mut s3 = Sketch::new();
  s3.add_rectangle([20.0, -5.0], [24.0, 25.0]).unwrap();
  let s3 = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: s3 }).unwrap();
  d.add_feature(FeatureKind::Extrude {
    sketch: s3,
    profiles: vec![],
    extent: Extent { kind: ExtentKind::Symmetric, ..Extent::distance("50") },
    operation: Operation::Cut,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 2);
}

#[test]
fn revolve_around_sketch_line() {
  let mut d = Design::new("vase");
  let mut s = Sketch::new();
  s.add_rectangle([10.0, 0.0], [20.0, 30.0]).unwrap();
  let (axis, _) = s.add_line(PtSpec::Point(ORIGIN), PtSpec::At(0.0, 50.0)).unwrap();
  s.set_construction(axis, true).unwrap();
  let sk =
    d.add_feature(FeatureKind::Sketch { plane: PlaneRef::Origin { plane: OriginPlane::Xz }, sketch: s }).unwrap();
  d.add_feature(FeatureKind::Revolve {
    sketch: sk,
    profiles: vec![],
    axis: AxisRef::SketchLine { curve: axis },
    angle: "360".into(),
    operation: Operation::NewBody,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - PI * (400.0 - 100.0) * 30.0).abs() < 0.1, "{}", volume(&m));
}

#[test]
fn patterns_mirror_primitives_and_shell() {
  let mut d = Design::new("p");
  d.add_feature(FeatureKind::Primitive {
    shape: Primitive::Cylinder { radius: "2".into(), height: "5".into() },
    plane: XY,
    at: [10.0, 0.0],
    operation: Operation::NewBody,
  })
  .unwrap();
  d.add_feature(FeatureKind::CircularPattern {
    bodies: BodySel::All,
    axis: AxisRef::Z,
    count: "6".into(),
    angle: "360 deg".into(),
    join: false,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 6);
  d.add_feature(FeatureKind::Mirror { bodies: BodySel::All, plane: XY, join: false }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 12);
  let mut d = Design::new("cup");
  d.add_feature(FeatureKind::Primitive {
    shape: Primitive::Box { width: "20".into(), depth: "20".into(), height: "10".into() },
    plane: XY,
    at: [0.0, 0.0],
    operation: Operation::NewBody,
  })
  .unwrap();
  let m = regenerate(&mut d);
  let b = &m.bodies[0];
  let top = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap();
  d.add_feature(FeatureKind::Shell { faces: vec![FaceRef::from_face(b.id, top, top.center)], thickness: "1".into() })
    .unwrap();
  d.add_feature(FeatureKind::RectangularPattern {
    bodies: BodySel::All,
    direction: [1.0, 0.0, 0.0],
    count: "3".into(),
    spacing: "30".into(),
    direction2: None,
    count2: "1".into(),
    spacing2: "0".into(),
    join: true,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 3, "disjoint pattern instances stay separate bodies");
  let cup = 4000.0 - 18.0 * 18.0 * 9.0;
  assert!((volume(&m) - 3.0 * cup).abs() < 1e-2, "{}", volume(&m));
}

#[test]
fn failing_feature_does_not_stop_the_timeline() {
  let (mut d, s, _) = block_design();
  let bad = d.add_feature(FeatureKind::Fillet { edges: vec![], radius: "1".into() }).unwrap();
  let bad2 = d
    .add_feature(FeatureKind::Extrude {
      sketch: s,
      profiles: vec![],
      extent: Extent::distance("nope"),
      operation: Operation::Join,
    })
    .unwrap();
  d.add_feature(FeatureKind::Primitive {
    shape: Primitive::Sphere { radius: "5".into() },
    plane: XY,
    at: [100.0, 0.0],
    operation: Operation::NewBody,
  })
  .unwrap();
  let m = regenerate(&mut d);
  assert!(m.status(bad).unwrap().error.is_some());
  assert!(m.status(bad2).unwrap().error.as_ref().unwrap().contains("nope"));
  assert_eq!(m.bodies.len(), 2);
}

#[test]
fn rollback_marker_and_suppress() {
  let (mut d, _, e) = block_design();
  d.set_marker(Some(1));
  let m = regenerate(&mut d);
  assert!(m.bodies.is_empty());
  assert!(m.status(e).unwrap().rolled_back);
  d.set_marker(None);
  d.feature_mut(e).unwrap().suppressed = true;
  assert!(regenerate(&mut d).bodies.is_empty());
}

#[test]
fn cache_reuses_unchanged_prefix() {
  let (mut d, _, e) = block_design();
  let mut r = Regenerator::default();
  let m1 = r.regenerate(&mut d);
  let snapshot = d.clone();
  let m2 = r.regenerate(&mut d);
  assert_eq!(d, snapshot, "a clean regen doesn't change the design");
  assert_eq!(m1.bodies.len(), m2.bodies.len());
  d.set_parameter("h", "20").unwrap();
  let m3 = r.regenerate(&mut d);
  assert!((volume(&m3) - 16000.0).abs() < 1e-6);
  assert!(m3.status(e).unwrap().error.is_none());
}

#[test]
fn sketch_plane_on_faces_is_world_aligned() {
  let top = sketch_plane_on(DVec3::Z, DVec3::new(5.0, 5.0, 10.0)).unwrap();
  assert_eq!(top.x, DVec3::X);
  assert!((top.origin - DVec3::new(0.0, 0.0, 10.0)).length() < 1e-12);
  let front = sketch_plane_on(-DVec3::Y, DVec3::new(5.0, 0.0, 3.0)).unwrap();
  assert!((front.x - DVec3::X).length() < 1e-12 && (front.y - DVec3::Z).length() < 1e-12);
  let back = sketch_plane_on(DVec3::Y, DVec3::new(5.0, 20.0, 3.0)).unwrap();
  assert!((back.x + DVec3::X).length() < 1e-12 && (back.y - DVec3::Z).length() < 1e-12);
}

#[test]
fn import_round_trip() {
  let (mut d, _, _) = block_design();
  let m = regenerate(&mut d);
  let brep = m.bodies[0].shape.to_brep().unwrap();
  let mut d2 = Design::new("imported");
  d2.add_feature(FeatureKind::Import { file_name: "block.step".into(), breps: vec![brep], meshes: vec![] }).unwrap();
  let m2 = regenerate(&mut d2);
  ok(&m2);
  assert!((volume(&m2) - 8000.0).abs() < 1e-6);
}

#[test]
fn imported_mesh_becomes_a_body() {
  // a 10 mm cube as 12 triangles
  #[rustfmt::skip]
  let positions = vec![
    0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 10.0, 10.0, 0.0, 0.0, 10.0, 0.0,
    0.0, 0.0, 10.0, 10.0, 0.0, 10.0, 10.0, 10.0, 10.0, 0.0, 10.0, 10.0,
  ];
  #[rustfmt::skip]
  let indices = vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7];
  let mut d = Design::new("mesh");
  let mesh = crate::MeshData { name: "Cube".into(), positions, indices, merge_coplanar: true };
  d.add_feature(FeatureKind::Import { file_name: "cube.stl".into(), breps: vec![], meshes: vec![mesh] }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 1);
  assert!((volume(&m) - 1000.0).abs() < 1e-6);
  assert_eq!(m.bodies[0].faces().len(), 6);
  // survives a save/load
  let d2 = Design::from_json(&d.to_json()).unwrap();
  assert_eq!(d2, d);
}
