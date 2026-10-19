//! Surface workspace data: how extended edges continue, which loops Untrim removes, the tools
//! Trim cuts with besides a plane, and per-face offset distances.

use clf_sketch::CurveId;
use serde::{Deserialize, Serialize};

use crate::{BodyId, FaceRef, FeatureId};

/// How Extend continues a surface past its edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtendMode {
  /// The surface itself carries on.
  #[default]
  Natural,
  /// Straight on, tangent to the surface.
  Tangent,
  /// Along the surface normal.
  Perpendicular,
}

impl ExtendMode {
  pub fn kernel(self) -> clf_kernel::Extension {
    match self {
      ExtendMode::Natural => clf_kernel::Extension::Natural,
      ExtendMode::Tangent => clf_kernel::Extension::Tangent,
      ExtendMode::Perpendicular => clf_kernel::Extension::Perpendicular,
    }
  }
}

/// Which boundary loops Untrim removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UntrimLoops {
  /// Holes and the outer trim: the whole untrimmed surface.
  #[default]
  All,
  /// Holes only.
  Inner,
  /// The outer boundary only.
  Outer,
}

impl UntrimLoops {
  pub fn kernel(self) -> clf_kernel::Untrim {
    match self {
      UntrimLoops::All => clf_kernel::Untrim::All,
      UntrimLoops::Inner => clf_kernel::Untrim::Inner,
      UntrimLoops::Outer => clf_kernel::Untrim::Outer,
    }
  }
}

/// What Trim cuts with, besides a plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TrimTool {
  /// A solid (keeps what's outside it) or a surface body (keeps what's behind its normals).
  Body { body: BodyId },
  /// Sketch curves projected along the sketch normal. With no curves picked and closed
  /// profiles in the sketch, the profiles' regions are cut away; otherwise the curves split the
  /// surface and the side behind them (to their right, looking along the sketch normal) stays.
  Sketch {
    sketch: FeatureId,
    #[serde(default)]
    curves: Vec<CurveId>,
  },
}

/// A face offset by a distance of its own (variable offset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceDistance {
  pub face: FaceRef,
  pub distance: String,
}
