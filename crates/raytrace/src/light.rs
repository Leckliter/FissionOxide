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
}

impl Default for Light {
  fn default() -> Self {
    Light { name: "Light1".into(), position: [0.0; 3], intensity: DEFAULT_LUMENS, color: [255, 255, 255], on: true }
  }
}

impl Light {
  /// Radiant intensity in the tracer's units: irradiance at `r` mm is this / r². The
  /// environment's own light (intensity 1) is 1000 lx, an irradiance of π; a flux of Φ lm gives
  /// Φ / 4π cd, or Φ / 4π · 10⁶ / r² lx at r mm.
  pub fn radiant(&self) -> Vec3 {
    let candela = self.intensity.max(0.0) / (4.0 * std::f32::consts::PI);
    srgb3(self.color) * candela * 1.0e6 * std::f32::consts::PI / 1000.0
  }
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
}
