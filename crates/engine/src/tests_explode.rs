//! Tests for exploded views and batch renders of components.

use fox_kernel::glam::DVec3;
use serde_json::{Value, json};

use crate::Session;
use crate::scene::{Pick, SceneBuilder, ViewOptions};

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => {
      assert!(v.get("error").is_none_or(Value::is_null), "{cmd} {p}: {v}");
      v
    }
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

/// A 40 x 40 x 2 plate lifted to `z`, in its own component; returns (component, body).
fn layer(s: &mut Session, name: &str, z: f64) -> (u64, u32) {
  run(s, "box", json!({"width": 40, "depth": 40, "height": 2}));
  let body = s.model.bodies.last().unwrap().id.0;
  if z != 0.0 {
    run(s, "move", json!({"bodies": [body], "translate": [0, 0, z]}));
  }
  let c = run(s, "component.create", json!({"name": name, "bodies": [body]}))["component"].as_u64().unwrap();
  (c, body)
}

/// Three stacked layers, given out of order: 02 on top of 01, 03 on top of 02.
fn sign(s: &mut Session) -> [(u64, u32); 3] {
  let b = layer(s, "02 - Face", 2.0);
  let a = layer(s, "01 - Back", 0.0);
  let c = layer(s, "03 - Letters", 4.0);
  [a, b, c]
}

/// The lowest Z of a body's drawn triangles.
fn drawn_min_z(s: &Session, body: u32) -> f32 {
  let (scene, picks) = SceneBuilder::default().build(s, &ViewOptions { hide_edges: true, ..Default::default() });
  scene
    .meshes
    .iter()
    .filter(|m| {
      let id = m.tri_ids.first().copied().unwrap_or(m.id);
      matches!(picks.get(id), Some(Pick::Face { body: b, .. }) if b.0 == body)
    })
    .flat_map(|m| m.positions.iter().map(|p| p[2]))
    .fold(f32::MAX, f32::min)
}

fn bbox(s: &Session, body: u32) -> (DVec3, DVec3) {
  let m = s.model.bodies.iter().find(|b| b.id.0 == body).unwrap().shape.mass().unwrap();
  (m.bbox_min, m.bbox_max)
}

#[test]
fn explode_lifts_components_by_position_for_display_only() {
  let mut s = Session::default();
  let [(c1, b1), (c2, b2), (c3, b3)] = sign(&mut s);
  let features = s.design.features.len();
  let undo = s.undo.len();
  let r = run(&mut s, "view.explode", json!({"factor": 3}));
  // range 6 mm (0..6), N = 3, factor 3: the i-th by i x 6 x 3 / 3 = 6i, lowest first
  let parts = r["components"].as_array().unwrap();
  let off = |c: u64| parts.iter().find(|p| p["component"] == json!(c)).unwrap()["offset"].clone();
  assert_eq!(off(c1), json!([0.0, 0.0, 0.0]), "{r}");
  assert_eq!(off(c2), json!([0.0, 0.0, 6.0]), "{r}");
  assert_eq!(off(c3), json!([0.0, 0.0, 12.0]), "{r}");
  // drawn where the view puts them; the model, timeline and undo history stay as they were
  assert!((drawn_min_z(&s, b3) - 16.0).abs() < 1e-3, "{}", drawn_min_z(&s, b3));
  assert!((drawn_min_z(&s, b1) - 0.0).abs() < 1e-3);
  assert!((bbox(&s, b2).0.z - 2.0).abs() < 1e-9);
  assert_eq!(s.design.features.len(), features);
  assert_eq!(s.undo.len(), undo);

  // fields not given keep the view's; a direction of its own
  let r = run(&mut s, "view.explode", json!({"direction": "x"}));
  assert_eq!(r["factor"], json!(3.0));
  // the boxes are side by side in X: extent 40, all centres equal → browser order kept
  let parts = r["components"].as_array().unwrap();
  assert_eq!(parts.iter().map(|p| p["offset"][0].as_f64().unwrap()).fold(0.0, f64::max), 80.0, "{r}");
  // auto picks the stacking direction
  let r = run(&mut s, "view.explode", json!({"direction": "auto"}));
  let top = r["components"].as_array().unwrap().iter().find(|p| p["component"] == json!(c3)).unwrap().clone();
  assert_eq!(top["offset"], json!([0.0, 0.0, 12.0]), "{r}");
  assert!(s.execute("view.explode", &json!({"direction": "w"})).is_err());
  assert!(s.execute("view.explode", &json!({"factor": -1})).is_err());

  // saved with the design
  let dir = std::env::temp_dir().join(format!("fox-explode-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("exploded.fox");
  s.save_to(&path).unwrap();
  let back = Session::open(&path).unwrap();
  assert_eq!(back.design.exploded_view, s.design.exploded_view);
  assert!((drawn_min_z(&back, b3) - 16.0).abs() < 1e-3);
  let _ = std::fs::remove_dir_all(&dir);

  run(&mut s, "view.explode", json!({"off": true}));
  assert!(s.design.exploded_view.is_none());
  assert!((drawn_min_z(&s, b3) - 4.0).abs() < 1e-3);
}

#[test]
fn nested_components_move_with_their_parent_only() {
  let mut s = Session::default();
  let [(c1, _), (c2, b2), (c3, b3)] = sign(&mut s);
  // 03 goes under 02: two top-level units, 02 (with 03) moves once
  run(&mut s, "component.parent", json!({"component": c3, "parent": c2}));
  let r = run(&mut s, "view.explode", json!({"factor": 2}));
  let parts = r["components"].as_array().unwrap();
  assert_eq!(parts.len(), 2, "{r}");
  // range 6, N 2, factor 2: 02 by 1 x 6 x 2 / 2 = 6
  let p2 = parts.iter().find(|p| p["component"] == json!(c2)).unwrap();
  assert_eq!(p2["offset"], json!([0.0, 0.0, 6.0]));
  assert!(parts.iter().any(|p| p["component"] == json!(c1)));
  assert!((drawn_min_z(&s, b2) - 8.0).abs() < 1e-3);
  assert!((drawn_min_z(&s, b3) - 10.0).abs() < 1e-3, "{}", drawn_min_z(&s, b3));
}

#[test]
fn render_and_export_respect_the_exploded_view() {
  let mut s = Session::default();
  let [_, _, (_, b3)] = sign(&mut s);
  let dir = std::env::temp_dir().join(format!("fox-explode-render-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  // an orthographic top view, and a 45/30 exploded one, without changing the design
  let top = dir.join("top.png");
  run(&mut s, "view.render", json!({"path": top, "view": "top", "ortho": true, "width": 64, "height": 64}));
  let exploded = dir.join("exploded.png");
  run(
    &mut s,
    "view.render",
    json!({"path": exploded, "yaw": 45, "pitch": 30, "ortho": true, "explode": {"factor": 20}, "width": 64, "height": 64}),
  );
  assert!(s.design.exploded_view.is_none());
  assert!(std::fs::metadata(&top).unwrap().len() > 0 && std::fs::metadata(&exploded).unwrap().len() > 0);
  // a named view's camera
  run(&mut s, "view.named.save", json!({"name": "Hero", "target": [0, 0, 0], "yaw": 0.3, "pitch": 0.4, "distance": 200}));
  run(&mut s, "view.render", json!({"path": dir.join("hero.png"), "camera": "Hero", "width": 32, "height": 32}));
  assert!(s.execute("view.render", &json!({"path": dir.join("x.png"), "camera": "Nope"})).is_err());

  // mesh export on request: 03 lifted by 2 x 6 x 1 / 3 = 4
  let stl = dir.join("exploded.obj");
  run(&mut s, "export", json!({"path": stl, "bodies": [b3], "explode": true}));
  let text = std::fs::read_to_string(&stl).unwrap();
  let max_z = text
    .lines()
    .filter_map(|l| l.strip_prefix("v "))
    .filter_map(|l| l.split_whitespace().nth(2)?.parse::<f64>().ok())
    .fold(f64::MIN, f64::max);
  assert!((max_z - 10.0).abs() < 1e-3, "{max_z}");
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn render_components_writes_one_image_each_and_a_manifest() {
  let mut s = Session::default();
  let [(_, b1), _, (c3, _)] = sign(&mut s);
  run(&mut s, "body.appearance", json!({"body": b1, "appearance": "Aluminum - Anodized Red"}));
  run(&mut s, "body.visible", json!({"body": b1, "visible": false}));
  run(&mut s, "view.explode", json!({"factor": 5}));
  let dir = std::env::temp_dir().join(format!("fox-render-components-{}", std::process::id()));
  let r = run(
    &mut s,
    "view.render_components",
    json!({"folder": dir, "size": 48, "label": "LA-1234", "camera": {"yaw": 15, "pitch": 20}}),
  );
  assert_eq!(r["images"], json!(3), "{r}");
  let parts = r["parts"].as_array().unwrap();
  let back = parts.iter().find(|p| p["name"] == "01 - Back").unwrap();
  assert_eq!(back["number"], "01");
  assert_eq!(back["appearance"], "Aluminum - Anodized Red");
  // 40 x 40 x 2: 2 x 1600 + 4 x 80 = 3520 mm²
  assert!((back["area_mm2"].as_f64().unwrap() - 3520.0).abs() < 1e-3, "{back}");
  assert!((back["area_in2"].as_f64().unwrap() - 3520.0 / 645.16).abs() < 1e-6);
  let img = dir.join("LA-1234 - 01 - Back.png");
  assert_eq!(back["image"], json!(img));
  let png = image::open(&img).unwrap();
  assert_eq!((png.width(), png.height()), (48, 48));
  let letters = parts.iter().find(|p| p["component"] == json!(c3)).unwrap();
  assert_eq!(letters["appearance"], "Default");
  // the manifest, in both forms
  let json: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("LA-1234 - Part Data.json")).unwrap()).unwrap();
  assert_eq!(json["parts"].as_array().unwrap().len(), 3);
  let csv = std::fs::read_to_string(dir.join("LA-1234 - Part Data.csv")).unwrap();
  assert_eq!(csv.lines().count(), 4, "{csv}");
  assert!(csv.lines().next().unwrap().starts_with("index,component,number,name,area_in2"));
  // visibility and the exploded view are as they were
  assert!(!s.body_visible(fox_model::BodyId(b1)));
  assert!(s.design.exploded_view.is_some());
  // other patterns, a named view and a bad camera
  run(&mut s, "view.named.save", json!({"name": "Side", "target": [0, 0, 0], "yaw": 1.0, "pitch": 0.2, "distance": 50}));
  let r = run(
    &mut s,
    "view.render_components",
    json!({"folder": dir.join("n"), "size": 16, "camera": "Side", "name_pattern": "{index} {number}", "components": [c3]}),
  );
  assert_eq!(r["images"], json!(1));
  assert!(dir.join("n").join("01 03.png").exists());
  assert!(s.execute("view.render_components", &json!({"folder": dir, "camera": "Nope"})).is_err());
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn leading_numbers_and_file_names() {
  use crate::explode::{leading_number, sanitize};
  assert_eq!(leading_number("01 - Letters").as_deref(), Some("01"));
  assert_eq!(leading_number("12").as_deref(), Some("12"));
  assert_eq!(leading_number("Letters 01"), None);
  assert_eq!(sanitize("A/B: c?"), "A B c");
  assert_eq!(sanitize("  "), "Untitled");
}
