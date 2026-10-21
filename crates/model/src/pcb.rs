//! Printed circuit boards in the 3D design (see `FeatureKind::Pcb`): where the outline comes
//! from, and the parts' 3D models placed on the board.

use fox_kernel::Plane;
use fox_kernel::glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::{FaceRef, FeatureId};

/// Where a board's outline comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PcbOutline {
  /// A polygon in board coordinates on the feature's plane.
  Polygon { points: Vec<[f64; 2]> },
  /// The region of a sketch that contains `point`; board coordinates are the sketch's.
  Profile { sketch: FeatureId, point: [f64; 2] },
  /// A planar face; board coordinates are [`face_frame`]'s.
  Face { face: FaceRef },
}

/// A part's 3D model on the board, standing on its side (bottom-side parts hang below).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PcbModel {
  Box {
    size: [f64; 3],
    #[serde(default)]
    offset: [f64; 2],
  },
  Cylinder {
    diameter: f64,
    height: f64,
    #[serde(default)]
    offset: [f64; 2],
  },
  /// A STEP file, scaled, rotated about X, Y then Z (degrees), then moved.
  File {
    path: String,
    #[serde(default)]
    offset: [f64; 3],
    #[serde(default)]
    rotate: [f64; 3],
    #[serde(default = "one3")]
    scale: [f64; 3],
  },
}

fn one3() -> [f64; 3] {
  [1.0; 3]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcbPart {
  pub refdes: String,
  pub at: [f64; 2],
  #[serde(default)]
  pub rot: f64,
  #[serde(default)]
  pub bottom: bool,
  pub model: PcbModel,
}

/// The board frame on a planar face: its origin and normal, X along world X (or Y when the
/// face faces along X).
pub fn face_frame(origin: DVec3, normal: DVec3) -> Option<Plane> {
  let hint = if normal.normalize_or_zero().x.abs() > 0.9 { DVec3::Y } else { DVec3::X };
  Plane::from_normal(origin, normal, hint).ok()
}
