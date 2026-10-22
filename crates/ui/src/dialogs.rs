//! Feature dialogs (the floating panels on the right), with live preview.
//!
//! A preview is a real command applied to the session; changing a value withdraws it (undo) and
//! applies the new one, so OK just keeps the last preview and Cancel withdraws it — exactly one
//! undo step per feature, as in Fusion.

use egui::RichText;
use fox_engine::model::{AxisRef, BodyId, ExtentKind, FeatureId, FeatureKind, Operation, PlaneRef, Primitive};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use crate::icons::Icon;
use crate::theme;
use crate::{App, panels};

mod analysis;
mod choice;
mod construct;
mod extrude;
mod hole_pattern;
mod icon_row;
#[cfg(test)]
mod inspect_tests;
mod interference;
mod live;
mod manip;
mod modify;
mod move_copy;
mod pieces;
mod press_owner;
mod rows;
mod simplify;
mod stitch;
pub(crate) use analysis::center_of_mass;
pub(crate) use hole_pattern::placement_sketches;
pub(crate) use move_copy::Paste;
mod auto_cut;
mod body_picks;
mod copies;
mod edit_more;
mod flange;
pub(crate) use flange::TYPE_ICONS as FLANGE_TYPES;
#[cfg(test)]
mod edit_tests;
mod export_dxf;
mod insert_mesh;
#[cfg(test)]
mod insert_mesh_tests;
mod mesh;
#[cfg(test)]
mod mesh_tests;
mod newcomp;
mod plane_cut;
mod primitive;
mod redefine;
mod rsl;
mod section;
mod sheet;
pub(crate) use plane_cut::overlay as plane_cut_overlay;
mod sheet_flat;
#[cfg(test)]
mod sheet_flat_tests;
#[cfg(test)]
mod sketch_row_tests;
mod solid;
mod thread;
pub(crate) use export_dxf::{SaveWindow, save_window};
pub(crate) use mesh::group_triangle;
pub(crate) use primitive::overlay as place_overlay;
#[cfg(test)]
mod construct_tests;

pub(crate) use section::{KeptSection, shown_section};

/// What Create Sketch asks for, in its dialog and the viewport tooltip (Fusion's wording).
pub(crate) const PICK_PLANE_HINT: &str = "Select a plane or planar face";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  PickPlane,
  Extrude,
  Revolve,
  Fillet,
  Chamfer,
  Shell,
  Box,
  Cylinder,
  Sphere,
  Torus,
  RectPattern,
  CircPattern,
  Mirror,
  Combine,
  Move,
  OffsetPlane,
  Inspect,
  Loft,
  Sweep,
  Hole,
  PressPull,
  /// Offset Face (MODIFY): faces moved along their normals by a distance.
  OffsetFace,
  Draft,
  /// Scale (MODIFY): bodies scaled about a point, uniformly or per axis.
  Scale,
  SplitBody,
  // sheet metal
  Flange,
  Hem,
  Fold,
  ConvertSheet,
  Rip,
  Unfold,
  SheetRules,
  FlatPattern,
  // assembly
  NewComponent,
  Joint,
  /// Joint two components where they already sit (Fusion's As-built Joint).
  AsBuiltJoint,
  /// A component's properties (name, part number, description, type).
  ComponentProps,
  Material,
  Interference,
  Bom,
  CenterOfMass,
  // sketch modify (run on the sketch being edited)
  /// The sketch Fillet and Chamfer dialogs: no longer in the menus (Fusion runs them as
  /// dialog-less tools, see `crate::sketch_modify`); still usable by the dialog tests.
  SkFillet,
  SkChamfer,
  SkChamfer2,
  SkChamferAngle,
  SkOffset,
  SkMirror,
  SkCircPattern,
  SkRectPattern,
  /// Sketch Scale: curves scaled about a picked sketch point.
  SkScale,
  Project,
  // assembly motion and component copies
  DriveJoint,
  MotionStudy,
  CopyComponent,
  // drawings
  NewDrawing,
  DrawingView,
  DrawingMoveView,
  DrawingDimension,
  DrawingNote,
  DrawingTitle,
  DrawingSettings,
  /// A declarative command dialog (see [`crate::forms`]).
  Form(&'static crate::forms::Form),
  /// Section analysis: cut the view open along a plane.
  Section,
  /// Surface analysis: zebra, draft, accessibility or curvature.
  Analysis(&'static str),
  /// Trace an image into the sketch being edited.
  TraceImage,
  /// Appearance: a library of swatches to drag onto bodies and faces, and the design's own.
  Appearance,
  /// Redefine Sketch Plane (a sketch row's menu): the sketch onto another plane or planar face.
  RedefinePlane,
  /// Export DXF (a sketch row's menu): the unit and the geometry, then where to save.
  ExportDxf,
  /// Insert Mesh after its file is picked: the unit and the placement (see [`insert_mesh`]).
  InsertMesh,
  /// Plane Cut on the MESH tab (see [`plane_cut`]).
  PlaneCut,
  /// Export Flat Pattern (the flat pattern environment's EXPORT panel, the Flat Pattern row).
  ExportFlat,
}

impl Kind {
  fn title(self) -> &'static str {
    match self {
      Kind::PickPlane => "CREATE SKETCH",
      Kind::Extrude => "EXTRUDE",
      Kind::Revolve => "REVOLVE",
      Kind::Fillet => "FILLET",
      Kind::Chamfer => "CHAMFER",
      Kind::Shell => "SHELL",
      Kind::Box => "BOX",
      Kind::Cylinder => "CYLINDER",
      Kind::Sphere => "SPHERE",
      Kind::Torus => "TORUS",
      Kind::RectPattern => "RECTANGULAR PATTERN",
      Kind::CircPattern => "CIRCULAR PATTERN",
      Kind::Mirror => "MIRROR",
      Kind::Combine => "COMBINE",
      Kind::Move => "MOVE/COPY",
      Kind::OffsetPlane => construct::TITLE,
      Kind::Inspect => "MEASURE",
      Kind::Appearance => "APPEARANCE",
      Kind::RedefinePlane => "REDEFINE SKETCH PLANE",
      Kind::ExportDxf => "EXPORT DXF",
      Kind::InsertMesh => "INSERT MESH",
      Kind::PlaneCut => "PLANE CUT",
      Kind::ExportFlat => "EXPORT FLAT PATTERN",
      Kind::Loft => "LOFT",
      Kind::Sweep => "SWEEP",
      Kind::Hole => "HOLE",
      Kind::PressPull => "PRESS PULL",
      Kind::OffsetFace => "OFFSET FACE",
      Kind::Draft => "DRAFT",
      Kind::Scale => "SCALE",
      Kind::SplitBody => "SPLIT BODY",
      Kind::Flange => "FLANGE",
      Kind::Hem => "HEM",
      Kind::Fold => "FOLD",
      Kind::ConvertSheet => "CONVERT TO SHEET METAL",
      Kind::Rip => "RIP",
      Kind::Unfold => "UNFOLD",
      Kind::SheetRules => "SHEET METAL RULES",
      Kind::FlatPattern => "CREATE FLAT PATTERN",
      Kind::NewComponent => "NEW COMPONENT",
      Kind::Joint => "JOINT",
      Kind::AsBuiltJoint => "AS-BUILT JOINT",
      Kind::ComponentProps => "PROPERTIES",
      Kind::Material => "PHYSICAL MATERIAL",
      Kind::Interference => "INTERFERENCE",
      Kind::Bom => "BILL OF MATERIALS",
      Kind::CenterOfMass => "CENTER OF MASS",
      Kind::SkFillet => "SKETCH FILLET",
      Kind::SkChamfer | Kind::SkChamfer2 | Kind::SkChamferAngle => "SKETCH CHAMFER",
      Kind::SkOffset => "OFFSET",
      Kind::TraceImage => "TRACE IMAGE",
      Kind::SkMirror => "MIRROR",
      Kind::SkCircPattern => "CIRCULAR PATTERN",
      Kind::SkRectPattern => "RECTANGULAR PATTERN",
      Kind::SkScale => "SKETCH SCALE",
      Kind::Project => "PROJECT",
      Kind::DriveJoint => "DRIVE JOINTS",
      Kind::MotionStudy => "MOTION STUDY",
      Kind::CopyComponent => "DUPLICATE COMPONENT",
      Kind::NewDrawing => "CREATE DRAWING",
      // every plane, axis and point command opens Fusion's one construction dialog
      Kind::Form(_) if construct::method_of(self).is_some() => construct::TITLE,
      Kind::Form(f) => f.title,
      Kind::Section => "SECTION ANALYSIS",
      Kind::Analysis("zebra") => "ZEBRA ANALYSIS",
      Kind::Analysis("draft") => "DRAFT ANALYSIS",
      Kind::Analysis("accessibility") => "ACCESSIBILITY ANALYSIS",
      Kind::Analysis("comb") => "CURVATURE COMB ANALYSIS",
      Kind::Analysis("environment") => "ENVIRONMENT MAP ANALYSIS",
      Kind::Analysis("isocurve") => "ISOCURVE ANALYSIS",
      Kind::Analysis("min_radius") => "MINIMUM RADIUS ANALYSIS",
      Kind::Analysis(_) => "CURVATURE MAP ANALYSIS",
      Kind::DrawingView => "DRAWING VIEW",
      Kind::DrawingMoveView => "MOVE VIEW",
      Kind::DrawingDimension => "DIMENSION",
      Kind::DrawingNote => "TEXT",
      Kind::DrawingTitle => "TITLE BLOCK",
      Kind::DrawingSettings => "SHEET SETTINGS",
    }
  }

  fn icon(self) -> Icon {
    match self {
      Kind::PickPlane => Icon::Sketch,
      Kind::Extrude => Icon::Extrude,
      Kind::Revolve => Icon::Revolve,
      Kind::Fillet => Icon::Fillet,
      Kind::Chamfer => Icon::Chamfer,
      Kind::Shell => Icon::Shell,
      Kind::Box => Icon::Box,
      Kind::Cylinder => Icon::Cylinder,
      Kind::Sphere => Icon::Sphere,
      Kind::Torus => Icon::Torus,
      Kind::RectPattern => Icon::Pattern,
      Kind::CircPattern => Icon::CircPattern,
      Kind::Mirror => Icon::Mirror,
      Kind::Combine => Icon::Combine,
      Kind::Move => Icon::Move,
      Kind::OffsetPlane => Icon::Plane,
      Kind::Inspect => Icon::Measure,
      Kind::Appearance => Icon::Appearance,
      Kind::RedefinePlane => Icon::Plane,
      Kind::ExportDxf => Icon::ExportDxf,
      Kind::InsertMesh => Icon::InsertMesh,
      Kind::PlaneCut => Icon::PlaneCut,
      Kind::ExportFlat => Icon::Export,
      Kind::Loft => Icon::Loft,
      Kind::Sweep => Icon::Sweep,
      Kind::Hole => Icon::Hole,
      Kind::PressPull => Icon::PressPull,
      Kind::OffsetFace => Icon::OffsetFace,
      Kind::Draft => Icon::Draft,
      Kind::Scale => Icon::Scale,
      Kind::SplitBody => Icon::SplitBody,
      Kind::Flange => Icon::Flange,
      Kind::Hem => Icon::Hem,
      Kind::Fold => Icon::Fold,
      Kind::ConvertSheet => Icon::ConvertSheet,
      Kind::Rip => Icon::Rip,
      Kind::Unfold => Icon::Unfold,
      Kind::SheetRules => Icon::SmRules,
      Kind::FlatPattern => Icon::FlatPattern,
      Kind::NewComponent => Icon::Component,
      Kind::Joint | Kind::AsBuiltJoint => Icon::JointOrigin,
      Kind::ComponentProps => Icon::Component,
      Kind::Material => Icon::PhysMaterial,
      Kind::Interference => Icon::Interference,
      Kind::Bom => Icon::Bom,
      Kind::Form(f) => f.icon,
      Kind::Section => Icon::Section,
      Kind::Analysis("zebra") => Icon::Zebra,
      Kind::Analysis("draft") => Icon::DraftAnalysis,
      Kind::Analysis("accessibility") => Icon::Accessibility,
      Kind::Analysis("comb") => Icon::CurvComb,
      Kind::Analysis("environment") => Icon::EnvMap,
      Kind::Analysis("isocurve") => Icon::Isocurve,
      Kind::Analysis("min_radius") => Icon::MinRadius,
      Kind::Analysis(_) => Icon::CurvMap,
      Kind::CenterOfMass => Icon::CenterOfMass,
      Kind::SkFillet => Icon::SkFillet,
      Kind::SkChamfer | Kind::SkChamfer2 | Kind::SkChamferAngle => Icon::SkChamfer,
      Kind::SkOffset => Icon::SkOffset,
      Kind::TraceImage => Icon::TraceImage,
      Kind::SkMirror => Icon::SkMirror,
      Kind::SkCircPattern => Icon::SkCircPattern,
      Kind::SkRectPattern => Icon::SkRectPattern,
      Kind::SkScale => Icon::SkScale,
      Kind::Project => Icon::Project,
      Kind::DriveJoint => Icon::DriveJoint,
      Kind::MotionStudy => Icon::MotionStudy,
      Kind::CopyComponent => Icon::Component,
      Kind::NewDrawing => Icon::NewDrawing,
      Kind::DrawingSettings => Icon::SheetSettings,
      Kind::DrawingView => Icon::BaseView,
      Kind::DrawingMoveView => Icon::Move,
      Kind::DrawingDimension => Icon::Dimension,
      Kind::DrawingNote => Icon::SkText,
      Kind::DrawingTitle => Icon::TitleBlock,
    }
  }

  /// Icon, name and tooltip of the dialogs added with the loft … sketch-modify commands.
  pub(crate) fn info(self) -> (Icon, &'static str, &'static str) {
    let (name, hint) = match self {
      Kind::Loft => ("Loft", "Blend through profiles on two or more sketches"),
      Kind::Sweep => ("Sweep", "Sweep a profile along a path sketch"),
      Kind::Hole => ("Hole", "Drill holes at a sketch's points (H)"),
      Kind::PressPull => ("Press Pull", "Push or pull faces along their normals (Q)"),
      Kind::Draft => ("Draft", "Taper faces about a pull direction"),
      Kind::OffsetFace => ("Offset Face", "Offset faces, flat or curved, along their normals"),
      Kind::Scale => ("Scale", "Scale bodies about a point"),
      Kind::SplitBody => ("Split Body", "Split bodies along a plane or planar face"),
      Kind::Flange => ("Flange", "Sheet metal from a profile, an open path or sheet edges"),
      Kind::Hem => ("Hem", "Fold sheet edges back on themselves"),
      Kind::Fold => ("Fold", "Bend a flat sheet along a sketch line"),
      Kind::ConvertSheet => ("Convert to Sheet Metal", "Turn a thin solid into sheet metal"),
      Kind::Rip => ("Rip", "Slit a solid along edges so it can unfold"),
      Kind::Unfold => ("Unfold", "Flatten a sheet metal body onto a face"),
      Kind::SheetRules => ("Sheet Metal Rules", "Thickness, bend radius and reliefs of sheet metal"),
      Kind::FlatPattern => ("Create Flat Pattern", "Lay a sheet metal body flat on a stationary face"),
      Kind::NewComponent => ("New Component", "Make an empty component, or one from the selected bodies"),
      Kind::Joint => ("Joint", "Join two components at a face or edge of each"),
      Kind::AsBuiltJoint => ("As-built Joint", "Joint two components where they already sit"),
      Kind::ComponentProps => ("Properties", "A component's name, part number, description and type"),
      Kind::Material => ("Physical Material", "Set a component's material for mass and the BOM"),
      Kind::Interference => {
        ("Interference", "Find overlapping bodies of different components, or of the selected bodies")
      }
      Kind::Bom => ("Bill of Materials", "Components, materials and masses"),
      Kind::Form(f) => (f.name, f.hint),
      Kind::Section => ("Section Analysis", "Cut the view open along a plane"),
      Kind::Analysis("zebra") => ("Zebra Analysis", "Reflected stripes show how smoothly surfaces join"),
      Kind::Analysis("draft") => ("Draft Analysis", "Which faces have draft for moulding along a pull direction"),
      Kind::Analysis("accessibility") => ("Accessibility Analysis", "Which faces a tool can reach from a direction"),
      Kind::Analysis("comb") => ("Curvature Comb Analysis", "Combs along edges show how their curvature changes"),
      Kind::Analysis("environment") => ("Environment Map Analysis", "A reflected environment shows how surfaces flow"),
      Kind::Analysis("isocurve") => ("Isocurve Analysis", "Curves across faces along their parameter directions"),
      Kind::Analysis("min_radius") => ("Minimum Radius Analysis", "Where surfaces curve tighter than a tool can reach"),
      Kind::Analysis(_) => ("Curvature Map Analysis", "Colour by how sharply surfaces curve"),
      Kind::CenterOfMass => ("Center of Mass", "Centre of mass of the selected bodies"),
      Kind::SkFillet => ("Fillet", "Round the corner where two lines or arcs meet"),
      Kind::SkChamfer => ("Equal Distance Chamfer", "Bevel the corner where two lines meet"),
      Kind::SkChamfer2 => ("Two Distance Chamfer", "Bevel a corner by two distances"),
      Kind::SkChamferAngle => {
        ("Distance and Angle Chamfer", "Bevel a corner by a distance along the first line and an angle from it")
      }
      Kind::SkOffset => ("Offset", "Offset a chain of curves (O)"),
      Kind::TraceImage => ("Trace Image", "Trace a photo, logo or scan into closed sketch curves"),
      Kind::SkMirror => ("Mirror", "Mirror sketch curves across a line"),
      Kind::SkCircPattern => ("Circular Pattern", "Copy sketch curves around a point"),
      Kind::SkRectPattern => ("Rectangular Pattern", "Copy sketch curves in rows and columns"),
      Kind::SkScale => ("Sketch Scale", "Scale sketch curves about a point; dimensions inside them scale too"),
      Kind::Project => ("Project", "Project a body's or face's edges into the sketch"),
      Kind::DriveJoint => ("Drive Joints", "Set a joint's rotation and slide"),
      Kind::MotionStudy => ("Motion Study", "Step a joint through a range and check for interference"),
      Kind::CopyComponent => ("Duplicate Component", "Another instance of a component that follows the original"),
      Kind::NewDrawing => ("Create Drawing", "A drawing of the design with standard views"),
      Kind::DrawingView => ("Base View", "Add a view to the drawing"),
      Kind::DrawingMoveView => ("Move View", "Place or delete a drawing view"),
      Kind::DrawingDimension => ("Dimension", "Dimension two model points in a view"),
      Kind::DrawingNote => ("Text", "A note on the sheet"),
      Kind::DrawingTitle => ("Title Block", "Fill in the title block"),
      Kind::DrawingSettings => ("Sheet Settings", "Sheet size, scale, projection and tables"),
      Kind::Appearance => ("Appearance", "Drag appearances onto bodies or faces (A)"),
      Kind::RedefinePlane => ("Redefine Sketch Plane", "Put the sketch on another plane or planar face"),
      Kind::ExportDxf => ("Export DXF", "Save the sketch as a DXF file"),
      Kind::InsertMesh => ("Insert Mesh", "Place a mesh file in the design: its unit, up direction and position"),
      Kind::PlaneCut => ("Plane Cut", "Cut a mesh body with a plane: trim it, split it in two or split its faces"),
      Kind::ExportFlat => ("Export Flat Pattern", "Save the flat pattern as a DXF or STEP file"),
      _ => (self.title(), ""),
    };
    (self.icon(), name, hint)
  }

  /// Dialogs whose input is picked edges/faces/bodies of the model: they preview on demand,
  /// because a preview changes the very geometry being picked.
  fn picks_model(self) -> bool {
    if let Kind::Form(f) = self {
      return f.picks_model;
    }
    matches!(
      self,
      Kind::Fillet
        | Kind::Chamfer
        | Kind::Shell
        | Kind::Combine
        | Kind::PressPull
        | Kind::OffsetFace
        | Kind::Draft
        | Kind::Hem
        | Kind::Rip
        | Kind::ConvertSheet
        | Kind::Fold
        | Kind::Joint
        | Kind::AsBuiltJoint
        | Kind::PlaneCut
    )
  }

  /// Fusion's library palettes (Appearance, Physical Material): swatches applied by a drag or a
  /// click, with Close rather than OK.
  pub(crate) fn palette(self) -> bool {
    matches!(self, Kind::Appearance | Kind::Material)
  }

  /// Dialogs that only show information (no OK, nothing applied).
  pub(crate) fn reports(self) -> bool {
    matches!(self, Kind::Inspect | Kind::Appearance | Kind::Material | Kind::Bom | Kind::MotionStudy)
  }

  /// Dialogs of the Drawing workspace.
  pub fn drawing(self) -> bool {
    matches!(
      self,
      Kind::DrawingView
        | Kind::DrawingMoveView
        | Kind::DrawingDimension
        | Kind::DrawingNote
        | Kind::DrawingTitle
        | Kind::DrawingSettings
    )
  }

  /// Dialogs that change the sketch being edited.
  pub fn sketch_edit(self) -> bool {
    matches!(
      self,
      Kind::SkFillet
        | Kind::SkChamfer
        | Kind::SkChamfer2
        | Kind::SkChamferAngle
        | Kind::SkOffset
        | Kind::SkMirror
        | Kind::SkCircPattern
        | Kind::SkRectPattern
        | Kind::SkScale
        | Kind::Project
        | Kind::TraceImage
    )
  }
}

/// A drag arrow in the viewport (see [`Dialog::manipulator`]).
#[derive(Debug, Clone, Copy)]
pub struct Manipulator {
  pub origin: fox_engine::kernel::glam::DVec3,
  pub dir: fox_engine::kernel::glam::DVec3,
  pub value: f64,
}

/// An edited primitive's Placement row: the plane it stands on, named as its creation dialog
/// names a picked one ("XY plane", "Face of Body1", a construction plane's name).
fn placement_label(app: &App, plane: &fox_engine::model::PlaneRef) -> String {
  use fox_engine::model::{OriginPlane, PlaneRef};
  match plane {
    PlaneRef::Origin { plane } => match plane {
      OriginPlane::Xy => "XY plane",
      OriginPlane::Xz => "XZ plane",
      OriginPlane::Yz => "YZ plane",
    }
    .into(),
    PlaneRef::Construction { feature } => {
      app.s.design.feature(*feature).map_or_else(|| "Plane".into(), |f| f.name.clone())
    }
    PlaneRef::Face { face, .. } => format!("Face of {}", app.s.body_name(face.body)),
    _ => "Plane".into(),
  }
}

/// The world plane a dialog's plane value ("xy", {"feature"}, {"body", "face"}) names.
fn plane_of(s: &Session, v: &Value) -> Option<fox_engine::kernel::Plane> {
  use fox_engine::model::OriginPlane;
  if let Some(n) = v.as_str() {
    let plane = match n {
      "xz" => OriginPlane::Xz,
      "yz" => OriginPlane::Yz,
      _ => OriginPlane::Xy,
    };
    return Some(plane.plane(s.design.frames()));
  }
  if let Some(f) = v.get("feature").and_then(Value::as_u64) {
    return s.model.planes.get(&FeatureId(f as u32)).copied();
  }
  let body = BodyId(v.get("body")?.as_u64()? as u32);
  let face = v.get("face")?.as_u64()? as u32;
  let f = s.model.body(body)?.faces().iter().find(|f| f.id == face).copied()?;
  fox_engine::model::regen::sketch_plane_on(f.normal, f.origin, s.design.frames()).ok()
}

const OPS: [(&str, &str); 5] = [
  ("new_body", "New Body"),
  ("join", "Join"),
  ("cut", "Cut"),
  ("intersect", "Intersect"),
  ("new_component", "New Component"),
];

pub struct Dialog {
  pub kind: Kind,
  /// Feature being edited (timeline double-click); None while creating.
  pub editing: Option<FeatureId>,
  /// While Edit Feature has the design rolled back to the edited feature (inside a transaction),
  /// the marker to roll forward to on OK; see [`Dialog::begin_edit`].
  rolled: Option<Option<usize>>,
  /// The edited feature's picks were found again on the rolled-back model: its rows are the
  /// creation dialog's, live (Fillet, Chamfer, Extrude).
  repicks: bool,
  /// The feature the applied preview made (or the one edited).
  previewed: Option<FeatureId>,
  /// A preview step is applied to the session.
  applied: bool,
  error: Option<String>,
  dirty: bool,
  /// Set by a field that finished the dialog itself (e.g. picking a sketch plane).
  closed: bool,
  /// OK picked from outside the dialog (the marking menu); handled on its next frame.
  pub(crate) ok_requested: bool,
  // captured inputs
  pub sketch: Option<FeatureId>,
  profiles: Vec<[f64; 2]>,
  edges: Vec<Value>,
  faces: Vec<Value>,
  bodies: Vec<u32>,
  plane: Value,
  plane_label: String,
  // fields
  distance: String,
  distance2: String,
  extent: &'static str,
  flip: bool,
  op: &'static str,
  angle: String,
  /// Revolve, two sides: the angle the other way.
  angle2: String,
  axis: String,
  radius: String,
  count: String,
  spacing: String,
  dir: String,
  use_dir2: bool,
  count2: String,
  spacing2: String,
  dir2: String,
  /// Sketch rectangular pattern: the distances are the whole pattern's size (Fusion's Extent).
  pattern_extent: bool,
  /// Sketch rectangular pattern: centre each direction's instances on the original.
  symmetric: [bool; 2],
  join: bool,
  a: String,
  b: String,
  c: String,
  at: [String; 2],
  translate: [String; 3],
  keep_tools: bool,
  /// Loft sections: sketch and a point inside the profile.
  sections: Vec<(u32, [f64; 2])>,
  /// Sweep path sketch.
  path: Option<FeatureId>,
  /// Picked sketch curves.
  curves: Vec<u32>,
  /// Mirror / fold line.
  line: Option<u32>,
  /// Mirror in "features" mode: the features whose cut/join is mirrored.
  mirror_features: Vec<u32>,
  /// Joint origins in pick order: body and `{"face"|"edge": ...}`.
  origins: Vec<(u32, Value)>,
  /// Components of the picked bodies / joint origins (None = a root body).
  comps: Vec<Option<u32>>,
  /// Hole type, flange type or joint type.
  choice: &'static str,
  /// Edge flange position / sheet metal relief shape.
  position: &'static str,
  rule: String,
  rules: Vec<String>,
  flag: bool,
  flag2: bool,
  d: String,
  e: String,
  name: String,
  /// Output of report dialogs (flat pattern, interference, BOM).
  report: Option<Value>,
  /// Joint, component or drawing the dialog works on; Split Body's tool body.
  target: Option<u32>,
  /// Drawing view.
  view: Option<u32>,
  /// Second model point (drawing dimensions).
  xyz: [String; 3],
  /// Create Drawing's standard views (see [`VIEWS`]).
  view_on: [bool; 7],
  /// A form dialog's values.
  form: crate::forms::State,
  /// Setback, guide rails, twist, centre line, extrude object (see [`solid`]).
  solid: solid::Extra,
  /// Edge flange corner closure (see [`CORNERS`]).
  corners: &'static str,
  /// Sheet metal rule corner relief (see [`CORNER_RELIEFS`]) and its size.
  corner_relief: &'static str,
  corner_relief_size: String,
  /// The design's length unit: values the dialog writes (drag arrows) are in it.
  unit: fox_engine::model::LengthUnit,
  /// Joint: the saved joint origin (by name) used instead of a pick on each side; empty = the pick.
  saved: [String; 2],
  /// Trace Image's settings and last result.
  pub trace: Option<Box<crate::trace::TraceState>>,
  /// Where the viewport manipulators sit (see [`manip`]).
  manip: manip::Cache,
  /// Measure: the selection filter as it was before the dialog changed it.
  saved_filter: Option<(crate::select::Priority, bool)>,
  /// The Appearance and Physical Material palettes' state (see [`crate::material_ui`]).
  palette: crate::material_ui::Palette,
  /// Move/Copy's inputs (see [`move_copy`]).
  mv: move_copy::MoveForm,
  /// Live preview (see [`live`]).
  live: live::Live,
  /// Sketch Offset: the distance and side follow the cursor until a click places the offset.
  follow: bool,
  /// Drawing Dimension: points picked on the sheet so far.
  sheet_picks: usize,
  /// New Component operation: the new component's name (empty = the next default name).
  comp_name: String,
  /// What the previewed command reports it did (Remove Duplicates' counts).
  summary: Option<String>,
  /// The sketch curves the previewed command made (Offset's dimension measures to them).
  made: Vec<u32>,
  /// The canvas the dialog was last docked to (its top-right corner): None until the first
  /// frame, so a dialog opens docked and docks again when the window is resized, as in Fusion.
  docked: Option<egui::Rect>,
  /// Sketch Mirror / patterns: the selection row the next picks go to (0 = Objects, 1 = the
  /// mirror line, centre point or directions), as Fusion's active (blue) row.
  pick_row: usize,
  /// Sketch Circular Pattern: the centre picked (a sketch point, or a circle's centre).
  center: Option<Sel>,
  /// Sketch Rectangular Pattern: the lines picked as directions, with their unit directions.
  dir_lines: Vec<(u32, [f64; 2])>,
  /// Text: the frame was just placed, so the Text box takes the keyboard on the next frame (its
  /// sample text selected, so typing replaces it), as in Fusion.
  focus_text: bool,
  /// Box, Cylinder, Sphere, Torus and Coil: the plane pick and drawing before the dialog shows
  /// (see [`primitive`]).
  place: primitive::Step,
  /// Redefine Sketch Plane's pick and the timeline it rolled back (see [`redefine`]).
  redefine: redefine::State,
  /// Export DXF's unit and Visible Geometry check boxes (see [`export_dxf`]).
  dxf: export_dxf::Options,
  /// Unfold's and Export Flat Pattern's choices (see [`sheet_flat`]).
  flat: sheet_flat::State,
  /// The patterns' Type row picked another pattern: the dialog becomes that one next frame
  /// (see [`copies`]).
  switch: Option<Kind>,
  /// Bodies the previewed Mirror or pattern added: drawn as ghosts until OK (see [`copies`]).
  ghosts: Vec<BodyId>,
  /// Mirror's plane or Circular Pattern's axis as last picked from the selection: only a new
  /// pick replaces the one in the dialog (see [`copies`]).
  last_pick: Value,
  /// Scale: the point it scales about (world mm), once picked or filled in from the first body.
  point: Option<[f64; 3]>,
  /// Another command this dialog switches to (a construction dialog's Type or Method): the app
  /// cancels this one and starts that.
  pub(crate) switch_to: Option<Kind>,
  /// INSERT MESH's file, unit and placement (see [`insert_mesh`]).
  pub(crate) mesh: insert_mesh::Options,
  /// An INSPECT analysis's settings (see [`analysis`]).
  an: analysis::AnalysisForm,
  /// The Flange dialog's Orientation and open groups (see [`flange`]).
  flange: flange::State,
  /// Plane Cut's plane and settings (see [`plane_cut`]).
  plane_cut: plane_cut::State,
}

const CORNERS: [(&str, &str); 5] =
  [("miter", "Miter"), ("open", "Open"), ("closed", "Closed"), ("overlap", "Overlap"), ("butt", "Butt")];
const COMPONENT_TYPES: [(&str, &str); 2] = [("standard", "Standard"), ("sheet_metal", "Sheet Metal")];
const COMPONENT_FROM: [(&str, &str); 2] = [("empty", "Empty Component"), ("bodies", "From Bodies")];
const CORNER_RELIEFS: [(&str, &str); 4] =
  [("none", "None"), ("round", "Round"), ("square", "Square"), ("tear", "Tear")];

const VIEWS: [(&str, &str); 7] = [
  ("front", "Front"),
  ("top", "Top"),
  ("right", "Right"),
  ("iso", "Isometric"),
  ("back", "Back"),
  ("bottom", "Bottom"),
  ("left", "Left"),
];
const VIEW_KINDS: [(&str, &str); 10] = [
  ("front", "Front"),
  ("top", "Top"),
  ("right", "Right"),
  ("iso", "Isometric"),
  ("back", "Back"),
  ("bottom", "Bottom"),
  ("left", "Left"),
  ("section", "Section"),
  ("detail", "Detail"),
  ("flat", "Flat pattern"),
];
const SHEETS: [(&str, &str); 7] =
  [("a4", "A4"), ("a3", "A3"), ("a2", "A2"), ("a1", "A1"), ("a0", "A0"), ("letter", "Letter"), ("tabloid", "Tabloid")];
const PROJECTIONS: [(&str, &str); 2] = [("third_angle", "Third angle"), ("first_angle", "First angle")];

impl Dialog {
  fn blank(kind: Kind) -> Dialog {
    Dialog {
      kind,
      editing: None,
      rolled: None,
      repicks: false,
      previewed: None,
      applied: false,
      error: None,
      dirty: false,
      closed: false,
      ok_requested: false,
      sketch: None,
      profiles: Vec::new(),
      edges: Vec::new(),
      faces: Vec::new(),
      bodies: Vec::new(),
      plane: json!("xy"),
      plane_label: "XY plane".into(),
      distance: "10 mm".into(),
      distance2: "10 mm".into(),
      extent: "one_side",
      flip: false,
      op: "new_body",
      angle: "360 deg".into(),
      angle2: "0 deg".into(),
      axis: "z".into(),
      radius: "2 mm".into(),
      count: "3".into(),
      spacing: "20 mm".into(),
      dir: "x".into(),
      use_dir2: false,
      count2: "2".into(),
      spacing2: "20 mm".into(),
      dir2: "y".into(),
      pattern_extent: false,
      symmetric: [false; 2],
      join: true,
      a: "20 mm".into(),
      b: "20 mm".into(),
      c: "20 mm".into(),
      at: ["0".into(), "0".into()],
      translate: ["0".into(), "0".into(), "0".into()],
      keep_tools: false,
      sections: Vec::new(),
      path: None,
      curves: Vec::new(),
      line: None,
      mirror_features: Vec::new(),
      origins: Vec::new(),
      comps: Vec::new(),
      choice: "",
      position: "",
      rule: String::new(),
      rules: Vec::new(),
      flag: false,
      flag2: false,
      d: String::new(),
      e: String::new(),
      name: String::new(),
      report: None,
      target: None,
      view: None,
      xyz: ["0".into(), "0".into(), "0".into()],
      view_on: [true, true, true, true, false, false, false],
      form: match kind {
        Kind::Form(f) => crate::forms::State::new(f),
        _ => crate::forms::State::default(),
      },
      solid: solid::Extra::default(),
      corners: "miter",
      corner_relief: "none",
      corner_relief_size: String::new(),
      unit: Default::default(),
      saved: [String::new(), String::new()],
      trace: (kind == Kind::TraceImage).then(Box::default),
      manip: manip::Cache::default(),
      saved_filter: None,
      palette: Default::default(),
      mv: move_copy::MoveForm::default(),
      live: live::Live::for_kind(kind),
      follow: false,
      sheet_picks: 0,
      comp_name: String::new(),
      summary: None,
      made: Vec::new(),
      docked: None,
      pick_row: 0,
      center: None,
      dir_lines: Vec::new(),
      focus_text: false,
      place: primitive::Step::Done,
      redefine: redefine::State::default(),
      dxf: export_dxf::Options::default(),
      flat: sheet_flat::State::default(),
      switch: None,
      ghosts: Vec::new(),
      last_pick: Value::Null,
      point: None,
      switch_to: None,
      mesh: insert_mesh::Options::default(),
      an: analysis::AnalysisForm::new(match kind {
        Kind::Analysis(k) => k,
        _ => "",
      }),
      flange: flange::State::default(),
      plane_cut: plane_cut::State::default(),
    }
  }

  /// Presets the type (drawing views): the toolbar's Base / Section / Detail View entries.
  pub fn set_choice(&mut self, choice: &'static str) {
    self.choice = choice;
    self.dirty = true;
  }

  /// Points a joint dialog at a joint (browser double-click).
  pub fn set_target(&mut self, s: &Session, target: u32) {
    self.target = Some(target);
    self.load_joint(s);
  }

  /// Does this dialog preview on demand (see [`Kind::picks_model`])?
  fn picks(&self) -> bool {
    self.kind.picks_model()
      || (self.kind == Kind::Flange && self.choice == "edge")
      || self.hole_single()
      || self.extrudes_faces()
  }

  /// Does this Extrude take planar body faces as its profiles (none of a sketch's picked)? Its
  /// preview then changes the very body picked, so it previews as the model-picking dialogs do.
  fn extrudes_faces(&self) -> bool {
    self.kind == Kind::Extrude && self.profiles.is_empty() && !self.faces.is_empty()
  }

  /// Opens a dialog for a new feature, capturing the current selection.
  pub fn open(app: &mut App, kind: Kind) -> Dialog {
    // Appearance (A) opens Fusion's panel of swatches rather than its command form
    let kind = match kind {
      Kind::Form(f) if std::ptr::eq(f, &crate::forms::APPEARANCE) => Kind::Appearance,
      // Press Pull (Q) is the command for what is picked: Extrude on a profile, Fillet on an edge,
      // Offset Face on a face
      Kind::PressPull => press_pull_kind(&app.s),
      k => k,
    };
    let mut d = Dialog::blank(kind);
    let s = &app.s;
    // profiles → sketch
    for sel in &s.selection {
      if let Sel::Profile { sketch, index } = sel
        && let Some(r) = s.model.sketches.get(sketch)
        && let Some(p) = r.profiles.get(*index)
      {
        d.sketch = Some(*sketch);
        d.profiles.push(p.interior_point());
      }
    }
    if matches!(kind, Kind::Extrude | Kind::Revolve | Kind::Sweep | Kind::Hole | Kind::Flange | Kind::Fold)
      && d.sketch.is_none()
    {
      // the newest sketch's profiles are the ones to pick; Extrude takes only a picked one (as
      // in Fusion, E opens with Profiles: Select and no preview)
      d.sketch = s.edit_sketch.or(app.last_sketch).or_else(|| last_unconsumed_sketch(s));
      if !matches!(kind, Kind::Extrude | Kind::Sweep | Kind::Flange)
        && let Some(sk) = d.sketch
        && let Some(r) = s.model.sketches.get(&sk)
        && r.profiles.len() == 1
      {
        d.profiles = vec![r.profiles[0].interior_point()];
      }
    }
    for sel in &s.selection {
      match sel {
        Sel::Edge { body, edge } => d.edges.push(json!({"body": body.0, "edge": edge})),
        Sel::Face { body, face } => {
          d.faces.push(json!({"body": body.0, "face": face}));
          if !d.bodies.contains(&body.0) {
            d.bodies.push(body.0);
          }
        }
        Sel::Body { body } if !d.bodies.contains(&body.0) => {
          d.bodies.push(body.0);
        }
        _ => {}
      }
    }
    // a selected planar face becomes the plane for primitives / offset planes / mirror
    if let Some(Sel::Face { body, face }) = s.selection.iter().find(|x| matches!(x, Sel::Face { .. }))
      && is_planar(s, *body, *face)
    {
      d.plane = json!({"body": body.0, "face": face});
      d.plane_label = format!("Face of {}", s.body_name(*body));
    }
    match kind {
      // Fusion starts a fillet's radius and a shell's thickness at 0.00, previewing nothing until
      // a value is typed or the arrow dragged
      Kind::Fillet => d.radius = "0 mm".into(),
      Kind::Chamfer => d.radius = "1 mm".into(),
      Kind::Shell => d.radius = "0 mm".into(),
      // Fusion's defaults for the values not drawn (0.10 in high, a 0.40 in sphere, a 0.20 in
      // tube); the drawing sets the rest
      Kind::Box => d.c = "2.54 mm".into(),
      Kind::Cylinder => d.b = "2.54 mm".into(),
      Kind::Sphere => d.a = "10.16 mm".into(),
      Kind::Torus => {
        d.b = "5.08 mm".into();
        d.position = "center";
      }
      // Fusion's Axis starts empty: no preview until one is picked
      Kind::Revolve => d.axis = String::new(),
      Kind::Mirror => {
        d.join = false;
        d.choice = "bodies";
        // Fusion: Objects take picked bodies, and the Mirror Plane starts empty (Select)
        d.bodies = sel_only_bodies(s);
        d.plane = Value::Null;
        d.plane_label = String::new();
      }
      Kind::RectPattern => d.join = false,
      Kind::CircPattern => d.circ_defaults(s),
      Kind::Combine => {
        // the first picked body is the target, the others tools; the Tool Bodies row is active
        // once there is a target, as in Fusion
        d.op = "join";
        d.faces.clear();
        d.edges.clear();
        d.target = (!d.bodies.is_empty()).then(|| d.bodies.remove(0));
        d.pick_row = usize::from(d.target.is_some());
      }
      // Move turns by nothing until asked (the blank dialog's angle is a full turn)
      Kind::Move => {
        d.angle = "0 deg".into();
        // in an open sketch the sketch objects move (as SKETCH > MODIFY > Move/Copy), unless
        // bodies are picked
        if s.edit_sketch.is_some() && (!sel_curves(s).is_empty() || sel_bodies(s).is_empty()) {
          d.mv.object = "sketch";
        }
      }
      Kind::Loft => {
        d.sections = sel_sections(s);
      }
      // Fusion's Profile and Path start empty: both are picked in the canvas
      Kind::Sweep => {}
      Kind::Hole => {
        d.choice = "simple";
        d.a = "5 mm".into();
        d.b = "10 mm".into();
        d.c = "9 mm".into();
        d.d = "3 mm".into();
        d.e = "90 deg".into();
        d.op = "cut";
        d.hole_defaults(s);
      }
      Kind::PressPull => {
        d.distance = "2 mm".into();
        d.choice = "automatic";
      }
      // Fusion: Fixed Plane, the Pull Direction row active and nothing for it yet, Tangent Chain
      // on, One Side; the angle (0.0 until set) shows once the picks are made
      Kind::Draft => {
        d.choice = "fixed_plane";
        d.position = "one_side";
        d.flag = true;
        d.angle = "0.0 deg".into();
        d.angle2 = "0.0 deg".into();
        d.plane = Value::Null;
        d.plane_label = String::new();
        d.pick_row = 0;
      }
      // Fusion: Entities active and nothing picked (no all-bodies default), Uniform, factor 1.00
      // once a body is picked
      Kind::Scale => {
        d.choice = "uniform";
        d.a = "1.00".into();
        d.b = "1.00".into();
        d.c = "1.00".into();
        d.bodies.clear();
        for x in &s.selection {
          if let Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } = x
            && !d.bodies.contains(&body.0)
          {
            d.bodies.push(body.0);
          }
        }
        d.point = d.bodies.first().and_then(|b| body_centre(s, *b));
      }
      Kind::SplitBody => {
        // Body to Split takes one body (the picked one), then Splitting Tool(s) is active; no
        // tool until one is picked
        d.bodies = sel_bodies(s).into_iter().take(1).collect();
        d.faces.clear();
        d.plane = Value::Null;
        d.pick_row = usize::from(!d.bodies.is_empty());
      }
      // the body: a picked body, or the body of a picked face (a mesh's faces aren't planes to cut with)
      Kind::PlaneCut => {
        d.bodies = sel_bodies(s).into_iter().take(1).collect();
        d.faces.clear();
      }
      Kind::Interference => d.flag = true,
      Kind::DriveJoint | Kind::MotionStudy => {
        d.target = joints(s).first().map(|(id, _)| *id);
        d.position = "rotation";
        d.a = "-45 deg".into();
        d.b = "45 deg".into();
        d.count = "8".into();
        d.load_joint(s);
      }
      Kind::CopyComponent => {
        d.target = sel_bodies(s)
          .iter()
          .find_map(|b| s.design.component_of(BodyId(*b)).map(|c| c.id.0))
          .or_else(|| s.design.components.first().map(|c| c.id.0));
        d.translate = ["50 mm".into(), "0 mm".into(), "0 mm".into()];
      }
      Kind::NewDrawing => {
        d.name = format!("Drawing{}", s.design.drawings.len() + 1);
        d.position = "a3";
        d.extent = "asme";
        d.choice = "third_angle";
        d.a = "auto".into();
        d.flag = true;
        d.flag2 = true;
      }
      Kind::DrawingView => {
        d.choice = "front";
        d.axis = "x".into();
        d.distance = "0".into();
        d.radius = "10".into();
        d.count = "2".into();
        d.flag = true;
        d.at = [String::new(), String::new()];
        if let Some(e) = sel_edges(s).first() {
          d.xyz = edge_point(s, e, false).map(|v| v.map(|x| format!("{x:.3}"))).unwrap_or(d.xyz.clone());
        }
      }
      Kind::DrawingDimension => {
        d.choice = "aligned";
        d.distance = "10".into();
        if let Some(e) = sel_edges(s).first() {
          d.translate = edge_point(s, e, true).map(|v| v.map(|x| format!("{x:.3}"))).unwrap_or(d.translate.clone());
          d.xyz = edge_point(s, e, false).map(|v| v.map(|x| format!("{x:.3}"))).unwrap_or(d.xyz.clone());
        }
      }
      Kind::DrawingNote => {
        d.name = "NOTE".into();
        d.at = ["20".into(), "20".into()];
      }
      Kind::Flange => d.flange_defaults(s),
      // Fusion's flat hem, four thicknesses long, its bend at the edge
      Kind::Hem => {
        d.choice = "flat";
        d.position = "adjacent";
        d.distance = sheet::HEM_LENGTH.into();
        d.distance2 = sheet::HEM_GAP.into();
      }
      Kind::Fold => {
        d.angle = "90 deg".into();
        d.line = d.sketch.and_then(|sk| s.design.sketch(sk).ok()).and_then(|sk| {
          sk.curves.iter().rev().find(|c| matches!(c.kind, fox_engine::sketch::CurveKind::Line { .. })).map(|c| c.id.0)
        });
      }
      Kind::Rip => d.distance = "0.5 mm".into(),
      Kind::SheetRules => {
        d.load_rule(s, "");
      }
      Kind::NewComponent => {
        d.name = format!("Component{}", s.design.components.len() + 1);
        d.position = "standard";
        // Fusion starts empty; selected bodies make it From Bodies
        d.choice = if sel_bodies(s).is_empty() { "empty" } else { "bodies" };
        d.flag2 = true;
      }
      Kind::AsBuiltJoint => {
        d.choice = "rigid";
        d.origins = sel_as_built(s);
        d.e = "2 mm".into();
      }
      Kind::Joint => {
        d.choice = "rigid";
        d.angle = "0 deg".into();
        d.distance = "0 mm".into();
        d.origins = sel_origins(s);
        d.e = "2 mm".into();
      }
      Kind::SkFillet => d.radius = "2 mm".into(),
      Kind::SkChamfer | Kind::SkChamfer2 | Kind::SkChamferAngle => {
        d.distance = "2 mm".into();
        d.distance2 = "3 mm".into();
        d.angle = "45 deg".into();
      }
      Kind::SkOffset => {
        // Fusion starts the offset at 0.00 on the curve; Two Sides adds a second distance
        d.distance = "0 mm".into();
        d.distance2 = "0 mm".into();
        d.follow = true;
      }
      Kind::SkCircPattern => {
        // Fusion: three instances all the way round (Distribution "Full")
        d.count = "3".into();
        d.angle = "360 deg".into();
        d.choice = "full";
      }
      Kind::SkScale => d.a = "1".into(),
      Kind::SkRectPattern => {
        // Fusion: two directions of three instances over an extent of 0 until a direction is
        // picked and the distance set
        d.count = "3".into();
        d.count2 = "3".into();
        d.spacing = "0 mm".into();
        d.spacing2 = "0 mm".into();
        d.pattern_extent = true;
        d.use_dir2 = true;
      }
      Kind::Project => {
        d.flag = true;
        // Fusion's Selection Filter opens on Specified Entities (faces and edges)
        d.choice = "specified";
      }
      Kind::Extrude => {
        // extruding from a face sketch defaults to Join, like Fusion
        if let Some(sk) = d.sketch
          && let Some(FeatureKind::Sketch { plane: PlaneRef::Face { .. }, .. }) = s.design.feature(sk).map(|f| &f.kind)
        {
          d.op = "join";
        }
        // selected faces become the profiles on the first selection sync (planar ones only)
        d.faces.clear();
      }
      _ => {}
    }
    if matches!(
      kind,
      Kind::SkFillet
        | Kind::SkChamfer
        | Kind::SkChamfer2
        | Kind::SkChamferAngle
        | Kind::SkOffset
        | Kind::SkMirror
        | Kind::SkCircPattern
        | Kind::SkRectPattern
        | Kind::SkScale
    ) {
      d.curves = sel_curves(s);
      if kind == Kind::SkMirror {
        d.line = mirror_line(s, &d.curves);
        d.curves.retain(|c| Some(*c) != d.line);
      }
      // Offset on pre-selected curves, as in Fusion: those curves alone (Chain Selection off) and
      // the offset on them at 0.00 until the arrow is dragged or a distance typed, instead of
      // following the pointer
      if kind == Kind::SkOffset && !d.curves.is_empty() {
        d.follow = false;
        d.solid.no_chain = true;
      }
    }
    d.comps = d.bodies.iter().map(|b| s.design.component_of(BodyId(*b)).map(|c| c.id.0)).collect();
    if matches!(kind, Kind::Joint | Kind::AsBuiltJoint) {
      d.comps = d.origins.iter().map(|(b, _)| s.design.component_of(BodyId(*b)).map(|c| c.id.0)).collect();
    }
    if kind.drawing() {
      d.target = app.drawing;
      d.load_drawing(s);
    }
    if matches!(kind, Kind::Flange | Kind::ConvertSheet) {
      d.rules = s.design.sheet_metal_rules.iter().map(|r| r.name.clone()).collect();
    }
    // Fusion finishes the sketch when a feature starts from it
    if matches!(kind, Kind::Extrude | Kind::Revolve | Kind::Loft | Kind::Sweep | Kind::Hole | Kind::Flange | Kind::Fold)
      && app.s.edit_sketch.is_some()
    {
      app.last_sketch = app.s.edit_sketch;
      app.left_sketch();
      let _ = app.s.execute("sketch.finish", &json!({}));
    }
    if kind.reports() {
      d.refresh_report(&mut app.s);
    }
    if let Kind::Form(f) = kind {
      d.form.sync(f, &app.s, app.last_sketch.or_else(|| last_unconsumed_sketch(&app.s)));
      crate::forms::prefill(f, &mut d.form, &app.s);
    }
    if let Kind::Form(f) = kind
      && f.cmd.starts_with("drawing.")
    {
      d.form.set_drawing(f, &app.s, app.drawing);
    }
    if kind == Kind::Section {
      d.section_open(&app.s);
    }
    if let Kind::Analysis(_) = kind {
      // nothing is drawn until a pick (see `inspect_analysis_rows`)
      app.analysis = None;
    }
    d.start_placing(app);
    d.localize(app.s.design.units);
    if kind == Kind::PressPull {
      d.sync_press_owner(&app.s);
    }
    // Fusion's Offset Face starts at 0.00 in the document unit
    if kind == Kind::OffsetFace {
      d.distance = d.unit.fixed(0.0, 2);
    }
    d.dirty = !d.picks();
    d
  }

  /// A new dialog's millimetre defaults in the design's unit, as Fusion fills its boxes in the
  /// document unit (`10 mm` → `0.393701 in`).
  fn localize(&mut self, unit: fox_engine::model::LengthUnit) {
    self.unit = unit;
    if unit.is_mm() {
      return;
    }
    let fields = [
      &mut self.distance,
      &mut self.distance2,
      &mut self.radius,
      &mut self.spacing,
      &mut self.spacing2,
      &mut self.a,
      &mut self.b,
      &mut self.c,
      &mut self.d,
      &mut self.e,
    ];
    for f in fields.into_iter().chain(self.translate.iter_mut()).chain(self.xyz.iter_mut()) {
      *f = unit.localize(f);
    }
    for v in self.form.values.iter_mut() {
      match v {
        Value::String(s) => *s = unit.localize(s),
        Value::Array(a) => {
          for x in a {
            if let Value::String(s) = x {
              *s = unit.localize(s);
            }
          }
        }
        _ => {}
      }
    }
  }

  /// Opens the dialog for an existing feature (double-click in the timeline).
  pub fn edit(app: &App, id: FeatureId) -> Option<Dialog> {
    let f = app.s.design.feature(id)?;
    let mut d = match &f.kind {
      FeatureKind::Extrude { sketch, profiles, faces, extent, operation } => {
        let mut d = Dialog::blank(Kind::Extrude);
        d.sketch = *sketch;
        d.profiles = profiles.clone();
        // the extruded faces where the model has them (for the drag arrow)
        d.faces = faces
          .iter()
          .filter_map(|f| {
            let id = app.s.model.body(f.body)?.find_face(f)?;
            Some(json!({"body": f.body.0, "face": id}))
          })
          .collect();
        // a flipped extrude shows a negative distance, as in Fusion (`extrude::split_sign`)
        d.distance = if extent.flip { extrude::negate(&extent.distance) } else { extent.distance.clone() };
        d.distance2 = extent.distance2.clone();
        d.flip = false;
        d.extent = match extent.kind {
          ExtentKind::OneSide => "one_side",
          ExtentKind::Symmetric => "symmetric",
          ExtentKind::TwoSides => "two_sides",
          ExtentKind::ThroughAll => "through_all",
          ExtentKind::ThroughAllBoth => "through_all_both",
          ExtentKind::ToFace | ExtentKind::ToBody | ExtentKind::ToVertex => "to_object",
        };
        match extent.kind {
          ExtentKind::ToFace => d.load_object("to_face", serde_json::to_value(&extent.to).unwrap_or_default()),
          ExtentKind::ToBody => d.load_object("to_body", serde_json::to_value(&extent.to).unwrap_or_default()),
          ExtentKind::ToVertex => {
            d.load_object("to_vertex", serde_json::to_value(&extent.to_point).unwrap_or_default());
          }
          _ => {}
        }
        d.load_extrude_rows(extent);
        d.op = op_str(*operation);
        d
      }
      FeatureKind::Revolve { sketch, angle, symmetric, angle2, operation, axis, .. } => {
        let mut d = Dialog::blank(Kind::Revolve);
        d.sketch = Some(*sketch);
        d.angle = angle.clone();
        d.extent = match (symmetric, angle2) {
          (true, _) => "symmetric",
          (_, Some(a2)) => {
            d.angle2 = a2.clone();
            "two_sides"
          }
          _ => "one_side",
        };
        d.solid.rev_full = angle.trim() == "360 deg";
        d.op = op_str(*operation);
        d.axis = axis_key(axis);
        d
      }
      FeatureKind::Fillet { radius, edges, setback, chord, sets, radius2, .. } => {
        let mut d = Dialog::blank(Kind::Fillet);
        d.radius = radius.clone();
        d.load_setback(setback, *chord, radius2);
        d.load_fillet_sets(sets);
        d.edges = vec![Value::Null; edges.len()];
        d
      }
      FeatureKind::Chamfer { distance, edges, distance2, angle, .. } => {
        let mut d = Dialog::blank(Kind::Chamfer);
        d.radius = distance.clone();
        d.load_chamfer(distance2, angle);
        d.edges = vec![Value::Null; edges.len()];
        d
      }
      FeatureKind::Shell { thickness, faces, round, direction, thickness_out, bodies } => {
        let mut d = Dialog::blank(Kind::Shell);
        d.radius = thickness.clone();
        d.flag = *round;
        d.load_shell(*direction, thickness_out, bodies.len());
        d.faces = vec![Value::Null; faces.len()];
        d
      }
      FeatureKind::Primitive { shape, plane, at, operation } => {
        let kind = match shape {
          Primitive::Box { .. } => Kind::Box,
          Primitive::Cylinder { .. } => Kind::Cylinder,
          Primitive::Sphere { .. } => Kind::Sphere,
          Primitive::Torus { .. } => Kind::Torus,
        };
        let mut d = Dialog::blank(kind);
        d.load_primitive(shape);
        d.at = [fox_engine::sketch::Dim::new(at[0]).expr, fox_engine::sketch::Dim::new(at[1]).expr];
        d.op = op_str(*operation);
        d.plane_label = placement_label(app, plane);
        d
      }
      FeatureKind::ConstructionPlane { offset, .. } => {
        let mut d = Dialog::blank(Kind::OffsetPlane);
        d.distance = offset.clone();
        d
      }
      FeatureKind::RectangularPattern {
        count,
        spacing,
        count2,
        spacing2,
        direction2,
        symmetric,
        symmetric2,
        extent,
        join,
        ..
      } => {
        let mut d = Dialog::blank(Kind::RectPattern);
        d.count = count.clone();
        d.spacing = spacing.clone();
        d.count2 = count2.clone();
        d.spacing2 = spacing2.clone();
        d.use_dir2 = direction2.is_some();
        d.symmetric = [*symmetric, *symmetric2];
        d.pattern_extent = *extent;
        d.join = *join;
        d
      }
      FeatureKind::CircularPattern { count, angle, symmetric, join, .. } => {
        let mut d = Dialog::blank(Kind::CircPattern);
        d.count = count.clone();
        d.angle = angle.clone();
        d.symmetric[0] = *symmetric;
        d.join = *join;
        d
      }
      kind @ FeatureKind::Move { .. } => {
        let mut d = Dialog::blank(Kind::Move);
        d.unit = app.s.design.units;
        d.load_move(&app.s, kind);
        d
      }
      kind @ FeatureKind::Draft { .. } => {
        let mut d = Dialog::blank(Kind::Draft);
        d.load_draft(kind);
        d
      }
      kind @ (FeatureKind::Emboss { .. } | FeatureKind::BoundaryFill { .. } | FeatureKind::ReplaceFace { .. }) => {
        solid::edit_form(kind)?
      }
      kind @ (FeatureKind::Hole { .. }
      | FeatureKind::Mirror { .. }
      | FeatureKind::Combine { .. }
      | FeatureKind::OffsetFaces { .. }) => edit_more::load(&app.s, kind)?,
      kind @ (FeatureKind::Flange { .. }
      | FeatureKind::ContourFlange { .. }
      | FeatureKind::EdgeFlange { .. }
      | FeatureKind::Hem { .. }) => flange::edit_dialog(app, kind)?,
      _ => return None,
    };
    d.editing = Some(id);
    d.unit = app.s.design.units;
    d.load_edit_anchor(&f.kind);
    d.load_flange_edit_anchor(&app.s, &f.kind);
    // a stored bare length is mm, and an edit box always shows the unit, as Fusion's do ("2 mm")
    let lengths = match d.kind {
      Kind::Box | Kind::Cylinder | Kind::Sphere | Kind::Torus => 3,
      Kind::Hole => 4,
      _ => 0,
    };
    let s = &mut d.solid;
    let fields = [&mut d.distance, &mut d.distance2, &mut d.radius, &mut d.spacing, &mut d.spacing2];
    let more = [&mut s.distance2, &mut s.radius2, &mut s.thickness_out];
    let abc = [&mut d.a, &mut d.b, &mut d.c, &mut d.d].into_iter().take(lengths);
    let rows = s.rows.iter_mut().map(|r| &mut r.radius);
    for f in fields.into_iter().chain(more).chain(abc).chain(rows).chain(d.translate.iter_mut()) {
      *f = fox_engine::model::LengthUnit::pinned(f);
    }
    Some(d)
  }

  /// Edit Feature rolls the design back to the edited feature while its dialog is open, as
  /// Fusion does: the marker moves to just after it, the later features roll back (faded in the
  /// timeline, their bodies gone), and OK or Cancel rolls forward again. It all happens in a
  /// transaction, so OK leaves one undo step and Cancel restores the design as it was. Fillet,
  /// Chamfer and Extrude find their picks again on the model before the feature, so their
  /// rows are the creation dialog's, live.
  pub(crate) fn begin_edit(&mut self, s: &mut Session) {
    let Some(id) = self.editing else { return };
    let Some(i) = s.design.index_of(id) else { return };
    if s.begin_transaction().is_err() {
      return;
    }
    let old = s.design.marker;
    if s.execute("timeline.marker", &json!({"at": i})).is_err() {
      let _ = s.rollback_transaction();
      return;
    }
    self.rolled = Some(old);
    self.repicks = self.load_edit_picks(s);
    self.preview(s);
  }

  /// Ends Edit Feature's rolled-back view: OK (`keep`) rolls the marker forward to where it was
  /// and keeps the edit as one undo step; Cancel restores the design.
  fn end_edit(&mut self, s: &mut Session, keep: bool) {
    let Some(old) = self.rolled.take() else { return };
    if keep {
      let _ = s.execute("timeline.marker", &json!({"at": old}));
      let _ = s.commit_transaction_named("Edit Feature");
    } else {
      let _ = s.rollback_transaction();
    }
  }

  /// Finds the edited feature's picks on the rolled-back model: a fillet's or chamfer's edges
  /// (every set's) and an extrude's profiles, selected as the creation dialog shows them. False
  /// when one can't be found (the rows then keep the stored picks unchanged).
  fn load_edit_picks(&mut self, s: &mut Session) -> bool {
    let Some(kind) = self.editing.and_then(|id| s.design.feature(id)).map(|f| f.kind.clone()) else {
      return false;
    };
    let find = |er: &fox_engine::model::EdgeRef| -> Option<Value> {
      let e = s.model.body(er.body)?.find_edge(er)?;
      Some(json!({"body": er.body.0, "edge": e}))
    };
    let all = |refs: &[fox_engine::model::EdgeRef]| refs.iter().map(find).collect::<Option<Vec<Value>>>();
    match kind {
      FeatureKind::Fillet { edges, sets, .. } => {
        let Some(first) = all(&edges) else { return false };
        let Some(rest) = sets.iter().map(|set| all(&set.edges)).collect::<Option<Vec<_>>>() else { return false };
        for (row, picks) in self.solid.rows.iter_mut().skip(1).zip(rest) {
          row.edges = picks;
        }
        if let Some(row) = self.solid.rows.first_mut() {
          row.edges = first.clone();
        }
        self.edges = first;
      }
      FeatureKind::Chamfer { edges, .. } => {
        let Some(picks) = all(&edges) else { return false };
        self.edges = picks;
      }
      FeatureKind::Extrude { sketch, profiles, .. } => {
        // faces extruded keep their picks
        let Some(sketch) = sketch else { return false };
        let Some(r) = s.model.sketches.get(&sketch) else { return false };
        // the regions the stored points fall in (none stored: every region)
        let mut picked = Vec::new();
        for p in &profiles {
          let Some(index) = r.profiles.iter().position(|x| x.contains(*p)) else { return false };
          picked.push((index, r.profiles[index].interior_point()));
        }
        if profiles.is_empty() {
          picked = r.profiles.iter().enumerate().map(|(i, x)| (i, x.interior_point())).collect();
        }
        self.profiles = picked.iter().map(|(_, p)| *p).collect();
        s.selection = picked.iter().map(|(index, _)| Sel::Profile { sketch, index: *index }).collect();
        return true;
      }
      _ => return false,
    }
    s.selection = self.picked_selection();
    true
  }

  /// Are the dialog's picks fixed? An edited feature's are, unless Edit Feature found them again
  /// on the rolled-back model ([`Dialog::begin_edit`]).
  fn picks_fixed(&self) -> bool {
    self.editing.is_some() && !self.repicks
  }

  /// Did the live preview fail (the feature computed, with an error)? OK greys and the error
  /// toast says why, as in Fusion.
  pub(crate) fn failed(&self) -> bool {
    self.applied && self.error.is_some()
  }

  /// The feature the showing preview made or edited (the error toast names its message rather
  /// than its name).
  pub(crate) fn own_feature(&self) -> Option<FeatureId> {
    self.previewed.filter(|_| self.applied)
  }

  /// The command (name, params) this dialog applies.
  fn command(&self) -> Option<(&'static str, Value)> {
    self.with_comp_name(self.command_base()).map(|(c, p)| (c, self.with_cut_targets(p)))
  }

  fn command_base(&self) -> Option<(&'static str, Value)> {
    if let Some(id) = self.editing {
      let mut p = json!({"feature": id.0, "set": self.edit_set()?});
      if self.rolled.is_some() {
        p["marker_after"] = json!(true);
      }
      if self.repicks {
        match self.kind {
          Kind::Fillet => p["picks"] = self.fillet_params(),
          Kind::Chamfer => p["picks"] = json!({"edges": self.edges, "tangent_chain": !self.solid.no_chain}),
          _ => {}
        }
      }
      return Some(("feature.edit", p));
    }
    let op = self.op;
    Some(match self.kind {
      Kind::Form(f) => crate::forms::engine_command(f, self.form.params(f)),
      Kind::PickPlane
      | Kind::Inspect
      | Kind::Appearance
      | Kind::Material
      | Kind::Interference
      | Kind::Bom
      | Kind::CenterOfMass
      | Kind::Section
      | Kind::Analysis(_) => {
        return None;
      }
      Kind::FlatPattern => ("sheet.create_flat_pattern", json!({"face": self.faces.first()?})),
      Kind::Loft => (
        "loft",
        self.with_guides(json!({
          "sections": self.sections.iter().map(|(sk, p)| json!({"sketch": sk, "profile": p})).collect::<Vec<_>>(),
          "ruled": self.flag, "closed": self.flag2, "operation": op,
        })),
      ),
      Kind::Sweep => (
        "sweep",
        self.with_guides(
          json!({"sketch": self.sketch?.0, "profiles": self.profiles, "path": self.path?.0, "operation": op}),
        ),
      ),
      Kind::Hole => ("hole", self.hole_params()?),
      Kind::PressPull => self.press_pull_command(),
      Kind::OffsetFace => ("press_pull", json!({"faces": self.faces, "distance": self.distance})),
      Kind::Draft => ("draft", self.draft_params()),
      Kind::Scale => {
        let mut p = json!({"bodies": self.bodies, "center": self.point?});
        if self.choice == "non_uniform" {
          p["factors"] = json!([self.a, self.b, self.c]);
        } else {
          p["factor"] = json!(self.a);
        }
        ("scale", p)
      }
      Kind::SplitBody if self.target.is_some() => {
        ("split_by_body", json!({"tool": self.target?, "bodies": [self.bodies.first()?]}))
      }
      Kind::SplitBody => {
        if self.plane.is_null() {
          return None;
        }
        ("split", json!({"plane": self.plane, "bodies": [self.bodies.first()?]}))
      }
      Kind::Flange => return self.flange_command(),
      Kind::Hem => {
        let gap = if self.choice == "open" { self.distance2.as_str() } else { sheet::FLAT_HEM_GAP };
        ("sheet.hem", json!({"edges": self.edges, "length": self.distance, "gap": gap, "flip": self.flip}))
      }
      Kind::Fold => (
        "sheet.bend",
        json!({"sketch": self.sketch?.0, "line": self.line?, "angle": self.angle, "flip": self.flip, "flip_side": self.flag}),
      ),
      Kind::ConvertSheet => {
        let mut p = json!({"body": self.bodies.first()?});
        if let Some(f) = self.faces.first() {
          p["face"] = f.clone();
        }
        self.put_rule(&mut p);
        ("sheet.convert", p)
      }
      Kind::Rip => ("sheet.rip", json!({"edges": self.edges, "gap": self.distance})),
      Kind::Unfold => ("sheet.unfold", json!({"body": self.bodies.first()?, "face": self.faces.first()?})),
      Kind::SheetRules => (
        "sheet.rule",
        json!({
          "name": self.name, "thickness": self.a, "bend_radius": self.b, "k_factor": self.c,
          "relief": self.position, "relief_width": self.d, "relief_depth": self.e, "gap": self.distance2,
          "corner_relief": self.corner_relief, "corner_relief_size": self.corner_relief_size,
        }),
      ),
      Kind::NewComponent => {
        // Activate is applied on OK (see `show`): previews come and go
        let bodies: &[u32] = if self.choice == "empty" { &[] } else { &self.bodies };
        let kind = if self.position.is_empty() { "standard" } else { self.position };
        (
          "component.create",
          json!({
            "name": self.name, "bodies": bodies, "type": kind, "part_number": self.d,
            "description": self.e, "parent": self.target,
          }),
        )
      }
      Kind::ComponentProps => (
        "component.properties",
        json!({
          "component": self.target?, "name": self.name, "part_number": self.d, "description": self.e,
          "type": self.position,
        }),
      ),
      Kind::AsBuiltJoint => {
        let (moving, fixed, at) = self.as_built_picks()?;
        let mut p = json!({
          "moving": moving, "fixed": fixed, "moving_origin": at, "fixed_origin": at, "kind": self.choice,
          "as_built": true,
        });
        if self.choice == "screw" {
          p["lead"] = json!(self.e);
        }
        ("joint", p)
      }
      Kind::Joint => {
        // a saved joint origin (by name) stands in for that side's pick
        let mut picks = self.origins.iter().map(|(_, o)| o).zip(&self.comps);
        let mut side = |saved: &str| {
          if saved.is_empty() {
            picks.next().map(|(o, c)| (o.clone(), *c))
          } else {
            Some((json!({"origin": saved}), None))
          }
        };
        let (mo, mc) = side(&self.saved[0])?;
        let (fo, fc) = side(&self.saved[1])?;
        let mut p = json!({
          "moving_origin": mo, "fixed_origin": fo, "kind": self.choice, "angle": self.angle, "offset": self.distance,
          "flip": self.flip,
        });
        if self.saved[0].is_empty() {
          p["moving"] = json!(mc?);
        }
        if self.saved[1].is_empty() {
          p["fixed"] = json!(fc);
        }
        if self.choice == "screw" {
          p["lead"] = json!(self.e);
        }
        ("joint", p)
      }
      Kind::SkFillet => ("sketch.fillet", json!({"curves": self.curves, "radius": self.radius})),
      Kind::SkChamfer => ("sketch.chamfer", json!({"curves": self.curves, "distance": self.distance})),
      Kind::SkChamfer2 => {
        ("sketch.chamfer", json!({"curves": self.curves, "distance": self.distance, "distance2": self.distance2}))
      }
      Kind::SkChamferAngle => {
        ("sketch.chamfer", json!({"curves": self.curves, "distance": self.distance, "angle": self.angle}))
      }
      Kind::SkOffset => {
        let mut p = json!({"curves": self.curves, "distance": self.distance, "both_sides": self.extent == "symmetric"});
        if self.extent == "two_sides" {
          p["distance2"] = json!(self.distance2);
        }
        ("sketch.offset", p)
      }
      Kind::TraceImage => ("sketch.trace", self.trace.as_ref()?.params()?),
      Kind::SkMirror => ("sketch.mirror", json!({"curves": self.curves, "line": self.line?})),
      Kind::SkCircPattern => {
        // Distribution: Full is a whole turn, Angle the typed total angle, Symmetric that angle
        // shared either side of the original
        let angle = if self.choice == "full" { "360 deg".to_string() } else { self.angle.clone() };
        (
          "sketch.pattern_circ",
          json!({"curves": self.curves, "center": self.at_pt()?, "count": self.count.trim().parse::<u64>().ok()?,
            "angle": angle, "symmetric": self.choice == "symmetric"}),
        )
      }
      Kind::SkScale => ("sketch.transform", json!({"curves": self.curves, "center": self.at_pt()?, "scale": self.a})),
      Kind::SkRectPattern => {
        // nothing to pattern (or preview) until the first distance is set, as Fusion's starts at
        // 0 and draws no instances
        let zero = |t: &str| {
          fox_engine::expr::eval(t, fox_engine::expr::Kind::Length, &fox_engine::expr::Empty)
            .is_ok_and(|d| d.abs() < 1e-9)
        };
        if zero(&self.spacing) {
          return None;
        }
        let v = |a: &str| if a == "y" { [0.0, 1.0] } else { [1.0, 0.0] };
        // picked lines set the directions (the second at right angles to a lone first), else
        // the dialog's choice, else the sketch axes
        let d1 = self.dir_lines.first().map(|l| l.1).unwrap_or_else(|| v(&self.dir));
        let d2 = self
          .dir_lines
          .get(1)
          .map(|l| l.1)
          .unwrap_or_else(|| if self.dir_lines.is_empty() { v(&self.dir2) } else { [-d1[1], d1[0]] });
        let mut p = json!({
          "curves": self.curves, "direction": d1, "count": self.count.trim().parse::<u64>().ok()?,
          "spacing": self.spacing, "symmetric": self.symmetric[0],
          "distance_type": if self.pattern_extent { "extent" } else { "spacing" },
        });
        // a second distance of 0 patterns one way
        if self.use_dir2 && !zero(&self.spacing2) {
          p["direction2"] = json!(d2);
          p["count2"] = json!(self.count2.trim().parse::<u64>().ok()?);
          p["spacing2"] = json!(self.spacing2);
          p["symmetric2"] = json!(self.symmetric[1]);
        }
        ("sketch.pattern_rect", p)
      }
      Kind::DriveJoint => (
        "joint.drive",
        json!({"joint": self.target?, "rotation": self.angle, "slide": self.distance, "slide2": self.distance2}),
      ),
      Kind::MotionStudy => return None,
      Kind::CopyComponent => ("component.copy", json!({"component": self.target?, "translate": self.translate})),
      Kind::NewDrawing => {
        let views: Vec<&str> = VIEWS.iter().zip(self.view_on).filter(|(_, on)| *on).map(|((k, _), _)| *k).collect();
        (
          "drawing.create",
          json!({
            "name": self.name, "views": views, "sheet": self.position, "scale": scale_value(&self.a),
            "projection": self.choice, "auto_dimensions": self.flag, "parts_list": self.flag2,
            "standard": drawing_standard(self.extent),
          }),
        )
      }
      Kind::DrawingSettings => (
        "drawing.set",
        json!({
          "drawing": self.target?, "rename": self.name, "sheet": self.position, "scale": scale_value(&self.a),
          "projection": self.choice, "auto_dimensions": self.flag, "parts_list": self.flag2,
          "standard": drawing_standard(self.extent), "auto_balloons": self.keep_tools,
        }),
      ),
      Kind::DrawingView => {
        let num = |t: &str| t.trim().parse::<f64>().ok();
        let kind = match self.choice {
          "section" => json!({"section": self.axis, "offset": num(&self.distance)?, "flip": self.flip}),
          "flat" => json!({"flat": self.bodies.first()?}),
          "detail" => json!({
            "detail": self.view?, "center": self.xyz.iter().map(|v| num(v)).collect::<Option<Vec<_>>>()?,
            "radius": num(&self.radius)?, "factor": num(&self.count)?,
          }),
          k => json!(k),
        };
        let mut p = json!({"drawing": self.target?, "kind": kind, "hidden_lines": self.flag});
        if let (Some(x), Some(y)) = (num(&self.at[0]), num(&self.at[1])) {
          p["at"] = json!([x, y]);
        }
        ("drawing.add_view", p)
      }
      Kind::DrawingMoveView => {
        let num = |t: &str| t.trim().parse::<f64>().ok();
        let at = if self.flag { Value::Null } else { json!([num(&self.at[0])?, num(&self.at[1])?]) };
        ("drawing.move_view", json!({"drawing": self.target?, "view": self.view?, "at": at}))
      }
      Kind::DrawingDimension => {
        let pt = |v: &[String; 3]| v.iter().map(|x| x.trim().parse::<f64>().ok()).collect::<Option<Vec<_>>>();
        (
          "drawing.dimension",
          json!({
            "drawing": self.target?, "view": self.view?, "a": pt(&self.translate)?, "b": pt(&self.xyz)?,
            "orientation": self.choice, "offset": self.distance.trim().parse::<f64>().ok()?,
          }),
        )
      }
      Kind::DrawingNote => {
        let num = |t: &str| t.trim().parse::<f64>().ok();
        (
          "drawing.note",
          json!({"drawing": self.target?, "at": [num(&self.at[0])?, num(&self.at[1])?], "text": self.name}),
        )
      }
      Kind::DrawingTitle => (
        "drawing.title",
        json!({
          "drawing": self.target?, "title": self.name, "author": self.a, "part_number": self.b,
          "material": self.c, "revision": self.d, "company": self.e,
        }),
      ),
      Kind::Project => {
        let mut p = json!({"body": self.bodies.first()?, "associative": self.flag});
        if let Some(f) = self.faces.first().and_then(|f| f.get("face")) {
          p["face"] = f.clone();
        }
        ("sketch.project", p)
      }
      Kind::Extrude => {
        let (distance, flip) = self.extrude_distance();
        (
          "extrude",
          self.with_extrude_rows(self.with_object(if self.extrudes_faces() {
            json!({
              "faces": self.faces, "distance": distance, "distance2": self.distance2, "extent": self.extent,
              "flip": flip, "operation": op,
            })
          } else {
            json!({
              "sketch": self.sketch?.0, "profiles": self.profiles, "distance": distance, "distance2": self.distance2,
              "extent": self.extent, "flip": flip, "operation": op,
            })
          })?)?,
        )
      }
      Kind::Revolve => {
        let (axis, angle) = (self.revolve_axis()?, self.revolve_angle());
        (
          "revolve",
          self.with_revolve_sides(
            json!({"sketch": self.sketch?.0, "profiles": self.profiles, "axis": axis, "angle": angle, "operation": op}),
          ),
        )
      }
      Kind::Fillet => ("fillet", self.fillet_params()),
      Kind::Chamfer => {
        ("chamfer", self.chamfer_params(json!({"edges": self.edges, "tangent_chain": !self.solid.no_chain}), false))
      }
      Kind::Shell => ("shell", self.shell_params()),
      Kind::Box | Kind::Cylinder | Kind::Sphere | Kind::Torus => {
        let mut p = self.primitive_shape();
        p["plane"] = self.plane.clone();
        p["at"] = json!(self.at_pt()?);
        p["operation"] = json!(op);
        let cmd = match self.kind {
          Kind::Box => "box",
          Kind::Cylinder => "cylinder",
          Kind::Sphere => "sphere",
          _ => "torus",
        };
        (cmd, p)
      }
      Kind::OffsetPlane => ("plane.offset", json!({"plane": self.plane, "offset": self.distance})),
      Kind::Mirror if self.choice == "features" => {
        ("mirror", json!({"plane": self.plane, "features": self.mirror_features}))
      }
      Kind::Mirror => ("mirror", json!({"plane": self.plane, "bodies": self.bodies, "join": self.join})),
      Kind::RectPattern => ("pattern.rectangular", self.pattern_params()?),
      Kind::CircPattern => ("pattern.circular", self.circ_params()?),
      Kind::Combine => (
        "combine",
        json!({"target": self.target?, "tools": self.bodies, "operation": op, "keep_tools": self.keep_tools}),
      ),
      Kind::Move => self.move_command()?,
      Kind::RedefinePlane => self.redefine_command()?,
      Kind::ExportDxf | Kind::ExportFlat => return None,
      Kind::InsertMesh => self.insert_mesh_command()?,
      Kind::PlaneCut => self.plane_cut_command()?,
    })
  }

  /// Fields for `feature.edit` when editing an existing feature.
  fn edit_set(&self) -> Option<Value> {
    Some(match self.kind {
      Kind::Extrude => {
        let (distance, flip) = self.extrude_distance();
        let mut set = json!({
          "extent": self.edit_extrude_rows(self.edit_extent(
            json!({"kind": self.extent, "distance": distance, "distance2": self.distance2, "flip": flip}),
          )),
          "operation": self.op,
        });
        // the profiles picked again (none: as stored)
        if self.repicks && !self.profiles.is_empty() {
          set["sketch"] = json!(self.sketch?.0);
          set["profiles"] = json!(self.profiles);
        }
        set
      }
      Kind::Revolve => {
        let two = self.extent == "two_sides";
        json!({"angle": self.revolve_angle(), "symmetric": self.extent == "symmetric",
          "angle2": if two { json!(self.angle2) } else { Value::Null }, "operation": self.op})
      }
      Kind::Fillet => self.fillet_edit_set(),
      Kind::Chamfer => self.chamfer_params(json!({}), true),
      Kind::Shell => self.shell_edit_set(),
      Kind::Box | Kind::Cylinder | Kind::Sphere | Kind::Torus => {
        let mut shape = self.primitive_shape();
        shape["type"] = json!(match self.kind {
          Kind::Box => "box",
          Kind::Cylinder => "cylinder",
          Kind::Sphere => "sphere",
          _ => "torus",
        });
        json!({"shape": shape, "at": self.at_pt()?, "operation": self.op})
      }
      Kind::OffsetPlane => json!({"offset": self.distance}),
      Kind::RectPattern => {
        json!({"count": self.count, "spacing": self.spacing, "count2": self.count2, "spacing2": self.spacing2, "join": self.join,
          "symmetric": self.symmetric[0], "symmetric2": self.symmetric[1], "extent": self.pattern_extent})
      }
      Kind::CircPattern => {
        json!({"count": self.count, "angle": self.angle, "symmetric": self.symmetric[0], "join": self.join})
      }
      Kind::Move => self.move_edit_set()?,
      Kind::Draft => self.draft_edit_set(),
      Kind::Form(f) => solid::form_edit_set(f, &self.form.values),
      Kind::Hole | Kind::Mirror | Kind::Combine | Kind::PressPull => self.more_edit_set()?,
      Kind::Flange | Kind::Hem => return self.flange_edit_set(),
      _ => return None,
    })
  }

  fn at_pt(&self) -> Option<[f64; 2]> {
    let v = |s: &str| s.trim().parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(0.0);
    Some([v(&self.at[0]), v(&self.at[1])])
  }

  /// Puts the dialog on an origin plane ("xy", "xz" or "yz"), as its plane chooser would, and
  /// re-previews (the browser's Offset Plane on an origin plane).
  pub(crate) fn use_origin_plane(&mut self, s: &mut Session, plane: &str) {
    self.plane = json!(plane);
    self.plane_label = format!("{} plane", plane.to_uppercase());
    self.refresh_manip(s);
    self.preview(s);
  }

  /// Puts the dialog on any plane reference (an origin plane's name, `{feature}` for a
  /// construction plane, `{body, face}` for a planar face) and re-previews (a sketch row's
  /// Offset Plane, on the sketch's plane).
  pub(crate) fn use_plane(&mut self, s: &mut Session, plane: Value, label: String) {
    self.plane = plane;
    self.plane_label = label;
    self.refresh_manip(s);
    self.preview(s);
  }

  /// An origin item picked in the canvas or browser: a form with construction picks takes it as
  /// a pick (true), as Fusion's CONSTRUCT dialogs do, rather than it being selected.
  pub(crate) fn take_origin_pick(&mut self, v: Value) -> bool {
    match self.kind {
      Kind::Form(_) if self.takes_construct_picks() => {
        self.form.pick_origin(v);
        true
      }
      _ => false,
    }
  }

  /// Does this dialog take construction picks (planes, axes and points from the browser)?
  pub(crate) fn takes_construct_picks(&self) -> bool {
    matches!(self.kind, Kind::Form(f) if crate::forms::State::takes_picks(f)) && self.editing.is_none()
  }

  /// The plane, axis or point an open CONSTRUCT method dialog is previewing (the last feature
  /// while its command is applied).
  pub(crate) fn previewed_construction(&self, s: &Session) -> Option<FeatureId> {
    let Kind::Form(f) = self.kind else { return None };
    if !self.applied || self.editing.is_some() || !matches!(f.cmd, "plane" | "axis" | "point") {
      return None;
    }
    s.design.features.last().map(|f| f.id)
  }

  /// Is this origin item one of the form's picks (drawn selected)?
  pub(crate) fn origin_picked(&self, v: &Value) -> bool {
    matches!(self.kind, Kind::Form(_)) && self.form.origin.contains(v)
  }

  /// Withdraws the current preview (if any) and applies the dialog's command.
  pub fn preview(&mut self, s: &mut Session) {
    self.dirty = false;
    let selection = s.selection.clone();
    self.withdraw(s);
    if self.kind == Kind::Move {
      self.prepare_move_edit(s);
    }
    if self.kind.reports() {
      self.refresh_report(s);
      return;
    }
    if matches!(self.kind, Kind::Form(f) if crate::forms::close_only(f)) {
      return;
    }
    let Some((cmd, params)) = self.command() else {
      return;
    };
    if !self.ready() {
      return;
    }
    if self.waits_for_value() {
      // at 0.00 (or half typed) an offset sits on its curve and a fillet or shell has no size:
      // nothing to preview, no complaint
      self.error = None;
      return;
    }
    self.cut_ghost(s, cmd, &params);
    self.live_applying(s, cmd, &params);
    let before: Vec<BodyId> = s.model.bodies.iter().map(|b| b.id).collect();
    let result = s.execute(cmd, &params);
    self.note_copies(s, &before, result.is_ok());
    if let Some(t) = self.trace.as_mut() {
      t.result = result.as_ref().ok().cloned();
    }
    match result {
      Ok(v) => {
        self.applied = true;
        self.previewed = self.editing.or_else(|| v.get("feature").and_then(Value::as_u64).map(|f| FeatureId(f as u32)));
        self.note_preview(&v);
        self.error = v.get("error").and_then(Value::as_str).map(str::to_string);
        self.summary = v.get("summary").and_then(Value::as_str).map(str::to_string);
        self.made = v["curves"]
          .as_array()
          .map(|a| a.iter().filter_map(Value::as_u64).map(|c| c as u32).collect())
          .unwrap_or_default();
        self.reselect_picks(s);
      }
      Err(e) => {
        self.error = Some(e.0);
        self.summary = None;
      }
    }
    let switched = self.auto_cut_check(s);
    if !self.picks() {
      s.selection = selection;
    }
    if switched {
      self.preview(s);
    }
    if self.kind == Kind::InsertMesh {
      self.insert_mesh_select(s);
    }
  }

  /// Enough input to try?
  fn ready(&self) -> bool {
    // an edit's stored picks stand; a fillet's or chamfer's picked again need an edge
    if self.editing.is_some() && !(self.repicks && matches!(self.kind, Kind::Fillet | Kind::Chamfer)) {
      return true;
    }
    if self.placing() {
      return false;
    }
    match self.kind {
      Kind::Form(f) => self.form.missing(f).is_none(),
      Kind::Extrude => (self.sketch.is_some() && !self.profiles.is_empty()) || self.extrudes_faces(),
      Kind::Revolve => self.sketch.is_some() && !self.axis.is_empty(),
      Kind::Fillet => self.fillet_ready(),
      Kind::Chamfer => !self.edges.is_empty(),
      Kind::Shell => !self.faces.is_empty() || !self.bodies.is_empty(),
      Kind::Combine => self.target.is_some() && !self.bodies.is_empty(),
      Kind::SplitBody => !self.bodies.is_empty() && (self.target.is_some() || !self.plane.is_null()),
      Kind::Mirror if self.choice == "features" => !self.mirror_features.is_empty() && !self.plane.is_null(),
      Kind::Mirror => !self.bodies.is_empty() && !self.plane.is_null(),
      Kind::CircPattern => !self.bodies.is_empty() && self.circ_axis().is_some(),
      Kind::Move => self.move_ready(),
      Kind::Loft => self.sections.len() >= 2,
      Kind::Sweep => self.sketch.is_some() && self.path.is_some() && !self.profiles.is_empty(),
      Kind::Hole => self.hole_ready(),
      Kind::Fold => !self.faces.is_empty() && self.sketch.is_some() && self.line.is_some(),
      Kind::RectPattern => self.pattern_ready(),
      Kind::OffsetPlane => !self.plane.is_null(),
      Kind::PressPull | Kind::OffsetFace => !self.faces.is_empty(),
      Kind::Draft => !self.plane.is_null() && !self.faces.is_empty(),
      Kind::Scale => !self.bodies.is_empty() && self.point.is_some(),
      Kind::Flange => self.flange_ready(),
      Kind::Hem => !self.edges.is_empty() && sheet::hem_folds(self.choice),
      Kind::Rip => !self.edges.is_empty(),
      Kind::NewComponent => self.choice == "empty" || !self.bodies.is_empty(),
      Kind::ComponentProps => self.target.is_some(),
      Kind::AsBuiltJoint => self.as_built_picks().is_some(),
      Kind::ConvertSheet | Kind::Project => !self.bodies.is_empty(),
      Kind::Joint => {
        let need = self.saved.iter().filter(|n| n.is_empty()).count();
        self.origins.len() >= need && (!self.saved[0].is_empty() || self.comps.first().is_some_and(Option::is_some))
      }
      Kind::SkFillet | Kind::SkChamfer | Kind::SkChamfer2 | Kind::SkChamferAngle => self.curves.len() == 2,
      Kind::SkMirror => !self.curves.is_empty() && self.line.is_some(),
      Kind::SkCircPattern | Kind::SkScale => !self.curves.is_empty() && self.center.is_some(),
      // Fusion's OK waits for a direction line
      Kind::SkRectPattern => !self.curves.is_empty() && !self.dir_lines.is_empty(),
      Kind::SkOffset => !self.curves.is_empty(),
      Kind::TraceImage => self.trace.as_ref().is_some_and(|t| t.path.is_some()),
      Kind::DriveJoint | Kind::CopyComponent => self.target.is_some(),
      Kind::NewDrawing => self.view_on.iter().any(|v| *v),
      Kind::DrawingMoveView | Kind::DrawingDimension => self.target.is_some() && self.view.is_some(),
      Kind::DrawingView => self.target.is_some() && (self.choice != "flat" || !self.bodies.is_empty()),
      Kind::RedefinePlane => self.sketch.is_some() && self.redefine.picked.is_some(),
      Kind::ExportDxf => self.sketch.is_some(),
      Kind::InsertMesh => self.mesh.path.is_some(),
      Kind::Analysis(k) => self.analysis_ready(k),
      Kind::CenterOfMass => !self.bodies.is_empty(),
      Kind::PlaneCut => self.plane_cut_ready(),
      Kind::FlatPattern => !self.faces.is_empty(),
      Kind::Unfold => !self.faces.is_empty() && self.flat.all_bends,
      _ => true,
    }
  }

  fn withdraw(&mut self, s: &mut Session) {
    if self.applied {
      s.undo();
      s.drop_redo();
      self.applied = false;
      if self.picks() && !self.picks_fixed() {
        // the picks are valid again on the un-previewed model
        s.selection = self.picked_selection();
      }
    }
  }

  fn picked_selection(&self) -> Vec<Sel> {
    if let Kind::Form(f) = self.kind {
      return self.form.selection(f);
    }
    let get = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
    let mut out = Vec::new();
    // Draft's pull face stays picked (and blue) with the faces to draft
    if self.kind == Kind::Draft
      && let (Some(b), Some(i)) = (get(&self.plane, "body"), get(&self.plane, "face"))
    {
      out.push(Sel::Face { body: BodyId(b), face: i });
    }
    for e in &self.edges {
      if let (Some(b), Some(i)) = (get(e, "body"), get(e, "edge")) {
        out.push(Sel::Edge { body: BodyId(b), edge: i });
      }
    }
    for f in &self.faces {
      if let (Some(b), Some(i)) = (get(f, "body"), get(f, "face")) {
        out.push(Sel::Face { body: BodyId(b), face: i });
      }
    }
    if matches!(self.kind, Kind::Combine | Kind::SplitBody) {
      return self.routed_selection();
    }
    // a face extrude's start face and object stay picked with its faces
    if self.kind == Kind::Extrude {
      for v in [&self.solid.ext.start_face, &self.solid.to] {
        match (get(v, "body"), get(v, "face")) {
          (Some(b), Some(i)) => out.push(Sel::Face { body: BodyId(b), face: i }),
          (Some(b), None) if v.get("vertex").is_none() => out.push(Sel::Body { body: BodyId(b) }),
          _ => {}
        }
      }
    }
    if matches!(self.kind, Kind::Joint | Kind::AsBuiltJoint) {
      out.clear();
      for (b, o) in &self.origins {
        if let Some(i) = o.get("face").and_then(|f| get(f, "face")) {
          out.push(Sel::Face { body: BodyId(*b), face: i });
        } else if let Some(i) = o.get("edge").and_then(|e| get(e, "edge")) {
          out.push(Sel::Edge { body: BodyId(*b), edge: i });
        } else if o.is_null() {
          out.push(Sel::Body { body: BodyId(*b) });
        }
      }
    }
    out
  }

  /// As-built Joint's picks: (moving component, fixed component, position). The first pick is
  /// on the moving component, the first one on another component (or a root body: the world)
  /// names the fixed one, and the last picked face, edge or point is the joint's position.
  fn as_built_picks(&self) -> Option<(u32, Option<u32>, Value)> {
    let moving = (*self.comps.first()?)?;
    let fixed = *self.comps.iter().find(|c| **c != Some(moving))?;
    let at = self.origins.iter().rev().map(|(_, o)| o).find(|o| !o.is_null())?;
    Some((moving, fixed, at.clone()))
  }

  /// Points the dialog at a component: Properties loads its values, New Component makes an
  /// empty child of it (the browser's component menu).
  pub(crate) fn for_component(&mut self, s: &Session, component: u32) {
    self.target = Some(component);
    if self.kind == Kind::NewComponent {
      self.choice = "empty";
    } else if let Some(c) = s.design.component(fox_engine::model::ComponentId(component)) {
      self.name = c.name.clone();
      self.d = c.part_number.clone();
      self.e = c.description.clone();
      self.position = if c.sheet_metal { "sheet_metal" } else { "standard" };
    }
    self.dirty = true;
  }

  pub fn cancel(&mut self, s: &mut Session) {
    self.withdraw(s);
    self.redefine_restore(s);
    self.end_edit(s, false);
    if self.kind == Kind::InsertMesh {
      // the previewed mesh is gone with its selection
      s.selection.clear();
    }
  }

  /// Is a model-picking dialog's preview showing? (Viewport clicks withdraw it first.)
  pub fn previewing_model(&self) -> bool {
    self.applied && self.picks() && !self.picks_fixed()
  }

  pub fn unpreview(&mut self, s: &mut Session) {
    self.withdraw(s);
  }

  /// The drag arrow this dialog shows in the viewport: where it starts, the direction a positive
  /// value points (unit) and the current value along it (mm). Extrude distance and offset plane
  /// offset, as in Fusion.
  pub fn manipulator(&self, s: &Session) -> Option<Manipulator> {
    let len = |e: &str| fox_engine::expr::eval(e, fox_engine::expr::Kind::Length, &s.model.params).ok();
    match self.kind {
      Kind::Extrude if matches!(self.extent, "one_side" | "symmetric" | "two_sides") && self.extrudes_faces() => {
        // on the first face, along its outward normal (as found before the preview changed it)
        let (origin, dir) = self.face_anchor().or_else(|| {
          let f = self.faces.first()?;
          let get = |k: &str| f.get(k).and_then(Value::as_u64).map(|x| x as u32);
          let info = s.model.body(BodyId(get("body")?))?.faces().iter().find(|x| Some(x.id) == get("face"))?;
          Some((info.center, info.normal))
        })?;
        let (distance, flip) = self.extrude_distance();
        let sign = if flip { -1.0 } else { 1.0 };
        Some(Manipulator { origin, dir, value: sign * len(&distance)? })
      }
      Kind::Extrude if matches!(self.extent, "one_side" | "symmetric" | "two_sides") => {
        let r = s.model.sketches.get(&self.sketch?)?;
        // no arrow until a profile is picked (an edited extrude keeps its stored profiles)
        let at = match self.profiles.first() {
          Some(p) => *p,
          None if self.editing.is_some() => r.profiles.first()?.interior_point(),
          None => return None,
        };
        let (distance, flip) = self.extrude_distance();
        let sign = if flip { -1.0 } else { 1.0 };
        Some(Manipulator { origin: r.plane.to_world(at), dir: r.plane.normal(), value: sign * len(&distance)? })
      }
      Kind::OffsetPlane => {
        let (base, centre) = self.offset_base(s)?;
        Some(Manipulator { origin: centre, dir: self.offset_sign(s) * base.normal(), value: len(&self.distance)? })
      }
      Kind::Section => self.section_manipulator(s),
      _ => self.more_manipulator(s),
    }
  }

  /// Sets the value the drag arrow changed (rounded to `step`) and re-previews.
  pub fn set_manipulator(&mut self, value: f64, step: f64) {
    let v = if step > 0.0 { (value / step).round() * step } else { value };
    match self.kind {
      // dragged back past the profile, the arrow writes a negative distance (no Flip row)
      Kind::Extrude => {
        self.flip = false;
        self.distance = manip::mm(self.unit, v);
      }
      Kind::OffsetPlane => self.distance = manip::mm(self.unit, v),
      Kind::Section => self.set_section_distance(v),
      _ => {
        if !self.set_more_manipulator(v, step) {
          return;
        }
      }
    }
    self.dirty = true;
  }

  /// The point a report dialog marks in the viewport (centre of mass).
  pub fn marker(&self) -> Option<fox_engine::kernel::glam::DVec3> {
    if self.kind != Kind::CenterOfMass {
      return None;
    }
    let c: Vec<f64> = self.report.as_ref()?.get("center")?.as_array()?.iter().filter_map(Value::as_f64).collect();
    match c.as_slice() {
      [x, y, z] => Some(fox_engine::kernel::glam::DVec3::new(*x, *y, *z)),
      _ => None,
    }
  }

  /// Sketch whose profiles should stay visible/pickable while this dialog is open.
  pub fn profile_sketches(&self) -> Vec<FeatureId> {
    match self.kind {
      Kind::Hole if self.solid.hp.single => Vec::new(),
      Kind::Extrude | Kind::Revolve | Kind::Flange | Kind::Hole => self.sketch.into_iter().collect(),
      Kind::Sweep => self.sketch.into_iter().chain(self.path).collect(),
      Kind::Loft => self.sections.iter().map(|(sk, _)| FeatureId(*sk)).collect(),
      _ => Vec::new(),
    }
  }

  /// Picks up selection changes made in the viewport while the dialog is open.
  fn sync_selection(&mut self, s: &Session) {
    if self.picks_fixed() {
      // Edit Move's Selection stays live, as in Fusion
      if self.kind == Kind::Move && self.mv.adds {
        self.sync_move(s);
      }
      return;
    }
    match self.kind {
      Kind::OffsetPlane => self.sync_offset_base(s),
      Kind::Form(f) if !self.applied || !self.picks() => {
        let before = self.form.values.clone();
        self.form.sync(f, s, last_unconsumed_sketch(s));
        self.form.chain_path(f, s);
        self.extend_chain(f, s);
        stitch::find_open_edges(&mut self.form, f, s);
        if self.form.values != before {
          self.dirty = !self.picks();
        }
      }
      Kind::Flange => self.flange_sync(s),
      Kind::Extrude | Kind::Revolve | Kind::Sweep => {
        let mut pts = Vec::new();
        for sel in &s.selection {
          if let Sel::Profile { sketch, index } = sel
            && let Some(p) = s.model.sketches.get(sketch).and_then(|r| r.profiles.get(*index))
          {
            if self.sketch != Some(*sketch) {
              self.sketch = Some(*sketch);
            }
            pts.push(p.interior_point());
          }
        }
        if !pts.is_empty() && pts != self.profiles {
          self.profiles = pts;
          self.dirty = true;
        }
        // (a face extrude's preview changes the picked body: its picks wait for a click, which
        // withdraws the preview first)
        if self.kind == Kind::Extrude && (!self.applied || !self.picks()) {
          self.sync_extrude_faces(s);
          self.sync_object(s);
        }
        if self.kind == Kind::Sweep {
          self.sync_sweep_picks(s);
        }
        if self.kind == Kind::Revolve {
          self.sync_revolve_axis(s);
        }
      }
      Kind::Fillet if !self.applied => {
        // a picked face rounds all its edges
        let (e, f) = (sel_edges(s), sel_faces(s));
        if e != self.edges || f != self.faces {
          self.edges = e;
          self.faces = f;
          self.round_sync();
        }
      }
      Kind::Chamfer if !self.applied => {
        let e = self.chamfer_picks(s);
        if e != self.edges {
          self.edges = e;
        }
      }
      Kind::Loft => {
        let secs = sel_sections(s);
        if !secs.is_empty() && secs != self.sections {
          self.sections = secs;
          self.dirty = true;
        }
        self.sync_loft_guides(s);
      }
      Kind::Hem | Kind::Rip if !self.applied => {
        let e = sel_edges(s);
        if e != self.edges {
          self.edges = e;
        }
      }
      Kind::PressPull | Kind::OffsetFace if !self.applied => {
        let f = sel_faces(s);
        if f != self.faces {
          self.faces = f;
          if self.kind == Kind::PressPull {
            self.sync_press_owner(s);
          }
        }
      }
      Kind::Draft if !self.applied => self.sync_draft(s),
      Kind::Scale => self.sync_scale(s),
      // Fold's Reference: the picked face of the sheet
      Kind::Fold if !self.applied => {
        let f: Vec<Value> = sel_faces(s).into_iter().take(1).collect();
        if f != self.faces {
          self.faces = f;
        }
      }
      Kind::DrawingView => {
        let b = sel_bodies(s);
        if !b.is_empty() && b != self.bodies {
          self.bodies = b;
          self.dirty = self.choice == "flat";
        }
      }
      Kind::FlatPattern | Kind::Unfold if !self.applied => self.flat_sync(s),
      Kind::ConvertSheet | Kind::Project if !self.applied || !self.picks() => {
        let f: Vec<Value> = sel_faces(s).into_iter().take(1).collect();
        let b = sel_bodies(s);
        if (f != self.faces || b != self.bodies) && (!b.is_empty() || self.kind != Kind::Project) {
          self.faces = f;
          self.bodies = b;
          self.dirty = !self.picks();
        }
      }
      Kind::SplitBody => self.sync_split_body(s),
      Kind::NewComponent => {
        let b = sel_bodies(s);
        if b != self.bodies {
          self.comps = b.iter().map(|x| s.design.component_of(BodyId(*x)).map(|c| c.id.0)).collect();
          if self.kind == Kind::NewComponent && !b.is_empty() && self.bodies.is_empty() {
            // picking bodies switches an empty component to From Bodies
            self.choice = "bodies";
          }
          self.bodies = b;
          self.dirty = true;
        }
      }
      Kind::Joint | Kind::AsBuiltJoint if !self.applied => {
        let o = if self.kind == Kind::Joint { sel_origins(s) } else { sel_as_built(s) };
        if o != self.origins {
          self.comps = o.iter().map(|(b, _)| s.design.component_of(BodyId(*b)).map(|c| c.id.0)).collect();
          self.origins = o;
        }
      }
      Kind::SkMirror | Kind::SkCircPattern | Kind::SkRectPattern | Kind::SkScale => self.sync_sketch_picks(s),
      Kind::SkFillet | Kind::SkChamfer | Kind::SkChamfer2 | Kind::SkChamferAngle | Kind::SkOffset => {
        let c = if self.kind == Kind::SkOffset { self.offset_picks(s) } else { sel_curves(s) };
        if !c.is_empty() && c != self.curves {
          self.curves = c;
          self.dirty = true;
        }
      }
      Kind::Shell if !self.applied => {
        // faces to open, or whole bodies to hollow
        let (f, b) = (sel_faces(s), sel_only_bodies(s));
        if f != self.faces || b != self.bodies {
          self.faces = f;
          self.bodies = b;
        }
      }
      Kind::Move => self.sync_move(s),
      Kind::RectPattern => self.pattern_sync(s),
      Kind::Mirror if self.editing.is_none() => self.mirror_sync(s),
      Kind::CircPattern if self.editing.is_none() => self.circ_sync(s),
      Kind::Interference => self.interference_sync(s),
      Kind::Section => self.section_sync(s),
      Kind::Combine if !self.applied => self.sync_combine(s),
      Kind::Mirror | Kind::CircPattern if !self.applied || !self.picks() => {
        let mut b: Vec<u32> = Vec::new();
        for x in &s.selection {
          let id = match x {
            Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } => body.0,
            _ => continue,
          };
          if !b.contains(&id) {
            b.push(id);
          }
        }
        if !b.is_empty() && b != self.bodies {
          self.bodies = b;
          self.dirty = !self.picks();
        }
      }
      _ => {}
    }
  }

  /// Draws the dialog. Returns false when it closed.
  pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> bool {
    self.body_pick_priority(app);
    self.sync_selection(&app.s);
    // an origin plane or axis picked in the browser: Mirror's plane, Circular Pattern's axis
    if let Some(item) = crate::origin::picked(app)
      && self.take_origin(item)
    {
      crate::origin::clear(app);
    }
    self.route_picks(app);
    self.hole_sync(app);
    self.redefine_sync(app);
    self.plane_cut_sync(app);
    if self.kind == Kind::PickPlane
      && let Some(Sel::Face { body, face }) = app.s.selection.iter().find(|x| matches!(x, Sel::Face { .. })).copied()
    {
      if is_planar(&app.s, body, face) {
        app.create_sketch(json!({"body": body.0, "face": face}));
        return false;
      }
      app.status = Some(("Pick a planar face".into(), true));
      app.s.selection.clear();
    }
    if self.kind == Kind::PickPlane {
      // no dialog, as in Fusion: the ribbon button stays pressed, the planes show and the cursor
      // hint asks; Escape or the button cancels
      return !self.closed;
    }
    if self.placing() {
      // nor while a primitive's plane is picked and its base drawn
      self.place_sync(app);
      return !self.closed;
    }
    let mut keep = true;
    // Enter is OK, from one of the dialog's boxes or with nothing focused (as in Fusion); OK can
    // also come from the marking menu
    let dialog_layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("feature-dialog"));
    let ok_enabled = self.ok_enabled();
    let mut ok = std::mem::take(&mut self.ok_requested)
      || ok_enabled
        && ctx.input(|i| i.key_pressed(egui::Key::Enter))
        && !self.kind.reports()
        && match ctx.memory(|m| m.focused()) {
          None => true,
          Some(id) => ctx
            .read_response(id)
            .is_some_and(|r| r.layer_id == dialog_layer || crate::value_box::is_box_layer(r.layer_id)),
        };
    let mut cancel = false;
    let title = self.heading();
    // FILLET's selection table needs the extra width, as in Fusion
    let width = if matches!(self.kind, Kind::Fillet | Kind::Loft) { 360.0 } else { 300.0 };
    // docked at the canvas's top-right corner when it opens and again after the window is
    // resized; in between it stays where it was dragged
    let dock = panels::dialog_dock(app.canvas, app.s.edit_sketch.is_some());
    let area = egui::Area::new(egui::Id::new("feature-dialog"))
      .pivot(egui::Align2::RIGHT_TOP)
      .constrain(true)
      .order(egui::Order::Foreground);
    let area = if self.docked == Some(app.canvas) {
      area.default_pos(dock)
    } else {
      self.docked = Some(app.canvas);
      area.current_pos(dock)
    };
    area.show(ctx, |ui| {
      panels::dialog_frame().show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        panels::dialog_header(ui, self.kind.icon(), &title, width);
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
          if self.kind.palette() {
            egui::ScrollArea::vertical().max_height(560.0).auto_shrink([false, true]).show(ui, |ui| {
              if self.kind == Kind::Material {
                crate::material_ui::material_palette(app, ui, &mut self.palette);
              } else {
                crate::material_ui::appearance_palette(app, ui, &mut self.palette);
              }
            });
          } else {
            self.tab_strip(ui);
            egui::Grid::new("dlg").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
              self.fields(app, ui);
            });
          }
          // Stitch's Stitch Result and Unstitched Edges groups
          if let Kind::Form(f) = self.kind
            && f.cmd == "surface.stitch"
          {
            self.stitch_groups(app, ui);
          }
          // a failed preview's message is in the error toast instead, as Fusion shows it
          if let Some(e) = self.error.as_ref().filter(|_| !self.failed()) {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("⚠ {e}")).color(theme::ERROR));
          } else if let Some(n) = self.summary.as_ref().filter(|_| self.applied) {
            ui.add_space(6.0);
            ui.label(RichText::new(n).color(theme::MUTED));
          }
        });
        let sep = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover()).0;
        ui.painter().rect_filled(sep, 0.0, theme::SEPARATOR);
        egui::Frame::new().fill(theme::DIALOG_BODY).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
          ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
            let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 24.0), egui::Sense::hover());
            ui.painter().circle_stroke(r.center(), 7.0, egui::Stroke::new(1.2, theme::GROUP_LABEL));
            ui.painter().text(
              r.center(),
              egui::Align2::CENTER_CENTER,
              "i",
              egui::FontId::proportional(11.0),
              theme::GROUP_LABEL,
            );
            let close_only = matches!(self.kind, Kind::Form(f) if crate::forms::close_only(f));
            if !self.kind.reports() && !close_only {
              if self.picks() && !self.picks_fixed() {
                self.live_checkbox(ui, &mut app.s);
              }
              if self.kind == Kind::InsertMesh
                && crate::icons::icon_button(ui, Icon::Undo, 22.0, "Reset", false, None).clicked()
              {
                self.insert_mesh_reset();
              }
              ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if panels::secondary_button(ui, "Cancel").clicked() {
                  cancel = true;
                }
                if ok_button(ui, self.accent(), ok_enabled).clicked() {
                  ok = true;
                }
              });
            } else {
              ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if panels::secondary_button(ui, "Close").clicked() {
                  cancel = true;
                }
              });
            }
          });
        });
      });
    });
    // typing no value box took (a dialog without one) is dropped
    ctx.data_mut(|d| d.remove::<String>(egui::Id::new(TYPING)));
    self.live_check();
    if let Some(k) = self.switch.take() {
      self.switch_pattern(app, k);
      return true;
    }
    if std::mem::take(&mut self.live.withdraw) {
      self.withdraw(&mut app.s);
    }
    if self.dirty {
      self.preview(&mut app.s);
    }
    // Export DXF's OK asks where to save; Redefine Sketch Plane's puts the timeline back first
    if ok && self.kind == Kind::ExportDxf {
      ok = false;
      keep = false;
      self.dxf_ok(app);
    }
    if ok && self.kind == Kind::ExportFlat {
      ok = false;
      keep = !self.export_flat_ok(app);
    }
    if ok && self.kind == Kind::RedefinePlane {
      ok = false;
      keep = !self.redefine_ok(app);
    }
    if ok && matches!(self.kind, Kind::Interference | Kind::Section) {
      // nothing to apply: OK closes, Section Analysis keeping its cut in the browser
      if ok_enabled {
        if self.kind == Kind::Section {
          self.keep_section(app);
        }
        keep = false;
      }
      ok = false;
    }
    // an analysis is kept in the browser's Analysis folder
    if ok && let Some((kind, params)) = self.analysis_ok() {
      crate::analyses::keep(app, kind, params);
      app.s.selection.clear();
      keep = false;
      ok = false;
    }
    if ok {
      if !self.applied {
        self.preview(&mut app.s);
      }
      if self.applied && self.error.is_none() {
        keep = false;
        self.end_edit(&mut app.s, true);
        app.s.selection.clear();
        // select the traced shapes (not their holes), ready to extrude
        if let (Some(r), Some(sketch)) = (self.trace.as_ref().and_then(|t| t.result.as_ref()), app.s.edit_sketch) {
          let ink = r["ink_profiles"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect::<Vec<_>>());
          if let Some(ink) = ink.filter(|i| !i.is_empty()) {
            app.s.selection = ink.iter().map(|i| Sel::Profile { sketch, index: *i as usize }).collect();
            app.status = Some((format!("Traced {} shapes, selected: Extrude (E) makes them solid", ink.len()), false));
          }
        }
        if self.kind == Kind::Move {
          self.finish_paste(app);
        }
        if self.kind == Kind::SkOffset {
          offset_dimension(app, &self.curves, &self.made, &self.distance);
        }
        if self.kind == Kind::NewDrawing {
          app.drawing = app.s.design.drawings.last().map(|d| d.id);
        }
        // OK on CREATE FLAT PATTERN enters the flat pattern environment
        if self.kind == Kind::FlatPattern {
          crate::flat_ui::enter(app);
        }
        // the new component is the newest (ids only grow)
        if self.kind == Kind::NewComponent
          && self.flag2
          && let Some(c) = app.s.design.components.iter().map(|c| c.id.0).max()
        {
          app.run("component.activate", json!({"component": c}));
        }
      } else if !self.applied && self.error.is_none() {
        self.error = Some(self.missing_input().into());
      }
      // a failing preview (OK greyed, from the marking menu or with Preview off) keeps the
      // dialog open, as Fusion's greyed OK does
    }
    if cancel {
      self.cancel(&mut app.s);
      keep = false;
      // Cancel closes with nothing selected, as OK, Escape and the marking menu's Cancel do
      if !self.kind.reports() {
        app.s.selection.clear();
      }
    }
    keep && !self.closed
  }

  /// The dialog's title: the command's, or EDIT FEATURE for every feature edited from the
  /// timeline, as in Fusion.
  pub(crate) fn heading(&self) -> String {
    if self.editing.is_some() {
      // sheet metal features name their command, as Fusion's EDIT EDGE FLANGE does
      return self.flange_title().map_or("EDIT FEATURE".into(), |t| format!("EDIT {t}"));
    }
    if let Some(t) = self.flange_title() {
      return t.into();
    }
    match self.kind {
      Kind::Move => self.move_title().unwrap_or(self.kind.title()),
      // Press Pull on a face is Fusion's Offset Face
      Kind::PressPull if !self.faces.is_empty() => "OFFSET FACE",
      _ => self.kind.title(),
    }
    .to_string()
  }

  /// The command family's accent colour, used by this dialog's chips and OK button.
  fn accent(&self) -> egui::Color32 {
    crate::icons::accent_of(self.kind.icon())
  }

  /// Fusion's cursor prompt for the dialog's current step: what to select while input is
  /// missing, then the values to specify. `hovering`: the pointer is over something selectable,
  /// when Fusion says how to change the selection instead.
  pub(crate) fn prompt(&self, hovering: bool) -> Option<String> {
    if self.kind == Kind::PickPlane {
      return Some(PICK_PLANE_HINT.into());
    }
    if let Some(p) = self.place_prompt() {
      return Some(p.into());
    }
    if self.kind == Kind::Inspect {
      return Some(MEASURE_PROMPT.into());
    }
    if self.wants_cut_plane() {
      return Some(section::CUT_PLANE_HINT.into());
    }
    if !self.ready() {
      // a form's own wording for the row waiting for a pick
      if let Kind::Form(f) = self.kind
        && f.cmd != "sketch.text.form"
        && let Some(p) = self.form.first_missing(f).and_then(|(key, _)| crate::forms::prompt(f, key))
      {
        return Some(p.into());
      }
      return Some(match self.kind {
        Kind::Extrude => "Select sketch profiles or planar faces to extrude".into(),
        Kind::Revolve if self.sketch.is_some() => "Select the axis to revolve the profile around".into(),
        Kind::Revolve => "Select sketch profiles or planar faces to revolve".into(),
        Kind::Fillet => "Select edges, faces, or features to fillet".into(),
        Kind::Chamfer => "Select edges or faces to chamfer".into(),
        Kind::Shell => "Select faces to remove or bodies to shell".into(),
        Kind::Loft => "Select profiles or planar faces".into(),
        Kind::Sweep if self.profiles.is_empty() => "Specify a sweep type or select an object to sweep".into(),
        Kind::Sweep => "Select path".into(),
        Kind::Mirror if self.choice == "features" && self.mirror_features.is_empty() => {
          "Select features to mirror".into()
        }
        Kind::Mirror if self.choice != "features" && self.bodies.is_empty() => "Select bodies to mirror".into(),
        Kind::Mirror => "Select mirror plane".into(),
        Kind::CircPattern if self.bodies.is_empty() => {
          "Select solid or surface bodies in the canvas or browser to pattern".into()
        }
        Kind::CircPattern => "Select axis".into(),
        Kind::Combine if self.pick_row == 0 => "Select target body".into(),
        Kind::Combine => "Select bodies to combine with target body".into(),
        Kind::SplitBody if self.pick_row == 0 => "Select Body to split".into(),
        Kind::SplitBody => "Select splitting tool(s)".into(),
        // a routed form asks for its active row's picks
        Kind::Form(f) if crate::forms::routed(f) => {
          sentence(self.form.row_prompt(f).unwrap_or_else(|| self.missing_input()))
        }
        Kind::PressPull => "Select faces, edges, or sketch profiles".into(),
        Kind::Draft if self.pick_row == 0 || self.plane.is_null() => "Select pull direction".into(),
        Kind::Draft => "Select faces to draft".into(),
        Kind::Fold if self.faces.is_empty() => "Click on the stationary side of a face on a sheet metal body.".into(),
        Kind::RedefinePlane => redefine::HINT.into(),
        Kind::OffsetFace => "Select face(s) to offset".into(),
        Kind::Scale if self.bodies.is_empty() => "Select bodies or sketches to scale".into(),
        Kind::Scale => "Select point".into(),
        Kind::Flange => flange::PICK_PROMPT.into(),
        Kind::PlaneCut => plane_cut::HINT.into(),
        Kind::FlatPattern => sheet_flat::FLAT_HINT.into(),
        Kind::Unfold if self.faces.is_empty() => sheet_flat::UNFOLD_HINT.into(),
        Kind::Unfold => sheet_flat::UNFOLD_BENDS_HINT.into(),
        // one prompt for the whole pick, as Fusion words it
        Kind::SkScale => {
          "Select the sketch geometry to scale and the point from which to scale, then specify the scale factor".into()
        }
        Kind::Form(f) => self
          .form
          .chip_prompt(f)
          .or_else(|| crate::forms::pick_prompt(f))
          .map(String::from)
          .unwrap_or_else(|| sentence(self.missing_input())),
        _ => sentence(self.missing_input()),
      });
    }
    // Hem's prompt once its edges are picked, whatever the pointer is over, as Fusion words it
    if self.kind == Kind::Hem {
      return Some("Adjust settings or hold Ctrl to modify selection".into());
    }
    // Offset's prompt names its values whatever the pointer is over, as Fusion's does
    if self.kind == Kind::SkOffset {
      return Some("Modify offset distance or side settings".into());
    }
    // the flange's, whatever the pointer is over, as Fusion words it
    if let Some(p) = self.flange_prompt() {
      return Some(p.into());
    }
    // Mirror's prompt once both picks are in, whatever the pointer is over, as Fusion words it
    if self.kind == Kind::Mirror && self.editing.is_none() {
      return Some("Hold Ctrl to modify object selection or mirror plane".into());
    }
    // so does Draft's, once its picks are made
    if self.kind == Kind::Draft {
      return Some("Specify draft angle".into());
    }
    // a mesh dialog keeps naming its settings, as Fusion's do
    if let Kind::Form(f) = self.kind
      && crate::forms::mesh_form(f)
    {
      return crate::forms::specify_prompt(f).map(String::from);
    }
    if hovering && self.selects() {
      return Some("Hold Ctrl to modify selection".into());
    }
    let specify = match self.kind {
      Kind::Extrude => "Specify distance and taper angle",
      Kind::Revolve => "Specify angle",
      Kind::Fillet => "Specify fillet radius",
      Kind::Chamfer => "Specify chamfer distance",
      Kind::Shell => "Specify thickness",
      Kind::OffsetPlane => "Specify offset distance",
      Kind::Hole => "Specify hole diameter and depth",
      Kind::Draft => "Specify draft angle",
      Kind::PressPull | Kind::OffsetFace => "Specify distance",
      Kind::Scale => "Specify scale factor",
      Kind::Loft | Kind::Sweep | Kind::Combine => "Specify operation",
      Kind::RectPattern | Kind::CircPattern => "Specify quantity and distance",
      Kind::Move => "Specify distance or angle",
      Kind::SkScale => "Specify scale factor",
      // Text with its frame placed: the rows are live and the frame's handle turns it
      Kind::Form(f) if std::ptr::eq(f, &crate::forms::SK_TEXT) && self.form.text_framed(f) => {
        "Adjust text formatting or rotate text frame"
      }
      Kind::Form(f) => return crate::forms::specify_prompt(f).map(String::from),
      _ => return None,
    };
    Some(specify.into())
  }

  /// Does this dialog take its input from a selection in the canvas (so a later pick changes it)?
  fn selects(&self) -> bool {
    self.picks()
      || matches!(
        self.kind,
        Kind::Extrude
          | Kind::Revolve
          | Kind::Loft
          | Kind::Sweep
          | Kind::Hole
          | Kind::Fold
          | Kind::Flange
          | Kind::NewComponent
          | Kind::Project
          | Kind::Mirror
          | Kind::RectPattern
          | Kind::CircPattern
          | Kind::Move
          | Kind::SkFillet
          | Kind::SkChamfer
          | Kind::SkChamfer2
          | Kind::SkChamferAngle
          | Kind::SkMirror
          | Kind::SkOffset
          | Kind::SkCircPattern
          | Kind::SkRectPattern
          | Kind::SkScale
          | Kind::Scale
          | Kind::Analysis(_)
          | Kind::CenterOfMass
      )
  }

  /// Is OK live? Greyed while the dialog lacks its input, as in Fusion: Extrude until a profile
  /// is picked, the others until what they need (`ready`) is there; and while the live preview
  /// fails (a fillet too big for its edges), until a value that works is typed.
  pub(crate) fn ok_enabled(&self) -> bool {
    let input = match self.kind {
      Kind::Extrude if !self.picks_fixed() => !self.profiles.is_empty() || !self.faces.is_empty(),
      // OK waits for Compute, and Section Analysis for its cut plane
      Kind::Interference => self.report.is_some(),
      Kind::Section => !self.plane.is_null(),
      // Remove Features applies from its own Remove button
      Kind::Form(f) if crate::forms::close_only(f) => false,
      // Trim previews its tool's cut, but OK waits for a piece to remove
      Kind::Form(f) if f.cmd == crate::forms::TRIM_CMD => {
        self.ready() && self.form_value("remove").and_then(Value::as_array).is_some_and(|a| !a.is_empty())
      }
      // an offset of 0.00 sits on its curve, a fillet or shell of 0.00 has no size: OK waits
      // for a value
      _ => self.ready() && !self.waits_for_value(),
    };
    input && !self.failed()
  }

  /// Is the dialog's value still at 0.00 (or half typed), where Fusion previews nothing and OK
  /// waits: Sketch Offset's and Offset Face's distance, Draft's angle, a new fillet's radius or
  /// shell's thickness.
  fn waits_for_value(&self) -> bool {
    match self.kind {
      Kind::SkOffset | Kind::OffsetFace => !self.offset_set(),
      Kind::Draft => !self.draft_angle_set(),
      Kind::Fillet if self.editing.is_none() && self.solid.fillet_type != "full_round" => !length_set(&self.radius),
      Kind::Shell if self.editing.is_none() => !length_set(&self.radius),
      Kind::Flange => self.flange_waits(),
      _ => false,
    }
  }

  /// Sketch Offset, Offset Face: has the distance left 0.00 (a value the arrow, the box or the
  /// row set)? A distance naming a user parameter counts as set; a half-typed one (".", "-")
  /// doesn't.
  fn offset_set(&self) -> bool {
    length_set(&self.distance)
  }

  fn missing_input(&self) -> &'static str {
    match self.kind {
      Kind::Form(f) => self.form.missing(f).unwrap_or("missing input"),
      Kind::Revolve if self.sketch.is_some() => "select the axis to revolve the profile around",
      Kind::Extrude | Kind::Revolve => "select a profile",
      Kind::Fillet => "select edges or faces",
      Kind::Chamfer => "select edges",
      Kind::Shell => "select faces to remove, or a body",
      Kind::Combine => "select a target body and tool bodies",
      Kind::Move => self.move_missing().unwrap_or("missing input"),
      Kind::Loft => "select profiles on two or more sketches",
      Kind::Sweep => "select a profile and a path sketch",
      Kind::Hole if self.solid.hp.single => "click a planar face where the hole goes",
      Kind::Hole => "sketch points to drill (Point tool) are needed",
      Kind::RectPattern => self.pattern_missing(),
      Kind::Fold if self.faces.is_empty() => "click the stationary side of a face on the sheet",
      Kind::Fold => "pick a sketch with a bend line",
      Kind::OffsetPlane => "select a plane",
      Kind::PressPull | Kind::OffsetFace => "select faces",
      Kind::Draft if self.plane.is_null() => "select the pull direction",
      Kind::Draft => "select faces to draft",
      Kind::Scale if self.bodies.is_empty() => "select bodies to scale",
      Kind::Scale => "select the point to scale about",
      Kind::Flange if self.choice == "edge" => "select sheet edges",
      Kind::Flange => "select a profile",
      Kind::Hem if self.edges.is_empty() => "select an edge to hem",
      Kind::Hem => "FissionOxide folds flat and open hems only so far",
      Kind::Rip => "select edges",
      Kind::Project if self.choice != "bodies" => "select geometry to project",
      Kind::ConvertSheet | Kind::NewComponent | Kind::Project => "select a body",
      Kind::Joint => "select a face or edge on the moving component, then one on the fixed one",
      Kind::AsBuiltJoint => {
        "select the moving component, then the one it's jointed to, then a face, edge or point for the joint"
      }
      Kind::ComponentProps => "pick a component in the browser",
      Kind::SkFillet => "select two lines or arcs that meet",
      Kind::SkChamfer | Kind::SkChamfer2 | Kind::SkChamferAngle => "select two lines",
      Kind::SkMirror if self.curves.is_empty() => "select sketch objects to mirror",
      Kind::SkMirror => "select the mirror line",
      Kind::SkCircPattern if self.curves.is_empty() => "select sketch objects to pattern",
      Kind::SkCircPattern => "select the center point",
      Kind::SkRectPattern if self.curves.is_empty() => "select sketch objects to pattern",
      Kind::SkRectPattern => "select a line for the direction",
      Kind::SkScale if self.curves.is_empty() => "select the sketch geometry to scale",
      Kind::SkScale => "select the point to scale from",
      Kind::SkOffset => "select sketch curves",
      Kind::TraceImage => "pick an image to trace",
      Kind::DriveJoint => "the design has no joints",
      Kind::CopyComponent => "the design has no components",
      Kind::NewDrawing => "pick at least one view",
      Kind::DrawingMoveView | Kind::DrawingDimension => "pick a view",
      Kind::DrawingView if self.choice == "flat" => "select a sheet metal body",
      Kind::RedefinePlane => "select a planar face or a work plane",
      Kind::Analysis(k) => self.analysis_missing(k),
      Kind::CenterOfMass => "select bodies and/or components",
      Kind::PlaneCut => "select a mesh body and a cut plane",
      Kind::FlatPattern => "select a stationary face",
      Kind::Unfold if self.faces.is_empty() => "select a stationary face",
      Kind::Unfold => "tick Unfold all bends",
      _ => "missing input",
    }
  }

  fn fields(&mut self, app: &mut App, ui: &mut egui::Ui) {
    let creating = !self.picks_fixed();
    self.construct_rows(ui);
    match self.kind {
      // Create Sketch has no dialog (`show` returns before drawing one): the planes to pick are
      // drawn in the viewport (`viewport::plane_picker`) with the cursor hint
      Kind::PickPlane => {}
      Kind::Extrude => self.extrude_rows(ui),
      Kind::Revolve => self.revolve_fields(ui),
      Kind::Fillet => {
        self.fillet_rows(app, ui, creating);
        self.setback_row(ui, creating);
      }
      Kind::Chamfer => self.chamfer_rows(app, ui, creating),
      Kind::Shell => self.shell_rows(ui, creating),
      Kind::Box | Kind::Cylinder | Kind::Sphere | Kind::Torus => self.primitive_rows(ui, creating),
      Kind::Mirror if creating => self.mirror_rows(app, ui),
      Kind::CircPattern if creating => self.circ_rows(ui),
      Kind::OffsetPlane | Kind::Mirror => {
        ui.label("Plane");
        if self.kind == Kind::OffsetPlane && creating {
          self.offset_plane_chip(ui, &mut app.s);
        } else if creating {
          self.plane_choice(ui);
        } else if self.kind == Kind::Mirror {
          sel_chip(ui, self.accent(), "1 selected", true);
        } else {
          ui.label("(unchanged)");
        }
        ui.end_row();
        if self.kind == Kind::OffsetPlane {
          self.offset_plane_rows(ui);
        } else {
          ui.label("Type");
          let types = [("bodies", "Bodies"), ("features", "Features")];
          let label = types.iter().find(|(k, _)| *k == self.choice).map_or("Bodies", |(_, l)| *l);
          egui::ComboBox::from_id_salt("mirror_type").selected_text(label).width(140.0).show_ui(ui, |ui| {
            for (k, l) in types {
              if ui.selectable_label(self.choice == k, l).clicked() {
                self.choice = k;
                self.dirty = true;
              }
            }
          });
          ui.end_row();
          if self.choice == "features" {
            // features that add or remove a solid: their tool is what gets mirrored
            let tools: Vec<(u32, String)> = app
              .s
              .design
              .features
              .iter()
              .filter(|f| {
                matches!(
                  f.kind,
                  FeatureKind::Extrude { .. }
                    | FeatureKind::Revolve { .. }
                    | FeatureKind::Primitive { .. }
                    | FeatureKind::Loft { .. }
                    | FeatureKind::Sweep { .. }
                    | FeatureKind::Hole { .. }
                )
              })
              .map(|f| (f.id.0, f.name.clone()))
              .collect();
            ui.label("Features");
            ui.vertical(|ui| {
              for (id, name) in tools {
                let mut on = self.mirror_features.contains(&id);
                if ui.checkbox(&mut on, name).changed() {
                  self.mirror_features.retain(|f| *f != id);
                  if on {
                    self.mirror_features.push(id);
                  }
                  self.dirty = true;
                }
              }
            });
            ui.end_row();
          } else {
            ui.label("Bodies");
            sel_chip(ui, self.accent(), &self.bodies_label(), !self.bodies.is_empty());
            ui.end_row();
            ui.label("");
            if ui.checkbox(&mut self.join, "Join to original").changed() {
              self.dirty = true;
            }
            ui.end_row();
          }
        }
      }
      Kind::RectPattern => {
        if creating {
          self.pattern_type_row(ui);
        }
        self.pattern_fields(app, ui)
      }
      Kind::CircPattern => {
        ui.label("Bodies");
        sel_chip(ui, self.accent(), &self.bodies_label(), !self.bodies.is_empty());
        ui.end_row();
        ui.label("Axis");
        self.axis_combo(ui, "axis", false, &app.s.model.axes.keys().map(|f| f.0).collect::<Vec<_>>());
        ui.end_row();
        ui.label("Count");
        if value_edit(ui, &mut self.count).changed() {
          self.dirty = true;
        }
        ui.end_row();
        ui.label("Total angle");
        if value_edit(ui, &mut self.angle).changed() {
          self.dirty = true;
        }
        ui.end_row();
        ui.label("");
        if ui.checkbox(&mut self.symmetric[0], "Symmetric").changed() {
          self.dirty = true;
        }
        ui.end_row();
        ui.label("");
        if ui.checkbox(&mut self.join, "Join instances").changed() {
          self.dirty = true;
        }
        ui.end_row();
      }
      Kind::Combine => self.combine_fields(ui),
      Kind::Move => self.move_fields(app, ui),
      Kind::Inspect => self.inspect(app, ui),
      // the palettes draw outside the grid (see `show`)
      Kind::Appearance | Kind::Material => {}
      Kind::RedefinePlane => self.redefine_rows(app, ui),
      Kind::ExportDxf => self.dxf_rows(app, ui),
      Kind::InsertMesh => self.insert_mesh_rows(app, ui),
      Kind::ExportFlat => self.export_flat_rows(app, ui),
      Kind::Section => self.section_fields(app, ui),
      Kind::PlaneCut => self.plane_cut_rows(app, ui),
      Kind::Analysis(kind) => self.inspect_analysis_rows(app, ui, kind),
      Kind::Form(f) => {
        let mut dirty = false;
        let op = self.operation().to_string();
        self.form_fields(f, app, ui, &mut dirty);
        self.path_type_switch(f);
        self.form_cut_rows(ui, &op);
        self.flange_type_switch(f);
        if dirty && (!self.picks() || self.applied) {
          self.dirty = true;
        }
      }
      Kind::TraceImage => {
        let mut dirty = false;
        if let Some(t) = self.trace.as_mut() {
          crate::trace::fields(t, ui, &mut dirty);
        }
        self.dirty |= dirty;
      }
      _ => {
        let mut dirty = false;
        self.more_fields(app, ui, &mut dirty);
        if dirty && (!self.picks() || self.applied) {
          self.dirty = true;
        }
      }
    }
  }

  /// A form dialog's inputs: selection chips, value fields, check boxes and drop-downs.
  fn form_fields(&mut self, f: &'static crate::forms::Form, app: &mut App, ui: &mut egui::Ui, dirty: &mut bool) {
    use crate::forms::Input;
    let accent = self.accent();
    let s = &app.s;
    let faces_mode = self.form.filter_faces(f);
    let mut filter: Option<&'static str> = None;
    let faces_picked = f
      .inputs
      .iter()
      .position(|x| x.key() == "faces")
      .and_then(|j| self.form.values[j].as_array().map(Vec::len))
      .unwrap_or(0);
    if std::ptr::eq(f, &crate::forms::REMOVE_FEATURES) {
      self.remove_features_fields(app, ui);
      return;
    }
    // Extend's Analysis tab
    if crate::forms::has_analysis_tab(f) && self.solid.rsl.analysis_tab {
      self.analysis_rows(ui);
      return;
    }
    let text_form = f.cmd == "sketch.text.form";
    let params = if text_form { crate::value_box::params_of(&app.s) } else { Vec::new() };
    for (i, input) in f.inputs.iter().enumerate() {
      if !self.form.shown(f, i) || crate::forms::State::hidden(f, i) {
        continue;
      }
      if text_form && self.text_form_field(f, i, &params, ui, dirty) {
        continue;
      }
      if f.cmd == "thread" && self.thread_form_field(f, i, s, ui, dirty) {
        continue;
      }
      if self.mesh_form_field(f, i, s, ui, dirty) {
        continue;
      }
      let v = self.form.values[i].clone();
      let greyed = crate::forms::greyed(f, input.key());
      // Stitch's Tolerance has a slider under it
      if f.cmd == "surface.stitch" && input.key() == "tolerance" {
        self.stitch_tolerance(i, input, ui, dirty);
        continue;
      }
      // Coil's Profile: the circle drawn before the dialog opened
      if f.cmd == "coil.form" && input.key() == "plane" {
        ui.label("Profile");
        sel_chip(ui, accent, "1 selected", true);
        ui.end_row();
        continue;
      }
      // Coil's and Pipe's Operation shows each value's icon, as Fusion's
      if let Input::Choice { key: "operation", label, options } = input
        && matches!(f.cmd, "coil.form" | "pipe.form")
      {
        ui.label(*label);
        let cur = v.as_str().unwrap_or("");
        let mut sel: &'static str = options.iter().find(|(k, _)| *k == cur).map_or("new_body", |(k, _)| *k);
        if choice::combo(ui, "form-op", options, &mut sel, choice::extrude_icon) {
          *dirty = true;
        }
        self.form.values[i] = json!(sel);
        ui.end_row();
        continue;
      }
      match input {
        Input::Faces { label, .. }
        | Input::FacesOpt { label, .. }
        | Input::Edges { label, .. }
        | Input::Curves { label, .. } => {
          ui.label(*label);
          let n = v.as_array().map(Vec::len).unwrap_or(0);
          let text = if n == 0 { "Select".to_string() } else { format!("{n} selected") };
          // Corner Closure's one row is lit from the start, as Fusion's active selection
          let lit = n > 0 || f.cmd == "sheet.corner_closure";
          sel_chip_lit(ui, accent, &text, lit, n > 0);
          ui.end_row();
        }
        Input::Bodies { label, .. } => {
          ui.label(*label);
          let n = v.as_array().map(Vec::len).unwrap_or(0);
          // Boundary Fill's tools: bodies and construction planes alike
          let planes = f
            .inputs
            .iter()
            .position(|x| matches!(x, Input::Planes { .. }))
            .and_then(|j| self.form.values[j].as_array().map(Vec::len))
            .unwrap_or(0);
          let text = match n {
            // with the Selection Filter on faces, the picks are the faces
            _ if faces_mode => chip_text(faces_picked, if faces_picked == 1 { "face" } else { "faces" }),
            _ if planes > 0 => format!("{} selected", n + planes),
            0 => "Select".to_string(),
            // a mesh dialog's chip counts, as Fusion's "1 selected"
            1 if crate::forms::mesh_form(f) => "1 selected".to_string(),
            1 => v[0].as_u64().map(|b| s.body_name(BodyId(b as u32))).unwrap_or_default(),
            n => format!("{n} bodies"),
          };
          sel_chip(ui, accent, &text, if faces_mode { faces_picked > 0 } else { n + planes > 0 });
          ui.end_row();
        }
        Input::Face { label, .. } | Input::Edge { label, .. } | Input::Construct { label, .. } => {
          ui.label(*label);
          sel_chip(ui, accent, if v.is_null() { "Select" } else { "1 selected" }, !v.is_null());
          ui.end_row();
        }
        Input::Sketch { label, .. } => {
          ui.label(*label);
          let list = Self::sketches(app, None);
          let cur = v.as_u64().and_then(|id| list.iter().find(|(f, _)| f.0 as u64 == id)).map(|(_, n)| n.clone());
          egui::ComboBox::from_id_salt(("form-sketch", i))
            .selected_text(cur.unwrap_or("Pick a sketch".into()))
            .width(140.0)
            .show_ui(ui, |ui| {
              for (id, name) in &list {
                if ui.selectable_label(v.as_u64() == Some(id.0 as u64), name).clicked() {
                  self.form.values[i] = json!(id.0);
                  self.form.chosen[i] = Some(json!(id.0));
                  *dirty = true;
                }
              }
            });
          ui.end_row();
        }
        Input::Plane { label, .. } | Input::Axis { label, .. } => {
          ui.label(*label);
          let plane = matches!(input, Input::Plane { .. });
          let options: Vec<(Value, String)> = if plane {
            let mut o: Vec<(Value, String)> = [("xy", "XY plane"), ("xz", "XZ plane"), ("yz", "YZ plane")]
              .iter()
              .map(|(k, l)| (json!(k), l.to_string()))
              .collect();
            for id in s.model.planes.keys() {
              let name = s.design.feature(*id).map(|f| f.name.clone()).unwrap_or_default();
              o.push((json!({"feature": id.0}), name));
            }
            o
          } else {
            let mut o: Vec<(Value, String)> =
              ["x", "y", "z"].iter().map(|k| (json!(k), format!("{} axis", k.to_uppercase()))).collect();
            for id in s.model.axes.keys() {
              let name = s.design.feature(*id).map(|f| f.name.clone()).unwrap_or_default();
              o.push((json!({"axis": id.0}), name));
            }
            o
          };
          let shown = options
            .iter()
            .find(|(k, _)| *k == v)
            .map(|(_, l)| l.clone())
            .unwrap_or_else(|| if plane { "Picked face".into() } else { "Picked edge".into() });
          egui::ComboBox::from_id_salt(("form-ref", i)).selected_text(shown).width(140.0).show_ui(ui, |ui| {
            for (k, l) in &options {
              if ui.selectable_label(*k == v, l).clicked() {
                self.form.values[i] = k.clone();
                self.form.chosen[i] = Some(k.clone());
                *dirty = true;
              }
            }
          });
          ui.end_row();
        }
        Input::Expr { label, .. } | Input::Text { label, .. } => {
          let mut t = v.as_str().unwrap_or("").to_string();
          if let Some(why) = greyed {
            ui.label(*label);
            ui.add_enabled_ui(false, |ui| value_edit(ui, &mut t)).inner.on_disabled_hover_text(why);
            ui.end_row();
          } else {
            Self::field(ui, label, &mut t, dirty);
          }
          self.form.values[i] = json!(t);
        }
        Input::Check { label, .. } | Input::CheckOn { label, .. } => {
          let mut b = v.as_bool().unwrap_or(false);
          if let Some(why) = greyed {
            ui.label("");
            ui.add_enabled_ui(false, |ui| ui.checkbox(&mut b, *label)).inner.on_disabled_hover_text(why);
            ui.end_row();
          } else {
            Self::check(ui, label, &mut b, dirty);
          }
          self.form.values[i] = json!(b);
        }
        Input::Choice { label, options, .. } => {
          let cur = v.as_str().unwrap_or("");
          let mut sel: &'static str = options.iter().find(|(k, _)| *k == cur).map(|(k, _)| *k).unwrap_or("");
          if let Some(why) = greyed {
            ui.label(*label);
            let shown = options.iter().find(|(k, _)| *k == sel).map(|(_, l)| *l).unwrap_or(sel);
            ui.add_enabled_ui(false, |ui| {
              egui::ComboBox::from_id_salt(("form-choice", i)).selected_text(shown).width(140.0).show_ui(ui, |_| {})
            })
            .response
            .on_disabled_hover_text(why);
            ui.end_row();
          } else if crate::forms::icon_dropdowns(f) {
            // an icon before every value, as Fusion's Extent Type, Direction and Operation
            ui.label(*label);
            if choice::combo(ui, &format!("form-choice-{i}"), options, &mut sel, choice::form_icon) {
              *dirty = true;
            }
          } else if options.iter().any(|(k, _)| crate::forms::greyed_option(f, input.key(), k).is_some()) {
            // a drop-down with entries FissionOxide can't do yet: those are greyed, with the reason
            ui.label(*label);
            let shown = options.iter().find(|(k, _)| *k == sel).map(|(_, l)| *l).unwrap_or(sel);
            egui::ComboBox::from_id_salt(("form-choice", i)).selected_text(shown).width(140.0).show_ui(ui, |ui| {
              for (k, l) in options.iter() {
                match crate::forms::greyed_option(f, input.key(), k) {
                  Some(why) => {
                    ui.add_enabled(false, egui::Button::selectable(false, *l)).on_disabled_hover_text(why);
                  }
                  None => {
                    if ui.selectable_label(sel == *k, *l).clicked() {
                      sel = k;
                      *dirty = true;
                    }
                  }
                }
              }
            });
            ui.end_row();
          } else {
            Self::choose(ui, label, &format!("form-choice-{i}"), &mut sel, options, dirty);
          }
          self.form.values[i] = json!(sel);
        }
        Input::Icons { key, label, options } => {
          ui.label(*label);
          let cur = v.as_str().unwrap_or("");
          let mut sel: &'static str = options.iter().find(|(k, _, _)| *k == cur).map(|(k, _, _)| *k).unwrap_or("");
          let mut why = "";
          let choices: Vec<icon_row::Choice> = options
            .iter()
            .map(|(k, icon, name)| match crate::forms::greyed_option(f, key, k) {
              Some(w) => {
                why = w;
                icon_row::greyed(k, *icon, name)
              }
              None => icon_row::choice(k, *icon, name),
            })
            .collect();
          if icon_row::icon_row(ui, &mut sel, &choices, why) {
            *dirty = true;
          }
          if *key == "filter" {
            filter = Some(sel);
          }
          self.form.values[i] = json!(sel);
          ui.end_row();
        }
        Input::Vec3 { label, .. } => {
          let mut xyz: [String; 3] = [0, 1, 2].map(|k| v.get(k).and_then(Value::as_str).unwrap_or("0").to_string());
          Self::xyz_row(ui, label, &mut xyz, dirty);
          self.form.values[i] = json!(xyz);
        }
        Input::DrawingView { label, .. } | Input::Component { label, .. } => {
          ui.label(*label);
          let list: Vec<(u32, String)> = if matches!(input, Input::DrawingView { .. }) {
            self.form.drawing.map(|d| drawing_views(s, d)).unwrap_or_default()
          } else {
            s.design.components.iter().map(|c| (c.id.0, c.name.clone())).collect()
          };
          let cur = list.iter().find(|(id, _)| v.as_u64() == Some(*id as u64)).map(|(_, n)| n.clone());
          let none = if list.is_empty() { "None" } else { "Pick one" };
          egui::ComboBox::from_id_salt(("form-item", i))
            .selected_text(cur.unwrap_or(none.into()))
            .width(140.0)
            .show_ui(ui, |ui| {
              for (id, name) in &list {
                if ui.selectable_label(v.as_u64() == Some(*id as u64), name).clicked() {
                  self.form.values[i] = json!(id);
                  *dirty = true;
                }
              }
            });
          ui.end_row();
        }
        Input::Profiles { .. } | Input::FaceOther { .. } | Input::PlaneOpt { .. } | Input::Cells { .. } => {
          self.solid_form_field(f, i, app, ui, dirty);
        }
        Input::Pieces { label, .. } => self.pieces_field(i, label, ui, dirty),
        Input::Flip { key, label } => {
          // Fusion's Flip icon button, lit while flipped
          ui.label(*label);
          let on = v.as_bool().unwrap_or(false);
          let tip = if *key == "flip" && f.cmd == "thread" { "Start the thread at the other end" } else { "Flip" };
          if crate::icons::icon_button(ui, Icon::Flip, 24.0, tip, on, None).clicked() {
            self.form.values[i] = json!(!on);
            *dirty = true;
          }
          ui.end_row();
        }
        // counted on the tools' chip
        Input::Planes { .. } => {}
        Input::Pick { label, multi, .. } => self.form_pick_row(app, ui, i, label, *multi),
        Input::TrimTool { label, .. } => {
          ui.label(*label);
          let n = match v.get("curves").and_then(Value::as_array) {
            Some(c) => c.len().max(1),
            None => usize::from(!v.is_null()),
          };
          sel_chip(ui, accent, &if n == 0 { "Select".to_string() } else { format!("{n} selected") }, n > 0);
          ui.end_row();
        }
        Input::Slider { label, max, .. } => {
          // the value, and a slider under it
          let mut t = v.as_str().unwrap_or("").to_string();
          Self::field(ui, label, &mut t, dirty);
          ui.label("");
          let mut x: f64 = t.trim().parse().unwrap_or(0.0);
          let slider = egui::Slider::new(&mut x, 0.0..=*max as f64).show_value(false);
          if ui.add(slider).changed() {
            t = format!("{x:.2}");
            *dirty = true;
          }
          ui.end_row();
          self.form.values[i] = json!(t);
        }
      }
    }
    if crate::forms::takes_sheet_points(f) {
      ui.label("");
      ui.label(RichText::new("click points on the sheet to fill them").small().color(theme::MUTED));
      ui.end_row();
    }
    // the Selection Filter decides what a click on the model picks while the dialog is open
    if let Some(mut f) = filter {
      self.filter_row_apply(app, &mut f);
    }
  }

  /// Text's own rows, drawn as Fusion draws them: a multi-line Text box with an fx button, the
  /// Font drop-down (the system's fonts, then the built-in two), the Typeface (B and I)
  /// toggles, the Flip toggles (greyed: the engine can't flip a text yet) and the Alignment
  /// toggles (three horizontal, three vertical). True when input `i` was drawn here (or belongs
  /// to a row drawn with another input).
  fn text_form_field(
    &mut self,
    f: &'static crate::forms::Form,
    i: usize,
    params: &[String],
    ui: &mut egui::Ui,
    dirty: &mut bool,
  ) -> bool {
    use icon_row::{choice, greyed, icon_row};
    let at = |k: &str| f.inputs.iter().position(|x| x.key() == k);
    match f.inputs[i].key() {
      "text" => {
        ui.label("Text");
        let mut t = self.form.values[i].as_str().unwrap_or("").to_string();
        let focus = self.take_text_focus();
        ui.horizontal(|ui| {
          ui.spacing_mut().item_spacing.x = 4.0;
          let r = ui.add(egui::TextEdit::multiline(&mut t).desired_rows(2).desired_width(112.0));
          if focus {
            // the sample text selected, so typing replaces it
            crate::viewport::focus_text(ui.ctx(), &r, &t, true);
          }
          if r.changed() {
            *dirty = true;
          }
          // fx: a user parameter's name as the text (its value is what Fusion shows)
          ui.menu_button(RichText::new("fx").italics().small(), |ui| {
            if params.is_empty() {
              ui.add_enabled(false, egui::Label::new("No user parameters"));
            }
            for p in params {
              if ui.button(p).clicked() {
                t = p.clone();
                *dirty = true;
                ui.close();
              }
            }
          })
          .response
          .on_hover_text("Insert a user parameter");
        });
        self.form.values[i] = json!(t);
        ui.end_row();
      }
      "font" => {
        ui.label("Font");
        let cur = self.form.values[i].as_str().unwrap_or("").to_string();
        let choices = crate::forms::font_choices();
        let shown = choices.iter().find(|(k, _)| *k == cur).map(|(_, l)| l.clone()).unwrap_or_else(|| cur.clone());
        egui::ComboBox::from_id_salt("text-font").selected_text(shown).width(140.0).show_ui(ui, |ui| {
          for (k, l) in choices {
            if ui.selectable_label(*k == cur, l).clicked() {
              self.form.values[i] = json!(k);
              *dirty = true;
            }
          }
        });
        ui.end_row();
      }
      "bold" => {
        ui.label("Typeface");
        let Some(it) = at("italic") else { return false };
        ui.horizontal(|ui| {
          ui.spacing_mut().item_spacing.x = 2.0;
          for (j, icon, name) in [(i, Icon::Bold, "Bold"), (it, Icon::Italic, "Italic")] {
            let on = self.form.values[j].as_bool().unwrap_or(false);
            if crate::icons::icon_button(ui, icon, 26.0, name, on, None).clicked() {
              self.form.values[j] = json!(!on);
              *dirty = true;
            }
          }
        });
        ui.end_row();
      }
      "flip_h" => {
        ui.label("Flip");
        const FLIPS: [icon_row::Choice; 2] =
          [greyed("h", Icon::FlipH, "Flip Horizontal"), greyed("v", Icon::FlipV, "Flip Vertical")];
        let mut none: &'static str = "";
        icon_row(ui, &mut none, &FLIPS, "flipping text isn't in FissionOxide yet");
        ui.end_row();
      }
      "halign" => {
        ui.label("Alignment");
        let Some(vi) = at("valign") else { return false };
        const H: [icon_row::Choice; 3] = [
          choice("left", Icon::AlignLeft, "Left"),
          choice("center", Icon::AlignCenter, "Center"),
          choice("right", Icon::AlignRight, "Right"),
        ];
        const V: [icon_row::Choice; 3] = [
          choice("top", Icon::AlignTop, "Top"),
          choice("middle", Icon::AlignMiddle, "Middle"),
          choice("bottom", Icon::AlignBottom, "Bottom"),
        ];
        let mut h: &'static str = match self.form.values[i].as_str() {
          Some("center") => "center",
          Some("right") => "right",
          _ => "left",
        };
        // (a box text's baseline alignment is its top)
        let mut v: &'static str = match self.form.values[vi].as_str() {
          Some("middle") => "middle",
          Some("bottom") => "bottom",
          _ => "top",
        };
        ui.horizontal(|ui| {
          ui.spacing_mut().item_spacing.x = 2.0;
          if icon_row(ui, &mut h, &H, "") {
            *dirty = true;
          }
          ui.add_space(6.0);
          if icon_row(ui, &mut v, &V, "") {
            *dirty = true;
          }
        });
        self.form.values[i] = json!(h);
        self.form.values[vi] = json!(v);
        ui.end_row();
      }
      // drawn with their row's first toggle
      "italic" | "flip_v" | "valign" => {}
      _ => return false,
    }
    true
  }

  /// Fields of the loft … sketch-modify dialogs.
  fn more_fields(&mut self, app: &mut App, ui: &mut egui::Ui, dirty: &mut bool) {
    let accent = self.accent();
    let chip = |ui: &mut egui::Ui, n: usize, what: &str| {
      let text = if n == 0 { "Select".to_string() } else { format!("{n} {what}") };
      sel_chip(ui, accent, &text, n > 0);
    };
    const RELIEFS: [(&str, &str); 3] = [("rectangular", "Rectangular"), ("round", "Round"), ("none", "None")];
    const JOINTS: [(&str, &str); 8] = [
      ("rigid", "Rigid"),
      ("revolute", "Revolute"),
      ("slider", "Slider"),
      ("cylindrical", "Cylindrical"),
      ("pin_slot", "Pin-slot"),
      ("planar", "Planar"),
      ("ball", "Ball"),
      ("screw", "Screw"),
    ];
    match self.kind {
      Kind::Loft => self.loft_fields(app, ui, dirty),
      Kind::Sweep => self.sweep_fields(app, ui, dirty),
      Kind::Hole => self.hole_fields(app, ui, dirty),
      Kind::PressPull => {
        ui.label("Faces");
        chip(ui, self.faces.len(), "selected");
        ui.end_row();
        self.offset_type_row(ui);
        Self::field(ui, "Distance", &mut self.distance, dirty);
      }
      Kind::Draft => self.draft_rows(ui, dirty),
      Kind::Scale => self.scale_rows(ui, dirty),
      Kind::OffsetFace => self.offset_face_rows(ui, dirty),
      Kind::SplitBody => self.split_body_fields(ui),
      Kind::Flange => self.flange_rows(app, ui, dirty),
      Kind::Hem => {
        self.flange_type_row(ui);
        self.hem_rows(app, ui, dirty);
      }
      Kind::Fold => self.fold_rows(app, ui, dirty),
      Kind::ConvertSheet => self.convert_rows(ui),
      Kind::Rip => self.rip_rows(ui, dirty),
      Kind::Unfold => self.unfold_rows(app, ui),
      Kind::SheetRules => {
        ui.label("Rule");
        let cur = self.name.clone();
        egui::ComboBox::from_id_salt("rules").selected_text(&cur).width(140.0).show_ui(ui, |ui| {
          for r in self.rules.clone() {
            if ui.selectable_label(cur == r, &r).clicked() {
              self.withdraw(&mut app.s);
              self.load_rule(&app.s, &r);
            }
          }
        });
        ui.end_row();
        Self::field(ui, "Name", &mut self.name, dirty);
        Self::field(ui, "Thickness", &mut self.a, dirty);
        Self::field(ui, "Bend radius", &mut self.b, dirty);
        Self::field(ui, "K factor", &mut self.c, dirty);
        Self::choose(ui, "Bend relief", "relief", &mut self.position, &RELIEFS, dirty);
        Self::field(ui, "Relief width", &mut self.d, dirty);
        Self::field(ui, "Relief depth", &mut self.e, dirty);
        Self::field(ui, "Gap", &mut self.distance2, dirty);
        Self::choose(ui, "Corner relief", "crelief", &mut self.corner_relief, &CORNER_RELIEFS, dirty);
        Self::field(ui, "Corner relief size", &mut self.corner_relief_size, dirty);
        ui.label("");
        ui.label(RichText::new("empty = from the thickness").small().color(theme::MUTED));
        ui.end_row();
      }
      Kind::FlatPattern => self.create_flat_rows(app, ui),
      Kind::NewComponent | Kind::ComponentProps => {
        let new = self.kind == Kind::NewComponent;
        Self::choose(ui, "Type", "comp-type", &mut self.position, &COMPONENT_TYPES, dirty);
        Self::field(ui, "Name", &mut self.name, dirty);
        Self::field(ui, "Part Number", &mut self.d, dirty);
        Self::field(ui, "Description", &mut self.e, dirty);
        if new {
          ui.label("Parent");
          let list: Vec<(u32, String)> = app.s.design.components.iter().map(|c| (c.id.0, c.name.clone())).collect();
          let cur = list.iter().find(|(c, _)| Some(*c) == self.target).map(|(_, n)| n.clone());
          egui::ComboBox::from_id_salt("comp-parent")
            .selected_text(cur.unwrap_or("Top level".into()))
            .width(140.0)
            .show_ui(ui, |ui| {
              if ui.selectable_label(self.target.is_none(), "Top level").clicked() {
                self.target = None;
                *dirty = true;
              }
              for (c, n) in &list {
                if ui.selectable_label(self.target == Some(*c), n).clicked() {
                  self.target = Some(*c);
                  *dirty = true;
                }
              }
            });
          ui.end_row();
          Self::choose(ui, "Create", "comp-from", &mut self.choice, &COMPONENT_FROM, dirty);
          if self.choice == "bodies" {
            ui.label("Bodies");
            chip(ui, self.bodies.len(), "selected");
            ui.end_row();
          }
          Self::check(ui, "Activate", &mut self.flag2, dirty);
        } else if let Some(c) = self.target.and_then(|c| app.s.design.component(fox_engine::model::ComponentId(c))) {
          ui.label("Material");
          ui.label(if c.material.is_empty() { "Steel" } else { c.material.as_str() });
          ui.end_row();
          ui.label("Bodies");
          ui.label(c.bodies.len().to_string());
          ui.end_row();
        }
      }
      Kind::AsBuiltJoint => {
        let n = self.comps.len();
        let fixed = self.comps.iter().position(|c| self.comps.first().is_some_and(|m| c != m));
        ui.label("Component 1");
        chip(ui, n.min(1), "component");
        ui.end_row();
        ui.label("Component 2");
        chip(ui, usize::from(fixed.is_some()), "component");
        ui.end_row();
        ui.label("Position");
        chip(ui, usize::from(fixed.is_some() && self.as_built_picks().is_some()), "origin");
        ui.end_row();
        if self.comps.first().is_some_and(Option::is_none) {
          ui.label("");
          ui.label(RichText::new("the first body isn't in a component").color(theme::WARNING));
          ui.end_row();
        }
        Self::choose(ui, "Motion", "joint", &mut self.choice, &JOINTS, dirty);
        if self.choice == "screw" {
          Self::field(ui, "Lead (per turn)", &mut self.e, dirty);
        }
      }
      Kind::Joint => {
        let saved = saved_origins(&app.s);
        let first = usize::from(self.saved[0].is_empty());
        ui.label("Component 1");
        if self.saved[0].is_empty() {
          chip(ui, self.origins.len().min(1), "origin");
        } else {
          ui.label(&self.saved[0]);
        }
        ui.end_row();
        if !saved.is_empty() {
          Self::saved_origin(ui, "jo-a", &mut self.saved[0], &saved, dirty);
        }
        ui.label("Component 2");
        if self.saved[1].is_empty() {
          chip(ui, self.origins.len().saturating_sub(first).min(1), "origin");
        } else {
          ui.label(&self.saved[1]);
        }
        ui.end_row();
        if !saved.is_empty() {
          Self::saved_origin(ui, "jo-b", &mut self.saved[1], &saved, dirty);
        }
        if self.saved[0].is_empty() && !self.origins.is_empty() && self.comps.first().is_some_and(Option::is_none) {
          ui.label("");
          ui.label(RichText::new("the first body isn't in a component").color(theme::WARNING));
          ui.end_row();
        }
        Self::choose(ui, "Motion", "joint", &mut self.choice, &JOINTS, dirty);
        if self.choice == "screw" {
          Self::field(ui, "Lead (per turn)", &mut self.e, dirty);
        }
        Self::field(ui, "Angle", &mut self.angle, dirty);
        Self::field(ui, "Offset", &mut self.distance, dirty);
        Self::check(ui, "Flip", &mut self.flip, dirty);
      }
      Kind::Interference => self.interference_fields(app, ui),
      Kind::CenterOfMass => self.center_of_mass_rows(app, ui),
      Kind::Bom => {
        let rows = self.report.as_ref().and_then(Value::as_array).cloned().unwrap_or_default();
        if rows.is_empty() {
          ui.label("No components");
          ui.end_row();
        }
        for r in rows {
          let t = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or("").to_string();
          let f = |k: &str| r.get(k).and_then(Value::as_f64).unwrap_or(0.0);
          ui.label(RichText::new(t("name")).strong());
          ui.label(format!("{} · {:.1} g", t("material"), f("mass_g")));
          ui.end_row();
        }
      }
      Kind::SkFillet | Kind::SkChamfer | Kind::SkChamfer2 | Kind::SkChamferAngle => {
        ui.label("Lines");
        chip(ui, self.curves.len(), "selected");
        ui.end_row();
        if self.kind == Kind::SkFillet {
          Self::field(ui, "Radius", &mut self.radius, dirty);
        } else {
          Self::field(
            ui,
            if self.kind == Kind::SkChamfer2 { "Distance 1" } else { "Distance" },
            &mut self.distance,
            dirty,
          );
          if self.kind == Kind::SkChamfer2 {
            Self::field(ui, "Distance 2", &mut self.distance2, dirty);
          }
          if self.kind == Kind::SkChamferAngle {
            Self::field(ui, "Angle", &mut self.angle, dirty);
          }
        }
      }
      Kind::SkOffset => {
        ui.label("Curves");
        chip(ui, self.curves.len(), "selected");
        ui.end_row();
        ui.label("Chain Selection");
        let mut chain = !self.solid.no_chain;
        if ui
          .checkbox(&mut chain, "")
          .on_hover_text("Picking a curve picks the curves joined to it end to end")
          .changed()
        {
          self.solid.no_chain = !chain;
          let c = self.offset_picks(&app.s);
          if !c.is_empty() && c != self.curves {
            self.curves = c;
            *dirty = true;
          }
        }
        ui.end_row();
        // Fusion's Direction values carry the icons Extrude's do
        const DIRECTIONS: [(&str, &str); 3] =
          [("one_side", "One Side"), ("two_sides", "Two Sides"), ("symmetric", "Symmetric")];
        ui.label("Direction");
        if choice::combo(ui, "offset-dir", &DIRECTIONS, &mut self.extent, choice::extrude_icon) {
          *dirty = true;
        }
        ui.end_row();
        let before = self.distance.clone();
        Self::field(ui, "Distance", &mut self.distance, dirty);
        if self.distance != before {
          // a typed distance holds: the offset stops following the cursor
          self.follow = false;
        }
        if self.extent == "two_sides" {
          Self::field(ui, "Distance 2", &mut self.distance2, dirty);
        }
        // Flip is an icon button, as in Fusion
        ui.label("Flip");
        let flip =
          crate::icons::icon_button(ui, Icon::Flip, 24.0, "Offset to the other side of the curves", false, None);
        if flip.clicked() {
          self.distance = negated(&self.distance);
          self.follow = false;
          *dirty = true;
        }
        ui.end_row();
      }
      Kind::SkMirror => {
        // Fusion: Objects and Mirror Line pick buttons (a click on a row makes it the one the
        // next pick goes to), nothing else
        let objects = chip_text(self.curves.len(), "selected");
        self.pick_row_chip(ui, "Objects", 0, &objects, !self.curves.is_empty());
        let line = if self.line.is_some() { "1 selected" } else { "Select" };
        self.pick_row_chip(ui, "Mirror Line", 1, line, self.line.is_some());
      }
      Kind::SkScale => {
        // Fusion: Entities and Point pick buttons; the Scale Factor row appears once the point is
        // picked (the arrow at the point and its value box set it too)
        let entities = chip_text(self.curves.len(), "selected");
        self.pick_row_chip(ui, "Entities", 0, &entities, !self.curves.is_empty());
        let point = if self.center.is_some() { "1 selected" } else { "Select" };
        self.pick_row_chip(ui, "Point", 1, point, self.center.is_some());
        if self.center.is_some() {
          Self::field(ui, "Scale Factor", &mut self.a, dirty);
        }
      }
      Kind::SkCircPattern => {
        let objects = chip_text(self.curves.len(), "selected");
        self.pick_row_chip(ui, "Objects", 0, &objects, !self.curves.is_empty());
        let centre = if self.center.is_some() { "1 selected" } else { "Select" };
        self.pick_row_chip(ui, "Center Point", 1, centre, self.center.is_some());
        // Fusion's rows once both are picked: Distribution (Full, Angle or Symmetric; the angle
        // row only for the last two), Suppression and the Quantity spinner
        if !self.curves.is_empty() && self.center.is_some() {
          const DISTRIBUTIONS: [(&str, Icon, &str); 3] = [
            ("full", Icon::DistFull, "Full"),
            ("angle", Icon::DistAngle, "Angle"),
            ("symmetric", Icon::DistSymmetric, "Symmetric"),
          ];
          if self.choice.is_empty() {
            self.choice = "full";
          }
          Self::icon_choose(ui, "Distribution", "skcirc-dist", &mut self.choice, &DISTRIBUTIONS, dirty);
          Self::check_greyed(ui, "Suppression", "Suppressing pattern instances isn't in FissionOxide yet");
          Self::count_field(ui, "Quantity", &mut self.count, dirty);
          if self.choice != "full" {
            Self::field(ui, "Total Angle", &mut self.angle, dirty);
          }
        }
      }
      Kind::SkRectPattern => {
        let objects = chip_text(self.curves.len(), "selected");
        self.pick_row_chip(ui, "Objects", 0, &objects, !self.curves.is_empty());
        let dirs = match self.dir_lines.len() {
          0 => "Select".to_string(),
          1 => "1 Line".to_string(),
          n => format!("{n} Lines"),
        };
        self.pick_row_chip(ui, "Direction/s", 1, &dirs, !self.dir_lines.is_empty());
        // Fusion's rows once objects are picked: Distribution and Suppression, then Quantity,
        // Distance and Direction for the first direction and the same three for the second
        if !self.curves.is_empty() {
          const DISTRIBUTIONS: [(&str, Icon, &str); 2] =
            [("extent", Icon::DistExtent, "Extent"), ("spacing", Icon::DistSpacing, "Spacing")];
          const DIRECTIONS: [(&str, Icon, &str); 2] =
            [("one", Icon::DirOne, "One Direction"), ("symmetric", Icon::DirSymmetric, "Symmetric")];
          let mut ty: &'static str = if self.pattern_extent { "extent" } else { "spacing" };
          Self::icon_choose(ui, "Distribution", "skrect-dist", &mut ty, &DISTRIBUTIONS, dirty);
          self.pattern_extent = ty == "extent";
          Self::check_greyed(ui, "Suppression", "Suppressing pattern instances isn't in FissionOxide yet");
          for k in 0..2 {
            let (count, spacing) =
              if k == 0 { (&mut self.count, &mut self.spacing) } else { (&mut self.count2, &mut self.spacing2) };
            Self::count_field(ui, "Quantity", count, dirty);
            Self::field(ui, "Distance", spacing, dirty);
            let mut dir: &'static str = if self.symmetric[k] { "symmetric" } else { "one" };
            Self::icon_choose(ui, "Direction", &format!("skrect-dir{k}"), &mut dir, &DIRECTIONS, dirty);
            self.symmetric[k] = dir == "symmetric";
          }
          // a second quantity of 1 patterns one way
          self.use_dir2 = self.count2.trim().parse::<u64>().ok().is_none_or(|n| n > 1);
        }
      }
      Kind::DriveJoint | Kind::MotionStudy => {
        ui.label("Joint");
        let list = joints(&app.s);
        let cur = list.iter().find(|(j, _)| Some(*j) == self.target).map(|(_, n)| n.clone());
        let mut picked = false;
        egui::ComboBox::from_id_salt("joint").selected_text(cur.unwrap_or("No joints".into())).width(140.0).show_ui(
          ui,
          |ui| {
            for (j, n) in &list {
              if ui.selectable_label(self.target == Some(*j), n).clicked() {
                self.target = Some(*j);
                picked = true;
              }
            }
          },
        );
        ui.end_row();
        if picked {
          self.withdraw(&mut app.s);
          self.load_joint(&app.s);
          self.report = None;
        }
        if self.kind == Kind::DriveJoint {
          Self::field(ui, "Rotation", &mut self.angle, dirty);
          Self::field(ui, "Slide", &mut self.distance, dirty);
          Self::field(ui, "Slide 2", &mut self.distance2, dirty);
        } else {
          const MOTIONS: [(&str, &str); 4] =
            [("rotation", "Rotation"), ("slide", "Slide"), ("slide2", "Slide 2"), ("pitch", "Pitch")];
          // the study runs on demand: it rebuilds the design at every step
          let mut later = false;
          Self::choose(ui, "Motion", "motion", &mut self.position, &MOTIONS, &mut later);
          Self::field(ui, "From", &mut self.a, &mut later);
          Self::field(ui, "To", &mut self.b, &mut later);
          Self::field(ui, "Steps", &mut self.count, &mut later);
          ui.label("");
          if panels::secondary_button(ui, "Run").clicked() {
            self.refresh_report(&mut app.s);
          }
          ui.end_row();
          for row in self.report.as_ref().and_then(Value::as_array).cloned().unwrap_or_default() {
            let v = row.get("value").and_then(Value::as_f64).unwrap_or(0.0);
            let unit = if matches!(self.position, "rotation" | "pitch") { "°" } else { " mm" };
            ui.label(RichText::new(format!("  {v:.1}{unit}")).color(theme::MUTED));
            let clashes = row.get("interference").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
            match row.get("error").and_then(Value::as_str) {
              Some(e) => ui.label(RichText::new(e).color(theme::ERROR)),
              None if clashes > 0 => ui.label(RichText::new(format!("{clashes} interference")).color(theme::WARNING)),
              None => ui.label("clear"),
            };
            ui.end_row();
          }
        }
      }
      Kind::CopyComponent => {
        ui.label("Component");
        let list: Vec<(u32, String)> = app.s.design.components.iter().map(|c| (c.id.0, c.name.clone())).collect();
        let cur = list.iter().find(|(c, _)| Some(*c) == self.target).map(|(_, n)| n.clone());
        egui::ComboBox::from_id_salt("comp").selected_text(cur.unwrap_or("No components".into())).width(140.0).show_ui(
          ui,
          |ui| {
            for (c, n) in &list {
              if ui.selectable_label(self.target == Some(*c), n).clicked() {
                self.target = Some(*c);
                *dirty = true;
              }
            }
          },
        );
        ui.end_row();
        for (i, l) in ["X distance", "Y distance", "Z distance"].iter().enumerate() {
          Self::field(ui, l, &mut self.translate[i], dirty);
        }
      }
      Kind::NewDrawing | Kind::DrawingSettings => {
        Self::field(ui, "Name", &mut self.name, dirty);
        if self.kind == Kind::NewDrawing {
          ui.label("Views");
          ui.vertical(|ui| {
            for (i, (_, l)) in VIEWS.iter().enumerate() {
              if ui.checkbox(&mut self.view_on[i], *l).changed() {
                *dirty = true;
              }
            }
          });
          ui.end_row();
        }
        Self::choose(ui, "Sheet size", "sheet", &mut self.position, &SHEETS, dirty);
        Self::field(ui, "Scale", &mut self.a, dirty);
        Self::choose(ui, "Projection", "proj", &mut self.choice, &PROJECTIONS, dirty);
        Self::check(ui, "Automatic dimensions", &mut self.flag, dirty);
        Self::check(ui, "Parts list", &mut self.flag2, dirty);
        const STANDARDS: [(&str, &str); 2] = [("asme", "ASME"), ("iso", "ISO")];
        Self::choose(ui, "Standard", "dstd", &mut self.extent, &STANDARDS, dirty);
        if self.kind == Kind::DrawingSettings {
          Self::check(ui, "Auto balloons", &mut self.keep_tools, dirty);
        }
        if self.kind == Kind::DrawingSettings {
          ui.label("");
          if panels::secondary_button(ui, "Delete Drawing").clicked() {
            self.withdraw(&mut app.s);
            if app.run("drawing.delete", json!({"drawing": self.target})).is_some() {
              app.drawing = None;
              self.closed = true;
            }
          }
          ui.end_row();
        }
      }
      Kind::DrawingView => {
        Self::choose(ui, "View", "vkind", &mut self.choice, &VIEW_KINDS, dirty);
        match self.choice {
          "section" => {
            const AXES: [(&str, &str); 3] = [("x", "Across X"), ("y", "Across Y"), ("z", "Across Z")];
            let mut a: &'static str = AXES.iter().find(|(k, _)| *k == self.axis).map(|(k, _)| *k).unwrap_or("x");
            Self::choose(ui, "Cut", "vaxis", &mut a, &AXES, dirty);
            self.axis = a.into();
            Self::field(ui, "Offset (mm)", &mut self.distance, dirty);
            Self::check(ui, "Flip", &mut self.flip, dirty);
          }
          "detail" => {
            self.view_combo(app, ui, "Parent view", dirty);
            Self::xyz_row(ui, "Centre (model)", &mut self.xyz, dirty);
            Self::field(ui, "Radius (mm)", &mut self.radius, dirty);
            Self::field(ui, "Enlargement", &mut self.count, dirty);
          }
          "flat" => {
            ui.label("Body");
            chip(ui, self.bodies.len().min(1), "body");
            ui.end_row();
          }
          _ => {}
        }
        Self::xy_row(ui, "Position (sheet)", &mut self.at, dirty);
        ui.label("");
        ui.label(RichText::new("empty = placed automatically").small().color(theme::MUTED));
        ui.end_row();
        Self::check(ui, "Hidden lines", &mut self.flag, dirty);
      }
      Kind::DrawingMoveView => {
        if self.view_combo(app, ui, "View", dirty) {
          self.withdraw(&mut app.s);
          self.load_view_at(&app.s);
        }
        Self::check(ui, "Place automatically", &mut self.flag, dirty);
        if !self.flag {
          Self::xy_row(ui, "Centre (sheet mm)", &mut self.at, dirty);
        }
        ui.label("");
        if panels::secondary_button(ui, "Delete View").clicked() && self.view.is_some() {
          self.withdraw(&mut app.s);
          if app.run("drawing.remove_view", json!({"drawing": self.target, "view": self.view})).is_some() {
            self.closed = true;
          }
        }
        ui.end_row();
      }
      Kind::DrawingDimension => {
        self.view_combo(app, ui, "View", dirty);
        Self::xyz_row(ui, "Point 1 (model)", &mut self.translate, dirty);
        Self::xyz_row(ui, "Point 2 (model)", &mut self.xyz, dirty);
        const ORIENT: [(&str, &str); 3] =
          [("aligned", "Aligned"), ("horizontal", "Horizontal"), ("vertical", "Vertical")];
        Self::choose(ui, "Orientation", "orient", &mut self.choice, &ORIENT, dirty);
        Self::field(ui, "Offset (sheet mm)", &mut self.distance, dirty);
        ui.label("");
        ui.label(RichText::new("click points on the sheet, or select a model edge").small().color(theme::MUTED));
        ui.end_row();
      }
      Kind::DrawingNote => {
        Self::field(ui, "Text", &mut self.name, dirty);
        Self::xy_row(ui, "Position (sheet mm)", &mut self.at, dirty);
      }
      Kind::DrawingTitle => {
        Self::field(ui, "Title", &mut self.name, dirty);
        Self::field(ui, "Author", &mut self.a, dirty);
        Self::field(ui, "Part number", &mut self.b, dirty);
        Self::field(ui, "Material", &mut self.c, dirty);
        Self::field(ui, "Revision", &mut self.d, dirty);
        Self::field(ui, "Company", &mut self.e, dirty);
      }
      Kind::Project => {
        ui.label("Geometry");
        let what = if self.faces.is_empty() { "body" } else { "face" };
        chip(ui, self.bodies.len().min(1), what);
        ui.end_row();
        // Fusion's Selection Filter: specified entities (faces and edges), or whole bodies
        let mut filter: &'static str = if self.choice == "bodies" { "bodies" } else { "specified" };
        ui.label("Selection Filter");
        if self.filter_row(app, ui, &mut filter) {
          self.choice = filter;
        }
        ui.end_row();
        Self::check(ui, "Projection Link", &mut self.flag, dirty);
      }
      _ => {}
    }
  }

  /// A selection row of the sketch Mirror and pattern dialogs: its chip is lit while it is the
  /// row the next picks go to (Fusion's blue active row), and a click on another row's chip
  /// makes that one the active row.
  fn pick_row_chip(&mut self, ui: &mut egui::Ui, label: &str, row: usize, text: &str, has: bool) {
    ui.label(label);
    let active = self.pick_row == row;
    let chip = sel_chip(ui, self.accent(), text, active || has);
    if !active && chip.interact(egui::Sense::click()).on_hover_text("Pick for this row").clicked() {
      self.pick_row = row;
    }
    ui.end_row();
  }

  /// Fusion's Selection Filter row (Specified Entities, then Bodies) as two icon toggles; while
  /// the dialog is open the choice decides what a click on the model picks. Returns true on a
  /// change.
  fn filter_row(&mut self, app: &mut App, ui: &mut egui::Ui, cur: &mut &'static str) -> bool {
    use icon_row::{choice, icon_row};
    const FILTERS: [icon_row::Choice; 2] =
      [choice("specified", Icon::FilterFaces, "Specified Entities"), choice("bodies", Icon::FilterBodies, "Bodies")];
    let changed = icon_row(ui, cur, &FILTERS, "");
    self.filter_row_apply(app, cur);
    changed
  }

  /// Makes the Selection Filter's choice the pick priority (put back when the dialog closes).
  fn filter_row_apply(&mut self, app: &mut App, cur: &mut &'static str) {
    use crate::select::Priority;
    if self.saved_filter.is_none() {
      self.saved_filter = Some((app.sel_priority, app.sel_filter.vertices));
    }
    app.sel_priority = if *cur == "bodies" { Priority::Body } else { Priority::Face };
  }

  /// A check box FissionOxide can't act on yet, drawn greyed with the reason as its tooltip.
  fn check_greyed(ui: &mut egui::Ui, label: &str, why: &str) {
    ui.label("");
    let mut off = false;
    ui.add_enabled_ui(false, |ui| ui.checkbox(&mut off, label)).inner.on_disabled_hover_text(why);
    ui.end_row();
  }

  /// Sketch Mirror, patterns and Scale: the picks, routed by the active row. Objects take every
  /// picked curve not used by the other row; the other row takes the newest pick that fits it (a
  /// line for the mirror line or a direction, a sketch point or circle for the centre). Scale's
  /// point is a sketch point, taken whichever row is active (points are never entities).
  fn sync_sketch_picks(&mut self, s: &Session) {
    use fox_engine::sketch::{CurveId, CurveKind};
    let Some(sk) = s.edit_sketch else { return };
    let Some(r) = s.model.sketches.get(&sk) else { return };
    let line_dir = |c: u32| -> Option<[f64; 2]> {
      let CurveKind::Line { a, b } = r.sketch.curve(CurveId(c))?.kind else { return None };
      let (pa, pb) = (r.sketch.pos(a)?, r.sketch.pos(b)?);
      let (dx, dy) = (pb[0] - pa[0], pb[1] - pa[1]);
      let len = dx.hypot(dy);
      (len > 1e-9).then(|| [dx / len, dy / len])
    };
    let picked = sel_curves(s);
    if self.pick_row == 1 || self.kind == Kind::SkScale {
      let used = self.other_row_curves();
      let fresh = picked.iter().copied().find(|c| !self.curves.contains(c) && !used.contains(c));
      match self.kind {
        Kind::SkMirror => {
          if let Some(c) = fresh.filter(|c| line_dir(*c).is_some()) {
            self.line = Some(c);
            self.dirty = true;
          }
        }
        Kind::SkRectPattern => {
          if let Some((c, d)) = fresh.and_then(|c| Some((c, line_dir(c)?)))
            && self.dir_lines.len() < 2
          {
            self.dir_lines.push((c, d));
            self.dirty = true;
          }
        }
        _ => {
          // a sketch point, or a circle's or arc's centre
          let point = s.selection.iter().rev().find_map(|x| match x {
            Sel::SketchPoint { sketch, point } if *sketch == sk => Some((*x, r.sketch.pos(*point)?)),
            _ => None,
          });
          // (a circle's centre only with the centre row active: it is an object otherwise)
          let circle = fresh.filter(|_| self.pick_row == 1).and_then(|c| match r.sketch.curve(CurveId(c))?.kind {
            CurveKind::Circle { center, .. } | CurveKind::Arc { center, .. } => {
              Some((Sel::SketchCurve { sketch: sk, curve: CurveId(c) }, r.sketch.pos(center)?))
            }
            _ => None,
          });
          if let Some((sel, at)) = point.or(circle)
            && self.center.as_ref() != Some(&sel)
          {
            self.at = [crate::forms::short(at[0]), crate::forms::short(at[1])];
            self.center = Some(sel);
            self.dirty = true;
          }
        }
      }
    }
    let used = self.other_row_curves();
    let objects: Vec<u32> = picked.into_iter().filter(|c| !used.contains(c)).collect();
    if !objects.is_empty() && objects != self.curves {
      // Mirror: the first objects pick moves the active row on to the mirror line with the
      // prompt, so the next line clicked is the line (Fusion); the other rows keep Objects
      // active for more picks
      if self.kind == Kind::SkMirror && self.curves.is_empty() && self.pick_row == 0 && self.line.is_none() {
        self.pick_row = 1;
      }
      self.curves = objects;
      self.dirty = true;
    }
  }

  /// The curves the sketch Mirror's or patterns' second row holds (not objects to copy).
  fn other_row_curves(&self) -> Vec<u32> {
    match self.kind {
      Kind::SkMirror => self.line.into_iter().collect(),
      Kind::SkRectPattern => self.dir_lines.iter().map(|l| l.0).collect(),
      _ => match self.center {
        Some(Sel::SketchCurve { curve, .. }) => vec![curve.0],
        _ => Vec::new(),
      },
    }
  }

  /// Text: is the dialog waiting for a corner of its frame to be clicked on the sketch?
  pub(crate) fn wants_text_frame(&self) -> bool {
    let form = &crate::forms::SK_TEXT;
    matches!(self.kind, Kind::Form(f) if std::ptr::eq(f, form))
      && matches!(
        self.form.missing(form),
        Some("Place first corner of text frame" | "Place second corner of text frame")
      )
  }

  /// Text: the frame's first corner (sketch coordinates) while the second is awaited, for the
  /// rubber-band frame that follows the pointer.
  pub(crate) fn text_frame_start(&self) -> Option<[f64; 2]> {
    match self.kind {
      Kind::Form(f) if std::ptr::eq(f, &crate::forms::SK_TEXT) => self.form.text_corner_at(f),
      _ => None,
    }
  }

  /// Text: a click at `at` (sketch coordinates) places the frame's first corner, then its second
  /// (with the angle, degrees, the text reads at); a second corner on the first's row or column
  /// is ignored (no frame).
  pub(crate) fn place_text_corner(&mut self, at: [f64; 2], angle: f64) {
    let Kind::Form(f) = self.kind else { return };
    match self.form.text_corner_at(f) {
      None => self.form.text_first_corner(f, at),
      Some(a) => {
        if (a[0] - at[0]).abs() < 1e-9 || (a[1] - at[1]).abs() < 1e-9 {
          return;
        }
        self.set_text_frame(a, at, angle);
      }
    }
  }

  /// Text: the placed frame's corners (sketch coordinates) and the angle (degrees) the text
  /// reads at, for the frame drawn round the text with its rotate handle.
  pub(crate) fn text_frame(&self) -> Option<([f64; 2], [f64; 2], f64)> {
    let Kind::Form(f) = self.kind else { return None };
    if !std::ptr::eq(f, &crate::forms::SK_TEXT) || !self.form.text_framed(f) {
      return None;
    }
    let (a, b) = self.form.text_corners(f)?;
    Some((a, b, self.form.text_angle(f)))
  }

  /// Text: turns the frame (and the text in it) to read at `angle` degrees.
  pub(crate) fn set_text_angle(&mut self, angle: f64) {
    if let Kind::Form(f) = self.kind
      && let Some(i) = f.inputs.iter().position(|x| x.key() == "angle")
    {
      self.form.values[i] = json!(crate::forms::short(angle));
      self.dirty = true;
    }
  }

  /// A form dialog's value for input `key`.
  pub(crate) fn form_value(&self, key: &str) -> Option<&Value> {
    let Kind::Form(f) = self.kind else { return None };
    self.form.values.get(f.inputs.iter().position(|x| x.key() == key)?)
  }

  /// Text: does the Text box take the keyboard this frame (the frame was just placed)?
  fn take_text_focus(&mut self) -> bool {
    std::mem::take(&mut self.focus_text)
  }

  /// The sketch curves this dialog's preview made, drawn as thin grey outlines until OK (the
  /// sketch Mirror's and patterns' copies, as Fusion previews them).
  pub(crate) fn preview_curves(&self) -> &[u32] {
    if self.applied && matches!(self.kind, Kind::SkMirror | Kind::SkCircPattern | Kind::SkRectPattern) {
      &self.made
    } else {
      &[]
    }
  }

  /// Text: the frame placed on the sketch, from corner `a` to `b`, and the angle (degrees) the
  /// text reads at; the dialog grows, the text appears in the frame and the Text box takes the
  /// keyboard.
  pub(crate) fn set_text_frame(&mut self, a: [f64; 2], b: [f64; 2], angle: f64) {
    if let Kind::Form(f) = self.kind {
      self.form.text_frame(f, a, b, angle);
      self.focus_text = true;
      self.dirty = true;
    }
  }

  /// Re-runs a report dialog's query.
  fn refresh_report(&mut self, s: &mut Session) {
    let r = match self.kind {
      Kind::Interference => {
        // the selected bodies (two or more), else every pair of components
        let picked = sel_only_bodies(s);
        self.bodies = if picked.len() >= 2 { picked } else { Vec::new() };
        s.execute("assembly.interference", &json!({"bodies": self.bodies}))
      }
      Kind::Bom => s.execute("assembly.bom", &json!({})),
      Kind::MotionStudy => match self.target {
        Some(j) => s.execute(
          "motion.study",
          &json!({
            "joint": j, "motion": self.position, "from": self.a, "to": self.b,
            "steps": self.count.trim().parse::<u64>().unwrap_or(8),
          }),
        ),
        None => return,
      },
      _ => return,
    };
    match r {
      Ok(v) => {
        self.report = Some(v);
        self.error = None;
      }
      Err(e) => {
        self.report = None;
        self.error = Some(e.0);
      }
    }
  }

  /// Adds the interferences of the bodies last computed as bodies (in components when ticked).
  fn create_interference(&mut self, s: &mut Session) {
    let p = json!({"bodies": self.bodies, "create_bodies": true, "components": self.flag});
    match s.execute("assembly.interference", &p) {
      Ok(v) => {
        self.report = Some(v);
        self.error = None;
      }
      Err(e) => self.error = Some(e.0),
    }
  }

  /// Fills the drive fields from the joint's current motion.
  fn load_joint(&mut self, s: &Session) {
    if let Some(FeatureKind::Joint { rotation, slide, slide2, .. }) =
      self.target.and_then(|j| s.design.feature(FeatureId(j))).map(|f| &f.kind)
    {
      self.angle = rotation.clone();
      self.distance = slide.clone();
      self.distance2 = slide2.clone();
    }
  }

  /// Fills the drawing dialogs' fields from the open drawing.
  fn load_drawing(&mut self, s: &Session) {
    let Some(d) = self.target.and_then(|id| s.design.drawing(id)) else { return };
    self.view = d.views.first().map(|v| v.id);
    match self.kind {
      Kind::DrawingSettings => {
        self.name = d.name.clone();
        self.position = SHEETS
          .iter()
          .find(|(k, _)| fox_engine::model::drawing::SheetSize::parse(k) == Some(d.sheet))
          .map(|(k, _)| *k)
          .unwrap_or("a3");
        self.a = d.scale.map(fox_engine::model::drawing::scale_text).unwrap_or("auto".into());
        self.choice = match d.projection {
          fox_engine::model::drawing::ProjectionAngle::FirstAngle => "first_angle",
          _ => "third_angle",
        };
        self.flag = d.auto_dimensions;
        self.flag2 = d.parts_list;
        self.extent = match d.standard {
          fox_engine::model::drawing::Standard::Iso => "iso",
          _ => "asme",
        };
        self.keep_tools = d.auto_balloons;
      }
      Kind::DrawingTitle => {
        let t = &d.title;
        self.name = t.title.clone();
        self.a = t.author.clone();
        self.b = t.part_number.clone();
        self.c = t.material.clone();
        self.d = t.revision.clone();
        self.e = t.company.clone();
      }
      Kind::DrawingMoveView => self.load_view_at(s),
      _ => {}
    }
  }

  fn load_view_at(&mut self, s: &Session) {
    let at = self
      .target
      .and_then(|id| s.design.drawing(id))
      .and_then(|d| d.views.iter().find(|v| Some(v.id) == self.view))
      .and_then(|v| v.at);
    self.flag = at.is_none();
    self.at = at.map(|[x, y]| [format!("{x:.1}"), format!("{y:.1}")]).unwrap_or(["".into(), "".into()]);
  }

  /// A drop-down of the open drawing's views.
  /// Does the dialog take model points picked on the drawing sheet?
  pub fn takes_sheet_points(&self) -> bool {
    match self.kind {
      Kind::DrawingDimension => true,
      Kind::Form(f) => crate::forms::takes_sheet_points(f),
      _ => false,
    }
  }

  /// A model point picked on the drawing sheet, as `view` shows it: Dimension takes its two
  /// points in turn, forms fill their point inputs (see [`crate::forms::State::sheet_pick`]).
  pub fn sheet_pick(&mut self, view: u32, point: [f64; 3]) {
    match self.kind {
      Kind::DrawingDimension => {
        if self.view != Some(view) {
          self.view = Some(view);
          self.sheet_picks = 0;
        }
        let xyz = point.map(crate::forms::short);
        if self.sheet_picks.is_multiple_of(2) {
          self.translate = xyz;
        } else {
          self.xyz = xyz;
        }
        self.sheet_picks += 1;
      }
      Kind::Form(f) => self.form.sheet_pick(f, view, point),
      _ => return,
    }
    if !self.picks() || self.applied {
      self.dirty = true;
    }
  }

  fn view_combo(&mut self, app: &App, ui: &mut egui::Ui, label: &str, dirty: &mut bool) -> bool {
    let views = self.target.map(|id| drawing_views(&app.s, id)).unwrap_or_default();
    ui.label(label);
    let cur = views.iter().find(|(v, _)| Some(*v) == self.view).map(|(_, l)| l.clone()).unwrap_or("Pick a view".into());
    let mut changed = false;
    egui::ComboBox::from_id_salt(("view", label)).selected_text(cur).width(140.0).show_ui(ui, |ui| {
      for (v, l) in &views {
        if ui.selectable_label(self.view == Some(*v), l).clicked() {
          self.view = Some(*v);
          *dirty = true;
          changed = true;
        }
      }
    });
    ui.end_row();
    changed
  }

  fn xyz_row(ui: &mut egui::Ui, label: &str, v: &mut [String; 3], dirty: &mut bool) {
    ui.label(label);
    ui.horizontal(|ui| {
      for x in v.iter_mut() {
        let mut r = ui.add(egui::TextEdit::singleline(x).desired_width(44.0));
        crate::units::pin(ui, &mut r, x);
        if r.changed() {
          *dirty = true;
        }
      }
    });
    ui.end_row();
  }

  fn xy_row(ui: &mut egui::Ui, label: &str, v: &mut [String; 2], dirty: &mut bool) {
    ui.label(label);
    ui.horizontal(|ui| {
      for x in v.iter_mut() {
        let mut r = ui.add(egui::TextEdit::singleline(x).desired_width(64.0));
        crate::units::pin(ui, &mut r, x);
        if r.changed() {
          *dirty = true;
        }
      }
    });
    ui.end_row();
  }

  /// Fills the rule fields from the named rule ("" = the default rule).
  fn load_rule(&mut self, s: &Session, name: &str) {
    let r = s.design.sheet_rule(name).unwrap_or_default();
    self.name = r.name.clone();
    self.a = r.thickness;
    self.b = r.bend_radius;
    self.c = r.k_factor;
    self.d = r.relief_width;
    self.e = r.relief_depth;
    self.distance2 = r.gap;
    self.position = match r.relief {
      fox_engine::model::ReliefShape::Rectangular => "rectangular",
      fox_engine::model::ReliefShape::Round => "round",
      fox_engine::model::ReliefShape::None => "none",
    };
    self.corner_relief = match r.corner_relief {
      fox_engine::model::CornerRelief::None => "none",
      fox_engine::model::CornerRelief::Round => "round",
      fox_engine::model::CornerRelief::Square => "square",
      fox_engine::model::CornerRelief::Tear => "tear",
    };
    self.corner_relief_size = r.corner_relief_size;
    self.rules = s.design.sheet_metal_rules.iter().map(|r| r.name.clone()).collect();
    if self.rules.is_empty() {
      self.rules.push(self.name.clone());
    }
  }

  fn put_rule(&self, p: &mut Value) {
    if !self.rule.is_empty() {
      p["rule"] = json!(self.rule);
    }
  }

  /// A text field that re-previews when edited.
  fn field(ui: &mut egui::Ui, label: &str, v: &mut String, dirty: &mut bool) {
    ui.label(label);
    if value_edit(ui, v).changed() {
      *dirty = true;
    }
    ui.end_row();
  }

  /// A drop-down of (key, label) choices that re-previews when changed.
  fn choose(
    ui: &mut egui::Ui,
    label: &str,
    id: &str,
    cur: &mut &'static str,
    items: &[(&'static str, &'static str)],
    dirty: &mut bool,
  ) {
    ui.label(label);
    let shown = items.iter().find(|(k, _)| k == cur).map(|(_, l)| *l).unwrap_or(cur);
    egui::ComboBox::from_id_salt(id).selected_text(shown).width(140.0).show_ui(ui, |ui| {
      for (k, l) in items {
        if ui.selectable_label(*cur == *k, *l).clicked() {
          *cur = k;
          *dirty = true;
        }
      }
    });
    ui.end_row();
  }

  /// Fusion's icon drop-down (the sketch patterns' Distribution and Direction rows): a
  /// drop-down whose entries are an icon and a name, the chosen one's icon and name on the
  /// button. Re-previews when changed.
  fn icon_choose(
    ui: &mut egui::Ui,
    label: &str,
    id: &str,
    cur: &mut &'static str,
    items: &[(&'static str, Icon, &'static str)],
    dirty: &mut bool,
  ) {
    ui.label(label);
    let shown = items.iter().find(|(k, _, _)| k == cur).map(|(_, _, l)| *l).unwrap_or(cur);
    let icon = items.iter().find(|(k, _, _)| k == cur).map(|(_, i, _)| *i);
    let entry = |ui: &mut egui::Ui, icon: Icon, name: &str, on: bool| -> egui::Response {
      ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (r, resp) = ui.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::hover());
        crate::icons::draw(ui.painter(), r, icon, theme::ICON_INK);
        ui.selectable_label(on, name).union(resp)
      })
      .inner
    };
    let mut combo = egui::ComboBox::from_id_salt(id).width(140.0);
    combo = match icon {
      Some(i) => combo.icon(move |ui: &egui::Ui, rect: egui::Rect, _: &egui::style::WidgetVisuals, _: bool| {
        crate::icons::draw(ui.painter(), rect, i, theme::ICON_INK);
      }),
      None => combo,
    };
    combo.selected_text(shown).show_ui(ui, |ui| {
      for (k, i, l) in items {
        if entry(ui, *i, l, *cur == *k).clicked() {
          *cur = k;
          *dirty = true;
        }
      }
    });
    ui.end_row();
  }

  /// A whole-number field with Fusion's spinner (up and down arrows) beside it, re-previewing
  /// when edited.
  fn count_field(ui: &mut egui::Ui, label: &str, v: &mut String, dirty: &mut bool) {
    ui.label(label);
    ui.horizontal(|ui| {
      ui.spacing_mut().item_spacing.x = 2.0;
      if value_edit(ui, v).changed() {
        *dirty = true;
      }
      if let Some(step) = spinner(ui) {
        let n = v.trim().parse::<i64>().unwrap_or(1);
        *v = (n + step).max(1).to_string();
        *dirty = true;
      }
    });
    ui.end_row();
  }

  /// A joint side's choice between the picked geometry and a saved joint origin.
  fn saved_origin(ui: &mut egui::Ui, id: &str, cur: &mut String, saved: &[String], dirty: &mut bool) {
    ui.label("");
    let shown = if cur.is_empty() { "Picked geometry" } else { cur.as_str() };
    egui::ComboBox::from_id_salt(id).selected_text(shown).width(140.0).show_ui(ui, |ui| {
      if ui.selectable_label(cur.is_empty(), "Picked geometry").clicked() {
        cur.clear();
        *dirty = true;
      }
      for n in saved {
        if ui.selectable_label(cur == n, n).clicked() {
          *cur = n.clone();
          *dirty = true;
        }
      }
    });
    ui.end_row();
  }

  fn check(ui: &mut egui::Ui, label: &str, v: &mut bool, dirty: &mut bool) {
    ui.label("");
    if ui.checkbox(v, label).changed() {
      *dirty = true;
    }
    ui.end_row();
  }

  fn rule_combo(&mut self, ui: &mut egui::Ui) {
    ui.label("Rule");
    let shown = if self.rule.is_empty() { "Default".to_string() } else { self.rule.clone() };
    egui::ComboBox::from_id_salt("rule").selected_text(shown).width(140.0).show_ui(ui, |ui| {
      if ui.selectable_label(self.rule.is_empty(), "Default").clicked() {
        self.rule.clear();
        self.dirty = true;
      }
      for r in self.rules.clone() {
        if ui.selectable_label(self.rule == r, &r).clicked() {
          self.rule = r;
          self.dirty = true;
        }
      }
    });
    ui.end_row();
  }

  /// Sketches other than `but`, newest first, for picking a path.
  fn sketches(app: &App, but: Option<FeatureId>) -> Vec<(FeatureId, String)> {
    let d = &app.s.design;
    d.features
      .iter()
      .take(d.active_len())
      .rev()
      .filter(|f| matches!(f.kind, FeatureKind::Sketch { .. }) && Some(f.id) != but)
      .map(|f| (f.id, f.name.clone()))
      .collect()
  }

  fn text(&mut self, ui: &mut egui::Ui, distance: bool) {
    let f = if distance { &mut self.distance } else { &mut self.radius };
    if value_edit(ui, f).changed() {
      self.dirty = true;
    }
  }

  /// Revolve: adds the symmetric flag or the second angle for the chosen direction.
  fn with_revolve_sides(&self, mut p: Value) -> Value {
    match self.extent {
      "symmetric" => p["symmetric"] = json!(true),
      "two_sides" => p["angle2"] = json!(self.angle2),
      _ => {}
    }
    p
  }

  /// The extent or operation dropdown: every value with its icon before the text, as in Fusion.
  fn combo(&mut self, ui: &mut egui::Ui, id: &str, items: &[(&'static str, &'static str)], extent: bool) {
    let mut cur = if extent { self.extent } else { self.op };
    if choice::combo(ui, id, items, &mut cur, choice::extrude_icon) {
      if extent {
        self.extent = cur;
      } else {
        self.op = cur;
      }
      self.dirty = true;
    }
  }

  /// Extrude's distance for the engine: its magnitude and the flip its sign (or the arrow) implies.
  fn extrude_distance(&self) -> (String, bool) {
    let (distance, negative) = extrude::split_sign(&self.distance);
    (distance, self.flip != negative)
  }

  fn axis_combo(&mut self, ui: &mut egui::Ui, id: &str, first: bool, axes: &[u32]) {
    let cur = if id == "dir1" {
      self.dir.clone()
    } else if id == "dir2" {
      self.dir2.clone()
    } else {
      self.axis.clone()
    };
    let _ = first;
    // construction axes can be revolve axes (not pattern directions)
    let mut extra: Vec<String> =
      if id == "dir1" || id == "dir2" { Vec::new() } else { axes.iter().map(|a| format!("axis:{a}")).collect() };
    // Move's Rotate also turns about a straight edge picked in the viewport
    if self.kind == Kind::Move && !self.solid.axis_edge.is_null() {
      extra.push("edge".into());
    }
    egui::ComboBox::from_id_salt(id).selected_text(axis_label(&cur)).width(140.0).show_ui(ui, |ui| {
      for a in ["x", "y", "z"].into_iter().map(str::to_string).chain(extra) {
        if ui.selectable_label(cur == a, axis_label(&a)).clicked() {
          match id {
            "dir1" => self.dir = a,
            "dir2" => self.dir2 = a,
            _ => self.axis = a,
          }
          self.dirty = true;
        }
      }
    });
  }

  fn plane_choice(&mut self, ui: &mut egui::Ui) {
    let cur = self.plane_label.clone();
    egui::ComboBox::from_id_salt("plane").selected_text(cur.clone()).width(140.0).show_ui(ui, |ui| {
      for (l, p) in [("XY plane", "xy"), ("XZ plane", "xz"), ("YZ plane", "yz")] {
        if ui.selectable_label(cur == l, l).clicked() {
          self.plane = json!(p);
          self.plane_label = l.into();
          self.dirty = true;
        }
      }
    });
  }

  fn bodies_label(&self) -> String {
    // nothing picked is nothing to copy, as in Fusion (no "all bodies" default)
    if self.bodies.is_empty() { "Select".into() } else { format!("{} selected", self.bodies.len()) }
  }

  fn sketch_lines(&self, app: &App) -> Vec<(u32, String)> {
    self.sketch_lines_of(self.sketch, app)
  }

  fn sketch_lines_of(&self, sketch: Option<FeatureId>, app: &App) -> Vec<(u32, String)> {
    let Some(sk) = sketch else { return vec![] };
    let Ok(s) = app.s.design.sketch(sk) else { return vec![] };
    s.curves
      .iter()
      .filter(|c| matches!(c.kind, fox_engine::sketch::CurveKind::Line { .. }))
      .map(|c| (c.id.0, format!("Sketch line {}{}", c.id.0, if c.construction { " (construction)" } else { "" })))
      .collect()
  }
}

/// The pointer's prompt while Measure is open, as Fusion's.
pub(crate) const MEASURE_PROMPT: &str = "Select objects to measure.";

/// Measure's length units: name and millimetres per unit.
pub(crate) const MEASURE_UNITS: [(&str, f64); 5] =
  [("mm", 1.0), ("cm", 10.0), ("m", 1000.0), ("in", 25.4), ("ft", 304.8)];

/// How Measure shows its numbers: decimal places, the length unit (the document's) and the
/// Secondary Units every value is also given in (kept between uses, as in Fusion).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasureFmt {
  pub precision: usize,
  /// Index into [`MEASURE_UNITS`]: the document's unit.
  pub unit: usize,
  /// Secondary Units, an index into [`MEASURE_UNITS`]; None (Fusion's default) shows one unit.
  pub secondary: Option<usize>,
}

impl Default for MeasureFmt {
  fn default() -> Self {
    MeasureFmt { precision: 3, unit: 0, secondary: None }
  }
}

/// A Precision choice as Fusion's drop-down shows it: "0", "0.1", "0.12", "0.123", ...
pub(crate) fn precision_label(p: usize) -> String {
  if p == 0 { "0".into() } else { format!("0.{}", &"12345678"[..p.min(8)]) }
}

impl MeasureFmt {
  fn unit(&self) -> (&'static str, f64) {
    MEASURE_UNITS[self.unit.min(MEASURE_UNITS.len() - 1)]
  }

  fn num(&self, v: f64) -> String {
    let p = self.precision.min(8);
    // no "-0.000"
    let s = format!("{v:.p$}");
    if s.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') { s.trim_start_matches('-').into() } else { s }
  }

  /// `v` (in mm to the `power`) in unit `i`, written as Fusion writes it: "0.663 in^2".
  fn in_unit(&self, i: usize, v: f64, power: i32) -> String {
    let (u, k) = MEASURE_UNITS[i.min(MEASURE_UNITS.len() - 1)];
    let suffix = match power {
      2 => "^2",
      3 => "^3",
      _ => "",
    };
    format!("{} {u}{suffix}", self.num(v / k.powi(power)))
  }

  /// In the document's unit, then in the Secondary Units when they're set (and differ).
  fn both(&self, v: f64, power: i32) -> String {
    let main = self.in_unit(self.unit, v, power);
    match self.secondary {
      Some(s) if s != self.unit => format!("{main} ({})", self.in_unit(s, v, power)),
      _ => main,
    }
  }

  pub fn len(&self, mm: f64) -> String {
    self.both(mm, 1)
  }

  pub fn area(&self, mm2: f64) -> String {
    self.both(mm2, 2)
  }

  pub fn volume(&self, mm3: f64) -> String {
    self.both(mm3, 3)
  }

  pub fn angle(&self, deg: f64) -> String {
    format!("{} deg", self.num(deg))
  }
}

/// One line of Measure's results.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Readout {
  pub label: String,
  pub value: String,
}

fn row(label: &str, value: String) -> Readout {
  Readout { label: label.into(), value }
}

/// One collapsible section of Measure's results, as Fusion's: "Selection 1" (and "Selection 2")
/// with a grey line saying what was picked, and, between two picks, "Results".
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeasureSection {
  pub title: String,
  /// The grey line under the title ("Face selected"); empty for Results.
  pub note: String,
  pub rows: Vec<Readout>,
}

/// Measure's sections for a `measure` result: for two picks first Results, only the distance
/// (and the angle between planes or lines), as Fusion's; then each pick's own numbers (a face's
/// area and loop length, an edge's length, a vertex's position, a body's volume).
pub(crate) fn measure_sections(r: &Value, f: &MeasureFmt) -> Vec<MeasureSection> {
  let selection = |n: usize, e: &Value| {
    let (note, rows) = entity_readouts(e, f);
    MeasureSection { title: format!("Selection {n}"), note, rows }
  };
  let Some(d) = r.get("distance").and_then(Value::as_f64) else { return vec![selection(1, r)] };
  let mut rows = vec![row("Distance", f.len(d))];
  if let Some(a) = r.get("angle_deg").and_then(Value::as_f64) {
    rows.push(row("Angle", f.angle(a)));
  }
  let mut out = vec![MeasureSection { title: "Results".into(), note: String::new(), rows }];
  out.extend(["a", "b"].iter().filter_map(|k| r.get(*k)).enumerate().map(|(i, e)| selection(i + 1, e)));
  out
}

fn v3(v: &Value, k: &str) -> Option<[f64; 3]> {
  let a = v.get(k)?.as_array()?;
  Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?])
}

/// One picked entity's grey line ("Face selected") and numbers.
fn entity_readouts(r: &Value, f: &MeasureFmt) -> (String, Vec<Readout>) {
  let num = |k: &str| r.get(k).and_then(Value::as_f64);
  let mut out = Vec::new();
  let radius = |out: &mut Vec<Readout>| {
    if let Some(x) = num("radius") {
      out.push(row("Radius", f.len(x)));
    }
    if let Some(x) = num("diameter") {
      out.push(row("Diameter", f.len(x)));
    }
  };
  let what = match r.get("kind").and_then(Value::as_str) {
    Some("face") => {
      if let Some(a) = num("area_mm2") {
        out.push(row("Area", f.area(a)));
      }
      if let Some(l) = num("loop_length") {
        out.push(row("Loop Length", f.len(l)));
      }
      radius(&mut out);
      "Face"
    }
    Some("edge") => {
      if let Some(l) = num("length") {
        out.push(row("Length", f.len(l)));
      }
      radius(&mut out);
      "Edge"
    }
    Some("point") => {
      if let Some(p) = v3(r, "at") {
        for (i, axis) in ["X Position", "Y Position", "Z Position"].iter().enumerate() {
          out.push(row(axis, f.len(p[i])));
        }
      }
      "Vertex"
    }
    Some("body") => {
      if let Some(v) = num("volume_mm3") {
        out.push(row("Volume", f.volume(v)));
      }
      if let Some(a) = num("area_mm2") {
        out.push(row("Area", f.area(a)));
      }
      let (u, k) = f.unit();
      if let Some(d) = v3(r, "size") {
        out.push(row("Size", format!("{} × {} × {} {u}", f.num(d[0] / k), f.num(d[1] / k), f.num(d[2] / k))));
      }
      if let Some(c) = v3(r, "center_of_mass") {
        out.push(row("Center of Mass", format!("{}, {}, {} {u}", f.num(c[0] / k), f.num(c[1] / k), f.num(c[2] / k))));
      }
      "Body"
    }
    _ => return (String::new(), out),
  };
  (format!("{what} selected"), out)
}

/// A collapsible section's title row in a dialog's grid, as Fusion's: a triangle and the title,
/// open by default; a click folds it. Returns whether it is open.
pub(crate) fn section_header(ui: &mut egui::Ui, id: egui::Id, title: &str) -> bool {
  let open = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(true);
  let galley = ui.painter().layout_no_wrap(title.to_string(), egui::FontId::proportional(12.5), theme::TEXT);
  let (rect, resp) = ui.allocate_exact_size(egui::vec2(galley.size().x + 16.0, 18.0), egui::Sense::click());
  let c = egui::pos2(rect.left() + 5.0, rect.center().y);
  let tri = if open {
    vec![c + egui::vec2(-4.0, -2.0), c + egui::vec2(4.0, -2.0), c + egui::vec2(0.0, 3.0)]
  } else {
    vec![c + egui::vec2(-2.0, -4.0), c + egui::vec2(3.0, 0.0), c + egui::vec2(-2.0, 4.0)]
  };
  ui.painter().add(egui::Shape::convex_polygon(tri, theme::TEXT, egui::Stroke::NONE));
  ui.painter().galley(egui::pos2(rect.left() + 14.0, rect.center().y - galley.size().y / 2.0), galley, theme::TEXT);
  if resp.clicked() {
    ui.ctx().data_mut(|d| d.insert_temp(id, !open));
  }
  ui.end_row();
  open
}

/// What Measure measures: the first two selected bodies, faces, edges and vertices, or (with
/// nothing selected) what the pointer is over.
fn measure_picks(app: &App) -> (Vec<Value>, bool) {
  let picks: Vec<Value> = app
    .s
    .selection
    .iter()
    .filter(|x| matches!(x, Sel::Body { .. } | Sel::Face { .. } | Sel::Edge { .. } | Sel::Vertex { .. }))
    .take(2)
    .filter_map(|x| serde_json::to_value(x).ok())
    .collect();
  if !picks.is_empty() {
    return (picks, false);
  }
  let hover =
    app.hover.map(|p| p.to_sel()).filter(|x| matches!(x, Sel::Face { .. } | Sel::Edge { .. } | Sel::Vertex { .. }));
  (hover.and_then(|h| serde_json::to_value(h).ok()).into_iter().collect(), true)
}

impl Dialog {
  /// Measure's panel, as Fusion's: Selection Filter (three icon toggles), Precision, Secondary
  /// Units, Clear Selection and Show Snap Points, then a collapsible section per pick and, for
  /// two, the Results between them (a value's right-click copies it).
  fn inspect(&mut self, app: &mut App, ui: &mut egui::Ui) {
    use crate::select::Priority;
    use icon_row::{choice, icon_row};
    app.measure = None;
    // Measure picks vertices too, while it's open
    if self.saved_filter.is_none() {
      self.saved_filter = Some((app.sel_priority, app.sel_filter.vertices));
      app.sel_filter.vertices = true;
    }
    ui.label("Selection Filter");
    const FILTERS: [icon_row::Choice; 3] = [
      choice("face", Icon::FilterFaces, "Select Face/Edge/Vertex"),
      choice("body", Icon::FilterBodies, "Select Body"),
      choice("component", Icon::Component, "Select Component"),
    ];
    let mut cur: &'static str = match app.sel_priority {
      Priority::Body => "body",
      Priority::Component => "component",
      Priority::Face | Priority::Edge => "face",
    };
    if icon_row(ui, &mut cur, &FILTERS, "") {
      app.sel_priority = match cur {
        "body" => Priority::Body,
        "component" => Priority::Component,
        _ => Priority::Face,
      };
    }
    ui.end_row();
    // values read in the document's unit
    let doc = app.s.design.units.symbol();
    app.measure_fmt.unit = MEASURE_UNITS.iter().position(|(u, _)| *u == doc).unwrap_or(0);
    let fmt = &mut app.measure_fmt;
    ui.label("Precision");
    egui::ComboBox::from_id_salt("measure-precision").selected_text(precision_label(fmt.precision)).show_ui(ui, |ui| {
      for p in 0..=6 {
        ui.selectable_value(&mut fmt.precision, p, precision_label(p));
      }
    });
    ui.end_row();
    ui.label("Secondary Units");
    let shown = fmt.secondary.map_or("None", |i| MEASURE_UNITS[i.min(MEASURE_UNITS.len() - 1)].0);
    egui::ComboBox::from_id_salt("measure-units").selected_text(shown).show_ui(ui, |ui| {
      ui.selectable_value(&mut fmt.secondary, None, "None");
      for (i, (u, _)) in MEASURE_UNITS.iter().enumerate() {
        ui.selectable_value(&mut fmt.secondary, Some(i), *u);
      }
    });
    ui.end_row();
    ui.label("Clear Selection");
    if crate::icons::icon_button(ui, Icon::Undo, 24.0, "Clear Selection", false, Some(theme::ACCENT)).clicked() {
      app.s.selection.clear();
    }
    ui.end_row();
    ui.label("Show Snap Points");
    let mut off = false;
    ui.add_enabled_ui(false, |ui| ui.checkbox(&mut off, ""))
      .inner
      .on_disabled_hover_text("FissionOxide doesn't show snap points yet");
    ui.end_row();
    let fmt = app.measure_fmt;
    let (picks, hovering) = measure_picks(app);
    if picks.is_empty() {
      return;
    }
    let p = if picks.len() == 2 { json!({"a": picks[0], "b": picks[1]}) } else { json!({"a": picks[0]}) };
    let r = match app.s.execute("measure", &p) {
      Ok(r) => r,
      Err(e) => {
        ui.label("");
        ui.label(RichText::new(e.0).color(theme::ERROR));
        ui.end_row();
        return;
      }
    };
    if let (Some(d), Some(a), Some(b)) = (r.get("distance").and_then(Value::as_f64), r.get("from"), r.get("to")) {
      use fox_engine::kernel::glam::DVec3;
      let v3 = |v: &Value| -> Option<DVec3> {
        let a = v.as_array()?;
        Some(DVec3::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?))
      };
      if let (Some(a), Some(b)) = (v3(a), v3(b)) {
        app.measure = Some((a, b, fmt.len(d)));
      }
    }
    for (i, sec) in measure_sections(&r, &fmt).iter().enumerate() {
      if !section_header(ui, ui.id().with(("measure-section", i)), &sec.title) {
        continue;
      }
      if !sec.note.is_empty() {
        // what the pointer is over reads as a preview until it is picked
        let note = if hovering { sec.note.replace("selected", "under the pointer") } else { sec.note.clone() };
        ui.label(RichText::new(note).color(theme::MUTED));
        ui.end_row();
      }
      for (j, r) in sec.rows.iter().enumerate() {
        ui.label(&r.label);
        let v = ui.push_id((i, j), |ui| ui.add(egui::Label::new(&r.value).sense(egui::Sense::click()))).inner;
        v.context_menu(|ui| {
          if ui.button("Copy").clicked() {
            ui.ctx().copy_text(r.value.clone());
            app.status = Some((format!("Copied {}", r.value), false));
            ui.close();
          }
        });
        ui.end_row();
      }
    }
  }

  /// The palettes' state (tests set the search, tab and favorites).
  #[cfg(test)]
  pub(crate) fn palette_mut(&mut self) -> &mut crate::material_ui::Palette {
    &mut self.palette
  }

  /// Puts back what the dialog changed in the app while open (Measure's selection filter and picks).
  pub(crate) fn leave(&mut self, app: &mut App) {
    // an edit closed some other way keeps what it shows and rolls forward
    self.end_edit(&mut app.s, true);
    if let Kind::Analysis(_) = self.kind {
      // OK kept it in the Analysis folder; Cancel and Escape leave nothing
      app.analysis = None;
    }
    if let Some((priority, vertices)) = self.saved_filter.take() {
      app.sel_priority = priority;
      app.sel_filter.vertices = vertices;
    }
    // Measure's picks are its own: closing it lets them go, as in Fusion
    if self.kind == Kind::Inspect {
      app.s.selection.clear();
      app.measure = None;
    }
    // Section Analysis's live cut goes (OK kept a copy in the browser)
    if self.kind == Kind::Section {
      app.section = None;
    }
  }
}

/// A swatch dragged from the Appearance or Physical Material palette (or the Material Browser)
/// onto the canvas.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SwatchDrag {
  pub name: String,
  pub color: [u8; 3],
  /// Apply to the face under the pointer rather than its body.
  pub faces: bool,
  /// A physical material (applied to the body's component) rather than an appearance.
  pub material: bool,
}

/// What bodies show without an appearance of their own: Fusion's default, Steel - Satin.
const DEFAULT_APPEARANCE: [u8; 3] = [205, 200, 190];

/// The appearances the design uses (name, colour, how many bodies and faces carry it), in the
/// order they are first met. Colours not in the library are named by their hex code.
pub(crate) fn in_this_design(app: &App, lib: &[(String, [u8; 3])]) -> Vec<(String, [u8; 3], usize)> {
  let d = &app.s.design;
  let colors = app
    .s
    .model
    .bodies
    .iter()
    .map(|b| d.body_color(b.id).unwrap_or(DEFAULT_APPEARANCE))
    .chain(d.face_appearances.iter().map(|f| f.color));
  let mut out: Vec<(String, [u8; 3], usize)> = Vec::new();
  for c in colors {
    match out.iter_mut().find(|x| x.1 == c) {
      Some(x) => x.2 += 1,
      None => {
        let name = lib
          .iter()
          .find(|(_, lc)| *lc == c)
          .map(|(n, _)| n.clone())
          .unwrap_or_else(|| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
        out.push((name, c, 1));
      }
    }
  }
  out
}

/// Applies an appearance (a library name, or a "#rrggbb" one) to the bodies of `targets`, or with
/// `faces` to its faces. Returns whether anything was applied.
pub(crate) fn apply_appearance(app: &mut App, name: &str, color: [u8; 3], faces: bool, targets: &[Sel]) -> bool {
  let mut p = if name.starts_with('#') { json!({"color": color}) } else { json!({"appearance": name}) };
  if faces {
    let list: Vec<Value> = targets
      .iter()
      .filter_map(|t| if let Sel::Face { body, face } = t { Some(json!({"body": body.0, "face": face})) } else { None })
      .collect();
    if list.is_empty() {
      return false;
    }
    p["faces"] = json!(list);
    return app.run("face.appearance", p).is_some();
  }
  let mut bodies: Vec<u32> = Vec::new();
  for t in targets {
    let b = match t {
      Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } | Sel::Vertex { body, .. } => body.0,
      _ => continue,
    };
    if !bodies.contains(&b) {
      bodies.push(b);
    }
  }
  if bodies.is_empty() {
    return false;
  }
  p["bodies"] = json!(bodies);
  app.run("body.appearance", p).is_some()
}

/// `s` with its first letter capitalised, as a cursor prompt is written.
fn sentence(s: &str) -> String {
  let mut c = s.chars();
  match c.next() {
    Some(f) => f.to_uppercase().chain(c).collect(),
    None => String::new(),
  }
}

/// A stored axis as the dialogs' axis choice names it ("x", "line:3", "axis:7").
fn axis_key(axis: &AxisRef) -> String {
  match axis {
    AxisRef::X => "x".into(),
    AxisRef::Y => "y".into(),
    AxisRef::Z => "z".into(),
    AxisRef::SketchLine { curve } | AxisRef::SketchCurve { curve, .. } => format!("line:{}", curve.0),
    AxisRef::Edge { .. } => "edge".into(),
    AxisRef::Construction { feature } => format!("axis:{}", feature.0),
  }
}

fn axis_label(a: &str) -> String {
  match a {
    "x" => "X axis".into(),
    "y" => "Y axis".into(),
    "z" => "Z axis".into(),
    "edge" => "Picked edge".into(),
    other => other.replace("line:", "Sketch line ").replace("axis:", "Axis "),
  }
}

fn axis_vec(a: &str) -> [f64; 3] {
  match a {
    "y" => [0.0, 1.0, 0.0],
    "z" => [0.0, 0.0, 1.0],
    _ => [1.0, 0.0, 0.0],
  }
}

fn op_str(o: Operation) -> &'static str {
  match o {
    Operation::NewBody => "new_body",
    Operation::Join => "join",
    Operation::Cut => "cut",
    Operation::Intersect => "intersect",
    Operation::NewComponent => "new_component",
  }
}

fn is_planar(s: &Session, body: BodyId, face: u32) -> bool {
  s.model
    .body(body)
    .and_then(|b| b.faces().iter().find(|f| f.id == face).map(|f| f.kind == fox_engine::kernel::SurfaceKind::Plane))
    .unwrap_or(false)
}

fn last_unconsumed_sketch(s: &Session) -> Option<FeatureId> {
  let active = s.design.active_len();
  let feats = &s.design.features[..active];
  feats
    .iter()
    .rev()
    .find(|f| {
      matches!(f.kind, FeatureKind::Sketch { .. })
        && !feats.iter().any(|g| match &g.kind {
          FeatureKind::Extrude { sketch, .. } => *sketch == Some(f.id),
          FeatureKind::Revolve { sketch, .. } => *sketch == f.id,
          _ => false,
        })
    })
    .map(|f| f.id)
}

fn sel_edges(s: &Session) -> Vec<Value> {
  s.selection
    .iter()
    .filter_map(|x| if let Sel::Edge { body, edge } = x { Some(json!({"body": body.0, "edge": edge})) } else { None })
    .collect()
}

fn sel_faces(s: &Session) -> Vec<Value> {
  s.selection
    .iter()
    .filter_map(|x| if let Sel::Face { body, face } = x { Some(json!({"body": body.0, "face": face})) } else { None })
    .collect()
}

/// Bodies of the selected bodies, faces and edges.
fn sel_bodies(s: &Session) -> Vec<u32> {
  let mut b: Vec<u32> = Vec::new();
  for x in &s.selection {
    if let Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } = x
      && !b.contains(&body.0)
    {
      b.push(body.0);
    }
  }
  b
}

fn sel_only_bodies(s: &Session) -> Vec<u32> {
  s.selection.iter().filter_map(|x| if let Sel::Body { body } = x { Some(body.0) } else { None }).collect()
}

fn sel_curves(s: &Session) -> Vec<u32> {
  s.selection.iter().filter_map(|x| if let Sel::SketchCurve { curve, .. } = x { Some(curve.0) } else { None }).collect()
}

/// Scale's point when a body is picked first: the body's centre, as Fusion fills the Point row
/// on the first pick.
fn body_centre(s: &Session, body: u32) -> Option<[f64; 3]> {
  manip::centre(s, &[body]).map(|c| [c.x, c.y, c.z].map(|v| (v * 1e4).round() / 1e4))
}

/// The command Press Pull (Q) runs for what is picked, as in Fusion: Extrude on a sketch
/// profile, Fillet on an edge (with no face picked), else Offset Face.
fn press_pull_kind(s: &Session) -> Kind {
  if s.selection.iter().any(|x| matches!(x, Sel::Profile { .. })) {
    Kind::Extrude
  } else if s.selection.iter().any(|x| matches!(x, Sel::Edge { .. }))
    && !s.selection.iter().any(|x| matches!(x, Sel::Face { .. }))
  {
    Kind::Fillet
  } else {
    Kind::PressPull
  }
}

impl Dialog {
  /// Extend with Tangent Chain: the picked edges and the edges that continue them smoothly.
  fn extend_chain(&mut self, f: &crate::forms::Form, s: &Session) {
    let at = |k: &str| f.inputs.iter().position(|x| x.key() == k);
    let (Some(ei), Some(ci)) = (at("edges"), at("chain")) else { return };
    if f.cmd != crate::forms::EXTEND_CMD || self.form.values[ci] != json!(true) {
      return;
    }
    let edges = self.form.values[ei].as_array().cloned().unwrap_or_default();
    self.form.values[ei] = json!(with_tangent_chain(s, &edges));
  }
}

/// `edges` (`{body, edge}`) with the edges that continue each of them smoothly, as Fusion's
/// Tangent Chain picks them.
fn with_tangent_chain(s: &Session, edges: &[Value]) -> Vec<Value> {
  let mut out = edges.to_vec();
  let mut i = 0;
  while i < out.len() {
    let get = |k: &str| out[i].get(k).and_then(Value::as_u64).map(|v| v as u32);
    if let (Some(b), Some(e)) = (get("body"), get("edge"))
      && let Some(body) = s.model.body(BodyId(b))
    {
      for n in fox_engine::tangent_neighbours(body.edges(), e) {
        let v = json!({"body": b, "edge": n});
        if !out.contains(&v) {
          out.push(v);
        }
      }
    }
    i += 1;
  }
  out
}

/// The two ends of a sketch curve (none for a circle or a closed spline).
fn curve_ends(c: &fox_engine::sketch::Curve) -> Vec<fox_engine::sketch::PointId> {
  use fox_engine::sketch::CurveKind as K;
  match &c.kind {
    K::Line { a, b } => vec![*a, *b],
    K::Arc { start, end, .. } => vec![*start, *end],
    K::Spline { points, .. } if points.len() >= 2 && points.first() != points.last() => {
      vec![points[0], points[points.len() - 1]]
    }
    _ => Vec::new(),
  }
}

/// `curves` with the curves of `sketch` joined to them end to end, as Fusion's Chain Selection
/// picks them: a chain runs on through a point where exactly two curves meet.
fn with_curve_chain(sketch: &fox_engine::sketch::Sketch, curves: &[u32]) -> Vec<u32> {
  use fox_engine::sketch::CurveId;
  let mut out = curves.to_vec();
  let mut i = 0;
  while i < out.len() {
    let Some(c) = sketch.curves.iter().find(|c| c.id == CurveId(out[i])) else {
      i += 1;
      continue;
    };
    for end in curve_ends(c) {
      let at: Vec<&fox_engine::sketch::Curve> =
        sketch.curves.iter().filter(|x| !x.construction && curve_ends(x).contains(&end)).collect();
      if let [a, b] = at.as_slice() {
        let other = if a.id == c.id { b } else { a };
        if !out.contains(&other.id.0) {
          out.push(other.id.0);
        }
      }
    }
    i += 1;
  }
  out
}

/// `expr` with its sign flipped: "2 mm" ↔ "-2 mm", anything else wrapped as "-(…)".
fn negated(expr: &str) -> String {
  let t = expr.trim();
  if let Some(rest) = t.strip_prefix('-') {
    let rest = rest.trim();
    return match rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
      Some(inner) => inner.to_string(),
      None => rest.to_string(),
    };
  }
  let simple = t.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == ' ');
  if simple { format!("-{t}") } else { format!("-({t})") }
}

/// Loft sections from the selected profiles, one per sketch, in pick order.
fn sel_sections(s: &Session) -> Vec<(u32, [f64; 2])> {
  let mut out: Vec<(u32, [f64; 2])> = Vec::new();
  for x in &s.selection {
    if let Sel::Profile { sketch, index } = x
      && let Some(p) = s.model.sketches.get(sketch).and_then(|r| r.profiles.get(*index))
      && !out.iter().any(|(sk, _)| *sk == sketch.0)
    {
      out.push((sketch.0, p.interior_point()));
    }
  }
  out
}

/// Joints in the timeline: id and name.
/// Names of the saved joint origins.
fn saved_origins(s: &Session) -> Vec<String> {
  s.design
    .features
    .iter()
    .filter(|f| matches!(f.kind, FeatureKind::JointOrigin { .. }))
    .map(|f| f.name.clone())
    .collect()
}

fn joints(s: &Session) -> Vec<(u32, String)> {
  s.design
    .features
    .iter()
    .filter(|f| matches!(f.kind, FeatureKind::Joint { .. }))
    .map(|f| (f.id.0, f.name.clone()))
    .collect()
}

/// The views of a drawing: id and a label like "2 · Top".
pub(crate) fn drawing_views(s: &Session, drawing: u32) -> Vec<(u32, String)> {
  use fox_engine::model::drawing::ViewKind as V;
  let Some(d) = s.design.drawing(drawing) else { return Vec::new() };
  d.views
    .iter()
    .map(|v| {
      let kind = match &v.kind {
        V::Section { .. } => "Section".to_string(),
        V::Flat { .. } => "Flat pattern".to_string(),
        V::Detail { .. } => "Detail".to_string(),
        V::Iso => "Isometric".to_string(),
        k => format!("{k:?}"),
      };
      (v.id, format!("{} · {kind}", v.id))
    })
    .collect()
}

/// "auto" → null, "1:2" stays text, "0.5" → a number.
fn scale_value(t: &str) -> Value {
  let t = t.trim();
  if t.is_empty() || t.eq_ignore_ascii_case("auto") {
    Value::Null
  } else {
    t.parse::<f64>().map(|v| json!(v)).unwrap_or_else(|_| json!(t))
  }
}

/// Start (or end) point of a picked edge `{body, edge}`.
fn edge_point(s: &Session, e: &Value, start: bool) -> Option<[f64; 3]> {
  let b = s.model.body(BodyId(e.get("body")?.as_u64()? as u32))?;
  let id = e.get("edge")?.as_u64()? as u32;
  let e = b.edges().iter().find(|x| x.id == id)?;
  let p = if start { e.start } else { e.end };
  Some([p.x, p.y, p.z])
}

/// Joint origins: the selected faces and edges, in pick order.
fn sel_origins(s: &Session) -> Vec<(u32, Value)> {
  s.selection.iter().filter_map(|x| origin_of(s, x)).collect()
}

/// A picked face, edge or point as a joint origin, with its body.
fn origin_of(s: &Session, x: &Sel) -> Option<(u32, Value)> {
  match x {
    Sel::Face { body, face } => Some((body.0, json!({"face": {"body": body.0, "face": face}}))),
    Sel::Edge { body, edge } => Some((body.0, json!({"edge": {"body": body.0, "edge": edge}}))),
    Sel::Vertex { body, edge, end } => {
      fox_engine::scene::vertex_pos(s, *body, *edge, *end).map(|p| (body.0, json!({"at": p.to_array()})))
    }
    _ => None,
  }
}

/// As-built Joint's picks: faces, edges and points as joint origins, and whole bodies (null)
/// that only name their component.
fn sel_as_built(s: &Session) -> Vec<(u32, Value)> {
  s.selection
    .iter()
    .filter_map(|x| match x {
      Sel::Body { body } => Some((body.0, Value::Null)),
      x => origin_of(s, x),
    })
    .collect()
}

/// The drafting standard picked in the drawing dialogs (null leaves the drawing's as it is).
fn drawing_standard(choice: &str) -> Value {
  if matches!(choice, "asme" | "iso") { json!(choice) } else { Value::Null }
}

/// The mirror line among the picked curves: the last picked line.
fn mirror_line(s: &Session, curves: &[u32]) -> Option<u32> {
  let sk = s.design.sketch(s.edit_sketch?).ok()?;
  curves.iter().rev().copied().find(|c| {
    sk.curve(fox_engine::sketch::CurveId(*c))
      .is_some_and(|c| matches!(c.kind, fox_engine::sketch::CurveKind::Line { .. }))
  })
}

/// Typing aimed at the open dialog (digits typed with no box focused), held for its first value
/// box.
const TYPING: &str = "dialog-typing";

/// Sends typing to the open dialog's primary value box (see [`value_edit`]).
pub fn type_into(ctx: &egui::Context, text: String) {
  ctx.data_mut(|d| d.insert_temp(egui::Id::new(TYPING), text));
}

/// Has a length field left 0.00? A value naming a user parameter counts as set; a half-typed one
/// (".", "-") doesn't.
fn length_set(e: &str) -> bool {
  match fox_engine::expr::eval(e, fox_engine::expr::Kind::Length, &fox_engine::expr::Empty) {
    Ok(v) => v.abs() >= 1e-9,
    Err(_) => !e.trim().trim_matches(['.', '-', '+', ' ']).is_empty(),
  }
}

/// Takes the typing aimed at the open dialog, if any (the floating value box draws before the
/// dialog, so it gets first call).
pub(crate) fn take_typing(ctx: &egui::Context) -> Option<String> {
  ctx.data_mut(|d| d.remove_temp::<String>(egui::Id::new(TYPING)))
}

impl Dialog {
  /// The field the floating value box beside the drag arrow edits (see [`crate::value_box`]):
  /// the distance of Extrude, Press Pull, Offset Plane and Sketch Offset, the radius or distance
  /// of Fillet and Chamfer, Sketch Scale's factor. None for dialogs without an arrow.
  pub fn value_box_field(&mut self) -> Option<&mut String> {
    match self.kind {
      Kind::Extrude | Kind::OffsetPlane | Kind::PressPull | Kind::OffsetFace | Kind::SkOffset | Kind::Section => {
        Some(&mut self.distance)
      }
      Kind::Fillet | Kind::Chamfer => Some(&mut self.radius),
      Kind::SkScale => Some(&mut self.a),
      Kind::Scale if self.choice != "non_uniform" => Some(&mut self.a),
      // an edge flange's Height, in the box on its arrow
      Kind::Flange if self.choice == "edge" => Some(&mut self.distance),
      _ => None,
    }
  }

  /// The floating value box changed its field: re-preview.
  pub fn value_box_edited(&mut self) {
    self.dirty = true;
    self.live_reset();
    if self.kind == Kind::SkOffset {
      // a typed distance holds: the offset stops following the cursor
      self.follow = false;
    }
  }
}

/// A dialog value box. The first one drawn takes typing aimed at the dialog: its text is replaced
/// and it gets the keyboard, like Fusion's floating value box next to the manipulator.
fn value_edit(ui: &mut egui::Ui, text: &mut String) -> egui::Response {
  value_edit_w(ui, text, 140.0)
}

/// [alue_edit] at a given width.
/// Fusion's spinner beside a whole-number box: up and down arrows, one above the other. Returns
/// +1 or -1 for the arrow clicked.
pub(crate) fn spinner(ui: &mut egui::Ui) -> Option<i64> {
  let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 20.0), egui::Sense::hover());
  let mut out = None;
  for (k, step) in [(0, 1), (1, -1)] {
    let r = egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 10.0 * k as f32), egui::vec2(12.0, 10.0));
    let resp = ui.interact(r, ui.id().with(("spin", k)), egui::Sense::click());
    let col = if resp.hovered() { theme::ACCENT } else { theme::GROUP_LABEL };
    let c = r.center();
    let pts = if step > 0 {
      vec![c + egui::vec2(-3.5, 2.0), c + egui::vec2(3.5, 2.0), c + egui::vec2(0.0, -2.0)]
    } else {
      vec![c + egui::vec2(-3.5, -2.0), c + egui::vec2(3.5, -2.0), c + egui::vec2(0.0, 2.0)]
    };
    ui.painter().add(egui::Shape::convex_polygon(pts, col, egui::Stroke::NONE));
    if resp.clicked() {
      out = Some(step);
    }
  }
  out
}

pub(super) fn value_edit_w(ui: &mut egui::Ui, text: &mut String, width: f32) -> egui::Response {
  let mut r = ui.add(egui::TextEdit::singleline(text).desired_width(width));
  if let Some(typed) = ui.data_mut(|d| d.remove_temp::<String>(egui::Id::new(TYPING))) {
    // what the box held tells whether it takes a length (see [`crate::units::pin`])
    crate::units::note(ui.ctx(), r.id, text);
    *text = typed;
    crate::viewport::focus_text(ui.ctx(), &r, text, false);
    r.mark_changed();
  }
  crate::units::pin(ui, &mut r, text);
  r
}

/// The OK button: the primary button, greyed while the dialog lacks its input (see
/// [`Dialog::ok_enabled`]).
pub(crate) fn ok_button(ui: &mut egui::Ui, accent: egui::Color32, enabled: bool) -> egui::Response {
  if enabled {
    return panels::primary_button(ui, "OK", accent);
  }
  ui.add_enabled(
    false,
    egui::Button::new(RichText::new("OK").color(theme::MUTED))
      .fill(egui::Color32::from_gray(240))
      .stroke(egui::Stroke::new(1.0, theme::BORDER))
      .corner_radius(3)
      .min_size(egui::vec2(72.0, 24.0)),
  )
}

/// A selection chip's text: "Select" until something is picked, then the count.
pub(crate) fn chip_text(n: usize, what: &str) -> String {
  if n == 0 { "Select".to_string() } else { format!("{n} {what}") }
}

/// A selection input: a white "Select" chip until something is picked, then a chip tinted with
/// the command's accent showing the count, with a ✕ beside it.
pub(crate) fn sel_chip(ui: &mut egui::Ui, accent: egui::Color32, text: &str, has: bool) -> egui::Response {
  sel_chip_lit(ui, accent, text, has, has)
}

/// A selection chip, lit (the accent's tint) when `lit`, with the clear x after it when `has`.
fn sel_chip_lit(ui: &mut egui::Ui, accent: egui::Color32, text: &str, lit: bool, has: bool) -> egui::Response {
  let font = egui::FontId::proportional(12.0);
  let tw = ui.painter().layout_no_wrap(text.to_string(), font.clone(), theme::TEXT).size().x;
  let (rect, resp) = ui.allocate_exact_size(egui::vec2(tw + 52.0, 22.0), egui::Sense::hover());
  let chip = egui::Rect::from_min_size(rect.min, egui::vec2(tw + 32.0, 22.0));
  let p = ui.painter();
  let ir = egui::Rect::from_center_size(chip.left_center() + egui::vec2(12.0, 0.0), egui::Vec2::splat(15.0));
  let fill = if lit { crate::icons::tint_of(accent, 0.84) } else { egui::Color32::WHITE };
  let stroke = if lit { accent } else { theme::BORDER };
  p.rect(chip, 3.0, fill, egui::Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
  crate::icons::draw(p, ir, Icon::Select, accent);
  p.text(chip.left_center() + egui::vec2(24.0, 0.0), egui::Align2::LEFT_CENTER, text, font, theme::TEXT);
  if has {
    let x = egui::pos2(chip.right() + 10.0, chip.center().y);
    let s = egui::Stroke::new(1.4, theme::GROUP_LABEL);
    p.line_segment([x + egui::vec2(-4.0, -4.0), x + egui::vec2(4.0, 4.0)], s);
    p.line_segment([x + egui::vec2(4.0, -4.0), x + egui::vec2(-4.0, 4.0)], s);
  }
  resp
}

/// Offset's dimension, added when the command commits as Fusion does: the offset distance from
/// the start of an offset line to the line it was offset from, drawn between the two and driving
/// the offset's distance. `sources` are the curves offset, `made` the curves the offset produced
/// and `distance` the dialog's offset (its sign is the side).
pub(crate) fn offset_dimension(app: &mut App, sources: &[u32], made: &[u32], distance: &str) {
  use fox_engine::sketch::{CurveId, CurveKind};
  let Some(sk) = app.s.edit_sketch else { return };
  let Ok(sketch) = app.s.design.sketch(sk) else { return };
  let line = |c: u32| match sketch.curve(CurveId(c)).map(|c| &c.kind) {
    Some(CurveKind::Line { a, b }) => Some((sketch.pos(*a)?, sketch.pos(*b)?, *a)),
    _ => None,
  };
  // the first source line, and the offset line parallel to it at the offset's distance
  let Some((src, (p0, p1, _))) = sources.iter().find_map(|c| Some((*c, line(*c)?))) else { return };
  let (dx, dy) = (p1[0] - p0[0], p1[1] - p0[1]);
  let len = dx.hypot(dy);
  if len < 1e-9 {
    return;
  }
  let dist = |q: [f64; 2]| ((q[0] - p0[0]) * dy - (q[1] - p0[1]) * dx).abs() / len;
  let text = distance.trim().trim_start_matches('-').trim();
  let typed = fox_engine::expr::eval(text, fox_engine::expr::Kind::Length, &app.s.model.params).ok();
  let Some((point, measured)) = made.iter().filter_map(|c| line(*c)).find_map(|(qa, qb, a)| {
    let (da, db) = (dist(qa), dist(qb));
    (da > 1e-9 && (da - db).abs() < 1e-6).then_some((a, da))
  }) else {
    return;
  };
  // the typed distance labels the dimension when it is what was measured (the engine read it the
  // same way), else the measurement
  let value = match typed {
    Some(t) if (t.abs() - measured).abs() < 1e-6 => text.to_string(),
    _ => app.s.design.units.format(measured),
  };
  app.run(
    "sketch.dimension",
    json!({"sketch": sk.0, "type": "point_line", "points": [point.0], "curves": [src], "value": value}),
  );
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The cursor prompt follows the command's step: select, then specify, and how to change the
  /// selection while the pointer is over something selectable.
  #[test]
  fn the_cursor_prompt_follows_the_dialogs_step() {
    let mut d = Dialog::blank(Kind::Extrude);
    assert_eq!(d.prompt(false).as_deref(), Some("Select sketch profiles or planar faces to extrude"));
    assert_eq!(d.prompt(true).as_deref(), Some("Select sketch profiles or planar faces to extrude"));
    d.sketch = Some(FeatureId(1));
    d.profiles.push([0.0, 0.0]);
    assert_eq!(d.prompt(false).as_deref(), Some("Specify distance and taper angle"));
    assert_eq!(d.prompt(true).as_deref(), Some("Hold Ctrl to modify selection"));

    let mut d = Dialog::blank(Kind::Fillet);
    assert_eq!(d.prompt(false).as_deref(), Some("Select edges, faces, or features to fillet"));
    d.edges.push(json!({"body": 1, "edge": 1}));
    assert_eq!(d.prompt(false).as_deref(), Some("Specify fillet radius"));

    assert_eq!(Dialog::blank(Kind::PickPlane).prompt(false).as_deref(), Some(PICK_PLANE_HINT));
    // a command without its own wording asks for its missing input as a sentence
    assert_eq!(Dialog::blank(Kind::Hem).prompt(false).as_deref(), Some("Select an edge to hem"));
    // one with nothing to select or specify has no prompt
    assert_eq!(Dialog::blank(Kind::Bom).prompt(false), None);
    // Center of Mass asks for its pick, as Fusion's does
    assert_eq!(Dialog::blank(Kind::CenterOfMass).prompt(false).as_deref(), Some("Select bodies and/or components"));
    assert_eq!(Dialog::blank(Kind::Inspect).prompt(false).as_deref(), Some("Select objects to measure."));
  }

  /// E opens Extrude with no profile picked, so nothing is previewed and no arrow shows, when
  /// the sketch has more than one closed region; a picked profile then makes it ready.
  #[test]
  fn extrude_opens_with_nothing_picked_on_a_sketch_of_several_regions() {
    let mut app = App::with_session(Session::default());
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let sk = app.s.edit_sketch.unwrap();
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]})).unwrap();
    app.run("sketch.rect", json!({"corner1": [20, 0], "corner2": [30, 10]})).unwrap();
    app.run("sketch.finish", json!({})).unwrap();
    app.s.selection.clear();
    let mut d = Dialog::open(&mut app, Kind::Extrude);
    assert_eq!(d.sketch, Some(sk));
    assert!(d.profiles.is_empty() && !d.ready(), "{:?}", d.profiles);
    assert_eq!(d.prompt(false).as_deref(), Some("Select sketch profiles or planar faces to extrude"));
    assert!(d.manipulator(&app.s).is_none(), "no arrow before a pick");
    let bodies = app.s.model.bodies.len();
    d.preview(&mut app.s);
    assert!(!d.applied && app.s.model.bodies.len() == bodies, "nothing is previewed");
    // picking a profile makes it ready and previews just that region
    app.s.selection = vec![Sel::Profile { sketch: sk, index: 0 }];
    d.sync_selection(&app.s);
    assert_eq!(d.profiles.len(), 1);
    assert!(d.ready());
    d.preview(&mut app.s);
    assert!(d.applied, "{:?}", d.error);
    assert_eq!(app.s.model.bodies.len(), bodies + 1);
    assert!(d.manipulator(&app.s).is_some());
    d.cancel(&mut app.s);
  }

  /// `ui:manip:zero-default-value`: Fillet and Shell start at 0.00 after the first pick, as in
  /// Fusion: nothing previewed, no complaint and OK waiting until a value is typed (an edited
  /// fillet keeps its radius); in inches the box reads "0 in".
  #[test]
  fn fillet_and_shell_start_at_zero() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    let body = app.s.model.bodies[0].id;
    let top = app.s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    let volume = |app: &App| app.s.model.bodies[0].shape.mass().unwrap().volume;
    let before = volume(&app);
    for (kind, sel) in [(Kind::Fillet, Sel::Edge { body, edge: 1 }), (Kind::Shell, Sel::Face { body, face: top })] {
      app.s.selection = vec![sel];
      let mut d = Dialog::open(&mut app, kind);
      assert_eq!(d.radius, "0 mm", "{kind:?}");
      assert!(d.ready() && !d.ok_enabled(), "{kind:?}: OK waits for a value");
      d.preview(&mut app.s);
      assert!(!d.applied && d.error.is_none(), "{kind:?}: nothing previewed at 0, nothing wrong");
      assert!((volume(&app) - before).abs() < 1e-9);
      d.radius = ".".into();
      assert!(!d.ok_enabled(), "{kind:?}: half typed");
      d.radius = "1 mm".into();
      assert!(d.ok_enabled());
      d.preview(&mut app.s);
      assert!(d.applied && d.error.is_none(), "{kind:?}: {:?}", d.error);
      assert!(volume(&app) < before);
      d.cancel(&mut app.s);
    }
    app.s.design.units = fox_engine::model::LengthUnit::In;
    app.s.selection = vec![Sel::Edge { body, edge: 1 }];
    assert_eq!(Dialog::open(&mut app, Kind::Fillet).radius, "0 in");
  }

  /// A Fillet preview keeps the body's own colour: only the picked edge draws in the selection
  /// blue, as Fusion's command picks look.
  #[test]
  fn a_fillet_preview_keeps_the_body_colour() {
    use fox_engine::scene::{self, Pick, SceneBuilder, ViewOptions};
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    let body = app.s.model.bodies[0].id;
    app.s.selection = vec![Sel::Edge { body, edge: 1 }];
    let mut d = Dialog::open(&mut app, Kind::Fillet);
    d.radius = "2 mm".into();
    d.preview(&mut app.s);
    assert!(d.applied, "{:?}", d.error);
    let (sc, picks) = SceneBuilder::default().build(&app.s, &ViewOptions::default());
    let colors: Vec<[u8; 4]> = sc
      .meshes
      .iter()
      .filter(|m| matches!(m.tri_ids.first().and_then(|id| picks.get(*id)), Some(Pick::Face { .. })))
      .map(|m| m.color)
      .collect();
    assert!(!colors.is_empty() && colors.iter().all(|c| *c == scene::BODY_COLOR), "{colors:?}");
    for l in sc.lines.iter().filter(|l| matches!(picks.get(l.id), Some(Pick::Edge { .. }))) {
      assert!(
        [scene::EDGE_COLOR, scene::EDGE_SELECT_COLOR, scene::EDGE_SELECT_HALO].contains(&l.color),
        "{:?}",
        l.color
      );
    }
    d.cancel(&mut app.s);
  }

  #[test]
  fn offset_commits_with_a_dimension_to_its_source() {
    use fox_engine::sketch::ConstraintKind;
    let mut app = App::with_session(Session::default());
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let sk = app.s.edit_sketch.unwrap();
    let line = app.run("sketch.line", json!({"sketch": sk.0, "points": [[0, 0], [30, 0]]})).unwrap()["curves"][0]
      .as_u64()
      .unwrap() as u32;
    let made = app.run("sketch.offset", json!({"sketch": sk.0, "curves": [line], "distance": "-5"})).unwrap();
    let made: Vec<u32> = made["curves"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
    offset_dimension(&mut app, &[line], &made, "-5");
    let s = app.s.design.sketch(sk).unwrap();
    let dim = s.constraints.iter().find_map(|c| match &c.kind {
      ConstraintKind::PointLineDistance { line: l, value, .. } => Some((*l, value.clone())),
      _ => None,
    });
    let (src, value) = dim.expect("an offset dimension");
    assert_eq!(src.0, line, "measured to the source line");
    assert_eq!((value.expr.as_str(), value.value), ("5", 5.0), "the typed distance, as a distance");
    // nothing to measure to (a circle's offset) adds nothing
    let circle =
      app.run("sketch.circle", json!({"sketch": sk.0, "center": [50, 50], "radius": 4})).unwrap()["curves"][0].clone();
    let made = app.run("sketch.offset", json!({"sketch": sk.0, "curves": [circle], "distance": "2"})).unwrap();
    let made: Vec<u32> = made["curves"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
    let before = app.s.design.sketch(sk).unwrap().constraints.len();
    offset_dimension(&mut app, &[circle.as_u64().unwrap() as u32], &made, "2");
    assert_eq!(app.s.design.sketch(sk).unwrap().constraints.len(), before);
  }

  #[test]
  fn typing_goes_to_the_first_value_box() {
    let ctx = egui::Context::default();
    let (mut distance, mut other) = (String::from("10 mm"), String::from("2 mm"));
    let mut changed = false;
    type_into(&ctx, "25".into());
    let raw = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
      ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| {
      changed = value_edit(ui, &mut distance).changed();
      value_edit(ui, &mut other);
    });
    out.textures_delta.clear();
    assert_eq!((distance.as_str(), other.as_str()), ("25", "2 mm"));
    assert!(changed && ctx.egui_wants_keyboard_input());
  }

  fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0))
  }

  /// Fusion's sketch Mirror and patterns: Objects and a second pick row (Mirror Line, Center
  /// Point, Direction/s), picks routed by the active row, OK (ready) only once both are set.
  #[test]
  fn sketch_mirror_and_patterns_route_picks_by_the_active_row() {
    use fox_engine::sketch::{CurveId, PointId};
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    let sk = s.edit_sketch.unwrap();
    let line = run(&mut s, "sketch.line", json!({"points": [[0, -20], [0, 20]]}))["curves"][0].as_u64().unwrap() as u32;
    let slant = run(&mut s, "sketch.line", json!({"points": [[0, 0], [30, 30]]}))["curves"][0].as_u64().unwrap() as u32;
    let circle =
      run(&mut s, "sketch.circle", json!({"center": [20, 40], "radius": 3}))["curves"][0].as_u64().unwrap() as u32;
    run(&mut s, "sketch.point", json!({"at": [50, 50]}));
    let pt = s.design.sketch(sk).unwrap().points.iter().find(|p| (p.x - 50.0).abs() < 1e-9).unwrap().id.0;
    let curve = |c: u32| Sel::SketchCurve { sketch: sk, curve: CurveId(c) };

    // Mirror: the circle picked as an object moves the active row on to the mirror line with
    // the prompt (Fusion), so the next line clicked is the line and OK comes live
    let mut d = Dialog::blank(Kind::SkMirror);
    assert!(!d.ready() && d.prompt(false).as_deref() == Some("Select sketch objects to mirror"));
    assert_eq!(d.pick_row, 0);
    s.selection = vec![curve(circle)];
    d.sync_selection(&s);
    assert_eq!((d.curves.clone(), d.line), (vec![circle], None));
    assert!(!d.ready() && d.prompt(false).as_deref() == Some("Select the mirror line"));
    assert_eq!(d.pick_row, 1, "the Mirror Line row is active after the objects pick");
    s.selection = vec![curve(circle), curve(line)];
    d.sync_selection(&s);
    assert_eq!((d.curves.clone(), d.line), (vec![circle], Some(line)));
    assert!(d.ready());
    assert_eq!(d.command().unwrap().0, "sketch.mirror");
    // with the Objects row made active again, a further pick (a line too) is another object
    d.pick_row = 0;
    s.selection = vec![curve(circle), curve(line), curve(slant)];
    d.sync_selection(&s);
    assert_eq!((d.curves.clone(), d.line), (vec![circle, slant], Some(line)));
    assert!(d.preview_curves().is_empty(), "nothing previewed yet");
    apply(&mut s, &mut d);
    // the mirrored copies preview as grey outlines (the curves the command made) until OK
    assert!(!d.made.is_empty() && d.preview_curves() == d.made.as_slice());
    d.withdraw(&mut s);

    // Circular pattern: a sketch point as the centre; the rows wait for both picks
    let mut d = Dialog::blank(Kind::SkCircPattern);
    d.count = "6".into();
    s.selection = vec![curve(circle)];
    d.sync_selection(&s);
    assert!(!d.ready() && d.center.is_none());
    d.pick_row = 1;
    s.selection = vec![curve(circle), Sel::SketchPoint { sketch: sk, point: PointId(pt) }];
    d.sync_selection(&s);
    assert!(d.center.is_some() && d.at == ["50".to_string(), "50".to_string()] && d.ready());
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, &p["center"], &p["count"]), ("sketch.pattern_circ", &json!([50.0, 50.0]), &json!(6)));
    apply(&mut s, &mut d);
    d.withdraw(&mut s);
    // a circle picked for the centre gives its centre and isn't patterned
    let mut d = Dialog::blank(Kind::SkCircPattern);
    d.pick_row = 1;
    s.selection = vec![curve(circle)];
    d.sync_selection(&s);
    assert!(d.curves.is_empty() && d.at == ["20".to_string(), "40".to_string()]);

    // Rectangular pattern: the picked lines are the directions, both ways by default; OK waits
    // for a direction line (Fusion)
    let mut d = Dialog::blank(Kind::SkRectPattern);
    d.count = "3".into();
    d.count2 = "3".into();
    d.spacing2 = "20 mm".into();
    d.use_dir2 = true;
    s.selection = vec![curve(circle)];
    d.sync_selection(&s);
    assert!(!d.ready() && d.dir_lines.is_empty());
    assert_eq!(d.prompt(false).as_deref(), Some("Select a line for the direction"));
    let (_, p) = d.command().unwrap();
    assert_eq!((&p["direction"], &p["direction2"]), (&json!([1.0, 0.0]), &json!([0.0, 1.0])));
    d.pick_row = 1;
    s.selection = vec![curve(circle), curve(slant)];
    d.sync_selection(&s);
    assert_eq!(d.dir_lines.len(), 1);
    assert!(d.ready(), "a direction line makes OK live");
    let (_, p) = d.command().unwrap();
    let r = 0.5f64.sqrt();
    let near = |v: &Value, want: [f64; 2]| {
      let a: Vec<f64> = v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
      (a[0] - want[0]).abs() < 1e-9 && (a[1] - want[1]).abs() < 1e-9
    };
    // a lone first direction gets a second at right angles
    assert!(near(&p["direction"], [r, r]) && near(&p["direction2"], [-r, r]), "{p}");
    assert_eq!(d.curves, vec![circle], "the direction line isn't an object");
    s.selection = vec![curve(circle), curve(slant), curve(line)];
    d.sync_selection(&s);
    let (_, p) = d.command().unwrap();
    assert!(near(&p["direction2"], [0.0, 1.0]), "{p}");
    apply(&mut s, &mut d);
  }

  /// Fusion's Text frame is placed by two clicks: the first corner moves the prompt on to the
  /// second (a rubber-band frame follows the pointer from it), the second places the frame, the
  /// dialog grows with "Sample Text" in the frame, the Text box takes the keyboard and the prompt
  /// asks to adjust the formatting or rotate the frame.
  #[test]
  fn text_frame_is_placed_by_two_clicks() {
    use crate::forms::SK_TEXT;
    let mut d = Dialog::blank(Kind::Form(&SK_TEXT));
    crate::forms::prefill(&SK_TEXT, &mut d.form, &Session::default());
    assert!(d.wants_text_frame() && d.text_frame_start().is_none() && d.text_frame().is_none());
    assert_eq!(d.prompt(false).as_deref(), Some("Place first corner of text frame"));
    d.place_text_corner([10.0, 5.0], 0.0);
    assert!(d.wants_text_frame() && !d.ready());
    assert_eq!(d.text_frame_start(), Some([10.0, 5.0]));
    assert_eq!(d.prompt(false).as_deref(), Some("Place second corner of text frame"));
    // a second corner in the first's column makes no frame: still waiting
    d.place_text_corner([10.0, 30.0], 0.0);
    assert!(d.wants_text_frame() && d.text_frame_start() == Some([10.0, 5.0]));
    d.place_text_corner([50.0, 15.0], 0.0);
    assert!(!d.wants_text_frame() && d.ready() && d.text_frame_start().is_none());
    assert_eq!(d.text_frame(), Some(([10.0, 5.0], [50.0, 15.0], 0.0)));
    assert_eq!(d.prompt(false).as_deref(), Some("Adjust text formatting or rotate text frame"));
    assert!(d.take_text_focus(), "the Text box takes the keyboard once the frame is placed");
    assert!(!d.take_text_focus(), "only once");
    let (cmd, p) = d.command().unwrap();
    // Fusion's sample text, 10 mm (0.394 in) tall, in its default font, the height an expression
    assert_eq!((cmd, &p["text"], &p["height"]), ("sketch.text", &json!("Sample Text"), &json!("10 mm")));
    assert_eq!(p["font"], json!(crate::forms::default_font()));
    assert_eq!(p["at"], json!([10.0, 15.0]));
    // the rotate handle turns the text: a dial at the frame's first corner
    let mut app = App::with_session(Session::default());
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let h = d.handles(&app.s);
    assert_eq!(h.len(), 1);
    assert!(matches!(h[0].shape, crate::handles::Shape::Dial { radius: Some(r), .. } if (r - 40.0).abs() < 1e-9));
    d.set_handle(&app.s, 0, 30f64.to_radians(), 1f64.to_radians());
    assert_eq!(d.text_frame().map(|f| f.2), Some(30.0));
    assert_eq!(d.command().unwrap().1["angle"], json!(30.0));
    // the rows Fusion shows once the frame is placed, in its order
    let shown: Vec<&str> = SK_TEXT
      .inputs
      .iter()
      .enumerate()
      .filter(|(i, _)| d.form.shown(&SK_TEXT, *i) && !crate::forms::State::hidden(&SK_TEXT, *i))
      .map(|(_, x)| x.key())
      .collect();
    assert_eq!(
      shown,
      vec!["type", "text", "font", "bold", "italic", "height", "spacing", "flip_h", "flip_v", "halign", "valign"]
    );
  }

  /// Fusion's sketch patterns open with three instances: the circular one distributed "Full"
  /// (its Quantity box has a spinner), the rectangular one over an extent of 0 in two
  /// directions, with nothing previewed and OK off until a direction line is picked.
  #[test]
  fn sketch_patterns_open_with_fusions_defaults() {
    use fox_engine::sketch::CurveId;
    let mut app = App::with_session(Session::default());
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let circle =
      app.run("sketch.circle", json!({"center": [20, 0], "radius": 3})).unwrap()["curves"][0].as_u64().unwrap() as u32;
    let line =
      app.run("sketch.line", json!({"points": [[0, 10], [40, 10]]})).unwrap()["curves"][0].as_u64().unwrap() as u32;
    let sk = app.s.edit_sketch.unwrap();
    let curve = |c: u32| Sel::SketchCurve { sketch: sk, curve: CurveId(c) };
    app.start(Kind::SkCircPattern);
    let mut d = app.dialog.take().unwrap();
    assert_eq!((d.count.as_str(), d.choice, d.angle.as_str()), ("3", "full", "360 deg"));
    assert_eq!(d.prompt(false).as_deref(), Some("Select sketch objects to pattern"));
    d.curves = vec![circle];
    let origin = app.s.design.sketch(sk).unwrap().points.iter().find(|p| p.x == 0.0 && p.y == 0.0).unwrap().id;
    d.center = Some(Sel::SketchPoint { sketch: sk, point: origin });
    d.at = ["0".into(), "0".into()];
    let (_, p) = d.command().unwrap();
    assert_eq!((&p["count"], &p["angle"], &p["symmetric"]), (&json!(3), &json!("360 deg"), &json!(false)));
    // Angle and Symmetric distributions send the typed angle
    d.choice = "symmetric";
    d.angle = "90 deg".into();
    let (_, p) = d.command().unwrap();
    assert_eq!((&p["angle"], &p["symmetric"]), (&json!("90 deg"), &json!(true)));
    d.choice = "full";
    // the quantity dot at the next instance round the centre, with the quantity's floating box
    let h = d.handles(&app.s);
    assert_eq!(h.len(), 1);
    assert!(matches!(h[0].head, crate::handles::Head::Dot));
    assert_eq!(d.handle_field(0).map(|f| f.clone()), Some("3".to_string()));
    d.set_handle(&app.s, 0, std::f64::consts::TAU / 6.0, 0.01);
    assert_eq!(d.count, "6");
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none() && d.preview_curves().len() == 5, "{:?}", d.error);
    d.withdraw(&mut app.s);

    app.start(Kind::SkRectPattern);
    let mut d = app.dialog.take().unwrap();
    assert_eq!((d.count.as_str(), d.count2.as_str(), d.spacing.as_str()), ("3", "3", "0 mm"));
    assert!(d.pattern_extent && d.use_dir2 && d.spacing2 == "0 mm");
    app.s.selection = vec![curve(circle)];
    d.sync_selection(&app.s);
    assert!(!d.ready(), "OK waits for a direction line");
    assert!(d.command().is_none(), "nothing previews at a distance of 0");
    assert!(d.handles(&app.s).is_empty(), "no floating boxes before a direction is picked");
    d.pick_row = 1;
    app.s.selection = vec![curve(circle), curve(line)];
    d.sync_selection(&app.s);
    assert!(d.ready() && d.dir_lines.len() == 1);
    assert!(d.command().is_none(), "still nothing at a distance of 0");
    // the distance and quantity boxes along the direction, one per handle
    let h = d.handles(&app.s);
    assert_eq!(h.len(), 4);
    assert!(d.boxes_every_handle());
    assert_eq!(d.handle_field(0).map(|f| f.clone()), Some("0 mm".to_string()));
    assert_eq!(d.handle_field(1).map(|f| f.clone()), Some("3".to_string()));
    d.spacing = "30 mm".into();
    let (_, p) = d.command().unwrap();
    assert_eq!((&p["distance_type"], &p["count"]), (&json!("extent"), &json!(3)));
    assert!(p.get("direction2").is_none(), "a second distance of 0 patterns one way");
    d.spacing2 = "20 mm".into();
    let (_, p) = d.command().unwrap();
    assert_eq!(p["count2"], json!(3));
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none() && d.preview_curves().len() == 8, "{:?}", d.error);
  }

  /// Fusion's Text opens with the Type row alone and the prompt to place the frame; the placed
  /// frame fills the hidden corners, grows the dialog and places the text in the frame, turned to
  /// read left to right on screen.
  #[test]
  fn text_waits_for_its_frame_then_grows() {
    use crate::forms::{Input, SK_TEXT};
    let mut d = Dialog::blank(Kind::Form(&SK_TEXT));
    assert!(!d.ready() && d.wants_text_frame());
    assert_eq!(d.prompt(false).as_deref(), Some("Place first corner of text frame"));
    let shown: Vec<&str> = SK_TEXT
      .inputs
      .iter()
      .enumerate()
      .filter(|(i, _)| d.form.shown(&SK_TEXT, *i) && !crate::forms::State::hidden(&SK_TEXT, *i))
      .map(|(_, x)| x.key())
      .collect();
    assert_eq!(shown, vec!["type"]);
    assert!(matches!(SK_TEXT.inputs[0], Input::Icons { key: "type", .. }));
    d.set_text_frame([10.0, 5.0], [50.0, 15.0], 0.0);
    assert!(d.ready() && !d.wants_text_frame());
    let (cmd, p) = d.command().unwrap();
    // left and top of the frame, the frame's corners not sent
    assert_eq!((cmd, &p["at"], &p["valign"]), ("sketch.text", &json!([10.0, 15.0]), &json!("top")));
    assert!(p.get("x2").is_none() && p.get("type").is_none(), "{p}");
    let shown: Vec<&str> = SK_TEXT
      .inputs
      .iter()
      .enumerate()
      .filter(|(i, _)| d.form.shown(&SK_TEXT, *i) && !crate::forms::State::hidden(&SK_TEXT, *i))
      .map(|(_, x)| x.key())
      .collect();
    assert!(shown.contains(&"text") && shown.contains(&"font") && shown.contains(&"height"));
    assert!(!shown.contains(&"path") && !shown.contains(&"x2"));
    // a text reading at 90° (along +y) on a vertical plane still sits in the frame as dragged:
    // it starts at the frame's low y and its top (towards -x) is the frame's low x
    let mut d = Dialog::blank(Kind::Form(&SK_TEXT));
    d.set_text_frame([0.0, 0.0], [10.0, -40.0], 90.0);
    let (_, p) = d.command().unwrap();
    let at: Vec<f64> = p["at"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    assert!(at[0].abs() < 1e-9 && (at[1] + 40.0).abs() < 1e-9 && p["angle"] == json!(90.0), "{p}");
    // text on a path: the path pick is what's waited for
    let mut d = Dialog::blank(Kind::Form(&SK_TEXT));
    d.form.values[0] = json!("path");
    assert_eq!(d.prompt(false).as_deref(), Some("Select path"));
    let mut app = App::with_session(Session::default());
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let sk = app.s.edit_sketch.unwrap();
    let arc = app.run("sketch.arc", json!({"start": [0, 0], "mid": [20, 10], "end": [40, 0]})).unwrap()["curves"][0]
      .as_u64()
      .unwrap() as u32;
    app.s.selection = vec![Sel::SketchCurve { sketch: sk, curve: fox_engine::sketch::CurveId(arc) }];
    d.sync_selection(&app.s);
    assert!(d.ready());
    let (_, p) = d.command().unwrap();
    assert_eq!((&p["path"], p.get("at")), (&json!(arc), None));
    apply(&mut app.s, &mut d);
  }

  /// Fusion's PROJECT: Geometry, a Selection Filter (bodies, or faces and edges) and Projection
  /// Link, OK only once something is picked.
  #[test]
  fn project_has_a_selection_filter_and_a_projection_link() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 20, "depth": 20, "height": 10})).unwrap();
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let d = Dialog::open(&mut app, Kind::Project);
    assert!((d.choice, d.flag) == ("specified", true) && !d.ready());
    let mut d = d;
    d.bodies = vec![app.s.model.bodies[0].id.0];
    assert!(d.ready());
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, &p["associative"]), ("sketch.project", &json!(true)));
    // the form dialogs' Selection Filter decides what the Geometry chip counts
    let mut d = Dialog::blank(Kind::Form(&crate::forms::SK_INTERSECT));
    assert!(!d.form.filter_faces(&crate::forms::SK_INTERSECT));
    d.form.values[2] = json!("faces");
    assert!(d.form.filter_faces(&crate::forms::SK_INTERSECT));
  }

  /// PROJECT's Selection Filter: Specified Entities (faces and edges) first and lit, then
  /// Bodies, which picks whole bodies until the dialog closes; the prompt follows the filter.
  #[test]
  fn project_filter_starts_on_specified_entities_and_bodies_pick_bodies() {
    use crate::select::Priority;
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 20, "depth": 20, "height": 10})).unwrap();
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let body = app.s.model.bodies[0].id;
    let mut d = Dialog::open(&mut app, Kind::Project);
    assert_eq!((d.choice, d.missing_input()), ("specified", "select geometry to project"));
    assert_eq!(chip_text(d.bodies.len().min(1), "body"), "Select");
    let mut f = d.choice;
    d.filter_row_apply(&mut app, &mut f);
    assert_eq!(app.sel_priority, Priority::Face, "specified entities pick faces and edges");
    let mut f = "bodies";
    d.filter_row_apply(&mut app, &mut f);
    d.choice = f;
    assert_eq!((app.sel_priority, d.missing_input()), (Priority::Body, "select a body"));
    // a body pick fills the Geometry chip with the body
    app.s.selection = vec![Sel::Body { body }];
    d.sync_selection(&app.s);
    assert!(d.bodies == [body.0] && d.faces.is_empty(), "{:?} {:?}", d.bodies, d.faces);
    d.leave(&mut app);
    assert_eq!(app.sel_priority, Priority::Face, "the filter is put back on close");
  }

  /// Previews the dialog's command and checks it applied without an error.
  fn apply(s: &mut Session, d: &mut Dialog) {
    assert!(d.ready(), "{:?} not ready", d.kind);
    d.preview(s);
    assert!(d.applied && d.error.is_none(), "{:?}: {:?} {:?}", d.kind, d.error, d.command());
  }

  fn face(s: &Session, body: usize, pick: impl Fn(&fox_engine::kernel::FaceInfo) -> bool) -> Value {
    let b = &s.model.bodies[body];
    json!({"body": b.id.0, "face": b.faces().iter().find(|f| pick(f)).unwrap().id})
  }

  fn edge(s: &Session, body: usize, pick: impl Fn(&fox_engine::kernel::EdgeInfo) -> bool) -> Value {
    let b = &s.model.bodies[body];
    json!({"body": b.id.0, "edge": b.edges().iter().find(|e| pick(e)).unwrap().id})
  }

  fn interior(s: &Session, sk: u32) -> [f64; 2] {
    s.model.sketches[&FeatureId(sk)].profiles[0].interior_point()
  }

  #[test]
  fn loft_sweep_and_hole_dialogs() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"center": [0, 0], "corner": [20, 20]}));
    let a = s.edit_sketch.unwrap().0;
    run(&mut s, "plane.offset", json!({"plane": "xy", "offset": 30}));
    let plane = s.design.features.last().unwrap().id.0;
    run(&mut s, "sketch.create", json!({"plane": {"feature": plane}}));
    run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 10}));
    let b = s.edit_sketch.unwrap().0;
    run(&mut s, "sketch.finish", json!({}));
    let mut d = Dialog::blank(Kind::Loft);
    d.sections = vec![(a, interior(&s, a)), (b, interior(&s, b))];
    apply(&mut s, &mut d);
    assert_eq!(s.model.bodies.len(), 1);

    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xz"}));
    run(&mut s, "sketch.line", json!({"points": [[0, 0], [0, 30]]}));
    let path = s.edit_sketch.unwrap();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.circle", json!({"center": [0, 0], "radius": 3}));
    let mut d = Dialog::blank(Kind::Sweep);
    d.sketch = s.edit_sketch;
    d.profiles = vec![s.model.sketches[&d.sketch.unwrap()].profiles[0].interior_point()];
    d.path = Some(path);
    apply(&mut s, &mut d);
    assert_eq!(s.model.bodies.len(), 1);

    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 60, "depth": 40, "height": 10}));
    let top = face(&s, 0, |f| f.normal.z > 0.9);
    run(&mut s, "sketch.create", json!({"plane": top}));
    run(&mut s, "sketch.point", json!({"at": [10, 5]}));
    let before = s.model.bodies[0].shape.mass().unwrap().volume;
    let mut d = Dialog::blank(Kind::Hole);
    d.sketch = s.edit_sketch;
    run(&mut s, "sketch.finish", json!({}));
    d.choice = "counterbore";
    d.a = "5 mm".into();
    d.b = "6 mm".into();
    d.c = "9 mm".into();
    d.d = "2 mm".into();
    apply(&mut s, &mut d);
    assert!(s.model.bodies[0].shape.mass().unwrap().volume < before);
  }

  #[test]
  fn modify_dialogs() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 40, "depth": 40, "height": 20}));
    let mut d = Dialog::blank(Kind::PressPull);
    d.faces = vec![face(&s, 0, |f| f.normal.z > 0.9)];
    d.distance = "5 mm".into();
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::Draft);
    let b = &s.model.bodies[0];
    d.faces =
      b.faces().iter().filter(|f| f.normal.z.abs() < 0.1).map(|f| json!({"body": b.id.0, "face": f.id})).collect();
    d.angle = "3 deg".into();
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::SplitBody);
    d.bodies = vec![s.model.bodies[0].id.0];
    d.plane = json!("xz");
    apply(&mut s, &mut d);
    assert_eq!(s.model.bodies.len(), 2);
  }

  #[test]
  fn sheet_metal_dialogs() {
    let mut s = Session::default();
    let mut d = Dialog::blank(Kind::SheetRules);
    d.load_rule(&s, "");
    d.name = "Alu".into();
    d.a = "1.5 mm".into();
    apply(&mut s, &mut d);
    d.applied = false;
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 40]}));
    let mut d = Dialog::blank(Kind::Flange);
    d.choice = "base";
    d.sketch = s.edit_sketch;
    d.rule = "Alu".into();
    run(&mut s, "sketch.finish", json!({}));
    // the profile picked (Fusion's Flange picks nothing itself)
    d.profiles = vec![s.model.sketches[&d.sketch.unwrap()].profiles[0].interior_point()];
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::Flange);
    d.choice = "edge";
    d.edges = vec![edge(&s, 0, |e| e.length > 79.0 && e.mid.y > 39.0 && e.mid.z > 1.0)];
    d.distance = "15 mm".into();
    d.angle = "90 deg".into();
    d.position = "adjacent";
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::Hem);
    d.edges = vec![edge(&s, 0, |e| e.length > 79.0 && e.mid.y < 1.0 && e.mid.z > 1.0)];
    d.choice = "open";
    d.distance = "5 mm".into();
    d.distance2 = "0.5 mm".into();
    apply(&mut s, &mut d);
    d.applied = false;
    let bottom = face(&s, 0, |f| f.normal.z < -0.9 && f.kind == fox_engine::kernel::SurfaceKind::Plane);
    let mut d = Dialog::blank(Kind::FlatPattern);
    assert!(!d.ready());
    d.faces = vec![bottom.clone()];
    apply(&mut s, &mut d);
    assert!(s.design.flat_pattern.is_some());
    let mut d = Dialog::blank(Kind::Unfold);
    d.bodies = vec![s.model.bodies[0].id.0];
    d.faces = vec![bottom];
    assert!(!d.ready(), "Unfold all bends unticked");
    d.flat.all_bends = true;
    apply(&mut s, &mut d);

    // fold a flat sheet along a sketch line
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 40]}));
    run(&mut s, "sheet.flange", json!({}));
    let top = face(&s, 0, |f| f.normal.z > 0.9);
    run(&mut s, "sketch.create", json!({"plane": top}));
    let line = run(&mut s, "sketch.line", json!({"points": [[40, -5], [40, 45]]}));
    let mut d = Dialog::blank(Kind::Fold);
    d.faces = vec![top];
    d.sketch = s.edit_sketch;
    d.line = line["curves"].as_array().and_then(|a| a.first()).and_then(Value::as_u64).map(|v| v as u32);
    d.angle = "90 deg".into();
    run(&mut s, "sketch.finish", json!({}));
    apply(&mut s, &mut d);

    // convert a thin box, then rip a shelled box
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 40, "depth": 30, "height": 2}));
    let mut d = Dialog::blank(Kind::ConvertSheet);
    d.bodies = vec![s.model.bodies[0].id.0];
    d.faces = vec![face(&s, 0, |f| f.normal.z < -0.9)];
    apply(&mut s, &mut d);
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 40, "depth": 30, "height": 20}));
    let top = face(&s, 0, |f| f.normal.z > 0.9);
    run(&mut s, "shell", json!({"faces": [top], "thickness": 1}));
    let mut d = Dialog::blank(Kind::Rip);
    d.edges = vec![edge(&s, 0, |e| e.length > 18.0 && e.length < 20.5 && e.mid.z > 5.0)];
    d.distance = "0.5 mm".into();
    apply(&mut s, &mut d);
  }

  #[test]
  fn assembly_dialogs() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 100, "depth": 60, "height": 10, "at": [50, 30]}));
    run(&mut s, "box", json!({"width": 80, "depth": 20, "height": 5, "at": [40, 200]}));
    let (base, arm) = (s.model.bodies[0].id.0, s.model.bodies[1].id.0);
    for (b, n) in [(base, "Base"), (arm, "Arm")] {
      let mut d = Dialog::blank(Kind::NewComponent);
      d.bodies = vec![b];
      d.name = n.into();
      apply(&mut s, &mut d);
    }
    let comp = |s: &Session, b: u32| s.design.component_of(BodyId(b)).map(|c| c.id.0);
    let arm_comp = comp(&s, arm);
    run(&mut s, "component.material", json!({"component": arm_comp, "material": "Aluminum"}));
    let mut d = Dialog::blank(Kind::Joint);
    d.choice = "rigid";
    d.origins = vec![
      (arm, json!({"face": face(&s, 1, |f| f.normal.z < -0.9)})),
      (base, json!({"face": face(&s, 0, |f| f.normal.z > 0.9)})),
    ];
    d.comps = vec![comp(&s, arm), comp(&s, base)];
    apply(&mut s, &mut d);
    let mut d = Dialog::blank(Kind::Bom);
    d.refresh_report(&mut s);
    assert_eq!(d.report.as_ref().and_then(Value::as_array).map(Vec::len), Some(2));
    let mut d = Dialog::blank(Kind::Interference);
    d.refresh_report(&mut s);
    assert!(d.report.is_some(), "{:?}", d.error);
    let clashes = d.report.as_ref().and_then(Value::as_array).map_or(0, Vec::len);
    let n = s.model.bodies.len();
    d.create_interference(&mut s);
    assert!(d.error.is_none(), "{:?}", d.error);
    assert_eq!(s.model.bodies.len(), n + clashes);
  }

  #[test]
  fn new_component_options_and_properties() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 10, "height": 10}));
    let body = s.model.bodies[0].id.0;
    let mut d = Dialog::blank(Kind::NewComponent);
    d.name = "Frame".into();
    d.choice = "bodies";
    d.bodies = vec![body];
    apply(&mut s, &mut d);
    let frame = s.design.component_of(BodyId(body)).unwrap().id.0;
    // an empty sheet metal child of Frame, with a part number and description
    let mut d = Dialog::blank(Kind::NewComponent);
    d.for_component(&s, frame);
    assert_eq!(d.choice, "empty");
    d.bodies = vec![body];
    d.name = "Bracket".into();
    d.position = "sheet_metal";
    d.d = "BR-01".into();
    d.e = "Mount".into();
    apply(&mut s, &mut d);
    let c = s.design.components.last().unwrap();
    assert_eq!((c.name.as_str(), c.part_number.as_str(), c.description.as_str()), ("Bracket", "BR-01", "Mount"));
    assert!(c.sheet_metal && c.bodies.is_empty() && c.parent.map(|p| p.0) == Some(frame));
    let bracket = c.id.0;
    // Properties loads and edits them
    let mut d = Dialog::blank(Kind::ComponentProps);
    assert!(!d.ready());
    d.for_component(&s, bracket);
    assert_eq!((d.d.as_str(), d.position), ("BR-01", "sheet_metal"));
    d.d = "BR-02".into();
    d.position = "standard";
    apply(&mut s, &mut d);
    let c = s.design.components.last().unwrap();
    assert!(c.part_number == "BR-02" && !c.sheet_metal);
  }

  #[test]
  fn as_built_joint_dialog() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 100, "depth": 60, "height": 10, "at": [50, 30]}));
    run(&mut s, "box", json!({"width": 40, "depth": 10, "height": 5, "at": [20, 100]}));
    let (base, arm) = (s.model.bodies[0].id.0, s.model.bodies[1].id.0);
    for b in [base, arm] {
      run(&mut s, "component.create", json!({"bodies": [b]}));
    }
    let comp = |s: &Session, b: u32| s.design.component_of(BodyId(b)).map(|c| c.id.0);
    let before = s.model.bodies[1].shape.mass().unwrap().bbox_min;
    let mut d = Dialog::blank(Kind::AsBuiltJoint);
    d.choice = "revolute";
    // the arm (whole body), the base, then the position: an edge of the arm
    d.origins = vec![(arm, Value::Null), (base, Value::Null)];
    d.comps = vec![comp(&s, arm), comp(&s, base)];
    assert!(!d.ready(), "a position is needed");
    d.origins.push((arm, json!({"edge": edge(&s, 1, |e| e.length > 9.0 && e.length < 11.0)})));
    d.comps.push(comp(&s, arm));
    let (moving, fixed, _) = d.as_built_picks().unwrap();
    assert_eq!((Some(moving), fixed), (comp(&s, arm), comp(&s, base)));
    apply(&mut s, &mut d);
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, &p["as_built"], &p["kind"]), ("joint", &json!(true), &json!("revolute")));
    // the arm stays where it sits
    assert!((s.model.bodies[1].shape.mass().unwrap().bbox_min - before).length() < 1e-6);
  }

  #[test]
  fn sketch_modify_dialogs() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20, "at": [100, 0]}));
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    let r = run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [40, 20]}));
    let lines: Vec<u32> = r["curves"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
    let mut d = Dialog::blank(Kind::SkFillet);
    d.curves = lines[..2].to_vec();
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::SkChamfer2);
    d.curves = lines[2..4].to_vec();
    d.distance = "2 mm".into();
    d.distance2 = "3 mm".into();
    apply(&mut s, &mut d);
    d.applied = false;
    let c = run(&mut s, "sketch.circle", json!({"center": [20, 40], "radius": 3}));
    let circle = c["curves"][0].as_u64().unwrap() as u32;
    let mut d = Dialog::blank(Kind::SkOffset);
    d.curves = vec![circle];
    d.distance = "1 mm".into();
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::SkMirror);
    d.curves = vec![circle];
    d.line = Some(lines[0]);
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::SkCircPattern);
    d.curves = vec![circle];
    d.count = "4".into();
    // the centre is a pick (here the sketch origin point)
    let sk = s.edit_sketch.unwrap();
    let origin = s.design.sketch(sk).unwrap().points.iter().find(|p| p.x == 0.0 && p.y == 0.0).map(|p| p.id);
    d.center = Some(Sel::SketchPoint { sketch: sk, point: origin.unwrap_or(fox_engine::sketch::PointId(0)) });
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::SkRectPattern);
    d.curves = vec![circle];
    d.count = "2".into();
    d.use_dir2 = true;
    d.pattern_extent = true;
    d.symmetric = [true, false];
    d.dir_lines = vec![(lines[0], [1.0, 0.0])];
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::Project);
    d.bodies = vec![s.model.bodies[0].id.0];
    d.flag = true;
    apply(&mut s, &mut d);
    run(&mut s, "sketch.spline", json!({"points": [[0, -10], [10, -5], [20, -10]]}));
    run(&mut s, "sketch.trim", json!({"at": [30, 0]}));
  }

  #[test]
  fn remaining_engine_commands() {
    // split by body
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 40, "depth": 40, "height": 10}));
    run(&mut s, "cylinder", json!({"radius": 5, "height": 30, "at": [20, 20]}));
    let (plate, pin) = (s.model.bodies[0].id.0, s.model.bodies[1].id.0);
    let mut d = Dialog::blank(Kind::SplitBody);
    d.bodies = vec![plate];
    d.target = Some(pin);
    apply(&mut s, &mut d);

    // components: duplicate, joint, drive, motion study
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 100, "depth": 60, "height": 10, "at": [50, 30]}));
    run(&mut s, "box", json!({"width": 80, "depth": 20, "height": 5, "at": [40, 200]}));
    let (base, arm) = (s.model.bodies[0].id.0, s.model.bodies[1].id.0);
    let b = run(&mut s, "component.create", json!({"bodies": [base]}))["component"].as_u64().unwrap() as u32;
    let a = run(&mut s, "component.create", json!({"bodies": [arm]}))["component"].as_u64().unwrap() as u32;
    let mut d = Dialog::blank(Kind::CopyComponent);
    d.target = Some(b);
    d.translate = ["0".into(), "-100 mm".into(), "0".into()];
    apply(&mut s, &mut d);
    d.applied = false;
    let j = run(
      &mut s,
      "joint",
      json!({"moving": a, "moving_origin": {"at": [10, 200, 0]}, "fixed": b, "fixed_origin": {"at": [50, 30, 10]}, "kind": "revolute"}),
    )["feature"]
      .as_u64()
      .unwrap() as u32;
    let mut d = Dialog::blank(Kind::DriveJoint);
    d.set_target(&s, j);
    d.angle = "30 deg".into();
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::MotionStudy);
    d.target = Some(j);
    d.position = "rotation";
    d.a = "0 deg".into();
    d.b = "90 deg".into();
    d.count = "3".into();
    d.refresh_report(&mut s);
    assert_eq!(d.report.as_ref().and_then(Value::as_array).map(Vec::len), Some(4), "{:?}", d.error);

    // drawings
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 40, "depth": 30, "height": 20}));
    let mut d = Dialog::blank(Kind::NewDrawing);
    d.name = "Part".into();
    d.position = "a4";
    d.choice = "first_angle";
    d.a = "1:2".into();
    apply(&mut s, &mut d);
    let id = s.design.drawings.last().unwrap().id;
    let views = drawing_views(&s, id);
    assert_eq!(views.len(), 4);
    let mut d = Dialog::blank(Kind::DrawingSettings);
    d.target = Some(id);
    d.load_drawing(&s);
    assert_eq!((d.position, d.choice, d.a.as_str()), ("a4", "first_angle", "1:2"));
    d.a = "auto".into();
    apply(&mut s, &mut d);
    let mut d = Dialog::blank(Kind::DrawingView);
    d.target = Some(id);
    d.choice = "section";
    d.axis = "x".into();
    d.distance = "0".into();
    d.at = [String::new(), String::new()];
    apply(&mut s, &mut d);
    let mut d = Dialog::blank(Kind::DrawingView);
    d.target = Some(id);
    d.choice = "detail";
    d.view = Some(views[0].0);
    d.xyz = ["0".into(), "0".into(), "0".into()];
    d.radius = "8".into();
    d.count = "2".into();
    d.at = ["60".into(), "60".into()];
    apply(&mut s, &mut d);
    let mut d = Dialog::blank(Kind::DrawingMoveView);
    d.target = Some(id);
    d.view = Some(views[1].0);
    d.at = ["100".into(), "80".into()];
    apply(&mut s, &mut d);
    let mut d = Dialog::blank(Kind::DrawingDimension);
    d.target = Some(id);
    d.view = Some(views[0].0);
    d.choice = "aligned";
    d.distance = "8".into();
    d.translate = ["-20".into(), "-15".into(), "0".into()];
    d.xyz = ["20".into(), "-15".into(), "0".into()];
    apply(&mut s, &mut d);
    let mut d = Dialog::blank(Kind::DrawingNote);
    d.target = Some(id);
    d.name = "BREAK SHARP EDGES".into();
    d.at = ["20".into(), "20".into()];
    apply(&mut s, &mut d);
    let mut d = Dialog::blank(Kind::DrawingTitle);
    d.target = Some(id);
    d.name = "Bracket".into();
    apply(&mut s, &mut d);
    let dr = s.design.drawing(id).unwrap();
    assert_eq!((dr.views.len(), dr.dimensions.len(), dr.title.title.as_str()), (6, 2, "Bracket"));
    let plot = fox_engine::model::drawing::render(dr, &s.design, &s.model).unwrap();
    assert!(plot.texts().contains(&"BREAK SHARP EDGES"));
  }

  /// A form dialog with the selection set, values overridden by key.
  fn form(s: &mut Session, f: &'static crate::forms::Form, sel: Vec<Sel>, set: &[(&str, Value)]) -> Dialog {
    let mut d = Dialog::blank(Kind::Form(f));
    s.selection = sel;
    d.form.sync(f, s, None);
    for (k, v) in set {
      let i = f.inputs.iter().position(|x| x.key() == *k).unwrap_or_else(|| panic!("{} has no {k}", f.name));
      d.form.values[i] = v.clone();
    }
    d
  }

  #[test]
  fn form_dialogs() {
    use crate::forms::*;
    let mut s = Session::default();
    run(&mut s, "cylinder", json!({"radius": 5, "height": 20}));
    let b = s.model.bodies[0].id;
    let side =
      s.model.bodies[0].faces().iter().find(|f| f.kind == fox_engine::kernel::SurfaceKind::Cylinder).unwrap().id;
    let mut d = form(&mut s, &THREAD, vec![Sel::Face { body: b, face: side }], &[("modeled", json!(true))]);
    let v0 = s.model.bodies[0].shape.mass().unwrap().volume;
    apply(&mut s, &mut d);
    assert!(s.model.bodies[0].shape.mass().unwrap().volume < v0);
    d.applied = false;

    let mut s = Session::default();
    let mut d = form(&mut s, &COIL, vec![], &[]);
    apply(&mut s, &mut d);
    assert_eq!(s.model.bodies.len(), 1);
    d.applied = false;
    let b = s.model.bodies[0].id;
    let mut d = form(&mut s, &APPEARANCE, vec![Sel::Body { body: b }], &[("appearance", json!("Copper"))]);
    apply(&mut s, &mut d);
    assert_eq!(s.design.body_settings(b).unwrap().color, Some([190, 112, 72]));
    d.applied = false;

    // construction geometry from picks and drop-downs
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 10}));
    let b = s.model.bodies[0].id;
    let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    // Midplane between the top face and the XY plane picked in the browser
    let mut d = form(&mut s, &MIDPLANE, vec![Sel::Face { body: b, face: top }], &[]);
    assert!(!d.ready(), "Plane 2 is still to pick");
    d.form.pick_origin(json!("xy"));
    d.form.sync(&MIDPLANE, &s, None);
    apply(&mut s, &mut d);
    assert_eq!(s.model.planes.len(), 1);
    assert!((s.model.planes.values().next().unwrap().origin.z - 5.0).abs() < 1e-6);
    d.applied = false;
    let mut d = form(&mut s, &AXIS_TWO_PLANES, vec![], &[]);
    d.form.pick_origin(json!("xz"));
    d.form.pick_origin(json!("yz"));
    // the origin picks survive a selection sync
    d.form.sync(&AXIS_TWO_PLANES, &s, None);
    apply(&mut s, &mut d);
    let (_, dir) = s.model.axes.values().next().copied().unwrap();
    assert!(dir.z.abs() > 0.999, "{dir:?}");
    d.applied = false;
    let mut d = Dialog::blank(Kind::Scale);
    d.bodies = vec![b.0];
    d.point = Some([0.0; 3]);
    d.a = "0.5".into();
    apply(&mut s, &mut d);
    assert!((s.model.bodies[0].shape.mass().unwrap().volume - 500.0).abs() < 1e-6);
    d.applied = false;

    // text in the sketch being edited
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    let mut d = form(&mut s, &SK_TEXT, vec![], &[("text", json!("Hi"))]);
    d.set_text_frame([0.0, 0.0], [40.0, 10.0], 0.0);
    apply(&mut s, &mut d);
    let sk = s.edit_sketch.unwrap();
    assert!(!s.model.sketches[&sk].profiles.is_empty());
  }

  /// Sketch Scale, as Fusion's: Entities and Point picks (OK waits for both, one prompt
  /// throughout), the Scale Factor row and the arrow at the point once it is picked; a 10 mm
  /// square doubled about its corner.
  #[test]
  fn sketch_scale_picks_entities_and_a_point_then_scales() {
    use fox_engine::kernel::glam::DVec3;
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let sk = s.edit_sketch.unwrap();
    let sketch = s.design.sketch(sk).unwrap();
    let curves: Vec<Sel> = sketch.curves.iter().map(|c| Sel::SketchCurve { sketch: sk, curve: c.id }).collect();
    let corner = sketch.points.iter().find(|p| p.x.abs() < 1e-9 && p.y.abs() < 1e-9).unwrap().id;
    let mut d = Dialog::blank(Kind::SkScale);
    d.a = "1".into();
    let prompt = "Select the sketch geometry to scale and the point from which to scale, then specify the scale factor";
    assert!(!d.ready() && !d.ok_enabled() && d.prompt(false).as_deref() == Some(prompt));
    assert!(d.manipulator(&s).is_none() && d.value_box_field().is_some());
    s.selection = curves;
    d.sync_selection(&s);
    assert_eq!(d.curves.len(), 4);
    assert!(!d.ready() && !d.ok_enabled() && d.prompt(false).as_deref() == Some(prompt));
    // the point picked with the Entities row still active is the point
    s.selection.push(Sel::SketchPoint { sketch: sk, point: corner });
    d.sync_selection(&s);
    assert!(d.center.is_some() && d.at == ["0".to_string(), "0".to_string()] && d.ready() && d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Specify scale factor"));
    // the arrow sits at the point along the sketch's x axis, as long as the entities reach
    let reach = 200f64.sqrt();
    d.refresh_manip(&s);
    let m = d.manipulator(&s).unwrap();
    assert!(m.origin.length() < 1e-9 && (m.dir - DVec3::X).length() < 1e-9 && (m.value - reach).abs() < 1e-6);
    // dragged to twice its reach: a factor of 2
    d.set_manipulator(2.0 * reach, 0.0);
    assert_eq!(d.a, "2");
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, &p["scale"], &p["center"]), ("sketch.transform", &json!("2"), &json!([0.0, 0.0])));
    apply(&mut s, &mut d);
    let sketch = s.design.sketch(sk).unwrap();
    let max_x = sketch
      .curves
      .iter()
      .flat_map(|c| sketch.curve_points(c.id))
      .filter_map(|p| sketch.pos(p))
      .map(|p| p[0])
      .fold(f64::MIN, f64::max);
    assert!((max_x - 20.0).abs() < 1e-6, "{max_x}");
    // under the preview the arrow reaches as far as the scaled entities
    d.refresh_manip(&s);
    let m = d.manipulator(&s).unwrap();
    assert!((m.value - 2.0 * reach).abs() < 1e-6, "{}", m.value);
  }

  #[test]
  fn sketch_project_intersect_and_align_forms() {
    use crate::forms::*;
    // a box under a sketch 30 mm up: Project To Surface and Intersection Curve onto its top
    let setup = |s: &mut Session| {
      run(s, "box", json!({"width": 40, "depth": 40, "height": 10}));
      run(s, "sketch.create", json!({"plane": {"origin": [0, 0, 30], "normal": [0, 0, 1]}}));
      run(s, "sketch.line", json!({"points": [[-15, 0], [15, 0]]}));
      let b = s.model.bodies[0].id;
      let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
      vec![Sel::Face { body: b, face: top }]
    };
    let mut s = Session::default();
    let top = setup(&mut s);
    let mut d = form(&mut s, &SK_PROJECT_SURFACE, top, &[]);
    let (cmd, p) = d.command().unwrap();
    assert_eq!(
      (cmd, p.get("type"), p.get("direction")),
      ("sketch.project_to_surface", Some(&json!("closest_point")), None)
    );
    apply(&mut s, &mut d);
    assert!(matches!(s.design.features.last().unwrap().kind, FeatureKind::ProjectToSurface { .. }));
    let mut s = Session::default();
    let top = setup(&mut s);
    let mut d = form(&mut s, &SK_INTERSECTION_CURVE, top, &[]);
    apply(&mut s, &mut d);
    assert!(matches!(s.design.features.last().unwrap().kind, FeatureKind::IntersectionCurve { .. }));

    // Align: a block's bottom made flush with a plate's top
    let mut s = Session::default();
    let mut comps = Vec::new();
    for (x, h) in [(0, 10), (60, 5)] {
      run(&mut s, "box", json!({"width": 20, "depth": 20, "height": h, "at": [x, 0]}));
      let b = s.model.bodies.last().unwrap().id.0;
      comps.push(run(&mut s, "component.create", json!({"bodies": [b]}))["component"].clone());
    }
    run(&mut s, "component.ground", json!({"component": comps[0]}));
    let (plate, block) = (s.model.bodies[0].id, s.model.bodies[1].id);
    let bottom = s.model.bodies[1].faces().iter().find(|f| f.normal.z < -0.9).unwrap().id;
    let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    let picks = vec![Sel::Face { body: block, face: bottom }, Sel::Face { body: plate, face: top }];
    let mut d = form(&mut s, &ALIGN, picks, &[("objects", json!("components"))]);
    assert_eq!(d.command().unwrap().0, "align");
    apply(&mut s, &mut d);
    let m = s.model.body(block).unwrap().shape.mass().unwrap();
    assert!((m.bbox_min.z - 10.0).abs() < 1e-6 && (m.bbox_min.x - 50.0).abs() < 1e-6, "{:?}", m.bbox_min);
  }

  #[test]
  fn align_remove_and_simplify_dialogs_as_fusion() {
    use crate::forms::*;
    let shown = |d: &Dialog, f: &'static Form| -> Vec<&str> {
      (0..f.inputs.len()).filter(|i| d.form.shown(f, *i)).map(|i| f.inputs[i].key()).collect()
    };
    // ALIGN: Object (Bodies), From and To; OK greyed and Fusion's prompt until both are picked
    let mut s = Session::default();
    s.execute("box", &json!({"width": 40, "depth": 30, "height": 10, "at": [20, 15]})).unwrap();
    s.execute("box", &json!({"width": 10, "depth": 10, "height": 10, "at": [55, 5]})).unwrap();
    let (plate, block) = (s.model.bodies[0].id, s.model.bodies[1].id);
    let mut d = form(&mut s, &ALIGN, vec![], &[]);
    assert_eq!(shown(&d, &ALIGN), ["objects", "moving", "fixed"]);
    assert_eq!(d.form.values[0], json!("bodies"));
    assert!(!d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Select geometry or snap point on alignable object"));
    let bottom = s.model.bodies[1].faces().iter().find(|f| f.normal.z < -0.9).unwrap().id;
    let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    d = form(&mut s, &ALIGN, vec![Sel::Face { body: block, face: bottom }], &[]);
    assert_eq!(d.prompt(false).as_deref(), Some("Select geometry or snap point on alignable object"));
    let picks = vec![Sel::Face { body: block, face: bottom }, Sel::Face { body: plate, face: top }];
    let mut d = form(&mut s, &ALIGN, picks, &[]);
    assert_eq!(shown(&d, &ALIGN), ["objects", "moving", "fixed", "relation", "offset", "flip"]);
    assert!(d.ok_enabled());
    // root bodies: the From body moves onto the plate
    apply(&mut s, &mut d);
    let m = s.model.body(block).unwrap().shape.mass().unwrap();
    assert!((m.bbox_min.z - 10.0).abs() < 1e-6 && (m.bbox_min.x - 50.0).abs() < 1e-6, "{:?}", m.bbox_min);
    assert!(!crate::forms::preview_box(&ALIGN));

    // Remove with nothing selected opens REMOVE: Objects, OK greyed, Fusion's prompt
    let mut app = App::with_session(Session::default());
    app.s.execute("box", &json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    let b = app.s.model.bodies[0].id;
    crate::panels::Cmd::RemoveBodies.run(&mut app);
    let d = app.dialog.as_mut().expect("REMOVE opens");
    assert_eq!(d.kind, Kind::Form(&REMOVE));
    assert_eq!(REMOVE.title, "REMOVE");
    assert!(matches!(REMOVE.inputs, [Input::Bodies { label: "Objects", required: true, .. }]));
    assert!(!d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Select bodies or components to remove"));
    app.s.selection = vec![Sel::Body { body: b }];
    d.form.sync(&REMOVE, &app.s, None);
    assert!(d.ok_enabled());
    apply(&mut app.s, d);
    assert!(app.s.model.bodies.is_empty());
    // with bodies selected it removes them at once, as before
    let mut app = App::with_session(Session::default());
    app.s.execute("box", &json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    app.s.selection = vec![Sel::Body { body: app.s.model.bodies[0].id }];
    crate::panels::Cmd::RemoveBodies.run(&mut app);
    assert!(app.dialog.is_none() && app.s.model.bodies.is_empty());

    // REMOVE FEATURES: Bodies, the Features group all ticked, only Close
    let mut s = Session::default();
    s.execute("box", &json!({"width": 40, "depth": 30, "height": 10, "at": [20, 15]})).unwrap();
    s.execute("cylinder", &json!({"radius": 5, "height": 10, "at": [12, 15], "operation": "cut"})).unwrap();
    let mut d = form(&mut s, &REMOVE_FEATURES, vec![], &[]);
    assert!(close_only(&REMOVE_FEATURES) && !d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Select solid bodies"));
    let keys: Vec<&str> = REMOVE_FEATURES.inputs.iter().map(Input::key).collect();
    assert_eq!(keys, ["bodies", "all", "fillet", "hole", "chamfer", "extrude", "revolve", "other"]);
    assert_eq!(d.rf_types(), REMOVE_FEATURE_TYPES);
    assert!(d.form.values[1..].iter().all(|v| *v == json!(true)));
    d.rf_tick("fillet", false);
    assert_eq!(d.form.values[1], json!(false));
    assert_eq!(d.rf_types(), ["hole", "chamfer", "extrude", "revolve", "other"]);
    d.rf_tick("all", true);
    assert_eq!(d.rf_types(), REMOVE_FEATURE_TYPES);
    assert!(greyed(&REMOVE_FEATURES, "chamfer").is_some() && greyed(&REMOVE_FEATURES, "hole").is_none());
    let body = s.model.bodies[0].id;
    let mut d = form(&mut s, &REMOVE_FEATURES, vec![Sel::Body { body }], &[]);
    // picking the body applies nothing; the hole is found and Remove takes it off
    let before = s.model.bodies[0].shape.mass().unwrap().volume;
    d.preview(&mut s);
    assert!(!d.applied);
    let found = d.rf_found(&mut s);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["type"], "hole");
    d.rf_remove(&mut s);
    assert!(d.error.is_none(), "{:?}", d.error);
    let after = s.model.bodies[0].shape.mass().unwrap().volume;
    assert!((after - before - std::f64::consts::PI * 250.0).abs() < 1e-3, "{before} {after}");
    assert!(d.rf_found(&mut s).is_empty());
    // closing keeps the removal
    d.cancel(&mut s);
    assert!((s.model.bodies[0].shape.mass().unwrap().volume - after).abs() < 1e-9);

    // Replace with Primitives: Bodies and the primitive; OK swaps the body for a box
    let mut s = Session::default();
    s.execute("cylinder", &json!({"radius": 5, "height": 20})).unwrap();
    let body = s.model.bodies[0].id;
    let d = form(&mut s, &REPLACE_PRIMITIVES, vec![], &[]);
    assert!(!d.ok_enabled());
    assert_eq!(d.prompt(false).as_deref(), Some("Select solid bodies"));
    let mut d = form(&mut s, &REPLACE_PRIMITIVES, vec![Sel::Body { body }], &[]);
    assert_eq!(d.command().unwrap().0, "replace_with_primitives");
    apply(&mut s, &mut d);
    assert_eq!(s.model.bodies.len(), 1);
    assert!((s.model.bodies[0].shape.mass().unwrap().volume - 2000.0).abs() < 1e-6);
  }

  #[test]
  fn move_in_a_sketch_moves_sketch_objects() {
    let mut app = App::with_session(Session::default());
    app.s.execute("box", &json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    let b = app.s.model.bodies[0].id;
    app.s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
    app.s.execute("sketch.rect", &json!({"corner1": [0, 0], "corner2": [10, 10]})).unwrap();
    app.s.selection.clear();
    assert_eq!(Dialog::open(&mut app, Kind::Move).mv.object, "sketch");
    // a picked body still moves the body
    app.s.selection = vec![Sel::Body { body: b }];
    assert_eq!(Dialog::open(&mut app, Kind::Move).mv.object, "bodies");
  }

  #[test]
  fn dialogs_fill_in_the_design_unit() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let mut d = Dialog::blank(Kind::Extrude);
    d.sketch = s.edit_sketch;
    d.profiles = vec![s.model.sketches[&d.sketch.unwrap()].profiles[0].interior_point()];
    run(&mut s, "sketch.finish", json!({}));
    d.localize(fox_engine::model::LengthUnit::In);
    assert_eq!(d.distance, "0.393701 in", "the 10 mm default, in inches");
    assert_eq!(d.angle, Dialog::blank(Kind::Extrude).angle, "angles are left alone");
    d.set_manipulator(25.4, 0.0);
    assert_eq!(d.distance, "1 in", "a dragged arrow writes inches");
    apply(&mut s, &mut d);
    let bb = s.model.bodies[0].shape.mass().unwrap();
    assert!((bb.bbox_max.z - 25.4).abs() < 1e-6, "{bb:?}");
  }

  #[test]
  fn extrude_arrow_drags_the_distance() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let mut d = Dialog::blank(Kind::Extrude);
    d.sketch = s.edit_sketch;
    d.profiles = vec![s.model.sketches[&d.sketch.unwrap()].profiles[0].interior_point()];
    run(&mut s, "sketch.finish", json!({}));
    let m = d.manipulator(&s).unwrap();
    assert!((m.value - 10.0).abs() < 1e-9 && m.dir.z > 0.99);
    d.set_manipulator(-7.26, 0.5);
    // no Flip row: dragged back past the profile, the value box shows the distance negative
    assert!(!d.flip && d.distance == "-7.5 mm", "{} {}", d.flip, d.distance);
    assert_eq!(d.extrude_distance(), ("7.5 mm".to_string(), true));
    let m = d.manipulator(&s).unwrap();
    assert!((m.value + 7.5).abs() < 1e-9);
    apply(&mut s, &mut d);
    let bb = s.model.bodies[0].shape.mass().unwrap();
    assert!((bb.bbox_min.z + 7.5).abs() < 1e-6, "{bb:?}");
    // typing a negative distance flips too; a flipped extrude is edited with its distance negative
    d.withdraw(&mut s);
    d.distance = "-4 mm".into();
    apply(&mut s, &mut d);
    let id = s.design.features.last().unwrap().id;
    let FeatureKind::Extrude { extent, .. } = &s.design.feature(id).unwrap().kind else { panic!() };
    assert!(extent.flip && extent.distance == "4 mm", "{extent:?}");
    let mut app = App::with_session(s);
    let e = Dialog::edit(&app, id).unwrap();
    assert!(!e.flip && e.distance == "-4 mm", "{} {}", e.flip, e.distance);
    assert_eq!(e.edit_set().unwrap()["extent"]["flip"], json!(true));
    assert_eq!(e.edit_set().unwrap()["extent"]["distance"], json!("4 mm"));
    app.s.undo();
  }

  #[test]
  fn extrude_waits_for_a_picked_profile() {
    // Sketch1 finished and still selected in the browser: E opens with Profiles: Select, no
    // preview and OK greyed, as in Fusion
    let mut app = App::with_session(Session::default());
    app.run("sketch.create", json!({"plane": "xy"}));
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let sk = app.s.edit_sketch.unwrap();
    app.run("sketch.finish", json!({}));
    app.s.selection = vec![Sel::Feature { feature: sk }];
    app.start(Kind::Extrude);
    let d = app.dialog.as_ref().unwrap();
    assert_eq!(d.sketch, Some(sk), "the sketch's profiles are the ones to pick");
    assert!(d.profiles.is_empty() && !d.ready() && !d.applied && !d.ok_enabled());
    assert!(app.s.model.bodies.is_empty(), "no preview body");
    assert!(d.preview_features(&app.s).is_empty());
    // a picked profile fills the row, previews and enables OK; the preview's feature stays out
    // of the timeline until OK
    app.s.selection = vec![Sel::Profile { sketch: sk, index: 0 }];
    let mut d = app.dialog.take().unwrap();
    d.sync_selection(&app.s);
    assert!(d.ready() && d.ok_enabled());
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none());
    assert_eq!(app.s.model.bodies.len(), 1);
    let previewed = d.preview_features(&app.s);
    assert_eq!(previewed, vec![app.s.design.features.last().unwrap().id]);
    // a profile selected before E is taken as the pick
    d.cancel(&mut app.s);
    app.s.selection = vec![Sel::Profile { sketch: sk, index: 0 }];
    let d = Dialog::open(&mut app, Kind::Extrude);
    assert_eq!((d.sketch, d.profiles.len()), (Some(sk), 1));
    assert!(d.ready());
  }

  #[test]
  fn manufacture_forms() {
    use crate::forms::*;
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 40, "depth": 40, "height": 10}));
    let b = s.model.bodies[0].id;
    let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    let mut d = form(&mut s, &CAM_SETUP, vec![], &[("name", json!("Op 10"))]);
    apply(&mut s, &mut d);
    assert_eq!(s.design.cam[0].name, "Op 10");
    d.applied = false;
    // pocket the top face as if it were a floor: a facing-like clearing at Z = 10
    let mut d = form(&mut s, &CAM_POCKET, vec![Sel::Face { body: b, face: top }], &[("tool_diameter", json!("8 mm"))]);
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = form(&mut s, &CAM_CONTOUR, vec![], &[]);
    apply(&mut s, &mut d);
    d.applied = false;
    assert_eq!(s.design.cam[0].operations.len(), 2);
    assert_eq!(s.design.cam[0].operations[0].tool.diameter, 8.0);
    run(&mut s, "cam.generate", json!({}));
    assert_eq!(s.toolpaths.len(), 2);
    // the fixtures form with nothing picked clears them
    let mut d = form(&mut s, &CAM_FIXTURES, vec![], &[]);
    apply(&mut s, &mut d);
    assert!(s.design.cam[0].fixtures.is_empty());
  }

  #[test]
  fn drawing_annotation_forms() {
    use crate::forms::*;
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 200, "depth": 20, "height": 10, "at": [100, 10]}));
    let b = s.model.bodies[0].id.0;
    run(&mut s, "component.create", json!({"name": "Bar", "bodies": [b]}));
    run(&mut s, "drawing.create", json!({"views": ["front", "top"]}));
    let id = s.design.drawings[0].id;
    let forms: [(&'static Form, Vec<(&str, Value)>); 7] = [
      (&DRAWING_BREAK, vec![]),
      (&DRAWING_ORDINATE, vec![("points", json!("20 0 0; 200 0 0"))]),
      (&DRAWING_ANGLE, vec![("a", json!(["200", "0", "0"])), ("b", json!(["200", "0", "10"]))]),
      (&DRAWING_FCF, vec![("diameter", json!(true)), ("modifier", json!("mmc"))]),
      (&DRAWING_DATUM, vec![]),
      (&DRAWING_FINISH, vec![("process", json!("removal"))]),
      (&DRAWING_BALLOON, vec![]),
    ];
    for (f, set) in forms {
      let mut d = form(&mut s, f, vec![], &set);
      d.form.set_drawing(f, &s, Some(id));
      apply(&mut s, &mut d);
      d.applied = false;
    }
    let dr = &s.design.drawings[0];
    assert_eq!(dr.views[0].breaks.len(), 1);
    assert_eq!(dr.dimensions.len(), 6);
    assert!(matches!(
      dr.dimensions[2],
      fox_engine::model::drawing::DrawingDim::FeatureControl { offset: [12.0, 12.0], .. }
    ));
    let text = fox_engine::model::drawing::render(dr, &s.design, &s.model).unwrap();
    for want in ["Ø0.1", "A", "1.6", "1"] {
      assert!(text.texts().contains(&want), "missing {want}: {:?}", text.texts());
    }
  }

  #[test]
  fn drawing_symbol_forms() {
    use crate::forms::*;
    use crate::sheet::{DRAWING_DATUM_TARGET, DRAWING_WELD};
    use fox_engine::model::drawing::{BackWeld, DrawingDim, WeldContour};
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 200, "depth": 20, "height": 10, "at": [100, 10]}));
    run(&mut s, "drawing.create", json!({"views": ["front"]}));
    let id = s.design.drawings[0].id;
    let add = |s: &mut Session, f: &'static Form, set: Vec<(&str, Value)>| {
      let mut d = form(s, f, vec![], &set);
      d.form.set_drawing(f, s, Some(id));
      apply(s, &mut d);
      d.applied = false;
      s.design.drawings[0].dimensions.last().cloned().unwrap()
    };
    let fcf = |composite: bool| {
      vec![
        ("characteristic", json!("position")),
        ("tolerance", json!("0.5")),
        ("diameter", json!(true)),
        ("datums", json!("A B")),
        ("composite", json!(composite)),
        ("lower_tolerance", json!("0.1")),
        ("lower_datums", json!("A")),
      ]
    };
    // a composite frame: the lower tier has the upper's zone shape
    let DrawingDim::FeatureControl { lower: Some(t), .. } = add(&mut s, &DRAWING_FCF, fcf(true)) else { panic!() };
    assert_eq!((t.tolerance, t.diameter, t.datums.clone()), (0.1, true, vec!["A".to_string()]));
    // unticked, the lower tier's fields don't count
    let DrawingDim::FeatureControl { lower, .. } = add(&mut s, &DRAWING_FCF, fcf(false)) else { panic!() };
    assert!(lower.is_none());

    // a weld's contour, finish and melt-through; the other side's fields only for both sides
    let weld = vec![
      ("kind", json!("v")),
      ("contour", json!("convex")),
      ("finish", json!("G")),
      ("other_contour", json!("flush")),
      ("back", json!("melt_through")),
    ];
    let DrawingDim::Weld { symbol: w, .. } = add(&mut s, &DRAWING_WELD, weld) else { panic!() };
    assert_eq!((w.contour, w.finish.as_str()), (Some(WeldContour::Convex), "G"));
    assert_eq!((w.other_contour, w.back), (None, Some(BackWeld::MeltThrough)));
    // no finish without a contour
    let DrawingDim::Weld { symbol: w, .. } = add(&mut s, &DRAWING_WELD, vec![("finish", json!("G"))]) else { panic!() };
    assert!(w.finish.is_empty());

    // datum targets: a line, and a point that keeps no area
    let line = vec![
      ("target", json!("line")),
      ("point", json!(["20", "0", "10"])),
      ("end", json!(["180", "0", "10"])),
      ("label", json!("A1")),
    ];
    let DrawingDim::DatumTarget { end, area, .. } = add(&mut s, &DRAWING_DATUM_TARGET, line) else { panic!() };
    assert_eq!((end, area), (Some([180.0, 0.0, 10.0]), 0.0));
    let point = vec![("target", json!("point")), ("area", json!("6")), ("label", json!("A2"))];
    let DrawingDim::DatumTarget { end, area, .. } = add(&mut s, &DRAWING_DATUM_TARGET, point) else { panic!() };
    assert_eq!((end, area), (None, 0.0));
  }

  #[test]
  fn picking_points_on_the_sheet() {
    use crate::forms::*;
    use fox_engine::model::drawing::{DrawingDim, render_sheet_snaps};
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 200, "depth": 20, "height": 10, "at": [100, 10]}));
    run(&mut s, "drawing.create", json!({"views": ["front", "top"]}));
    let id = s.design.drawings[0].id;
    let (_, snaps) = {
      let d = &s.design.drawings[0];
      render_sheet_snaps(d, &s.design, &s.model, d.sheet_list()[0].id).unwrap()
    };
    let snap = |view: u32, p: [f64; 3]| *snaps.iter().find(|x| x.view == view && x.point == p).unwrap();

    // Dimension: two picks, its view from the picks
    let mut d = Dialog::blank(Kind::DrawingDimension);
    d.target = Some(id);
    d.choice = "horizontal";
    d.distance = "10".into();
    assert!(d.takes_sheet_points());
    for p in [[0.0, 0.0, 0.0], [200.0, 0.0, 10.0]] {
      let x = snap(1, p);
      d.sheet_pick(x.view, x.point);
    }
    apply(&mut s, &mut d);
    let DrawingDim::Distance { view, a, b, .. } = s.design.drawings[0].dimensions[0].clone() else { panic!() };
    assert_eq!((view, a, b), (1, [0.0, 0.0, 0.0], [200.0, 0.0, 10.0]));

    // a form: the origin first, then the list of points grows; another view starts over
    let mut d = form(&mut s, &DRAWING_ORDINATE, vec![], &[]);
    d.form.set_drawing(&DRAWING_ORDINATE, &s, Some(id));
    assert!(d.takes_sheet_points());
    let top = s.design.drawings[0].views[1].id;
    let x = snap(top, [200.0, 20.0, 10.0]);
    d.sheet_pick(x.view, x.point);
    for p in [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [200.0, 0.0, 0.0]] {
      let x = snap(1, p);
      d.sheet_pick(x.view, x.point);
    }
    let get =
      |d: &Dialog, k: &str| d.form.values[DRAWING_ORDINATE.inputs.iter().position(|i| i.key() == k).unwrap()].clone();
    assert_eq!(get(&d, "view"), json!(1));
    assert_eq!(get(&d, "origin"), json!(["0", "0", "0"]));
    assert_eq!(get(&d, "points"), json!("100 0 0; 200 0 0"));
    apply(&mut s, &mut d);
    assert!(matches!(&s.design.drawings[0].dimensions[1], DrawingDim::Ordinate { points, .. } if points.len() == 2));

    // dialogs without model points don't take picks
    assert!(!Dialog::blank(Kind::DrawingNote).takes_sheet_points());
    assert!(!Dialog::blank(Kind::Form(&crate::sheet::DRAWING_REVISION)).takes_sheet_points());
    assert_eq!(short(-0.0001), "0");
    assert_eq!(short(12.5), "12.5");
  }

  #[test]
  fn surface_forms() {
    use crate::forms::*;
    let area = |s: &Session, b: BodyId| s.model.body(b).unwrap().shape.mass().unwrap().area;
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let sk = s.edit_sketch.unwrap().0;
    run(&mut s, "surface.patch", json!({"sketch": sk}));
    let b = s.model.bodies[0].id;
    // extend the edge at x = 10 by the default 5 mm
    let e = s.model.bodies[0].edges().iter().find(|e| (e.mid.x - 10.0).abs() < 1e-6).unwrap().id;
    let mut d = form(&mut s, &EXTEND, vec![Sel::Edge { body: b, edge: e }], &[("distance", json!("5 mm"))]);
    apply(&mut s, &mut d);
    assert!((area(&s, b) - 150.0).abs() < 1e-6, "{}", area(&s, b));
    d.applied = false;
    // trim with a sphere picked by a face: it cuts a round hole and stays itself
    run(&mut s, "sphere", json!({"at": [5, 5], "radius": 2}));
    let tool = s.model.bodies[1].id;
    let sel = vec![Sel::Body { body: b }, Sel::Face { body: tool, face: 1 }];
    let mut d = form(&mut s, &TRIM_BODY, sel, &[]);
    apply(&mut s, &mut d);
    assert!((area(&s, b) - (150.0 - 4.0 * std::f64::consts::PI)).abs() < 1e-6, "{}", area(&s, b));
    assert!(s.model.body(tool).is_some());
    d.applied = false;
    // untrim the hole
    let mut d = form(&mut s, &UNTRIM, vec![Sel::Face { body: b, face: 1 }], &[("loops", json!("inner"))]);
    apply(&mut s, &mut d);
    assert!((area(&s, b) - 150.0).abs() < 1e-6, "{}", area(&s, b));
    d.applied = false;
    // trim with a sketch line at x = 5: the side left of it goes
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.line", json!({"points": [[5, -1], [5, 11]]}));
    let sk = s.edit_sketch.unwrap();
    let curve = s.design.sketch(sk).unwrap().curves[0].id;
    run(&mut s, "sketch.finish", json!({}));
    let mut d = form(&mut s, &TRIM_SKETCH, vec![Sel::Body { body: b }, Sel::SketchCurve { sketch: sk, curve }], &[]);
    apply(&mut s, &mut d);
    assert!((area(&s, b) - 100.0).abs() < 1e-6, "{}", area(&s, b));
    d.applied = false;
    // a variable offset: the whole body by 2, its one face by 3
    let n = s.model.bodies.len();
    let sel = vec![Sel::Face { body: b, face: 1 }];
    let mut d = form(&mut s, &VARIABLE_OFFSET, sel, &[("distance2", json!("3 mm"))]);
    apply(&mut s, &mut d);
    assert_eq!(s.model.bodies.len(), n + 1);
    let z = s.model.bodies.last().unwrap().faces()[0].origin.z;
    assert!((z.abs() - 3.0).abs() < 1e-6, "{z}");
  }

  #[test]
  fn joint_to_a_saved_joint_origin() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 100, "depth": 60, "height": 10, "at": [50, 30]}));
    run(&mut s, "box", json!({"width": 80, "depth": 20, "height": 5, "at": [40, 200]}));
    let (base, arm) = (s.model.bodies[0].id.0, s.model.bodies[1].id.0);
    run(&mut s, "component.create", json!({"name": "Base", "bodies": [base]}));
    run(&mut s, "component.create", json!({"name": "Arm", "bodies": [arm]}));
    let top = face(&s, 0, |f| f.normal.z > 0.9);
    run(&mut s, "joint_origin", json!({"face": top, "name": "Top"}));
    assert_eq!(saved_origins(&s), vec!["Top".to_string()]);
    // the fixed side is the saved origin: one pick (the moving side) is enough
    let mut d = Dialog::blank(Kind::Joint);
    d.choice = "rigid";
    d.saved[1] = "Top".into();
    // the arm's bottom onto the base's top: flipped, the Z axes face each other and it sits on top
    d.flip = true;
    (d.angle, d.distance) = ("0 deg".into(), "0 mm".into());
    // as in Fusion, unflipped Z axes line up; flipped, the arm's bottom faces the base's top
    d.flip = true;
    d.origins = vec![(arm, json!({"face": face(&s, 1, |f| f.normal.z < -0.9)}))];
    d.comps = vec![s.design.component_of(BodyId(arm)).map(|c| c.id.0)];
    apply(&mut s, &mut d);
    let m = s.model.bodies.iter().find(|b| b.id.0 == arm).unwrap().shape.mass().unwrap();
    assert!((m.bbox_min.z - 10.0).abs() < 1e-6, "{:?}", m.bbox_min);
  }

  #[test]
  fn manufacture_forms_tools_entries_and_posts() {
    use crate::forms::*;
    use fox_engine::model::cam::{Entry, OpKind, ToolKind};
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 40, "depth": 40, "height": 10}));
    let b = s.model.bodies[0].id;
    let top = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    let floor = vec![Sel::Face { body: b, face: top }];
    let mut d = form(&mut s, &CAM_SETUP, vec![], &[("post", json!("grbl")), ("units", json!("inch"))]);
    apply(&mut s, &mut d);
    assert_eq!((s.design.cam[0].post.as_str(), s.design.cam[0].imperial), ("grbl", true));
    let mut d = form(&mut s, &CAM_ADAPTIVE, floor.clone(), &[("tool_diameter", json!("8 mm"))]);
    apply(&mut s, &mut d);
    let mut d = form(&mut s, &CAM_POCKET, floor, &[("rest", json!(true)), ("entry", json!("ramp"))]);
    apply(&mut s, &mut d);
    let mut d = form(&mut s, &CAM_CONTOUR, vec![], &[("tabs", json!("2"))]);
    apply(&mut s, &mut d);
    let mut d = form(&mut s, &CAM_PARALLEL, vec![], &[("stepover", json!("0.3"))]);
    apply(&mut s, &mut d);
    let ops = &s.design.cam[0].operations;
    assert_eq!(ops.len(), 4);
    assert!(matches!(ops[0].kind, OpKind::Adaptive { .. }) && ops[0].entry == Entry::Helix && ops[0].stepover == 0.1);
    assert!(matches!(ops[1].kind, OpKind::Pocket { rest: true, .. }) && ops[1].entry == Entry::Ramp);
    assert!(matches!(ops[2].kind, OpKind::Contour { tabs: Some(t), .. } if t.count == 2 && t.width == 5.0));
    assert!(matches!(ops[3].kind, OpKind::Parallel { .. }) && ops[3].tool.kind == ToolKind::Ball);
    let g = run(&mut s, "cam.post", json!({}));
    assert!(g["gcode"].as_str().unwrap().contains("G20") && g["post"] == "grbl");
    let r = run(&mut s, "cam.simulate", json!({}));
    assert!(r["removed"].as_array().unwrap().len() == 4);
    assert!(s.simulation.is_some());
  }

  #[test]
  fn text_form_places_styled_text_in_a_two_corner_box() {
    use crate::forms::*;
    let mut s = Session::default();
    let sk = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].as_u64().unwrap() as u32;
    let vals = [
      ("text", json!("Hi")),
      ("placement", json!("box")),
      ("x", json!("10")),
      ("y", json!("0")),
      ("x2", json!("50")),
      ("y2", json!("20")),
      ("halign", json!("right")),
      ("valign", json!("bottom")),
      ("bold", json!(true)),
      ("spacing", json!("10")),
    ];
    let mut d = form(&mut s, &SK_TEXT, vec![], &vals);
    apply(&mut s, &mut d);
    d.applied = false;
    let sketch = s.design.sketch(FeatureId(sk)).unwrap();
    let t = &sketch.texts[0];
    assert!(t.style.bold && !t.style.underline && !t.style.italic && t.style.spacing == 10.0);
    // right and bottom of the box: the anchor is its lower right corner
    assert_eq!(sketch.pos(t.anchor), Some([50.0, 0.0]));
    let (lo, hi) = sketch.bounds().unwrap();
    assert!(hi[0] <= 50.0 + 1e-6 && lo[0] > 10.0 && lo[1] < 0.0, "{lo:?} {hi:?}");
    // Edit Text opens with the style and changes it
    let sel = vec![Sel::SketchCurve { sketch: FeatureId(sk), curve: t.curves[0] }];
    let mut d = form(&mut s, &SK_TEXT_EDIT, sel, &[]);
    prefill(&SK_TEXT_EDIT, &mut d.form, &s);
    let at = |k: &str| SK_TEXT_EDIT.inputs.iter().position(|x| x.key() == k).unwrap();
    assert_eq!(d.form.values[at("bold")], json!(true));
    assert_eq!(d.form.values[at("halign")], json!("right"));
    d.form.values[at("italic")] = json!(true);
    d.form.values[at("bold")] = json!(false);
    d.form.values[at("underline")] = json!(true);
    apply(&mut s, &mut d);
    let t = &s.design.sketch(FeatureId(sk)).unwrap().texts[0];
    assert!(t.style.italic && !t.style.bold && t.style.underline);
  }

  #[test]
  fn text_forms_edit_and_explode() {
    use crate::forms::*;
    let mut s = Session::default();
    let sk = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].as_u64().unwrap() as u32;
    let mut d = form(&mut s, &SK_TEXT, vec![], &[("text", json!("AB")), ("font", json!("mono"))]);
    d.set_text_frame([0.0, 0.0], [40.0, 10.0], 0.0);
    apply(&mut s, &mut d);
    d.applied = false;
    let sketch = s.design.sketch(FeatureId(sk)).unwrap();
    let (glyph, font) = (sketch.texts[0].curves[0], sketch.texts[0].font.clone());
    assert_eq!(font, "mono");
    // Edit Text opens with the picked text's values
    let sel = vec![Sel::SketchCurve { sketch: FeatureId(sk), curve: glyph }];
    let mut d = form(&mut s, &SK_TEXT_EDIT, sel.clone(), &[]);
    prefill(&SK_TEXT_EDIT, &mut d.form, &s);
    let i = SK_TEXT_EDIT.inputs.iter().position(|x| x.key() == "text").unwrap();
    assert_eq!(d.form.values[i], json!("AB"));
    d.form.values[i] = json!("ABC");
    let h = SK_TEXT_EDIT.inputs.iter().position(|x| x.key() == "angle").unwrap();
    d.form.values[h] = json!("90");
    apply(&mut s, &mut d);
    d.applied = false;
    let t = &s.design.sketch(FeatureId(sk)).unwrap().texts[0];
    assert_eq!((t.text.as_str(), t.font.as_str()), ("ABC", "mono"));
    assert!((t.angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    let glyph = t.curves[0];
    let sel = vec![Sel::SketchCurve { sketch: FeatureId(sk), curve: glyph }];
    let mut d = form(&mut s, &SK_TEXT_EXPLODE, sel, &[]);
    apply(&mut s, &mut d);
    assert!(s.design.sketch(FeatureId(sk)).unwrap().texts.is_empty());
  }

  /// Draws a dialog for one frame.
  fn show_once(app: &mut App, d: &mut Dialog) -> bool {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))),
      ..Default::default()
    };
    let mut keep = false;
    let mut out = ctx.run_ui(raw, |ui| keep = d.show(app, ui.ctx()));
    out.textures_delta.clear();
    keep
  }

  fn labels(rows: &[Readout]) -> Vec<(&str, &str)> {
    rows.iter().map(|r| (r.label.as_str(), r.value.as_str())).collect()
  }

  #[test]
  fn measure_reads_out_single_picks_and_pairs_with_precision_and_units() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 10, "depth": 20, "height": 30}));
    let b = s.model.bodies[0].id;
    let body = s.model.body(b).unwrap();
    let faces: Vec<_> = body.faces().iter().filter(|f| (f.area - 200.0).abs() < 1e-6).map(|f| f.id).collect();
    let face = faces[0];
    let edge = body.edges().iter().find(|e| (e.length - 30.0).abs() < 1e-6).unwrap().id;
    let mm = MeasureFmt::default();

    // a face: "Selection 1", "Face selected", its area and loop length, as Fusion's
    let r = s.execute("measure", &json!({"a": {"type": "face", "body": b.0, "face": face}})).unwrap();
    let secs = measure_sections(&r, &mm);
    assert_eq!(secs.len(), 1);
    assert_eq!((secs[0].title.as_str(), secs[0].note.as_str()), ("Selection 1", "Face selected"));
    assert_eq!(labels(&secs[0].rows), [("Area", "200.000 mm^2"), ("Loop Length", "60.000 mm")]);
    // an edge: its length; a vertex: its position
    let r = s.execute("measure", &json!({"a": {"type": "edge", "body": b.0, "edge": edge}})).unwrap();
    let secs = measure_sections(&r, &mm);
    assert_eq!(secs[0].note, "Edge selected");
    assert_eq!(labels(&secs[0].rows)[0], ("Length", "30.000 mm"));
    let r = s.execute("measure", &json!({"a": {"type": "vertex", "body": b.0, "edge": edge, "end": true}})).unwrap();
    let secs = measure_sections(&r, &mm);
    assert_eq!(secs[0].note, "Vertex selected");
    assert_eq!(
      secs[0].rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
      ["X Position", "Y Position", "Z Position"]
    );
    assert_eq!(labels(&secs[0].rows)[2], ("Z Position", "30.000 mm"));
    // a body: its volume, in other units and precision
    let cm1 = MeasureFmt { precision: 1, unit: 1, secondary: None };
    let r = s.execute("measure", &json!({"a": {"body": b.0}})).unwrap();
    let rows = &measure_sections(&r, &cm1)[0].rows;
    assert!(labels(rows).contains(&("Volume", "6.0 cm^3")), "{rows:?}");
    assert!(labels(rows).contains(&("Area", "22.0 cm^2")), "{rows:?}");
    // Secondary Units add the value in a second unit
    let both = MeasureFmt { precision: 3, unit: 3, secondary: Some(0) };
    assert_eq!(both.area(645.16), "1.000 in^2 (645.160 mm^2)");
    assert_eq!(MeasureFmt { secondary: Some(3), ..both }.len(25.4), "1.000 in", "the same unit once");

    // two picks: Results with the distance and angle (no deltas), then a section each
    let r = s
      .execute("measure", &json!({"a": {"body": b.0, "face": faces[0]}, "b": {"body": b.0, "face": faces[1]}}))
      .unwrap();
    let inch = MeasureFmt { precision: 2, unit: 3, secondary: None };
    let secs = measure_sections(&r, &inch);
    let titles: Vec<&str> = secs.iter().map(|x| x.title.as_str()).collect();
    assert_eq!(titles, ["Results", "Selection 1", "Selection 2"]);
    assert_eq!(secs[2].note, "Face selected");
    assert_eq!(labels(&secs[0].rows), [("Distance", "1.18 in"), ("Angle", "180.00 deg")]);
    assert_eq!(MeasureFmt { precision: 0, ..MeasureFmt::default() }.len(-0.2), "0 mm");
    // Fusion's Precision list
    assert_eq!((0..=4).map(precision_label).collect::<Vec<_>>(), ["0", "0.1", "0.12", "0.123", "0.1234"]);
  }

  #[test]
  fn measure_dialog_shows_the_selection_and_restores_the_filter() {
    let mut app = App::with_session(Session::default());
    app.s.execute("box", &json!({"width": 10, "depth": 20, "height": 30})).unwrap();
    let b = app.s.model.bodies[0].id;
    let edge = app.s.model.body(b).unwrap().edges()[0].id;
    app.s.selection = vec![Sel::Edge { body: b, edge }];
    // with the Body Vertices filter off, Measure turns it on while it is open
    app.sel_filter.vertices = false;
    let mut d = Dialog::open(&mut app, Kind::Inspect);
    assert!(show_once(&mut app, &mut d));
    assert!(app.sel_filter.vertices, "Measure picks vertices");
    app.s.selection = vec![Sel::Edge { body: b, edge }, Sel::Body { body: b }];
    assert!(show_once(&mut app, &mut d));
    assert!(app.measure.is_some(), "two picks draw the distance line");
    // values read in the document's unit
    app.s.design.units = fox_engine::model::LengthUnit::In;
    assert!(show_once(&mut app, &mut d));
    assert_eq!(MEASURE_UNITS[app.measure_fmt.unit].0, "in");
    d.leave(&mut app);
    assert!(!app.sel_filter.vertices);
    assert!(app.s.selection.is_empty() && app.measure.is_none(), "closing Measure lets its picks go");
  }

  #[test]
  fn appearance_panel_applies_swatches_to_bodies_and_faces() {
    let mut app = App::with_session(Session::default());
    app.s.execute("box", &json!({"width": 10, "depth": 20, "height": 30})).unwrap();
    let b = app.s.model.bodies[0].id;
    let face = app.s.model.body(b).unwrap().faces()[0].id;
    // A opens the panel, not the old form
    let mut d = Dialog::open(&mut app, Kind::Form(&crate::forms::APPEARANCE));
    assert_eq!(d.kind, Kind::Appearance);
    assert!(d.kind.reports());
    assert!(show_once(&mut app, &mut d));

    let lib: Vec<(String, [u8; 3])> =
      crate::material_ui::appearances(&mut app).into_iter().map(|s| (s.name, s.color)).collect();
    assert!(lib.len() >= 16 && lib.iter().any(|(n, _)| n == "Copper"));
    // a new body shows the default appearance
    assert_eq!(in_this_design(&app, &lib), [("Steel - Satin".to_string(), [205, 200, 190], 1)]);

    let copper = lib.iter().find(|(n, _)| n == "Copper").unwrap().1;
    assert!(!apply_appearance(&mut app, "Copper", copper, true, &[Sel::Body { body: b }]), "faces mode needs faces");
    assert!(apply_appearance(&mut app, "Copper", copper, false, &[Sel::Face { body: b, face }]));
    assert_eq!(app.s.design.body_color(b), Some(copper));
    assert!(apply_appearance(&mut app, "#102030", [16, 32, 48], true, &[Sel::Face { body: b, face }]));
    assert_eq!(app.s.design.face_appearances.len(), 1);
    let used = in_this_design(&app, &lib);
    assert_eq!(used, [("Copper".to_string(), copper, 1), ("#102030".to_string(), [16, 32, 48], 1)]);
  }

  #[test]
  fn named_views_save_and_restore_the_camera() {
    let mut app = App::with_session(Session::default());
    app.cam = fox_engine::render::Camera {
      target: [5.0, 6.0, 7.0],
      yaw: 1.0,
      pitch: 0.3,
      distance: 321.0,
      fov: 0.5,
      ortho: true,
      roll: 0.0,
    };
    let saved = app.cam;
    let name = app.save_named_view().unwrap();
    assert_eq!(name, "NamedView");
    assert!(app.s.dirty, "named views save with the design");
    app.cam = fox_engine::render::Camera::default();
    app.restore_named_view(&name);
    assert_eq!(app.cam_move.map(|m| m.1), Some(saved));
    // the round trip through the design's JSON keeps it
    let json = serde_json::to_string(&app.s.design).unwrap();
    let back: fox_engine::model::Design = serde_json::from_str(&json).unwrap();
    assert_eq!(back.named_views, app.s.design.named_views);
  }

  #[test]
  fn text_form_lays_text_along_a_picked_curve() {
    use crate::forms::*;
    let mut s = Session::default();
    let sk = run(&mut s, "sketch.create", json!({"plane": "xy"}))["sketch"].as_u64().unwrap() as u32;
    let arc = run(&mut s, "sketch.arc", json!({"center": [0, 0], "start": [20, 0], "end": [0, 20]}))["curves"][0]
      .as_u64()
      .unwrap() as u32;
    let sel = vec![Sel::SketchCurve { sketch: FeatureId(sk), curve: fox_engine::sketch::CurveId(arc) }];
    let mut d =
      form(&mut s, &SK_TEXT, sel, &[("type", json!("path")), ("text", json!("LL")), ("align", json!("center"))]);
    apply(&mut s, &mut d);
    let t = &s.design.sketch(FeatureId(sk)).unwrap().texts[0];
    let path = t.path.expect("the text runs along the arc");
    assert_eq!((path.curve.0, path.above, path.align), (arc, true, fox_engine::sketch::TextAlign::Center));
  }

  #[test]
  fn offset_plane_takes_a_picked_face_or_plane_as_its_base() {
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 10, "at": [30, 0]}));
    let b = s.model.bodies[0].id;
    let top = *s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap();
    let mut d = Dialog::blank(Kind::OffsetPlane);
    d.sync_selection(&s);
    assert_eq!(d.plane, json!("xy"));
    apply(&mut s, &mut d);
    assert_eq!(d.manipulator(&s).unwrap().origin, fox_engine::kernel::glam::DVec3::ZERO);

    // clicking the top face makes it the base: the label, the arrow and the preview follow it
    s.selection = vec![Sel::Face { body: b, face: top.id }];
    d.sync_selection(&s);
    assert_eq!(d.plane, json!({"body": b.0, "face": top.id}));
    assert!(d.plane_label.starts_with("Face of"), "{}", d.plane_label);
    assert!(d.dirty);
    d.preview(&mut s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    let m = d.manipulator(&s).unwrap();
    assert!((m.origin - top.center).length() < 1e-6, "{:?} vs {:?}", m.origin, top.center);
    assert!((m.dir.z - 1.0).abs() < 1e-9 && (m.value - 10.0).abs() < 1e-9);
    let made = d.previewed_plane().expect("the preview's plane");
    assert!((s.model.planes[&made].origin.z - 20.0).abs() < 1e-6);
    let (pl, centre) = d.offset_plane_preview(&s).unwrap();
    assert!(
      (pl.origin.z - 20.0).abs() < 1e-6
        && (centre - (top.center + fox_engine::kernel::glam::DVec3::Z * 10.0)).length() < 1e-6
    );
    // the scene leaves the previewed plane to the UI's filled square
    let opts = fox_engine::scene::ViewOptions { hide_planes: vec![made], ..Default::default() };
    let (hidden, _) = fox_engine::scene::SceneBuilder::default().build(&s, &opts);
    let (shown, _) = fox_engine::scene::SceneBuilder::default().build(&s, &Default::default());
    assert_eq!(shown.lines.iter().filter(|l| l.dashed).count(), hidden.lines.iter().filter(|l| l.dashed).count() + 1);

    // the dropdown still offers the origin planes, and a face left selected doesn't undo it
    d.use_origin_plane(&mut s, "xz");
    d.sync_selection(&s);
    assert_eq!(d.plane, json!("xz"));

    // a construction plane picked in the browser is a base too
    d.unpreview(&mut s);
    let pl =
      FeatureId(run(&mut s, "plane.offset", json!({"plane": "yz", "offset": 5}))["feature"].as_u64().unwrap() as u32);
    s.selection = vec![Sel::Feature { feature: pl }];
    d.sync_selection(&s);
    assert_eq!(d.plane, json!({"feature": pl.0}));
    apply(&mut s, &mut d);
    assert!((d.manipulator(&s).unwrap().origin.x - 5.0).abs() < 1e-9);
    let ok = d.command().unwrap();
    assert_eq!(ok.1["plane"], json!({"feature": pl.0}));
  }

  #[test]
  fn remove_duplicates_and_clean_up_forms() {
    use crate::forms::*;
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0]]}));
    run(&mut s, "sketch.line", json!({"points": [[10, 0.01], [0, 0]]}));
    let sk = s.edit_sketch.unwrap();
    // a custom tolerance under the 0.01 mm offset keeps both lines
    let set = [("tolerance", json!("custom")), ("custom", json!("0.001 mm"))];
    let mut d = form(&mut s, &SK_REMOVE_DUPLICATES, vec![], &set);
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, &p["tolerance"]), ("sketch.remove_duplicates", &json!("0.001 mm")));
    assert!(p.get("custom").is_none());
    apply(&mut s, &mut d);
    assert_eq!(d.summary.as_deref(), Some("No duplicates"));
    d.unpreview(&mut s);
    // the standard 0.001" takes the copy, and the dialog says so
    let mut d = form(&mut s, &SK_REMOVE_DUPLICATES, vec![], &[]);
    assert_eq!(d.command().unwrap().1["tolerance"], "0.001 in");
    apply(&mut s, &mut d);
    assert_eq!(d.summary.as_deref(), Some("1 line removed"));
    assert_eq!(s.design.sketch(sk).unwrap().curves.len(), 1);

    // Clean Up: links break and stray points go unless kept
    run(&mut s, "sketch.point", json!({"at": [5, 5]}));
    let mut d = form(&mut s, &SK_CLEANUP, vec![], &[("keep_points", json!(true))]);
    let p = d.command().unwrap().1;
    assert_eq!((p["break_links"].clone(), p["purge_points"].clone()), (json!(true), json!(false)));
    assert_eq!(p["delete_constraints"], "projected");
    assert_eq!(p["splines_to"], "none");
    let points = s.design.sketch(sk).unwrap().points.len();
    apply(&mut s, &mut d);
    assert_eq!(s.design.sketch(sk).unwrap().points.len(), points);
    d.unpreview(&mut s);
    let mut d = form(&mut s, &SK_CLEANUP, vec![], &[]);
    apply(&mut s, &mut d);
    assert_eq!(s.design.sketch(sk).unwrap().points.len(), points - 1);
  }

  #[test]
  fn press_pull_opens_the_command_for_the_pick() {
    // a face: Offset Face (the Press Pull dialog); an edge: Fillet; a profile: Extrude
    let mut app = App::with_session(Session::default());
    app.s.execute("box", &json!({"width": 20, "depth": 20, "height": 10})).unwrap();
    let b = app.s.model.bodies[0].id;
    // a side face (the box's top edits its Height, see `press_owner`)
    let side = face(&app.s, 0, |f| f.normal.x > 0.9)["face"].as_u64().unwrap() as u32;
    app.s.selection = vec![Sel::Face { body: b, face: side }];
    let d = Dialog::open(&mut app, Kind::PressPull);
    assert_eq!((d.kind, d.faces.len(), d.choice), (Kind::PressPull, 1, "automatic"));
    assert_eq!(d.command().unwrap().0, "press_pull");
    let mut d = d;
    d.choice = "new";
    assert_eq!(d.command().unwrap().0, "press_pull", "New Offset adds an Offset Face feature too");
    let e = app.s.model.bodies[0].edges()[0].id;
    app.s.selection = vec![Sel::Edge { body: b, edge: e }];
    assert_eq!(Dialog::open(&mut app, Kind::PressPull).kind, Kind::Fillet);
    app.s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
    app.s.execute("sketch.rect", &json!({"corner1": [30, 30], "corner2": [40, 40]})).unwrap();
    let sk = app.s.edit_sketch.unwrap();
    app.s.execute("sketch.finish", &json!({})).unwrap();
    app.s.selection = vec![Sel::Profile { sketch: sk, index: 0 }];
    assert_eq!(Dialog::open(&mut app, Kind::PressPull).kind, Kind::Extrude);
    assert_eq!(press_pull_kind(&Session::default()), Kind::PressPull);
  }

  #[test]
  fn chamfer_picks_the_tangent_chain() {
    // a box with one vertical edge rounded: a top edge meeting the round chains on round it
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 20, "depth": 20, "height": 20}));
    // the box is centred on the origin: its corner edge is at (10, 10)
    let corner = edge(&s, 0, |e| e.mid.x > 9.9 && e.mid.y > 9.9);
    run(&mut s, "fillet", json!({"edges": [corner], "radius": 5}));
    let b = s.model.bodies[0].id;
    let top_right = s.model.bodies[0]
      .edges()
      .iter()
      .find(|e| e.mid.z > 19.9 && e.mid.x > 9.9 && e.kind == fox_engine::kernel::CurveKind::Line)
      .unwrap()
      .id;
    s.selection = vec![Sel::Edge { body: b, edge: top_right }];
    let mut d = Dialog::blank(Kind::Chamfer);
    d.radius = "1 mm".into();
    d.sync_selection(&s);
    assert_eq!(d.edges.len(), 3, "the line, the arc and the line past it: {:?}", d.edges);
    let (cmd, p) = d.command().unwrap();
    assert_eq!((cmd, p["tangent_chain"].as_bool()), ("chamfer", Some(true)));
    apply(&mut s, &mut d);
    d.unpreview(&mut s);
    // Tangent Chain off: just the pick (the withdrawn preview put all three back in the selection)
    d.solid.no_chain = true;
    s.selection = vec![Sel::Edge { body: b, edge: top_right }];
    d.sync_selection(&s);
    assert_eq!(d.edges.len(), 1);
    assert_eq!(d.command().unwrap().1["tangent_chain"], json!(false));
  }

  /// `ui:sketch:offset-default-follow`, `ui:sketch:offset-dialog-rows`: Offset on a pre-selected
  /// curve starts at 0.00 on the curve (no following, nothing previewed, no complaint, OK
  /// waiting), with Chain Selection off and the prompt naming its values; a typed distance
  /// previews and OK applies. Two Sides sends a second distance.
  #[test]
  fn sketch_offset_starts_at_zero_on_a_preselected_curve() {
    let mut app = App::with_session(Session::default());
    app.run("sketch.create", json!({"plane": "xy"}));
    app.run("sketch.line", json!({"points": [[0, 0], [10, 0], [10, 10]]}));
    let sk = app.s.edit_sketch.unwrap();
    let first = app.s.design.sketch(sk).unwrap().curves[0].id;
    app.s.selection = vec![Sel::SketchCurve { sketch: sk, curve: first }];
    let d = Dialog::open(&mut app, Kind::SkOffset);
    assert_eq!(d.distance, "0 mm");
    assert!(!d.follows_cursor(), "the offset stays on the curve until dragged or typed");
    assert!(d.solid.no_chain, "Chain Selection is off for a pre-selected curve");
    assert_eq!(d.curves, vec![first.0], "the picked curve alone");
    assert_eq!(d.prompt(false).as_deref(), Some("Modify offset distance or side settings"));
    assert_eq!(d.prompt(true).as_deref(), Some("Modify offset distance or side settings"));
    assert!(!d.ok_enabled(), "OK waits for a distance");
    assert!(d.box_by_pointer(&app.s), "the value box sits by the pointer");
    let mut d = d;
    d.preview(&mut app.s);
    assert!(d.error.is_none() && !d.applied, "nothing previewed at 0.00, nothing wrong");
    assert_eq!(app.s.design.sketch(sk).unwrap().curves.len(), 2);
    // a half-typed distance is the same; a typed one previews
    d.distance = ".".into();
    assert!(!d.ok_enabled() && !d.offset_set());
    d.preview(&mut app.s);
    assert!(d.error.is_none() && !d.applied);
    d.distance = ".1".into();
    assert!(d.ok_enabled() && !d.box_by_pointer(&app.s));
    d.preview(&mut app.s);
    assert!(d.applied && d.error.is_none(), "{:?}", d.error);
    assert_eq!(app.s.design.sketch(sk).unwrap().curves.len(), 3);
    d.unpreview(&mut app.s);
    // a user parameter counts as a distance
    d.distance = "d1".into();
    assert!(d.offset_set());
    // Two Sides sends the second distance; One Side and Symmetric don't
    d.distance = "2 mm".into();
    d.extent = "two_sides";
    assert_eq!(d.command().unwrap().1["distance2"], json!("0 mm"));
    d.extent = "one_side";
    assert!(d.command().unwrap().1.get("distance2").is_none());
    // with nothing pre-selected the offset follows the pointer once a curve is picked, chained
    app.s.selection.clear();
    let d = Dialog::open(&mut app, Kind::SkOffset);
    assert!(d.follow && !d.solid.no_chain && d.curves.is_empty());
  }

  #[test]
  fn sketch_offset_chains_curves_and_flips() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    let r = run(&mut s, "sketch.line", json!({"points": [[0, 0], [10, 0], [10, 10], [0, 10]]}));
    let chain: Vec<u32> = r["curves"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
    run(&mut s, "sketch.line", json!({"points": [[20, 0], [30, 0]]}));
    let sk = s.edit_sketch.unwrap();
    s.selection = vec![Sel::SketchCurve { sketch: sk, curve: fox_engine::sketch::CurveId(chain[1]) }];
    let mut d = Dialog::blank(Kind::SkOffset);
    d.distance = "1 mm".into();
    d.sync_selection(&s);
    let mut got = d.curves.clone();
    got.sort();
    let mut want = chain.clone();
    want.sort();
    assert_eq!(got, want, "Chain Selection picks the curves joined end to end");
    assert_eq!(d.command().unwrap().1["both_sides"], json!(false));
    apply(&mut s, &mut d);
    d.unpreview(&mut s);
    // Direction Symmetric offsets both sides; Flip turns the distance round
    d.extent = "symmetric";
    assert_eq!(d.command().unwrap().1["both_sides"], json!(true));
    d.distance = negated(&d.distance);
    assert_eq!(d.distance, "-1 mm");
    assert_eq!(negated(&d.distance), "1 mm");
    assert_eq!(negated("a + b"), "-(a + b)");
    assert_eq!(negated("-(a + b)"), "a + b");
    // Chain Selection off: the pick alone
    d.solid.no_chain = true;
    s.selection = vec![Sel::SketchCurve { sketch: sk, curve: fox_engine::sketch::CurveId(chain[1]) }];
    assert_eq!(d.offset_picks(&s), vec![chain[1]]);
  }

  #[test]
  fn shell_type_and_floating_boxes_edit_the_fields() {
    let mut d = Dialog::blank(Kind::Shell);
    d.radius = "2 mm".into();
    assert_eq!(d.shell_params()["corners"], json!("sharp"));
    d.flag = true;
    assert_eq!(d.shell_params()["corners"], json!("round"));
    // Shell's handles edit the inside, then the outside thickness
    *d.handle_field(0).unwrap() = "3 mm".into();
    d.solid.thickness_out = "1 mm".into();
    *d.handle_field(1).unwrap() = "4 mm".into();
    assert_eq!((d.radius.as_str(), d.solid.thickness_out.as_str()), ("3 mm", "4 mm"));
    assert!(d.handle_field(2).is_none());
    // the drag arrows' boxes
    let mut c = Dialog::blank(Kind::Chamfer);
    *c.value_box_field().unwrap() = "5 mm".into();
    assert_eq!(c.radius, "5 mm");
    assert!(Dialog::blank(Kind::Box).value_box_field().is_none());
  }
}
