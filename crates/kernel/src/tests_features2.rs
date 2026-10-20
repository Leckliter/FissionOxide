use std::f64::consts::{FRAC_PI_2, PI};

use super::*;
use fox_sketch::{PtSpec, Sketch, find_profiles};

/// The front plane: sketch x = X, sketch y = Z (up).
const FRONT: Plane = Plane { origin: DVec3::ZERO, x: DVec3::X, y: DVec3::Z };

fn rect(plane: &Plane, a: [f64; 2], b: [f64; 2]) -> Shape {
  let mut s = Sketch::new();
  s.add_rectangle(a, b).unwrap();
  Shape::face(plane, &find_profiles(&s).remove(0)).unwrap()
}

fn circle(plane: &Plane, r: f64) -> Shape {
  let mut s = Sketch::new();
  s.add_circle(PtSpec::At(0.0, 0.0), r).unwrap();
  Shape::face(plane, &find_profiles(&s).remove(0)).unwrap()
}

/// A straight path up the Z axis.
fn z_path(len: f64) -> Shape {
  Shape::path(&FRONT, &[Seg::Line { a: [0.0, 0.0], b: [0.0, len] }]).unwrap()
}

fn volume(s: &Shape) -> f64 {
  s.mass().unwrap().volume
}

fn close(a: f64, b: f64, rel: f64) -> bool {
  (a - b).abs() <= rel * b.abs()
}

#[test]
fn sweep_with_a_guide_rail_scales_the_profile() {
  // a circle swept up 50 mm while a rail leans out from r = 5 to r = 10: a cone frustum
  let rail = Shape::path(&FRONT, &[Seg::Line { a: [5.0, 0.0], b: [10.0, 50.0] }]).unwrap();
  let s = circle(&Plane::XY, 5.0).sweep_rail(&z_path(50.0), &rail).unwrap();
  let expected = PI * 50.0 / 3.0 * (25.0 + 50.0 + 100.0);
  assert!(close(volume(&s), expected, 0.01), "{} vs {expected}", volume(&s));
  assert!(s.is_valid());
  let m = s.mass().unwrap();
  assert!((m.bbox_max.x - 10.0).abs() < 0.1 && (m.bbox_max.z - 50.0).abs() < 1e-3, "{m:?}");
}

#[test]
fn twisted_sweep_keeps_the_section() {
  let face = rect(&Plane::XY, [-10.0, -5.0], [10.0, 5.0]);
  let s = face.sweep_twisted(&z_path(50.0), FRAC_PI_2).unwrap();
  assert!(close(volume(&s), 10_000.0, 0.01), "{}", volume(&s));
  assert!(s.is_valid());
  // a quarter turn: the top is the rectangle turned across, and the middle sticks out further
  let m = s.mass().unwrap();
  assert!(m.bbox_max.y > 8.0, "{m:?}");
  let top = s.split(DVec3::new(0.0, 0.0, 49.0), DVec3::Z).unwrap();
  let upper = top.iter().find(|p| p.mass().unwrap().center_of_mass.z > 49.0).unwrap().mass().unwrap();
  assert!(upper.bbox_max.y > 9.0 && upper.bbox_max.x < 6.0, "{upper:?}");
  // no twist with a hole in the profile is still a plain sweep
  let mut sk = Sketch::new();
  sk.add_circle(PtSpec::At(0.0, 0.0), 6.0).unwrap();
  sk.add_circle(PtSpec::At(0.0, 0.0), 3.0).unwrap();
  let ring = find_profiles(&sk).into_iter().find(|p| !p.holes.is_empty()).unwrap();
  let tube = Shape::face(&Plane::XY, &ring).unwrap().sweep_twisted(&z_path(10.0), 1.0).unwrap();
  assert!(close(volume(&tube), PI * 27.0 * 10.0, 0.01), "{}", volume(&tube));
}

#[test]
fn loft_along_a_centre_line() {
  let top = Plane { origin: DVec3::new(0.0, 0.0, 40.0), ..Plane::XY };
  let sections = [circle(&Plane::XY, 10.0), circle(&top, 5.0)];
  let s = Shape::loft_centerline(&sections, &z_path(40.0)).unwrap();
  let frustum = PI * 40.0 / 3.0 * (100.0 + 50.0 + 25.0);
  assert!(close(volume(&s), frustum, 0.03), "{} vs {frustum}", volume(&s));
  assert!(s.is_valid());
}

/// An arc in the XZ plane from (x0, 0) through (x0 + bulge, 20) to (x0, 40).
fn bulging_rail(x0: f64, bulge: f64) -> Shape {
  // centre on z = 20 at distance c from x0: (c)² + 20² = (c + bulge)²
  let c = (400.0 - bulge * bulge) / (2.0 * bulge.abs());
  let r = c + bulge.abs();
  let half = (20.0f64).atan2(c);
  let s = bulge.signum();
  let center = [x0 - s * c, 20.0];
  let (start, sweep) = if s > 0.0 { (-half, 2.0 * half) } else { (PI + half, -2.0 * half) };
  Shape::path(&FRONT, &[Seg::Arc { center, radius: r, start_angle: start, sweep }]).unwrap()
}

#[test]
fn loft_follows_rails() {
  let top = Plane { origin: DVec3::new(0.0, 0.0, 40.0), ..Plane::XY };
  let sections = [circle(&Plane::XY, 10.0), circle(&top, 10.0)];
  let rails = [bulging_rail(10.0, 5.0), bulging_rail(-10.0, -5.0)];
  let s = Shape::loft_rails(&sections, &rails).unwrap();
  assert!(s.is_valid());
  let m = s.mass().unwrap();
  assert!((m.bbox_max.x - 15.0).abs() < 0.3 && (m.bbox_min.x + 15.0).abs() < 0.3, "{m:?}");
  assert!((m.bbox_max.y - 10.0).abs() < 0.3, "{m:?}");
  let cylinder = PI * 100.0 * 40.0;
  assert!(m.volume > cylinder * 1.1 && m.volume < cylinder * 1.5, "{}", m.volume);
  // one rail pulls one side only
  let one = Shape::loft_rails(&sections, &rails[..1]).unwrap();
  let m1 = one.mass().unwrap();
  assert!((m1.bbox_max.x - 15.0).abs() < 0.3 && (m1.bbox_min.x + 10.0).abs() < 0.3, "{m1:?}");
  // square sections keep their corners
  let sq = [rect(&Plane::XY, [-10.0, -10.0], [10.0, 10.0]), rect(&top, [-10.0, -10.0], [10.0, 10.0])];
  let s = Shape::loft_rails(&sq, &rails[..1]).unwrap();
  assert!(s.is_valid());
  let m = s.mass().unwrap();
  assert!(m.volume > 16_000.0 && (m.bbox_max.y - 10.0).abs() < 0.3, "{m:?}");
  // a rail that misses the sections is an error
  let far = Shape::path(&FRONT, &[Seg::Line { a: [50.0, 0.0], b: [50.0, 40.0] }]).unwrap();
  assert!(Shape::loft_rails(&sections, &[far]).is_err());
}

fn cylinder_face(s: &Shape) -> u32 {
  s.faces().unwrap().iter().find(|f| f.kind == SurfaceKind::Cylinder).unwrap().id
}

#[test]
fn emboss_on_a_cylinder() {
  let body = Shape::cylinder(DVec3::ZERO, DVec3::Z, 20.0, 40.0).unwrap();
  let face = cylinder_face(&body);
  // a 10 × 10 square on a plane in front of the cylinder (y = −30), looking along +Y
  let front = Plane { origin: DVec3::new(0.0, -30.0, 0.0), x: DVec3::X, y: DVec3::Z };
  let sq = rect(&front, [-5.0, 15.0], [5.0, 25.0]);
  // wrapped: 10 mm of arc is half a radian at r = 20
  let tool = body.emboss_tool(face, std::slice::from_ref(&sq), DVec3::Y, 2.0, false, true).unwrap();
  let wrapped = (22.0f64.powi(2) - 400.0) / 2.0 * 0.5 * 10.0;
  assert!(close(volume(&tool), wrapped, 0.01), "{} vs {wrapped}", volume(&tool));
  let m = tool.mass().unwrap();
  assert!(m.bbox_max.y < -15.0 && m.bbox_min.z > 14.9 && m.bbox_max.z < 25.1, "{m:?}");
  let deboss = body.emboss_tool(face, std::slice::from_ref(&sq), DVec3::Y, 2.0, true, true).unwrap();
  let inside = (400.0 - 18.0f64.powi(2)) / 2.0 * 0.5 * 10.0;
  assert!(close(volume(&deboss), inside, 0.01), "{} vs {inside}", volume(&deboss));
  // projected straight: the straight walls cut the ring between r = 20 and 22 where |x| <= 5
  let tool = body.emboss_tool(face, std::slice::from_ref(&sq), DVec3::Y, 2.0, false, false).unwrap();
  let n = 2000;
  let area: f64 = (0..n)
    .map(|i| {
      let r = 20.0 + 2.0 * (i as f64 + 0.5) / n as f64;
      2.0 * (5.0 / r).asin() * r * 2.0 / n as f64
    })
    .sum();
  assert!(close(volume(&tool), area * 10.0, 0.01), "{} vs {}", volume(&tool), area * 10.0);
  assert!(tool.mass().unwrap().bbox_max.y < 0.0, "only the side facing the sketch");
  let joined = body.boolean(&tool, BoolOp::Union).unwrap();
  assert!(close(volume(&joined), volume(&body) + area * 10.0, 1e-3));
  // wrapping needs a cylinder
  let block = Shape::make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
  assert!(block.emboss_tool(1, &[sq], DVec3::Y, 1.0, false, true).is_err());
}

#[test]
fn boundary_cells_of_overlapping_bodies_and_a_plane() {
  let a = Shape::make_box(DVec3::ZERO, DVec3::splat(20.0)).unwrap();
  let b = Shape::make_box(DVec3::new(10.0, 0.0, 0.0), DVec3::new(30.0, 20.0, 20.0)).unwrap();
  let cells = Shape::boundary_cells(&[a.clone(), b]).unwrap();
  assert_eq!(cells.len(), 3);
  let mid: Vec<&Shape> =
    cells.iter().filter(|c| c.classify(DVec3::new(15.0, 10.0, 10.0)).unwrap() == Inside::In).collect();
  assert_eq!(mid.len(), 1);
  assert!((volume(mid[0]) - 4000.0).abs() < 1e-3);
  let total: f64 = cells.iter().map(volume).sum();
  assert!((total - 12_000.0).abs() < 1e-3);
  let plane = Shape::plane_face(DVec3::new(10.0, 10.0, 5.0), DVec3::Z, 100.0).unwrap();
  let cells = Shape::boundary_cells(&[a, plane]).unwrap();
  let mut v: Vec<f64> = cells.iter().map(volume).collect();
  v.sort_by(f64::total_cmp);
  assert!(v.len() == 2 && (v[0] - 2000.0).abs() < 1e-3 && (v[1] - 6000.0).abs() < 1e-3, "{v:?}");
}

#[test]
fn extended_surfaces_and_sides() {
  let body = Shape::cylinder(DVec3::ZERO, DVec3::Z, 20.0, 10.0).unwrap();
  let side = body.face_shape(cylinder_face(&body)).unwrap();
  let big = side.extended_face(50.0, 0.0, DVec3::X, DVec3::X).unwrap();
  // outward normal: positive outside the cylinder, negative inside, whole way round
  assert!((big.side_of(DVec3::new(25.0, 0.0, 40.0)).unwrap() - 5.0).abs() < 1e-6);
  assert!((big.side_of(DVec3::new(0.0, -15.0, -30.0)).unwrap() + 5.0).abs() < 1e-6);
  // offset 3 mm outwards, judged at +X; then inwards
  let off = side.extended_face(50.0, 3.0, DVec3::new(30.0, 0.0, 5.0), DVec3::X).unwrap();
  assert!((off.side_of(DVec3::new(25.0, 0.0, 5.0)).unwrap() - 2.0).abs() < 1e-6);
  let inner = side.extended_face(50.0, 3.0, DVec3::new(0.0, -30.0, 5.0), DVec3::Y).unwrap();
  assert!((inner.side_of(DVec3::new(25.0, 0.0, 5.0)).unwrap() - 8.0).abs() < 1e-6);
  // a long bar split by the extended surface
  let bar = Shape::make_box(DVec3::new(-5.0, -5.0, -30.0), DVec3::new(5.0, 5.0, 30.0)).unwrap();
  assert_eq!(bar.split_by(&big).unwrap().len(), 1, "inside the cylinder: nothing to split");
  let bar = Shape::make_box(DVec3::new(0.0, -2.0, 0.0), DVec3::new(40.0, 2.0, 4.0)).unwrap();
  assert_eq!(bar.split_by(&big).unwrap().len(), 2);
  let top = body
    .face_shape(body.faces().unwrap().iter().find(|f| f.kind == SurfaceKind::Plane && f.normal.z > 0.9).unwrap().id)
    .unwrap();
  let plane = top.extended_face(100.0, 0.0, DVec3::ZERO, DVec3::Z).unwrap();
  assert!((plane.side_of(DVec3::new(80.0, 80.0, 15.0)).unwrap() - 5.0).abs() < 1e-6);
}

#[test]
fn setback_corner_replaces_the_blend() {
  let b = Shape::make_box(DVec3::ZERO, DVec3::splat(20.0)).unwrap();
  let corner = DVec3::splat(20.0);
  let ids: Vec<u32> = b
    .edges()
    .unwrap()
    .iter()
    .filter(|e| (e.start - corner).length() < 1e-9 || (e.end - corner).length() < 1e-9)
    .map(|e| e.id)
    .collect();
  assert_eq!(ids.len(), 3);
  let filleted = b.fillet(&ids, 3.0).unwrap();
  let s = filleted.setback_corners(&[corner], 7.0).unwrap();
  assert!(s.is_valid());
  assert_eq!(s.solid_count(), 1);
  let (v0, v) = (volume(&filleted), volume(&s));
  // the patch takes a little more off than the rolling-ball corner, never a chunk
  assert!(v < v0 - 0.5 && v > v0 - 60.0, "{v} vs {v0}");
  assert!(s.faces().unwrap().iter().any(|f| f.kind == SurfaceKind::Other));
  assert!(filleted.setback_corners(&[DVec3::splat(100.0)], 2.0).is_err(), "nothing to cut there");
}

#[test]
fn shell_inside_outside_and_both() {
  // a 20 mm cube opened at the top
  let b = Shape::make_box(DVec3::ZERO, DVec3::splat(20.0)).unwrap();
  let top = b.faces().unwrap().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  let inside = b.shell_sides(&[top], 2.0, 0.0, false).unwrap();
  assert!(close(volume(&inside), 8000.0 - 16.0 * 16.0 * 18.0, 1e-6), "{}", volume(&inside));
  // outside: the cube's volume becomes the cavity, the walls go around it (sharp corners)
  let outside = b.shell_sides(&[top], 0.0, 2.0, false).unwrap();
  assert!(close(volume(&outside), 24.0 * 24.0 * 22.0 - 8000.0, 1e-6), "{}", volume(&outside));
  // both: 1 in, 2 out
  let both = b.shell_sides(&[top], 1.0, 2.0, false).unwrap();
  assert_eq!(both.solid_count(), 1);
  assert!(close(volume(&both), 24.0 * 24.0 * 22.0 - 18.0 * 18.0 * 19.0, 1e-6), "{}", volume(&both));
  assert!(b.shell_sides(&[top], 0.0, 0.0, false).is_err());
  // no faces opened: a hollow, closed body
  let hollow = b.shell_sides(&[], 2.0, 0.0, false).unwrap();
  assert!(close(volume(&hollow), 8000.0 - 16.0f64.powi(3), 1e-6), "{}", volume(&hollow));
  let hollow = b.shell_sides(&[], 1.0, 2.0, false).unwrap();
  assert!(close(volume(&hollow), 24.0f64.powi(3) - 18.0f64.powi(3), 1e-6), "{}", volume(&hollow));
}
