use super::*;

fn edge_at(s: &Shape, p: DVec3) -> u32 {
  s.edges().unwrap().iter().find(|e| (e.mid - p).length() < 1e-6).unwrap().id
}

fn face_at(s: &Shape, p: DVec3) -> u32 {
  s.faces().unwrap().iter().find(|f| (f.center - p).length() < 1e-6).unwrap().id
}

#[test]
fn a_tangent_patch_domes_over_a_cone() {
  // a cone narrowing from radius 10 at z = 0 to 5 at z = 10, open at both ends
  let xz = Plane::from_normal(DVec3::ZERO, DVec3::NEG_Y, DVec3::X).unwrap();
  let side = Shape::path(&xz, &[Seg::Line { a: [10.0, 0.0], b: [5.0, 10.0] }]).unwrap();
  let cone = side.revolve(DVec3::ZERO, DVec3::Z, std::f64::consts::TAU).unwrap();
  let rim = cone.edges().unwrap().iter().find(|e| (e.center.z - 10.0).abs() < 1e-6 && e.radius > 4.0).unwrap().id;
  let wall = cone.faces().unwrap()[0].id;
  let edge = cone.edge_shape(rim).unwrap();
  let flat = Shape::patch_with(&[(edge.clone(), Continuity::Connected, None)], &[]).unwrap();
  let disc = std::f64::consts::PI * 25.0;
  assert!((flat.mass().unwrap().area - disc).abs() < 0.5, "{}", flat.mass().unwrap().area);
  // tangent to the cone: the patch carries on inwards and up, a dome
  let dome = Shape::patch_with(&[(edge, Continuity::Tangent, Some(cone.face_shape(wall).unwrap()))], &[]).unwrap();
  assert!(dome.mass().unwrap().area > disc + 5.0, "{}", dome.mass().unwrap().area);
}

#[test]
fn ruled_from_an_edge_stands_up_or_runs_on() {
  let b = Shape::make_box(DVec3::ZERO, DVec3::new(20.0, 10.0, 5.0)).unwrap();
  let e = b.edge_shape(edge_at(&b, DVec3::new(10.0, 0.0, 5.0))).unwrap();
  let top = b.face_shape(face_at(&b, DVec3::new(10.0, 5.0, 5.0))).unwrap();
  // along the top face's normal: a 20 × 4 wall standing on the edge
  let wall = e.ruled_from_edge(&top, 4.0, 0.0, false).unwrap();
  let f = wall.faces().unwrap()[0];
  assert!((f.area - 80.0).abs() < 1e-3, "{}", f.area);
  assert!((f.center - DVec3::new(10.0, 0.0, 7.0)).length() < 1e-6, "{}", f.center);
  // along the face, away from it: the top runs on to y = -4
  let run = e.ruled_from_edge(&top, 4.0, 0.0, true).unwrap();
  let f = run.faces().unwrap()[0];
  assert!((f.center - DVec3::new(10.0, -2.0, 5.0)).length() < 1e-6, "{}", f.center);
  // turned a right angle about the edge, the normal wall lies along the face
  let turned = e.ruled_from_edge(&top, 4.0, std::f64::consts::FRAC_PI_2, false).unwrap();
  let c = turned.faces().unwrap()[0].center;
  assert!((c.z - 5.0).abs() < 1e-6 && (c.y.abs() - 2.0).abs() < 1e-6, "{c}");
}

#[test]
fn open_sweep_and_loft_make_surfaces_from_curves() {
  let line = |pl: &Plane, a: [f64; 2], b: [f64; 2]| Shape::path(pl, &[Seg::Line { a, b }]).unwrap();
  // a 5 tall upright line swept along an L of 10 + 10
  let xz = Plane::from_normal(DVec3::ZERO, DVec3::Y, DVec3::X).unwrap();
  let profile = line(&xz, [0.0, 0.0], [0.0, -5.0]);
  let path = Shape::path(
    &Plane::XY,
    &[Seg::Line { a: [0.0, 0.0], b: [10.0, 0.0] }, Seg::Line { a: [10.0, 0.0], b: [10.0, 10.0] }],
  )
  .unwrap();
  let swept = profile.sweep_open(&path, false).unwrap();
  assert_eq!(swept.solid_count(), 0);
  assert!((swept.mass().unwrap().area - 100.0).abs() < 1e-3, "{}", swept.mass().unwrap().area);
  // two parallel lines 6 apart: a 10 × 6 strip
  let a = line(&Plane::XY, [0.0, 0.0], [10.0, 0.0]);
  let b = line(&Plane::XY.offset(6.0), [0.0, 0.0], [10.0, 0.0]);
  let loft = Shape::loft_open(&[a, b], false).unwrap();
  assert_eq!(loft.solid_count(), 0);
  assert!((loft.mass().unwrap().area - 60.0).abs() < 1e-3, "{}", loft.mass().unwrap().area);
}
