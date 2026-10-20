use super::*;

// A 40 × 20 × 10 box with its two vertical edges at y = 20 chamfered 10 at 45°: the y = 20 face
// is 20 wide between two slanted neighbours.
fn sloped() -> Shape {
  let b = Shape::make_box(DVec3::ZERO, DVec3::new(40.0, 20.0, 10.0)).unwrap();
  let edges: Vec<u32> = b
    .edges()
    .unwrap()
    .iter()
    .filter(|e| {
      (e.mid - DVec3::new(0.0, 20.0, 5.0)).length() < 1e-6 || (e.mid - DVec3::new(40.0, 20.0, 5.0)).length() < 1e-6
    })
    .map(|e| e.id)
    .collect();
  assert_eq!(edges.len(), 2);
  b.chamfer(&edges, 10.0).unwrap()
}

fn face_at(s: &Shape, p: DVec3) -> u32 {
  s.faces().unwrap().iter().find(|f| f.kind == SurfaceKind::Plane && (f.center - p).length() < 1e-6).unwrap().id
}

#[test]
fn offset_face_extends_slanted_neighbours() {
  let s = sloped();
  assert!((s.mass().unwrap().volume - 7000.0).abs() < 1e-6);
  let top = face_at(&s, DVec3::new(20.0, 20.0, 5.0));
  // out by 5: the slopes run on, the face ends 10 wide at y = 25 (a 30/10 trapezoid strip)
  let out = s.offset_faces(&[top], 5.0).unwrap();
  assert!((out.mass().unwrap().volume - 7750.0).abs() < 1e-6, "{}", out.mass().unwrap().volume);
  assert_eq!(out.faces().unwrap().len(), 8);
  // in by 5: the slopes are trimmed back, the face is 30 wide at y = 15 (a 30/20 strip goes)
  let top = face_at(&s, DVec3::new(20.0, 20.0, 5.0));
  let r#in = s.offset_faces(&[top], -5.0).unwrap();
  assert!((r#in.mass().unwrap().volume - 5750.0).abs() < 1e-6, "{}", r#in.mass().unwrap().volume);
  assert_eq!(r#in.faces().unwrap().len(), 8);
}

#[test]
fn offset_face_shrinks_a_hole() {
  let plate = Shape::make_box(DVec3::ZERO, DVec3::new(30.0, 20.0, 5.0)).unwrap();
  let hole = Shape::cylinder(DVec3::new(15.0, 10.0, -1.0), DVec3::Z, 8.0, 7.0).unwrap();
  let s = plate.boolean(&hole, BoolOp::Subtract).unwrap();
  let wall = s.faces().unwrap().iter().find(|f| f.kind == SurfaceKind::Cylinder).unwrap().id;
  let out = s.offset_faces(&[wall], 3.0).unwrap();
  let want = 3000.0 - std::f64::consts::PI * 25.0 * 5.0;
  assert!((out.mass().unwrap().volume - want).abs() < 1e-6, "{}", out.mass().unwrap().volume);
}

#[test]
fn offset_face_rejects_zero() {
  let b = Shape::make_box(DVec3::ZERO, DVec3::ONE).unwrap();
  assert!(b.offset_faces(&[1], 0.0).is_err());
  assert!(b.offset_faces(&[], 1.0).is_err());
}
