//! Screw joints, cam followers and component hierarchies.

use super::*;
use crate::assembly::{JointGeometry, JointKind};
use crate::*;
use fox_kernel::glam::DVec3;

const XY: PlaneRef = PlaneRef::Origin { plane: OriginPlane::Xy };

fn ok(m: &Model) {
  assert!(m.errors().is_empty(), "{:?}", m.errors());
}

fn add(d: &mut Design, shape: Primitive, at: [f64; 2]) -> BodyId {
  d.add_feature(FeatureKind::Primitive { shape, plane: XY, at, operation: Operation::NewBody }).unwrap();
  let m = regenerate(d);
  m.bodies.last().unwrap().id
}

fn cylinder(r: &str, h: &str) -> Primitive {
  Primitive::Cylinder { radius: r.into(), height: h.into() }
}

fn cube(s: &str) -> Primitive {
  Primitive::Box { width: s.into(), depth: s.into(), height: s.into() }
}

fn point(p: DVec3, z: DVec3) -> JointGeometry {
  JointGeometry::Point { point: p.to_array(), z: z.to_array(), x: [0.0, 1.0, 0.0] }
}

/// An as-built joint about / along a world axis through `at`.
fn joint(moving: ComponentId, kind: JointKind, at: DVec3, z: DVec3) -> FeatureKind {
  FeatureKind::Joint {
    moving,
    moving_origin: point(at, z),
    fixed: None,
    fixed_origin: point(at, z),
    kind,
    angle: "0".into(),
    offset: "0".into(),
    flip: false,
    as_built: true,
    rotation: "0".into(),
    slide: "0".into(),
    slide2: "0".into(),
    pitch: "0".into(),
    limits: Default::default(),
    lead: "0".into(),
  }
}

fn bbox(m: &Model, b: BodyId) -> (DVec3, DVec3) {
  let s = m.body(b).unwrap().shape.mass().unwrap();
  (s.bbox_min, s.bbox_max)
}

fn set(d: &mut Design, id: FeatureId, f: impl FnOnce(&mut FeatureKind)) {
  f(&mut d.feature_mut(id).unwrap().kind);
}

#[test]
fn screw_joint_advances_by_its_lead_per_turn() {
  let mut d = Design::new("screw");
  d.add_parameter("lead", "1.5 mm", ParamKind::Length, "").unwrap();
  let b = add(&mut d, cylinder("4", "20"), [0.0, 0.0]);
  let c = d.add_component("Screw", vec![b]).unwrap();
  let z0 = bbox(&regenerate(&mut d), b).0.z;
  let mut k = joint(c, JointKind::Screw, DVec3::ZERO, DVec3::Z);
  if let FeatureKind::Joint { lead, rotation, slide, .. } = &mut k {
    *lead = "lead".into();
    *rotation = "720 deg".into();
    // ignored: the rotation sets a screw's slide
    *slide = "50 mm".into();
  }
  let j = d.add_feature(k).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((bbox(&m, b).0.z - z0 - 3.0).abs() < 1e-6, "{:?}", bbox(&m, b));
  // a left-handed thread goes the other way; the lead follows its parameter
  d.set_parameter("lead", "-2 mm").unwrap();
  set(&mut d, j, |k| {
    if let FeatureKind::Joint { rotation, .. } = k {
      *rotation = "-90 deg".into();
    }
  });
  let m = regenerate(&mut d);
  ok(&m);
  assert!((bbox(&m, b).0.z - z0 - 0.5).abs() < 1e-6, "{:?}", bbox(&m, b));
  // a screw's slide can't be linked: its rotation drives it
  assert!(!assembly::MotionAxis::Slide.allowed(JointKind::Screw));
  assert!(assembly::MotionAxis::Rotation.allowed(JointKind::Screw));
}

/// An eccentric disc cam (radius 10, centre 5 off its axis) on a revolute joint about Z, and a
/// cube follower sliding along X that rests on it.
fn cam_design(flat: bool) -> (Design, FeatureId, BodyId, FeatureId) {
  let mut d = Design::new("cam");
  let disc = add(&mut d, cylinder("10", "5"), [5.0, 0.0]);
  let follower = add(&mut d, cube("4"), [40.0, 0.0]);
  let cam = d.add_component("Cam", vec![disc]).unwrap();
  let slider = d.add_component("Follower", vec![follower]).unwrap();
  let m = regenerate(&mut d);
  let (lo, hi) = bbox(&m, disc);
  assert!(((lo + hi) / 2.0 - DVec3::new(5.0, 0.0, 2.5)).length() < 1e-6, "{lo} {hi}");
  let (flo, fhi) = bbox(&m, follower);
  // the follower's tip: the middle of its face towards the cam, at the disc's mid-height
  let tip = DVec3::new(flo.x, (flo.y + fhi.y) / 2.0, 2.5);
  assert!(fhi.z > 2.5 && flo.z < 2.5);
  let rev = d.add_feature(joint(cam, JointKind::Revolute, DVec3::ZERO, DVec3::Z)).unwrap();
  let j = d.add_feature(joint(slider, JointKind::Slider, tip, DVec3::X)).unwrap();
  d.add_feature(FeatureKind::CamFollower { cam, follower: j, flat, clearance: "0 mm".into() }).unwrap();
  (d, rev, follower, j)
}

fn turn(d: &mut Design, rev: FeatureId, deg: f64) {
  set(d, rev, |k| {
    if let FeatureKind::Joint { rotation, .. } = k {
      *rotation = format!("{deg} deg");
    }
  });
}

#[test]
fn cam_follower_rides_the_cam_as_it_turns() {
  let (mut d, rev, follower, _) = cam_design(false);
  for deg in [0.0, 60.0, 90.0, 180.0, 250.0] {
    turn(&mut d, rev, deg);
    let m = regenerate(&mut d);
    ok(&m);
    let t = f64::to_radians(deg);
    // where the line y = 0 leaves the disc on the +X side
    let x = 5.0 * t.cos() + (100.0 - 25.0 * t.sin().powi(2)).sqrt();
    let got = bbox(&m, follower).0.x;
    assert!((got - x).abs() < 0.01, "{deg}°: follower at {got}, cam surface at {x}");
  }
  // a flat follower touches the disc's furthest point instead: 5 cos θ + 10
  let (mut d, rev, follower, _) = cam_design(true);
  turn(&mut d, rev, 90.0);
  let m = regenerate(&mut d);
  ok(&m);
  assert!((bbox(&m, follower).0.x - 10.0).abs() < 0.01, "{:?}", bbox(&m, follower));
}

#[test]
fn cam_followers_report_bad_setups() {
  let (mut d, rev, _, j) = cam_design(false);
  let (cam, slider) = (d.components[0].id, d.components[1].id);
  // the cam's own (revolute) joint doesn't slide, and a follower can't be its own cam
  for (c, f, why) in [(cam, rev, "doesn't slide"), (slider, j, "follow itself")] {
    let bad =
      d.add_feature(FeatureKind::CamFollower { cam: c, follower: f, flat: false, clearance: "0".into() }).unwrap();
    let m = regenerate(&mut d);
    assert!(m.status(bad).unwrap().error.as_deref().unwrap().contains(why), "{:?}", m.status(bad));
    d.features.retain(|f| f.id != bad);
  }
  // two cams on one joint
  let twice = d.add_feature(FeatureKind::CamFollower { cam, follower: j, flat: true, clearance: "0".into() }).unwrap();
  let m = regenerate(&mut d);
  assert!(m.status(twice).unwrap().error.as_deref().unwrap().contains("already follows"));
  d.features.retain(|f| f.id != twice);
  // the follower's line missing the cam fails the joint
  set(&mut d, j, |k| {
    if let FeatureKind::Joint { moving_origin, fixed_origin, .. } = k {
      *moving_origin = point(DVec3::new(41.0, 0.0, 30.0), DVec3::X);
      *fixed_origin = moving_origin.clone();
    }
  });
  let m = regenerate(&mut d);
  assert!(m.status(j).unwrap().error.as_deref().unwrap().contains("misses the cam"), "{:?}", m.status(j));
}

#[test]
fn joints_move_sub_components_with_their_parent() {
  let mut d = Design::new("nest");
  let a = add(&mut d, cube("4"), [0.0, 0.0]);
  let b = add(&mut d, cube("4"), [10.0, 0.0]);
  let top = d.add_component("Top", vec![a]).unwrap();
  let child = d.add_component("Child", vec![b]).unwrap();
  d.component_mut(child).unwrap().parent = Some(top);
  let before = bbox(&regenerate(&mut d), b).0;
  let mut k = joint(top, JointKind::Slider, DVec3::ZERO, DVec3::Z);
  if let FeatureKind::Joint { slide, .. } = &mut k {
    *slide = "7 mm".into();
  }
  d.add_feature(k).unwrap();
  let m = regenerate(&mut d);
  ok(&m);
  assert!((bbox(&m, b).0 - before - DVec3::new(0.0, 0.0, 7.0)).length() < 1e-6);
  // deleting the parent moves the child up a level
  let mut d2 = d.clone();
  d2.features.retain(|f| !matches!(f.kind, FeatureKind::Joint { .. }));
  d2.remove_component(top).unwrap();
  assert_eq!(d2.component(child).unwrap().parent, None);
  // the hierarchy survives saving; files without it still load
  let back = Design::from_json(&d.to_json()).unwrap();
  assert_eq!(back.component(child).unwrap().parent, Some(top));
  let plain = Design::from_json(&d2.to_json()).unwrap();
  assert!(!d2.to_json().contains("\"parent\""));
  assert_eq!(plain.components.len(), 1);
}
