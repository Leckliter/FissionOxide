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
  let d = std::env::temp_dir().join(format!("clf-engine-files-{name}-{}", std::process::id()));
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
  assert_eq!(r["entities"], 4, "{r}");
  let text = std::fs::read_to_string(&path).unwrap();
  assert!(text.contains("SPLINE") && text.contains("ELLIPSE"), "real entities, not line chains");
  assert!(text.matches("\nLINE\n").count() == 1, "only the line is a LINE");

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
  let line = sk.curves.iter().find(|c| matches!(c.kind, clf_sketch::CurveKind::Line { .. })).unwrap();
  let clf_sketch::CurveKind::Line { a: la, .. } = line.kind else { panic!("not a line") };
  let fit = sk.curves.iter().find_map(|c| match &c.kind {
    clf_sketch::CurveKind::Spline { points, .. } => Some(points.clone()),
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
