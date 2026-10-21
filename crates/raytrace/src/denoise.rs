//! The denoiser: an edge-avoiding à-trous wavelet filter (Dammertz et al.) guided by the first
//! hit's albedo, normal and depth, run on the lighting with the albedo divided out so textures
//! stay sharp. No pure-Rust port of a learned denoiser (OIDN, OptiX) exists, and this needs no
//! native library.

use glam::Vec3;

use crate::sampling::luminance;

pub struct Guides<'a> {
  pub albedo: &'a [Vec3],
  pub normal: &'a [Vec3],
  pub depth: &'a [f32],
  /// Variance of each pixel's mean luminance.
  pub variance: &'a [f32],
}

const KERNEL: [f32; 3] = [3.0 / 8.0, 1.0 / 4.0, 1.0 / 16.0];

/// Filters `color` (linear radiance) in place.
pub fn denoise(color: &mut [Vec3], width: u32, height: u32, g: &Guides) {
  let (w, h) = (width as i32, height as i32);
  let n = color.len();
  if n == 0 || g.albedo.len() != n {
    return;
  }
  let demod: Vec<Vec3> = g.albedo.iter().map(|a| a.max(Vec3::splat(0.02))).collect();
  let mut cur: Vec<Vec3> = color.iter().zip(&demod).map(|(c, a)| *c / *a).collect();
  let mut var: Vec<f32> = g.variance.iter().zip(&demod).map(|(v, a)| v / luminance(*a).powi(2)).collect();
  // depth tolerance relative to the scene depth
  let finite: Vec<f32> = g.depth.iter().copied().filter(|d| *d < 1e8).collect();
  let depth_scale = if finite.is_empty() { 1.0 } else { finite.iter().sum::<f32>() / finite.len() as f32 * 0.02 };
  let mut next = cur.clone();
  let mut next_var = var.clone();
  for it in 0..5 {
    let step = 1 << it;
    for y in 0..h {
      for x in 0..w {
        let i = (y * w + x) as usize;
        let (c0, n0, d0) = (cur[i], g.normal[i], g.depth[i]);
        let l0 = luminance(c0);
        let sigma_l = 4.0 * var[i].max(1e-8).sqrt() + 1e-4;
        let (mut sum, mut wsum, mut vsum) = (Vec3::ZERO, 0.0, 0.0);
        for dy in -2i32..=2 {
          let yy = y + dy * step;
          if yy < 0 || yy >= h {
            continue;
          }
          for dx in -2i32..=2 {
            let xx = x + dx * step;
            if xx < 0 || xx >= w {
              continue;
            }
            let j = (yy * w + xx) as usize;
            let k = KERNEL[dx.unsigned_abs() as usize] * KERNEL[dy.unsigned_abs() as usize];
            let wn = n0.dot(g.normal[j]).max(0.0).powi(64);
            let wd = (-(d0 - g.depth[j]).abs() / (depth_scale * step as f32)).exp();
            let wl = (-(l0 - luminance(cur[j])).abs() / sigma_l).exp();
            let wa = (-(g.albedo[i] - g.albedo[j]).length_squared() * 50.0).exp();
            let wt = k * wn * wd * wl * wa;
            sum += cur[j] * wt;
            wsum += wt;
            vsum += var[j] * wt * wt;
          }
        }
        if wsum > 0.0 {
          next[i] = sum / wsum;
          next_var[i] = vsum / (wsum * wsum);
        } else {
          next[i] = c0;
          next_var[i] = var[i];
        }
      }
    }
    std::mem::swap(&mut cur, &mut next);
    std::mem::swap(&mut var, &mut next_var);
  }
  for ((c, f), a) in color.iter_mut().zip(&cur).zip(&demod) {
    *c = *f * *a;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::sampling::Rng;

  #[test]
  fn smooths_noise_but_keeps_edges() {
    let (w, h) = (32u32, 32u32);
    let mut rng = Rng::new(1);
    let truth = |x: u32| if x < 16 { 0.2 } else { 0.8 };
    let mut color: Vec<Vec3> = (0..w * h).map(|i| Vec3::splat(truth(i % w) * (0.5 + rng.f32()))).collect();
    let albedo: Vec<Vec3> = (0..w * h).map(|i| if i % w < 16 { Vec3::splat(0.3) } else { Vec3::splat(0.9) }).collect();
    let normal = vec![Vec3::Z; (w * h) as usize];
    let depth = vec![10.0; (w * h) as usize];
    let variance = vec![0.01; (w * h) as usize];
    let err = |c: &[Vec3]| c.iter().enumerate().map(|(i, v)| (v.x - truth(i as u32 % w)).abs()).sum::<f32>();
    let before = err(&color);
    denoise(&mut color, w, h, &Guides { albedo: &albedo, normal: &normal, depth: &depth, variance: &variance });
    let after = err(&color);
    assert!(after < before * 0.5, "{before} -> {after}");
    // the edge stays put
    assert!(color[(16 * w + 14) as usize].x < 0.35 && color[(16 * w + 17) as usize].x > 0.6);
  }
}
