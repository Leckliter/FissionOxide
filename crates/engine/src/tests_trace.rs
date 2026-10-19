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
