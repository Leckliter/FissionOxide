use super::*;

// An r10 h30 cylinder on XY, its side face, and an r5 circle on the plane x = 20 centred at
// (y 0, z 15), as a wire.
fn setup() -> (Shape, Shape, Shape) {
  let cyl = Shape::cylinder(DVec3::ZERO, DVec3::Z, 10.0, 30.0).unwrap();
  let side = cyl.faces().unwrap().into_iter().find(|f| f.kind == SurfaceKind::Cylinder).unwrap().id;
  let side = cyl.face_shape(side).unwrap();
  let plane = Plane::from_normal(DVec3::new(20.0, 0.0, 0.0), DVec3::X, DVec3::Y).unwrap();
  let circle = Shape::path(&plane, &[Seg::Circle { center: [0.0, 15.0], radius: 5.0, ccw: true }]).unwrap();
  (cyl, side, circle)
}

fn groove(cyl: &Shape, loops: &[Shape]) -> Shape {
  let mut body = cyl.clone();
  for l in loops {
    let pe = l.path_ends().unwrap();
    let n = l.edge_lines(0.1).unwrap().len();
    let pipe0 = l.round_pipe(1.0, 0.0);
    eprintln!("loop edges {n} ends {pe:?} pipe {:?}", pipe0.as_ref().map(|p| (p.is_valid(), p.mass().map(|m| m.volume).ok(), p.faces().map(|f| f.len()).ok())));
    assert!(pe.closed);
    let pipe = l.round_pipe(1.0, 0.0).unwrap();
    assert!(pipe.is_valid());
    body = body.boolean(&pipe, BoolOp::Subtract).unwrap();
  }
  assert!(body.is_valid());
  body
}

#[test]
fn along_a_vector_gives_two_saddles() {
  let (cyl, side, circle) = setup();
  let loops = Shape::project_to_faces(&[circle], &[side], Projection3d::Along(DVec3::X), DVec3::X).unwrap();
  assert_eq!(loops.len(), 2);
  for l in &loops {
    // every point lies on the cylinder with y, z on the circle
    for e in l.edge_lines(0.01).unwrap() {
      for p in e.points {
        assert!((p.x.hypot(p.y) - 10.0).abs() < 1e-3, "{p}");
        assert!((p.y.hypot(p.z - 15.0) - 5.0).abs() < 1e-3, "{p}");
      }
    }
  }
  let body = groove(&cyl, &loops);
  let m = body.mass().unwrap();
  // Fusion: 9326.976 mm³, 2576.520 mm²
  assert!((m.volume - 9326.976).abs() < 1.0, "{}", m.volume);
  assert!((m.area - 2576.520).abs() < 1.0, "{}", m.area);
  eprintln!("along: faces {}", body.faces().unwrap().len());
}

#[test]
fn to_the_closest_point_gives_one_loop() {
  let (cyl, side, circle) = setup();
  let loops = Shape::project_to_faces(&[circle], &[side], Projection3d::ClosestPoint, DVec3::X).unwrap();
  assert_eq!(loops.len(), 1);
  for e in loops[0].edge_lines(0.01).unwrap() {
    for p in e.points {
      assert!((p.x.hypot(p.y) - 10.0).abs() < 1e-3, "{p}");
      assert!(p.x > 0.0, "{p}");
    }
  }
  let body = groove(&cyl, &loops);
  let m = body.mass().unwrap();
  // Fusion: 9388.016 mm³, 2537.638 mm²
  assert!((m.volume - 9388.016).abs() < 1.0, "{}", m.volume);
  assert!((m.area - 2537.638).abs() < 1.0, "{}", m.area);
  eprintln!("closest: faces {}", body.faces().unwrap().len());
}
