//! Project To Surface and pipes along the 3D curves it makes.

use super::*;
use crate::construct::PipeSection;
use crate::*;
use fox_kernel::SurfaceKind;
use fox_sketch::{PtSpec, Sketch};

/// An r10 h30 cylinder on XY, an r5 circle on the plane x = 20 centred at (y 0, z 15) projected
/// onto its side, and a d2 pipe cut along the curves that gives.
fn groove(along: Option<AxisRef>) -> (Design, FeatureId) {
  let mut d = Design::new("t");
  let shape = Primitive::Cylinder { radius: "10".into(), height: "30".into() };
  let xy = PlaneRef::Origin { plane: OriginPlane::Xy };
  d.add_feature(FeatureKind::Primitive { shape, plane: xy, at: [0.0, 0.0], operation: Operation::NewBody }).unwrap();
  let mut sketch = Sketch::new();
  sketch.add_circle(PtSpec::At(0.0, 15.0), 5.0).unwrap();
  let plane = PlaneRef::Free { origin: [20.0, 0.0, 0.0], normal: [1.0, 0.0, 0.0], x: Some([0.0, 1.0, 0.0]) };
  let sk = d.add_feature(FeatureKind::Sketch { plane, sketch, links: vec![] }).unwrap();
  let m = regenerate(&mut d);
  let body = &m.bodies[0];
  let side = body.faces().iter().find(|f| f.kind == SurfaceKind::Cylinder).copied().unwrap();
  let face = FaceRef::from_face(body.id, &side, DVec3::new(-10.0, 0.0, 5.0));
  let proj =
    d.add_feature(FeatureKind::ProjectToSurface { sketch: sk, curves: vec![], faces: vec![face], along }).unwrap();
  d.add_feature(FeatureKind::Pipe {
    path_sketch: proj,
    path_curves: vec![],
    diameter: "2".into(),
    wall: String::new(),
    operation: Operation::Cut,
    section: PipeSection::Circular,
    distance: String::new(),
  })
  .unwrap();
  (d, proj)
}

#[test]
fn along_a_vector_cuts_two_grooves() {
  let (mut d, proj) = groove(Some(AxisRef::X));
  let m = regenerate(&mut d);
  assert!(m.errors().is_empty(), "{:?}", m.errors());
  assert_eq!(m.curves3d[&proj].len(), 2);
  let p = m.bodies[0].shape.mass().unwrap();
  // Fusion: 9326.976 mm³, 2576.520 mm²
  assert!((p.volume - 9326.976).abs() < 5.0, "{}", p.volume);
  assert!((p.area - 2576.520).abs() < 2.0, "{}", p.area);
}

#[test]
fn to_the_closest_point_cuts_one_groove() {
  let (mut d, proj) = groove(None);
  let m = regenerate(&mut d);
  assert!(m.errors().is_empty(), "{:?}", m.errors());
  assert_eq!(m.curves3d[&proj].len(), 1);
  let p = m.bodies[0].shape.mass().unwrap();
  // Fusion: 9388.016 mm³, 2537.638 mm²
  assert!((p.volume - 9388.016).abs() < 5.0, "{}", p.volume);
  assert!((p.area - 2537.638).abs() < 2.0, "{}", p.area);
}
