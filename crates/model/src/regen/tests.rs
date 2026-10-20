use super::*;
use crate::*;
use fox_kernel::glam::DVec3;
use fox_sketch::{ConstraintKind, Dim, ORIGIN, PtSpec, Sketch};
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
  let s = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: rect_sketch("w", "20"), links: Vec::new() }).unwrap();
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
  let hs =
    d.add_feature(FeatureKind::Sketch { plane: PlaneRef::Face { face }, sketch: hole, links: Vec::new() }).unwrap();
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
  d.add_feature(FeatureKind::Fillet {
    edges,
    radius: "2".into(),
    radius2: None,
    setback: None,
    chord: false,
    radii: vec![],
    sets: vec![],
    distance2: None,
    flipped: false,
  })
  .unwrap();
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
  let s2 = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: s2, links: Vec::new() }).unwrap();
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
  let s3 = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: s3, links: Vec::new() }).unwrap();
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
  let sk = d
    .add_feature(FeatureKind::Sketch {
      plane: PlaneRef::Origin { plane: OriginPlane::Xz },
      sketch: s,
      links: Vec::new(),
    })
    .unwrap();
  d.add_feature(FeatureKind::Revolve {
    sketch: sk,
    profiles: vec![],
    axis: AxisRef::SketchLine { curve: axis },
    angle: "360".into(),
    symmetric: false,
    angle2: None,
    operation: Operation::NewBody,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - PI * (400.0 - 100.0) * 30.0).abs() < 0.1, "{}", volume(&m));
}

/// A 90 degree revolve of the rectangle x 5..15, z 0..30 (on a plane facing +Y) about a sketch
/// line on x = 0 drawn downwards, from z = 30 to z = 0, in a design with the given behaviours.
fn revolve_about_downward_line(behaviours: Behaviours) -> (Design, Model) {
  let mut d = Design::new("rev");
  d.behaviours = behaviours;
  let mut s = Sketch::new();
  s.add_rectangle([5.0, 0.0], [15.0, -30.0]).unwrap();
  let (axis, _) = s.add_line(PtSpec::At(0.0, -30.0), PtSpec::At(0.0, 0.0)).unwrap();
  s.set_construction(axis, true).unwrap();
  let plane = PlaneRef::Free { origin: [0.0; 3], normal: [0.0, 1.0, 0.0], x: Some([1.0, 0.0, 0.0]) };
  let sk = d.add_feature(FeatureKind::Sketch { plane, sketch: s, links: Vec::new() }).unwrap();
  d.add_feature(FeatureKind::Revolve {
    sketch: sk,
    profiles: vec![],
    axis: AxisRef::SketchLine { curve: axis },
    angle: "90 deg".into(),
    symmetric: false,
    angle2: None,
    operation: Operation::NewBody,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - PI * (225.0 - 25.0) * 30.0 / 4.0).abs() < 0.1, "{}", volume(&m));
  (d, m)
}

#[test]
fn revolve_sketch_line_axis_legacy_follows_the_drawn_direction() {
  // no REVOLVE_AXIS_UNDIRECTED: right-handed about the line from its start to its end (-Z), as
  // on main, so the profile at +X turns towards -Y
  let (_, m) = revolve_about_downward_line(Behaviours::legacy());
  let mp = m.bodies[0].shape.mass().unwrap();
  assert!(mp.bbox_min.y < -14.9 && mp.bbox_max.y.abs() < 1e-6, "{mp:?}");
}

#[test]
fn revolve_sketch_line_axis_new_design_ignores_the_drawn_direction() {
  // the profile turns towards the sketch normal (+Y) whichever way the line was drawn
  let (_, m) = revolve_about_downward_line(Behaviours::current());
  let mp = m.bodies[0].shape.mass().unwrap();
  assert!(mp.bbox_max.y > 14.9 && mp.bbox_min.y.abs() < 1e-6, "{mp:?}");
}

#[test]
fn revolve_without_symmetric_or_angle2_is_one_sided() {
  // a revolve saved before the symmetric and two-sided options reads as one side and rebuilds
  // exactly as an explicit one-sided revolve does
  let (d, m) = revolve_about_downward_line(Behaviours::legacy());
  let mut v: serde_json::Value = serde_json::from_str(&d.to_json()).unwrap();
  let mut stripped = 0;
  for f in v["features"].as_array_mut().unwrap() {
    if let Some(k) = f["kind"].as_object_mut().filter(|k| k.contains_key("symmetric")) {
      k.remove("symmetric");
      k.remove("angle2");
      stripped += 1;
    }
  }
  assert_eq!(stripped, 1);
  let mut old = Design::from_json(&v.to_string()).unwrap();
  assert_eq!(old, d);
  let FeatureKind::Revolve { symmetric, angle2, .. } = &old.features[1].kind else { panic!("not a revolve") };
  assert!(!symmetric && angle2.is_none());
  let again = regenerate(&mut old);
  ok(&again);
  let (a, b) = (m.bodies[0].shape.mass().unwrap(), again.bodies[0].shape.mass().unwrap());
  assert_eq!((a.volume, a.bbox_min, a.bbox_max), (b.volume, b.bbox_min, b.bbox_max));
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
    symmetric: false,
    join: false,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 6);
  d.add_feature(FeatureKind::Mirror { bodies: BodySel::All, plane: XY, features: vec![], join: false }).unwrap();
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
  d.add_feature(FeatureKind::Shell {
    faces: vec![FaceRef::from_face(b.id, top, top.center)],
    thickness: "1".into(),
    round: false,
    direction: Default::default(),
    thickness_out: String::new(),
    bodies: Vec::new(),
  })
  .unwrap();
  d.add_feature(FeatureKind::RectangularPattern {
    bodies: BodySel::All,
    direction: [1.0, 0.0, 0.0],
    count: "3".into(),
    spacing: "30".into(),
    direction2: None,
    count2: "1".into(),
    spacing2: "0".into(),
    symmetric: false,
    symmetric2: false,
    extent: false,
    join: true,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 3, "disjoint pattern instances stay separate bodies");
  let cup = 4000.0 - 18.0 * 18.0 * 9.0;
  assert!((volume(&m) - 3.0 * cup).abs() < 1e-2, "{}", volume(&m));
}

/// Fusion's placements (measured in Fusion 2705): a symmetric count of 4 at 12 mm runs -12..24
/// (the extra instance goes on the positive side), 2 runs 0..12, and an extent reaches from the
/// original to the farthest instance on the positive side.
#[test]
fn rectangular_pattern_symmetric_and_extent_offsets() {
  let at = |n, d, sym, ext| pattern_offsets(n, d, sym, ext);
  assert_eq!(at(3, 12.0, true, false), (vec![-12.0, 0.0, 12.0], 1));
  assert_eq!(at(4, 12.0, true, false), (vec![-12.0, 0.0, 12.0, 24.0], 1));
  assert_eq!(at(2, 12.0, true, false), (vec![0.0, 12.0], 0));
  assert_eq!(at(4, 30.0, false, true), (vec![0.0, 10.0, 20.0, 30.0], 0));
  assert_eq!(at(4, 12.0, true, true), (vec![-6.0, 0.0, 6.0, 12.0], 1));
  assert_eq!(at(3, 12.0, true, true), (vec![-12.0, 0.0, 12.0], 1));
  assert_eq!(at(1, 12.0, true, true), (vec![0.0], 0));
}

#[test]
fn failing_feature_does_not_stop_the_timeline() {
  let (mut d, s, _) = block_design();
  let bad = d
    .add_feature(FeatureKind::Fillet {
      edges: vec![],
      radius: "1".into(),
      radius2: None,
      setback: None,
      chord: false,
      radii: vec![],
      sets: vec![],
      distance2: None,
      flipped: false,
    })
    .unwrap();
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

// ---------------------------------------------------------------- sheet metal

/// t = 2, r = 3, K = 0.5 (with K = 0.5 the neutral layer is the mid-surface, so unfolding keeps
/// the volume exactly).
fn sheet_design() -> Design {
  let mut d = Design::new("sheet");
  d.add_parameter("t", "2 mm", ParamKind::Length, "").unwrap();
  d.set_sheet_rule(SheetMetalRule {
    name: "Steel".into(),
    thickness: "t".into(),
    bend_radius: "3 mm".into(),
    k_factor: "0.5".into(),
    ..SheetMetalRule::default()
  })
  .unwrap();
  d
}

fn edge_near(m: &Model, p: [f64; 3]) -> EdgeRef {
  let b = &m.bodies[0];
  let e = b
    .edges()
    .iter()
    .min_by(|a, c| (a.mid - DVec3::from(p)).length().total_cmp(&(c.mid - DVec3::from(p)).length()))
    .unwrap();
  assert!((e.mid - DVec3::from(p)).length() < 1e-6, "no edge at {p:?}");
  EdgeRef::from_edge(b.id, e, b.faces())
}

fn plate_flange(d: &mut Design) {
  let s =
    d.add_feature(FeatureKind::Sketch { plane: XY, sketch: rect_sketch("100", "50"), links: Vec::new() }).unwrap();
  d.add_feature(FeatureKind::Flange {
    sketch: s,
    profiles: vec![],
    rule: String::new(),
    flip: false,
    operation: Operation::NewBody,
  })
  .unwrap();
}

#[test]
fn flange_edge_flange_and_flat_pattern() {
  let mut d = sheet_design();
  plate_flange(&mut d);
  let m = regenerate(&mut d);
  ok(&m);
  assert!((volume(&m) - 100.0 * 50.0 * 2.0).abs() < 1e-6);
  let info = m.bodies[0].sheet.clone().unwrap();
  assert_eq!((info.thickness, info.bend_radius, info.k_factor), (2.0, 3.0, 0.5));
  // a hole in the base, then a 20 mm flange up from the top edge at y = 50
  let mut hole = Sketch::new();
  hole.add_circle(PtSpec::At(50.0, 25.0), 5.0).unwrap();
  let hs = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: hole, links: Vec::new() }).unwrap();
  d.add_feature(FeatureKind::Extrude {
    sketch: hs,
    profiles: vec![],
    extent: Extent { kind: ExtentKind::Symmetric, ..Extent::distance("10") },
    operation: Operation::Cut,
  })
  .unwrap();
  let m = regenerate(&mut d);
  let edge = edge_near(&m, [50.0, 50.0, 2.0]);
  let ef = d
    .add_feature(FeatureKind::EdgeFlange {
      edges: vec![edge],
      height: "20".into(),
      angle: "90 deg".into(),
      flip: false,
      position: BendPosition::Adjacent,
      offset_start: "0".into(),
      offset_end: "0".into(),
      corners: CornerMode::Miter,
    })
    .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 1);
  let hole_v = PI * 25.0 * 2.0;
  let bend_v = PI / 4.0 * (25.0 - 9.0) * 100.0;
  let folded = 10000.0 - hole_v + bend_v + 15.0 * 2.0 * 100.0;
  assert!((volume(&m) - folded).abs() < 0.01, "{} vs {folded}", volume(&m));
  let (_, hi) = m.bounds().unwrap();
  // height is measured from the outside of the sheet (z = 0) to the tip
  assert!((hi.y - 55.0).abs() < 1e-6 && (hi.z - 20.0).abs() < 1e-6, "{hi:?}");

  // flat pattern: 50 + bend allowance + 15 long; same volume when K = 0.5
  let flat = m.bodies[0].flat_pattern(None).unwrap();
  assert_eq!(flat.bends.len(), 1);
  assert!((flat.bends[0].angle - PI / 2.0).abs() < 1e-6);
  let fm = flat.shape.mass().unwrap();
  assert!((fm.volume - folded).abs() < 0.05, "{} vs {folded}", fm.volume);
  let len = 50.0 + PI / 2.0 * (3.0 + 0.5 * 2.0) + 15.0;
  let size = fm.bbox_max - fm.bbox_min;
  assert!((size.y - len).abs() < 1e-3 && (size.x - 100.0).abs() < 1e-3 && (size.z - 2.0).abs() < 1e-3, "{size:?}");
  let dxf = crate::sheet::flat_dxf(&flat).unwrap();
  assert!(dxf.contains("CIRCLE") && dxf.contains("BEND_"), "{dxf}");

  // Unfold as a timeline feature, then the rule drives everything
  d.add_feature(FeatureKind::Unfold { body: m.bodies[0].id, face: None, keep_folded: true }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 2);
  assert!(m.bodies[1].sheet.as_ref().unwrap().flat);
  d.set_parameter("t", "1.5").unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let mm = m.bodies[0].shape.mass().unwrap();
  assert!((mm.bbox_max.z - 20.0).abs() < 1e-6 && (mm.bbox_max.y - 54.5).abs() < 1e-6, "{mm:?}");
  // inside position keeps the outside of the flange on the original edge
  let FeatureKind::EdgeFlange { position, .. } = &mut d.feature_mut(ef).unwrap().kind else { panic!() };
  *position = BendPosition::Inside;
  let m = regenerate(&mut d);
  ok(&m);
  assert!((m.bodies[0].shape.mass().unwrap().bbox_max.y - 50.0).abs() < 1e-6);
}

#[test]
fn contour_flange_unfolds_to_its_flat_length() {
  let mut d = sheet_design();
  let mut path = Sketch::new();
  path.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(50.0, 0.0)).unwrap();
  path.add_line(PtSpec::At(50.0, 0.0), PtSpec::At(50.0, 30.0)).unwrap();
  path.add_line(PtSpec::At(50.0, 30.0), PtSpec::At(80.0, 30.0)).unwrap();
  let s = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: path, links: Vec::new() }).unwrap();
  d.add_feature(FeatureKind::ContourFlange {
    sketch: s,
    curves: vec![],
    distance: "40".into(),
    symmetric: false,
    flip: false,
    flip_direction: false,
    rule: "Steel".into(),
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 1);
  let flat = m.bodies[0].flat_pattern(None).unwrap();
  assert_eq!(flat.bends.len(), 2);
  // left turn (path outside, radius 5) then right turn (path inside, radius 3); each bend
  // trims its tangent length (= radius on the path side) off both neighbours
  let ba = PI / 2.0 * (3.0 + 0.5 * 2.0);
  let len = (50.0 - 5.0) + (30.0 - 5.0 - 3.0) + (30.0 - 3.0) + 2.0 * ba;
  let fm = flat.shape.mass().unwrap();
  assert!((fm.volume - volume(&m)).abs() < 0.05, "{} vs {}", fm.volume, volume(&m));
  let size = fm.bbox_max - fm.bbox_min;
  let mut dims = [size.x, size.y, size.z];
  dims.sort_by(f64::total_cmp);
  assert!(
    (dims[0] - 2.0).abs() < 1e-3 && (dims[1] - 40.0).abs() < 1e-3 && (dims[2] - len).abs() < 1e-3,
    "{dims:?} {len}"
  );
}

#[test]
fn convert_solid_to_sheet_metal() {
  let mut d = sheet_design();
  let s = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: rect_sketch("60", "40"), links: Vec::new() }).unwrap();
  d.add_feature(FeatureKind::Extrude {
    sketch: s,
    profiles: vec![],
    extent: Extent::distance("3"),
    operation: Operation::NewBody,
  })
  .unwrap();
  let m = regenerate(&mut d);
  let body = m.bodies[0].id;
  assert!(m.bodies[0].flat_pattern(None).is_err(), "not sheet metal yet");
  d.add_feature(FeatureKind::ConvertToSheetMetal { body, face: None, rule: String::new(), bends: true }).unwrap();
  let m = regenerate(&mut d);
  assert!(m.errors().is_empty(), "{:?}", m.errors());
  assert!(m.status.last().unwrap().warning.as_ref().unwrap().contains("3.000 mm thick"));
  assert_eq!(m.bodies[0].sheet.as_ref().unwrap().thickness, 3.0);
  let e = edge_near(&m, [30.0, 0.0, 3.0]);
  d.add_feature(FeatureKind::EdgeFlange {
    edges: vec![e],
    height: "10".into(),
    angle: "45 deg".into(),
    flip: true,
    position: BendPosition::Adjacent,
    offset_start: "5".into(),
    offset_end: "5".into(),
    corners: CornerMode::Miter,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let (lo, _) = m.bounds().unwrap();
  assert!(lo.z < -1.0 && lo.y < -1.0, "flipped flange bends down and out: {lo:?}");
  let flat = m.bodies[0].flat_pattern(None).unwrap();
  assert_eq!(flat.bends.len(), 1);
  assert!((flat.bends[0].angle - PI / 4.0).abs() < 1e-6);
  assert!(((flat.bends[0].end - flat.bends[0].start).length() - 50.0).abs() < 1e-6);
}

#[test]
fn hem_folds_back_and_unfolds() {
  let mut d = sheet_design();
  plate_flange(&mut d);
  let m = regenerate(&mut d);
  let e = edge_near(&m, [100.0, 25.0, 2.0]);
  d.add_feature(FeatureKind::Hem { edges: vec![e], length: "12".into(), gap: "1".into(), flip: false }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let mm = m.bodies[0].shape.mass().unwrap();
  // fold: outside radius 0.5 + 2 beyond the edge; back on top: 2 + 1 + 2 high
  assert!((mm.bbox_max.x - 102.5).abs() < 1e-6 && (mm.bbox_max.z - 5.0).abs() < 1e-6, "{mm:?}");
  let flat = m.bodies[0].flat_pattern(None).unwrap();
  assert_eq!(flat.bends.len(), 1);
  assert!((flat.bends[0].angle - PI).abs() < 1e-6);
  let len = 100.0 + PI * (0.5 + 0.5 * 2.0) + (12.0 - 2.5);
  let fm = flat.shape.mass().unwrap();
  assert!(((fm.bbox_max - fm.bbox_min).x - len).abs() < 1e-3, "{fm:?} {len}");
  assert!((fm.volume - mm.volume).abs() < 0.05);
}

#[test]
fn bend_along_a_sketch_line() {
  let mut d = sheet_design();
  plate_flange(&mut d);
  let m = regenerate(&mut d);
  let body = &m.bodies[0];
  let top = body.faces().iter().find(|f| f.normal.z > 0.9).unwrap();
  let face = FaceRef::from_face(body.id, top, top.center);
  let mut sk = Sketch::new();
  let (line, _) = sk.add_line(PtSpec::At(70.0, -10.0), PtSpec::At(70.0, 60.0)).unwrap();
  let s = d.add_feature(FeatureKind::Sketch { plane: PlaneRef::Face { face }, sketch: sk, links: Vec::new() }).unwrap();
  d.add_feature(FeatureKind::Bend { sketch: s, line, angle: "90 deg".into(), flip: false, flip_side: true }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 1);
  // K = 0.5: volume is unchanged and the flat pattern is the original plate
  assert!((volume(&m) - 10000.0).abs() < 0.05, "{}", volume(&m));
  let mm = m.bodies[0].shape.mass().unwrap();
  assert!(mm.bbox_max.z > 20.0 && mm.bbox_max.x < 80.0, "{mm:?}");
  let flat = m.bodies[0].flat_pattern(None).unwrap();
  assert_eq!(flat.bends.len(), 1);
  let fm = flat.shape.mass().unwrap();
  let size = fm.bbox_max - fm.bbox_min;
  assert!((size.x - 100.0).abs() < 1e-3 && (size.y - 50.0).abs() < 1e-3, "{size:?}");
}

fn flange_on(d: &mut Design, edges: Vec<EdgeRef>, offsets: (&str, &str)) -> FeatureId {
  d.add_feature(FeatureKind::EdgeFlange {
    edges,
    height: "20".into(),
    angle: "90 deg".into(),
    flip: false,
    position: BendPosition::Adjacent,
    offset_start: offsets.0.into(),
    offset_end: offsets.1.into(),
    corners: CornerMode::Miter,
  })
  .unwrap()
}

#[test]
fn flanges_meeting_at_a_corner_are_mitered() {
  for position in [BendPosition::Adjacent, BendPosition::Inside] {
    let mut d = sheet_design();
    plate_flange(&mut d);
    let m = regenerate(&mut d);
    let edges = vec![edge_near(&m, [50.0, 50.0, 2.0]), edge_near(&m, [100.0, 25.0, 2.0])];
    let f = flange_on(&mut d, edges, ("0", "0"));
    let FeatureKind::EdgeFlange { position: p, .. } = &mut d.feature_mut(f).unwrap().kind else { panic!() };
    *p = position;
    let m = regenerate(&mut d);
    ok(&m);
    assert_eq!(m.bodies.len(), 1);
    assert!(m.bodies[0].shape.is_valid());
    let mm = m.bodies[0].shape.mass().unwrap();
    // the walls reach into the corner (mitered), not just along their own edges
    let reach = if position == BendPosition::Adjacent { 105.0 } else { 100.0 };
    assert!((mm.bbox_max.x - reach).abs() < 1e-6 && (mm.bbox_max.y - (reach - 50.0)).abs() < 1e-6, "{mm:?}");
    let flat = m.bodies[0].flat_pattern(None).unwrap();
    assert_eq!(flat.bends.len(), 2, "{position:?}");
    assert!(flat.shape.is_valid());
  }
}

#[test]
fn partial_flanges_get_bend_reliefs() {
  let volume_with = |relief: ReliefShape| {
    let mut d = sheet_design();
    d.sheet_metal_rules[0].relief = relief;
    plate_flange(&mut d);
    let m = regenerate(&mut d);
    let e = edge_near(&m, [50.0, 50.0, 2.0]);
    flange_on(&mut d, vec![e], ("10", "10"));
    let m = regenerate(&mut d);
    ok(&m);
    assert!(m.bodies[0].flat_pattern(None).is_ok());
    volume(&m)
  };
  let none = volume_with(ReliefShape::None);
  // two notches, relief width t = 2, half a thickness (1) into the sheet, through t = 2
  assert!((none - volume_with(ReliefShape::Rectangular) - 2.0 * 2.0 * 1.0 * 2.0).abs() < 1e-6);
  assert!(none - volume_with(ReliefShape::Round) > 1.0);
}

#[test]
fn shelled_box_rips_converts_and_unfolds() {
  let mut d = sheet_design();
  d.add_feature(FeatureKind::Primitive {
    shape: Primitive::Box { width: "60".into(), depth: "40".into(), height: "30".into() },
    plane: XY,
    at: [30.0, 20.0],
    operation: Operation::NewBody,
  })
  .unwrap();
  let m = regenerate(&mut d);
  let b = &m.bodies[0];
  let top = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap();
  d.add_feature(FeatureKind::Shell {
    faces: vec![FaceRef::from_face(b.id, top, top.center)],
    thickness: "2".into(),
    round: false,
    direction: Default::default(),
    thickness_out: String::new(),
    bodies: Vec::new(),
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let corners = [[0.0, 0.0], [60.0, 0.0], [60.0, 40.0], [0.0, 40.0]];
  let edges = corners.iter().map(|c| edge_near(&m, [c[0], c[1], 15.0])).collect();
  d.add_feature(FeatureKind::Rip { edges, gap: String::new() }).unwrap();
  let body = m.bodies[0].id;
  d.add_feature(FeatureKind::ConvertToSheetMetal { body, face: None, rule: String::new(), bends: true }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 1);
  let cyl = m.bodies[0].faces().iter().filter(|f| f.kind == fox_kernel::SurfaceKind::Cylinder).count();
  assert_eq!(cyl, 8, "four bends, inside and outside");
  let flat = m.bodies[0].flat_pattern(None).unwrap();
  assert_eq!(flat.bends.len(), 4);
  let fm = flat.shape.mass().unwrap();
  let size = fm.bbox_max - fm.bbox_min;
  // outside dims kept: walls of 30 − (r + t) plus a bend allowance each side of the base
  let ba = PI / 2.0 * (3.0 + 0.5 * 2.0);
  let (lx, ly) = (60.0 - 10.0 + 2.0 * (ba + 25.0), 40.0 - 10.0 + 2.0 * (ba + 25.0));
  assert!((size.x - lx).abs() < 1e-3 && (size.y - ly).abs() < 1e-3 && (size.z - 2.0).abs() < 1e-4, "{size:?}");
}

#[test]
fn refold_carries_cuts_made_in_the_flat() {
  let mut d = sheet_design();
  plate_flange(&mut d);
  let m = regenerate(&mut d);
  let e = edge_near(&m, [50.0, 50.0, 2.0]);
  flange_on(&mut d, vec![e], ("0", "0"));
  let m = regenerate(&mut d);
  let folded = volume(&m);
  let body = m.bodies[0].id;
  d.add_feature(FeatureKind::Unfold { body, face: None, keep_folded: false }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  // a slot across the bend zone of the flat (it starts at y = 50, is 2π long)
  let fm = m.bodies[0].shape.mass().unwrap();
  let mut slot = Sketch::new();
  slot.add_rectangle([40.0, 45.0], [60.0, 60.0]).unwrap();
  let s = d.add_feature(FeatureKind::Sketch { plane: XY, sketch: slot, links: Vec::new() }).unwrap();
  d.add_feature(FeatureKind::Extrude {
    sketch: s,
    profiles: vec![],
    extent: Extent { kind: ExtentKind::Symmetric, ..Extent::distance("10") },
    operation: Operation::Cut,
  })
  .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let cut = fm.volume - volume(&m);
  assert!((cut - 20.0 * 15.0 * 2.0).abs() < 1e-3, "{cut}");
  d.add_feature(FeatureKind::Refold { body }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let mm = m.bodies[0].shape.mass().unwrap();
  assert!((mm.bbox_max.z - 20.0).abs() < 1e-3, "folded again: {mm:?}");
  // K = 0.5: the folded cut has the flat cut's volume
  assert!(((folded - mm.volume) - cut).abs() / cut < 0.01, "{} vs {cut}", folded - mm.volume);
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

#[test]
fn flanges_from_separate_features_leave_an_open_corner() {
  let mut d = sheet_design();
  plate_flange(&mut d);
  let m = regenerate(&mut d);
  let first = flange_on(&mut d, vec![edge_near(&m, [50.0, 50.0, 2.0])], ("0", "0"));
  let FeatureKind::EdgeFlange { position, .. } = &mut d.feature_mut(first).unwrap().kind else { panic!() };
  *position = BendPosition::Inside;
  let m = regenerate(&mut d);
  ok(&m);
  // the second flange's edge now ends where the first flange's bend begins
  let e = m.bodies[0]
    .edges()
    .iter()
    .filter(|e| e.kind == fox_kernel::CurveKind::Line && (e.mid.x - 100.0).abs() < 1e-6 && (e.mid.z - 2.0).abs() < 1e-6)
    .map(|e| EdgeRef::from_edge(m.bodies[0].id, e, m.bodies[0].faces()))
    .next()
    .unwrap();
  let second = flange_on(&mut d, vec![e], ("0", "0"));
  let FeatureKind::EdgeFlange { position, .. } = &mut d.feature_mut(second).unwrap().kind else { panic!() };
  *position = BendPosition::Inside;
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 1);
  let flat = m.bodies[0].flat_pattern(None).unwrap();
  assert_eq!(flat.bends.len(), 2);
}
