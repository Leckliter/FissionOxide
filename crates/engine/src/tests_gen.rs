//! Tests for the generative design commands.

use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

/// The example script's commands.
fn example() -> Vec<(String, Value)> {
  let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/generative_bracket.jsonl");
  std::fs::read_to_string(path)
    .unwrap()
    .lines()
    .map(str::trim)
    .filter(|l| !l.is_empty() && !l.starts_with('#'))
    .map(|l| {
      let mut p: Value = serde_json::from_str(l).unwrap();
      let cmd = p["cmd"].as_str().unwrap().to_string();
      p.as_object_mut().unwrap().remove("cmd");
      (cmd, p)
    })
    .collect()
}

/// The example bracket up to (not including) generating, coarser so the test is quick.
fn bracket() -> Session {
  let mut s = Session::default();
  for (cmd, p) in example() {
    if cmd == "gen.generate" {
      break;
    }
    run(&mut s, &cmd, p);
  }
  run(&mut s, "gen.design_space", json!({"resolution": 3, "iterations": 24}));
  s
}

#[test]
fn example_script_commands_exist() {
  for (cmd, _) in example() {
    assert!(crate::COMMANDS.iter().any(|c| c.id == cmd), "{cmd}");
  }
}

#[test]
fn study_inputs_and_precheck() {
  let mut s = bracket();
  let st = run(&mut s, "gen.info", json!({}));
  assert_eq!(st["preserve"], json!([1000, 3000, 5000]));
  assert_eq!(st["obstacles"], json!([7000, 8000, 9000]));
  let cases = st["cases"].as_array().unwrap();
  assert_eq!(cases.len(), 2, "{st}");
  // every case kept the bolt constraints
  for c in cases {
    assert_eq!(c["constraints"].as_array().unwrap().len(), 2);
    assert_eq!(c["loads"].as_array().unwrap().len(), 1);
  }
  assert_eq!(st["materials"].as_array().unwrap().len(), 2);
  assert_eq!(st["methods"].as_array().unwrap().len(), 2);
  let pc = run(&mut s, "gen.precheck", json!({}));
  assert_eq!(pc["ready"], true, "{pc}");
  assert_eq!(pc["outcomes_to_generate"], 4);
  assert!(pc["preserve"].as_u64().unwrap() > 20, "{pc}");
  for c in pc["cases"].as_array().unwrap() {
    assert!(c["load_nodes"][0].as_u64().unwrap() > 0, "{pc}");
    assert!(c["constraint_nodes"].as_array().unwrap().iter().all(|n| n.as_u64().unwrap() > 0), "{pc}");
  }
  // the symmetry plane at y = 25 is the grid's middle
  let (o, h, n) =
    (pc["origin"][1].as_f64().unwrap(), pc["element_size"].as_f64().unwrap(), pc["grid"][1].as_f64().unwrap());
  assert!((o + 0.5 * h * n - 25.0).abs() < 1e-6, "{pc}");
  // undo takes a study edit back
  run(&mut s, "gen.objective", json!({"mass_target": 0.4}));
  s.undo();
  assert_eq!(run(&mut s, "gen.info", json!({}))["mass_target"], 0.25);
  // bad inputs
  assert!(s.execute("gen.materials", &json!({"materials": ["unobtainium"]})).is_err());
  assert!(s.execute("gen.manufacturing", &json!({"methods": ["casting"]})).is_err());
  assert!(s.execute("gen.objective", &json!({"mass_target": 2})).is_err());
  assert!(s.execute("gen.load", &json!({"point": [0, 0, 0]})).is_err());
}

#[test]
fn generate_explore_and_insert_an_outcome() {
  let mut s = bracket();
  run(&mut s, "gen.materials", json!({"materials": ["Aluminum AlSi10Mg"]}));
  run(&mut s, "gen.manufacturing", json!({"methods": ["unrestricted", "milling_3axis"]}));
  let r = run(&mut s, "gen.generate", json!({}));
  let os = r["outcomes"].as_array().unwrap();
  assert_eq!(os.len(), 2, "{r}");
  for o in os {
    assert!(o["mass"].as_f64().unwrap() > 0.0, "{o}");
    assert!(o["safety_factor"].as_f64().unwrap() > 0.0, "{o}");
    assert!(o["max_displacement"].as_f64().unwrap() > 0.0, "{o}");
    let vf = o["volume_fraction"].as_f64().unwrap();
    assert!((vf - 0.25).abs() < 0.1, "volume fraction {vf}: {o}");
    assert!(o["triangles"].as_u64().unwrap() > 100);
  }
  // the milled outcome is no stiffer than the unrestricted one
  let c = |i: usize| os[i]["compliance"].as_f64().unwrap();
  assert!(c(1) >= 0.95 * c(0), "{r}");
  assert_eq!(run(&mut s, "gen.outcomes", json!({})).as_array().unwrap().len(), 2);
  // a mesh body ...
  let before = s.model.bodies.len();
  let m = run(&mut s, "gen.insert", json!({"outcome": os[0]["outcome"]}));
  assert_eq!(s.model.bodies.len(), before + 1);
  let body = s.model.body(fox_model::BodyId(m["body"].as_u64().unwrap() as u32)).unwrap();
  assert!(body.shape.solid_count() > 0, "{m}");
  // the outcome reaches the bolt bosses and the pin boss
  let xs: Vec<f32> = body.mesh().unwrap().positions.iter().map(|p| p[0]).collect();
  let (lo, hi) = xs.iter().fold((f32::MAX, f32::MIN), |(l, h), x| (l.min(*x), h.max(*x)));
  assert!(lo < 0.0 && hi > 100.0, "{lo} .. {hi}");
  // ... and an export
  let path = std::env::temp_dir().join(format!("fox-gen-{}.stl", std::process::id()));
  run(&mut s, "gen.export", json!({"outcome": os[0]["outcome"], "path": path.to_str().unwrap()}));
  assert!(std::fs::metadata(&path).unwrap().len() > 1000);
  let _ = std::fs::remove_file(path);
  // the study survives a save
  let text = serde_json::to_string(&s.design).unwrap();
  let d: fox_model::Design = serde_json::from_str(&text).unwrap();
  assert_eq!(d.generative[0].outcomes.len(), 2);
  assert_eq!(d.generative[0].outcomes[0].mesh.triangles(), os[0]["triangles"].as_u64().unwrap() as usize);
}

#[test]
fn outcome_as_brep_joined_with_the_preserve_geometry() {
  let mut s = bracket();
  run(&mut s, "gen.materials", json!({"materials": ["Titanium 6Al-4V"]}));
  run(&mut s, "gen.manufacturing", json!({"methods": ["unrestricted"], "symmetry": []}));
  run(&mut s, "gen.design_space", json!({"resolution": 4, "iterations": 20}));
  run(&mut s, "gen.generate", json!({}));
  let m = run(&mut s, "gen.insert", json!({"triangles": 800, "brep": true, "merge_preserve": true}));
  // one body, no slivers left over from the booleans
  assert_eq!(s.model.bodies.len(), 7, "{m}");
  assert_eq!(m["steps"], json!(["mesh body", "reduce", "B-rep", "join preserve", "cut obstacles"]), "{m}");
  let body = s.model.body(fox_model::BodyId(m["body"].as_u64().unwrap() as u32)).unwrap();
  assert_eq!(body.shape.solid_count(), 1, "{m}");
  // the exact bolt holes are there: no material at a hole's centre
  let mesh = body.mesh().unwrap();
  let solid_at = |x: f64, y: f64| {
    let probe = fox_gen::Grid { origin: [x - 0.5, y - 0.5, 5.5], h: 1.0, n: [1, 1, 1] };
    probe.inside(&mesh.positions, &mesh.indices)[0]
  };
  assert!(!solid_at(0.0, 0.0) && !solid_at(100.0, 25.0), "the holes are cut");
  assert!(solid_at(6.5, 0.0) && solid_at(100.0, 31.0), "the bosses are there");
  // checked with finite elements on the real body
  let v = run(&mut s, "gen.verify", json!({"size": 6}));
  assert!(v["elements"].as_u64().unwrap() > 100, "{v}");
  assert_eq!(v["cases"].as_array().unwrap().len(), 2);
  let sf = v["safety_factor"].as_f64().unwrap();
  assert!(sf > 1.0 && v["max_displacement"].as_f64().unwrap() > 0.0, "{v}");
  assert_eq!(run(&mut s, "gen.outcomes", json!({}))[0]["verified"]["safety_factor"], json!(sf));
  s.undo();
  // one undo step takes the whole insert back
  let n = s.model.bodies.len();
  s.undo();
  assert_eq!(s.model.bodies.len(), n - 1);
}

#[test]
fn minimise_mass_with_a_safety_factor() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 60, "depth": 10, "height": 20, "at": [30, 5]}));
  run(&mut s, "gen.study", json!({}));
  run(&mut s, "gen.design_space", json!({"min": [0, 0, 0], "max": [60, 10, 20], "resolution": 2.5, "iterations": 16}));
  run(&mut s, "gen.starting_shape", json!({"body": 1000}));
  run(&mut s, "gen.constraint", json!({"face": {"point": [0, 5, 10]}}));
  run(&mut s, "gen.load", json!({"face": {"point": [60, 5, 10]}, "force": [0, 0, -300]}));
  run(&mut s, "gen.load", json!({"case": "Pressure", "face": {"point": [30, 5, 20]}, "pressure": 0.2}));
  run(&mut s, "gen.objective", json!({"objective": "min_mass", "safety_factor": 1.5}));
  run(&mut s, "gen.materials", json!({"materials": ["steel"]}));
  let r = run(&mut s, "gen.generate", json!({}));
  let o = &r["outcomes"][0];
  assert_eq!(o["meets_limits"], true, "{r}");
  assert!(o["safety_factor"].as_f64().unwrap() >= 1.5);
  assert!(o["volume_fraction"].as_f64().unwrap() < 0.4, "{o}");
}
