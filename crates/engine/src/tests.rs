use super::*;
use serde_json::json;

fn run(s: &mut Session, cmd: &str, p: serde_json::Value) -> serde_json::Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn total_volume(s: &Session) -> f64 {
  s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
}

/// Builds the classic first Fusion part: a plate with a hole and rounded corners.
fn plate(s: &mut Session) {
  run(s, "param.add", json!({"name": "thick", "expr": "6 mm"}));
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 50]}));
  let info = run(s, "sketch.info", json!({}));
  assert_eq!(info["dof"], 2, "corner on the origin snaps coincident: {info}");
  run(s, "extrude", json!({"distance": "thick"}));
  assert_eq!(s.model.bodies.len(), 1);
  let b = &s.model.bodies[0];
  let top = b.faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  let body = b.id.0;
  run(s, "sketch.create", json!({"plane": {"body": body, "face": top}}));
  run(s, "sketch.circle", json!({"center": [40, 25], "diameter": 12}));
  run(s, "extrude", json!({"extent": "through_all", "flip": true, "operation": "cut"}));
}

#[test]
fn build_a_plate_with_commands() {
  let mut s = Session::default();
  plate(&mut s);
  let expected = 80.0 * 50.0 * 6.0 - std::f64::consts::PI * 36.0 * 6.0;
  assert!((total_volume(&s) - expected).abs() < 1e-3, "{}", total_volume(&s));
  // round the four vertical corner edges
  let b = &s.model.bodies[0];
  let edges: Vec<_> = b
    .edges()
    .iter()
    .filter(|e| {
      e.kind == clf_kernel::CurveKind::Line && e.faces[1] != 0 && (e.end - e.start).normalize().z.abs() > 0.99
    })
    .map(|e| json!({"body": b.id.0, "edge": e.id}))
    .collect();
  assert_eq!(edges.len(), 4);
  let r = run(&mut s, "fillet", json!({"edges": edges, "radius": 5}));
  assert!(r["error"].is_null(), "{r}");
  let v_fillet = total_volume(&s);
  assert!(v_fillet < expected);
  // parameters drive everything
  run(&mut s, "param.set", json!({"name": "thick", "expr": "10"}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!(total_volume(&s) > v_fillet * 1.5);
  // undo / redo
  assert!(s.undo());
  assert!((total_volume(&s) - v_fillet).abs() < 1e-6);
  assert!(s.redo());
  let info = run(&mut s, "info", json!({}));
  assert_eq!(info["timeline"].as_array().unwrap().len(), 5);
  assert_eq!(info["bodies"].as_array().unwrap().len(), 1);
}

#[test]
fn export_save_reopen() {
  let mut s = Session::default();
  plate(&mut s);
  let dir = std::env::temp_dir().join(format!("clf-engine-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  for ext in ["step", "stl", "obj"] {
    let path = dir.join(format!("plate.{ext}"));
    run(&mut s, "export", json!({"path": path}));
    assert!(std::fs::metadata(&path).unwrap().len() > 100);
  }
  assert!(s.execute("export", &json!({"path": dir.join("x.dwg")})).is_err());
  let file = dir.join("plate.clf");
  run(&mut s, "save", json!({"path": file}));
  let v = total_volume(&s);
  let mut t = Session::default();
  run(&mut t, "open", json!({"path": file}));
  assert!((total_volume(&t) - v).abs() < 1e-6);
  run(&mut t, "import", json!({"path": dir.join("plate.step")}));
  assert_eq!(t.model.bodies.len(), 2);
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sketch_constraints_and_dimensions() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  let r = run(&mut s, "sketch.line", json!({"points": [[0, 0], [30, 0], [30, 20], [0, 20]], "closed": true}));
  let curves: Vec<u64> = r["curves"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
  assert_eq!(curves.len(), 4);
  assert_eq!(r["profiles"], 1);
  let info = run(&mut s, "sketch.info", json!({}));
  assert_eq!(info["dof"], 2, "{info}");
  let d = run(&mut s, "sketch.dimension", json!({"type": "distance", "curves": [curves[0]], "value": "45"}));
  let cid = d["constraint"].as_u64().unwrap();
  run(&mut s, "sketch.dimension", json!({"type": "distance", "curves": [curves[1]], "value": 25}));
  let info = run(&mut s, "sketch.info", json!({}));
  assert_eq!(info["dof"], 0, "{info}");
  run(&mut s, "sketch.set_dimension", json!({"constraint": cid, "value": "60"}));
  // conflicting constraint is rejected and leaves the sketch unchanged
  assert!(s.execute("sketch.constrain", &json!({"type": "vertical", "curves": [curves[0]]})).is_err());
  run(&mut s, "extrude", json!({"distance": 10, "extent": "symmetric"}));
  let expected = 60.0 * 25.0 * 20.0;
  assert!((total_volume(&s) - expected).abs() < 1e-6, "{}", total_volume(&s));
}

#[test]
fn revolve_patterns_and_feature_edits() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.rect", json!({"corner1": [20, 0], "corner2": [25, 8]}));
  let r = run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 10]]}));
  let axis = r["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.construction", json!({"curves": [axis]}));
  let r = run(&mut s, "revolve", json!({"axis": {"line": axis}, "angle": 90}));
  let rev = r["feature"].as_u64().unwrap();
  let quarter = total_volume(&s);
  run(&mut s, "feature.edit", json!({"feature": rev, "set": {"angle": "360 deg"}}));
  assert!((total_volume(&s) - 4.0 * quarter).abs() < 1e-3);
  run(&mut s, "cylinder", json!({"at": [40, 0], "radius": 3, "height": 8}));
  let cyl_body = s.model.bodies.last().unwrap().id.0;
  run(&mut s, "pattern.circular", json!({"bodies": [cyl_body], "axis": "z", "count": 4, "join": false}));
  assert_eq!(s.model.bodies.len(), 5);
  let r = run(&mut s, "feature.suppress", json!({"feature": rev}));
  assert!(r.is_object());
  assert_eq!(s.model.bodies.len(), 4);
  assert!(s.execute("nonsense", &json!({})).is_err());
}

#[test]
fn hostile_params_never_panic() {
  let mut s = Session::default();
  let junk = [
    json!(null),
    json!({}),
    json!({"points": "x"}),
    json!({"points": [[1e308, -1e308], [f64::MAX, 0]]}),
    json!({"distance": "1/0"}),
    json!({"sketch": 4294967295u64}),
    json!({"edges": [{"body": 1, "edge": 1}], "radius": -1}),
    json!({"count": 1e9, "direction": [0, 0, 0], "spacing": 1}),
    json!({"path": "/nonexistent/dir/file.step"}),
    json!({"feature": 1, "set": {"type": "sketch", "plane": 5}}),
    json!({"items": [{"type": "nope"}]}),
  ];
  let names: Vec<&str> = COMMANDS.iter().map(|c| c.id).filter(|c| !matches!(*c, "new" | "open")).collect();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
  run(&mut s, "extrude", json!({"distance": 5}));
  for n in &names {
    for p in &junk {
      let _ = s.execute(n, p);
    }
  }
  // still usable
  run(&mut s, "info", json!({}));
}

#[test]
fn scene_builds_and_picks() {
  let mut s = Session::default();
  plate(&mut s);
  let mut sb = scene::SceneBuilder::default();
  let (scene, picks) = sb.build(&s, &scene::ViewOptions::default());
  let mut cam = render::Camera::default();
  let (lo, hi) = s.model.bounds().unwrap();
  cam.fit(lo, hi, 4.0 / 3.0);
  let frame = render::render(&scene, &cam, 400, 300);
  let id = frame.pick(200.0, 150.0, 3, |_| true);
  assert!(matches!(picks.get(id), Some(scene::Pick::Face { .. } | scene::Pick::Edge { .. })), "{:?}", picks.get(id));
  if let Ok(dir) = std::env::var("CLF_SNAPSHOT_DIR") {
    frame.save_png(&std::path::Path::new(&dir).join("plate.png")).unwrap();
  }
}

#[test]
fn sheet_metal_with_commands() {
  let mut s = Session::default();
  run(&mut s, "sheet.rule", json!({"name": "Alu", "thickness": 1.5, "bend_radius": 2, "k_factor": 0.4}));
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 40]}));
  let r = run(&mut s, "sheet.flange", json!({}));
  assert!(r["error"].is_null(), "{r}");
  let r = run(
    &mut s,
    "sheet.edge_flange",
    json!({"edges": [{"point": [40, 40, 1.5]}, {"point": [40, 0, 1.5]}], "height": 15}),
  );
  assert!(r["error"].is_null(), "{r}");
  let info = run(&mut s, "info", json!({}));
  assert_eq!(info["bodies"][0]["sheet_metal"]["thickness"], 1.5, "{info}");
  let dxf = std::env::temp_dir().join(format!("clf-flat-{}.dxf", std::process::id()));
  let flat = run(&mut s, "sheet.flat_pattern", json!({"path": dxf}));
  assert_eq!(flat["bends"].as_array().unwrap().len(), 2, "{flat}");
  let ba = std::f64::consts::PI / 2.0 * (2.0 + 0.4 * 1.5);
  let width = 40.0 + 2.0 * (ba + 15.0 - 3.5);
  let size = flat["size"].as_array().unwrap();
  let (a, b) = (size[0].as_f64().unwrap(), size[1].as_f64().unwrap());
  assert!((a - 80.0).abs() < 1e-3 && (b - width).abs() < 1e-3, "{flat} {width}");
  let text = std::fs::read_to_string(&dxf).unwrap();
  let _ = std::fs::remove_file(&dxf);
  assert!(text.contains("BEND_") && text.contains("OUTLINE"));
  // unfold in the timeline; the rule still drives the folded part underneath
  let r = run(&mut s, "sheet.unfold", json!({}));
  assert!(r["error"].is_null(), "{r}");
  assert!(s.model.bodies[0].sheet.as_ref().unwrap().flat);
  run(&mut s, "sheet.rule", json!({"name": "Alu", "thickness": 2}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!(s.execute("sheet.rule_remove", &json!({"name": "Alu"})).is_err(), "rule in use");
}

#[test]
fn loft_sweep_hole_split_draft() {
  use std::f64::consts::PI;
  let mut s = Session::default();
  // loft: 40 mm square at z = 0 to a 20 mm circle at z = 30
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"center": [0, 0], "corner": [20, 20]}));
  let a = s.edit_sketch.unwrap().0;
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 30}));
  let plane = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": plane}}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
  let b = s.edit_sketch.unwrap().0;
  let r = run(&mut s, "loft", json!({"sections": [a, b]}));
  assert!(r["error"].is_null(), "{r}");
  let v = total_volume(&s);
  assert!(v > PI * 100.0 * 30.0 && v < 1600.0 * 30.0, "{v}");
  // split it at z = 10
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 10}));
  let mid = s.design.features.last().unwrap().id.0;
  let r = run(&mut s, "split", json!({"plane": {"feature": mid}}));
  assert!(r["error"].is_null() && s.model.bodies.len() == 2, "{r}");
  assert!((total_volume(&s) - v).abs() < 1e-3);

  // sweep a circle along an L path, then drill a counterbored hole in a plate
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 30]]}));
  run(&mut s, "sketch.arc", json!({"center": [10, 30], "start": [10, 40], "end": [0, 30]}));
  run(&mut s, "sketch.line", json!({"points": [[10, 40], [30, 40]]}));
  let path = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 3}));
  let r = run(&mut s, "sweep", json!({"path": path}));
  assert!(r["error"].is_null(), "{r}");
  let v = total_volume(&s);
  let len = 30.0 + PI / 2.0 * 10.0 + 20.0;
  assert!((v - PI * 9.0 * len).abs() / v < 1e-3, "{v}");

  let mut s = Session::default();
  plate(&mut s);
  let before = total_volume(&s);
  let b = s.model.bodies[0].id.0;
  let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  run(&mut s, "sketch.create", json!({"plane": {"body": b, "face": top}}));
  run(&mut s, "sketch.point", json!({"at": [15, 15]}));
  run(&mut s, "sketch.point", json!({"at": [65, 15]}));
  let r = run(
    &mut s,
    "hole",
    json!({"diameter": 6, "through_all": true, "kind": "counterbore", "head_diameter": 10, "head_depth": 2}),
  );
  assert!(r["error"].is_null(), "{r}");
  let removed = 2.0 * (PI * 9.0 * 6.0 + PI * (25.0 - 9.0) * 2.0);
  assert!((before - total_volume(&s) - removed).abs() < 1e-2, "{} vs {removed}", before - total_volume(&s));

  // draft the four sides of a box by 5° about its base
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 40, "depth": 40, "height": 20}));
  let b = &s.model.bodies[0];
  let sides: Vec<_> =
    b.faces().iter().filter(|f| f.normal.z.abs() < 0.1).map(|f| json!({"body": b.id.0, "face": f.id})).collect();
  let r = run(&mut s, "draft", json!({"faces": sides, "plane": "xy", "angle": "5 deg"}));
  assert!(r["error"].is_null(), "{r}");
  let v = total_volume(&s);
  assert!((v - 40.0 * 40.0 * 20.0).abs() > 100.0, "{v}");
}

#[test]
fn every_listed_command_is_handled() {
  for c in COMMANDS {
    let mut s = Session::default();
    if let Err(e) = s.execute(c.id, &json!({})) {
      assert!(!e.0.starts_with("unknown command"), "{}: {e}", c.id);
    }
  }
}

#[test]
fn assembly_with_a_revolute_joint() {
  let mut s = Session::default();
  // base plate with a hole at (50, 30); an arm (somewhere else) with a hole near one end
  run(&mut s, "box", json!({"width": 100, "depth": 60, "height": 10, "at": [50, 30]}));
  run(&mut s, "cylinder", json!({"radius": 5, "height": 10, "at": [50, 30], "operation": "cut"}));
  run(&mut s, "box", json!({"width": 80, "depth": 20, "height": 5, "at": [40, 200]}));
  run(&mut s, "cylinder", json!({"radius": 5, "height": 5, "at": [10, 200], "operation": "cut"}));
  let (base, arm) = (s.model.bodies[0].id.0, s.model.bodies[1].id.0);
  let b = run(&mut s, "component.create", json!({"name": "Base", "bodies": [base]}))["component"].clone();
  run(&mut s, "component.ground", json!({"component": b}));
  let a = run(&mut s, "component.create", json!({"name": "Arm", "bodies": [arm]}))["component"].clone();
  run(&mut s, "component.material", json!({"component": a, "material": "aluminum"}));
  let r = run(
    &mut s,
    "joint",
    json!({
      "moving": a, "moving_origin": {"edge": {"point": [15, 200, 0]}},
      "fixed": b, "fixed_origin": {"edge": {"point": [55, 30, 10]}},
      "kind": "revolute",
    }),
  );
  assert!(r["error"].is_null(), "{r}");
  let j = r["feature"].as_u64().unwrap();
  let bbox = |s: &Session| {
    let m = s.model.bodies.iter().find(|x| x.id.0 == arm).unwrap().shape.mass().unwrap();
    (m.bbox_min, m.bbox_max)
  };
  let near = |a: clf_kernel::glam::DVec3, b: [f64; 3]| (a - clf_kernel::glam::DVec3::from(b)).length() < 1e-3;
  let (lo, hi) = bbox(&s);
  assert!(near(lo, [40.0, 20.0, 10.0]) && near(hi, [120.0, 40.0, 15.0]), "{lo:?} {hi:?}");
  run(&mut s, "joint.drive", json!({"joint": j, "rotation": "90 deg"}));
  let (lo, hi) = bbox(&s);
  assert!(near(lo, [40.0, 20.0, 10.0]) && near(hi, [60.0, 100.0, 15.0]), "{lo:?} {hi:?}");
  assert!(run(&mut s, "assembly.interference", json!({})).as_array().unwrap().is_empty());
  // limits are checked on every rebuild
  run(&mut s, "feature.edit", json!({"feature": j, "set": {"limits": {"rotation": ["-45 deg", "45 deg"]}}}));
  assert!(s.model.errors().iter().any(|e| e.contains("outside the joint's limits")), "{:?}", s.model.errors());
  run(&mut s, "joint.drive", json!({"joint": j, "rotation": 30}));
  assert!(s.model.errors().is_empty());
  let study = run(&mut s, "motion.study", json!({"joint": j, "from": "-45 deg", "to": "45 deg", "steps": 3}));
  assert_eq!(study.as_array().unwrap().len(), 4);
  assert!(study.as_array().unwrap().iter().all(|st| st["error"].is_null() && st["interference"] == json!([])));
  // the grounded base can't be moved by a joint; a clash shows up as interference
  let bad =
    s.execute("joint", &json!({"moving": b, "moving_origin": {"at": [0, 0, 0]}, "fixed_origin": {"at": [0, 0, 5]}}));
  assert!(bad.unwrap()["error"].as_str().unwrap().contains("grounded"));
  s.undo();
  run(&mut s, "joint.drive", json!({"joint": j, "slide": 0, "rotation": 0}));
  run(&mut s, "feature.edit", json!({"feature": j, "set": {"offset": "-3"}}));
  let clash = run(&mut s, "assembly.interference", json!({}));
  assert_eq!(clash.as_array().unwrap().len(), 1, "{clash}");
  let bom = run(&mut s, "assembly.bom", json!({}));
  let arm_row = bom.as_array().unwrap().iter().find(|r| r["name"] == "Arm").unwrap();
  let v = 80.0 * 20.0 * 5.0 - std::f64::consts::PI * 25.0 * 5.0;
  assert!((arm_row["mass_g"].as_f64().unwrap() - v * 2.7 / 1000.0).abs() < 1e-3, "{bom}");
  assert_eq!(arm_row["material"], "Aluminum");
}

#[test]
fn drawings_with_views_dimensions_and_export() {
  let mut s = Session::default();
  plate(&mut s);
  let r = run(&mut s, "drawing.create", json!({"name": "Plate", "sheet": "a3"}));
  assert!(r["error"].is_null(), "{r}");
  let text: Vec<String> = serde_json::from_value(r["text"].clone()).unwrap();
  for want in ["80", "50", "6", "Ø12", "FRONT", "TOP", "RIGHT", "ISOMETRIC", "Plate"] {
    assert!(text.iter().any(|t| t == want), "missing {want}: {text:?}");
  }
  run(&mut s, "drawing.add_view", json!({"kind": {"section": "y", "offset": 25}}));
  run(&mut s, "drawing.dimension", json!({"view": 1, "a": [0, 0, 0], "b": [40, 0, 6], "orientation": "horizontal"}));
  run(&mut s, "drawing.title", json!({"part_number": "CLF-001", "material": "Aluminum"}));
  run(&mut s, "drawing.set", json!({"scale": "1:1", "projection": "first_angle"}));
  let info = run(&mut s, "drawing.info", json!({}));
  assert!(info["error"].is_null(), "{info}");
  let text: Vec<String> = serde_json::from_value(info["text"].clone()).unwrap();
  assert!(
    text.iter().any(|t| t == "SECTION E-E") && text.iter().any(|t| t == "1:1") && text.iter().any(|t| t == "40"),
    "{text:?}"
  );
  let dir = std::env::temp_dir().join(format!("clf-drawing-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  for ext in ["svg", "pdf", "dxf"] {
    let path = dir.join(format!("plate.{ext}"));
    run(&mut s, "drawing.export", json!({"path": path}));
    assert!(std::fs::metadata(&path).unwrap().len() > 1000, "{ext}");
  }
  let _ = std::fs::remove_dir_all(&dir);
  // drawings save with the design and follow parameter changes
  run(&mut s, "param.set", json!({"name": "thick", "expr": "9"}));
  let text: Vec<String> = serde_json::from_value(run(&mut s, "drawing.info", json!({}))["text"].clone()).unwrap();
  assert!(text.iter().any(|t| t == "9"), "{text:?}");
  let back = clf_model::Design::from_json(&s.design.to_json()).unwrap();
  assert_eq!(back.drawings, s.design.drawings);
}

#[test]
fn sketch_modify_tools_and_project() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let rect = run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]}))["curves"].clone();
  let c: Vec<u64> = serde_json::from_value(rect).unwrap();
  run(&mut s, "sketch.fillet", json!({"curves": [c[0], c[1]], "radius": 4}));
  let off = run(&mut s, "sketch.offset", json!({"curves": [c[2], c[3]], "distance": 3}));
  assert_eq!(off["curves"].as_array().unwrap().len(), 2, "{off}");
  let before = run(&mut s, "sketch.info", json!({}))["curves"].as_array().unwrap().len();
  let first = off["curves"][0].clone();
  run(&mut s, "sketch.trim", json!({"curve": first, "at": [20, 23]}));
  let info = run(&mut s, "sketch.info", json!({}));
  assert!(info["curves"].as_array().unwrap().len() <= before, "{info}");
  assert!(info["solved"].as_bool().unwrap(), "{info}");
  run(&mut s, "sketch.finish", json!({}));
  // project a box's top face into a sketch above it and extrude the outline
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 30, "depth": 20, "height": 10}));
  let b = s.model.bodies[0].id.0;
  let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 25}));
  let pl = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": pl}}));
  let r = run(&mut s, "sketch.project", json!({"body": b, "face": top}));
  assert_eq!(r["curves"].as_array().unwrap().len(), 4, "{r}");
  assert_eq!(r["profiles"], 1);
  let r = run(&mut s, "extrude", json!({"distance": 5}));
  assert!(r["error"].is_null(), "{r}");
  assert!((total_volume(&s) - 30.0 * 20.0 * 15.0).abs() < 1e-6);
}
