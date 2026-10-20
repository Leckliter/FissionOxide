//! Tests for intersection curves (3D curves where pushed sketch curves cross faces or other
//! pushed curves) and pipes along them.

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

/// Ramanujan's perimeter of an ellipse.
fn perimeter(a: f64, b: f64) -> f64 {
  let h = ((a - b) / (a + b)).powi(2);
  std::f64::consts::PI * (a + b) * (1.0 + 3.0 * h / (10.0 + (4.0 - 3.0 * h).sqrt()))
}

fn pipe_volume(r: f64) -> f64 {
  // the slanted line rises 1:1, so the ellipse's semi-axes are r√2 and r; the tube is d2
  std::f64::consts::PI * perimeter(r * 2f64.sqrt(), r)
}

#[test]
fn intersection_curve_with_a_face_follows_the_body() {
  let mut s = Session::default();
  run(&mut s, "param.add", json!({"name": "r", "expr": "10 mm"}));
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": "r"}));
  run(&mut s, "extrude", json!({"distance": 30}));
  let line =
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 0], "normal": [0, -1, 0], "x": [1, 0, 0]}}));
  let sketch = line["sketch"].as_u64().unwrap();
  run(&mut s, "sketch.line", json!({"points": [[-10, 5], [10, 25]]}));
  run(&mut s, "sketch.finish", json!({}));
  let ic = run(&mut s, "sketch.intersection_curve", json!({"sketch": sketch, "faces": [{"point": [0, -10, 15]}]}));
  assert!(!s.model.curves3d.is_empty(), "the curve is drawn");
  run(&mut s, "pipe", json!({"path": ic["feature"], "diameter": 2, "operation": "new_body"}));
  assert_eq!(s.model.bodies.len(), 2);
  let rod = |r: f64| std::f64::consts::PI * r * r * 30.0;
  let want = rod(10.0) + pipe_volume(10.0);
  assert!((volume(&s) - want).abs() < 1e-3 * want, "{} vs {want}", volume(&s));
  // a thinner rod: the line now runs past its sides, still one whole ellipse, smaller
  run(&mut s, "param.set", json!({"name": "r", "expr": "8 mm"}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let want = rod(8.0) + pipe_volume(8.0);
  assert!((volume(&s) - want).abs() < 1e-3 * want, "{} vs {want}", volume(&s));
}

#[test]
fn intersection_curve_of_two_sketches() {
  let mut s = Session::default();
  let circle = run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
  run(&mut s, "sketch.finish", json!({}));
  let line =
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 0], "normal": [0, -1, 0], "x": [1, 0, 0]}}));
  run(&mut s, "sketch.line", json!({"points": [[-20, -5], [20, 35]]}));
  run(&mut s, "sketch.finish", json!({}));
  let ic = run(&mut s, "sketch.intersection_curve", json!({"sketch": line["sketch"], "with": [circle["sketch"]]}));
  run(&mut s, "pipe", json!({"path": ic["feature"], "diameter": 2}));
  let want = pipe_volume(10.0);
  assert!((volume(&s) - want).abs() < 1e-3 * want, "{} vs {want}", volume(&s));
}

#[test]
fn intersection_curve_needs_something_to_cross() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]]}));
  run(&mut s, "sketch.finish", json!({}));
  assert!(s.execute("sketch.intersection_curve", &json!({"sketch": 1})).is_err());
}

/// The volume of a pipe (`p`: its options) along where a 40 x 20 rectangle on xy (x 10..50,
/// y -10..10), pushed up and down, crosses the side of an r20 rod along x (0..60): one closed loop
/// on top and one below, each two straight runs of 40 at y = ±10 and two arcs of r20 spanning
/// 2·asin(10/20) = 60°, meeting square at 4 corners.
fn rectangle_pipe(p: Value) -> f64 {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "yz"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 20}));
  run(&mut s, "extrude", json!({"distance": 60}));
  let rect = run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [10, -10], "corner2": [50, 10]}));
  run(&mut s, "sketch.finish", json!({}));
  let ic =
    run(&mut s, "sketch.intersection_curve", json!({"sketch": rect["sketch"], "faces": [{"point": [30, 0, 20]}]}));
  let mut p = p;
  p["path"] = ic["feature"].clone();
  p["operation"] = json!("new_body");
  run(&mut s, "pipe", p);
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  // the sides' pieces chain into one closed path above the rod and one below; each smooth run is
  // swept on its own and every corner closed with a ball, so no flat end is left anywhere
  for b in &s.model.bodies[1..] {
    assert!(b.shape.is_valid(), "the pipe is a valid solid");
  }
  let flat: usize = s.model.bodies[1..]
    .iter()
    .map(|b| b.shape.faces().unwrap().iter().filter(|f| f.kind == fox_kernel::SurfaceKind::Plane).count())
    .sum();
  assert_eq!(flat, 0, "the pipes along closed paths have no end faces or notches");
  volume(&s) - std::f64::consts::PI * 20.0 * 20.0 * 60.0
}

#[test]
fn pipe_along_a_rectangles_intersection_curve_has_round_corners() {
  // by hand, for a round section of radius r: the runs hold πr² a unit of length; at each square
  // corner the two runs' ends overlap by 4r³/3 (two crossed half-cylinders) and the ball adds
  // the quarter lune both miss, πr³/3. Flat-ended tubes unioned at the corners lack the lune.
  let pi = std::f64::consts::PI;
  let path = 2.0 * 40.0 + 2.0 * 20.0 * std::f64::consts::FRAC_PI_3;
  let notched = |r: f64| 2.0 * (pi * r * r * path - 4.0 * 4.0 * r.powi(3) / 3.0);
  let round = |r: f64| notched(r) + 2.0 * 4.0 * pi * r.powi(3) / 3.0;
  let solid = rectangle_pipe(json!({"diameter": 2}));
  assert!((solid - round(1.0)).abs() < 1e-4 * round(1.0), "pipes {solid} vs {}", round(1.0));
  assert!(solid > notched(1.0) + 8.0, "pipes {solid} fill the corners the notched {} lack", notched(1.0));
  // hollow: the bore, corners and all, cut out of the solid pipe
  let hollow = rectangle_pipe(json!({"diameter": 2, "wall": 0.5}));
  let want = round(1.0) - round(0.5);
  assert!((hollow - want).abs() < 1e-4 * want, "hollow pipes {hollow} vs {want}");
}
