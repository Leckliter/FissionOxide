//! Tests for the sheet metal MODIFY commands: Join by Bend, Sheet Metal Fillet and Sheet Metal
//! Chamfer.

use std::f64::consts::PI;

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

fn rule(s: &mut Session) {
  run(s, "sheet.rule", json!({"name": "Steel", "thickness": 2, "bend_radius": 3, "k_factor": 0.5}));
}

/// A 100 × 50 plate, 2 thick (z 0..2), with a 20 high flange up from its y = 50 edge.
fn bracket(s: &mut Session) {
  rule(s);
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [100, 50]}));
  run(s, "sheet.flange", json!({}));
  run(s, "sheet.edge_flange", json!({"edges": [{"point": [50, 50, 2]}], "height": 20}));
}

fn flat_area(s: &mut Session) -> f64 {
  run(s, "sheet.flat_pattern", json!({}))["area_mm2"].as_f64().unwrap()
}

fn volume(s: &Session) -> f64 {
  s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
}

#[test]
fn sheet_metal_fillet_rounds_a_corner_into_the_flat_pattern() {
  let mut s = Session::default();
  bracket(&mut s);
  let before = flat_area(&mut s);
  // the plate's corner at x = 100, y = 0 runs through the thickness
  let r = run(&mut s, "sheet.fillet", json!({"edges": [{"point": [100, 0, 1]}], "radius": 5}));
  assert!(s.model.errors().is_empty(), "{:?} {r}", s.model.errors());
  let f = s.design.features.last().unwrap();
  assert!(matches!(f.kind, FeatureKind::SheetFillet { .. }), "{:?}", f.kind);
  assert!(f.name.starts_with("SheetMetalFillet"), "{}", f.name);
  let corner = 25.0 - PI * 25.0 / 4.0;
  let after = flat_area(&mut s);
  assert!((before - after - corner).abs() < 1e-3, "{before} {after}");
  // and through Unfold: the flat body keeps the round corner
  run(&mut s, "sheet.unfold", json!({}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let b = &s.model.bodies[0];
  assert!(b.sheet.as_ref().is_some_and(|i| i.flat));
  let v = b.shape.mass().unwrap().volume;
  assert!((v - after * 2.0).abs() < 1e-2, "{v} {after}");
  assert!(b.faces().iter().any(|f| f.kind == fox_kernel::SurfaceKind::Cylinder && (f.radius - 5.0).abs() < 1e-6));
}

#[test]
fn sheet_metal_chamfer_equal_and_two_distances() {
  let mut s = Session::default();
  bracket(&mut s);
  let before = flat_area(&mut s);
  run(&mut s, "sheet.chamfer", json!({"edges": [{"point": [100, 0, 1]}], "distance": 4}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let after = flat_area(&mut s);
  assert!((before - after - 8.0).abs() < 1e-3, "{before} {after}");
  run(
    &mut s,
    "sheet.chamfer",
    json!({"edges": [{"point": [0, 0, 1]}], "chamfer_type": "two_distances", "distance": 4, "distance2": 6}),
  );
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let last = flat_area(&mut s);
  assert!((after - last - 12.0).abs() < 1e-3, "{after} {last}");
  let FeatureKind::SheetChamfer { distance2, .. } = &s.design.features.last().unwrap().kind else { panic!() };
  assert_eq!(distance2.as_deref(), Some("6"));
}

#[test]
fn sheet_metal_fillet_takes_only_thickness_edges() {
  let mut s = Session::default();
  bracket(&mut s);
  // an edge along the top of the plate is not a corner through the sheet
  let r = s.execute("sheet.fillet", &json!({"edges": [{"point": [50, 0, 2]}], "radius": 3}));
  let failed = match r {
    Err(_) => true,
    Ok(v) => !v["error"].is_null(),
  };
  assert!(failed);
  // nor does it work on a body that isn't sheet metal
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
  let r = s.execute("sheet.fillet", &json!({"edges": [{"point": [5, 5, 5]}], "radius": 3}));
  assert!(r.is_err() || !r.unwrap()["error"].is_null());
}

/// Two separate flanges: A flat (x -40..`a_end`, y 0..30, z 0..2) and B upright in the YZ plane
/// (y `b_y`, z 10..40). Returns B's x range.
fn two_flanges(s: &mut Session, a_end: f64, b_y: (f64, f64)) -> (f64, f64) {
  rule(s);
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [-40, 0], "corner2": [a_end, 30]}));
  run(s, "sheet.flange", json!({}));
  run(s, "sketch.create", json!({"plane": "yz"}));
  // on YZ the sketch x runs down world Z and y along world Y
  run(s, "sketch.rect", json!({"corner1": [-40, b_y.0], "corner2": [-10, b_y.1]}));
  run(s, "sheet.flange", json!({}));
  assert_eq!(s.model.bodies.len(), 2);
  let m = s.model.bodies[1].shape.mass().unwrap();
  assert!((m.bbox_min.y - b_y.0).abs() < 1e-6 && (m.bbox_max.z - 40.0).abs() < 1e-6, "{m:?}");
  (m.bbox_min.x, m.bbox_max.x)
}

fn edges(a_end: f64, bx: f64) -> Value {
  json!({"edge1": {"point": [a_end, 15, 2]}, "edge2": {"point": [bx, 15, 10]}})
}

#[test]
fn join_by_bend_makes_one_body_that_unfolds() {
  let mut s = Session::default();
  let (x0, x1) = two_flanges(&mut s, -10.0, (0.0, 30.0));
  let before = volume(&s);
  let mut p = edges(-10.0, x0);
  run(&mut s, "sheet.join_by_bend", p.clone());
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert_eq!(s.model.bodies.len(), 1);
  assert!(s.model.bodies[0].shape.is_valid());
  assert!(matches!(s.design.features.last().unwrap().kind, FeatureKind::JoinByBend { .. }));
  // A runs on to the bend (centre r + t/2 short of B's middle), B runs down to it
  let (t, r) = (2.0, 3.0);
  let xm = (x0 + x1) / 2.0;
  let ext_a = xm - (r + t / 2.0) - -10.0;
  let ext_b = 10.0 - (t / 2.0 + r + t / 2.0);
  let bend = PI / 4.0 * ((r + t) * (r + t) - r * r) * 30.0;
  let expect = before + (ext_a + ext_b) * t * 30.0 + bend;
  assert!((volume(&s) - expect).abs() < 1e-3 * expect, "{} {expect}", volume(&s));
  // the flat pattern: both flanges, the extensions and the bend's allowance (K = 0.5)
  let flat = run(&mut s, "sheet.flat_pattern", json!({}));
  assert_eq!(flat["bends"].as_array().unwrap().len(), 1, "{flat}");
  let area = flat["area_mm2"].as_f64().unwrap();
  let allowance = PI / 2.0 * (r + t / 2.0);
  let expect = 30.0 * (30.0 + ext_a + allowance + ext_b + 30.0);
  assert!((area - expect).abs() < 1e-2, "{area} {expect}");
  // picking the same body twice is refused
  p["edge2"] = json!({"point": [-40, 15, 2]});
  let mut s2 = Session::default();
  two_flanges(&mut s2, -10.0, (0.0, 30.0));
  let r2 = s2.execute("sheet.join_by_bend", &p);
  assert!(r2.is_err() || !r2.unwrap()["error"].is_null());
}

#[test]
fn join_by_bend_from_an_edge_keeps_that_flange() {
  let mut s = Session::default();
  let (x0, _) = two_flanges(&mut s, -10.0, (0.0, 30.0));
  let mut p = edges(-10.0, x0);
  p["position"] = json!("first");
  let r = run(&mut s, "sheet.join_by_bend", p);
  assert!(s.model.errors().is_empty(), "{:?} {r}", s.model.errors());
  assert_eq!(s.model.bodies.len(), 1);
  // flange A still ends at x = -10: the bend's cylinder faces start there
  let b = &s.model.bodies[0];
  let cyl: Vec<_> = b.faces().iter().filter(|f| f.kind == fox_kernel::SurfaceKind::Cylinder).collect();
  assert_eq!(cyl.len(), 2, "{cyl:?}");
  assert!(cyl.iter().all(|f| (f.origin.x - -10.0).abs() < 1e-6), "{cyl:?}");
  run(&mut s, "sheet.flat_pattern", json!({}));
}

#[test]
fn join_by_bend_relief_notches_the_wider_flange() {
  // A runs past the bend (it is trimmed back) and is wider than B: reliefs at B's ends
  let mut vols = Vec::new();
  for relief in ["none", "rectangular"] {
    let mut s = Session::default();
    let (x0, _) = two_flanges(&mut s, -2.0, (5.0, 25.0));
    let mut p = edges(-2.0, x0);
    p["relief"] = json!(relief);
    run(&mut s, "sheet.join_by_bend", p);
    assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
    assert_eq!(s.model.bodies.len(), 1);
    vols.push(volume(&s));
  }
  // two notches, the rule's relief width (t) wide, through A from its end back past the bend
  assert!(vols[0] - vols[1] > 2.0 * 2.0 * 2.0 * 1.0, "{vols:?}");
}
