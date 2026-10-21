//! Kerf and minimum-feature check: the parts of a traced layer that a cut `kerf` wide (a laser
//! or plasma kerf, a waterjet stream, a router bit's diameter) can't make.
//!
//! On the cleaned mask the outlines are traced from, a disc of the kerf's diameter is rolled
//! through the shapes and through the gaps between them (a morphological opening, from distance
//! transforms), to within about a pixel. What the disc can't reach is too thin:
//! - in a shape: a neck or detail thinner than the kerf ([`NarrowKind::ThinShape`]), or a whole
//!   shape it can't fit in ([`NarrowKind::SmallShape`]): engraving or pocketing it with that
//!   tool loses it, and on a laser it is the size of the kerf's heat zone;
//! - in the background: a slot or gap between shapes narrower than the kerf
//!   ([`NarrowKind::NarrowGap`]), or a whole hole it can't fit in ([`NarrowKind::SmallHole`]): the
//!   cut can't get in, so the gap closes up or the hole is lost.
//!
//! Rolling a disc also rounds every corner it can't get into, which is how any round tool cuts
//! and not a problem; those slivers are left out (smaller than a quarter of the disc's area).

use serde::{Deserialize, Serialize};

/// What kind of spot is too narrow for the kerf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrowKind {
  /// Part of a shape narrower than the kerf.
  ThinShape,
  /// A whole shape smaller than the kerf.
  SmallShape,
  /// A gap or slot between shapes narrower than the kerf.
  NarrowGap,
  /// A whole hole smaller than the kerf.
  SmallHole,
}

/// One spot too narrow for the kerf.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Narrow {
  pub kind: NarrowKind,
  /// A point in the spot (pixels, y up): the one furthest from its edges.
  pub at: [f64; 2],
  /// How wide it is at its widest (px): under the kerf.
  pub width: f64,
  /// Its area (px²).
  pub area: f64,
}

/// Chamfer distance (1, √2) from each pixel centre to the nearest `seed` pixel centre. With
/// `border`, the outside of the image counts as seeds too.
fn distance(seed: &[bool], w: usize, h: usize, border: bool) -> Vec<f64> {
  let far = (w + h) as f64 * 2.0;
  let mut d: Vec<f64> = seed.iter().map(|&s| if s { 0.0 } else { far }).collect();
  let s2 = std::f64::consts::SQRT_2;
  let get = |d: &[f64], x: i64, y: i64| {
    if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
      if border { 0.0 } else { far }
    } else {
      d[y as usize * w + x as usize]
    }
  };
  for y in 0..h as i64 {
    for x in 0..w as i64 {
      let i = y as usize * w + x as usize;
      let v = (get(&d, x - 1, y) + 1.0)
        .min(get(&d, x, y - 1) + 1.0)
        .min(get(&d, x - 1, y - 1) + s2)
        .min(get(&d, x + 1, y - 1) + s2);
      d[i] = d[i].min(v);
    }
  }
  for y in (0..h as i64).rev() {
    for x in (0..w as i64).rev() {
      let i = y as usize * w + x as usize;
      let v = (get(&d, x + 1, y) + 1.0)
        .min(get(&d, x, y + 1) + 1.0)
        .min(get(&d, x + 1, y + 1) + s2)
        .min(get(&d, x - 1, y + 1) + s2);
      d[i] = d[i].min(v);
    }
  }
  d
}

/// 8-connected components of `on`: a label per pixel (`usize::MAX` off) and the count.
fn components(on: &[bool], w: usize, h: usize) -> (Vec<usize>, usize) {
  let mut label = vec![usize::MAX; on.len()];
  let mut n = 0;
  let mut stack = Vec::new();
  for start in 0..on.len() {
    if !on[start] || label[start] != usize::MAX {
      continue;
    }
    label[start] = n;
    stack.push(start);
    while let Some(i) = stack.pop() {
      let (x, y) = ((i % w) as i64, (i / w) as i64);
      for dy in -1..=1 {
        for dx in -1..=1 {
          let (nx, ny) = (x + dx, y + dy);
          if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
            continue;
          }
          let j = ny as usize * w + nx as usize;
          if on[j] && label[j] == usize::MAX {
            label[j] = n;
            stack.push(j);
          }
        }
      }
    }
    n += 1;
  }
  (label, n)
}

/// The spots of `region` a disc `kerf` px across can't reach. `border`: the image's outside is
/// the region's edge (shapes) rather than more of it (the background runs on past the image).
fn unreachable(
  region: &[bool],
  w: usize,
  h: usize,
  kerf: f64,
  border: bool,
  small: NarrowKind,
  thin: NarrowKind,
) -> Vec<Narrow> {
  let r = kerf / 2.0;
  let outside: Vec<bool> = region.iter().map(|v| !v).collect();
  // distance from each pixel centre of the region to the nearest centre outside it (its edge is
  // half a pixel nearer). A disc fits centred near the pixel when that is at least r: centred on
  // the pixel it may overlap the edge by half a pixel, which a centre between pixels avoids, so
  // the check is good to about a pixel
  let edge = distance(&outside, w, h, border);
  let centres: Vec<bool> = region.iter().zip(&edge).map(|(&on, &d)| on && d >= r).collect();
  let reach = distance(&centres, w, h, false);
  // the disc covers a pixel it reaches into, not only one whose centre it covers
  let missed: Vec<bool> = region.iter().zip(&reach).map(|(&on, &d)| on && d > r + 0.5).collect();
  // whole regions the disc never gets into
  let (region_label, regions) = components(region, w, h);
  let mut has_centre = vec![false; regions];
  for (i, &c) in centres.iter().enumerate() {
    if c {
      has_centre[region_label[i]] = true;
    }
  }
  let (label, n) = components(&missed, w, h);
  let mut area = vec![0usize; n];
  let mut best: Vec<(f64, usize)> = vec![(-1.0, 0); n];
  let mut whole = vec![false; n];
  for (i, &l) in label.iter().enumerate() {
    if l == usize::MAX {
      continue;
    }
    area[l] += 1;
    if edge[i] > best[l].0 {
      best[l] = (edge[i], i);
    }
    whole[l] = !has_centre[region_label[i]];
  }
  // the slivers a round tool leaves in corners are smaller than this
  let corner = (0.25 * std::f64::consts::PI * r * r).max(2.0);
  (0..n)
    .filter(|&l| whole[l] || area[l] as f64 >= corner)
    .map(|l| {
      let (d, i) = best[l];
      Narrow {
        kind: if whole[l] { small } else { thin },
        at: [(i % w) as f64 + 0.5, (i / w) as f64 + 0.5],
        width: (2.0 * (d - 0.5)).clamp(1.0, kerf),
        area: area[l] as f64,
      }
    })
    .collect()
}

/// Everything in a layer's (cleaned) mask that a cut `kerf` px wide can't make: its shapes'
/// thin parts and small shapes first, then narrow gaps and small holes.
pub(crate) fn check(mask: &[bool], w: usize, h: usize, kerf: f64) -> Vec<Narrow> {
  if !(kerf > 1.0) {
    // narrower than a pixel: nothing in a pixel image is thinner
    return Vec::new();
  }
  let mut out = unreachable(mask, w, h, kerf, true, NarrowKind::SmallShape, NarrowKind::ThinShape);
  let gaps: Vec<bool> = mask.iter().map(|v| !v).collect();
  out.extend(unreachable(&gaps, w, h, kerf, false, NarrowKind::SmallHole, NarrowKind::NarrowGap));
  out
}
