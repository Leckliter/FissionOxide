//! Simulation workspace commands on real (OpenCASCADE) bodies, checked against closed-form
//! results.

use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn close(a: f64, b: f64, rel: f64) -> bool {
  (a - b).abs() <= rel * b.abs()
}

/// A 100 × 10 × 10 steel bar along X, from x = 0 to 100, z = 0 to 10.
fn bar(s: &mut Session) {
  run(s, "box", json!({"width": 100, "depth": 10, "height": 10, "at": [50, 0]}));
}

#[test]
fn cantilever_static() {
  let mut s = Session::default();
  bar(&mut s);
  let st = run(&mut s, "sim.study", json!({"type": "static", "name": "Beam"}));
  assert_eq!(st["study"], 1);
  run(&mut s, "sim.constraint", json!({"type": "fixed", "faces": [{"point": [0, 0, 5]}]}));
  run(&mut s, "sim.load", json!({"type": "force", "faces": [{"point": [100, 0, 5]}], "force": [0, 0, -100]}));
  let mesh = run(&mut s, "sim.mesh", json!({"size": 2.5}));
  assert!(mesh["elements"].as_u64().unwrap() > 1000, "{mesh}");
  let r = run(&mut s, "sim.solve", json!({}));
  let (e, i) = (210_000.0, 10.0f64.powi(4) / 12.0);
  let expect = 100.0 * 100f64.powi(3) / (3.0 * e * i) + 100.0 * 100.0 / (5.0 / 6.0 * e / 2.6 * 100.0);
  let tip = run(&mut s, "sim.probe", json!({"field": "uz", "point": [100, 0, 5]}));
  let uz = tip["probes"][0]["value"].as_f64().unwrap();
  assert!(close(-uz, expect, 0.03), "tip {uz} vs {expect}");
  let mid = run(&mut s, "sim.probe", json!({"field": "sxx", "points": [[50, 0, 10], [50, 0, 0]]}));
  let sigma = 100.0 * 50.0 * 5.0 / i;
  let top = mid["probes"][0]["value"].as_f64().unwrap();
  let bottom = mid["probes"][1]["value"].as_f64().unwrap();
  assert!(close(top, sigma, 0.05) && close(-bottom, sigma, 0.05), "{top} {bottom} vs {sigma}");
  let reaction = r["results"]["reactions"][0][2].as_f64().unwrap();
  assert!(close(reaction, 100.0, 1e-6), "{r}");
  let res = run(&mut s, "sim.results", json!({"field": "von_mises"}));
  assert!(res["max"].as_f64().unwrap() > sigma, "{res}");
  let sf = run(&mut s, "sim.results", json!({"field": "safety_factor"}));
  assert!(sf["min"].as_f64().unwrap() > 1.0 && sf["min"].as_f64().unwrap() < 4.0, "{sf}");
  // a report, as HTML and Markdown
  let html = run(&mut s, "sim.report", json!({}));
  assert!(html["report"].as_str().unwrap().contains("Fixed (XYZ) on 1 face"));
  let md = run(&mut s, "sim.report", json!({"format": "md"}))["report"].as_str().unwrap().to_string();
  assert!(md.contains("| Maximum displacement |"), "{md}");
  // results drawn in the viewport
  let png = crate::scene::render_png(&s, &json!({"width": 200, "height": 120, "sim": {"field": "von_mises"}})).unwrap();
  assert!(png.len() > 1000);
  // an edit to the study makes the results stale
  run(&mut s, "sim.load", json!({"type": "gravity"}));
  assert!(s.execute("sim.results", &json!({})).is_err());
  let list = run(&mut s, "sim.list", json!({}));
  assert_eq!(list[0]["solved"], false);
  assert_eq!(list[0]["loads"].as_array().unwrap().len(), 2);
}

#[test]
fn aluminium_and_material_override() {
  let mut s = Session::default();
  bar(&mut s);
  run(&mut s, "sim.study", json!({}));
  run(&mut s, "sim.constraint", json!({"faces": [{"point": [0, 0, 5]}]}));
  run(
    &mut s,
    "sim.load",
    json!({"type": "force", "faces": [{"point": [100, 0, 5]}], "magnitude": 50, "direction": "-z"}),
  );
  let m = run(&mut s, "sim.material", json!({"material": "Aluminum"}));
  assert_eq!(m[0]["material"]["name"], "Aluminum");
  run(&mut s, "sim.mesh", json!({"size": 3}));
  run(&mut s, "sim.solve", json!({}));
  let uz =
    run(&mut s, "sim.probe", json!({"field": "uz", "point": [100, 0, 5]}))["probes"][0]["value"].as_f64().unwrap();
  let expect = 50.0 * 100f64.powi(3) / (3.0 * 68_900.0 * 10.0f64.powi(4) / 12.0);
  assert!(close(-uz, expect, 0.04), "{uz} vs {expect}");
  // pressure along the normal by default: a pressure on the end face pushes in −X
  run(&mut s, "sim.edit", json!({"name": "Renamed"}));
  assert_eq!(run(&mut s, "sim.info", json!({"study": "Renamed"}))["name"], "Renamed");
  assert!(
    s.execute("sim.load", &json!({"type": "temperature", "faces": [{"point": [0, 0, 5]}], "temperature": 20})).is_err()
  );
}

/// Kt of a hole in a plate in tension: Heywood's 2 + (1 − d/W)³ on the net section stress.
#[test]
fn plate_with_hole() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 40, "depth": 120, "height": 4}));
  run(&mut s, "cylinder", json!({"radius": 4, "height": 4, "operation": "cut"}));
  run(&mut s, "sim.study", json!({}));
  run(&mut s, "sim.constraint", json!({"faces": [{"point": [0, -60, 2]}]}));
  run(&mut s, "sim.load", json!({"type": "pressure", "faces": [{"point": [0, 60, 2]}], "pressure": -10}));
  let m = run(&mut s, "sim.mesh", json!({"size": 4, "refine": [{"faces": [{"point": [4, 0, 2]}], "size": 0.6}]}));
  assert!(m["bodies"][0]["min_quality"].as_f64().unwrap() > 0.0, "{m}");
  run(&mut s, "sim.solve", json!({}));
  let far =
    run(&mut s, "sim.probe", json!({"field": "syy", "point": [0, 40, 2]}))["probes"][0]["value"].as_f64().unwrap();
  assert!(close(far, 10.0, 0.02), "{far}");
  let edge = run(&mut s, "sim.probe", json!({"field": "syy", "points": [[4, 0, 2], [-4, 0, 2]]}));
  let peak = (edge["probes"][0]["value"].as_f64().unwrap() + edge["probes"][1]["value"].as_f64().unwrap()) / 2.0;
  let expect = (2.0 + 0.8f64.powi(3)) * 10.0 / 0.8;
  assert!(close(peak, expect, 0.07), "peak {peak} expected {expect}");
}

#[test]
fn natural_frequencies() {
  let mut s = Session::default();
  bar(&mut s);
  run(&mut s, "sim.study", json!({"type": "modal", "modes": 4}));
  run(&mut s, "sim.constraint", json!({"faces": [{"point": [0, 0, 5]}]}));
  run(&mut s, "sim.mesh", json!({"size": 3}));
  let r = run(&mut s, "sim.solve", json!({}));
  let f: Vec<f64> = r["results"]["frequencies"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
  let f1 =
    1.875104f64.powi(2) / std::f64::consts::TAU * (210_000.0 * 10.0f64.powi(4) / 12.0 / (7.85e-9 * 100.0 * 1e8)).sqrt();
  assert!(close(f[0], f1, 0.03) && close(f[1], f1, 0.03), "{f:?} vs {f1}");
  let shape = run(&mut s, "sim.results", json!({"mode": 2}));
  assert!(close(shape["max"].as_f64().unwrap(), 1.0, 1e-9), "{shape}");
  let png =
    crate::scene::render_png(&s, &json!({"width": 120, "height": 80, "sim": {"mode": 1, "phase": 1.0}})).unwrap();
  assert!(!png.is_empty());
  // a load is no part of a modal study
  assert!(
    s.execute("sim.load", &json!({"type": "temperature", "faces": [{"point": [0, 0, 5]}], "temperature": 1})).is_err()
  );
}

#[test]
fn steady_state_conduction() {
  let mut s = Session::default();
  bar(&mut s);
  run(&mut s, "sim.study", json!({"type": "thermal"}));
  run(&mut s, "sim.load", json!({"type": "temperature", "faces": [{"point": [0, 0, 5]}], "temperature": 100}));
  run(&mut s, "sim.load", json!({"type": "temperature", "faces": [{"point": [100, 0, 5]}], "temperature": 20}));
  run(&mut s, "sim.mesh", json!({"size": 4}));
  let r = run(&mut s, "sim.solve", json!({}));
  let t = run(&mut s, "sim.probe", json!({"field": "temperature", "point": [25, 1, 3]}))["probes"][0]["value"]
    .as_f64()
    .unwrap();
  assert!((t - 80.0).abs() < 1e-6, "{t}");
  // q = k A ΔT / L with steel's 50 W/(m·K)
  let q = r["results"]["heat_flow"][0].as_f64().unwrap();
  assert!(close(q, 0.05 * 100.0 * 80.0 / 100.0, 1e-6), "{r}");
  let flux = run(&mut s, "sim.results", json!({"field": "heat_flux"}));
  assert!(close(flux["max"].as_f64().unwrap(), 40_000.0, 1e-6), "{flux}");
  // convection at the cold end instead: all the heat goes out through it
  run(&mut s, "sim.remove", json!({"load": 1}));
  run(
    &mut s,
    "sim.load",
    json!({"type": "convection", "faces": [{"point": [100, 0, 5]}], "coefficient": 100, "ambient": 20}),
  );
  let r = run(&mut s, "sim.solve", json!({}));
  let out = r["results"]["heat_flow"][1].as_f64().unwrap();
  let held = r["results"]["heat_flow"][0].as_f64().unwrap();
  assert!(close(out, held, 1e-6), "{r}");
}

#[test]
fn bonded_assembly() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 50, "depth": 10, "height": 10, "at": [25, 0]}));
  run(&mut s, "box", json!({"width": 50, "depth": 10, "height": 10, "at": [75, 0], "operation": "new_body"}));
  assert_eq!(s.model.bodies.len(), 2);
  run(&mut s, "sim.study", json!({}));
  run(&mut s, "sim.constraint", json!({"faces": [{"point": [0, 0, 5]}]}));
  run(&mut s, "sim.load", json!({"type": "force", "faces": [{"point": [100, 0, 5]}], "force": [0, 0, -100]}));
  run(&mut s, "sim.mesh", json!({"size": 2.5}));
  let r = run(&mut s, "sim.solve", json!({}));
  assert!(r["ties"].as_u64().unwrap() > 0, "{r}");
  let uz =
    run(&mut s, "sim.probe", json!({"field": "uz", "point": [100, 0, 5]}))["probes"][0]["value"].as_f64().unwrap();
  let (e, i) = (210_000.0, 10.0f64.powi(4) / 12.0);
  let expect = 100.0 * 100f64.powi(3) / (3.0 * e * i) + 100.0 * 100.0 / (5.0 / 6.0 * e / 2.6 * 100.0);
  assert!(close(-uz, expect, 0.04), "{uz} vs {expect}");
  // unbonded, the free part has nothing holding it
  run(&mut s, "sim.contact", json!({"mode": "manual"}));
  assert!(s.execute("sim.solve", &json!({})).is_err());
  let b: Vec<u32> = s.model.bodies.iter().map(|b| b.id.0).collect();
  run(&mut s, "sim.contact", json!({"bond": b}));
  run(&mut s, "sim.solve", json!({}));
}

#[test]
fn studies_save_with_the_design() {
  let mut s = Session::default();
  bar(&mut s);
  run(&mut s, "sim.study", json!({"type": "thermal", "name": "Heat"}));
  run(&mut s, "sim.load", json!({"type": "heat_flux", "faces": [{"point": [0, 0, 5]}], "flux": 1000}));
  run(&mut s, "sim.mesh", json!({"size": 5, "refine": [{"faces": [{"point": [0, 0, 5]}], "size": 2}]}));
  let text = serde_json::to_string(&s.design).unwrap();
  let back: fox_model::Design = serde_json::from_str(&text).unwrap();
  assert_eq!(back.sim, s.design.sim);
  assert_eq!(back.sim[0].mesh.refine.len(), 1);
  run(&mut s, "undo", json!({}));
  assert!(s.design.sim[0].mesh.refine.is_empty());
  run(&mut s, "sim.delete", json!({}));
  assert!(s.design.sim.is_empty());
}
