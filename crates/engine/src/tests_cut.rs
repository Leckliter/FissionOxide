//! Tests for the 2D cutting commands (laser, plasma, waterjet).

use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn hub(name: &str) -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("fox-cut-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

/// A 100 × 50 sheet, 2 thick, with a 20 mm hole at (30, 25) and a bend up along the far edge.
fn sheet(s: &mut Session) {
  run(s, "sheet.rule", json!({"name": "Steel", "thickness": 2, "bend_radius": 3, "k_factor": 0.5}));
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [100, 50]}));
  run(s, "sheet.flange", json!({}));
  run(s, "sketch.create", json!({"plane": "xy"}));
  run(s, "sketch.circle", json!({"center": [30, 25], "radius": 10}));
  run(s, "extrude", json!({"distance": 10, "extent": "symmetric", "operation": "cut"}));
  run(s, "sheet.edge_flange", json!({"edges": [{"point": [50, 50, 2]}], "height": 20}));
}

#[test]
fn flat_pattern_to_gcode() {
  let mut s = Session { store_root: Some(hub("flat")), ..Default::default() };
  sheet(&mut s);
  let c = run(&mut s, "cut.contours", json!({}));
  let cs = c["contours"].as_array().unwrap();
  assert_eq!(cs.len(), 2, "{c}");
  assert_eq!(cs.iter().filter(|c| c["hole"] == true).count(), 1);
  // lower-left corner at the origin
  let min = cs.iter().find(|c| c["hole"] == false).unwrap()["min"].clone();
  assert!(min[0].as_f64().unwrap().abs() < 1e-6 && min[1].as_f64().unwrap().abs() < 1e-6, "{min}");

  let g = run(&mut s, "cut.gcode", json!({"machine": "grbl_laser", "kerf": 0.2, "power": 50}));
  assert_eq!(g["contours"], 2);
  assert_eq!(g["holes"], 1);
  assert_eq!(g["pierces"], 2);
  let text = g["gcode"].as_str().unwrap();
  assert!(text.contains("M4 S500"), "{text}");
  // the hole is round: arcs of radius 10 - 0.1
  assert!(text.lines().any(|l| l.starts_with("G3")));
  // hole first: the first pierce is inside the hole
  let first = text.lines().find(|l| l.starts_with("G0 X")).unwrap();
  let xy: Vec<f64> = first.split_whitespace().skip(1).map(|w| w[1..].parse().unwrap()).collect();
  let hole = cs.iter().find(|c| c["hole"] == true).unwrap();
  let f = |v: &Value, k: usize| v[k].as_f64().unwrap();
  let centre = [(f(&hole["min"], 0) + f(&hole["max"], 0)) / 2.0, (f(&hole["min"], 1) + f(&hole["max"], 1)) / 2.0];
  assert!((f(&hole["max"], 0) - f(&hole["min"], 0) - 20.0).abs() < 1e-6, "{hole}");
  let near = (xy[0] - centre[0]).hypot(xy[1] - centre[1]) < 10.0;
  assert!(near, "first pierce {xy:?} inside the hole");

  // to a file, plasma with micro-joints, inches
  let dir = hub("post");
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("plate.ngc");
  let r = run(
    &mut s,
    "cut.gcode",
    json!({"machine": "qtplasmac", "process": "6 mm mild steel, 45 A", "tabs": 2, "units": "inch", "path": path}),
  );
  assert_eq!(r["pierces"], 4, "{r}");
  let text = std::fs::read_to_string(&path).unwrap();
  assert!(text.contains("G20") && text.contains("M3 $0 S1"));
  // R12 DXF of the part geometry
  let d = run(&mut s, "cut.gcode", json!({"machine": "dxf"}));
  assert!(d["gcode"].as_str().unwrap().contains("AC1009"));
  assert!(s.execute("cut.gcode", &json!({"machine": "nope"})).is_err());
  assert!(s.execute("cut.gcode", &json!({"process": "unobtainium"})).is_err());
}

#[test]
fn sketch_and_faces() {
  let mut s = Session { store_root: Some(hub("faces")), ..Default::default() };
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [60, 40]}));
  run(&mut s, "sketch.circle", json!({"center": [20, 20], "radius": 5}));
  let id = s.edit_sketch.unwrap().0;
  let r = run(&mut s, "cut.gcode", json!({"sketch": id, "machine": "wazer", "kerf": 1}));
  assert_eq!(r["contours"], 2, "{r}");
  assert_eq!(r["holes"], 1);
  assert!(r["time_s"].as_f64().unwrap() > 0.0);
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "extrude", json!({"sketch": id, "profiles": [[50, 30]], "distance": 5}));
  let r = run(&mut s, "cut.contours", json!({"faces": [{"point": [50, 30, 5]}]}));
  assert_eq!(r["contours"].as_array().unwrap().len(), 2, "{r}");
}

#[test]
fn machines_in_the_hub() {
  let mut s = Session { store_root: Some(hub("machines")), ..Default::default() };
  let builtin = run(&mut s, "cut.machines", json!({})).as_array().unwrap().len();
  assert!(builtin >= 6);
  run(
    &mut s,
    "cut.machine.add",
    json!({"machine": {"id": "shop_laser", "name": "Shop laser", "header": ["G21 G90"], "pierce": ["M3 S{power}"],
      "off": ["M5"], "footer": ["M2"], "processes": [{"name": "birch 4 mm", "kerf": 0.18, "feed": 450}]}}),
  );
  assert_eq!(run(&mut s, "cut.machines", json!({})).as_array().unwrap().len(), builtin + 1);
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
  let id = s.edit_sketch.unwrap().0;
  let g = run(&mut s, "cut.gcode", json!({"sketch": id, "machine": "shop_laser"}));
  let text = g["gcode"].as_str().unwrap();
  assert!(text.starts_with("G21 G90\n") && text.contains("F450"), "{text}");
  run(&mut s, "cut.machine.delete", json!({"id": "shop_laser"}));
  assert_eq!(run(&mut s, "cut.machines", json!({})).as_array().unwrap().len(), builtin);
}

/// A 100 × 60 plate sketch with a hole of radius `r` (a parameter) at (30, 30).
fn plate_sketch(s: &mut Session) -> u64 {
  run(s, "param.add", json!({"name": "r", "expr": "10 mm"}));
  let id = run(s, "sketch.create", json!({"plane": "xy"}))["sketch"].as_u64().unwrap();
  run(s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [100, 60]}));
  let c = run(s, "sketch.circle", json!({"center": [30, 30], "radius": 10}));
  let curve = c["curves"][0].as_u64().or(c["curve"].as_u64()).unwrap_or_else(|| panic!("{c}"));
  run(s, "sketch.dimension", json!({"type": "radius", "curves": [curve], "value": "r"}));
  run(s, "sketch.finish", json!({}));
  id
}

#[test]
fn nesting_to_sheets() {
  let mut s = Session { store_root: Some(hub("nest")), ..Default::default() };
  let id = plate_sketch(&mut s);
  // 3 x 2 plates fit a 320 x 140 sheet (7 would need more area): 8 copies take two sheets
  let p = json!({"sketch": id, "machine": "grbl_laser", "kerf": 0.2, "nest": {"sheet": "320 x 140"}, "quantity": 8});
  let n = run(&mut s, "cut.nest", p.clone());
  assert_eq!(n["sheets"].as_array().unwrap().len(), 2, "{n}");
  assert_eq!(n["sheets"][0]["parts"], 6);
  assert_eq!(n["parts"][0]["placed"], 8);
  assert!(n["unplaced"].as_array().unwrap().is_empty());
  let u = n["utilisation"].as_f64().unwrap();
  let area = 6000.0 - 100.0 * std::f64::consts::PI;
  assert!((u - 8.0 * area / (2.0 * 320.0 * 140.0)).abs() < 1e-3, "{u}");
  assert_eq!(n["cut"].as_array().unwrap().len(), 2);

  // G-code: one program per sheet
  let g = run(&mut s, "cut.gcode", p.clone());
  assert_eq!(g["sheets"], 2);
  assert_eq!(g["programs"].as_array().unwrap().len(), 2);
  assert_eq!(g["contours"], 16, "{g}");
  assert_eq!(g["pierces"], 16);
  assert!(g["gcode"].as_str().unwrap().contains("sheet 1 of 2"));
  let mut second = p.clone();
  second["sheet"] = json!(2);
  let two = run(&mut s, "cut.gcode", second.clone());
  assert_eq!(two["contours"], 4, "{two}");
  assert!(two["gcode"].as_str().unwrap().contains("sheet 2 of 2"));
  second["sheet"] = json!(3);
  assert!(s.execute("cut.gcode", &second).is_err());

  // to files: plate-sheet1.gcode, plate-sheet2.gcode; DXF with the sheet outline
  let dir = hub("nest-files");
  std::fs::create_dir_all(&dir).unwrap();
  let mut w = p.clone();
  w["path"] = json!(dir.join("plate.gcode"));
  let r = run(&mut s, "cut.gcode", w);
  let paths = r["paths"].as_array().unwrap();
  assert_eq!(paths.len(), 2);
  assert!(paths[1].as_str().unwrap().ends_with("plate-sheet2.gcode"));
  assert!(dir.join("plate-sheet1.gcode").exists() && dir.join("plate-sheet2.gcode").exists());
  let mut d = p.clone();
  d["machine"] = json!("dxf");
  d["sheet"] = json!(1);
  let dxf = run(&mut s, "cut.gcode", d);
  assert!(dxf["gcode"].as_str().unwrap().contains("\nSHEET\n"));

  // a second part with its own quantity, rotations off, and errors
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 15}));
  let disc = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.finish", json!({}));
  let mixed = json!({"sketch": id, "quantity": 2, "parts": [{"sketch": disc, "quantity": 5, "name": "disc"}],
    "nest": {"sheet": [320, 140], "rotations": 1, "spacing": 4}});
  let m = run(&mut s, "cut.nest", mixed);
  assert_eq!(m["sheets"].as_array().unwrap().len(), 1, "{m}");
  assert_eq!(m["parts"][1]["name"], "disc");
  assert_eq!(m["parts"][1]["placed"], 5);
  for pl in m["sheets"][0]["placements"].as_array().unwrap() {
    assert_eq!(pl["rotation"].as_f64().unwrap(), 0.0);
  }
  let big = json!({"sketch": id, "nest": {"sheet": [80, 80]}});
  let b = run(&mut s, "cut.nest", big);
  assert_eq!(b["unplaced"][0]["count"], 1, "{b}");
  assert!(s.execute("cut.gcode", &json!({"sketch": id, "quantity": 3})).is_err(), "quantity needs a nest");
  assert!(s.execute("cut.nest", &json!({"sketch": id, "kerf": 2, "nest": {"spacing": 1}})).is_err());
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cut_jobs_follow_the_design() {
  let mut s = Session { store_root: Some(hub("jobs")), ..Default::default() };
  let id = plate_sketch(&mut s);
  let undo = s.undo.len();
  let j = run(&mut s, "cut.job.create", json!({"name": "Plate", "sketch": id, "machine": "wazer", "kerf": 1}));
  assert_eq!(j["job"], 1);
  assert_eq!(s.design.cut_jobs.len(), 1);
  assert_eq!(s.undo.len(), undo + 1, "an undo step");
  let before = run(&mut s, "cut.job.preview", json!({"job": "plate"}));
  let cp = s.cut_preview.as_ref().unwrap();
  assert_eq!(cp.revision, s.revision);
  assert!(cp.lines.iter().any(|l| l.kind == crate::CutLineKind::Hole));
  assert!(cp.lines.iter().any(|l| l.kind == crate::CutLineKind::Lead));
  assert!(cp.lines.iter().any(|l| l.kind == crate::CutLineKind::Rapid));
  // drawn in the viewport while it matches the design
  let opts = crate::scene::view_options(&s, &json!({"cut": true})).unwrap();
  assert!(opts.cut);

  // the hole grows: the stored job follows
  run(&mut s, "param.set", json!({"name": "r", "expr": "15 mm"}));
  assert_ne!(s.cut_preview.as_ref().unwrap().revision, s.revision, "the preview is stale");
  let after = run(&mut s, "cut.job.preview", json!({}));
  let len = |v: &Value| v["cut_length"].as_f64().unwrap();
  assert!(len(&after) - len(&before) > 2.0 * std::f64::consts::PI * 4.0, "{} → {}", len(&before), len(&after));
  // posted from the job, with a per-run override
  let g = run(&mut s, "cut.gcode", json!({"job": 1, "units": "inch"}));
  assert!(g["gcode"].as_str().unwrap().contains("G20"));
  assert_eq!(g["contours"], 2);

  // edit: nest it; null removes a setting
  // (at most five 100 x 60 plates fit a 250 x 150 sheet by area)
  run(&mut s, "cut.job.edit", json!({"job": 1, "nest": {"sheet": [250, 150]}, "quantity": 6, "kerf": null}));
  assert!(s.design.cut_jobs[0].params.get("kerf").is_none());
  let n = run(&mut s, "cut.job.preview", json!({}));
  let sheets = n["nest"]["sheets"].as_array().unwrap().len();
  assert!(sheets >= 2, "{n}");
  assert_eq!(n["nest"]["parts"][0]["placed"], 6);
  let drawn = s.cut_preview.as_ref().unwrap().lines.iter().filter(|l| l.kind == crate::CutLineKind::Sheet).count();
  assert_eq!(drawn, sheets);
  run(&mut s, "cut.job.edit", json!({"job": 1, "nest": null, "quantity": null, "name": "Plate A"}));
  assert_eq!(s.design.cut_jobs[0].name, "Plate A");

  // kept in the file
  let text = serde_json::to_string(&s.design).unwrap();
  let back: fox_model::Design = serde_json::from_str(&text).unwrap();
  assert_eq!(back.cut_jobs, s.design.cut_jobs);
  assert_eq!(run(&mut s, "cut.job.list", json!({})).as_array().unwrap().len(), 1);

  run(&mut s, "cut.job.delete", json!({"job": "Plate A"}));
  assert!(s.design.cut_jobs.is_empty() && s.cut_preview.is_none());
  assert!(s.execute("cut.job.preview", &json!({})).is_err());
  run(&mut s, "undo", json!({}));
  assert_eq!(s.design.cut_jobs.len(), 1);
}

/// The start of the first cutting move of the pass that cuts the outline (the contour not a
/// hole), from the job's G-code-free summary: taken from the preview's outline line.
fn outline_start(s: &Session) -> fox_kernel::glam::DVec3 {
  let cp = s.cut_preview.as_ref().unwrap();
  cp.lines.iter().find(|l| l.kind == crate::CutLineKind::Outline).unwrap().points[0]
}

#[test]
fn lead_positions_picked_per_contour() {
  let mut s = Session { store_root: Some(hub("leads")), ..Default::default() };
  let id = plate_sketch(&mut s);
  run(&mut s, "cut.job.create", json!({"sketch": id, "machine": "wazer", "kerf": 1}));
  run(&mut s, "cut.job.preview", json!({}));
  let first = outline_start(&s);
  // pick on the outline's top edge, in the model (the sketch is on XY at the origin)
  run(&mut s, "cut.job.lead", json!({"point": [70.0, 60.0, 0.0]}));
  run(&mut s, "cut.job.preview", json!({}));
  let p = outline_start(&s);
  assert!((p.x - 70.0).abs() < 1e-6 && (p.y - 60.5).abs() < 1e-6, "{p:?} (was {first:?})");
  // a second pick on the same contour replaces the first; one in the hole adds a point
  run(&mut s, "cut.job.lead", json!({"point": [0.0, 20.0, 0.0]}));
  run(&mut s, "cut.job.lead", json!({"point": [30.0, 22.0, 0.0]}));
  let leads = s.design.cut_jobs[0].params["lead_points"].as_array().unwrap().clone();
  assert_eq!(leads.len(), 2, "{leads:?}");
  run(&mut s, "cut.job.preview", json!({}));
  let p = outline_start(&s);
  assert!((p.x + 0.5).abs() < 1e-6 && (p.y - 20.0).abs() < 1e-6, "{p:?}");
  let cp = s.cut_preview.as_ref().unwrap();
  let hole = cp.lines.iter().find(|l| l.kind == crate::CutLineKind::Hole).unwrap().points[0];
  assert!((hole.x - 30.0).abs() < 1e-6 && (hole.y - 20.5).abs() < 1e-6, "{hole:?}");
  // the picks stay on their contours when the design changes
  run(&mut s, "param.set", json!({"name": "r", "expr": "12 mm"}));
  run(&mut s, "cut.job.preview", json!({}));
  let cp = s.cut_preview.as_ref().unwrap();
  let hole = cp.lines.iter().find(|l| l.kind == crate::CutLineKind::Hole).unwrap().points[0];
  assert!((hole.x - 30.0).abs() < 1e-6 && (hole.y - 18.5).abs() < 1e-6, "{hole:?}");

  // nested: a pick on a copy goes back to the part, so every copy starts there
  run(&mut s, "cut.job.lead", json!({"clear": true}));
  run(&mut s, "cut.job.edit", json!({"nest": {"sheet": [250, 150], "rotations": 1}, "quantity": 4}));
  run(&mut s, "cut.job.preview", json!({}));
  let n = run(&mut s, "cut.job.list", json!({}));
  assert!(n[0]["params"].get("lead_points").is_none());
  let job = run(&mut s, "cut.nest", json!({"job": 1}));
  let off = job["sheets"][0]["placements"][1]["offset"].clone();
  let (ox, oy) = (off[0].as_f64().unwrap(), off[1].as_f64().unwrap());
  // the copy's left edge, 20 up from its corner (sheet coordinates = model, the sketch's
  // lower-left corner being the origin)
  run(&mut s, "cut.job.lead", json!({"point": [ox, oy + 20.0, 0.0]}));
  let leads = s.design.cut_jobs[0].params["lead_points"].as_array().unwrap().clone();
  assert_eq!(leads.len(), 1);
  assert!((leads[0][0].as_f64().unwrap()).abs() < 1e-6 && (leads[0][1].as_f64().unwrap() - 20.0).abs() < 1e-6, "{leads:?}");
  let g = run(&mut s, "cut.gcode", json!({"job": 1, "sheet": 1}));
  let text = g["gcode"].as_str().unwrap();
  // every outline starts on its copy's left edge: a G1 to x = offset - 0.5 after each lead-in
  let starts = text.lines().filter(|l| l.starts_with("G0 ")).count();
  assert_eq!(starts, g["pierces"].as_u64().unwrap() as usize);
}
