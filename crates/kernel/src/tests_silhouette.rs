use super::*;

// Total length of the silhouette's edges, and whether they all lie on the plane.
fn outline(s: &Shape, plane: &Plane) -> (f64, Vec<EdgeInfo>) {
  let edges = s.silhouette(plane).unwrap().edges().unwrap();
  let n = plane.normal();
  for e in &edges {
    for p in [e.start, e.mid, e.end] {
      assert!((p - plane.origin).dot(n).abs() < 1e-6, "{p} off the plane");
    }
  }
  (edges.iter().map(|e| e.length).sum(), edges)
}

#[test]
fn plate_with_hole_shows_its_outline_once() {
  let plate = Shape::make_box(DVec3::ZERO, DVec3::new(30.0, 20.0, 5.0)).unwrap();
  let hole = Shape::cylinder(DVec3::new(15.0, 10.0, -1.0), DVec3::Z, 4.0, 7.0).unwrap();
  let s = plate.boolean(&hole, BoolOp::Subtract).unwrap();
  // top and bottom loops land on each other and appear once
  let (len, edges) = outline(&s, &Plane::XY.offset(20.0));
  assert!((len - (100.0 + std::f64::consts::TAU * 4.0)).abs() < 1e-6, "{len}");
  assert!(edges.iter().any(|e| e.kind == CurveKind::Circle && (e.radius - 4.0).abs() < 1e-9));
}

#[test]
fn cylinder_on_its_side_shows_a_rectangle() {
  let c = Shape::cylinder(DVec3::ZERO, DVec3::X, 5.0, 20.0).unwrap();
  let (len, edges) = outline(&c, &Plane::XY.offset(20.0));
  assert!((len - 60.0).abs() < 1e-6, "{len}");
  assert!(edges.iter().all(|e| e.kind == CurveKind::Line), "{edges:#?}");
  // seen end on it is a circle
  let (len, _) = outline(&c, &Plane::YZ.offset(-3.0));
  assert!((len - std::f64::consts::TAU * 5.0).abs() < 1e-6, "{len}");
}

#[test]
fn sphere_shows_its_great_circle() {
  let s = Shape::sphere(DVec3::new(1.0, 2.0, 3.0), 6.0).unwrap();
  let plane = Plane::from_normal(DVec3::new(0.0, 0.0, -10.0), DVec3::new(1.0, 1.0, 1.0), DVec3::X).unwrap();
  let (len, _) = outline(&s, &plane);
  assert!((len - std::f64::consts::TAU * 6.0).abs() < 1e-3, "{len}");
}
