//! Tests for configurations (the table, switching, exporting one) and base features (history
//! off, direct edits).

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

/// A 40 × 20 × h block with h a parameter (10 mm).
fn block(s: &mut Session) {
  run(s, "param.add", json!({"name": "h", "expr": "10 mm"}));
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]}));
  run(s, "extrude", json!({"distance": "h"}));
}

#[test]
fn configuration_table_switch_and_export() {
  let mut s = Session::default();
  block(&mut s);
  let body = s.model.bodies[0].id.0;
  let ext = s.design.features[1].id.0;
  run(&mut s, "config.create", json!({"name": "Tall"}));
  run(&mut s, "config.set", json!({"configuration": "Tall", "parameter": "h", "expr": "30 mm"}));
  run(&mut s, "config.set", json!({"configuration": "Tall", "body": body, "appearance": "ABS (Red)"}));
  run(&mut s, "config.create", json!({"name": "Empty", "copy": "Tall"}));
  run(&mut s, "config.set", json!({"configuration": "Empty", "feature": ext, "suppressed": true}));
  assert!(s.execute("config.create", &json!({"name": "Tall"})).is_err());
  assert!(s.execute("config.set", &json!({"configuration": "Tall", "parameter": "nope", "expr": 1})).is_err());
  assert!(s.execute("config.set", &json!({"configuration": "Tall", "joint": ext, "value": 1})).is_err());

  let t = run(&mut s, "config.table", json!({}));
  assert_eq!(t["columns"].as_array().unwrap().len(), 3);
  assert_eq!(t["rows"][1]["name"], "Empty");
  assert_eq!(t["columns"][0]["label"], "h");
  assert_eq!(t["columns"][0]["default"], "10 mm");
  assert_eq!(t["rows"][0]["cells"][0], "30 mm");
  assert!(t["rows"][0]["cells"][1].is_null());

  assert!((volume(&s) - 8000.0).abs() < 1e-6);
  run(&mut s, "config.activate", json!({"name": "Tall"}));
  assert!((volume(&s) - 24000.0).abs() < 1e-6);
  let info = run(&mut s, "info", json!({}));
  assert!(info.to_string().contains("#"), "{info}");
  run(&mut s, "config.activate", json!({"name": "Empty"}));
  assert!(s.model.bodies.is_empty());
  run(&mut s, "config.activate", json!({}));
  assert!((volume(&s) - 8000.0).abs() < 1e-6);
  assert!(s.undo());
  assert!(s.model.bodies.is_empty());
  s.redo();

  // export one configuration without switching
  let dir = std::env::temp_dir().join(format!("clf-config-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let step = dir.join("tall.step");
  let r = run(&mut s, "export", json!({"path": step, "configuration": "Tall"}));
  assert_eq!(r["configuration"], "Tall");
  let tall = Session::open(&step).unwrap();
  assert!((volume(&tall) - 24000.0).abs() < 1e-3);
  assert!(s.design.configurations.active.is_none());
  assert!(s.execute("export", &json!({"path": step, "configuration": "Nope"})).is_err());

  // the table survives save/load; clearing a cell and renaming and deleting rows
  let clf = dir.join("configs.clf");
  run(&mut s, "config.activate", json!({"name": "Tall"}));
  run(&mut s, "save", json!({"path": clf}));
  let mut back = Session::open(&clf).unwrap();
  assert_eq!(back.design.configurations, s.design.configurations);
  assert!((volume(&back) - 24000.0).abs() < 1e-6);
  run(&mut back, "config.set", json!({"configuration": "Tall", "parameter": "h", "expr": null}));
  assert!((volume(&back) - 8000.0).abs() < 1e-6);
  run(&mut back, "config.rename", json!({"name": "Tall", "new_name": "Red"}));
  assert_eq!(back.design.configurations.active.as_deref(), Some("Red"));
  run(&mut back, "config.delete", json!({"name": "Red"}));
  let l = run(&mut back, "config.list", json!({}));
  assert!(l["active"].is_null());
  assert_eq!(l["configurations"].as_array().unwrap().len(), 1);
  std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn history_off_and_direct_edits() {
  let mut s = Session::default();
  block(&mut s);
  let body = s.model.bodies[0].id.0;
  let c = run(&mut s, "component.create", json!({"name": "Block", "bodies": [body]}))["component"].clone();
  // a direct edit in a parametric timeline goes into a new base feature
  let r = run(&mut s, "direct.press_pull", json!({"faces": [{"point": [20, 10, 10]}], "distance": "h / 2"}));
  assert_eq!(r["edits"], 1);
  assert!((volume(&s) - 40.0 * 20.0 * 15.0).abs() < 1e-6);
  run(&mut s, "base.finish", json!({}));
  // the base feature follows the parameters upstream of it, with its own value fixed
  run(&mut s, "param.set", json!({"name": "h", "expr": "20 mm"}));
  assert!((volume(&s) - 40.0 * 20.0 * 25.0).abs() < 1e-6);
  assert_eq!(s.design.features.len(), 3);

  let r = run(&mut s, "design.history", json!({"capture": false}));
  assert_eq!(r["bodies"], 1);
  assert_eq!(s.design.features.len(), 1);
  assert!(s.design.history_off);
  assert_eq!(s.model.bodies[0].id.0, body);
  assert_eq!(s.design.components[0].bodies[0].0, body);
  assert!(c.is_number());
  assert!((volume(&s) - 20000.0).abs() < 1e-6);
  run(&mut s, "direct.move_face", json!({"faces": [{"point": [20, 10, 25]}], "translate": [0, 0, -5]}));
  let r = run(&mut s, "direct.delete_face", json!({"faces": [{"point": [20, 10, 20]}]}));
  // a box can't lose a face and stay solid: the edit says so and leaves the body as it was
  assert!(r["warning"].as_str().unwrap().contains("Delete Face 2: the faces could not be removed"), "{r}");
  assert!((volume(&s) - 16000.0).abs() < 1e-6);
  s.undo();
  assert_eq!(s.design.features.len(), 1);
  assert!((volume(&s) - 16000.0).abs() < 1e-6);

  // saved and reopened, the base feature replays
  let path = std::env::temp_dir().join(format!("clf-base-{}.clf", std::process::id()));
  run(&mut s, "save", json!({"path": path}));
  let back = Session::open(&path).unwrap();
  std::fs::remove_file(&path).ok();
  assert!(back.design.history_off);
  assert!((volume(&back) - 16000.0).abs() < 1e-6);
  assert_eq!(back.model.bodies[0].id.0, body);

  // history back on: parametric features follow the base feature
  run(&mut s, "design.history", json!({"capture": true}));
  assert!(!s.design.history_off);
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [100, 0], "corner2": [110, 10]}));
  run(&mut s, "extrude", json!({"distance": 10}));
  assert_eq!(s.model.bodies.len(), 2);
  // editing the base feature rolls the timeline back to it
  let base = s.design.features[0].id.0;
  run(&mut s, "base.edit", json!({"feature": base}));
  assert_eq!(s.design.open_base_feature().map(|f| f.0), Some(base));
  assert_eq!(s.model.bodies.len(), 1);
  run(&mut s, "base.finish", json!({}));
  assert_eq!(s.model.bodies.len(), 2);
  assert!(s.execute("base.edit", &json!({"feature": s.design.features[1].id.0})).is_err());
}
