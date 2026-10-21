//! `sketch.trace` and `trace.info`: images traced into closed sketch profiles; SVG sketches.

use super::*;
use serde_json::{Value, json};

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
  match s.execute(cmd, &p) {
    Ok(v) => v,
    Err(e) => panic!("{cmd} {p}: {e}"),
  }
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
  let d = std::env::temp_dir().join(format!("fox-trace-{tag}-{}", std::process::id()));
  std::fs::create_dir_all(&d).unwrap();
  d
}

/// A 200 × 100 PNG on white, filled where `ink(x, y)` gives a colour (y down, as in the file).
fn png(path: &std::path::Path, ink: impl Fn(f64, f64) -> Option<[u8; 3]>) {
  let img = image::RgbImage::from_fn(200, 100, |x, y| {
    image::Rgb(ink(f64::from(x) + 0.5, f64::from(y) + 0.5).unwrap_or([255, 255, 255]))
  });
  img.save(path).unwrap();
}

/// A 60 px black square with a 12 px-radius hole, and a 20 px-radius black disc.
fn plate_and_disc(x: f64, y: f64) -> Option<[u8; 3]> {
  let square = (20.0..80.0).contains(&x) && (20.0..80.0).contains(&y);
  let hole = (x - 50.0).hypot(y - 50.0) < 12.0;
  let disc = (x - 140.0).hypot(y - 50.0) < 20.0;
  ((square && !hole) || disc).then_some([0, 0, 0])
}

fn profiles(s: &mut Session) -> Vec<Value> {
  let info = run(s, "sketch.info", json!({}));
  assert_eq!(info["open_curves"], json!([]), "every traced curve closes a profile: {}", info["open_curves"]);
  info["profiles"].as_array().unwrap().clone()
}

#[test]
fn traced_shapes_are_closed_profiles_at_the_given_size() {
  let dir = temp_dir("shapes");
  let image = dir.join("shapes.png");
  png(&image, plate_and_disc);
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  // 200 px wide → 200 mm: one millimetre per pixel, image centre on the origin
  let r = run(&mut s, "sketch.trace", json!({"image": image, "width": 200}));
  assert_eq!(r["shapes"], 2, "{r}");
  assert_eq!(r["holes"], 1, "{r}");
  assert_eq!(r["size"], json!([200.0, 100.0]));
  assert!(!r["curves"].as_array().unwrap().is_empty());
  let p = profiles(&mut s);
  // the square with its hole, the disc in the hole, and the other disc
  assert_eq!(p.len(), 3, "{p:?}");
  let mut areas: Vec<f64> = p.iter().map(|p| p["area"].as_f64().unwrap()).collect();
  areas.sort_by(f64::total_cmp);
  let pi = std::f64::consts::PI;
  for (got, want) in areas.iter().zip([pi * 144.0, pi * 400.0, 3600.0 - pi * 144.0]) {
    assert!((got - want).abs() / want < 0.03, "area {got} vs {want}: {areas:?}");
  }
  // the right-hand disc is centred 40 mm right of the origin (pixel 140 of 200, y centred)
  let disc = p.iter().find(|p| p["bbox"][0][0].as_f64().unwrap() > 0.0).unwrap();
  let (lo, hi) = (&disc["bbox"][0], &disc["bbox"][1]);
  let cx = (lo[0].as_f64().unwrap() + hi[0].as_f64().unwrap()) / 2.0;
  let cy = (lo[1].as_f64().unwrap() + hi[1].as_f64().unwrap()) / 2.0;
  assert!((cx - 40.0).abs() < 0.5 && cy.abs() < 0.5, "{disc}");
  // one undo step removes the whole trace
  assert!(s.undo());
  assert!(run(&mut s, "sketch.info", json!({}))["curves"].as_array().unwrap().is_empty());
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn traced_sketch_extrudes() {
  let dir = temp_dir("extrude");
  let image = dir.join("shapes.png");
  png(&image, plate_and_disc);
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let r = run(&mut s, "sketch.trace", json!({"image": image, "width": 200, "fit": "lines"}));
  // the square-with-hole and the disc are ink; the disc inside the hole is not
  assert_eq!(r["ink"].as_array().unwrap().len(), 2, "{r}");
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "extrude", json!({"distance": 3, "profiles": r["ink"]}));
  assert!(s.model.errors().is_empty(), "{:?}", s.model.errors());
  let pi = std::f64::consts::PI;
  let want = (3600.0 - pi * 144.0 + pi * 400.0) * 3.0;
  let v: f64 = s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum();
  assert!((v - want).abs() / want < 0.02, "volume {v} vs {want}");
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_fit_and_placement_option_closes() {
  let dir = temp_dir("fits");
  let image = dir.join("shapes.png");
  png(&image, plate_and_disc);
  for fit in ["lines", "arcs", "curves"] {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "param.add", json!({"name": "tall", "expr": "2 in"}));
    let r = run(
      &mut s,
      "sketch.trace",
      json!({"image": image, "height": "tall - 0.8 mm", "at": [10, -5], "angle": "30 deg", "fit": fit}),
    );
    let size = [r["size"][0].as_f64().unwrap(), r["size"][1].as_f64().unwrap()];
    assert!((size[0] - 100.0).abs() < 1e-9 && (size[1] - 50.0).abs() < 1e-9, "{fit}: {size:?}");
    assert_eq!(profiles(&mut s).len(), 3, "{fit}");
  }
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn colour_layers_trace_separately() {
  let dir = temp_dir("colours");
  let image = dir.join("flags.png");
  png(&image, |x, y| {
    let band = (25.0..75.0).contains(&y);
    if !band {
      None
    } else if (10.0..60.0).contains(&x) {
      Some([220, 30, 30])
    } else if (75.0..125.0).contains(&x) {
      Some([30, 160, 40])
    } else if (140.0..190.0).contains(&x) {
      Some([30, 60, 220])
    } else {
      None
    }
  });
  let mut s = Session::default();
  let info = run(&mut s, "trace.info", json!({"image": image, "mode": "colors", "colors": 4}));
  let layers = info["layers"].as_array().unwrap();
  assert_eq!(layers.len(), 4, "{info}");
  let inked: Vec<&Value> = layers.iter().filter(|l| l["background"] == false).collect();
  assert_eq!(inked.len(), 3, "{info}");
  assert!(inked.iter().all(|l| l["shapes"] == 1), "{info}");
  let channel =
    |l: &Value, i: usize| u8::from_str_radix(&l["color"].as_str().unwrap()[1 + 2 * i..3 + 2 * i], 16).unwrap();
  let red = inked.iter().find(|l| channel(l, 0) > 150 && channel(l, 1) < 100).map(|l| l["index"].clone());

  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let r = run(&mut s, "sketch.trace", json!({"image": image, "mode": "colors", "colors": 4}));
  assert_eq!(r["shapes"], 3, "{r}");
  assert_eq!(profiles(&mut s).len(), 3);
  let red = red.expect("a red layer");
  {
    s.undo();
    let r = run(&mut s, "sketch.trace", json!({"image": image, "mode": "colors", "colors": 4, "layers": [red]}));
    assert_eq!(r["shapes"], 1, "{r}");
  }
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bad_trace_input_is_refused() {
  let dir = temp_dir("bad");
  let image = dir.join("shapes.png");
  png(&image, plate_and_disc);
  let not_an_image = dir.join("notes.png");
  std::fs::write(&not_an_image, "hello").unwrap();
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  for p in [
    json!({}),
    json!({"image": dir.join("missing.png")}),
    json!({"image": not_an_image}),
    json!({"image": image, "mode": "sepia"}),
    json!({"image": image, "fit": "nurbs"}),
    json!({"image": image, "width": -5}),
    json!({"image": image, "tolerance": 0}),
    json!({"image": image, "threshold": 300}),
    json!({"image": image, "mode": "colors", "colors": 99}),
    json!({"image": image, "layers": [7]}),
  ] {
    assert!(s.execute("sketch.trace", &p).is_err(), "{p}");
  }
  assert!(run(&mut s, "sketch.info", json!({}))["curves"].as_array().unwrap().is_empty());
  // all white: nothing to trace, said plainly
  let blank = dir.join("blank.png");
  png(&blank, |_, _| None);
  let r = run(&mut s, "sketch.trace", json!({"image": blank}));
  assert_eq!(r["shapes"], 0);
  assert!(r["warning"].is_string(), "{r}");
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn svg_sketches_round_trip_with_exact_curves() {
  let dir = temp_dir("svg");
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let sk = s.edit_sketch.unwrap().0;
  run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]}));
  run(&mut s, "sketch.circle", json!({"center": [20, 10], "radius": 5}));
  run(&mut s, "sketch.control_spline", json!({"points": [[0, 30], [10, 45], [30, 15], [40, 30]]}));
  let svg = dir.join("part.svg");
  let r = run(&mut s, "export", json!({"path": svg, "sketch": sk}));
  assert!(r["entities"].as_u64().unwrap() >= 6, "{r}");
  assert!(std::fs::read_to_string(&svg).unwrap().contains("<svg"));
  assert_eq!(run(&mut s, "inspect", json!({"path": svg}))["entities"], r["entities"]);

  // back into a new sketch: the rectangle and circle still make a ring and a disc
  run(&mut s, "sketch.finish", json!({}));
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  let r = run(&mut s, "sketch.insert", json!({"from": {"path": svg}}));
  assert_eq!(r["curves"].as_array().unwrap().len(), 6, "4 lines, a circle and one exact spline: {r}");
  assert_eq!(run(&mut s, "sketch.info", json!({}))["profiles"].as_array().unwrap().len(), 2);

  // an SVG from a drawing program opens as a design with one sketch
  let drawn = dir.join("badge.svg");
  std::fs::write(
    &drawn,
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm" viewBox="0 0 100 50">
         <path d="M10 10 h30 v30 h-30 z M60 25 a15 15 0 1 0 30 0 a15 15 0 1 0 -30 0 z"/>
         <text x="5" y="5">ignored</text>
       </svg>"#,
  )
  .unwrap();
  let mut d = Session::open(&drawn).unwrap();
  let info = run(&mut d, "info", json!({}));
  assert_eq!(info["timeline"].as_array().map(Vec::len), Some(1), "{info}");
  let r = run(&mut d, "import", json!({"path": drawn}));
  assert!(r["warning"].as_str().is_some_and(|w| w.contains("SVG")), "{r}");
  let _ = std::fs::remove_dir_all(&dir);
}

/// The sketch being edited (the first one).
fn first_sketch(s: &Session) -> (fox_model::FeatureId, fox_sketch::Sketch) {
  let f = s.design.features.iter().find(|f| matches!(f.kind, fox_model::FeatureKind::Sketch { .. })).unwrap();
  (f.id, s.design.sketch(f.id).unwrap().clone())
}

#[test]
fn centerline_strokes_are_single_connected_lines_with_constraints() {
  let dir = temp_dir("centerline");
  let image = dir.join("t.png");
  // a T: an 8 px bar along y = 30 from x = 40 to 160, and a stem down to y = 85 (y down)
  png(&image, |x, y| {
    let bar = (40.0..160.0).contains(&x) && (26.0..34.0).contains(&y);
    let stem = (96.0..104.0).contains(&x) && (26.0..85.0).contains(&y);
    (bar || stem).then_some([0, 0, 0])
  });
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let r = run(&mut s, "sketch.trace", json!({"image": image, "width": 200, "mode": "centerline"}));
  assert_eq!(r["strokes"], 3, "{r}");
  let w = r["stroke_width"].as_f64().unwrap();
  assert!((w - 8.0).abs() < 1.5, "{r}");
  let (_, sk) = first_sketch(&s);
  assert_eq!(sk.curves.len(), 3, "{:?}", sk.curves);
  // the three lines meet at one sketch point, on the bar's middle above the stem
  let mut uses: std::collections::HashMap<fox_sketch::PointId, usize> = Default::default();
  for c in &sk.curves {
    for p in sk.curve_points(c.id) {
      *uses.entry(p).or_default() += 1;
    }
  }
  let (&junction, _) = uses.iter().find(|(_, n)| **n == 3).expect("a shared junction point");
  let j = sk.pos(junction).unwrap();
  // pixel (100, 30) of a 200 × 100 image centred on the origin, y up: (0, 20)
  assert!(j[0].abs() < 1.5 && (j[1] - 20.0).abs() < 1.5, "{j:?}");
  // the bar's two halves are horizontal (and equal), the stem vertical
  let counts = &r["constraint_counts"];
  assert_eq!(counts["horizontal"], 2, "{r}");
  assert_eq!(counts["vertical"], 1, "{r}");
  // strokes are open: nothing to extrude, so no ink
  assert_eq!(r["ink"], json!([]));
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn traced_outlines_get_constraints_unless_turned_off() {
  let dir = temp_dir("constraints");
  let image = dir.join("plate.png");
  // a 120 × 60 rectangle with a 15 px-radius hole and a second, equal hole
  png(&image, |x, y| {
    let rect = (40.0..160.0).contains(&x) && (20.0..80.0).contains(&y);
    let holes = (x - 70.0).hypot(y - 50.0) < 15.0 || (x - 130.0).hypot(y - 50.0) < 15.0;
    (rect && !holes).then_some([0, 0, 0])
  });
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let r = run(&mut s, "sketch.trace", json!({"image": image, "width": 200}));
  let counts = &r["constraint_counts"];
  assert_eq!(counts["horizontal"], 2, "{r}");
  assert_eq!(counts["vertical"], 2, "{r}");
  // the opposite sides, and the two holes' radii
  assert_eq!(counts["equal"], 3, "{r}");
  assert!(r.get("constraint_warning").is_none(), "{r}");
  assert_eq!(profiles(&mut s).len(), 3);
  // the sketch solves with them, and the edges are exactly straight now
  let (id, sk) = first_sketch(&s);
  assert!(s.model.sketches[&id].report.ok());
  for c in &sk.curves {
    if let fox_sketch::CurveKind::Line { a, b } = c.kind {
      let (a, b) = (sk.pos(a).unwrap(), sk.pos(b).unwrap());
      assert!((a[0] - b[0]).abs() < 1e-7 || (a[1] - b[1]).abs() < 1e-7, "{a:?} {b:?}");
    }
  }
  // turned off: plain geometry
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let r = run(&mut s, "sketch.trace", json!({"image": image, "width": 200, "constraints": false}));
  assert!(r.get("constraints").is_none(), "{r}");
  assert!(first_sketch(&s).1.constraints.is_empty());
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rounded_corners_trace_tangent() {
  let dir = temp_dir("rounded");
  let image = dir.join("rounded.png");
  // a 120 × 60 rectangle with 15 px corner radii
  png(&image, |x, y| {
    let (cx, cy) = (x.clamp(55.0, 145.0), y.clamp(35.0, 65.0));
    ((x - cx).hypot(y - cy) < 15.0).then_some([0, 0, 0])
  });
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let r = run(&mut s, "sketch.trace", json!({"image": image, "width": 200, "fit": "arcs"}));
  let counts = &r["constraint_counts"];
  assert!(counts["tangent"].as_u64().unwrap_or(0) >= 4, "{r}");
  assert_eq!(counts["horizontal"], 2, "{r}");
  assert_eq!(counts["vertical"], 2, "{r}");
  assert_eq!(profiles(&mut s).len(), 1);
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_canvas_traces_where_it_lies_at_its_calibrated_size() {
  let dir = temp_dir("canvas");
  let image = dir.join("shapes.png");
  png(&image, plate_and_disc);
  let mut s = Session::default();
  let c = run(&mut s, "canvas.insert", json!({"image": image, "width": 100, "at": [30, 10]}))["feature"].clone();
  // calibrate: 50 mm on the canvas is really 100 mm, scaled about its centre → 200 mm wide
  run(&mut s, "canvas.calibrate", json!({"feature": c, "from": [30, 10], "to": [80, 10], "distance": 100}));
  run(&mut s, "canvas.edit", json!({"feature": c, "opacity": 0.5, "visible": false}));
  let v = run(&mut s, "canvas.edit", json!({"feature": c}));
  assert_eq!(v["visible"], false, "{v}");
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let r = run(&mut s, "sketch.trace", json!({"canvas": c}));
  assert_eq!(r["size"], json!([200.0, 100.0]), "{r}");
  assert_eq!(r["at"], json!([30.0, 10.0]), "{r}");
  // the disc (pixel 140 of 200) lands 40 mm right of the canvas centre
  let p = profiles(&mut s);
  let disc = p.iter().find(|p| p["bbox"][0][0].as_f64().unwrap() > 40.0).unwrap();
  let cx = (disc["bbox"][0][0].as_f64().unwrap() + disc["bbox"][1][0].as_f64().unwrap()) / 2.0;
  assert!((cx - 70.0).abs() < 0.5, "{disc}");
  run(&mut s, "sketch.finish", json!({}));
  // a sketch on another plane can't trace it
  run(&mut s, "sketch.create", json!({"plane": "xz"}));
  assert!(s.execute("sketch.trace", &json!({"canvas": c})).is_err());
  // a rotated canvas on a parallel offset plane: the trace turns with it
  let mut s = Session::default();
  let plane = json!({"origin": [0, 0, 5], "normal": [0, 0, 1], "x": [1, 0, 0]});
  let c = run(&mut s, "canvas.insert", json!({"image": image, "plane": plane, "width": 200, "angle": 90}))["feature"]
    .clone();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  let r = run(&mut s, "sketch.trace", json!({"canvas": c}));
  assert!((r["angle"].as_f64().unwrap() - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "{r}");
  let p = profiles(&mut s);
  let disc = p.iter().find(|p| p["bbox"][0][1].as_f64().unwrap() > 10.0).unwrap();
  let cy = (disc["bbox"][0][1].as_f64().unwrap() + disc["bbox"][1][1].as_f64().unwrap()) / 2.0;
  assert!((cy - 40.0).abs() < 0.5, "{disc}");
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_kerf_check_finds_what_the_cut_cant_make() {
  let dir = temp_dir("kerf");
  let image = dir.join("plate.png");
  // a 120 × 60 plate (y down): a 3 px slot from its top edge at x = 60 and a 4 px hole at
  // (140, 50), at 0.5 mm per pixel: a 1.5 mm slot, a 2 mm hole
  png(&image, |x, y| {
    let plate = (40.0..160.0).contains(&x) && (20.0..80.0).contains(&y);
    let slot = (58.5..61.5).contains(&x) && y < 50.0;
    let hole = (x - 140.0).hypot(y - 50.0) < 2.0;
    (plate && !slot && !hole).then_some([0, 0, 0])
  });
  let mut s = Session::default();
  run(&mut s, "sketch.create", json!({"plane": "xy"}));
  // a 4 mm kerf can't cut either
  let r = run(&mut s, "sketch.trace", json!({"image": image, "width": 100, "kerf": "4 mm", "at": [10, 0], "holes": 4}));
  let spots = r["narrow"].as_array().unwrap();
  let kinds: Vec<&str> = spots.iter().map(|n| n["kind"].as_str().unwrap()).collect();
  assert!(kinds.contains(&"narrow_gap") && kinds.contains(&"small_hole"), "{r}");
  assert!(r["kerf_warning"].is_string(), "{r}");
  // the hole is at pixel (140, 50): 20 mm right of the image centre, then moved by `at`
  let hole = spots.iter().find(|n| n["kind"] == "small_hole").unwrap();
  let at = [hole["at"][0].as_f64().unwrap(), hole["at"][1].as_f64().unwrap()];
  assert!((at[0] - 30.0).abs() < 1.0 && at[1].abs() < 1.0, "{hole}");
  assert!(hole["width"].as_f64().unwrap() < 4.0, "{hole}");
  // a 1 mm kerf makes both; no kerf, no check
  let info = run(&mut s, "trace.info", json!({"image": image, "width": 100, "kerf": 1, "holes": 4}));
  assert_eq!(info["narrow"], json!([]), "{info}");
  let info = run(&mut s, "trace.info", json!({"image": image, "width": 100}));
  assert!(info.get("narrow").is_none(), "{info}");
  assert!(s.execute("trace.info", &json!({"image": image, "kerf": -2})).is_err());
  let _ = std::fs::remove_dir_all(&dir);
}
