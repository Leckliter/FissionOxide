use super::*;

fn vol(s: &Shape) -> f64 {
  s.mass().unwrap().volume
}

fn plane_at(s: &Shape, p: DVec3) -> u32 {
  s.faces().unwrap().iter().find(|f| f.kind == SurfaceKind::Plane && (f.center - p).length() < 1e-6).unwrap().id
}

fn cylinders(s: &Shape) -> Vec<FaceInfo> {
  s.faces().unwrap().into_iter().filter(|f| f.kind == SurfaceKind::Cylinder).collect()
}

fn turn(origin: DVec3, axis: DVec3, deg: f64) -> DMat4 {
  DMat4::from_translation(origin)
    * DMat4::from_axis_angle(axis.normalize(), deg.to_radians())
    * DMat4::from_translation(-origin)
}

#[test]
fn a_flat_face_tilts_and_its_neighbours_follow() {
  let b = Shape::make_box(DVec3::ZERO, DVec3::new(40.0, 20.0, 10.0)).unwrap();
  let top = plane_at(&b, DVec3::new(20.0, 10.0, 10.0));
  // about the top's middle line along y: one half rises as much as the other sinks
  let out = b.move_faces(&[top], turn(DVec3::new(20.0, 10.0, 10.0), DVec3::Y, 10.0)).unwrap();
  assert!((vol(&out) - 8000.0).abs() < 1e-3, "{}", vol(&out));
  assert_eq!(out.faces().unwrap().len(), 6);
  let m = out.mass().unwrap();
  let rise = 20.0 * 10f64.to_radians().tan();
  assert!((m.bbox_max.z - (10.0 + rise)).abs() < 1e-4, "{}", m.bbox_max.z);
  // about the top's edge at x = 0: a wedge is added
  let top = plane_at(&b, DVec3::new(20.0, 10.0, 10.0));
  let out = b.move_faces(&[top], turn(DVec3::new(0.0, 0.0, 10.0), DVec3::Y, -5.0)).unwrap();
  let wedge = 0.5 * 40.0 * 40.0 * 5f64.to_radians().tan() * 20.0;
  assert!((vol(&out) - (8000.0 + wedge)).abs() < 1e-3, "{}", vol(&out));
}

#[test]
fn a_boss_moves_and_turns_with_its_faces() {
  let plate = Shape::make_box(DVec3::ZERO, DVec3::new(60.0, 40.0, 10.0)).unwrap();
  let boss = Shape::cylinder(DVec3::new(15.0, 20.0, 10.0), DVec3::Z, 5.0, 8.0).unwrap();
  let s = plate.boolean(&boss, BoolOp::Union).unwrap();
  let v0 = vol(&s);
  let wall = cylinders(&s)[0].id;
  let top = plane_at(&s, DVec3::new(15.0, 20.0, 18.0));
  let out = s.move_faces(&[wall, top], DMat4::from_translation(DVec3::new(25.0, 0.0, 0.0))).unwrap();
  assert!((vol(&out) - v0).abs() < 1e-3);
  let c = cylinders(&out);
  assert_eq!(c.len(), 1);
  assert!((c[0].origin.x - 40.0).abs() < 1e-6 && (c[0].origin.y - 20.0).abs() < 1e-6, "{:?}", c[0].origin);
  // a square boss turned 45° about its own axis
  let square = Shape::make_box(DVec3::new(10.0, 10.0, 10.0), DVec3::new(20.0, 20.0, 15.0)).unwrap();
  let s = plate.boolean(&square, BoolOp::Union).unwrap();
  let picked: Vec<u32> = s.faces().unwrap().iter().filter(|f| f.center.z > 10.0 + 1e-6).map(|f| f.id).collect();
  assert_eq!(picked.len(), 5);
  let out = s.move_faces(&picked, turn(DVec3::new(15.0, 15.0, 0.0), DVec3::Z, 45.0)).unwrap();
  assert!((vol(&out) - vol(&s)).abs() < 1e-3);
  let m = out.mass().unwrap();
  assert!((m.bbox_max.z - 15.0).abs() < 1e-6);
  // the turned square's corner now points along x
  let far = out.faces().unwrap().iter().filter(|f| f.center.z > 10.0 + 1e-6).map(|f| f.center.x).fold(0.0, f64::max);
  assert!(far > 18.5, "{far}");
}

#[test]
fn a_hole_slides_sideways() {
  let plate = Shape::make_box(DVec3::ZERO, DVec3::new(60.0, 40.0, 10.0)).unwrap();
  let pin = Shape::cylinder(DVec3::new(15.0, 20.0, -1.0), DVec3::Z, 4.0, 12.0).unwrap();
  let s = plate.boolean(&pin, BoolOp::Subtract).unwrap();
  let v0 = vol(&s);
  let wall = cylinders(&s)[0].id;
  let out = s.move_faces(&[wall], DMat4::from_translation(DVec3::new(20.0, 5.0, 0.0))).unwrap();
  assert!((vol(&out) - v0).abs() < 1e-3);
  let c = cylinders(&out);
  assert_eq!(c.len(), 1);
  assert!((c[0].origin.x - 35.0).abs() < 1e-6 && (c[0].origin.y - 25.0).abs() < 1e-6);
  // tilted 10° about a line across the plate through its middle: still one hole
  let out = s.move_faces(&[wall], turn(DVec3::new(15.0, 20.0, 5.0), DVec3::X, 10.0)).unwrap();
  assert_eq!(out.solid_count(), 1);
  let c = cylinders(&out);
  assert_eq!(c.len(), 1);
  assert!((c[0].normal.dot(DVec3::Z).abs() - 10f64.to_radians().cos()).abs() < 1e-6);
}

#[test]
fn a_raised_boss_grows_and_one_slid_off_the_body_is_refused() {
  let plate = Shape::make_box(DVec3::ZERO, DVec3::new(60.0, 40.0, 10.0)).unwrap();
  let boss = Shape::cylinder(DVec3::new(15.0, 20.0, 10.0), DVec3::Z, 5.0, 8.0).unwrap();
  let s = plate.boolean(&boss, BoolOp::Union).unwrap();
  let wall = cylinders(&s)[0].id;
  let top = plane_at(&s, DVec3::new(15.0, 20.0, 18.0));
  // raised 20: its wall runs on down to the plate, as Fusion extends it
  let up = s.move_faces(&[wall, top], DMat4::from_translation(DVec3::new(0.0, 0.0, 20.0))).unwrap();
  let want = 24000.0 + std::f64::consts::PI * 25.0 * 28.0;
  assert!((vol(&up) - want).abs() < 1e-3, "{} {want}", vol(&up));
  let e = s.move_faces(&[wall, top], DMat4::from_translation(DVec3::new(100.0, 0.0, 0.0))).unwrap_err();
  assert!(e.to_string().contains("no longer meet"), "{e}");
  // a face can't stand on its side
  let top = plane_at(&plate, DVec3::new(30.0, 20.0, 10.0));
  assert!(plate.move_faces(&[top], turn(DVec3::new(30.0, 20.0, 10.0), DVec3::X, 90.0)).is_err());
}
