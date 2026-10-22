//! Tests for the design's flat pattern (Create Flat Pattern), Export Flat Pattern's DXF options
//! and the flat pattern environment's canvas.

use fox_kernel::SurfaceKind;
use serde_json::{Value, json};

use crate::Session;
use crate::flat::FlatShow;
use crate::scene::{SceneBuilder, ViewOptions};

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

/// A base flange 80 x 40 with a 15 mm edge flange on its back edge, and its bottom face.
fn sheet() -> (Session, Value) {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 40]}));
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "sheet.flange", json!({}));
  run(&mut s, "sheet.edge_flange", json!({"edges": [{"point": [40, 40, 1]}], "height": 15}));
  let b = &s.model.bodies[0];
  let f = b.faces().iter().find(|f| f.kind == SurfaceKind::Plane && f.normal.z < -0.9).unwrap().id;
  let face = json!({"body": b.id.0, "face": f});
  (s, face)
}

#[test]
fn create_and_delete_the_flat_pattern() {
  let (mut s, face) = sheet();
  let features = s.design.features.len();
  let bend = s.model.bodies[0].faces().iter().find(|f| f.kind == SurfaceKind::Cylinder).unwrap().id;
  let body = s.model.bodies[0].id.0;
  assert!(s.execute("sheet.create_flat_pattern", &json!({"face": {"body": body, "face": bend}})).is_err());
  assert!(s.execute("sheet.create_flat_pattern", &json!({})).is_err());
  run(&mut s, "sheet.create_flat_pattern", json!({"face": face}));
  // kept with the design, not in the timeline
  assert_eq!(s.design.features.len(), features);
  assert_eq!(s.design.flat_pattern.as_ref().map(|f| f.body.0), Some(body));
  // sheet.flat_pattern reads it
  let size = run(&mut s, "sheet.flat_pattern", json!({}))["size"].clone();
  assert!(size[0].as_f64().unwrap() > 79.0, "{size}");
  // saved with the design
  let text = serde_json::to_string(&s.design).unwrap();
  let back: fox_model::Design = serde_json::from_str(&text).unwrap();
  assert_eq!(back.flat_pattern, s.design.flat_pattern);
  run(&mut s, "sheet.delete_flat_pattern", json!({}));
  assert!(s.design.flat_pattern.is_none());
  assert!(s.execute("sheet.delete_flat_pattern", &json!({})).is_err());
  assert!(s.undo());
  assert!(s.design.flat_pattern.is_some());
}

#[test]
fn flat_pattern_dxf_options() {
  let (mut s, face) = sheet();
  run(&mut s, "sheet.create_flat_pattern", json!({"face": face}));
  let dir = std::env::temp_dir().join(format!("fox-flat-opts-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("a.dxf");
  let read = |s: &mut Session, p: Value| {
    let mut p = p;
    p["path"] = json!(path);
    run(s, "sheet.flat_pattern", p);
    std::fs::read_to_string(&path).unwrap()
  };
  // as before: centre lines, no extent lines, millimetres
  let plain = read(&mut s, json!({}));
  assert!(plain.contains("BEND_DOWN") || plain.contains("BEND_UP"));
  assert!(!plain.contains("BEND_EXTENT"));
  assert!(plain.contains("$INSUNITS\n70\n4\n"));
  // extent lines: the two edges of the bend zone, on their own layer
  let ext = read(&mut s, json!({"extent_lines": true}));
  let n = ext.matches("LINE\n8\nBEND_EXTENT").count();
  assert_eq!(n, 2, "{ext}");
  // no centre lines: no bend line and no bend note
  let none = read(&mut s, json!({"center_lines": false}));
  assert!(!none.contains("LINE\n8\nBEND_") && !none.contains("%%d"));
  // centimetres: the width shrinks ten times
  let cm = read(&mut s, json!({"units": "cm"}));
  assert!(cm.contains("$INSUNITS\n70\n5\n"));
  let max_x = |t: &str| {
    let l: Vec<&str> = t.lines().collect();
    l.windows(2).filter(|w| w[0] == "10" || w[0] == "11").filter_map(|w| w[1].parse::<f64>().ok()).fold(0.0, f64::max)
  };
  assert!((max_x(&plain) / max_x(&cm) - 10.0).abs() < 1e-6, "{} {}", max_x(&plain), max_x(&cm));
  assert!(s.execute("sheet.flat_pattern", &json!({"path": path, "units": "parsec"})).is_err());
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_environment_draws_the_flat_from_the_front() {
  let (mut s, face) = sheet();
  let folded = SceneBuilder::default().build(&s, &ViewOptions::default()).0;
  let span = |m: &[[f32; 3]], k: usize| {
    let (lo, hi) = m.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[k]), hi.max(p[k])));
    hi - lo
  };
  let all = |sc: &fox_render::Scene| sc.meshes.iter().flat_map(|m| m.positions.clone()).collect::<Vec<_>>();
  // without a flat pattern the option draws the folded body
  let show = Some(FlatShow { center_lines: true, extent_lines: true });
  let opts = ViewOptions { flat: show, ..Default::default() };
  assert_eq!(all(&SceneBuilder::default().build(&s, &opts).0).len(), all(&folded).len());
  run(&mut s, "sheet.create_flat_pattern", json!({"face": face}));
  let (flat, _) = SceneBuilder::default().build(&s, &opts);
  let pts = all(&flat);
  // the flat stands in the XZ plane, as thin as the sheet, longer than the base flange
  let t = s.model.bodies[0].sheet.as_ref().unwrap().thickness as f32;
  assert!((span(&pts, 1) - t).abs() < 1e-3, "{}", span(&pts, 1));
  assert!(span(&pts, 0) > 79.0 && span(&pts, 2) > 50.0, "{} {}", span(&pts, 0), span(&pts, 2));
  // one dash-dot centre line for the one bend, and two extent lines
  let centre = flat.lines.iter().filter(|l| l.color == crate::flat::CENTER_LINE_COLOR).count();
  let extent = flat.lines.iter().filter(|l| l.color == crate::flat::EXTENT_LINE_COLOR).count();
  assert_eq!((centre, extent), (1, 2));
  let off = ViewOptions { flat: Some(FlatShow { center_lines: false, extent_lines: false }), ..Default::default() };
  let (bare, _) = SceneBuilder::default().build(&s, &off);
  assert!(
    bare.lines.iter().all(|l| l.color != crate::flat::CENTER_LINE_COLOR && l.color != crate::flat::EXTENT_LINE_COLOR)
  );
  // the parameter as the app passes it
  let o = crate::scene::view_options(&s, &json!({"flat": {"center_lines": true, "extent_lines": false}})).unwrap();
  assert_eq!(o.flat, Some(FlatShow { center_lines: true, extent_lines: false }));
  let b = crate::flat::flat_bounds(&s).unwrap();
  assert!((b.1.y - b.0.y - f64::from(t)).abs() < 1e-3);
}
