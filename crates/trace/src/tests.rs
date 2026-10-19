#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::f64::consts::PI;

use crate::validate::{flatten, touches};
use crate::*;

/// A black-on-white image; `inside(x, y)` gets pixel centres in y-up coordinates.
fn mask_img(w: u32, h: u32, inside: impl Fn(f64, f64) -> bool) -> Image {
  color_img(w, h, |x, y| if inside(x, y) { [0, 0, 0] } else { [255, 255, 255] })
}

fn color_img(w: u32, h: u32, f: impl Fn(f64, f64) -> [u8; 3]) -> Image {
  let mut rgba = Vec::with_capacity((w * h * 4) as usize);
  for row in 0..h {
    for x in 0..w {
      let c = f(x as f64 + 0.5, (h - 1 - row) as f64 + 0.5);
      rgba.extend([c[0], c[1], c[2], 255]);
    }
  }
  Image::from_rgba(w, h, rgba).unwrap()
}

fn count_pixels(w: u32, h: u32, inside: impl Fn(f64, f64) -> bool) -> f64 {
  let mut n = 0;
  for y in 0..h {
    for x in 0..w {
      if inside(x as f64 + 0.5, y as f64 + 0.5) {
        n += 1;
      }
    }
  }
  n as f64
}

fn polygon(c: &Contour) -> Vec<[f64; 2]> {
  let mut pts = vec![c.segs[0].start()];
  for s in &c.segs {
    flatten(s, c.hole, &mut pts);
  }
  pts.pop(); // the closing point repeats the first
  pts
}

fn area(pts: &[[f64; 2]]) -> f64 {
  let n = pts.len();
  (0..n).map(|i| pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]).sum::<f64>() / 2.0
}

/// Checks the layer's guarantees and returns its total signed area.
fn check_layer(layer: &Layer) -> f64 {
  let mut total = 0.0;
  let mut edges: Vec<([f64; 2], [f64; 2], usize, usize, usize)> = Vec::new();
  let mut seen_outer = false;
  for (ci, c) in layer.contours.iter().enumerate() {
    assert!(!c.segs.is_empty());
    if c.hole {
      assert!(seen_outer, "a hole must follow its outer boundary");
    } else {
      seen_outer = true;
    }
    for (k, s) in c.segs.iter().enumerate() {
      if matches!(s, Seg::Circle { .. }) {
        assert_eq!(c.segs.len(), 1, "a circle is always alone");
      }
      let next = &c.segs[(k + 1) % c.segs.len()];
      assert!(s.end() == next.start(), "contour {ci} open between segments {k} and next: {s:?} {next:?}");
    }
    let pts = polygon(c);
    let a = area(&pts);
    assert_eq!(a < 0.0, c.hole, "contour {ci} orientation (area {a})");
    total += a;
    let n = pts.len();
    for i in 0..n {
      edges.push((pts[i], pts[(i + 1) % n], ci, i, n));
    }
  }
  // sweep over x for intersecting / touching edge pairs
  edges.sort_by(|a, b| a.0[0].min(a.1[0]).total_cmp(&b.0[0].min(b.1[0])));
  for i in 0..edges.len() {
    let (p1, p2, c1, k1, n1) = edges[i];
    let xmax = p1[0].max(p2[0]);
    for &(q1, q2, c2, k2, _) in &edges[i + 1..] {
      if q1[0].min(q2[0]) > xmax + 1e-9 {
        break;
      }
      if c1 == c2 {
        let d = (k1 + n1 - k2) % n1;
        if d == 1 || d == n1 - 1 {
          continue;
        }
      }
      assert!(!touches(p1, p2, q1, q2), "edges touch: contour {c1} {p1:?}-{p2:?} and contour {c2} {q1:?}-{q2:?}");
    }
  }
  total
}

fn opts(fit: Fit) -> Options {
  Options { fit, ..Options::default() }
}

fn count(layer: &Layer, f: impl Fn(&Seg) -> bool) -> usize {
  layer.contours.iter().flat_map(|c| &c.segs).filter(|s| f(s)).count()
}

#[test]
fn filled_square_is_four_exact_lines() {
  let img = mask_img(40, 40, |x, y| (10.0..30.0).contains(&x) && (8.0..28.0).contains(&y));
  for fit in [Fit::Lines, Fit::Arcs, Fit::Curves] {
    let t = trace(&img, &opts(fit)).unwrap();
    assert_eq!((t.width, t.height), (40, 40));
    let l = &t.layers[0];
    assert_eq!(l.contours.len(), 1);
    let c = &l.contours[0];
    assert!(!c.hole);
    assert_eq!(c.segs.len(), 4, "{fit:?}: {:?}", c.segs);
    let mut corners: Vec<[f64; 2]> = c
      .segs
      .iter()
      .map(|s| match s {
        Seg::Line { a, .. } => *a,
        s => panic!("not a line: {s:?}"),
      })
      .collect();
    corners.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(corners, vec![[10.0, 8.0], [10.0, 28.0], [30.0, 8.0], [30.0, 28.0]]);
    assert_eq!(check_layer(l), 400.0);
  }
}

#[test]
fn disc_is_one_circle() {
  let img = mask_img(120, 120, |x, y| (x - 60.0).hypot(y - 58.0) < 40.0);
  for fit in [Fit::Arcs, Fit::Curves] {
    let t = trace(&img, &opts(fit)).unwrap();
    let c = &t.layers[0].contours;
    assert_eq!(c.len(), 1);
    match c[0].segs.as_slice() {
      [Seg::Circle { center, radius }] => {
        assert!((radius - 40.0).abs() < 0.5, "radius {radius}");
        assert!((center[0] - 60.0).abs() < 0.25 && (center[1] - 58.0).abs() < 0.25, "{center:?}");
      }
      s => panic!("not one circle: {s:?}"),
    }
    assert!(!c[0].hole);
  }
}

#[test]
fn ring_is_circle_and_hole_circle() {
  let img = mask_img(120, 120, |x, y| (25.0..50.0).contains(&(x - 60.0).hypot(y - 60.0)));
  let t = trace(&img, &Options::default()).unwrap();
  let l = &t.layers[0];
  assert_eq!(l.contours.len(), 2);
  let radius = |c: &Contour| match c.segs.as_slice() {
    [Seg::Circle { radius, .. }] => *radius,
    s => panic!("not a circle: {s:?}"),
  };
  assert!(!l.contours[0].hole && l.contours[1].hole);
  assert!((radius(&l.contours[0]) - 50.0).abs() < 0.5);
  assert!((radius(&l.contours[1]) - 25.0).abs() < 0.5);
  let a = check_layer(l);
  let px = count_pixels(120, 120, |x, y| (25.0..50.0).contains(&(x - 60.0).hypot(y - 60.0)));
  assert!((a - px).abs() / px < 0.01, "area {a} vs {px}");
}

fn rounded_rect(x: f64, y: f64) -> bool {
  let (cx, cy, hw, hh, r) = (110.0, 70.0, 90.0, 50.0, 25.0);
  let (dx, dy) = (((x - cx).abs() - (hw - r)).max(0.0), ((y - cy).abs() - (hh - r)).max(0.0));
  (x - cx).abs() < hw && (y - cy).abs() < hh && dx.hypot(dy) < r
}

#[test]
fn rounded_rect_is_lines_and_arcs() {
  let img = mask_img(220, 140, rounded_rect);
  let px = count_pixels(220, 140, rounded_rect);
  for fit in [Fit::Arcs, Fit::Curves] {
    let t = trace(&img, &opts(fit)).unwrap();
    let l = &t.layers[0];
    assert_eq!(l.contours.len(), 1);
    let lines = count(l, |s| matches!(s, Seg::Line { .. }));
    let arcs = count(l, |s| matches!(s, Seg::Arc { .. }));
    let cubics = count(l, |s| matches!(s, Seg::Cubic { .. }));
    assert!(lines >= 4 && arcs >= 4, "{fit:?}: {lines} lines, {arcs} arcs, {cubics} cubics");
    assert!(lines + arcs + cubics <= 16, "{fit:?}: too many segments: {:?}", l.contours[0].segs);
    if fit == Fit::Arcs {
      assert_eq!(cubics, 0);
    }
    let a = check_layer(l);
    assert!((a - px).abs() / px < 0.01, "area {a} vs {px}");
  }
}

#[test]
fn diagonal_touching_never_touches() {
  // two blocks touching at a corner, two single pixels touching at a corner
  let simple = |x: f64, y: f64| {
    let (xi, yi) = (x as i64, y as i64);
    ((5..15).contains(&xi) && (5..15).contains(&yi))
      || ((15..25).contains(&xi) && (15..25).contains(&yi))
      || (xi, yi) == (40, 5)
      || (xi, yi) == (41, 6)
  };
  // plus a checkerboard: every pixel touches its neighbours only at corners
  let board = |x: f64, y: f64| {
    let (xi, yi) = (x as i64, y as i64);
    simple(x, y) || ((30..38).contains(&xi) && (20..28).contains(&yi) && (xi + yi) % 2 == 0)
  };
  for fit in [Fit::Lines, Fit::Arcs, Fit::Curves] {
    let o = Options { specks: 0.0, holes: 0.0, ..opts(fit) };
    let t = trace(&mask_img(50, 40, simple), &o).unwrap();
    let l = &t.layers[0];
    let a = check_layer(l);
    let px = count_pixels(50, 40, simple);
    assert!((a - px).abs() / px < 0.01, "{fit:?}: area {a} vs {px}");
    // corner-touching pixels make one shape: the blocks, and the pixel pair
    assert_eq!(l.contours.len(), 2, "{fit:?}");
    let t = trace(&mask_img(50, 40, board), &o).unwrap();
    let l = &t.layers[0];
    check_layer(l);
    assert_eq!(l.contours.iter().filter(|c| !c.hole).count(), 3, "{fit:?}");
  }
}

/// Letter-like shapes: an A (triangle with a hole and legs), an L, an S-ish double bend and an O.
fn letters(x: f64, y: f64) -> bool {
  // A: outer triangle minus inner triangle minus a notch at the bottom
  let tri = |x: f64, y: f64, x0: f64, x1: f64, y0: f64, y1: f64| {
    let t = (y - y0) / (y1 - y0);
    (0.0..=1.0).contains(&t) && x > x0 + (x1 - x0) / 2.0 * t && x < x1 - (x1 - x0) / 2.0 * t
  };
  let a = tri(x, y, 10.0, 70.0, 10.0, 100.0)
    && !tri(x, y, 30.0, 50.0, 45.0, 75.0)
    && !(tri(x, y, 25.0, 55.0, 10.0, 30.0) && y < 30.0 && x > 28.0 && x < 52.0);
  let l = ((80.0..95.0).contains(&x) && (10.0..100.0).contains(&y))
    || ((80.0..130.0).contains(&x) && (10.0..24.0).contains(&y));
  let s = {
    let r = (x - 165.0).hypot(y - 75.0);
    let r2 = (x - 165.0).hypot(y - 35.0);
    ((12.0..24.0).contains(&r) && !(x > 165.0 && y < 75.0)) || ((12.0..24.0).contains(&r2) && !(x < 165.0 && y > 35.0))
  };
  let o = {
    let e = ((x - 225.0) / 25.0).powi(2) + ((y - 55.0) / 42.0).powi(2);
    (0.45..1.0).contains(&e)
  };
  a || l || s || o
}

#[test]
fn letters_are_closed_and_clean() {
  let px = count_pixels(260, 110, letters);
  let img = mask_img(260, 110, letters);
  for fit in [Fit::Lines, Fit::Arcs, Fit::Curves] {
    let t = trace(&img, &opts(fit)).unwrap();
    let l = &t.layers[0];
    let a = check_layer(l);
    assert!((a - px).abs() / px < 0.015, "{fit:?}: area {a} vs {px}");
    assert_eq!(l.contours.iter().filter(|c| c.hole).count(), 2, "{fit:?}: holes of A and O");
    if fit == Fit::Lines {
      assert_eq!(count(l, |s| !matches!(s, Seg::Line { .. })), 0);
    }
  }
}

fn lcg(seed: &mut u64) -> f64 {
  *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
  (*seed >> 11) as f64 / (1u64 << 53) as f64
}

#[test]
fn noisy_blob_is_valid() {
  let mut seed = 7u64;
  let noise: Vec<f64> = (0..200 * 160).map(|_| lcg(&mut seed)).collect();
  let inside = |x: f64, y: f64| {
    let (dx, dy) = (x - 100.0, y - 80.0);
    let t = dy.atan2(dx);
    let r = 55.0 + 8.0 * (3.0 * t).sin() + 4.0 * (7.0 * t + 1.0).cos() + 2.0 * (13.0 * t).sin();
    let d = dx.hypot(dy);
    let n = noise[(y as usize) * 200 + x as usize];
    // a ragged edge, a few holes and specks
    (d < r - 2.0 || (d < r + 2.0 && n < 0.5) || n < 0.01) && !(d < r - 6.0 && n > 0.995)
  };
  let px = count_pixels(200, 160, inside);
  let img = mask_img(200, 160, inside);
  for fit in [Fit::Lines, Fit::Arcs, Fit::Curves] {
    for tolerance in [0.5, 1.0, 2.0] {
      let o = Options { specks: 0.0, holes: 0.0, tolerance, ..opts(fit) };
      let t = trace(&img, &o).unwrap();
      let a = check_layer(&t.layers[0]);
      assert!((a - px).abs() / px < 0.02, "{fit:?} {tolerance}: area {a} vs {px}");
    }
    // with cleaning: still valid
    let t = trace(&img, &opts(fit)).unwrap();
    check_layer(&t.layers[0]);
  }
}

#[test]
fn specks_removed_and_small_holes_filled() {
  let inside = |x: f64, y: f64| {
    let (xi, yi) = (x as i64, y as i64);
    let big = (10..60).contains(&xi) && (10..60).contains(&yi);
    let small_hole = (20..23).contains(&xi) && (20..23).contains(&yi); // 9 px²
    let big_hole = (35..42).contains(&xi) && (35..42).contains(&yi); // 49 px²
    let speck = (70..73).contains(&xi) && (70..73).contains(&yi); // 9 px²
    (big && !small_hole && !big_hole) || speck
  };
  let t = trace(&mask_img(80, 80, inside), &Options::default()).unwrap();
  let l = &t.layers[0];
  assert_eq!(l.contours.len(), 2, "{:?}", l.contours);
  assert!(!l.contours[0].hole && l.contours[1].hole);
  assert_eq!(check_layer(l), 2500.0 - 49.0);
  // nothing dropped when cleaning is off
  let t = trace(&mask_img(80, 80, inside), &Options { specks: 0.0, holes: 0.0, ..Options::default() }).unwrap();
  assert_eq!(t.layers[0].contours.len(), 4);
}

#[test]
fn colour_layers() {
  let img = color_img(90, 60, |x, y| {
    if (5.0..25.0).contains(&x) && (10.0..50.0).contains(&y) {
      [220, 30, 30]
    } else if (35.0..55.0).contains(&x) && (10.0..50.0).contains(&y) {
      [30, 160, 40]
    } else if (65.0..85.0).contains(&x) && (10.0..50.0).contains(&y) {
      [20, 40, 200]
    } else {
      [250, 250, 250]
    }
  });
  let t = trace(&img, &Options { mode: Mode::Colors { count: 4 }, ..Options::default() }).unwrap();
  assert_eq!(t.layers.len(), 4);
  let bg: Vec<&Layer> = t.layers.iter().filter(|l| l.background).collect();
  assert_eq!(bg.len(), 1);
  assert_eq!(bg[0].color, [250, 250, 250]);
  let mut colours: Vec<[u8; 3]> = t.layers.iter().filter(|l| !l.background).map(|l| l.color).collect();
  colours.sort();
  assert_eq!(colours, vec![[20, 40, 200], [30, 160, 40], [220, 30, 30]]);
  for l in &t.layers {
    let a = check_layer(l);
    if !l.background {
      assert_eq!(l.contours.len(), 1);
      assert_eq!(l.contours[0].segs.len(), 4);
      assert_eq!(a, 800.0);
    } else {
      // the white surround, with three holes
      assert_eq!(a, 90.0 * 60.0 - 3.0 * 800.0);
    }
  }
}

#[test]
fn lines_only() {
  let img = mask_img(120, 120, |x, y| (x - 60.0).hypot(y - 60.0) < 40.0 && (x - 70.0).hypot(y - 65.0) > 12.0);
  for tolerance in [0.3, 1.0, 3.0] {
    let t = trace(&img, &Options { tolerance, ..opts(Fit::Lines) }).unwrap();
    let l = &t.layers[0];
    assert_eq!(count(l, |s| !matches!(s, Seg::Line { .. })), 0);
    assert_eq!(l.contours.len(), 2);
    check_layer(l);
    // within tolerance of the outline: the outer polygon's vertices sit near radius 40
    for s in &l.contours[0].segs {
      let a = s.start();
      assert!(((a[0] - 60.0).hypot(a[1] - 60.0) - 40.0).abs() < tolerance + 0.75, "{a:?}");
    }
  }
}

#[test]
fn invert_and_threshold() {
  // light disc on dark ground
  let img = color_img(80, 80, |x, y| if (x - 40.0).hypot(y - 40.0) < 20.0 { [200, 200, 200] } else { [40, 40, 40] });
  let t = trace(&img, &Options { invert: true, ..Options::default() }).unwrap();
  let l = &t.layers[0];
  assert_eq!(l.contours.len(), 1);
  assert!(matches!(l.contours[0].segs.as_slice(), [Seg::Circle { .. }]));
  assert_eq!(l.color, [200, 200, 200]);
  // a threshold below both levels finds nothing; above both, the whole image
  let t = trace(&img, &Options { mode: Mode::Outline { threshold: Some(10) }, ..Options::default() }).unwrap();
  assert!(t.layers[0].contours.is_empty());
  let t = trace(&img, &Options { mode: Mode::Outline { threshold: Some(250) }, ..Options::default() }).unwrap();
  assert_eq!(check_layer(&t.layers[0]), 6400.0);
}

#[test]
fn alpha_is_composited_on_white() {
  let (w, h) = (40u32, 40u32);
  let mut rgba = vec![0u8; (w * h * 4) as usize];
  for y in 10..30 {
    for x in 10..30 {
      let i = ((y * w + x) * 4) as usize;
      rgba[i + 3] = 255; // opaque black square; the rest transparent black
    }
  }
  let t = trace(&Image::from_rgba(w, h, rgba).unwrap(), &Options::default()).unwrap();
  assert_eq!(check_layer(&t.layers[0]), 400.0);
}

#[test]
fn downscales_to_max_side() {
  let img = mask_img(400, 100, |x, y| (x - 200.0).hypot(y - 50.0) < 30.0);
  let t = trace(&img, &Options { max_side: 200, ..Options::default() }).unwrap();
  assert_eq!((t.width, t.height), (200, 50));
  match t.layers[0].contours[0].segs.as_slice() {
    [Seg::Circle { center, radius }] => {
      assert!((radius - 15.0).abs() < 0.5 && (center[0] - 100.0).abs() < 0.3 && (center[1] - 25.0).abs() < 0.3);
    }
    s => panic!("{s:?}"),
  }
}

#[test]
fn blur_keeps_shapes() {
  let img = mask_img(80, 80, |x, y| (20.0..60.0).contains(&x) && (20.0..60.0).contains(&y));
  let t = trace(&img, &Options { blur: 1.5, ..Options::default() }).unwrap();
  assert_eq!(t.layers[0].contours.len(), 1);
  let a = check_layer(&t.layers[0]);
  assert!((a - 1600.0).abs() < 100.0, "{a}");
}

#[test]
fn empty_and_invalid() {
  let t = trace(&mask_img(10, 10, |_, _| false), &Options::default()).unwrap();
  assert!(t.layers[0].contours.is_empty());
  assert!(Image::from_rgba(2, 2, vec![0; 15]).is_err());
  assert!(Image::from_rgba(0, 2, vec![]).is_err());
  let img = mask_img(10, 10, |_, _| true);
  assert!(trace(&img, &Options { tolerance: 0.0, ..Options::default() }).is_err());
  assert!(trace(&img, &Options { mode: Mode::Colors { count: 0 }, ..Options::default() }).is_err());
  assert!(trace(&img, &Options { corner_angle: f64::NAN, ..Options::default() }).is_err());
}

#[test]
fn options_from_json() {
  let o: Options =
    serde_json::from_str(r#"{"mode": {"type": "colors", "count": 5}, "fit": "lines", "tolerance": 0.5}"#).unwrap();
  assert_eq!(o.mode, Mode::Colors { count: 5 });
  assert_eq!(o.fit, Fit::Lines);
  assert_eq!(o.tolerance, 0.5);
  assert_eq!(o.max_side, 2000);
  let o: Options = serde_json::from_str(r#"{"mode": {"type": "outline"}}"#).unwrap();
  assert_eq!(o, Options::default());
  let back: Options = serde_json::from_str(&serde_json::to_string(&Options::default()).unwrap()).unwrap();
  assert_eq!(back, Options::default());
}

#[test]
fn svg_is_well_formed() {
  let inside =
    |x: f64, y: f64| rounded_rect(x, y) && (x - 60.0).hypot(y - 70.0) > 20.0 || (x - 200.0).hypot(y - 20.0) < 10.0;
  let img = mask_img(220, 140, inside);
  let t = trace(&img, &Options::default()).unwrap();
  let svg = t.to_svg();
  assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 220 140\""));
  assert!(svg.ends_with("</svg>\n"));
  let shapes = t.layers[0].contours.iter().filter(|c| !c.hole).count();
  assert_eq!(svg.matches("<path ").count(), shapes);
  assert_eq!(svg.matches(" Z").count(), t.layers[0].contours.len());
  let mut reader = quick_xml::Reader::from_str(&svg);
  loop {
    match reader.read_event() {
      Ok(quick_xml::events::Event::Eof) => break,
      Ok(_) => {}
      Err(e) => panic!("bad SVG: {e}\n{svg}"),
    }
  }
}

#[test]
fn arcs_run_counter_clockwise_when_flagged() {
  // every arc's flattening must stay on its circle and end at b
  let img = mask_img(220, 140, rounded_rect);
  let t = trace(&img, &opts(Fit::Arcs)).unwrap();
  for c in &t.layers[0].contours {
    for s in &c.segs {
      if let Seg::Arc { a, b, center, ccw } = *s {
        let r = (a[0] - center[0]).hypot(a[1] - center[1]);
        assert!(((b[0] - center[0]).hypot(b[1] - center[1]) - r).abs() < 1e-6);
        // the arc's midpoint lies inside the shape's bounding box and on the outline side
        let sweep = crate::fit::arc_sweep(a, b, center, ccw);
        let a0 = (a[1] - center[1]).atan2(a[0] - center[0]);
        let m = a0 + if ccw { sweep / 2.0 } else { -sweep / 2.0 };
        let p = [center[0] + r * m.cos(), center[1] + r * m.sin()];
        assert!(sweep < PI, "corner arcs are short: {sweep}");
        assert!((0.0..220.0).contains(&p[0]) && (0.0..140.0).contains(&p[1]));
      }
    }
  }
}

/// Run with `cargo test --release -p clf-trace -- --ignored --nocapture`.
#[test]
#[ignore]
fn timing_1000px_logo() {
  let inside = |x: f64, y: f64| {
    let r = (x - 500.0).hypot(y - 500.0);
    ((300.0..420.0).contains(&r) && !(x > 500.0 && (y - 500.0).abs() < 60.0))
      || ((150.0..450.0).contains(&x) && (440.0..560.0).contains(&y))
      || letters((x - 100.0) / 3.0, (y - 600.0) / 3.0)
  };
  let img = mask_img(1000, 1000, inside);
  for fit in [Fit::Lines, Fit::Arcs, Fit::Curves] {
    let t0 = std::time::Instant::now();
    let t = trace(&img, &opts(fit)).unwrap();
    let dt = t0.elapsed();
    let segs: usize = t.layers[0].contours.iter().map(|c| c.segs.len()).sum();
    println!("{fit:?}: {} contours, {segs} segments in {dt:?}", t.layers[0].contours.len());
    check_layer(&t.layers[0]);
    let l = &t.layers[0];
    println!("{} lines {} arcs {} cubics", count(l, |s| matches!(s, Seg::Line { .. })), count(l, |s| matches!(s, Seg::Arc { .. })), count(l, |s| matches!(s, Seg::Cubic { .. })));
  }
  let t0 = std::time::Instant::now();
  let t = trace(&img, &Options { mode: Mode::Colors { count: 4 }, ..Options::default() }).unwrap();
  println!("colors: {} layers in {:?}", t.layers.len(), t0.elapsed());
}

#[test]
fn photo_like_colours_are_valid() {
  // smooth colour fields, a hard-edged disc and per-pixel noise
  let mut seed = 11u64;
  let noise: Vec<f64> = (0..160 * 120).map(|_| lcg(&mut seed) - 0.5).collect();
  let img = color_img(160, 120, |x, y| {
    let n = noise[(y as usize) * 160 + x as usize] * 60.0;
    let base = if (x - 100.0).hypot(y - 60.0) < 30.0 {
      [200.0, 40.0, 40.0]
    } else {
      [120.0 + 100.0 * (x / 25.0).sin(), 80.0 + y, 150.0 + 80.0 * (y / 17.0 + x / 31.0).cos()]
    };
    base.map(|v: f64| (v + n).clamp(0.0, 255.0) as u8)
  });
  for (fit, blur, specks) in
    [(Fit::Curves, 0.0, 0.0), (Fit::Arcs, 1.0, 16.0), (Fit::Lines, 0.0, 4.0), (Fit::Curves, 2.0, 30.0)]
  {
    let o = Options { mode: Mode::Colors { count: 6 }, fit, blur, specks, holes: specks, ..Options::default() };
    let t = trace(&img, &o).unwrap();
    assert!(t.layers.iter().filter(|l| l.background).count() == 1);
    for l in &t.layers {
      check_layer(l);
    }
  }
  // outline mode on the same image
  for tolerance in [0.3, 1.0, 4.0] {
    let t = trace(&img, &Options { tolerance, specks: 0.0, holes: 0.0, ..Options::default() }).unwrap();
    check_layer(&t.layers[0]);
  }
}

#[test]
fn extreme_options_stay_valid() {
  let img = mask_img(260, 110, letters);
  for fit in [Fit::Lines, Fit::Arcs, Fit::Curves] {
    for (tolerance, corner_angle) in [(0.05, 120.0), (20.0, 120.0), (1.0, 0.0), (1.0, 180.0), (3.0, 179.0)] {
      let o = Options { tolerance, corner_angle, ..opts(fit) };
      let t = trace(&img, &o).unwrap();
      let l = &t.layers[0];
      check_layer(l);
      assert_eq!(l.contours.len(), 6, "{fit:?} {tolerance} {corner_angle}");
    }
  }
}

#[test]
fn load_and_decode_files() {
  let img = mask_img(40, 30, |x, y| (x - 20.0).hypot(y - 15.0) < 10.0);
  let mut png = Vec::new();
  image::RgbaImage::from_raw(40, 30, img.rgba.clone())
    .unwrap()
    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
    .unwrap();
  assert_eq!(decode(&png).unwrap(), img);
  // the format is sniffed from the content, whatever the extension
  let path = std::env::temp_dir().join(format!("clf-trace-test-{}.jpg", std::process::id()));
  std::fs::write(&path, &png).unwrap();
  let loaded = load(&path);
  std::fs::remove_file(&path).ok();
  assert_eq!(loaded.unwrap(), img);
  assert!(matches!(load("/nonexistent/x.png"), Err(TraceError::Io(_))));
  assert!(matches!(decode(b"not an image"), Err(TraceError::Decode(_))));
}
