//! Tests for the 2D cutting commands (laser, plasma, waterjet).

use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn hub(name: &str) -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("fox-cut-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

/// A 100 × 50 sheet, 2 thick, with a 20 mm hole at (30, 25) and a bend up along the far edge.
fn sheet(s: &mut Session) {
  run(s, "sheet.rule", json!({"name": "Steel", "thickness": 2, "bend_radius": 3, "k_factor": 0.5}));
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [100, 50]}));
  run(s, "sheet.flange", json!({}));
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.circle", json!({"center": [30, 25], "radius": 10}));
  run(s, "extrude", json!({"distance": 10, "extent": "symmetric", "operation": "cut"}));
  run(s, "sheet.edge_flange", json!({"edges": [{"point": [50, 50, 2]}], "height": 20}));
}

#[test]
fn flat_pattern_to_gcode() {
  let mut s = Session::default();
  sheet(&mut s);
  let c = run(&mut s, "cut.contours", json!({}));
  let cs = c["contours"].as_array().unwrap();
  assert_eq!(cs.len(), 2, "{c}");
  assert_eq!(cs.iter().filter(|c| c["hole"] == true).count(), 1);
  // lower-left corner at the origin
  let min = cs.iter().find(|c| c["hole"] == false).unwrap()["min"].clone();
  assert!(min[0].as_f64().unwrap().abs() < 1e-6 && min[1].as_f64().unwrap().abs() < 1e-6, "{min}");

  let g = run(&mut s, "cut.gcode", json!({"machine": "grbl_laser", "kerf": 0.2, "power": 50}));
  assert_eq!(g["contours"], 2);
  assert_eq!(g["holes"], 1);
  assert_eq!(g["pierces"], 2);
  let text = g["gcode"].as_str().unwrap();
  assert!(text.contains("M4 S500"), "{text}");
  // the hole is round: arcs of radius 10 - 0.1
  assert!(text.lines().any(|l| l.starts_with("G3")));
  // hole first: the first pierce is inside the hole
  let first = text.lines().find(|l| l.starts_with("G0 X")).unwrap();
  let xy: Vec<f64> = first.split_whitespace().skip(1).map(|w| w[1..].parse().unwrap()).collect();
  let flat_len = 50.0 + std::f64::consts::PI / 2.0 * 4.0 + 15.0;
  let centres = [[30.0, 25.0], [30.0, flat_len - 25.0], [70.0, 25.0], [70.0, flat_len - 25.0]];
  let near = centres.iter().any(|c| (xy[0] - c[0]).hypot(xy[1] - c[1]) < 10.0);
  assert!(near, "first pierce {xy:?} inside the hole");

  // to a file, plasma with micro-joints, inches
  let dir = hub("post");
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("plate.ngc");
  let r = run(
    &mut s,
    "cut.gcode",
    json!({"machine": "qtplasmac", "process": "6 mm mild steel, 45 A", "tabs": 2, "units": "inch", "path": path}),
  );
  assert_eq!(r["pierces"], 4, "{r}");
  let text = std::fs::read_to_string(&path).unwrap();
  assert!(text.contains("G20") && text.contains("M3 $0 S1"));
  // R12 DXF of the part geometry
  let d = run(&mut s, "cut.gcode", json!({"machine": "dxf"}));
  assert!(d["gcode"].as_str().unwrap().contains("AC1009"));
  assert!(s.execute("cut.gcode", &json!({"machine": "nope"})).is_err());
  assert!(s.execute("cut.gcode", &json!({"process": "unobtainium"})).is_err());
}

#[test]
fn sketch_and_faces() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [60, 40]}));
  run(&mut s, "sketch.circle", json!({"center": [20, 20], "radius": 5}));
  let id = s.edit_sketch.unwrap().0;
  let r = run(&mut s, "cut.gcode", json!({"sketch": id, "machine": "wazer", "kerf": 1}));
  assert_eq!(r["contours"], 2, "{r}");
  assert_eq!(r["holes"], 1);
  assert!(r["time_s"].as_f64().unwrap() > 0.0);
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "extrude", json!({"sketch": id, "profiles": [[50, 30]], "distance": 5}));
  let r = run(&mut s, "cut.contours", json!({"faces": [{"point": [50, 30, 5]}]}));
  assert_eq!(r["contours"].as_array().unwrap().len(), 2, "{r}");
}

#[test]
fn machines_in_the_hub() {
  let mut s = Session::default();
  s.store_root = Some(hub("machines"));
  let builtin = run(&mut s, "cut.machines", json!({})).as_array().unwrap().len();
  assert!(builtin >= 6);
  run(
    &mut s,
    "cut.machine.add",
    json!({"machine": {"id": "shop_laser", "name": "Shop laser", "header": ["G21 G90"], "pierce": ["M3 S{power}"],
      "off": ["M5"], "footer": ["M2"], "processes": [{"name": "birch 4 mm", "kerf": 0.18, "feed": 450}]}}),
  );
  assert_eq!(run(&mut s, "cut.machines", json!({})).as_array().unwrap().len(), builtin + 1);
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
  let id = s.edit_sketch.unwrap().0;
  let g = run(&mut s, "cut.gcode", json!({"sketch": id, "machine": "shop_laser"}));
  let text = g["gcode"].as_str().unwrap();
  assert!(text.starts_with("G21 G90\n") && text.contains("F450"), "{text}");
  run(&mut s, "cut.machine.delete", json!({"id": "shop_laser"}));
  assert_eq!(run(&mut s, "cut.machines", json!({})).as_array().unwrap().len(), builtin);
}
