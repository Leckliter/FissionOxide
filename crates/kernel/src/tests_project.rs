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

// The cylinder with a d2 groove cut along each loop, its faces split at the tubes' seams.
fn groove(cyl: &Shape, loops: &[Shape]) -> Shape {
  let mut body = cyl.clone();
  for l in loops {
    assert!(l.path_ends().unwrap().closed);
    let pipe = l.round_pipe(1.0, 0.0).unwrap();
    assert!(pipe.is_valid());
    // a closed tube: two half tubes, no caps
    assert_eq!(pipe.faces().unwrap().len(), 2);
    body = body.boolean(&pipe, BoolOp::Subtract).unwrap();
  }
  let body = body.split_seams().unwrap();
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
  // Fusion: 9326.976 mm³, 2576.520 mm², 13 faces (each groove in four: halves, and the tube's
  // seam runs inside it)
  assert!((m.volume - 9326.976).abs() < 5.0, "{}", m.volume);
  assert!((m.area - 2576.520).abs() < 2.0, "{}", m.area);
  assert_eq!(body.faces().unwrap().len(), 13);
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
  // Fusion: 9388.016 mm³, 2537.638 mm², 6 faces (the tube's seam runs outside the groove)
  assert!((m.volume - 9388.016).abs() < 2.0, "{}", m.volume);
  assert!((m.area - 2537.638).abs() < 2.0, "{}", m.area);
  assert_eq!(body.faces().unwrap().len(), 6);
}

#[test]
fn mass_is_exact_where_a_cylinder_is_trimmed_by_free_form_edges() {
  // the groove's trim crosses the cylinder's seam, where default Gauss integration was ~12 mm³
  // off; turning the cylinder moves the seam and must not change the volume
  let (cyl, side, circle) = setup();
  let loops = Shape::project_to_faces(&[circle], &[side], Projection3d::ClosestPoint, DVec3::X).unwrap();
  let pipe = loops[0].round_pipe(1.0, 0.0).unwrap();
  let a = cyl.boolean(&pipe, BoolOp::Subtract).unwrap().mass().unwrap().volume;
  let turned = cyl.rotated(DVec3::ZERO, DVec3::Z, 2.0).unwrap();
  let b = turned.boolean(&pipe, BoolOp::Subtract).unwrap().mass().unwrap().volume;
  assert!((a - b).abs() < 0.05, "{a} vs {b}");
}
