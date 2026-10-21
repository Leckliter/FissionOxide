//! Mechanical and thermal properties of the physical materials.
//!
//! The names match `fox_model::assembly::MATERIALS` (the components' physical materials), so a
//! component's material gives its simulation properties too. Values are typical room-temperature
//! figures (as in Fusion's material library), stored in the units people look them up in;
//! [`Material::conductivity_mm`] and friends convert to the solver's mm–N–tonne–s–W units.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
  pub name: String,
  /// Young's modulus (MPa).
  pub youngs: f64,
  /// Poisson's ratio.
  pub poisson: f64,
  /// Density (g/cm³).
  pub density: f64,
  /// Yield strength (MPa).
  pub yield_strength: f64,
  /// Ultimate tensile strength (MPa).
  pub tensile_strength: f64,
  /// Thermal conductivity (W/(m·K)).
  pub conductivity: f64,
  /// Thermal expansion coefficient (1/K).
  pub expansion: f64,
  /// Specific heat (J/(kg·K)).
  pub specific_heat: f64,
}

/// (name, E MPa, ν, ρ g/cm³, yield MPa, ultimate MPa, k W/(m·K), α 1/K, c J/(kg·K)).
const LIBRARY: &[(&str, f64, f64, f64, f64, f64, f64, f64, f64)] = &[
  ("Steel", 210_000.0, 0.30, 7.85, 207.0, 345.0, 50.0, 1.2e-5, 480.0),
  ("Stainless Steel", 193_000.0, 0.29, 8.0, 215.0, 505.0, 16.2, 1.73e-5, 500.0),
  ("Aluminum", 68_900.0, 0.33, 2.70, 275.0, 310.0, 167.0, 2.36e-5, 896.0),
  ("Brass", 100_000.0, 0.31, 8.50, 124.0, 338.0, 115.0, 2.05e-5, 380.0),
  ("Copper", 117_000.0, 0.34, 8.96, 69.0, 220.0, 391.0, 1.7e-5, 385.0),
  ("Titanium", 114_000.0, 0.34, 4.43, 880.0, 950.0, 6.7, 8.6e-6, 526.0),
  ("Cast Iron", 110_000.0, 0.26, 7.20, 150.0, 200.0, 46.0, 1.1e-5, 460.0),
  ("ABS Plastic", 2_240.0, 0.38, 1.04, 40.0, 44.0, 0.17, 8.5e-5, 1400.0),
  ("PLA", 3_500.0, 0.36, 1.24, 60.0, 65.0, 0.13, 6.8e-5, 1800.0),
  ("Nylon", 2_800.0, 0.39, 1.15, 60.0, 75.0, 0.25, 8.0e-5, 1700.0),
  ("Polycarbonate", 2_400.0, 0.37, 1.20, 62.0, 69.0, 0.20, 6.8e-5, 1200.0),
  ("Acrylic", 3_200.0, 0.37, 1.18, 70.0, 72.0, 0.19, 7.0e-5, 1470.0),
  ("Wood", 11_000.0, 0.30, 0.70, 40.0, 50.0, 0.12, 5.0e-6, 1700.0),
  ("Rubber", 5.0, 0.49, 1.10, 15.0, 15.0, 0.16, 7.7e-4, 2000.0),
];

impl Material {
  /// A library material by name (case-insensitive); unknown or empty names are steel, as for mass.
  pub fn named(name: &str) -> Material {
    let r = LIBRARY.iter().find(|r| r.0.eq_ignore_ascii_case(name.trim())).unwrap_or(&LIBRARY[0]);
    Material {
      name: r.0.to_string(),
      youngs: r.1,
      poisson: r.2,
      density: r.3,
      yield_strength: r.4,
      tensile_strength: r.5,
      conductivity: r.6,
      expansion: r.7,
      specific_heat: r.8,
    }
  }

  /// Every library material.
  pub fn library() -> Vec<Material> {
    LIBRARY.iter().map(|r| Material::named(r.0)).collect()
  }

  /// Steel.
  pub fn steel() -> Material {
    Material::named("Steel")
  }

  /// Density in tonne/mm³ (the mass unit that goes with N, mm and s).
  pub fn density_t_mm3(&self) -> f64 {
    self.density * 1e-9
  }

  /// Conductivity in W/(mm·K).
  pub fn conductivity_mm(&self) -> f64 {
    self.conductivity * 1e-3
  }

  /// Lamé's first parameter and the shear modulus (MPa).
  pub fn lame(&self) -> (f64, f64) {
    let (e, nu) = (self.youngs, self.poisson.clamp(-0.99, 0.4999));
    (e * nu / ((1.0 + nu) * (1.0 - 2.0 * nu)), e / (2.0 * (1.0 + nu)))
  }

  /// Checks the properties make sense.
  pub fn validate(&self) -> Result<(), String> {
    if !(self.youngs > 0.0) {
      return Err(format!("{}: Young's modulus must be positive", self.name));
    }
    if !(self.poisson > -1.0 && self.poisson < 0.5) {
      return Err(format!("{}: Poisson's ratio must be between -1 and 0.5", self.name));
    }
    if !(self.density >= 0.0) || !(self.conductivity >= 0.0) || !(self.yield_strength >= 0.0) {
      return Err(format!("{}: density, conductivity and yield strength can't be negative", self.name));
    }
    Ok(())
  }
}

impl Default for Material {
  fn default() -> Self {
    Material::steel()
  }
}
