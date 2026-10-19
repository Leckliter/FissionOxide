//! Edits the way people make them in Fusion (change a sketch dimension, roll the timeline back and
//! insert a feature, delete one), checking that later features stay on the faces and edges they
//! were put on. Volumes are worked out by hand.

use fox_engine::Session;
use serde_json::{Value, json};
use std::f64::consts::PI;

fn run(s: &mut Session, c: &str, p: Value) -> Value {
  s.execute(c, &p).unwrap_or_else(|e| panic!("{c} {p}: {}", e.0))
}
fn vol(s: &Session) -> f64 {
  s.model.bodies.iter().filter(|b| b.shape.solid_count() > 0).map(|b| b.shape.mass().unwrap().volume).sum()
}
fn check(what: &str, s: &Session, want: f64) {
  assert!(s.model.errors().is_empty(), "{what}: {:?}", s.model.errors());
  let warnings: Vec<_> = s.model.status.iter().filter_map(|st| st.warning.clone()).collect();
  assert!(warnings.is_empty(), "{what}: {warnings:?}");
  let v = vol(s);
  assert!((v - want).abs() < 1e-6 * want, "{what}: volume {v:.3}, want {want:.3}");
}

#[test]
fn later_features_follow_edits() {
  // a quarter-round fillet of radius r takes (1 - pi/4) r^2 per length of edge
  let k = 1.0 - PI / 4.0;
  // 1: double the sketch width under a fillet
  {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [60, 40]}));
    let info = run(&mut s, "sketch.info", json!({}));
    let bottom = info["curves"].as_array().unwrap().iter().find(|c| c["kind"]["type"] == "line").unwrap()["id"].clone();
    let d = run(&mut s, "sketch.dimension", json!({"type": "distance", "curves": [bottom], "value": "60"}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 20}));
    run(&mut s, "fillet", json!({"edges": [{"point": [30, 0, 20]}], "radius": 3}));
    check("1 before", &s, 60.0 * 40.0 * 20.0 - k * 9.0 * 60.0);
    let cid = d["constraint"].clone();
    run(&mut s, "sketch.edit", json!({"sketch": 1}));
    run(&mut s, "sketch.set_dimension", json!({"constraint": cid, "value": "120"}));
    run(&mut s, "sketch.finish", json!({}));
    check("1 width 60 -> 120", &s, 120.0 * 40.0 * 20.0 - k * 9.0 * 120.0);
  }
  // 2: insert a slot before the fillet, splitting the filleted edge in two
  {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [60, 40]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 20}));
    run(&mut s, "fillet", json!({"edges": [{"point": [30, 0, 20]}], "radius": 3}));
    run(&mut s, "timeline.marker", json!({"at": 2}));
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 20], "normal": [0, 0, 1], "x": [1, 0, 0]}}));
    run(&mut s, "sketch.rect", json!({"corner1": [25, -5], "corner2": [35, 10]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 5, "flip": true, "operation": "cut"}));
    run(&mut s, "timeline.marker", json!({"at": null}));
    check("2 slot inserted before fillet", &s, 60.0 * 40.0 * 20.0 - 10.0 * 10.0 * 5.0 - k * 9.0 * 50.0);
    let slot = s.design.features.iter().find(|f| f.name == "Extrude2").unwrap().id.0;
    let sk2 = s.design.features.iter().find(|f| f.name == "Sketch2").unwrap().id.0;
    run(&mut s, "feature.delete", json!({"feature": slot}));
    run(&mut s, "feature.delete", json!({"feature": sk2}));
    check("4 slot deleted again", &s, 60.0 * 40.0 * 20.0 - k * 9.0 * 60.0);
  }
  // 3: chamfer a hole's top edge, then move the hole by its dimension
  {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [60, 40]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "extrude", json!({"distance": 20}));
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 20], "normal": [0, 0, 1], "x": [1, 0, 0]}}));
    run(&mut s, "sketch.circle", json!({"center": [20, 20], "radius": 5}));
    let info = run(&mut s, "sketch.info", json!({}));
    let circle = info["curves"].as_array().unwrap().iter().find(|c| c["kind"]["type"] == "circle").unwrap().clone();
    let center = circle["kind"]["center"].clone();
    let dim = run(&mut s, "sketch.dimension", json!({"type": "horizontal", "points": [0, center], "value": "20"}));
    run(&mut s, "sketch.dimension", json!({"type": "vertical", "points": [0, center], "value": "20"}));
    run(&mut s, "sketch.finish", json!({}));
    let sk = s.design.features.last().unwrap().id.0;
    run(&mut s, "extrude", json!({"extent": "through_all", "flip": true, "operation": "cut"}));
    run(&mut s, "chamfer", json!({"edges": [{"point": [25, 20, 20]}], "distance": 1}));
    // a 45 degree chamfer ring on a hole of radius r: volume pi*d^2*(r + d/3)
    let ring = PI * 1.0 * (5.0 + 1.0 / 3.0);
    let hole = PI * 25.0 * 20.0;
    check("3 before", &s, 48000.0 - hole - ring);
    run(&mut s, "sketch.edit", json!({"sketch": sk}));
    run(&mut s, "sketch.set_dimension", json!({"constraint": dim["constraint"], "value": "40"}));
    run(&mut s, "sketch.finish", json!({}));
    check("3 hole moved x 20 -> 40", &s, 48000.0 - hole - ring);
  }
}
