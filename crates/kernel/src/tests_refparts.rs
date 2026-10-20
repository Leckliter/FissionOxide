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
  // radius 4 -> 0 straight along 40: quarter-round sections would cut (1 - pi/4) L a^2 / 3;
  // OpenCASCADE's rolling ball of changing size cuts a few percent more than that
  let removed = 24000.0 - f.mass().unwrap().volume;
  let quarter_rounds = cut(4.0) * 40.0 / 3.0;
  assert!(removed > quarter_rounds && removed < 1.1 * quarter_rounds, "{removed} vs {quarter_rounds}");
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

#[test]
fn tangent_holes_keep_whole_walls() {
  // two d10 holes 10 apart touch along a line; each wall stays one face, as it would apart
  let plate = Shape::make_box(DVec3::new(-30.0, -20.0, 0.0), DVec3::new(30.0, 20.0, 5.0)).unwrap();
  let hole = |x: f64| Shape::cylinder(DVec3::new(x, 0.0, -1.0), DVec3::Z, 5.0, 7.0).unwrap();
  let p = plate.boolean(&hole(-5.0), BoolOp::Subtract).unwrap().boolean(&hole(5.0), BoolOp::Subtract).unwrap();
  assert!(p.is_valid());
  assert_eq!(p.faces().unwrap().len(), 8);
  let want = 12000.0 - 2.0 * std::f64::consts::PI * 25.0 * 5.0;
  assert!((p.mass().unwrap().volume - want).abs() < 1e-6);
  // ordinary results are left alone
  let q = plate.boolean(&hole(-10.0), BoolOp::Subtract).unwrap();
  assert_eq!(q.faces().unwrap().len(), 7);
}
