//! Elliptical arc profile segments become exact OCCT ellipse edges.

use super::*;
use fox_sketch::{CurveKind, Loop, Sketch, find_profiles};
use std::f64::consts::PI;

fn volume(s: &Shape) -> f64 {
  s.mass().unwrap().volume
}

fn ellipse_loop(minor: f64) -> Loop {
  let half = |start: f64| Seg::EllipseArc { center: [5.0, 3.0], major: [0.0, 8.0], minor, start, sweep: PI };
  Loop::new(vec![half(0.0), half(PI)])
}

#[test]
fn whole_ellipses_extrude_exactly_either_way_round() {
  // a = 8 along y; minor 4 (smaller) and 12 (larger: OCCT's axes swap)
  for minor in [4.0, 12.0] {
    for outer in [ellipse_loop(minor), ellipse_loop(minor).reversed()] {
      let p = Profile { outer, holes: vec![] };
      let s = Shape::face(&Plane::XY, &p).unwrap().extrude(DVec3::new(0.0, 0.0, 2.0)).unwrap();
      let want = PI * 8.0 * minor * 2.0;
      assert!((volume(&s) - want).abs() < 1e-6 * want, "{} vs {want}", volume(&s));
      assert_eq!(s.faces().unwrap().len(), 3, "one elliptical side, top and bottom");
      assert!(s.is_valid());
    }
  }
}

#[test]
fn an_elliptical_arc_closed_by_a_line_extrudes_on_any_plane() {
  let mut s = Sketch::new();
  let c = s.add_point(0.0, 0.0);
  let m = s.add_point(0.0, 10.0);
  let a = s.add_point(0.0, 10.0);
  let b = s.add_point(0.0, -10.0);
  // from the top round the left (the minor side, x < 0) to the bottom
  s.add_curve(CurveKind::EllipticalArc { center: c, major: m, minor: 6.0, start: a, end: b }).unwrap();
  s.add_curve(CurveKind::Line { a: b, b: a }).unwrap();
  let p = find_profiles(&s);
  assert_eq!(p.len(), 1);
  let f = Shape::face(&Plane::XZ, &p[0]).unwrap();
  let solid = f.extrude(DVec3::new(0.0, -3.0, 0.0)).unwrap();
  let want = PI * 10.0 * 6.0 / 2.0 * 3.0;
  assert!((volume(&solid) - want).abs() < 1e-6 * want, "{} vs {want}", volume(&solid));
  assert_eq!(solid.faces().unwrap().len(), 4);
}
