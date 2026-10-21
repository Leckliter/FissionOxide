//! Tests for the Properties inspector (`inspect.properties`) and overall size
//! (`inspect.overall_size`).

use serde_json::{Value, json};

use crate::{Sel, Session};

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn near(a: &Value, b: f64) -> bool {
  a.as_f64().is_some_and(|a| (a - b).abs() < 1e-3)
}

/// A 40 x 20 x 3 plate on XY.
fn plate(s: &mut Session) -> u32 {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]}));
  run(s, "extrude", json!({"distance": 3}));
  s.model.bodies[0].id.0
}

#[test]
fn properties_of_faces_edges_vertices_bodies_and_components() {
  let mut s = Session::default();
  let b = plate(&mut s);
  let body = s.model.body(fox_model::BodyId(b)).unwrap();
  let top = body.faces().iter().max_by(|x, y| (x.center.z, x.area).partial_cmp(&(y.center.z, y.area)).unwrap()).unwrap().id;
  let edge = body.edges().iter().find(|e| (e.length - 40.0).abs() < 1e-6).unwrap().id;
  run(&mut s, "component.create", json!({"name": "Plate", "bodies": [b]}));
  let comp = s.design.components.iter().find(|c| c.name == "Plate").unwrap().id.0;
  run(&mut s, "component.material", json!({"component": comp, "material": "Aluminum"}));

  // default: the selection
  s.selection = vec![
    Sel::Face { body: fox_model::BodyId(b), face: top },
    Sel::Edge { body: fox_model::BodyId(b), edge },
    Sel::Vertex { body: fox_model::BodyId(b), edge, end: false },
    Sel::Body { body: fox_model::BodyId(b) },
  ];
  let r = run(&mut s, "inspect.properties", json!({}));
  assert_eq!(r["count"], 4);
  let face = &r["items"][0];
  assert_eq!(face["type"], "face");
  assert_eq!(face["surface"], "plane");
  assert!(near(&face["area_mm2"], 800.0), "{face}");
  assert!(near(&face["normal"][2], 1.0), "{face}");
  assert!(near(&face["box"]["size"][0], 40.0), "{face}");
  assert_eq!(face["component"]["name"], "Plate");
  let e = &r["items"][1];
  assert_eq!(e["curve"], "line");
  assert!(near(&e["length"], 40.0), "{e}");
  assert_eq!(r["items"][2]["type"], "vertex");
  assert!(r["items"][2]["point"].is_array());
  let bd = &r["items"][3];
  assert_eq!(bd["kind"], "solid");
  assert!(near(&bd["volume_mm3"], 2400.0), "{bd}");
  assert!(near(&bd["mass_g"], 2400.0 * 2.70 / 1000.0), "{bd}");
  assert!(near(&bd["box"]["size"][2], 3.0), "{bd}");

  // explicit items: a component
  let r = run(&mut s, "inspect.properties", json!({"items": [{"type": "component", "component": comp}]}));
  let c = &r["items"][0];
  assert_eq!(c["name"], "Plate");
  assert_eq!(c["bodies"], json!([b]));
  assert!(near(&c["volume_mm3"], 2400.0), "{c}");

  // exports
  let dir = std::env::temp_dir().join(format!("fox-props-{}", std::process::id()));
  let json_path = dir.join("dump.json");
  run(&mut s, "inspect.properties", json!({"items": [{"body": b}], "export": json_path}));
  let dumped: Value = serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
  assert_eq!(dumped["items"][0]["type"], "body");
  let txt = dir.join("dump.txt");
  run(&mut s, "inspect.properties", json!({"items": [{"body": b, "face": top}], "export": txt}));
  let text = std::fs::read_to_string(&txt).unwrap();
  assert!(text.starts_with("face body="), "{text}");
  assert!(text.contains("area_mm2: 800"), "{text}");
  assert!(s.execute("inspect.properties", &json!({"items": [{"body": b}], "export": dir.join("x.pdf")})).is_err());
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn properties_of_sketches_and_sketch_curves() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [30, 40]]}));
  run(&mut s, "sketch.circle", json!({"center": [10, 10], "radius": 5}));
  run(&mut s, "sketch.arc", json!({"center": [0, 0], "start": [10, 0], "end": [0, 10]}));
  let sk = s.edit_sketch.unwrap();
  let curves: Vec<_> = s.design.sketch(sk).unwrap().curves.iter().map(|c| c.id).collect();
  let items: Vec<Value> = curves
    .iter()
    .map(|c| json!({"type": "sketch_curve", "sketch": sk.0, "curve": c.0}))
    .chain([json!({"type": "sketch", "sketch": sk.0})])
    .collect();
  let r = run(&mut s, "inspect.properties", json!({"items": items}));
  let line = &r["items"][0];
  assert_eq!(line["curve"], "line");
  assert!(near(&line["length"], 50.0), "{line}");
  assert_eq!(line["start_2d"], json!([0.0, 0.0]));
  assert!(line["sketch_plane"]["normal"].is_array(), "{line}");
  let circle = &r["items"][1];
  assert!(near(&circle["radius"], 5.0), "{circle}");
  assert!(near(&circle["length"], std::f64::consts::TAU * 5.0), "{circle}");
  let arc = &r["items"][2];
  assert!(near(&arc["sweep_deg"], 90.0), "{arc}");
  assert!(near(&arc["length"], std::f64::consts::FRAC_PI_2 * 10.0), "{arc}");
  let sketch = &r["items"][3];
  assert_eq!(sketch["type"], "sketch");
  assert_eq!(sketch["curves"], 3);
}

#[test]
fn overall_size_planar_and_box() {
  let mut s = Session::default();
  // a plate standing up on XZ: width along X, height along Z, thickness along Y
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [60, 25]}));
  run(&mut s, "extrude", json!({"distance": 2}));
  let r = run(&mut s, "inspect.overall_size", json!({}));
  assert_eq!(r["mode"], "planar");
  assert!(near(&r["width"], 60.0), "{r}");
  assert!(near(&r["height"], 25.0), "{r}");
  assert!(near(&r["thickness"], 2.0), "{r}");

  // a sphere has no planar face: sorted box extents
  let mut s = Session::default();
  run(&mut s, "sphere", json!({"radius": 5}));
  let r = run(&mut s, "inspect.overall_size", json!({}));
  assert_eq!(r["mode"], "box");
  assert!(near(&r["width"], 10.0) && near(&r["thickness"], 10.0), "{r}");

  // exports in inches
  let mut s = Session::default();
  let b = plate(&mut s);
  let dir = std::env::temp_dir().join(format!("fox-size-{}", std::process::id()));
  let csv = dir.join("size.csv");
  let r = run(&mut s, "inspect.overall_size", json!({"bodies": [b], "export": csv, "units": "in"}));
  assert!(near(&r["width"], 40.0), "{r}");
  let text = std::fs::read_to_string(&csv).unwrap();
  assert!(text.lines().nth(1).unwrap().contains(&format!("{}", 40.0 / 25.4)), "{text}");
  let js = dir.join("size.json");
  run(&mut s, "inspect.overall_size", json!({"export": js}));
  let v: Value = serde_json::from_str(&std::fs::read_to_string(&js).unwrap()).unwrap();
  assert_eq!(v["units"], "mm");
  assert!(near(&v["thickness"], 3.0), "{v}");
  let txt = dir.join("size.txt");
  run(&mut s, "inspect.overall_size", json!({"export": txt}));
  assert!(std::fs::read_to_string(&txt).unwrap().contains("Width: 40 mm"));
  let _ = std::fs::remove_dir_all(&dir);
}
