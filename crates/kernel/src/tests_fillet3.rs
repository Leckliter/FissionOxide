use super::*;
use fox_sketch::{Sketch, find_profiles};

// A 40 x 30 x 20 box and the id of its top front edge (y = 0, z = 20).
fn box_and_edge() -> (Shape, u32) {
  let mut s = Sketch::new();
  s.add_rectangle([0.0, 0.0], [40.0, 30.0]).unwrap();
  let b = Shape::face(&Plane::XY, &find_profiles(&s).remove(0)).unwrap().extrude(DVec3::Z * 20.0).unwrap();
  let e = b.edges().unwrap().into_iter().find(|e| e.mid.distance(DVec3::new(20.0, 0.0, 20.0)) < 1e-6).unwrap().id;
  (b, e)
}

// What a quarter round of radius r removes per unit length.
fn cut(r: f64) -> f64 {
  (1.0 - std::f64::consts::PI / 4.0) * r * r
}

#[test]
fn radius_running_down_to_zero() {
  let (b, e) = box_and_edge();
  let f = b.fillet_law(&[e], &[(0.0, 4.0), (1.0, 0.0)]).unwrap();
  assert!(f.is_valid());
  assert_eq!(f.faces().unwrap().len(), 7);
  // radius 4 -> 0 straight along 40: the cut is (1 - pi/4) L a^2 / 3
  let want = 24000.0 - cut(4.0) * 40.0 / 3.0;
  let v = f.mass().unwrap().volume;
  assert!((v - want).abs() < 0.05, "{v} vs {want}");
}

#[test]
fn radii_at_points_along_the_edge() {
  let (b, e) = box_and_edge();
  let f = b.fillet_law(&[e], &[(0.0, 2.0), (0.5, 4.0), (1.0, 2.0)]).unwrap();
  assert!(f.is_valid());
  assert_eq!(f.faces().unwrap().len(), 7);
  let v = f.mass().unwrap().volume;
  assert!(v < 24000.0 - cut(2.0) * 40.0 && v > 24000.0 - cut(4.0) * 40.0, "{v}");
  // symmetric law, so the centre of mass stays mid-length
  assert!((f.mass().unwrap().center_of_mass.x - 20.0).abs() < 1e-3);
}

#[test]
fn bad_radius_laws_are_refused() {
  let (b, e) = box_and_edge();
  assert!(b.fillet_law(&[e], &[(0.0, 0.0), (1.0, 0.0)]).is_err());
  assert!(b.fillet_law(&[e], &[(0.0, 2.0)]).is_err());
  assert!(b.fillet_law(&[e], &[(0.0, -1.0), (1.0, 2.0)]).is_err());
}
