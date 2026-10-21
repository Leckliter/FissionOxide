//! Tests for the cut-service checks, the cut estimate and collecting flat patterns on one sheet.

use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn hub(name: &str) -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("fox-cutsvc-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

fn kinds(r: &Value) -> Vec<String> {
  r["issues"].as_array().unwrap().iter().map(|i| i["kind"].as_str().unwrap().to_string()).collect()
}

#[test]
fn service_profiles_built_in_and_in_the_hub() {
  let mut s = Session { store_root: Some(hub("profiles")), ..Default::default() };
  let l = run(&mut s, "cut.service.list", json!({}));
  let ids: Vec<&str> = l.as_array().unwrap().iter().map(|v| v["id"].as_str().unwrap()).collect();
  assert_eq!(ids, vec!["sendcutsend", "oshcut", "ponoko"]);
  assert!(l.as_array().unwrap().iter().all(|v| v["builtin"] == true && v["approximate"] == true));
  let shop = json!({"service": {"id": "myshop", "name": "My Shop", "processes": [
    {"process": "router", "max_part": [600, 400], "min_feature": 6.35}
  ]}});
  run(&mut s, "cut.service.add", shop);
  let l = run(&mut s, "cut.service.list", json!({}));
  assert_eq!(l.as_array().unwrap().len(), 4);
  assert_eq!(l[3]["builtin"], false);
  // a 100 × 50 plate with a Ø4 hole: under My Shop's 1/4" router bit
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [100, 50]}));
  run(&mut s, "sketch.circle", json!({"center": [30, 25], "radius": 2}));
  let r = run(&mut s, "cut.service.check", json!({"service": "myshop"}));
  assert_eq!(r["status"], "warnings", "{r}");
  assert_eq!(kinds(&r), vec!["small_feature"]);
  assert_eq!(r["thickness_assumed"], true);
  assert!(s.execute("cut.service.check", &json!({"service": "nope"})).is_err());
  run(&mut s, "cut.service.delete", json!({"id": "myshop"}));
  assert!(s.execute("cut.service.delete", &json!({"id": "sendcutsend"})).is_err());
  assert_eq!(run(&mut s, "cut.service.list", json!({})).as_array().unwrap().len(), 3);
}

#[test]
fn check_a_sketch_for_sendcutsend_and_estimate_it() {
  let dir = hub("sketch");
  let mut s = Session { store_root: Some(dir.clone()), ..Default::default() };
  let sk = run(&mut s, "sketch.create", json!({"plane": "xy"}))["feature"].as_u64().unwrap();
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [100, 50]}));
  run(&mut s, "sketch.circle", json!({"center": [20, 25], "radius": 3}));
  run(&mut s, "sketch.circle", json!({"center": [80, 25], "radius": 3}));
  let r = run(&mut s, "cut.service.check", json!({"sketch": sk, "thickness": "0.125 in", "material": "Mild steel"}));
  assert_eq!(r["status"], "pass", "{r}");
  assert_eq!(r["process"], "Laser");
  assert!(r["summary"].as_str().unwrap().starts_with("PASS"));
  assert!((r["min_feature"].as_f64().unwrap() - 0.0625 * 25.4).abs() < 1e-9);
  let e = run(&mut s, "cut.estimate", json!({"sketch": sk}));
  assert_eq!((e["pieces"].as_u64(), e["holes"].as_u64(), e["pierces"].as_u64()), (Some(1), Some(2), Some(3)), "{e}");
  let area = 5000.0 - 2.0 * std::f64::consts::PI * 9.0;
  assert!((e["area_mm2"].as_f64().unwrap() - area).abs() < 0.1, "{e}");
  assert!((e["area_in2"].as_f64().unwrap() - area / 645.16).abs() < 1e-3);
  let cut = 300.0 + 2.0 * std::f64::consts::TAU * 3.0;
  assert!((e["cut_length_mm"].as_f64().unwrap() - cut).abs() < 0.05, "{e}");
  assert_eq!(e["parts"][0]["size_mm"], json!([100.0, 50.0]));
  assert_eq!(e["parts"][0]["world_center"], json!([50.0, 25.0, 0.0]));
  // the estimate as CSV
  std::fs::create_dir_all(&dir).unwrap();
  let csv = dir.join("estimate.csv");
  run(&mut s, "cut.estimate", json!({"sketch": sk, "export": csv.to_string_lossy()}));
  let text = std::fs::read_to_string(&csv).unwrap();
  assert!(text.starts_with("part,") && text.contains("\ntotal,"), "{text}");
  // a second profile overlapping the plate: an error, located in the model
  run(&mut s, "sketch.rect", json!({"corner1": [90, 40], "corner2": [130, 70]}));
  let r = run(&mut s, "cut.service.check", json!({"sketch": sk, "thickness": "0.125 in"}));
  assert_eq!(r["status"], "errors", "{r}");
  let overlap = r["issues"].as_array().unwrap().iter().find(|i| i["kind"] == "overlap").expect("overlap");
  assert!(overlap["world"].as_array().is_some_and(|w| w.len() == 3), "{overlap}");
  // the JSON report
  let report = dir.join("report.json");
  run(&mut s, "cut.service.check", json!({"sketch": sk, "export": report.to_string_lossy()}));
  let v: Value = serde_json::from_str(&std::fs::read_to_string(&report).unwrap()).unwrap();
  assert_eq!(v["status"], "errors");
}

/// A `w` × `h` flat sheet at `at` with a Ø10 hole in the middle.
fn plate(s: &mut Session, at: [f64; 2], w: f64, h: f64) -> u64 {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": at, "corner2": [at[0] + w, at[1] + h]}));
  run(s, "sketch.circle", json!({"center": [at[0] + w / 2.0, at[1] + h / 2.0], "radius": 5}));
  let r = run(s, "sheet.flange", json!({}));
  r["result_bodies"][0].as_u64().unwrap()
}

#[test]
fn flat_patterns_collected_on_one_sheet() {
  let dir = hub("collect");
  std::fs::create_dir_all(&dir).unwrap();
  let mut s = Session { store_root: Some(dir.clone()), ..Default::default() };
  run(&mut s, "sheet.rule", json!({"name": "Alu", "thickness": 2, "bend_radius": 2, "k_factor": 0.44}));
  let a = plate(&mut s, [0.0, 0.0], 100.0, 50.0);
  let b = plate(&mut s, [0.0, 200.0], 60.0, 40.0);
  let r = run(&mut s, "cut.service.check", json!({"body": a}));
  assert_ne!(r["status"], "errors", "{r}");
  assert_eq!(r["thickness"], 2.0);
  assert_eq!(r["estimate"]["holes"], 1);
  // 100 + 10 + 60 = 170: wraps under 170, not over it
  let r = run(
    &mut s,
    "sheet.collect_flat",
    json!({"max_width": 165, "gap": 10, "path": dir.join("x.dxf").to_string_lossy()}),
  );
  assert_eq!(r["rows"], 2, "{r}");
  assert_eq!(r["parts"][1]["min"][0], 0.0);
  let r = run(&mut s, "sheet.collect_flat", json!({"max_width": 175, "gap": 10}));
  assert_eq!(r["rows"], 1, "{r}");
  assert_eq!(r["name"], "Cut File");
  assert!((r["parts"][1]["min"][0].as_f64().unwrap() - 110.0).abs() < 1e-6, "{r}");
  let sk = r["sketch"].as_u64().unwrap();
  let e = run(&mut s, "cut.estimate", json!({"sketch": sk}));
  assert_eq!((e["pieces"].as_u64(), e["holes"].as_u64()), (Some(2), Some(2)), "{e}");
  let area = 100.0 * 50.0 + 60.0 * 40.0 - 2.0 * std::f64::consts::PI * 25.0;
  assert!((e["area_mm2"].as_f64().unwrap() - area).abs() < 0.5, "{e}");
  // two parts in one file: SendCutSend wants one per file, OSH Cut doesn't mind
  let r = run(&mut s, "cut.service.check", json!({"sketch": sk, "thickness": 2}));
  assert!(kinds(&r).contains(&"several_parts".to_string()) && r["status"] == "warnings", "{r}");
  let r = run(&mut s, "cut.service.check", json!({"sketch": sk, "thickness": 2, "service": "oshcut"}));
  assert_eq!(r["status"], "pass", "{r}");
  // a second collection gets its own name; only the bodies asked for
  let r = run(&mut s, "sheet.collect_flat", json!({"bodies": [b]}));
  assert_eq!(r["name"], "Cut File 02");
  assert_eq!(r["parts"].as_array().unwrap().len(), 1);
  // straight to one DXF, and measured from it
  let path = dir.join("cut.dxf");
  run(&mut s, "sheet.collect_flat", json!({"path": path.to_string_lossy()}));
  let e = run(&mut s, "cut.estimate", json!({"dxf": path.to_string_lossy()}));
  assert_eq!(e["pieces"], 2, "{e}");
}
