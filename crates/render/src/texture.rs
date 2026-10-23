//! Textures laid over meshes by projection: an appearance's grain or image drawn on a body in
//! the modeling viewport. The projection maths is `fox_raytrace::texture::project`'s, so the
//! viewport's grain lines up with the Render workspace's preview and the final render.

use std::f32::consts::{PI, TAU};
use std::sync::Arc;

use glam::{Vec2, Vec3};

/// A tiling image in sRGB RGBA8, top row first.
#[derive(Debug, PartialEq)]
pub struct Tile {
  /// What the tile was made from: equal keys hold equal texels (the GPU keeps one copy per key).
  pub key: String,
  pub width: u32,
  pub height: u32,
  pub rgba: Vec<u8>,
}

impl Tile {
  /// Bilinear lookup (0-1 per channel), repeating outside [0, 1).
  pub fn sample(&self, uv: Vec2) -> Vec3 {
    let (w, h) = (self.width.max(1) as i64, self.height.max(1) as i64);
    if self.rgba.len() < (w * h * 4) as usize {
      return Vec3::ONE;
    }
    let x = uv.x.rem_euclid(1.0) * w as f32 - 0.5;
    let y = (1.0 - uv.y.rem_euclid(1.0)) * h as f32 - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |i: i64, j: i64| -> Vec3 {
      let k = ((j.rem_euclid(h) * w + i.rem_euclid(w)) * 4) as usize;
      Vec3::new(self.rgba[k] as f32, self.rgba[k + 1] as f32, self.rgba[k + 2] as f32) / 255.0
    };
    let (i, j) = (x0 as i64, y0 as i64);
    let top = at(i, j).lerp(at(i + 1, j), fx);
    let bottom = at(i, j + 1).lerp(at(i + 1, j + 1), fx);
    top.lerp(bottom, fy)
  }
}

/// How a texture's coordinates follow from a surface point and its normal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TexProjection {
  /// 0 box (each point mapped across the axis its normal is nearest), 1 planar, 2 cylindrical,
  /// 3 spherical: the GPU shaders' `u_proj.x`.
  pub kind: i32,
  /// Axis of planar, cylindrical and spherical projections (0 X, 1 Y, 2 Z).
  pub axis: i32,
  /// One repeat of the texture (mm).
  pub scale: f32,
  /// Turn of the texture on the surface (degrees).
  pub rotation: f32,
  /// Shift, in repeats.
  pub offset: [f32; 2],
  /// Where the projection is centred (world).
  pub origin: [f32; 3],
}

impl Default for TexProjection {
  fn default() -> Self {
    TexProjection { kind: 0, axis: 2, scale: 20.0, rotation: 0.0, offset: [0.0; 2], origin: [0.0; 3] }
  }
}

impl TexProjection {
  /// Texture coordinates (in repeats) of world point `p` with normal `n`.
  pub fn uv(&self, p: Vec3, n: Vec3) -> Vec2 {
    let p = p - Vec3::from(self.origin);
    let scale = self.scale.max(1e-3);
    let planar = |axis: i32| match axis {
      0 => Vec2::new(p.y, p.z),
      1 => Vec2::new(p.x, p.z),
      _ => Vec2::new(p.x, p.y),
    };
    let raw = match self.kind {
      0 => {
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
      1 => planar(self.axis) / scale,
      _ => {
        let (q, h) = match self.axis {
          0 => (Vec2::new(p.y, p.z), p.x),
          1 => (Vec2::new(p.z, p.x), p.y),
          _ => (Vec2::new(p.x, p.y), p.z),
        };
        let u = q.y.atan2(q.x) / TAU + 0.5;
        if self.kind == 2 {
          Vec2::new(u, h / scale)
        } else {
          Vec2::new(u, (h / p.length().max(1e-9)).clamp(-1.0, 1.0).acos() / PI)
        }
      }
    };
    let (s, c) = self.rotation.to_radians().sin_cos();
    Vec2::new(raw.x * c - raw.y * s, raw.x * s + raw.y * c) + Vec2::from(self.offset)
  }
}

/// A texture over a mesh: where it lies, its texels stand in for the mesh's colour (still lit).
#[derive(Debug, Clone, PartialEq)]
pub struct MeshTexture {
  pub tile: Arc<Tile>,
  pub projection: TexProjection,
}

impl MeshTexture {
  /// The texture's colour (0-1) at world point `p` with normal `n`.
  pub fn color_at(&self, p: Vec3, n: Vec3) -> Vec3 {
    self.tile.sample(self.projection.uv(p, n))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn checker() -> Tile {
    // 2 x 2: white, black / black, white
    let px = [255u8, 0, 0, 255];
    let rgba = [px[0], px[1], px[1], px[0]].iter().flat_map(|v| [*v, *v, *v, 255]).collect();
    Tile { key: "checker".into(), width: 2, height: 2, rgba }
  }

  #[test]
  fn box_projection_follows_the_normal_and_the_origin() {
    let p = TexProjection { scale: 10.0, origin: [1.0, 2.0, 3.0], ..TexProjection::default() };
    assert_eq!(p.uv(Vec3::new(6.0, 22.0, 33.0), Vec3::Z), Vec2::new(0.5, 2.0));
    assert_eq!(p.uv(Vec3::new(6.0, 22.0, 33.0), Vec3::X), Vec2::new(2.0, 3.0));
  }

  #[test]
  fn tiles_repeat() {
    let t = checker();
    let a = t.sample(Vec2::new(0.25, 0.75));
    let b = t.sample(Vec2::new(1.25, -0.25));
    assert!((a - b).length() < 1e-5);
    assert!(a.x > 0.9, "{a}");
    assert!(t.sample(Vec2::new(0.75, 0.75)).x < 0.1);
  }
}
