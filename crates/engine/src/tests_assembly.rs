//! Tests for joint origins, rigid groups, motion links, contact sets and components inserted
//! from the data hub.

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

fn near(a: DVec3, b: [f64; 3], tol: f64) -> bool {
  (a - DVec3::from(b)).length() < tol
}

/// The base plate and swing arm of `assembly_with_a_revolute_joint`.
fn base_and_arm(s: &mut Session) -> (u64, u64, u32) {
  let (base, _) = part(s, "Base", [100.0, 60.0, 10.0], [50.0, 30.0]);
  run(s, "cylinder", json!({"radius": 5, "height": 10, "at": [50, 30], "operation": "cut"}));
  run(s, "component.ground", json!({"component": base}));
  let (arm, arm_body) = part(s, "Arm", [80.0, 20.0, 5.0], [40.0, 200.0]);
  run(s, "cylinder", json!({"radius": 5, "height": 5, "at": [10, 200], "operation": "cut"}));
  (base, arm, arm_body)
}

#[test]
fn joints_mate_saved_joint_origins_that_follow_their_component() {
  let mut s = Session::default();
  let (_, arm, arm_body) = base_and_arm(&mut s);
  // the component comes from the picked edge's body
  let hole = run(&mut s, "joint_origin", json!({"edge": {"point": [55, 30, 10]}, "name": "Hole"}));
  assert!(near(
    DVec3::from_slice(&serde_json::from_value::<Vec<f64>>(hole["origin"].clone()).unwrap()),
    [50.0, 30.0, 10.0],
    1e-6
  ));
  run(&mut s, "joint_origin", json!({"edge": {"point": [15, 200, 0]}, "name": "Pivot"}));
  // a point on the arm's far end, saved before the arm is jointed
  run(&mut s, "joint_origin", json!({"component": arm, "at": [70, 200, 5], "name": "Tip"}));
  // no components given: the saved origins know theirs
  let j = run(
    &mut s,
    "joint",
    json!({"moving_origin": {"origin": "Pivot"}, "fixed_origin": {"origin": "Hole"}, "kind": "revolute"}),
  );
  let j = j["feature"].as_u64().unwrap();
  let (lo, hi) = bbox(&s, arm_body);
  assert!(near(lo, [40.0, 20.0, 10.0], 1e-3) && near(hi, [120.0, 40.0, 15.0], 1e-3), "{lo:?} {hi:?}");
  // a cap jointed to the tip follows the arm as it swings
  let (cap, cap_body) = part(&mut s, "Cap", [10.0, 10.0, 4.0], [0.0, -100.0]);
  run(
    &mut s,
    "joint",
    json!({"moving": cap, "moving_origin": {"at": [0, -100, 0], "z": [0, 0, -1]}, "fixed_origin": {"origin": "Tip"}}),
  );
  let (lo, hi) = bbox(&s, cap_body);
  assert!(near((lo + hi) / 2.0, [110.0, 30.0, 17.0], 1e-3), "{lo:?} {hi:?}");
  run(&mut s, "joint.drive", json!({"joint": j, "rotation": "90 deg"}));
  let (lo, hi) = bbox(&s, cap_body);
  assert!(near((lo + hi) / 2.0, [50.0, 90.0, 17.0], 1e-3), "{lo:?} {hi:?}");
  // an offset joint origin lifts what mates to it
  let (lo, _) = bbox(&s, arm_body);
  run(&mut s, "feature.edit", json!({"feature": hole["feature"], "set": {"offset": "5 mm"}}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!((bbox(&s, arm_body).0.z - lo.z - 5.0).abs() < 1e-6);
  // a joint origin is in use: deleting it takes its joints along
  let removed = run(&mut s, "feature.delete", json!({"feature": hole["feature"]}));
  assert!(!s.design.features.iter().any(|f| f.id.0 as u64 == j), "{removed}");
  let bad = s.execute("joint", &json!({"moving_origin": {"origin": "Nope"}, "fixed_origin": {"at": [0, 0, 0]}}));
  assert!(bad.is_err());
}

#[test]
fn rigid_groups_move_together() {
  let mut s = Session::default();
  let (a, a_body) = part(&mut s, "A", [10.0, 10.0, 10.0], [0.0, 0.0]);
  let (b, b_body) = part(&mut s, "B", [10.0, 10.0, 10.0], [50.0, 0.0]);
  let (c, c_body) = part(&mut s, "C", [10.0, 10.0, 10.0], [100.0, 0.0]);
  let g = run(&mut s, "rigid_group", json!({"bodies": [a_body, b_body], "name": "Pair"}));
  assert_eq!(g["name"], "Pair");
  // an as-built slider lifts A, and B with it; C stays
  let r = run(
    &mut s,
    "joint",
    json!({"moving": a, "moving_origin": {"at": [0, 0, 0]}, "fixed_origin": {"at": [0, 0, 0]}, "kind": "slider", "as_built": true, "slide": "10 mm"}),
  );
  assert!(r["error"].is_null(), "{r}");
  assert!((bbox(&s, a_body).0.z - 10.0).abs() < 1e-6);
  assert!((bbox(&s, b_body).0.z - 10.0).abs() < 1e-6);
  assert!(bbox(&s, c_body).0.z.abs() < 1e-6);
  // a group can't be jointed to itself, or moved when a member is grounded
  let bad = s.execute(
    "joint",
    &json!({"moving": a, "fixed": b, "moving_origin": {"at": [0, 0, 0]}, "fixed_origin": {"at": [0, 0, 5]}}),
  );
  assert!(bad.unwrap()["error"].as_str().unwrap().contains("same rigid group"));
  s.undo();
  run(&mut s, "component.ground", json!({"component": b}));
  assert!(s.model.errors().iter().any(|e| e.contains("grounded")), "{:?}", s.model.errors());
  run(&mut s, "component.ground", json!({"component": b, "grounded": false}));
  // removing the group frees B
  run(&mut s, "feature.delete", json!({"feature": g["feature"]}));
  assert!(bbox(&s, b_body).0.z.abs() < 1e-6);
  assert!(s.execute("rigid_group", &json!({"components": [c]})).is_err());
  assert!(s.execute("component.delete", &json!({"component": c})).is_ok());
}

#[test]
fn rigid_groups_bring_along_parts_modeled_after_the_joint() {
  let mut s = Session::default();
  let (_, arm, arm_body) = base_and_arm(&mut s);
  let j = run(
    &mut s,
    "joint",
    json!({"moving": arm, "moving_origin": {"edge": {"point": [15, 200, 0]}}, "fixed_origin": {"edge": {"point": [55, 30, 10]}}, "kind": "revolute"}),
  )["feature"]
    .clone();
  // a weight modeled where it should stay relative to the jointed arm, then grouped with it
  let (weight, weight_body) = part(&mut s, "Weight", [10.0, 20.0, 10.0], [200.0, 30.0]);
  run(&mut s, "rigid_group", json!({"components": [arm, weight]}));
  let center = |s: &Session, b: u32| {
    let (lo, hi) = bbox(s, b);
    (lo + hi) / 2.0
  };
  assert!(near(center(&s, weight_body), [200.0, 30.0, 5.0], 1e-6));
  // swinging the arm a quarter turn about (50, 30) takes the weight along
  run(&mut s, "joint.drive", json!({"joint": j, "rotation": "90 deg"}));
  assert!(near(center(&s, weight_body), [50.0, 180.0, 5.0], 1e-6), "{:?}", center(&s, weight_body));
  assert!(near(center(&s, arm_body), [50.0, 60.0, 12.5], 1e-6), "{:?}", center(&s, arm_body));
  run(&mut s, "joint.drive", json!({"joint": j, "rotation": "0 deg"}));
  assert!(near(center(&s, weight_body), [200.0, 30.0, 5.0], 1e-6));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
}

#[test]
fn motion_links_drive_one_joint_from_another() {
  let mut s = Session::default();
  let (pinion, pinion_body) = part(&mut s, "Pinion", [10.0, 10.0, 10.0], [0.0, 0.0]);
  let (rack, rack_body) = part(&mut s, "Rack", [40.0, 5.0, 5.0], [0.0, 20.0]);
  let (lift, lift_body) = part(&mut s, "Lift", [5.0, 5.0, 5.0], [0.0, 60.0]);
  let as_built = |moving: u64, kind: &str, z: [f64; 3]| json!({"moving": moving, "moving_origin": {"at": [0, 0, 0]}, "fixed_origin": {"at": [0, 0, 0], "z": z}, "kind": kind, "as_built": true});
  let turn = run(&mut s, "joint", as_built(pinion, "revolute", [0.0, 0.0, 1.0]))["feature"].clone();
  let slide = run(&mut s, "joint", as_built(rack, "slider", [1.0, 0.0, 0.0]))["feature"].clone();
  let up = run(&mut s, "joint", as_built(lift, "slider", [0.0, 0.0, 1.0]))["feature"].clone();
  // rack and pinion: a turn of the pinion moves the rack its pitch circumference (r = 5)
  let link = run(
    &mut s,
    "motion_link",
    json!({"leader": turn, "follower": slide, "leader_value": "360 deg", "follower_value": "2 * pi * 5 mm"}),
  );
  run(&mut s, "joint.drive", json!({"joint": turn, "rotation": "90 deg"}));
  let x = |s: &Session, b: u32| bbox(s, b).0.x;
  assert!((x(&s, rack_body) - (-20.0 + std::f64::consts::PI * 2.5)).abs() < 1e-6, "{}", x(&s, rack_body));
  // links chain: the rack drives the lift at half speed, reversed
  run(&mut s, "motion_link", json!({"leader": slide, "follower": up, "follower_value": "5 mm", "reverse": true}));
  assert!((bbox(&s, lift_body).0.z + std::f64::consts::PI * 1.25).abs() < 1e-6, "{:?}", bbox(&s, lift_body));
  // editing the ratio (a later feature) rebuilds the joints before it
  run(&mut s, "feature.edit", json!({"feature": link["feature"], "set": {"follower_value": "0 mm"}}));
  assert!((x(&s, rack_body) + 20.0).abs() < 1e-6);
  // the follower's own value is ignored while linked; a loop or a second driver is an error
  run(&mut s, "joint.drive", json!({"joint": slide, "slide": "7 mm"}));
  assert!((x(&s, rack_body) + 20.0).abs() < 1e-6);
  let lp = s.execute("motion_link", &json!({"leader": up, "follower": turn, "follower_motion": "rotation"})).unwrap();
  assert!(lp["error"].as_str().unwrap().contains("loop"), "{lp}");
  s.undo();
  let twice = s.execute("motion_link", &json!({"leader": up, "follower": slide})).unwrap();
  assert!(twice["error"].as_str().unwrap().contains("already driven"), "{twice}");
  s.undo();
  assert!(s.execute("motion_link", &json!({"leader": turn, "follower": "Nope"})).is_err());
  let bad = s.execute("motion_link", &json!({"leader": turn, "follower": up, "follower_motion": "rotation"})).unwrap();
  assert!(bad["error"].as_str().unwrap().contains("no rotation"), "{bad}");
  s.undo();
  // deleting the leader takes its links along
  run(&mut s, "feature.delete", json!({"feature": turn}));
  assert!(!s.design.features.iter().any(|f| f.id.0 as u64 == link["feature"].as_u64().unwrap()));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let _ = pinion_body;
}

#[test]
fn contact_sets_stop_motion_at_contact() {
  let mut s = Session::default();
  let (base, _) = part(&mut s, "Base", [100.0, 60.0, 10.0], [50.0, 30.0]);
  run(&mut s, "component.ground", json!({"component": base}));
  let (block, block_body) = part(&mut s, "Block", [10.0, 10.0, 10.0], [-50.0, 30.0]);
  let j = run(
    &mut s,
    "joint",
    json!({"moving": block, "moving_origin": {"at": [0, 0, 0]}, "fixed_origin": {"at": [0, 0, 0], "z": [1, 0, 0]}, "kind": "slider", "as_built": true}),
  )["feature"]
    .clone();
  let cs = run(&mut s, "contact_set.create", json!({"components": [base, block]}));
  let id = cs["contact_set"].clone();
  assert_eq!(run(&mut s, "contact_set.list", json!({})).as_array().unwrap().len(), 1);
  // the block slides toward the base and stops where they touch (block max x = 0)
  let r = run(&mut s, "joint.drive", json!({"joint": j, "slide": "100 mm"}));
  assert_eq!(r["contact"], true, "{r}");
  let hi = bbox(&s, block_body).1.x;
  assert!(hi <= 1e-6 && hi > -0.02, "{hi}");
  assert!(run(&mut s, "assembly.interference", json!({})).as_array().unwrap().is_empty());
  // a motion study stops there too
  run(&mut s, "joint.drive", json!({"joint": j, "slide": "0 mm"}));
  let study = run(&mut s, "motion.study", json!({"joint": j, "motion": "slide", "from": 0, "to": 100, "steps": 4}));
  let steps = study.as_array().unwrap();
  assert_eq!(steps.len(), 3, "{study}");
  let last = steps.last().unwrap();
  assert_eq!(last["contact"], true);
  assert!((last["value"].as_f64().unwrap() - 45.0).abs() < 0.02, "{study}");
  // disabled, the block goes straight through
  run(&mut s, "contact_set.enable", json!({"contact_set": id, "enabled": false}));
  let r = run(&mut s, "joint.drive", json!({"joint": j, "slide": "100 mm"}));
  assert!(r.get("contact").is_none(), "{r}");
  assert_eq!(run(&mut s, "assembly.interference", json!({})).as_array().unwrap().len(), 1);
  run(&mut s, "contact_set.delete", json!({"contact_set": id}));
  assert!(s.design.contact_sets.is_empty());
  assert!(s.execute("contact_set.create", &json!({"components": [base]})).is_err());
}

#[test]
fn inserted_hub_designs_update_and_unlink() {
  let dir = std::env::temp_dir().join(format!("fox-engine-insert-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  // the source design: one bolt-ish block, saved to the hub
  let mut src = Session { store_root: Some(dir.clone()), ..Session::default() };
  run(&mut src, "box", json!({"width": 10, "depth": 10, "height": 30}));
  let item = run(&mut src, "hub.save", json!({"name": "Bolt"}))["item"].as_str().unwrap().to_string();
  // the assembly inserts it, linked
  let mut s = Session { store_root: Some(dir.clone()), ..Session::default() };
  run(&mut s, "box", json!({"width": 100, "depth": 100, "height": 5}));
  let r = run(&mut s, "component.insert", json!({"item": item, "translate": [0, 0, 5]}));
  assert_eq!(r["version"], 1);
  let c = r["component"].as_u64().unwrap();
  let comp = s.design.components.iter().find(|x| x.id.0 as u64 == c).unwrap();
  assert_eq!(comp.name, "Bolt");
  let b = comp.bodies[0].0;
  let (lo, hi) = bbox(&s, b);
  assert!(near(lo, [-5.0, -5.0, 5.0], 1e-6) && near(hi, [5.0, 5.0, 35.0], 1e-6), "{lo:?} {hi:?}");
  let links = run(&mut s, "component.links", json!({}));
  assert_eq!(links[0]["out_of_date"], false, "{links}");
  assert_eq!(links[0]["source_name"], "Bolt");
  // a new version of the source shows up as out of date, and updating brings it in
  run(&mut src, "box", json!({"width": 4, "depth": 4, "height": 4, "at": [20, 0]}));
  assert_eq!(run(&mut src, "hub.save", json!({}))["version"], 2);
  let links = run(&mut s, "component.links", json!({}));
  assert_eq!(links[0]["out_of_date"], true, "{links}");
  assert_eq!(links[0]["latest"], 2);
  let up = run(&mut s, "component.update_link", json!({}));
  assert_eq!(up["updated"].as_array().unwrap().len(), 1, "{up}");
  let comp = s.design.components.iter().find(|x| x.id.0 as u64 == c).unwrap();
  assert_eq!(comp.bodies.len(), 2);
  assert_eq!(s.model.bodies.len(), 3);
  // the inserted bodies are stored in the design: they come back without the hub
  let reopened = Session::new(fox_model::Design::from_json(&s.design.to_json()).unwrap());
  assert_eq!(reopened.model.bodies.len(), 3);
  assert!(reopened.model.errors().is_empty());
  // jointing and the parts list treat it like any component
  run(&mut s, "joint", json!({"moving": c, "moving_origin": {"at": [0, 0, 5]}, "fixed_origin": {"at": [0, 0, 5]}}));
  let bom = run(&mut s, "assembly.bom", json!({}));
  assert!(bom.as_array().unwrap().iter().any(|r| r["name"] == "Bolt" && r["bodies"] == 2), "{bom}");
  // break the link: a local copy that no longer updates
  run(&mut s, "component.break_link", json!({"component": c}));
  run(&mut src, "box", json!({"width": 4, "depth": 4, "height": 4, "at": [-20, 0]}));
  run(&mut src, "hub.save", json!({}));
  let links = run(&mut s, "component.links", json!({}));
  assert_eq!(links[0]["linked"], false);
  assert!(run(&mut s, "component.update_link", json!({}))["updated"].as_array().unwrap().is_empty());
  assert!(s.execute("component.break_link", &json!({"component": 999})).is_err());
  // an unlinked insert of an old version
  let r = run(&mut s, "component.insert", json!({"item": item, "version": 1, "link": false, "name": "Old"}));
  assert_eq!(r["linked"], false);
  assert!(s.execute("component.insert", &json!({"item": item, "version": 9})).is_err());
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn as_built_joints_keep_components_where_they_sit() {
  let mut s = Session::default();
  let (base, base_body) = part(&mut s, "Base", [100.0, 60.0, 10.0], [50.0, 120.0]);
  run(&mut s, "component.ground", json!({"component": base}));
  let (arm, arm_body) = part(&mut s, "Arm", [40.0, 10.0, 5.0], [20.0, 30.0]);
  let (lo, hi) = bbox(&s, arm_body);
  // jointed at the arm's left end, about Z: nothing moves when the joint is made
  let at = json!({"at": [0, 30, 0], "z": [0, 0, 1]});
  let j = run(
    &mut s,
    "joint",
    json!({"moving": arm, "fixed": base, "moving_origin": at, "fixed_origin": at, "kind": "revolute", "as_built": true}),
  );
  assert!(near(bbox(&s, arm_body).0, lo.to_array(), 1e-6) && near(bbox(&s, arm_body).1, hi.to_array(), 1e-6));
  assert!(near(bbox(&s, base_body).0, [0.0, 90.0, 0.0], 1e-6));
  // and it turns about the picked position
  run(&mut s, "joint.drive", json!({"joint": j["feature"], "rotation": "90 deg"}));
  let (lo, hi) = bbox(&s, arm_body);
  assert!(near(lo, [-5.0, 30.0, 0.0], 1e-6) && near(hi, [5.0, 70.0, 5.0], 1e-6), "{lo} {hi}");
}

#[test]
fn new_component_options_and_activation() {
  let mut s = Session::default();
  let (top, _) = part(&mut s, "Frame", [10.0, 10.0, 10.0], [0.0, 0.0]);
  // an empty sheet metal component inside Frame, with its part number and description
  let c = run(
    &mut s,
    "component.create",
    json!({"name": "Bracket", "type": "sheet_metal", "part_number": "BR-01", "description": "Mount", "parent": top}),
  )["component"]
    .as_u64()
    .unwrap();
  let p = run(&mut s, "component.properties", json!({"component": c}));
  assert_eq!(p["part_number"], "BR-01");
  assert_eq!(p["description"], "Mount");
  assert_eq!(p["type"], "sheet_metal");
  assert_eq!(p["parent"], top);
  assert_eq!(p["bodies"], 0);
  let p = run(&mut s, "component.properties", json!({"component": c, "part_number": "BR-02", "type": "standard"}));
  assert_eq!((p["part_number"].as_str(), p["type"].as_str()), (Some("BR-02"), Some("standard")));
  assert!(s.execute("component.create", &json!({"type": "wooden"})).is_err());
  // while Bracket is active, new bodies go into it (and undo takes them out with the feature)
  run(&mut s, "component.activate", json!({"component": c}));
  assert_eq!(run(&mut s, "component.properties", json!({"component": c}))["active"], true);
  run(&mut s, "box", json!({"width": 5, "depth": 5, "height": 5, "at": [50, 0]}));
  let b = s.model.bodies.last().unwrap().id;
  assert_eq!(s.design.component_of(b).map(|x| x.id.0 as u64), Some(c));
  s.undo();
  assert!(s.design.component(fox_model::ComponentId(c as u32)).unwrap().bodies.is_empty());
  // back at the top level, new bodies stay in the root
  run(&mut s, "component.activate", json!({}));
  run(&mut s, "box", json!({"width": 5, "depth": 5, "height": 5, "at": [50, 0]}));
  assert!(s.design.component_of(s.model.bodies.last().unwrap().id).is_none());
  // "activate" on create
  let d = run(&mut s, "component.create", json!({"name": "Lid", "activate": true}))["component"].as_u64().unwrap();
  assert_eq!(s.active_component.map(|x| x.0 as u64), Some(d));
}
