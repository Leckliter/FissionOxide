use glam::DVec3;

use crate::Shape;

#[test]
fn box_spans_and_extent_about_z() {
  let b = Shape::make_box(DVec3::new(10.0, -5.0, 0.0), DVec3::new(20.0, 5.0, 10.0)).unwrap();
  let ev = b.axial_events(DVec3::ZERO, DVec3::Z).unwrap();
  assert!((ev.tmin - 0.0).abs() < 1e-6 && (ev.tmax - 10.0).abs() < 1e-6, "{ev:?}");
  assert_eq!(ev.vertices.len(), 8);
  let s = b.radial_slices(DVec3::ZERO, DVec3::Z, &[-1.0, 1e-6, 5.0]).unwrap();
  assert!(s[0].is_empty());
  for spans in &s[1..] {
    assert_eq!(spans.len(), 1, "{s:?}");
    assert!((spans[0].0 - 10.0).abs() < 1e-6 && (spans[0].1 - 425f64.sqrt()).abs() < 1e-6, "{s:?}");
  }
}

#[test]
fn sphere_spans_about_an_offset_axis() {
  let s = Shape::sphere(DVec3::new(15.0, 0.0, 5.0), 5.0).unwrap();
  let ev = s.axial_events(DVec3::ZERO, DVec3::Z).unwrap();
  assert!((ev.tmin - 0.0).abs() < 1e-6 && (ev.tmax - 10.0).abs() < 1e-6, "{ev:?}");
  let sl = s.radial_slices(DVec3::ZERO, DVec3::Z, &[2.0, 5.0]).unwrap();
  let rho = (25.0f64 - 9.0).sqrt();
  assert!((sl[0][0].0 - (15.0 - rho)).abs() < 1e-6 && (sl[0][0].1 - (15.0 + rho)).abs() < 1e-6, "{sl:?}");
  assert!((sl[1][0].0 - 10.0).abs() < 1e-6 && (sl[1][0].1 - 20.0).abs() < 1e-6, "{sl:?}");
}
