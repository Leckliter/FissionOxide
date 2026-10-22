//! Combine's result bodies keep Fusion's names.

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

/// A 20 mm box at the origin and a cylinder 100 mm away (they don't touch): their ids.
fn apart() -> (Session, u32, u32) {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
  run(&mut s, "cylinder", json!({"at": [100, 0], "radius": 5, "height": 10}));
  let ids = s.model.bodies.iter().map(|b| b.id.0).collect::<Vec<_>>();
  (s, ids[0], ids[1])
}

fn volume_of(s: &Session, id: u32) -> f64 {
  let b = s.model.bodies.iter().find(|b| b.id.0 == id).expect("the body is there");
  b.shape.mass().unwrap().volume
}

#[test]
fn joining_bodies_apart_keeps_the_targets_name_on_the_target() {
  let cyl = std::f64::consts::PI * 25.0 * 10.0;
  // either way round, the target keeps its id and name; the tool's lump is a new body
  for target_is_box in [true, false] {
    let (mut s, bx, cy) = apart();
    let (target, tool, kept) = if target_is_box { (bx, cy, 8000.0) } else { (cy, bx, cyl) };
    let name = s.body_name(fox_model::BodyId(target));
    run(&mut s, "combine", json!({"target": target, "tools": [tool], "operation": "join"}));
    assert_eq!(s.model.bodies.len(), 2);
    assert!((volume_of(&s, target) - kept).abs() < 1e-3, "the target's lump keeps its id ({target_is_box})");
    assert_eq!(s.body_name(fox_model::BodyId(target)), name);
    let other = s.model.bodies.iter().find(|b| b.id.0 != target).unwrap();
    assert!(other.id.0 != tool, "the tool's lump is a new body");
  }
}

#[test]
fn deleting_a_single_hole_takes_its_placement_sketch() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 60, "depth": 40, "height": 10}));
  let b = &s.model.bodies[0];
  let face = json!({"body": b.id.0, "face": b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id});
  let r = run(&mut s, "hole", json!({"face": face, "at": [20, 20, 10], "diameter": 4, "depth": 5}));
  assert_eq!(s.design.features.len(), 3);
  let hole = r["feature"].as_u64().unwrap();
  run(&mut s, "feature.delete", json!({"feature": hole}));
  assert_eq!(s.design.features.len(), 1, "the box alone");
}
