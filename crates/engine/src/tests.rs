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
  if let Ok(dir) = std::env::var("CLF_SNAPSHOT_DIR") {
    frame.save_png(&std::path::Path::new(&dir).join("plate.png")).unwrap();
  }
}

fn temp_dir(name: &str) -> std::path::PathBuf {
  let d = std::env::temp_dir().join(format!("clf-engine-{name}-{}", std::process::id()));
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
    ("clf", 1e-6),
    ("clfz", 1e-6),
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
    assert_eq!(t.path.is_some(), ext == "clf" || ext == "clfz", "{ext}");
  }
  // the parametric timeline survives the archive
  let mut t = Session::default();
  run(&mut t, "open", json!({"path": dir.join("plate.clfz")}));
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
  let arch = root.join("plate.clfz");
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
