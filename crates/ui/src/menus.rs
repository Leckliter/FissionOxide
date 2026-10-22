//! The toolbar's tool groups and their drop-down menus for every tab of the Design workspace,
//! laid out entry for entry like Fusion's: same order, separators, shortcuts, submenus and
//! greyed-out items. Entries FissionOxide can run carry a [`Cmd`]; the rest are listed so the
//! toolbar reads like Fusion and say so in the status line when clicked.
//!
//! To wire up a new command, give its entry a `Cmd` (and add the command itself in
//! `panels::Cmd`).

use egui::{Color32, Rect, RichText, Stroke, Vec2, pos2, vec2};

use crate::customize::{self, Action, Binding, Prefs};
use crate::dialogs::Kind;
use crate::icons::{self, Icon, Tint};
use crate::panels::{self, Cmd, Tab};
use crate::render_ui::RenderCmd;
use crate::tips::{self, Scope};
use crate::{App, Tool, assembly_forms, config_ui, theme};

#[derive(Clone, Copy)]
pub(crate) struct Item {
  label: &'static str,
  icon: Icon,
  tint: Tint,
  /// Keyboard shortcut shown in the menu.
  key: &'static str,
  /// What it runs; None when FissionOxide doesn't have the command yet.
  cmd: Option<Cmd>,
  /// Greyed out, as Fusion shows commands unavailable in the current context.
  disabled: bool,
  /// Greyed out while the design is unsaved.
  needs_saved: bool,
  /// The SURFACE tab's own command of a SOLID label (Fusion's surface Extrude and Sweep): its
  /// own tooltip, and no shortcut shown.
  surface: bool,
  /// Carries Fusion's blue round subscription badge (a command of the paid Fusion subscription).
  paid: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum E {
  I(Item),
  Sep,
  /// A submenu row (no icon), opening on hover.
  Sub(&'static str, &'static [E]),
}

impl Item {
  /// The label its tooltip texts are found under.
  fn tip_label(&self) -> &'static str {
    if self.surface { tips::surface_label(self.label) } else { self.label }
  }
}

/// A command FissionOxide runs.
const fn c(label: &'static str, icon: Icon, cmd: Cmd) -> E {
  E::I(Item {
    label,
    icon,
    tint: Tint::Blue,
    key: "",
    cmd: Some(cmd),
    disabled: false,
    needs_saved: false,
    surface: false,
    paid: false,
  })
}

/// A Fusion command FissionOxide doesn't have yet.
const fn x(label: &'static str, icon: Icon) -> E {
  E::I(Item {
    label,
    icon,
    tint: Tint::Blue,
    key: "",
    cmd: None,
    disabled: false,
    needs_saved: false,
    surface: false,
    paid: false,
  })
}

impl E {
  /// Adds the keyboard shortcut shown in the menu.
  const fn k(self, key: &'static str) -> E {
    match self {
      E::I(mut i) => {
        i.key = key;
        E::I(i)
      }
      other => other,
    }
  }

  /// Orange (surface) icon.
  const fn o(self) -> E {
    match self {
      E::I(mut i) => {
        i.tint = Tint::Orange;
        E::I(i)
      }
      other => other,
    }
  }

  /// Gold (mesh) icon.
  const fn g(self) -> E {
    match self {
      E::I(mut i) => {
        i.tint = Tint::Gold;
        E::I(i)
      }
      other => other,
    }
  }

  /// Greyed out while the design is unsaved.
  const fn saved(self) -> E {
    match self {
      E::I(mut i) => {
        i.needs_saved = true;
        E::I(i)
      }
      other => other,
    }
  }

  /// The SURFACE tab's own version of a SOLID command: see [`Item::surface`].
  const fn surface(self) -> E {
    match self {
      E::I(mut i) => {
        i.surface = true;
        E::I(i)
      }
      other => other,
    }
  }

  /// Greyed out.
  const fn off(self) -> E {
    match self {
      E::I(mut i) => {
        i.disabled = true;
        E::I(i)
      }
      other => other,
    }
  }

  /// With Fusion's subscription badge.
  const fn p(self) -> E {
    match self {
      E::I(mut i) => {
        i.paid = true;
        E::I(i)
      }
      other => other,
    }
  }
}

/// Most commands a group shows on the toolbar; every command stays in the group's menu.
/// Fusion's SKETCH > CONSTRAINTS panel shows all twelve constraints, the widest default.
const MAX_QUICK: usize = 12;

pub(crate) struct Group {
  label: &'static str,
  /// Labels of the entries shown as toolbar icons.
  quick: &'static [&'static str],
  menu: &'static [E],
}

use crate::form_ui::{self, Act as FormAct};
use crate::forms;
use crate::insert_window::Target;
use crate::select::{Priority, SelTool};
use Cmd::Dialog as D;

const PATTERN: &[E] = &[
  c("Rectangular Pattern", Icon::Pattern, D(Kind::RectPattern)),
  c("Circular Pattern", Icon::CircPattern, D(Kind::CircPattern)),
  c("Pattern on Path", Icon::Pattern, D(Kind::Form(&forms::PATH_PATTERN))),
];

const SIMPLIFY: &[E] = &[
  c("Remove Features", Icon::RemoveFeatures, D(Kind::Form(&forms::REMOVE_FACES))),
  c("Remove Faces", Icon::RemoveFaces, D(Kind::Form(&forms::REMOVE_FACES))),
  x("Replace with Primitives", Icon::ReplacePrimitives),
];

const CREATE_PCB: &[E] = &[x("Associative PCB", Icon::InsertComponent), x("Create 3D PCB", Icon::InsertComponent)];

const SOLID_CREATE: &[E] = &[
  c("Create Sketch", Icon::Sketch, Cmd::NewSketch),
  c("Create Form", Icon::CreateForm, Cmd::Form(FormAct::Enter)),
  x("Derive", Icon::Derive),
  E::Sep,
  c("Extrude", Icon::Extrude, D(Kind::Extrude)).k("E"),
  c("Revolve", Icon::Revolve, D(Kind::Revolve)),
  c("Sweep", Icon::Sweep, D(Kind::Sweep)),
  c("Loft", Icon::Loft, D(Kind::Loft)),
  c("Rib", Icon::Rib, D(Kind::Form(&forms::RIB))),
  c("Web", Icon::Web, D(Kind::Form(&forms::WEB))),
  c("Emboss", Icon::Emboss, D(Kind::Form(&forms::EMBOSS))),
  E::Sep,
  c("Hole", Icon::Hole, D(Kind::Hole)).k("H"),
  c("Thread", Icon::Thread, D(Kind::Form(&forms::THREAD))),
  E::Sep,
  c("Box", Icon::Box, D(Kind::Box)),
  c("Cylinder", Icon::Cylinder, D(Kind::Cylinder)),
  c("Sphere", Icon::Sphere, D(Kind::Sphere)),
  c("Torus", Icon::Torus, D(Kind::Torus)),
  c("Coil", Icon::Coil, D(Kind::Form(&forms::COIL))),
  c("Pipe", Icon::Pipe, D(Kind::Form(&forms::PIPE))),
  E::Sep,
  E::Sub("Pattern", PATTERN),
  c("Mirror", Icon::Mirror, D(Kind::Mirror)),
  E::Sep,
  c("Thicken", Icon::Thicken, D(Kind::Form(&forms::THICKEN))),
  c("Boundary Fill", Icon::BoundaryFill, D(Kind::Form(&forms::BOUNDARY_FILL))),
  E::Sep,
  c("Create Base Feature", Icon::BaseFeature, Cmd::BaseFeature(true)),
  E::Sub("Create PCB", CREATE_PCB),
  c("Joint Origin", Icon::JointOrigin, D(Kind::Form(&assembly_forms::JOINT_ORIGIN))),
];

/// SOLID > MODIFY, as Fusion lists it with design history on; `$extra` ends the menu.
macro_rules! solid_modify {
  ($($extra:expr),*) => {
    &[
      c("Press Pull", Icon::PressPull, D(Kind::PressPull)).k("Q"),
      c("Fillet", Icon::Fillet, D(Kind::Fillet)).k("F"),
      c("Chamfer", Icon::Chamfer, D(Kind::Chamfer)),
      E::Sep,
      c("Shell", Icon::Shell, D(Kind::Shell)),
      c("Draft", Icon::Draft, D(Kind::Draft)),
      c("Scale", Icon::Scale, D(Kind::Scale)),
      c("Combine", Icon::Combine, D(Kind::Combine)),
      c("Offset Face", Icon::OffsetFace, D(Kind::OffsetFace)),
      c("Replace Face", Icon::ReplaceFace, D(Kind::Form(&forms::REPLACE_FACE))),
      c("Split Face", Icon::SplitFace, D(Kind::Form(&forms::SPLIT_FACE))),
      c("Split Body", Icon::SplitBody, D(Kind::SplitBody)),
      c("Silhouette Split", Icon::SilhouetteSplit, D(Kind::Form(&forms::SILHOUETTE_SPLIT))),
      E::Sep,
      c("Move/Copy", Icon::Move, D(Kind::Move)).k("M"),
      c("Align", Icon::Align, D(Kind::Form(&forms::ALIGN))),
      c("Delete", Icon::Remove, Cmd::Delete).k("Del"),
      c("Remove", Icon::RemoveBody, Cmd::RemoveBodies),
      E::Sub("Simplify", SIMPLIFY),
      E::Sep,
      c("Physical Material", Icon::PhysMaterial, D(Kind::Material)),
      c("Appearance", Icon::Appearance, D(Kind::Form(&forms::APPEARANCE))).k("A"),
      c("Manage Materials", Icon::ManageMaterials, Cmd::ManageMaterials),
      E::Sep,
      c("Change Parameters", Icon::Params, Cmd::Params),
      c("Compute All", Icon::ComputeAll, Cmd::ComputeAll).k("Ctrl+B"),
      E::Sep,
      c("Bill of Materials", Icon::Bom, D(Kind::Bom)),
      $($extra),*
    ]
  };
}

const SOLID_MODIFY: &[E] = solid_modify!();

/// With design history off Fusion edits faces directly; FissionOxide's direct edits then join the
/// menu (Fusion has no such entry while history is on).
const SOLID_MODIFY_DIRECT: &[E] = solid_modify!(E::Sep, E::Sub("Direct Edit", DIRECT_EDIT));

const DIRECT_EDIT: &[E] = &[
  c("Move Face", Icon::Move, D(Kind::Form(&config_ui::DIRECT_MOVE_FACE))),
  c("Press Pull", Icon::PressPull, D(Kind::Form(&config_ui::DIRECT_PRESS_PULL))),
  c("Delete Face", Icon::RemoveBody, D(Kind::Form(&config_ui::DIRECT_DELETE_FACE))),
  E::Sep,
  c("Create Base Feature", Icon::Box, Cmd::BaseFeature(true)),
  c("Finish Base Feature", Icon::Box, Cmd::BaseFeature(false)),
  c("Capture Design History", Icon::Box, Cmd::CaptureHistory),
];

const CONFIGURE: &[E] = &[
  c("Configure", Icon::Configure, Cmd::ConfigTable),
  c("Display Configuration Table", Icon::ConfigTable, Cmd::ConfigTable),
  x("Create Configuration Rules", Icon::ConfigRules),
];

const CONSTRUCT: &[E] = &[
  x("User Coordinate System (UCS)", Icon::Ucs),
  E::Sep,
  c("Offset Plane", Icon::Plane, D(Kind::OffsetPlane)),
  c("Plane at Angle", Icon::PlaneAngle, D(Kind::Form(&forms::PLANE_ANGLE))),
  c("Tangent Plane", Icon::PlaneTangent, D(Kind::Form(&forms::TANGENT_PLANE))),
  c("Midplane", Icon::Midplane, D(Kind::Form(&forms::MIDPLANE))),
  c("Perpendicular Plane", Icon::PlanePerpendicular, D(Kind::Form(&forms::PLANE_ALONG_PATH))),
  c("Plane Through Two Edges", Icon::PlaneTwoEdges, D(Kind::Form(&forms::PLANE_TWO_EDGES))),
  c("Plane Through Three Points", Icon::PlaneThreePoints, D(Kind::Form(&forms::PLANE_THREE_POINTS))),
  c("Plane Along Path", Icon::PlaneAlongPath, D(Kind::Form(&forms::PLANE_ALONG_PATH))),
  E::Sep,
  c("Axis Through Cylinder/Cone/Torus", Icon::AxisCylinder, D(Kind::Form(&forms::AXIS_FACE))),
  c("Axis Perpendicular To Face", Icon::AxisNormal, D(Kind::Form(&forms::AXIS_NORMAL))),
  c("Axis Through Two Planes", Icon::AxisTwoPlanes, D(Kind::Form(&forms::AXIS_TWO_PLANES))),
  c("Axis Through Two Points", Icon::AxisTwoPoints, D(Kind::Form(&forms::AXIS_TWO_POINTS))),
  c("Axis Through Edge", Icon::AxisEdge, D(Kind::Form(&forms::AXIS_EDGE))),
  E::Sep,
  c("Point At Vertex", Icon::PointVertex, D(Kind::Form(&forms::POINT_VERTEX))),
  c("Point Through Two Edges", Icon::PointTwoEdges, D(Kind::Form(&forms::POINT_TWO_EDGES))),
  c("Point Through Three Planes", Icon::PointThreePlanes, D(Kind::Form(&forms::POINT_THREE_PLANES))),
  c("Point At Center Of Circle/Sphere/Torus", Icon::PointCenter, D(Kind::Form(&forms::POINT_CENTER))),
  c("Point At Edge And Plane", Icon::PointEdgePlane, D(Kind::Form(&forms::POINT_EDGE_PLANE))),
  c("Point Along Path", Icon::PointAlongPath, D(Kind::Form(&forms::POINT_ALONG_PATH))),
];

const INSPECT: &[E] = &[
  c("Measure", Icon::Measure, D(Kind::Inspect)).k("I"),
  c("Interference", Icon::Interference, D(Kind::Interference)),
  c("Section Analysis", Icon::Section, D(Kind::Section)),
  E::Sep,
  x("Curvature Comb Analysis", Icon::CurvComb),
  c("Curvature Map Analysis", Icon::CurvMap, D(Kind::Analysis("curvature"))),
  c("Draft Analysis", Icon::DraftAnalysis, D(Kind::Analysis("draft"))),
  x("Environment Map Analysis", Icon::EnvMap),
  x("Isocurve Analysis", Icon::Isocurve),
  c("Zebra Analysis", Icon::Zebra, D(Kind::Analysis("zebra"))),
  E::Sep,
  c("Accessibility Analysis", Icon::Accessibility, D(Kind::Analysis("accessibility"))),
  c("Center of Mass", Icon::CenterOfMass, D(Kind::CenterOfMass)),
  x("Minimum Radius Analysis", Icon::MinRadius),
  E::Sep,
  x("Navigator", Icon::Navigator),
  E::Sep,
  x("Display Component Colors", Icon::ComponentColors).k("Shift+N"),
  x("Display Mesh Face Groups", Icon::FaceGroups).k("Shift+F"),
  E::Sep,
  x("History", Icon::History),
  x("Sustainability Insights", Icon::Sustainability),
];

const INSERT: &[E] = &[
  c("Decal", Icon::Decal, Cmd::Insert(Target::Decal)),
  c("Canvas", Icon::Canvas, Cmd::Canvas),
  c("Insert SVG", Icon::InsertSvg, Cmd::Insert(Target::Svg)),
  c("Insert DXF", Icon::InsertDxf, Cmd::InsertDxf),
  c("Insert Mesh", Icon::InsertMesh, Cmd::Insert(Target::Mesh)).g(),
  // greyed until the design is saved, as Fusion's
  c("Insert Component", Icon::InsertComponent, Cmd::InsertComponent).saved(),
  x("Insert Fastener", Icon::Fastener).off(),
  x("Duplicate With Joints", Icon::DuplicateJoints).off(),
  c("Insert Derive", Icon::Derive, Cmd::Insert(Target::Derive)),
  x("Insert McMaster-Carr Component", Icon::McMaster).off(),
  x("Insert a manufacturer part", Icon::Manufacturer).off(),
  x("Insert TraceParts Supplier Components", Icon::Manufacturer).off(),
];

// a Part Design document's ASSEMBLE holds only Add To Assembly, as Fusion's (FissionOxide makes
// part designs only)
const ASSEMBLE: &[E] = &[x("Add To Assembly", Icon::AddToAssembly)];

/// FissionOxide's component and joint commands, which Fusion's ASSEMBLE lists only in an
/// assembly design: not in a Part Design's dropdown, still found by the S toolbox and the
/// keyboard shortcuts (J, Shift+J).
const ASSEMBLE_MORE: &[E] = &[
  c("New Component", Icon::Component, D(Kind::NewComponent)),
  c("Joint", Icon::JointOrigin, D(Kind::Joint)).k("J"),
  c("As-built Joint", Icon::JointOrigin, D(Kind::AsBuiltJoint)).k("Shift+J"),
  c("Drive Joints", Icon::DriveJoint, D(Kind::DriveJoint)),
  c("Motion Study", Icon::MotionStudy, D(Kind::MotionStudy)),
  c("Duplicate Component", Icon::Component, D(Kind::CopyComponent)),
  c("Joint Origin", Icon::JointOrigin, D(Kind::Form(&assembly_forms::JOINT_ORIGIN))),
  c("Rigid Group", Icon::Component, D(Kind::Form(&assembly_forms::RIGID_GROUP))),
  c("Motion Link", Icon::DriveJoint, D(Kind::Form(&assembly_forms::MOTION_LINK))),
  c("Contact Set", Icon::Interference, D(Kind::Form(&assembly_forms::CONTACT_SET))),
  c("Cam Follower", Icon::DriveJoint, D(Kind::Form(&assembly_forms::CAM_FOLLOWER))),
];

// Fusion's Selection Tools (Select All and Select Other stay as commands: Select All is a
// filter row below, Select Other opens by holding the button on the model)
const SELECTION_TOOLS: &[E] = &[
  c("Select By Name", Icon::Select, Cmd::SelTool(SelTool::ByName)),
  c("Select By Boundary", Icon::Select, Cmd::SelTool(SelTool::ByBoundary)),
  c("Select By Size", Icon::Select, Cmd::SelTool(SelTool::BySize)),
  c("Invert Selection", Icon::Select, Cmd::SelectInvert),
  c("Seed And Boundary", Icon::Select, Cmd::SelTool(SelTool::SeedAndBoundary)),
];

// three, with no mark on the current one, as Fusion's (the component priority stays reachable
// through the Measure dialog's filter buttons)
const SELECTION_PRIORITY: &[E] = &[
  c("Select Body Priority", Icon::Body, Cmd::Priority(Priority::Body)),
  c("Select Face Priority", Icon::SplitFace, Cmd::Priority(Priority::Face)),
  c("Select Edge Priority", Icon::Line, Cmd::Priority(Priority::Edge)),
];

/// What the Selection Filters submenu says when hovered, as Fusion's.
const SELECTION_FILTERS_TIP: &str = "Specifies which object types can be selected. Use Select All to include or \
  exclude all object types. Check only specific types to limit selection to those types. Check Select Through to \
  enable selection of hidden objects during Window or Freeform selection.";

// Fusion's list, every type checked by default (see `select::FILTER_TYPES`)
const SELECTION_FILTERS: &[E] = &[
  c("Select All", Icon::Select, Cmd::Filter("all")),
  c("Select Through", Icon::Eye, Cmd::Filter("through")),
  c("Bodies", Icon::Body, Cmd::Filter("bodies")),
  c("Body Edges", Icon::Line, Cmd::Filter("edges")),
  c("Body Faces", Icon::SplitFace, Cmd::Filter("faces")),
  c("Body Vertices", Icon::Point, Cmd::Filter("vertices")),
  c("Canvas", Icon::Canvas, Cmd::Filter("canvas")),
  c("Components", Icon::Component, Cmd::Filter("components")),
  c("Construction Geometry", Icon::Plane, Cmd::Filter("construction")),
  c("Coordinate Systems", Icon::Ucs, Cmd::Filter("coordinate_systems")),
  c("Custom Graphics", Icon::Display, Cmd::Filter("custom_graphics")),
  c("Decal", Icon::Decal, Cmd::Filter("decal")),
  c("Dimension", Icon::Dimension, Cmd::Filter("dimension")),
  c("Features", Icon::History, Cmd::Filter("features")),
  c("Joint Origins", Icon::JointOrigin, Cmd::Filter("joint_origins")),
  c("Joints", Icon::DriveJoint, Cmd::Filter("joints")),
  c("Mesh Bodies", Icon::InsertMesh, Cmd::Filter("mesh_bodies")),
  c("Mesh Face Groups", Icon::FaceGroups, Cmd::Filter("mesh_face_groups")),
  c("Mesh Faces", Icon::Tessellate, Cmd::Filter("mesh_faces")),
  c("PMI", Icon::FeatureControl, Cmd::Filter("pmi")),
  c("Sketch Curves", Icon::SketchSmall, Cmd::Filter("sketch_curves")),
  c("Sketch Geometry Constraint", Icon::Coincident, Cmd::Filter("sketch_constraints")),
  c("Sketch Points", Icon::Point, Cmd::Filter("sketch_points")),
  c("Sketch Profiles", Icon::Rect, Cmd::Filter("profiles")),
  c("Sketch Surface", Icon::Patch, Cmd::Filter("sketch_surface")),
  c("T-Spline Body", Icon::CreateForm, Cmd::Filter("tspline")),
  c("Text Selection", Icon::SkText, Cmd::Filter("text")),
];

const SELECT: &[E] = &[
  c("Select", Icon::Select, Cmd::Select),
  E::Sep,
  c("Window Selection", Icon::WindowSel, Cmd::WindowSelect).k("1"),
  c("Freeform Selection", Icon::FreeformSel, Cmd::SelectMode(crate::select::Mode::Freeform)).k("2"),
  c("Paint Selection", Icon::PaintSel, Cmd::SelectMode(crate::select::Mode::Paint)).k("3"),
  E::Sep,
  E::Sub("Selection Tools", SELECTION_TOOLS),
  E::Sep,
  E::Sub("Selection Priority", SELECTION_PRIORITY),
  E::Sub("Selection Filters", SELECTION_FILTERS),
];

const SURFACE_CREATE: &[E] = &[
  c("Create Sketch", Icon::Sketch, Cmd::NewSketch),
  c("Create Form", Icon::CreateForm, Cmd::Form(FormAct::Enter)),
  x("Derive", Icon::Derive),
  E::Sep,
  c("Extrude", Icon::Extrude, D(Kind::Form(&forms::SURFACE_EXTRUDE))).o().surface(),
  c("Revolve", Icon::Revolve, D(Kind::Form(&forms::SURFACE_REVOLVE))).o(),
  x("Sweep", Icon::Sweep).o().surface(),
  x("Loft", Icon::Loft).o(),
  c("Patch", Icon::Patch, D(Kind::Form(&forms::PATCH))).o(),
  c("Ruled", Icon::Ruled, D(Kind::Form(&forms::RULED))).o(),
  c("Offset", Icon::SurfOffset, D(Kind::Form(&forms::SURFACE_OFFSET))).o(),
  E::Sep,
  E::Sub("Pattern", PATTERN),
  c("Mirror", Icon::Mirror, D(Kind::Mirror)),
  E::Sep,
  c("Thicken", Icon::Thicken, D(Kind::Form(&forms::THICKEN))),
  c("Boundary Fill", Icon::BoundaryFill, D(Kind::Form(&forms::BOUNDARY_FILL))),
  c("Joint Origin", Icon::JointOrigin, D(Kind::Form(&assembly_forms::JOINT_ORIGIN))),
];

const SURFACE_MODIFY: &[E] = &[
  c("Press Pull", Icon::PressPull, D(Kind::PressPull)).k("Q"),
  c("Fillet", Icon::Fillet, D(Kind::Fillet)).k("F"),
  c("Chamfer", Icon::Chamfer, D(Kind::Chamfer)),
  E::Sep,
  c("Trim", Icon::Trim, D(Kind::Form(&forms::TRIM))).o(),
  c("Untrim", Icon::Untrim, D(Kind::Form(&forms::UNTRIM))).o(),
  c("Extend", Icon::Extend, D(Kind::Form(&forms::EXTEND))).o(),
  c("Stitch", Icon::Stitch, D(Kind::Form(&forms::STITCH))).o(),
  c("Unstitch", Icon::Unstitch, D(Kind::Form(&forms::UNSTITCH))).o(),
  c("Reverse Normal", Icon::ReverseNormal, D(Kind::Form(&forms::REVERSE_NORMAL))).o(),
  c("Scale", Icon::Scale, D(Kind::Scale)),
  c("Split Face", Icon::SplitFace, D(Kind::Form(&forms::SPLIT_FACE))),
  c("Split Body", Icon::SplitBody, D(Kind::SplitBody)),
  E::Sep,
  c("Move/Copy", Icon::Move, D(Kind::Move)).k("M"),
  c("Align", Icon::Align, D(Kind::Form(&forms::ALIGN))),
  c("Delete", Icon::Remove, Cmd::Delete).k("Del"),
  c("Remove", Icon::RemoveBody, Cmd::RemoveBodies),
  E::Sep,
  c("Physical Material", Icon::PhysMaterial, D(Kind::Material)),
  c("Appearance", Icon::Appearance, D(Kind::Form(&forms::APPEARANCE))).k("A"),
  c("Manage Materials", Icon::ManageMaterials, Cmd::ManageMaterials),
  E::Sep,
  c("Change Parameters", Icon::Params, Cmd::Params),
  c("Compute All", Icon::ComputeAll, Cmd::ComputeAll).k("Ctrl+B"),
  E::Sep,
  c("Bill of Materials", Icon::Bom, D(Kind::Bom)),
];

const MESH_CREATE: &[E] = &[
  c("Insert Mesh", Icon::InsertMesh, Cmd::Insert(Target::Mesh)).g(),
  x("Tessellate", Icon::Tessellate).g(),
  E::Sep,
  x("Create Mesh Section Sketch", Icon::MeshSection).g(),
  x("Create Base Mesh Feature", Icon::BaseMesh).g(),
];

const MESH_PREPARE: &[E] = &[
  x("Repair", Icon::Repair).g(),
  x("Generate Face Groups", Icon::GenFaceGroups),
  x("Combine Face Groups", Icon::CombineFaceGroups),
];

const EXPORT: &[E] = &[c("3D Print", Icon::Print3d, Cmd::Print3d)];

const SHEET_CREATE: &[E] = &[
  c("Create Sketch", Icon::Sketch, Cmd::NewSketch),
  c("Flange (Base/Edge/Contour)", Icon::Flange, D(Kind::Flange)),
  c("Hem", Icon::Hem, D(Kind::Hem)),
  c("Lofted Flange", Icon::LoftedFlange, D(Kind::Form(&forms::LOFTED_FLANGE))),
  c("Fold", Icon::Fold, D(Kind::Fold)),
  c("Convert to Sheet Metal", Icon::ConvertSheet, D(Kind::ConvertSheet)).k("Ctrl+Alt+S"),
  x("Derive", Icon::Derive),
  E::Sep,
  c("Extrude", Icon::Extrude, D(Kind::Extrude)).k("E"),
  E::Sep,
  c("Hole", Icon::Hole, D(Kind::Hole)).k("H"),
  c("Thread", Icon::Thread, D(Kind::Form(&forms::THREAD))),
  E::Sub("Pattern", PATTERN),
  c("Mirror", Icon::Mirror, D(Kind::Mirror)),
  E::Sep,
  c("Create Flat Pattern", Icon::FlatPattern, D(Kind::FlatPattern)),
  c("Joint Origin", Icon::JointOrigin, D(Kind::Form(&assembly_forms::JOINT_ORIGIN))),
];

/// Sheet metal commands FissionOxide has that Fusion's CREATE and MODIFY dropdowns don't list:
/// not in the dropdowns, still found by the S toolbox and the keyboard shortcuts.
const SHEET_MORE: &[E] = &[
  c("Miter Flange", Icon::Flange, D(Kind::Form(&forms::MITER_FLANGE))),
  c("Flat Pattern Sketch", Icon::FlatPattern, D(Kind::Form(&forms::FLAT_SKETCH))),
  c("All Flat Patterns on One Sheet", Icon::FlatPattern, D(Kind::Form(&forms::COLLECT_FLAT))),
  c("Cut Service Check", Icon::Interference, Cmd::CutCheck),
  c("Refold Faces", Icon::Refold, D(Kind::Refold)),
];

const SHEET_MODIFY: &[E] = &[
  c("Unfold", Icon::Unfold, D(Kind::Unfold)),
  c("Sheet Metal Rules", Icon::SmRules, D(Kind::SheetRules)),
  c("Corner Closure", Icon::CornerClosure, D(Kind::Form(&forms::CORNER_CLOSURE))),
  c("Join by bend", Icon::Fold, D(Kind::Form(&forms::JOIN_BY_BEND))).p(),
  c("Rip", Icon::Rip, D(Kind::Rip)),
  E::Sep,
  c("Sheet Metal Fillet", Icon::Fillet, D(Kind::Form(&forms::SHEET_FILLET))).p(),
  c("Sheet Metal Chamfer", Icon::Chamfer, D(Kind::Form(&forms::SHEET_CHAMFER))).p(),
  c("Fillet", Icon::Fillet, D(Kind::Fillet)).k("F"),
  c("Chamfer", Icon::Chamfer, D(Kind::Chamfer)),
  E::Sep,
  c("Move/Copy", Icon::Move, D(Kind::Move)).k("M"),
  c("Align", Icon::Align, D(Kind::Form(&forms::ALIGN))),
  c("Delete", Icon::Remove, Cmd::Delete).k("Del"),
  c("Remove", Icon::RemoveBody, Cmd::RemoveBodies),
  E::Sep,
  c("Physical Material", Icon::PhysMaterial, D(Kind::Material)),
  c("Appearance", Icon::Appearance, D(Kind::Form(&forms::APPEARANCE))).k("A"),
  c("Manage Materials", Icon::ManageMaterials, Cmd::ManageMaterials),
  E::Sep,
  c("Change Parameters", Icon::Params, Cmd::Params),
  c("Compute All", Icon::ComputeAll, Cmd::ComputeAll).k("Ctrl+B"),
  E::Sep,
  c("Bill of Materials", Icon::Bom, D(Kind::Bom)),
];

const MESH_MODIFY: &[E] = &[
  x("Direct Edit", Icon::DirectEdit).g(),
  x("Remesh", Icon::Remesh).g(),
  c("Reduce", Icon::Reduce, D(Kind::Form(&forms::MESH_REDUCE))).g(),
  c("Plane Cut", Icon::PlaneCut, D(Kind::SplitBody)).g(),
  x("Shell", Icon::Shell),
  x("Combine", Icon::Combine).g(),
  c("Smooth", Icon::Smooth, D(Kind::Form(&forms::MESH_SMOOTH))).g(),
  c("Reverse Normal", Icon::ReverseNormal, D(Kind::Form(&forms::REVERSE_NORMAL))).g(),
  E::Sep,
  x("Separate", Icon::Separate).g(),
  E::Sep,
  c("Move/Copy", Icon::Move, D(Kind::Move)).k("M"),
  x("Scale Mesh", Icon::ScaleMesh).g(),
  c("Delete", Icon::Remove, Cmd::Delete).k("Del"),
  c("Remove", Icon::RemoveBody, Cmd::RemoveBodies),
  E::Sep,
  c("Convert Mesh", Icon::ConvertMesh, D(Kind::Form(&forms::MESH_CONVERT))).g(),
  E::Sep,
  c("Physical Material", Icon::PhysMaterial, D(Kind::Material)),
  c("Appearance", Icon::Appearance, D(Kind::Form(&forms::APPEARANCE))).k("A"),
  E::Sep,
  c("Change Parameters", Icon::Params, Cmd::Params),
  c("Compute All", Icon::ComputeAll, Cmd::ComputeAll).k("Ctrl+B"),
  E::Sep,
  c("Bill of Materials", Icon::Bom, D(Kind::Bom)),
];

/// Groups every Design tab ends with (and any the tab adds after them).
macro_rules! shared_groups {
  ($($extra:expr),*) => {
    [
      Group {
        label: "CONFIGURE",
        quick: &["Configure", "Display Configuration Table", "Create Configuration Rules"],
        menu: CONFIGURE,
      },
      Group { label: "CONSTRUCT", quick: &["Offset Plane"], menu: CONSTRUCT },
      Group { label: "INSPECT", quick: &["Measure", "Section Analysis"], menu: INSPECT },
      Group { label: "INSERT", quick: &["Insert Derive", "Canvas", "Insert Mesh"], menu: INSERT },
      Group { label: "ASSEMBLE", quick: &["Add To Assembly"], menu: ASSEMBLE },
      Group { label: "SELECT", quick: &["Select"], menu: SELECT },
      $($extra),*
    ]
  };
}

const SHARED: [Group; 6] = shared_groups!();

// ---------------------------------------------------------------- Assembly Design

// an Assembly Design's ASSEMBLY tab, in Fusion's order
const ASM_INSERT: &[E] = &[
  c("Insert Component", Icon::InsertComponent, Cmd::InsertComponent),
  x("Insert Fastener", Icon::Fastener).off(),
  x("Duplicate With Joints", Icon::DuplicateJoints).off(),
  c("Insert Derive", Icon::Derive, Cmd::Insert(Target::Derive)),
  c("Decal", Icon::Decal, Cmd::Insert(Target::Decal)),
  c("Canvas", Icon::Canvas, Cmd::Canvas),
  E::Sep,
  x("Add To Assembly", Icon::AddToAssembly),
  E::Sep,
  x("Insert McMaster-Carr Component", Icon::McMaster).off(),
];

const ASM_CREATE: &[E] = &[
  c("New Component", Icon::Component, D(Kind::NewComponent)),
  c("Create Sketch", Icon::Sketch, Cmd::NewSketch),
  E::Sub("Pattern", PATTERN),
  x("Create Linked PCB", Icon::InsertComponent),
  x("Derive", Icon::Derive),
];

const ASM_RELATIONSHIPS: &[E] = &[
  x("Constrain Components", Icon::Coincident),
  c("Align Constraint", Icon::Align, D(Kind::Form(&forms::ALIGN))),
  x("Angle Constraint", Icon::AngleDim),
  x("Center Constraint", Icon::Concentric),
  x("Tangent Constraint", Icon::Tangent),
  E::Sep,
  c("Joint", Icon::JointOrigin, D(Kind::Joint)).k("J"),
  c("As-built Joint", Icon::JointOrigin, D(Kind::AsBuiltJoint)).k("Shift+J"),
  c("Joint Origin", Icon::JointOrigin, D(Kind::Form(&assembly_forms::JOINT_ORIGIN))),
  c("Rigid Group", Icon::Component, D(Kind::Form(&assembly_forms::RIGID_GROUP))),
];

const ASM_MOTION: &[E] = &[
  c("Drive Joints", Icon::DriveJoint, D(Kind::DriveJoint)),
  c("Motion Link", Icon::DriveJoint, D(Kind::Form(&assembly_forms::MOTION_LINK))),
  c("Motion Study", Icon::MotionStudy, D(Kind::MotionStudy)),
];

const ASM_MODIFY: &[E] = &[
  c("Move/Copy", Icon::Move, D(Kind::Move)).k("M"),
  c("Align", Icon::Align, D(Kind::Form(&forms::ALIGN))),
  c("Delete", Icon::Remove, Cmd::Delete).k("Del"),
  c("Remove", Icon::RemoveBody, Cmd::RemoveBodies),
  E::Sep,
  c("Bill of Materials", Icon::Bom, D(Kind::Bom)),
  E::Sep,
  c("Change Parameters", Icon::Params, Cmd::Params),
  c("Physical Material", Icon::PhysMaterial, D(Kind::Material)),
  c("Appearance", Icon::Appearance, D(Kind::Form(&forms::APPEARANCE))).k("A"),
  E::Sep,
  x("Manage Materials", Icon::ManageMaterials),
  E::Sep,
  c("Compute All", Icon::ComputeAll, Cmd::ComputeAll).k("Ctrl+B"),
];

const ASM_CONTACT: &[E] = &[x("Enable Contact Sets", Icon::Interference), x("Enable All Contact", Icon::Interference)];

const ASSEMBLY: [Group; 6] = [
  Group { label: "INSERT", quick: &["Insert Component", "Insert Derive", "Canvas"], menu: ASM_INSERT },
  Group { label: "CREATE", quick: &["New Component", "Create Sketch"], menu: ASM_CREATE },
  Group {
    label: "RELATIONSHIPS",
    quick: &["Constrain Components", "Joint", "As-built Joint", "Rigid Group"],
    menu: ASM_RELATIONSHIPS,
  },
  Group { label: "MOTION", quick: &["Drive Joints", "Motion Link", "Motion Study"], menu: ASM_MOTION },
  Group { label: "MODIFY", quick: &["Move/Copy", "Align", "Bill of Materials"], menu: ASM_MODIFY },
  Group { label: "CONTACT", quick: &["Enable Contact Sets"], menu: ASM_CONTACT },
];

// the ASSEMBLY tab ends with CONFIGURE, CONSTRUCT, INSPECT and SELECT (INSERT comes first)
const ASSEMBLY_SHARED: [Group; 4] = [
  Group {
    label: "CONFIGURE",
    quick: &["Configure", "Display Configuration Table", "Create Configuration Rules"],
    menu: CONFIGURE,
  },
  Group { label: "CONSTRUCT", quick: &["Offset Plane"], menu: CONSTRUCT },
  Group { label: "INSPECT", quick: &["Measure", "Section Analysis"], menu: INSPECT },
  Group { label: "SELECT", quick: &["Select"], menu: SELECT },
];

// as in Fusion, only MESH has an EXPORT panel (elsewhere 3D Print is under UTILITIES > MAKE)
const MESH_SHARED: [Group; 7] = shared_groups!(Group { label: "EXPORT", quick: &["3D Print"], menu: EXPORT });

// Fusion's SOLID ribbon: CREATE 5 icons, MODIFY 4; the rest stay in the panel menus
const SOLID: [Group; 2] = [
  Group {
    label: "CREATE",
    quick: &["Create Sketch", "Extrude", "Revolve", "Hole", "Rectangular Pattern"],
    menu: SOLID_CREATE,
  },
  Group { label: "MODIFY", quick: &["Press Pull", "Fillet", "Shell", "Combine"], menu: SOLID_MODIFY },
];

const SURFACE: [Group; 2] = [
  Group { label: "CREATE", quick: &["Create Sketch", "Patch", "Extrude", "Revolve", "Sweep"], menu: SURFACE_CREATE },
  Group { label: "MODIFY", quick: &["Stitch", "Unstitch", "Trim", "Extend"], menu: SURFACE_MODIFY },
];

use Cmd::SketchTool as T;

const SK_RECT: &[E] = &[
  c("2-Point Rectangle", Icon::Rect, T(Tool::Rect)).k("R"),
  c("3-Point Rectangle", Icon::Rect3, T(Tool::Rect3)),
  c("Center Rectangle", Icon::CenterRect, T(Tool::CenterRect)),
];

const SK_CIRCLE: &[E] = &[
  c("Center Diameter Circle", Icon::Circle, T(Tool::Circle)).k("C"),
  c("2-Point Circle", Icon::Circle2P, T(Tool::Circle2P)),
  c("3-Point Circle", Icon::Circle3P, T(Tool::Circle3P)),
  c("2-Tangent Circle", Icon::Circle2T, T(Tool::Circle2T)),
  c("3-Tangent Circle", Icon::Circle3T, T(Tool::Circle3T)),
];

const SK_ARC: &[E] = &[
  c("3-Point Arc", Icon::Arc3, T(Tool::Arc3)),
  c("Center Point Arc", Icon::CenterArc, T(Tool::CenterArc)),
  c("Tangent Arc", Icon::TangentArc, T(Tool::TangentArc)),
];

const SK_POLYGON: &[E] = &[
  c("Circumscribed Polygon", Icon::PolygonCirc, T(Tool::CircPolygon)),
  c("Inscribed Polygon", Icon::Polygon, T(Tool::Polygon)),
  c("Edge Polygon", Icon::PolygonEdge, T(Tool::EdgePolygon)),
];

const SK_SLOT: &[E] = &[
  c("Center to Center Slot", Icon::Slot, T(Tool::Slot)),
  c("Overall Slot", Icon::OverallSlot, T(Tool::OverallSlot)),
  c("Center Point Slot", Icon::CenterPointSlot, T(Tool::CenterSlot)),
  c("Three Point Arc Slot", Icon::ArcSlot3, T(Tool::ArcSlot3)),
  c("Center Point Arc Slot", Icon::ArcSlotCenter, T(Tool::ArcSlotCenter)),
];

const SK_SPLINE: &[E] = &[
  c("Fit Point Spline", Icon::SplineFit, T(Tool::Spline)),
  c("Control Point Spline", Icon::SplineCtrl, T(Tool::ControlSpline)),
];

const SK_PROJECT: &[E] = &[
  c("Project", Icon::Project, D(Kind::Project)).k("P"),
  c("Intersect", Icon::Intersect, D(Kind::Form(&forms::SK_INTERSECT))),
  c("Spun Profile", Icon::Revolve, D(Kind::Form(&forms::SK_SPUN_PROFILE))),
  c("Include 3D Geometry", Icon::Include3D, D(Kind::Form(&forms::SK_INCLUDE))),
  c("Project To Surface", Icon::ProjectSurface, D(Kind::Form(&forms::SK_PROJECT_SURFACE))),
  c("Intersection Curve", Icon::Intersect, D(Kind::Form(&forms::SK_INTERSECTION_CURVE))),
  c("Isoparametric Curve", Icon::Intersect, D(Kind::Form(&forms::SK_ISOPARAMETRIC))),
];

const SKETCH_CREATE: &[E] = &[
  c("Line", Icon::Line, T(Tool::Line)).k("L"),
  c("Midpoint Line", Icon::MidpointLine, T(Tool::MidpointLine)),
  E::Sub("Rectangle", SK_RECT),
  E::Sub("Circle", SK_CIRCLE),
  E::Sub("Arc", SK_ARC),
  E::Sub("Polygon", SK_POLYGON),
  c("Ellipse", Icon::Ellipse, T(Tool::Ellipse)),
  E::Sub("Slot", SK_SLOT),
  E::Sub("Spline", SK_SPLINE),
  c("Conic Curve", Icon::Conic, T(Tool::Conic)),
  c("Point", Icon::Point, T(Tool::Point)),
  c("Text", Icon::SkText, D(Kind::Form(&forms::SK_TEXT))),
  // Trace Image isn't a Fusion CREATE command: it sits in INSERT, next to Canvas
  E::Sep,
  c("Mirror", Icon::SkMirror, D(Kind::SkMirror)),
  c("Circular Pattern", Icon::SkCircPattern, D(Kind::SkCircPattern)),
  c("Rectangular Pattern", Icon::SkRectPattern, D(Kind::SkRectPattern)),
  E::Sep,
  E::Sub("Project / Include", SK_PROJECT),
  E::Sep,
  c("Sketch Dimension", Icon::Dimension, T(Tool::Dimension)).k("D"),
];

const SK_CHAMFER: &[E] = &[
  c("Equal Distance Chamfer", Icon::SkChamfer, T(Tool::Chamfer)),
  c("Distance and Angle Chamfer", Icon::SkChamferAngle, T(Tool::ChamferAngle)),
  c("Two Distance Chamfer", Icon::SkChamfer2, T(Tool::Chamfer2)),
];

const SKETCH_MODIFY: &[E] = &[
  c("Fillet", Icon::SkFillet, T(Tool::Fillet)),
  E::Sub("Chamfer", SK_CHAMFER),
  c("Blend Curve", Icon::BlendCurve, T(Tool::Blend)),
  c("Offset", Icon::SkOffset, D(Kind::SkOffset)).k("O"),
  E::Sep,
  c("Trim", Icon::SkTrim, T(Tool::Trim)).k("T"),
  c("Extend", Icon::SkExtend, T(Tool::Extend)),
  c("Break", Icon::SkBreak, T(Tool::Break)),
  c("Sketch Scale", Icon::SkScale, D(Kind::SkScale)),
  c("Move/Copy", Icon::Move, D(Kind::Move)).k("M"),
  E::Sep,
  c("Change Parameters", Icon::Params, Cmd::Params),
];

/// Sketch commands FissionOxide has that Fusion's MODIFY dropdown doesn't list: not in the
/// dropdown, still found by the S toolbox and the keyboard shortcuts.
const SKETCH_MODIFY_MORE: &[E] = &[
  c("Scale to Size", Icon::SkScaleTo, D(Kind::Form(&forms::SK_SCALE_TO))),
  c("Edit Text", Icon::SkText, D(Kind::Form(&forms::SK_TEXT_EDIT))),
  c("Explode Text", Icon::SkText, D(Kind::Form(&forms::SK_TEXT_EXPLODE))),
  c("Remove Duplicates", Icon::SkRemoveDuplicates, D(Kind::Form(&forms::SK_REMOVE_DUPLICATES))),
  c("Clean Up", Icon::SkCleanup, D(Kind::Form(&forms::SK_CLEANUP))),
];

/// A tab's commands that its dropdowns don't show (see [`SKETCH_MODIFY_MORE`] and
/// [`ASSEMBLE_MORE`]).
fn extra_commands(tab: Tab) -> &'static [&'static [E]] {
  match tab {
    Tab::Sketch => &[SKETCH_MODIFY_MORE, ASSEMBLE_MORE],
    Tab::SheetMetal => &[ASSEMBLE_MORE, SHEET_MORE],
    Tab::Solid | Tab::Surface | Tab::Mesh | Tab::Plastic => &[ASSEMBLE_MORE],
    _ => &[],
  }
}

const SKETCH_CONSTRAINTS: &[E] = &[
  c("Horizontal/Vertical", Icon::HorizVert, Cmd::Constraint("hv")),
  c("Coincident", Icon::Coincident, Cmd::Constraint("coincident")),
  c("Tangent", Icon::Tangent, Cmd::Constraint("tangent")),
  c("Equal", Icon::Equal, Cmd::Constraint("equal")),
  c("Parallel", Icon::Parallel, Cmd::Constraint("parallel")),
  c("Perpendicular", Icon::Perpendicular, Cmd::Constraint("perpendicular")),
  c("Fix/UnFix", Icon::Fix, Cmd::Constraint("fix")),
  c("MidPoint", Icon::Midpoint, Cmd::Constraint("midpoint")),
  c("Concentric", Icon::Concentric, Cmd::Constraint("concentric")),
  c("Collinear", Icon::Collinear, Cmd::Constraint("collinear")),
  c("Symmetry", Icon::Symmetric, Cmd::Constraint("symmetric")),
  c("Curvature", Icon::Curvature, Cmd::Constraint("curvature")),
  c("Polygon", Icon::PolygonCon, Cmd::Constraint("polygon")),
];

// the sketch tab's INSERT has no Insert Component
const SKETCH_INSERT: &[E] = &[
  c("Trace Image", Icon::TraceImage, D(Kind::TraceImage)),
  c("Decal", Icon::Decal, Cmd::Insert(Target::Decal)),
  c("Canvas", Icon::Canvas, Cmd::Canvas),
  c("Insert SVG", Icon::InsertSvg, Cmd::Insert(Target::Svg)),
  c("Insert DXF", Icon::InsertDxf, Cmd::InsertDxf),
  c("Sketch Library...", Icon::SketchLibrary, Cmd::SketchLibrary),
  c("Insert Mesh", Icon::InsertMesh, Cmd::Insert(Target::Mesh)).g(),
  x("Insert Fastener", Icon::Fastener).off(),
  x("Duplicate With Joints", Icon::DuplicateJoints).off(),
  c("Insert Derive", Icon::Derive, Cmd::Insert(Target::Derive)),
  x("Insert McMaster-Carr Component", Icon::McMaster).off(),
  x("Insert a manufacturer part", Icon::Manufacturer).off(),
  x("Insert TraceParts Supplier Components", Icon::Manufacturer).off(),
];

const FINISH: &[E] =
  &[c("Finish Sketch", Icon::Finish, Cmd::FinishSketch), x("Finish with AutoConstrain", Icon::FinishAuto)];

/// The contextual SKETCH tab, shown only while a sketch is open.
const SKETCH: [Group; 9] = [
  Group {
    label: "CREATE",
    quick: &[
      "Line",
      "2-Point Rectangle",
      "Center Diameter Circle",
      "3-Point Arc",
      "Fit Point Spline",
      "Mirror",
      "Sketch Dimension",
    ],
    menu: SKETCH_CREATE,
  },
  Group { label: "MODIFY", quick: &["Fillet", "Trim", "Extend", "Offset", "Blend Curve"], menu: SKETCH_MODIFY },
  Group {
    label: "CONSTRAINTS",
    quick: &[
      "Horizontal/Vertical",
      "Coincident",
      "Tangent",
      "Equal",
      "Parallel",
      "Perpendicular",
      "Fix/UnFix",
      "MidPoint",
      "Concentric",
      "Collinear",
      "Symmetry",
      "Curvature",
    ],
    menu: SKETCH_CONSTRAINTS,
  },
  Group {
    label: "CONFIGURE",
    quick: &["Configure", "Display Configuration Table", "Create Configuration Rules"],
    menu: CONFIGURE,
  },
  Group { label: "INSPECT", quick: &["Measure", "Section Analysis"], menu: INSPECT },
  Group { label: "INSERT", quick: &["Insert Derive", "Canvas", "Insert Mesh"], menu: SKETCH_INSERT },
  Group { label: "ASSEMBLE", quick: &["Add To Assembly"], menu: ASSEMBLE },
  Group { label: "SELECT", quick: &["Select"], menu: SELECT },
  Group { label: "FINISH SKETCH", quick: &["Finish Sketch"], menu: FINISH },
];

use Cmd::DrawingExport as X;
use Cmd::DrawingView as V;

const DRAWING_CREATE: &[E] = &[
  c("Base View", Icon::BaseView, V("front")),
  c("Projected View", Icon::ProjectedView, V("top")),
  E::Sep,
  c("Section View", Icon::SectionView, V("section")),
  c("Detail View", Icon::DetailView, V("detail")),
  c("Flat Pattern View", Icon::FlatPattern, V("flat")),
  E::Sep,
  c("Create Drawing", Icon::NewDrawing, D(Kind::NewDrawing)),
  E::Sep,
  c("Break View", Icon::BreakView, D(Kind::Form(&forms::DRAWING_BREAK))),
];

const DRAWING_MODIFY: &[E] = &[
  c("Move", Icon::Move, D(Kind::DrawingMoveView)).k("M"),
  c("Delete", Icon::Remove, D(Kind::DrawingMoveView)),
  E::Sep,
  c("Sheet Settings", Icon::SheetSettings, D(Kind::DrawingSettings)),
  E::Sep,
  c("New Sheet", Icon::Plus, D(Kind::Form(&crate::sheet::DRAWING_NEW_SHEET))),
  c("Edit Sheet", Icon::SheetSettings, D(Kind::Form(&crate::sheet::DRAWING_EDIT_SHEET))),
];

const DRAWING_DIMENSIONS: &[E] = &[
  c("Dimension", Icon::Dimension, D(Kind::DrawingDimension)).k("D"),
  c("Ordinate Dimension", Icon::OrdinateDim, D(Kind::Form(&forms::DRAWING_ORDINATE))),
  c("Angular Dimension", Icon::AngleDim, D(Kind::Form(&forms::DRAWING_ANGLE))),
  c("Baseline Dimension", Icon::OrdinateDim, D(Kind::Form(&crate::sheet::DRAWING_BASELINE))),
  c("Chain Dimension", Icon::Dimension, D(Kind::Form(&crate::sheet::DRAWING_CHAIN))),
];

const DRAWING_TEXT: &[E] = &[c("Text", Icon::SkText, D(Kind::DrawingNote)).k("T")];

const DRAWING_TABLES: &[E] = &[
  c("Parts List", Icon::PartsList, D(Kind::DrawingSettings)),
  c("Title Block", Icon::TitleBlock, D(Kind::DrawingTitle)),
  E::Sep,
  c("Balloon", Icon::Balloon, D(Kind::Form(&forms::DRAWING_BALLOON))),
  c("Auto Balloon", Icon::Balloon, D(Kind::DrawingSettings)),
  E::Sep,
  c("Revision Table", Icon::TitleBlock, D(Kind::Form(&crate::sheet::DRAWING_REVISION))),
];

const DRAWING_SYMBOLS: &[E] = &[
  c("Feature Control Frame", Icon::FeatureControl, D(Kind::Form(&forms::DRAWING_FCF))),
  c("Datum Identifier", Icon::DatumId, D(Kind::Form(&forms::DRAWING_DATUM))),
  c("Surface Texture", Icon::SurfaceTexture, D(Kind::Form(&forms::DRAWING_FINISH))),
  c("Weld Symbol", Icon::Fillet, D(Kind::Form(&crate::sheet::DRAWING_WELD))),
  c("Datum Target", Icon::DatumId, D(Kind::Form(&crate::sheet::DRAWING_DATUM_TARGET))),
];

const DRAWING_EXPORT: &[E] = &[
  c("PDF Export", Icon::ExportPdf, X("pdf")),
  c("DXF Export", Icon::ExportDxf, X("dxf")),
  c("SVG Export", Icon::ExportSvg, X("svg")),
];

const FINISH_DRAWING: &[E] = &[c("Finish Drawing", Icon::Finish, Cmd::FinishDrawing)];

/// The Drawing workspace's toolbar.
const DRAWING: [Group; 8] = [
  Group {
    label: "CREATE",
    quick: &["Base View", "Projected View", "Section View", "Detail View"],
    menu: DRAWING_CREATE,
  },
  Group { label: "MODIFY", quick: &["Move", "Sheet Settings"], menu: DRAWING_MODIFY },
  Group { label: "DIMENSIONS", quick: &["Dimension"], menu: DRAWING_DIMENSIONS },
  Group { label: "TEXT", quick: &["Text"], menu: DRAWING_TEXT },
  Group {
    label: "SYMBOLS",
    quick: &["Feature Control Frame", "Datum Identifier", "Surface Texture"],
    menu: DRAWING_SYMBOLS,
  },
  Group { label: "TABLES", quick: &["Parts List", "Title Block", "Balloon"], menu: DRAWING_TABLES },
  Group { label: "EXPORT", quick: &["PDF Export", "DXF Export", "SVG Export"], menu: DRAWING_EXPORT },
  Group { label: "FINISH DRAWING", quick: &["Finish Drawing"], menu: FINISH_DRAWING },
];

const CAM_SETUP: &[E] = &[
  c("New Setup", Icon::Box, D(Kind::Form(&forms::CAM_SETUP))),
  c("Fixtures", Icon::Component, D(Kind::Form(&forms::CAM_FIXTURES))),
  E::Sep,
  c("Tool Library", Icon::Configure, Cmd::CamTools),
];

const CAM_2D: &[E] = &[
  c("Face", Icon::Extrude, D(Kind::Form(&forms::CAM_FACE))),
  c("2D Pocket", Icon::Shell, D(Kind::Form(&forms::CAM_POCKET))),
  c("2D Contour", Icon::SkOffset, D(Kind::Form(&forms::CAM_CONTOUR))),
  c("2D Adaptive Clearing", Icon::Shell, D(Kind::Form(&forms::CAM_ADAPTIVE))),
  E::Sep,
  x("Slot", Icon::Slot),
  x("Trace", Icon::Line),
  x("Thread", Icon::Thread),
  x("Bore", Icon::Hole),
  x("Engrave", Icon::SkText),
];

const CAM_3D: &[E] = &[
  c("Parallel", Icon::Zebra, D(Kind::Form(&forms::CAM_PARALLEL))),
  c("Scallop", Icon::Isocurve, D(Kind::Form(&forms::CAM_SCALLOP))),
  c("Adaptive Clearing", Icon::Shell, D(Kind::Form(&forms::CAM_ADAPTIVE3D))),
  c("Steep and Shallow", Icon::Zebra, D(Kind::Form(&forms::CAM_STEEP_SHALLOW))),
  c("Contour", Icon::SkOffset, D(Kind::Form(&forms::CAM_CONTOUR3D))),
  c("Pencil", Icon::Line, D(Kind::Form(&forms::CAM_PENCIL))),
];

const CAM_DRILLING: &[E] = &[c("Drill", Icon::Hole, D(Kind::Form(&forms::CAM_DRILL)))];

const CAM_MULTI_AXIS: &[E] = &[
  c("Rotary", Icon::Revolve, D(Kind::Form(&forms::CAM_ROTARY))),
  c("Swarf", Icon::Draft, D(Kind::Form(&forms::CAM_SWARF))),
];

const CAM_TURN_SETUP: &[E] = &[
  c("New Turning Setup", Icon::Revolve, D(Kind::Form(&forms::CAM_TURN_SETUP))),
  E::Sep,
  c("Tool Library", Icon::Configure, Cmd::CamTools),
];

const CAM_TURNING: &[E] = &[
  c("Turning Face", Icon::Extrude, D(Kind::Form(&forms::CAM_TURN_FACE))),
  c("Profile Roughing", Icon::Shell, D(Kind::Form(&forms::CAM_TURN_ROUGH))),
  c("Profile Finishing", Icon::SkOffset, D(Kind::Form(&forms::CAM_TURN_FINISH))),
  c("Groove", Icon::Slot, D(Kind::Form(&forms::CAM_GROOVE))),
  c("Turning Thread", Icon::Thread, D(Kind::Form(&forms::CAM_THREAD))),
  c("Part Off", Icon::SplitBody, D(Kind::Form(&forms::CAM_PART_OFF))),
];

const CAM_ACTIONS: &[E] = &[
  c("Generate", Icon::Play, Cmd::CamGenerate).k("Ctrl+G"),
  c("Simulate", Icon::MotionStudy, Cmd::CamSimulate),
  c("Post Process", Icon::ExportDxf, Cmd::CamPost),
  c("Delete Setup", Icon::Remove, Cmd::CamDelete),
  c("2D Cut (Laser, Plasma, Waterjet)", Icon::ExportDxf, Cmd::Cut2d),
  c("Cut Service Check", Icon::Interference, Cmd::CutCheck),
  c("All Flat Patterns on One Sheet", Icon::FlatPattern, D(Kind::Form(&forms::COLLECT_FLAT))),
];

const GEN_DEFINE: &[E] = &[
  c("New Generative Study", Icon::Plus, Cmd::Gen("study")),
  E::Sep,
  c("Preserve Geometry", Icon::Fix, Cmd::Gen("preserve")),
  c("Obstacle Geometry", Icon::Remove, Cmd::Gen("obstacle")),
  c("Starting Shape", Icon::Body, Cmd::Gen("start")),
  E::Sep,
  c("Structural Loads", Icon::Move, Cmd::Gen("load")),
  c("Structural Constraints", Icon::Fix, Cmd::Gen("constraint")),
];

const GEN_CRITERIA: &[E] = &[c("Study Settings", Icon::Gauge, Cmd::Gen("criteria"))];

const GEN_GENERATE: &[E] =
  &[c("Pre-check", Icon::Interference, Cmd::Gen("precheck")), c("Generate", Icon::Play, Cmd::Gen("generate"))];

const GEN_EXPLORE: &[E] = &[c("Explore", Icon::Views, Cmd::Gen("explore"))];

/// The Generative Design workspace's tab.
const GENERATIVE: [Group; 4] = [
  Group {
    label: "DEFINE",
    quick: &["Preserve Geometry", "Obstacle Geometry", "Structural Loads", "Structural Constraints"],
    menu: GEN_DEFINE,
  },
  Group { label: "DESIGN CRITERIA", quick: &["Study Settings"], menu: GEN_CRITERIA },
  Group { label: "GENERATE", quick: &["Pre-check", "Generate"], menu: GEN_GENERATE },
  Group { label: "EXPLORE", quick: &["Explore"], menu: GEN_EXPLORE },
];

const RENDER_SETUP: &[E] = &[
  c("Appearance", Icon::Appearance, Cmd::Render(RenderCmd::Appearance)),
  c("Scene Settings", Icon::EnvMap, Cmd::Render(RenderCmd::SceneSettings)),
];
const RENDER_IN_CANVAS: &[E] = &[c("In-Canvas Render", Icon::Display, Cmd::Render(RenderCmd::InCanvas))];
const RENDER_RENDER: &[E] = &[
  c("Render", Icon::Play, Cmd::Render(RenderCmd::Render)),
  c("Render Gallery", Icon::Folder, Cmd::Render(RenderCmd::Gallery)),
];
const RENDER_EXIT: &[E] = &[c("Finish Render", Icon::Finish, Cmd::Render(RenderCmd::Finish))];

/// The Render workspace's RENDER tab.
const RENDER: [Group; 4] = [
  Group { label: "SETUP", quick: &["Appearance", "Scene Settings"], menu: RENDER_SETUP },
  Group { label: "IN-CANVAS RENDER", quick: &["In-Canvas Render"], menu: RENDER_IN_CANVAS },
  Group { label: "RENDER", quick: &["Render", "Render Gallery"], menu: RENDER_RENDER },
  Group { label: "EXIT", quick: &["Finish Render"], menu: RENDER_EXIT },
];

/// The Manufacture workspace's MILLING tab.
const MILLING: [Group; 6] = [
  Group { label: "SETUP", quick: &["New Setup", "Tool Library"], menu: CAM_SETUP },
  Group { label: "2D", quick: &["2D Adaptive Clearing", "2D Pocket", "Face", "2D Contour"], menu: CAM_2D },
  Group { label: "3D", quick: &["Parallel", "Scallop"], menu: CAM_3D },
  Group { label: "DRILLING", quick: &["Drill"], menu: CAM_DRILLING },
  Group { label: "MULTI-AXIS", quick: &["Rotary", "Swarf"], menu: CAM_MULTI_AXIS },
  Group { label: "ACTIONS", quick: &["Generate", "Simulate", "Post Process"], menu: CAM_ACTIONS },
];

/// The Manufacture workspace's TURNING tab.
const TURNING: [Group; 3] = [
  Group { label: "SETUP", quick: &["New Turning Setup", "Tool Library"], menu: CAM_TURN_SETUP },
  Group {
    label: "TURNING",
    quick: &["Turning Face", "Profile Roughing", "Profile Finishing", "Groove", "Turning Thread", "Part Off"],
    menu: CAM_TURNING,
  },
  Group { label: "ACTIONS", quick: &["Generate", "Simulate", "Post Process"], menu: CAM_ACTIONS },
];

const MANAGE: [Group; 1] =
  [Group { label: "BOM", quick: &["Bill of Materials"], menu: &[c("Bill of Materials", Icon::Bom, D(Kind::Bom))] }];

const ADD_INS: &[E] =
  &[c("Scripts and Add-Ins...", Icon::Scripts, Cmd::Plugins), x("Design and Make Marketplace", Icon::Marketplace)];

const UTILITY: &[E] = &[
  c("Manage Materials", Icon::ManageMaterials, Cmd::ManageMaterials),
  x("Manage Threads Library", Icon::Configure),
  c("Compute All", Icon::ComputeAll, Cmd::ComputeAll).k("Ctrl+B"),
];

// UTILITIES' INSPECT has no Environment Map Analysis
const UTIL_INSPECT: &[E] = &[
  c("Measure", Icon::Measure, D(Kind::Inspect)).k("I"),
  c("Interference", Icon::Interference, D(Kind::Interference)),
  c("Section Analysis", Icon::Section, D(Kind::Section)),
  E::Sep,
  x("Curvature Comb Analysis", Icon::CurvComb),
  c("Curvature Map Analysis", Icon::CurvMap, D(Kind::Analysis("curvature"))),
  c("Draft Analysis", Icon::DraftAnalysis, D(Kind::Analysis("draft"))),
  x("Isocurve Analysis", Icon::Isocurve),
  c("Zebra Analysis", Icon::Zebra, D(Kind::Analysis("zebra"))),
  E::Sep,
  c("Accessibility Analysis", Icon::Accessibility, D(Kind::Analysis("accessibility"))),
  c("Center of Mass", Icon::CenterOfMass, D(Kind::CenterOfMass)),
  x("Minimum Radius Analysis", Icon::MinRadius),
  E::Sep,
  x("Navigator", Icon::Navigator),
  E::Sep,
  x("Display Component Colors", Icon::ComponentColors).k("Shift+N"),
  x("Display Mesh Face Groups", Icon::FaceGroups).k("Shift+F"),
  E::Sep,
  x("History", Icon::History),
  x("Sustainability Insights", Icon::Sustainability),
  E::Sep,
  c("Properties", Icon::Properties, Cmd::Properties),
];

const UTILITIES: [Group; 5] = [
  Group { label: "MAKE", quick: &["3D Print"], menu: EXPORT },
  Group { label: "ADD-INS", quick: &["Scripts and Add-Ins...", "Design and Make Marketplace"], menu: ADD_INS },
  Group { label: "UTILITY", quick: &["Compute All", "Manage Materials"], menu: UTILITY },
  Group {
    label: "INSPECT",
    quick: &[
      "Measure",
      "Interference",
      "Draft Analysis",
      "Accessibility Analysis",
      "Section Analysis",
      "Display Component Colors",
      "Sustainability Insights",
    ],
    menu: UTIL_INSPECT,
  },
  Group { label: "SELECT", quick: &["Select"], menu: SELECT },
];

const FORM_CREATE: &[E] = &[
  c("Box", Icon::Box, Cmd::Form(FormAct::Primitive(&forms::FORM_BOX))),
  c("Plane", Icon::Plane, Cmd::Form(FormAct::Primitive(&form_ui::FORM_PLANE))),
  c("Cylinder", Icon::Cylinder, Cmd::Form(FormAct::Primitive(&form_ui::FORM_CYLINDER))),
  c("Sphere", Icon::Sphere, Cmd::Form(FormAct::Primitive(&form_ui::FORM_SPHERE))),
  c("Torus", Icon::Torus, Cmd::Form(FormAct::Primitive(&form_ui::FORM_TORUS))),
  c("Quadball", Icon::Sphere, Cmd::Form(FormAct::Primitive(&form_ui::FORM_QUADBALL))),
  c("Face", Icon::Patch, Cmd::Form(FormAct::Primitive(&form_ui::FORM_FACE))),
];

const FORM_MODIFY: &[E] = &[
  c("Edit Form", Icon::Move, Cmd::Form(FormAct::EditForm)),
  c("Insert Edge", Icon::SplitFace, Cmd::Form(FormAct::InsertEdge)),
  c("Insert Point", Icon::CPoint, Cmd::Form(FormAct::InsertPoint)),
  c("Subdivide", Icon::Grid, Cmd::Form(FormAct::Subdivide)),
  c("Delete", Icon::Remove, Cmd::Form(FormAct::Delete)),
  E::Sep,
  c("Extrude", Icon::Extrude, Cmd::Form(FormAct::Extrude)),
  c("Bridge", Icon::Loft, Cmd::Form(FormAct::Bridge)),
  c("Fill Hole", Icon::Patch, Cmd::Form(FormAct::FillHole)),
  c("Weld Vertices", Icon::Stitch, Cmd::Form(FormAct::Weld)),
  E::Sep,
  c("Crease", Icon::Chamfer, Cmd::Form(FormAct::Crease)),
  c("UnCrease", Icon::Fillet, Cmd::Form(FormAct::Uncrease)),
  c("Pull", Icon::ProjectSurface, Cmd::Form(FormAct::Pull)),
  c("Make Uniform", Icon::Smooth, Cmd::Form(FormAct::MakeUniform)),
  c("Reverse Normal", Icon::ReverseNormal, Cmd::Form(FormAct::Flip)),
];

const FORM_SYMMETRY: &[E] = &[
  c("Mirror - Internal", Icon::Mirror, Cmd::Form(FormAct::MirrorInternal)),
  c("Circular - Internal", Icon::CircPattern, Cmd::Form(FormAct::CircularInternal)),
  c("Mirror - Duplicate", Icon::Mirror, Cmd::Form(FormAct::MirrorDuplicate)),
  c("Circular - Duplicate", Icon::CircPattern, Cmd::Form(FormAct::CircularDuplicate)),
  c("Clear Symmetry", Icon::Remove, Cmd::Form(FormAct::ClearSymmetry)),
];

const FORM_UTILITIES: &[E] = &[
  c("Box Display", Icon::Box, Cmd::Form(FormAct::Display(fox_engine::model::form::Display::Box))).k("Alt+1"),
  c(
    "Control Frame Display",
    Icon::Display,
    Cmd::Form(FormAct::Display(fox_engine::model::form::Display::ControlFrame)),
  )
  .k("Alt+2"),
  c("Smooth Display", Icon::Sphere, Cmd::Form(FormAct::Display(fox_engine::model::form::Display::Smooth))).k("Alt+3"),
];

const FORM_FINISH: &[E] = &[c("Finish Form", Icon::Finish, Cmd::Form(FormAct::Finish))];

const BASE_FINISH: &[E] = &[c("Finish Base Feature", Icon::Finish, Cmd::BaseFeature(false))];

/// The green group closing the BASE FEATURE SOLID / SURFACE tabs while a base feature is open.
const BASE_FEATURE_FINISH: Group =
  Group { label: "FINISH BASE FEATURE", quick: &["Finish Base Feature"], menu: BASE_FINISH };

/// The Form workspace's contextual tab.
const FORM: [Group; 5] = [
  Group {
    label: "CREATE",
    quick: &["Box", "Plane", "Cylinder", "Sphere", "Torus", "Quadball", "Face"],
    menu: FORM_CREATE,
  },
  Group {
    label: "MODIFY",
    quick: &["Edit Form", "Insert Edge", "Subdivide", "Extrude", "Bridge", "Fill Hole", "Crease", "Pull"],
    menu: FORM_MODIFY,
  },
  Group { label: "SYMMETRY", quick: &["Mirror - Internal", "Circular - Internal"], menu: FORM_SYMMETRY },
  Group { label: "UTILITIES", quick: &["Box Display", "Smooth Display", "Make Uniform"], menu: FORM_UTILITIES },
  Group { label: "FINISH FORM", quick: &["Finish Form"], menu: FORM_FINISH },
];

const SHEET_METAL: [Group; 2] = [
  Group { label: "CREATE", quick: &["Flange (Base/Edge/Contour)", "Create Flat Pattern"], menu: SHEET_CREATE },
  Group {
    label: "MODIFY",
    quick: &[
      "Sheet Metal Rules",
      "Sheet Metal Fillet",
      "Sheet Metal Chamfer",
      "Fillet",
      "Chamfer",
      "Bill of Materials",
    ],
    menu: SHEET_MODIFY,
  },
];

// PLASTIC shows the SOLID menus with its own quick icons (Web and Rib up front)
const PLASTIC: [Group; 2] = [
  Group {
    label: "CREATE",
    quick: &["Create Sketch", "Extrude", "Revolve", "Hole", "Web", "Rib", "Rectangular Pattern", "Create Form"],
    menu: SOLID_CREATE,
  },
  Group {
    label: "MODIFY",
    quick: &["Press Pull", "Fillet", "Shell", "Combine", "Split Body", "Move/Copy", "Bill of Materials"],
    menu: SOLID_MODIFY,
  },
];

const MESH: [Group; 3] = [
  Group { label: "CREATE", quick: &["Insert Mesh", "Tessellate", "Create Mesh Section Sketch"], menu: MESH_CREATE },
  Group { label: "PREPARE", quick: &["Repair", "Generate Face Groups", "Combine Face Groups"], menu: MESH_PREPARE },
  Group {
    label: "MODIFY",
    quick: &[
      "Remesh",
      "Reduce",
      "Plane Cut",
      "Shell",
      "Combine",
      "Move/Copy",
      "Scale Mesh",
      "Direct Edit",
      "Bill of Materials",
    ],
    menu: MESH_MODIFY,
  },
];

fn find(menu: &[E], label: &str) -> Option<Item> {
  menu.iter().find_map(|e| match e {
    E::I(i) if i.label == label => Some(*i),
    E::Sub(_, sub) => find(sub, label),
    _ => None,
  })
}

/// The tool groups of `tab`: its own, then the ones every modelling tab ends with.
fn groups(tab: Tab) -> (&'static [Group], &'static [Group]) {
  // MANAGE and UTILITIES stand alone; the modelling tabs end with the shared groups
  match tab {
    Tab::Solid => (&SOLID, &SHARED),
    Tab::Surface => (&SURFACE, &SHARED),
    Tab::Mesh => (&MESH, &MESH_SHARED),
    Tab::SheetMetal => (&SHEET_METAL, &SHARED),
    Tab::Plastic => (&PLASTIC, &SHARED),
    Tab::Manage => (&MANAGE, &[]),
    Tab::Utilities => (&UTILITIES, &[]),
    Tab::Sketch => (&SKETCH, &[]),
    Tab::Drawing => (&DRAWING, &[]),
    Tab::Milling => (&MILLING, &[]),
    Tab::Plugin(_) => (&[], &[]),
    Tab::Form => (&FORM, &[]),
    Tab::Generative => (&GENERATIVE, &[]),
    Tab::Render => (&RENDER, &[]),
    Tab::Turning => (&TURNING, &[]),
    Tab::Assembly => (&ASSEMBLY, &ASSEMBLY_SHARED),
  }
}

/// A command FissionOxide runs, as the toolbar lists it (for the S toolbox's search).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Entry {
  pub label: &'static str,
  pub icon: Icon,
  pub tint: Tint,
  pub key: &'static str,
  pub cmd: Cmd,
  /// The tab it was found on.
  pub tab: Tab,
}

impl Entry {
  /// Its id in the key map and the S toolbox's shortcuts.
  pub(crate) fn id(&self) -> String {
    customize::cmd_id(Scope::of(self.tab), self.label)
  }
}

/// Every tab with a toolbar.
const ALL_TABS: [Tab; 15] = [
  Tab::Solid,
  Tab::Surface,
  Tab::Mesh,
  Tab::SheetMetal,
  Tab::Plastic,
  Tab::Manage,
  Tab::Utilities,
  Tab::Sketch,
  Tab::Drawing,
  Tab::Milling,
  Tab::Form,
  Tab::Generative,
  Tab::Render,
  Tab::Turning,
  Tab::Assembly,
];

/// Every command that can have a keyboard shortcut, with the key its menus show (a label that
/// shows a key on one tab has it on all of its toolbar's tabs).
pub(crate) fn bindings() -> &'static [Binding] {
  static ALL: std::sync::LazyLock<Vec<Binding>> = std::sync::LazyLock::new(|| {
    let mut out: Vec<Binding> = Vec::new();
    // tab by tab, so a command on two toolbars (Move/Copy on SOLID and SKETCH) binds on each
    for tab in ALL_TABS {
      for e in commands(&[tab]) {
        let id = e.id();
        match out.iter_mut().find(|b| b.id == id) {
          Some(b) if b.default.is_empty() => b.default = e.key,
          Some(_) => {}
          None => out.push(Binding { id, scope: Scope::of(e.tab), label: e.label, default: e.key }),
        }
      }
    }
    out
  });
  &ALL
}

/// The keyboard shortcut of `label` on toolbar `scope`: the user's, or the menus' default.
pub(crate) fn key_of<'a>(prefs: &'a Prefs, scope: Scope, label: &str) -> &'a str {
  let id = customize::cmd_id(scope, label);
  let default = bindings().iter().find(|b| b.id == id).map_or("", |b| b.default);
  prefs.keys.key(&id, default)
}

/// A command's tooltip, as in Fusion: see [`crate::ribbon_tip`].
fn rich_tip(resp: egui::Response, scope: Scope, it: &Item, key: &str) -> egui::Response {
  crate::ribbon_tip::show(resp, scope, it.label, it.tip_label(), it.icon, it.tint, it.cmd.is_none(), key)
}

/// The ribbon's tooltip on a sketch tool's toggle elsewhere (the Sketch Palette's Feature
/// Options): its name, description line and shortcut, expanding like a ribbon button's.
pub(crate) fn sketch_tool_tip(resp: egui::Response, prefs: &Prefs, tool: Tool) -> egui::Response {
  let (icon, label, _) = Cmd::SketchTool(tool).info();
  let it = Item {
    label,
    icon,
    tint: Tint::Blue,
    key: "",
    cmd: Some(Cmd::SketchTool(tool)),
    disabled: false,
    needs_saved: false,
    surface: false,
    paid: false,
  };
  let key = key_of(prefs, Scope::Sketch, label);
  rich_tip(resp, Scope::Sketch, &it, key)
}

/// Every command the toolbar offers on `tabs`, in toolbar order, each label/command pair once.
pub(crate) fn commands(tabs: &[Tab]) -> Vec<Entry> {
  fn walk(menu: &[E], tab: Tab, out: &mut Vec<Entry>) {
    for e in menu {
      match e {
        E::I(Item { label, icon, tint, key, cmd: Some(cmd), disabled: false, .. }) => {
          if !out.iter().any(|o| o.label == *label && o.cmd == *cmd) {
            out.push(Entry { label, icon: *icon, tint: *tint, key, cmd: *cmd, tab });
          }
        }
        E::Sub(_, sub) => walk(sub, tab, out),
        _ => {}
      }
    }
  }
  let mut out = Vec::new();
  for &tab in tabs {
    let (own, shared) = groups(tab);
    for g in own.iter().chain(shared.iter()) {
      walk(g.menu, tab, &mut out);
    }
    for more in extra_commands(tab) {
      walk(more, tab, &mut out);
    }
  }
  out
}

/// Whether the toolbar offers `cmd` on `tab`.
pub(crate) fn offers(tab: Tab, cmd: Cmd) -> bool {
  commands(&[tab]).iter().any(|e| e.cmd == cmd)
}

/// Draws the tool groups of `tab` in `width` pixels. Returns false for tabs without a layout yet.
///
/// Like Fusion's ribbon, a row that doesn't fit drops icons from the right of its panels
/// (widest panel first, down to one icon each) until every panel is on screen; the panel menus
/// still list everything.
pub fn tab_toolbar(app: &mut App, ui: &mut egui::Ui, tab: Tab, width: f32) -> bool {
  let (own, shared) = groups(tab);
  let mut pick: Option<Item> = None;
  let mut act: Option<Action> = None;
  let mut all: Vec<&Group> = own.iter().chain(shared.iter()).collect();
  if app.base_mode && matches!(tab, Tab::Solid | Tab::Surface) {
    all.push(&BASE_FEATURE_FINISH);
  }
  let items: Vec<Vec<Item>> = all.iter().map(|g| quick_items(&app.prefs, tab, g)).collect();
  let font = egui::FontId::proportional(12.0);
  let panels: Vec<PanelSize> = all
    .iter()
    .zip(&items)
    .map(|(g, its)| PanelSize {
      label: ui.painter().layout_no_wrap(g.label.to_string(), font.clone(), Color32::BLACK).size().x + 22.0,
      icons: its.len(),
    })
    .collect();
  let counts = fit_counts(&panels, width);
  let shown: Vec<(&'static str, usize)> = all.iter().zip(&counts).map(|(g, n)| (g.label, *n)).collect();
  ui.ctx().data_mut(|d| d.insert_temp(fit_id(tab), shown));
  for ((g, its), n) in all.iter().zip(items).zip(counts) {
    show_group(app, ui, tab, g, &its[..n], &mut pick, &mut act);
  }
  if let Some(it) = pick {
    match it.cmd {
      Some(c) => c.run(app),
      None => app.status = Some((format!("{} isn't in FissionOxide yet", it.label), false)),
    }
  }
  if let Some(a) = act {
    customize::apply(app, a);
  }
  true
}

/// Where [`tab_toolbar`] keeps how many icons each panel of `tab` showed in the last frame.
pub(crate) fn fit_id(tab: Tab) -> egui::Id {
  egui::Id::new(("toolbar-fit", tab))
}

/// Where a menu is: what its rows' options and tooltips need.
struct At<'a> {
  prefs: &'a Prefs,
  tab: Tab,
  group: &'static str,
  /// The group's default toolbar icons.
  quick: &'static [&'static str],
  /// The design has never been saved (commands that need a saved design grey out).
  unsaved: bool,
}

impl At<'_> {
  fn scope(&self) -> Scope {
    Scope::of(self.tab)
  }

  /// `it` as the menu shows it here: greyed if it needs a saved design and this one isn't.
  fn shown(&self, it: &Item) -> Item {
    Item { disabled: it.disabled || it.needs_saved && self.unsaved, ..*it }
  }

  fn key(&self, it: &Item) -> &str {
    if it.surface {
      ""
    } else if it.cmd.is_some() {
      key_of(self.prefs, self.scope(), it.label)
    } else {
      it.key
    }
  }

  /// A command's ⋮ options.
  fn options(&self, ui: &mut egui::Ui, it: &Item, act: &mut Option<Action>) {
    let id = customize::cmd_id(self.scope(), it.label);
    let binding = bindings().iter().find(|b| b.id == id).cloned().unwrap_or(Binding {
      id,
      scope: self.scope(),
      label: it.label,
      default: "",
    });
    let default = self.quick.iter().take(MAX_QUICK).any(|l| *l == it.label);
    customize::options_menu(ui, self.prefs, (self.tab, self.group, it.label, default), &binding, act);
  }
}

/// The icons a group shows on the toolbar: its first [`MAX_QUICK`] commands unless unpinned,
/// then the ones the user pinned.
fn quick_items(prefs: &Prefs, tab: Tab, g: &Group) -> Vec<Item> {
  let mut out: Vec<Item> = g
    .quick
    .iter()
    .take(MAX_QUICK)
    .filter(|l| prefs.shows(tab, g.label, l, true))
    .filter_map(|l| find(g.menu, l))
    .collect();
  let prefix = customize::pin_id(tab, g.label, "");
  for id in &prefs.pinned {
    if let Some(it) = id.strip_prefix(&prefix).and_then(|l| find(g.menu, l))
      && !out.iter().any(|o| o.label == it.label)
    {
      out.push(it);
    }
  }
  out
}

/// What decides a panel's width on the toolbar: its name (with the chevron) and its icons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PanelSize {
  /// The minimum width of its name row.
  pub label: f32,
  /// How many icons it would show.
  pub icons: usize,
}

/// A toolbar icon's cell and the gap after it ([`icons::tool_button_bare`]).
const ICON_W: f32 = icons::TOOL_BUTTON_W + 1.0;
/// The separator after each panel ([`panels::separator`]).
const SEPARATOR_W: f32 = 7.0;

fn panel_width(p: &PanelSize, icons: usize) -> f32 {
  (icons as f32 * ICON_W - 1.0).max(p.label) + SEPARATOR_W
}

/// How many icons each panel shows so the row fits `width`: all of them when there is room,
/// else icons come off the right of the widest panel first, never below one per panel.
pub(crate) fn fit_counts(panels: &[PanelSize], width: f32) -> Vec<usize> {
  let mut counts: Vec<usize> = panels.iter().map(|p| p.icons).collect();
  loop {
    let total: f32 = panels.iter().zip(&counts).map(|(p, n)| panel_width(p, *n)).sum();
    if total <= width {
      return counts;
    }
    // only a panel whose icons (not its name) set its width can get narrower
    let widest = panels
      .iter()
      .zip(&counts)
      .enumerate()
      .filter(|(_, (p, n))| **n > 1 && panel_width(p, **n - 1) < panel_width(p, **n))
      .map(|(i, (p, n))| (i, panel_width(p, *n)))
      .fold(None, |best: Option<(usize, f32)>, (i, w)| match best {
        Some((_, bw)) if bw >= w => best,
        _ => Some((i, w)),
      });
    match widest {
      Some((i, _)) => counts[i] -= 1,
      None => return counts,
    }
  }
}

fn show_group(
  app: &App,
  ui: &mut egui::Ui,
  tab: Tab,
  g: &Group,
  quick: &[Item],
  pick: &mut Option<Item>,
  act: &mut Option<Action>,
) {
  let at = At { prefs: &app.prefs, tab, group: g.label, quick: g.quick, unsaved: panels::is_unsaved(app) };
  let popup_id = egui::Id::new(("toolbar-menu", tab, g.label));
  let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
  // FINISH SKETCH sits on light blue, like the contextual SKETCH tab
  let bg = ui.painter().add(egui::Shape::Noop);
  let finish = matches!(g.label, "FINISH SKETCH" | "FINISH DRAWING" | "FINISH BASE FEATURE");
  let inner = ui.vertical(|ui| {
    ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
    ui.add_space(2.0);
    ui.horizontal(|ui| {
      // like Fusion, a group shows a handful of its commands (and the ones pinned to it); the
      // rest live in its menu
      for it in quick {
        let active = it.cmd.is_some_and(|c| c.active(app));
        // the big FINISH button is clickable across the whole panel, not just its icon
        let resp = if finish {
          let w = ui.painter().layout_no_wrap(g.label.to_string(), egui::FontId::proportional(12.0), Color32::BLACK);
          icons::tool_button_wide(ui, it.icon, it.tint, active, w.size().x + 22.0)
        } else {
          icons::tool_button_bare(ui, it.icon, it.tint, active)
        };
        if it.paid {
          paid_badge(ui.painter(), resp.rect.shrink(4.0));
        }
        let resp = rich_tip(resp, at.scope(), it, at.key(it));
        if it.cmd.is_some() {
          resp.context_menu(|ui| at.options(ui, it, act));
        }
        if resp.clicked() && !at.shown(it).disabled {
          *pick = Some(*it);
        }
      }
    });
    ui.add_space(3.0);
    let w = ui.min_rect().width();
    panels::group_label(ui, g.label, w, open)
  });
  // toggles (selection priority and filters) show a check mark while on
  let menu = menu_of(app, tab, g);
  let on: Vec<&'static str> =
    toggles(menu).into_iter().filter(|i| i.cmd.is_some_and(|c| c.active(app))).map(|i| i.label).collect();
  egui::Popup::menu(&inner.inner).id(popup_id).show(|ui| entries(ui, &at, menu, pick, act, &on));
  if finish {
    // FINISH BASE FEATURE is Fusion's green
    let fill = if g.label == "FINISH BASE FEATURE" { theme::BASE_FEATURE } else { theme::CONTEXT_TAB };
    ui.painter().set(bg, egui::Shape::rect_filled(inner.response.rect.expand2(vec2(4.0, 2.0)), 2.0, fill));
  }
  panels::separator(ui);
}

/// The menu a group's name opens: SOLID's MODIFY (on SOLID and PLASTIC) gains the direct edits
/// while design history is off.
fn menu_of(app: &App, tab: Tab, g: &Group) -> &'static [E] {
  if app.s.design.history_off && g.label == "MODIFY" && matches!(tab, Tab::Solid | Tab::Plastic) {
    SOLID_MODIFY_DIRECT
  } else {
    g.menu
  }
}

/// The toggle commands in a menu (and its submenus): the selection filters get a check mark
/// when on; the selection priorities show none, as Fusion's.
fn toggles(menu: &'static [E]) -> Vec<Item> {
  let mut out = Vec::new();
  for e in menu {
    match e {
      E::I(i) if matches!(i.cmd, Some(Cmd::Filter(_))) => out.push(*i),
      E::Sub(_, sub) => out.extend(toggles(sub)),
      _ => {}
    }
  }
  out
}

fn entries(
  ui: &mut egui::Ui,
  at: &At,
  menu: &'static [E],
  pick: &mut Option<Item>,
  act: &mut Option<Action>,
  on: &[&'static str],
) {
  ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
  let font = egui::FontId::proportional(12.5);
  let widest = menu
    .iter()
    .filter_map(|e| match e {
      E::I(i) => Some(i.label),
      E::Sub(l, _) => Some(*l),
      E::Sep => None,
    })
    .map(|l| ui.painter().layout_no_wrap(l.to_string(), font.clone(), Color32::BLACK).size().x)
    .fold(0.0f32, f32::max);
  let keys = menu.iter().any(|e| matches!(e, E::I(i) if !at.key(i).is_empty()));
  // the last column holds a command's ⋮ (or a submenu's arrow)
  let width = widest + 44.0 + if keys { 72.0 } else { 0.0 } + 24.0;
  ui.set_min_width(width);
  ui.set_max_width(width);
  for e in menu {
    match e {
      E::I(it) => {
        let it = &at.shown(it);
        let r = row(ui, width, Some(it), at.key(it), it.label, false);
        if it.cmd.is_some() {
          // Fusion's ⋮ on the hovered row: its options open beside it
          let dots_rect = Rect::from_min_max(pos2(r.rect.right() - 22.0, r.rect.top()), r.rect.max);
          let dots = ui.interact(dots_rect, r.id.with("options"), egui::Sense::click());
          let open = egui::Popup::is_id_open(ui.ctx(), egui::containers::menu::SubMenu::id_from_widget_id(dots.id));
          if r.hovered() || dots.hovered() || open {
            if dots.hovered() || open {
              ui.painter().rect_filled(dots_rect.shrink(2.0), 2.0, Color32::from_gray(205));
            }
            options_dots(ui.painter(), dots_rect.center(), theme::TEXT);
          }
          egui::containers::menu::SubMenu::new().show(ui, &dots, |ui| at.options(ui, it, act));
        }
        let r = menu_tip(r, at.scope(), it);
        if on.contains(&it.label) {
          ui.painter().text(
            r.rect.right_center() - vec2(30.0, 0.0),
            egui::Align2::CENTER_CENTER,
            "✔",
            egui::FontId::proportional(12.0),
            theme::TEXT,
          );
        }
        if r.clicked() && !it.disabled {
          *pick = Some(*it);
          ui.close();
        }
      }
      E::Sep => {
        let (r, _) = ui.allocate_exact_size(vec2(width, 5.0), egui::Sense::hover());
        ui.painter().line_segment(
          [pos2(r.left() + 4.0, r.center().y), pos2(r.right() - 4.0, r.center().y)],
          Stroke::new(1.0, Color32::from_gray(110)),
        );
      }
      E::Sub(label, sub) => {
        let resp = row(ui, width, None, "", label, true);
        let resp = if *label == "Selection Filters" { resp.on_hover_text(SELECTION_FILTERS_TIP) } else { resp };
        egui::containers::menu::SubMenu::new().show(ui, &resp, |ui| entries(ui, at, sub, pick, act, on));
      }
    }
  }
}

/// One menu row: icon, name, shortcut column, or a submenu arrow.
fn row(ui: &mut egui::Ui, width: f32, it: Option<&Item>, key: &str, label: &str, sub: bool) -> egui::Response {
  let disabled = it.is_some_and(|i| i.disabled);
  let (rect, resp) = ui.allocate_exact_size(vec2(width, 22.0), egui::Sense::click());
  let p = ui.painter();
  if resp.hovered() && !disabled {
    p.rect_filled(rect, 0.0, theme::HOVER);
  }
  if let Some(i) = it {
    let ir = Rect::from_center_size(rect.left_center() + vec2(14.0, 0.0), Vec2::splat(17.0));
    if disabled {
      icons::draw(p, ir, i.icon, Color32::from_gray(185));
    } else {
      icons::draw_tinted(p, ir, i.icon, i.tint);
    }
    if i.paid {
      paid_badge(p, ir);
    }
    if !key.is_empty() {
      p.text(
        pos2(rect.right() - 90.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        key,
        egui::FontId::proportional(12.0),
        theme::TEXT,
      );
    }
  }
  let color = if disabled { Color32::from_gray(170) } else { theme::TEXT };
  p.text(
    rect.left_center() + vec2(28.0, 0.0),
    egui::Align2::LEFT_CENTER,
    label,
    egui::FontId::proportional(12.5),
    color,
  );
  if sub {
    let c = rect.right_center() - vec2(12.0, 0.0);
    p.add(egui::Shape::convex_polygon(
      vec![c + vec2(-3.0, -5.0), c + vec2(4.0, 0.0), c + vec2(-3.0, 5.0)],
      Color32::from_gray(80),
      Stroke::NONE,
    ));
  }
  resp
}

/// Fusion's subscription badge on a command's icon: a small blue disc with a white rim and a
/// white four-pointed sparkle, at the icon's top right.
fn paid_badge(p: &egui::Painter, icon: Rect) {
  let c = icon.right_top() + vec2(-1.0, 2.0);
  p.circle(c, 4.5, theme::ACCENT, Stroke::new(1.0, Color32::WHITE));
  // the sparkle: a tall and a wide diamond across each other
  for (w, h) in [(0.9, 3.0), (3.0, 0.9)] {
    let pts = vec![c + vec2(0.0, -h), c + vec2(w, 0.0), c + vec2(0.0, h), c + vec2(-w, 0.0)];
    p.add(egui::Shape::convex_polygon(pts, Color32::WHITE, Stroke::NONE));
  }
}

/// The icon the toolbar menus draw for the command labelled `label` on any tab (the Undo list
/// shows a step's command with it).
pub(crate) fn icon_of_label(label: &str) -> Option<Icon> {
  ALL_TABS.iter().find_map(|tab| {
    let (own, shared) = groups(*tab);
    let menus = own.iter().chain(shared).map(|g| g.menu).chain(extra_commands(*tab).iter().copied());
    menus.into_iter().find_map(|m| find(m, label)).map(|i| i.icon)
  })
}

/// Fusion's ⋮ (a command's options, a value box's parameters) painted as three stacked dots at
/// `c`: the UI font has no U+22EE, which would render as a missing-glyph box.
pub(crate) fn options_dots(p: &egui::Painter, c: egui::Pos2, color: Color32) {
  for dy in [-4.0, 0.0, 4.0] {
    p.circle_filled(c + vec2(0.0, dy), 1.4, color);
  }
}

/// A menu row's tooltip, as in Fusion: a dark panel with white text to the right of the open
/// menu, level with the row, holding the command's description and instructions as paragraphs
/// with a blank line between them. No title and no shortcut line: the row shows both.
fn menu_tip(resp: egui::Response, scope: Scope, it: &Item) -> egui::Response {
  let paragraphs = tips::menu_paragraphs(scope, it.tip_label());
  if paragraphs.is_empty() && it.cmd.is_some() {
    return resp;
  }
  let style = resp.ctx.global_style();
  // the row spans the menu's content; its frame (the menu margin and a 1 px stroke) lies beyond
  let edge = resp.rect.right() + f32::from(style.spacing.menu_margin.right) + 1.0;
  let anchor = Rect::from_min_max(pos2(edge, resp.rect.top()), pos2(edge, resp.rect.bottom()));
  let frame = egui::Frame::new()
    .fill(theme::TOOLTIP)
    .corner_radius(egui::CornerRadius::same(4))
    .inner_margin(egui::Margin::symmetric(10, 8))
    .shadow(style.visuals.popup_shadow);
  let mut tooltip = egui::Tooltip::for_enabled(&resp);
  tooltip.popup = tooltip.popup.anchor(anchor).align(egui::RectAlign::RIGHT_START).gap(6.0).width(300.0).frame(frame);
  tooltip.show(|ui| {
    ui.set_max_width(300.0);
    ui.spacing_mut().item_spacing.y = 12.0;
    ui.style_mut().visuals.override_text_color = Some(Color32::WHITE);
    for paragraph in paragraphs {
      ui.add(egui::Label::new(paragraph).wrap());
    }
    if it.cmd.is_none() {
      ui.add(egui::Label::new(RichText::new("Not in FissionOxide yet").color(theme::MUTED)).wrap());
    }
  });
  resp
}

#[cfg(test)]
mod tests {
  use super::*;

  fn labels(menu: &[E], out: &mut Vec<&'static str>) {
    for e in menu {
      match e {
        E::I(i) => out.push(i.label),
        E::Sub(_, sub) => labels(sub, out),
        E::Sep => {}
      }
    }
  }

  #[test]
  fn every_command_has_a_tooltip() {
    let mut missing = Vec::new();
    for tab in ALL_TABS {
      let (own, shared) = groups(tab);
      let mut all = Vec::new();
      for g in own.iter().chain(shared.iter()) {
        labels(g.menu, &mut all);
      }
      for l in all {
        // Fusion's Bill of Materials tooltip is its bare name
        if l == "Bill of Materials" {
          assert!(tips::tip(Scope::of(tab), l).is_none());
          continue;
        }
        match tips::tip(Scope::of(tab), l) {
          // Fusion's Press Pull and Display Configuration Table have a description only
          Some(t)
            if !t.line.is_empty()
              && t.more != t.line
              && (!t.more.is_empty() || matches!(l, "Press Pull" | "Display Configuration Table")) => {}
          _ => missing.push((tab, l)),
        }
      }
    }
    assert!(missing.is_empty(), "{missing:?}");
  }

  /// Fusion's Project / Include submenu, in its order, and the Arc submenu without a shortcut
  /// on 3-Point Arc (Fusion's sketch defaults are L, R, C, D, P, X, T, O).
  #[test]
  fn project_include_is_fusions_and_three_point_arc_has_no_key() {
    let mut project = Vec::new();
    labels(SK_PROJECT, &mut project);
    assert_eq!(
      project,
      [
        "Project",
        "Intersect",
        "Spun Profile",
        "Include 3D Geometry",
        "Project To Surface",
        "Intersection Curve",
        "Isoparametric Curve"
      ]
    );
    assert!(SK_PROJECT.iter().all(|e| matches!(e, E::I(i) if i.cmd.is_some())), "every entry opens a dialog");
    let keys: Vec<&str> = SK_ARC.iter().filter_map(|e| if let E::I(i) = e { Some(i.key) } else { None }).collect();
    assert_eq!(keys, ["", "", ""]);
  }

  #[test]
  fn the_sketch_create_list_is_fusions() {
    let mut create = Vec::new();
    labels(SKETCH_CREATE, &mut create);
    assert!(!create.contains(&"Trace Image"), "not a Fusion CREATE command: {create:?}");
    let mut insert = Vec::new();
    labels(SKETCH_INSERT, &mut insert);
    assert!(insert.contains(&"Trace Image"), "it lives in INSERT, next to Canvas");
    let top: Vec<&str> = SKETCH_CREATE
      .iter()
      .filter_map(|e| match e {
        E::I(i) => Some(i.label),
        E::Sub(l, _) => Some(l),
        E::Sep => None,
      })
      .collect();
    assert_eq!(
      top,
      [
        "Line",
        "Midpoint Line",
        "Rectangle",
        "Circle",
        "Arc",
        "Polygon",
        "Ellipse",
        "Slot",
        "Spline",
        "Conic Curve",
        "Point",
        "Text",
        "Mirror",
        "Circular Pattern",
        "Rectangular Pattern",
        "Project / Include",
        "Sketch Dimension"
      ]
    );
  }

  #[test]
  fn keys_come_from_the_menus_and_the_user() {
    let mut prefs = Prefs::default();
    assert_eq!(key_of(&prefs, Scope::Design, "Extrude"), "E");
    assert_eq!(key_of(&prefs, Scope::Sketch, "Line"), "L");
    assert_eq!(key_of(&prefs, Scope::Design, "Shell"), "");
    // no two commands on a toolbar share a default key
    let all = bindings();
    for b in all.iter().filter(|b| customize::parse_combo(b.default).is_some()) {
      assert_eq!(prefs.keys.conflict(&b.id, b.scope, b.default, all), None, "{}", b.id);
    }
    prefs.keys.set("design.Extrude", "Shift+E", "E");
    assert_eq!(key_of(&prefs, Scope::Design, "Extrude"), "Shift+E");
  }

  /// The command `label` runs in `group` of `tab`'s toolbar.
  fn cmd_of(tab: Tab, group: &str, label: &str) -> Option<Cmd> {
    let (own, shared) = groups(tab);
    let g = own.iter().chain(shared.iter()).find(|g| g.label == group)?;
    find(g.menu, label)?.cmd
  }

  #[test]
  fn engine_commands_are_wired_to_their_menu_items() {
    use crate::forms;
    let wired = [
      (Tab::Sketch, "MODIFY", "Move/Copy", D(Kind::Move)),
      (Tab::Sketch, "MODIFY", "Sketch Scale", D(Kind::SkScale)),
      (Tab::Sketch, "INSERT", "Sketch Library...", Cmd::SketchLibrary),
      (Tab::Sketch, "CREATE", "Project To Surface", D(Kind::Form(&forms::SK_PROJECT_SURFACE))),
      (Tab::Sketch, "CREATE", "Intersection Curve", D(Kind::Form(&forms::SK_INTERSECTION_CURVE))),
      (Tab::Sketch, "CREATE", "Intersect", D(Kind::Form(&forms::SK_INTERSECT))),
      (Tab::Sketch, "CREATE", "Spun Profile", D(Kind::Form(&forms::SK_SPUN_PROFILE))),
      (Tab::Sketch, "CREATE", "Isoparametric Curve", D(Kind::Form(&forms::SK_ISOPARAMETRIC))),
      (Tab::Sketch, "CONSTRAINTS", "Polygon", Cmd::Constraint("polygon")),
      (Tab::Solid, "MODIFY", "Align", D(Kind::Form(&forms::ALIGN))),
      (Tab::Surface, "MODIFY", "Align", D(Kind::Form(&forms::ALIGN))),
      (Tab::SheetMetal, "MODIFY", "Align", D(Kind::Form(&forms::ALIGN))),
      (Tab::Solid, "CREATE", "Create Base Feature", Cmd::BaseFeature(true)),
      (Tab::Solid, "SELECT", "Construction Geometry", Cmd::Filter("construction")),
    ];
    for (tab, group, label, cmd) in wired {
      assert_eq!(cmd_of(tab, group, label), Some(cmd), "{tab:?} {group} > {label}");
    }
    // M in a sketch moves sketch objects instead of finishing the sketch
    assert!(offers(Tab::Sketch, D(Kind::Move)));
  }

  #[test]
  fn only_mesh_has_an_export_panel() {
    for tab in [Tab::Solid, Tab::Surface, Tab::SheetMetal, Tab::Plastic] {
      let (own, shared) = groups(tab);
      assert!(!own.iter().chain(shared.iter()).any(|g| g.label == "EXPORT"), "{tab:?}");
      assert!(!offers(tab, Cmd::Print3d), "{tab:?}");
    }
    assert_eq!(cmd_of(Tab::Mesh, "EXPORT", "3D Print"), Some(Cmd::Print3d));
    assert_eq!(cmd_of(Tab::Utilities, "MAKE", "3D Print"), Some(Cmd::Print3d));
    // ASSEMBLE keeps its Part Design place, between INSERT and SELECT
    let (own, shared) = groups(Tab::Solid);
    let order: Vec<&str> = own.iter().chain(shared.iter()).map(|g| g.label).collect();
    assert_eq!(order, ["CREATE", "MODIFY", "CONFIGURE", "CONSTRUCT", "INSPECT", "INSERT", "ASSEMBLE", "SELECT"]);
  }

  #[test]
  fn construction_filter_toggles() {
    let mut app = App::with_session(fox_engine::Session::default());
    assert!(app.sel_filter.construction);
    Cmd::Filter("construction").run(&mut app);
    assert!(!app.sel_filter.construction && !Cmd::Filter("construction").active(&app));
    assert!(app.sel_filter.bodies && app.sel_filter.profiles, "only Construction changes");
  }

  /// The labels of a menu's rows (submenus flattened), with their keys.
  fn rows(menu: &[E]) -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for e in menu {
      match e {
        E::I(i) => out.push((i.label, i.key)),
        E::Sub(l, _) => out.push((*l, "")),
        E::Sep => {}
      }
    }
    out
  }

  /// SELECT's submenus list what Fusion's do: the full filter set, all on by default and with a
  /// check mark; three priorities with no mark; the five selection tools.
  #[test]
  fn select_submenus_match_fusion() {
    let names: Vec<&str> = rows(SELECTION_FILTERS).into_iter().map(|(l, _)| l).collect();
    let mut want = vec!["Select All", "Select Through"];
    want.extend(crate::select::FILTER_TYPES.iter().map(|(_, n)| *n));
    assert_eq!(names, want);
    let mut app = App::with_session(fox_engine::Session::default());
    for e in SELECTION_FILTERS {
      if let E::I(i) = e {
        assert!(i.cmd.unwrap().active(&app), "{} starts checked", i.label);
      }
    }
    // Select All unchecks every type, and comes back once all are on again
    Cmd::Filter("all").run(&mut app);
    assert!(!app.sel_filter.bodies && !app.sel_filter.vertices && !Cmd::Filter("all").active(&app));
    Cmd::Filter("all").run(&mut app);
    assert!(app.sel_filter.bodies && app.sel_filter.vertices && Cmd::Filter("all").active(&app));
    Cmd::Filter("mesh_faces").run(&mut app);
    assert!(!Cmd::Filter("mesh_faces").active(&app) && !Cmd::Filter("all").active(&app), "one off: Select All off");
    assert!(Cmd::Filter("through").active(&app), "Select Through is its own flag");
    assert_eq!(
      rows(SELECTION_PRIORITY).into_iter().map(|(l, _)| l).collect::<Vec<_>>(),
      ["Select Body Priority", "Select Face Priority", "Select Edge Priority"]
    );
    assert!(toggles(SELECTION_PRIORITY).is_empty(), "no check mark on the current priority");
    assert_eq!(toggles(SELECTION_FILTERS).len(), SELECTION_FILTERS.len());
    assert_eq!(
      rows(SELECTION_TOOLS).into_iter().map(|(l, _)| l).collect::<Vec<_>>(),
      ["Select By Name", "Select By Boundary", "Select By Size", "Invert Selection", "Seed And Boundary"]
    );
    assert_eq!(cmd_of(Tab::Solid, "SELECT", "Select By Size"), Some(Cmd::SelTool(SelTool::BySize)));
    assert!(!rows(SELECTION_TOOLS).iter().any(|(l, _)| *l == "Select All" || *l == "Select Other"));
  }

  /// The sketch commands dropped from the MODIFY dropdown are still commands of the SKETCH tab
  /// (the S toolbox finds them).
  #[test]
  fn dropped_sketch_commands_stay_reachable() {
    use crate::forms;
    assert!(cmd_of(Tab::Sketch, "MODIFY", "Scale to Size").is_none());
    for (label, kind) in [
      ("Scale to Size", Kind::Form(&forms::SK_SCALE_TO)),
      ("Remove Duplicates", Kind::Form(&forms::SK_REMOVE_DUPLICATES)),
      ("Clean Up", Kind::Form(&forms::SK_CLEANUP)),
    ] {
      assert!(offers(Tab::Sketch, D(kind)), "{label}");
      assert!(commands(&[Tab::Sketch]).iter().any(|e| e.label == label), "{label}");
    }
  }

  /// The sketch MODIFY dropdown lists Fusion's ten commands, Move/Copy with its M.
  #[test]
  fn sketch_modify_lists_fusions_commands() {
    assert_eq!(
      rows(SKETCH_MODIFY),
      [
        ("Fillet", ""),
        ("Chamfer", ""),
        ("Blend Curve", ""),
        ("Offset", "O"),
        ("Trim", "T"),
        ("Extend", ""),
        ("Break", ""),
        ("Sketch Scale", ""),
        ("Move/Copy", "M"),
        ("Change Parameters", ""),
      ]
    );
    // the key column shows it on the SKETCH toolbar too, not only on SOLID's
    assert_eq!(key_of(&Prefs::default(), Scope::Sketch, "Move/Copy"), "M");
    assert_eq!(key_of(&Prefs::default(), Scope::Design, "Move/Copy"), "M");
  }

  #[test]
  fn polygon_constraint_runs_on_the_selected_lines() {
    use fox_engine::Sel;
    use serde_json::json;
    let mut app = App::with_session(fox_engine::Session::default());
    app.s.execute("sketch.create", &json!({"plane": "xy"})).unwrap();
    app.s.execute("sketch.polygon", &json!({"center": [0, 0], "vertex": [10, 0], "sides": 6})).unwrap();
    let sk = app.s.edit_sketch.unwrap();
    let sketch = app.s.design.sketch(sk).unwrap();
    let before = sketch.constraints.len();
    app.s.selection = sketch
      .curves
      .iter()
      .filter(|c| matches!(c.kind, fox_engine::sketch::CurveKind::Line { .. }))
      .map(|c| Sel::SketchCurve { sketch: sk, curve: c.id })
      .collect();
    Cmd::Constraint("polygon").run(&mut app);
    let sketch = app.s.design.sketch(sk).unwrap();
    assert!(sketch.constraints.len() > before, "no polygon constraint added: {:?}", app.status);
    assert!(app.s.selection.is_empty());
  }

  #[test]
  fn pinned_icons_join_their_group() {
    let mut prefs = Prefs::default();
    let create = &SOLID[0];
    let before: Vec<&str> = quick_items(&prefs, Tab::Solid, create).iter().map(|i| i.label).collect();
    assert!(before.contains(&"Extrude") && !before.contains(&"Shell"));
    prefs.pinned.push(customize::pin_id(Tab::Solid, "CREATE", "Torus"));
    prefs.unpinned.push(customize::pin_id(Tab::Solid, "CREATE", "Extrude"));
    let after: Vec<&str> = quick_items(&prefs, Tab::Solid, create).iter().map(|i| i.label).collect();
    assert_eq!(after.last(), Some(&"Torus"));
    assert!(!after.contains(&"Extrude"));
    // PLASTIC shares the menu but not the pins
    assert!(!quick_items(&prefs, Tab::Plastic, &PLASTIC[0]).iter().any(|i| i.label == "Torus"));
    // a default icon pinned again comes back at the group's end, not in its old slot (Fusion)
    prefs.pinned.push(customize::pin_id(Tab::Solid, "CREATE", "Extrude"));
    let again: Vec<&str> = quick_items(&prefs, Tab::Solid, create).iter().map(|i| i.label).collect();
    assert_eq!(again.last(), Some(&"Extrude"));
    assert_ne!(again.first(), Some(&"Extrude"));
  }

  /// Fusion's SKETCH tab at full width: CREATE 7, MODIFY 5 and all twelve constraints.
  #[test]
  fn sketch_panels_show_fusions_icon_counts() {
    let prefs = Prefs::default();
    let counts: Vec<(&str, usize)> =
      SKETCH.iter().map(|g| (g.label, quick_items(&prefs, Tab::Sketch, g).len())).collect();
    assert_eq!(counts[0], ("CREATE", 7));
    assert_eq!(counts[1], ("MODIFY", 5));
    assert_eq!(counts[2], ("CONSTRAINTS", 12));
    let names: Vec<&str> = quick_items(&prefs, Tab::Sketch, &SKETCH[2]).iter().map(|i| i.label).collect();
    assert!(!names.contains(&"Polygon"), "Polygon is in the menu only: {names:?}");
  }

  #[test]
  fn a_wide_toolbar_keeps_every_icon() {
    let panels =
      [PanelSize { label: 70.0, icons: 6 }, PanelSize { label: 70.0, icons: 7 }, PanelSize { label: 64.0, icons: 1 }];
    assert_eq!(fit_counts(&panels, 2000.0), vec![6, 7, 1]);
  }

  /// Icons come off the right of the widest panel first, and a panel never loses its last one
  /// or shrinks below its name.
  #[test]
  fn a_narrow_toolbar_drops_icons_from_the_widest_panel() {
    let create = PanelSize { label: 70.0, icons: 6 };
    let modify = PanelSize { label: 70.0, icons: 7 };
    let select = PanelSize { label: 64.0, icons: 1 };
    let panels = [create, modify, select];
    let full: f32 = panels.iter().map(|p| panel_width(p, p.icons)).sum();
    // one icon too many: MODIFY (the widest) gives one up
    assert_eq!(fit_counts(&panels, full - 1.0), vec![6, 6, 1]);
    // then the two trade off: CREATE is the widest once MODIFY is down to five
    assert_eq!(fit_counts(&panels, full - ICON_W * 3.0), vec![5, 5, 1]);
    // far too narrow: each panel keeps one icon (or its name's width) and the row overflows
    let counts = fit_counts(&panels, 10.0);
    assert_eq!(counts, vec![1, 1, 1]);
    // a panel whose name is wider than its icons isn't asked to give any up
    let named = [PanelSize { label: 200.0, icons: 3 }, PanelSize { label: 40.0, icons: 4 }];
    let two_icons = ICON_W * 2.0 - 1.0 + SEPARATOR_W;
    assert_eq!(fit_counts(&named, 200.0 + SEPARATOR_W + two_icons), vec![3, 2]);
  }

  /// `ui:toolbar:solid-modify-menu-list`: Fusion's SOLID > MODIFY, separators included, with no
  /// Direct Edit entry while design history is on; Simplify in Fusion's order with an icon each.
  #[test]
  fn the_solid_modify_list_is_fusions() {
    let rows: Vec<&str> = SOLID_MODIFY
      .iter()
      .map(|e| match e {
        E::I(i) => i.label,
        E::Sep => "|",
        E::Sub(l, _) => l,
      })
      .collect();
    assert_eq!(
      rows.join(", "),
      "Press Pull, Fillet, Chamfer, |, Shell, Draft, Scale, Combine, Offset Face, Replace Face, Split Face, \
       Split Body, Silhouette Split, |, Move/Copy, Align, Delete, Remove, Simplify, |, Physical Material, \
       Appearance, Manage Materials, |, Change Parameters, Compute All, |, Bill of Materials"
    );
    let mut simplify = Vec::new();
    labels(SIMPLIFY, &mut simplify);
    assert_eq!(simplify, ["Remove Features", "Remove Faces", "Replace with Primitives"]);
    let icons: Vec<Icon> = SIMPLIFY.iter().filter_map(|e| if let E::I(i) = e { Some(i.icon) } else { None }).collect();
    assert!(icons[0] != icons[1] && icons[1] != icons[2] && icons[0] != icons[2], "{icons:?}");
    // the direct edits come back with design history off
    let mut app = App::with_session(fox_engine::Session::default());
    let g = &SOLID[1];
    assert_eq!(menu_of(&app, Tab::Solid, g).len(), SOLID_MODIFY.len());
    app.s.design.history_off = true;
    assert!(matches!(menu_of(&app, Tab::Solid, g).last(), Some(E::Sub("Direct Edit", _))));
    assert_eq!(menu_of(&app, Tab::Surface, &SURFACE[1]).len(), SURFACE_MODIFY.len());
  }

  /// `ui:toolbar:solid-modify-menu-tooltips`: Draft and Compute All carry Fusion's paragraphs.
  #[test]
  fn draft_and_compute_all_tooltips_are_fusions() {
    let draft = tips::tip(Scope::Design, "Draft").unwrap();
    assert_eq!(draft.line, "Applies a fixed or parting line draft angle to planar faces.");
    assert_eq!(
      draft.more,
      "Select a pull direction, parting tool, and faces to draft, then specify the draft angle.\n\nPress Ctrl+/ \
       for more help."
    );
    let all = tips::tip(Scope::Design, "Compute All").unwrap();
    assert_eq!(all.line, "Computes all of the parameters, features, and assembly relationships in a design.");
    assert_eq!(
      all.more,
      "Use at major milestones in the design process and after making significant changes to features in the \
       Timeline, to ensure all assembly relationships are calculated properly.\n\nPress Ctrl+/ for more help."
    );
  }

  /// A menu's rows as `label` / `label (key)`, `|` for a separator and `name >` for a submenu.
  fn menu_rows(tab: Tab, menu: &[E]) -> Vec<String> {
    let prefs = Prefs::default();
    let at = At { prefs: &prefs, tab, group: "", quick: &[], unsaved: false };
    menu
      .iter()
      .map(|e| match e {
        E::I(i) if at.key(i).is_empty() => i.label.to_string(),
        E::I(i) => format!("{} ({})", i.label, at.key(i)),
        E::Sep => "|".into(),
        E::Sub(l, _) => format!("{l} >"),
      })
      .collect()
  }

  /// Fusion's SURFACE CREATE and MODIFY dropdowns, row for row: no Patch from Sketch, Variable
  /// Offset or separate Trim with Body / Sketch, and no E on the surface Extrude.
  #[test]
  fn surface_menus_are_fusions() {
    assert_eq!(
      menu_rows(Tab::Surface, SURFACE_CREATE).join(", "),
      "Create Sketch, Create Form, Derive, |, Extrude, Revolve, Sweep, Loft, Patch, Ruled, Offset, |, Pattern >, \
       Mirror, |, Thicken, Boundary Fill, Joint Origin"
    );
    assert_eq!(
      menu_rows(Tab::Surface, SURFACE_MODIFY).join(", "),
      "Press Pull (Q), Fillet (F), Chamfer, |, Trim, Untrim, Extend, Stitch, Unstitch, Reverse Normal, Scale, \
       Split Face, Split Body, |, Move/Copy (M), Align, Delete (Del), Remove, |, Physical Material, Appearance (A), \
       Manage Materials, |, Change Parameters, Compute All (Ctrl+B), |, Bill of Materials"
    );
    // the SOLID Extrude keeps its E
    assert!(menu_rows(Tab::Solid, SOLID_CREATE).contains(&"Extrude (E)".to_string()));
  }

  /// Fusion's SURFACE ribbon: five CREATE buttons and four MODIFY ones, the surface commands
  /// in orange.
  #[test]
  fn surface_quick_buttons_are_fusions() {
    let prefs = Prefs::default();
    let shown = |g: &Group| quick_items(&prefs, Tab::Surface, g).iter().map(|i| i.label).collect::<Vec<_>>();
    assert_eq!(shown(&SURFACE[0]), ["Create Sketch", "Patch", "Extrude", "Revolve", "Sweep"]);
    assert_eq!(shown(&SURFACE[1]), ["Stitch", "Unstitch", "Trim", "Extend"]);
    let modify = quick_items(&prefs, Tab::Surface, &SURFACE[1]);
    for it in quick_items(&prefs, Tab::Surface, &SURFACE[0]).iter().skip(1).chain(&modify) {
      assert_eq!(it.tint, Tint::Orange, "{}", it.label);
    }
  }

  /// The SURFACE buttons' tooltips are Fusion's, the surface Extrude and Sweep their own rather
  /// than the SOLID ones.
  #[test]
  fn surface_tooltips_are_fusions() {
    let prefs = Prefs::default();
    let tip = |g: &Group, label: &str| {
      let it = quick_items(&prefs, Tab::Surface, g).into_iter().find(|i| i.label == label).unwrap();
      tips::tip(Scope::Design, it.tip_label()).unwrap_or_else(|| panic!("{label} has a tooltip"))
    };
    let patch = tip(&SURFACE[0], "Patch");
    assert_eq!(patch.line, "Creates a planar or 3D surface within the boundary of a specified closed loop.");
    assert!(tips::illustrated(Scope::Design, "Patch"), "Fusion's Patch tooltip grows an illustration");
    let extrude = tip(&SURFACE[0], "Extrude");
    assert_eq!(extrude.line, "Adds depth to open or closed sketch profiles or planar faces.");
    assert_eq!(
      extrude.paragraphs().collect::<Vec<_>>()[1..],
      ["Select the profile or planar face then specify the distance to extrude.", tips::MORE_HELP]
    );
    assert_ne!(tips::tip(Scope::Design, "Extrude").unwrap().line, extrude.line, "SOLID keeps its own");
    assert!(tip(&SURFACE[0], "Revolve").line.starts_with("Revolves a sketch profile or planar face around"));
    assert!(tip(&SURFACE[0], "Sweep").line.starts_with("Sweeps a sketch profile or planar face along a"));
    assert!(tip(&SURFACE[1], "Stitch").line.starts_with("Joins surfaces to make a single surface body or solid body"));
    assert_eq!(tip(&SURFACE[1], "Unstitch").line, "Breaks joined surfaces into individual surfaces.");
    assert!(tip(&SURFACE[1], "Trim").line.starts_with("Removes the selected area of a surface body using"));
    assert!(tip(&SURFACE[1], "Extend").line.starts_with("Enlarges a surface by extending its edges a specified"));
  }

  /// The sketch MODIFY tooltips carry Fusion's wording: a first line, then the instruction and,
  /// where Fusion shows it, "Press Ctrl+/ for more help." as its own paragraph.
  #[test]
  fn sketch_modify_tooltips_are_fusions_paragraphs() {
    let help = "Press Ctrl+/ for more help.";
    for (label, line, paragraphs, has_help) in [
      ("Fillet", "Places an arc of a specified radius at the intersection of two lines or arcs.", 3, true),
      ("Blend Curve", "Creates a tangent or curvature continuous blend curve", 2, false),
      ("Offset", "Copies the selected sketch curves a specified distance from the original curves.", 3, true),
      ("Trim", "Trims a sketch curve to the nearest intersecting curve or boundary geometry.", 3, true),
      ("Extend", "Extends a curve to the nearest intersecting curve or boundary geometry.", 3, true),
      ("Break", "Breaks a sketch entity into two or more segments", 2, false),
      ("Sketch Scale", "Scales sketch geometry.", 2, false),
      ("Move/Copy", "Moves the selected face, body, sketch, or construction geometry", 3, true),
      ("Change Parameters", "Displays the Parameters dialog", 3, true),
    ] {
      let t = tips::tip(Scope::Sketch, label).unwrap_or_else(|| panic!("{label} has a tooltip"));
      assert!(t.line.starts_with(line), "{label}: {}", t.line);
      assert!(!t.line.contains("Shortcut"), "{label}: the row shows the key, not the tooltip");
      let more: Vec<&str> = t.more.split("\n\n").collect();
      assert_eq!(1 + more.len(), paragraphs, "{label}: {more:?}");
      assert_eq!(more.last() == Some(&help), has_help, "{label}: {more:?}");
    }
  }

  /// A Part Design document's ASSEMBLE, INSPECT and INSERT list what Fusion's do; the joint and
  /// component commands stay on the S toolbox and their keys.
  #[test]
  fn assemble_inspect_and_insert_are_fusions_part_design_lists() {
    let shared = |tab: Tab, group: &str| {
      let (own, shared) = groups(tab);
      own.iter().chain(shared.iter()).find(|g| g.label == group).unwrap().menu
    };
    for tab in [Tab::Solid, Tab::Surface, Tab::Mesh, Tab::SheetMetal, Tab::Plastic, Tab::Sketch] {
      assert_eq!(rows(shared(tab, "ASSEMBLE")), [("Add To Assembly", "")], "{tab:?}");
      for cmd in [D(Kind::Joint), D(Kind::AsBuiltJoint), D(Kind::NewComponent), D(Kind::MotionStudy)] {
        assert!(offers(tab, cmd), "{tab:?} still runs {cmd:?}");
      }
    }
    let prefs = Prefs::default();
    assert_eq!(key_of(&prefs, Scope::Design, "Joint"), "J");
    assert_eq!(key_of(&prefs, Scope::Design, "As-built Joint"), "Shift+J");
    assert_eq!(icon_of_label("Joint"), Some(Icon::JointOrigin), "the Undo list still finds its icon");

    let inspect: Vec<&str> = rows(INSPECT).into_iter().map(|(l, _)| l).collect();
    assert_eq!(inspect[inspect.len() - 2..], ["History", "Sustainability Insights"]);
    assert!(!matches!(INSPECT.last(), Some(E::Sep)));

    let insert: Vec<&str> = rows(INSERT).into_iter().map(|(l, _)| l).collect();
    assert_eq!(
      insert,
      [
        "Decal",
        "Canvas",
        "Insert SVG",
        "Insert DXF",
        "Insert Mesh",
        "Insert Component",
        "Insert Fastener",
        "Duplicate With Joints",
        "Insert Derive",
        "Insert McMaster-Carr Component",
        "Insert a manufacturer part",
        "Insert TraceParts Supplier Components",
      ]
    );
    assert!(!INSERT.iter().any(|e| matches!(e, E::Sep)));
    // Insert Component greys out until the design is saved
    let component = find(INSERT, "Insert Component").unwrap();
    let at = |unsaved| At { prefs: &prefs, tab: Tab::Solid, group: "INSERT", quick: &[], unsaved };
    assert!(at(true).shown(&component).disabled);
    assert!(!at(false).shown(&component).disabled);
    let mut app = App::with_session(fox_engine::Session::default());
    assert!(panels::is_unsaved(&app));
    app.s.path = Some("plate.fox".into());
    assert!(!panels::is_unsaved(&app));
    assert!(offers(Tab::Solid, Cmd::InsertComponent));
  }

  /// Every CONSTRUCT entry draws its own icon, as Fusion's do.
  #[test]
  fn construct_entries_each_have_their_own_icon() {
    let icons: Vec<(&str, Icon)> =
      CONSTRUCT.iter().filter_map(|e| if let E::I(i) = e { Some((i.label, i.icon)) } else { None }).collect();
    for (i, (label, icon)) in icons.iter().enumerate() {
      assert!(!icons[i + 1..].iter().any(|(_, o)| o == icon), "{label}'s {icon:?} is shared");
    }
  }

  #[test]
  fn an_assembly_design_has_fusions_assembly_tab() {
    let (own, shared) = groups(Tab::Assembly);
    let labels: Vec<&str> = own.iter().chain(shared.iter()).map(|g| g.label).collect();
    assert_eq!(
      labels,
      [
        "INSERT",
        "CREATE",
        "RELATIONSHIPS",
        "MOTION",
        "MODIFY",
        "CONTACT",
        "CONFIGURE",
        "CONSTRUCT",
        "INSPECT",
        "SELECT"
      ]
    );
    let names = |g: &str| -> Vec<&str> {
      own
        .iter()
        .find(|x| x.label == g)
        .unwrap()
        .menu
        .iter()
        .filter_map(|e| match e {
          E::I(i) => Some(i.label),
          E::Sub(l, _) => Some(*l),
          _ => None,
        })
        .collect()
    };
    assert_eq!(
      names("INSERT"),
      [
        "Insert Component",
        "Insert Fastener",
        "Duplicate With Joints",
        "Insert Derive",
        "Decal",
        "Canvas",
        "Add To Assembly",
        "Insert McMaster-Carr Component"
      ]
    );
    assert_eq!(names("CREATE"), ["New Component", "Create Sketch", "Pattern", "Create Linked PCB", "Derive"]);
    assert_eq!(
      names("RELATIONSHIPS"),
      [
        "Constrain Components",
        "Align Constraint",
        "Angle Constraint",
        "Center Constraint",
        "Tangent Constraint",
        "Joint",
        "As-built Joint",
        "Joint Origin",
        "Rigid Group"
      ]
    );
    assert_eq!(names("MOTION"), ["Drive Joints", "Motion Link", "Motion Study"]);
    assert_eq!(
      names("MODIFY"),
      [
        "Move/Copy",
        "Align",
        "Delete",
        "Remove",
        "Bill of Materials",
        "Change Parameters",
        "Physical Material",
        "Appearance",
        "Manage Materials",
        "Compute All"
      ]
    );
    assert_eq!(names("CONTACT"), ["Enable Contact Sets", "Enable All Contact"]);
    assert_eq!(cmd_of(Tab::Assembly, "RELATIONSHIPS", "Joint"), Some(D(Kind::Joint)));
    assert_eq!(cmd_of(Tab::Assembly, "CREATE", "New Component"), Some(D(Kind::NewComponent)));
  }

  #[test]
  fn insert_entries_open_the_insert_window() {
    use crate::insert_window::Target;
    for tab in [Tab::Solid, Tab::Sketch] {
      let group = "INSERT";
      assert_eq!(cmd_of(tab, group, "Decal"), Some(Cmd::Insert(Target::Decal)), "{tab:?}");
      assert_eq!(cmd_of(tab, group, "Insert SVG"), Some(Cmd::Insert(Target::Svg)));
      assert_eq!(cmd_of(tab, group, "Insert DXF"), Some(Cmd::InsertDxf));
      assert_eq!(cmd_of(tab, group, "Insert Mesh"), Some(Cmd::Insert(Target::Mesh)));
      assert_eq!(cmd_of(tab, group, "Insert Derive"), Some(Cmd::Insert(Target::Derive)));
    }
    assert_eq!(cmd_of(Tab::Mesh, "CREATE", "Insert Mesh"), Some(Cmd::Insert(Target::Mesh)));
  }

  /// A dropdown as Fusion's reads: each row with its shortcut in brackets, submenus with ">",
  /// separators as "|".
  fn listing(menu: &[E]) -> String {
    let rows: Vec<String> = menu
      .iter()
      .map(|e| match e {
        E::I(i) if i.key.is_empty() => i.label.to_string(),
        E::I(i) => format!("{} ({})", i.label, i.key),
        E::Sub(l, _) => format!("{l} >"),
        E::Sep => "|".into(),
      })
      .collect();
    rows.join(", ").replace(", |, ", " | ")
  }

  /// The SHEET METAL tab's ribbon and dropdowns are Fusion's, its subscription commands carry the
  /// badge, and the commands Fusion doesn't list stay reachable (S toolbox, shortcuts).
  #[test]
  fn sheet_metal_lists_are_fusions() {
    assert_eq!(
      listing(SHEET_CREATE),
      "Create Sketch, Flange (Base/Edge/Contour), Hem, Lofted Flange, Fold, Convert to Sheet Metal (Ctrl+Alt+S), \
       Derive | Extrude (E) | Hole (H), Thread, Pattern >, Mirror | Create Flat Pattern, Joint Origin"
    );
    assert_eq!(
      listing(SHEET_MODIFY),
      "Unfold, Sheet Metal Rules, Corner Closure, Join by bend, Rip | Sheet Metal Fillet, Sheet Metal Chamfer, \
       Fillet (F), Chamfer | Move/Copy (M), Align, Delete (Del), Remove | Physical Material, Appearance (A), \
       Manage Materials | Change Parameters, Compute All (Ctrl+B) | Bill of Materials"
    );
    let quick =
      |g: &Group| quick_items(&Prefs::default(), Tab::SheetMetal, g).iter().map(|i| i.label).collect::<Vec<_>>();
    assert_eq!(quick(&SHEET_METAL[0]), ["Flange (Base/Edge/Contour)", "Create Flat Pattern"]);
    assert_eq!(
      quick(&SHEET_METAL[1]),
      ["Sheet Metal Rules", "Sheet Metal Fillet", "Sheet Metal Chamfer", "Fillet", "Chamfer", "Bill of Materials"]
    );
    let paid: Vec<&str> =
      SHEET_MODIFY.iter().filter_map(|e| if let E::I(i) = e { i.paid.then_some(i.label) } else { None }).collect();
    assert_eq!(paid, ["Join by bend", "Sheet Metal Fillet", "Sheet Metal Chamfer"]);
    let offered: Vec<&str> = commands(&[Tab::SheetMetal]).iter().map(|e| e.label).collect();
    for l in
      ["Miter Flange", "Flat Pattern Sketch", "All Flat Patterns on One Sheet", "Cut Service Check", "Refold Faces"]
    {
      assert!(offered.contains(&l), "{l} stays reachable");
    }
    assert!(tips::tip(Scope::Design, "Bill of Materials").is_none(), "the tooltip is the bare name");
  }
}
