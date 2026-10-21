//! Tests for Scale to Size / Fit in Box (`sketch.scale_to`) and the sketch library.

use serde_json::{Value, json};

use crate::Session;
use crate::store;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn near(a: f64, b: f64) -> bool {
  (a - b).abs() < 1e-6 * b.abs().max(1.0)
}

/// Bounds of the edited sketch (min, max).
fn bounds(s: &Session) -> ([f64; 2], [f64; 2]) {
  let sk = s.edit_sketch.unwrap();
  s.model.sketches[&sk].sketch.bounds().unwrap()
}

fn size_of(v: &Value) -> [f64; 2] {
  [v["size"][0].as_f64().unwrap(), v["size"][1].as_f64().unwrap()]
}

#[test]
fn scale_to_width_measures_the_curves_true_extent() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
  // a half circle bulging 5 past its ends at x = 25: its end points alone would say 20 wide
  run(&mut s, "sketch.arc", json!({"center": [20, 5], "start": [20, 0], "end": [20, 10]}));
  let r = run(&mut s, "sketch.scale_to", json!({"mode": "width", "width": 50, "anchor": "corner"}));
  assert!(near(r["before"][0].as_f64().unwrap(), 25.0), "{r}");
  assert!(near(r["scale"].as_f64().unwrap(), 2.0), "{r}");
  let [w, h] = size_of(&r);
  assert!(near(w, 50.0) && near(h, 20.0), "{r}");
  // the corner stays
  let (lo, hi) = bounds(&s);
  assert!(near(lo[0], 0.0) && near(lo[1], 0.0) && near(hi[0], 50.0), "{lo:?} {hi:?}");

  // height, about the origin, in inches with a margin on each side
  let r = run(
    &mut s,
    "sketch.scale_to",
    json!({"mode": "height", "height": "2 in", "margin": "0.25 in", "anchor": "origin"}),
  );
  assert!(near(size_of(&r)[1], 1.5 * 25.4), "{r}");
  let (lo, _) = bounds(&s);
  assert!(near(lo[0], 0.0) && near(lo[1], 0.0), "the origin anchor stays put: {lo:?}");

  // nothing to scale to
  assert!(s.execute("sketch.scale_to", &json!({"mode": "width", "width": 0})).is_err());
  assert!(s.execute("sketch.scale_to", &json!({"mode": "box", "box": [20, 20], "margin": 10})).is_err());
  assert!(s.execute("sketch.scale_to", &json!({"mode": "diagonal"})).is_err());
}

#[test]
fn fit_in_box_applies_the_best_rotation() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  // a 30 x 2 bar: flat, it fits a 20 box at 2/3 scale; turned 45° it grows to 20 / (32 / √2)
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [30, 2]}));
  let flat = run(&mut s, "sketch.scale_to", json!({"mode": "box", "box": [20, 20]}));
  assert!(near(flat["scale"].as_f64().unwrap(), 20.0 / 30.0) && flat["angle"] == json!(0.0), "{flat}");
  run(&mut s, "undo", json!({}));
  let c0 = {
    let (lo, hi) = bounds(&s);
    [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]
  };
  let r = run(&mut s, "sketch.scale_to", json!({"mode": "box", "box": [20, 20], "rotate": true}));
  let angle = r["angle"].as_f64().unwrap();
  assert!((angle - 45.0).abs() < 0.02, "{r}");
  assert!((r["scale"].as_f64().unwrap() - 20.0 * 2f64.sqrt() / 32.0).abs() < 1e-4, "{r}");
  let [w, h] = size_of(&r);
  assert!(w <= 20.0 + 1e-6 && h <= 20.0 + 1e-6 && w.max(h) > 19.99, "{r}");
  // about the centroid: the middle stays
  let (lo, hi) = bounds(&s);
  let c1 = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
  assert!((c0[0] - c1[0]).abs() < 1e-6 && (c0[1] - c1[1]).abs() < 1e-6, "{c0:?} {c1:?}");
  // rotated, the corner anchor keeps the box's lower left corner
  run(&mut s, "undo", json!({}));
  let r = run(&mut s, "sketch.scale_to", json!({"mode": "box", "box": [20, 20], "rotate": true, "anchor": "corner"}));
  let (lo, _) = bounds(&s);
  assert!(lo[0].abs() < 1e-6 && lo[1].abs() < 1e-6, "{lo:?} {r}");
}

#[test]
fn scale_to_scales_circles_ellipses_splines_and_text() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let sk = s.edit_sketch.unwrap();
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 5}));
  run(&mut s, "sketch.ellipse", json!({"center": [20, 0], "major": [26, 0], "minor": 3}));
  run(&mut s, "sketch.spline", json!({"points": [[0, 10], [10, 16], [20, 10]]}));
  run(&mut s, "sketch.text", json!({"at": [0, -20], "text": "Hi", "height": 5}));
  let before = run(&mut s, "sketch.info", json!({}));
  let n = before["curves"].as_array().map(Vec::len);
  let r = run(&mut s, "sketch.scale_to", json!({"box": [100, 100], "anchor": "origin"}));
  let k = r["scale"].as_f64().unwrap();
  assert!(k > 1.5, "{r}");
  let [w, h] = size_of(&r);
  assert!(w.max(h) > 99.99 && w <= 100.0 + 1e-6 && h <= 100.0 + 1e-6, "{r}");
  let sketch = &s.model.sketches[&sk].sketch;
  let circle = sketch.curves.iter().find(|c| matches!(c.kind, fox_sketch::CurveKind::Circle { .. })).unwrap();
  assert!(near(sketch.radius(circle.id).unwrap(), 5.0 * k), "{} vs {} {r}", sketch.radius(circle.id).unwrap(), 5.0 * k);
  assert!(
    sketch
      .curves
      .iter()
      .any(|c| matches!(c.kind, fox_sketch::CurveKind::Ellipse { minor, .. } if near(minor, 3.0 * k)))
  );
  assert!(
    near(sketch.texts[0].height, 5.0 * k),
    "the text grows with the rest: {} vs {} {r}",
    sketch.texts[0].height,
    5.0 * k
  );
  assert_eq!(run(&mut s, "sketch.info", json!({}))["curves"].as_array().map(Vec::len), n);
}

fn write_part(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
  // a 10 x 4 slot-ish part centred on its origin, written as a DXF
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [-5, -2], "corner2": [5, 2]}));
  run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 1}));
  let sk = s.edit_sketch.unwrap().0;
  std::fs::create_dir_all(dir).unwrap();
  let path = dir.join(name);
  run(&mut s, "export", json!({"path": path, "sketch": sk}));
  path
}

#[test]
fn library_parts_from_several_roots_go_where_you_click() {
  let hub = store::Store::scratch_root();
  write_part(&hub.join("library").join("Hardware"), "slot.dxf");
  write_part(&hub.join("library").join("Hardware").join("studs"), "stud.dxf");
  // an add-in's pack folder: its parts sit directly in it
  let addin = std::env::temp_dir().join(format!("fox-lib-{}", std::process::id())).join("SignPack");
  write_part(&addin, "keyhole.dxf");
  std::fs::write(addin.join("notes.txt"), "not a part").unwrap();

  let mut s = Session { store_root: Some(hub.clone()), ..Session::default() };
  let folders = json!([addin]);
  let l = run(&mut s, "sketch.library.list", json!({"folders": folders}));
  let ids: Vec<&str> = l["parts"].as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap()).collect();
  assert_eq!(ids, ["Hardware/slot", "Hardware/studs/stud", "SignPack/keyhole"], "{l}");
  assert_eq!(l["packs"], json!([{"name": "Hardware", "parts": 2}, {"name": "SignPack", "parts": 1}]));
  let part = &l["parts"][0];
  assert!(near(part["size"][0].as_f64().unwrap(), 10.0) && near(part["size"][1].as_f64().unwrap(), 4.0), "{part}");
  assert!(!part["outline"].as_array().unwrap().is_empty());

  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let r = run(&mut s, "sketch.library.insert", json!({"part": "Hardware/slot", "at": [100, 50], "angle": 90}));
  assert_eq!(r["curves"].as_array().unwrap().len(), 5, "{r}");
  let (lo, hi) = bounds(&s);
  assert!(near(lo[0], 98.0) && near(hi[0], 102.0) && near(lo[1], 45.0) && near(hi[1], 55.0), "{lo:?} {hi:?}");
  // again, by its bare name and scaled, from the add-in's pack
  run(&mut s, "sketch.library.insert", json!({"part": "keyhole", "at": [0, 0], "scale": 2, "folders": folders}));
  let (lo, _) = bounds(&s);
  assert!(near(lo[0], -10.0) && near(lo[1], -4.0), "{lo:?}");
  // a part added for the whole process (as an add-in does) needs no folders
  crate::library::add_root(addin.clone());
  run(&mut s, "sketch.library.insert", json!({"part": "SignPack/keyhole", "at": [0, 0]}));
  crate::library::remove_root(&addin);
  assert!(s.execute("sketch.library.insert", &json!({"part": "nope", "at": [0, 0]})).is_err());
  assert!(s.execute("sketch.library.insert", &json!({"part": "Hardware/slot"})).is_err());
  let _ = std::fs::remove_dir_all(addin.parent().unwrap());
}
