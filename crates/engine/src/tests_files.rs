//! Files and data: DXF splines and ellipses, STEP assemblies with names and colours, hub
//! comments and the visual version diff, driven through commands.

use serde_json::json;

use crate::Session;

fn run(s: &mut Session, cmd: &str, p: serde_json::Value) -> serde_json::Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn temp_dir(name: &str) -> std::path::PathBuf {
  let d = std::env::temp_dir().join(format!("fox-engine-files-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&d);
  std::fs::create_dir_all(&d).unwrap();
  d
}

fn curve_kinds(s: &Session) -> Vec<String> {
  let id = s.design.features.last().unwrap().id;
  let sk = &s.model.sketches[&id].sketch;
  sk.curves.iter().map(|c| serde_json::to_value(&c.kind).unwrap()["type"].as_str().unwrap().to_string()).collect()
}

#[test]
fn dxf_keeps_splines_and_ellipses_as_curves() {
  let dir = temp_dir("dxf-splines");
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.spline", json!({"points": [[0, 0], [10, 8], [20, 0], [30, 5]]}));
  run(&mut s, "sketch.control_spline", json!({"points": [[0, 20], [10, 30], [20, 10], [30, 25], [40, 20]]}));
  run(&mut s, "sketch.ellipse", json!({"center": [60, 0], "major": [70, 0], "minor": 4}));
  run(&mut s, "sketch.line", json!({"points": [[30, 5], [40, 20]]}));
  run(&mut s, "sketch.finish", json!({}));
  let before = curve_kinds(&s);
  let path = dir.join("curves.dxf");
  let r = run(&mut s, "export", json!({"path": path}));
  assert_eq!(r["entities"], 6, "the ellipse keeps its two construction axes: {r}");
  let text = std::fs::read_to_string(&path).unwrap();
  assert!(text.contains("SPLINE") && text.contains("ELLIPSE"), "real entities, not line chains");
  assert!(text.matches("\nLINE\n").count() == 3, "only the line and the ellipse's axes are LINEs");

  let mut t = Session::default();
  run(&mut t, "import", json!({"path": path}));
  assert_eq!(curve_kinds(&t), before, "each curve comes back as the same kind");
  // and the same shape: the curves draw the same Bézier pieces
  let (a, b) = (&s.model.sketches[&s.design.features[0].id].sketch, &t.model.sketches[&t.design.features[0].id].sketch);
  for (ca, cb) in a.curves.iter().zip(&b.curves) {
    for (p, q) in a.beziers(ca.id).iter().flatten().zip(b.beziers(cb.id).iter().flatten()) {
      assert!((p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6, "{p:?} {q:?}");
    }
  }
  // the spline's ends are joined to the line, as before
  let sk = &t.model.sketches[&t.design.features[0].id].sketch;
  // the drawn line, not the ellipse's construction axes
  let line =
    sk.curves.iter().find(|c| !c.construction && matches!(c.kind, fox_sketch::CurveKind::Line { .. })).unwrap();
  let fox_sketch::CurveKind::Line { a: la, .. } = line.kind else { panic!("not a line") };
  let fit = sk.curves.iter().find_map(|c| match &c.kind {
    fox_sketch::CurveKind::Spline { points, .. } => Some(points.clone()),
    _ => None,
  });
  assert_eq!(fit.unwrap().last(), Some(&la), "shared end point");
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn step_assemblies_keep_names_colours_and_components() {
  let dir = temp_dir("step-asm");
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 5}));
  run(&mut s, "box", json!({"at": [50, 0], "width": 10, "depth": 10, "height": 10, "operation": "new_body"}));
  run(&mut s, "box", json!({"at": [0, 50], "width": 8, "depth": 8, "height": 8, "operation": "new_body"}));
  let ids: Vec<u32> = s.model.bodies.iter().map(|b| b.id.0).collect();
  assert_eq!(ids.len(), 3);
  run(&mut s, "body.rename", json!({"body": ids[0], "name": "Base plate"}));
  run(&mut s, "body.rename", json!({"body": ids[1], "name": "Wheel hub"}));
  run(&mut s, "body.rename", json!({"body": ids[2], "name": "Pin"}));
  run(&mut s, "body.appearance", json!({"body": ids[1], "color": [200, 30, 40]}));
  run(&mut s, "body.appearance", json!({"body": ids[2], "color": "#2050c0"}));
  run(&mut s, "component.create", json!({"name": "Wheel", "bodies": [ids[1], ids[2]]}));
  let path = dir.join("asm.step");
  run(&mut s, "export", json!({"path": path}));
  let text = std::fs::read_to_string(&path).unwrap();
  assert!(text.contains("Wheel hub") && text.contains("Wheel") && text.contains("COLOUR_RGB"), "names and colours");

  let mut t = Session::default();
  let r = run(&mut t, "import", json!({"path": path}));
  assert_eq!(r["bodies"].as_array().unwrap().len(), 3, "{r}");
  let mut names: Vec<String> = t.model.bodies.iter().map(|b| t.body_name(b.id)).collect();
  names.sort();
  assert_eq!(names, ["Base plate", "Pin", "Wheel hub"]);
  let by_name = |n: &str| t.model.bodies.iter().find(|b| t.body_name(b.id) == n).unwrap().id;
  let color = |n: &str| t.design.body_settings(by_name(n)).and_then(|b| b.color);
  let close =
    |c: Option<[u8; 3]>, want: [u8; 3]| c.is_some_and(|c| c.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 1));
  assert!(close(color("Wheel hub"), [200, 30, 40]), "{:?}", color("Wheel hub"));
  assert!(close(color("Pin"), [0x20, 0x50, 0xc0]), "{:?}", color("Pin"));
  assert_eq!(color("Base plate"), None);
  assert_eq!(t.design.components.len(), 1);
  let c = &t.design.components[0];
  assert_eq!(c.name, "Wheel");
  let mut members: Vec<String> = c.bodies.iter().map(|b| t.body_name(*b)).collect();
  members.sort();
  assert_eq!(members, ["Pin", "Wheel hub"]);
  // geometry and placement survive
  let vol = |s: &Session, n: &str| {
    let b = s.model.bodies.iter().find(|b| s.body_name(b.id) == n).unwrap();
    b.shape.mass().unwrap()
  };
  let (a, b) = (vol(&s, "Pin"), vol(&t, "Pin"));
  assert!((a.volume - b.volume).abs() < 1e-6 && (a.center_of_mass.y - b.center_of_mass.y).abs() < 1e-6);
  // one undo removes the whole import (bodies, names and components)
  t.undo();
  assert!(t.design.features.is_empty() && t.design.components.is_empty());
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn hub_comments_and_visual_diff() {
  let dir = temp_dir("hub-diff");
  let mut s = Session { store_root: Some(dir.join("hub")), ..Default::default() };
  run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 10}));
  let v1 = run(&mut s, "hub.save", json!({"name": "Block"}));
  let item = v1["item"].as_str().unwrap().to_string();
  // v2: taller (adds material) and with a hole (removes some)
  run(
    &mut s,
    "box",
    json!({"plane": {"origin": [0, 0, 10], "normal": [0, 0, 1]}, "width": 20, "depth": 20, "height": 5, "operation": "join"}),
  );
  run(
    &mut s,
    "cylinder",
    json!({"plane": {"origin": [0, 0, -5], "normal": [0, 0, 1]}, "radius": 3, "height": 40, "operation": "cut"}),
  );
  run(&mut s, "hub.save", json!({"description": "taller, with a hole"}));

  // comments: on the open version by default, or a given one
  let c = run(&mut s, "hub.comment", json!({"text": "Hole looks good"}));
  assert_eq!(c["version"], 2);
  run(&mut s, "hub.comment", json!({"version": 1, "text": "Too short"}));
  let list = run(&mut s, "hub.comments", json!({}));
  let mut texts: Vec<&str> = list["comments"].as_array().unwrap().iter().map(|c| c["text"].as_str().unwrap()).collect();
  texts.sort();
  assert_eq!(texts, ["Hole looks good", "Too short"]);
  let versions = run(&mut s, "hub.versions", json!({"item": item}));
  assert_eq!(versions["versions"][0]["comments"][0]["text"], "Hole looks good");
  run(&mut s, "hub.comment.delete", json!({"comment": c["comment"]}));
  assert_eq!(run(&mut s, "hub.comments", json!({"version": 2}))["comments"].as_array().unwrap().len(), 0);

  // the diff: a 20×20×5 slab added on top (less the hole through it), the hole's part of v1 removed
  let d = run(&mut s, "hub.diff", json!({"item": item, "a": 1}));
  let hole = std::f64::consts::PI * 9.0;
  let added = 20.0 * 20.0 * 5.0 - hole * 5.0;
  let removed = hole * 10.0;
  let unchanged = 20.0 * 20.0 * 10.0 - hole * 10.0;
  let near = |k: &str, v: f64| (d[k].as_f64().unwrap() - v).abs() < v * 0.01;
  assert!(near("added_mm3", added) && near("removed_mm3", removed) && near("unchanged_mm3", unchanged), "{d}");
  assert!(s.diff.is_some());
  // the viewport draws the comparison instead of the bodies: green and red meshes, ghost edges
  let mut b = crate::scene::SceneBuilder::default();
  let (scene, _) = b.build(&s, &crate::scene::ViewOptions::default());
  let colors: Vec<[u8; 4]> = scene.meshes.iter().map(|m| m.color).collect();
  assert!(colors.contains(&crate::diff::ADDED_COLOR) && colors.contains(&crate::diff::REMOVED_COLOR), "{colors:?}");
  assert_eq!(scene.meshes.len(), 2, "unchanged geometry is only ghosted edges");
  run(&mut s, "hub.diff", json!({"clear": true}));
  assert!(s.diff.is_none());
  let (scene, _) = b.build(&s, &crate::scene::ViewOptions::default());
  assert!(!scene.meshes.iter().any(|m| m.color == crate::diff::ADDED_COLOR));
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn insert_tspline_from_a_tsm_file() {
  let tsm = include_str!("../../exchange/testdata/box-cage.tsm");
  let dir = temp_dir("tsm");
  let path = dir.join("box.tsm");
  std::fs::write(&path, tsm).unwrap();
  // the file, its contents and a plain import all give the same smooth six-faced body
  let mut s = Session::default();
  let r = run(&mut s, "form.insert_tsm", json!({"path": path}));
  assert_eq!(r["solids"], 1);
  run(&mut s, "form.insert_tsm", json!({"text": tsm}));
  run(&mut s, "import", json!({"path": path}));
  assert_eq!(s.model.bodies.len(), 3);
  for b in &s.model.bodies {
    assert!(b.shape.is_valid());
    assert_eq!(b.faces().len(), 6);
    let m = b.shape.mass().unwrap();
    // Catmull-Clark limit of the 80 × 60 × 20 mm cage (Fusion measures 75586.9 mm³)
    assert!((m.volume - 75538.0).abs() < 5.0, "{}", m.volume);
    assert!((m.bbox_max.z - 20.0).abs() < 1e-3 && (m.bbox_max.x - 40.0).abs() < 1e-3);
  }
  assert!(s.execute("form.insert_tsm", &json!({"text": "not a T-spline"})).is_err());
  assert!(s.execute("form.insert_tsm", &json!({"path": dir.join("box.step")})).is_err());
  assert!(s.execute("form.insert_tsm", &json!({})).is_err());
}

#[test]
fn quintic_control_splines_extrude_as_one_face_and_keep_their_degree_in_dxf() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let pts = json!([[0, 0], [8, 20], [18, 2], [28, 22], [40, 4], [50, 12]]);
  assert!(s.execute("sketch.control_spline", &json!({"points": pts, "degree": 4})).is_err(), "degree 3 or 5 only");
  run(&mut s, "sketch.control_spline", json!({"points": pts, "degree": 5}));
  run(&mut s, "sketch.line", json!({"points": [[50, 12], [50, -10]]}));
  run(&mut s, "sketch.line", json!({"points": [[50, -10], [0, -10]]}));
  run(&mut s, "sketch.line", json!({"points": [[0, -10], [0, 0]]}));
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "extrude", json!({"sketch": 1, "distance": 5, "operation": "new_body"}));
  // one span of a quintic, the Bézier with these control points (area exact; Fox follows it with
  // short cubics, about 1e-5 mm off)
  let body = &s.model.bodies[0];
  assert_eq!(body.faces().len(), 6, "the spline is one side face");
  let v = body.shape.mass().unwrap().volume;
  assert!((v - 5036.507936507936).abs() < 1e-2, "{v:.9}");

  let dir = temp_dir("dxf-quintic");
  let path = dir.join("quintic.dxf");
  run(&mut s, "export", json!({"path": path, "sketch": 1}));
  let mut t = Session::default();
  run(&mut t, "import", json!({"path": path}));
  let sk = &t.model.sketches[&t.design.features[0].id].sketch;
  assert!(
    sk.curves
      .iter()
      .any(|c| matches!(&c.kind, fox_sketch::CurveKind::ControlSpline { points, degree: 5 } if points.len() == 6)),
    "{:?}",
    sk.curves
  );
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_old_files_control_spline_keeps_its_pieces_and_a_new_one_is_one_edge() {
  // the six-point cubic of reference case 188: three Bézier spans, closed by three lines
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.control_spline", json!({"points": [[0, 0], [8, 20], [18, 2], [28, 22], [40, 4], [50, 12]]}));
  run(&mut s, "sketch.line", json!({"points": [[50, 12], [50, -10]]}));
  run(&mut s, "sketch.line", json!({"points": [[50, -10], [0, -10]]}));
  run(&mut s, "sketch.line", json!({"points": [[0, -10], [0, 0]]}));
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "extrude", json!({"sketch": 1, "distance": 5, "operation": "new_body"}));
  let counts = |s: &Session| {
    let b = &s.model.bodies[0];
    (b.faces().len(), b.edges().len(), b.shape.mass().unwrap().volume)
  };
  let (faces, edges, v) = counts(&s);
  assert_eq!((faces, edges), (6, 12), "a new design: the spline is one edge and one side face");

  // the same design saved before behaviour flags: no `behaviours`, so it rebuilds as it did
  let mut old = s.design.clone();
  old.behaviours = fox_sketch::Behaviours::legacy();
  for f in &mut old.features {
    if let fox_model::FeatureKind::Sketch { sketch, .. } = &mut f.kind {
      sketch.behaviours = fox_sketch::Behaviours::legacy();
    }
  }
  let json = old.to_json();
  assert!(!json.contains("behaviours"));
  let dir = temp_dir("legacy-control-spline");
  let path = dir.join("old.fox");
  std::fs::write(&path, json).unwrap();
  let t = Session::open(&path).unwrap();
  let (faces, edges, w) = counts(&t);
  assert_eq!((faces, edges), (8, 18), "an old design: one edge and side face per span, as before");
  assert!((v - w).abs() < 1e-3 * v, "{v} vs {w}");
  let _ = std::fs::remove_dir_all(dir);
}

/// Fusion's Export DXF: the unit and the Visible Geometry check boxes decide what the file holds.
#[test]
fn sketch_dxf_export_takes_a_unit_and_leaves_out_unticked_geometry() {
  use fox_exchange::dxf::{Entity, read_dxf};
  let dir = temp_dir("dxf-options");
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 30, "depth": 20, "height": 10}));
  let b = s.model.bodies[0].id.0;
  let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 25}));
  let pl = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": {"feature": pl}}));
  let sk = s.edit_sketch.unwrap();
  run(&mut s, "sketch.project", json!({"body": b, "face": top}));
  run(&mut s, "sketch.line", json!({"points": [[101.6, 0], [127, 0]]}));
  run(&mut s, "sketch.line", json!({"points": [[100, 10], [110, 10]]}));
  run(&mut s, "sketch.point", json!({"at": [200, 200]}));
  let last = s.design.sketch(sk).unwrap().curves.last().unwrap().id.0;
  run(&mut s, "sketch.construction", json!({"curves": [last], "on": true}));
  run(&mut s, "sketch.finish", json!({}));
  let read = |s: &mut Session, extra: serde_json::Value| {
    let path = dir.join("out.dxf");
    let mut p = json!({"path": path, "sketch": sk.0});
    for (k, v) in extra.as_object().unwrap() {
      p[k] = v.clone();
    }
    run(s, "export", p);
    let text = std::fs::read_to_string(&path).unwrap();
    (text.clone(), read_dxf(text.as_bytes()).unwrap().entities)
  };
  // everything: 4 projected lines, 2 lines (one construction) and the point
  let (text, all) = read(&mut s, json!({}));
  assert_eq!(all.len(), 7, "{all:?}");
  assert!(text.contains("$INSUNITS\n 70\n4\n"));
  let (_, e) = read(&mut s, json!({"points": false}));
  assert_eq!(e.iter().filter(|(e, _)| matches!(e, Entity::Point { .. })).count(), 0);
  assert_eq!(e.len(), 6);
  let (_, e) = read(&mut s, json!({"construction": false}));
  assert!(e.len() == 6 && e.iter().all(|(_, c)| !c), "{e:?}");
  let (_, e) = read(&mut s, json!({"projected": false}));
  assert_eq!(e.len(), 3, "{e:?}");
  // in inches the file holds inches (and reads back as the same millimetres)
  let (text, e) = read(&mut s, json!({"units": "in", "projected": false, "points": false, "construction": false}));
  assert!(text.contains("$INSUNITS\n 70\n1\n"), "{text}");
  assert!(text.contains("\n 10\n4\n") && text.contains("\n 11\n5\n"), "4 and 5 in the file: {text}");
  match e.as_slice() {
    [(Entity::Line { a, b }, false)] => assert!((a[0] - 101.6).abs() < 1e-6 && (b[0] - 127.0).abs() < 1e-6),
    o => panic!("{o:?}"),
  }
  let err = s.execute("export", &json!({"path": dir.join("x.dxf"), "sketch": sk.0, "units": "furlong"}));
  assert!(err.is_err_and(|e| e.0.contains("unknown DXF unit")));
  let _ = std::fs::remove_dir_all(dir);
}

/// Redefine Sketch Plane: the sketch moves to another plane or face made before it, keeping its
/// sketch coordinates, and what is built on it follows.
#[test]
fn sketch_plane_moves_a_sketch_and_its_features_follow() {
  let mut s = Session::default();
  run(&mut s, "box", json!({"width": 30, "depth": 20, "height": 10}));
  let b = s.model.bodies[0].id.0;
  let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
  run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 40}));
  let pl = s.design.features.last().unwrap().id.0;
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  let sk = s.edit_sketch.unwrap();
  run(&mut s, "sketch.circle", json!({"center": [100, 100], "radius": 3}));
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "extrude", json!({"sketch": sk.0, "distance": 5}));
  let later = s.design.features.last().unwrap().id.0;
  let undo = s.undo.len();

  // onto the box's top face: the cylinder now starts at z = 10
  let r = run(&mut s, "sketch.plane", json!({"sketch": sk.0, "plane": {"body": b, "face": top}}));
  assert!(r["error"].is_null(), "{r}");
  assert_eq!(s.undo.len(), undo + 1, "one undo step");
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let z = |s: &Session| s.model.sketches[&sk].plane.origin.z;
  assert!((z(&s) - 10.0).abs() < 1e-6, "{}", z(&s));
  // onto the construction plane
  run(&mut s, "sketch.plane", json!({"sketch": sk.0, "plane": {"feature": pl}}));
  assert!((z(&s) - 40.0).abs() < 1e-6);
  // the curves keep their sketch coordinates
  let c = &s.model.sketches[&sk].sketch;
  assert!(c.points.iter().any(|p| (p.x - 100.0).abs() < 1e-9 && (p.y - 100.0).abs() < 1e-9));

  // a feature made after the sketch can't be its plane
  let err = s.execute("sketch.plane", &json!({"sketch": sk.0, "plane": {"feature": later}})).unwrap_err();
  assert!(!err.0.is_empty());
  run(&mut s, "plane.offset", json!({"plane": "yz", "offset": 5}));
  let after = s.design.features.last().unwrap().id.0;
  let err = s.execute("sketch.plane", &json!({"sketch": sk.0, "plane": {"feature": after}})).unwrap_err();
  assert!(err.0.contains("after the sketch"), "{}", err.0);
  assert!((z(&s) - 40.0).abs() < 1e-6, "unchanged");
  assert!(s.execute("sketch.plane", &json!({"sketch": b, "plane": "xy"})).is_err(), "not a sketch");
}
