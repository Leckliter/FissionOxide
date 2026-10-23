//! Tests for the Animation workspace's storyboards and actions.

use fox_kernel::glam::DVec3;
use serde_json::{Value, json};

use crate::Session;
use crate::scene::SceneBuilder;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

/// A 10 mm cube at `x` in its own component; returns (component, body).
fn part(s: &mut Session, name: &str, x: f64, parent: Option<u64>) -> (u64, u32) {
  run(s, "box", json!({"width": 10, "depth": 10, "height": 10}));
  let body = s.model.bodies.last().unwrap().id.0;
  if x != 0.0 {
    run(s, "move", json!({"bodies": [body], "translate": [x, 0, 0]}));
  }
  let mut p = json!({"name": name, "bodies": [body]});
  if let Some(parent) = parent {
    p["parent"] = json!(parent);
  }
  let c = run(s, "component.create", p)["component"].as_u64().unwrap();
  (c, body)
}

fn translation(pose: &Value, body: u32) -> Option<DVec3> {
  let t = pose["transforms"].as_array()?.iter().find(|t| t["body"] == body)?;
  let v: Vec<f64> = t["translation"].as_array()?.iter().filter_map(Value::as_f64).collect();
  Some(DVec3::new(v[0], v[1], v[2]))
}

#[test]
fn transform_components_records_a_move_that_plays_and_undoes() {
  let mut s = Session::default();
  let (c, body) = part(&mut s, "Lid", 0.0, None);
  let list = run(&mut s, "animation.storyboards", json!({}));
  assert_eq!(list["storyboards"][0]["name"], "Storyboard1", "one storyboard to start");
  let before = s.model.bodies[0].shape.mass().unwrap().bbox_min;
  let r = run(
    &mut s,
    "animation.transform",
    json!({"components": [c], "start": 0, "duration": 2, "translate": [0, 0, 20], "rotate": [0, 0, 90]}),
  );
  assert_eq!(r["end"], 2.0);
  let half = run(&mut s, "animation.pose", json!({"time": 1}));
  let t = translation(&half, body).unwrap();
  assert!((t.z - 10.0).abs() < 1e-6, "{half}");
  let rot = half["transforms"][0]["rotation"][2].as_f64().unwrap();
  assert!((rot - 45.0).abs() < 1e-6, "halfway round: {rot}");
  // the design itself never moves: display only
  assert_eq!(s.model.bodies[0].shape.mass().unwrap().bbox_min, before);
  assert!(s.dirty);
  // the pose draws the body moved
  let pose = crate::animation::pose(&s, 0, 2.0);
  let opts = crate::animation::view_options(&s, &pose);
  let (scene, _) = SceneBuilder::default().build(&s, &opts);
  let top = scene.meshes.iter().flat_map(|m| m.positions.iter()).map(|p| p[2]).fold(f32::MIN, f32::max);
  assert!((top - 30.0).abs() < 1e-3, "lifted 20 mm: {top}");
  assert_eq!(s.undo_history()[0], "Transform Components");
  assert!(s.undo());
  assert_eq!(run(&mut s, "animation.storyboards", json!({}))["storyboards"][0]["actions"], json!([]));
  // a transform that moves nothing is refused
  assert!(s.execute("animation.transform", &json!({"components": [c]})).is_err());
}

#[test]
fn restore_home_and_explodes_record_actions() {
  let mut s = Session::default();
  let (a, ba) = part(&mut s, "A", 0.0, None);
  let (b, bb) = part(&mut s, "B", 20.0, None);
  let (_c, bc) = part(&mut s, "C", 40.0, None);
  run(&mut s, "animation.transform", json!({"components": [a], "start": 0, "translate": [0, 5, 0]}));
  let home = run(&mut s, "animation.restore_home", json!({"start": 1}));
  assert_eq!(home["actions"].as_array().unwrap().len(), 1, "only A has moved: {home}");
  let p = run(&mut s, "animation.pose", json!({"time": 2}));
  assert!(translation(&p, ba).is_none(), "back home: {p}");
  assert!(s.execute("animation.restore_home", &json!({"start": 2})).is_err(), "already home");

  // the three spread along X, the first staying put
  let ex = run(&mut s, "animation.explode", json!({"start": 2, "levels": "one"}));
  assert_eq!(ex["actions"].as_array().unwrap().len(), 2, "{ex}");
  let p = run(&mut s, "animation.pose", json!({"time": 3}));
  let (ob, oc) = (translation(&p, bb).unwrap(), translation(&p, bc).unwrap());
  assert!(ob.x > 0.0 && oc.x > ob.x && ob.y.abs() < 1e-9, "{p}");
  assert!(translation(&p, ba).is_none());
  assert_eq!(s.undo_history()[0], "Auto Explode: One Level");

  // Manual Explode along Z: level with each other, they keep the order given; the last moves all
  // the way, the first half of it
  let m = run(
    &mut s,
    "animation.manual_explode",
    json!({"components": [b, a], "direction": "z", "distance": 30, "start": 3}),
  );
  assert_eq!(m["actions"].as_array().unwrap().len(), 2);
  let p = run(&mut s, "animation.pose", json!({"time": 10}));
  let za = translation(&p, ba).unwrap().z;
  let zb = translation(&p, bb).unwrap().z;
  assert!((za + zb - 45.0).abs() < 1e-6 && (za.max(zb) - 30.0).abs() < 1e-6, "{za} {zb}");
}

#[test]
fn all_levels_explode_the_sub_assemblies_too() {
  let mut s = Session::default();
  let top = run(&mut s, "component.create", json!({"name": "Frame"}))["component"].as_u64().unwrap();
  let (_x, bx) = part(&mut s, "X", 0.0, Some(top));
  let (_y, by) = part(&mut s, "Y", 0.0, Some(top));
  let (_z, bz) = part(&mut s, "Z", 30.0, None);
  let r = run(&mut s, "animation.explode", json!({"levels": "all", "start": 0}));
  let p = run(&mut s, "animation.pose", json!({"time": 10}));
  // Frame and Z spread at the top level, then X and Y inside Frame
  let moved = [bx, by, bz].iter().filter(|b| translation(&p, **b).is_some()).count();
  assert!(moved >= 2, "{r} {p}");
  let (tx, ty) = (translation(&p, bx).unwrap_or_default(), translation(&p, by).unwrap_or_default());
  assert!((tx - ty).length() > 1e-6, "X and Y come apart: {p}");
  assert!(r["length"].as_f64().unwrap() >= 2.0, "two levels, one after the other: {r}");
}

#[test]
fn show_hide_opacity_callouts_and_views() {
  let mut s = Session::default();
  let (c, body) = part(&mut s, "Lid", 0.0, None);
  run(&mut s, "animation.visibility", json!({"components": [c], "start": 0, "duration": 2}));
  let p = run(&mut s, "animation.pose", json!({"time": 1}));
  assert!((p["opacity"][0]["opacity"].as_f64().unwrap() - 0.5).abs() < 0.01, "fading: {p}");
  let p = run(&mut s, "animation.pose", json!({"time": 2}));
  assert_eq!(p["visible"][0], json!({"body": body, "visible": false}));
  assert!(crate::animation::shown_bodies(&s, &crate::animation::pose(&s, 0, 2.0)).is_empty());
  // toggling again shows it
  run(&mut s, "animation.visibility", json!({"components": [c]}));
  let p = run(&mut s, "animation.pose", json!({"time": 9}));
  assert_eq!(p["visible"][0]["visible"], true);
  run(&mut s, "animation.opacity", json!({"components": [c], "opacity": 0.2, "start": 3, "duration": 0}));
  let p = run(&mut s, "animation.pose", json!({"time": 9}));
  assert!((p["opacity"][0]["opacity"].as_f64().unwrap() - 0.2).abs() < 0.01, "{p}");
  // a callout moves with its body
  run(&mut s, "animation.callout", json!({"point": [5, 5, 10], "body": body, "text": "Lid", "start": 1}));
  run(&mut s, "animation.transform", json!({"components": [c], "start": 4, "duration": 1, "translate": [10, 0, 0]}));
  let p = run(&mut s, "animation.pose", json!({"time": 0.5}));
  assert_eq!(p["callouts"], json!([]), "not yet");
  let p = run(&mut s, "animation.pose", json!({"time": 6}));
  assert_eq!(p["callouts"][0]["anchor"], json!([15.0, 5.0, 10.0]));
  assert!(s.execute("animation.callout", &json!({"point": [0, 0, 0], "text": " "})).is_err());
  // a view glides the camera
  let cam = |yaw: f64| json!({"target": [0, 0, 0], "yaw": yaw, "pitch": 0.5, "distance": 100});
  run(&mut s, "animation.view", json!({"from": cam(0.0), "to": cam(1.0), "start": 6, "duration": 2}));
  let p = run(&mut s, "animation.pose", json!({"time": 7}));
  assert!((p["camera"]["yaw"].as_f64().unwrap() - 0.5).abs() < 1e-9);
  // actions can be moved, lengthened and deleted
  let list = run(&mut s, "animation.storyboards", json!({}));
  let view =
    list["storyboards"][0]["actions"].as_array().unwrap().iter().find(|a| a["label"] == "View").unwrap()["id"].clone();
  run(&mut s, "animation.action.edit", json!({"action": view, "start": 10, "duration": 4}));
  assert_eq!(run(&mut s, "animation.storyboards", json!({}))["storyboards"][0]["length"], 14.0);
  run(&mut s, "animation.action.delete", json!({"action": view}));
  assert!(run(&mut s, "animation.pose", json!({"time": 12}))["camera"].is_null());
}

#[test]
fn storyboards_are_saved_with_the_design_and_publish_a_video() {
  let mut s = Session::default();
  let (c, body) = part(&mut s, "Lid", 0.0, None);
  let sb = run(&mut s, "animation.storyboard.add", json!({"type": "start_from_end"}));
  assert_eq!(sb, json!({"index": 1, "name": "Storyboard2"}));
  run(&mut s, "animation.transform", json!({"components": [c], "translate": [0, 0, 8], "storyboard": 0}));
  // Storyboard2 starts where Storyboard1 ends
  let p = run(&mut s, "animation.pose", json!({"storyboard": "Storyboard2", "time": 0}));
  assert_eq!(translation(&p, body), Some(DVec3::new(0.0, 0.0, 8.0)));

  let dir = std::env::temp_dir().join(format!("fox-anim-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("a.fox");
  s.save_to(&path).unwrap();
  let back = Session::open(&path).unwrap();
  assert_eq!(back.design.animation, s.design.animation);

  let video = dir.join("a.avi");
  let r = run(&mut s, "animation.publish", json!({"path": video, "width": 64, "height": 48, "fps": 4}));
  assert_eq!(r["frames"], 5, "one second at 4 fps, both ends: {r}");
  let bytes = std::fs::read(&video).unwrap();
  assert_eq!(&bytes[..4], b"RIFF");
  assert_eq!(&bytes[8..12], b"AVI ");
  assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize, bytes.len() - 8);
  assert!(bytes.windows(4).any(|w| w == b"MJPG") && bytes.windows(4).any(|w| w == b"idx1"));
  let all =
    run(&mut s, "animation.publish", json!({"path": video, "width": 32, "height": 32, "fps": 2, "scope": "all"}));
  assert_eq!(all["frames"], 3 + 2, "Storyboard1 (1 s) then the empty Storyboard2: {all}");
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_coordinate_systems_are_kept_with_the_design() {
  let mut s = Session::default();
  let r = run(&mut s, "ucs.create", json!({"origin": [10, 0, 0], "x": [0, 1, 0]}));
  assert_eq!(r["name"], "UCS1");
  let l = run(&mut s, "ucs.list", json!({}));
  assert_eq!(l["coordinate_systems"][0]["z"], json!([0.0, 0.0, 1.0]));
  assert_eq!(l["coordinate_systems"][0]["y"], json!([-1.0, 0.0, 0.0]));
  assert_eq!(run(&mut s, "ucs.create", json!({}))["name"], "UCS2");
  assert!(s.execute("ucs.create", &json!({"name": "ucs1"})).is_err(), "names are unique");
  assert!(s.execute("ucs.create", &json!({"x": [0, 0, 1]})).is_err(), "X along Z");
  run(&mut s, "ucs.rename", json!({"name": "UCS2", "to": "Fixture"}));
  run(&mut s, "ucs.visible", json!({"name": "Fixture", "visible": false}));
  assert!(!s.design.user_coordinate_systems[1].visible);
  run(&mut s, "ucs.delete", json!({"name": "UCS1"}));
  assert_eq!(s.design.user_coordinate_systems.len(), 1);
  assert!(s.undo());
  assert_eq!(s.design.user_coordinate_systems.len(), 2);
}
