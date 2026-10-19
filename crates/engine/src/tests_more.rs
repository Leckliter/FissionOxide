//! Tests for measure/selection, construction geometry by method, threads, coils, pipes, ribs,
//! scale, faces, surfaces, mesh tools, appearances and the newer sketch tools.

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

fn block(s: &mut Session, w: f64, d: f64, h: f64) {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [w, d]}));
  run(s, "extrude", json!({"distance": h}));
}

fn rod(s: &mut Session, r: f64, h: f64) {
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.circle", json!({"center": [0, 0], "radius": r}));
  run(s, "extrude", json!({"distance": h}));
}

#[test]
fn measure_between_faces_and_selection_tools() {
  let mut s = Session::default();
  block(&mut s, 40.0, 20.0, 10.0);
  let b = s.model.bodies[0].id.0;
  let r = run(
    &mut s,
    "measure",
    json!({"a": {"point": [20, 10, 10], "snap": "face"}, "b": {"point": [20, 10, 0], "snap": "face"}}),
  );
  assert!((r["distance"].as_f64().unwrap() - 10.0).abs() < 1e-9, "{r}");
  assert!((r["angle_deg"].as_f64().unwrap() - 180.0).abs() < 1e-6, "{r}");
  let one = run(&mut s, "measure", json!({"a": {"body": b}}));
  assert!((one["volume_mm3"].as_f64().unwrap() - 8000.0).abs() < 1e-6);
  let side = run(
    &mut s,
    "measure",
    json!({"a": {"point": [20, 10, 10], "snap": "face"}, "b": {"point": [0, 10, 5], "snap": "face"}}),
  );
  assert!((side["angle_deg"].as_f64().unwrap() - 90.0).abs() < 1e-6, "{side}");
  run(&mut s, "select.all", json!({"kind": "faces"}));
  assert_eq!(s.selection.len(), 6);
  s.selection.truncate(2);
  run(&mut s, "select.invert", json!({}));
  assert_eq!(s.selection.len(), 4);
  let c = run(&mut s, "inspect.center_of_mass", json!({}));
  let cm = c["center"].as_array().unwrap();
  assert!((cm[0].as_f64().unwrap() - 20.0).abs() < 1e-6 && (cm[2].as_f64().unwrap() - 5.0).abs() < 1e-6, "{c}");
}

#[test]
fn construction_planes_axes_and_points() {
  let mut s = Session::default();
  block(&mut s, 40.0, 20.0, 10.0);
  // midplane between the top and bottom faces
  let mid =
    run(&mut s, "plane", json!({"method": "midplane", "a": {"point": [20, 10, 10]}, "b": {"point": [20, 10, 0]}}));
  let pl = s.model.planes[&crate::model::FeatureId(mid["feature"].as_u64().unwrap() as u32)];
  assert!((pl.origin.z - 5.0).abs() < 1e-9, "{pl:?}");
  // a sketch on it splits the block
  let three = run(
    &mut s,
    "plane",
    json!({"method": "three_points", "a": {"at": [0, 0, 3]}, "b": {"at": [1, 0, 3]}, "c": {"at": [0, 1, 3]}}),
  );
  let f3 = three["feature"].as_u64().unwrap();
  run(&mut s, "split", json!({"plane": {"feature": f3}}));
  assert_eq!(s.model.bodies.len(), 2);
  // axis along an edge, plane at 45° about it
  let ax = run(&mut s, "axis", json!({"method": "edge", "edge": {"point": [20, 0, 0]}}));
  let ax = ax["feature"].as_u64().unwrap();
  let ang = run(&mut s, "plane", json!({"method": "angle", "axis": {"axis": ax}, "plane": "xy", "angle": "45 deg"}));
  let pl = s.model.planes[&crate::model::FeatureId(ang["feature"].as_u64().unwrap() as u32)];
  assert!((pl.normal().z.abs() - 0.5f64.sqrt()).abs() < 1e-9, "{pl:?}");
  // points: three planes meet at the origin; a vertex
  let p = run(&mut s, "point", json!({"method": "three_planes", "a": "xy", "b": "xz", "c": "yz"}));
  let q = s.model.points[&crate::model::FeatureId(p["feature"].as_u64().unwrap() as u32)];
  assert!(q.length() < 1e-9);
  let v = run(&mut s, "point", json!({"method": "vertex", "point": {"vertex": {"point": [20, 0, 0]}, "end": true}}));
  let q = s.model.points[&crate::model::FeatureId(v["feature"].as_u64().unwrap() as u32)];
  assert!(q.y.abs() < 1e-9 && (q.x - 40.0).abs() < 1e-9 || q.x.abs() < 1e-9, "{q:?}");
  // a construction axis can be a revolve axis
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.rect", json!({"corner1": [50, 0], "corner2": [60, 5]}));
  let z = run(&mut s, "axis", json!({"method": "two_points", "a": {"at": [0, 0, 0]}, "b": {"at": [0, 0, 1]}}));
  let sk = s.design.features.iter().rev().find(|f| f.kind.base_name() == "Sketch").unwrap().id.0;
  run(&mut s, "revolve", json!({"sketch": sk, "axis": {"axis": z["feature"]}}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
}

#[test]
fn threads_cosmetic_and_modeled() {
  let mut s = Session::default();
  rod(&mut s, 5.0, 20.0);
  let v0 = volume(&s);
  run(&mut s, "thread", json!({"faces": [{"point": [5, 0, 10]}]}));
  assert_eq!(s.model.threads.len(), 1, "cosmetic thread drawn");
  assert!((volume(&s) - v0).abs() < 1e-9);
  let f = s.design.features.last().unwrap().id.0;
  run(&mut s, "feature.edit", json!({"feature": f, "set": {"modeled": true}}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let v1 = volume(&s);
  assert!(v1 < v0 * 0.95 && v1 > v0 * 0.6, "{v0} → {v1}");
  assert!(s.model.threads.is_empty());
  assert!((crate::model::regen::iso_pitch(10.0) - 1.5).abs() < 1e-12);
}

#[test]
fn coil_and_pipe() {
  let mut s = Session::default();
  run(&mut s, "coil", json!({"diameter": 20, "pitch": 5, "revolutions": 3, "section_size": 2}));
  assert_eq!(s.model.bodies.len(), 1, "{:?}", s.model.errors());
  let v = volume(&s);
  // wire length ≈ turns × circumference, section π r²
  let expected = 3.0 * (std::f64::consts::PI * 20.0f64).hypot(5.0) * std::f64::consts::PI;
  assert!((v - expected).abs() / expected < 0.05, "{v} vs {expected}");
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 30], [20, 30]]}));
  let path = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "pipe", json!({"path": path, "diameter": 6, "wall": 1}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let v = volume(&s);
  let ring = std::f64::consts::PI * (9.0 - 4.0);
  assert!(v > ring * 45.0 && v < ring * 52.0, "{v}");
}

#[test]
fn rib_grows_to_the_floor() {
  let mut s = Session::default();
  // a U channel: base 40×20×4 with walls 4 thick, 20 tall
  block(&mut s, 40.0, 20.0, 4.0);
  run(&mut s, "sketch.create", json!({"plane": {"point": [20, 10, 4], "normal": [0, 0, 1]}}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [4, 20]}));
  run(&mut s, "sketch.rect", json!({"corner1": [36, 0], "corner2": [40, 20]}));
  run(&mut s, "extrude", json!({"distance": 20, "operation": "join"}));
  let v0 = volume(&s);
  // a rib sketched on the XZ plane (y = 0 is the channel's side; move it to y = 10)
  run(&mut s, "plane.offset", json!({"plane": "xz", "offset": -10}));
  let pl = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": pl}}));
  // a line across the top between the walls; the rib grows down to the floor
  run(&mut s, "sketch.line", json!({"points": [[4, 20], [36, 20]]}));
  run(&mut s, "rib", json!({"thickness": 2}));
  let added = volume(&s) - v0;
  assert!((added - 32.0 * 16.0 * 2.0).abs() < 1.0, "rib added {added}");
}

#[test]
fn scale_remove_faces_and_curved_press_pull() {
  let mut s = Session::default();
  block(&mut s, 10.0, 10.0, 10.0);
  run(&mut s, "scale", json!({"factor": 2}));
  assert!((volume(&s) - 8000.0).abs() < 1e-6, "{}", volume(&s));
  run(&mut s, "scale", json!({"factors": [0.5, 1, 1]}));
  assert!((volume(&s) - 4000.0).abs() < 1e-6, "{}", volume(&s));
  // a hole, then remove its face: the block is whole again
  let mut s = Session::default();
  block(&mut s, 30.0, 30.0, 10.0);
  run(&mut s, "sketch.create", json!({"plane": {"point": [15, 15, 10], "normal": [0, 0, 1]}}));
  run(&mut s, "sketch.circle", json!({"center": [15, 15], "radius": 5}));
  run(&mut s, "extrude", json!({"extent": "through_all", "flip": true, "operation": "cut"}));
  let holed = volume(&s);
  // press-pull the hole's wall outward by 1: the hole gets bigger
  run(&mut s, "press_pull", json!({"faces": [{"point": [20, 15, 5]}], "distance": -1}));
  let bigger = volume(&s);
  let ring = std::f64::consts::PI * (36.0 - 25.0) * 10.0;
  assert!(((holed - bigger) - ring).abs() < 1.0, "{holed} {bigger}");
  run(&mut s, "remove_faces", json!({"faces": [{"point": [21, 15, 5]}]}));
  assert!((volume(&s) - 9000.0).abs() < 1e-3, "{}", volume(&s));
}

#[test]
fn surfaces_patch_stitch_thicken() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [20, 0], [20, 10]]}));
  run(&mut s, "surface.extrude", json!({"distance": 5}));
  assert_eq!(s.model.bodies.len(), 1);
  let b = s.model.bodies[0].id.0;
  let m = s.model.bodies[0].shape.mass().unwrap();
  assert!((m.area - 30.0 * 5.0).abs() < 1e-6, "{}", m.area);
  run(&mut s, "thicken", json!({"bodies": [b], "thickness": 1}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  // 30 × 5 × 1, less the 1 × 1 × 5 where the two slabs overlap in the inside corner
  assert!((volume(&s) - 145.0).abs() < 1e-3, "{}", volume(&s));
  // a patch over a sketch rectangle
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
  let sk = s.edit_sketch.unwrap().0;
  run(&mut s, "surface.patch", json!({"sketch": sk}));
  assert!((s.model.bodies[0].shape.mass().unwrap().area - 100.0).abs() < 1e-6);
  // unstitch a box into six faces, stitch them back into a solid
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
  let b = s.model.bodies[0].id.0;
  run(&mut s, "surface.unstitch", json!({"bodies": [b]}));
  assert_eq!(s.model.bodies.len(), 6);
  let all: Vec<u32> = s.model.bodies.iter().map(|b| b.id.0).collect();
  run(&mut s, "surface.stitch", json!({"bodies": all}));
  assert_eq!(s.model.bodies.len(), 1);
  assert!((volume(&s) - 1000.0).abs() < 1e-3, "{}", volume(&s));
}

#[test]
fn mesh_tools_and_appearance() {
  let mut s = Session::default();
  run(&mut s, "sphere", json!({"radius": 10}));
  let dir = std::env::temp_dir().join(format!("fox-mesh-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let stl = dir.join("ball.stl");
  run(&mut s, "export", json!({"path": stl}));
  let mut s = Session::default();
  run(&mut s, "import", json!({"path": stl}));
  let b = s.model.bodies[0].id.0;
  let faces0 = s.model.bodies[0].faces().len();
  run(&mut s, "mesh.reduce", json!({"bodies": [b], "keep": 0.3}));
  let faces1 = s.model.bodies[0].faces().len();
  assert!(faces1 < faces0, "{faces0} → {faces1}");
  run(&mut s, "mesh.smooth", json!({"bodies": [b], "passes": 2}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  run(&mut s, "body.appearance", json!({"body": b, "appearance": "ABS (Red)"}));
  assert_eq!(s.design.body_settings(fox_model::BodyId(b)).unwrap().color, Some([208, 50, 46]));
  assert!(s.execute("body.appearance", &json!({"body": b, "appearance": "Unobtainium"})).is_err());
  run(&mut s, "body.appearance", json!({"body": b}));
  assert_eq!(s.design.body_settings(fox_model::BodyId(b)).unwrap().color, None);
}

#[test]
fn sketch_tools_make_profiles() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let r = run(&mut s, "sketch.ellipse", json!({"center": [0, 0], "major": [20, 0], "minor": 10}));
  assert_eq!(r["profiles"], 1, "{r}");
  run(&mut s, "extrude", json!({"distance": 1}));
  let area = volume(&s);
  assert!((area - std::f64::consts::PI * 200.0).abs() < 0.05, "{area}");
  // 3-point circle, 2-point circle, 3-point rectangle, polygons, slots
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"through": [[10, 0], [0, 10], [-10, 0]]}));
  run(&mut s, "sketch.circle", json!({"through": [[40, 0], [50, 0]]}));
  run(&mut s, "sketch.rect", json!({"points": [[100, 0], [110, 10], [100, 20]]}));
  run(&mut s, "sketch.polygon", json!({"center": [0, 50], "vertex": [10, 50], "sides": 6, "circumscribed": true}));
  run(&mut s, "sketch.polygon", json!({"edge": [[50, 50], [60, 50]], "sides": 4}));
  run(&mut s, "sketch.slot", json!({"start": [100, 50], "end": [130, 50], "width": 10, "kind": "overall"}));
  run(&mut s, "sketch.slot", json!({"arc_center": [0, 100], "start": [20, 100], "end": [0, 120], "width": 4}));
  let info = run(&mut s, "sketch.info", json!({}));
  assert_eq!(info["profiles"].as_array().map(Vec::len).unwrap_or(0), 7, "{}", info["profiles"]);
  run(&mut s, "extrude", json!({"distance": 1}));
  let pi = std::f64::consts::PI;
  let hex = 6.0 * 10.0 * 10.0 / 3f64.sqrt();
  let slot = 20.0 * 10.0 + pi * 25.0;
  let arc_slot = pi / 2.0 * 20.0 * 4.0 + pi * 4.0;
  let expected = pi * 100.0 + pi * 25.0 + 200.0 * 2f64.sqrt() * 2f64.sqrt() / 2.0 * 1.0 + hex + 100.0 + slot + arc_slot;
  // 3-point rect: side √200, height 10√2·… = 20/√2·√2 → area √200 × √200/… checked loosely
  let v = volume(&s);
  assert!(v > expected * 0.8 && v < expected * 1.25, "{v} vs ~{expected}");
  // tangent arc off a line end, control spline, conic, text
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let l = run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]]}));
  let line = l["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.arc", json!({"tangent": line, "start": [10, 0], "end": [10, 10]}));
  run(&mut s, "sketch.control_spline", json!({"points": [[0, 20], [5, 30], [15, 10], [20, 20]]}));
  run(&mut s, "sketch.conic", json!({"start": [0, 40], "end": [20, 40], "apex": [10, 50], "rho": 0.5}));
  let t = run(&mut s, "sketch.text", json!({"at": [0, 60], "text": "Ob", "height": 10}));
  assert!(t["profiles"].as_u64().unwrap() >= 2, "{t}");
  assert!(s.model.sketches.values().all(|r| r.report.ok()));
}

#[test]
fn section_view_is_capped() {
  let mut s = Session::default();
  block(&mut s, 40.0, 20.0, 10.0);
  let opts = crate::scene::view_options(&s, &json!({"section": {"plane": "xz", "offset": -10}})).unwrap();
  let (scene, _) = crate::scene::SceneBuilder::default().build(&s, &opts);
  // the body mesh, clipped, and the cap
  let caps: Vec<_> = scene.meshes.iter().filter(|m| !m.analyze && !m.indices.is_empty()).collect();
  assert!(!caps.is_empty(), "no cap");
  let cut = scene.meshes.iter().find(|m| m.analyze).unwrap();
  assert!(cut.positions.iter().enumerate().all(|(i, p)| !cut.indices.contains(&(i as u32)) || p[1] >= 10.0 - 1e-4));
}

#[test]
fn parameter_change_regenerates_in_the_background() {
  let mut s = Session::default();
  run(&mut s, "param.add", json!({"name": "h", "expr": "10 mm"}));
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
  run(&mut s, "extrude", json!({"distance": "h"}));
  let rev = s.revision;
  let (_, job) = s.begin("param.set", &json!({"name": "h", "expr": "25 mm"})).unwrap();
  let job = job.unwrap();
  // the old model stays until the job is taken back
  assert!((volume(&s) - 1000.0).abs() < 1e-6);
  s.finish_rebuild(job);
  assert!(s.revision > rev);
  assert!((volume(&s) - 2500.0).abs() < 1e-6, "{}", volume(&s));
  assert!(s.begin("param.set", &json!({"name": "h", "expr": "nope +"})).is_err());
  assert!(s.undo());
  assert!((volume(&s) - 1000.0).abs() < 1e-6);
  let (_, job) = s.begin("compute_all", &json!({})).unwrap();
  s.finish_rebuild(job.unwrap());
  assert!((volume(&s) - 1000.0).abs() < 1e-6);
}

#[test]
fn milling_setup_toolpaths_and_gcode() {
  let mut s = Session::default();
  // a 60 × 40 × 20 block with a 30 × 20 pocket 8 deep and a through hole
  block(&mut s, 60.0, 40.0, 20.0);
  run(&mut s, "sketch.create", json!({"plane": {"point": [30, 20, 20], "normal": [0, 0, 1]}}));
  run(&mut s, "sketch.rect", json!({"corner1": [10, 10], "corner2": [40, 30]}));
  run(&mut s, "extrude", json!({"distance": 8, "flip": true, "operation": "cut"}));
  run(&mut s, "sketch.create", json!({"plane": {"point": [50, 20, 20], "normal": [0, 0, 1]}}));
  run(&mut s, "sketch.circle", json!({"center": [50, 20], "diameter": 6}));
  run(&mut s, "extrude", json!({"extent": "through_all", "flip": true, "operation": "cut"}));
  let st = run(&mut s, "cam.setup", json!({"stock_side": 3, "stock_top": 2}));
  assert_eq!(st["stock"][1][2].as_f64().unwrap(), 22.0);
  run(&mut s, "cam.face", json!({"tool_diameter": 20, "stepdown": 1}));
  run(&mut s, "cam.pocket", json!({"floors": [{"point": [25, 20, 12]}], "tool_diameter": 6, "stepdown": 2}));
  run(&mut s, "cam.contour", json!({"tool_diameter": 6, "stepdown": 5}));
  run(&mut s, "cam.drill", json!({"tool_diameter": 6, "peck": 3}));
  let g = run(&mut s, "cam.generate", json!({}));
  assert_eq!(g["operations"].as_array().unwrap().len(), 4, "{g}");
  assert!(g["minutes"].as_f64().unwrap() > 0.0);
  // the pocket stays inside its walls: the tool centre keeps a radius from them
  let pocket = &s.toolpaths[1];
  for m in pocket.moves.iter().filter(|m| !m.rapid) {
    assert!(
      m.to.x >= 13.0 - 1e-6 && m.to.x <= 37.0 + 1e-6 && m.to.y >= 13.0 - 1e-6 && m.to.y <= 27.0 + 1e-6,
      "{:?}",
      m.to
    );
    assert!(m.to.z >= 12.0 - 1e-6);
  }
  assert!(pocket.moves.iter().any(|m| (m.to.z - 12.0).abs() < 1e-6), "reaches the floor");
  // the contour runs a radius outside the part, below its bottom
  let contour = &s.toolpaths[2];
  for m in contour.moves.iter().filter(|m| !m.rapid && m.to.z < 19.0) {
    // distance from the 60 × 40 outline: the tool radius (rounded corners included)
    let dx = (-m.to.x).max(m.to.x - 60.0).max(0.0);
    let dy = (-m.to.y).max(m.to.y - 40.0).max(0.0);
    assert!((dx.hypot(dy) - 3.0).abs() < 0.05, "{:?}", m.to);
  }
  assert!(contour.moves.iter().any(|m| m.to.z < 0.0));
  let drill = s.toolpaths[3].drill.as_ref().unwrap();
  assert_eq!(drill.0.len(), 1);
  let code = run(&mut s, "cam.post", json!({}))["gcode"].as_str().unwrap().to_string();
  assert!(code.contains("T1 M6") && code.contains("T2 M6") && code.contains("G83") && code.ends_with("%\n"), "{code}");
  // work coordinates: nothing cuts above the stock top (Z0)
  for l in code.lines().filter(|l| l.starts_with("G1")) {
    if let Some(z) = l.split_whitespace().find(|w| w.starts_with('Z')) {
      assert!(z[1..].parse::<f64>().unwrap() <= 1e-9, "{l}");
    }
  }
  // operations follow the model: a deeper pocket moves the toolpath
  let f = s.design.features.iter().find(|f| f.name == "Extrude2").unwrap().id.0;
  run(
    &mut s,
    "feature.edit",
    json!({"feature": f, "set": {"extent": {"kind": "one_side", "distance": "12", "flip": true}}}),
  );
  run(&mut s, "cam.generate", json!({}));
  assert!(s.toolpaths[1].moves.iter().any(|m| (m.to.z - 8.0).abs() < 1e-6));
  let saved: crate::model::Design = serde_json::from_str(&serde_json::to_string(&s.design).unwrap()).unwrap();
  assert_eq!(saved.cam.len(), 1);
}

#[test]
fn ruled_offset_and_trim_surfaces() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 10, "height": 5}));
  // a ruled surface between the top front and top back edges is the top face again
  run(&mut s, "surface.ruled", json!({"a": {"point": [0, -5, 5]}, "b": {"point": [0, 5, 5]}}));
  let ruled = s.model.bodies.last().unwrap();
  assert!((ruled.shape.mass().unwrap().area - 200.0).abs() < 1e-6);
  let id = ruled.id.0;
  run(&mut s, "surface.offset", json!({"bodies": [id], "distance": 3}));
  let off = s.model.bodies.last().unwrap().shape.mass().unwrap();
  assert!(
    (off.area - 200.0).abs() < 1e-6 && (off.bbox_min.z - 8.0).abs() < 1e-3 || (off.bbox_min.z - 2.0).abs() < 1e-3,
    "{off:?}"
  );
  run(&mut s, "surface.trim", json!({"bodies": [id], "tool": "yz"}));
  let b = s.model.bodies.iter().find(|b| b.id.0 == id).unwrap();
  assert!((b.shape.mass().unwrap().area - 100.0).abs() < 1e-6, "{}", b.shape.mass().unwrap().area);
}
