//! Generative design: density-based topology optimisation.
//!
//! A [`Problem`] is a voxel [`Grid`] whose elements are free to optimise, kept solid (preserve
//! geometry) or kept empty (obstacles and the space outside), one or more load cases (forces on
//! nodes and fixed degrees of freedom), a [`Material`], an [`Objective`] with its [`Limits`] and
//! the manufacturing constraints. [`solve`] runs SIMP (solid isotropic material with
//! penalisation): a density per element, Young's modulus `E = Emin + rho^p (E0 - Emin)`, a cone
//! density filter whose radius sets the minimum member size, Heaviside projection towards a crisp
//! 0/1 design with a rising sharpness, milling (no undercuts from the tool directions) and
//! symmetry constraints, and optimality-criteria updates. The finite element solve is
//! matrix-free over identical 8-node bricks, threaded, with preconditioned conjugate gradients.
//!
//! The outcome's iso-surface at density 0.5 ([`surface`]) is a closed, smoothed triangle mesh
//! that the engine inserts as a mesh body (and converts to B-rep).
//!
//! Units are millimetres, newtons, MPa and g/cm³.

// index loops read best in the finite element and grid code
#![allow(clippy::needless_range_loop)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

pub mod fe;
pub mod grid;
mod mg;
mod opt;
pub mod surface;
#[cfg(test)]
mod tests;

pub use grid::{Grid, V3};
pub use opt::solve;
pub use surface::SurfaceMesh;

/// An engineering material for the outcomes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
  pub name: String,
  /// Young's modulus, MPa.
  pub youngs: f64,
  pub poisson: f64,
  /// g/cm³.
  pub density: f64,
  /// Yield strength, MPa.
  #[serde(rename = "yield")]
  pub yield_strength: f64,
}

impl Material {
  fn new(name: &str, youngs: f64, poisson: f64, density: f64, yield_strength: f64) -> Material {
    Material { name: name.into(), youngs, poisson, density, yield_strength }
  }
}

/// Fusion's common generative design materials.
pub fn materials() -> Vec<Material> {
  vec![
    Material::new("Aluminum AlSi10Mg", 71_000.0, 0.33, 2.67, 230.0),
    Material::new("Aluminum 6061", 68_900.0, 0.33, 2.70, 275.0),
    Material::new("Steel AISI 1020", 205_000.0, 0.29, 7.87, 350.0),
    Material::new("Stainless Steel 316L", 193_000.0, 0.30, 8.00, 290.0),
    Material::new("Titanium 6Al-4V", 113_800.0, 0.34, 4.43, 880.0),
    Material::new("ABS Plastic", 2_240.0, 0.38, 1.06, 40.0),
    Material::new("Nylon 12", 1_700.0, 0.39, 1.01, 48.0),
    Material::new("PLA", 3_500.0, 0.36, 1.24, 50.0),
  ]
}

/// A library material by name (case and spacing don't matter; a unique prefix will do).
pub fn material(name: &str) -> Option<Material> {
  let key = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
  let k = key(name);
  let all = materials();
  if let Some(m) = all.iter().find(|m| key(&m.name) == k) {
    return Some(m.clone());
  }
  // a unique prefix, else a unique part of the name
  for hit in [|n: &str, k: &str| n.starts_with(k), |n: &str, k: &str| n.contains(k)] {
    let found: Vec<&Material> = all.iter().filter(|m| hit(&key(&m.name), &k)).collect();
    if found.len() == 1 {
      return Some(found[0].clone());
    }
    if found.len() > 1 {
      return None;
    }
  }
  None
}

/// What an element is to the optimiser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cell {
  /// Free to be solid or empty.
  Design,
  /// Always solid (preserve geometry).
  Preserve,
  /// Always empty (obstacles, outside the design space).
  Void,
}

/// One load case: forces on nodes (N) and fixed degrees of freedom.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LoadCase {
  pub name: String,
  pub forces: Vec<(usize, V3)>,
  /// (node, fixed x, fixed y, fixed z).
  pub fixed: Vec<(usize, [bool; 3])>,
}

/// What the optimisation drives at.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Objective {
  /// Maximise stiffness (minimise compliance) at a mass target: the fraction of the design
  /// space's volume that may be solid.
  MaxStiffness { volume_fraction: f64 },
  /// Minimise mass while the limits hold: the volume fraction steps down from `start` (warm
  /// started each time) and the lightest design that still meets the limits is the outcome.
  MinMass { start: f64 },
}

/// Limits the outcome must meet.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Limits {
  /// Yield strength over peak von Mises stress.
  pub safety_factor: f64,
  /// Largest displacement anywhere, mm.
  pub max_displacement: Option<f64>,
}

impl Default for Limits {
  fn default() -> Self {
    Limits { safety_factor: 2.0, max_displacement: None }
  }
}

/// An axis direction, `+x` .. `-z`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dir {
  pub axis: usize,
  pub positive: bool,
}

impl Dir {
  pub fn parse(s: &str) -> Option<Dir> {
    let s = s.trim().to_ascii_lowercase();
    let (positive, rest) = match s.strip_prefix('-') {
      Some(r) => (false, r),
      None => (true, s.strip_prefix('+').unwrap_or(&s)),
    };
    let axis = match rest {
      "x" => 0,
      "y" => 1,
      "z" => 2,
      _ => return None,
    };
    Some(Dir { axis, positive })
  }

  pub fn label(&self) -> String {
    format!("{}{}", if self.positive { "+" } else { "-" }, ["x", "y", "z"][self.axis])
  }
}

/// A manufacturing method the outcome has to suit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Method {
  /// Any shape (additive manufacturing).
  Unrestricted,
  /// 3-axis milling: every empty cell is reachable along one of the tool directions (the
  /// direction the tool comes from), so there are no undercuts.
  Milling3 { directions: Vec<Dir> },
  /// 2.5-axis milling: prismatic, every column along the tool axis all solid or all empty.
  Milling25 { direction: Dir },
}

impl Method {
  pub fn label(&self) -> String {
    match self {
      Method::Unrestricted => "Unrestricted".into(),
      Method::Milling3 { directions } => {
        format!("3-axis milling ({})", directions.iter().map(Dir::label).collect::<Vec<_>>().join(", "))
      }
      Method::Milling25 { direction } => format!("2.5-axis milling ({})", direction.label()),
    }
  }
}

/// Solver settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Settings {
  /// SIMP penalty.
  pub penalty: f64,
  /// Density filter radius, mm (the minimum member size is about twice it).
  pub filter_radius: f64,
  pub max_iterations: usize,
  /// Largest change of a density in one update.
  pub move_limit: f64,
  /// Heaviside projection sharpness at the end (1 = no projection).
  pub beta_max: f64,
  /// Threads for the finite element solve (0 = all cores).
  pub threads: usize,
}

impl Default for Settings {
  fn default() -> Self {
    Settings { penalty: 3.0, filter_radius: 0.0, max_iterations: 80, move_limit: 0.2, beta_max: 8.0, threads: 0 }
  }
}

/// A topology optimisation problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Problem {
  pub grid: Grid,
  pub cells: Vec<Cell>,
  /// Initial densities (a starting shape), else uniform at the volume target.
  pub start: Option<Vec<f32>>,
  pub cases: Vec<LoadCase>,
  pub material: Material,
  pub objective: Objective,
  pub limits: Limits,
  pub method: Method,
  /// Mirror symmetry about the grid's middle plane normal to x, y, z.
  pub symmetry: [bool; 3],
  pub settings: Settings,
}

/// What [`solve`] found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
  /// Physical density per element.
  pub density: Vec<f32>,
  /// Solid fraction of the design space.
  pub volume_fraction: f64,
  /// Solid volume, mm³ (elements over 0.5).
  pub volume: f64,
  /// Mass, g.
  pub mass: f64,
  /// Compliance (work of the loads, N mm) summed over the load cases, of the crisp design.
  pub compliance: f64,
  pub max_displacement: f64,
  /// Peak von Mises stress (MPa), leaving out the most stressed 0.5 % of the solid elements
  /// (the singular corners of a voxel model).
  pub peak_stress: f64,
  pub safety_factor: f64,
  pub meets_limits: bool,
  pub iterations: usize,
  pub converged: bool,
  /// Compliance (of the grey design being optimised) and volume fraction per iteration.
  pub history: Vec<(f64, f64)>,
}

/// Progress of a running [`solve`], shared with another thread; setting `cancel` stops it at
/// the next iteration with the design so far.
#[derive(Debug, Default)]
pub struct Progress {
  pub iteration: AtomicUsize,
  pub total: AtomicUsize,
  pub cancel: AtomicBool,
}

impl Progress {
  pub fn cancelled(&self) -> bool {
    self.cancel.load(Ordering::Relaxed)
  }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenError(pub String);

impl std::fmt::Display for GenError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.0)
  }
}

impl std::error::Error for GenError {}
