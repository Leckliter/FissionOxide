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
      e.kind == fox_kernel::CurveKind::Line && e.faces[1] != 0 && (e.end - e.start).normalize().z.abs() > 0.99
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
  let dir = std::env::temp_dir().join(format!("fox-engine-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  for ext in ["step", "stl", "obj"] {
    let path = dir.join(format!("plate.{ext}"));
    run(&mut s, "export", json!({"path": path}));
    assert!(std::fs::metadata(&path).unwrap().len() > 100);
  }
  assert!(s.execute("export", &json!({"path": dir.join("x.dwg")})).is_err());
  let file = dir.join("plate.fox");
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
  // hub commands must never touch the real data directory
  s.store_root = Some(temp_dir("hostile"));
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
  let _ = std::fs::remove_dir_all(s.store_root.take().unwrap());
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
  if let Ok(dir) = std::env::var("FOX_SNAPSHOT_DIR") {
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
  let dxf = std::env::temp_dir().join(format!("fox-flat-{}.dxf", std::process::id()));
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
  let near = |a: fox_kernel::glam::DVec3, b: [f64; 3]| (a - fox_kernel::glam::DVec3::from(b)).length() < 1e-3;
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
  let dwg = run(&mut s, "drawing.create", json!({"views": ["front", "top"]}));
  let text: Vec<String> = serde_json::from_value(dwg["text"].clone()).unwrap();
  for want in ["ITEM", "QTY", "Base", "Arm", "Aluminum"] {
    assert!(text.iter().any(|t| t == want), "parts list missing {want}: {text:?}");
  }
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
  let det =
    run(&mut s, "drawing.add_view", json!({"kind": {"detail": 2, "center": [40, 25, 6], "radius": 10, "factor": 3}}));
  assert!(det["view"].as_u64().is_some(), "{det}");
  run(&mut s, "drawing.dimension", json!({"view": 1, "a": [0, 0, 0], "b": [40, 0, 6], "orientation": "horizontal"}));
  run(&mut s, "drawing.title", json!({"part_number": "FOX-001", "material": "Aluminum"}));
  run(&mut s, "drawing.set", json!({"scale": "1:1", "projection": "first_angle"}));
  let info = run(&mut s, "drawing.info", json!({}));
  assert!(info["error"].is_null(), "{info}");
  let text: Vec<String> = serde_json::from_value(info["text"].clone()).unwrap();
  assert!(
    text.iter().any(|t| t == "SECTION E-E") && text.iter().any(|t| t == "1:1") && text.iter().any(|t| t == "40"),
    "{text:?}"
  );
  let dir = std::env::temp_dir().join(format!("fox-drawing-{}", std::process::id()));
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
  let back = fox_model::Design::from_json(&s.design.to_json()).unwrap();
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
  // the projection follows the box: make it wider and the extrusion above follows
  let bx = s.design.features[0].id.0;
  run(
    &mut s,
    "feature.edit",
    json!({"feature": bx, "set": {"shape": {"type": "box", "width": "50", "depth": "20", "height": "10"}}}),
  );
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  assert!((total_volume(&s) - 50.0 * 20.0 * 15.0).abs() < 1e-6, "{}", total_volume(&s));
}

fn temp_dir(name: &str) -> std::path::PathBuf {
  let d = std::env::temp_dir().join(format!("fox-engine-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&d);
  std::fs::create_dir_all(&d).unwrap();
  d
}

#[test]
fn every_format_round_trips() {
  let mut s = Session::default();
  plate(&mut s);
  let v = total_volume(&s);
  let dir = temp_dir("formats");
  // exact formats come back exact; meshes come back faceted (a little smaller than the curved hole)
  for (ext, tol) in [
    ("step", 1e-3),
    ("iges", 0.5),
    ("brep", 1e-6),
    ("stl", 5.0),
    ("obj", 5.0),
    ("3mf", 5.0),
    ("ply", 5.0),
    ("fox", 1e-6),
    ("foxz", 1e-6),
  ] {
    let path = dir.join(format!("plate.{ext}"));
    let r = run(&mut s, "export", json!({"path": path}));
    assert!(std::fs::metadata(&path).unwrap().len() > 100, "{ext}: {r}");
    // open as a new design
    let mut t = Session::default();
    let r = run(&mut t, "open", json!({"path": path}));
    assert_eq!(t.model.bodies.len(), 1, "{ext}: {r}");
    assert!(t.model.bodies[0].shape.solid_count() == 1, "{ext} opens as a solid");
    assert!((total_volume(&t) - v).abs() < tol, "{ext}: {} vs {v}", total_volume(&t));
    assert!(t.model.errors().is_empty(), "{ext}: {:?}", t.model.errors());
    // native designs keep their path, translations start unsaved
    assert_eq!(t.path.is_some(), ext == "fox" || ext == "foxz", "{ext}");
  }
  // the parametric timeline survives the archive
  let mut t = Session::default();
  run(&mut t, "open", json!({"path": dir.join("plate.foxz")}));
  assert_eq!(t.design, s.design);
  for ext in ["glb", "usdz"] {
    let path = dir.join(format!("plate.{ext}"));
    run(&mut s, "export", json!({"path": path}));
    assert!(std::fs::metadata(&path).unwrap().len() > 100);
    assert!(s.execute("open", &json!({"path": path})).is_err(), "{ext} is export-only");
  }
  run(&mut s, "export", json!({"path": dir.join("ascii.stl"), "ascii": true}));
  assert!(std::fs::read_to_string(dir.join("ascii.stl")).unwrap().starts_with("solid"));
  // a mesh import joins the timeline and takes part in booleans
  let before = s.model.bodies.len();
  let r = run(&mut s, "import", json!({"path": dir.join("plate.stl")}));
  assert_eq!(s.model.bodies.len(), before + 1, "{r}");
  assert!(s.undo());
  assert_eq!(s.model.bodies.len(), before);
  let formats = run(&mut s, "formats", json!({}));
  assert!(formats.as_array().unwrap().iter().any(|f| f["id"] == "f3d" && f["export"] == false));
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn dxf_sketch_round_trip() {
  let dir = temp_dir("dxf");
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.slot", json!({"start": [0, 0], "end": [40, 0], "width": 10}));
  run(&mut s, "sketch.circle", json!({"center": [60, 0], "radius": 4}));
  run(&mut s, "sketch.finish", json!({}));
  let path = dir.join("slot.dxf");
  let r = run(&mut s, "export", json!({"path": path}));
  assert_eq!(r["entities"], 5, "{r}");
  // reading it back makes a sketch whose profiles extrude
  let mut t = Session::default();
  let r = run(&mut t, "import", json!({"path": path}));
  assert_eq!(r["curves"], 5, "{r}");
  assert_eq!(r["profiles"], 2, "endpoints were joined into closed profiles: {r}");
  assert!(t.can_undo());
  run(&mut t, "extrude", json!({"distance": 3}));
  let slot = 40.0 * 10.0 + std::f64::consts::PI * 25.0;
  let disc = std::f64::consts::PI * 16.0;
  assert!((total_volume(&t) - 3.0 * (slot + disc)).abs() < 1e-3, "{}", total_volume(&t));
  // one undo step removes the extrude, the next the whole imported sketch
  t.undo();
  t.undo();
  assert!(t.design.features.is_empty());
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn fusion_archives_are_explained_not_faked() {
  let dir = temp_dir("f3d");
  let path = dir.join("part.f3d");
  {
    use std::io::Write;
    let mut w = zip_writer();
    w.0.start_file("Design[Active]/Breps.BlobParts/BRep.1.smb", w.1).unwrap();
    w.0.write_all(b"ShapeManager binary").unwrap();
    std::fs::write(&path, w.0.finish().unwrap().into_inner()).unwrap();
  }
  let mut s = Session::default();
  let e = s.execute("import", &json!({"path": path})).unwrap_err();
  assert!(e.0.contains("STEP"), "{e}");
  let info = run(&mut s, "inspect", json!({"path": path}));
  assert_eq!(info["fusion"]["brep_blobs"], 1, "{info}");
  // it can still be kept in the hub, as a file
  s.store_root = Some(dir.join("hub"));
  let r = run(&mut s, "hub.upload", json!({"path": path}));
  assert_eq!(r["kind"], "file", "{r}");
  let _ = std::fs::remove_dir_all(dir);
}

fn zip_writer() -> (zip::ZipWriter<std::io::Cursor<Vec<u8>>>, zip::write::SimpleFileOptions) {
  (zip::ZipWriter::new(std::io::Cursor::new(Vec::new())), Default::default())
}

#[test]
fn hub_versions_milestones_and_history() {
  let root = temp_dir("hub");
  let mut s = Session { store_root: Some(root.clone()), ..Default::default() };
  plate(&mut s);
  let r = run(&mut s, "hub.save", json!({"description": "first plate"}));
  let item = r["item"].as_str().unwrap().to_string();
  assert_eq!(r["version"], 1);
  assert!(!s.dirty);
  assert_eq!(s.hub.as_ref().unwrap().item, item);
  // saving again adds versions to the same item
  run(&mut s, "param.set", json!({"name": "thick", "expr": "10"}));
  assert_eq!(run(&mut s, "hub.save", json!({"description": "thicker"}))["version"], 2);
  run(&mut s, "param.set", json!({"name": "thick", "expr": "14"}));
  assert_eq!(run(&mut s, "hub.save", json!({"milestone": "Prototype"}))["version"], 3);
  let vs = run(&mut s, "hub.versions", json!({}));
  let list = vs["versions"].as_array().unwrap();
  assert_eq!(list.len(), 3);
  assert_eq!(list[0]["number"], 3, "newest first");
  assert_eq!(list[0]["milestone"], "Prototype");
  assert_eq!(list[2]["description"], "first plate");
  assert!(list[0]["thumbnail"].as_bool().unwrap());
  assert_eq!(list[0]["stats"]["bodies"], 1);
  // what changed between v1 and v3
  let c = run(&mut s, "hub.compare", json!({"a": 1, "b": 3}));
  assert!(c["changes"].as_array().unwrap().iter().any(|x| x["what"] == "parameter" && x["name"] == "thick"), "{c}");
  // open the old version: thickness 6
  run(&mut s, "hub.open", json!({"item": item, "version": 1}));
  let thin = total_volume(&s);
  assert!(thin < 80.0 * 50.0 * 7.0);
  assert_eq!(s.store_root.as_ref(), Some(&root), "the hub location survives opening");
  // promote v1: it becomes v4, the latest
  assert_eq!(run(&mut s, "hub.promote", json!({"version": 1}))["version"], 4);
  run(&mut s, "hub.open", json!({"item": item}));
  assert!((total_volume(&s) - thin).abs() < 1e-6);
  // milestones are protected, other old versions can go
  assert!(s.execute("hub.version.delete", &json!({"version": 3})).is_err());
  run(&mut s, "hub.version.delete", json!({"version": 2}));
  // projects, folders, listing, search, activity
  let p = run(&mut s, "hub.project.create", json!({"name": "Fixtures"}))["project"].as_str().unwrap().to_string();
  let f = run(&mut s, "hub.folder.create", json!({"project": "Fixtures", "name": "Plates"}))["folder"]
    .as_str()
    .unwrap()
    .to_string();
  run(&mut s, "hub.item.move", json!({"project": p, "folder": f}));
  let l = run(&mut s, "hub.list", json!({"project": "fixtures", "folder": f}));
  assert_eq!(l["items"].as_array().unwrap().len(), 1, "{l}");
  assert_eq!(l["items"][0]["path"][0], "Plates");
  assert_eq!(run(&mut s, "hub.search", json!({"query": "proto"})).as_array().unwrap().len(), 1);
  assert!(run(&mut s, "hub.activity", json!({})).as_array().unwrap().len() >= 6);
  assert_eq!(run(&mut s, "hub.recent", json!({}))[0]["id"], item);
  // export the whole history as an archive and bring it back as a new item
  let arch = root.join("plate.foxz");
  assert_eq!(run(&mut s, "hub.download", json!({"path": arch}))["versions"], 3);
  let up = run(&mut s, "hub.upload", json!({"path": arch, "name": "Plate (restored)"}));
  let restored = up["item"].as_str().unwrap();
  let vs = run(&mut s, "hub.versions", json!({"item": restored}));
  assert_eq!(vs["versions"].as_array().unwrap().len(), 3, "{vs}");
  assert!(vs["versions"].as_array().unwrap().iter().any(|v| v["milestone"] == "Prototype"));
  // download as STEP translates
  let step = root.join("plate.step");
  run(&mut s, "hub.download", json!({"item": item, "version": 3, "path": step}));
  let mut t = Session::default();
  run(&mut t, "open", json!({"path": step}));
  assert!(total_volume(&t) > thin * 2.0);
  // uploading a STEP translates it into a design that keeps the original
  let up = run(&mut s, "hub.upload", json!({"path": step, "project": "Fixtures"}));
  assert_eq!(up["kind"], "design");
  let vs = run(&mut s, "hub.versions", json!({"item": up["item"]}));
  assert_eq!(vs["versions"][0]["original"], "v1.orig.step");
  // trash and restore
  run(&mut s, "hub.item.delete", json!({}));
  assert!(s.hub.is_none());
  assert_eq!(run(&mut s, "hub.trash", json!({}))["items"].as_array().unwrap().len(), 1);
  run(&mut s, "hub.item.restore", json!({"item": item}));
  assert_eq!(run(&mut s, "hub.trash", json!({}))["items"].as_array().unwrap().len(), 0);
  // a second session (the CLI, another window) sees the same hub
  let mut other = Session { store_root: Some(root.clone()), ..Default::default() };
  assert_eq!(run(&mut other, "hub.projects", json!({})).as_array().unwrap().len(), 2);
  let _ = std::fs::remove_dir_all(root);
}

#[test]
fn component_copies_follow_the_original() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 10, "height": 5}));
  let b = s.model.bodies[0].id.0;
  let c = run(&mut s, "component.create", json!({"name": "Block", "bodies": [b]}))["component"].clone();
  let r = run(&mut s, "component.copy", json!({"component": c, "translate": [50, 0, 0]}));
  assert!(r["error"].is_null(), "{r}");
  assert_eq!(s.model.bodies.len(), 2);
  let info = run(&mut s, "info", json!({}));
  assert_eq!(info["components"][1]["name"], "Block:2", "{info}");
  // edit the original: the copy follows
  let bx = s.design.features[0].id.0;
  run(
    &mut s,
    "feature.edit",
    json!({"feature": bx, "set": {"shape": {"type": "box", "width": "30", "depth": "10", "height": "5"}}}),
  );
  let v: Vec<f64> = s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).collect();
  assert!((v[0] - 1500.0).abs() < 1e-6 && (v[1] - 1500.0).abs() < 1e-6, "{v:?}");
  let m = s.model.bodies[1].shape.mass().unwrap();
  assert!((m.bbox_min.x - 35.0).abs() < 1e-6, "{m:?}");
}

#[test]
fn extrude_options_press_pull_chamfers_and_split_by_body() {
  use std::f64::consts::PI;
  let square = |s: &mut Session| {
    run(s, "sketch.create", json!({"plane": "xy"}));
    run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [20, 20]}));
  };
  // thin wall 2 mm inside a 20 × 20 square, 10 high
  let mut s = Session::default();
  square(&mut s);
  let r = run(&mut s, "extrude", json!({"distance": 10, "thin": 2}));
  assert!(r["error"].is_null(), "{r}");
  assert!((total_volume(&s) - (400.0 - 256.0) * 10.0).abs() < 1e-6, "{}", total_volume(&s));
  // tapered: a frustum, narrower at the top
  let mut s = Session::default();
  square(&mut s);
  let r = run(&mut s, "extrude", json!({"distance": 10, "taper": "10 deg"}));
  assert!(r["error"].is_null(), "{r}");
  let top = 20.0 - 2.0 * 10.0 * (10f64).to_radians().tan();
  let frustum = 10.0 / 3.0 * (400.0 + top * top + 20.0 * top);
  assert!((total_volume(&s) - frustum).abs() < 1e-3, "{} vs {frustum}", total_volume(&s));
  // to a face: a box's top, 30 up
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 30, "at": [50, 50]}));
  square(&mut s);
  let r = run(&mut s, "extrude", json!({"extent": "to_face", "to": {"point": [50, 50, 30]}}));
  assert!(r["error"].is_null(), "{r}");
  assert!((total_volume(&s) - (3000.0 + 400.0 * 30.0)).abs() < 1e-6);
  // press-pull the top up 5 then the side in 2
  let b = s.model.bodies[1].id.0;
  run(&mut s, "press_pull", json!({"faces": [{"point": [10, 10, 30]}], "distance": 5}));
  run(&mut s, "press_pull", json!({"faces": [{"point": [20, 10, 10]}], "distance": -2}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let v = s.model.bodies.iter().find(|x| x.id.0 == b).unwrap().shape.mass().unwrap().volume;
  assert!((v - 18.0 * 20.0 * 35.0).abs() < 1e-6, "{v}");
  // chamfers and a variable fillet
  for (params, ok) in [
    (json!({"distance": 2, "distance2": 4}), 4.0),
    (json!({"distance": 2, "angle": "30 deg"}), 2.0 * (30f64).to_radians().tan()),
  ] {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
    let mut p = params.clone();
    p["edges"] = json!([{"point": [10, 0, 20]}]);
    let r = run(&mut s, "chamfer", p);
    assert!(r["error"].is_null(), "{r}");
    let cut = 8000.0 - total_volume(&s);
    assert!((cut - 2.0 * ok / 2.0 * 20.0).abs() < 1e-6, "{cut}");
  }
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [10, 10]}));
  let r = run(&mut s, "fillet", json!({"edges": [{"point": [10, 0, 20]}], "radius": 1, "radius2": 4}));
  assert!(r["error"].is_null(), "{r}");
  let removed = 8000.0 - total_volume(&s);
  assert!(removed > (1.0 - PI / 4.0) * 20.0 && removed < (1.0 - PI / 4.0) * 16.0 * 20.0, "{removed}");
  // split a block with a cylinder
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 40, "depth": 20, "height": 10, "at": [20, 10]}));
  run(&mut s, "cylinder", json!({"radius": 5, "height": 30, "at": [20, 10], "plane": "xy"}));
  let tool = s.model.bodies[1].id.0;
  let r = run(&mut s, "split_by_body", json!({"tool": tool, "remove_tool": true}));
  assert!(r["error"].is_null(), "{r}");
  assert_eq!(s.model.bodies.len(), 2, "the block becomes the disc and the rest");
  assert!((total_volume(&s) - 8000.0).abs() < 1e-6);
}

#[test]
fn splines_and_sketch_chamfers_make_profiles() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.spline", json!({"points": [[0, 0], [10, 8], [20, 3], [30, 10]]}));
  run(&mut s, "sketch.line", json!({"points": [[30, 10], [30, -10], [0, -10], [0, 0]]}));
  let info = run(&mut s, "sketch.info", json!({}));
  assert_eq!(info["profiles"].as_array().unwrap().len(), 1, "{info}");
  let r = run(&mut s, "extrude", json!({"distance": 5}));
  assert!(r["error"].is_null(), "{r}");
  let v = total_volume(&s);
  assert!(v > 30.0 * 10.0 * 5.0 && v < 30.0 * 20.0 * 5.0, "{v}");
  assert!(s.model.bodies[0].faces().iter().any(|f| f.kind == fox_kernel::SurfaceKind::Other), "a smooth spline face");
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let c: Vec<u64> = serde_json::from_value(
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [20, 10]}))["curves"].clone(),
  )
  .unwrap();
  run(&mut s, "sketch.chamfer", json!({"curves": [c[0], c[1]], "distance": 2, "distance2": 3}));
  run(&mut s, "extrude", json!({"distance": 1}));
  assert!((total_volume(&s) - (200.0 - 3.0)).abs() < 1e-6, "{}", total_volume(&s));
}

#[test]
fn transactions_group_and_roll_back() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
  run(&mut s, "transaction.begin", json!({}));
  assert!(s.execute("transaction.begin", &json!({})).is_err(), "no nesting");
  run(&mut s, "param.add", json!({"name": "w", "expr": "20"}));
  run(&mut s, "box", json!({"at": [50, 0], "width": "w", "depth": 5, "height": 5}));
  // a failing parameter change reverts only itself, not the transaction
  assert!(s.execute("param.set", &json!({"name": "w", "expr": "nope"})).is_err());
  assert_eq!(s.model.bodies.len(), 2);
  assert_eq!(run(&mut s, "transaction.commit", json!({}))["changed"], true);
  // one undo removes the whole group
  assert!(s.undo());
  assert_eq!(s.model.bodies.len(), 1);
  assert!(s.design.parameters.is_empty());
  assert!(s.redo());
  assert_eq!(s.model.bodies.len(), 2);

  run(&mut s, "transaction.begin", json!({}));
  let first = s.model.bodies[0].id.0;
  run(&mut s, "body.remove", json!({"bodies": [first]}));
  assert_eq!(s.model.bodies.len(), 1);
  assert_eq!(run(&mut s, "transaction.rollback", json!({}))["changed"], true);
  assert_eq!(s.model.bodies.len(), 2);
  // undo inside a transaction can't reach edits made before it
  run(&mut s, "transaction.begin", json!({}));
  assert!(!s.undo());
  assert_eq!(run(&mut s, "transaction.commit", json!({}))["changed"], false);
  assert!(s.execute("transaction.commit", &json!({})).is_err());
}

#[test]
fn body_topology_lists_faces_and_edges() {
  let mut s = Session::default();
  let r = run(&mut s, "box", json!({"width": 20, "depth": 10, "height": 5}));
  let body = r["result_bodies"][0].as_u64().unwrap();
  let t = run(&mut s, "body.topology", json!({"body": body}));
  let faces = t["faces"].as_array().unwrap();
  let edges = t["edges"].as_array().unwrap();
  assert_eq!((faces.len(), edges.len()), (6, 12));
  let top = faces.iter().find(|f| f["normal"][2].as_f64().unwrap() > 0.9).unwrap();
  assert_eq!(top["kind"], "plane");
  assert!((top["area"].as_f64().unwrap() - 200.0).abs() < 1e-6);
  assert_eq!(top["edges"].as_array().unwrap().len(), 4);
  for e in edges {
    assert_eq!(e["faces"].as_array().unwrap().len(), 2, "{e}");
    assert_eq!(e["kind"], "line");
  }
  let total: f64 = edges.iter().map(|e| e["length"].as_f64().unwrap()).sum();
  assert!((total - 4.0 * (20.0 + 10.0 + 5.0)).abs() < 1e-6, "{total}");
  let only = run(&mut s, "body.topology", json!({"body": body, "faces": false}));
  assert!(only.get("faces").is_none() && only["edges"].is_array());
  assert!(s.execute("body.topology", &json!({"body": 999})).is_err());
}

#[test]
fn colour_selection_and_sketch_visibility() {
  let mut s = Session::default();
  plate(&mut s);
  let b = s.model.bodies[0].id.0;
  run(&mut s, "body.appearance", json!({"body": b, "color": "#ff8000"}));
  assert_eq!(run(&mut s, "info", json!({}))["bodies"][0]["color"], "#ff8000");
  run(&mut s, "body.appearance", json!({"body": b, "color": [0, 16, 255]}));
  assert_eq!(s.design.body_settings(BodyId(b)).unwrap().color, Some([0, 16, 255]));
  assert!(s.execute("body.appearance", &json!({"body": b, "color": "#12"})).is_err());
  assert!(s.undo());
  assert_eq!(s.design.body_settings(BodyId(b)).unwrap().color, Some([255, 128, 0]));
  run(&mut s, "body.appearance", json!({"body": b, "color": null}));
  assert!(run(&mut s, "info", json!({}))["bodies"][0]["color"].is_null());

  run(&mut s, "select", json!({"items": [{"type": "body", "body": b}]}));
  assert_eq!(run(&mut s, "selection", json!({})), json!([{"type": "body", "body": b}]));

  // consumed sketches are hidden automatically; the flag overrides that either way
  let sketch = s.design.features.iter().find(|f| f.kind.base_name() == "Sketch").unwrap().id;
  let sketch_meshes = |s: &Session, hide: bool| {
    let opts = scene::ViewOptions { hide_sketches: hide, ..Default::default() };
    scene::SceneBuilder::default().build(s, &opts).0.meshes.len()
  };
  let auto = sketch_meshes(&s, false);
  run(&mut s, "sketch.visible", json!({"sketch": sketch.0, "visible": true}));
  assert!(sketch_meshes(&s, false) > auto);
  assert!(sketch_meshes(&s, true) < sketch_meshes(&s, false));
  run(&mut s, "sketch.visible", json!({"sketch": sketch.0, "visible": null}));
  assert_eq!(sketch_meshes(&s, false), auto);
}

#[test]
fn attributes_follow_their_targets() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [20, 10]}));
  let sk = s.edit_sketch.unwrap().0;
  let curve = run(&mut s, "sketch.info", json!({}))["curves"][0]["id"].as_u64().unwrap();
  let r = run(&mut s, "extrude", json!({"distance": 5}));
  let (feature, body) = (r["feature"].as_u64().unwrap(), r["result_bodies"][0].as_u64().unwrap());
  let g = "com.example.tabs";
  run(&mut s, "attribute.set", json!({"target": {"body": body}, "group": g, "name": "role", "value": "panel"}));
  run(&mut s, "attribute.set", json!({"target": {"feature": feature}, "group": g, "name": "tab", "value": {"id": 3}}));
  run(
    &mut s,
    "attribute.set",
    json!({"target": {"sketch": sk, "curve": curve}, "group": g, "name": "edge", "value": 1}),
  );
  run(&mut s, "attribute.set", json!({"group": g, "name": "version", "value": 2}));
  // set replaces
  run(&mut s, "attribute.set", json!({"target": {"body": body}, "group": g, "name": "role", "value": "letter"}));
  let on_body = run(&mut s, "attribute.get", json!({"target": {"body": body}}));
  assert_eq!(on_body, json!([{"target": {"body": body}, "group": g, "name": "role", "value": "letter"}]));
  assert_eq!(run(&mut s, "attribute.get", json!({}))[0]["value"], 2);
  let tabs = run(&mut s, "attribute.find", json!({"name": "tab"}));
  assert_eq!(tabs[0]["target"], json!({"feature": feature}));
  assert_eq!(run(&mut s, "attribute.find", json!({"value": "letter"})).as_array().unwrap().len(), 1);
  assert!(s.execute("attribute.set", &json!({"target": {"body": 999}, "group": g, "name": "x", "value": 1})).is_err());
  // round-trips through a file
  let d: Design = serde_json::from_str(&serde_json::to_string(&s.design).unwrap()).unwrap();
  assert_eq!(d.attributes.len(), 4);
  // deleting the sketch deletes the extrude and the attributes on both
  run(&mut s, "feature.delete", json!({"feature": sk}));
  assert_eq!(run(&mut s, "attribute.find", json!({"group": g})).as_array().unwrap().len(), 1);
  assert_eq!(s.design.attributes.len(), 2, "the body's attribute stays stored, but its body is gone");
  assert!(s.undo());
  assert_eq!(run(&mut s, "attribute.find", json!({"group": g})).as_array().unwrap().len(), 4);
  assert_eq!(run(&mut s, "attribute.delete", json!({"target": {"body": body}, "group": g}))["removed"], 1);
}

#[test]
fn sketch_info_reports_frames_lengths_and_profile_curves() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 10], "normal": [1, 0, 0], "x": [0, 1, 0]}}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [30, 20]}));
  run(&mut s, "sketch.circle", json!({"center": [15, 10], "radius": 4}));
  run(&mut s, "sketch.line", json!({"points": [[40, 0], [50, 0]]}));
  run(&mut s, "sketch.spline", json!({"points": [[0, 30], [10, 35], [20, 30]]}));
  let i = run(&mut s, "sketch.info", json!({"samples": true, "loops": true}));
  assert_eq!(i["plane"]["origin"], json!([0.0, 0.0, 10.0]));
  assert_eq!(i["plane"]["normal"], json!([1.0, 0.0, 0.0]));
  assert_eq!(i["plane"]["x"], json!([0.0, 1.0, 0.0]));
  let c: Vec<u64> = i["curves"].as_array().unwrap().iter().map(|c| c["id"].as_u64().unwrap()).collect();
  assert_eq!(c.len(), 7);
  let geo = i["geometry"].as_array().unwrap();
  let len = |id: u64| geo.iter().find(|g| g["curve"] == id).unwrap()["length"].as_f64().unwrap();
  let lens = [len(c[0]), len(c[1])];
  assert!(lens.contains(&30.0) && lens.contains(&20.0), "{lens:?}");
  assert!((len(c[4]) - std::f64::consts::TAU * 4.0).abs() < 1e-9);
  assert!(len(c[6]) > 20.0 && len(c[6]) < 23.0, "{}", len(c[6]));
  assert!(geo[0]["samples"].as_array().unwrap().len() >= 2);
  let profiles = i["profiles"].as_array().unwrap();
  assert_eq!(profiles.len(), 2, "ring and disc");
  let ring = profiles.iter().find(|p| p["holes"] == 1).unwrap();
  let mut outer: Vec<u64> = ring["curves"].as_array().unwrap().iter().map(|c| c.as_u64().unwrap()).collect();
  outer.sort();
  assert_eq!(outer, c[..4].to_vec());
  assert_eq!(ring["hole_curves"], json!([[c[4]]]));
  assert_eq!(ring["bbox"], json!([[0.0, 0.0], [30.0, 20.0]]));
  assert!((ring["perimeter"].as_f64().unwrap() - (100.0 + std::f64::consts::TAU * 4.0)).abs() < 0.05);
  assert!(ring["outer"].as_array().unwrap().len() == 4);
  assert_eq!(i["open_curves"], json!([c[5], c[6]]));

  // a free plane can also be a timeline construction plane
  let r = run(&mut s, "plane.offset", json!({"plane": {"origin": [5, 5, 5], "normal": [0, 0, 2]}}));
  assert!(r["error"].is_null(), "{r}");
  let pl = s.model.planes.get(&FeatureId(r["feature"].as_u64().unwrap() as u32)).unwrap();
  assert!((pl.origin - fox_kernel::glam::DVec3::new(5.0, 5.0, 5.0)).length() < 1e-9);
  assert!(s.execute("sketch.create", &json!({"plane": {"origin": [0, 0, 0], "normal": [0, 0, 0]}})).is_err());
}

#[test]
fn transform_and_insert_sketch_geometry() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [10, 10], "corner2": [30, 20]}));
  run(&mut s, "sketch.circle", json!({"center": [20, 15], "radius": 2}));
  let sk = s.edit_sketch.unwrap().0;
  let bbox = |s: &mut Session| run(s, "sketch.info", json!({"sketch": sk}))["profiles"][0]["bbox"].clone();
  run(&mut s, "sketch.transform", json!({"scale": 2, "center": [20, 15]}));
  assert_eq!(bbox(&mut s), json!([[0.0, 5.0], [40.0, 25.0]]));
  let r = run(&mut s, "sketch.transform", json!({"angle": "90 deg", "center": [20, 15], "translate": [100, 0]}));
  assert!(!r["removed_constraints"].as_array().unwrap().is_empty(), "horizontal/vertical constraints go");
  let b = bbox(&mut s);
  let near = |v: &Value, x: f64| (v.as_f64().unwrap() - x).abs() < 1e-6;
  assert!(near(&b[0][0], 110.0) && near(&b[0][1], -5.0) && near(&b[1][0], 130.0) && near(&b[1][1], 35.0), "{b}");
  let circle = s
    .design
    .sketch(FeatureId(sk))
    .unwrap()
    .curves
    .iter()
    .find(|c| matches!(c.kind, fox_sketch::CurveKind::Circle { .. }))
    .unwrap()
    .id;
  assert_eq!(s.design.sketch(FeatureId(sk)).unwrap().radius(circle), Some(4.0));
  assert!(s.execute("sketch.transform", &json!({"scale": -1})).is_err());

  // insert from another sketch and from a DXF written by export
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  let dst = s.edit_sketch.unwrap().0;
  let r = run(&mut s, "sketch.insert", json!({"from": {"sketch": sk}, "translate": [-120, 0]}));
  assert_eq!(r["curves"].as_array().unwrap().len(), 5);
  let dir = std::env::temp_dir().join(format!("fox-insert-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let dxf = dir.join("part.dxf");
  run(&mut s, "export", json!({"path": dxf, "sketch": sk}));
  let r = run(&mut s, "sketch.insert", json!({"from": {"path": dxf}, "scale": 0.5}));
  assert_eq!(r["curves"].as_array().unwrap().len(), 5);
  let info = run(&mut s, "sketch.info", json!({"sketch": dst}));
  assert_eq!(info["profiles"].as_array().unwrap().len(), 4, "two rings and two discs");
  let fox = dir.join("lib.fox");
  run(&mut s, "save", json!({"path": fox}));
  let r = run(&mut s, "sketch.insert", json!({"from": {"path": fox, "sketch": sk}, "curves": [circle.0]}));
  assert_eq!(r["curves"].as_array().unwrap().len(), 1);
  assert!(s.execute("sketch.insert", &json!({"from": {"path": fox, "sketch": "nope"}})).is_err());
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn join_and_cut_only_touch_their_targets() {
  let mut s = Session::default();
  let a = run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 10}))["result_bodies"][0].as_u64().unwrap();
  let b = run(&mut s, "box", json!({"at": [15, 0], "width": 20, "depth": 20, "height": 10, "operation": "new_body"}))
    ["result_bodies"][0]
    .as_u64()
    .unwrap();
  assert_eq!(s.model.bodies.len(), 2);
  let vol = |s: &Session, id: u64| s.model.body(BodyId(id as u32)).unwrap().shape.mass().unwrap().volume;
  let (va, vb) = (vol(&s, a), vol(&s, b));
  // a cylinder through the overlap cuts only body b
  let r = run(&mut s, "cylinder", json!({"at": [8, 0], "radius": 2, "height": 10, "operation": "cut", "targets": [b]}));
  assert_eq!(r["result_bodies"], json!([b]));
  assert!((vol(&s, a) - va).abs() < 1e-6);
  assert!(vol(&s, b) < vb);
  // a tab joined to a only, though it touches both
  run(&mut s, "box", json!({"at": [8, 12], "width": 4, "depth": 6, "height": 2, "operation": "join", "targets": [a]}));
  assert_eq!(s.model.bodies.len(), 2);
  assert!(vol(&s, a) > va);
  assert!(
    s.execute("box", &json!({"width": 1, "depth": 1, "height": 1, "operation": "cut", "targets": [99]})).is_err()
  );
}

#[test]
fn render_to_a_file() {
  let mut s = Session::default();
  let a = run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 10}))["result_bodies"][0].as_u64().unwrap();
  run(&mut s, "box", json!({"at": [60, 0], "width": 5, "depth": 5, "height": 5, "operation": "new_body"}));
  let path = std::env::temp_dir().join(format!("fox-render-{}.png", std::process::id()));
  let r = run(
    &mut s,
    "view.render",
    json!({"path": path, "width": 120, "height": 80, "yaw": 45, "pitch": 30, "ortho": true, "bodies": [a], "background": "#ffffff"}),
  );
  assert!(r["bytes"].as_u64().unwrap() > 100);
  let png = std::fs::read(&path).unwrap();
  assert_eq!(&png[1..4], b"PNG");
  let be = |i: usize| u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]);
  assert_eq!((be(16), be(20)), (120, 80), "IHDR size");
  let _ = std::fs::remove_file(&path);
  assert!(s.execute("view.render", &json!({"path": path, "view": "sideways"})).is_err());
  assert!(s.execute("view.render", &json!({"path": path, "bodies": [99]})).is_err());
}
