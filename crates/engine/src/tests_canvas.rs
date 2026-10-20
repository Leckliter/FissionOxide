//! Canvases: reference images on a plane, their edits and Calibrate.

use super::*;
use fox_model::FeatureKind;
use serde_json::{Value, json};

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
  let d = std::env::temp_dir().join(format!("fox-canvas-{tag}-{}", std::process::id()));
  std::fs::create_dir_all(&d).unwrap();
  d
}

/// A 200 × 100 px image.
fn image(dir: &std::path::Path) -> String {
  let p = dir.join("photo.png");
  image::RgbImage::from_pixel(200, 100, image::Rgb([200, 120, 40])).save(&p).unwrap();
  p.to_string_lossy().into_owned()
}

fn canvas_of(s: &Session, id: u64) -> FeatureKind {
  s.design.feature(fox_model::FeatureId(id as u32)).unwrap().kind.clone()
}

fn close(a: f64, b: f64) -> bool {
  (a - b).abs() < 1e-9
}

#[test]
fn calibrate_scales_about_the_first_point() {
  // 100 × 50 centred at the origin; the picked points are 20 apart but really 50
  let (at, w, h) = canvas::calibrate([0.0, 0.0], 100.0, 50.0, [10.0, 0.0], [30.0, 0.0], 50.0).unwrap();
  assert!(close(w, 250.0) && close(h, 125.0), "{w} {h}");
  // the first point stays put: it was 10 right of the centre, now 25
  assert!(close(at[0], 10.0 - 25.0) && close(at[1], 0.0), "{at:?}");
  // diagonal picks: 3-4-5
  let (at, w, _) = canvas::calibrate([5.0, 5.0], 40.0, 20.0, [0.0, 0.0], [3.0, 4.0], 10.0).unwrap();
  assert!(close(w, 80.0) && close(at[0], 10.0) && close(at[1], 10.0), "{w} {at:?}");
  // shrinking works too
  let (_, w, _) = canvas::calibrate([0.0, 0.0], 100.0, 50.0, [0.0, 0.0], [0.0, 40.0], 10.0).unwrap();
  assert!(close(w, 25.0), "{w}");
  assert!(canvas::calibrate([0.0, 0.0], 1.0, 1.0, [1.0, 1.0], [1.0, 1.0], 5.0).is_err());
  assert!(canvas::calibrate([0.0, 0.0], 1.0, 1.0, [0.0, 0.0], [1.0, 1.0], 0.0).is_err());
}

#[test]
fn canvas_inserts_edits_calibrates_and_saves() {
  let dir = temp_dir("roundtrip");
  let img = image(&dir);
  let mut s = Session::default();
  let r = run(&mut s, "canvas.insert", json!({"image": img, "plane": "xz", "width": 80}));
  assert!(r["error"].is_null(), "{r}");
  let id = r["feature"].as_u64().unwrap();
  // the height follows the image's 2:1 aspect; the canvas lies on its plane and adds no bodies
  match canvas_of(&s, id) {
    FeatureKind::Canvas { width, height, opacity, .. } => {
      assert!(close(width, 80.0) && close(height, 40.0) && close(opacity, 1.0));
    }
    k => panic!("{k:?}"),
  }
  let plane = s.model.canvases.get(&fox_model::FeatureId(id as u32)).copied().unwrap();
  assert!(close(plane.x.cross(plane.y).y.abs(), 1.0));
  assert!(s.model.bodies.is_empty());

  // Edit Canvas: opacity (clamped) and width, keeping the aspect
  run(&mut s, "canvas.edit", json!({"feature": id, "opacity": 0.4, "width": 100}));
  run(&mut s, "canvas.edit", json!({"feature": id, "opacity": 7}));
  // Calibrate: two points 25 mm apart on the image are really 50 mm apart
  let r = run(&mut s, "canvas.calibrate", json!({"feature": id, "from": [0, 0], "to": [25, 0], "distance": 50}));
  assert!(close(r["width"].as_f64().unwrap(), 200.0), "{r}");

  // persisted in the design file
  let path = dir.join("canvas.fox");
  s.save_to(&path).unwrap();
  let back = Session::open(&path).unwrap();
  match canvas_of(&back, id) {
    FeatureKind::Canvas { width, height, opacity, image, .. } => {
      assert!(close(width, 200.0) && close(height, 100.0) && close(opacity, 1.0), "{width} {height} {opacity}");
      assert_eq!(image, img);
    }
    k => panic!("{k:?}"),
  }
  assert!(back.model.canvases.contains_key(&fox_model::FeatureId(id as u32)));

  // undo goes back through the calibration
  s.undo();
  match canvas_of(&s, id) {
    FeatureKind::Canvas { width, .. } => assert!(close(width, 100.0), "{width}"),
    k => panic!("{k:?}"),
  }
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn canvas_commands_reject_bad_input() {
  let dir = temp_dir("bad");
  let img = image(&dir);
  let mut s = Session::default();
  assert!(s.execute("canvas.insert", &json!({"image": dir.join("missing.png")})).is_err());
  assert!(s.execute("canvas.insert", &json!({"image": img, "width": -3})).is_err());
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let sketch = s.design.features[0].id.0;
  assert!(s.execute("canvas.edit", &json!({"feature": sketch, "opacity": 0.5})).is_err());
  let id = run(&mut s, "canvas.insert", json!({"image": img}))["feature"].clone();
  assert!(s.execute("canvas.calibrate", &json!({"feature": id, "from": [1, 1], "to": [1, 1], "distance": 4})).is_err());
  let _ = std::fs::remove_dir_all(dir);
}
