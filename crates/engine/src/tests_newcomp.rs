//! Tests for the New Component operation and `component.next_number`.

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

fn plate(s: &mut Session, x: f64) {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [x, 0], "corner2": [x + 20.0, 10]}));
}

/// The component holding `body`, by name.
fn comp_of(s: &Session, body: u64) -> Option<String> {
  s.design.component_of(fox_model::BodyId(body as u32)).map(|c| c.name.clone())
}

#[test]
fn extrude_new_component_puts_its_body_in_a_new_component() {
  let mut s = Session::default();
  plate(&mut s, 0.0);
  let r = run(&mut s, "extrude", json!({"distance": 2, "operation": "new_component", "component_name": "01 - Back"}));
  let body = r["result_bodies"][0].as_u64().unwrap();
  assert_eq!(comp_of(&s, body).as_deref(), Some("01 - Back"));
  let feature = s.design.features.last().unwrap();
  let comp = feature.component.unwrap();
  assert_eq!(s.design.component(comp).unwrap().parent, None);
  let fid = feature.id.0;

  // the feature regenerates its body in the component
  run(&mut s, "feature.edit", json!({"feature": fid, "set": {"extent": {"distance": "5 mm"}}}));
  assert_eq!(comp_of(&s, body).as_deref(), Some("01 - Back"));
  let vol = s.model.body(fox_model::BodyId(body as u32)).unwrap().shape.mass().unwrap().volume;
  assert!((vol - 1000.0).abs() < 1e-6, "{vol}");

  // one undo step takes the feature and its component away
  run(&mut s, "undo", json!({}));
  run(&mut s, "undo", json!({}));
  assert!(s.design.components.is_empty());

  // without a name it gets the next default name; a box can make one too
  run(&mut s, "redo", json!({}));
  let r =
    run(&mut s, "box", json!({"at": [100, 0], "width": 5, "depth": 5, "height": 5, "operation": "new_component"}));
  let b = r["result_bodies"][0].as_u64().unwrap();
  assert!(comp_of(&s, b).is_some_and(|n| n.starts_with("Component")));
  assert_eq!(s.design.components.len(), 2);
}

#[test]
fn new_component_goes_under_the_active_component() {
  let mut s = Session::default();
  let top = run(&mut s, "component.create", json!({"name": "Sign", "activate": true}))["component"].as_u64().unwrap();
  plate(&mut s, 0.0);
  let r = run(&mut s, "extrude", json!({"distance": 2, "operation": "new_component", "component_name": "Layer"}));
  let body = r["result_bodies"][0].as_u64().unwrap();
  let c = s.design.component_of(fox_model::BodyId(body as u32)).unwrap();
  assert_eq!(c.name, "Layer");
  assert_eq!(c.parent.map(|p| p.0 as u64), Some(top));
  // the active component didn't take the body as well
  assert!(s.design.component(fox_model::ComponentId(top as u32)).unwrap().bodies.is_empty());
}

#[test]
fn switching_an_extrude_to_new_component_makes_the_component() {
  let mut s = Session::default();
  plate(&mut s, 0.0);
  let r = run(&mut s, "extrude", json!({"distance": 2}));
  let body = r["result_bodies"][0].as_u64().unwrap();
  assert!(comp_of(&s, body).is_none());
  let fid = r["feature"].as_u64().unwrap();
  run(&mut s, "feature.edit", json!({"feature": fid, "set": {"operation": "new_component"}}));
  assert!(comp_of(&s, body).is_some());
  assert!(s.execute("extrude", &json!({"distance": 2, "operation": "bogus"})).is_err());
}

#[test]
fn next_number_fills_gaps_among_top_level_components() {
  let mut s = Session::default();
  assert_eq!(run(&mut s, "component.next_number", json!({}))["prefix"], "01");
  run(&mut s, "component.create", json!({"name": "01"}));
  let letters = run(&mut s, "component.create", json!({"name": "03 - Letters"}))["component"].clone();
  // a sub-component's number doesn't count
  run(&mut s, "component.create", json!({"name": "02 - Inner", "parent": letters}));
  run(&mut s, "component.create", json!({"name": "Bracket"}));
  let r = run(&mut s, "component.next_number", json!({}));
  assert_eq!((r["number"].as_u64(), r["prefix"].as_str()), (Some(2), Some("02")));
  run(&mut s, "component.create", json!({"name": "02 - Face"}));
  assert_eq!(run(&mut s, "component.next_number", json!({"prefix_digits": 3}))["prefix"], "004");
  assert!(s.execute("component.next_number", &json!({"prefix_digits": 0})).is_err());
}
