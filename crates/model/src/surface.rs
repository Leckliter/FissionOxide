//! Surface workspace data: how extended edges continue, which loops Untrim removes, the tools
//! Trim cuts with besides a plane, and per-face offset distances.

use fox_sketch::CurveId;
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
  pub fn kernel(self) -> fox_kernel::Extension {
    match self {
      ExtendMode::Natural => fox_kernel::Extension::Natural,
      ExtendMode::Tangent => fox_kernel::Extension::Tangent,
      ExtendMode::Perpendicular => fox_kernel::Extension::Perpendicular,
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
  pub fn kernel(self) -> fox_kernel::Untrim {
    match self {
      UntrimLoops::All => fox_kernel::Untrim::All,
      UntrimLoops::Inner => fox_kernel::Untrim::Inner,
      UntrimLoops::Outer => fox_kernel::Untrim::Outer,
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

/// An open edge of bodies being stitched, and the widest gap between it and the open edge
/// across from it (the nearest one along its whole length).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenEdge {
  pub body: BodyId,
  pub edge: u32,
  pub gap: f64,
  /// The edge's box (its ends and middle), to zoom to it.
  pub lo: fox_kernel::glam::DVec3,
  pub hi: fox_kernel::glam::DVec3,
}

/// The open edges of `bodies` that face another open edge less than `max_gap` away, widest
/// gap first: what Stitch's Unstitched Edges table lists (the ones wider than the tolerance
/// stay open). An edge with nothing across from it (the outer boundary) isn't listed.
pub fn open_edges(model: &crate::regen::Model, bodies: &[BodyId], max_gap: f64) -> Vec<OpenEdge> {
  use fox_kernel::Shape;
  let mut open: Vec<(BodyId, fox_kernel::EdgeInfo, Shape)> = Vec::new();
  for b in bodies {
    let Some(body) = model.body(*b) else { continue };
    for e in body.edges().iter().filter(|e| e.faces[1] == 0 && e.length > 1e-9) {
      if let Ok(shape) = body.shape.edge_shape(e.id) {
        open.push((*b, *e, shape));
      }
      // past this many the pairwise search costs too much for a dialog
      if open.len() >= 200 {
        break;
      }
    }
  }
  let mut out = Vec::new();
  for (k, (body, e, _)) in open.iter().enumerate() {
    let pts: Vec<Shape> = [e.start, e.mid, e.end].iter().filter_map(|p| Shape::vertex(*p).ok()).collect();
    let mut best = f64::INFINITY;
    for (j, (_, _, other)) in open.iter().enumerate() {
      if j == k {
        continue;
      }
      let widest = pts.iter().map(|p| p.distance_to(other).map_or(f64::INFINITY, |d| d.0)).fold(0.0, f64::max);
      best = best.min(widest);
    }
    if best < max_gap {
      out.push(OpenEdge {
        body: *body,
        edge: e.id,
        gap: best,
        lo: e.start.min(e.end).min(e.mid),
        hi: e.start.max(e.end).max(e.mid),
      });
    }
  }
  out.sort_by(|a, b| b.gap.total_cmp(&a.gap));
  out
}

/// How a patch meets the face beside a boundary edge (Fusion's Patch continuity column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Continuity {
  /// G0: the patch only touches the edge.
  #[default]
  Connected,
  /// G1: the patch leaves the edge along the face.
  Tangent,
  /// G2: the patch's curvature matches the face's too.
  Curvature,
}

impl Continuity {
  pub fn kernel(self) -> fox_kernel::Continuity {
    match self {
      Continuity::Connected => fox_kernel::Continuity::Connected,
      Continuity::Tangent => fox_kernel::Continuity::Tangent,
      Continuity::Curvature => fox_kernel::Continuity::Curvature,
    }
  }
}

/// Which way a ruled surface grows from its edges (Fusion's Ruled Type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuledType {
  /// Along the normal of the face beside the edge.
  #[default]
  Normal,
  /// Along that face, away from it.
  Tangent,
}

/// How Offset joins the offsets of picked faces that meet (Fusion's Offset Type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetCorners {
  /// Each face offset on its own (offsets made before the Offset Type row).
  #[default]
  Separate,
  /// The faces of a body offset together, extended until they meet.
  Sharp,
  /// The faces of a body offset together, joined by rounds.
  Round,
}

impl OffsetCorners {
  pub fn is_separate(&self) -> bool {
    *self == OffsetCorners::Separate
  }
}

/// A surface loft section: a sketch's profile (an interior point), or some of its curves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceSection {
  pub sketch: FeatureId,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub profile: Option<[f64; 2]>,
  /// The curves (joined end to end); with no profile and no curves, every curve of the sketch.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub curves: Vec<CurveId>,
}

/// A curve or point a patch passes through (Fusion's Patch Interior Rails/Points).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatchRail {
  /// A body edge.
  Edge { edge: crate::EdgeRef },
  /// A sketch curve.
  Curve { sketch: FeatureId, curve: CurveId },
  /// A point in space (a sketch point or vertex where it was picked).
  Point { at: [f64; 3] },
}
