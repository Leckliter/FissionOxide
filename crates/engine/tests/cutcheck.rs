//! `sheet.check` on a real bent part: the flat pattern is checked the way a shop would before
//! cutting it.

use fox_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, c: &str, p: Value) -> Value {
  s.execute(c, &p).unwrap_or_else(|e| panic!("{c} {p}: {}", e.0))
}

/// An 80 x 40 bracket in 2.5 mm steel (bend radius 2.5) with a 30 mm flange along y = 0, and holes.
fn bracket(holes: &[([f64; 2], f64)]) -> Session {
  let mut s = Session::default();
  run(&mut s, "sheet.rule", json!({"name": "Steel", "thickness": 2.5, "bend_radius": 2.5, "k_factor": 0.44}));
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 40]}));
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "sheet.flange", json!({"sketch": 1, "rule": "Steel"}));
  run(&mut s, "sheet.edge_flange", json!({"edges": [{"point": [40.0, 0.0, 2.5]}], "height": 30, "angle": "90 deg"}));
  if !holes.is_empty() {
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    for (c, r) in holes {
      run(&mut s, "sketch.circle", json!({"center": c, "radius": r}));
    }
    run(&mut s, "sketch.finish", json!({}));
    let sk = s.design.features.last().unwrap().id.0;
    let body = s.model.bodies[0].id.0;
    run(&mut s, "extrude", json!({"sketch": sk, "extent": "through_all", "operation": "cut", "targets": [body]}));
  }
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  s
}

fn kinds(r: &Value) -> Vec<String> {
  let mut k: Vec<String> =
    r["findings"].as_array().unwrap().iter().map(|f| f["kind"].as_str().unwrap().to_string()).collect();
  k.sort();
  k
}

#[test]
fn a_bracket_with_good_holes_passes() {
  let mut s = bracket(&[([25.0, 25.0], 3.0), ([55.0, 25.0], 3.0)]);
  let r = run(&mut s, "sheet.check", json!({}));
  assert_eq!(r["ok"], true, "{r}");
  // the rules follow the material: holes and webs at least 2.5, holes 2.5 * 2.5 + 2.5 from bends
  assert_eq!(r["rules"]["min_hole"], 2.5);
  assert_eq!(r["rules"]["bend_clearance"], 8.75);
}

#[test]
fn small_holes_and_holes_by_a_bend_are_found() {
  // a 2 mm hole in 2.5 mm steel, and a hole about 5.3 mm from the bend line (the bend sits just
  // outside the base's edge; it needs 8.75)
  let mut s = bracket(&[([40.0, 25.0], 1.0), ([60.0, 5.5], 3.0)]);
  let r = run(&mut s, "sheet.check", json!({}));
  assert_eq!(r["ok"], false);
  assert_eq!(kinds(&r), ["near_bend", "small_hole"], "{r}");
  let small = r["findings"].as_array().unwrap().iter().find(|f| f["kind"] == "small_hole").unwrap();
  assert!((small["value"].as_f64().unwrap() - 2.0).abs() < 0.01, "{small}");
  // where, as in the flat pattern's DXF (lower-left corner at the origin): the base's x is kept
  let at = &small["at"];
  assert!((at[0].as_f64().unwrap() - 40.0).abs() < 0.01, "{small}");
  // a coarser shop's rules, and a sheet the part doesn't fit
  let r = run(&mut s, "sheet.check", json!({"min_hole": 1.5, "bend_clearance": "2 mm", "sheet": [100, 60]}));
  assert_eq!(kinds(&r), ["too_big"], "the flat is 80 x 70.7: {r}");
}
