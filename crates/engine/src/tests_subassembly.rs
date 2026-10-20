//! Tests for screw joints, cam followers, nested sub-assemblies from inserted designs and the
//! bill of materials by level.

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

/// A box (base centred at `at`, on XY) made into its own component; returns (component, body).
fn part(s: &mut Session, name: &str, size: [f64; 3], at: [f64; 2]) -> (u64, u32) {
  run(s, "box", json!({"width": size[0], "depth": size[1], "height": size[2], "at": at}));
  let body = s.model.bodies.last().unwrap().id.0;
  let c = run(s, "component.create", json!({"name": name, "bodies": [body]}))["component"].as_u64().unwrap();
  (c, body)
}

fn bbox(s: &Session, body: u32) -> (DVec3, DVec3) {
  let m = s.model.bodies.iter().find(|b| b.id.0 == body).unwrap().shape.mass().unwrap();
  (m.bbox_min, m.bbox_max)
}

fn near(a: DVec3, b: DVec3, tol: f64) -> bool {
  (a - b).length() < tol
}

#[test]
fn screw_joints_turn_and_advance() {
  let mut s = Session::default();
  let (nut, body) = part(&mut s, "Nut", [10.0, 10.0, 5.0], [0.0, 0.0]);
  let z0 = bbox(&s, body).0.z;
  let at = json!({"at": [0, 0, 0]});
  assert!(
    s.execute(
      "joint",
      &json!({"moving": nut, "moving_origin": at, "fixed_origin": at, "kind": "screw", "as_built": true})
    )
    .is_err(),
    "a screw needs its lead"
  );
  let j = run(
    &mut s,
    "joint",
    json!({"moving": nut, "moving_origin": at, "fixed_origin": at, "kind": "screw", "lead": "2 mm", "as_built": true}),
  )["feature"]
    .as_u64()
    .unwrap();
  run(&mut s, "joint.drive", json!({"joint": j, "rotation": "540 deg"}));
  assert!((bbox(&s, body).0.z - z0 - 3.0).abs() < 1e-6, "{:?}", bbox(&s, body));
  // the lead is an expression: parameters drive it
  run(&mut s, "param.add", json!({"name": "pitch", "expr": "0.5 mm"}));
  run(&mut s, "feature.edit", json!({"feature": j, "set": {"lead": "pitch * 2"}}));
  assert!((bbox(&s, body).0.z - z0 - 1.5).abs() < 1e-6, "{:?}", bbox(&s, body));
  // a motion link can turn a screw but not slide it
  let (other, _) = part(&mut s, "Crank", [4.0, 4.0, 4.0], [50.0, 0.0]);
  let crank = run(
    &mut s,
    "joint",
    json!({"moving": other, "moving_origin": {"at": [50, 0, 0]}, "fixed_origin": {"at": [50, 0, 0]}, "kind": "revolute", "as_built": true, "rotation": "90 deg"}),
  )["feature"]
    .as_u64()
    .unwrap();
  let bad = s.execute("motion_link", &json!({"leader": crank, "follower": j, "follower_motion": "slide"})).unwrap();
  assert!(bad["error"].as_str().unwrap().contains("no slide motion"), "{bad}");
  run(&mut s, "undo", json!({}));
  run(
    &mut s,
    "motion_link",
    json!({"leader": crank, "follower": j, "leader_value": "90 deg", "follower_value": "360 deg"}),
  );
  // the crank's 90° turns the screw a whole turn: one lead (1 mm)
  assert!((bbox(&s, body).0.z - z0 - 1.0).abs() < 1e-6, "{:?}", bbox(&s, body));
}

#[test]
fn cam_followers_keep_contact_while_the_cam_turns() {
  let mut s = Session::default();
  // an eccentric disc cam: radius 10, its centre 5 off the revolute axis at the origin
  run(&mut s, "cylinder", json!({"radius": 10, "height": 5, "at": [5, 0]}));
  let disc = s.model.bodies.last().unwrap().id.0;
  let cam = run(&mut s, "component.create", json!({"name": "Cam", "bodies": [disc]}))["component"].as_u64().unwrap();
  let (follower, fbody) = part(&mut s, "Follower", [4.0, 4.0, 4.0], [42.0, 0.0]);
  let o = json!({"at": [0, 0, 0]});
  let rev = run(
    &mut s,
    "joint",
    json!({"moving": cam, "moving_origin": o, "fixed_origin": o, "kind": "revolute", "as_built": true}),
  )["feature"]
    .as_u64()
    .unwrap();
  run(&mut s, "feature.rename", json!({"feature": rev, "name": "Spin"}));
  let tip = json!({"at": [40, 0, 2.5], "z": [1, 0, 0]});
  let slide = run(
    &mut s,
    "joint",
    json!({"moving": follower, "moving_origin": tip, "fixed_origin": tip, "kind": "slider", "as_built": true}),
  )["feature"]
    .as_u64()
    .unwrap();
  let r =
    run(&mut s, "cam_follower", json!({"bodies": [disc], "follower": slide, "clearance": "1 mm", "name": "Lift"}));
  assert_eq!(r["name"], "Lift");
  for deg in [0.0_f64, 45.0, 120.0, 200.0] {
    run(&mut s, "joint.drive", json!({"joint": rev, "rotation": format!("{deg} deg")}));
    let t = deg.to_radians();
    let x = 5.0 * t.cos() + (100.0 - 25.0 * t.sin().powi(2)).sqrt() + 1.0;
    let got = bbox(&s, fbody).0.x;
    assert!((got - x).abs() < 0.01, "{deg}°: follower at {got}, expected {x}");
  }
  // a motion study of the cam moves the follower too
  let study = run(&mut s, "motion.study", json!({"joint": rev, "from": "0 deg", "to": "180 deg", "steps": 2}));
  assert!(study.as_array().unwrap().iter().all(|st| st["error"].is_null()), "{study}");
  // the browser's cam folder and the timeline see it; it can't drive a joint without a slide
  assert!(s.execute("cam_follower", &json!({"cam": cam, "follower": "Spin"})).unwrap()["error"].is_string());
  run(&mut s, "undo", json!({}));
  // saved and reopened, it still follows
  let reopened = Session::new(fox_model::Design::from_json(&s.design.to_json()).unwrap());
  assert!(reopened.model.errors().is_empty(), "{:?}", reopened.model.errors());
  assert!(near(bbox(&reopened, fbody).0, bbox(&s, fbody).0, 1e-6));
}

/// A two-part linkage (grounded base, arm on a revolute joint turned 90°) saved to a hub.
fn linkage(dir: &std::path::Path) -> (Session, String, u32, f64, u64) {
  let mut src = Session { store_root: Some(dir.to_path_buf()), ..Session::default() };
  let (base, _) = part(&mut src, "Base", [100.0, 60.0, 10.0], [50.0, 30.0]);
  run(&mut src, "component.ground", json!({"component": base}));
  let (arm, arm_body) = part(&mut src, "Arm", [80.0, 20.0, 5.0], [40.0, 200.0]);
  run(&mut src, "component.material", json!({"component": arm, "material": "Aluminum"}));
  let hinge = run(
    &mut src,
    "joint",
    json!({"moving": arm, "moving_origin": {"at": [10, 200, 0]}, "fixed": base, "fixed_origin": {"at": [50, 30, 10]}, "kind": "revolute", "rotation": "90 deg"}),
  )["feature"]
    .as_u64()
    .unwrap();
  run(&mut src, "feature.rename", json!({"feature": hinge, "name": "Hinge"}));
  let item = run(&mut src, "hub.save", json!({"name": "Linkage"}))["item"].as_str().unwrap().to_string();
  let arm_vol = src.model.bodies.iter().find(|b| b.id.0 == arm_body).unwrap().shape.mass().unwrap().volume;
  (src, item, arm_body, arm_vol, hinge)
}

fn comp_named(s: &Session, name: &str) -> fox_model::Component {
  s.design.components.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no component {name}")).clone()
}

#[test]
fn inserted_assemblies_keep_their_components_and_joints() {
  let dir = std::env::temp_dir().join(format!("fox-engine-subasm-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  let (mut src, item, src_arm, arm_vol, hinge) = linkage(&dir);
  let src_at_90 = bbox(&src, src_arm);
  run(&mut src, "joint.drive", json!({"joint": hinge, "rotation": "0 deg"}));
  let src_at_0 = bbox(&src, src_arm);
  run(&mut src, "joint.drive", json!({"joint": hinge, "rotation": "90 deg"}));
  assert!(!near(src_at_0.1, src_at_90.1, 1.0), "{src_at_0:?} {src_at_90:?}");

  let mut s = Session { store_root: Some(dir.clone()), ..Session::default() };
  let lift = DVec3::new(0.0, 0.0, 100.0);
  let top =
    run(&mut s, "component.insert", json!({"item": item, "translate": [0, 0, 100]}))["component"].as_u64().unwrap();
  // the hierarchy: Linkage > Base, Arm; the parts keep their materials
  let (base, arm) = (comp_named(&s, "Base"), comp_named(&s, "Arm"));
  assert_eq!(base.parent.map(|p| p.0 as u64), Some(top));
  assert_eq!(arm.parent.map(|p| p.0 as u64), Some(top));
  assert_eq!(arm.material, "Aluminum");
  assert!(s.design.component(fox_model::ComponentId(top as u32)).unwrap().bodies.is_empty());
  // rigid: where the source's joint put the arm
  let b = arm.bodies[0].0;
  assert!(near(bbox(&s, b).0, src_at_90.0 + lift, 1e-6), "{:?} vs {:?}", bbox(&s, b), src_at_90);
  // only the inserted component is listed as linked, not its parts
  let links = run(&mut s, "component.links", json!({}));
  assert_eq!(links.as_array().unwrap().len(), 1, "{links}");
  assert_eq!(links[0]["component"].as_u64(), Some(top));
  // the joints inside
  let joints = run(&mut s, "component.sub_joints", json!({"component": top}));
  assert_eq!(joints[0]["name"], "Hinge");
  assert_eq!(joints[0]["rotation"], "90 deg");
  assert!(s.execute("component.drive", &json!({"component": top, "joint": "Hinge", "rotation": 0})).is_err());
  // flexible: solved again here, same place until driven
  run(&mut s, "component.flexible", json!({"component": arm.id.0, "flexible": true}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!(near(bbox(&s, b).0, src_at_90.0 + lift, 1e-6), "{:?} vs {:?}", bbox(&s, b), src_at_90);
  let r = run(&mut s, "component.drive", json!({"component": top, "joint": "Hinge", "rotation": "0 deg"}));
  assert!(r["warning"].is_null(), "{r}");
  assert!(near(bbox(&s, b).0, src_at_0.0 + lift, 1e-6), "{:?} vs {:?}", bbox(&s, b), src_at_0);
  assert!(s.execute("component.drive", &json!({"component": top, "joint": "Nope", "rotation": 0})).is_err());
  // a joint in the host moves the whole sub-assembly
  let before = bbox(&s, b).0;
  run(
    &mut s,
    "joint",
    json!({"moving": top, "moving_origin": {"at": [0, 0, 0]}, "fixed_origin": {"at": [0, 0, 0]}, "kind": "slider", "as_built": true, "slide": "7 mm"}),
  );
  assert!(near(bbox(&s, b).0, before + DVec3::new(0.0, 0.0, 7.0), 1e-6));
  let base_body = base.bodies[0].0;
  assert!((bbox(&s, base_body).0.z - 107.0).abs() < 1e-6, "{:?}", bbox(&s, base_body));

  // the bill of materials by level
  let bom = run(&mut s, "assembly.bom", json!({}));
  let rows = bom.as_array().unwrap();
  assert_eq!(rows[0]["name"], "Linkage");
  assert_eq!(rows[0]["level"], 0);
  assert_eq!(rows[0]["sub_components"], 2);
  assert_eq!(rows[1]["level"], 1);
  assert_eq!(rows[1]["parent"].as_u64(), Some(top));
  let arm_row = rows.iter().find(|r| r["name"] == "Arm").unwrap();
  assert!((arm_row["mass_g"].as_f64().unwrap() - arm_vol * 2.70 / 1000.0).abs() < 1e-6);
  let total = rows[0]["total_mass_g"].as_f64().unwrap();
  let top_view = run(&mut s, "assembly.bom", json!({"view": "top"}));
  assert_eq!(top_view.as_array().unwrap().len(), 1, "{top_view}");
  assert!((top_view[0]["mass_g"].as_f64().unwrap() - total).abs() < 1e-6);
  assert_eq!(top_view[0]["bodies"], 2);
  // two copies of the linkage: the parts list counts alike parts together
  run(&mut s, "component.insert", json!({"item": item, "translate": [300, 0, 0], "name": "Linkage 2"}));
  let parts = run(&mut s, "assembly.bom", json!({"view": "parts"}));
  let arm_part = parts.as_array().unwrap().iter().find(|r| r["name"] == "Arm").unwrap();
  assert_eq!(arm_part["quantity"], 2, "{parts}");
  assert!(parts.as_array().unwrap().iter().all(|r| r["name"] != "Linkage"));
  assert!(s.execute("assembly.bom", &json!({"view": "sideways"})).is_err());

  // saved and reopened: hierarchy, flexibility and the driven joint come back
  let reopened = Session::new(fox_model::Design::from_json(&s.design.to_json()).unwrap());
  assert!(reopened.model.errors().is_empty(), "{:?}", reopened.model.errors());
  assert!(near(bbox(&reopened, b).0, bbox(&s, b).0, 1e-6));
  assert_eq!(comp_named(&reopened, "Arm").parent.map(|p| p.0 as u64), Some(top));

  // updating from the source keeps the components (and the host joint on the top one)
  run(&mut src, "box", json!({"width": 4, "depth": 4, "height": 4, "at": [-50, 0]}));
  run(&mut src, "hub.save", json!({}));
  let up = run(&mut s, "component.update_link", json!({"component": top}));
  assert_eq!(up["updated"].as_array().unwrap().len(), 1, "{up}");
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert_eq!(comp_named(&s, "Arm").id, arm.id);
  // and the motion driven here
  assert!(near(bbox(&s, b).0, src_at_0.0 + lift + DVec3::new(0.0, 0.0, 7.0), 1e-6), "{:?}", bbox(&s, b));
  // the new loose box joins the inserted component itself
  let top_comp = s.design.component(fox_model::ComponentId(top as u32)).unwrap();
  assert_eq!(top_comp.bodies.len(), 1);
  // back to rigid
  run(&mut s, "component.flexible", json!({"component": top, "flexible": false}));
  assert!(s.model.errors().is_empty());
  // nesting by hand: a component can go into another, never into itself
  let (loose, _) = part(&mut s, "Bracket", [5.0, 5.0, 5.0], [-80.0, 0.0]);
  run(&mut s, "component.parent", json!({"component": loose, "parent": top}));
  assert!(s.execute("component.parent", &json!({"component": top, "parent": loose})).is_err());
  run(&mut s, "component.parent", json!({"component": loose, "parent": null}));
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn designs_without_components_insert_as_before() {
  let dir = std::env::temp_dir().join(format!("fox-engine-subasm-plain-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  let mut src = Session { store_root: Some(dir.clone()), ..Session::default() };
  run(&mut src, "box", json!({"width": 10, "depth": 10, "height": 30}));
  let item = run(&mut src, "hub.save", json!({"name": "Bolt"}))["item"].as_str().unwrap().to_string();
  let mut s = Session { store_root: Some(dir.clone()), ..Session::default() };
  let c = run(&mut s, "component.insert", json!({"item": item}))["component"].as_u64().unwrap();
  assert_eq!(s.design.components.len(), 1);
  assert!(!s.design.to_json().contains("\"sub\""));
  assert!(s.execute("component.flexible", &json!({"component": c})).is_err());
  let _ = std::fs::remove_dir_all(&dir);
}
