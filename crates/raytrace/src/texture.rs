//! Textures: images and built-in patterns baked to a tile, and the projections that map surface
//! points to texture coordinates. The GPU viewport uses the same tiles and the same maths (see
//! `fox-render-gl`'s PBR shader), so its preview matches the final render.

use std::f32::consts::{PI, TAU};

use glam::{Vec2, Vec3};

use crate::material::{Axis, Material, Pattern, Projection, TextureSource, TextureSpec};
use crate::sampling::{linear_to_srgb, srgb_to_linear, srgb3};

/// Size of a baked pattern tile.
pub const PATTERN_SIZE: u32 = 256;
/// Largest image side kept (bigger images are scaled down).
pub const MAX_IMAGE: u32 = 2048;

/// A tiling texture in linear RGB.
#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
  pub width: u32,
  pub height: u32,
  pub data: Vec<[f32; 3]>,
}

impl Texture {
  /// Bilinear lookup, repeating outside [0, 1).
  pub fn sample(&self, uv: Vec2) -> Vec3 {
    let (w, h) = (self.width as i64, self.height as i64);
    let x = uv.x.rem_euclid(1.0) * w as f32 - 0.5;
    let y = (1.0 - uv.y.rem_euclid(1.0)) * h as f32 - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |i: i64, j: i64| -> Vec3 {
      let i = i.rem_euclid(w) as usize;
      let j = j.rem_euclid(h) as usize;
      Vec3::from(self.data[j * w as usize + i])
    };
    let (i, j) = (x0 as i64, y0 as i64);
    let top = at(i, j).lerp(at(i + 1, j), fx);
    let bottom = at(i, j + 1).lerp(at(i + 1, j + 1), fx);
    top.lerp(bottom, fy)
  }

  /// sRGB RGBA8 pixels, top row first (for uploading to the GPU).
  pub fn to_srgb8(&self) -> Vec<u8> {
    let mut out = Vec::with_capacity(self.data.len() * 4);
    for c in &self.data {
      for v in c {
        out.push((linear_to_srgb(*v) * 255.0 + 0.5) as u8);
      }
      out.push(255);
    }
    out
  }

  /// Loads an image file, tinted by `tint` (linear).
  pub fn load(path: &str, tint: Vec3) -> Result<Texture, String> {
    let img = image::ImageReader::open(path)
      .and_then(|r| r.with_guessed_format())
      .map_err(|e| format!("{path}: {e}"))?
      .decode()
      .map_err(|e| format!("{path}: {e}"))?;
    let img = if img.width() > MAX_IMAGE || img.height() > MAX_IMAGE {
      img.resize(MAX_IMAGE, MAX_IMAGE, image::imageops::FilterType::Triangle)
    } else {
      img
    };
    let rgb = img.to_rgb8();
    let data = rgb.pixels().map(|p| [0, 1, 2].map(|k| srgb_to_linear(p.0[k]) * tint[k])).collect();
    Ok(Texture { width: rgb.width(), height: rgb.height(), data })
  }

  /// A built-in pattern blending colour `a` (value 0) to `b` (value 1), both linear.
  pub fn pattern(p: Pattern, a: Vec3, b: Vec3) -> Texture {
    let n = PATTERN_SIZE;
    let mut data = Vec::with_capacity((n * n) as usize);
    for y in 0..n {
      for x in 0..n {
        let u = (x as f32 + 0.5) / n as f32;
        let v = 1.0 - (y as f32 + 0.5) / n as f32;
        let t = pattern_value(p, u, v).clamp(0.0, 1.0);
        data.push(a.lerp(b, t).to_array());
      }
    }
    Texture { width: n, height: n, data }
  }

  /// The texture of a material (none if it has none, or its image can't be read).
  pub fn for_material(m: &Material) -> Option<Result<Texture, String>> {
    let spec = m.texture.as_ref()?;
    let base = srgb3(m.color);
    Some(match &spec.source {
      TextureSource::Image(path) => Texture::load(path, base),
      TextureSource::Pattern(p) => Ok(Texture::pattern(*p, base, srgb3(spec.color2))),
    })
  }
}

fn hash2(x: i32, y: i32, seed: u32) -> f32 {
  let mut h = (x as u32).wrapping_mul(0x8da6b343) ^ (y as u32).wrapping_mul(0xd8163841) ^ seed.wrapping_mul(0xcb1ab31f);
  h ^= h >> 13;
  h = h.wrapping_mul(0x5bd1e995);
  h ^= h >> 15;
  (h & 0xffffff) as f32 / 16_777_215.0
}

/// Value noise that repeats every `period` lattice cells.
fn periodic_noise(u: f32, v: f32, period: i32, seed: u32) -> f32 {
  noise2(u, v, period, period, seed)
}

/// Value noise repeating every `pu` cells across u and `pv` across v.
fn noise2(u: f32, v: f32, pu: i32, pv: i32, seed: u32) -> f32 {
  let (x, y) = (u * pu as f32, v * pv as f32);
  let (x0, y0) = (x.floor(), y.floor());
  let (fx, fy) = (x - x0, y - y0);
  let s = |t: f32| t * t * (3.0 - 2.0 * t);
  let (sx, sy) = (s(fx), s(fy));
  let at = |i: i32, j: i32| hash2(i.rem_euclid(pu), j.rem_euclid(pv), seed);
  let (i, j) = (x0 as i32, y0 as i32);
  let a = at(i, j) + (at(i + 1, j) - at(i, j)) * sx;
  let b = at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * sx;
  a + (b - a) * sy
}

fn fbm(u: f32, v: f32, base: i32, octaves: u32, seed: u32) -> f32 {
  let (mut sum, mut amp, mut norm) = (0.0, 0.5, 0.0);
  for o in 0..octaves {
    sum += periodic_noise(u, v, base << o, seed + o) * amp;
    norm += amp;
    amp *= 0.5;
  }
  sum / norm
}

/// A pattern's value (0-1) at (u, v) in its tile; every pattern tiles seamlessly.
pub fn pattern_value(p: Pattern, u: f32, v: f32) -> f32 {
  match p {
    Pattern::Checker => {
      let a = (u * 2.0).floor() as i32 + (v * 2.0).floor() as i32;
      (a & 1) as f32
    }
    Pattern::Wood => {
      // growth rings across v, wobbling along u, with fine grain
      let wobble = fbm(u, v, 2, 3, 11) * 2.5;
      let rings = ((v * 6.0 + wobble) * TAU).sin() * 0.5 + 0.5;
      let grain = noise2(u, v, 8, 128, 7);
      (rings.powf(3.0) * 0.8 + grain * 0.25).min(1.0)
    }
    Pattern::Brushed => {
      // long streaks along u
      let coarse = noise2(u, v, 2, 64, 3);
      let fine = noise2(u, v, 4, PATTERN_SIZE as i32, 9);
      (coarse * 0.45 + fine * 0.55).clamp(0.0, 1.0)
    }
    Pattern::Carbon => {
      // 2x2 twill: tows alternate direction, shaded across their width
      let (cu, cv) = (u * 4.0, v * 4.0);
      let (iu, iv) = (cu.floor() as i32, cv.floor() as i32);
      let along_u = (iu + iv).rem_euclid(4) < 2;
      let f = if along_u { cv.fract() } else { cu.fract() };
      let shade = (f * PI).sin();
      if along_u { 0.15 + 0.55 * shade } else { 0.45 + 0.55 * shade }
    }
    Pattern::Noise => fbm(u, v, 8, 4, 21),
    Pattern::Grid => {
      let d = |t: f32| {
        let f = (t * 4.0).fract();
        f.min(1.0 - f)
      };
      if d(u).min(d(v)) < 0.04 { 1.0 } else { 0.0 }
    }
    Pattern::Knurl => {
      let a = ((u + v) * 8.0 * PI).sin().abs();
      let b = ((u - v) * 8.0 * PI).sin().abs();
      1.0 - a.min(b)
    }
  }
}

/// Texture coordinates (in repeats) of point `p` (relative to the projection origin) with normal
/// `n`. Mirrored in the GPU shader's `proj_uv`.
pub fn project(spec: &TextureSpec, p: Vec3, n: Vec3) -> Vec2 {
  let scale = spec.scale.max(1e-3);
  let planar = |axis: usize| match axis {
    0 => Vec2::new(p.y, p.z),
    1 => Vec2::new(p.x, p.z),
    _ => Vec2::new(p.x, p.y),
  };
  let raw = match spec.projection {
    Projection::Box | Projection::Automatic => {
      let a = n.abs();
      let axis = if a.x >= a.y && a.x >= a.z {
        0
      } else if a.y >= a.z {
        1
      } else {
        2
      };
      planar(axis) / scale
    }
    Projection::Planar => planar(spec.axis.index()) / scale,
    Projection::Cylindrical => {
      let (q, h) = around(spec.axis, p);
      Vec2::new(q.y.atan2(q.x) / TAU + 0.5, h / scale)
    }
    Projection::Spherical => {
      let (q, h) = around(spec.axis, p);
      let r = p.length().max(1e-9);
      Vec2::new(q.y.atan2(q.x) / TAU + 0.5, (h / r).clamp(-1.0, 1.0).acos() / PI)
    }
  };
  let (s, c) = spec.rotation.to_radians().sin_cos();
  Vec2::new(raw.x * c - raw.y * s, raw.x * s + raw.y * c) + Vec2::from(spec.offset)
}

/// The point in the plane across `axis`, and its height along it.
fn around(axis: Axis, p: Vec3) -> (Vec2, f32) {
  match axis {
    Axis::X => (Vec2::new(p.y, p.z), p.x),
    Axis::Y => (Vec2::new(p.z, p.x), p.y),
    Axis::Z => (Vec2::new(p.x, p.y), p.z),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn patterns_tile_seamlessly() {
    for p in Pattern::ALL {
      if matches!(p, Pattern::Checker | Pattern::Grid | Pattern::Carbon) {
        continue; // hard edges by design
      }
      for k in 0..16 {
        let t = k as f32 / 16.0;
        let a = pattern_value(p, 1e-4, t);
        let b = pattern_value(p, 1.0 - 1e-4, t);
        assert!((a - b).abs() < 0.08, "{p:?} at v={t}: {a} vs {b}");
      }
    }
  }

  /// The modeling viewport's projection (`fox_render::TexProjection`) is this one.
  #[test]
  fn the_viewport_projects_textures_alike() {
    let p = Vec3::new(12.0, -7.0, 31.0);
    for projection in [Projection::Box, Projection::Planar, Projection::Cylindrical, Projection::Spherical] {
      for axis in [Axis::X, Axis::Y, Axis::Z] {
        let spec =
          TextureSpec { projection, axis, scale: 15.0, rotation: 30.0, offset: [0.25, -0.5], ..TextureSpec::default() };
        let view = fox_render::TexProjection {
          kind: projection.index(),
          axis: axis.index() as i32,
          scale: spec.scale,
          rotation: spec.rotation,
          offset: spec.offset,
          origin: [1.0, 2.0, 3.0],
        };
        let n = Vec3::new(0.3, -0.8, 0.5).normalize();
        let a = project(&spec, p - Vec3::new(1.0, 2.0, 3.0), n);
        let b = view.uv(p, n);
        assert!((a - b).length() < 1e-4, "{projection:?} {axis:?}: {a} vs {b}");
      }
    }
  }

  #[test]
  fn box_projection_follows_the_normal() {
    let spec = TextureSpec { scale: 10.0, ..TextureSpec::default() };
    let uv = project(&spec, Vec3::new(5.0, 20.0, 30.0), Vec3::Z);
    assert_eq!(uv, Vec2::new(0.5, 2.0));
    let uv = project(&spec, Vec3::new(5.0, 20.0, 30.0), Vec3::X);
    assert_eq!(uv, Vec2::new(2.0, 3.0));
  }
}
