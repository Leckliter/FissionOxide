//! Tests for editable text, Break, tangent circles, blend curves, spline constraints and the
//! modify tools on splines and ellipses.

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

fn ids(v: &Value) -> Vec<u64> {
  v.as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default()
}

#[test]
fn editable_text_regenerates_its_extrusion() {
  let mut s = Session::default();
  let sid = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].as_u64().unwrap();
  let fid = fox_model::FeatureId(sid as u32);
  let t = run(&mut s, "sketch.text", json!({"at": [0, 0], "text": "I", "height": 10}));
  let text = t["text"].as_u64().unwrap();
  assert_eq!(t["profiles"], 1, "{t}");
  run(&mut s, "extrude", json!({"distance": 2}));
  let one = volume(&s);
  assert!(one > 1.0, "{one}");
  // editing the text rebuilds the glyphs and the extrusion follows
  run(&mut s, "sketch.edit", json!({"sketch": sid}));
  let e = run(&mut s, "sketch.text_edit", json!({"text_id": text, "text": "II"}));
  assert!(!ids(&e["curves"]).is_empty());
  let two = volume(&s);
  assert!((two - 2.0 * one).abs() < 0.05 * one, "{one} {two}");
  // moving it, a bigger font, and the info listing
  run(&mut s, "sketch.text_edit", json!({"text_id": text, "height": "20 mm", "at": [50, 0]}));
  assert!(volume(&s) > 3.0 * two);
  let info = run(&mut s, "sketch.info", json!({}));
  assert_eq!(info["texts"][0]["text"], "II");
  assert!((info["texts"][0]["at"][0].as_f64().unwrap() - 50.0).abs() < 1e-9);
  assert!((info["texts"][0]["height"].as_f64().unwrap() - 20.0).abs() < 1e-9);
  // undo goes back one edit
  run(&mut s, "undo", json!({}));
  let info = run(&mut s, "sketch.info", json!({}));
  assert!((info["texts"][0]["height"].as_f64().unwrap() - 10.0).abs() < 1e-9);
  // the glyph points are not drawn as sketch points, the anchor is
  let (scene, _) = crate::scene::SceneBuilder::default().build(&s, &crate::scene::ViewOptions::default());
  assert!(scene.markers.len() < 5, "{}", scene.markers.len());
  // exploding leaves plain curves
  let curves = s.design.sketch(fid).unwrap().curves.len();
  run(&mut s, "sketch.text_explode", json!({"text_id": text}));
  let sk = s.design.sketch(fid).unwrap();
  assert!(sk.texts.is_empty());
  assert_eq!(sk.curves.len(), curves);
  // text can still be added exploded, as before
  let t = run(&mut s, "sketch.text", json!({"at": [0, 40], "text": "o", "height": 5, "explode": true, "font": "mono"}));
  assert!(t.get("text").is_none_or(Value::is_null));
  assert!(s.execute("sketch.text", &json!({"at": [0, 0], "text": "x", "font": "nope"})).is_err());
}

#[test]
fn text_survives_save_and_open() {
  let dir = std::env::temp_dir().join(format!("fox-text-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("text.fox");
  let mut s = Session::default();
  let sid = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].as_u64().unwrap();
  let fid = fox_model::FeatureId(sid as u32);
  run(&mut s, "sketch.text", json!({"at": [5, 5], "text": "Hi", "height": 8, "angle": 30}));
  run(&mut s, "save", json!({"path": path}));
  let mut s2 = Session::default();
  run(&mut s2, "open", json!({"path": path}));
  let a = s.design.sketch(fid).unwrap();
  let b = s2.design.sketch(fid).unwrap();
  assert_eq!(a.texts, b.texts);
  assert!((b.texts[0].angle - 30f64.to_radians()).abs() < 1e-12);
  let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn break_tangent_circles_and_blend_from_commands() {
  let mut s = Session::default();
  run(&mut s, "param.add", json!({"name": "r", "expr": "4 mm"}));
  let sid = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].as_u64().unwrap();
  let fid = fox_model::FeatureId(sid as u32);
  let h = run(&mut s, "sketch.line", json!({"points": [[0, 0], [40, 0]]}))["curves"][0].as_u64().unwrap();
  let v = run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 40]]}))["curves"][0].as_u64().unwrap();
  // 2-tangent circle driven by a parameter
  let c = run(&mut s, "sketch.tangent_circle", json!({"curves": [h, v], "radius": "r", "near": [5, 5]}));
  let circle = c["circle"].as_u64().unwrap();
  run(&mut s, "param.set", json!({"name": "r", "expr": "6 mm"}));
  let sk = s.design.sketch(fid).unwrap();
  let centre = sk.pos(sk.curve_points(fox_sketch::CurveId(circle as u32))[0]).unwrap();
  assert!((centre[0] - 6.0).abs() < 1e-6 && (centre[1] - 6.0).abs() < 1e-6, "{centre:?}");
  // 3-tangent: the incircle of a right triangle
  let d = run(&mut s, "sketch.line", json!({"points": [[40, 0], [0, 40]]}))["curves"][0].as_u64().unwrap();
  let t = run(&mut s, "sketch.tangent_circle", json!({"curves": [h, v, d]}));
  let sk = s.design.sketch(fid).unwrap();
  let r = sk.radius(fox_sketch::CurveId(t["circle"].as_u64().unwrap() as u32)).unwrap();
  assert!((r - (80.0 - 40.0 * 2f64.sqrt()) / 2.0).abs() < 1e-6, "{r}");
  // break the hypotenuse at a point
  let b = run(&mut s, "sketch.break", json!({"curve": d, "at": [30, 10], "at_point": true}));
  assert_eq!(ids(&b["pieces"]).len(), 2);
  // blend two line ends with a curvature-continuous spline
  let l1 = run(&mut s, "sketch.line", json!({"points": [[100, 0], [110, 0]]}))["curves"][0].as_u64().unwrap();
  let l2 = run(&mut s, "sketch.line", json!({"points": [[130, 20], [130, 30]]}))["curves"][0].as_u64().unwrap();
  let bl = run(&mut s, "sketch.blend", json!({"curves": [l1, l2], "near": [[110, 0], [130, 20]]}));
  assert!(bl["blend"].as_u64().is_some());
  let info = run(&mut s, "sketch.info", json!({}));
  let curvature = info["constraints"].as_array().unwrap().iter().filter(|c| c["kind"]["type"] == "curvature").count();
  assert_eq!(curvature, 2, "{}", info["constraints"]);
  assert_eq!(info["solved"], true);
  assert!(s.execute("sketch.blend", &json!({"curves": [l1]})).is_err());
}

#[test]
fn splines_and_ellipses_in_trim_extend_offset_and_constraints() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let sp = run(&mut s, "sketch.spline", json!({"points": [[0, 0], [10, 10], [20, 0], [30, 10]]}))["curves"][0]
    .as_u64()
    .unwrap();
  run(&mut s, "sketch.line", json!({"points": [[15, -10], [15, 20]]}));
  let t = run(&mut s, "sketch.trim", json!({"curve": sp, "at": [25, 5]}));
  assert_eq!(ids(&t["curves"]).len(), 1);
  // a line reaches the trimmed spline
  let l = run(&mut s, "sketch.line", json!({"points": [[-10, 4], [-5, 4]]}))["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.extend", json!({"curve": l, "at": [-5, 4]}));
  let info = run(&mut s, "sketch.info", json!({}));
  assert_eq!(info["solved"], true);
  // an ellipse offset makes a ring and a core
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let e = run(&mut s, "sketch.ellipse", json!({"center": [0, 0], "major": [20, 0], "minor": 10}))["curves"][0]
    .as_u64()
    .unwrap();
  let o = run(&mut s, "sketch.offset", json!({"curves": [e], "distance": 2}));
  assert_eq!(o["profiles"], 2, "{o}");
  run(&mut s, "extrude", json!({"profiles": [[21, 0]], "distance": 1}));
  let ring = volume(&s);
  // ring area between an ellipse and its 2 mm offset ≈ perimeter × 2 + π·2²
  let perimeter = std::f64::consts::PI * (3.0 * 30.0 - ((3.0 * 20.0 + 10.0) * (20.0 + 3.0 * 10.0f64)).sqrt());
  let expect = perimeter * 2.0 + std::f64::consts::PI * 4.0;
  assert!((ring - expect).abs() < 0.01 * expect, "{ring} vs {expect}");
  // a spline tangent to a line, and curvature continuous with an arc
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let l = run(&mut s, "sketch.line", json!({"points": [[-10, 0], [0, 0]]}))["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.constrain", json!({"type": "horizontal", "curves": [l]}));
  let sp = run(&mut s, "sketch.spline", json!({"points": [[0, 0], [10, 8], [20, 0]]}))["curves"][0].as_u64().unwrap();
  run(&mut s, "sketch.constrain", json!({"type": "tangent", "curves": [l, sp]}));
  let a = run(&mut s, "sketch.arc", json!({"center": [25, 0], "start": [20, 0], "end": [30, 0]}))["curves"][0]
    .as_u64()
    .unwrap();
  run(&mut s, "sketch.constrain", json!({"type": "curvature", "curves": [sp, a]}));
  let info = run(&mut s, "sketch.info", json!({}));
  assert_eq!(info["solved"], true);
  let spline = info["curves"].as_array().unwrap().iter().find(|c| c["id"] == sp).unwrap();
  assert!(spline["kind"]["start_handle"].is_u64() && spline["kind"]["end_handle"].is_u64(), "{spline}");
}

#[test]
fn sketch_rectangular_pattern_options() {
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let line = run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 10]]}))["curves"][0].as_u64().unwrap();
  let c = run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 1}))["curves"][0].as_u64().unwrap();
  let centres = |s: &mut Session, made: &Value| -> Vec<[f64; 2]> {
    let info = run(s, "sketch.info", json!({}));
    let curves = info["geometry"].as_array().cloned().unwrap_or_default();
    ids(&made["curves"])
      .iter()
      .map(|id| {
        let cv = curves.iter().find(|v| v["curve"].as_u64() == Some(*id)).unwrap_or_else(|| panic!("{id}: {info}"));
        let b = &cv["bbox"];
        let mid = |k: usize| (b[0][k].as_f64().unwrap() + b[1][k].as_f64().unwrap()) / 2.0;
        [mid(0), mid(1)]
      })
      .collect()
  };
  let close = |got: &[[f64; 2]], want: &[[f64; 2]]| {
    got.len() == want.len() && got.iter().zip(want).all(|(p, q)| (p[0] - q[0]).abs() + (p[1] - q[1]).abs() < 1e-6)
  };
  // Extent along a picked line (start to end is +Y), symmetric, with the second direction
  // given as 90° counter-clockwise from it (-X).
  let made = run(
    &mut s,
    "sketch.pattern_rect",
    json!({"curves": [c], "direction": line, "count": 3, "spacing": 20, "distance_type": "extent",
           "symmetric": true, "direction2": [-1, 0], "count2": 2, "spacing2": 5}),
  );
  let mut got = centres(&mut s, &made);
  got.sort_by(|a, b| a.partial_cmp(b).unwrap());
  let want = [[-5.0, -10.0], [-5.0, 0.0], [-5.0, 10.0], [0.0, -10.0], [0.0, 10.0]];
  assert!(close(&got, &want), "{got:?}");
  // Without direction2 the second direction stays +Y, even when the first is not along +X.
  let made = run(
    &mut s,
    "sketch.pattern_rect",
    json!({"curves": [c], "direction": [0, -1], "count": 2, "spacing": 4, "count2": 2, "spacing2": 7}),
  );
  let mut got = centres(&mut s, &made);
  got.sort_by(|a, b| a.partial_cmp(b).unwrap());
  assert!(close(&got, &[[0.0, -4.0], [0.0, 3.0], [0.0, 7.0]]), "{got:?}");
  // unknown distance types are errors, not ignored
  let bad = json!({"curves": [c], "count": 2, "spacing": 5, "distance_type": "total"});
  assert!(s.execute("sketch.pattern_rect", &bad).is_err());
}
