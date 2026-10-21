//! Random numbers and sampling helpers. Every sample's generator is seeded from its pixel and
//! sample index alone, so a render comes out the same whatever the thread count or tile order.

use glam::Vec3;
use std::f32::consts::PI;

/// PCG32.
#[derive(Debug, Clone)]
pub struct Rng {
  state: u64,
}

const MUL: u64 = 6364136223846793005;
const INC: u64 = 1442695040888963407;

fn mix64(mut z: u64) -> u64 {
  z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
  z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
  z ^ (z >> 31)
}

impl Rng {
  pub fn new(seed: u64) -> Rng {
    let mut r = Rng { state: mix64(seed).wrapping_add(INC) };
    r.next_u32();
    r
  }

  /// The generator of one sample of one pixel.
  pub fn for_sample(x: u32, y: u32, sample: u32, seed: u32) -> Rng {
    Rng::new(((x as u64) << 40) ^ ((y as u64) << 16) ^ ((sample as u64) << 1).rotate_left(23) ^ ((seed as u64) << 52))
  }

  pub fn next_u32(&mut self) -> u32 {
    let old = self.state;
    self.state = old.wrapping_mul(MUL).wrapping_add(INC);
    let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
    let rot = (old >> 59) as u32;
    xorshifted.rotate_right(rot)
  }

  /// Uniform in [0, 1).
  pub fn f32(&mut self) -> f32 {
    (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
  }

  pub fn pair(&mut self) -> (f32, f32) {
    (self.f32(), self.f32())
  }
}

/// An orthonormal basis around `n` (Duff et al.).
pub fn basis(n: Vec3) -> (Vec3, Vec3) {
  let s = if n.z >= 0.0 { 1.0 } else { -1.0 };
  let a = -1.0 / (s + n.z);
  let b = n.x * n.y * a;
  (Vec3::new(1.0 + s * n.x * n.x * a, s * b, -s * n.x), Vec3::new(b, s + n.y * n.y * a, -n.y))
}

/// Cosine-weighted direction about +Z.
pub fn cosine_hemisphere(u1: f32, u2: f32) -> Vec3 {
  let r = u1.sqrt();
  let phi = 2.0 * PI * u2;
  Vec3::new(r * phi.cos(), r * phi.sin(), (1.0 - u1).max(0.0).sqrt())
}

/// A point on the unit disc.
pub fn concentric_disc(u1: f32, u2: f32) -> (f32, f32) {
  let (a, b) = (2.0 * u1 - 1.0, 2.0 * u2 - 1.0);
  if a == 0.0 && b == 0.0 {
    return (0.0, 0.0);
  }
  let (r, phi) = if a.abs() > b.abs() { (a, PI / 4.0 * (b / a)) } else { (b, PI / 2.0 - PI / 4.0 * (a / b)) };
  (r * phi.cos(), r * phi.sin())
}

/// MIS power heuristic.
pub fn power_heuristic(a: f32, b: f32) -> f32 {
  let (a2, b2) = (a * a, b * b);
  if a2 + b2 <= 0.0 { 0.0 } else { a2 / (a2 + b2) }
}

/// sRGB (0-255) to linear.
pub fn srgb_to_linear(c: u8) -> f32 {
  let c = c as f32 / 255.0;
  if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

pub fn srgb3(c: [u8; 3]) -> Vec3 {
  Vec3::new(srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]))
}

/// Linear to sRGB (0-1).
pub fn linear_to_srgb(c: f32) -> f32 {
  let c = c.clamp(0.0, 1.0);
  if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

pub fn luminance(c: Vec3) -> f32 {
  c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}
