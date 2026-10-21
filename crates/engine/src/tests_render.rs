//! The Render workspace's commands: materials on bodies and faces, environment, camera,
//! settings, final renders and the gallery kept in the design.

use super::*;
use serde_json::{Value, json};

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
  let d = std::env::temp_dir().join(format!("fox-render-{tag}-{}", std::process::id()));
  std::fs::create_dir_all(&d).unwrap();
  d
}

/// A plate with a cylinder on it.
fn part() -> Session {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 60, "depth": 40, "height": 5}));
  run(
    &mut s,
    "cylinder",
    json!({"at": [0, 0], "radius": 8, "height": 20, "plane": {"point": [0, 0, 5], "normal": [0, 0, 1]}}),
  );
  s
}

fn tiny() -> Value {
  json!({"width": 48, "height": 32, "samples": 4, "gallery": false})
}

#[test]
fn materials_go_on_bodies_and_faces_and_undo() {
  let mut s = part();
  let bodies: Vec<u32> = s.model.bodies.iter().map(|b| b.id.0).collect();
  assert_eq!(bodies.len(), 2);
  // the library holds the appearances under their names
  let lib = run(&mut s, "render.materials", json!({"query": "brass"}));
  assert!(lib.as_array().unwrap().iter().any(|m| m["name"] == "Brass - Polished" && m["metallic"] == 1.0));
  run(&mut s, "render.material.apply", json!({"material": "Gold - Polished", "body": bodies[0]}));
  run(&mut s, "render.material.apply", json!({"material": "Glass - Clear", "faces": [{"point": [0, 0, 25]}]}));
  let a = run(&mut s, "render.assignments", json!({}));
  assert_eq!(a["bodies"][0]["material"], "Gold - Polished");
  assert_eq!(a["bodies"][1]["faces"][0]["material"], "Glass - Clear");
  // the cylinder's top face is a mesh of its own in the render scene
  let scene = rendering::render_scene(&s, rendering::render_camera(&s), None);
  let names: Vec<&str> = scene.materials.iter().map(|m| m.name.as_str()).collect();
  assert!(names.contains(&"Gold - Polished") && names.contains(&"Glass - Clear") && names.contains(&"Steel - Satin"));
  assert_eq!(scene.meshes.len(), 3);
  // a painted body renders as its appearance's material
  run(&mut s, "body.appearance", json!({"body": bodies[1], "appearance": "Copper"}));
  assert_eq!(rendering::body_material(&s, fox_model::BodyId(bodies[1])).name, "Copper");
  // undo takes the face material back off, without touching the model
  let rev = s.revision;
  s.undo();
  s.undo();
  assert!(s.design.render.faces.is_empty());
  assert!(s.revision >= rev);
  assert!(s.execute("render.material.apply", &json!({"material": "Unobtainium", "body": bodies[0]})).is_err());
}

#[test]
fn design_materials_with_textures() {
  let mut s = part();
  let m = run(
    &mut s,
    "render.material.define",
    json!({"name": "Floor", "from": "Wood - Oak", "roughness": 0.2, "texture": {"pattern": "checker", "scale": 5, "projection": "planar"}}),
  );
  assert_eq!(m["design"], true);
  assert_eq!(m["texture"]["scale"], 5.0);
  assert!(s.execute("render.material.define", &json!({"name": "Bad", "metallic": 3})).is_err());
  assert!(s.execute("render.material.define", &json!({"name": "Bad", "texture": {"pattern": "plaid"}})).is_err());
  let body = s.model.bodies[0].id.0;
  run(&mut s, "render.material.apply", json!({"material": "floor", "body": body}));
  assert_eq!(s.design.render.bodies[0].material, "Floor");
  run(&mut s, "render.material.delete", json!({"name": "Floor"}));
  assert!(s.design.render.bodies.is_empty(), "bodies using a deleted material go back to their appearance");
}

#[test]
fn environment_camera_and_settings_are_stored() {
  let mut s = part();
  let e =
    run(&mut s, "render.environment", json!({"preset": "sky", "background": "gradient", "ground_reflection": 0.5}));
  assert_eq!(e["preset"], "sky");
  assert!(s.execute("render.environment", &json!({"preset": "moon"})).is_err());
  assert!(s.execute("render.environment", &json!({"hdri": "/no/such.hdr"})).is_err());
  let c = run(&mut s, "render.camera", json!({"view": "front", "fstop": 2.0, "exposure": 1, "fov": 40}));
  assert_eq!(c["stored"], true);
  assert!((c["fov"].as_f64().unwrap() - 40.0).abs() < 1e-9);
  let st = run(&mut s, "render.settings", json!({"resolution": "1080p", "samples": 32}));
  assert_eq!((st["width"].as_u64(), st["height"].as_u64()), (Some(1920), Some(1080)));
  // all of it survives a save and reload
  let d = temp_dir("store");
  let path = d.join("part.fox");
  run(&mut s, "save", json!({"path": path}));
  let s2 = Session::open(&path).unwrap();
  assert_eq!(s2.design.render, s.design.render);
  let c = run(&mut s, "render.camera", json!({"reset": true}));
  assert_eq!(c["stored"], false);
}

#[test]
fn renders_are_deterministic_and_kept_in_the_gallery() {
  let mut s = part();
  let d = temp_dir("gallery");
  let a = d.join("a.png");
  let b = d.join("b.png");
  run(&mut s, "render.image", json!({"path": a, "width": 48, "height": 32, "samples": 4, "name": "First"}));
  run(&mut s, "render.image", json!({"path": b, "width": 48, "height": 32, "samples": 4}));
  assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
  let g = run(&mut s, "render.gallery", json!({}));
  assert_eq!(g.as_array().unwrap().len(), 2);
  assert_eq!(g[0]["name"], "First");
  assert_eq!(g[1]["name"], "Render 2");
  // the gallery is in the design file (as PNG), and exports byte for byte
  let path = d.join("part.foxz");
  run(&mut s, "save", json!({"path": path}));
  let mut s2 = Session::open(&path).unwrap();
  let out = d.join("out.png");
  run(&mut s2, "render.gallery.export", json!({"id": 1, "path": out}));
  assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&a).unwrap());
  run(&mut s2, "render.gallery.rename", json!({"id": 1, "name": "Hero"}));
  run(&mut s2, "render.gallery.delete", json!({"id": 2}));
  let g = run(&mut s2, "render.gallery", json!({}));
  assert_eq!(g.as_array().unwrap().len(), 1);
  assert_eq!(g[0]["name"], "Hero");
  run(&mut s2, "render.gallery.add", json!({"path": out, "name": "Imported"}));
  assert_eq!(s2.design.render.gallery.last().unwrap().id, 2);
  // a render that isn't kept leaves the gallery alone
  let n = s2.design.render.gallery.len();
  run(&mut s2, "render.image", tiny());
  assert_eq!(s2.design.render.gallery.len(), n);
}

#[test]
fn transparent_background_renders_alpha() {
  let mut s = part();
  run(&mut s, "render.environment", json!({"background": "transparent", "ground": false}));
  let (img, ..) = rendering::render_with(&s, &tiny()).unwrap();
  assert!(img.rgba.chunks(4).any(|p| p[3] == 0), "the background is see-through");
  assert!(img.rgba.chunks(4).any(|p| p[3] == 255), "the part isn't");
}

#[test]
fn the_example_script_runs() {
  let script =
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/render_studio.jsonl")).unwrap();
  let d = temp_dir("example");
  let mut s = Session::default();
  for line in script.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
    let mut p: Value = serde_json::from_str(line).unwrap();
    let cmd = p["cmd"].as_str().unwrap().to_string();
    p.as_object_mut().unwrap().remove("cmd");
    if cmd == "render.image" {
      // quicker, and into the scratch directory
      p["path"] = json!(d.join("studio.png"));
      p["samples"] = json!(4);
    }
    run(&mut s, &cmd, p);
  }
  assert_eq!(s.design.render.gallery.len(), 1);
  assert_eq!(s.design.render.faces.len(), 1);
  let img = image::open(d.join("studio.png")).unwrap();
  assert_eq!((img.width(), img.height()), (640, 400));
}
