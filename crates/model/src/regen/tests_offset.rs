use super::*;
use crate::*;
use fox_kernel::glam::DVec3;
use fox_sketch::{PtSpec, Sketch};

/// Reference case 510: a trapezoid (base 40, top 20, 20 high) extruded 10, its y = 20 face offset
/// by `dist`, with the given behaviours: the result's volume and face count.
fn offset_trapezoid_top(behaviours: Behaviours, dist: &str) -> (f64, usize) {
  let mut d = Design::new("offset");
  d.behaviours = behaviours;
  let mut s = Sketch::new();
  let (_, [p0, p1]) = s.add_line(PtSpec::At(0.0, 0.0), PtSpec::At(40.0, 0.0)).unwrap();
  let (_, [_, p2]) = s.add_line(PtSpec::Point(p1), PtSpec::At(30.0, 20.0)).unwrap();
  let (_, [_, p3]) = s.add_line(PtSpec::Point(p2), PtSpec::At(10.0, 20.0)).unwrap();
  s.add_line(PtSpec::Point(p3), PtSpec::Point(p0)).unwrap();
  let plane = PlaneRef::Origin { plane: OriginPlane::Xy };
  let sk = d.add_feature(FeatureKind::Sketch { plane, sketch: s, links: Vec::new() }).unwrap();
  let extent = Extent::distance("10");
  d.add_feature(FeatureKind::Extrude { sketch: sk, profiles: vec![], extent, operation: Operation::NewBody }).unwrap();
  let m = regenerate(&mut d);
  assert!(m.errors().is_empty(), "{:?}", m.errors());
  let b = &m.bodies[0];
  let f = b.faces().iter().find(|f| (f.center - DVec3::new(20.0, 20.0, 5.0)).length() < 1e-6).unwrap();
  let face = FaceRef::from_face(b.id, f, f.center);
  d.add_feature(FeatureKind::OffsetFaces { faces: vec![face], distance: dist.into() }).unwrap();
  let m = regenerate(&mut d);
  assert!(m.errors().is_empty(), "{:?}", m.errors());
  assert_eq!(m.bodies.len(), 1);
  (m.bodies[0].shape.mass().unwrap().volume, m.bodies[0].faces().len())
}

#[test]
fn offset_face_extends_slanted_neighbours_in_new_designs_only() {
  // a design saved before OFFSET_FACES_EXTEND extrudes the face straight out: a stepped prism
  let (v, faces) = offset_trapezoid_top(Behaviours::legacy(), "5");
  assert!((v - 7000.0).abs() < 1e-6 && faces == 8, "legacy: {v} mm³, {faces} faces");
  let (v, faces) = offset_trapezoid_top(Behaviours::legacy(), "-5");
  assert!((v - 5000.0).abs() < 1e-6 && faces == 8, "legacy, inward: {v} mm³, {faces} faces");
  // a new design runs the slanted sides on to the moved face, as Fusion does
  let (v, faces) = offset_trapezoid_top(Design::new("n").behaviours, "5");
  assert!((v - 6875.0).abs() < 1e-6 && faces == 6, "new: {v} mm³, {faces} faces");
  let (v, faces) = offset_trapezoid_top(Design::new("n").behaviours, "-5");
  assert!((v - 4875.0).abs() < 1e-6 && faces == 6, "new, inward: {v} mm³, {faces} faces");
  assert!(Behaviours::current().has(fox_sketch::behaviour::OFFSET_FACES_EXTEND));
}
