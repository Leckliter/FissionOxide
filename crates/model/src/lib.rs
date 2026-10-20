//! The parametric design: user parameters and a timeline of features (sketches, extrudes,
//! revolves, fillets, ...) that regenerate into solid bodies.
//!
//! A [`Design`] is pure data (saved as JSON, `.fox` files). [`regen::Regenerator`] replays it
//! through the kernel into a [`regen::Model`]: bodies, solved sketches, construction planes and a
//! per-feature status, the way Fusion's timeline turns red on a failing feature without stopping
//! the features after it.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]
// `!(x > 0.0)` is deliberate: it rejects NaN as well as non-positive values.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

pub mod arrange;
pub mod assembly;
pub mod bounding;
pub mod cam;
pub mod config;
pub mod construct;
pub mod cutcheck;
pub mod direct;
pub mod drawing;
pub mod fillet;
pub mod form;
pub mod groups;
pub mod include;
pub mod mechanism;
pub mod meshops;
pub mod pathpattern;
pub mod plot;
pub mod refs;
pub mod regen;
pub mod sheet;
pub mod sheet2;
pub mod sheet3;
pub mod subassembly;
pub mod surface;
pub mod thread;
pub mod units;

use serde::{Deserialize, Serialize};

pub use assembly::{Component, ComponentId, JointGeometry, JointKind, JointLimits};
pub use fillet::{FilletSet, FullRoundSet, RadiusPoint};
pub use fox_expr::Kind as ParamKind;
pub use fox_kernel as kernel;
pub use fox_sketch as sketch;
pub use fox_sketch::Behaviours;
pub use groups::TimelineGroup;
pub use refs::{EdgeRef, FaceRef};
pub use units::{LengthUnit, ModelParam, ModelParamRef};

use fox_sketch::{CurveId, PointId, Sketch};

/// Current file format version.
///
/// - 2: a symmetric thicken's `thickness` is the thickness on each side (it was the total).
pub const FORMAT_VERSION: u32 = 2;
/// Hostile-file guards.
pub const MAX_FEATURES: usize = 10_000;
pub const MAX_PARAMETERS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FeatureId(pub u32);

/// Stable body identity: assigned by the feature that created the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BodyId(pub u32);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Design {
  #[serde(default = "format_version")]
  pub version: u32,
  pub name: String,
  /// Document Settings > Units: how lengths show and what a bare number typed means. Values and
  /// stored expressions stay in millimetres (see [`units`]).
  #[serde(default, skip_serializing_if = "LengthUnit::is_mm")]
  pub units: LengthUnit,
  #[serde(default)]
  pub parameters: Vec<Parameter>,
  /// Model parameters starred in Change Parameters (Favorites).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub model_favorites: Vec<ModelParamRef>,
  #[serde(default)]
  pub features: Vec<Feature>,
  /// Timeline marker: only the first `marker` features are computed. `None` = all.
  #[serde(default)]
  pub marker: Option<usize>,
  /// Per-body display settings (visibility, colour), by body id.
  #[serde(default)]
  pub bodies: Vec<BodySettings>,
  /// Sheet metal rules (material thickness, bend radius, K-factor). Sheet metal features name
  /// one; the first is the default.
  #[serde(default)]
  pub sheet_metal_rules: Vec<SheetMetalRule>,
  /// Components: named groups of bodies that joints move together.
  #[serde(default)]
  pub components: Vec<Component>,
  /// 2D drawings of the design.
  #[serde(default)]
  pub drawings: Vec<drawing::Drawing>,
  /// Data that scripts and plugins attach to the design and its parts (Fusion's attributes).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub attributes: Vec<Attribute>,
  /// Manufacture setups (CAM).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub cam: Vec<cam::Setup>,
  /// Assembly contact sets (see [`assembly::ContactSet`]).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub contact_sets: Vec<assembly::ContactSet>,
  /// Configurations: named variants overriding parameters, suppression, appearances and joints.
  #[serde(default, skip_serializing_if = "config::ConfigTable::is_empty")]
  pub configurations: config::ConfigTable,
  /// "Capture design history" is off: direct edits collect in a base feature.
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub history_off: bool,
  /// The feature and regeneration behaviours this design was made with: a new design has all of
  /// [`sketch::behaviour::CURRENT`], one read from a file what it stored (none if the field is
  /// missing), so it rebuilds as it did. Each sketch carries its own set for sketch-level
  /// behaviour. See [`fox_sketch::behaviour`] for how to gate a change.
  #[serde(default, skip_serializing_if = "Behaviours::is_empty")]
  pub behaviours: Behaviours,
  /// Saved cameras (the browser's Named Views below Top, Front, Right and Home).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub named_views: Vec<NamedView>,
  /// Appearances applied to single faces (Appearance's Faces mode); they draw over the body's.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub face_appearances: Vec<FaceAppearance>,
  /// Timeline groups (Create Group): how the timeline shows its features; never what they build.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub timeline_groups: Vec<TimelineGroup>,
  #[serde(default)]
  next_id: u32,
}

/// A saved camera: one of Fusion's user Named Views.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NamedView {
  pub name: String,
  /// The point the camera orbits.
  pub target: [f64; 3],
  /// Rotation about +Z and elevation above the XY plane (radians).
  pub yaw: f64,
  pub pitch: f64,
  pub distance: f64,
  /// Vertical field of view (radians).
  #[serde(default = "default_fov")]
  pub fov: f64,
  #[serde(default)]
  pub ortho: bool,
}

fn default_fov() -> f64 {
  30f64.to_radians()
}

/// An appearance on one face of a body; the face is found again after edits (see [`FaceRef`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceAppearance {
  pub face: FaceRef,
  pub color: [u8; 3],
}

fn format_version() -> u32 {
  FORMAT_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Parameter {
  pub name: String,
  pub expr: String,
  #[serde(default)]
  pub kind: ParamKind,
  #[serde(default)]
  pub comment: String,
  /// Starred in Change Parameters (Favorites).
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub favorite: bool,
}

/// A sheet metal rule, as in Fusion: the material settings sheet metal features share.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SheetMetalRule {
  pub name: String,
  pub thickness: String,
  /// Inside bend radius.
  pub bend_radius: String,
  /// Where the neutral layer sits, as a fraction of the thickness from the inside of a bend.
  pub k_factor: String,
  /// Notches cut beside flanges that don't run the full edge.
  #[serde(default)]
  pub relief: ReliefShape,
  /// Relief width; empty = the thickness.
  #[serde(default)]
  pub relief_width: String,
  /// How far reliefs reach past the bend; empty = half the thickness.
  #[serde(default)]
  pub relief_depth: String,
  /// Gap left at mitered corners and rips; empty = half the thickness.
  #[serde(default)]
  pub gap: String,
  /// Notch cut where two flanges' bends meet at a corner.
  #[serde(default)]
  pub corner_relief: CornerRelief,
  /// Corner relief size (square side / round diameter / tear width); empty = default, see
  /// [`sheet::SheetInfo::corner_relief_size`].
  #[serde(default)]
  pub corner_relief_size: String,
}

impl Default for SheetMetalRule {
  fn default() -> Self {
    SheetMetalRule {
      name: "Default".into(),
      thickness: "1 mm".into(),
      bend_radius: "1 mm".into(),
      k_factor: "0.44".into(),
      relief: ReliefShape::Rectangular,
      relief_width: String::new(),
      relief_depth: String::new(),
      gap: String::new(),
      corner_relief: CornerRelief::None,
      corner_relief_size: String::new(),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReliefShape {
  #[default]
  Rectangular,
  Round,
  None,
}

/// What edge flanges built together do where they meet at a corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CornerMode {
  /// Run both flanges into the corner and cut them along the bisector, leaving the rule's gap.
  #[default]
  Miter,
  /// Leave the flanges as they are (an open corner).
  Open,
  /// Run both flanges into the corner and join them along the bisector (a closed, welded seam).
  Closed,
  /// The first flange runs over the end of the second, which stops the rule's gap short of it.
  Overlap,
  /// The walls run on, cut square, until their inside corners are the rule's gap apart; the
  /// bends stop where the sheet does, leaving an open corner relief under the walls.
  Butt,
}

/// The notch cut where two flanges' bends meet at a corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CornerRelief {
  #[default]
  None,
  Round,
  Square,
  Tear,
}

/// One profile of a lofted flange: a sketch's largest closed profile, or an open chain of its
/// curves.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoftProfile {
  pub sketch: FeatureId,
  /// The chain's curves; empty = the sketch's largest closed profile, else all its curves.
  #[serde(default)]
  pub curves: Vec<CurveId>,
}

/// How a lofted flange is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoftMethod {
  /// One smooth surface (formed in a die).
  #[default]
  DieFormed,
  /// Flat facets joined by straight bends (formed on a press brake).
  PressBrake,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BodySettings {
  pub body: BodyId,
  #[serde(default)]
  pub name: Option<String>,
  #[serde(default = "yes")]
  pub visible: bool,
  #[serde(default)]
  pub color: Option<[u8; 3]>,
}

fn yes() -> bool {
  true
}

/// What an [`Attribute`] is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AttrTarget {
  Design,
  Body { body: BodyId },
  Component { component: ComponentId },
  Feature { feature: FeatureId },
  SketchCurve { sketch: FeatureId, curve: CurveId },
  SketchPoint { sketch: FeatureId, point: PointId },
}

impl AttrTarget {
  /// The feature the target belongs to, if any.
  pub fn feature(&self) -> Option<FeatureId> {
    match *self {
      AttrTarget::Feature { feature } => Some(feature),
      AttrTarget::SketchCurve { sketch, .. } | AttrTarget::SketchPoint { sketch, .. } => Some(sketch),
      _ => None,
    }
  }
}

/// A named value in a group (by convention the plugin's id), attached to a target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Attribute {
  pub target: AttrTarget,
  pub group: String,
  pub name: String,
  pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Feature {
  pub id: FeatureId,
  pub name: String,
  #[serde(default)]
  pub suppressed: bool,
  /// Sketches: shown or hidden by the user. `None` = automatic (hidden once a feature uses it).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub visible: Option<bool>,
  /// Join/cut/intersect features: the bodies they may change (empty = any they touch).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub targets: Vec<BodyId>,
  /// Deleted sketches this feature still builds from (Fusion's "cached geometry").
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub cached: Vec<CachedSketch>,
  pub kind: FeatureKind,
}

/// A frozen copy of a deleted sketch that a feature kept using: its solved curves, without
/// constraints, on the plane it was on when it was deleted. The feature finds it under the
/// deleted sketch's id and warns that its profile reference is lost.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CachedSketch {
  pub sketch: FeatureId,
  pub plane: PlaneRef,
  pub geometry: Sketch,
}

/// The world origin planes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginPlane {
  Xy,
  Xz,
  Yz,
}

impl OriginPlane {
  /// The plane's frame in a design framed as `frames` says.
  pub fn plane(self, frames: Frames) -> kernel::Plane {
    match (self, frames) {
      (OriginPlane::Xy, _) => kernel::Plane::XY,
      (OriginPlane::Xz, Frames::Legacy) => kernel::Plane::XZ,
      (OriginPlane::Yz, Frames::Legacy) => kernel::Plane::YZ,
      (OriginPlane::Xz, Frames::Fusion) => kernel::Plane::FUSION_XZ,
      (OriginPlane::Yz, Frames::Fusion) => kernel::Plane::FUSION_YZ,
    }
  }

  /// Which way a positive offset from this plane moves along its frame's normal,
  /// `plane(Frames::of(behaviours)).normal()`, in a design made with `behaviours`. The world
  /// direction depends only on [`XZ_OFFSET_PLUS_Y`](fox_sketch::behaviour::XZ_OFFSET_PLUS_Y):
  /// with it an offset from XZ moves along +Y, as in Fusion; without it (designs from before #62)
  /// along -Y. The frame only decides which of those is its normal: the legacy XZ frame's (X right,
  /// Z up) is -Y, Fusion's (X right, -Z up) +Y. So the two flags never compound.
  pub fn offset_sign(self, behaviours: &Behaviours) -> f64 {
    if self != OriginPlane::Xz {
      return 1.0;
    }
    let plus_y = behaviours.has(fox_sketch::behaviour::XZ_OFFSET_PLUS_Y);
    let normal_plus_y = Frames::of(behaviours) == Frames::Fusion;
    if plus_y == normal_plus_y { 1.0 } else { -1.0 }
  }
}

/// How a design frames its origin planes and planes facing down (the
/// [`FUSION_SKETCH_FRAMES`](fox_sketch::behaviour::FUSION_SKETCH_FRAMES) behaviour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frames {
  /// Designs saved before the flag: XZ is x = +X, y = +Z; YZ is x = +Y, y = +Z; a downward
  /// plane has x = +X.
  Legacy,
  /// Fusion's frames: XZ is x = +X, y = −Z; YZ is x = −Z, y = +Y; a downward plane has x = −X.
  Fusion,
}

impl Frames {
  pub fn of(behaviours: &Behaviours) -> Frames {
    if behaviours.has(fox_sketch::behaviour::FUSION_SKETCH_FRAMES) { Frames::Fusion } else { Frames::Legacy }
  }
}

/// Where a sketch or construction plane sits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlaneRef {
  Origin {
    plane: OriginPlane,
  },
  /// A construction plane feature.
  Construction {
    feature: FeatureId,
  },
  /// A planar face of a body (follows the face as the model changes). `x` hints the in-plane
  /// X axis; without it the frame is [`regen::sketch_plane_on`]'s.
  Face {
    face: FaceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x: Option<[f64; 3]>,
  },
  /// A fixed plane through a point (the sketch origin) with a normal; `x` hints the in-plane
  /// X axis.
  Free {
    origin: [f64; 3],
    normal: [f64; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x: Option<[f64; 3]>,
  },
}

/// Which side of a body's surface a shell's walls go (Fusion's Shell Direction).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellDirection {
  #[default]
  Inside,
  Outside,
  Both,
}

impl ShellDirection {
  pub fn is_inside(&self) -> bool {
    *self == ShellDirection::Inside
  }
}

/// An axis for revolves and circular patterns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AxisRef {
  /// World X / Y / Z through the origin.
  X,
  Y,
  Z,
  /// A line in the revolve's own sketch (usually construction).
  SketchLine {
    curve: CurveId,
  },
  /// A line in a given sketch, for features outside it (e.g. a plane at an angle).
  SketchCurve {
    sketch: FeatureId,
    curve: CurveId,
  },
  /// A straight edge of a body.
  Edge {
    edge: EdgeRef,
  },
  /// A construction axis feature.
  Construction {
    feature: FeatureId,
  },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
  #[default]
  NewBody,
  Join,
  Cut,
  Intersect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExtentKind {
  /// `distance` along the sketch normal (or against it when flipped).
  #[default]
  OneSide,
  /// `distance` to each side.
  Symmetric,
  /// `distance` along the normal and `distance2` against it.
  TwoSides,
  /// Through every body, in the chosen direction.
  ThroughAll,
  /// Through every body on both sides of the sketch plane (Fusion's All, symmetric).
  ThroughAllBoth,
  /// Up to a face (`to`, flat or curved), plus `distance` past it.
  ToFace,
  /// Up to the body `to` is on: the extrusion stops where it meets the body.
  ToBody,
}

/// Which way a thin extrusion's wall grows from the profile's outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThinSide {
  /// Into the profile.
  #[default]
  Inside,
  Outside,
  Center,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Extent {
  #[serde(default)]
  pub kind: ExtentKind,
  #[serde(default = "zero")]
  pub distance: String,
  #[serde(default = "zero")]
  pub distance2: String,
  #[serde(default)]
  pub flip: bool,
  /// Side taper (angle; positive narrows). Empty = none.
  #[serde(default)]
  pub taper: String,
  /// Taper of the second side of a two-sided extent (Fusion's Side 2 taper). Empty = `taper`.
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub taper2: String,
  /// Wall thickness of a thin extrusion. Empty = solid.
  #[serde(default)]
  pub thin: String,
  #[serde(default)]
  pub thin_side: ThinSide,
  /// The face a `to_face` extent stops at.
  #[serde(default)]
  pub to: Option<FaceRef>,
  /// Start offset from the sketch plane along its normal (Fusion's Start: Offset). Empty = the
  /// profile plane.
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub start: String,
  /// Start at a flat face parallel to the sketch (Fusion's Start: Object); `start` is added to it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub start_face: Option<FaceRef>,
}

fn zero() -> String {
  "0".to_string()
}

impl Extent {
  pub fn distance(d: impl Into<String>) -> Self {
    Extent {
      kind: ExtentKind::OneSide,
      distance: d.into(),
      distance2: zero(),
      flip: false,
      taper: String::new(),
      taper2: String::new(),
      thin: String::new(),
      thin_side: ThinSide::Inside,
      to: None,
      start: String::new(),
      start_face: None,
    }
  }
}

/// A sketch curve that tracks a body edge (projected geometry, as in Fusion).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchLink {
  pub curve: CurveId,
  pub edge: EdgeRef,
}

/// Curves of a sketch used as a path: a sweep's guide rail, a loft's rail or centre line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathRef {
  pub sketch: FeatureId,
  /// The curves; empty = every non-construction curve of the sketch.
  #[serde(default)]
  pub curves: Vec<CurveId>,
}

/// One loft section: a profile of a sketch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoftSection {
  pub sketch: FeatureId,
  /// A point inside the profile; `None` = the sketch's largest profile.
  #[serde(default)]
  pub profile: Option<[f64; 2]>,
  /// How the loft meets this section (the first or last).
  #[serde(default, skip_serializing_if = "LoftEnd::is_free")]
  pub end: LoftEnd,
}

/// A loft's end condition at its first or last section.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoftEnd {
  /// No condition: the loft leaves the section as the other sections shape it.
  #[default]
  Free,
  /// A point section the loft comes to a sharp tip at (as `Free`).
  PointSharp,
  /// A point section the loft arrives at tangent to the point's sketch plane, rounded like a
  /// dome; `weight` scales how far the flat top reaches (1 = a paraboloid).
  PointTangent { weight: f64 },
}

impl LoftEnd {
  pub fn is_free(&self) -> bool {
    matches!(self, LoftEnd::Free)
  }
}

/// How a sweep's profile is oriented along the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SweepOrientation {
  /// The profile stays at the same angle to the path as it turns.
  #[default]
  Perpendicular,
  /// The profile keeps its orientation and only moves along the path.
  Parallel,
}

/// How a sweep's profile follows a guide rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProfileScaling {
  /// The profile scales about the path as the rail moves away from it or closer.
  #[default]
  Scale,
  /// The profile keeps its size and only turns to face the rail.
  None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HoleKind {
  #[default]
  Simple,
  Counterbore,
  Countersink,
}

/// Where an edge flange's bend sits relative to the picked edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BendPosition {
  /// The bend starts at the edge; the sheet gets longer.
  #[default]
  Adjacent,
  /// The flange's outside face lands on the edge (the sheet is trimmed back).
  Inside,
  /// The flange's inside face lands on the edge.
  Outside,
}

/// Where Join by Bend puts its bend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JoinBendPosition {
  /// The bend is tangent to both flanges, which are extended (or trimmed) to meet it.
  #[default]
  Tangent,
  /// The bend starts at the first edge (that flange stays as it is); its radius is what makes it
  /// tangent to the second flange, which alone is extended.
  First,
  /// The bend starts at the second edge; only the first flange is extended.
  Second,
}

/// The bend relief Join by Bend cuts where a flange runs on past the end of the bend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JoinRelief {
  /// The rule's relief shape and size.
  #[default]
  Rule,
  Rectangular,
  Round,
  None,
}

/// Which bodies a body-level feature acts on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BodySel {
  #[default]
  All,
  Bodies {
    ids: Vec<BodyId>,
  },
  /// Every body of a component.
  Component {
    id: ComponentId,
  },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Primitive {
  Box { width: String, depth: String, height: String },
  Cylinder { radius: String, height: String },
  Sphere { radius: String },
  Torus { major: String, minor: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)] // plain data, cloned rarely
pub enum FeatureKind {
  Sketch {
    plane: PlaneRef,
    sketch: Sketch,
    /// Sketch curves projected from body edges; they follow the edges as the model changes.
    #[serde(default)]
    links: Vec<SketchLink>,
  },
  Extrude {
    sketch: FeatureId,
    /// Interior points (sketch coordinates) of the chosen regions; empty = every region.
    #[serde(default)]
    profiles: Vec<[f64; 2]>,
    extent: Extent,
    #[serde(default)]
    operation: Operation,
  },
  Revolve {
    sketch: FeatureId,
    #[serde(default)]
    profiles: Vec<[f64; 2]>,
    axis: AxisRef,
    angle: String,
    /// Symmetric: `angle` turns each way from the profile (twice `angle` in all).
    #[serde(default)]
    symmetric: bool,
    /// Two sides: the angle turned the other (negative) way, as well as `angle` the positive way.
    #[serde(default)]
    angle2: Option<String>,
    #[serde(default)]
    operation: Operation,
  },
  Fillet {
    edges: Vec<EdgeRef>,
    radius: String,
    /// Variable radius: the radius at each edge's end (it runs from `radius` at the start).
    #[serde(default)]
    radius2: Option<String>,
    /// Setback corners: where three or more of the edges meet, the corner is re-blended with a
    /// smooth patch starting this far from the vertex.
    #[serde(default)]
    setback: Option<String>,
    /// `radius` is the chord length: the width of the round across the edge.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    chord: bool,
    /// Variable radius with radii at points along each edge (Fusion's variable-radius fillet):
    /// the radius runs smoothly from `radius` at the start, through these, to `radius2` (or
    /// `radius`) at the end. Radii may be zero.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    radii: Vec<RadiusPoint>,
    /// More selection sets, each with its own radius (the rows of Fusion's dialog); `edges` and
    /// `radius` are the first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sets: Vec<FilletSet>,
    /// Asymmetric fillet (an elliptical round): the offset along each edge's second face, with
    /// `radius` the offset along its first (the face whose outward normal sorts first by x, y,
    /// z; the other way round with `flipped`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    distance2: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    flipped: bool,
  },
  Chamfer {
    edges: Vec<EdgeRef>,
    distance: String,
    /// Two-distance chamfer: the distance on the other face.
    #[serde(default)]
    distance2: Option<String>,
    /// Distance-and-angle chamfer.
    #[serde(default)]
    angle: Option<String>,
    /// Two-distance and distance-and-angle chamfers: swap the faces the distances (and the
    /// angle) are measured on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    flipped: bool,
  },
  /// Press-pull: planar faces pushed out (join) or in (cut) along their normals.
  OffsetFaces {
    faces: Vec<FaceRef>,
    distance: String,
  },
  /// Splits bodies with another body (kept unless `remove_tool`).
  SplitByBody {
    #[serde(default)]
    bodies: BodySel,
    tool: BodyId,
    #[serde(default)]
    remove_tool: bool,
  },
  Shell {
    /// Faces to open; the rest of each body becomes a wall of `thickness`.
    faces: Vec<FaceRef>,
    thickness: String,
    /// Round the corners where walls meet (default: sharp).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    round: bool,
    /// Fusion's Direction: walls inside the surface (`thickness`), outside it (`thickness`), or
    /// both (`thickness` inside, `thickness_out` outside).
    #[serde(default, skip_serializing_if = "ShellDirection::is_inside")]
    direction: ShellDirection,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    thickness_out: String,
    /// Whole bodies hollowed with nothing opened (Fusion's body selection).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bodies: Vec<BodyId>,
  },
  /// Box / cylinder / sphere / torus placed on a plane at a point (plane coordinates).
  Primitive {
    shape: Primitive,
    plane: PlaneRef,
    #[serde(default)]
    at: [f64; 2],
    #[serde(default)]
    operation: Operation,
  },
  /// Plane offset from another plane.
  ConstructionPlane {
    base: PlaneRef,
    offset: String,
  },
  Mirror {
    #[serde(default)]
    bodies: BodySel,
    plane: PlaneRef,
    /// Mirror these features instead of bodies (Fusion's "Features" mode): each one's tool solid
    /// is mirrored and applied again with the feature's own operation, so a mirrored cut cuts.
    /// Empty = mirror bodies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    features: Vec<FeatureId>,
    /// Join the mirrored copy to the original (default) or keep it as new bodies.
    #[serde(default = "yes")]
    join: bool,
  },
  RectangularPattern {
    #[serde(default)]
    bodies: BodySel,
    /// Direction (world vector).
    direction: [f64; 3],
    count: String,
    spacing: String,
    #[serde(default)]
    direction2: Option<[f64; 3]>,
    #[serde(default = "one")]
    count2: String,
    #[serde(default = "zero")]
    spacing2: String,
    /// Centre direction 1's instances on the original, as Fusion's Symmetric option: `count`
    /// instances at offsets `k - (count - 1) / 2` (integer division, so an even count puts the
    /// extra instance on the positive side).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    symmetric: bool,
    /// The same for direction 2.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    symmetric2: bool,
    /// Fusion's Extent distance type: `spacing`/`spacing2` are the distance from the original to
    /// the farthest instance (on the longer side of a symmetric direction), not between neighbours.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    extent: bool,
    #[serde(default = "yes")]
    join: bool,
  },
  CircularPattern {
    #[serde(default)]
    bodies: BodySel,
    axis: AxisRef,
    count: String,
    /// Total angle the instances span (360° = evenly around).
    #[serde(default = "full_turn")]
    angle: String,
    /// Spread the instances over -angle/2..angle/2 around the original (Fusion's Symmetric).
    #[serde(default)]
    symmetric: bool,
    #[serde(default = "yes")]
    join: bool,
  },
  /// Moves bodies: turned about `pivot` by `rotate` (about X, then Y, then Z; Fusion's Free
  /// Move angles), then the translation, then the rotation about `axis`. With `copy` the bodies
  /// stay and the moved ones are added as new bodies (Fusion's Create Copy).
  Move {
    #[serde(default)]
    bodies: BodySel,
    #[serde(default = "zeros")]
    translate: [String; 3],
    #[serde(default)]
    axis: Option<AxisRef>,
    #[serde(default = "zero")]
    angle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rotate: Option<[String; 3]>,
    /// The point `rotate` turns about (default the origin).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pivot: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    copy: bool,
  },
  /// Boolean between bodies: tools are joined into / cut from / intersected with the target.
  Combine {
    target: BodyId,
    tools: Vec<BodyId>,
    operation: Operation,
    #[serde(default)]
    keep_tools: bool,
  },
  /// Bodies imported from a file: exact geometry (STEP, IGES, BRep) stored losslessly as OCCT
  /// BRep text, and/or triangle meshes (STL, OBJ, 3MF, PLY) that become faceted bodies.
  Import {
    file_name: String,
    #[serde(default)]
    breps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    meshes: Vec<MeshData>,
  },
  RemoveBodies {
    bodies: Vec<BodyId>,
  },
  /// Solid through profiles of several sketches, in order.
  Loft {
    sections: Vec<LoftSection>,
    /// Straight (ruled) faces between sections instead of smooth ones.
    #[serde(default)]
    ruled: bool,
    /// Loop back to the first section.
    #[serde(default)]
    closed: bool,
    #[serde(default)]
    operation: Operation,
    /// Curves the loft's sides follow (they must touch every section).
    #[serde(default)]
    rails: Vec<PathRef>,
    /// A path the sections are placed along (instead of rails).
    #[serde(default)]
    centerline: Option<PathRef>,
  },
  /// Sweeps profiles along a path (an open chain of lines and arcs in another sketch).
  Sweep {
    sketch: FeatureId,
    #[serde(default)]
    profiles: Vec<[f64; 2]>,
    path_sketch: FeatureId,
    /// The path's curves; empty = every non-construction line and arc of the path sketch.
    #[serde(default)]
    path_curves: Vec<CurveId>,
    #[serde(default)]
    operation: Operation,
    /// A guide rail: the profile turns and scales to keep touching it.
    #[serde(default)]
    rail: Option<PathRef>,
    /// Twist angle from start to end of the path (no rail); empty = none.
    #[serde(default)]
    twist: String,
    /// Taper angle: the profile grows all round by tan(taper) per unit of path length (negative
    /// shrinks it); empty = none.
    #[serde(default)]
    taper: String,
    #[serde(default)]
    orientation: SweepOrientation,
    /// With a rail: whether the profile scales to keep touching it.
    #[serde(default)]
    profile_scaling: ProfileScaling,
  },
  /// Drilled holes at sketch points, into the material (against the sketch normal).
  Hole {
    sketch: FeatureId,
    /// Hole centres; empty = every sketch point that isn't part of a curve.
    #[serde(default)]
    points: Vec<PointId>,
    #[serde(default)]
    kind: HoleKind,
    diameter: String,
    /// Depth of the full-diameter part; ignored when `through_all` or `to` is set.
    #[serde(default = "zero")]
    depth: String,
    #[serde(default)]
    through_all: bool,
    /// 'To' extent: each hole's full-diameter part ends on this plane (the tip goes past it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    to: Option<PlaneRef>,
    /// Drill point angle (0 = flat bottom).
    #[serde(default = "drill_point")]
    tip_angle: String,
    #[serde(default = "zero")]
    head_diameter: String,
    /// Counterbore depth.
    #[serde(default = "zero")]
    head_depth: String,
    /// Countersink included angle.
    #[serde(default = "countersink")]
    head_angle: String,
    #[serde(default)]
    flip: bool,
  },
  /// Splits bodies in two along a plane.
  SplitBody {
    #[serde(default)]
    bodies: BodySel,
    plane: PlaneRef,
  },
  /// Tapers faces by an angle about a neutral plane (pulling along its normal).
  Draft {
    faces: Vec<FaceRef>,
    plane: PlaneRef,
    angle: String,
    #[serde(default)]
    flip: bool,
    /// Split the faces at the neutral plane and taper both sides away from it by `angle`.
    #[serde(default)]
    symmetric: bool,
    /// Two-angle draft: split at the neutral plane; `angle` on the side the (flipped) normal
    /// points to, this on the other side.
    #[serde(default)]
    angle2: Option<String>,
  },
  /// Sheet metal flange from closed sketch profiles: a flat sheet of the rule's thickness.
  Flange {
    sketch: FeatureId,
    #[serde(default)]
    profiles: Vec<[f64; 2]>,
    /// Sheet metal rule name; empty = the design's default rule.
    #[serde(default)]
    rule: String,
    /// Thicken against the sketch normal instead of along it.
    #[serde(default)]
    flip: bool,
    #[serde(default)]
    operation: Operation,
  },
  /// Sheet metal contour flange: an open sketch path (lines and arcs) becomes one face of a bent
  /// sheet; sharp corners get the rule's bend radius. The section is extruded `distance` along
  /// the sketch normal.
  ContourFlange {
    sketch: FeatureId,
    /// The path's curves; empty = every non-construction line and arc in the sketch.
    #[serde(default)]
    curves: Vec<CurveId>,
    distance: String,
    #[serde(default)]
    symmetric: bool,
    /// Put the material on the right of the path (default left).
    #[serde(default)]
    flip: bool,
    /// Extrude against the sketch normal.
    #[serde(default)]
    flip_direction: bool,
    #[serde(default)]
    rule: String,
  },
  /// Sheet metal edge flange: a wall bent up from straight edges of a sheet.
  EdgeFlange {
    edges: Vec<EdgeRef>,
    /// Flange height, measured from the outside of the sheet to the tip.
    height: String,
    #[serde(default = "right_angle")]
    angle: String,
    /// Bend towards the far side of the sheet instead of the side the edge is on.
    #[serde(default)]
    flip: bool,
    #[serde(default)]
    position: BendPosition,
    /// Shorten the flange from the edge's start / end (the sheet gets bend reliefs there).
    #[serde(default = "zero")]
    offset_start: String,
    #[serde(default = "zero")]
    offset_end: String,
    #[serde(default)]
    corners: CornerMode,
  },
  /// Sheet metal hem: the edge folded back 180° onto the sheet.
  Hem {
    edges: Vec<EdgeRef>,
    /// From the outside of the fold to the end of the hem.
    length: String,
    /// Space between the sheet and the folded-back part.
    gap: String,
    #[serde(default)]
    flip: bool,
  },
  /// Sheet metal bend along a sketch line drawn on a flat face of the sheet. The bend is
  /// centred on the line; the side the line's left normal points to swings up (towards the
  /// sketch's side of the sheet) unless flipped.
  Bend {
    sketch: FeatureId,
    line: CurveId,
    angle: String,
    /// Bend away from the sketch side.
    #[serde(default)]
    flip: bool,
    /// Swing the other side of the line.
    #[serde(default)]
    flip_side: bool,
  },
  /// Turns a thin solid into sheet metal (thickness is measured from the body).
  ConvertToSheetMetal {
    body: BodyId,
    /// The face that defines the sheet's side; default the largest planar face.
    #[serde(default)]
    face: Option<FaceRef>,
    #[serde(default)]
    rule: String,
    /// Round sharp corners between the sheet's faces into bends of the rule's radius.
    #[serde(default = "yes")]
    bends: bool,
  },
  /// Slits a solid along edges (e.g. the corners of a shelled box) so it can unfold.
  Rip {
    edges: Vec<EdgeRef>,
    /// Slit width; empty = the sheet rule's gap.
    #[serde(default)]
    gap: String,
  },
  /// Folds an unfolded sheet back up, carrying cuts made while it was flat.
  Refold {
    body: BodyId,
  },
  /// Positions a component by mating one of its joint origins to another's, leaving the joint's
  /// motion (rotation / slide) free to drive.
  Joint {
    /// The component that moves.
    moving: ComponentId,
    moving_origin: JointGeometry,
    /// The component it attaches to; `None` = the world.
    #[serde(default)]
    fixed: Option<ComponentId>,
    fixed_origin: JointGeometry,
    kind: JointKind,
    /// Fixed rotation about the joint's Z axis.
    #[serde(default = "zero")]
    angle: String,
    /// Fixed offset along the joint's Z axis.
    #[serde(default = "zero")]
    offset: String,
    /// Point the Z axes the same way instead of against each other.
    #[serde(default)]
    flip: bool,
    /// Keep the moving component where it is (an as-built joint): only the motion moves it.
    #[serde(default)]
    as_built: bool,
    /// Motion: rotation about Z (revolute, cylindrical, pin-slot, planar, ball).
    #[serde(default = "zero")]
    rotation: String,
    /// Motion: slide along Z (slider, cylindrical) or X (pin-slot, planar).
    #[serde(default = "zero")]
    slide: String,
    /// Motion: slide along Y (planar).
    #[serde(default = "zero")]
    slide2: String,
    /// Motion: tilt about X (ball).
    #[serde(default = "zero")]
    pitch: String,
    #[serde(default)]
    limits: JointLimits,
    /// Screw joints: the advance along Z per full turn of rotation.
    #[serde(default = "zero")]
    lead: String,
  },
  /// Another instance of a component: copies of its bodies, rebuilt from the source on every
  /// change (edit the original and every copy follows), placed by `translate`. Joints can then
  /// position the copy's component like any other.
  CopyComponent {
    source: ComponentId,
    #[serde(default = "zeros")]
    translate: [String; 3],
  },
  /// Construction plane by another method (angle, midplane, three points, ...).
  PlaneBy {
    def: construct::PlaneDef,
  },
  ConstructionAxis {
    def: construct::AxisDef,
  },
  ConstructionPoint {
    def: construct::PointDef,
  },
  /// Threads on cylindrical faces: cosmetic (drawn on the face) or modeled (cut as a helical
  /// groove). External on a boss, internal in a hole.
  Thread {
    faces: Vec<FaceRef>,
    /// Pitch; empty = ISO metric coarse for the diameter.
    #[serde(default)]
    pitch: String,
    /// Threaded length from the face's start; empty = the whole face.
    #[serde(default)]
    length: String,
    #[serde(default = "zero")]
    offset: String,
    #[serde(default)]
    modeled: bool,
    /// Left-handed.
    #[serde(default)]
    left: bool,
    /// Start from the face's other end.
    #[serde(default)]
    flip: bool,
    /// ISO metric size (`M10x1.5`, or `M10` for the coarse pitch). The face is resized to the
    /// class-mean diameter (major outside, minor inside) and its pitch overrides `pitch`. Empty =
    /// the drawn diameter is the basic size, kept as it is.
    #[serde(default)]
    designation: String,
    /// Tolerance class (`6g` outside, `6H` inside when empty).
    #[serde(default)]
    class: String,
  },
  /// A helical coil around an axis normal to a plane, centred at a plane point.
  Coil {
    plane: PlaneRef,
    #[serde(default)]
    at: [f64; 2],
    diameter: String,
    pitch: String,
    height: String,
    #[serde(default)]
    section: construct::CoilSection,
    /// Wire size (circle diameter, square or triangle side).
    section_size: String,
    #[serde(default)]
    left: bool,
    #[serde(default)]
    operation: Operation,
  },
  /// A tube (round, square or triangular) swept along a path of lines and arcs, solid or hollow.
  Pipe {
    path_sketch: FeatureId,
    #[serde(default)]
    path_curves: Vec<CurveId>,
    diameter: String,
    /// Wall thickness; empty = a solid rod.
    #[serde(default)]
    wall: String,
    #[serde(default)]
    operation: Operation,
    /// Section shape; `diameter` is its size.
    #[serde(default)]
    section: construct::PipeSection,
    /// Fraction of the path (from its start) the pipe follows; empty = all of it.
    #[serde(default)]
    distance: String,
  },
  /// A rib (thickness across the sketch plane) or web (thickness in the sketch plane) grown from
  /// open sketch lines until it meets the body, or to a depth.
  Rib {
    sketch: FeatureId,
    #[serde(default)]
    curves: Vec<CurveId>,
    thickness: String,
    /// Depth; empty = to the next face of the body.
    #[serde(default)]
    depth: String,
    /// Grow towards the other side.
    #[serde(default)]
    flip: bool,
    /// A web (in-plane thickness, grows along the sketch normal) instead of a rib.
    #[serde(default)]
    web: bool,
  },
  /// Scales bodies about a point, uniformly or per axis.
  Scale {
    #[serde(default)]
    bodies: BodySel,
    #[serde(default)]
    center: [f64; 3],
    factor: String,
    /// Non-uniform X/Y/Z factors (replace `factor`).
    #[serde(default)]
    factors: Option<[String; 3]>,
  },
  /// Splits faces along a plane; the body stays one piece.
  SplitFace {
    faces: Vec<FaceRef>,
    tool: PlaneRef,
  },
  /// Removes faces and heals the body around them (Simplify > Remove Faces).
  RemoveFaces {
    faces: Vec<FaceRef>,
  },
  /// Thickens faces or surface bodies into solids.
  Thicken {
    #[serde(default)]
    faces: Vec<FaceRef>,
    #[serde(default)]
    bodies: Vec<BodyId>,
    thickness: String,
    /// The full thickness on each side (twice it in all), as Fusion does.
    #[serde(default)]
    symmetric: bool,
    #[serde(default)]
    flip: bool,
    #[serde(default)]
    operation: Operation,
    /// Round the corners where the faces of a surface meet at an angle (default: sharp, the
    /// offset faces extended until they meet).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    round: bool,
  },
  /// A surface extruded from open (or closed) sketch curves.
  SurfaceExtrude {
    sketch: FeatureId,
    #[serde(default)]
    curves: Vec<CurveId>,
    distance: String,
    #[serde(default)]
    symmetric: bool,
    #[serde(default)]
    flip: bool,
  },
  /// A surface revolved from sketch curves.
  SurfaceRevolve {
    sketch: FeatureId,
    #[serde(default)]
    curves: Vec<CurveId>,
    axis: AxisRef,
    #[serde(default = "full_turn")]
    angle: String,
  },
  /// A surface swept from sketch curves (open or closed) along a path (Fusion's SURFACE >
  /// Sweep): a new surface body, never joined or cut.
  SurfaceSweep {
    sketch: FeatureId,
    /// The profile curves; empty = every non-construction curve of the sketch.
    #[serde(default)]
    curves: Vec<CurveId>,
    path_sketch: FeatureId,
    /// The path's curves; empty = every non-construction curve of the path sketch.
    #[serde(default)]
    path_curves: Vec<CurveId>,
    /// Twist angle from start to end of the path; empty = none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    twist: String,
    #[serde(default)]
    orientation: SweepOrientation,
  },
  /// A surface lofted through sketch curves (open or closed), in order (Fusion's SURFACE >
  /// Loft): a new surface body, never joined or cut.
  SurfaceLoft {
    /// Each section: a sketch's curves forming one chain (empty = all of them).
    sections: Vec<PathRef>,
    #[serde(default)]
    ruled: bool,
    #[serde(default)]
    closed: bool,
  },
  /// A surface filling a closed boundary: body edges or a sketch's curves.
  Patch {
    #[serde(default)]
    edges: Vec<EdgeRef>,
    #[serde(default)]
    sketch: Option<FeatureId>,
  },
  /// Sews surface bodies into one (a solid when they close up).
  Stitch {
    bodies: Vec<BodyId>,
    #[serde(default = "stitch_tolerance")]
    tolerance: String,
  },
  /// Breaks bodies into one surface body per face.
  Unstitch {
    bodies: Vec<BodyId>,
  },
  /// A ruled surface between two edges.
  Ruled {
    a: EdgeRef,
    b: EdgeRef,
  },
  /// New surface bodies offset from faces or bodies along their normals.
  SurfaceOffset {
    #[serde(default)]
    faces: Vec<FaceRef>,
    #[serde(default)]
    bodies: Vec<BodyId>,
    distance: String,
    /// Faces offset by distances of their own (variable offset).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    face_distances: Vec<surface::FaceDistance>,
  },
  /// Trims surface bodies with a plane, keeping the side behind it (or in front, flipped).
  Trim {
    bodies: Vec<BodyId>,
    tool: PlaneRef,
    #[serde(default)]
    flip: bool,
  },
  /// Flips the normals of surface bodies.
  ReverseNormal {
    bodies: Vec<BodyId>,
  },
  /// Mesh tools on faceted (imported mesh) bodies: reduce, smooth or convert.
  Mesh {
    bodies: Vec<BodyId>,
    op: construct::MeshOp,
    #[serde(default = "one")]
    amount: String,
  },
  /// Flattens a sheet metal body onto its stationary face.
  Unfold {
    body: BodyId,
    /// Stationary face; default the largest planar face.
    #[serde(default)]
    face: Option<FaceRef>,
    /// Keep the folded body and add the flat pattern as a new body.
    #[serde(default)]
    keep_folded: bool,
  },
  /// Sketch profiles (or text) raised from or sunk into a face, flat or curved.
  Emboss {
    sketch: FeatureId,
    #[serde(default)]
    profiles: Vec<[f64; 2]>,
    face: FaceRef,
    depth: String,
    /// Sink into the face instead of raising.
    #[serde(default)]
    deboss: bool,
    /// Wrap around a cylindrical face (keeping lengths) instead of projecting.
    #[serde(default)]
    wrap: bool,
    /// Moves the profiles along the sketch's x axis first (Fusion's horizontal distance).
    #[serde(default = "zero")]
    horizontal: String,
    /// Moves the profiles along the face's vertical: the sketch's y axis as seen from outside.
    #[serde(default = "zero")]
    vertical: String,
    /// Turns the profiles about their centre, counterclockwise seen from outside the face.
    #[serde(default = "zero")]
    rotation: String,
  },
  /// Solids from the closed cells that bodies and planes bound.
  BoundaryFill {
    #[serde(default)]
    tools: Vec<BodyId>,
    #[serde(default)]
    planes: Vec<PlaneRef>,
    /// A point inside each cell to keep; empty = every cell.
    #[serde(default)]
    cells: Vec<[f64; 3]>,
    #[serde(default)]
    operation: Operation,
    /// Remove the tool bodies afterwards.
    #[serde(default)]
    remove_tools: bool,
  },
  /// Replaces flat faces of a body with a surface (a face of another body, or a plane): the
  /// body grows or shrinks to it and the neighbouring faces are extended or trimmed.
  ReplaceFace {
    faces: Vec<FaceRef>,
    #[serde(default)]
    target: Option<FaceRef>,
    #[serde(default)]
    plane: Option<PlaneRef>,
  },
  /// Extends open edges of surface bodies outward by a distance.
  SurfaceExtend {
    edges: Vec<EdgeRef>,
    distance: String,
    #[serde(default)]
    mode: surface::ExtendMode,
  },
  /// Removes holes from faces and/or restores their untrimmed outer boundary.
  Untrim {
    #[serde(default)]
    faces: Vec<FaceRef>,
    #[serde(default)]
    bodies: Vec<BodyId>,
    #[serde(default)]
    loops: surface::UntrimLoops,
  },
  /// Trims surface bodies with another body or with sketch curves projected onto them.
  TrimWith {
    bodies: Vec<BodyId>,
    tool: surface::TrimTool,
    /// Keep the other side.
    #[serde(default)]
    flip: bool,
    /// Points picking the pieces to keep (instead of a side).
    #[serde(default)]
    keep: Vec<[f64; 3]>,
  },
  /// Sheet metal lofted flange: a sheet between two profiles (closed profiles are opened by the
  /// rule's gap so the flange unfolds). The profiles are one face of the sheet.
  LoftedFlange {
    from: LoftProfile,
    to: LoftProfile,
    #[serde(default)]
    method: LoftMethod,
    /// Press brake: how many bends (facet boundaries) to aim for.
    #[serde(default = "lofted_bends")]
    bends: String,
    /// Put the material on the other side of the profiles.
    #[serde(default)]
    flip: bool,
    #[serde(default)]
    rule: String,
    /// Press brake: round the bends between facets to the rule's bend radius (else sharp folds).
    #[serde(default)]
    rounded: bool,
  },
  /// Sheet metal miter flange: an open sketch profile (drawn across the first edge, starting on
  /// it) swept along a chain of sheet edges, with mitred corners.
  MiterFlange {
    sketch: FeatureId,
    /// The profile's curves; empty = every non-construction line and arc in the sketch.
    #[serde(default)]
    curves: Vec<CurveId>,
    edges: Vec<EdgeRef>,
    #[serde(default)]
    corners: CornerMode,
  },
  /// Sheet metal Join by Bend: joins two separate sheet metal bodies, an edge picked on a flange
  /// of each, with a bend; flanges are extended (or trimmed) to meet it. The first body keeps
  /// the result.
  JoinByBend {
    edge1: EdgeRef,
    edge2: EdgeRef,
    /// Inside bend radius; empty = the first body's rule.
    #[serde(default)]
    bend_radius: String,
    #[serde(default)]
    position: JoinBendPosition,
    #[serde(default)]
    relief: JoinRelief,
  },
  /// Sheet metal fillet: rounds edges that run through the sheet's thickness (flange corners),
  /// so the rounding shows in the flat pattern.
  SheetFillet {
    edges: Vec<EdgeRef>,
    radius: String,
  },
  /// Sheet metal chamfer: bevels edges that run through the sheet's thickness, by an equal
  /// distance or two distances (`distance` on the edge's first face, `distance2` on the other;
  /// `flipped` swaps them).
  SheetChamfer {
    edges: Vec<EdgeRef>,
    distance: String,
    #[serde(default)]
    distance2: Option<String>,
    #[serde(default)]
    flipped: bool,
  },
  /// A saved joint origin on a component: a frame (from a face, an edge or a point, then
  /// offset, turned and flipped) that joints can mate to and that moves with the component.
  JointOrigin {
    #[serde(default = "no_component")]
    component: ComponentId,
    #[serde(default = "origin_point")]
    geometry: JointGeometry,
    /// Rotation about the origin's Z axis.
    #[serde(default = "zero")]
    angle: String,
    /// Offset along the origin's Z axis.
    #[serde(default = "zero")]
    offset: String,
    /// Offset along the origin's own X axis.
    #[serde(default = "zero")]
    offset_x: String,
    /// Offset along the origin's own Y axis.
    #[serde(default = "zero")]
    offset_y: String,
    /// Reverse the Z axis.
    #[serde(default)]
    flip: bool,
  },
  /// Components that move as one: a joint moving any of them moves them all.
  RigidGroup {
    #[serde(default)]
    components: Vec<ComponentId>,
    /// Where joints had put each member when the group was made (column-major matrices, one per
    /// component). Members that joints haven't moved since, e.g. bodies modeled after the joint,
    /// are brought along by the others' movement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    poses: Vec<[f64; 16]>,
  },
  /// Drives one joint's motion from another's (gears, rack and pinion, leadscrews): when the
  /// leader moves `leader_value`, the follower moves `follower_value`.
  MotionLink {
    #[serde(default = "no_feature")]
    leader: FeatureId,
    #[serde(default)]
    leader_motion: assembly::MotionAxis,
    #[serde(default = "no_feature")]
    follower: FeatureId,
    #[serde(default)]
    follower_motion: assembly::MotionAxis,
    #[serde(default = "full_turn")]
    leader_value: String,
    #[serde(default = "full_turn")]
    follower_value: String,
    #[serde(default)]
    reverse: bool,
  },
  /// A component inserted from another design: the source's bodies, stored as BRep text. With a
  /// `source`, it is a linked reference to a data hub design that can be updated to the source's
  /// newest version (or unlinked to keep a local copy).
  InsertDesign {
    #[serde(default)]
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<assembly::HubSource>,
    #[serde(default)]
    breps: Vec<String>,
    #[serde(default = "zeros")]
    translate: [String; 3],
    /// The source's own components and joints, when it was an assembly (see [`subassembly`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sub: Option<Box<subassembly::SubAssembly>>,
  },
  /// Non-parametric geometry: frozen bodies and direct edits (move face, press-pull, delete
  /// face) replayed in order. See [`direct`].
  BaseFeature {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bodies: Vec<direct::BaseBody>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    edits: Vec<direct::DirectEdit>,
  },
  /// Cam motion: a joint's slide (the follower) is set so the follower stays in contact with
  /// the cam component's bodies wherever they are (see [`mechanism`]).
  CamFollower {
    #[serde(default = "no_component")]
    cam: ComponentId,
    /// The follower's joint (slider, cylindrical or pin-slot): its slide is driven.
    #[serde(default = "no_feature")]
    follower: FeatureId,
    /// A flat-faced follower (touches the cam's furthest point along the slide) instead of a
    /// knife edge (touches where the slide line meets the cam).
    #[serde(default)]
    flat: bool,
    /// Gap kept between the follower and the cam (a roller's radius, or clearance).
    #[serde(default = "zero")]
    clearance: String,
  },
  /// Create Form: a T-spline body, the smooth limit surface of a quad control cage given in the
  /// plane's coordinates (see [`form`]). History-free: form edits change the cage itself.
  Form {
    plane: PlaneRef,
    cage: form::FormCage,
    #[serde(default)]
    operation: Operation,
  },
  /// An align (assembly) constraint: makes a moving component's face, axis or point flush,
  /// concentric or coincident with one on a fixed component (or the world), moving it only as
  /// far as that takes (see [`assembly::align_transform`]); unlike a joint, the rest of its
  /// position is kept.
  AlignConstraint {
    #[serde(default = "no_component")]
    moving: ComponentId,
    #[serde(default = "origin_point")]
    moving_geometry: JointGeometry,
    /// `None` = the world.
    #[serde(default)]
    fixed: Option<ComponentId>,
    #[serde(default = "origin_point")]
    fixed_geometry: JointGeometry,
    #[serde(default)]
    relation: assembly::AlignRelation,
    /// Gap between flush planes (along the fixed normal) or coincident points (along fixed Z).
    #[serde(default = "zero")]
    offset: String,
    /// Flush: normals point the same way instead of facing. Concentric: reverse the axis.
    #[serde(default)]
    flip: bool,
  },
  /// Assembly constraint (Center): moves a component the least it can so its plane lands on a
  /// plane of another component, `offset` apart along the fixed plane's normal. Each side is one
  /// plane or the midplane between two parallel ones, so centring a part between two faces is a
  /// pair of faces on each side. Motion along the plane stays where it is.
  PlaneConstraint {
    #[serde(default = "no_component")]
    moving: ComponentId,
    /// One plane, or two whose midplane is used.
    #[serde(default)]
    moving_planes: Vec<PlaneRef>,
    /// The component it is placed against; `None` = the world.
    #[serde(default)]
    fixed: Option<ComponentId>,
    #[serde(default)]
    fixed_planes: Vec<PlaneRef>,
    #[serde(default = "zero")]
    offset: String,
  },
  /// Bounding Solid: a new body enclosing the bodies (a box, or a cylinder about the frame's
  /// normal), in a plane's frame (default world XY), grown by the offsets. The bodies stay.
  BoundingSolid {
    #[serde(default)]
    bodies: BodySel,
    #[serde(default)]
    shape: bounding::BoundShape,
    /// The frame the box or cylinder lines up with (its normal is the cylinder's axis).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plane: Option<PlaneRef>,
    /// Grown on every side (the cylinder: radially and at both ends).
    #[serde(default = "zero")]
    offset: String,
    /// Per side instead of `offset`, in the frame: -x, +x, -y, +y, -z, +z. The cylinder takes
    /// -z, +z for its ends and grows radially by `offset`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sides: Vec<String>,
  },
  /// Sketch curves draped onto faces as 3D curves (Fusion's Project To Surface): pushed along a
  /// direction until they meet the faces, or each point dropped onto the nearest point of the
  /// faces. A pipe can follow them.
  ProjectToSurface {
    sketch: FeatureId,
    /// The curves to project; empty = every non-construction curve.
    #[serde(default)]
    curves: Vec<CurveId>,
    faces: Vec<FaceRef>,
    /// The direction to project along; none = to the closest point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    along: Option<AxisRef>,
  },
  /// Copies bodies, or re-applies features, along a sketch path (Fusion's Pattern on Path).
  PathPattern {
    #[serde(default)]
    bodies: BodySel,
    /// Features whose tools are re-applied at each copy; when empty, the bodies are copied.
    #[serde(default)]
    features: Vec<FeatureId>,
    path_sketch: FeatureId,
    /// The path's curves, chained end to end (default every line and arc of the sketch).
    #[serde(default)]
    path_curves: Vec<CurveId>,
    count: String,
    /// Arc length between neighbours, or the whole pattern's length when `extent` is set.
    distance: String,
    #[serde(default)]
    extent: bool,
    /// Where the original sits along the path, as a fraction of its length (0 = its start).
    #[serde(default = "zero")]
    start: String,
    /// Run toward the path's start instead of its end.
    #[serde(default)]
    flip: bool,
    /// Centre the copies on the original (`count` in total), both ways along the path.
    #[serde(default)]
    symmetric: bool,
    #[serde(default)]
    orientation: pathpattern::PathOrientation,
    #[serde(default = "yes")]
    join: bool,
  },
  /// Copies of components laid flat on their largest planar face and packed on a rectangular
  /// envelope (`length` along the plane's X, `width` along its Y, from its origin) for
  /// manufacturing; the sources stay where they are. See [`arrange`].
  Arrange {
    #[serde(default)]
    items: Vec<arrange::ArrangeItem>,
    plane: PlaneRef,
    length: String,
    width: String,
    #[serde(default)]
    solver: arrange::ArrangeSolver,
    /// Smallest gap between parts.
    #[serde(default = "zero")]
    spacing: String,
    /// Margin kept clear along the envelope's edges.
    #[serde(default = "zero")]
    frame: String,
  },
  /// Full round fillet: each set's centre face is replaced by one round tangent to it and to
  /// the faces on both sides (a half-cylinder across a rib with parallel sides).
  FullRoundFillet {
    sets: Vec<FullRoundSet>,
  },
  /// 3D curves where sketch curves, pushed along their sketch's normal, cross body faces or
  /// other sketch curves pushed along theirs (Fusion's Intersection Curve). Pipes follow them as
  /// a path (see `regen/curves3d.rs`).
  IntersectionCurve {
    /// The curves pushed along their sketch's normal (no curves: every non-construction curve).
    curves: PathRef,
    /// Faces they cross.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    faces: Vec<FaceRef>,
    /// Other sketches' curves, each pushed along its own sketch's normal, that they cross.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    with: Vec<PathRef>,
  },
  /// A reference image placed on a plane (Insert > Canvas). It adds no geometry: the viewport
  /// draws it, see-through, for tracing over. Calibrate rescales it from a known distance.
  Canvas {
    plane: PlaneRef,
    /// The image file.
    image: String,
    /// The image's centre on the plane (plane coordinates, mm).
    #[serde(default)]
    at: [f64; 2],
    /// Size on the plane (mm); the height keeps the image's aspect.
    width: f64,
    height: f64,
    /// Rotation about the centre (degrees).
    #[serde(default)]
    angle: f64,
    /// 0 (invisible) to 1 (opaque).
    #[serde(default = "full_opacity")]
    opacity: f64,
  },
}

fn full_opacity() -> f64 {
  1.0
}

fn lofted_bends() -> String {
  "8".to_string()
}

fn no_component() -> ComponentId {
  ComponentId(0)
}

fn no_feature() -> FeatureId {
  FeatureId(0)
}

fn origin_point() -> JointGeometry {
  JointGeometry::Point { point: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0] }
}

fn stitch_tolerance() -> String {
  "0.01 mm".to_string()
}

fn drill_point() -> String {
  "118 deg".to_string()
}

fn countersink() -> String {
  "90 deg".to_string()
}

fn right_angle() -> String {
  "90 deg".to_string()
}

/// An imported triangle mesh (millimetres).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshData {
  pub name: String,
  /// x, y, z per vertex.
  pub positions: Vec<f32>,
  /// Three vertex indices per triangle.
  pub indices: Vec<u32>,
  /// Merge coplanar triangles into planar faces when building the body.
  #[serde(default)]
  pub merge_coplanar: bool,
}

fn one() -> String {
  "1".to_string()
}

fn full_turn() -> String {
  "360 deg".to_string()
}

fn zeros() -> [String; 3] {
  [zero(), zero(), zero()]
}

impl FeatureKind {
  /// Base name for new features of this kind ("Extrude" → "Extrude3").
  pub fn base_name(&self) -> &'static str {
    match self {
      FeatureKind::Sketch { .. } => "Sketch",
      FeatureKind::Extrude { .. } => "Extrude",
      FeatureKind::Revolve { .. } => "Revolve",
      FeatureKind::Fillet { .. } => "Fillet",
      FeatureKind::OffsetFaces { .. } => "PressPull",
      FeatureKind::SplitByBody { .. } => "Split",
      FeatureKind::Chamfer { .. } => "Chamfer",
      FeatureKind::Shell { .. } => "Shell",
      FeatureKind::Primitive { shape: Primitive::Box { .. }, .. } => "Box",
      FeatureKind::Primitive { shape: Primitive::Cylinder { .. }, .. } => "Cylinder",
      FeatureKind::Primitive { shape: Primitive::Sphere { .. }, .. } => "Sphere",
      FeatureKind::Primitive { shape: Primitive::Torus { .. }, .. } => "Torus",
      FeatureKind::ConstructionPlane { .. } => "Plane",
      FeatureKind::Mirror { .. } => "Mirror",
      FeatureKind::RectangularPattern { .. } => "RectangularPattern",
      FeatureKind::CircularPattern { .. } => "CircularPattern",
      FeatureKind::Move { .. } => "Move",
      FeatureKind::Combine { .. } => "Combine",
      FeatureKind::Import { .. } => "Import",
      FeatureKind::RemoveBodies { .. } => "Remove",
      FeatureKind::Loft { .. } => "Loft",
      FeatureKind::Sweep { .. } => "Sweep",
      FeatureKind::Hole { .. } => "Hole",
      FeatureKind::SplitBody { .. } => "Split",
      FeatureKind::Draft { .. } => "Draft",
      FeatureKind::Flange { .. } => "Flange",
      FeatureKind::ContourFlange { .. } => "ContourFlange",
      FeatureKind::EdgeFlange { .. } => "EdgeFlange",
      FeatureKind::Hem { .. } => "Hem",
      FeatureKind::Bend { .. } => "Bend",
      FeatureKind::ConvertToSheetMetal { .. } => "ConvertToSheetMetal",
      FeatureKind::Unfold { .. } => "Unfold",
      FeatureKind::Rip { .. } => "Rip",
      FeatureKind::Joint { .. } => "Joint",
      FeatureKind::CopyComponent { .. } => "Copy",
      FeatureKind::Refold { .. } => "Refold",
      FeatureKind::PlaneBy { .. } => "Plane",
      FeatureKind::ConstructionAxis { .. } => "Axis",
      FeatureKind::ConstructionPoint { .. } => "Point",
      FeatureKind::Thread { .. } => "Thread",
      FeatureKind::Coil { .. } => "Coil",
      FeatureKind::Pipe { .. } => "Pipe",
      FeatureKind::Rib { web: true, .. } => "Web",
      FeatureKind::Rib { .. } => "Rib",
      FeatureKind::Scale { .. } => "Scale",
      FeatureKind::SplitFace { .. } => "SplitFace",
      FeatureKind::RemoveFaces { .. } => "RemoveFaces",
      FeatureKind::Thicken { .. } => "Thicken",
      FeatureKind::SurfaceExtrude { .. } => "SurfaceExtrude",
      FeatureKind::SurfaceRevolve { .. } => "SurfaceRevolve",
      FeatureKind::SurfaceSweep { .. } => "SurfaceSweep",
      FeatureKind::SurfaceLoft { .. } => "SurfaceLoft",
      FeatureKind::Patch { .. } => "Patch",
      FeatureKind::Stitch { .. } => "Stitch",
      FeatureKind::Unstitch { .. } => "Unstitch",
      FeatureKind::ReverseNormal { .. } => "ReverseNormal",
      FeatureKind::Ruled { .. } => "Ruled",
      FeatureKind::SurfaceOffset { .. } => "Offset",
      FeatureKind::Trim { .. } => "Trim",
      FeatureKind::Mesh { op: construct::MeshOp::Reduce, .. } => "Reduce",
      FeatureKind::Mesh { op: construct::MeshOp::Smooth, .. } => "Smooth",
      FeatureKind::Mesh { op: construct::MeshOp::Convert, .. } => "ConvertMesh",
      FeatureKind::Emboss { deboss: true, .. } => "Deboss",
      FeatureKind::Emboss { .. } => "Emboss",
      FeatureKind::BoundaryFill { .. } => "BoundaryFill",
      FeatureKind::ReplaceFace { .. } => "ReplaceFace",
      FeatureKind::SurfaceExtend { .. } => "Extend",
      FeatureKind::Untrim { .. } => "Untrim",
      FeatureKind::TrimWith { .. } => "Trim",
      FeatureKind::LoftedFlange { .. } => "LoftedFlange",
      FeatureKind::MiterFlange { .. } => "MiterFlange",
      FeatureKind::JoinByBend { .. } => "JoinByBend",
      FeatureKind::SheetFillet { .. } => "SheetMetalFillet",
      FeatureKind::SheetChamfer { .. } => "SheetMetalChamfer",
      FeatureKind::JointOrigin { .. } => "JointOrigin",
      FeatureKind::RigidGroup { .. } => "RigidGroup",
      FeatureKind::MotionLink { .. } => "MotionLink",
      FeatureKind::InsertDesign { .. } => "Insert",
      FeatureKind::BaseFeature { .. } => "BaseFeature",
      FeatureKind::CamFollower { .. } => "Cam",
      FeatureKind::Form { .. } => "Form",
      FeatureKind::AlignConstraint { .. } => "Align",
      FeatureKind::PlaneConstraint { .. } => "CenterConstraint",
      FeatureKind::BoundingSolid { .. } => "BoundingSolid",
      FeatureKind::ProjectToSurface { .. } => "ProjectToSurface",
      FeatureKind::PathPattern { .. } => "PathPattern",
      FeatureKind::Arrange { .. } => "Arrange",
      FeatureKind::FullRoundFillet { .. } => "Fillet",
      FeatureKind::IntersectionCurve { .. } => "IntersectionCurve",
      FeatureKind::Canvas { .. } => "Canvas",
    }
  }

  /// The sheet metal rule a feature names, if it is a sheet metal feature with one.
  pub fn sheet_rule(&self) -> Option<&str> {
    match self {
      FeatureKind::Flange { rule, .. }
      | FeatureKind::ContourFlange { rule, .. }
      | FeatureKind::ConvertToSheetMetal { rule, .. } => Some(rule.as_str()),
      FeatureKind::LoftedFlange { rule, .. } => Some(rule.as_str()),
      _ => None,
    }
  }

  /// Features this one depends on directly (sketches, construction planes).
  pub fn depends_on(&self) -> Vec<FeatureId> {
    let plane_dep = |p: &PlaneRef| match p {
      PlaneRef::Construction { feature } => vec![*feature],
      _ => vec![],
    };
    let axis_dep = |a: &AxisRef| match a {
      AxisRef::Construction { feature } | AxisRef::SketchCurve { sketch: feature, .. } => vec![*feature],
      _ => vec![],
    };
    match self {
      FeatureKind::Sketch { plane, .. } => plane_dep(plane),
      FeatureKind::Revolve { sketch, axis, .. } => std::iter::once(*sketch).chain(axis_dep(axis)).collect(),
      FeatureKind::Extrude { sketch, .. }
      | FeatureKind::Flange { sketch, .. }
      | FeatureKind::ContourFlange { sketch, .. }
      | FeatureKind::Bend { sketch, .. } => vec![*sketch],
      FeatureKind::Hole { sketch, to, .. } => std::iter::once(*sketch).chain(to.iter().flat_map(plane_dep)).collect(),
      FeatureKind::Loft { sections, rails, centerline, .. } => {
        sections.iter().map(|s| s.sketch).chain(rails.iter().chain(centerline).map(|r| r.sketch)).collect()
      }
      FeatureKind::Sweep { sketch, path_sketch, rail, .. } => {
        std::iter::once(*sketch).chain(std::iter::once(*path_sketch)).chain(rail.iter().map(|r| r.sketch)).collect()
      }
      FeatureKind::SplitBody { plane, .. } | FeatureKind::Draft { plane, .. } => plane_dep(plane),
      FeatureKind::Primitive { plane, .. } => plane_dep(plane),
      FeatureKind::Mirror { plane, features, .. } => {
        plane_dep(plane).into_iter().chain(features.iter().copied()).collect()
      }
      FeatureKind::ConstructionPlane { base, .. } => plane_dep(base),
      FeatureKind::PlaneBy { def } => def.depends_on(),
      FeatureKind::ConstructionAxis { def } => def.depends_on(),
      FeatureKind::ConstructionPoint { def } => def.depends_on(),
      FeatureKind::Coil { plane, .. }
      | FeatureKind::SplitFace { tool: plane, .. }
      | FeatureKind::Trim { tool: plane, .. } => plane_dep(plane),
      FeatureKind::Pipe { path_sketch, .. } => vec![*path_sketch],
      FeatureKind::Rib { sketch, .. } | FeatureKind::SurfaceExtrude { sketch, .. } => vec![*sketch],
      FeatureKind::SurfaceRevolve { sketch, axis, .. } => {
        let mut v = vec![*sketch];
        v.extend(axis_dep(axis));
        v
      }
      FeatureKind::SurfaceSweep { sketch, path_sketch, .. } => vec![*sketch, *path_sketch],
      FeatureKind::SurfaceLoft { sections, .. } => sections.iter().map(|r| r.sketch).collect(),
      FeatureKind::Patch { sketch, .. } => sketch.iter().copied().collect(),
      FeatureKind::CircularPattern { axis, .. } => axis_dep(axis),
      FeatureKind::Move { axis: Some(axis), .. } => axis_dep(axis),
      FeatureKind::Emboss { sketch, .. } => vec![*sketch],
      FeatureKind::BoundaryFill { planes, .. } => planes.iter().flat_map(plane_dep).collect(),
      FeatureKind::ReplaceFace { plane: Some(plane), .. } => plane_dep(plane),
      FeatureKind::TrimWith { tool: surface::TrimTool::Sketch { sketch, .. }, .. } => vec![*sketch],
      FeatureKind::LoftedFlange { from, to, .. } => vec![from.sketch, to.sketch],
      FeatureKind::MiterFlange { sketch, .. } => vec![*sketch],
      FeatureKind::Joint { moving_origin, fixed_origin, .. } => {
        [moving_origin, fixed_origin].into_iter().filter_map(assembly::JointGeometry::origin).collect()
      }
      FeatureKind::JointOrigin { geometry, .. } => geometry.origin().into_iter().collect(),
      FeatureKind::MotionLink { leader, follower, .. } => vec![*leader, *follower],
      FeatureKind::CamFollower { follower, .. } => vec![*follower],
      FeatureKind::Form { plane, .. } => plane_dep(plane),
      FeatureKind::AlignConstraint { moving_geometry, fixed_geometry, .. } => {
        [moving_geometry, fixed_geometry].into_iter().filter_map(assembly::JointGeometry::origin).collect()
      }
      FeatureKind::PlaneConstraint { moving_planes, fixed_planes, .. } => {
        moving_planes.iter().chain(fixed_planes).flat_map(plane_dep).collect()
      }
      FeatureKind::BoundingSolid { plane: Some(plane), .. } => plane_dep(plane),
      FeatureKind::ProjectToSurface { sketch, along, .. } => {
        std::iter::once(*sketch).chain(along.iter().flat_map(axis_dep)).collect()
      }
      FeatureKind::PathPattern { features, path_sketch, .. } => {
        std::iter::once(*path_sketch).chain(features.iter().copied()).collect()
      }
      FeatureKind::Arrange { plane, .. } => plane_dep(plane),
      FeatureKind::IntersectionCurve { curves, with, .. } => {
        std::iter::once(curves.sketch).chain(with.iter().map(|w| w.sketch)).collect()
      }
      FeatureKind::Canvas { plane, .. } => plane_dep(plane),
      _ => vec![],
    }
  }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DesignError {
  #[error("no feature {0:?}")]
  NoFeature(FeatureId),
  #[error("feature {0:?} is not a sketch")]
  NotASketch(FeatureId),
  #[error("invalid parameter name '{0}'")]
  BadParameterName(String),
  #[error("a parameter named '{0}' already exists")]
  DuplicateParameter(String),
  #[error("no parameter '{0}'")]
  NoParameter(String),
  #[error("'{0}' is used by {1}")]
  InUse(String, String),
  #[error("too many features")]
  TooMany,
  #[error("unsupported file version {0}")]
  Version(u32),
  #[error("file error: {0}")]
  File(String),
}

impl Default for Design {
  fn default() -> Self {
    Self::new("Untitled")
  }
}

impl Design {
  pub fn new(name: impl Into<String>) -> Self {
    Design {
      version: FORMAT_VERSION,
      name: name.into(),
      units: LengthUnit::Mm,
      parameters: Vec::new(),
      model_favorites: Vec::new(),
      features: Vec::new(),
      marker: None,
      bodies: Vec::new(),
      sheet_metal_rules: Vec::new(),
      components: Vec::new(),
      drawings: Vec::new(),
      attributes: Vec::new(),
      cam: Vec::new(),
      contact_sets: Vec::new(),
      configurations: config::ConfigTable::default(),
      history_off: false,
      behaviours: Behaviours::current(),
      named_views: Vec::new(),
      face_appearances: Vec::new(),
      timeline_groups: Vec::new(),
      next_id: 1,
    }
  }

  /// How this design frames its origin planes and downward planes.
  pub fn frames(&self) -> Frames {
    Frames::of(&self.behaviours)
  }

  /// Number of features the timeline computes (the marker position).
  pub fn active_len(&self) -> usize {
    self.marker.map(|m| m.min(self.features.len())).unwrap_or(self.features.len())
  }

  pub fn feature(&self, id: FeatureId) -> Option<&Feature> {
    self.features.iter().find(|f| f.id == id)
  }

  pub fn feature_mut(&mut self, id: FeatureId) -> Option<&mut Feature> {
    self.features.iter_mut().find(|f| f.id == id)
  }

  pub fn index_of(&self, id: FeatureId) -> Option<usize> {
    self.features.iter().position(|f| f.id == id)
  }

  pub fn sketch(&self, id: FeatureId) -> Result<&Sketch, DesignError> {
    match &self.feature(id).ok_or(DesignError::NoFeature(id))?.kind {
      FeatureKind::Sketch { sketch, .. } => Ok(sketch),
      _ => Err(DesignError::NotASketch(id)),
    }
  }

  pub fn sketch_mut(&mut self, id: FeatureId) -> Result<&mut Sketch, DesignError> {
    match &mut self.feature_mut(id).ok_or(DesignError::NoFeature(id))?.kind {
      FeatureKind::Sketch { sketch, .. } => Ok(sketch),
      _ => Err(DesignError::NotASketch(id)),
    }
  }

  fn alloc(&mut self) -> u32 {
    let used = self.features.iter().map(|f| f.id.0).max().unwrap_or(0);
    self.next_id = self.next_id.max(used + 1);
    let id = self.next_id;
    self.next_id = self.next_id.saturating_add(1);
    id
  }

  /// Inserts a feature at the timeline marker (the end unless rolled back). Returns its id.
  pub fn add_feature(&mut self, kind: FeatureKind) -> Result<FeatureId, DesignError> {
    if self.features.len() >= MAX_FEATURES {
      return Err(DesignError::TooMany);
    }
    let id = FeatureId(self.alloc());
    let name = self.unique_name(kind.base_name());
    let at = self.active_len();
    self.features.insert(
      at,
      Feature { id, name, suppressed: false, visible: None, targets: Vec::new(), cached: Vec::new(), kind },
    );
    if let Some(m) = self.marker.as_mut() {
      *m += 1;
    }
    Ok(id)
  }

  fn unique_name(&self, base: &str) -> String {
    let n = self.features.iter().filter(|f| f.kind.base_name() == base).count() + 1;
    let mut k = n;
    loop {
      let name = format!("{base}{k}");
      if !self.features.iter().any(|f| f.name == name) {
        return name;
      }
      k += 1;
    }
  }

  /// Sets an attribute, replacing one with the same target, group and name.
  pub fn set_attribute(&mut self, target: AttrTarget, group: &str, name: &str, value: serde_json::Value) {
    match self.attributes.iter_mut().find(|a| a.target == target && a.group == group && a.name == name) {
      Some(a) => a.value = value,
      None => self.attributes.push(Attribute { target, group: group.into(), name: name.into(), value }),
    }
  }

  /// Removes matching attributes (`None` matches any name); returns how many.
  pub fn remove_attributes(&mut self, target: AttrTarget, group: &str, name: Option<&str>) -> usize {
    let before = self.attributes.len();
    self.attributes.retain(|a| !(a.target == target && a.group == group && name.is_none_or(|n| a.name == n)));
    before - self.attributes.len()
  }

  /// Deletes a feature. Features that depend on it directly are deleted too (like Fusion's
  /// "delete dependents"); returns everything removed.
  pub fn remove_feature(&mut self, id: FeatureId) -> Result<Vec<FeatureId>, DesignError> {
    if self.feature(id).is_none() {
      return Err(DesignError::NoFeature(id));
    }
    let mut doomed = vec![id];
    loop {
      let more: Vec<FeatureId> = self
        .features
        .iter()
        .filter(|f| !doomed.contains(&f.id) && f.kind.depends_on().iter().any(|d| doomed.contains(d)))
        .map(|f| f.id)
        .collect();
      if more.is_empty() {
        break;
      }
      doomed.extend(more);
    }
    let before_marker = self.features.iter().take(self.active_len()).filter(|f| doomed.contains(&f.id)).count();
    self.features.retain(|f| !doomed.contains(&f.id));
    self.attributes.retain(|a| a.target.feature().is_none_or(|f| !doomed.contains(&f)));
    if let Some(m) = self.marker.as_mut() {
      *m = m.saturating_sub(before_marker);
    }
    Ok(doomed)
  }

  /// Deletes a sketch but keeps the features that use it, as Fusion's Delete does: each gets a
  /// frozen copy of the sketch (`solved`, on `plane`) to rebuild from, so its bodies stay.
  /// Returns the features kept on the cached geometry.
  pub fn remove_sketch_keep_dependents(
    &mut self,
    id: FeatureId,
    plane: PlaneRef,
    solved: &Sketch,
  ) -> Result<Vec<FeatureId>, DesignError> {
    let f = self.feature(id).ok_or(DesignError::NoFeature(id))?;
    if !matches!(f.kind, FeatureKind::Sketch { .. }) {
      return Err(DesignError::NoFeature(id));
    }
    let mut geometry = solved.clone();
    geometry.constraints.clear();
    let cache = CachedSketch { sketch: id, plane, geometry };
    let mut kept = Vec::new();
    for f in self.features.iter_mut().filter(|f| f.id != id && f.kind.depends_on().contains(&id)) {
      f.cached.retain(|c| c.sketch != id);
      f.cached.push(cache.clone());
      kept.push(f.id);
    }
    let before_marker = self.features.iter().take(self.active_len()).filter(|f| f.id == id).count();
    self.features.retain(|f| f.id != id);
    self.attributes.retain(|a| a.target.feature() != Some(id));
    if let Some(m) = self.marker.as_mut() {
      *m = m.saturating_sub(before_marker);
    }
    Ok(kept)
  }

  /// Moves a feature to a new timeline index, refusing moves that would put it before a feature
  /// it depends on (or after one that depends on it).
  pub fn move_feature(&mut self, id: FeatureId, to: usize) -> Result<(), DesignError> {
    self.check_move(id, to)?;
    let from = self.index_of(id).ok_or(DesignError::NoFeature(id))?;
    let f = self.features.remove(from);
    self.features.insert(to.min(self.features.len()), f);
    Ok(())
  }

  /// Whether [`Design::move_feature`] would accept moving `id` to timeline index `to`, without
  /// moving it. Only the references the timeline records directly (sketches, construction
  /// geometry) are checked; references to bodies need a regeneration to judge.
  pub fn check_move(&self, id: FeatureId, to: usize) -> Result<(), DesignError> {
    let from = self.index_of(id).ok_or(DesignError::NoFeature(id))?;
    let mut order: Vec<FeatureId> = self.features.iter().map(|f| f.id).collect();
    order.remove(from);
    order.insert(to.min(order.len()), id);
    let pos = |f: FeatureId| order.iter().position(|o| *o == f);
    for (i, fid) in order.iter().enumerate() {
      let Some(feat) = self.feature(*fid) else { continue };
      if let Some(dep) = feat.kind.depends_on().into_iter().find(|d| pos(*d).is_some_and(|di| di > i)) {
        let dep_name = self.feature(dep).map(|f| f.name.clone()).unwrap_or_default();
        return Err(DesignError::InUse(dep_name, format!("{}, which would come before it", feat.name)));
      }
    }
    Ok(())
  }

  pub fn set_marker(&mut self, at: Option<usize>) {
    self.marker = at.map(|m| m.min(self.features.len())).filter(|m| *m < self.features.len());
  }

  // ---- parameters ----

  pub fn parameter(&self, name: &str) -> Option<&Parameter> {
    self.parameters.iter().find(|p| p.name == name)
  }

  pub fn add_parameter(&mut self, name: &str, expr: &str, kind: ParamKind, comment: &str) -> Result<(), DesignError> {
    if !fox_expr::is_valid_parameter_name(name) {
      return Err(DesignError::BadParameterName(name.to_string()));
    }
    if self.parameter(name).is_some() {
      return Err(DesignError::DuplicateParameter(name.to_string()));
    }
    if self.parameters.len() >= MAX_PARAMETERS {
      return Err(DesignError::TooMany);
    }
    self.parameters.push(Parameter {
      name: name.to_string(),
      expr: expr.to_string(),
      kind,
      comment: comment.to_string(),
      favorite: false,
    });
    Ok(())
  }

  pub fn set_parameter(&mut self, name: &str, expr: &str) -> Result<(), DesignError> {
    let p = self.parameters.iter_mut().find(|p| p.name == name).ok_or(DesignError::NoParameter(name.to_string()))?;
    p.expr = expr.to_string();
    Ok(())
  }

  /// Removes a parameter unless an expression still refers to it.
  pub fn remove_parameter(&mut self, name: &str) -> Result<(), DesignError> {
    if self.parameter(name).is_none() {
      return Err(DesignError::NoParameter(name.to_string()));
    }
    for p in &self.parameters {
      if p.name != name && fox_expr::identifiers(&p.expr).iter().any(|i| i == name) {
        return Err(DesignError::InUse(name.to_string(), format!("parameter {}", p.name)));
      }
    }
    for f in &self.features {
      if expr_strings(&f.kind).iter().any(|e| fox_expr::identifiers(e).iter().any(|i| i == name)) {
        return Err(DesignError::InUse(name.to_string(), f.name.clone()));
      }
    }
    for r in &self.sheet_metal_rules {
      if [&r.thickness, &r.bend_radius, &r.k_factor, &r.relief_width, &r.relief_depth, &r.gap, &r.corner_relief_size]
        .iter()
        .any(|e| fox_expr::identifiers(e).iter().any(|i| i == name))
      {
        return Err(DesignError::InUse(name.to_string(), format!("sheet metal rule {}", r.name)));
      }
    }
    if let Some(c) = self.configurations.uses_parameter(name) {
      return Err(DesignError::InUse(name.to_string(), format!("configuration {c}")));
    }
    for c in &mut self.configurations.list {
      c.clear(&config::Column::Parameter { name: name.to_string() });
    }
    self.parameters.retain(|p| p.name != name);
    Ok(())
  }

  /// Renames a parameter and rewrites every expression that uses it.
  pub fn rename_parameter(&mut self, old: &str, new: &str) -> Result<(), DesignError> {
    if !fox_expr::is_valid_parameter_name(new) {
      return Err(DesignError::BadParameterName(new.to_string()));
    }
    if self.parameter(new).is_some() {
      return Err(DesignError::DuplicateParameter(new.to_string()));
    }
    let p = self.parameters.iter_mut().find(|p| p.name == old).ok_or(DesignError::NoParameter(old.to_string()))?;
    p.name = new.to_string();
    for p in &mut self.parameters {
      p.expr = rename_ident(&p.expr, old, new);
    }
    for f in &mut self.features {
      for_each_expr(&mut f.kind, &mut |e| *e = rename_ident(e, old, new));
    }
    for r in &mut self.sheet_metal_rules {
      for e in [
        &mut r.thickness,
        &mut r.bend_radius,
        &mut r.k_factor,
        &mut r.relief_width,
        &mut r.relief_depth,
        &mut r.gap,
        &mut r.corner_relief_size,
      ] {
        *e = rename_ident(e, old, new);
      }
    }
    self.configurations.rename_parameter(old, new);
    Ok(())
  }

  // ---- drawings ----

  pub fn drawing(&self, id: u32) -> Option<&drawing::Drawing> {
    self.drawings.iter().find(|d| d.id == id)
  }

  pub fn drawing_mut(&mut self, id: u32) -> Option<&mut drawing::Drawing> {
    self.drawings.iter_mut().find(|d| d.id == id)
  }

  /// A new drawing with the given views; returns its id.
  pub fn add_drawing(&mut self, name: &str, views: Vec<drawing::ViewKind>) -> u32 {
    let id = self.drawings.iter().map(|d| d.id).max().unwrap_or(0) + 1;
    let name = if name.trim().is_empty() { format!("Drawing{id}") } else { name.to_string() };
    self.drawings.push(drawing::Drawing::new(id, name, views));
    id
  }

  // ---- components ----

  pub fn component(&self, id: ComponentId) -> Option<&Component> {
    self.components.iter().find(|c| c.id == id)
  }

  pub fn component_mut(&mut self, id: ComponentId) -> Option<&mut Component> {
    self.components.iter_mut().find(|c| c.id == id)
  }

  /// The component a body belongs to.
  pub fn component_of(&self, body: BodyId) -> Option<&Component> {
    self.components.iter().find(|c| c.bodies.contains(&body))
  }

  /// A new component holding `bodies` (taken out of any component they were in).
  pub fn add_component(&mut self, name: &str, bodies: Vec<BodyId>) -> Result<ComponentId, DesignError> {
    if self.components.len() >= MAX_PARAMETERS {
      return Err(DesignError::TooMany);
    }
    let id = ComponentId(self.components.iter().map(|c| c.id.0).max().unwrap_or(0) + 1);
    for c in &mut self.components {
      c.bodies.retain(|b| !bodies.contains(b));
    }
    let name = if name.trim().is_empty() { format!("Component{}", id.0) } else { name.to_string() };
    self.components.push(Component {
      id,
      name,
      bodies,
      grounded: false,
      material: String::new(),
      parent: None,
      part_number: String::new(),
      description: String::new(),
      sheet_metal: false,
    });
    Ok(id)
  }

  /// Deletes a component (its bodies go back to the root) unless a joint uses it.
  pub fn remove_component(&mut self, id: ComponentId) -> Result<(), DesignError> {
    let c = self.component(id).ok_or(DesignError::File(format!("no component {}", id.0)))?;
    for f in &self.features {
      if let FeatureKind::Joint { moving, fixed, .. } = &f.kind
        && (*moving == id || *fixed == Some(id))
      {
        return Err(DesignError::InUse(c.name.clone(), f.name.clone()));
      }
      if let FeatureKind::JointOrigin { component, .. } = &f.kind
        && *component == id
      {
        return Err(DesignError::InUse(c.name.clone(), f.name.clone()));
      }
      if let FeatureKind::RigidGroup { components, .. } = &f.kind
        && components.contains(&id)
      {
        return Err(DesignError::InUse(c.name.clone(), f.name.clone()));
      }
      if let FeatureKind::AlignConstraint { moving, fixed, .. } = &f.kind
        && (*moving == id || *fixed == Some(id))
      {
        return Err(DesignError::InUse(c.name.clone(), f.name.clone()));
      }
      if let FeatureKind::PlaneConstraint { moving, fixed, .. } = &f.kind
        && (*moving == id || *fixed == Some(id))
      {
        return Err(DesignError::InUse(c.name.clone(), f.name.clone()));
      }
    }
    // its sub-components move up a level
    let up = self.component(id).and_then(|c| c.parent);
    for c in self.components.iter_mut().filter(|c| c.parent == Some(id)) {
      c.parent = up;
    }
    self.components.retain(|c| c.id != id);
    for cs in &mut self.contact_sets {
      cs.components.retain(|c| *c != id);
    }
    Ok(())
  }

  // ---- sheet metal rules ----

  fn clone_rules(&self) -> Rules {
    Rules { list: self.sheet_metal_rules.clone() }
  }

  /// The rule a sheet metal feature uses: by name, or the default (the first rule, else the
  /// built-in one) for an empty name.
  pub fn sheet_rule(&self, name: &str) -> Option<SheetMetalRule> {
    self.clone_rules().get(name).ok()
  }

  /// Adds a rule or replaces the one with the same name.
  pub fn set_sheet_rule(&mut self, rule: SheetMetalRule) -> Result<(), DesignError> {
    if rule.name.trim().is_empty() {
      return Err(DesignError::BadParameterName(rule.name));
    }
    match self.sheet_metal_rules.iter_mut().find(|r| r.name == rule.name) {
      Some(r) => *r = rule,
      None => {
        if self.sheet_metal_rules.len() >= MAX_PARAMETERS {
          return Err(DesignError::TooMany);
        }
        self.sheet_metal_rules.push(rule)
      }
    }
    Ok(())
  }

  /// Removes a rule unless a feature names it.
  pub fn remove_sheet_rule(&mut self, name: &str) -> Result<(), DesignError> {
    if !self.sheet_metal_rules.iter().any(|r| r.name == name) {
      return Err(DesignError::NoParameter(name.to_string()));
    }
    let is_default = self.sheet_metal_rules.first().is_some_and(|r| r.name == name);
    for f in &self.features {
      if f.kind.sheet_rule().is_some_and(|r| r == name || (r.is_empty() && is_default)) {
        return Err(DesignError::InUse(name.to_string(), f.name.clone()));
      }
    }
    self.sheet_metal_rules.retain(|r| r.name != name);
    Ok(())
  }

  // ---- bodies ----

  pub fn body_settings(&self, id: BodyId) -> Option<&BodySettings> {
    self.bodies.iter().find(|b| b.body == id)
  }

  pub fn body_settings_mut(&mut self, id: BodyId) -> &mut BodySettings {
    if let Some(i) = self.bodies.iter().position(|b| b.body == id) {
      &mut self.bodies[i]
    } else {
      self.bodies.push(BodySettings { body: id, name: None, visible: true, color: None });
      let last = self.bodies.len() - 1;
      &mut self.bodies[last]
    }
  }

  // ---- files ----

  pub fn to_json(&self) -> String {
    serde_json::to_string_pretty(self).unwrap_or_default()
  }

  pub fn from_json(s: &str) -> Result<Design, DesignError> {
    let mut d: Design = serde_json::from_str(s).map_err(|e| DesignError::File(e.to_string()))?;
    if d.version > FORMAT_VERSION {
      return Err(DesignError::Version(d.version));
    }
    if d.features.len() > MAX_FEATURES || d.parameters.len() > MAX_PARAMETERS {
      return Err(DesignError::TooMany);
    }
    // a hand-edited rho outside the range would give the conic an infinite or negative weight
    for f in &d.features {
      if let FeatureKind::Sketch { sketch, .. } = &f.kind {
        for c in &sketch.curves {
          if let fox_sketch::CurveKind::Conic { rho, .. } = c.kind
            && !fox_sketch::RHO_RANGE.contains(&rho)
          {
            return Err(DesignError::File(format!("conic rho {rho} (it must be 0.01 to 0.99)")));
          }
        }
      }
    }
    d.migrate();
    Ok(d)
  }

  /// Brings a design read from an older file up to [`FORMAT_VERSION`], so it rebuilds as it did.
  fn migrate(&mut self) {
    if self.version < 2 {
      // a symmetric thicken's thickness was the total, half on each side; now it is per side
      for f in &mut self.features {
        if let FeatureKind::Thicken { thickness, symmetric: true, .. } = &mut f.kind {
          *thickness = format!("({}) / 2", thickness.trim());
        }
      }
    }
    self.version = FORMAT_VERSION;
  }

  pub fn save(&self, path: &std::path::Path) -> Result<(), DesignError> {
    std::fs::write(path, self.to_json()).map_err(|e| DesignError::File(e.to_string()))
  }

  pub fn load(path: &std::path::Path) -> Result<Design, DesignError> {
    let meta = std::fs::metadata(path).map_err(|e| DesignError::File(e.to_string()))?;
    if meta.len() > 512 * 1024 * 1024 {
      return Err(DesignError::File("file is too large".into()));
    }
    let s = std::fs::read_to_string(path).map_err(|e| DesignError::File(e.to_string()))?;
    Design::from_json(&s)
  }
}

/// The rules a regeneration resolves sheet metal features against.
pub(crate) struct Rules {
  list: Vec<SheetMetalRule>,
}

impl Rules {
  pub(crate) fn get(&self, name: &str) -> Result<SheetMetalRule, String> {
    if name.is_empty() {
      return Ok(self.list.first().cloned().unwrap_or_default());
    }
    match self.list.iter().find(|r| r.name == name) {
      Some(r) => Ok(r.clone()),
      None if name == "Default" && self.list.is_empty() => Ok(SheetMetalRule::default()),
      None => Err(format!("there is no sheet metal rule '{name}'")),
    }
  }
}

/// Applies `f` to every expression string inside a feature.
pub fn for_each_expr(kind: &mut FeatureKind, f: &mut dyn FnMut(&mut String)) {
  match kind {
    FeatureKind::Sketch { sketch, .. } => {
      for c in &mut sketch.constraints {
        if let Some(d) = c.kind.dim_mut() {
          f(&mut d.expr);
        }
      }
    }
    FeatureKind::Extrude { extent, .. } => {
      f(&mut extent.distance);
      f(&mut extent.distance2);
      for e in [&mut extent.taper, &mut extent.taper2, &mut extent.thin, &mut extent.start] {
        if !e.is_empty() {
          f(e);
        }
      }
    }
    FeatureKind::Revolve { angle, angle2, .. } => {
      f(angle);
      if let Some(a) = angle2 {
        f(a);
      }
    }
    FeatureKind::Fillet { radius, radius2, setback, radii, sets, distance2, .. } => {
      f(radius);
      for set in sets {
        f(&mut set.radius);
      }
      for r in [radius2, setback, distance2].into_iter().flatten() {
        f(r);
      }
      for p in radii {
        f(&mut p.radius);
      }
    }
    FeatureKind::Chamfer { distance, distance2, angle, .. } => {
      f(distance);
      for e in [distance2, angle].into_iter().flatten() {
        f(e);
      }
    }
    FeatureKind::OffsetFaces { distance, .. } => f(distance),
    FeatureKind::SplitByBody { .. } => {}
    FeatureKind::Shell { thickness, thickness_out, .. } => {
      f(thickness);
      if !thickness_out.is_empty() {
        f(thickness_out);
      }
    }
    FeatureKind::Primitive { shape, .. } => match shape {
      Primitive::Box { width, depth, height } => {
        f(width);
        f(depth);
        f(height);
      }
      Primitive::Cylinder { radius, height } => {
        f(radius);
        f(height);
      }
      Primitive::Sphere { radius } => f(radius),
      Primitive::Torus { major, minor } => {
        f(major);
        f(minor);
      }
    },
    FeatureKind::ConstructionPlane { offset, .. } => f(offset),
    FeatureKind::RectangularPattern { count, spacing, count2, spacing2, .. } => {
      f(count);
      f(spacing);
      f(count2);
      f(spacing2);
    }
    FeatureKind::CircularPattern { count, angle, .. } => {
      f(count);
      f(angle);
    }
    FeatureKind::Move { translate, angle, rotate, .. } => {
      for t in translate.iter_mut().chain(rotate.iter_mut().flatten()) {
        f(t);
      }
      f(angle);
    }
    FeatureKind::ContourFlange { distance, .. } => f(distance),
    FeatureKind::EdgeFlange { height, angle, offset_start, offset_end, .. } => {
      f(height);
      f(angle);
      f(offset_start);
      f(offset_end);
    }
    FeatureKind::Hole { diameter, depth, tip_angle, head_diameter, head_depth, head_angle, .. } => {
      for e in [diameter, depth, tip_angle, head_diameter, head_depth, head_angle] {
        f(e);
      }
    }
    FeatureKind::Draft { angle, angle2, .. } => {
      f(angle);
      if let Some(a) = angle2 {
        f(a);
      }
    }
    FeatureKind::Bend { angle, .. } => f(angle),
    FeatureKind::Joint { angle, offset, rotation, slide, slide2, pitch, limits, lead, .. } => {
      for e in [angle, offset, rotation, slide, slide2, pitch, lead] {
        f(e);
      }
      for (lo, hi) in [&mut limits.rotation, &mut limits.slide].into_iter().flatten() {
        f(lo);
        f(hi);
      }
    }
    FeatureKind::CopyComponent { translate, .. } => {
      for t in translate.iter_mut() {
        f(t);
      }
    }
    FeatureKind::Rip { gap, .. } => {
      if !gap.is_empty() {
        f(gap)
      }
    }
    FeatureKind::Hem { length, gap, .. } => {
      f(length);
      f(gap);
    }
    FeatureKind::PlaneBy { def } => {
      for e in def.exprs() {
        f(e);
      }
    }
    FeatureKind::ConstructionPoint { def } => {
      for e in def.exprs() {
        f(e);
      }
    }
    FeatureKind::Thread { pitch, length, offset, .. } => {
      for e in [pitch, length, offset] {
        if !e.is_empty() {
          f(e);
        }
      }
    }
    FeatureKind::Coil { diameter, pitch, height, section_size, .. } => {
      for e in [diameter, pitch, height, section_size] {
        f(e);
      }
    }
    FeatureKind::Pipe { diameter, wall, distance, .. } => {
      f(diameter);
      if !wall.is_empty() {
        f(wall);
      }
      if !distance.is_empty() {
        f(distance);
      }
    }
    FeatureKind::Rib { thickness, depth, .. } => {
      f(thickness);
      if !depth.is_empty() {
        f(depth);
      }
    }
    FeatureKind::Scale { factor, factors, .. } => {
      f(factor);
      for e in factors.iter_mut().flatten() {
        f(e);
      }
    }
    FeatureKind::Thicken { thickness, .. } => f(thickness),
    FeatureKind::SurfaceOffset { distance, face_distances, .. } => {
      f(distance);
      for fd in face_distances {
        f(&mut fd.distance);
      }
    }
    FeatureKind::Ruled { .. } | FeatureKind::Trim { .. } => {}
    FeatureKind::SurfaceExtrude { distance, .. } => f(distance),
    FeatureKind::SurfaceRevolve { angle, .. } => f(angle),
    FeatureKind::SurfaceSweep { twist, .. } => {
      if !twist.is_empty() {
        f(twist);
      }
    }
    FeatureKind::SurfaceLoft { .. } => {}
    FeatureKind::Stitch { tolerance, .. } => f(tolerance),
    FeatureKind::Mesh { amount, .. } => f(amount),
    FeatureKind::ConstructionAxis { .. }
    | FeatureKind::SplitFace { .. }
    | FeatureKind::RemoveFaces { .. }
    | FeatureKind::Patch { .. }
    | FeatureKind::Unstitch { .. }
    | FeatureKind::ReverseNormal { .. } => {}
    FeatureKind::Loft { .. } | FeatureKind::SplitBody { .. } => {}
    FeatureKind::Sweep { twist, taper, .. } => {
      if !twist.is_empty() {
        f(twist);
      }
      if !taper.is_empty() {
        f(taper);
      }
    }
    FeatureKind::Mirror { .. }
    | FeatureKind::Combine { .. }
    | FeatureKind::Import { .. }
    | FeatureKind::RemoveBodies { .. }
    | FeatureKind::Flange { .. }
    | FeatureKind::ConvertToSheetMetal { .. }
    | FeatureKind::Unfold { .. }
    | FeatureKind::Refold { .. } => {}
    FeatureKind::Emboss { depth, horizontal, vertical, rotation, .. } => {
      f(depth);
      f(horizontal);
      f(vertical);
      f(rotation);
    }
    FeatureKind::BoundaryFill { .. } | FeatureKind::ReplaceFace { .. } => {}
    FeatureKind::SurfaceExtend { distance, .. } => f(distance),
    FeatureKind::Untrim { .. } | FeatureKind::TrimWith { .. } => {}
    FeatureKind::LoftedFlange { bends, .. } => f(bends),
    FeatureKind::MiterFlange { .. } => {}
    FeatureKind::JoinByBend { bend_radius, .. } => {
      if !bend_radius.is_empty() {
        f(bend_radius)
      }
    }
    FeatureKind::SheetFillet { radius, .. } => f(radius),
    FeatureKind::SheetChamfer { distance, distance2, .. } => {
      f(distance);
      if let Some(d) = distance2 {
        f(d);
      }
    }
    FeatureKind::JointOrigin { angle, offset, offset_x, offset_y, .. } => {
      f(angle);
      f(offset);
      f(offset_x);
      f(offset_y);
    }
    FeatureKind::MotionLink { leader_value, follower_value, .. } => {
      f(leader_value);
      f(follower_value);
    }
    FeatureKind::InsertDesign { translate, .. } => {
      for t in translate.iter_mut() {
        f(t);
      }
    }
    FeatureKind::RigidGroup { .. } => {}
    FeatureKind::BaseFeature { .. } => {}
    FeatureKind::CamFollower { clearance, .. } => f(clearance),
    FeatureKind::Form { .. } => {}
    FeatureKind::AlignConstraint { offset, .. } => f(offset),
    FeatureKind::PlaneConstraint { offset, .. } => f(offset),
    FeatureKind::BoundingSolid { offset, sides, .. } => {
      f(offset);
      for e in sides {
        f(e);
      }
    }
    FeatureKind::ProjectToSurface { .. } => {}
    FeatureKind::PathPattern { count, distance, start, .. } => {
      f(count);
      f(distance);
      f(start);
    }
    FeatureKind::Arrange { length, width, spacing, frame, .. } => {
      for e in [length, width, spacing, frame] {
        f(e);
      }
    }
    FeatureKind::FullRoundFillet { .. } => {}
    FeatureKind::IntersectionCurve { .. } => {}
    FeatureKind::Canvas { .. } => {}
  }
}

/// All expression strings in a feature (used for "is this parameter used?").
pub fn expr_strings(kind: &FeatureKind) -> Vec<String> {
  let mut v = kind.clone();
  let mut out = Vec::new();
  for_each_expr(&mut v, &mut |e| out.push(e.clone()));
  out
}

/// Replaces whole-word identifier `old` with `new` in an expression.
fn rename_ident(expr: &str, old: &str, new: &str) -> String {
  let mut out = String::with_capacity(expr.len());
  let chars: Vec<char> = expr.chars().collect();
  let mut i = 0;
  while i < chars.len() {
    let c = chars[i];
    if c.is_alphabetic() || c == '_' {
      let start = i;
      while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
        i += 1;
      }
      let word: String = chars[start..i].iter().collect();
      out.push_str(if word == old { new } else { &word });
    } else {
      out.push(c);
      i += 1;
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn add_remove_features_with_dependents() {
    let mut d = Design::new("t");
    let s = d
      .add_feature(FeatureKind::Sketch {
        plane: PlaneRef::Origin { plane: OriginPlane::Xy },
        sketch: Sketch::new(),
        links: Vec::new(),
      })
      .unwrap();
    let e = d
      .add_feature(FeatureKind::Extrude {
        sketch: s,
        profiles: vec![],
        extent: Extent::distance("10"),
        operation: Operation::NewBody,
      })
      .unwrap();
    assert_eq!(d.feature(s).unwrap().name, "Sketch1");
    assert_eq!(d.feature(e).unwrap().name, "Extrude1");
    assert!(d.move_feature(e, 0).is_err(), "extrude can't move before its sketch");
    let removed = d.remove_feature(s).unwrap();
    assert_eq!(removed.len(), 2);
    assert!(d.features.is_empty());
  }

  #[test]
  fn marker_inserts_before_rolled_back_features() {
    let mut d = Design::new("t");
    let a = d.add_feature(FeatureKind::RemoveBodies { bodies: vec![] }).unwrap();
    let b = d.add_feature(FeatureKind::RemoveBodies { bodies: vec![] }).unwrap();
    d.set_marker(Some(1));
    let c = d.add_feature(FeatureKind::RemoveBodies { bodies: vec![] }).unwrap();
    assert_eq!(d.features.iter().map(|f| f.id).collect::<Vec<_>>(), vec![a, c, b]);
    assert_eq!(d.active_len(), 2);
    d.remove_feature(a).unwrap();
    assert_eq!(d.active_len(), 1);
  }

  #[test]
  fn parameters_rename_and_in_use() {
    let mut d = Design::new("t");
    d.add_parameter("width", "40 mm", ParamKind::Length, "").unwrap();
    d.add_parameter("half", "width / 2", ParamKind::Length, "").unwrap();
    assert!(d.add_parameter("width", "1", ParamKind::Length, "").is_err());
    assert!(d.add_parameter("mm", "1", ParamKind::Length, "").is_err());
    let s = d
      .add_feature(FeatureKind::Sketch {
        plane: PlaneRef::Origin { plane: OriginPlane::Xy },
        sketch: Sketch::new(),
        links: Vec::new(),
      })
      .unwrap();
    d.add_feature(FeatureKind::Extrude {
      sketch: s,
      profiles: vec![],
      extent: Extent::distance("width * 2"),
      operation: Operation::NewBody,
    })
    .unwrap();
    assert!(matches!(d.remove_parameter("width"), Err(DesignError::InUse(..))));
    d.rename_parameter("width", "w").unwrap();
    assert_eq!(d.parameter("half").unwrap().expr, "w / 2");
    let FeatureKind::Extrude { extent, .. } = &d.features[1].kind else { panic!() };
    assert_eq!(extent.distance, "w * 2");
    d.remove_parameter("half").unwrap();
  }

  #[test]
  fn json_round_trip_and_hostile_files() {
    let mut d = Design::new("Bracket");
    d.add_parameter("t", "3", ParamKind::Length, "thickness").unwrap();
    d.add_feature(FeatureKind::Sketch {
      plane: PlaneRef::Origin { plane: OriginPlane::Xz },
      sketch: Sketch::new(),
      links: Vec::new(),
    })
    .unwrap();
    let back = Design::from_json(&d.to_json()).unwrap();
    assert_eq!(back, d);
    assert!(Design::from_json("{").is_err());
    assert!(Design::from_json("{\"version\": 99, \"name\": \"x\"}").is_err());
    assert!(Design::from_json("[]").is_err());
  }

  #[test]
  fn rename_ident_whole_words_only() {
    assert_eq!(rename_ident("w + width*w2 - w", "w", "x"), "x + width*w2 - x");
  }

  #[test]
  fn behaviour_flags_new_legacy_and_unknown() {
    // a new design and a new sketch get every current behaviour
    assert_eq!(Design::new("n").behaviours, Behaviours::current());
    // a file from before behaviour flags has none, in the design and in its sketches
    let old = r#"{"version": 2, "name": "old", "features": [{"id": 1, "name": "Sketch1",
      "kind": {"type": "sketch", "plane": {"type": "origin", "plane": "xy"},
      "sketch": {"points": [], "curves": [], "constraints": [], "next_id": 1}}}]}"#;
    let d = Design::from_json(old).unwrap();
    assert!(d.behaviours.is_empty());
    let FeatureKind::Sketch { sketch, .. } = &d.features[0].kind else { panic!("not a sketch") };
    assert!(sketch.behaviours.is_empty());
    // a sketch added to an old design is new geometry: it gets the current set, not the design's
    let mut d = d;
    let s = d
      .add_feature(FeatureKind::Sketch {
        plane: PlaneRef::Origin { plane: OriginPlane::Xy },
        sketch: Sketch::new(),
        links: Vec::new(),
      })
      .unwrap();
    let Some(FeatureKind::Sketch { sketch, .. }) = d.feature(s).map(|f| &f.kind) else { panic!("no sketch") };
    assert_eq!(sketch.behaviours, Behaviours::current());
    // names from a newer build survive a load and save, on the design and on a sketch
    let mut d = Design::new("newer");
    d.behaviours.insert("future_regen_change");
    let mut sk = Sketch::new();
    sk.behaviours.insert("future_profile_change");
    d.add_feature(FeatureKind::Sketch {
      plane: PlaneRef::Origin { plane: OriginPlane::Xy },
      sketch: sk,
      links: Vec::new(),
    })
    .unwrap();
    let json = d.to_json();
    let back = Design::from_json(&json).unwrap();
    assert_eq!(back, d);
    assert_eq!(Design::from_json(&back.to_json()).unwrap(), d);
    assert!(back.behaviours.iter().any(|n| n == "future_regen_change"));
    assert!(json.contains("future_profile_change"));
    // with no current behaviours, a legacy design saves without the field (files unchanged)
    if behaviour_current_is_empty() {
      assert!(!Design::new("plain").to_json().contains("behaviours"));
    }
  }

  fn behaviour_current_is_empty() -> bool {
    fox_sketch::behaviour::CURRENT.is_empty()
  }
}
