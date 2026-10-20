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
    json!({"moving_origin": {"origin": "Pivot"}, "fixed_origin": {"origin": "Hole"}, "kind": "revolute", "flip": true}),
  );
  let j = j["feature"].as_u64().unwrap();
  let (lo, hi) = bbox(&s, arm_body);
  assert!(near(lo, [40.0, 20.0, 10.0], 1e-3) && near(hi, [120.0, 40.0, 15.0], 1e-3), "{lo:?} {hi:?}");
  // a cap jointed to the tip follows the arm as it swings
  let (cap, cap_body) = part(&mut s, "Cap", [10.0, 10.0, 4.0], [0.0, -100.0]);
  run(
    &mut s,
    "joint",
    json!({"moving": cap, "moving_origin": {"at": [0, -100, 0], "z": [0, 0, -1]}, "fixed_origin": {"origin": "Tip"}, "flip": true}),
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
    json!({"moving": arm, "moving_origin": {"edge": {"point": [15, 200, 0]}}, "fixed_origin": {"edge": {"point": [55, 30, 10]}}, "kind": "revolute", "flip": true}),
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
fn align_moves_a_component_only_as_far_as_the_relation_needs() {
  let mut s = Session::default();
  let (plate, _) = part(&mut s, "Plate", [40.0, 30.0, 10.0], [20.0, 15.0]);
  run(&mut s, "component.ground", json!({"component": plate}));
  let (_, block) = part(&mut s, "Block", [10.0, 10.0, 10.0], [55.0, 5.0]);
  // flush: the block's bottom 5 mm above the plate's top; it keeps its x and y
  let r = run(
    &mut s,
    "align",
    json!({"moving_geometry": {"face": {"point": [55, 5, 0]}}, "fixed_geometry": {"face": {"point": [20, 15, 10]}}, "offset": "5"}),
  );
  assert_eq!(r["name"], "Align1");
  let (lo, hi) = bbox(&s, block);
  assert!(near(lo, [50.0, 0.0, 15.0], 1e-6) && near(hi, [60.0, 10.0, 25.0], 1e-6), "{lo} {hi}");
  // the offset is an expression: editing it moves the block along the normal again
  let f = r["feature"].as_u64().unwrap();
  run(&mut s, "feature.edit", json!({"feature": f, "set": {"offset": "0"}}));
  assert!(near(bbox(&s, block).0, [50.0, 0.0, 10.0], 1e-6));
  // a pin made concentric with a hole slides across onto its axis and keeps its height
  run(&mut s, "cylinder", json!({"radius": 3, "height": 10, "at": [10, 10], "operation": "cut"}));
  run(&mut s, "cylinder", json!({"radius": 2, "height": 8, "at": [100, 100], "operation": "new_body"}));
  let pin_body = s.model.bodies.last().unwrap().id.0;
  let pin_comp = run(&mut s, "component.create", json!({"name": "Pin", "bodies": [pin_body]}))["component"].clone();
  run(
    &mut s,
    "align",
    json!({"moving": pin_comp, "moving_geometry": {"face": {"point": [102, 100, 4]}}, "fixed_geometry": {"face": {"point": [13, 10, 5]}}}),
  );
  let (lo, hi) = bbox(&s, pin_body);
  assert!(near(lo, [8.0, 8.0, 0.0], 1e-6) && near(hi, [12.0, 12.0, 8.0], 1e-6), "{lo} {hi}");
  // a component can't be aligned to itself
  assert!(
    s.execute(
      "align",
      &json!({"moving_geometry": {"face": {"point": [55, 5, 10]}}, "fixed_geometry": {"face": {"point": [55, 5, 10]}}})
    )
    .is_err()
  );
}

#[test]
fn center_constraint_moves_a_component_only_along_the_plane_normal() {
  let mut s = Session::default();
  run(&mut s, "param.add", json!({"name": "wall", "expr": "10 mm"}));
  // a channel of two walls, inner faces at x = 10 and x = 40
  run(&mut s, "box", json!({"width": 10, "depth": 20, "height": 10, "at": [5, 10]}));
  run(&mut s, "box", json!({"width": "wall", "depth": 20, "height": 10, "at": [45, 10]}));
  let walls: Vec<u32> = s.model.bodies.iter().map(|b| b.id.0).collect();
  let channel = run(&mut s, "component.create", json!({"name": "Channel", "bodies": walls}))["component"].clone();
  run(&mut s, "component.ground", json!({"component": channel}));
  let (_, block) = part(&mut s, "Block", [10.0, 10.0, 10.0], [65.0, 15.0]);
  let c = run(
    &mut s,
    "constraint.center",
    json!({
      "moving_faces": [{"point": [60, 15, 5]}, {"point": [70, 15, 5]}],
      "fixed_faces": [{"point": [10, 10, 5]}, {"point": [40, 10, 5]}],
    }),
  );
  // centred in x; y and z stay where they were
  let (lo, hi) = bbox(&s, block);
  assert!(near(lo, [20.0, 10.0, 0.0], 1e-6) && near(hi, [30.0, 20.0, 10.0], 1e-6), "{lo} {hi}");
  // it re-solves when the channel changes: a 20 mm wall puts the inner face at x = 35
  run(&mut s, "param.set", json!({"name": "wall", "expr": "20 mm"}));
  let (lo, _) = bbox(&s, block);
  assert!(near(lo, [17.5, 10.0, 0.0], 1e-6), "{lo}");
  // an offset moves it along the fixed midplane's normal (+x, the first fixed face's)
  run(&mut s, "feature.edit", json!({"feature": c["feature"], "set": {"offset": "2 mm"}}));
  let (lo, _) = bbox(&s, block);
  assert!(near(lo, [19.5, 10.0, 0.0], 1e-6), "{lo}");
  // the grounded channel can't be the one that moves
  let bad = s.execute(
    "constraint.center",
    &json!({"moving_faces": [{"point": [10, 10, 5]}], "fixed_faces": [{"point": [0, 10, 5]}], "fixed": null}),
  );
  assert!(bad.unwrap()["error"].as_str().is_some_and(|e| e.contains("grounded")));
}

#[test]
fn interference_creates_bodies() {
  let mut s = Session::default();
  let cube = |s: &mut Session, at: [f64; 2], w: f64| {
    run(s, "box", json!({"at": at, "width": w, "depth": 20, "height": 20, "operation": "new_body"}));
    s.model.bodies.last().unwrap().id.0
  };
  let (a, b) = (cube(&mut s, [10.0, 10.0], 20.0), cube(&mut s, [20.0, 10.0], 20.0));
  // a third body in A's component overlaps A (and touches B): only checked when selected
  let c = cube(&mut s, [5.0, 10.0], 10.0);
  run(&mut s, "component.create", json!({"name": "A", "bodies": [a, c]}));
  run(&mut s, "component.create", json!({"name": "B", "bodies": [b]}));
  let all = run(&mut s, "assembly.interference", json!({}));
  assert_eq!(all.as_array().unwrap().len(), 1, "{all}");
  let picked = run(&mut s, "assembly.interference", json!({"bodies": [a, c]}));
  assert_eq!(picked.as_array().unwrap().len(), 1, "{picked}");
  assert!((picked[0]["volume_mm3"].as_f64().unwrap() - 4000.0).abs() < 1e-3, "{picked}");
  // the overlap of A and B becomes a body in a new component
  let n = s.model.bodies.len();
  let r = run(&mut s, "assembly.interference", json!({"bodies": [a, b], "create_bodies": true}));
  let body = r[0]["body"].as_u64().unwrap() as u32;
  assert_eq!(s.model.bodies.len(), n + 1);
  let (lo, hi) = bbox(&s, body);
  assert!(near(lo, [10.0, 0.0, 0.0], 1e-6) && near(hi, [20.0, 20.0, 20.0], 1e-6), "{lo:?} {hi:?}");
  assert_eq!(s.design.component_of(fox_model::BodyId(body)).unwrap().name, "Interference1");
  // without components it stays in the root
  s.undo();
  assert_eq!(s.model.bodies.len(), n);
  let r = run(&mut s, "assembly.interference", json!({"bodies": [a, b], "create_bodies": true, "components": false}));
  assert!(r[0]["component"].is_null(), "{r}");
  assert!(s.design.component_of(fox_model::BodyId(r[0]["body"].as_u64().unwrap() as u32)).is_none());
}

#[test]
fn arrange_lays_copies_flat_in_the_envelope() {
  let mut s = Session::default();
  // a plate standing on edge: its largest faces look along X
  let (c, src) = part(&mut s, "Plate", [10.0, 50.0, 30.0], [200.0, 0.0]);
  let r = run(
    &mut s,
    "arrange",
    json!({"components": [{"component": c, "quantity": 3}], "length": 100, "width": 60, "spacing": 2, "frame": 1}),
  );
  let fid = r["feature"].as_u64().unwrap() as u32;
  let placed: Vec<u64> = r["components"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
  assert_eq!(placed.len(), 3);
  let envelope = r["envelope"].as_u64().unwrap() as u32;
  let comps = &s.design.components;
  assert!(comps.iter().any(|k| k.id.0 == envelope && k.name == "Envelope1" && k.parent.is_some()));
  let copies: Vec<u32> = s.model.bodies.iter().filter(|b| b.id.0 / 1000 == fid).map(|b| b.id.0).collect();
  assert_eq!(copies.len(), 3);
  for b in &copies {
    assert!(comps.iter().any(|k| placed.contains(&(k.id.0 as u64)) && k.bodies.iter().any(|x| x.0 == *b)));
    let (lo, hi) = bbox(&s, *b);
    assert!((hi.z - lo.z - 10.0).abs() < 1e-6 && lo.z.abs() < 1e-6, "{lo} {hi}");
    assert!(lo.x > 1.0 - 1e-6 && lo.y > 1.0 - 1e-6 && hi.x < 99.0 + 1e-6 && hi.y < 59.0 + 1e-6, "{lo} {hi}");
  }
  // the source stays where it was
  let (lo, _) = bbox(&s, src);
  assert!((lo.x - 195.0).abs() < 1e-6);
  // a smaller envelope can't take all three
  let r = s.execute("feature.edit", &json!({"feature": fid, "set": {"length": "70"}})).unwrap();
  let err = s.model.status(fox_model::FeatureId(fid)).and_then(|x| x.error.clone());
  assert!(err.as_deref().is_some_and(|e| e.contains("don't fit")), "{r} {err:?}");
}

/// Grounded block A 20x20x10 at the origin and block B 10x6x4 at x 40..50, y 0..6, B's bottom
/// face jointed (rigid) to A's top face, flipped or not; returns B's body.
fn face_on_face(s: &mut Session, flip: bool) -> u32 {
  let (a, _) = part(s, "A", [20.0, 20.0, 10.0], [10.0, 10.0]);
  run(s, "component.ground", json!({"component": a}));
  let (b, b_body) = part(s, "B", [10.0, 6.0, 4.0], [45.0, 3.0]);
  run(
    s,
    "joint",
    json!({
      "moving": b, "moving_origin": {"face": {"point": [45, 3, 0]}},
      "fixed": a, "fixed_origin": {"face": {"point": [10, 10, 10]}},
      "kind": "rigid", "flip": flip,
    }),
  );
  b_body
}

#[test]
fn a_new_design_lines_joint_axes_up_as_fusion_does() {
  assert!(Session::default().design.behaviours.has(fox_sketch::behaviour::FUSION_JOINT_AXES));
  // unflipped, the Z axes point the same way: B's bottom normal (−Z) turns to +Z, so B turns over
  // and hangs below A's top face
  let mut s = Session::default();
  let b = face_on_face(&mut s, false);
  let (lo, hi) = bbox(&s, b);
  assert!(near(lo, [5.0, 7.0, 6.0], 1e-6) && near(hi, [15.0, 13.0, 10.0], 1e-6), "{lo:?} {hi:?}");
  // flipped, the Z axes face each other and B sits on A
  let mut s = Session::default();
  let b = face_on_face(&mut s, true);
  let (lo, hi) = bbox(&s, b);
  assert!(near(lo, [5.0, 7.0, 10.0], 1e-6) && near(hi, [15.0, 13.0, 14.0], 1e-6), "{lo:?} {hi:?}");
}

#[test]
fn a_legacy_design_keeps_its_joints_where_they_were() {
  // a design saved before behaviour flags (no `behaviours` field) faces the Z axes unless flipped,
  // as before: its unflipped joint rebuilds exactly where a new design's flipped one goes
  let legacy = |flip: bool| {
    let mut s = Session::default();
    let b = face_on_face(&mut s, flip);
    let mut file: Value = serde_json::from_str(&s.design.to_json()).unwrap();
    file.as_object_mut().unwrap().remove("behaviours");
    s.design = fox_model::Design::from_json(&file.to_string()).unwrap();
    assert!(s.design.behaviours.is_empty());
    s.rebuild();
    bbox(&s, b)
  };
  let fresh = |flip: bool| {
    let mut s = Session::default();
    let b = face_on_face(&mut s, flip);
    bbox(&s, b)
  };
  let (lo, hi) = legacy(false);
  assert!(near(lo, [5.0, 7.0, 10.0], 1e-6) && near(hi, [15.0, 13.0, 14.0], 1e-6), "{lo:?} {hi:?}");
  assert_eq!(legacy(false), fresh(true));
  assert_eq!(legacy(true), fresh(false));
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
