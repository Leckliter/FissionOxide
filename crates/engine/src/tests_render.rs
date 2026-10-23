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

/// `ui:render:default-background`: a new design's scene is Fusion's, a grey Solid Color, and it
/// survives a save; a design without render data keeps the environment behind it.
#[test]
fn a_new_design_renders_on_grey() {
  let mut s = part();
  let e = run(&mut s, "render.environment", json!({}));
  assert_eq!(e["background"], "color");
  assert_eq!(e["background_color"], json!(fox_raytrace::env::NEW_DESIGN_GREY));
  let d = temp_dir("grey");
  let path = d.join("part.fox");
  run(&mut s, "save", json!({"path": path}));
  assert_eq!(Session::open(&path).unwrap().design.render.environment.background, fox_raytrace::Background::Color);
  // a file written before render data existed reads back with the environment as background
  let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
  v.as_object_mut().unwrap().remove("render");
  std::fs::write(&path, v.to_string()).unwrap();
  assert_eq!(Session::open(&path).unwrap().design.render.environment, fox_raytrace::Environment::default());
}

/// `ui:render:lights-in-scene`: lights go in the design (up to eight), light the render, and
/// undo; `ui:render:flatten-ground` is an environment setting.
#[test]
fn lights_are_placed_edited_and_rendered() {
  let mut s = part();
  run(&mut s, "render.environment", json!({"preset": "dark", "background": "environment", "flatten_ground": true}));
  assert!(s.design.render.environment.flatten_ground);
  let (dark, ..) = rendering::render_with(&s, &tiny()).unwrap();
  let l = run(&mut s, "render.light.add", json!({"position": [0, 0, 60], "intensity": 3000}));
  assert_eq!(l["name"], "Light1");
  assert_eq!(rendering::render_scene(&s, rendering::render_camera(&s), None).lights.len(), 1);
  let (lit, ..) = rendering::render_with(&s, &tiny()).unwrap();
  let sum = |img: &fox_raytrace::RenderImage| img.rgba.iter().map(|c| *c as u64).sum::<u64>();
  assert!(sum(&lit) > sum(&dark), "the light brightens the render");
  run(&mut s, "render.light.edit", json!({"name": "light1", "on": false, "rename": "Key"}));
  let (off, ..) = rendering::render_with(&s, &tiny()).unwrap();
  assert_eq!(off.rgba, dark.rgba);
  for _ in 0..7 {
    run(&mut s, "render.light.add", json!({"position": [10, 0, 60]}));
  }
  assert_eq!(s.design.render.lights.len(), 8);
  assert!(s.execute("render.light.add", &json!({"position": [0, 0, 1]})).is_err(), "eight at most");
  assert!(s.execute("render.light.add", &json!({})).is_err());
  run(&mut s, "render.light.delete", json!({"name": "Key"}));
  assert_eq!(s.design.render.lights.len(), 7);
  s.undo();
  assert_eq!(s.design.render.lights.len(), 8);
}

/// `ui:render:light-editing`: EDIT LIGHT's values: candela, a colour temperature and an aim.
#[test]
fn lights_take_candela_kelvin_and_rotation() {
  let mut s = part();
  let l = run(
    &mut s,
    "render.light.add",
    json!({"position": [0, 0, 60], "candela": 1500, "kelvin": 3500, "rotation": [0, -90, 0]}),
  );
  assert!((l["intensity"].as_f64().unwrap() / (4.0 * std::f64::consts::PI) - 1500.0).abs() < 0.1, "{l}");
  assert_eq!(l["kelvin"], 3500.0);
  assert_eq!(l["rotation"], json!([0.0, -90.0, 0.0]));
  let light = &s.design.render.lights[0];
  assert_eq!(light.shade(), fox_raytrace::light::kelvin_rgb(3500.0));
  // an RGB colour is Color Mode RGB
  let l = run(&mut s, "render.light.edit", json!({"name": "Light1", "color": [10, 200, 30]}));
  assert!(l["kelvin"].is_null(), "{l}");
  assert!(s.execute("render.light.edit", &json!({"name": "Light1", "kelvin": 20})).is_err());
}

/// `ui:render:dielectric-priority`: bodies get priorities 0, 1 or 2, carried into the render scene.
#[test]
fn dielectric_priorities_reach_the_render_scene() {
  let mut s = part();
  let b = s.model.bodies[1].id.0;
  run(&mut s, "render.priority", json!({"bodies": [b], "priority": 2}));
  assert_eq!(run(&mut s, "render.priorities", json!({})), json!([{"body": b, "priority": 2}]));
  let scene = rendering::render_scene(&s, rendering::render_camera(&s), None);
  assert_eq!(scene.meshes.iter().map(|m| m.priority).max(), Some(2));
  assert!(s.execute("render.priority", &json!({"bodies": [b], "priority": 3})).is_err());
  run(&mut s, "render.priority", json!({"bodies": [b], "priority": 0}));
  assert!(s.design.render.priorities.is_empty());
}

/// `ui:render:texture-map-controls`: a textured body's projection, size, turn and shift change
/// (its library material copied into the design), Reset puts them back; an untextured body has
/// nothing to map.
#[test]
fn texture_map_controls_change_a_bodys_mapping() {
  let mut s = part();
  let b = s.model.bodies[0].id.0;
  assert!(s.execute("render.texture_map", &json!({"body": b, "projection": "planar"})).is_err());
  run(&mut s, "render.material.apply", json!({"material": "Wood - Oak", "body": b}));
  let t = run(
    &mut s,
    "render.texture_map",
    json!({"body": b, "projection": "spherical", "axis": "x", "scale": 12, "rotation": 30, "offset": [0.5, 0]}),
  );
  assert_eq!((t["projection"].as_str(), t["scale"].as_f64()), (Some("spherical"), Some(12.0)));
  let m = s.design.render.materials.iter().find(|m| m.name == "Wood - Oak").unwrap();
  let spec = m.texture.clone().unwrap();
  assert_eq!((spec.projection, spec.rotation, spec.offset), (fox_raytrace::Projection::Spherical, 30.0, [0.5, 0.0]));
  run(&mut s, "render.texture_map", json!({"body": b, "projection": "automatic"}));
  assert_eq!(
    rendering::body_texture(&s, fox_model::BodyId(b)).unwrap().projection,
    fox_raytrace::Projection::Automatic
  );
  run(&mut s, "render.texture_map", json!({"body": b, "reset": true}));
  let lib = fox_raytrace::material::find("Wood - Oak").unwrap().texture.unwrap();
  assert_eq!(rendering::body_texture(&s, fox_model::BodyId(b)).unwrap(), lib);
}

/// One box, 60 x 40 x 5.
fn slab() -> Session {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 60, "depth": 40, "height": 5}));
  s
}

/// The body meshes the scene draws: (colour, texture's scale if it has one).
fn drawn(s: &Session) -> Vec<([u8; 3], Option<f32>)> {
  let (scene, _) = crate::scene::SceneBuilder::default().build(s, &crate::scene::ViewOptions::default());
  scene
    .meshes
    .iter()
    .filter(|m| m.analyze)
    .map(|m| ([m.color[0], m.color[1], m.color[2]], m.texture.as_ref().map(|t| t.projection.scale)))
    .collect()
}

/// `ui:render:appearance-not-drawn`: a material put on a body or a face is drawn in the modeling
/// viewport at once (its colour, and its texture laid over it), as Fusion draws an applied
/// appearance in every workspace; taking it off brings the default look back.
#[test]
fn applied_materials_are_drawn_in_the_viewport() {
  let mut s = slab();
  let b = s.model.bodies[0].id.0;
  let plain = drawn(&s);
  assert!(plain.iter().all(|m| m.1.is_none()), "{plain:?}");
  let oak = fox_raytrace::material::find("Wood - Oak").unwrap();
  let rev = s.revision;
  run(&mut s, "render.material.apply", json!({"material": "Wood - Oak", "body": b}));
  assert!(s.revision > rev, "the viewport sees the change");
  let scale = oak.texture.as_ref().unwrap().scale;
  assert!(drawn(&s).iter().all(|m| *m == (oak.color, Some(scale))), "{:?}", drawn(&s));
  // a face of its own: gold, untextured, over the oak
  let gold = fox_raytrace::material::find("Gold - Polished").unwrap();
  run(&mut s, "render.material.apply", json!({"material": "Gold - Polished", "faces": [{"point": [0, 0, 5]}]}));
  let d = drawn(&s);
  assert!(d.contains(&(gold.color, None)) && d.contains(&(oak.color, Some(scale))), "{d:?}");
  run(&mut s, "render.material.apply", json!({"body": b}));
  run(&mut s, "render.material.apply", json!({"faces": [{"point": [0, 0, 5]}]}));
  assert_eq!(drawn(&s), plain);
  // undo brings it back
  s.undo();
  assert!(drawn(&s).contains(&(gold.color, None)));
}

/// A face has one appearance, a body one: the one applied last, from either palette, shows.
#[test]
fn the_appearance_applied_last_wins() {
  let mut s = slab();
  let b = s.model.bodies[0].id.0;
  run(&mut s, "render.material.apply", json!({"material": "Wood - Oak", "body": b}));
  run(&mut s, "body.appearance", json!({"body": b, "appearance": "ABS (Red)"}));
  assert!(s.design.render.body_material(fox_model::BodyId(b)).is_none());
  assert_eq!(rendering::body_material(&s, fox_model::BodyId(b)).name, "ABS (Red)");
  run(&mut s, "render.material.apply", json!({"material": "Gold - Polished", "faces": [{"point": [0, 0, 5]}]}));
  run(&mut s, "face.appearance", json!({"faces": [{"point": [0, 0, 5]}], "appearance": "Copper"}));
  assert!(s.design.render.faces.is_empty());
  // the Design palette's Wood - Oak brings its grain too
  run(&mut s, "body.appearance", json!({"body": b, "appearance": "Wood - Oak"}));
  let oak = fox_raytrace::material::find("Wood - Oak").unwrap();
  assert!(drawn(&s).contains(&(oak.color, Some(oak.texture.unwrap().scale))));
}

/// `ui:render:texture-map-controls`: a face wearing a textured material of its own has that
/// texture mapped, not its body's; a face without one maps its body's.
#[test]
fn texture_map_controls_map_a_faces_own_texture() {
  let mut s = slab();
  let b = s.model.bodies[0].id.0;
  let r = run(&mut s, "render.material.apply", json!({"material": "Wood - Walnut", "faces": [{"point": [0, 0, 5]}]}));
  let face = r["faces"][0]["face"].as_u64().unwrap() as u32;
  let body = fox_model::BodyId(b);
  let (t, own) = rendering::face_texture(&s, body, face).unwrap();
  assert!(own && t.scale == 40.0);
  let out = run(&mut s, "render.texture_map", json!({"body": b, "face": face, "scale": 90, "rotation": 15}));
  assert_eq!((out["material"].as_str(), out["face"].as_u64()), (Some("Wood - Walnut"), Some(face as u64)));
  let (t, _) = rendering::face_texture(&s, body, face).unwrap();
  assert_eq!((t.scale, t.rotation), (90.0, 15.0));
  assert!(s.design.render.body_material(body).is_none(), "the body keeps its own look");
  // a face whose appearance (Wood - Oak from the Design palette) brings the texture gets it as
  // its render material
  let r = run(&mut s, "face.appearance", json!({"faces": [{"point": [0, 0, 5]}], "appearance": "Wood - Oak"}));
  let face = r["faces"][0]["face"].as_u64().unwrap() as u32;
  run(&mut s, "render.texture_map", json!({"body": b, "face": face, "scale": 25}));
  assert_eq!(rendering::face_texture(&s, body, face).unwrap().0.scale, 25.0);
  assert!(s.design.render.faces.iter().any(|f| f.material == "Wood - Oak"));
  // taken off, the face has nothing to map: its body has no texture either
  run(&mut s, "face.appearance", json!({"faces": [{"body": b, "face": face}]}));
  assert!(rendering::face_texture(&s, body, face).is_none());
  assert!(s.execute("render.texture_map", &json!({"body": b, "face": face, "scale": 25})).is_err());
}
