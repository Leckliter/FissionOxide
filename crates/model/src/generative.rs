//! Generative design studies (Fusion's Generative Design workspace): the design space (preserve
//! geometry, obstacles, a starting shape), load cases, objectives and limits, materials and
//! manufacturing methods, and the outcomes the solver ([`fox_gen`]) found, one per material and
//! method.

use std::sync::Arc;

use fox_gen::{Grid, Material, Method, SurfaceMesh};
use serde::{Deserialize, Serialize};

use crate::BodyId;
use crate::refs::FaceRef;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Study {
  pub id: u32,
  pub name: String,
  /// Bodies the outcome must keep (where loads and constraints go, bolt bosses ...).
  #[serde(default)]
  pub preserve: Vec<BodyId>,
  /// Bodies the outcome must stay out of (bolt holes, clearance for other parts).
  #[serde(default)]
  pub obstacles: Vec<BodyId>,
  /// A body the optimisation starts from (and whose bounds the design space includes).
  #[serde(default)]
  pub starting_shape: Option<BodyId>,
  /// Extra design space around the preserve geometry and starting shape, mm.
  #[serde(default)]
  pub margin: f64,
  /// Explicit design space bounds (min, max), instead of those of the bodies.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub bounds: Option<([f64; 3], [f64; 3])>,
  #[serde(default)]
  pub cases: Vec<LoadCase>,
  #[serde(default)]
  pub objective: Goal,
  /// Max stiffness: the fraction of the design space that may be solid.
  #[serde(default = "mass_target")]
  pub mass_target: f64,
  #[serde(default = "safety")]
  pub safety_factor: f64,
  /// mm.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub max_displacement: Option<f64>,
  #[serde(default)]
  pub materials: Vec<Material>,
  #[serde(default)]
  pub methods: Vec<Method>,
  /// Symmetry planes: (axis 0..2, offset of the plane along it, mm).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub symmetry: Vec<(usize, f64)>,
  /// Smallest member thickness, mm (0 = about three elements).
  #[serde(default)]
  pub min_member: f64,
  /// Element size, mm (0 = from `elements`).
  #[serde(default)]
  pub resolution: f64,
  /// About how many elements the grid has when no resolution is set.
  #[serde(default = "elements")]
  pub elements: usize,
  #[serde(default = "iterations")]
  pub iterations: usize,
  #[serde(default)]
  pub outcomes: Vec<Outcome>,
  /// Last outcome id handed out.
  #[serde(default)]
  pub next_outcome: u32,
}

fn mass_target() -> f64 {
  0.3
}
fn safety() -> f64 {
  2.0
}
fn elements() -> usize {
  24_000
}
fn iterations() -> usize {
  60
}

impl Study {
  pub fn new(id: u32, name: impl Into<String>) -> Study {
    Study {
      id,
      name: name.into(),
      preserve: Vec::new(),
      obstacles: Vec::new(),
      starting_shape: None,
      margin: 0.0,
      bounds: None,
      cases: vec![LoadCase { name: "Load Case 1".into(), loads: Vec::new(), constraints: Vec::new() }],
      objective: Goal::MaxStiffness,
      mass_target: mass_target(),
      safety_factor: safety(),
      max_displacement: None,
      materials: Vec::new(),
      methods: Vec::new(),
      symmetry: Vec::new(),
      min_member: 0.0,
      resolution: 0.0,
      elements: elements(),
      iterations: iterations(),
      outcomes: Vec::new(),
      next_outcome: 0,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Goal {
  /// Stiffest shape at the mass target.
  #[default]
  MaxStiffness,
  /// Lightest shape that meets the safety factor (and displacement limit).
  MinMass,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadCase {
  pub name: String,
  #[serde(default)]
  pub loads: Vec<Load>,
  #[serde(default)]
  pub constraints: Vec<Constraint>,
}

/// Where a load or constraint acts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Target {
  /// A face (of a preserve body, usually).
  Face { face: FaceRef },
  /// The nodes within `radius` of a point.
  Point { at: [f64; 3], radius: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoadKind {
  /// A total force, N.
  Force { force: [f64; 3] },
  /// A pressure on a face, MPa, pushing into it.
  Pressure { pressure: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Load {
  pub target: Target,
  #[serde(flatten)]
  pub kind: LoadKind,
}

/// Fixed displacement (x, y, z) at a target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
  pub target: Target,
  pub fix: [bool; 3],
}

/// One solution: a material and a manufacturing method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
  pub id: u32,
  pub material: Material,
  pub method: Method,
  pub mass: f64,
  pub volume: f64,
  pub volume_fraction: f64,
  pub compliance: f64,
  pub max_displacement: f64,
  pub peak_stress: f64,
  pub safety_factor: f64,
  pub meets_limits: bool,
  pub converged: bool,
  pub iterations: usize,
  pub grid: Grid,
  /// Element densities (not saved; the surface is).
  #[serde(skip)]
  pub density: Arc<Vec<f32>>,
  pub mesh: Arc<SurfaceMesh>,
}
