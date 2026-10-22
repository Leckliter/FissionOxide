//! The Effects the renderers draw into the picture itself (Display Settings ▸ Effects), beside the
//! ground, its shadow and reflections that [`crate::Effects::apply`] adds as meshes:
//!
//! - **Environment Dome**: a sky over a ground behind the model, turning with the camera, in
//!   place of the background gradient ([`Dome`]).
//! - **Object Shadow**: the bodies shadow each other (and themselves) from a light high above,
//!   leaning as the ground shadow's does ([`ShadowMap`]).
//! - **Ambient Occlusion**: darkening where surfaces meet in a crease or a corner, found from the
//!   depth of the picture ([`ambient_occlusion`]).
//! - **Anti-Aliasing**: edges in the picture smoothed ([`anti_alias`]).
//!
//! The GPU renderer (`fox-render-gl`) does the same arithmetic in its shaders, with the constants
//! here, so the two paths look alike.

use glam::{DMat4, DVec3, DVec4, Vec3};

use crate::{Camera, Frame, Mesh};

/// The effects a scene is drawn with ([`crate::Scene::post`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Post {
  /// Environment Dome: drawn behind the model in place of the background gradient.
  pub dome: Option<Dome>,
  /// Object Shadow.
  pub object_shadow: bool,
  /// Ambient Occlusion.
  pub ambient_occlusion: bool,
  /// Anti-Aliasing.
  pub anti_aliasing: bool,
}

/// An environment's dome: its sky at the zenith and at the horizon, and its ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dome {
  pub zenith: [u8; 4],
  pub horizon: [u8; 4],
  pub ground: [u8; 4],
}

impl Dome {
  /// The dome's colour along unit direction `d` (world, Z up): the sky from the horizon up to the
  /// zenith, then below a soft band the ground.
  pub fn color(&self, d: Vec3) -> [u8; 4] {
    let e = d.z.clamp(-1.0, 1.0);
    if e >= 0.0 {
      lerp(self.horizon, self.zenith, e.sqrt())
    } else {
      lerp(self.horizon, self.ground, (-e * 6.0).min(1.0))
    }
  }

  /// The view direction through pixel (`x`, `y`) of a `w` × `h` picture: as a perspective
  /// camera with the camera's field of view sees it, whatever its projection, so the dome turns
  /// with the camera in orthographic views too.
  pub fn ray(cam: &Camera, w: u32, h: u32, x: f32, y: f32) -> Vec3 {
    let (right, up, back) = cam.basis();
    let t = (cam.fov / 2.0).tan() as f32;
    let aspect = w as f32 / h.max(1) as f32;
    let sx = (x / w.max(1) as f32) * 2.0 - 1.0;
    let sy = 1.0 - (y / h.max(1) as f32) * 2.0;
    (-back.as_vec3() + right.as_vec3() * (sx * t * aspect) + up.as_vec3() * (sy * t)).normalize_or_zero()
  }
}

fn lerp(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
  let mut o = [0u8; 4];
  for i in 0..4 {
    o[i] = (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round().clamp(0.0, 255.0) as u8;
  }
  o
}

/// How the shadows' light leans: a point at height `z` above the ground casts its shadow
/// `z · SHADOW_LEAN` away along X and Y (the ground shadow and Object Shadow alike).
pub const SHADOW_LEAN: [f32; 2] = [0.35, 0.25];
/// The shadow map's size (texels a side).
pub const SHADOW_SIZE: usize = 1024;
/// How much a shadowed surface darkens.
pub const SHADOW_DARKEN: f32 = 0.3;
/// The shadow map's depth bias.
pub const SHADOW_BIAS: f32 = 0.002;
/// Surfaces turned further from the light than this (cosine) take no object shadow: they are
/// already the ones the light misses, and would only show the map's grain.
pub const SHADOW_MIN_COS: f32 = 0.1;

/// The unit direction towards the shadows' light.
pub fn to_light() -> Vec3 {
  Vec3::new(-SHADOW_LEAN[0], -SHADOW_LEAN[1], 1.0).normalize()
}

/// Does a mesh cast and take object shadows? The opaque body surfaces.
pub fn casts_shadow(m: &Mesh) -> bool {
  m.analyze && m.color[3] == 255
}

/// The light's view of the bodies for the shadow map: an orthographic view along
/// [`to_light`] that holds their bounding sphere, world → normalized device coordinates. None
/// without bodies.
pub fn shadow_light(meshes: &[Mesh]) -> Option<DMat4> {
  let mut pts = meshes.iter().filter(|m| casts_shadow(m)).flat_map(|m| m.positions.iter());
  let first = DVec3::from(pts.next()?.map(f64::from));
  let (mut lo, mut hi) = (first, first);
  for p in pts {
    let p = DVec3::from(p.map(f64::from));
    lo = lo.min(p);
    hi = hi.max(p);
  }
  let c = (lo + hi) / 2.0;
  let r = ((hi - lo).length() / 2.0).max(1e-3);
  let eye = c + to_light().as_dvec3() * (2.0 * r);
  let view = DMat4::look_at_rh(eye, c, DVec3::Y);
  Some(DMat4::orthographic_rh_gl(-r, r, -r, r, 0.5 * r, 3.5 * r) * view)
}

/// The world length of a shadow map texel for the light's view `light` (see [`shadow_light`]).
pub fn shadow_texel(light: &DMat4) -> f32 {
  // the view's first row is a unit vector over its half width
  let row = DVec3::new(light.x_axis.x, light.y_axis.x, light.z_axis.x);
  (2.0 / row.length().max(1e-12) / SHADOW_SIZE as f64) as f32
}

/// The bodies' depth seen from the shadows' light (see [`shadow_light`]).
pub struct ShadowMap {
  light: DMat4,
  /// Depth (0 near the light – 1) of the nearest body surface per texel, rows bottom-up.
  depth: Vec<f32>,
  /// World length of a texel, for the offset that keeps a surface off its own shadow.
  pub texel: f32,
}

impl ShadowMap {
  /// Rasterizes the bodies' depth from the light.
  pub fn build(meshes: &[Mesh]) -> Option<ShadowMap> {
    let light = shadow_light(meshes)?;
    let n = SHADOW_SIZE;
    let mut depth = vec![1.0f32; n * n];
    let to_map = |p: &[f32; 3]| {
      let c = light * DVec4::new(p[0] as f64, p[1] as f64, p[2] as f64, 1.0);
      let q = c.truncate() / c.w;
      ((q.x * 0.5 + 0.5) * n as f64, (q.y * 0.5 + 0.5) * n as f64, (q.z * 0.5 + 0.5) as f32)
    };
    for m in meshes.iter().filter(|m| casts_shadow(m)) {
      for t in m.indices.chunks_exact(3) {
        let (Some(a), Some(b), Some(c)) =
          (m.positions.get(t[0] as usize), m.positions.get(t[1] as usize), m.positions.get(t[2] as usize))
        else {
          continue;
        };
        let v = [to_map(a), to_map(b), to_map(c)];
        let area = (v[1].0 - v[0].0) * (v[2].1 - v[0].1) - (v[2].0 - v[0].0) * (v[1].1 - v[0].1);
        if !area.is_finite() || area.abs() < 1e-12 {
          continue;
        }
        let x0 = v.iter().map(|p| p.0).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize;
        let x1 = (v.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max).ceil() as usize).min(n - 1);
        let y0 = v.iter().map(|p| p.1).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize;
        let y1 = (v.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max).ceil() as usize).min(n - 1);
        for y in y0..=y1 {
          for x in x0..=x1 {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let e = |a: (f64, f64, f32), b: (f64, f64, f32)| (b.0 - a.0) * (py - a.1) - (px - a.0) * (b.1 - a.1);
            let (b0, b1, b2) = (e(v[1], v[2]) / area, e(v[2], v[0]) / area, e(v[0], v[1]) / area);
            if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
              continue;
            }
            let z = (b0 * v[0].2 as f64 + b1 * v[1].2 as f64 + b2 * v[2].2 as f64) as f32;
            if let Some(d) = depth.get_mut(y * n + x)
              && z < *d
            {
              *d = z;
            }
          }
        }
      }
    }
    Some(ShadowMap { light, depth, texel: shadow_texel(&light) })
  }

  /// How far (0–1) point `p` is in shadow: the share of the 3 × 3 texels round it whose surface
  /// is nearer the light.
  pub fn occlusion(&self, p: Vec3) -> f32 {
    let n = SHADOW_SIZE;
    let c = self.light * DVec4::new(p.x as f64, p.y as f64, p.z as f64, 1.0);
    let q = c.truncate() / c.w;
    let (u, v, z) = (q.x * 0.5 + 0.5, q.y * 0.5 + 0.5, (q.z * 0.5 + 0.5) as f32);
    if u <= 0.0 || u >= 1.0 || v <= 0.0 || v >= 1.0 || z >= 1.0 {
      return 0.0;
    }
    let (cx, cy) = ((u * n as f64).floor() as i64, (v * n as f64).floor() as i64);
    let mut hit = 0;
    for dy in -1..=1 {
      for dx in -1..=1 {
        let (x, y) = ((cx + dx).clamp(0, n as i64 - 1) as usize, (cy + dy).clamp(0, n as i64 - 1) as usize);
        if self.depth.get(y * n + x).is_some_and(|d| z - SHADOW_BIAS > *d) {
          hit += 1;
        }
      }
    }
    hit as f32 / 9.0
  }

  /// A body surface's colour `c` at `p` with unit normal `n` (turned to the eye), shadowed.
  pub fn shade(&self, c: Vec3, p: Vec3, n: Vec3) -> Vec3 {
    if n.dot(to_light()) <= SHADOW_MIN_COS {
      return c;
    }
    c * (1.0 - SHADOW_DARKEN * self.occlusion(p + n * (self.texel * 1.5)))
  }
}

/// How dark ambient occlusion gets at most.
pub const AO_STRENGTH: f32 = 0.5;
/// The sampling radii (pixels).
pub const AO_RADII: [i32; 2] = [3, 8];

/// Ambient Occlusion over the surfaces drawn so far: each pixel darkens by how far it sits
/// behind the mean depth of pairs of pixels round it (a crease or a corner seen from inside); a
/// flat or convex surface stays as it is. Depth jumps of more than a few pixels' worth (one
/// surface far behind another) count for nothing.
pub fn ambient_occlusion(f: &mut Frame, cam: &Camera) {
  let (w, h) = (f.width as i64, f.height as i64);
  let lin = linear_depth(f, cam);
  let per_px = (cam.view_height() / cam.distance / h.max(1) as f64) as f32;
  let px_world = |d: f32| if cam.ortho { per_px * cam.distance as f32 } else { per_px * d };
  let at = |x: i64, y: i64| if x < 0 || y < 0 || x >= w || y >= h { None } else { lin[(y * w + x) as usize] };
  for y in 0..h {
    for x in 0..w {
      let Some(d) = at(x, y) else { continue };
      let ao = ao_at(d, px_world(d), |dx, dy| at(x + dx, y + dy));
      if ao < 1.0
        && let Some(p) = f.rgba.get_mut(((y * w + x) * 4) as usize..((y * w + x) * 4 + 3) as usize)
      {
        for c in p {
          *c = (*c as f32 * ao).round() as u8;
        }
      }
    }
  }
}

/// The ambient occlusion factor (1 = none) of a pixel at linear depth `d`, a pixel `pxw` world
/// units wide there, with `near(dx, dy)` the linear depth of the pixel that far off.
pub fn ao_at(d: f32, pxw: f32, near: impl Fn(i64, i64) -> Option<f32>) -> f32 {
  let mut occ = 0.0;
  let mut pairs = 0.0;
  for r in AO_RADII {
    let k = ((r as f32) * std::f32::consts::FRAC_1_SQRT_2).round() as i64;
    let r = r as i64;
    for (dx, dy) in [(r, 0), (0, r), (k, k), (k, -k)] {
      pairs += 1.0;
      let (Some(a), Some(b)) = (near(dx, dy), near(-dx, -dy)) else { continue };
      let scale = r as f32 * pxw;
      let diff = d - 0.5 * (a + b);
      if diff > 0.02 * scale && diff < 4.0 * scale {
        occ += (diff / scale).min(1.0);
      }
    }
  }
  1.0 - AO_STRENGTH * occ / pairs
}

/// Each pixel's distance in front of the eye along the view (None where nothing is drawn).
fn linear_depth(f: &Frame, cam: &Camera) -> Vec<Option<f32>> {
  let (w, h) = (f.width, f.height);
  let inv = cam.view_proj(w, h).inverse();
  let eye = cam.eye();
  let fwd = -cam.basis().2;
  f.depth
    .iter()
    .enumerate()
    .map(|(i, z)| {
      if !z.is_finite() {
        return None;
      }
      let (x, y) = ((i as u32 % w) as f64 + 0.5, (i as u32 / w) as f64 + 0.5);
      let ndc = DVec4::new(x / w as f64 * 2.0 - 1.0, 1.0 - y / h as f64 * 2.0, *z as f64, 1.0);
      let p = inv * ndc;
      let p = p.truncate() / p.w;
      Some((p - eye).dot(fwd) as f32)
    })
    .collect()
}

/// How much brighter or darker (0–255 luma) a neighbour must be for a pixel to count as on an
/// edge.
pub const AA_EDGE: f32 = 24.0;

/// Anti-Aliasing: each pixel on an edge in the picture (its four neighbours' luma spread over
/// [`AA_EDGE`]) takes half its colour from them, which softens the stair steps.
pub fn anti_alias(f: &mut Frame) {
  let (w, h) = (f.width as usize, f.height as usize);
  if w < 3 || h < 3 {
    return;
  }
  let src = f.rgba.clone();
  let luma = |i: usize| 0.299 * src[i * 4] as f32 + 0.587 * src[i * 4 + 1] as f32 + 0.114 * src[i * 4 + 2] as f32;
  for y in 1..h - 1 {
    for x in 1..w - 1 {
      let i = y * w + x;
      let around = [i - w, i + w, i - 1, i + 1];
      let l = around.map(luma);
      let c = luma(i);
      let lo = l.iter().fold(c, |a, b| a.min(*b));
      let hi = l.iter().fold(c, |a, b| a.max(*b));
      if hi - lo <= AA_EDGE {
        continue;
      }
      for k in 0..3 {
        let sum: f32 = around.iter().map(|j| src[j * 4 + k] as f32).sum();
        f.rgba[i * 4 + k] = (0.5 * src[i * 4 + k] as f32 + 0.125 * sum).round() as u8;
      }
    }
  }
}
