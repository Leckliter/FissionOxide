//! Tests for butt corners, setbacks through earlier bends, flat pattern DXF layers and checks,
//! rounded press-brake bends and cuts in lofted flanges' flat patterns.

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
  s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
}

/// A `w` × `h` plate, t = 2.5, r = 2.5, K = 0.44.
fn plate(s: &mut Session, w: f64, h: f64) {
  run(s, "sheet.rule", json!({"name": "Steel", "thickness": 2.5, "bend_radius": 2.5, "k_factor": 0.44}));
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [w, h]}));
  run(s, "sheet.flange", json!({}));
}

#[test]
fn butt_corners_make_an_open_cornered_tray() {
  let mut s = Session::default();
  plate(&mut s, 100.0, 60.0);
  run(&mut s, "sheet.rule", json!({"name": "Steel", "gap": 2.5}));
  let edges =
    json!([{"point": [50, 60, 2.5]}, {"point": [100, 30, 2.5]}, {"point": [50, 0, 2.5]}, {"point": [0, 30, 2.5]}]);
  run(&mut s, "sheet.edge_flange", json!({"edges": edges, "height": 25, "position": "inside", "corners": "butt"}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  // base 90 × 50; bends R 2.5..5 along it; walls z 5..25 running 0.7322 past their bends
  let (t, r) = (2.5, 2.5);
  let ext = (r + t) - (t + 2.5 / 2f64.sqrt());
  let bend = std::f64::consts::PI / 4.0 * ((r + t) * (r + t) - r * r);
  let expect = 90.0 * 50.0 * t
    + bend * (2.0 * 90.0 + 2.0 * 50.0)
    + 20.0 * t * (2.0 * (90.0 + 2.0 * ext) + 2.0 * (50.0 + 2.0 * ext));
  assert!((volume(&s) - expect).abs() < 1e-3 * expect, "{} {expect}", volume(&s));
  // the corners are open: nothing at the corner below the walls
  let b = &s.model.bodies[0];
  assert!(b.shape.is_valid());
  run(&mut s, "sheet.flat_pattern", json!({}));
}

#[test]
fn a_second_flange_trims_the_first_ones_bend() {
  let mut s = Session::default();
  plate(&mut s, 80.0, 40.0);
  run(&mut s, "sheet.edge_flange", json!({"edges": [{"point": [40, 0, 2.5]}], "height": 30, "position": "inside"}));
  run(&mut s, "sheet.edge_flange", json!({"edges": [{"point": [80, 22.5, 2.5]}], "height": 15, "position": "inside"}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let (t, r) = (2.5, 2.5);
  let bend = std::f64::consts::PI / 4.0 * ((r + t) * (r + t) - r * r);
  // base 75 × 35, bend 1 over x 0..75, flange 1 80 × 25, bend 2 over y 5..40, flange 2 35 × 10
  let expect = 75.0 * 35.0 * t + 75.0 * bend + 80.0 * 25.0 * t + 35.0 * bend + 35.0 * 10.0 * t;
  assert!((volume(&s) - expect).abs() < 1e-4 * expect, "{} {expect}", volume(&s));
}

#[test]
fn flat_pattern_dxf_has_role_layers_notes_and_passes_the_checks() {
  let dir = std::env::temp_dir().join(format!("fox-sheet3-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let mut s = Session::default();
  plate(&mut s, 80.0, 40.0);
  run(&mut s, "sheet.edge_flange", json!({"edges": [{"point": [40, 0, 2.5]}], "height": 30, "position": "inside"}));
  run(&mut s, "sheet.edge_flange", json!({"edges": [{"point": [80, 22.5, 2.5]}], "height": 15, "position": "inside"}));
  let sk = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].clone();
  run(&mut s, "sketch.circle", json!({"center": [25, 20], "radius": 3}));
  run(&mut s, "sketch.circle", json!({"center": [55, 20], "radius": 3}));
  let body = s.model.bodies[0].id.0;
  run(&mut s, "extrude", json!({"sketch": sk, "extent": "through_all", "operation": "cut", "targets": [body]}));
  let path = dir.join("bracket.dxf");
  run(&mut s, "sheet.flat_pattern", json!({"path": path.to_string_lossy()}));
  let text = std::fs::read_to_string(&path).unwrap();
  for layer in ["CUT", "BEND_UP", "ETCH", "NOTES"] {
    assert!(text.contains(&format!("2\n{layer}\n")), "layer {layer} missing");
  }
  assert!(text.contains("UP 90%%d R2.5") && text.contains("SHEET T=2.5 R=2.5 K=0.44"), "{text}");
  // the file checks clean against its own SHEET note
  let r = run(&mut s, "sheet.check_dxf", json!({"path": path.to_string_lossy()}));
  assert_eq!(r["ok"], true, "{r}");
  assert_eq!((r["holes"].as_u64(), r["bends"].as_u64(), r["contours"].as_u64()), (Some(2), Some(2), Some(3)), "{r}");
  // and the body's flat pattern straight away; a stricter hole-to-bend rule finds the holes
  let r = run(&mut s, "sheet.check_dxf", json!({"hole_to_bend": 16}));
  assert_eq!(r["ok"], false, "{r}");
  let kinds: Vec<&str> = r["issues"].as_array().unwrap().iter().filter_map(|i| i["kind"].as_str()).collect();
  assert!(kinds.iter().all(|k| *k == "hole_near_bend") && !kinds.is_empty(), "{r}");
  std::fs::remove_dir_all(&dir).ok();
}

/// Two circles of radius 20, 30 apart, and a press-brake lofted flange between them (12 facets).
fn press_brake_cylinder(s: &mut Session, rounded: bool) -> u64 {
  run(s, "sheet.rule", json!({"name": "Steel", "thickness": 2, "bend_radius": 3, "k_factor": 0.5}));
  let a = run(s, "sketch.create", json!({"plane": "xy"}))["sketch"].clone();
  run(s, "sketch.circle", json!({"center": [0, 0], "radius": 20}));
  let pl = run(s, "plane.offset", json!({"plane": "xy", "offset": 30}))["feature"].clone();
  let b = run(s, "sketch.create", json!({"plane": {"feature": pl}}))["sketch"].clone();
  run(s, "sketch.circle", json!({"center": [0, 0], "radius": 20}));
  let p = json!({"from": a, "to": b, "method": "press_brake", "bends": 11, "rounded": rounded});
  run(s, "sheet.lofted_flange", p);
  s.model.bodies.last().unwrap().id.0 as u64
}

#[test]
fn rounded_press_brake_bends() {
  let mut sharp = Session::default();
  press_brake_cylinder(&mut sharp, false);
  let mut s = Session::default();
  let body = press_brake_cylinder(&mut s, true);
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let b = s.model.bodies.last().unwrap();
  assert!(b.shape.is_valid());
  let cylinders = b.faces().iter().filter(|f| f.kind == fox_kernel::SurfaceKind::Cylinder).count();
  assert_eq!(cylinders, 22, "an inside and an outside face for each of the 11 bends");
  // rounding trims the sharp outsides of the folds
  assert!(volume(&s) < volume(&sharp) && volume(&s) > 0.99 * volume(&sharp), "{} {}", volume(&s), volume(&sharp));
  let flat = run(&mut s, "sheet.flat_pattern", json!({"body": body}));
  assert_eq!(flat["bends"].as_array().unwrap().len(), 11, "{flat}");
}

#[test]
fn cuts_in_a_lofted_flange_reach_its_flat_pattern() {
  // a 40 square to a 20 square, 30 up: four flat trapezoids
  let mut s = Session::default();
  run(&mut s, "sheet.rule", json!({"name": "Steel", "thickness": 2, "bend_radius": 3, "k_factor": 0.5}));
  let a = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].clone();
  run(&mut s, "sketch.rect", json!({"corner1": [-20, -20], "corner2": [20, 20]}));
  let pl = run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 30}))["feature"].clone();
  let b = run(&mut s, "sketch.create", json!({"plane": {"feature": pl}}))["sketch"].clone();
  run(&mut s, "sketch.rect", json!({"corner1": [-10, -10], "corner2": [10, 10]}));
  run(&mut s, "sheet.lofted_flange", json!({"from": a, "to": b, "method": "press_brake", "bends": 3}));
  let body = s.model.bodies.last().unwrap().id.0;
  let folded0 = volume(&s);
  let flat0 = run(&mut s, "sheet.flat_pattern", json!({"body": body}))["volume_mm3"].as_f64().unwrap();
  // a d6 hole through the y = ±15 (at z 15) faces, off the seam in the middle of a side
  let sk = run(&mut s, "sketch.create", json!({"plane": "xz"}))["sketch"].clone();
  run(&mut s, "sketch.circle", json!({"center": [8, -15], "radius": 3}));
  run(
    &mut s,
    "extrude",
    json!({"sketch": sk, "extent": "symmetric", "distance": 60, "operation": "cut", "targets": [body]}),
  );
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let removed = folded0 - volume(&s);
  assert!(removed > 100.0, "{removed}");
  let flat = run(&mut s, "sheet.flat_pattern", json!({"body": body}))["volume_mm3"].as_f64().unwrap();
  assert!((flat0 - flat - removed).abs() < 0.03 * removed, "{flat0} {flat} {removed}");
  // the flat has the two holes, closed and drawn once
  let r = run(&mut s, "sheet.check_dxf", json!({"body": body}));
  assert_eq!(r["holes"], 2, "{r}");
  let kinds: Vec<&str> = r["issues"].as_array().unwrap().iter().filter_map(|i| i["kind"].as_str()).collect();
  assert!(kinds.iter().all(|k| *k == "hole_near_bend" || *k == "thin_web"), "{r}");
}

/// Runs a reference part's command script (`tests/reference/<case>/part.jsonl`).
fn reference_part(case: &str) -> Session {
  let path =
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/reference").join(case).join("part.jsonl");
  let text = std::fs::read_to_string(&path).unwrap();
  let mut s = Session::default();
  for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
    let mut p: Value = serde_json::from_str(line).unwrap();
    let cmd = p["cmd"].as_str().unwrap().to_string();
    p.as_object_mut().unwrap().remove("cmd");
    s.execute(&cmd, &p).unwrap_or_else(|e| panic!("{case}: {cmd}: {e}"));
  }
  s
}

#[test]
fn sheet_metal_reference_flat_patterns_pass_the_checks() {
  for case in [
    "67-sm-base-flange",
    "68-sm-edge-flange",
    "69-sm-tray",
    "70-sm-flange-45",
    "71-sm-custom-rule",
    "72-sm-contour-flange",
    "73-sm-hem",
    "75-sm-bracket-holes",
    "76-sm-thickness-variant",
  ] {
    let mut s = reference_part(case);
    let r = run(&mut s, "sheet.check_dxf", json!({}));
    assert_eq!(r["ok"], true, "{case}: {r}");
    assert!(r["contours"].as_u64().unwrap() >= 1, "{case}: {r}");
  }
  // 74's slot, cut while flat, runs across a bend: the check says so
  let mut s = reference_part("74-sm-unfold-refold");
  let r = run(&mut s, "sheet.check_dxf", json!({}));
  let kinds: Vec<&str> = r["issues"].as_array().unwrap().iter().filter_map(|i| i["kind"].as_str()).collect();
  assert!(!kinds.is_empty() && kinds.iter().all(|k| *k == "hole_near_bend"), "{r}");
}
