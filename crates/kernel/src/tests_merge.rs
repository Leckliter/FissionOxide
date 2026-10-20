//! One edge per sketch curve: profile pieces flagged to merge (pending behaviours turned on in
//! the sketch) become one edge; pieces of different curves never do.

use super::*;
use fox_sketch::behaviour::{
  ONE_EDGE_CONTROL_SPLINE, ONE_EDGE_ELLIPSE, ONE_EDGE_FIT_SPLINE, ONE_EDGE_SPLIT_ARCS, PENDING,
};
use fox_sketch::{Behaviours, CurveKind, PtSpec, Sketch, find_profiles};
use std::f64::consts::PI;

/// Extrudes every profile of the sketch 2 mm; (faces, volume) of each, largest profile first.
fn extrude_all(s: &Sketch) -> Vec<(usize, f64)> {
  find_profiles(s)
    .iter()
    .map(|p| {
      let solid = Shape::face(&Plane::XY, p).unwrap().extrude(DVec3::new(0.0, 0.0, 2.0)).unwrap();
      assert!(solid.is_valid());
      (solid.faces().unwrap().len(), solid.mass().unwrap().volume)
    })
    .collect()
}

/// The volume of a 2 mm extrusion of the profile, from a fine mesh.
fn mesh_volume(p: &fox_sketch::Profile) -> f64 {
  let solid = Shape::face(&Plane::XY, p).unwrap().extrude(DVec3::new(0.0, 0.0, 2.0)).unwrap();
  let m = solid.tessellate(1e-4).unwrap();
  let at = |i: u32| m.positions[i as usize].map(f64::from);
  m.indices
    .chunks_exact(3)
    .map(|t| {
      let (a, b, c) = (at(t[0]), at(t[1]), at(t[2]));
      a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])
    })
    .sum::<f64>()
    / 6.0
}

/// The sketch with only these behaviours (pending or current) on.
fn with(mut s: Sketch, names: &[&str]) -> Sketch {
  s.behaviours = Behaviours::legacy();
  for n in names {
    if PENDING.contains(n) {
      s.behaviours.enable_pending_for_tests(n);
    } else {
      s.behaviours.insert(*n);
    }
  }
  s
}

fn ellipse(s: &mut Sketch, c: [f64; 2], major: [f64; 2], minor: f64) {
  let (c, m) = (s.add_point(c[0], c[1]), s.add_point(major[0], major[1]));
  s.add_curve(CurveKind::Ellipse { center: c, major: m, minor }).unwrap();
}

#[test]
fn a_whole_ellipse_extrudes_to_one_side_face_with_one_edge_ellipse() {
  let mut s = Sketch::new();
  ellipse(&mut s, [5.0, 3.0], [5.0, 11.0], 4.0);
  let legacy = extrude_all(&s);
  let merged = extrude_all(&with(s, &[ONE_EDGE_ELLIPSE]));
  assert_eq!(legacy.len(), 1);
  assert_eq!(legacy[0].0, 4, "two half-ellipse sides, top and bottom");
  assert_eq!(merged[0].0, 3, "one side face");
  let want = PI * 8.0 * 4.0 * 2.0;
  for (_, v) in legacy.iter().chain(&merged) {
    assert!((v - want).abs() < 1e-6 * want, "{v} vs {want}");
  }
}

#[test]
fn an_ellipse_hole_merges_too() {
  let mut s = Sketch::new();
  s.add_rectangle([-20.0, -20.0], [20.0, 20.0]).unwrap();
  // minor larger than major: OCCT swaps the axes
  ellipse(&mut s, [0.0, 0.0], [6.0, 0.0], 9.0);
  let legacy = extrude_all(&s);
  let merged = extrude_all(&with(s, &[ONE_EDGE_ELLIPSE]));
  assert_eq!(legacy.iter().map(|p| p.0).collect::<Vec<_>>(), [8, 4], "plate with a hole, the ellipse");
  assert_eq!(merged.iter().map(|p| p.0).collect::<Vec<_>>(), [7, 3]);
  for (a, b) in legacy.iter().zip(&merged) {
    assert!((a.1 - b.1).abs() < 1e-6 * a.1, "{} vs {}", a.1, b.1);
  }
}

#[test]
fn separate_curves_joined_smoothly_stay_separate_edges() {
  // two arcs of one circle, and two elliptical arcs of one ellipse: each pair makes a whole round
  // that one curve would, but they are two curves
  let mut s = Sketch::new();
  let c = s.add_point(0.0, 0.0);
  let (a, b) = (s.add_point(10.0, 0.0), s.add_point(-10.0, 0.0));
  s.add_curve(CurveKind::Arc { center: c, start: a, end: b }).unwrap();
  s.add_curve(CurveKind::Arc { center: c, start: b, end: a }).unwrap();
  let c = s.add_point(40.0, 0.0);
  let m = s.add_point(40.0, 8.0);
  let (top, bottom) = (s.add_point(40.0, 8.0), s.add_point(40.0, -8.0));
  s.add_curve(CurveKind::EllipticalArc { center: c, major: m, minor: 5.0, start: top, end: bottom }).unwrap();
  s.add_curve(CurveKind::EllipticalArc { center: c, major: m, minor: 5.0, start: bottom, end: top }).unwrap();
  let legacy = extrude_all(&s);
  let all = extrude_all(&with(s, PENDING));
  assert_eq!(legacy.len(), 2);
  assert_eq!(legacy.iter().map(|p| p.0).collect::<Vec<_>>(), [4, 4]);
  assert_eq!(all.iter().map(|p| p.0).collect::<Vec<_>>(), [4, 4], "every flag on, still two sides each");
}

#[test]
fn a_circle_split_where_lines_touch_it_merges_back_with_one_edge_split_arcs() {
  let mut s = Sketch::new();
  s.add_circle(PtSpec::At(0.0, 0.0), 10.0).unwrap();
  // dangling lines ending on the circle split it, then are pruned from the profile
  s.add_line(PtSpec::At(20.0, 0.0), PtSpec::At(10.0, 0.0)).unwrap();
  s.add_line(PtSpec::At(-20.0, 0.0), PtSpec::At(-10.0, 0.0)).unwrap();
  let legacy = extrude_all(&s);
  let merged = extrude_all(&with(s, &[ONE_EDGE_SPLIT_ARCS]));
  assert_eq!(legacy.iter().map(|p| p.0).collect::<Vec<_>>(), [4], "the disc's side in two halves");
  assert_eq!(merged.iter().map(|p| p.0).collect::<Vec<_>>(), [3]);
  assert!((legacy[0].1 - merged[0].1).abs() < 1e-6 * legacy[0].1);
}

#[test]
fn spline_pieces_become_one_b_spline_edge() {
  for (kind, flag) in [(0, ONE_EDGE_FIT_SPLINE), (1, ONE_EDGE_CONTROL_SPLINE)] {
    let mut s = Sketch::new();
    s.behaviours = Behaviours::legacy();
    let pts: Vec<_> =
      [[0.0, 0.0], [5.0, 6.0], [12.0, 4.0], [18.0, 9.0], [24.0, 0.0]].iter().map(|p| s.add_point(p[0], p[1])).collect();
    let curve = if kind == 0 {
      CurveKind::Spline { points: pts.clone(), start_handle: None, end_handle: None }
    } else {
      CurveKind::control_spline(pts.clone())
    };
    let id = s.add_curve(curve).unwrap();
    s.add_curve(CurveKind::Line { a: pts[4], b: pts[0] }).unwrap();
    let pieces = s.beziers(id).len();
    assert!(pieces > 1);
    let legacy = extrude_all(&s);
    let merged = extrude_all(&with(s.clone(), &[flag]));
    assert_eq!(legacy[0].0, pieces + 3, "{flag}");
    assert_eq!(merged[0].0, 4, "{flag}: one spline side, the line's, top and bottom");
    // (OCCT's default volume integration is ~4e-4 off on the merged B-spline face; the shape is
    // the same, as the meshes show)
    let (a, b) = (mesh_volume(&find_profiles(&s)[0]), mesh_volume(&find_profiles(&with(s.clone(), &[flag]))[0]));
    assert!((a - b).abs() < 1e-4 * a, "{flag}: {a} vs {b}");
    // the other spline kind's flag leaves it alone
    let other = if kind == 0 { ONE_EDGE_CONTROL_SPLINE } else { ONE_EDGE_FIT_SPLINE };
    assert_eq!(extrude_all(&with(s, &[other, ONE_EDGE_ELLIPSE, ONE_EDGE_SPLIT_ARCS]))[0].0, pieces + 3);
  }
}

#[test]
fn a_path_merges_flagged_pieces() {
  let half = |start: f64| Seg::EllipseArc { center: [0.0, 0.0], major: [8.0, 0.0], minor: 4.0, start, sweep: PI / 2.0 };
  let segs = [half(0.0), half(PI / 2.0), Seg::Line { a: [-8.0, 0.0], b: [-8.0, -10.0] }];
  let n = |w: &Shape| w.edges().unwrap().len();
  assert_eq!(n(&Shape::path(&Plane::XY, &segs).unwrap()), 3);
  assert_eq!(n(&Shape::path_with(&Plane::XY, &segs, &[false, false, false]).unwrap()), 3);
  assert_eq!(n(&Shape::path_with(&Plane::XY, &segs, &[true, false, false]).unwrap()), 2);
}

#[test]
fn a_control_spline_merges_after_a_quintic_that_takes_three_records() {
  // a blend quintic (three segment records) then a control spline's pieces: the merge flags are
  // per segment, so the spline's run must be found past the quintic's extra records
  let mut s = Sketch::new();
  let q: Vec<_> = (0..10).map(|k| s.add_point(4.0 * k as f64, if k == 0 || k == 9 { 0.0 } else { 6.0 })).collect();
  s.add_curve(CurveKind::QuinticSpline { points: q.clone() }).unwrap();
  let mut c = vec![q[9]];
  c.extend([[30.0, -8.0], [20.0, -2.0], [10.0, -9.0], [4.0, -3.0]].iter().map(|p| s.add_point(p[0], p[1])));
  c.push(q[0]);
  let id = s.add_curve(CurveKind::control_spline(c)).unwrap();
  assert!(s.beziers(id).len() > 1);
  let legacy = extrude_all(&with(s.clone(), &[]));
  let merged = extrude_all(&with(s, &[ONE_EDGE_CONTROL_SPLINE]));
  assert_eq!(merged[0].0, 4, "the quintic's side, the spline's, top and bottom");
  assert!(legacy[0].0 > 4);
  assert!((legacy[0].1 - merged[0].1).abs() < 1e-3 * legacy[0].1);
}
