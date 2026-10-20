use super::*;

// An r10 h30 cylinder on XY and the slanted line (-10, 5)-(10, 25) on XZ pushed through it along
// Y: the curve where they cross is a whole ellipse with semi-axes 10 and 10√2.
#[test]
fn slanted_line_through_a_cylinder_side() {
  let cyl = Shape::cylinder(DVec3::ZERO, DVec3::Z, 10.0, 30.0).unwrap();
  let side = cyl.faces().unwrap().into_iter().find(|f| f.kind == SurfaceKind::Cylinder).unwrap().id;
  let side = cyl.face_shape(side).unwrap();
  let plane = Plane { origin: DVec3::new(0.0, -50.0, 0.0), x: DVec3::X, y: DVec3::Z };
  let seg = Seg::Line { a: [-10.0, 5.0], b: [10.0, 25.0] };
  let wall = Shape::path(&plane, &[seg]).unwrap().extrude(DVec3::Y * 100.0).unwrap();
  let wires = wall.section_wires(&side, 1e-4).unwrap();
  assert_eq!(wires.len(), 1);
  let len: f64 = wires[0].edges().unwrap().iter().map(|e| e.length).sum();
  // Ramanujan's perimeter of an ellipse with a = 10√2, b = 10
  let (a, b) = (10.0 * 2f64.sqrt(), 10.0);
  let h = ((a - b) / (a + b)).powi(2);
  let ellipse = std::f64::consts::PI * (a + b) * (1.0 + 3.0 * h / (10.0 + (4.0 - 3.0 * h).sqrt()));
  assert!((len - ellipse).abs() < 1e-3 * ellipse, "{len} vs {ellipse}");
  let (start, dir) = wires[0].wire_start().unwrap();
  assert!((start.x.hypot(start.y) - 10.0).abs() < 1e-4);
  assert!((dir.length() - 1.0).abs() < 1e-9);
}

// A closed path restarts halfway along its first edge: one edge more, the same length.
#[test]
fn closed_wire_restarts_at_its_middle() {
  let seg = |a: [f64; 2], b: [f64; 2]| Seg::Line { a, b };
  let square = [seg([0.0, 0.0], [10.0, 0.0]), seg([10.0, 0.0], [10.0, 10.0]), seg([10.0, 10.0], [0.0, 10.0])];
  let mut closed = square.to_vec();
  closed.push(seg([0.0, 10.0], [0.0, 0.0]));
  let w = Shape::path(&Plane::XY, &closed).unwrap().wire_from_middle().unwrap();
  let edges = w.edges().unwrap();
  assert_eq!(edges.len(), 5);
  assert!((edges.iter().map(|e| e.length).sum::<f64>() - 40.0).abs() < 1e-9);
  let (start, dir) = w.wire_start().unwrap();
  assert!(start.distance(DVec3::new(5.0, 0.0, 0.0)) < 1e-9, "{start}");
  assert!(dir.distance(DVec3::X) < 1e-9);
  let open = Shape::path(&Plane::XY, &square).unwrap().wire_from_middle().unwrap();
  assert_eq!(open.edges().unwrap().len(), 3);
}
