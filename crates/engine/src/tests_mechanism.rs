//! Tests for roller and rocker-arm cam followers, motion links and cams inside flexible
//! sub-assemblies, and dragging components (stopping at contact sets).

use fox_kernel::SurfaceKind;
use fox_kernel::glam::DVec3;
use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => {
      assert!(v.get("error").is_none_or(Value::is_null), "{cmd} {p}: {v}");
      v
    }
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

/// The last body made into its own component; returns (component, body).
fn component(s: &mut Session, name: &str) -> (u64, u32) {
  let body = s.model.bodies.last().unwrap().id.0;
  let c = run(s, "component.create", json!({"name": name, "bodies": [body]}))["component"].as_u64().unwrap();
  (c, body)
}

fn part(s: &mut Session, name: &str, size: [f64; 3], at: [f64; 2]) -> (u64, u32) {
  run(s, "box", json!({"width": size[0], "depth": size[1], "height": size[2], "at": at}));
  component(s, name)
}

fn mass(s: &Session, body: u32) -> fox_kernel::MassProps {
  s.model.bodies.iter().find(|b| b.id.0 == body).unwrap().shape.mass().unwrap()
}

fn joint(s: &mut Session, p: Value) -> u64 {
  run(s, "joint", p)["feature"].as_u64().unwrap()
}

/// An eccentric disc cam (radius 10, its centre 5 off a revolute axis at the origin, 5 thick);
/// returns (component, body, revolute joint).
fn disc_cam(s: &mut Session) -> (u64, u32, u64) {
  run(s, "cylinder", json!({"radius": 10, "height": 5, "at": [5, 0]}));
  let (cam, disc) = component(s, "Cam");
  let o = json!({"at": [0, 0, 0]});
  let rev =
    joint(s, json!({"moving": cam, "moving_origin": o, "fixed_origin": o, "kind": "revolute", "as_built": true}));
  (cam, disc, rev)
}

/// The cam's centre when it has turned `deg`.
fn cam_centre(deg: f64) -> DVec3 {
  let t = deg.to_radians();
  DVec3::new(5.0 * t.cos(), 5.0 * t.sin(), 0.0)
}

#[test]
fn rollers_touch_the_cam_round_their_radius() {
  let mut s = Session::default();
  let (_, disc, rev) = disc_cam(&mut s);
  // a roller: a cylinder of radius 3 standing on XY, the cam's thickness, on a slider along X
  run(&mut s, "cylinder", json!({"radius": 3, "height": 5, "at": [45, 0]}));
  let (_, roller) = component(&mut s, "Roller");
  let face = s
    .model
    .body(fox_model::BodyId(roller))
    .unwrap()
    .faces()
    .iter()
    .find(|f| f.kind == SurfaceKind::Cylinder)
    .unwrap()
    .id;
  let axis = json!({"at": [45, 0, 2.5], "z": [1, 0, 0]});
  let roller_comp = s.design.component_of(fox_model::BodyId(roller)).unwrap().id.0;
  let slide = joint(
    &mut s,
    json!({"moving": roller_comp, "moving_origin": axis, "fixed_origin": axis, "kind": "slider", "as_built": true}),
  );
  let r = run(
    &mut s,
    "cam_follower",
    json!({"bodies": [disc], "follower": slide, "tip": {"face": {"body": roller, "face": face}}}),
  );
  let cam_feature = r["feature"].as_u64().unwrap();
  // the roller's centre stays 3 from the disc's edge, along the slide line
  let roller_x = |deg: f64| {
    let c = cam_centre(deg);
    c.x + (13.0_f64.powi(2) - c.y * c.y).sqrt()
  };
  for deg in [0.0_f64, 60.0, 135.0, 250.0] {
    run(&mut s, "joint.drive", json!({"joint": rev, "rotation": format!("{deg} deg")}));
    let got = mass(&s, roller).center_of_mass.x;
    assert!((got - roller_x(deg)).abs() < 0.01, "{deg}°: roller at {got}, expected {}", roller_x(deg));
  }
  // a ball roller of the same radius on a plain tip rests the same on this plate cam
  run(&mut s, "feature.delete", json!({"feature": cam_feature}));
  run(
    &mut s,
    "cam_follower",
    json!({"bodies": [disc], "follower": slide, "tip": {"at": [45, 0, 2.5]}, "roller": "3 mm"}),
  );
  let got = mass(&s, roller).center_of_mass.x;
  assert!((got - roller_x(250.0)).abs() < 0.01, "ball: {got}");
  // a flat follower has no roller
  let bad =
    s.execute("cam_follower", &json!({"bodies": [disc], "follower": slide, "flat": true, "roller": 2})).unwrap();
  assert!(bad["error"].is_string(), "{bad}");
  run(&mut s, "undo", json!({}));
  // saved and reopened, it still rolls
  let reopened = Session::new(fox_model::Design::from_json(&s.design.to_json()).unwrap());
  assert!(reopened.model.errors().is_empty(), "{:?}", reopened.model.errors());
  assert!((mass(&reopened, roller).center_of_mass.x - got).abs() < 1e-6);
}

#[test]
fn rocker_arms_swing_onto_the_cam() {
  let mut s = Session::default();
  let (_, disc, rev) = disc_cam(&mut s);
  // an arm pivoting about Z at (20, 20); its tip at (20, 0) swings on a circle of radius 20
  let (arm, arm_body) = part(&mut s, "Arm", [4.0, 20.0, 5.0], [20.0, 10.0]);
  let pivot = json!({"at": [20, 20, 0]});
  let swing = joint(
    &mut s,
    json!({"moving": arm, "moving_origin": pivot, "fixed_origin": pivot, "kind": "revolute", "as_built": true}),
  );
  let r = run(
    &mut s,
    "cam_follower",
    json!({"bodies": [disc], "follower": swing, "motion": "rotation", "tip": {"at": [20, 0, 2.5]}, "name": "Rocker"}),
  );
  let feature = r["feature"].as_u64().unwrap();
  let pivot = DVec3::new(20.0, 20.0, 0.0);
  let com0 = DVec3::new(20.0, 10.0, 0.0);
  // how far the arm has turned, from where its centre of mass went
  let turned = |s: &Session| {
    let c = mass(s, arm_body).center_of_mass - pivot;
    let c0 = com0 - pivot;
    (c0.x * c.y - c0.y * c.x).atan2(c0.x * c.x + c0.y * c.y)
  };
  let tip = |a: f64| pivot + DVec3::new(20.0 * a.sin(), -20.0 * a.cos(), 0.0);
  let mut seen = Vec::new();
  for (deg, radius) in [(0.0_f64, 10.0), (45.0, 10.0), (90.0, 10.0)] {
    run(&mut s, "joint.drive", json!({"joint": rev, "rotation": format!("{deg} deg")}));
    let a = turned(&s);
    let d = (tip(a) - cam_centre(deg)).length();
    assert!((d - radius).abs() < 0.01, "{deg}°: the tip is {d} from the cam's centre");
    // it rests on the side it started on: turning back off the cam (+) clears it
    assert!((tip(a + 0.01) - cam_centre(deg)).length() > radius, "{deg}°");
    seen.push(a);
  }
  // at 0° it's the nearer of the two touches (worked by hand: −14.535°)
  assert!((seen[0].to_degrees() + 14.535).abs() < 0.02, "{}", seen[0].to_degrees());
  // a ball roller on the tip keeps its radius off the cam
  run(&mut s, "feature.edit", json!({"feature": feature, "set": {"roller": "3 mm"}}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let d = (tip(turned(&s)) - cam_centre(90.0)).length();
  assert!((d - 13.0).abs() < 0.01, "roller: {d}");
  // a motion study of the cam swings the arm with it (as far as the arm reaches the cam)
  let study = run(&mut s, "motion.study", json!({"joint": rev, "from": "0 deg", "to": "90 deg", "steps": 3}));
  assert!(study.as_array().unwrap().iter().all(|st| st["error"].is_null()), "{study}");
  // a flat follower can't swing, and a ball joint's pitch isn't the cam's
  let bad =
    s.execute("cam_follower", &json!({"bodies": [disc], "follower": swing, "motion": "rotation", "flat": true}));
  assert!(bad.unwrap()["error"].is_string());
  run(&mut s, "undo", json!({}));
  assert!(s.execute("cam_follower", &json!({"bodies": [disc], "follower": swing, "motion": "pitch"})).is_err());
}

/// A source design with a gear pair (a motion link) and a cam pushing a slider, saved to a hub.
fn mechanism(dir: &std::path::Path) -> (Session, String) {
  let mut src = Session { store_root: Some(dir.to_path_buf()), ..Session::default() };
  let (_, disc, rev) = disc_cam(&mut src);
  run(&mut src, "feature.rename", json!({"feature": rev, "name": "Spin"}));
  let (follower, _) = part(&mut src, "Follower", [4.0, 4.0, 4.0], [42.0, 0.0]);
  let tip = json!({"at": [40, 0, 2.5], "z": [1, 0, 0]});
  let slide = joint(
    &mut src,
    json!({"moving": follower, "moving_origin": tip, "fixed_origin": tip, "kind": "slider", "as_built": true}),
  );
  run(&mut src, "feature.rename", json!({"feature": slide, "name": "Push"}));
  run(&mut src, "cam_follower", json!({"bodies": [disc], "follower": slide, "name": "Lift"}));
  let (gear, _) = part(&mut src, "Gear", [6.0, 6.0, 6.0], [0.0, 80.0]);
  let at = json!({"at": [0, 80, 0]});
  let g1 = joint(
    &mut src,
    json!({"moving": gear, "moving_origin": at, "fixed_origin": at, "kind": "revolute", "as_built": true}),
  );
  run(&mut src, "feature.rename", json!({"feature": g1, "name": "Drive"}));
  let (wheel, _) = part(&mut src, "Wheel", [6.0, 6.0, 6.0], [0.0, 120.0]);
  let at = json!({"at": [0, 120, 0]});
  let g2 = joint(
    &mut src,
    json!({"moving": wheel, "moving_origin": at, "fixed_origin": at, "kind": "revolute", "as_built": true}),
  );
  run(&mut src, "feature.rename", json!({"feature": g2, "name": "Driven"}));
  run(
    &mut src,
    "motion_link",
    json!({"leader": g1, "follower": g2, "leader_value": "90 deg", "follower_value": "45 deg", "reverse": true, "name": "Gears"}),
  );
  let item = run(&mut src, "hub.save", json!({"name": "Mechanism"}))["item"].as_str().unwrap().to_string();
  (src, item)
}

fn comp_named(s: &Session, name: &str) -> fox_model::Component {
  s.design.components.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no component {name}")).clone()
}

#[test]
fn flexible_sub_assemblies_keep_their_links_and_cams() {
  let dir = std::env::temp_dir().join(format!("fox-engine-mechanism-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  let (_src, item) = mechanism(&dir);
  let mut s = Session { store_root: Some(dir.clone()), ..Session::default() };
  let top =
    run(&mut s, "component.insert", json!({"item": item, "translate": [0, 0, 100]}))["component"].as_u64().unwrap();
  run(&mut s, "component.flexible", json!({"component": top}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let follower = comp_named(&s, "Follower").bodies[0].0;
  let wheel = comp_named(&s, "Wheel").bodies[0].0;
  // the link and the cam came along: the listing says what sets which motion
  let joints = run(&mut s, "component.sub_joints", json!({"component": top}));
  let row = |n: &str| joints.as_array().unwrap().iter().find(|j| j["name"] == n).unwrap().clone();
  assert_eq!(row("Driven")["linked"][0]["by"], "Gears", "{joints}");
  assert_eq!(row("Push")["cam"]["by"], "Lift", "{joints}");
  assert!(row("Spin")["cam"].is_null());
  // driving the cam inside the sub-assembly moves its follower
  for deg in [0.0_f64, 45.0, 120.0] {
    let r =
      run(&mut s, "component.drive", json!({"component": top, "joint": "Spin", "rotation": format!("{deg} deg")}));
    assert!(r["warning"].is_null(), "{r}");
    let t = deg.to_radians();
    let x = 5.0 * t.cos() + (100.0 - 25.0 * t.sin().powi(2)).sqrt();
    let got = mass(&s, follower).bbox_min.x;
    assert!((got - x).abs() < 0.01, "{deg}°: follower at {got}, expected {x}");
    assert!((mass(&s, follower).bbox_min.z - 100.0).abs() < 1e-6);
  }
  // driving the leading gear turns the other one back half as far
  let corner = |s: &Session| {
    let m = mass(s, wheel);
    (m.bbox_max.x - m.bbox_min.x, m.center_of_mass)
  };
  let (w0, c0) = corner(&s);
  run(&mut s, "component.drive", json!({"component": top, "joint": "Drive", "rotation": "90 deg"}));
  let (w1, c1) = corner(&s);
  // a 6 × 6 square turned 45° is 6√2 wide
  assert!((w0 - 6.0).abs() < 1e-6 && (w1 - 6.0 * 2f64.sqrt()).abs() < 1e-6, "{w0} {w1}");
  assert!((c1 - c0).length() < 1e-6);
  // the driven motions can't be driven by hand
  let e =
    s.execute("component.drive", &json!({"component": top, "joint": "Driven", "rotation": "10 deg"})).unwrap_err();
  assert!(e.0.contains("Gears"), "{e}");
  assert!(s.execute("component.drive", &json!({"component": top, "joint": "Push", "slide": "1 mm"})).is_err());
  // saved and reopened, they still act
  let reopened = Session::new(fox_model::Design::from_json(&s.design.to_json()).unwrap());
  assert!(reopened.model.errors().is_empty(), "{:?}", reopened.model.errors());
  assert!((mass(&reopened, wheel).center_of_mass - c1).length() < 1e-6);
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dragged_components_follow_their_joints_and_stop_at_contact() {
  let mut s = Session::default();
  let (base, _) = part(&mut s, "Base", [100.0, 60.0, 10.0], [50.0, 30.0]);
  run(&mut s, "component.ground", json!({"component": base}));
  let (block, block_body) = part(&mut s, "Block", [10.0, 10.0, 10.0], [-50.0, 30.0]);
  let j = joint(
    &mut s,
    json!({"moving": block, "moving_origin": {"at": [0, 0, 0]}, "fixed_origin": {"at": [0, 0, 0], "z": [1, 0, 0]}, "kind": "slider", "as_built": true}),
  );
  // without contact sets, the block slides as far as the cursor goes along its slide line,
  // looking down at it
  let drag = json!({"component": block, "from": [-50, 30, 10], "to": [-20, 50, 200], "view": [0, 0, -1]});
  let r = run(&mut s, "component.drag", drag.clone());
  assert_eq!(r["joint"].as_u64(), Some(j), "{r}");
  assert!((mass(&s, block_body).center_of_mass.x - -20.0).abs() < 1e-6, "{r}");
  // one step to undo
  run(&mut s, "undo", json!({}));
  assert!((mass(&s, block_body).center_of_mass.x - -50.0).abs() < 1e-6);
  // with a contact set it stops where the block meets the base
  run(&mut s, "contact_set.create", json!({"components": [base, block]}));
  let r = run(&mut s, "component.drag", json!({"component": block, "from": [-50, 30, 10], "to": [60, 30, 10]}));
  assert_eq!(r["contact"], true, "{r}");
  let hi = mass(&s, block_body).bbox_max.x;
  assert!(hi <= 1e-6 && hi > -0.02, "{hi}");
  // limits hold a drag too
  let (stop, stop_body) = part(&mut s, "Stop", [4.0, 4.0, 4.0], [0.0, -200.0]);
  let at = json!({"at": [0, -200, 0], "z": [1, 0, 0]});
  joint(
    &mut s,
    json!({"moving": stop, "moving_origin": at, "fixed_origin": at, "kind": "slider", "as_built": true, "limits": {"slide": ["-10 mm", "20 mm"]}}),
  );
  run(&mut s, "component.drag", json!({"component": stop, "from": [0, -200, 2], "to": [80, -200, 2]}));
  assert!((mass(&s, stop_body).center_of_mass.x - 20.0).abs() < 1e-6);

  // a revolute arm turns so the grabbed point points at the cursor
  let (arm, arm_body) = part(&mut s, "Arm", [20.0, 4.0, 4.0], [30.0, -100.0]);
  let o = json!({"at": [0, -100, 0]});
  joint(&mut s, json!({"moving": arm, "moving_origin": o, "fixed_origin": o, "kind": "revolute", "as_built": true}));
  run(
    &mut s,
    "component.drag",
    json!({"component": arm, "from": [30, -100, 2], "to": [0, -70, 50], "view": [0, 0, -1]}),
  );
  let c = mass(&s, arm_body).center_of_mass;
  assert!((c - DVec3::new(0.0, -70.0, 2.0)).length() < 1e-4, "{c}");

  // an unjointed component slides in the view plane, as one Move, and stops at contact
  let (wall, _) = part(&mut s, "Wall", [10.0, 60.0, 40.0], [200.0, 30.0]);
  run(&mut s, "component.ground", json!({"component": wall}));
  let (free, free_body) = part(&mut s, "Free", [10.0, 10.0, 10.0], [150.0, 30.0]);
  let n = s.design.features.len();
  run(
    &mut s,
    "component.drag",
    json!({"component": free, "from": [150, 30, 5], "to": [160, 40, 300], "view": [0, 0, -1]}),
  );
  assert_eq!(s.design.features.len(), n + 1);
  let c = mass(&s, free_body).center_of_mass;
  assert!((c - DVec3::new(160.0, 40.0, 5.0)).length() < 1e-6, "{c}");
  // dragged again, the same Move takes it on
  run(&mut s, "contact_set.create", json!({"components": [wall, free]}));
  let r = run(&mut s, "component.drag", json!({"component": free, "from": [160, 40, 5], "to": [260, 40, 5]}));
  assert_eq!(s.design.features.len(), n + 1);
  assert_eq!(r["contact"], true, "{r}");
  let hi = mass(&s, free_body).bbox_max.x;
  assert!(hi <= 195.0 + 1e-6 && hi > 195.0 - 0.02, "{hi}");

  // grounded and rigidly jointed components don't drag
  assert!(s.execute("component.drag", &json!({"component": wall, "from": [0, 0, 0], "to": [1, 0, 0]})).is_err());
  let (fixed, _) = part(&mut s, "Fixed", [4.0, 4.0, 4.0], [0.0, 300.0]);
  let at = json!({"at": [0, 300, 0]});
  joint(&mut s, json!({"moving": fixed, "moving_origin": at, "fixed_origin": at, "as_built": true}));
  let e =
    s.execute("component.drag", &json!({"component": fixed, "from": [0, 300, 0], "to": [5, 300, 0]})).unwrap_err();
  assert!(e.0.contains("rigid"), "{e}");
}
