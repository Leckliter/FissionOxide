//! Construction geometry beyond offset planes: planes, axes and points defined from the model,
//! as in Fusion's CONSTRUCT menu. They follow the faces and edges they were built on.

use fox_sketch::CurveId;
use serde::{Deserialize, Serialize};

use crate::{AxisRef, EdgeRef, FaceRef, FeatureId, PlaneRef};

/// A point used to build construction geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PointRef {
  /// A fixed point in space.
  At { at: [f64; 3] },
  /// An end of an edge (`end`: the end vertex, else the start).
  Vertex {
    edge: EdgeRef,
    #[serde(default)]
    end: bool,
  },
  /// The centre of a circular edge.
  Center { edge: EdgeRef },
  /// A construction point feature.
  Construction { feature: FeatureId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlaneDef {
  /// Through an axis, turned by `angle` from the plane through the axis closest to `base`.
  Angle { axis: AxisRef, base: PlaneRef, angle: String },
  /// Halfway between two planes (the bisector when they are not parallel).
  Midplane { a: PlaneRef, b: PlaneRef },
  /// Through three points.
  ThreePoints { a: PointRef, b: PointRef, c: PointRef },
  /// Through two coplanar straight edges.
  TwoEdges { a: EdgeRef, b: EdgeRef },
  /// Tangent to a cylinder, cone or sphere at the point of it nearest `near`.
  Tangent { face: FaceRef, near: [f64; 3] },
  /// Perpendicular to an edge at a fraction `t` (0 = start, 1 = end) of its length.
  AlongPath {
    edge: EdgeRef,
    #[serde(default = "half")]
    t: String,
  },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AxisDef {
  /// The axis of a cylinder, cone or torus face (or a circular edge).
  Face { face: FaceRef },
  /// Perpendicular to a face through the point of it nearest `near`.
  Normal { face: FaceRef, near: [f64; 3] },
  /// Where two planes meet.
  TwoPlanes { a: PlaneRef, b: PlaneRef },
  /// Through two points.
  TwoPoints { a: PointRef, b: PointRef },
  /// Along a straight edge (or a circular edge's axis).
  Edge { edge: EdgeRef },
  /// Along a line of a sketch (construction or not).
  SketchLine { sketch: FeatureId, curve: CurveId },
  /// Along another construction axis.
  Axis { feature: FeatureId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PointDef {
  /// A vertex, a circle's centre, or a point in space.
  Ref { point: PointRef },
  /// Where two edges meet (the midpoint of their closest approach).
  TwoEdges { a: EdgeRef, b: EdgeRef },
  /// Where three planes meet.
  ThreePlanes { a: PlaneRef, b: PlaneRef, c: PlaneRef },
  /// The centre of a sphere or torus face, or of a circular edge's face.
  Center { face: FaceRef },
  /// Where an edge (extended if straight) crosses a plane.
  EdgePlane { edge: EdgeRef, plane: PlaneRef },
  /// At a fraction `t` of an edge's length.
  AlongPath {
    edge: EdgeRef,
    #[serde(default = "half")]
    t: String,
  },
}

fn half() -> String {
  "0.5".to_string()
}

/// Profile of a coil's wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CoilSection {
  #[default]
  Circular,
  Square,
  /// A triangle pointing out (external), as for a thread.
  TriangleOut,
  /// A triangle pointing in.
  TriangleIn,
}

/// What a mesh feature does to faceted bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshOp {
  /// Fewer triangles: `amount` is the fraction to keep (0.05 – 1).
  Reduce,
  /// Laplacian smoothing: `amount` is the number of passes.
  Smooth,
  /// Merge coplanar triangles back into flat faces (prismatic meshes become clean B-rep).
  Convert,
}

impl PointRef {
  pub(crate) fn feature(&self) -> Option<FeatureId> {
    match self {
      PointRef::Construction { feature } => Some(*feature),
      _ => None,
    }
  }
}

fn plane_dep(p: &PlaneRef) -> Option<FeatureId> {
  match p {
    PlaneRef::Construction { feature } => Some(*feature),
    _ => None,
  }
}

fn axis_dep(a: &AxisRef) -> Option<FeatureId> {
  match a {
    AxisRef::Construction { feature } => Some(*feature),
    _ => None,
  }
}

impl PlaneDef {
  pub(crate) fn depends_on(&self) -> Vec<FeatureId> {
    match self {
      PlaneDef::Angle { axis, base, .. } => axis_dep(axis).into_iter().chain(plane_dep(base)).collect(),
      PlaneDef::Midplane { a, b } => plane_dep(a).into_iter().chain(plane_dep(b)).collect(),
      PlaneDef::ThreePoints { a, b, c } => [a, b, c].iter().filter_map(|p| p.feature()).collect(),
      _ => vec![],
    }
  }

  pub(crate) fn exprs(&mut self) -> Vec<&mut String> {
    match self {
      PlaneDef::Angle { angle, .. } => vec![angle],
      PlaneDef::AlongPath { t, .. } => vec![t],
      _ => vec![],
    }
  }
}

impl AxisDef {
  pub(crate) fn depends_on(&self) -> Vec<FeatureId> {
    match self {
      AxisDef::TwoPlanes { a, b } => plane_dep(a).into_iter().chain(plane_dep(b)).collect(),
      AxisDef::TwoPoints { a, b } => [a, b].iter().filter_map(|p| p.feature()).collect(),
      AxisDef::SketchLine { sketch, .. } => vec![*sketch],
      AxisDef::Axis { feature } => vec![*feature],
      _ => vec![],
    }
  }
}

impl PointDef {
  pub(crate) fn depends_on(&self) -> Vec<FeatureId> {
    match self {
      PointDef::Ref { point } => point.feature().into_iter().collect(),
      PointDef::ThreePlanes { a, b, c } => [a, b, c].iter().filter_map(|p| plane_dep(p)).collect(),
      PointDef::EdgePlane { plane, .. } => plane_dep(plane).into_iter().collect(),
      _ => vec![],
    }
  }

  pub(crate) fn exprs(&mut self) -> Vec<&mut String> {
    match self {
      PointDef::AlongPath { t, .. } => vec![t],
      _ => vec![],
    }
  }
}

/// Cross-section of a pipe, centred on its path at the start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PipeSection {
  /// A circle; the size is its diameter.
  #[default]
  Circular,
  /// A square; the size is its side.
  Square,
  /// An equilateral triangle; the size is its height, measured along the path sketch's normal
  /// from the base to the apex, with the base centred on the path (as Fusion places it).
  Triangular,
}
