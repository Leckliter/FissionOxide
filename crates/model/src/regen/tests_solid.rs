use super::*;
use crate::*;
use fox_kernel::glam::DVec3;
use fox_sketch::{PtSpec, Sketch};
use std::f64::consts::PI;

const XY: PlaneRef = PlaneRef::Origin { plane: OriginPlane::Xy };
const XZ: PlaneRef = PlaneRef::Origin { plane: OriginPlane::Xz };
const YZ: PlaneRef = PlaneRef::Origin { plane: OriginPlane::Yz };

fn ok(m: &Model) {
  assert!(m.errors().is_empty(), "{:?}", m.errors());
}

fn vol(b: &Body) -> f64 {
  b.shape.mass().unwrap().volume
}

fn close(a: f64, b: f64, rel: f64) -> bool {
  (a - b).abs() <= rel * b.abs()
}

fn sketch(d: &mut Design, plane: PlaneRef, sk: Sketch) -> FeatureId {
  d.add_feature(FeatureKind::Sketch { plane, sketch: sk, links: Vec::new() }).unwrap()
}

fn circle(r: f64) -> Sketch {
  let mut s = Sketch::new();
  s.add_circle(PtSpec::At(0.0, 0.0), r).unwrap();
  s
}

fn rect(a: [f64; 2], b: [f64; 2]) -> Sketch {
  let mut s = Sketch::new();
  s.add_rectangle(a, b).unwrap();
  s
}

fn offset_plane(d: &mut Design, base: PlaneRef, offset: &str) -> PlaneRef {
  let f = d.add_feature(FeatureKind::ConstructionPlane { base, offset: offset.into() }).unwrap();
  PlaneRef::Construction { feature: f }
}

fn primitive(d: &mut Design, shape: Primitive, plane: PlaneRef, at: [f64; 2]) -> FeatureId {
  d.add_feature(FeatureKind::Primitive { shape, plane, at, operation: Operation::NewBody }).unwrap()
}

fn cube(d: &mut Design, at: [f64; 2], size: &str) -> FeatureId {
  let s = size.to_string();
  primitive(d, Primitive::Box { width: s.clone(), depth: s.clone(), height: s }, XY, at)
}

fn bodies_of(m: &Model, f: FeatureId) -> Vec<&Body> {
  m.bodies.iter().filter(|b| b.feature == f).collect()
}

#[test]
fn sweep_along_a_rail_then_twisted() {
  let mut d = Design::new("sweep");
  let prof = sketch(&mut d, XY, circle(5.0));
  // path and rail in one sketch on the front plane (sketch x = X, sketch y = Z)
  let mut ps = Sketch::new();
  let path = ps.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(0.0, 50.0)).unwrap().0;
  let rail = ps.add_line(PtSpec::At(5.0, 0.0), PtSpec::At(10.0, 50.0)).unwrap().0;
  let path_sketch = sketch(&mut d, XZ, ps);
  let sw = d
    .add_feature(FeatureKind::Sweep {
      sketch: prof,
      profiles: vec![],
      path_sketch,
      path_curves: vec![path],
      operation: Operation::NewBody,
      rail: Some(PathRef { sketch: path_sketch, curves: vec![rail] }),
      twist: String::new(),
    })
    .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let frustum = PI * 50.0 / 3.0 * (25.0 + 50.0 + 100.0);
  assert!(close(vol(bodies_of(&m, sw)[0]), frustum, 0.01), "{}", vol(&m.bodies[0]));
  assert!(d.feature(sw).unwrap().kind.depends_on().contains(&path_sketch));
  // a rail and a twist together are an error, shown on the feature
  if let Some(FeatureKind::Sweep { twist, .. }) = d.feature_mut(sw).map(|f| &mut f.kind) {
    *twist = "90 deg".into();
  }
  let m = regenerate(&mut d);
  assert!(m.status(sw).unwrap().error.as_deref().is_some_and(|e| e.contains("twist")));
  // twisted without the rail: a rectangle turned a quarter keeps its area all the way up
  let bar = sketch(&mut d, XY, rect([-10.0, -5.0], [10.0, 5.0]));
  if let Some(FeatureKind::Sweep { rail, sketch, .. }) = d.feature_mut(sw).map(|f| &mut f.kind) {
    *rail = None;
    *sketch = bar;
  }
  d.move_feature(bar, 0).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let b = bodies_of(&m, sw)[0];
  assert!(close(vol(b), 10_000.0, 0.01), "{}", vol(b));
  assert!(b.shape.mass().unwrap().bbox_max.y > 8.0);
}

#[test]
fn loft_with_rails_or_a_centre_line() {
  let mut d = Design::new("loft");
  let a = sketch(&mut d, XY, circle(10.0));
  let top = offset_plane(&mut d, XY, "40");
  let b = sketch(&mut d, top, circle(10.0));
  // two rails bulging out to x = ±15 at mid-height, and a straight centre line
  let mut rs = Sketch::new();
  let r1 = rs.add_arc_3pt(PtSpec::At(10.0, 0.0), [15.0, 20.0], PtSpec::At(10.0, 40.0)).unwrap();
  let r2 = rs.add_arc_3pt(PtSpec::At(-10.0, 0.0), [-15.0, 20.0], PtSpec::At(-10.0, 40.0)).unwrap();
  let c = rs.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(0.0, 40.0)).unwrap().0;
  let rails = sketch(&mut d, XZ, rs);
  let sections = vec![LoftSection { sketch: a, profile: None }, LoftSection { sketch: b, profile: None }];
  let loft = d
    .add_feature(FeatureKind::Loft {
      sections: sections.clone(),
      ruled: false,
      closed: false,
      operation: Operation::NewBody,
      rails: vec![PathRef { sketch: rails, curves: vec![r1] }, PathRef { sketch: rails, curves: vec![r2] }],
      centerline: None,
    })
    .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let mp = bodies_of(&m, loft)[0].shape.mass().unwrap();
  assert!((mp.bbox_max.x - 15.0).abs() < 0.3 && (mp.bbox_min.x + 15.0).abs() < 0.3, "{mp:?}");
  assert!((mp.bbox_max.y - 10.0).abs() < 0.3, "{mp:?}");
  assert!(mp.volume > PI * 100.0 * 40.0 * 1.1);
  // rails and a centre line together are rejected
  let center = PathRef { sketch: rails, curves: vec![c] };
  if let Some(FeatureKind::Loft { centerline, .. }) = d.feature_mut(loft).map(|f| &mut f.kind) {
    *centerline = Some(center.clone());
  }
  let m = regenerate(&mut d);
  assert!(m.status(loft).unwrap().error.is_some());
  // a centre line alone: a cylinder
  if let Some(FeatureKind::Loft { rails, .. }) = d.feature_mut(loft).map(|f| &mut f.kind) {
    rails.clear();
  }
  let m = regenerate(&mut d);
  ok(&m);
  assert!(close(vol(bodies_of(&m, loft)[0]), PI * 100.0 * 40.0, 0.01));
}

#[test]
fn setback_fillet_at_a_box_corner() {
  let mut d = Design::new("setback");
  let bx = cube(&mut d, [0.0, 0.0], "20");
  let m = regenerate(&mut d);
  let body = &m.bodies[0];
  let corner = DVec3::new(10.0, 10.0, 20.0);
  let edges: Vec<EdgeRef> = body
    .edges()
    .iter()
    .filter(|e| (e.start - corner).length() < 1e-6 || (e.end - corner).length() < 1e-6)
    .map(|e| EdgeRef::from_edge(body.id, e, body.faces()))
    .collect();
  assert_eq!(edges.len(), 3);
  let f = d
    .add_feature(FeatureKind::Fillet {
      edges,
      radius: "3".into(),
      radius2: None,
      setback: None,
      chord: false,
      radii: vec![],
      sets: vec![],
    })
    .unwrap();
  let plain = vol(&regenerate(&mut d).bodies[0]);
  if let Some(FeatureKind::Fillet { setback, .. }) = d.feature_mut(f).map(|f| &mut f.kind) {
    *setback = Some("7 mm".into());
  }
  let m = regenerate(&mut d);
  ok(&m);
  let b = &m.bodies[0];
  assert!(b.shape.is_valid() && vol(b) < plain - 0.5 && vol(b) > plain - 60.0, "{} vs {plain}", vol(b));
  assert_eq!(b.feature, f);
  // two of the edges only: no corner to set back
  if let Some(FeatureKind::Fillet { edges, .. }) = d.feature_mut(f).map(|f| &mut f.kind) {
    edges.pop();
  }
  let m = regenerate(&mut d);
  assert!(m.status(f).unwrap().error.as_deref().is_some_and(|e| e.contains("three")));
  let _ = bx;
}

/// A cylinder of radius 20 lying along X (x = 0..40), and a 20 × 10 rectangle on a plane 40 mm
/// below its axis.
fn bar_under_a_cylinder() -> (Design, FeatureId, FeatureId) {
  let mut d = Design::new("to object");
  let cyl = primitive(&mut d, Primitive::Cylinder { radius: "20".into(), height: "40".into() }, YZ, [0.0, 0.0]);
  let below = offset_plane(&mut d, XY, "-40");
  let sk = sketch(&mut d, below, rect([10.0, -5.0], [30.0, 5.0]));
  (d, cyl, sk)
}

fn extrude_to(d: &mut Design, sk: FeatureId, kind: ExtentKind, to: FaceRef, distance: &str) -> FeatureId {
  d.add_feature(FeatureKind::Extrude {
    sketch: sk,
    profiles: vec![],
    extent: Extent { kind, to: Some(to), ..Extent::distance(distance) },
    operation: Operation::NewBody,
  })
  .unwrap()
}

#[test]
fn extrude_up_to_a_curved_face_or_a_body() {
  let (mut d, cyl, sk) = bar_under_a_cylinder();
  let m = regenerate(&mut d);
  let body = &m.bodies[0];
  let side = body.faces().iter().find(|f| f.kind == fox_kernel::SurfaceKind::Cylinder).unwrap();
  let to = FaceRef::from_face(body.id, side, DVec3::new(20.0, 0.0, -20.0));
  let e = extrude_to(&mut d, sk, ExtentKind::ToFace, to.clone(), "0");
  let m = regenerate(&mut d);
  ok(&m);
  // from z = −40 up to z = −√(400 − y²) over |y| <= 5, 20 long
  let under = 5.0 * 375.0f64.sqrt() + 400.0 * 0.25f64.asin();
  let expected = 20.0 * (400.0 - under);
  assert!(close(vol(bodies_of(&m, e)[0]), expected, 1e-3), "{} vs {expected}", vol(bodies_of(&m, e)[0]));
  // 2 mm past the face
  if let Some(FeatureKind::Extrude { extent, .. }) = d.feature_mut(e).map(|f| &mut f.kind) {
    extent.distance = "2".into();
  }
  let m = regenerate(&mut d);
  ok(&m);
  let more = vol(bodies_of(&m, e)[0]) - expected;
  assert!(more > 380.0 && more < 430.0, "{more}");
  // up to the body: the same as to its face here
  if let Some(FeatureKind::Extrude { extent, .. }) = d.feature_mut(e).map(|f| &mut f.kind) {
    extent.kind = ExtentKind::ToBody;
  }
  let m = regenerate(&mut d);
  ok(&m);
  assert!(close(vol(bodies_of(&m, e)[0]), expected, 1e-3));
  assert_eq!(bodies_of(&m, cyl).len(), 1);
  // a profile that misses the target
  let (mut d, _, _) = bar_under_a_cylinder();
  let far = offset_plane(&mut d, XY, "-40");
  let sk2 = sketch(&mut d, far, rect([100.0, -5.0], [120.0, 5.0]));
  let m = regenerate(&mut d);
  let body = &m.bodies[0];
  let side = body.faces().iter().find(|f| f.kind == fox_kernel::SurfaceKind::Cylinder).unwrap();
  let to = FaceRef::from_face(body.id, side, DVec3::new(20.0, 0.0, -20.0));
  let e = extrude_to(&mut d, sk2, ExtentKind::ToBody, to, "0");
  let m = regenerate(&mut d);
  assert!(m.status(e).unwrap().error.is_some());
}

/// A cylinder r = 20, 40 tall on the XY plane, a 10 × 10 square sketched on a plane in front of
/// it (y = −30), and its side face.
fn cylinder_and_square() -> (Design, FeatureId, FaceRef, f64) {
  let mut d = Design::new("emboss");
  primitive(&mut d, Primitive::Cylinder { radius: "20".into(), height: "40".into() }, XY, [0.0, 0.0]);
  let front = offset_plane(&mut d, XZ, "30");
  let sk = sketch(&mut d, front, rect([-5.0, 15.0], [5.0, 25.0]));
  let m = regenerate(&mut d);
  ok(&m);
  let body = &m.bodies[0];
  let side = body.faces().iter().find(|f| f.kind == fox_kernel::SurfaceKind::Cylinder).unwrap();
  (d.clone(), sk, FaceRef::from_face(body.id, side, DVec3::new(0.0, -20.0, 20.0)), vol(body))
}

#[test]
fn emboss_and_deboss_on_a_cylinder() {
  let (mut d, sk, face, v0) = cylinder_and_square();
  let e = d
    .add_feature(FeatureKind::Emboss {
      sketch: sk,
      profiles: vec![],
      face,
      depth: "2 mm".into(),
      deboss: false,
      wrap: true,
      horizontal: "0".into(),
      vertical: "0".into(),
      rotation: "0".into(),
    })
    .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 1);
  // 10 mm of arc = half a radian at r = 20
  let raised = (22.0f64.powi(2) - 400.0) / 2.0 * 0.5 * 10.0;
  assert!(close(vol(&m.bodies[0]) - v0, raised, 0.01), "{}", vol(&m.bodies[0]) - v0);
  assert_eq!(d.feature(e).unwrap().kind.base_name(), "Emboss");
  if let Some(FeatureKind::Emboss { deboss, .. }) = d.feature_mut(e).map(|f| &mut f.kind) {
    *deboss = true;
  }
  let m = regenerate(&mut d);
  ok(&m);
  let sunk = (400.0 - 18.0f64.powi(2)) / 2.0 * 0.5 * 10.0;
  assert!(close(v0 - vol(&m.bodies[0]), sunk, 0.01), "{}", v0 - vol(&m.bodies[0]));
  // projected instead of wrapped: straight sides, a little less than the wrapped arc
  if let Some(FeatureKind::Emboss { deboss, wrap, .. }) = d.feature_mut(e).map(|f| &mut f.kind) {
    *deboss = false;
    *wrap = false;
  }
  let m = regenerate(&mut d);
  ok(&m);
  let up = vol(&m.bodies[0]) - v0;
  assert!(up > 190.0 && up < raised, "{up}");
  // the depth follows a parameter
  d.add_parameter("depth", "1 mm", ParamKind::Length, "").unwrap();
  if let Some(FeatureKind::Emboss { depth, .. }) = d.feature_mut(e).map(|f| &mut f.kind) {
    *depth = "depth".into();
  }
  let m = regenerate(&mut d);
  ok(&m);
  assert!(vol(&m.bodies[0]) - v0 < up * 0.6);
}

#[test]
fn boundary_fill_keeps_the_picked_cells() {
  let mut d = Design::new("fill");
  let a = cube(&mut d, [0.0, 0.0], "20");
  let b = cube(&mut d, [10.0, 0.0], "20");
  let m = regenerate(&mut d);
  let tools: Vec<BodyId> = m.bodies.iter().map(|b| b.id).collect();
  let f = d
    .add_feature(FeatureKind::BoundaryFill {
      tools: tools.clone(),
      planes: vec![],
      cells: vec![[5.0, 0.0, 10.0]],
      operation: Operation::NewBody,
      remove_tools: true,
    })
    .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 1);
  assert!((vol(&m.bodies[0]) - 4000.0).abs() < 1e-3);
  // every cell, keeping the tools
  if let Some(FeatureKind::BoundaryFill { cells, remove_tools, .. }) = d.feature_mut(f).map(|f| &mut f.kind) {
    cells.clear();
    *remove_tools = false;
  }
  let m = regenerate(&mut d);
  ok(&m);
  assert_eq!(m.bodies.len(), 3);
  assert!((vol(bodies_of(&m, f)[0]) - 12_000.0).abs() < 1e-3);
  // a body and a plane: the lower cell only
  let mut d = Design::new("fill2");
  cube(&mut d, [0.0, 0.0], "20");
  let cut = offset_plane(&mut d, XY, "5");
  let m = regenerate(&mut d);
  let f = d
    .add_feature(FeatureKind::BoundaryFill {
      tools: vec![m.bodies[0].id],
      planes: vec![cut],
      cells: vec![[0.0, 0.0, 2.0], [100.0, 0.0, 0.0]],
      operation: Operation::NewBody,
      remove_tools: true,
    })
    .unwrap();
  let m = regenerate(&mut d);
  assert!(m.status(f).unwrap().warning.is_some(), "one picked point is in no cell");
  assert!((vol(&m.bodies[0]) - 2000.0).abs() < 1e-3);
  let _ = (a, b);
}

/// The 40 × 20 × 10 block from the origin, and its top face.
fn block() -> (Design, FaceRef) {
  let mut d = Design::new("block");
  let s = sketch(&mut d, XY, rect([0.0, 0.0], [40.0, 20.0]));
  d.add_feature(FeatureKind::Extrude {
    sketch: s,
    profiles: vec![],
    extent: Extent::distance("10"),
    operation: Operation::NewBody,
  })
  .unwrap();
  let m = regenerate(&mut d);
  let body = &m.bodies[0];
  let top = body.faces().iter().find(|f| f.normal.z > 0.9).unwrap();
  (d.clone(), FaceRef::from_face(body.id, top, top.center))
}

#[test]
fn replace_face_with_a_plane_or_a_curved_face() {
  let (mut d, top) = block();
  let up = offset_plane(&mut d, XY, "15");
  let r = d
    .add_feature(FeatureKind::ReplaceFace { faces: vec![top.clone()], target: None, plane: Some(up.clone()) })
    .unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((vol(&m.bodies[0]) - 12_000.0).abs() < 1e-3, "{}", vol(&m.bodies[0]));
  // lower than the face: the body shrinks
  let feature = if let PlaneRef::Construction { feature } = up { feature } else { FeatureId(0) };
  if let Some(FeatureKind::ConstructionPlane { offset, .. }) = d.feature_mut(feature).map(|f| &mut f.kind) {
    *offset = "6".into();
  }
  let m = regenerate(&mut d);
  ok(&m);
  assert!((vol(&m.bodies[0]) - 4800.0).abs() < 1e-3, "{}", vol(&m.bodies[0]));
  assert_eq!(m.bodies[0].feature, r);

  // a big cylinder along X whose top runs at z = 15 over y = 10
  let (mut d, top) = block();
  let cyl = primitive(&mut d, Primitive::Cylinder { radius: "50".into(), height: "60".into() }, YZ, [10.0, -35.0]);
  let m = regenerate(&mut d);
  let cb = bodies_of(&m, cyl)[0];
  let side = cb.faces().iter().find(|f| f.kind == fox_kernel::SurfaceKind::Cylinder).unwrap();
  let target = FaceRef::from_face(cb.id, side, DVec3::new(20.0, 10.0, 15.0));
  d.add_feature(FeatureKind::ReplaceFace { faces: vec![top], target: Some(target), plane: None }).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  let under = 10.0 * 2400.0f64.sqrt() + 2500.0 * 0.2f64.asin();
  let expected = 40.0 * (under - 700.0);
  let blk = m.bodies.iter().find(|b| b.id == m.bodies[0].id).unwrap();
  assert!(close(vol(blk), expected, 1e-3), "{} vs {expected}", vol(blk));
}

#[test]
fn old_files_open_without_the_new_fields() {
  let json = r#"{"type":"fillet","edges":[],"radius":"1"}"#;
  let k: FeatureKind = serde_json::from_str(json).unwrap();
  assert!(matches!(k, FeatureKind::Fillet { setback: None, .. }));
  let json = r#"{"type":"sweep","sketch":1,"path_sketch":2}"#;
  let k: FeatureKind = serde_json::from_str(json).unwrap();
  assert!(matches!(k, FeatureKind::Sweep { rail: None, ref twist, .. } if twist.is_empty()));
  let json = r#"{"type":"loft","sections":[]}"#;
  let k: FeatureKind = serde_json::from_str(json).unwrap();
  assert!(matches!(k, FeatureKind::Loft { ref rails, centerline: None, .. } if rails.is_empty()));
}
