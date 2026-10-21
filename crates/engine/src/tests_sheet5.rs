//! Tests for flat pattern sketches that follow the model, rounded bends where press-brake folds
//! converge, and splines in flat pattern DXFs.

use std::f64::consts::PI;

use fox_model::FeatureId;
use serde_json::{Value, json};

use crate::Session;
use crate::tests_sheet3::reference_part;

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => {
      assert!(v.get("error").is_none_or(Value::is_null), "{cmd} {p}: {v}");
      v
    }
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn sketch_of(r: &Value) -> FeatureId {
  FeatureId(r["feature"].as_u64().unwrap() as u32)
}

/// The area of a sketch's largest region (the flat's outline less its holes).
fn largest_area(s: &Session, sk: FeatureId) -> f64 {
  s.model.sketches[&sk].profiles.iter().map(|p| p.area()).fold(0.0, f64::max)
}

/// The flat pattern's area: its volume over the sheet's thickness.
fn flat_area(s: &mut Session) -> f64 {
  let t = s.model.bodies.iter().find_map(|b| b.sheet.as_ref()).unwrap().thickness;
  run(s, "sheet.flat_pattern", json!({}))["volume_mm3"].as_f64().unwrap() / t
}

#[test]
fn flat_pattern_sketch_follows_the_body() {
  let mut s = Session::default();
  run(&mut s, "param.add", json!({"name": "h", "expr": "20 mm"}));
  run(&mut s, "sheet.rule", json!({"name": "Steel", "thickness": 2, "bend_radius": 3, "k_factor": 0.5}));
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [100, 50]}));
  run(&mut s, "sheet.flange", json!({}));
  run(&mut s, "sheet.edge_flange", json!({"edges": [{"point": [50, 50, 2]}], "height": "h"}));
  let on_face = sketch_of(&run(&mut s, "sheet.flat_sketch", json!({})));
  let r = run(&mut s, "sheet.flat_sketch", json!({"on": "xy"}));
  assert_eq!(r["follows"], true, "{r}");
  let at_origin = sketch_of(&r);
  let once = sketch_of(&run(&mut s, "sheet.flat_sketch", json!({"on": "xy", "curves": "once"})));
  // 100 × (50 + bend allowance + (h − r − t))
  let len = |h: f64| 50.0 + PI / 2.0 * 4.0 + (h - 5.0);
  for sk in [on_face, at_origin, once] {
    assert!((largest_area(&s, sk) - 100.0 * len(20.0)).abs() < 1e-3, "{sk:?}: {}", largest_area(&s, sk));
  }
  let res = &s.model.sketches[&at_origin];
  assert_eq!(res.sketch.flats.len(), 1);
  let curves = res.sketch.flats[0].curves.clone();
  assert_eq!(curves.len(), 5, "four sides and the bend line");
  assert!(res.sketch.points.iter().all(|p| p.fixed), "the flat's points are fixed");
  assert!(s.model.sketches[&once].sketch.flats.is_empty());

  // a taller flange: the two following sketches grow, the one made once stays
  run(&mut s, "param.set", json!({"name": "h", "expr": "30 mm"}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  for sk in [on_face, at_origin] {
    assert!((largest_area(&s, sk) - 100.0 * len(30.0)).abs() < 1e-3, "{sk:?}: {}", largest_area(&s, sk));
  }
  assert!((largest_area(&s, once) - 100.0 * len(20.0)).abs() < 1e-3);
  // the same curves, moved: the lower-left corner stays at the origin
  let res = &s.model.sketches[&at_origin];
  assert_eq!(res.sketch.flats[0].curves, curves);
  let (lo, hi) = res.sketch.bounds().unwrap();
  assert!(lo[0].abs() < 1e-9 && lo[1].abs() < 1e-9, "{lo:?}");
  let mut size = [hi[0] - lo[0], hi[1] - lo[1]];
  size.sort_by(f64::total_cmp);
  assert!((size[0] - len(30.0)).abs() < 1e-3 && (size[1] - 100.0).abs() < 1e-3, "{size:?}");
  // the sketch can be extruded straight away and saved
  let r = run(&mut s, "extrude", json!({"sketch": at_origin.0, "distance": 2}));
  assert!(r["error"].is_null(), "{r}");
  let text = serde_json::to_string(&s.design).unwrap();
  let back: fox_model::Design = serde_json::from_str(&text).unwrap();
  assert_eq!(back, s.design);
}

#[test]
fn flat_pattern_sketches_follow_the_reference_parts() {
  for case in [
    "67-sm-base-flange",
    "68-sm-edge-flange",
    "69-sm-tray",
    "70-sm-flange-45",
    "71-sm-custom-rule",
    "72-sm-contour-flange",
    "73-sm-hem",
    "74-sm-unfold-refold",
    "75-sm-bracket-holes",
    "76-sm-thickness-variant",
  ] {
    let mut s = reference_part(case);
    let sk = sketch_of(&run(&mut s, "sheet.flat_sketch", json!({"on": "xy"})));
    assert!(s.model.errors().is_empty(), "{case}: {:?}", s.model.errors());
    let st = s.model.status(sk).unwrap();
    assert!(st.warning.is_none(), "{case}: {:?}", st.warning);
    let (area, flat) = (largest_area(&s, sk), flat_area(&mut s));
    assert!((area - flat).abs() < 1e-3 * flat, "{case}: sketch {area}, flat {flat}");
  }
  // 76 changes its thickness: the flat pattern sketch follows (a thinner sheet unfolds longer)
  let mut s = reference_part("76-sm-thickness-variant");
  let sk = sketch_of(&run(&mut s, "sheet.flat_sketch", json!({})));
  let thick = largest_area(&s, sk);
  run(&mut s, "param.set", json!({"name": "sm_thickness", "expr": "1 mm"}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let (area, flat) = (largest_area(&s, sk), flat_area(&mut s));
  assert!((area - flat).abs() < 1e-3 * flat && (area - thick).abs() > 10.0, "{thick} {area} {flat}");
}

#[test]
fn splines_in_flat_pattern_dxfs() {
  // a 100 × 50 plate whose top side is a spline, and a flange along its bottom edge
  let mut s = Session::default();
  run(&mut s, "sheet.rule", json!({"name": "Steel", "thickness": 2, "bend_radius": 3, "k_factor": 0.5}));
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  run(&mut s, "sketch.line", json!({"points": [[0, 50], [0, 0], [100, 0], [100, 50]]}));
  run(&mut s, "sketch.spline", json!({"points": [[100, 50], [70, 62], [30, 58], [0, 50]]}));
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "sheet.flange", json!({}));
  run(&mut s, "sheet.edge_flange", json!({"edges": [{"point": [50, 0, 2]}], "height": 20}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let dir = std::env::temp_dir().join(format!("fox-flat-spline-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("flat.dxf");
  run(&mut s, "sheet.flat_pattern", json!({"path": path.to_str().unwrap()}));
  let text = std::fs::read_to_string(&path).unwrap();
  assert!(text.contains("AC1015") && text.contains("SPLINE"), "R2000 with a SPLINE");
  // the spline reads back as one exact NURBS on CUT, with the layers and notes of the R12 file
  let layered = fox_exchange::flatcheck::read_layered(text.as_bytes()).unwrap();
  let splines: Vec<_> = layered
    .entities
    .iter()
    .filter_map(|(layer, e)| match e {
      fox_exchange::dxf::Entity::Spline(sp) => Some((layer.as_str(), sp)),
      _ => None,
    })
    .collect();
  assert_eq!(splines.len(), 1, "{:?}", layered.entities);
  assert_eq!(splines[0].0, "CUT");
  assert!(splines[0].1.control.len() >= 4);
  let layers: Vec<&str> = layered.entities.iter().map(|(l, _)| l.as_str()).collect();
  assert!(layers.contains(&"BEND_UP") || layers.contains(&"BEND_DOWN"), "{layers:?}");
  assert!(layered.texts.iter().any(|(l, t)| l == "NOTES" && t.starts_with("SHEET T=2")), "{:?}", layered.texts);
  // the file passes the checks: one closed contour, the spline ending where the lines do
  let r = run(&mut s, "sheet.check_dxf", json!({"path": path.to_str().unwrap()}));
  assert_eq!(r["ok"], true, "{r}");
  assert_eq!(r["contours"], 1, "{r}");
  let r = run(&mut s, "sheet.check_dxf", json!({}));
  assert_eq!(r["ok"], true, "{r}");
  // the spline spans the plate
  let pts: Vec<[f64; 2]> =
    fox_exchange::dxf::Entity::Spline(splines[0].1.clone()).beziers().iter().flat_map(|b| [b[0], b[3]]).collect();
  let (a, b) = (pts[0], pts[pts.len() - 1]);
  assert!(((a[0] - b[0]).hypot(a[1] - b[1]) - 100.0).abs() < 1e-6, "{a:?} {b:?}");
  // and the flat pattern sketch carries it as a Bézier chain, its region the flat's area
  let sk = sketch_of(&run(&mut s, "sheet.flat_sketch", json!({})));
  let res = &s.model.sketches[&sk];
  assert!(res.sketch.curves.iter().any(|c| matches!(c.kind, fox_sketch::CurveKind::Bezier { .. })));
  let (area, flat) = (largest_area(&s, sk), flat_area(&mut s));
  assert!((area - flat).abs() < 1e-3 * flat, "{area} {flat}");
  std::fs::remove_dir_all(&dir).ok();
}

/// A 40 square and a circle of radius 15, 30 above it, and a press-brake lofted flange between
/// them: at each corner of the square a fan of facets converges.
fn square_to_round(s: &mut Session, rounded: bool) -> Value {
  run(s, "sheet.rule", json!({"name": "Steel", "thickness": 1, "bend_radius": 2, "k_factor": 0.5}));
  let a = run(s, "sketch.create", json!({"plane": "xy"}))["sketch"].clone();
  run(s, "sketch.rect", json!({"corner1": [-20, -20], "corner2": [20, 20]}));
  let pl = run(s, "plane.offset", json!({"plane": "xy", "offset": 30}))["feature"].clone();
  let b = run(s, "sketch.create", json!({"plane": {"feature": pl}}))["sketch"].clone();
  run(s, "sketch.circle", json!({"center": [0, 0], "radius": 15}));
  let p = json!({"from": a, "to": b, "method": "press_brake", "bends": 15, "rounded": rounded});
  run(s, "sheet.lofted_flange", p)
}

fn volume(s: &Session) -> f64 {
  s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
}

#[test]
fn rounded_bends_where_press_brake_folds_converge() {
  let mut sharp = Session::default();
  square_to_round(&mut sharp, false);
  let mut s = Session::default();
  let r = square_to_round(&mut s, true);
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let id = FeatureId(r["feature"].as_u64().unwrap() as u32);
  let warning = s.model.status(id).and_then(|st| st.warning.clone()).unwrap_or_default();
  assert!(!warning.contains("stay sharp"), "{warning}");
  let b = s.model.bodies.last().unwrap();
  assert!(b.shape.is_valid() && b.shape.solid_count() == 1);
  // no fold is left sharp, and the fans at the corners are rounded (curved faces there)
  let sheet = b.sheet.as_ref().unwrap();
  let sharp_folds = fox_model::sheet3::lofted_folds(b.faces(), b.edges(), sheet.thickness, sheet.bend_radius);
  assert!(sharp_folds.is_empty(), "{sharp_folds:?}");
  let at_corner = |f: &fox_kernel::FaceInfo| f.kind != fox_kernel::SurfaceKind::Plane && f.center.z < 15.0;
  let curved: Vec<_> =
    b.faces().iter().filter(|f| f.kind != fox_kernel::SurfaceKind::Plane).map(|f| (f.kind, f.center)).collect();
  assert!(b.faces().iter().filter(|f| at_corner(f)).count() >= 8, "{warning} {curved:?}");
  // rounding trims the outsides of the folds, and the reliefs take a little more
  assert!(volume(&s) < volume(&sharp) && volume(&s) > 0.9 * volume(&sharp), "{} {}", volume(&s), volume(&sharp));
  // the flat pattern is still there, with a bend at every fold
  let flat = run(&mut s, "sheet.flat_pattern", json!({}));
  assert!(flat["bends"].as_array().unwrap().len() >= 8, "{flat}");
}
