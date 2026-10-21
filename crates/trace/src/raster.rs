//! Pixel preparation: alpha compositing on white, area-averaging downscale and Gaussian blur.
//!
//! Pixels are held as `f32` sRGB triples (0–255) with row 0 at the **bottom**, so every later
//! step works in the y-up coordinates of the output.

use crate::Image;

pub(crate) struct Rgb {
  pub w: usize,
  pub h: usize,
  pub px: Vec<[f32; 3]>,
}

impl Rgb {
  /// The image composited over white, flipped to y-up rows, and shrunk (averaging the covered
  /// source area) so its longer side is at most `max_side`. Source rows are streamed, so a big
  /// photo never exists at full size in floating point. The caller checked the size.
  pub fn from_image(img: &Image, max_side: u32) -> Rgb {
    let (w, h) = (img.width as usize, img.height as usize);
    let (nw, nh) = crate::traced_size(img.width, img.height, max_side);
    let (nw, nh) = (nw as usize, nh as usize);
    let wx = weights(w, nw);
    // for each source row (top down): the output rows it feeds, and how much
    let mut feeds: Vec<Vec<(usize, f32)>> = vec![Vec::new(); h];
    for (oy, ws) in weights(h, nh).into_iter().enumerate() {
      for (sy, k) in ws {
        feeds[sy].push((nh - 1 - oy, k));
      }
    }
    let mut px = vec![[0f32; 3]; nw * nh];
    let (mut line, mut hline) = (vec![[0f32; 3]; w], vec![[0f32; 3]; nw]);
    for (row, src) in img.rgba.chunks_exact(w * 4).enumerate().take(h) {
      for (p, c) in line.iter_mut().zip(src.chunks_exact(4)) {
        let a = f32::from(c[3]) / 255.0;
        let bg = 255.0 * (1.0 - a);
        *p = [f32::from(c[0]) * a + bg, f32::from(c[1]) * a + bg, f32::from(c[2]) * a + bg];
      }
      for (hp, ws) in hline.iter_mut().zip(&wx) {
        *hp = mix(ws.iter().map(|&(i, kx)| (line[i], kx)));
      }
      for &(oy, k) in &feeds[row] {
        for (o, v) in px[oy * nw..(oy + 1) * nw].iter_mut().zip(&hline) {
          for c in 0..3 {
            o[c] += v[c] * k;
          }
        }
      }
    }
    Rgb { w: nw, h: nh, px }
  }

  /// Separable Gaussian blur with standard deviation `sigma` px, edges clamped.
  pub fn blur(&mut self, sigma: f64) {
    let r = (3.0 * sigma).ceil() as usize;
    if r == 0 {
      return;
    }
    let mut k: Vec<f32> =
      (0..=2 * r).map(|i| (-((i as f64 - r as f64).powi(2)) / (2.0 * sigma * sigma)).exp() as f32).collect();
    let sum: f32 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= sum);
    let (w, h) = (self.w, self.h);
    let mut tmp = vec![[0f32; 3]; w * h];
    for y in 0..h {
      for x in 0..w {
        tmp[y * w + x] =
          mix(k.iter().enumerate().map(|(i, &kv)| (self.px[y * w + (x + i).saturating_sub(r).min(w - 1)], kv)));
      }
    }
    for y in 0..h {
      for x in 0..w {
        self.px[y * w + x] =
          mix(k.iter().enumerate().map(|(i, &kv)| (tmp[(y + i).saturating_sub(r).min(h - 1) * w + x], kv)));
      }
    }
  }

  /// Luma (Rec. 601) per pixel, rounded to 0–255.
  pub fn grey(&self) -> Vec<u8> {
    self.px.iter().map(|p| (0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]).round().clamp(0.0, 255.0) as u8).collect()
  }
}

fn mix(it: impl Iterator<Item = ([f32; 3], f32)>) -> [f32; 3] {
  let mut acc = [0f32; 3];
  for (p, k) in it {
    acc[0] += p[0] * k;
    acc[1] += p[1] * k;
    acc[2] += p[2] * k;
  }
  acc
}

/// For each of `m` output cells along an axis of `n` source pixels: the covered source pixels
/// and their share of the cell (summing to 1).
fn weights(n: usize, m: usize) -> Vec<Vec<(usize, f32)>> {
  let s = n as f64 / m as f64;
  (0..m)
    .map(|o| {
      let (lo, hi) = (o as f64 * s, (o + 1) as f64 * s);
      let mut v = Vec::new();
      let mut i = lo.floor() as usize;
      while (i as f64) < hi && i < n {
        let cover = hi.min(i as f64 + 1.0) - lo.max(i as f64);
        if cover > 0.0 {
          v.push((i, (cover / s) as f32));
        }
        i += 1;
      }
      v
    })
    .collect()
}
