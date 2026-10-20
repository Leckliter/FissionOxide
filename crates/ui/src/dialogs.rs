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

mod extrude;
mod live;
mod manip;
mod solid;

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
  Draft,
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
  Material,
  Interference,
  Bom,
  CenterOfMass,
  // sketch modify (run on the sketch being edited)
  SkFillet,
  SkChamfer,
  SkChamfer2,
  SkChamferAngle,
  SkOffset,
  SkMirror,
  SkCircPattern,
  SkRectPattern,
  Project,
  Refold,
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
      Kind::Move => "MOVE / ROTATE",
      Kind::OffsetPlane => "OFFSET PLANE",
      Kind::Inspect => "MEASURE",
      Kind::Appearance => "APPEARANCE",
      Kind::Loft => "LOFT",
      Kind::Sweep => "SWEEP",
      Kind::Hole => "HOLE",
      Kind::PressPull => "PRESS PULL",
      Kind::Draft => "DRAFT",
      Kind::SplitBody => "SPLIT BODY",
      Kind::Flange => "FLANGE",
      Kind::Hem => "HEM",
      Kind::Fold => "FOLD",
      Kind::ConvertSheet => "CONVERT TO SHEET METAL",
      Kind::Rip => "RIP",
      Kind::Unfold => "UNFOLD",
      Kind::SheetRules => "SHEET METAL RULES",
      Kind::FlatPattern => "FLAT PATTERN",
      Kind::NewComponent => "NEW COMPONENT",
      Kind::Joint => "JOINT",
      Kind::Material => "PHYSICAL MATERIAL",
      Kind::Interference => "INTERFERENCE",
      Kind::Bom => "BILL OF MATERIALS",
      Kind::CenterOfMass => "CENTER OF MASS",
      Kind::SkFillet => "SKETCH FILLET",
      Kind::SkChamfer | Kind::SkChamfer2 | Kind::SkChamferAngle => "SKETCH CHAMFER",
      Kind::SkOffset => "OFFSET",
      Kind::TraceImage => "TRACE IMAGE",
      Kind::SkMirror => "SKETCH MIRROR",
      Kind::SkCircPattern => "CIRCULAR PATTERN",
      Kind::SkRectPattern => "RECTANGULAR PATTERN",
      Kind::Project => "PROJECT",
      Kind::Refold => "REFOLD FACES",
      Kind::DriveJoint => "DRIVE JOINTS",
      Kind::MotionStudy => "MOTION STUDY",
      Kind::CopyComponent => "DUPLICATE COMPONENT",
      Kind::NewDrawing => "CREATE DRAWING",
      Kind::Form(f) => f.title,
      Kind::Section => "SECTION ANALYSIS",
      Kind::Analysis("zebra") => "ZEBRA ANALYSIS",
      Kind::Analysis("draft") => "DRAFT ANALYSIS",
      Kind::Analysis("accessibility") => "ACCESSIBILITY ANALYSIS",
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
      Kind::Loft => Icon::Loft,
      Kind::Sweep => Icon::Sweep,
      Kind::Hole => Icon::Hole,
      Kind::PressPull => Icon::PressPull,
      Kind::Draft => Icon::Draft,
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
      Kind::Joint => Icon::JointOrigin,
      Kind::Material => Icon::PhysMaterial,
      Kind::Interference => Icon::Interference,
      Kind::Bom => Icon::Bom,
      Kind::Form(f) => f.icon,
      Kind::Section => Icon::Section,
      Kind::Analysis("zebra") => Icon::Zebra,
      Kind::Analysis("draft") => Icon::DraftAnalysis,
      Kind::Analysis("accessibility") => Icon::Accessibility,
      Kind::Analysis(_) => Icon::CurvMap,
      Kind::CenterOfMass => Icon::CenterOfMass,
      Kind::SkFillet => Icon::SkFillet,
      Kind::SkChamfer | Kind::SkChamfer2 | Kind::SkChamferAngle => Icon::SkChamfer,
      Kind::SkOffset => Icon::SkOffset,
      Kind::TraceImage => Icon::TraceImage,
      Kind::SkMirror => Icon::SkMirror,
      Kind::SkCircPattern => Icon::SkCircPattern,
      Kind::SkRectPattern => Icon::SkRectPattern,
      Kind::Project => Icon::Project,
      Kind::Refold => Icon::Refold,
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
      Kind::SplitBody => ("Split Body", "Split bodies along a plane or planar face"),
      Kind::Flange => ("Flange", "Sheet metal from a profile, an open path or sheet edges"),
      Kind::Hem => ("Hem", "Fold sheet edges back on themselves"),
      Kind::Fold => ("Fold", "Bend a flat sheet along a sketch line"),
      Kind::ConvertSheet => ("Convert to Sheet Metal", "Turn a thin solid into sheet metal"),
      Kind::Rip => ("Rip", "Slit a solid along edges so it can unfold"),
      Kind::Unfold => ("Unfold", "Flatten a sheet metal body onto a face"),
      Kind::SheetRules => ("Sheet Metal Rules", "Thickness, bend radius and reliefs of sheet metal"),
      Kind::FlatPattern => ("Create Flat Pattern", "Flat pattern size and bends, and DXF export"),
      Kind::NewComponent => ("New Component", "Make a component from the selected bodies"),
      Kind::Joint => ("Joint", "Join two components at a face or edge of each"),
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
      Kind::Analysis(_) => ("Curvature Map Analysis", "Colour by how sharply surfaces curve"),
      Kind::CenterOfMass => ("Center of Mass", "Centre of mass of the selected (or all) bodies"),
      Kind::SkFillet => ("Fillet", "Round the corner where two lines or arcs meet"),
      Kind::SkChamfer => ("Equal Distance Chamfer", "Bevel the corner where two lines meet"),
      Kind::SkChamfer2 => ("Distance-Distance Chamfer", "Bevel a corner by two distances"),
      Kind::SkChamferAngle => {
        ("Distance and Angle Chamfer", "Bevel a corner by a distance along the first line and an angle from it")
      }
      Kind::SkOffset => ("Offset", "Offset a chain of curves (O)"),
      Kind::TraceImage => ("Trace Image", "Trace a photo, logo or scan into closed sketch curves"),
      Kind::SkMirror => ("Mirror", "Mirror sketch curves across a line"),
      Kind::SkCircPattern => ("Circular Pattern", "Copy sketch curves around a point"),
      Kind::SkRectPattern => ("Rectangular Pattern", "Copy sketch curves in rows and columns"),
      Kind::Project => ("Project", "Project a body's or face's edges into the sketch"),
      Kind::Refold => ("Refold Faces", "Fold an unfolded sheet back up, keeping cuts made while flat"),
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
        | Kind::Draft
        | Kind::Hem
        | Kind::Rip
        | Kind::ConvertSheet
        | Kind::Unfold
        | Kind::Joint
        | Kind::Refold
    )
  }

  /// Dialogs that only show information (no OK, nothing applied).
  pub(crate) fn reports(self) -> bool {
    matches!(
      self,
      Kind::Inspect
        | Kind::Appearance
        | Kind::FlatPattern
        | Kind::Interference
        | Kind::Bom
        | Kind::MotionStudy
        | Kind::CenterOfMass
        | Kind::Section
        | Kind::Analysis(_)
    )
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

/// The world plane a dialog's plane value ("xy", {"feature"}, {"body", "face"}) names.
fn plane_of(s: &Session, v: &Value) -> Option<fox_engine::kernel::Plane> {
  use fox_engine::kernel::Plane;
  if let Some(n) = v.as_str() {
    return Some(match n {
      "xz" => Plane::XZ,
      "yz" => Plane::YZ,
      _ => Plane::XY,
    });
  }
  if let Some(f) = v.get("feature").and_then(Value::as_u64) {
    return s.model.planes.get(&FeatureId(f as u32)).copied();
  }
  let body = BodyId(v.get("body")?.as_u64()? as u32);
  let face = v.get("face")?.as_u64()? as u32;
  let f = s.model.body(body)?.faces().iter().find(|f| f.id == face).copied()?;
  fox_engine::model::regen::sketch_plane_on(f.normal, f.origin).ok()
}

const OPS: [(&str, &str); 4] = [("new_body", "New body"), ("join", "Join"), ("cut", "Cut"), ("intersect", "Intersect")];
const REVOLVE_DIRECTIONS: [(&str, &str); 3] =
  [("one_side", "One side"), ("two_sides", "Two sides"), ("symmetric", "Symmetric")];

pub struct Dialog {
  pub kind: Kind,
  /// Feature being edited (timeline double-click); None while creating.
  pub editing: Option<FeatureId>,
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
  /// Joint: the saved joint origin (by name) used instead of a pick on each side; empty = the pick.
  saved: [String; 2],
  /// Trace Image's settings and last result.
  pub trace: Option<Box<crate::trace::TraceState>>,
  /// Where the viewport manipulators sit (see [`manip`]).
  manip: manip::Cache,
  /// Measure: the selection filter as it was before the dialog changed it.
  saved_filter: Option<(crate::select::Priority, bool)>,
  /// Appearance applies to faces rather than bodies.
  appearance_faces: bool,
  /// Live preview (see [`live`]).
  live: live::Live,
  /// Sketch Offset: the distance and side follow the cursor until a click places the offset.
  follow: bool,
}

const CORNERS: [(&str, &str); 5] =
  [("miter", "Miter"), ("open", "Open"), ("closed", "Closed"), ("overlap", "Overlap"), ("butt", "Butt")];
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
      saved: [String::new(), String::new()],
      trace: (kind == Kind::TraceImage).then(Box::default),
      manip: manip::Cache::default(),
      saved_filter: None,
      appearance_faces: false,
      live: live::Live::default(),
      follow: false,
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
    self.kind.picks_model() || (self.kind == Kind::Flange && self.choice == "edge")
  }

  /// Opens a dialog for a new feature, capturing the current selection.
  pub fn open(app: &mut App, kind: Kind) -> Dialog {
    // Appearance (A) opens Fusion's panel of swatches rather than its command form
    let kind = match kind {
      Kind::Form(f) if std::ptr::eq(f, &crate::forms::APPEARANCE) => Kind::Appearance,
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
      d.sketch = s.edit_sketch.or(app.last_sketch).or_else(|| last_unconsumed_sketch(s));
      if let Some(sk) = d.sketch
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
      Kind::Fillet => d.radius = "2 mm".into(),
      Kind::Chamfer => d.radius = "1 mm".into(),
      Kind::Shell => d.radius = "2 mm".into(),
      Kind::Cylinder => {
        d.a = "10 mm".into();
        d.b = "20 mm".into();
      }
      Kind::Sphere => d.a = "10 mm".into(),
      Kind::Torus => {
        d.a = "20 mm".into();
        d.b = "5 mm".into();
      }
      Kind::Revolve => d.axis = "z".into(),
      Kind::Mirror => {
        d.join = false;
        d.choice = "bodies";
      }
      Kind::RectPattern | Kind::CircPattern => d.join = false,
      Kind::Combine => d.op = "join",
      Kind::Loft => {
        d.sections = sel_sections(s);
      }
      Kind::Sweep => {
        d.path = s
          .design
          .features
          .iter()
          .take(s.design.active_len())
          .rev()
          .find(|f| matches!(f.kind, FeatureKind::Sketch { .. }) && Some(f.id) != d.sketch)
          .map(|f| f.id);
      }
      Kind::Hole => {
        d.choice = "simple";
        d.a = "5 mm".into();
        d.b = "10 mm".into();
        d.c = "9 mm".into();
        d.d = "3 mm".into();
        d.e = "90 deg".into();
        d.op = "cut";
      }
      Kind::PressPull => d.distance = "2 mm".into(),
      Kind::Draft => {
        d.angle = "3 deg".into();
        d.plane = json!("xy");
        d.plane_label = "XY plane".into();
      }
      Kind::SplitBody => d.bodies = sel_only_bodies(s),
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
      Kind::Flange => {
        // edges picked → edge flange; a sketch of open curves only → contour flange
        let open_only = d.sketch.and_then(|sk| s.model.sketches.get(&sk)).is_some_and(|r| r.profiles.is_empty());
        d.choice = if !d.edges.is_empty() {
          "edge"
        } else if open_only {
          "contour"
        } else {
          "base"
        };
        d.distance = "10 mm".into();
        d.angle = "90 deg".into();
        d.position = "adjacent";
        d.op = "new_body";
      }
      Kind::Hem => {
        d.distance = "5 mm".into();
        d.distance2 = "0.5 mm".into();
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
      }
      Kind::Joint => {
        d.choice = "rigid";
        d.angle = "0 deg".into();
        d.distance = "0 mm".into();
        d.origins = sel_origins(s);
        d.e = "2 mm".into();
      }
      Kind::Material => d.name = "Steel".into(),
      Kind::SkFillet => d.radius = "2 mm".into(),
      Kind::SkChamfer | Kind::SkChamfer2 | Kind::SkChamferAngle => {
        d.distance = "2 mm".into();
        d.distance2 = "3 mm".into();
        d.angle = "45 deg".into();
      }
      Kind::SkOffset => {
        d.distance = "2 mm".into();
        d.follow = true;
      }
      Kind::SkCircPattern => {
        d.count = "6".into();
        d.angle = "360 deg".into();
      }
      Kind::SkRectPattern => {
        d.count = "3".into();
        d.count2 = "2".into();
      }
      Kind::Project => d.flag = true,
      Kind::Extrude => {
        // extruding from a face sketch defaults to Join, like Fusion
        if let Some(sk) = d.sketch
          && let Some(FeatureKind::Sketch { plane: PlaneRef::Face { .. }, .. }) = s.design.feature(sk).map(|f| &f.kind)
        {
          d.op = "join";
        }
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
    ) {
      d.curves = sel_curves(s);
      if kind == Kind::SkMirror {
        d.line = mirror_line(s, &d.curves);
        d.curves.retain(|c| Some(*c) != d.line);
      }
    }
    d.comps = d.bodies.iter().map(|b| s.design.component_of(BodyId(*b)).map(|c| c.id.0)).collect();
    if kind == Kind::Joint {
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
    if kind == Kind::Section && app.section.is_none() {
      // a picked planar face, else the front plane through the middle of the model
      let face = app.s.selection.iter().find_map(|x| match x {
        Sel::Face { body, face } if is_planar(&app.s, *body, *face) => Some(json!({"body": body.0, "face": face})),
        _ => None,
      });
      let mid = app.s.model.bounds().map(|(a, b)| -(a.y + b.y) / 2.0).unwrap_or(0.0);
      app.section = Some(match face {
        Some(f) => json!({"plane": f, "offset": 0.0, "flip": true}),
        None => json!({"plane": "xz", "offset": (mid * 1000.0).round() / 1000.0, "flip": false}),
      });
    }
    if let Kind::Analysis(a) = kind {
      app.analysis = Some(json!({"analysis": a, "pull": "z", "angle": 1.0}));
    }
    d.dirty = !d.picks();
    d
  }

  /// Opens the dialog for an existing feature (double-click in the timeline).
  pub fn edit(app: &App, id: FeatureId) -> Option<Dialog> {
    let f = app.s.design.feature(id)?;
    let mut d = match &f.kind {
      FeatureKind::Extrude { sketch, extent, operation, .. } => {
        let mut d = Dialog::blank(Kind::Extrude);
        d.sketch = Some(*sketch);
        d.distance = extent.distance.clone();
        d.distance2 = extent.distance2.clone();
        d.flip = extent.flip;
        d.extent = match extent.kind {
          ExtentKind::OneSide => "one_side",
          ExtentKind::Symmetric => "symmetric",
          ExtentKind::TwoSides => "two_sides",
          ExtentKind::ThroughAll => "through_all",
          ExtentKind::ThroughAllBoth => "through_all_both",
          ExtentKind::ToFace | ExtentKind::ToBody => "to_object",
        };
        if let ExtentKind::ToFace | ExtentKind::ToBody = extent.kind {
          let kind = if extent.kind == ExtentKind::ToBody { "to_body" } else { "to_face" };
          d.load_object(kind, serde_json::to_value(&extent.to).unwrap_or_default());
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
        d.axis = match axis {
          AxisRef::X => "x".into(),
          AxisRef::Y => "y".into(),
          AxisRef::Z => "z".into(),
          AxisRef::SketchLine { curve } | AxisRef::SketchCurve { curve, .. } => format!("line:{}", curve.0),
          AxisRef::Edge { .. } => "edge".into(),
          AxisRef::Construction { feature } => format!("axis:{}", feature.0),
        };
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
      FeatureKind::Primitive { shape, at, operation, .. } => {
        let (kind, a, b, c) = match shape {
          Primitive::Box { width, depth, height } => (Kind::Box, width.clone(), depth.clone(), height.clone()),
          Primitive::Cylinder { radius, height } => (Kind::Cylinder, radius.clone(), height.clone(), String::new()),
          Primitive::Sphere { radius } => (Kind::Sphere, radius.clone(), String::new(), String::new()),
          Primitive::Torus { major, minor } => (Kind::Torus, major.clone(), minor.clone(), String::new()),
        };
        let mut d = Dialog::blank(kind);
        d.a = a;
        d.b = b;
        d.c = c;
        d.at = [fox_engine::sketch::Dim::new(at[0]).expr, fox_engine::sketch::Dim::new(at[1]).expr];
        d.op = op_str(*operation);
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
      FeatureKind::Move { translate, angle, .. } => {
        let mut d = Dialog::blank(Kind::Move);
        d.translate = translate.clone();
        d.angle = angle.clone();
        d
      }
      kind @ (FeatureKind::Emboss { .. } | FeatureKind::BoundaryFill { .. } | FeatureKind::ReplaceFace { .. }) => {
        solid::edit_form(kind)?
      }
      _ => return None,
    };
    d.editing = Some(id);
    Some(d)
  }

  /// The command (name, params) this dialog applies.
  fn command(&self) -> Option<(&'static str, Value)> {
    if let Some(id) = self.editing {
      return Some(("feature.edit", json!({"feature": id.0, "set": self.edit_set()?})));
    }
    let op = self.op;
    Some(match self.kind {
      Kind::Form(f) => crate::forms::engine_command(f, self.form.params(f)),
      Kind::PickPlane
      | Kind::Inspect
      | Kind::Appearance
      | Kind::FlatPattern
      | Kind::Interference
      | Kind::Bom
      | Kind::CenterOfMass
      | Kind::Section
      | Kind::Analysis(_) => {
        return None;
      }
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
      Kind::Hole => {
        let mut p = json!({
          "sketch": self.sketch?.0, "kind": self.choice, "diameter": self.a, "flip": self.flip,
        });
        if self.flag {
          p["through_all"] = json!(true);
        } else {
          p["depth"] = json!(self.b);
        }
        match self.choice {
          "counterbore" => {
            p["head_diameter"] = json!(self.c);
            p["head_depth"] = json!(self.d);
          }
          "countersink" => {
            p["head_diameter"] = json!(self.c);
            p["head_angle"] = json!(self.e);
          }
          _ => {}
        }
        ("hole", p)
      }
      Kind::PressPull => ("press_pull", json!({"faces": self.faces, "distance": self.distance})),
      Kind::Draft => {
        ("draft", json!({"faces": self.faces, "plane": self.plane, "angle": self.angle, "flip": self.flip}))
      }
      Kind::SplitBody if self.target.is_some() => {
        let tool = self.target?;
        let bodies: Vec<u32> = self.bodies.iter().copied().filter(|b| *b != tool).collect();
        let mut p = json!({"tool": tool, "remove_tool": self.flag});
        if !bodies.is_empty() {
          p["bodies"] = json!(bodies);
        }
        ("split_by_body", p)
      }
      Kind::SplitBody => {
        let mut p = json!({"plane": self.plane});
        if !self.bodies.is_empty() {
          p["bodies"] = json!(self.bodies);
        }
        ("split", p)
      }
      Kind::Flange => match self.choice {
        "edge" => (
          "sheet.edge_flange",
          json!({
            "edges": self.edges, "height": self.distance, "angle": self.angle, "flip": self.flip,
            "position": self.position, "corners": self.corners,
          }),
        ),
        "contour" => {
          let mut p =
            json!({"sketch": self.sketch?.0, "distance": self.distance, "symmetric": self.flag, "flip": self.flip});
          self.put_rule(&mut p);
          ("sheet.contour_flange", p)
        }
        _ => {
          let mut p = json!({"sketch": self.sketch?.0, "profiles": self.profiles, "flip": self.flip, "operation": op});
          self.put_rule(&mut p);
          ("sheet.flange", p)
        }
      },
      Kind::Hem => {
        ("sheet.hem", json!({"edges": self.edges, "length": self.distance, "gap": self.distance2, "flip": self.flip}))
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
      Kind::Unfold => {
        let mut p = json!({});
        if let Some(b) = self.bodies.first() {
          p["body"] = json!(b);
        }
        if let Some(f) = self.faces.first() {
          p["face"] = f.clone();
        }
        ("sheet.unfold", p)
      }
      Kind::SheetRules => (
        "sheet.rule",
        json!({
          "name": self.name, "thickness": self.a, "bend_radius": self.b, "k_factor": self.c,
          "relief": self.position, "relief_width": self.d, "relief_depth": self.e, "gap": self.distance2,
          "corner_relief": self.corner_relief, "corner_relief_size": self.corner_relief_size,
        }),
      ),
      Kind::NewComponent => ("component.create", json!({"name": self.name, "bodies": self.bodies})),
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
      Kind::Material => {
        ("component.material", json!({"component": self.comps.iter().flatten().next()?, "material": self.name}))
      }
      Kind::SkFillet => ("sketch.fillet", json!({"curves": self.curves, "radius": self.radius})),
      Kind::SkChamfer => ("sketch.chamfer", json!({"curves": self.curves, "distance": self.distance})),
      Kind::SkChamfer2 => {
        ("sketch.chamfer", json!({"curves": self.curves, "distance": self.distance, "distance2": self.distance2}))
      }
      Kind::SkChamferAngle => {
        ("sketch.chamfer", json!({"curves": self.curves, "distance": self.distance, "angle": self.angle}))
      }
      Kind::SkOffset => ("sketch.offset", json!({"curves": self.curves, "distance": self.distance})),
      Kind::TraceImage => ("sketch.trace", self.trace.as_ref()?.params()?),
      Kind::SkMirror => ("sketch.mirror", json!({"curves": self.curves, "line": self.line?})),
      Kind::SkCircPattern => (
        "sketch.pattern_circ",
        json!({"curves": self.curves, "center": self.at_pt()?, "count": self.count.trim().parse::<u64>().ok()?, "angle": self.angle,
          "symmetric": self.flag}),
      ),
      Kind::SkRectPattern => {
        let v = |a: &str| if a == "y" { [0.0, 1.0] } else { [1.0, 0.0] };
        let mut p = json!({
          "curves": self.curves, "direction": v(&self.dir), "count": self.count.trim().parse::<u64>().ok()?,
          "spacing": self.spacing, "symmetric": self.symmetric[0],
          "distance_type": if self.pattern_extent { "extent" } else { "spacing" },
        });
        if self.use_dir2 {
          p["direction2"] = json!(v(&self.dir2));
          p["count2"] = json!(self.count2.trim().parse::<u64>().ok()?);
          p["spacing2"] = json!(self.spacing2);
          p["symmetric2"] = json!(self.symmetric[1]);
        }
        ("sketch.pattern_rect", p)
      }
      Kind::Refold => {
        let mut p = json!({});
        if let Some(b) = self.bodies.first() {
          p["body"] = json!(b);
        }
        ("sheet.refold", p)
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
      Kind::Extrude => (
        "extrude",
        self.with_extrude_rows(self.with_object(json!({
          "sketch": self.sketch?.0, "profiles": self.profiles, "distance": self.distance, "distance2": self.distance2,
          "extent": self.extent, "flip": self.flip, "operation": op,
        }))?)?,
      ),
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
      Kind::Chamfer => ("chamfer", self.chamfer_params(json!({"edges": self.edges}), false)),
      Kind::Shell => ("shell", self.shell_params()),
      Kind::Box => (
        "box",
        json!({"plane": self.plane, "at": self.at_pt()?, "width": self.a, "depth": self.b, "height": self.c, "operation": op}),
      ),
      Kind::Cylinder => (
        "cylinder",
        json!({"plane": self.plane, "at": self.at_pt()?, "radius": self.a, "height": self.b, "operation": op}),
      ),
      Kind::Sphere => ("sphere", json!({"plane": self.plane, "at": self.at_pt()?, "radius": self.a, "operation": op})),
      Kind::Torus => {
        ("torus", json!({"plane": self.plane, "at": self.at_pt()?, "major": self.a, "minor": self.b, "operation": op}))
      }
      Kind::OffsetPlane => ("plane.offset", json!({"plane": self.plane, "offset": self.distance})),
      Kind::Mirror if self.choice == "features" => {
        ("mirror", json!({"plane": self.plane, "features": self.mirror_features}))
      }
      Kind::Mirror => ("mirror", json!({"plane": self.plane, "bodies": self.bodies, "join": self.join})),
      Kind::RectPattern => {
        let mut p = json!({
          "bodies": self.bodies, "direction": axis_vec(&self.dir), "count": self.count, "spacing": self.spacing, "join": self.join,
          "symmetric": self.symmetric[0], "distance_type": if self.pattern_extent { "extent" } else { "spacing" },
        });
        if self.use_dir2 {
          p["direction2"] = json!(axis_vec(&self.dir2));
          p["count2"] = json!(self.count2);
          p["spacing2"] = json!(self.spacing2);
          p["symmetric2"] = json!(self.symmetric[1]);
        }
        ("pattern.rectangular", p)
      }
      Kind::CircPattern => (
        "pattern.circular",
        json!({
          "bodies": self.bodies, "axis": self.axis, "count": self.count, "angle": self.angle,
          "symmetric": self.symmetric[0], "join": self.join,
        }),
      ),
      Kind::Combine => {
        let (target, tools) = self.bodies.split_first()?;
        ("combine", json!({"target": target, "tools": tools, "operation": op, "keep_tools": self.keep_tools}))
      }
      Kind::Move => {
        let mut p = json!({"bodies": self.bodies, "translate": self.translate});
        if !matches!(self.angle.trim(), "" | "0" | "0 deg") {
          p["axis"] = json!(self.axis);
          p["angle"] = json!(self.angle);
        }
        ("move", p)
      }
    })
  }

  /// Fields for `feature.edit` when editing an existing feature.
  fn edit_set(&self) -> Option<Value> {
    Some(match self.kind {
      Kind::Extrude => json!({
        "extent": self.edit_extrude_rows(self.edit_extent(
          json!({"kind": self.extent, "distance": self.distance, "distance2": self.distance2, "flip": self.flip}),
        )),
        "operation": self.op,
      }),
      Kind::Revolve => {
        let two = self.extent == "two_sides";
        json!({"angle": self.revolve_angle(), "symmetric": self.extent == "symmetric",
          "angle2": if two { json!(self.angle2) } else { Value::Null }, "operation": self.op})
      }
      Kind::Fillet => self.fillet_edit_set(),
      Kind::Chamfer => self.chamfer_params(json!({}), true),
      Kind::Shell => self.shell_edit_set(),
      Kind::Box => {
        json!({"shape": {"type": "box", "width": self.a, "depth": self.b, "height": self.c}, "at": self.at_pt()?, "operation": self.op})
      }
      Kind::Cylinder => {
        json!({"shape": {"type": "cylinder", "radius": self.a, "height": self.b}, "at": self.at_pt()?, "operation": self.op})
      }
      Kind::Sphere => json!({"shape": {"type": "sphere", "radius": self.a}, "at": self.at_pt()?, "operation": self.op}),
      Kind::Torus => {
        json!({"shape": {"type": "torus", "major": self.a, "minor": self.b}, "at": self.at_pt()?, "operation": self.op})
      }
      Kind::OffsetPlane => json!({"offset": self.distance}),
      Kind::RectPattern => {
        json!({"count": self.count, "spacing": self.spacing, "count2": self.count2, "spacing2": self.spacing2, "join": self.join,
          "symmetric": self.symmetric[0], "symmetric2": self.symmetric[1], "extent": self.pattern_extent})
      }
      Kind::CircPattern => {
        json!({"count": self.count, "angle": self.angle, "symmetric": self.symmetric[0], "join": self.join})
      }
      Kind::Move => json!({"translate": self.translate, "angle": self.angle}),
      Kind::Form(f) => solid::form_edit_set(f, &self.form.values),
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

  /// Withdraws the current preview (if any) and applies the dialog's command.
  pub fn preview(&mut self, s: &mut Session) {
    self.dirty = false;
    let selection = s.selection.clone();
    self.withdraw(s);
    if self.kind.reports() {
      self.refresh_report(s);
      return;
    }
    let Some((cmd, params)) = self.command() else {
      return;
    };
    if !self.ready() {
      return;
    }
    self.live_applying(s, cmd, &params);
    let result = s.execute(cmd, &params);
    if let Some(t) = self.trace.as_mut() {
      t.result = result.as_ref().ok().cloned();
    }
    match result {
      Ok(v) => {
        self.applied = true;
        self.error = v.get("error").and_then(Value::as_str).map(str::to_string);
      }
      Err(e) => self.error = Some(e.0),
    }
    if !self.picks() {
      s.selection = selection;
    }
  }

  /// Enough input to try?
  fn ready(&self) -> bool {
    if self.editing.is_some() {
      return true;
    }
    match self.kind {
      Kind::Form(f) => self.form.missing(f).is_none(),
      Kind::Extrude | Kind::Revolve => self.sketch.is_some(),
      Kind::Fillet => self.fillet_ready(),
      Kind::Chamfer => !self.edges.is_empty(),
      Kind::Shell => !self.faces.is_empty() || !self.bodies.is_empty(),
      Kind::Combine => self.bodies.len() >= 2,
      Kind::Mirror if self.choice == "features" => !self.mirror_features.is_empty(),
      Kind::Loft => self.sections.len() >= 2,
      Kind::Sweep => self.sketch.is_some() && self.path.is_some(),
      Kind::Hole | Kind::Fold => self.sketch.is_some(),
      Kind::PressPull | Kind::Draft => !self.faces.is_empty(),
      Kind::Flange => {
        if self.choice == "edge" {
          !self.edges.is_empty()
        } else {
          self.sketch.is_some()
        }
      }
      Kind::Hem | Kind::Rip => !self.edges.is_empty(),
      Kind::ConvertSheet | Kind::NewComponent | Kind::Project => !self.bodies.is_empty(),
      Kind::Joint => {
        let need = self.saved.iter().filter(|n| n.is_empty()).count();
        self.origins.len() >= need && (!self.saved[0].is_empty() || self.comps.first().is_some_and(Option::is_some))
      }
      Kind::Material => self.comps.iter().any(Option::is_some),
      Kind::SkFillet | Kind::SkChamfer | Kind::SkChamfer2 | Kind::SkChamferAngle => self.curves.len() == 2,
      Kind::SkMirror => !self.curves.is_empty() && self.line.is_some(),
      Kind::SkOffset | Kind::SkCircPattern | Kind::SkRectPattern => !self.curves.is_empty(),
      Kind::TraceImage => self.trace.as_ref().is_some_and(|t| t.path.is_some()),
      Kind::DriveJoint | Kind::CopyComponent => self.target.is_some(),
      Kind::NewDrawing => self.view_on.iter().any(|v| *v),
      Kind::DrawingMoveView | Kind::DrawingDimension => self.target.is_some() && self.view.is_some(),
      Kind::DrawingView => self.target.is_some() && (self.choice != "flat" || !self.bodies.is_empty()),
      _ => true,
    }
  }

  fn withdraw(&mut self, s: &mut Session) {
    if self.applied {
      s.undo();
      s.drop_redo();
      self.applied = false;
      if self.picks() && self.editing.is_none() {
        // the picks are valid again on the un-previewed model
        s.selection = self.picked_selection();
      }
    }
  }

  fn picked_selection(&self) -> Vec<Sel> {
    let get = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).map(|x| x as u32);
    let mut out = Vec::new();
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
    if self.kind == Kind::Combine {
      out.extend(self.bodies.iter().map(|b| Sel::Body { body: BodyId(*b) }));
    }
    if self.kind == Kind::Joint {
      out.clear();
      for (b, o) in &self.origins {
        if let Some(i) = o.get("face").and_then(|f| get(f, "face")) {
          out.push(Sel::Face { body: BodyId(*b), face: i });
        } else if let Some(i) = o.get("edge").and_then(|e| get(e, "edge")) {
          out.push(Sel::Edge { body: BodyId(*b), edge: i });
        }
      }
    }
    out
  }

  pub fn cancel(&mut self, s: &mut Session) {
    self.withdraw(s);
  }

  /// Is a model-picking dialog's preview showing? (Viewport clicks withdraw it first.)
  pub fn previewing_model(&self) -> bool {
    self.applied && self.picks() && self.editing.is_none()
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
      Kind::Extrude if matches!(self.extent, "one_side" | "symmetric" | "two_sides") => {
        let r = s.model.sketches.get(&self.sketch?)?;
        let at = match self.profiles.first() {
          Some(p) => *p,
          None => r.profiles.first()?.interior_point(),
        };
        let sign = if self.flip { -1.0 } else { 1.0 };
        Some(Manipulator { origin: r.plane.to_world(at), dir: r.plane.normal(), value: sign * len(&self.distance)? })
      }
      Kind::OffsetPlane => {
        let base = plane_of(s, &self.plane)?;
        // offsets from XZ go along +Y in designs made with XZ_OFFSET_PLUS_Y (see
        // `OriginPlane::offset_sign`)
        let sign = if self.plane.as_str() == Some("xz") {
          fox_engine::model::OriginPlane::Xz.offset_sign(&s.design.behaviours)
        } else {
          1.0
        };
        Some(Manipulator { origin: base.origin, dir: sign * base.normal(), value: len(&self.distance)? })
      }
      _ => self.more_manipulator(s),
    }
  }

  /// Sets the value the drag arrow changed (rounded to `step`) and re-previews.
  pub fn set_manipulator(&mut self, value: f64, step: f64) {
    let v = if step > 0.0 { (value / step).round() * step } else { value };
    match self.kind {
      Kind::Extrude => {
        self.flip = v < 0.0;
        self.distance = format!("{} mm", fox_engine::sketch::Dim::new(v.abs()).expr);
      }
      Kind::OffsetPlane => self.distance = format!("{} mm", fox_engine::sketch::Dim::new(v).expr),
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
      Kind::Extrude | Kind::Revolve | Kind::Flange | Kind::Hole | Kind::Fold => self.sketch.into_iter().collect(),
      Kind::Sweep => self.sketch.into_iter().chain(self.path).collect(),
      Kind::Loft => self.sections.iter().map(|(sk, _)| FeatureId(*sk)).collect(),
      _ => Vec::new(),
    }
  }

  /// Picks up selection changes made in the viewport while the dialog is open.
  fn sync_selection(&mut self, s: &Session) {
    if self.editing.is_some() {
      return;
    }
    match self.kind {
      Kind::Form(f) if !self.applied || !self.picks() => {
        if self.form.sync(f, s, last_unconsumed_sketch(s)) {
          self.dirty = !self.picks();
        }
      }
      Kind::Extrude | Kind::Revolve | Kind::Sweep | Kind::Flange
        if !(self.kind == Kind::Flange && self.choice == "edge") =>
      {
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
        if self.kind == Kind::Extrude {
          self.sync_object(s);
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
        }
      }
      Kind::Chamfer if !self.applied => {
        let e: Vec<Value> = s
          .selection
          .iter()
          .filter_map(
            |x| if let Sel::Edge { body, edge } = x { Some(json!({"body": body.0, "edge": edge})) } else { None },
          )
          .collect();
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
      }
      Kind::Hem | Kind::Rip | Kind::Flange if !self.applied => {
        let e = sel_edges(s);
        if e != self.edges {
          self.edges = e;
        }
      }
      Kind::PressPull | Kind::Draft if !self.applied => {
        let f = sel_faces(s);
        if f != self.faces {
          self.faces = f;
        }
      }
      Kind::Refold if !self.applied => {
        let b = sel_bodies(s);
        if b != self.bodies {
          self.bodies = b;
        }
      }
      Kind::DrawingView => {
        let b = sel_bodies(s);
        if !b.is_empty() && b != self.bodies {
          self.bodies = b;
          self.dirty = self.choice == "flat";
        }
      }
      Kind::ConvertSheet | Kind::Unfold | Kind::Project if !self.applied || !self.picks() => {
        let f: Vec<Value> = sel_faces(s).into_iter().take(1).collect();
        let b = sel_bodies(s);
        if (f != self.faces || b != self.bodies) && (!b.is_empty() || self.kind != Kind::Project) {
          self.faces = f;
          self.bodies = b;
          self.dirty = !self.picks();
        }
      }
      Kind::SplitBody => {
        let b = sel_only_bodies(s);
        if b != self.bodies {
          self.bodies = b;
          self.dirty = true;
        }
      }
      Kind::NewComponent | Kind::Material => {
        let b = sel_bodies(s);
        if b != self.bodies {
          self.comps = b.iter().map(|x| s.design.component_of(BodyId(*x)).map(|c| c.id.0)).collect();
          self.bodies = b;
          self.dirty = true;
        }
      }
      Kind::Joint if !self.applied => {
        let o = sel_origins(s);
        if o != self.origins {
          self.comps = o.iter().map(|(b, _)| s.design.component_of(BodyId(*b)).map(|c| c.id.0)).collect();
          self.origins = o;
        }
      }
      Kind::SkFillet
      | Kind::SkChamfer
      | Kind::SkChamfer2
      | Kind::SkChamferAngle
      | Kind::SkOffset
      | Kind::SkMirror
      | Kind::SkCircPattern
      | Kind::SkRectPattern => {
        let mut c = sel_curves(s);
        if self.kind == Kind::SkMirror {
          c.retain(|x| Some(*x) != self.line);
        }
        if !c.is_empty() && c != self.curves {
          self.curves = c;
          self.dirty = true;
        }
      }
      Kind::FlatPattern => {
        let b = sel_bodies(s);
        if b != self.bodies {
          self.bodies = b;
          self.faces = sel_faces(s).into_iter().take(1).collect();
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
      Kind::Combine | Kind::Mirror | Kind::RectPattern | Kind::CircPattern | Kind::Move
        if !self.applied || !self.picks() =>
      {
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
    self.sync_selection(&app.s);
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
    let mut keep = true;
    // Enter is OK, from one of the dialog's boxes or with nothing focused (as in Fusion); OK can
    // also come from the marking menu
    let dialog_layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("feature-dialog"));
    let mut ok = std::mem::take(&mut self.ok_requested)
      || ctx.input(|i| i.key_pressed(egui::Key::Enter))
        && !self.kind.reports()
        && self.kind != Kind::PickPlane
        && match ctx.memory(|m| m.focused()) {
          None => true,
          Some(id) => ctx.read_response(id).is_some_and(|r| r.layer_id == dialog_layer),
        };
    let mut cancel = false;
    let title =
      if self.editing.is_some() { format!("EDIT {}", self.kind.title()) } else { self.kind.title().to_string() };
    let width = 300.0;
    egui::Area::new(egui::Id::new("feature-dialog"))
      .default_pos(app.canvas.right_top() + egui::vec2(-34.0, 190.0))
      .pivot(egui::Align2::RIGHT_TOP)
      .constrain(true)
      .order(egui::Order::Foreground)
      .show(ctx, |ui| {
        panels::dialog_frame().show(ui, |ui| {
          ui.set_width(width);
          ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
          panels::dialog_header(ui, self.kind.icon(), &title, width);
          egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            egui::Grid::new("dlg").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
              self.fields(app, ui);
            });
            if let Some(e) = &self.error {
              ui.add_space(6.0);
              ui.label(RichText::new(format!("⚠ {e}")).color(theme::ERROR));
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
              if self.kind != Kind::PickPlane && !self.kind.reports() {
                if self.picks() && self.editing.is_none() {
                  self.live_checkbox(ui, &mut app.s);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                  if panels::secondary_button(ui, "Cancel").clicked() {
                    cancel = true;
                  }
                  if panels::primary_button(ui, "OK", self.accent()).clicked() {
                    ok = true;
                  }
                });
              } else {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                  if panels::secondary_button(ui, if self.kind == Kind::PickPlane { "Cancel" } else { "Close" })
                    .clicked()
                  {
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
    if self.dirty {
      self.preview(&mut app.s);
    }
    if ok {
      if !self.applied {
        self.preview(&mut app.s);
      }
      if self.applied && self.error.is_none() {
        keep = false;
        app.s.selection.clear();
        app.status = Some((format!("{} done", self.kind.title().to_lowercase()), false));
        // select the traced shapes (not their holes), ready to extrude
        if let (Some(r), Some(sketch)) = (self.trace.as_ref().and_then(|t| t.result.as_ref()), app.s.edit_sketch) {
          let ink = r["ink_profiles"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect::<Vec<_>>());
          if let Some(ink) = ink.filter(|i| !i.is_empty()) {
            app.s.selection = ink.iter().map(|i| Sel::Profile { sketch, index: *i as usize }).collect();
            app.status = Some((format!("Traced {} shapes, selected: Extrude (E) makes them solid", ink.len()), false));
          }
        }
        if self.kind == Kind::NewDrawing {
          app.drawing = app.s.design.drawings.last().map(|d| d.id);
        }
      } else if !self.applied && self.error.is_none() {
        self.error = Some(self.missing_input().into());
      } else if self.applied {
        // keep the failing feature in the timeline (red), like Fusion
        keep = false;
      }
    }
    if cancel {
      self.cancel(&mut app.s);
      keep = false;
    }
    keep && !self.closed
  }

  /// The command family's accent colour, used by this dialog's chips and OK button.
  fn accent(&self) -> egui::Color32 {
    crate::icons::accent_of(self.kind.icon())
  }

  fn missing_input(&self) -> &'static str {
    match self.kind {
      Kind::Form(f) => self.form.missing(f).unwrap_or("missing input"),
      Kind::Extrude | Kind::Revolve => "select a profile",
      Kind::Fillet => "select edges or faces",
      Kind::Chamfer => "select edges",
      Kind::Shell => "select faces to remove, or a body",
      Kind::Combine => "select a target body and tool bodies",
      Kind::Loft => "select profiles on two or more sketches",
      Kind::Sweep => "select a profile and a path sketch",
      Kind::Hole => "sketch points to drill (Point tool) are needed",
      Kind::Fold => "pick a sketch with a bend line",
      Kind::PressPull | Kind::Draft => "select faces",
      Kind::Flange if self.choice == "edge" => "select sheet edges",
      Kind::Flange => "select a profile",
      Kind::Hem | Kind::Rip => "select edges",
      Kind::ConvertSheet | Kind::NewComponent | Kind::Project => "select a body",
      Kind::Joint => "select a face or edge on the moving component, then one on the fixed one",
      Kind::Material => "select a body of a component",
      Kind::SkFillet => "select two lines or arcs that meet",
      Kind::SkChamfer | Kind::SkChamfer2 | Kind::SkChamferAngle => "select two lines",
      Kind::SkMirror => "select curves and a mirror line",
      Kind::SkOffset | Kind::SkCircPattern | Kind::SkRectPattern => "select sketch curves",
      Kind::TraceImage => "pick an image to trace",
      Kind::DriveJoint => "the design has no joints",
      Kind::CopyComponent => "the design has no components",
      Kind::NewDrawing => "pick at least one view",
      Kind::DrawingMoveView | Kind::DrawingDimension => "pick a view",
      Kind::DrawingView if self.choice == "flat" => "select a sheet metal body",
      _ => "missing input",
    }
  }

  fn fields(&mut self, app: &mut App, ui: &mut egui::Ui) {
    let creating = self.editing.is_none();
    match self.kind {
      // the planes to pick are drawn in the viewport (`viewport::plane_picker`)
      Kind::PickPlane => {
        ui.label(PICK_PLANE_HINT);
        ui.end_row();
      }
      Kind::Extrude => self.extrude_rows(ui),
      Kind::Revolve => {
        ui.label("Profiles");
        let n = self.profiles.len();
        if self.editing.is_some() {
          ui.label("(unchanged)");
        } else if n == 0 {
          sel_chip(ui, self.accent(), "Select (all regions)", false);
        } else {
          sel_chip(ui, self.accent(), &format!("{n} selected"), true);
        }
        ui.end_row();
        ui.label("Axis");
        let lines = self.sketch_lines(app);
        let current = self.axis.clone();
        egui::ComboBox::from_id_salt("axis")
          .selected_text(axis_label(&current))
          .show_ui(ui, |ui| {
            for a in ["x", "y", "z"] {
              if ui.selectable_label(current == a, axis_label(a)).clicked() {
                self.axis = a.into();
                self.dirty = true;
              }
            }
            for (id, label) in lines {
              let key = format!("line:{id}");
              if ui.selectable_label(current == key, label).clicked() {
                self.axis = key;
                self.dirty = true;
              }
            }
            // construction axes, and an edge picked in the viewport
            for id in app.s.model.axes.keys() {
              let key = format!("axis:{}", id.0);
              let name = app.s.design.feature(*id).map(|f| f.name.clone()).unwrap_or_else(|| axis_label(&key));
              if ui.selectable_label(current == key, name).clicked() {
                self.axis = key;
                self.dirty = true;
              }
            }
            if !self.solid.axis_edge.is_null() && ui.selectable_label(current == "edge", axis_label("edge")).clicked() {
              self.axis = "edge".into();
              self.dirty = true;
            }
          })
          .response
          .on_hover_text("Or pick a straight edge, sketch line or construction axis in the canvas");
        ui.end_row();
        ui.label("Direction");
        self.combo(ui, "rev_dir", &REVOLVE_DIRECTIONS, true);
        ui.end_row();
        self.revolve_type_rows(ui);
        if self.extent == "two_sides" {
          ui.label("Angle 2");
          if value_edit(ui, &mut self.angle2).changed() {
            self.dirty = true;
          }
          ui.end_row();
        }
        ui.label("Operation");
        self.combo(ui, "op", &OPS, false);
        ui.end_row();
      }
      Kind::Fillet => {
        self.fillet_rows(app, ui, creating);
        self.setback_row(ui, creating);
      }
      Kind::Chamfer => {
        let n = self.edges.len();
        ui.label("Edges");
        if creating {
          sel_chip(ui, self.accent(), &if n == 0 { "Select".to_string() } else { format!("{n} selected") }, n > 0);
        } else {
          ui.label(format!("{n} (unchanged)"));
        }
        ui.end_row();
        self.chamfer_rows(ui, creating);
      }
      Kind::Shell => self.shell_rows(ui, creating),
      Kind::Box | Kind::Cylinder | Kind::Sphere | Kind::Torus => {
        ui.label("Plane");
        ui.label(&self.plane_label);
        ui.end_row();
        let labels: &[&str] = match self.kind {
          Kind::Box => &["Width (X)", "Depth (Y)", "Height"],
          Kind::Cylinder => &["Radius", "Height"],
          Kind::Sphere => &["Radius"],
          _ => &["Major radius", "Minor radius"],
        };
        for (i, l) in labels.iter().enumerate() {
          ui.label(*l);
          let f = match i {
            0 => &mut self.a,
            1 => &mut self.b,
            _ => &mut self.c,
          };
          if value_edit(ui, f).changed() {
            self.dirty = true;
          }
          ui.end_row();
        }
        ui.label("Position X, Y");
        ui.horizontal(|ui| {
          for v in self.at.iter_mut() {
            if ui.add(egui::TextEdit::singleline(v).desired_width(64.0)).changed() {
              self.dirty = true;
            }
          }
        });
        ui.end_row();
        ui.label("Operation");
        self.combo(ui, "op", &OPS, false);
        ui.end_row();
      }
      Kind::OffsetPlane | Kind::Mirror => {
        ui.label("Plane");
        if creating {
          self.plane_choice(ui);
        } else {
          ui.label("(unchanged)");
        }
        ui.end_row();
        if self.kind == Kind::OffsetPlane {
          ui.label("Offset");
          self.text(ui, true);
          ui.end_row();
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
      Kind::RectPattern | Kind::CircPattern => {
        ui.label("Bodies");
        sel_chip(ui, self.accent(), &self.bodies_label(), !self.bodies.is_empty());
        ui.end_row();
        if self.kind == Kind::RectPattern {
          ui.label("Direction");
          self.axis_combo(ui, "dir1", true, &[]);
          ui.end_row();
          ui.label("Count");
          if value_edit(ui, &mut self.count).changed() {
            self.dirty = true;
          }
          ui.end_row();
          ui.label("Spacing");
          if value_edit(ui, &mut self.spacing).changed() {
            self.dirty = true;
          }
          ui.end_row();
          ui.label("");
          if ui.checkbox(&mut self.pattern_extent, "Spacing is the extent").changed() {
            self.dirty = true;
          }
          ui.end_row();
          ui.label("");
          if ui.checkbox(&mut self.symmetric[0], "Symmetric").changed() {
            self.dirty = true;
          }
          ui.end_row();
          ui.label("");
          if ui.checkbox(&mut self.use_dir2, "Second direction").changed() {
            self.dirty = true;
          }
          ui.end_row();
          if self.use_dir2 {
            ui.label("Direction 2");
            self.axis_combo(ui, "dir2", false, &[]);
            ui.end_row();
            ui.label("Count 2");
            if value_edit(ui, &mut self.count2).changed() {
              self.dirty = true;
            }
            ui.end_row();
            ui.label("Spacing 2");
            if value_edit(ui, &mut self.spacing2).changed() {
              self.dirty = true;
            }
            ui.end_row();
            ui.label("");
            if ui.checkbox(&mut self.symmetric[1], "Symmetric 2").changed() {
              self.dirty = true;
            }
            ui.end_row();
          }
        } else {
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
        }
        ui.label("");
        if ui.checkbox(&mut self.join, "Join instances").changed() {
          self.dirty = true;
        }
        ui.end_row();
      }
      Kind::Combine => {
        ui.label("Target");
        match self.bodies.first() {
          Some(b) => sel_chip(ui, self.accent(), &app.s.body_name(BodyId(*b)), true),
          None => sel_chip(ui, self.accent(), "Select", false),
        };
        ui.end_row();
        ui.label("Tools");
        ui.label(format!("{}", self.bodies.len().saturating_sub(1)));
        ui.end_row();
        ui.label("Operation");
        self.combo(ui, "op", &OPS[1..], false);
        ui.end_row();
        ui.label("");
        ui.checkbox(&mut self.keep_tools, "Keep tools");
        ui.end_row();
      }
      Kind::Move => {
        ui.label("Bodies");
        sel_chip(ui, self.accent(), &self.bodies_label(), !self.bodies.is_empty());
        ui.end_row();
        for (i, l) in ["X distance", "Y distance", "Z distance"].iter().enumerate() {
          ui.label(*l);
          if value_edit(ui, &mut self.translate[i]).changed() {
            self.dirty = true;
          }
          ui.end_row();
        }
        ui.label("Rotate about");
        self.axis_combo(ui, "axis", false, &app.s.model.axes.keys().map(|f| f.0).collect::<Vec<_>>());
        ui.end_row();
        ui.label("Angle");
        if value_edit(ui, &mut self.angle).changed() {
          self.dirty = true;
        }
        ui.end_row();
      }
      Kind::Inspect => self.inspect(app, ui),
      Kind::Appearance => self.appearance(app, ui),
      Kind::Section => section_fields(app, ui),
      Kind::Analysis(kind) => analysis_fields(app, ui, kind),
      Kind::Form(f) => {
        let mut dirty = false;
        self.form_fields(f, app, ui, &mut dirty);
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
    for (i, input) in f.inputs.iter().enumerate() {
      let v = self.form.values[i].clone();
      match input {
        Input::Faces { label, .. }
        | Input::FacesOpt { label, .. }
        | Input::Edges { label, .. }
        | Input::Curves { label, .. } => {
          ui.label(*label);
          let n = v.as_array().map(Vec::len).unwrap_or(0);
          sel_chip(ui, accent, &if n == 0 { "Select".to_string() } else { format!("{n} selected") }, n > 0);
          ui.end_row();
        }
        Input::Bodies { label, .. } => {
          ui.label(*label);
          let n = v.as_array().map(Vec::len).unwrap_or(0);
          let text = match n {
            0 => "Select".to_string(),
            1 => v[0].as_u64().map(|b| s.body_name(BodyId(b as u32))).unwrap_or_default(),
            n => format!("{n} bodies"),
          };
          sel_chip(ui, accent, &text, n > 0);
          ui.end_row();
        }
        Input::Face { label, .. } | Input::Edge { label, .. } => {
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
          Self::field(ui, label, &mut t, dirty);
          self.form.values[i] = json!(t);
        }
        Input::Check { label, .. } => {
          let mut b = v.as_bool().unwrap_or(false);
          Self::check(ui, label, &mut b, dirty);
          self.form.values[i] = json!(b);
        }
        Input::Choice { label, options, .. } => {
          let cur = v.as_str().unwrap_or("");
          let mut sel: &'static str = options.iter().find(|(k, _)| *k == cur).map(|(k, _)| *k).unwrap_or("");
          Self::choose(ui, label, &format!("form-choice-{i}"), &mut sel, options, dirty);
          self.form.values[i] = json!(sel);
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
      }
    }
  }

  /// Fields of the loft … sketch-modify dialogs.
  fn more_fields(&mut self, app: &mut App, ui: &mut egui::Ui, dirty: &mut bool) {
    let accent = self.accent();
    let chip = |ui: &mut egui::Ui, n: usize, what: &str| {
      let text = if n == 0 { "Select".to_string() } else { format!("{n} {what}") };
      sel_chip(ui, accent, &text, n > 0);
    };
    const HOLES: [(&str, &str); 3] =
      [("simple", "Simple"), ("counterbore", "Counterbore"), ("countersink", "Countersink")];
    const FLANGES: [(&str, &str); 3] =
      [("base", "Base (profile)"), ("edge", "Edge"), ("contour", "Contour (open path)")];
    const POSITIONS: [(&str, &str); 3] = [("adjacent", "Adjacent"), ("inside", "Inside"), ("outside", "Outside")];
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
      Kind::Loft => {
        ui.label("Profiles");
        chip(ui, self.sections.len(), "profiles");
        ui.end_row();
        for (i, (sk, _)) in self.sections.iter().enumerate() {
          ui.label(RichText::new(format!("  Section {}", i + 1)).color(theme::MUTED));
          ui.label(app.s.design.feature(FeatureId(*sk)).map(|f| f.name.clone()).unwrap_or_default());
          ui.end_row();
        }
        Self::check(ui, "Ruled (straight between sections)", &mut self.flag, dirty);
        Self::check(ui, "Closed (back to the first)", &mut self.flag2, dirty);
        self.loft_rows(app, ui, dirty);
        Self::choose(ui, "Operation", "op", &mut self.op, &OPS, dirty);
      }
      Kind::Sweep => {
        ui.label("Profile");
        chip(ui, self.profiles.len(), "selected");
        ui.end_row();
        ui.label("Path");
        let list = Self::sketches(app, self.sketch);
        let cur = self.path.and_then(|p| list.iter().find(|(id, _)| *id == p)).map(|(_, n)| n.clone());
        egui::ComboBox::from_id_salt("path").selected_text(cur.unwrap_or("Pick a sketch".into())).width(140.0).show_ui(
          ui,
          |ui| {
            for (id, n) in &list {
              if ui.selectable_label(self.path == Some(*id), n).clicked() {
                self.path = Some(*id);
                *dirty = true;
              }
            }
          },
        );
        ui.end_row();
        self.sweep_rows(app, ui, dirty);
        Self::choose(ui, "Operation", "op", &mut self.op, &OPS, dirty);
      }
      Kind::Hole => {
        ui.label("Sketch");
        ui.label(self.sketch.and_then(|s| app.s.design.feature(s)).map(|f| f.name.clone()).unwrap_or("none".into()));
        ui.end_row();
        ui.label("Placement");
        ui.label(RichText::new("at the sketch's points").color(theme::MUTED));
        ui.end_row();
        Self::choose(ui, "Hole type", "hole", &mut self.choice, &HOLES, dirty);
        Self::field(ui, "Diameter", &mut self.a, dirty);
        if self.choice != "simple" {
          Self::field(ui, "Head diameter", &mut self.c, dirty);
          if self.choice == "counterbore" {
            Self::field(ui, "Head depth", &mut self.d, dirty);
          } else {
            Self::field(ui, "Head angle", &mut self.e, dirty);
          }
        }
        Self::check(ui, "Through all", &mut self.flag, dirty);
        if !self.flag {
          Self::field(ui, "Depth", &mut self.b, dirty);
        }
        Self::check(ui, "Flip direction", &mut self.flip, dirty);
      }
      Kind::PressPull => {
        ui.label("Faces");
        chip(ui, self.faces.len(), "selected");
        ui.end_row();
        Self::field(ui, "Distance", &mut self.distance, dirty);
      }
      Kind::Draft => {
        ui.label("Faces");
        chip(ui, self.faces.len(), "selected");
        ui.end_row();
        ui.label("Pull direction");
        let before = self.plane_label.clone();
        self.plane_choice(ui);
        *dirty |= before != self.plane_label;
        ui.end_row();
        Self::field(ui, "Angle", &mut self.angle, dirty);
        Self::check(ui, "Flip", &mut self.flip, dirty);
      }
      Kind::SplitBody => {
        ui.label("Bodies to split");
        sel_chip(ui, accent, &self.bodies_label(), !self.bodies.is_empty());
        ui.end_row();
        ui.label("Splitting tool");
        let shown = match self.target {
          Some(b) => app.s.body_name(BodyId(b)),
          None => self.plane_label.clone(),
        };
        let bodies: Vec<(u32, String)> = app.s.model.bodies.iter().map(|b| (b.id.0, app.s.body_name(b.id))).collect();
        egui::ComboBox::from_id_salt("splittool").selected_text(shown).width(140.0).show_ui(ui, |ui| {
          for (l, p) in [("XY plane", "xy"), ("XZ plane", "xz"), ("YZ plane", "yz")] {
            if ui.selectable_label(self.target.is_none() && self.plane_label == l, l).clicked() {
              self.target = None;
              self.plane = json!(p);
              self.plane_label = l.into();
              *dirty = true;
            }
          }
          ui.separator();
          for (id, n) in &bodies {
            if ui.selectable_label(self.target == Some(*id), n).clicked() {
              self.target = Some(*id);
              *dirty = true;
            }
          }
        });
        ui.end_row();
        if self.target.is_some() {
          Self::check(ui, "Remove the tool body", &mut self.flag, dirty);
        }
      }
      Kind::Flange => {
        let before = self.choice;
        Self::choose(ui, "Type", "flange", &mut self.choice, &FLANGES, dirty);
        if before != self.choice {
          self.withdraw(&mut app.s);
        }
        match self.choice {
          "edge" => {
            ui.label("Edges");
            chip(ui, self.edges.len(), "selected");
            ui.end_row();
            Self::field(ui, "Height", &mut self.distance, dirty);
            Self::field(ui, "Bend angle", &mut self.angle, dirty);
            Self::choose(ui, "Position", "pos", &mut self.position, &POSITIONS, dirty);
            Self::choose(ui, "Corners", "corners", &mut self.corners, &CORNERS, dirty);
          }
          "contour" => {
            ui.label("Sketch");
            ui.label(self.sketch.and_then(|s| app.s.design.feature(s)).map(|f| f.name.clone()).unwrap_or_default());
            ui.end_row();
            Self::field(ui, "Distance", &mut self.distance, dirty);
            Self::check(ui, "Symmetric", &mut self.flag, dirty);
            self.rule_combo(ui);
          }
          _ => {
            ui.label("Profiles");
            chip(ui, self.profiles.len(), "selected");
            ui.end_row();
            self.rule_combo(ui);
            Self::choose(ui, "Operation", "op", &mut self.op, &OPS[..2], dirty);
          }
        }
        Self::check(ui, "Flip", &mut self.flip, dirty);
      }
      Kind::Hem => {
        ui.label("Edges");
        chip(ui, self.edges.len(), "selected");
        ui.end_row();
        Self::field(ui, "Length", &mut self.distance, dirty);
        Self::field(ui, "Gap", &mut self.distance2, dirty);
        Self::check(ui, "Flip", &mut self.flip, dirty);
      }
      Kind::Fold => {
        ui.label("Sketch");
        let list = Self::sketches(app, None);
        let cur = self.sketch.and_then(|p| list.iter().find(|(id, _)| *id == p)).map(|(_, n)| n.clone());
        egui::ComboBox::from_id_salt("fsk").selected_text(cur.unwrap_or_default()).width(140.0).show_ui(ui, |ui| {
          for (id, n) in &list {
            if ui.selectable_label(self.sketch == Some(*id), n).clicked() {
              self.sketch = Some(*id);
              self.line = None;
              *dirty = true;
            }
          }
        });
        ui.end_row();
        ui.label("Bend line");
        let lines = self.sketch_lines(app);
        let cur = self.line.map(|l| format!("Sketch line {l}")).unwrap_or("Pick a line".into());
        egui::ComboBox::from_id_salt("fline").selected_text(cur).width(140.0).show_ui(ui, |ui| {
          for (id, label) in lines {
            if ui.selectable_label(self.line == Some(id), label).clicked() {
              self.line = Some(id);
              *dirty = true;
            }
          }
        });
        ui.end_row();
        Self::field(ui, "Angle", &mut self.angle, dirty);
        Self::check(ui, "Flip direction", &mut self.flip, dirty);
        Self::check(ui, "Fold the other side", &mut self.flag, dirty);
      }
      Kind::ConvertSheet => {
        ui.label("Body");
        chip(ui, self.bodies.len().min(1), "body");
        ui.end_row();
        ui.label("Stationary face");
        chip(ui, self.faces.len(), "face");
        ui.end_row();
        self.rule_combo(ui);
      }
      Kind::Rip => {
        ui.label("Edges");
        chip(ui, self.edges.len(), "selected");
        ui.end_row();
        Self::field(ui, "Gap", &mut self.distance, dirty);
      }
      Kind::Unfold => {
        ui.label("Body");
        if self.bodies.is_empty() {
          ui.label(RichText::new("the sheet metal body").color(theme::MUTED));
        } else {
          chip(ui, 1, "body");
        }
        ui.end_row();
        ui.label("Stationary face");
        chip(ui, self.faces.len(), "face");
        ui.end_row();
      }
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
      Kind::FlatPattern => {
        ui.label("Body");
        if self.bodies.is_empty() {
          ui.label(RichText::new("the sheet metal body").color(theme::MUTED));
        } else {
          chip(ui, 1, "body");
        }
        ui.end_row();
        if let Some(r) = self.report.clone() {
          if let Some([w, h]) = r.get("size").and_then(|v| serde_json::from_value::<[f64; 2]>(v.clone()).ok()) {
            ui.label("Flat size");
            ui.label(format!("{w:.2} × {h:.2} mm"));
            ui.end_row();
          }
          let bends = r.get("bends").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
          ui.label("Bends");
          ui.label(bends.to_string());
          ui.end_row();
          if let Some(check) = r.get("check") {
            ui.label("Ready to cut");
            let findings = check["findings"].as_array().cloned().unwrap_or_default();
            if findings.is_empty() {
              ui.label(RichText::new("✓ passes the shop checks").color(theme::ACCENT));
            } else {
              ui.vertical(|ui| {
                for f in &findings {
                  let at = &f["at"];
                  let text = format!(
                    "⚠ {} (at {:.1}, {:.1})",
                    f["message"].as_str().unwrap_or(""),
                    at[0].as_f64().unwrap_or(0.0),
                    at[1].as_f64().unwrap_or(0.0)
                  );
                  ui.label(RichText::new(text).color(theme::WARNING));
                }
              });
            }
            ui.end_row();
          }
          ui.label("Export");
          if panels::secondary_button(ui, "DXF…").clicked()
            && let Some(path) =
              rfd::FileDialog::new().add_filter("DXF", &["dxf"]).set_file_name("flat pattern.dxf").save_file()
          {
            let mut p = json!({"path": path});
            if let Some(b) = self.bodies.first() {
              p["body"] = json!(b);
            }
            if app.run("sheet.flat_pattern", p).is_some() {
              app.status = Some((format!("Exported {}", path.display()), false));
            }
          }
          ui.end_row();
        }
      }
      Kind::NewComponent => {
        Self::field(ui, "Name", &mut self.name, dirty);
        ui.label("Bodies");
        chip(ui, self.bodies.len(), "selected");
        ui.end_row();
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
      Kind::Material => {
        ui.label("Component");
        let n = self.comps.iter().flatten().count();
        chip(ui, n.min(1), "component");
        ui.end_row();
        ui.label("Material");
        let cur = self.name.clone();
        egui::ComboBox::from_id_salt("mat").selected_text(&cur).width(140.0).show_ui(ui, |ui| {
          for (m, _) in fox_engine::model::assembly::MATERIALS {
            if ui.selectable_label(cur == *m, *m).clicked() {
              self.name = m.to_string();
              *dirty = true;
            }
          }
        });
        ui.end_row();
      }
      Kind::Interference => {
        let rows = self.report.as_ref().and_then(Value::as_array).cloned().unwrap_or_default();
        ui.label("Result");
        ui.label(if rows.is_empty() { "No interference".to_string() } else { format!("{} overlaps", rows.len()) });
        ui.end_row();
        for r in rows {
          let ids: Vec<u32> = r
            .get("bodies")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_u64().map(|v| v as u32)).collect())
            .unwrap_or_default();
          let names: Vec<String> = ids.iter().map(|b| app.s.body_name(BodyId(*b))).collect();
          if ui.link(names.join(" ↔ ")).clicked() {
            app.s.selection = ids.iter().map(|b| Sel::Body { body: BodyId(*b) }).collect();
          }
          ui.label(format!("{:.3} mm³", r.get("volume_mm3").and_then(Value::as_f64).unwrap_or(0.0)));
          ui.end_row();
        }
        ui.label("");
        if panels::secondary_button(ui, "Compute").clicked() {
          self.refresh_report(&mut app.s);
        }
        ui.end_row();
        // adds each overlap as a static body (in a new component when ticked)
        ui.label("");
        ui.checkbox(&mut self.flag, "Create components");
        ui.end_row();
        ui.label("");
        let overlap = self.report.as_ref().and_then(Value::as_array).is_some_and(|a| !a.is_empty());
        if ui.add_enabled(overlap, egui::Button::new("Create Bodies")).clicked() {
          self.create_interference(&mut app.s);
        }
        ui.end_row();
      }
      Kind::CenterOfMass => {
        let r = self.report.clone().unwrap_or(Value::Null);
        let c: Vec<f64> = r
          .get("center")
          .and_then(Value::as_array)
          .map(|a| a.iter().filter_map(Value::as_f64).collect())
          .unwrap_or_default();
        ui.label("Bodies");
        ui.label(format!("{}", r.get("bodies").and_then(Value::as_array).map(Vec::len).unwrap_or(0)));
        ui.end_row();
        if let [x, y, z] = c.as_slice() {
          ui.label(RichText::new("Centre").strong());
          ui.label(format!("{x:.3}, {y:.3}, {z:.3} mm"));
          ui.end_row();
        }
        ui.label("Mass");
        ui.label(format!("{:.2} g", r.get("mass_g").and_then(Value::as_f64).unwrap_or(0.0)));
        ui.end_row();
        ui.label("Volume");
        ui.label(format!("{:.3} mm³", r.get("volume_mm3").and_then(Value::as_f64).unwrap_or(0.0)));
        ui.end_row();
      }
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
        let before = self.distance.clone();
        Self::field(ui, "Offset position", &mut self.distance, dirty);
        if self.distance != before {
          // a typed distance holds: the offset stops following the cursor
          self.follow = false;
        }
      }
      Kind::SkMirror => {
        ui.label("Objects");
        chip(ui, self.curves.len(), "selected");
        ui.end_row();
        ui.label("Mirror line");
        let lines = self.sketch_lines_of(app.s.edit_sketch, app);
        let cur = self.line.map(|l| format!("Sketch line {l}")).unwrap_or("Pick a line".into());
        egui::ComboBox::from_id_salt("mline").selected_text(cur).width(140.0).show_ui(ui, |ui| {
          for (id, label) in lines {
            if ui.selectable_label(self.line == Some(id), label).clicked() {
              self.line = Some(id);
              self.curves.retain(|c| *c != id);
              *dirty = true;
            }
          }
        });
        ui.end_row();
      }
      Kind::SkCircPattern => {
        ui.label("Objects");
        chip(ui, self.curves.len(), "selected");
        ui.end_row();
        ui.label("Center X, Y");
        ui.horizontal(|ui| {
          for v in self.at.iter_mut() {
            if ui.add(egui::TextEdit::singleline(v).desired_width(64.0)).changed() {
              *dirty = true;
            }
          }
        });
        ui.end_row();
        Self::field(ui, "Quantity", &mut self.count, dirty);
        Self::field(ui, "Total angle", &mut self.angle, dirty);
        Self::check(ui, "Symmetric", &mut self.flag, dirty);
      }
      Kind::SkRectPattern => {
        ui.label("Objects");
        chip(ui, self.curves.len(), "selected");
        ui.end_row();
        const DIRS: [(&str, &str); 2] = [("x", "Horizontal"), ("y", "Vertical")];
        let mut d1: &'static str = if self.dir == "y" { "y" } else { "x" };
        Self::choose(ui, "Direction", "sdir1", &mut d1, &DIRS, dirty);
        self.dir = d1.into();
        const TYPES: [(&str, &str); 2] = [("spacing", "Spacing"), ("extent", "Extent")];
        let mut ty: &'static str = if self.pattern_extent { "extent" } else { "spacing" };
        Self::choose(ui, "Distance type", "sdtype", &mut ty, &TYPES, dirty);
        self.pattern_extent = ty == "extent";
        Self::field(ui, "Quantity", &mut self.count, dirty);
        Self::field(ui, "Distance", &mut self.spacing, dirty);
        Self::check(ui, "Symmetric", &mut self.symmetric[0], dirty);
        Self::check(ui, "Second direction", &mut self.use_dir2, dirty);
        if self.use_dir2 {
          let mut d2: &'static str = if self.dir2 == "x" { "x" } else { "y" };
          Self::choose(ui, "Direction 2", "sdir2", &mut d2, &DIRS, dirty);
          self.dir2 = d2.into();
          Self::field(ui, "Quantity 2", &mut self.count2, dirty);
          Self::field(ui, "Distance 2", &mut self.spacing2, dirty);
          Self::check(ui, "Symmetric 2", &mut self.symmetric[1], dirty);
        }
      }
      Kind::Refold => {
        ui.label("Body");
        if self.bodies.is_empty() {
          ui.label(RichText::new("the unfolded sheet").color(theme::MUTED));
        } else {
          chip(ui, 1, "body");
        }
        ui.end_row();
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
        ui.label(RichText::new("a selected model edge fills the points").small().color(theme::MUTED));
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
        Self::check(ui, "Projected link (follows the body)", &mut self.flag, dirty);
      }
      _ => {}
    }
  }

  /// Re-runs a report dialog's query.
  fn refresh_report(&mut self, s: &mut Session) {
    let r = match self.kind {
      Kind::FlatPattern => {
        let mut p = json!({});
        if let Some(b) = self.bodies.first() {
          p["body"] = json!(b);
        }
        if let Some(f) = self.faces.first() {
          p["face"] = f.clone();
        }
        // with the shop checks of the flat (hole sizes, webs, holes near bends)
        s.execute("sheet.flat_pattern", &p).map(|mut v| {
          if let Ok(c) = s.execute("sheet.check", &p) {
            v["check"] = c;
          }
          v
        })
      }
      Kind::Interference => {
        // the selected bodies (two or more), else every pair of components
        let picked = sel_only_bodies(s);
        self.bodies = if picked.len() >= 2 { picked } else { Vec::new() };
        s.execute("assembly.interference", &json!({"bodies": self.bodies}))
      }
      Kind::Bom => s.execute("assembly.bom", &json!({})),
      Kind::CenterOfMass => {
        let bodies = sel_only_bodies(s);
        s.execute("inspect.center_of_mass", &json!({"bodies": bodies}))
      }
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
        if ui.add(egui::TextEdit::singleline(x).desired_width(44.0)).changed() {
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
        if ui.add(egui::TextEdit::singleline(x).desired_width(64.0)).changed() {
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

  fn combo(&mut self, ui: &mut egui::Ui, id: &str, items: &[(&'static str, &'static str)], extent: bool) {
    let cur = if extent { self.extent } else { self.op };
    let label = items.iter().find(|(k, _)| *k == cur).map(|(_, l)| *l).unwrap_or(cur);
    egui::ComboBox::from_id_salt(id).selected_text(label).width(140.0).show_ui(ui, |ui| {
      for (k, l) in items {
        if ui.selectable_label(cur == *k, *l).clicked() {
          if extent {
            self.extent = k;
          } else {
            self.op = k;
          }
          self.dirty = true;
        }
      }
    });
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
    let extra: Vec<String> =
      if id == "dir1" || id == "dir2" { Vec::new() } else { axes.iter().map(|a| format!("axis:{a}")).collect() };
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
    if self.bodies.is_empty() { "all bodies".into() } else { format!("{} selected", self.bodies.len()) }
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

/// Measure's length units: name and millimetres per unit.
pub(crate) const MEASURE_UNITS: [(&str, f64); 5] =
  [("mm", 1.0), ("cm", 10.0), ("m", 1000.0), ("in", 25.4), ("ft", 304.8)];

/// How Measure shows its numbers: decimal places and length unit (kept between uses, as in Fusion).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasureFmt {
  pub precision: usize,
  /// Index into [`MEASURE_UNITS`].
  pub unit: usize,
}

impl Default for MeasureFmt {
  fn default() -> Self {
    MeasureFmt { precision: 3, unit: 0 }
  }
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

  pub fn len(&self, mm: f64) -> String {
    let (u, k) = self.unit();
    format!("{} {u}", self.num(mm / k))
  }

  pub fn area(&self, mm2: f64) -> String {
    let (u, k) = self.unit();
    format!("{} {u}²", self.num(mm2 / (k * k)))
  }

  pub fn volume(&self, mm3: f64) -> String {
    let (u, k) = self.unit();
    format!("{} {u}³", self.num(mm3 / (k * k * k)))
  }

  pub fn angle(&self, deg: f64) -> String {
    format!("{} deg", self.num(deg))
  }
}

/// One line of Measure's results; a heading names what was measured.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Readout {
  pub label: String,
  pub value: String,
  pub heading: bool,
}

fn row(label: &str, value: String) -> Readout {
  Readout { label: label.into(), value, heading: false }
}

fn heading(label: &str, value: String) -> Readout {
  Readout { label: label.into(), value, heading: true }
}

/// Measure's readouts for a `measure` result: the distance, deltas and angle between two picks,
/// or one pick's own numbers (a face's area, an edge's length, a vertex's position, a body's
/// volume).
pub(crate) fn measure_rows(r: &Value, f: &MeasureFmt) -> Vec<Readout> {
  let num = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64);
  let v3 = |v: &Value, k: &str| -> Option<[f64; 3]> {
    let a = v.get(k)?.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?])
  };
  let xyz = |out: &mut Vec<Readout>, p: [f64; 3]| {
    for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
      out.push(row(axis, f.len(p[i])));
    }
  };
  let mut out = Vec::new();
  if let Some(d) = num(r, "distance") {
    out.push(heading("Distance", f.len(d)));
    if let Some(dl) = v3(r, "delta") {
      for (i, axis) in ["ΔX", "ΔY", "ΔZ"].iter().enumerate() {
        out.push(row(axis, f.len(dl[i].abs())));
      }
    }
    if let Some(a) = num(r, "angle_deg") {
      out.push(row("Angle", f.angle(a)));
    }
    return out;
  }
  let words = |s: &str| {
    let mut c = s.chars();
    c.next().map(|x| x.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
  };
  match r.get("kind").and_then(Value::as_str) {
    Some("face") => {
      out.push(heading("Face", words(r.get("surface").and_then(Value::as_str).unwrap_or(""))));
      if let Some(a) = num(r, "area_mm2") {
        out.push(row("Area", f.area(a)));
      }
      if let Some(x) = num(r, "radius") {
        out.push(row("Radius", f.len(x)));
      }
      if let Some(x) = num(r, "diameter") {
        out.push(row("Diameter", f.len(x)));
      }
    }
    Some("edge") => {
      out.push(heading("Edge", words(r.get("curve").and_then(Value::as_str).unwrap_or(""))));
      if let Some(l) = num(r, "length") {
        out.push(row("Length", f.len(l)));
      }
      if let Some(x) = num(r, "radius") {
        out.push(row("Radius", f.len(x)));
      }
      if let Some(x) = num(r, "diameter") {
        out.push(row("Diameter", f.len(x)));
      }
    }
    Some("point") => {
      out.push(heading("Vertex", "Position".into()));
      if let Some(p) = v3(r, "at") {
        xyz(&mut out, p);
      }
    }
    Some("body") => {
      out.push(heading("Body", r.get("name").and_then(Value::as_str).unwrap_or("").into()));
      if let Some(v) = num(r, "volume_mm3") {
        out.push(row("Volume", f.volume(v)));
      }
      if let Some(a) = num(r, "area_mm2") {
        out.push(row("Area", f.area(a)));
      }
      if let Some(d) = v3(r, "size") {
        let (u, k) = f.unit();
        out.push(row("Size", format!("{} × {} × {} {u}", f.num(d[0] / k), f.num(d[1] / k), f.num(d[2] / k))));
      }
      if let Some(c) = v3(r, "center_of_mass") {
        out.push(heading("Center of mass", String::new()));
        xyz(&mut out, c);
      }
    }
    _ => {}
  }
  out
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
  /// Measure's panel: selection filter, precision and units, then the readouts with copy buttons.
  fn inspect(&mut self, app: &mut App, ui: &mut egui::Ui) {
    app.measure = None;
    // Measure picks vertices too, while it's open
    if self.saved_filter.is_none() {
      self.saved_filter = Some((app.sel_priority, app.sel_filter.vertices));
      app.sel_filter.vertices = true;
    }
    ui.label("Selection Filter");
    ui.horizontal(|ui| {
      use crate::select::Priority;
      let whole = matches!(app.sel_priority, Priority::Body | Priority::Component);
      if ui.selectable_label(whole, "Body").on_hover_text("Pick bodies").clicked() {
        app.sel_priority = Priority::Body;
      }
      if ui.selectable_label(!whole, "Face/Edge/Vertex").on_hover_text("Pick faces, edges and vertices").clicked() {
        app.sel_priority = Priority::Face;
      }
    });
    ui.end_row();
    let fmt = &mut app.measure_fmt;
    ui.label("Precision");
    egui::ComboBox::from_id_salt("measure-precision")
      .selected_text(if fmt.precision == 0 { "0".to_string() } else { format!("0.{}", "0".repeat(fmt.precision)) })
      .show_ui(ui, |ui| {
        for p in 0..=6 {
          let text = if p == 0 { "0".to_string() } else { format!("0.{}", "0".repeat(p)) };
          ui.selectable_value(&mut fmt.precision, p, text);
        }
      });
    ui.end_row();
    ui.label("Units");
    egui::ComboBox::from_id_salt("measure-units").selected_text(MEASURE_UNITS[fmt.unit].0).show_ui(ui, |ui| {
      for (i, (u, _)) in MEASURE_UNITS.iter().enumerate() {
        ui.selectable_value(&mut fmt.unit, i, *u);
      }
    });
    ui.end_row();
    let fmt = app.measure_fmt;
    let (picks, hovering) = measure_picks(app);
    if picks.is_empty() {
      ui.label("");
      ui.label(RichText::new("Select a body, face, edge or vertex — two to measure between them").color(theme::MUTED));
      ui.end_row();
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
    let rows = measure_rows(&r, &fmt);
    ui.label(RichText::new(if hovering { "Results (hover)" } else { "Results" }).color(theme::GROUP_LABEL));
    if ui.small_button("Copy all").on_hover_text("Copy every result to the clipboard").clicked() {
      let text: Vec<String> = rows.iter().map(|r| format!("{}: {}", r.label, r.value)).collect();
      ui.ctx().copy_text(text.join("\n"));
      app.status = Some(("Copied the measurements".into(), false));
    }
    ui.end_row();
    for (i, r) in rows.iter().enumerate() {
      if r.heading {
        ui.label(RichText::new(&r.label).strong());
      } else {
        ui.label(&r.label);
      }
      ui.horizontal(|ui| {
        let v = if r.heading { RichText::new(&r.value).strong() } else { RichText::new(&r.value) };
        ui.label(v);
        if !r.value.is_empty()
          && ui.push_id(i, |ui| ui.small_button("Copy")).inner.on_hover_text("Copy to clipboard").clicked()
        {
          ui.ctx().copy_text(r.value.clone());
          app.status = Some((format!("Copied {}", r.value), false));
        }
      });
      ui.end_row();
    }
  }

  /// Fusion's Appearance panel: Apply To (bodies or faces), the appearances In This Design, and
  /// the library. Swatches drag onto the model, or a click applies one to the selection.
  fn appearance(&mut self, app: &mut App, ui: &mut egui::Ui) {
    ui.label("Apply To");
    ui.horizontal(|ui| {
      if ui.selectable_label(!self.appearance_faces, "Bodies/Components").clicked() {
        self.appearance_faces = false;
      }
      if ui.selectable_label(self.appearance_faces, "Faces").clicked() {
        self.appearance_faces = true;
      }
    });
    ui.end_row();
    let lib = appearance_library(app);
    let used = in_this_design(app, &lib);
    let faces = self.appearance_faces;
    let mut apply: Option<(String, [u8; 3])> = None;
    for (title, list) in [("In This Design", used), ("Library", lib.iter().map(|(n, c)| (n.clone(), *c, 0)).collect())]
    {
      for (i, (name, color, n)) in list.into_iter().enumerate() {
        ui.label(if i == 0 { RichText::new(title).color(theme::GROUP_LABEL) } else { RichText::new("") });
        let label = if n > 0 { format!("{name}  ({n})") } else { name.clone() };
        if swatch_row(ui, (title, i), &label, &name, color, faces) {
          apply = Some((name, color));
        }
        ui.end_row();
      }
    }
    ui.label("");
    ui.label(
      RichText::new("Drag a swatch onto a body or face, or click one to apply it to the selection")
        .small()
        .color(theme::MUTED),
    );
    ui.end_row();
    if let Some((name, color)) = apply {
      let sel = app.s.selection.clone();
      if !apply_appearance(app, &name, color, faces, &sel) {
        let what = if faces { "faces" } else { "bodies" };
        app.status = Some((format!("Select {what} first, or drag the swatch onto the model"), false));
      }
    }
  }

  /// Puts back what the dialog changed in the app while open (Measure's selection filter).
  pub(crate) fn leave(&mut self, app: &mut App) {
    if let Some((priority, vertices)) = self.saved_filter.take() {
      app.sel_priority = priority;
      app.sel_filter.vertices = vertices;
    }
  }
}

/// A swatch dragged from the Appearance panel onto the canvas.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SwatchDrag {
  pub name: String,
  pub color: [u8; 3],
  /// Apply to the face under the pointer rather than its body.
  pub faces: bool,
}

/// The appearance library (name, colour), as the engine knows it.
pub(crate) fn appearance_library(app: &mut App) -> Vec<(String, [u8; 3])> {
  let Ok(v) = app.s.execute("appearances", &json!({})) else { return Vec::new() };
  let rgb = |c: &Value| -> Option<[u8; 3]> {
    let a = c.as_array()?;
    Some([a.first()?.as_u64()? as u8, a.get(1)?.as_u64()? as u8, a.get(2)?.as_u64()? as u8])
  };
  v.as_array()
    .map(|a| a.iter().filter_map(|x| Some((x.get("name")?.as_str()?.to_string(), rgb(x.get("color")?)?))).collect())
    .unwrap_or_default()
}

/// What bodies show without an appearance of their own: Fusion's default, Steel - Satin.
const DEFAULT_APPEARANCE: [u8; 3] = [186, 190, 196];

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

/// One swatch of the Appearance panel: drags as a [`SwatchDrag`]; returns whether it was clicked.
fn swatch_row(
  ui: &mut egui::Ui,
  salt: impl std::hash::Hash + std::fmt::Debug,
  label: &str,
  name: &str,
  color: [u8; 3],
  faces: bool,
) -> bool {
  let size = egui::vec2(ui.available_width().clamp(120.0, 180.0), 20.0);
  let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
  let resp = ui.interact(rect, ui.id().with(("swatch", salt)), egui::Sense::click_and_drag());
  resp.dnd_set_drag_payload(SwatchDrag { name: name.to_string(), color, faces });
  let p = ui.painter();
  if resp.hovered() {
    p.rect_filled(rect, 2.0, theme::HOVER);
  }
  let fill = egui::Color32::from_rgb(color[0], color[1], color[2]);
  let sw = egui::Rect::from_center_size(egui::pos2(rect.min.x + 10.0, rect.center().y), egui::vec2(14.0, 14.0));
  p.rect(sw, 2.0, fill, egui::Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
  p.text(
    egui::pos2(sw.max.x + 6.0, rect.center().y),
    egui::Align2::LEFT_CENTER,
    label,
    egui::FontId::proportional(12.5),
    theme::TEXT,
  );
  if resp.dragged()
    && let Some(at) = ui.ctx().pointer_latest_pos()
  {
    // the swatch follows the pointer
    let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("swatch-drag"));
    let r = egui::Rect::from_center_size(at + egui::vec2(14.0, 14.0), egui::vec2(18.0, 18.0));
    ui.ctx().layer_painter(layer).rect(r, 3.0, fill, egui::Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
  }
  resp.on_hover_text("Drag onto a body or face, or click to apply to the selection").clicked()
}

/// The section analysis settings, edited in place (the view updates as they change).
fn section_fields(app: &mut App, ui: &mut egui::Ui) {
  let mut on = app.section.is_some();
  let mut sec = app.section.clone().unwrap_or(json!({"plane": "xz", "offset": 0.0, "flip": false}));
  ui.label("");
  ui.checkbox(&mut on, "Show section");
  ui.end_row();
  ui.label("Plane");
  let cur = sec["plane"].clone();
  let mut options: Vec<(Value, String)> = [("xy", "XY plane"), ("xz", "XZ plane"), ("yz", "YZ plane")]
    .iter()
    .map(|(k, l)| (json!(k), l.to_string()))
    .collect();
  for id in app.s.model.planes.keys() {
    options.push((json!({"feature": id.0}), app.s.design.feature(*id).map(|f| f.name.clone()).unwrap_or_default()));
  }
  let shown = options.iter().find(|(k, _)| *k == cur).map(|(_, l)| l.clone()).unwrap_or("Picked face".into());
  egui::ComboBox::from_id_salt("section-plane").selected_text(shown).width(140.0).show_ui(ui, |ui| {
    for (k, l) in &options {
      if ui.selectable_label(*k == cur, l).clicked() {
        sec["plane"] = k.clone();
      }
    }
  });
  ui.end_row();
  ui.label("Offset");
  let mut off = sec["offset"].as_f64().unwrap_or(0.0);
  ui.add(egui::DragValue::new(&mut off).speed(0.5).suffix(" mm"));
  sec["offset"] = json!(off);
  ui.end_row();
  ui.label("");
  let mut flip = sec["flip"].as_bool().unwrap_or(false);
  ui.checkbox(&mut flip, "Flip (keep the other side)");
  sec["flip"] = json!(flip);
  ui.end_row();
  app.section = on.then_some(sec);
}

/// Surface analysis settings: the pull direction and draft angle where they apply.
fn analysis_fields(app: &mut App, ui: &mut egui::Ui, kind: &'static str) {
  let mut on = app.analysis.as_ref().is_some_and(|a| a["analysis"] == kind);
  let mut a = app
    .analysis
    .clone()
    .filter(|a| a["analysis"] == kind)
    .unwrap_or(json!({"analysis": kind, "pull": "z", "angle": 1.0}));
  ui.label("");
  ui.checkbox(&mut on, "Show analysis");
  ui.end_row();
  if matches!(kind, "draft" | "accessibility") {
    ui.label(if kind == "draft" { "Pull direction" } else { "Tool direction" });
    let cur = a["pull"].as_str().unwrap_or("z").to_string();
    egui::ComboBox::from_id_salt("pull").selected_text(axis_label(&cur)).width(140.0).show_ui(ui, |ui| {
      for k in ["x", "y", "z"] {
        if ui.selectable_label(cur == k, axis_label(k)).clicked() {
          a["pull"] = json!(k);
        }
      }
    });
    ui.end_row();
  }
  if kind == "draft" {
    ui.label("Draft angle");
    let mut ang = a["angle"].as_f64().unwrap_or(1.0);
    ui.add(egui::DragValue::new(&mut ang).speed(0.1).range(0.0..=45.0).suffix("°"));
    a["angle"] = json!(ang);
    ui.end_row();
  }
  let legend = match kind {
    "zebra" => "Stripes that kink or jump show where faces meet without tangency or curvature continuity",
    "draft" => "Green: positive draft · yellow: less than the angle · red: undercut",
    "accessibility" => "Green: reachable along the direction · red: hidden from it",
    _ => "Blue: flat · green → yellow → red: curving more tightly",
  };
  ui.label("");
  ui.label(RichText::new(legend).small().color(theme::MUTED));
  ui.end_row();
  app.analysis = on.then_some(a);
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
        && !feats.iter().any(|g| matches!(&g.kind, FeatureKind::Extrude { sketch, .. } | FeatureKind::Revolve { sketch, .. } if *sketch == f.id))
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
  s.selection
    .iter()
    .filter_map(|x| match x {
      Sel::Face { body, face } => Some((body.0, json!({"face": {"body": body.0, "face": face}}))),
      Sel::Edge { body, edge } => Some((body.0, json!({"edge": {"body": body.0, "edge": edge}}))),
      Sel::Vertex { body, edge, end } => {
        fox_engine::scene::vertex_pos(s, *body, *edge, *end).map(|p| (body.0, json!({"at": p.to_array()})))
      }
      _ => None,
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

/// A dialog value box. The first one drawn takes typing aimed at the dialog: its text is replaced
/// and it gets the keyboard, like Fusion's floating value box next to the manipulator.
fn value_edit(ui: &mut egui::Ui, text: &mut String) -> egui::Response {
  value_edit_w(ui, text, 140.0)
}

/// [alue_edit] at a given width.
pub(super) fn value_edit_w(ui: &mut egui::Ui, text: &mut String, width: f32) -> egui::Response {
  let mut r = ui.add(egui::TextEdit::singleline(text).desired_width(width));
  if let Some(typed) = ui.data_mut(|d| d.remove_temp::<String>(egui::Id::new(TYPING))) {
    *text = typed;
    crate::viewport::focus_text(ui.ctx(), &r, text, false);
    r.mark_changed();
  }
  r
}

/// A selection input: a white "Select" chip until something is picked, then a chip tinted with
/// the command's accent showing the count, with a ✕ beside it.
fn sel_chip(ui: &mut egui::Ui, accent: egui::Color32, text: &str, has: bool) -> egui::Response {
  let font = egui::FontId::proportional(12.0);
  let tw = ui.painter().layout_no_wrap(text.to_string(), font.clone(), theme::TEXT).size().x;
  let (rect, resp) = ui.allocate_exact_size(egui::vec2(tw + 52.0, 22.0), egui::Sense::hover());
  let chip = egui::Rect::from_min_size(rect.min, egui::vec2(tw + 32.0, 22.0));
  let p = ui.painter();
  let ir = egui::Rect::from_center_size(chip.left_center() + egui::vec2(12.0, 0.0), egui::Vec2::splat(15.0));
  let fill = if has { crate::icons::tint_of(accent, 0.84) } else { egui::Color32::WHITE };
  let stroke = if has { accent } else { theme::BORDER };
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

#[cfg(test)]
mod tests {
  use super::*;

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
    d.distance = "5 mm".into();
    d.distance2 = "0.5 mm".into();
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::FlatPattern);
    d.refresh_report(&mut s);
    assert!(d.report.as_ref().is_some_and(|r| r["bends"].as_array().unwrap().len() == 2), "{:?}", d.error);
    let mut d = Dialog::blank(Kind::Unfold);
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
    let mut d = Dialog::blank(Kind::Material);
    d.comps = vec![comp(&s, arm)];
    d.name = "Aluminum".into();
    apply(&mut s, &mut d);
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
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = Dialog::blank(Kind::SkRectPattern);
    d.curves = vec![circle];
    d.count = "2".into();
    d.use_dir2 = true;
    d.pattern_extent = true;
    d.symmetric = [true, false];
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
    // split by body, refold
    let mut s = Session::default();
    run(&mut s, "box", json!({"width": 40, "depth": 40, "height": 10}));
    run(&mut s, "cylinder", json!({"radius": 5, "height": 30, "at": [20, 20]}));
    let (plate, pin) = (s.model.bodies[0].id.0, s.model.bodies[1].id.0);
    let mut d = Dialog::blank(Kind::SplitBody);
    d.bodies = vec![plate];
    d.target = Some(pin);
    apply(&mut s, &mut d);

    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [80, 40]}));
    run(&mut s, "sheet.flange", json!({}));
    run(&mut s, "sheet.edge_flange", json!({"edges": [{"point": [40, 40, 1]}], "height": 15}));
    run(&mut s, "sheet.unfold", json!({}));
    let mut d = Dialog::blank(Kind::Refold);
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
    let mut d = form(&mut s, &MIDPLANE, vec![Sel::Face { body: b, face: top }], &[]);
    apply(&mut s, &mut d);
    assert_eq!(s.model.planes.len(), 1);
    d.applied = false;
    let mut d = form(&mut s, &AXIS_TWO_PLANES, vec![], &[("a", json!("xz")), ("b", json!("yz"))]);
    // the drop-down choices survive a selection sync
    d.form.chosen = vec![Some(json!("xz")), Some(json!("yz"))];
    d.form.sync(&AXIS_TWO_PLANES, &s, None);
    apply(&mut s, &mut d);
    let (_, dir) = s.model.axes.values().next().copied().unwrap();
    assert!(dir.z.abs() > 0.999, "{dir:?}");
    d.applied = false;
    let mut d = form(&mut s, &PLANE_THREE_POINTS, vec![], &[("c", json!(["0", "0", "10"]))]);
    apply(&mut s, &mut d);
    d.applied = false;
    let mut d = form(&mut s, &SCALE, vec![Sel::Body { body: b }], &[("factor", json!("0.5"))]);
    apply(&mut s, &mut d);
    assert!((s.model.bodies[0].shape.mass().unwrap().volume - 500.0).abs() < 1e-6);
    d.applied = false;

    // text in the sketch being edited
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    let mut d = form(&mut s, &SK_TEXT, vec![], &[("text", json!("Hi"))]);
    apply(&mut s, &mut d);
    let sk = s.edit_sketch.unwrap();
    assert!(!s.model.sketches[&sk].profiles.is_empty());
  }

  #[test]
  fn extrude_arrow_drags_the_distance() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "xy"}));
    run(&mut s, "sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let mut d = Dialog::blank(Kind::Extrude);
    d.sketch = s.edit_sketch;
    run(&mut s, "sketch.finish", json!({}));
    let m = d.manipulator(&s).unwrap();
    assert!((m.value - 10.0).abs() < 1e-9 && m.dir.z > 0.99);
    d.set_manipulator(-7.26, 0.5);
    assert!(d.flip && d.distance == "7.5 mm", "{} {}", d.flip, d.distance);
    apply(&mut s, &mut d);
    let bb = s.model.bodies[0].shape.mass().unwrap();
    assert!((bb.bbox_min.z + 7.5).abs() < 1e-6, "{bb:?}");
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
    let mut d = form(&mut s, &EXTEND, vec![Sel::Edge { body: b, edge: e }], &[]);
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
      ("underline", json!(true)),
      ("spacing", json!("10")),
    ];
    let mut d = form(&mut s, &SK_TEXT, vec![], &vals);
    apply(&mut s, &mut d);
    d.applied = false;
    let sketch = s.design.sketch(FeatureId(sk)).unwrap();
    let t = &sketch.texts[0];
    assert!(t.style.bold && t.style.underline && !t.style.italic && t.style.spacing == 10.0);
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

    // a face: its area; an edge: its length; a vertex: its position
    let r = s.execute("measure", &json!({"a": {"type": "face", "body": b.0, "face": face}})).unwrap();
    let rows = measure_rows(&r, &mm);
    assert_eq!(labels(&rows)[..2], [("Face", "Plane"), ("Area", "200.000 mm²")]);
    assert!(rows[0].heading && !rows[1].heading);
    let r = s.execute("measure", &json!({"a": {"type": "edge", "body": b.0, "edge": edge}})).unwrap();
    assert_eq!(labels(&measure_rows(&r, &mm))[1], ("Length", "30.000 mm"));
    let r = s.execute("measure", &json!({"a": {"type": "vertex", "body": b.0, "edge": edge, "end": true}})).unwrap();
    let rows = measure_rows(&r, &mm);
    assert_eq!(rows[0].label, "Vertex");
    assert_eq!(rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["Vertex", "X", "Y", "Z"]);
    assert_eq!(labels(&rows)[3], ("Z", "30.000 mm"));
    // a body: its volume, in other units and precision
    let cm1 = MeasureFmt { precision: 1, unit: 1 };
    let r = s.execute("measure", &json!({"a": {"body": b.0}})).unwrap();
    let rows = measure_rows(&r, &cm1);
    assert!(labels(&rows).contains(&("Volume", "6.0 cm³")), "{rows:?}");
    assert!(labels(&rows).contains(&("Area", "22.0 cm²")), "{rows:?}");

    // two picks: distance, deltas and angle
    let r = s
      .execute("measure", &json!({"a": {"body": b.0, "face": faces[0]}, "b": {"body": b.0, "face": faces[1]}}))
      .unwrap();
    let inch = MeasureFmt { precision: 2, unit: 3 };
    let rows = measure_rows(&r, &inch);
    assert_eq!(labels(&rows)[0], ("Distance", "1.18 in"));
    assert!(labels(&rows).contains(&("Angle", "180.00 deg")), "{rows:?}");
    assert_eq!(MeasureFmt { precision: 0, unit: 0 }.len(-0.2), "0 mm");
  }

  #[test]
  fn measure_dialog_shows_the_selection_and_restores_the_filter() {
    let mut app = App::with_session(Session::default());
    app.s.execute("box", &json!({"width": 10, "depth": 20, "height": 30})).unwrap();
    let b = app.s.model.bodies[0].id;
    let edge = app.s.model.body(b).unwrap().edges()[0].id;
    app.s.selection = vec![Sel::Edge { body: b, edge }];
    let mut d = Dialog::open(&mut app, Kind::Inspect);
    assert!(show_once(&mut app, &mut d));
    assert!(app.sel_filter.vertices, "Measure picks vertices");
    app.s.selection = vec![Sel::Edge { body: b, edge }, Sel::Body { body: b }];
    assert!(show_once(&mut app, &mut d));
    assert!(app.measure.is_some(), "two picks draw the distance line");
    d.leave(&mut app);
    assert!(!app.sel_filter.vertices);
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

    let lib = appearance_library(&mut app);
    assert!(lib.len() >= 16 && lib.iter().any(|(n, _)| n == "Copper"));
    // a new body shows the default appearance
    assert_eq!(in_this_design(&app, &lib), [("Steel - Satin".to_string(), [186, 190, 196], 1)]);

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
    assert_eq!(name, "Named View 1");
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
    let mut d = form(&mut s, &SK_TEXT, sel, &[("text", json!("LL")), ("align", json!("center"))]);
    apply(&mut s, &mut d);
    let t = &s.design.sketch(FeatureId(sk)).unwrap().texts[0];
    let path = t.path.expect("the text runs along the arc");
    assert_eq!((path.curve.0, path.above, path.align), (arc, true, fox_engine::sketch::TextAlign::Center));
  }
}
