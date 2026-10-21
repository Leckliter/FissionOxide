#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Kerf and minimum-feature check on generated images.

use crate::*;

/// Black shapes on white; `inside(x, y)` gets pixel centres in y-up coordinates.
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

fn narrow(image: &Image, kerf: f64) -> Vec<Narrow> {
  let t = trace(image, &Options { kerf, specks: 4.0, holes: 4.0, ..Options::default() }).unwrap();
  t.layers.iter().flat_map(|l| l.narrow.clone()).collect()
}

fn inside(n: &Narrow, x: std::ops::Range<f64>, y: std::ops::Range<f64>) -> bool {
  x.contains(&n.at[0]) && y.contains(&n.at[1])
}

/// A 120 × 60 plate from (20, 20): a 3 px slot cut in from its top edge at x = 50, a 4 px hole,
/// a 20 px hole, and a 2 px tab sticking out of its right side; a 5 px dot beside it.
fn plate(x: f64, y: f64) -> bool {
  let body = (20.0..140.0).contains(&x) && (20.0..80.0).contains(&y);
  let slot = (48.5..51.5).contains(&x) && y > 50.0;
  let small_hole = (x - 80.0).hypot(y - 50.0) < 2.0;
  let big_hole = (x - 115.0).hypot(y - 50.0) < 10.0;
  let tab = (140.0..160.0).contains(&x) && (49.0..51.0).contains(&y);
  let dot = (170.0..175.0).contains(&x) && (48.0..53.0).contains(&y);
  (body && !slot && !small_hole && !big_hole) || tab || dot
}

#[test]
fn spots_narrower_than_the_kerf_are_found_and_named() {
  let image = img(190, 100, plate);
  let found = narrow(&image, 8.0);
  let of = |k: NarrowKind| found.iter().filter(|n| n.kind == k).collect::<Vec<_>>();
  // the slot, as one narrow gap along it
  let gaps = of(NarrowKind::NarrowGap);
  assert_eq!(gaps.len(), 1, "{found:?}");
  assert!(inside(gaps[0], 47.0..53.0, 50.0..81.0), "{:?}", gaps[0]);
  assert!((gaps[0].width - 3.0).abs() <= 1.0, "{:?}", gaps[0]);
  // the 4 px hole, not the 20 px one
  let holes = of(NarrowKind::SmallHole);
  assert_eq!(holes.len(), 1, "{found:?}");
  assert!(inside(holes[0], 77.0..83.0, 47.0..53.0), "{:?}", holes[0]);
  // the tab is a thin part of the plate; the dot a whole shape too small
  let thin = of(NarrowKind::ThinShape);
  assert_eq!(thin.len(), 1, "{found:?}");
  assert!(inside(thin[0], 139.0..161.0, 48.0..52.0), "{:?}", thin[0]);
  assert!(thin[0].width <= 3.0, "{:?}", thin[0]);
  let small = of(NarrowKind::SmallShape);
  assert_eq!(small.len(), 1, "{found:?}");
  assert!(inside(small[0], 169.0..176.0, 47.0..54.0), "{:?}", small[0]);
  // a finer cut makes all of them
  assert!(narrow(&image, 1.5).is_empty(), "{:?}", narrow(&image, 1.5));
  // and between: the 3 px slot and 4 px hole are fine for a 2.5 px kerf, the 2 px tab isn't
  let mid = narrow(&image, 2.5);
  assert!(mid.iter().all(|n| n.kind == NarrowKind::ThinShape), "{mid:?}");
}

#[test]
fn rounded_corners_are_not_flagged() {
  // a plain rectangle and an L: every corner rounds under a big kerf, nothing is too thin
  let image = img(120, 120, |x, y| {
    let rect = (10.0..60.0).contains(&x) && (10.0..50.0).contains(&y);
    let l = ((70.0..110.0).contains(&x) && (70.0..82.0).contains(&y)) || ((70.0..82.0).contains(&x) && (70.0..110.0).contains(&y));
    rect || l
  });
  let found = narrow(&image, 10.0);
  assert!(found.is_empty(), "{found:?}");
}

#[test]
fn kerf_is_off_by_default_and_checked() {
  let image = img(190, 100, plate);
  let t = trace(&image, &Options::default()).unwrap();
  assert!(t.layers.iter().all(|l| l.narrow.is_empty()));
  assert!(trace(&image, &Options { kerf: -1.0, ..Options::default() }).is_err());
  assert!(trace(&image, &Options { kerf: f64::NAN, ..Options::default() }).is_err());
  // the background colour layer isn't checked
  let t = trace(&image, &Options { kerf: 8.0, mode: Mode::Colors { count: 2 }, ..Options::default() }).unwrap();
  assert!(t.layers.iter().filter(|l| l.background).all(|l| l.narrow.is_empty()));
  assert!(t.layers.iter().any(|l| !l.narrow.is_empty()));
}
