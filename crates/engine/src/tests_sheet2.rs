//! Tests for lofted and miter flanges, corner closures and flat pattern sketches.

use fox_model::FeatureKind;
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

/// A 100 × 50 plate, 2 thick; r = 3, K = 0.5.
fn plate(s: &mut Session) {
  run(s, "sheet.rule", json!({"name": "Steel", "thickness": 2, "bend_radius": 3, "k_factor": 0.5}));
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [100, 50]}));
  run(s, "sheet.flange", json!({}));
}

#[test]
fn flat_pattern_sketch_from_commands() {
  let mut s = Session::default();
  plate(&mut s);
  run(&mut s, "sheet.edge_flange", json!({"edges": [{"point": [50, 50, 2]}], "height": 20}));
  let r = run(&mut s, "sheet.flat_sketch", json!({}));
  assert_eq!(r["bends"], 1, "{r}");
  let sk = fox_model::FeatureId(r["feature"].as_u64().unwrap() as u32);
  let res = s.model.sketches.get(&sk).unwrap();
  // the outline is one closed profile: 100 × (50 + bend allowance + 15)
  let len = 50.0 + std::f64::consts::PI / 2.0 * 4.0 + 15.0;
  assert_eq!(res.profiles.len(), 1);
  assert!((res.profiles[0].area() - 100.0 * len).abs() < 1e-3, "{}", res.profiles[0].area());
  let bend = res.sketch.curves.iter().filter(|c| c.construction).count();
  assert_eq!(bend, 1);
  // on the stationary face (a face of the plate), so it can be extruded straight away
  let z = res.plane.origin.z;
  assert!((z.abs() < 1e-9 || (z - 2.0).abs() < 1e-9) && res.plane.normal().z.abs() > 0.999, "{:?}", res.plane);
  // or on XY with the lower-left corner at the origin
  let body = s.model.bodies[0].id.0;
  let r = run(&mut s, "sheet.flat_sketch", json!({"body": [body], "on": "xy"}));
  let sk = fox_model::FeatureId(r["feature"].as_u64().unwrap() as u32);
  let (lo, hi) = s.model.sketches[&sk].sketch.bounds().unwrap();
  assert!(lo[0].abs() < 1e-9 && lo[1].abs() < 1e-9, "{lo:?}");
  let mut size = [hi[0] - lo[0], hi[1] - lo[1]];
  size.sort_by(f64::total_cmp);
  assert!((size[0] - len).abs() < 1e-3 && (size[1] - 100.0).abs() < 1e-3, "{size:?}");
}

#[test]
fn miter_flange_and_corner_closure_from_commands() {
  let mut s = Session::default();
  plate(&mut s);
  run(&mut s, "sketch.create", json!({"plane": "yz"}));
  run(&mut s, "sketch.line", json!({"points": [[-2, 50], [-22, 50], [-22, 60]]}));
  let r = run(
    &mut s,
    "sheet.miter_flange",
    json!({"edges": [{"point": [50, 50, 2]}, {"point": [100, 25, 2]}, {"point": [50, 0, 2]}]}),
  );
  assert_eq!(r["bodies"], 1, "{r}");
  let flat = run(&mut s, "sheet.flat_pattern", json!({}));
  assert_eq!(flat["bends"].as_array().unwrap().len(), 6, "{flat}");
  let r = run(&mut s, "sheet.corner_closure", json!({"corners": "closed", "relief": "round"}));
  assert!(r["error"].is_null(), "{r}");
  let FeatureKind::MiterFlange { corners, .. } = &s.design.features.last().unwrap().kind else { panic!() };
  assert_eq!(*corners, fox_model::CornerMode::Closed);
  assert_eq!(s.design.sheet_rule("").unwrap().corner_relief, fox_model::CornerRelief::Round);
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  run(&mut s, "sheet.flat_pattern", json!({}));
}

#[test]
fn lofted_flange_from_commands() {
  let mut s = Session::default();
  run(&mut s, "sheet.rule", json!({"name": "Steel", "thickness": 1, "bend_radius": 1, "k_factor": 0.5}));
  let a = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].clone();
  run(&mut s, "sketch.rect", json!({"corner1": [-30, -20], "corner2": [30, 20]}));
  let pl = run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 40}))["feature"].clone();
  let b = run(&mut s, "sketch.create", json!({"plane": {"feature": pl}}))["sketch"].clone();
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 15}));
  for method in ["die_formed", "press_brake"] {
    let r = run(&mut s, "sheet.lofted_flange", json!({"from": a, "to": {"sketch": b}, "method": method, "bends": 12}));
    assert!(r["error"].is_null(), "{method}: {r}");
    let body = s.model.bodies.last().unwrap().id.0;
    let flat = run(&mut s, "sheet.flat_pattern", json!({"body": body}));
    // K = 0.5: the flat has about the folded volume (sharp press-brake folds add a little at
    // their outsides)
    let folded = s.model.bodies.last().unwrap().shape.mass().unwrap().volume;
    let v = flat["volume_mm3"].as_f64().unwrap();
    let tol = if method == "die_formed" { 0.02 } else { 0.05 };
    assert!((v - folded).abs() < tol * folded, "{method}: {v} {folded}");
    // the sketch's outline is one region of the flat's area (t = 1)
    let r = run(&mut s, "sheet.flat_sketch", json!({"body": body}));
    let sk = fox_model::FeatureId(r["feature"].as_u64().unwrap() as u32);
    let res = &s.model.sketches[&sk];
    assert_eq!(res.profiles.len(), 1, "{method}");
    assert!((res.profiles[0].area() - v).abs() < 1e-3 * v, "{method}: {} {v}", res.profiles[0].area());
  }
}
