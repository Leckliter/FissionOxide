use super::*;
use crate::*;
use fox_sketch::Sketch;
use std::f64::consts::PI;

/// An x by y by z box at the origin (parameter `h` is 20 mm).
fn block(x: f64, y: f64, z: &str) -> Design {
  let mut d = Design::new("block");
  d.add_parameter("h", "20 mm", ParamKind::Length, "").unwrap();
  let mut s = Sketch::new();
  s.add_rectangle([0.0, 0.0], [x, y]).unwrap();
  let plane = PlaneRef::Origin { plane: OriginPlane::Xy };
  let s = d.add_feature(FeatureKind::Sketch { plane, sketch: s, links: Vec::new() }).unwrap();
  let extent = Extent::distance(z);
  d.add_feature(FeatureKind::Extrude {
    sketch: Some(s),
    faces: vec![],
    profiles: vec![],
    extent,
    operation: Operation::NewBody,
  })
  .unwrap();
  d
}

fn built(d: &mut Design) -> (f64, usize) {
  let m = regenerate(d);
  assert!(m.errors().is_empty(), "{:?}", m.errors());
  assert_eq!(m.bodies.len(), 1);
  (m.bodies[0].shape.mass().unwrap().volume, m.bodies[0].faces().len())
}

fn edge_at(d: &mut Design, at: [f64; 3]) -> EdgeRef {
  let m = regenerate(d);
  let b = &m.bodies[0];
  let e = b.edges().iter().find(|e| (e.mid - fox_kernel::glam::DVec3::from(at)).length() < 1e-6).unwrap();
  EdgeRef::from_edge(b.id, e, b.faces())
}

fn face_at(d: &mut Design, normal: [f64; 3]) -> FaceRef {
  let m = regenerate(d);
  let b = &m.bodies[0];
  let f = b.faces().iter().find(|f| f.normal.dot(normal.into()) > 0.999).unwrap();
  FaceRef::from_face(b.id, f, f.center)
}

fn fillet(edges: Vec<EdgeRef>, radius: &str, distance2: Option<&str>, flipped: bool) -> FeatureKind {
  FeatureKind::Fillet {
    edges,
    radius: radius.into(),
    radius2: None,
    setback: None,
    chord: false,
    radii: vec![],
    sets: vec![],
    distance2: distance2.map(Into::into),
    flipped,
  }
}

#[test]
fn asymmetric_fillet_cuts_a_quarter_ellipse() {
  let mut d = block(40.0, 30.0, "20");
  let e = edge_at(&mut d, [20.0, 0.0, 20.0]);
  // the front face (normal -y) sorts before the top: 6 down the front, 2 into the top
  let f = d.add_feature(fillet(vec![e], "6", Some("2"), false)).unwrap();
  let (v, faces) = built(&mut d);
  assert!((v - (24000.0 - 12.0 * (1.0 - PI / 4.0) * 40.0)).abs() < 1e-3, "{v}");
  assert_eq!(faces, 7);
  let m = regenerate(&mut d);
  let top = m.bodies[0].faces().iter().find(|f| f.normal.z > 0.999).unwrap().area;
  assert!((top - 40.0 * 28.0).abs() < 1e-6, "{top}");
  // flipped: 2 down the front, 6 into the top
  if let Some(FeatureKind::Fillet { flipped, .. }) = d.feature_mut(f).map(|f| &mut f.kind) {
    *flipped = true;
  }
  let m = regenerate(&mut d);
  let top = m.bodies[0].faces().iter().find(|f| f.normal.z > 0.999).unwrap().area;
  assert!((top - 40.0 * 24.0).abs() < 1e-6, "{top}");
}

#[test]
fn equal_offsets_match_the_circular_fillet() {
  let mut a = block(40.0, 30.0, "20");
  let e = edge_at(&mut a, [20.0, 0.0, 20.0]);
  a.add_feature(fillet(vec![e.clone()], "3", Some("3"), false)).unwrap();
  let mut b = block(40.0, 30.0, "20");
  b.add_feature(fillet(vec![e], "3", None, false)).unwrap();
  let ((va, fa), (vb, fb)) = (built(&mut a), built(&mut b));
  assert!((va - vb).abs() < 1e-3, "{va} vs {vb}");
  assert_eq!(fa, fb);
}

#[test]
fn full_round_fillet_rounds_a_rib_top() {
  let mut d = block(40.0, 10.0, "h");
  let sets = vec![FullRoundSet {
    center: face_at(&mut d, [0.0, 0.0, 1.0]),
    side_one: vec![face_at(&mut d, [0.0, -1.0, 0.0])],
    side_two: vec![face_at(&mut d, [0.0, 1.0, 0.0])],
  }];
  d.add_feature(FeatureKind::FullRoundFillet { sets }).unwrap();
  let (v, faces) = built(&mut d);
  assert!((v - (40.0 * 10.0 * 15.0 + PI * 12.5 * 40.0)).abs() < 1e-3, "{v}");
  assert_eq!(faces, 6);
  // a taller rib keeps the round on top
  d.set_parameter("h", "30").unwrap();
  let (v, faces) = built(&mut d);
  assert!((v - (40.0 * 10.0 * 25.0 + PI * 12.5 * 40.0)).abs() < 1e-3, "{v}");
  assert_eq!(faces, 6);
}
