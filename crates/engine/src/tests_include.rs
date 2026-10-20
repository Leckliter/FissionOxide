//! Tests for Include 3D Geometry (`sketch.include`): body edges and sketch curves kept at their
//! real positions, as sweep and pipe paths, following their source.

use std::f64::consts::PI;

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

fn volume(s: &Session) -> f64 {
  s.model.bodies.iter().map(|b| b.shape.mass().map(|m| m.volume).unwrap_or(0.0)).sum()
}

fn close(a: f64, b: f64) -> bool {
  (a - b).abs() <= 1e-6 * b.abs()
}

/// A 20 × 20 box `h` high on XY; returns the extrusion's id.
fn block(s: &mut Session) -> u64 {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [20, 20]}));
  run(s, "sketch.finish", json!({}));
  run(s, "extrude", json!({"distance": 10}))["feature"].as_u64().unwrap()
}

#[test]
fn included_vertical_edge_is_a_sweep_path_that_follows_the_body() {
  let mut s = Session::default();
  let ext = block(&mut s);
  let path = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].clone();
  let r = run(&mut s, "sketch.include", json!({"edges": [{"point": [20, 20, 5]}]}));
  assert_eq!(r["curves_3d"], 1, "{r}");
  assert_eq!(r["profiles"], 0, "{r}");
  run(&mut s, "sketch.finish", json!({}));
  let prof = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].clone();
  run(&mut s, "sketch.circle", json!({"center": [20, 20], "radius": 3}));
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "sweep", json!({"sketch": prof, "path": path, "operation": "join"}));
  assert!(close(volume(&s), 4000.0 + 0.75 * PI * 9.0 * 10.0), "{}", volume(&s));
  // the included edge follows the box: a taller box sweeps further
  run(&mut s, "feature.edit", json!({"feature": ext, "set": {"extent": {"kind": "one_side", "distance": "16"}}}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!(close(volume(&s), 6400.0 + 0.75 * PI * 9.0 * 16.0), "{}", volume(&s));
  // a pipe runs along it too
  run(&mut s, "pipe", json!({"path": path, "diameter": 2, "operation": "new_body"}));
  assert!(close(volume(&s), 6400.0 + 0.75 * PI * 9.0 * 16.0 + PI * 16.0), "{}", volume(&s));
}

#[test]
fn included_edges_in_the_sketch_plane_form_profiles() {
  let mut s = Session::default();
  block(&mut s);
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let edges: Vec<Value> =
    [[10, 0, 0], [20, 10, 0], [10, 20, 0], [0, 10, 0]].iter().map(|p| json!({"point": p})).collect();
  let r = run(&mut s, "sketch.include", json!({"edges": edges}));
  assert_eq!(r["curves_3d"], 0, "{r}");
  assert_eq!(r["profiles"], 1, "{r}");
  let r = run(&mut s, "extrude", json!({"distance": 5, "flip": true, "operation": "new_body"}));
  assert!(r["error"].is_null(), "{r}");
}

#[test]
fn included_sketch_curve_follows_its_sketch_and_deletes() {
  let mut s = Session::default();
  let src = run(&mut s, "sketch.create", json!({"plane": "xz"}))["sketch"].clone();
  let line = run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 30]]}))["curves"][0].clone();
  let dim = run(&mut s, "sketch.dimension", json!({"type": "distance", "curves": [line], "value": "30"}));
  let dim = dim["constraint"].clone();
  run(&mut s, "sketch.finish", json!({}));
  let sk = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].clone();
  let inc = run(&mut s, "sketch.include", json!({"curves": [{"sketch": src, "curve": line}]}));
  let c = inc["curves"][0].clone();
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 5}));
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "sweep", json!({"sketch": sk, "path": sk, "path_curves": [c]}));
  assert!(close(volume(&s), PI * 25.0 * 30.0), "{}", volume(&s));
  // the included line follows its sketch: a longer line sweeps further
  run(&mut s, "sketch.edit", json!({"sketch": src}));
  run(&mut s, "sketch.set_dimension", json!({"constraint": dim, "value": "40"}));
  run(&mut s, "sketch.finish", json!({}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!(close(volume(&s), PI * 25.0 * 40.0), "{}", volume(&s));
  // a sketch can't include a later sketch's curves
  assert!(s.execute("sketch.include", &json!({"sketch": src, "curves": [{"sketch": sk, "curve": c}]})).is_err());
  run(&mut s, "sketch.edit", json!({"sketch": sk}));
  run(&mut s, "sketch.delete", json!({"curves": [c]}));
  assert!(
    s.model.sketches.values().all(|r| r.sketch.curve3d(fox_sketch::CurveId(c.as_u64().unwrap() as u32)).is_none())
  );
}
