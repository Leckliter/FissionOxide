#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Centre-line mode on generated stroke images.

use crate::validate::flatten;
use crate::*;

/// Black strokes on white; `inside(x, y)` gets pixel centres in y-up coordinates.
fn img(w: u32, h: u32, inside: impl Fn(f64, f64) -> bool) -> Image {
  let mut rgba = Vec::with_capacity((w * h * 4) as usize);
  for row in 0..h {
    for x in 0..w {
      let v = if inside(x as f64 + 0.5, (h - 1 - row) as f64 + 0.5) { 0 } else { 255 };
      rgba.extend([v, v, v, 255]);
    }
  }
  Image::from_rgba(w, h, rgba).unwrap()
}

fn centerline() -> Options {
  Options { mode: Mode::Centerline { threshold: None }, specks: 4.0, holes: 4.0, ..Options::default() }
}

fn strokes(image: &Image, options: &Options) -> Vec<Stroke> {
  let t = trace(image, options).unwrap();
  assert_eq!(t.layers.len(), 1);
  assert!(t.layers[0].contours.is_empty());
  let s = t.layers[0].strokes.clone();
  for (i, st) in s.iter().enumerate() {
    assert!(!st.segs.is_empty());
    for w in st.segs.windows(2) {
      assert!(w[0].end() == w[1].start(), "stroke {i} broken between {:?} and {:?}", w[0], w[1]);
    }
    if st.closed {
      assert!(st.segs[0].start() == st.segs[st.segs.len() - 1].end(), "closed stroke {i} doesn't close");
    }
  }
  s
}

/// Points along a stroke.
fn points(st: &Stroke) -> Vec<[f64; 2]> {
  let mut pts = vec![st.segs[0].start()];
  for s in &st.segs {
    flatten(s, false, &mut pts);
  }
  pts
}

fn ends(st: &Stroke) -> [[f64; 2]; 2] {
  [st.segs[0].start(), st.segs[st.segs.len() - 1].end()]
}

fn near(a: [f64; 2], b: [f64; 2], tol: f64) -> bool {
  (a[0] - b[0]).hypot(a[1] - b[1]) <= tol
}

#[test]
fn a_bar_becomes_one_line_along_its_middle() {
  // 8 px thick, from x = 10 to 110, centred on y = 20
  let image = img(120, 40, |x, y| (10.0..110.0).contains(&x) && (16.0..24.0).contains(&y));
  let s = strokes(&image, &centerline());
  assert_eq!(s.len(), 1, "{s:?}");
  let st = &s[0];
  assert!(!st.closed);
  assert_eq!(st.segs.len(), 1, "{:?}", st.segs);
  let Seg::Line { a, b } = st.segs[0] else { panic!("{:?}", st.segs[0]) };
  let (l, r) = if a[0] < b[0] { (a, b) } else { (b, a) };
  // on the middle, reaching (about) out to the bar's ends
  assert!((l[1] - 20.0).abs() < 0.6 && (r[1] - 20.0).abs() < 0.6, "{l:?} {r:?}");
  assert!(l[0] < 14.5 && r[0] > 105.5, "{l:?} {r:?}");
  assert!((st.width - 8.0).abs() < 1.5, "width {}", st.width);
}

#[test]
fn a_diagonal_stroke_stays_straight() {
  // 6 px wide band along y = x
  let image = img(100, 100, |x, y| (x - y).abs() < 3.0 * std::f64::consts::SQRT_2 && (10.0..90.0).contains(&x));
  let s = strokes(&image, &centerline());
  assert_eq!(s.len(), 1, "{s:?}");
  assert_eq!(s[0].segs.len(), 1, "{:?}", s[0].segs);
  let [a, b] = ends(&s[0]);
  let ang = (b[1] - a[1]).atan2(b[0] - a[0]).to_degrees().rem_euclid(180.0);
  assert!((ang - 45.0).abs() < 2.0, "{ang}");
  for p in [a, b] {
    assert!((p[0] - p[1]).abs() < 1.0, "{p:?}");
  }
}

#[test]
fn an_l_keeps_its_sharp_corner() {
  let image = img(100, 100, |x, y| {
    let v = (16.0..24.0).contains(&x) && (16.0..90.0).contains(&y);
    let h = (16.0..90.0).contains(&x) && (16.0..24.0).contains(&y);
    v || h
  });
  let s = strokes(&image, &centerline());
  assert_eq!(s.len(), 1, "{s:?}");
  let lines: Vec<_> = s[0].segs.iter().filter(|g| matches!(g, Seg::Line { .. })).collect();
  assert_eq!(lines.len(), s[0].segs.len(), "{:?}", s[0].segs);
  assert!(s[0].segs.len() <= 3, "{:?}", s[0].segs);
  // the corner is near (20, 20) and the two legs are vertical and horizontal
  let pts = points(&s[0]);
  assert!(pts.iter().any(|p| near(*p, [20.0, 20.0], 2.0)), "{pts:?}");
  let [a, b] = ends(&s[0]);
  let (top, right) = if a[1] > b[1] { (a, b) } else { (b, a) };
  assert!((top[0] - 20.0).abs() < 1.0 && top[1] > 85.0, "{top:?}");
  assert!((right[1] - 20.0).abs() < 1.0 && right[0] > 85.0, "{right:?}");
}

#[test]
fn a_plus_gives_four_strokes_sharing_the_junction() {
  let image = img(101, 101, |x, y| {
    ((46.0..55.0).contains(&x) && (10.0..91.0).contains(&y)) || ((46.0..55.0).contains(&y) && (10.0..91.0).contains(&x))
  });
  let s = strokes(&image, &centerline());
  assert_eq!(s.len(), 4, "{s:?}");
  // every stroke has exactly one end at the very same junction point
  let all: Vec<[f64; 2]> = s.iter().flat_map(ends).collect();
  let shared: Vec<[f64; 2]> = all.iter().copied().filter(|p| all.iter().filter(|q| *q == p).count() == 4).collect();
  assert_eq!(shared.len(), 4, "{all:?}");
  assert!(near(shared[0], [50.5, 50.5], 1.5), "{:?}", shared[0]);
  for st in &s {
    assert_eq!(st.segs.len(), 1, "{:?}", st.segs);
  }
}

#[test]
fn a_t_keeps_its_short_stem() {
  // a stem only a little longer than the bar is thick is a real branch, not a spur
  let image = img(100, 60, |x, y| {
    ((10.0..90.0).contains(&x) && (40.0..48.0).contains(&y)) || ((46.0..54.0).contains(&x) && (14.0..48.0).contains(&y))
  });
  let s = strokes(&image, &centerline());
  assert_eq!(s.len(), 3, "{s:?}");
}

#[test]
fn a_ring_is_one_closed_circle() {
  let (cx, cy) = (50.0, 50.0);
  let image = img(100, 100, |x, y| {
    let r = (x - cx).hypot(y - cy);
    (26.0..34.0).contains(&r)
  });
  let s = strokes(&image, &centerline());
  assert_eq!(s.len(), 1, "{s:?}");
  assert!(s[0].closed);
  let Seg::Circle { center, radius } = s[0].segs[0] else { panic!("{:?}", s[0].segs) };
  assert!(near(center, [cx, cy], 0.6), "{center:?}");
  assert!((radius - 30.0).abs() < 1.0, "{radius}");
  // lines only: a closed polygon instead
  let poly = strokes(&image, &Options { fit: Fit::Lines, ..centerline() });
  assert_eq!(poly.len(), 1);
  assert!(poly[0].closed && poly[0].segs.len() >= 8);
  assert!(poly[0].segs.iter().all(|g| matches!(g, Seg::Line { .. })));
}

#[test]
fn a_bent_stroke_fits_arcs_or_curves_within_tolerance() {
  // a half ring, 6 px thick, radius 40
  let image = img(120, 70, |x, y| {
    let r = (x - 60.0).hypot(y - 10.0);
    y >= 10.0 && (37.0..43.0).contains(&r)
  });
  for fit in [Fit::Lines, Fit::Arcs, Fit::Curves] {
    let s = strokes(&image, &Options { fit, ..centerline() });
    assert_eq!(s.len(), 1, "{fit:?} {s:?}");
    let pts = points(&s[0]);
    // the fitted line stays on the stroke's middle
    for p in &pts {
      let r = (p[0] - 60.0).hypot(p[1] - 10.0);
      assert!((r - 40.0).abs() < 2.0, "{fit:?}: {p:?} at radius {r}");
    }
    match fit {
      Fit::Lines => assert!(s[0].segs.iter().all(|g| matches!(g, Seg::Line { .. }))),
      _ => {
        assert!(s[0].segs.iter().any(|g| matches!(g, Seg::Arc { .. } | Seg::Cubic { .. })), "{:?}", s[0].segs);
        assert!(s[0].segs.len() <= 4, "{fit:?}: {:?}", s[0].segs);
      }
    }
  }
}

#[test]
fn small_bumps_are_pruned_as_spurs() {
  // a bar with a little knob on its side
  let image = img(120, 40, |x, y| {
    let bar = (10.0..110.0).contains(&x) && (16.0..24.0).contains(&y);
    let knob = (58.0..64.0).contains(&x) && (24.0..32.0).contains(&y);
    bar || knob
  });
  let s = strokes(&image, &centerline());
  assert_eq!(s.len(), 1, "{s:?}");
  // with pruning off, the knob is a branch of its own
  let raw = strokes(&image, &Options { spurs: Some(0.0), ..centerline() });
  assert!(raw.len() >= 3, "{raw:?}");
}

#[test]
fn centerline_options_are_checked_and_svg_draws_strokes() {
  let image = img(60, 20, |x, y| (5.0..55.0).contains(&x) && (7.0..13.0).contains(&y));
  assert!(trace(&image, &Options { spurs: Some(-1.0), ..centerline() }).is_err());
  let t = trace(&image, &centerline()).unwrap();
  let svg = t.to_svg();
  assert!(svg.contains("fill=\"none\"") && svg.contains("stroke-width"), "{svg}");
  // round-trips through JSON
  let back: Trace = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
  assert_eq!(back, t);
}
