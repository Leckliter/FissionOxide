//! Photometric lights placed in the scene (Fusion's NEW LIGHT): small point emitters with a
//! luminous flux in lumens and a colour, lighting the model beside the environment.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::sampling::srgb3;

/// Most lights a design takes, as Fusion.
pub const MAX_LIGHTS: usize = 8;

/// A new light's flux (lm).
pub const DEFAULT_LUMENS: f32 = 100.0;

/// A point light.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Light {
  pub name: String,
  /// Where it is (mm).
  pub position: [f32; 3],
  /// Luminous flux (lm), spread evenly in every direction.
  pub intensity: f32,
  /// Its colour (sRGB).
  pub color: [u8; 3],
  /// Switched off lights stay in the design but don't light it.
  pub on: bool,
  /// Which way it points (degrees, see [`Light::aim`]); a uniform light shines the same every way.
  pub rotation: [f32; 3],
  /// Color Mode Kelvin: its colour is a black body's at this temperature (K) instead of `color`.
  pub kelvin: Option<f32>,
}

impl Default for Light {
  fn default() -> Self {
    Light {
      name: "Light1".into(),
      position: [0.0; 3],
      intensity: DEFAULT_LUMENS,
      color: [255, 255, 255],
      on: true,
      rotation: [0.0; 3],
      kelvin: None,
    }
  }
}

impl Light {
  /// Radiant intensity in the tracer's units: irradiance at `r` mm is this / r². The
  /// environment's own light (intensity 1) is 1000 lx, an irradiance of π; a flux of Φ lm gives
  /// Φ / 4π cd, or Φ / 4π · 10⁶ / r² lx at r mm.
  pub fn radiant(&self) -> Vec3 {
    let candela = self.intensity.max(0.0) / (4.0 * std::f32::consts::PI);
    srgb3(self.shade()) * candela * 1.0e6 * std::f32::consts::PI / 1000.0
  }

  /// Its colour: the black body's of its temperature in Color Mode Kelvin, else `color`.
  pub fn shade(&self) -> [u8; 3] {
    self.kelvin.map(kelvin_rgb).unwrap_or(self.color)
  }

  /// Its intensity in candela (a uniform light's flux spread over the sphere).
  pub fn candela(&self) -> f32 {
    self.intensity / (4.0 * std::f32::consts::PI)
  }

  /// The flux (lm) of a uniform light of `cd` candela.
  pub fn lumens_of(cd: f32) -> f32 {
    cd * 4.0 * std::f32::consts::PI
  }

  /// The way it points: turned by its Z angle about +Z from +X, tilted up by its Y angle (its X
  /// angle turns it about that direction).
  pub fn aim(&self) -> Vec3 {
    let [_, y, z] = self.rotation.map(f32::to_radians);
    Vec3::new(y.cos() * z.cos(), y.cos() * z.sin(), y.sin())
  }

  /// The rotation that aims along `d`.
  pub fn rotation_for(d: Vec3) -> [f32; 3] {
    let d = d.normalize_or_zero();
    if d == Vec3::ZERO {
      return [0.0; 3];
    }
    // to a ten-thousandth of a degree, so straight down reads -90
    let tidy = |a: f32| (a * 1e4).round() / 1e4;
    let y = tidy(d.z.clamp(-1.0, 1.0).asin().to_degrees());
    let z = if d.x.abs() + d.y.abs() < 1e-6 { 0.0 } else { tidy(d.y.atan2(d.x).to_degrees()) };
    [0.0, y, z]
  }
}

/// A black body's colour at `k` kelvin, as sRGB (Tanner Helland's fit, 1000 K to 40000 K).
pub fn kelvin_rgb(k: f32) -> [u8; 3] {
  let t = k.clamp(1000.0, 40000.0) / 100.0;
  let r = if t <= 66.0 { 255.0 } else { 329.698_73 * (t - 60.0).powf(-0.133_204_76) };
  let g = if t <= 66.0 { 99.470_8 * t.ln() - 161.119_57 } else { 288.122_16 * (t - 60.0).powf(-0.075_514_85) };
  let b = if t >= 66.0 {
    255.0
  } else if t <= 19.0 {
    0.0
  } else {
    138.517_73 * (t - 10.0).ln() - 305.044_8
  };
  [r, g, b].map(|c: f32| c.clamp(0.0, 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_light_gives_the_environments_illuminance_at_the_right_distance() {
    // 1000 lx (the environment's light, an irradiance of π) comes from Φ / 4π · 10⁶ / r² = 1000
    let l = Light { intensity: 4.0 * std::f32::consts::PI * 1000.0, ..Light::default() };
    let r = 1000.0f32;
    let e = l.radiant() / (r * r);
    assert!((e.x - std::f32::consts::PI).abs() < 1e-3, "{e}");
  }

  #[test]
  fn kelvin_colours_and_aims() {
    assert_eq!(kelvin_rgb(6600.0), [255, 255, 255]);
    let warm = kelvin_rgb(3500.0);
    assert!(warm[0] == 255 && warm[1] < 220 && warm[2] < warm[1], "{warm:?}");
    let l = Light { kelvin: Some(3500.0), ..Light::default() };
    assert_eq!(l.shade(), warm);
    assert!((Light::lumens_of(1500.0) / (4.0 * std::f32::consts::PI) - 1500.0).abs() < 1e-2);
    // pointing down is -90 about Y
    assert_eq!(Light::rotation_for(Vec3::NEG_Z), [0.0, -90.0, 0.0]);
    let down = Light { rotation: [0.0, -90.0, 0.0], ..Light::default() };
    assert!((down.aim() - Vec3::NEG_Z).length() < 1e-6);
    let r = Light::rotation_for(Vec3::new(0.0, 1.0, 1.0));
    assert!((Light { rotation: r, ..Light::default() }.aim() - Vec3::new(0.0, 1.0, 1.0).normalize()).length() < 1e-5);
  }
}
