//! Simulation studies (the Simulation workspace): what is simulated and how — the bodies, their
//! materials, constraints, loads, contacts and mesh settings. Faces are [`FaceRef`]s, found again
//! when the study is solved, so studies follow edits to the design. Solving is `fox-sim`'s job;
//! results aren't saved.
//!
//! Values are in the units people type: N, MPa, N·mm, mm/s², °C, W, W/m² and W/(m²·K).

use serde::{Deserialize, Serialize};

use crate::{BodyId, FaceRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudyKind {
  /// Linear static stress.
  #[default]
  Static,
  /// Modal frequencies.
  Modal,
  /// Steady-state heat transfer.
  Thermal,
}

impl StudyKind {
  pub fn id(self) -> &'static str {
    match self {
      StudyKind::Static => "static",
      StudyKind::Modal => "modal",
      StudyKind::Thermal => "thermal",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      StudyKind::Static => "Static Stress",
      StudyKind::Modal => "Modal Frequencies",
      StudyKind::Thermal => "Thermal",
    }
  }

  pub fn parse(s: &str) -> Option<StudyKind> {
    Some(match s.trim().to_ascii_lowercase().replace([' ', '-'], "_").as_str() {
      "static" | "static_stress" | "stress" => StudyKind::Static,
      "modal" | "modal_frequencies" | "frequencies" | "frequency" => StudyKind::Modal,
      "thermal" | "heat" | "heat_transfer" => StudyKind::Thermal,
      _ => return None,
    })
  }
}

fn yes() -> bool {
  true
}

fn is_true(b: &bool) -> bool {
  *b
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Constraint {
  Fixed {
    faces: Vec<FaceRef>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    x: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    y: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    z: bool,
  },
  Pin {
    faces: Vec<FaceRef>,
    #[serde(default = "yes")]
    radial: bool,
    #[serde(default = "yes")]
    axial: bool,
    #[serde(default)]
    tangential: bool,
  },
  Frictionless {
    faces: Vec<FaceRef>,
  },
}

impl Constraint {
  pub fn faces(&self) -> &[FaceRef] {
    match self {
      Constraint::Fixed { faces, .. } | Constraint::Pin { faces, .. } | Constraint::Frictionless { faces } => faces,
    }
  }

  pub fn label(&self) -> &'static str {
    match self {
      Constraint::Fixed { .. } => "Fixed",
      Constraint::Pin { .. } => "Pin",
      Constraint::Frictionless { .. } => "Frictionless",
    }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Load {
  /// Total force (N), spread over the faces.
  Force { faces: Vec<FaceRef>, force: [f64; 3] },
  /// Pressure (MPa) into the faces.
  Pressure { faces: Vec<FaceRef>, pressure: f64 },
  /// Bearing load (N) on cylindrical faces.
  Bearing { faces: Vec<FaceRef>, force: [f64; 3] },
  /// Moment (N·mm) about the faces' centroid.
  Moment { faces: Vec<FaceRef>, moment: [f64; 3] },
  /// Gravity (mm/s²) on every body.
  Gravity { acceleration: [f64; 3] },
  /// Faces held at a temperature (°C).
  Temperature { faces: Vec<FaceRef>, temperature: f64 },
  /// Heat flux into the faces (W/m²).
  HeatFlux { faces: Vec<FaceRef>, flux: f64 },
  /// Total heat into the faces (W).
  HeatPower { faces: Vec<FaceRef>, power: f64 },
  /// Convection (W/(m²·K)) to a fluid at `ambient` °C.
  Convection { faces: Vec<FaceRef>, coefficient: f64, ambient: f64 },
  /// Heat generated inside a body (W).
  InternalHeat { body: BodyId, power: f64 },
}

impl Load {
  pub fn faces(&self) -> &[FaceRef] {
    match self {
      Load::Force { faces, .. }
      | Load::Pressure { faces, .. }
      | Load::Bearing { faces, .. }
      | Load::Moment { faces, .. }
      | Load::Temperature { faces, .. }
      | Load::HeatFlux { faces, .. }
      | Load::HeatPower { faces, .. }
      | Load::Convection { faces, .. } => faces,
      Load::Gravity { .. } | Load::InternalHeat { .. } => &[],
    }
  }

  /// A thermal load (else structural).
  pub fn thermal(&self) -> bool {
    matches!(
      self,
      Load::Temperature { .. }
        | Load::HeatFlux { .. }
        | Load::HeatPower { .. }
        | Load::Convection { .. }
        | Load::InternalHeat { .. }
    )
  }

  pub fn label(&self) -> &'static str {
    match self {
      Load::Force { .. } => "Force",
      Load::Pressure { .. } => "Pressure",
      Load::Bearing { .. } => "Bearing Load",
      Load::Moment { .. } => "Moment",
      Load::Gravity { .. } => "Gravity",
      Load::Temperature { .. } => "Temperature",
      Load::HeatFlux { .. } => "Heat Flux",
      Load::HeatPower { .. } => "Heat Source",
      Load::Convection { .. } => "Convection",
      Load::InternalHeat { .. } => "Internal Heat",
    }
  }
}

/// A study's own material for a body, over its component's physical material; unset values come
/// from the named material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialOverride {
  pub body: BodyId,
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub material: String,
  /// Young's modulus (MPa).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub youngs: Option<f64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub poisson: Option<f64>,
  /// g/cm³.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub density: Option<f64>,
  /// Yield strength (MPa).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub yield_strength: Option<f64>,
  /// W/(m·K).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub conductivity: Option<f64>,
}

impl MaterialOverride {
  /// Nothing overridden.
  pub fn none(body: BodyId) -> MaterialOverride {
    MaterialOverride {
      body,
      material: String::new(),
      youngs: None,
      poisson: None,
      density: None,
      yield_strength: None,
      conductivity: None,
    }
  }
}

/// A local mesh size on faces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshRefinement {
  pub faces: Vec<FaceRef>,
  /// mm.
  pub size: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MeshSettings {
  /// Element size (mm); `None` = automatic, from each body's size.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub size: Option<f64>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub refine: Vec<MeshRefinement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactMode {
  /// Bond every pair of touching bodies (Fusion's automatic contacts).
  #[default]
  Automatic,
  /// Only the listed bonded pairs.
  Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Study {
  pub id: u32,
  pub name: String,
  pub kind: StudyKind,
  /// The bodies simulated (empty = every visible solid body).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub bodies: Vec<BodyId>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub materials: Vec<MaterialOverride>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub constraints: Vec<Constraint>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub loads: Vec<Load>,
  #[serde(default)]
  pub contacts: ContactMode,
  /// Bonded pairs (manual contacts).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub bonded: Vec<(BodyId, BodyId)>,
  #[serde(default)]
  pub mesh: MeshSettings,
  /// Modes to find (modal).
  #[serde(default = "modes")]
  pub modes: u32,
}

fn modes() -> u32 {
  8
}

impl Study {
  pub fn new(id: u32, name: impl Into<String>, kind: StudyKind) -> Study {
    Study {
      id,
      name: name.into(),
      kind,
      bodies: Vec::new(),
      materials: Vec::new(),
      constraints: Vec::new(),
      loads: Vec::new(),
      contacts: ContactMode::Automatic,
      bonded: Vec::new(),
      mesh: MeshSettings::default(),
      modes: modes(),
    }
  }
}
