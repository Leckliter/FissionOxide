//! Segmentation: which pixels belong to which layer.
//!
//! Outline mode thresholds the grey level (Otsu's method when no level is given). Colours mode
//! runs k-means in CIE Lab on a 15-bit colour histogram (so the cost doesn't grow with the
//! image), seeded deterministically: the most common colour first, then repeatedly the colour
//! with the largest count × squared distance to the chosen ones.

use crate::raster::Rgb;

pub(crate) struct LayerMask {
  pub color: [u8; 3],
  pub background: bool,
  /// Foreground pixels, y-up rows.
  pub mask: Vec<bool>,
}

/// Lloyd iterations at most.
const KMEANS_ITERS: usize = 40;

pub(crate) fn outline(rgb: &Rgb, threshold: Option<u8>, invert: bool) -> LayerMask {
  let grey = rgb.grey();
  let t = threshold.unwrap_or_else(|| otsu(&grey));
  let mask: Vec<bool> = grey.iter().map(|&g| (g < t) != invert).collect();
  let color = mean_color(rgb, |i| mask[i]);
  LayerMask { color, background: false, mask }
}

/// The level `t` that best splits the histogram into `< t` and `>= t` (128 for a flat image).
/// Where several levels split equally well (two clean peaks), the middle one.
pub(crate) fn otsu(grey: &[u8]) -> u8 {
  let mut hist = [0u64; 256];
  for &g in grey {
    hist[g as usize] += 1;
  }
  let total = grey.len() as f64;
  let sum_all: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
  let mut var = [0.0f64; 256];
  let (mut wb, mut sb) = (0.0, 0.0);
  for (i, &c) in hist.iter().enumerate().take(255) {
    wb += c as f64;
    sb += i as f64 * c as f64;
    let wf = total - wb;
    if wb > 0.0 && wf > 0.0 {
      let (mb, mf) = (sb / wb, (sum_all - sb) / wf);
      var[i + 1] = wb * wf * (mb - mf) * (mb - mf);
    }
  }
  let best = var.iter().copied().fold(0.0, f64::max);
  if !(best > 0.0) {
    return 128;
  }
  let near = |v: f64| v >= best * (1.0 - 1e-9);
  let first = var.iter().position(|&v| near(v)).unwrap_or(128);
  let last = var.iter().rposition(|&v| near(v)).unwrap_or(first);
  // levels in first..=last split identically; keep the middle of the gap they leave
  ((first + last).div_ceil(2)).min(255) as u8
}

fn mean_color(rgb: &Rgb, inside: impl Fn(usize) -> bool) -> [u8; 3] {
  let (mut sum, mut n) = ([0f64; 3], 0usize);
  for (i, p) in rgb.px.iter().enumerate() {
    if inside(i) {
      for c in 0..3 {
        sum[c] += f64::from(p[c]);
      }
      n += 1;
    }
  }
  if n == 0 {
    return [0, 0, 0];
  }
  sum.map(|s| (s / n as f64).round().clamp(0.0, 255.0) as u8)
}

fn bin_of(p: &[f32; 3]) -> usize {
  let q = |v: f32| (v.round().clamp(0.0, 255.0) as usize) >> 3;
  (q(p[0]) << 10) | (q(p[1]) << 5) | q(p[2])
}

fn lab(rgb: [f64; 3]) -> [f64; 3] {
  let lin = |c: f64| {
    let c = c / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
  };
  let (r, g, b) = (lin(rgb[0]), lin(rgb[1]), lin(rgb[2]));
  let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
  let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
  let f = |t: f64| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
  let (fx, fy, fz) = (f(x), f(y), f(z));
  [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn d2(a: [f64; 3], b: [f64; 3]) -> f64 {
  (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// One layer per colour cluster (empty clusters dropped), largest first; the one most common
/// along the image border is the background.
pub(crate) fn colors(rgb: &Rgb, count: u8) -> Vec<LayerMask> {
  // histogram: count and RGB sum per 5-bit-per-channel bin
  let mut bins = vec![(0u64, [0f64; 3]); 1 << 15];
  for p in &rgb.px {
    let b = &mut bins[bin_of(p)];
    b.0 += 1;
    for (sum, &v) in b.1.iter_mut().zip(p) {
      *sum += f64::from(v);
    }
  }
  let used: Vec<usize> = (0..bins.len()).filter(|&i| bins[i].0 > 0).collect();
  let weight: Vec<f64> = used.iter().map(|&i| bins[i].0 as f64).collect();
  let item: Vec<[f64; 3]> = used.iter().map(|&i| lab(bins[i].1.map(|s| s / bins[i].0 as f64))).collect();

  // deterministic seeding
  let mut centers: Vec<[f64; 3]> = Vec::new();
  let mut nearest = vec![f64::INFINITY; item.len()];
  let mut pick = (0..item.len()).max_by(|&a, &b| weight[a].total_cmp(&weight[b]));
  while let Some(p) = pick {
    centers.push(item[p]);
    if centers.len() >= count as usize {
      break;
    }
    for (k, it) in item.iter().enumerate() {
      nearest[k] = nearest[k].min(d2(*it, item[p]));
    }
    pick = (0..item.len())
      .filter(|&k| nearest[k] > 1e-9)
      .max_by(|&a, &b| (weight[a] * nearest[a]).total_cmp(&(weight[b] * nearest[b])));
  }

  // Lloyd iterations on the weighted bins
  let closest =
    |c: &[[f64; 3]], v: [f64; 3]| (0..c.len()).min_by(|&a, &b| d2(c[a], v).total_cmp(&d2(c[b], v))).unwrap_or(0);
  let mut assign: Vec<usize> = item.iter().map(|&v| closest(&centers, v)).collect();
  for _ in 0..KMEANS_ITERS {
    let mut acc = vec![([0f64; 3], 0f64); centers.len()];
    for ((&a, it), &wt) in assign.iter().zip(&item).zip(&weight) {
      for (sum, &v) in acc[a].0.iter_mut().zip(it) {
        *sum += v * wt;
      }
      acc[a].1 += wt;
    }
    for (c, (s, w)) in centers.iter_mut().zip(&acc) {
      if *w > 0.0 {
        *c = s.map(|v| v / w);
      }
    }
    let next: Vec<usize> = item.iter().map(|&v| closest(&centers, v)).collect();
    if next == assign {
      break;
    }
    assign = next;
  }

  // per-pixel labels via the bins
  let mut of_bin = vec![0usize; bins.len()];
  for (k, &i) in used.iter().enumerate() {
    of_bin[i] = assign[k];
  }
  let labels: Vec<usize> = rgb.px.iter().map(|p| of_bin[bin_of(p)]).collect();
  let mut area = vec![0usize; centers.len()];
  for &l in &labels {
    area[l] += 1;
  }
  let (w, h) = (rgb.w, rgb.h);
  let mut border = vec![0usize; centers.len()];
  for y in 0..h {
    for x in 0..w {
      if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
        border[labels[y * w + x]] += 1;
      }
    }
  }
  let bg = (0..centers.len()).max_by_key(|&c| (border[c], area[c])).unwrap_or(0);
  let mut order: Vec<usize> = (0..centers.len()).filter(|&c| area[c] > 0).collect();
  order.sort_by_key(|&c| std::cmp::Reverse(area[c]));
  order
    .into_iter()
    .map(|c| LayerMask {
      color: mean_color(rgb, |i| labels[i] == c),
      background: c == bg,
      mask: labels.iter().map(|&l| l == c).collect(),
    })
    .collect()
}
