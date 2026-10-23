//! Environments: image-based lighting from an HDRI or a built-in preset, the background, and a
//! ground plane that catches shadows and reflections.
//!
//! Every environment becomes an equirectangular radiance map ([`EnvMap`]) with +Z up. The path
//! tracer importance-samples it; the GPU viewport uploads it as a mipmapped texture for
//! reflections and takes its diffuse light from the map's spherical harmonics.

use std::f32::consts::{PI, TAU};

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::sampling::srgb3;

/// The environment settings stored with a design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Environment {
  pub preset: Preset,
  /// An HDRI (`.hdr`, or any image) lighting the scene instead of the preset.
  pub hdri: Option<String>,
  /// Brightness of the environment's light.
  pub intensity: f32,
  /// Turn of the environment about the vertical (degrees).
  pub rotation: f32,
  pub background: Background,
  /// Colour of a solid background, and the top of a gradient one (sRGB).
  pub background_color: [u8; 3],
  /// Bottom of a gradient background.
  pub background_color2: [u8; 3],
  /// A ground plane under the model that catches shadows (and reflections).
  pub ground: bool,
  pub shadows: bool,
  /// How mirror-like the ground is (0 = matte).
  pub ground_reflection: f32,
  /// Gap between the model's lowest point and the ground (mm).
  pub ground_offset: f32,
  /// The environment's lower half projected onto the ground plane (Fusion's Flatten Ground), so
  /// the model stands on the environment's own ground instead of floating in a sphere.
  pub flatten_ground: bool,
}

impl Default for Environment {
  fn default() -> Self {
    Environment {
      preset: Preset::Studio,
      hdri: None,
      intensity: 1.0,
      rotation: 0.0,
      background: Background::Environment,
      background_color: [250, 250, 251],
      background_color2: [214, 218, 224],
      ground: true,
      shadows: true,
      ground_reflection: 0.0,
      ground_offset: 0.0,
      flatten_ground: false,
    }
  }
}

/// The grey of a new design's Solid Color background (Fusion's).
pub const NEW_DESIGN_GREY: [u8; 3] = [204, 204, 204];

impl Environment {
  /// What a new design's scene starts with, as Fusion's: the Photo Booth light over a Solid
  /// Color grey background. [`Environment::default`] (the environment as the background) is what
  /// designs saved before kept, and what their files leave out.
  pub fn new_design() -> Environment {
    Environment { background: Background::Color, background_color: NEW_DESIGN_GREY, ..Environment::default() }
  }
}

/// What's seen behind the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Background {
  /// The environment itself.
  #[default]
  Environment,
  Color,
  Gradient,
  /// Alpha 0 in the PNG.
  Transparent,
}

impl Background {
  pub fn parse(s: &str) -> Option<Background> {
    Some(match s.to_ascii_lowercase().as_str() {
      "environment" | "env" | "hdri" => Background::Environment,
      "color" | "colour" | "solid" => Background::Color,
      "gradient" => Background::Gradient,
      "transparent" | "none" => Background::Transparent,
      _ => return None,
    })
  }
}

/// Built-in lighting environments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
  /// Grey studio with soft boxes (Fusion's Photo Booth).
  #[default]
  Studio,
  /// Even, overcast light.
  Soft,
  /// Blue sky with the sun high.
  Sky,
  /// Low warm sun.
  Sunset,
  /// Dark room with rim lights.
  Dark,
  /// Uniform white (a clay look).
  White,
}

impl Preset {
  pub const ALL: [Preset; 6] = [Preset::Studio, Preset::Soft, Preset::Sky, Preset::Sunset, Preset::Dark, Preset::White];

  pub fn name(self) -> &'static str {
    match self {
      Preset::Studio => "studio",
      Preset::Soft => "soft",
      Preset::Sky => "sky",
      Preset::Sunset => "sunset",
      Preset::Dark => "dark",
      Preset::White => "white",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Preset::Studio => "Photo Booth",
      Preset::Soft => "Soft Light",
      Preset::Sky => "Sharp Highlights (Sky)",
      Preset::Sunset => "Warm Sunset",
      Preset::Dark => "Dark Sky",
      Preset::White => "Plain White",
    }
  }

  pub fn parse(s: &str) -> Option<Preset> {
    Preset::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(s) || p.label().eq_ignore_ascii_case(s))
  }
}

/// An equirectangular radiance map (linear RGB, +Z up, rows from the zenith down), with what's
/// needed to importance-sample it.
#[derive(Debug, Clone)]
pub struct EnvMap {
  pub width: u32,
  pub height: u32,
  pub data: Vec<Vec3>,
  /// Turn about +Z (radians).
  pub rotation: f32,
  /// Row CDFs (per row, normalised) and the marginal CDF over rows.
  cond: Vec<f32>,
  marg: Vec<f32>,
  /// Integral of the sampling weights (for the pdf).
  total: f32,
}

/// Size of a preset map.
const PRESET_W: u32 = 512;

impl EnvMap {
  pub fn new(width: u32, height: u32, data: Vec<Vec3>, rotation: f32) -> EnvMap {
    let (w, h) = (width as usize, height as usize);
    let mut cond = vec![0.0; w * h];
    let mut row_sums = vec![0.0; h];
    for y in 0..h {
      let sin_t = ((y as f32 + 0.5) / h as f32 * PI).sin();
      let mut acc = 0.0;
      for x in 0..w {
        let c = data[y * w + x];
        acc += (c.x + c.y + c.z).max(0.0) / 3.0 * sin_t + 1e-6;
        cond[y * w + x] = acc;
      }
      for x in 0..w {
        cond[y * w + x] /= acc;
      }
      row_sums[y] = acc;
    }
    let mut marg = vec![0.0; h];
    let mut acc = 0.0;
    for y in 0..h {
      acc += row_sums[y];
      marg[y] = acc;
    }
    for m in &mut marg {
      *m /= acc;
    }
    EnvMap { width, height, data, rotation, cond, marg, total: acc }
  }

  /// The map of an environment: its HDRI, else its preset, scaled by its intensity.
  pub fn build(env: &Environment) -> Result<EnvMap, String> {
    let (w, h, mut data) = match &env.hdri {
      Some(path) => load_hdri(path)?,
      None => preset_map(env.preset),
    };
    let k = env.intensity.max(0.0);
    for c in &mut data {
      *c *= k;
    }
    Ok(EnvMap::new(w, h, data, env.rotation.to_radians()))
  }

  fn dir_to_uv(&self, d: Vec3) -> (f32, f32) {
    let phi = (d.y.atan2(d.x) - self.rotation).rem_euclid(TAU);
    let theta = d.z.clamp(-1.0, 1.0).acos();
    (phi / TAU, theta / PI)
  }

  fn uv_to_dir(&self, u: f32, v: f32) -> Vec3 {
    let phi = u * TAU + self.rotation;
    let theta = v * PI;
    let s = theta.sin();
    Vec3::new(s * phi.cos(), s * phi.sin(), theta.cos())
  }

  fn texel(&self, x: usize, y: usize) -> Vec3 {
    self.data[y.min(self.height as usize - 1) * self.width as usize + x % self.width as usize]
  }

  /// Radiance arriving from direction `d` (bilinear).
  pub fn lookup(&self, d: Vec3) -> Vec3 {
    let (u, v) = self.dir_to_uv(d);
    let x = u * self.width as f32 - 0.5;
    let y = (v * self.height as f32 - 0.5).max(0.0);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let xi = x0.rem_euclid(self.width as f32) as usize;
    let yi = y0 as usize;
    let a = self.texel(xi, yi).lerp(self.texel(xi + 1, yi), fx);
    let b = self.texel(xi, yi + 1).lerp(self.texel(xi + 1, yi + 1), fx);
    a.lerp(b, fy)
  }

  /// What's seen along `d` from `o` with the lower half flattened onto the ground at height
  /// `ground_z`: below the horizon the ray meets the ground, and the map is looked up from a
  /// point `height` above `centre` (the environment's camera) toward that spot.
  pub fn lookup_flat(&self, o: Vec3, d: Vec3, ground_z: f32, centre: Vec3, height: f32) -> Vec3 {
    self.lookup(flat_dir(o, d, ground_z, centre, height))
  }

  /// Draws a direction proportionally to the map's brightness: (direction, radiance, pdf per
  /// solid angle).
  pub fn sample(&self, u1: f32, u2: f32) -> (Vec3, Vec3, f32) {
    let (w, h) = (self.width as usize, self.height as usize);
    let y = self.marg.partition_point(|c| *c < u1).min(h - 1);
    let row = &self.cond[y * w..(y + 1) * w];
    let x = row.partition_point(|c| *c < u2).min(w - 1);
    let d = self.uv_to_dir((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
    let pdf = self.pdf_texel(x, y);
    (d, self.texel(x, y), pdf)
  }

  fn pdf_texel(&self, x: usize, y: usize) -> f32 {
    let (w, h) = (self.width as usize, self.height as usize);
    let c = self.texel(x, y);
    let sin_t = ((y as f32 + 0.5) / h as f32 * PI).sin();
    let weight = (c.x + c.y + c.z).max(0.0) / 3.0 * sin_t + 1e-6;
    // probability of the texel, over its solid angle (2π/w · π/h · sinθ)
    let p = weight / self.total;
    if sin_t <= 1e-6 { 0.0 } else { p * (w * h) as f32 / (2.0 * PI * PI * sin_t) }
  }

  /// The pdf [`EnvMap::sample`] gives direction `d`.
  pub fn pdf(&self, d: Vec3) -> f32 {
    let (u, v) = self.dir_to_uv(d);
    let x = ((u * self.width as f32) as usize).min(self.width as usize - 1);
    let y = ((v * self.height as f32) as usize).min(self.height as usize - 1);
    self.pdf_texel(x, y)
  }

  /// Irradiance as 9 spherical-harmonic coefficients (RGB), for diffuse light on the GPU.
  /// Evaluate with [`sh_irradiance`].
  pub fn sh9(&self) -> [Vec3; 9] {
    let (w, h) = (self.width as usize, self.height as usize);
    let mut sh = [Vec3::ZERO; 9];
    let step = (w / 128).max(1);
    let mut weight = 0.0;
    for y in (0..h).step_by(step) {
      for x in (0..w).step_by(step) {
        let v = (y as f32 + 0.5) / h as f32;
        let u = (x as f32 + 0.5) / w as f32;
        let d = self.uv_to_dir(u, v);
        let dw = (v * PI).sin();
        let c = self.texel(x, y) * dw;
        let b = sh_basis(d);
        for k in 0..9 {
          sh[k] += c * b[k];
        }
        weight += dw;
      }
    }
    let norm = 4.0 * PI / weight;
    // convolve with the clamped cosine (Ramamoorthi & Hanrahan)
    let band = [PI, 2.0 * PI / 3.0, 2.0 * PI / 3.0, 2.0 * PI / 3.0, PI / 4.0, PI / 4.0, PI / 4.0, PI / 4.0, PI / 4.0];
    for k in 0..9 {
      sh[k] *= norm * band[k] / PI;
    }
    sh
  }

  /// A downsampled copy (box filter) at most `max_w` wide, for the GPU.
  pub fn downsampled(&self, max_w: u32) -> (u32, u32, Vec<Vec3>) {
    let f = (self.width / max_w.max(1)).max(1) as usize;
    let (w, h) = (self.width as usize / f, self.height as usize / f);
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
      for x in 0..w {
        let mut acc = Vec3::ZERO;
        for j in 0..f {
          for i in 0..f {
            acc += self.texel(x * f + i, y * f + j);
          }
        }
        out.push(acc / (f * f) as f32);
      }
    }
    (w as u32, h as u32, out)
  }

  /// The brightest direction (the key light, for the GPU's shadows).
  pub fn key_direction(&self) -> Vec3 {
    let (w, h) = (self.width as usize, self.height as usize);
    let mut best = (0.0, Vec3::Z);
    // blur by looking at blocks, so one hot pixel doesn't win over a broad light
    let b = (w / 32).max(1);
    for by in (0..h / 2).step_by(b) {
      for bx in (0..w).step_by(b) {
        let mut acc = 0.0;
        for y in by..(by + b).min(h) {
          for x in bx..(bx + b).min(w) {
            let c = self.texel(x, y);
            acc += c.x + c.y + c.z;
          }
        }
        if acc > best.0 {
          let d = self.uv_to_dir((bx as f32 + b as f32 / 2.0) / w as f32, (by as f32 + b as f32 / 2.0) / h as f32);
          best = (acc, d);
        }
      }
    }
    best.1.normalize_or_zero()
  }
}

/// The direction [`EnvMap::lookup_flat`] looks up. Mirrored in the GPU background shader.
pub fn flat_dir(o: Vec3, d: Vec3, ground_z: f32, centre: Vec3, height: f32) -> Vec3 {
  if d.z >= -1e-6 || o.z <= ground_z {
    return d;
  }
  let p = o + d * ((ground_z - o.z) / d.z);
  let eye = Vec3::new(centre.x, centre.y, ground_z + height.max(1e-3));
  let v = (p - eye).normalize_or_zero();
  if v == Vec3::ZERO { d } else { v }
}

fn sh_basis(d: Vec3) -> [f32; 9] {
  [
    0.282095,
    0.488603 * d.y,
    0.488603 * d.z,
    0.488603 * d.x,
    1.092548 * d.x * d.y,
    1.092548 * d.y * d.z,
    0.315392 * (3.0 * d.z * d.z - 1.0),
    1.092548 * d.x * d.z,
    0.546274 * (d.x * d.x - d.y * d.y),
  ]
}

/// Irradiance / π (the diffuse light for an albedo of 1) from [`EnvMap::sh9`] coefficients.
/// Mirrored in the GPU shader.
pub fn sh_irradiance(sh: &[Vec3; 9], n: Vec3) -> Vec3 {
  let b = sh_basis(n);
  let mut c = Vec3::ZERO;
  for k in 0..9 {
    c += sh[k] * b[k];
  }
  c.max(Vec3::ZERO)
}

fn load_hdri(path: &str) -> Result<(u32, u32, Vec<Vec3>), String> {
  let img = image::ImageReader::open(path)
    .and_then(|r| r.with_guessed_format())
    .map_err(|e| format!("{path}: {e}"))?
    .decode()
    .map_err(|e| format!("{path}: {e}"))?;
  let img = if img.width() > 4096 { img.resize(4096, 2048, image::imageops::FilterType::Triangle) } else { img };
  let hdr = matches!(img, image::DynamicImage::ImageRgb32F(_) | image::DynamicImage::ImageRgba32F(_));
  let rgb = img.to_rgb32f();
  let data = rgb
    .pixels()
    .map(|p| {
      let c = Vec3::from(p.0);
      // an 8-bit image is sRGB-encoded
      if hdr { c } else { c.powf(2.2) }
    })
    .collect();
  Ok((rgb.width(), rgb.height(), data))
}

/// A soft rectangular light: brightness falling off from its centre direction.
struct Softbox {
  dir: Vec3,
  /// Half-size (radians) of its square, and its edge softness.
  size: f32,
  color: Vec3,
}

fn softbox(az: f32, el: f32, size: f32, color: Vec3) -> Softbox {
  let (az, el) = (az.to_radians(), el.to_radians());
  Softbox { dir: Vec3::new(el.cos() * az.cos(), el.cos() * az.sin(), el.sin()), size: size.to_radians(), color }
}

impl Softbox {
  fn at(&self, d: Vec3) -> Vec3 {
    let (t, b) = crate::sampling::basis(self.dir);
    let z = d.dot(self.dir);
    if z <= 0.0 {
      return Vec3::ZERO;
    }
    let (x, y) = ((d.dot(t) / z).atan().abs(), (d.dot(b) / z).atan().abs());
    let edge = |a: f32| 1.0 - smoothstep(self.size * 0.8, self.size * 1.1, a);
    self.color * edge(x) * edge(y)
  }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
  let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
  t * t * (3.0 - 2.0 * t)
}

/// Builds a preset's map.
pub fn preset_map(p: Preset) -> (u32, u32, Vec<Vec3>) {
  let (w, h) = (PRESET_W, PRESET_W / 2);
  let lin = |c: [u8; 3]| srgb3(c);
  let boxes: Vec<Softbox> = match p {
    Preset::Studio => vec![
      softbox(-120.0, 35.0, 18.0, Vec3::splat(9.0)),
      softbox(30.0, 25.0, 14.0, Vec3::splat(4.0)),
      softbox(100.0, 70.0, 20.0, Vec3::splat(5.0)),
    ],
    Preset::Dark => vec![softbox(-150.0, 20.0, 10.0, Vec3::splat(12.0)), softbox(150.0, 20.0, 10.0, Vec3::splat(8.0))],
    Preset::Soft => vec![softbox(0.0, 90.0, 40.0, Vec3::splat(1.5))],
    // Fusion's Warm Light: a warm studio, a broad amber key low on one side and a soft fill
    Preset::Sunset => {
      vec![softbox(-100.0, 30.0, 22.0, Vec3::new(9.0, 6.0, 3.4)), softbox(60.0, 45.0, 26.0, Vec3::new(2.4, 2.0, 1.5))]
    }
    _ => Vec::new(),
  };
  let (sun, sun_color, sun_size) = match p {
    Preset::Sky => (Some((-60f32, 45f32)), Vec3::new(1.0, 0.95, 0.88) * 900.0, 1.5f32),
    _ => (None, Vec3::ZERO, 0.0),
  };
  // the studios' floor: a pool of light under the lights, fading out toward the horizon, so the
  // ground reads as a floor (and Flatten Ground lays the pool under the model)
  let pool: Option<Vec3> = match p {
    Preset::Studio => Some(lin([164, 164, 166])),
    Preset::Soft => Some(lin([200, 200, 200])),
    Preset::Sunset => Some(lin([214, 178, 138])),
    Preset::Dark => Some(lin([46, 46, 50])),
    _ => None,
  };
  let sun_dir = sun.map(|(az, el)| {
    let (az, el) = (f32::to_radians(az), f32::to_radians(el));
    Vec3::new(el.cos() * az.cos(), el.cos() * az.sin(), el.sin())
  });
  let mut data = Vec::with_capacity((w * h) as usize);
  for y in 0..h {
    for x in 0..w {
      let u = (x as f32 + 0.5) / w as f32;
      let v = (y as f32 + 0.5) / h as f32;
      let (phi, theta) = (u * TAU, v * PI);
      let d = Vec3::new(theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos());
      let up = d.z;
      let mut c = match p {
        Preset::Studio => {
          if up >= 0.0 {
            lin([150, 152, 156]).lerp(lin([96, 98, 102]), up.powf(0.6))
          } else {
            lin([120, 120, 122]).lerp(lin([60, 60, 62]), (-up).powf(0.5))
          }
        }
        Preset::Soft => {
          if up >= 0.0 {
            lin([228, 230, 234]).lerp(lin([250, 250, 252]), up)
          } else {
            lin([160, 160, 160])
          }
        }
        Preset::Sky => {
          if up >= 0.0 {
            lin([200, 220, 240]).lerp(lin([70, 120, 200]), up.powf(0.5)) * 1.4
          } else {
            lin([110, 104, 96]).lerp(lin([70, 66, 60]), (-up).powf(0.5))
          }
        }
        Preset::Sunset => {
          if up >= 0.0 {
            lin([246, 196, 140]).lerp(lin([222, 204, 184]), up.powf(0.5))
          } else {
            lin([150, 120, 96]).lerp(lin([112, 86, 66]), (-up).powf(0.5))
          }
        }
        Preset::Dark => {
          if up >= 0.0 {
            lin([40, 42, 48]).lerp(lin([12, 12, 16]), up.powf(0.5))
          } else {
            lin([20, 20, 22])
          }
        }
        Preset::White => Vec3::ONE,
      };
      if let Some(pool) = pool
        && up < 0.0
      {
        // brightest straight down, gone by 45° below the horizon
        let t = smoothstep(0.7, 0.97, -up);
        c = c.lerp(pool, t);
      }
      for b in &boxes {
        c += b.at(d);
      }
      if let Some(s) = sun_dir {
        let a = d.dot(s).clamp(-1.0, 1.0).acos().to_degrees();
        c += sun_color * (1.0 - smoothstep(sun_size * 0.6, sun_size, a));
        // glow around the sun
        c += sun_color * 0.002 * (-a / 12.0).exp();
      }
      data.push(c);
    }
  }
  (w, h, data)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sampling_pdf_matches_lookup() {
    let env = EnvMap::build(&Environment { preset: Preset::Sky, ..Environment::default() }).unwrap();
    // the sun direction is drawn far more often than the sky opposite it
    let mut hits = 0;
    for i in 0..2000 {
      let (d, _, pdf) = env.sample((i as f32 + 0.5) / 2000.0, ((i * 7919) % 2000) as f32 / 2000.0);
      assert!(pdf > 0.0);
      assert!((env.pdf(d) - pdf).abs() <= pdf * 1e-3 + 1e-6);
      if d.z > 0.5 {
        hits += 1;
      }
    }
    assert!(hits > 300, "{hits}");
  }

  #[test]
  fn a_new_design_has_a_grey_solid_background() {
    let e = Environment::new_design();
    assert_eq!((e.background, e.background_color, e.preset), (Background::Color, NEW_DESIGN_GREY, Preset::Studio));
    assert!(!e.flatten_ground);
  }

  #[test]
  fn flattening_bends_only_the_lower_half() {
    let o = Vec3::new(0.0, -100.0, 50.0);
    let up = Vec3::new(0.0, 1.0, 0.2).normalize();
    assert_eq!(flat_dir(o, up, 0.0, Vec3::ZERO, 40.0), up);
    // a ray reaching the ground right under the environment's camera looks straight down
    let d = (Vec3::new(0.0, 0.0, 0.0) - o).normalize();
    let f = flat_dir(o, d, 0.0, Vec3::ZERO, 40.0);
    assert!((f - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-5, "{f}");
    // far away on the ground it nears the horizon
    let far = Vec3::new(0.0, 1.0, -0.01).normalize();
    assert!(flat_dir(o, far, 0.0, Vec3::ZERO, 40.0).z > -0.02);
  }

  /// The studios have a floor: a pool of light straight down, darker toward the horizon, so
  /// Flatten Ground has something to lay under the model. Warm Light's is warm and light, not
  /// the near-black of a sunset's ground.
  #[test]
  fn studio_floors_have_a_pool_of_light_under_the_model() {
    for p in [Preset::Studio, Preset::Soft, Preset::Sunset] {
      let env = EnvMap::build(&Environment { preset: p, ..Environment::default() }).unwrap();
      let down = env.lookup(Vec3::new(0.0, 0.0, -1.0));
      let near = env.lookup(Vec3::new(1.0, 0.0, -0.2).normalize());
      assert!(down.length() > near.length() * 1.2, "{p:?}: {down} straight down, {near} near the horizon");
    }
    let warm = EnvMap::build(&Environment { preset: Preset::Sunset, ..Environment::default() }).unwrap();
    let floor = warm.lookup(Vec3::new(1.0, 0.0, -0.3).normalize());
    assert!(floor.x > 0.15 && floor.x > floor.z, "{floor}");
    // seen from a camera over the model, the flattened ground under it shows the pool
    let o = Vec3::new(0.0, -100.0, 50.0);
    let d = (Vec3::new(0.0, 5.0, 0.0) - o).normalize();
    let flat = warm.lookup_flat(o, d, 0.0, Vec3::ZERO, 30.0);
    assert!(flat.length() > warm.lookup(d).length() * 1.2, "{flat} vs {}", warm.lookup(d));
  }

  #[test]
  fn white_environment_lights_a_white_surface_to_one() {
    let env = EnvMap::build(&Environment { preset: Preset::White, ..Environment::default() }).unwrap();
    let sh = env.sh9();
    let e = sh_irradiance(&sh, Vec3::Z);
    assert!((e.x - 1.0).abs() < 0.03, "{e}");
  }
}
