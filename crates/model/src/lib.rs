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
pub mod explode;
pub mod fillet;
pub mod form;
pub mod generative;
pub mod groups;
pub mod include;
pub mod mechanism;
pub mod meshops;
pub mod meshprep;
pub mod meshtools;
pub mod naming;
pub mod newcomp;
pub mod pathpattern;
pub mod pcb;
pub mod plot;
pub mod refs;
pub mod regen;
pub mod render;
pub mod sheet;
pub mod sheet2;
pub mod sheet3;
pub mod sim;
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
/// - 3: the built-in sheet metal rule is Fusion's ".063 Aluminum" (it was a 1 mm "Default").
pub const FORMAT_VERSION: u32 = 3;
/// Hostile-file guards.
pub const MAX_FEATURES: usize = 10_000;
pub const MAX_PARAMETERS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FeatureId(pub u32);

/// Stable body identity: assigned by the feature that created the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BodyId(pub u32);

/// A 2D cut kept in the design: its source (sketch, faces or a sheet metal body's flat pattern),
/// machine, process, overrides, lead points and nesting, as the parameters of `cut.gcode`. The
/// cut itself is planned from them whenever it is needed, so it follows the design.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CutJob {
  pub id: u32,
  pub name: String,
  #[serde(default)]
  pub params: serde_json::Value,
}

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
  /// 2D cut jobs (laser, plasma, waterjet) kept with the design and recomputed as it changes.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub cut_jobs: Vec<CutJob>,
  /// Generative design studies.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub generative: Vec<generative::Study>,
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
  /// The Electronics workspace's design: schematic, board and libraries (see `fox_ecad`).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub electronics: Option<Box<fox_ecad::Project>>,
  /// The exploded view shown (`view.explode`): display only, never in the timeline.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub exploded_view: Option<explode::ExplodedView>,
  /// The Render workspace: materials, environment, camera, settings and the render gallery.
  #[serde(default = "render::RenderDoc::saved", skip_serializing_if = "render::RenderDoc::is_empty")]
  pub render: render::RenderDoc,
  /// Simulation studies (the Simulation workspace).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub sim: Vec<sim::Study>,
  /// Fusion's design type (Part, Assembly or Hybrid Design): which toolbar tabs the Design
  /// workspace shows and what Document Settings names it.
  #[serde(default, skip_serializing_if = "DesignType::is_part")]
  pub design_type: DesignType,
  /// The sheet metal flat pattern (Create Flat Pattern; the browser's Flat Pattern row).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub flat_pattern: Option<FlatPatternDef>,
  /// The root component's physical material (Preferences > Material's default for new designs):
  /// what bodies with no material of their own or of their component are made of; empty = steel.
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub material: String,
  #[serde(default)]
  next_id: u32,
}

/// Fusion's design types. A Part Design models one part (SOLID, SURFACE, MESH, ... tabs); an
/// Assembly Design brings components together (the ASSEMBLY tab); a Hybrid Design does both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignType {
  #[default]
  Part,
  Assembly,
  Hybrid,
}

impl DesignType {
  pub fn is_part(&self) -> bool {
    *self == DesignType::Part
  }

  pub fn parse(s: &str) -> Option<DesignType> {
    match s {
      "part" => Some(DesignType::Part),
      "assembly" => Some(DesignType::Assembly),
      "hybrid" => Some(DesignType::Hybrid),
      _ => None,
    }
  }

  pub fn key(self) -> &'static str {
    match self {
      DesignType::Part => "part",
      DesignType::Assembly => "assembly",
      DesignType::Hybrid => "hybrid",
    }
  }

  /// "Part Design", as the browser's Document Settings shows it.
  pub fn label(self) -> &'static str {
    match self {
      DesignType::Part => "Part Design",
      DesignType::Assembly => "Assembly Design",
      DesignType::Hybrid => "Hybrid Design",
    }
  }
}

/// A flat pattern: the sheet metal body laid flat on its stationary face. Fusion keeps one per
/// component; it is not a timeline feature and changes nothing the timeline builds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlatPatternDef {
  pub body: BodyId,
  /// The stationary face, found again after edits.
  pub face: FaceRef,
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

/// A sheet metal rule, as in Fusion: the material settings sheet metal features share. Every
/// value but the thickness may refer to `Thickness`; an empty value takes Fusion's default
/// expression (see [`sheet::eval_rule`]).
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
  /// Material narrower than this left beside a relief is removed with it; empty = twice the
  /// thickness.
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub relief_remnant: String,
  /// Gap left at mitered corners and rips; empty = half the thickness.
  #[serde(default)]
  pub gap: String,
  /// Notch cut where two flanges' bends meet at a corner (Fusion's 2-Bend Intersection).
  #[serde(default)]
  pub corner_relief: CornerRelief,
  /// Corner relief size (square side / round diameter / tear width); empty = default, see
  /// [`sheet::SheetInfo::corner_relief_size`].
  #[serde(default)]
  pub corner_relief_size: String,
  /// Where a 2-bend corner relief is centred.
  #[serde(default, skip_serializing_if = "is_default")]
  pub corner_placement: CornerPlacement,
  /// What is cut where three bends meet (Fusion's 3-Bend Intersection).
  #[serde(default, skip_serializing_if = "is_default")]
  pub corner3_relief: Corner3Relief,
  /// Radius of a Round with Radius 3-bend relief; empty = the thickness.
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub corner3_radius: String,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
  *v == T::default()
}

/// The library's default rule, which a design's first sheet metal feature takes.
pub const DEFAULT_SHEET_RULE: &str = ".063 Aluminum";

impl SheetMetalRule {
  /// A rule with Fusion's default expressions: `thickness` and `bend_radius` as given, the rest
  /// in terms of the thickness (round reliefs, round 2-bend and round-with-radius 3-bend corner
  /// reliefs).
  pub fn library(name: &str, thickness: &str, bend_radius: &str, k_factor: &str) -> Self {
    SheetMetalRule {
      name: name.into(),
      thickness: thickness.into(),
      bend_radius: bend_radius.into(),
      k_factor: k_factor.into(),
      relief: ReliefShape::Round,
      relief_width: "Thickness".into(),
      relief_depth: "Thickness * 0.5".into(),
      relief_remnant: "Thickness * 2.0".into(),
      gap: "Thickness".into(),
      corner_relief: CornerRelief::Round,
      corner_relief_size: "Thickness * 4.0".into(),
      corner_placement: CornerPlacement::Intersection,
      corner3_relief: Corner3Relief::RoundWithRadius,
      corner3_radius: "Thickness".into(),
    }
  }
}

impl SheetMetalRule {
  /// A plain rule, 1 mm thick with a 1 mm bend radius and K 0.44, whose reliefs and gap follow
  /// the thickness (the empty values) and with no corner relief: what `sheet.rule` starts a new
  /// rule from, and the built-in rule of designs saved before format 3.
  pub fn plain(name: &str) -> Self {
    SheetMetalRule {
      name: name.into(),
      thickness: "1 mm".into(),
      bend_radius: "1 mm".into(),
      k_factor: "0.44".into(),
      relief: ReliefShape::Rectangular,
      relief_width: String::new(),
      relief_depth: String::new(),
      relief_remnant: String::new(),
      gap: String::new(),
      corner_relief: CornerRelief::None,
      corner_relief_size: String::new(),
      corner_placement: CornerPlacement::Intersection,
      corner3_relief: Corner3Relief::NoReplacement,
      corner3_radius: String::new(),
    }
  }
}

impl Default for SheetMetalRule {
  /// The library default, ".063 Aluminum".
  fn default() -> Self {
    SheetMetalRule::library(DEFAULT_SHEET_RULE, "0.063 in", "0.035 in", "0.4")
  }
}

/// Fusion's sheet metal rule library, in its order. [`DEFAULT_SHEET_RULE`] is the default.
pub fn sheet_rule_library() -> Vec<SheetMetalRule> {
  let r = SheetMetalRule::library;
  vec![
    r("Steel (mm)", "2.5 mm", "Thickness", "0.44"),
    r("Stainless Steel (mm)", "2.5 mm", "Thickness", "0.44"),
    r("Aluminum (mm)", "2.5 mm", "Thickness", "0.4"),
    r("Steel (in)", "0.1 in", "Thickness", "0.44"),
    r("Stainless Steel (in)", "0.1 in", "Thickness", "0.44"),
    r("Aluminum (in)", "0.1 in", "Thickness", "0.4"),
    r("Stainless Steel (0.0329 in)", "0.0329 in", "Thickness", "0.44"),
    r(".050 Aluminum", "0.05 in", "Thickness", "0.4"),
    r("18 Gauge Steel", "0.0478 in", "Thickness", "0.44"),
    r(".125 Aluminum", "0.125 in", "Thickness", "0.4"),
    SheetMetalRule::default(),
    r(".125 Steel", "0.125 in", "Thickness", "0.44"),
    r("10 Gauge Steel", "0.1345 in", "Thickness", "0.44"),
    r(".063 Steel", "0.063 in", "Thickness", "0.44"),
    r("16 Gauge Steel", "0.0598 in", "Thickness", "0.44"),
    r("5/32 Single Wall Cardboard", "0.15625 in", "Thickness", "0.5"),
    r(".25 Aluminum", "0.25 in", "Thickness", "0.4"),
    r("Stainless Steel (18 Gauge)", "0.05 in", "Thickness", "0.44"),
    r("Steel (in)(1)", "0.1 in", "Thickness", "0.44"),
  ]
}

/// A sheet metal rule library: the rules a design can take, and the one a design's first sheet
/// metal feature takes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SheetRuleLibrary {
  pub rules: Vec<SheetMetalRule>,
  /// The default rule's name.
  pub default: String,
}

impl Default for SheetRuleLibrary {
  /// Fusion's library ([`sheet_rule_library`]).
  fn default() -> Self {
    SheetRuleLibrary { rules: sheet_rule_library(), default: DEFAULT_SHEET_RULE.into() }
  }
}

impl SheetRuleLibrary {
  pub fn get(&self, name: &str) -> Option<&SheetMetalRule> {
    self.rules.iter().find(|r| r.name == name)
  }

  /// The default rule (Fusion's default if the named one is gone).
  pub fn default_rule(&self) -> SheetMetalRule {
    self.get(&self.default).or_else(|| self.rules.first()).cloned().unwrap_or_default()
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

/// Where a 2-bend corner relief is centred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CornerPlacement {
  /// On the point where the bends' outer lines cross.
  #[default]
  Intersection,
  /// Tangent to the bends' ends.
  Tangent,
}

/// What is cut where three bends meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Corner3Relief {
  #[default]
  NoReplacement,
  Intersection,
  FullRound,
  RoundWithRadius,
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
  /// The body's own physical material; it wins over its component's.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub material: Option<String>,
}

fn yes() -> bool {
  true
}

fn is_yes(b: &bool) -> bool {
  *b
}

/// Split Face's Split Type: how sketch curves split the faces.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitFaceType {
  /// Split with Surface: the curves swept along their sketch's normal.
  #[default]
  Surface,
  /// Along Vector: the curves swept along a direction.
  AlongVector,
  /// Closest Point: each point of the curves dropped onto the nearest point of the faces.
  ClosestPoint,
}

impl SplitFaceType {
  fn is_surface(&self) -> bool {
    *self == SplitFaceType::Surface
  }
}

/// Sketch curves a Split Face splits with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitCurves {
  pub sketch: FeatureId,
  pub curves: Vec<fox_sketch::CurveId>,
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
  /// New Component features: the component they created, which holds the bodies they make.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub component: Option<ComponentId>,
  pub kind: FeatureKind,
  /// The component the feature was made in (the active one then; `None` = the top level), as
  /// Fusion's timeline items belong to a component.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub owner: Option<ComponentId>,
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
  /// A new body in a new component the feature creates (see [`newcomp`]).
  NewComponent,
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
  /// Up to the plane through a point (`to_point`: a vertex, point or construction point)
  /// parallel to the sketch plane, plus `distance` past it.
  ToVertex,
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
  /// The point a `to_vertex` extent stops at.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub to_point: Option<construct::PointRef>,
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
      to_point: None,
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
  /// With two rails: the profile stretches across them only (its other direction keeps its size).
  Stretch,
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
    /// The sketch whose profiles are extruded; `None` when `faces` are.
    sketch: Option<FeatureId>,
    /// Interior points (sketch coordinates) of the chosen regions; empty = every region.
    #[serde(default)]
    profiles: Vec<[f64; 2]>,
    /// Planar body faces extruded as profiles instead of a sketch's, as Fusion's Extrude takes
    /// them; they extrude along the first face's normal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    faces: Vec<FaceRef>,
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
    /// Fusion's Corner Type where three chamfered edges meet: "miter" or "blend" (None is
    /// Chamfer). Kept for the dialog; the kernel closes every corner with Chamfer's flat patch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    corner: Option<String>,
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
  /// BRep text, and/or triangle meshes (STL, OBJ, 3MF, PLY) that stay mesh bodies named after the file.
  Import {
    file_name: String,
    #[serde(default)]
    breps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    meshes: Vec<MeshData>,
  },
  RemoveBodies {
    bodies: Vec<BodyId>,
    /// Components removed whole (Fusion's REMOVE takes components too): their bodies, and
    /// those of the components below them, go, and the browser drops their rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    components: Vec<ComponentId>,
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
    /// A second guide rail (with `rail`): each section is anchored on the first rail and turned,
    /// and stretched or scaled by `profile_scaling`, to keep reaching the second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rail2: Option<PathRef>,
    /// A guide surface: the profile turns about the path to keep its angle to the surface normal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    guide_surface: Option<FaceRef>,
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
    /// The sketch was made to place this hole (Fusion's single hole on a face): the browser
    /// doesn't list it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    placed: bool,
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
    /// Fusion's Orientation Center: the sheet straddles the sketch plane (`flip` is ignored).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    center: bool,
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
  /// change (edit the original and every copy follows), placed by `transform` (a rigid
  /// placement, column-major) and then `translate`. Joints can then position the copy's
  /// component like any other. A source holding no bodies of its own (a sub-assembly) gives none.
  CopyComponent {
    source: ComponentId,
    #[serde(default = "zeros")]
    translate: [String; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transform: Option<[f64; 16]>,
    /// The component holding the copies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    component: Option<ComponentId>,
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
    /// Taper angle (Fusion's Angle): positive widens the coil as it rises; empty is none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    angle: String,
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
    /// The whole thickness on one side of the lines (Fusion's one-side Thickness Direction)
    /// instead of half on each.
    #[serde(default)]
    one_side: bool,
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
  /// Splits faces with splitting tools (a plane, faces of bodies or sketch curves); the body stays
  /// one piece (Fusion's Split Face).
  SplitFace {
    faces: Vec<FaceRef>,
    /// A construction or origin plane to split with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool: Option<PlaneRef>,
    /// Faces of bodies, flat or curved, to split with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    surfaces: Vec<FaceRef>,
    /// Sketch curves to split with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    curves: Option<SplitCurves>,
    /// How curves split the faces (faces and planes always split with their surface).
    #[serde(default, skip_serializing_if = "SplitFaceType::is_surface")]
    split_type: SplitFaceType,
    /// Along Vector's direction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    direction: Option<AxisRef>,
    /// Extend Splitting Tool: grow tool faces past their edges and lines past their ends until
    /// they cross the faces.
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    extend: bool,
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
    /// Profiles (interior points) extruded by their boundaries, as well as `curves` (with
    /// profiles and no curves, only the profiles).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    profiles: Vec<[f64; 2]>,
    /// Fusion's Start: Offset, the distance from the profile plane it starts at; empty = on it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    offset: String,
    /// Two sides: the distance the other way (`distance` goes the first way).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    distance2: Option<String>,
  },
  /// A surface revolved from sketch curves.
  SurfaceRevolve {
    sketch: FeatureId,
    #[serde(default)]
    curves: Vec<CurveId>,
    axis: AxisRef,
    #[serde(default = "full_turn")]
    angle: String,
    /// Profiles (interior points) revolved by their boundaries, as well as `curves`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    profiles: Vec<[f64; 2]>,
    /// `angle` each way from the profile.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    symmetric: bool,
    /// Two sides: the angle turned the other way as well.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    angle2: Option<String>,
  },
  /// A surface filling a closed boundary: body edges or a sketch's curves.
  Patch {
    #[serde(default)]
    edges: Vec<EdgeRef>,
    #[serde(default)]
    sketch: Option<FeatureId>,
    /// How the patch meets the face beside each boundary edge (one per edge; missing ones
    /// are connected).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    continuity: Vec<surface::Continuity>,
    /// Reverse the patch's normal.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    flip: bool,
    /// Curves and points the patch passes through (Interior Rails/Points).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    interior: Vec<surface::PatchRail>,
  },
  /// Sews surface bodies into one (a solid when they close up).
  Stitch {
    bodies: Vec<BodyId>,
    #[serde(default = "stitch_tolerance")]
    tolerance: String,
  },
  /// Breaks bodies into one surface body per face; picked faces come off their body each as a
  /// surface body of its own (the rest of the body stays together).
  Unstitch {
    bodies: Vec<BodyId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    faces: Vec<FaceRef>,
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
    /// How the offsets of picked faces that meet are joined.
    #[serde(default, skip_serializing_if = "surface::OffsetCorners::is_separate")]
    corners: surface::OffsetCorners,
  },
  /// Trims surface bodies with a plane, keeping the side behind it (or in front, flipped).
  Trim {
    bodies: Vec<BodyId>,
    tool: PlaneRef,
    #[serde(default)]
    flip: bool,
    /// Points picking the pieces to keep (instead of a side).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    keep: Vec<[f64; 3]>,
    /// Points picking the pieces to remove (everything else stays).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    remove: Vec<[f64; 3]>,
    /// Fusion's Trim: only the bodies the tool cuts change, and only the pieces nearest the
    /// `remove` points go (none: the cut bodies are just split); a tool that cuts none fails.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    cut_only: bool,
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
    /// The settings of Fusion's mesh dialogs (none in features saved before them).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    settings: Option<construct::MeshSettings>,
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
    /// Close the gap between the extensions of edges meeting at a corner.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    close_corners: bool,
  },
  /// Removes holes from faces and/or restores their untrimmed outer boundary, optionally grown
  /// outward by an extension.
  Untrim {
    #[serde(default)]
    faces: Vec<FaceRef>,
    #[serde(default)]
    bodies: Vec<BodyId>,
    #[serde(default)]
    loops: surface::UntrimLoops,
    /// How far the restored outer boundary grows outward (empty: not at all).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    extension: String,
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
    /// Points picking the pieces to remove (everything else stays).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    remove: Vec<[f64; 3]>,
    /// Fusion's Trim: only the bodies the tool cuts change, and only the pieces nearest the
    /// `remove` points go (none: the cut bodies are just split); a tool that cuts none fails.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    cut_only: bool,
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
    /// The follower's motion the cam sets: its slide (a straight push, the default), slide2 or
    /// rotation (a rocker arm whose tip swings along an arc, or a screw's helix).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    motion: Option<assembly::MotionAxis>,
    /// Where the follower touches the cam, on the follower: a point, face, edge or saved joint
    /// origin; `None` = the joint's moving origin. A cylindrical or spherical face is a roller
    /// (its radius and, for a cylinder, its axis and width).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tip: Option<assembly::JointGeometry>,
    /// A ball roller's radius on the tip (empty = none, or the tip face's own radius).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    roller: String,
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
    /// Fusion's Object "Bodies": these bodies move instead of `moving` (then unused).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bodies: Vec<BodyId>,
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
  /// A printed circuit board from the Electronics workspace: the outline extruded to the board
  /// thickness, drilled, with its parts' 3D models on it (see [`pcb`]). The engine rewrites it
  /// whenever the layout changes; the outline follows its sketch or face.
  Pcb {
    name: String,
    outline: pcb::PcbOutline,
    /// The plane a polygon outline lies on (default XY).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plane: Option<PlaneRef>,
    thickness: f64,
    /// Drilled holes: (x, y, diameter) in board coordinates.
    #[serde(default)]
    holes: Vec<[f64; 3]>,
    #[serde(default)]
    parts: Vec<pcb::PcbPart>,
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
  /// MESH > Tessellate: a solid or surface body becomes a mesh body, triangulated to a
  /// refinement (see [`meshprep::refinement`]).
  Tessellate {
    body: BodyId,
    /// "high", "medium", "low" or "custom" (the values below).
    #[serde(default = "medium")]
    refinement: String,
    /// Custom refinement values; an empty one takes the refinement's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    surface_deviation: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    normal_deviation: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    max_edge: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    aspect_ratio: String,
    /// Create Quads (kept for the dialog: the mesh is triangles).
    #[serde(default)]
    quads: bool,
  },
  /// MESH > Repair: fixes holes, stray triangles and flipped triangles of mesh bodies.
  MeshRepair {
    bodies: Vec<BodyId>,
    #[serde(default)]
    repair: meshprep::RepairType,
  },
  /// MESH > Generate Face Groups: divides mesh bodies' faces into groups by the angle between
  /// them (see [`meshprep::face_groups`]).
  GenerateFaceGroups {
    bodies: Vec<BodyId>,
    /// Type Accurate (the group's average normal) instead of Fast (its first face's).
    #[serde(default)]
    accurate: bool,
    #[serde(default = "face_group_angle")]
    angle: String,
    /// Minimum Face Group size, as a fraction of an average group's area.
    #[serde(default = "face_group_size")]
    min_size: String,
  },
  /// MESH > Combine Face Groups: the groups of the picked faces become one.
  CombineFaceGroups {
    faces: Vec<FaceRef>,
  },
  /// Cuts a mesh body with a plane (Fusion's Plane Cut): the plane moved `distance` along its
  /// normal and turned `y_angle` and `z_angle` about its own X and Y directions; the part in
  /// front of it (behind it with `flip`) is trimmed away, split off as another body, or the
  /// triangles are only split along it.
  MeshPlaneCut {
    body: BodyId,
    plane: PlaneRef,
    #[serde(default = "zero")]
    distance: String,
    #[serde(default = "zero")]
    y_angle: String,
    #[serde(default = "zero")]
    z_angle: String,
    #[serde(default)]
    flip: bool,
    #[serde(default)]
    cut: construct::PlaneCutType,
    #[serde(default)]
    fill: construct::FillType,
  },
  /// Mesh Remesh: regenerates the triangles of mesh bodies to an even size (see
  /// `meshops::remesh`).
  MeshRemesh {
    bodies: Vec<BodyId>,
    /// Adaptive (coarse where flat) rather than Uniform.
    #[serde(default)]
    adaptive: bool,
    /// Method Edge Length: the length to aim for; none is Method Density.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    edge_length: Option<String>,
    /// 0–1, higher is more faces.
    density: String,
    /// Shape Preservation, 0–1.
    shape: String,
    #[serde(default)]
    sharp: bool,
    #[serde(default)]
    boundaries: bool,
  },
  /// Mesh Shell: hollows mesh bodies to a wall `thickness` thick (negative: the wall grows
  /// outward); an open mesh gets walls along its outline.
  MeshShell {
    bodies: Vec<BodyId>,
    thickness: String,
  },
  /// Mesh Combine: tools joined to, cut from or intersected with the target, or merged into it
  /// as they are; the result stays one mesh body.
  MeshCombine {
    target: BodyId,
    tools: Vec<BodyId>,
    operation: construct::MeshCombineOp,
    #[serde(default)]
    keep_tools: bool,
  },
  /// Separate: a mesh body split into one body per connected shell.
  MeshSeparate {
    body: BodyId,
    #[serde(default)]
    keep_body: bool,
  },
  /// Fusion's Ruled surface: grown from edges (chained) out of the face beside them, along its
  /// normal or along the face, by a distance and turned by an angle about the edges.
  RuledSurface {
    edges: Vec<EdgeRef>,
    distance: String,
    #[serde(default = "zero_angle")]
    angle: String,
    #[serde(default)]
    along: surface::RuledType,
  },
  /// A surface swept from sketch curves or profile boundaries along a path (Fusion's surface
  /// Sweep, single path).
  SurfaceSweep {
    sketch: FeatureId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    profiles: Vec<[f64; 2]>,
    /// The swept curves (with no profiles and no curves, every curve of the sketch).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    curves: Vec<CurveId>,
    path_sketch: FeatureId,
    /// The path's curves, chained end to end (default every curve of the path sketch).
    #[serde(default)]
    path_curves: Vec<CurveId>,
    /// Orientation Parallel: the profile keeps its own orientation (default: square to the path).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    parallel: bool,
  },
  /// A surface lofted through sketch curves or profile boundaries (Fusion's surface Loft).
  SurfaceLoft {
    sections: Vec<surface::SurfaceSection>,
    /// Loop back to the first section.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    closed: bool,
  },
}

fn medium() -> String {
  "medium".into()
}

fn face_group_angle() -> String {
  "25 deg".into()
}

fn face_group_size() -> String {
  "0.2".into()
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

fn zero_angle() -> String {
  "0 deg".to_string()
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
      // Fusion's Insert Mesh adds a base feature holding the mesh
      FeatureKind::Import { breps, meshes, .. } if breps.is_empty() && !meshes.is_empty() => "Base Mesh Feature",
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
      FeatureKind::Patch { .. } => "SurfacePatch",
      FeatureKind::Stitch { .. } => "Stitch",
      FeatureKind::Unstitch { .. } => "Unstitch",
      FeatureKind::ReverseNormal { .. } => "ReverseNormal",
      FeatureKind::Ruled { .. } => "Ruled",
      FeatureKind::SurfaceOffset { .. } => "Offset",
      FeatureKind::Trim { .. } => "Trim",
      FeatureKind::Mesh { op: construct::MeshOp::Reduce, .. } => "Reduce",
      FeatureKind::Mesh { op: construct::MeshOp::Smooth, .. } => "Smooth",
      FeatureKind::Mesh { op: construct::MeshOp::Convert, .. } => "Convert Mesh",
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
      FeatureKind::Pcb { .. } => "PCB",
      FeatureKind::FullRoundFillet { .. } => "Fillet",
      FeatureKind::IntersectionCurve { .. } => "IntersectionCurve",
      FeatureKind::Canvas { .. } => "Canvas",
      FeatureKind::Tessellate { .. } => "Tessellate",
      FeatureKind::MeshRepair { .. } => "MeshRepair",
      FeatureKind::GenerateFaceGroups { .. } => "GenerateFaceGroups",
      FeatureKind::CombineFaceGroups { .. } => "CombineFaceGroups",
      FeatureKind::MeshPlaneCut { .. } => "MeshPlaneCut",
      FeatureKind::MeshRemesh { .. } => "Remesh",
      FeatureKind::MeshShell { .. } => "MeshShell",
      FeatureKind::MeshCombine { .. } => "MeshCombine",
      FeatureKind::MeshSeparate { .. } => "MeshSeparate",
      FeatureKind::RuledSurface { .. } => "Ruled",
      FeatureKind::SurfaceSweep { .. } => "SurfaceSweep",
      FeatureKind::SurfaceLoft { .. } => "SurfaceLoft",
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

  pub fn sheet_rule_mut(&mut self) -> Option<&mut String> {
    match self {
      FeatureKind::Flange { rule, .. }
      | FeatureKind::ContourFlange { rule, .. }
      | FeatureKind::ConvertToSheetMetal { rule, .. }
      | FeatureKind::LoftedFlange { rule, .. } => Some(rule),
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
      FeatureKind::Extrude { sketch, extent, .. } => {
        sketch.iter().copied().chain(extent.to_point.as_ref().and_then(construct::PointRef::feature)).collect()
      }
      FeatureKind::Flange { sketch, .. }
      | FeatureKind::ContourFlange { sketch, .. }
      | FeatureKind::Bend { sketch, .. } => vec![*sketch],
      FeatureKind::Hole { sketch, to, .. } => std::iter::once(*sketch).chain(to.iter().flat_map(plane_dep)).collect(),
      FeatureKind::Loft { sections, rails, centerline, .. } => {
        sections.iter().map(|s| s.sketch).chain(rails.iter().chain(centerline).map(|r| r.sketch)).collect()
      }
      FeatureKind::Sweep { sketch, path_sketch, rail, rail2, .. } => std::iter::once(*sketch)
        .chain(std::iter::once(*path_sketch))
        .chain(rail.iter().chain(rail2).map(|r| r.sketch))
        .collect(),
      FeatureKind::SplitBody { plane, .. } | FeatureKind::Draft { plane, .. } => plane_dep(plane),
      FeatureKind::Primitive { plane, .. } => plane_dep(plane),
      FeatureKind::Mirror { plane, features, .. } => {
        plane_dep(plane).into_iter().chain(features.iter().copied()).collect()
      }
      FeatureKind::ConstructionPlane { base, .. } => plane_dep(base),
      FeatureKind::PlaneBy { def } => def.depends_on(),
      FeatureKind::ConstructionAxis { def } => def.depends_on(),
      FeatureKind::ConstructionPoint { def } => def.depends_on(),
      FeatureKind::SplitFace { tool, curves, direction, .. } => tool
        .iter()
        .flat_map(plane_dep)
        .chain(curves.iter().map(|c| c.sketch))
        .chain(direction.iter().flat_map(axis_dep))
        .collect(),
      FeatureKind::Coil { plane, .. }
      | FeatureKind::Trim { tool: plane, .. }
      | FeatureKind::MeshPlaneCut { plane, .. } => plane_dep(plane),
      FeatureKind::Pipe { path_sketch, .. } => vec![*path_sketch],
      FeatureKind::Rib { sketch, .. } | FeatureKind::SurfaceExtrude { sketch, .. } => vec![*sketch],
      FeatureKind::SurfaceRevolve { sketch, axis, .. } => {
        let mut v = vec![*sketch];
        v.extend(axis_dep(axis));
        v
      }
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
      FeatureKind::CamFollower { follower, tip, .. } => {
        std::iter::once(*follower).chain(tip.as_ref().and_then(assembly::JointGeometry::origin)).collect()
      }
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
      FeatureKind::Pcb { outline, plane, .. } => {
        let mut v: Vec<FeatureId> = plane.iter().flat_map(plane_dep).collect();
        if let pcb::PcbOutline::Profile { sketch, .. } = outline {
          v.push(*sketch);
        }
        v
      }
      FeatureKind::IntersectionCurve { curves, with, .. } => {
        std::iter::once(curves.sketch).chain(with.iter().map(|w| w.sketch)).collect()
      }
      FeatureKind::Canvas { plane, .. } => plane_dep(plane),
      FeatureKind::SurfaceSweep { sketch, path_sketch, .. } => vec![*sketch, *path_sketch],
      FeatureKind::SurfaceLoft { sections, .. } => sections.iter().map(|x| x.sketch).collect(),
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
      cut_jobs: Vec::new(),
      generative: Vec::new(),
      contact_sets: Vec::new(),
      configurations: config::ConfigTable::default(),
      history_off: false,
      behaviours: Behaviours::current(),
      named_views: Vec::new(),
      face_appearances: Vec::new(),
      timeline_groups: Vec::new(),
      electronics: None,
      exploded_view: None,
      render: render::RenderDoc::default(),
      sim: Vec::new(),
      design_type: DesignType::Part,
      flat_pattern: None,
      material: String::new(),
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
    let name = self.unique_name(&kind);
    let at = self.active_len();
    self.features.insert(
      at,
      Feature {
        id,
        name,
        suppressed: false,
        visible: None,
        targets: Vec::new(),
        cached: Vec::new(),
        component: None,
        kind,
        owner: None,
      },
    );
    if let Some(m) = self.marker.as_mut() {
      *m += 1;
    }
    Ok(id)
  }

  fn unique_name(&self, kind: &FeatureKind) -> String {
    let base = kind.name_stem();
    // Fusion numbers base, edge and contour flanges with one counter (BaseFlange1, EdgeFlange2)
    let n = self
      .features
      .iter()
      .filter(|f| if kind.is_flange() { f.kind.is_flange() } else { f.kind.base_name() == kind.base_name() })
      .count()
      + 1;
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
    self.configurations.clear_rule(&config::Column::Parameter { name: name.to_string() });
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

  /// The root component's physical material: [`Design::material`], steel when it names none.
  pub fn root_material(&self) -> &str {
    if self.material.is_empty() { "Steel" } else { &self.material }
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
  /// library default) for an empty name. A name the design lacks resolves in Fusion's library.
  pub fn sheet_rule(&self, name: &str) -> Option<SheetMetalRule> {
    self.clone_rules().get(name).ok()
  }

  /// The rule the design's sheet metal uses, as the browser's Rule row shows it: the first sheet
  /// metal feature's. `None` before the first one.
  pub fn component_sheet_rule(&self) -> Option<SheetMetalRule> {
    let name = self.features.iter().find_map(|f| f.kind.sheet_rule())?;
    self.sheet_rule(name)
  }

  /// Switch Rule: makes `rule` the design's default (adding or replacing it) and points every
  /// sheet metal feature at it.
  pub fn switch_sheet_rule(&mut self, rule: SheetMetalRule) -> Result<(), DesignError> {
    let name = rule.name.clone();
    self.set_sheet_rule(rule)?;
    if let Some(i) = self.sheet_metal_rules.iter().position(|r| r.name == name) {
      let r = self.sheet_metal_rules.remove(i);
      self.sheet_metal_rules.insert(0, r);
    }
    for f in &mut self.features {
      if let Some(r) = f.kind.sheet_rule_mut() {
        *r = name.clone();
      }
    }
    Ok(())
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

  /// Renames a rule, keeping its place in the list, and the features that name it follow.
  pub fn rename_sheet_rule(&mut self, from: &str, to: &str) -> Result<(), DesignError> {
    if to.trim().is_empty() || self.sheet_metal_rules.iter().any(|r| r.name == to) {
      return Err(DesignError::BadParameterName(to.to_string()));
    }
    let r = self.sheet_metal_rules.iter_mut().find(|r| r.name == from);
    r.ok_or_else(|| DesignError::NoParameter(from.to_string()))?.name = to.to_string();
    for f in &mut self.features {
      if let Some(r) = f.kind.sheet_rule_mut().filter(|r| *r == from) {
        *r = to.to_string();
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
      self.bodies.push(BodySettings { body: id, name: None, visible: true, color: None, material: None });
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
    if self.version < 3
      && self.sheet_metal_rules.is_empty()
      && self.features.iter().any(|f| f.kind.sheet_rule().is_some())
    {
      // the built-in rule was "Default", 1 mm thick, before it became Fusion's ".063 Aluminum"
      self.sheet_metal_rules.push(SheetMetalRule::plain("Default"));
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
      None => sheet_rule_library()
        .into_iter()
        .find(|r| r.name == name)
        .ok_or_else(|| format!("there is no sheet metal rule '{name}'")),
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
    FeatureKind::Coil { diameter, pitch, height, section_size, angle, .. } => {
      for e in [diameter, pitch, height, section_size] {
        f(e);
      }
      if !angle.is_empty() {
        f(angle);
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
    FeatureKind::SurfaceExtrude { distance, offset, distance2, .. } => {
      f(distance);
      if !offset.is_empty() {
        f(offset);
      }
      if let Some(d) = distance2 {
        f(d);
      }
    }
    FeatureKind::SurfaceRevolve { angle, angle2, .. } => {
      f(angle);
      if let Some(a) = angle2 {
        f(a);
      }
    }
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
    FeatureKind::Untrim { extension, .. } => {
      if !extension.is_empty() {
        f(extension);
      }
    }
    FeatureKind::TrimWith { .. } => {}
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
    FeatureKind::CamFollower { clearance, roller, .. } => {
      f(clearance);
      if !roller.is_empty() {
        f(roller);
      }
    }
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
    FeatureKind::Pcb { .. } => {}
    FeatureKind::Tessellate { surface_deviation, normal_deviation, max_edge, aspect_ratio, .. } => {
      for e in [surface_deviation, normal_deviation, max_edge, aspect_ratio] {
        if !e.is_empty() {
          f(e);
        }
      }
    }
    FeatureKind::MeshRepair { .. } | FeatureKind::CombineFaceGroups { .. } => {}
    FeatureKind::GenerateFaceGroups { angle, min_size, .. } => {
      f(angle);
      f(min_size);
    }
    FeatureKind::MeshPlaneCut { distance, y_angle, z_angle, .. } => {
      for e in [distance, y_angle, z_angle] {
        f(e);
      }
    }
    FeatureKind::MeshRemesh { edge_length, density, shape, .. } => {
      if let Some(e) = edge_length {
        f(e);
      }
      f(density);
      f(shape);
    }
    FeatureKind::MeshShell { thickness, .. } => f(thickness),
    FeatureKind::MeshCombine { .. } | FeatureKind::MeshSeparate { .. } => {}
    FeatureKind::RuledSurface { distance, angle, .. } => {
      f(distance);
      f(angle);
    }
    FeatureKind::SurfaceSweep { .. } | FeatureKind::SurfaceLoft { .. } => {}
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

impl FeatureKind {
  /// The stem of a new feature's name: the base name, but Fusion's "BaseFlange" for a flange
  /// from profiles.
  pub fn name_stem(&self) -> &'static str {
    match self {
      FeatureKind::Flange { .. } => "BaseFlange",
      _ => self.base_name(),
    }
  }

  /// A base, edge or contour flange (the features Fusion's one Flange command makes).
  pub fn is_flange(&self) -> bool {
    matches!(self, FeatureKind::Flange { .. } | FeatureKind::EdgeFlange { .. } | FeatureKind::ContourFlange { .. })
  }
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
        sketch: Some(s),
        faces: vec![],
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
    let a = d.add_feature(FeatureKind::RemoveBodies { bodies: vec![], components: vec![] }).unwrap();
    let b = d.add_feature(FeatureKind::RemoveBodies { bodies: vec![], components: vec![] }).unwrap();
    d.set_marker(Some(1));
    let c = d.add_feature(FeatureKind::RemoveBodies { bodies: vec![], components: vec![] }).unwrap();
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
      sketch: Some(s),
      faces: vec![],
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
