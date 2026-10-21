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
}

#[derive(Clone, Copy)]
pub(crate) enum E {
  I(Item),
  Sep,
  /// A submenu row (no icon), opening on hover.
  Sub(&'static str, &'static [E]),
}

/// A command FissionOxide runs.
const fn c(label: &'static str, icon: Icon, cmd: Cmd) -> E {
  E::I(Item { label, icon, tint: Tint::Blue, key: "", cmd: Some(cmd), disabled: false })
}

/// A Fusion command FissionOxide doesn't have yet.
const fn x(label: &'static str, icon: Icon) -> E {
  E::I(Item { label, icon, tint: Tint::Blue, key: "", cmd: None, disabled: false })
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
}

/// Most commands a group shows on the toolbar; every command stays in the group's menu.
const MAX_QUICK: usize = 8;

pub(crate) struct Group {
  label: &'static str,
  /// Labels of the entries shown as toolbar icons.
  quick: &'static [&'static str],
  menu: &'static [E],
}

use crate::form_ui::{self, Act as FormAct};
use crate::forms;
use crate::select::Priority;
use Cmd::Dialog as D;

const PATTERN: &[E] = &[
  c("Rectangular Pattern", Icon::Pattern, D(Kind::RectPattern)),
  c("Circular Pattern", Icon::CircPattern, D(Kind::CircPattern)),
  c("Pattern on Path", Icon::Pattern, D(Kind::Form(&forms::PATH_PATTERN))),
];

const SIMPLIFY: &[E] = &[
  c("Remove Faces", Icon::RemoveBody, D(Kind::Form(&forms::REMOVE_FACES))),
  c("Remove Features", Icon::RemoveBody, D(Kind::Form(&forms::REMOVE_FACES))),
  x("Replace with Primitives", Icon::Box),
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
  c("Create Base Feature", Icon::BaseFeature, D(Kind::Form(&config_ui::BASE_START))),
  E::Sub("Create PCB", CREATE_PCB),
  c("Joint Origin", Icon::JointOrigin, D(Kind::Form(&assembly_forms::JOINT_ORIGIN))),
];

const SOLID_MODIFY: &[E] = &[
  c("Press Pull", Icon::PressPull, D(Kind::PressPull)).k("Q"),
  E::Sep,
  c("Fillet", Icon::Fillet, D(Kind::Fillet)).k("F"),
  c("Chamfer", Icon::Chamfer, D(Kind::Chamfer)),
  E::Sep,
  c("Shell", Icon::Shell, D(Kind::Shell)),
  c("Draft", Icon::Draft, D(Kind::Draft)),
  c("Scale", Icon::Scale, D(Kind::Form(&forms::SCALE))),
  c("Combine", Icon::Combine, D(Kind::Combine)),
  c("Offset Face", Icon::OffsetFace, D(Kind::Form(&forms::OFFSET_FACE))),
  c("Replace Face", Icon::ReplaceFace, D(Kind::Form(&forms::REPLACE_FACE))),
  c("Split Face", Icon::SplitFace, D(Kind::Form(&forms::SPLIT_FACE))),
  c("Split Body", Icon::SplitBody, D(Kind::SplitBody)),
  x("Silhouette Split", Icon::SilhouetteSplit),
  E::Sep,
  c("Move/Copy", Icon::Move, D(Kind::Move)).k("M"),
  c("Align", Icon::Align, D(Kind::Form(&forms::ALIGN))),
  c("Delete", Icon::Remove, Cmd::Delete).k("Del"),
  c("Remove", Icon::RemoveBody, Cmd::RemoveBodies),
  E::Sep,
  E::Sub("Simplify", SIMPLIFY),
  E::Sep,
  c("Physical Material", Icon::PhysMaterial, D(Kind::Material)),
  c("Appearance", Icon::Appearance, D(Kind::Form(&forms::APPEARANCE))).k("A"),
  x("Manage Materials", Icon::ManageMaterials),
  E::Sep,
  c("Change Parameters", Icon::Params, Cmd::Params),
  c("Compute All", Icon::ComputeAll, Cmd::ComputeAll).k("Ctrl+B"),
  E::Sep,
  c("Bill of Materials", Icon::Bom, D(Kind::Bom)),
  E::Sep,
  E::Sub("Direct Edit", DIRECT_EDIT),
];

const DIRECT_EDIT: &[E] = &[
  c("Move Face", Icon::Move, D(Kind::Form(&config_ui::DIRECT_MOVE_FACE))),
  c("Press Pull", Icon::PressPull, D(Kind::Form(&config_ui::DIRECT_PRESS_PULL))),
  c("Delete Face", Icon::RemoveBody, D(Kind::Form(&config_ui::DIRECT_DELETE_FACE))),
  E::Sep,
  c("Create Base Feature", Icon::Box, D(Kind::Form(&config_ui::BASE_START))),
  c("Finish Base Feature", Icon::Box, D(Kind::Form(&config_ui::BASE_FINISH))),
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
  c("Plane at Angle", Icon::PlaneAlt, D(Kind::Form(&forms::PLANE_ANGLE))),
  c("Tangent Plane", Icon::PlaneAlt, D(Kind::Form(&forms::TANGENT_PLANE))),
  c("Midplane", Icon::Plane, D(Kind::Form(&forms::MIDPLANE))),
  c("Perpendicular Plane", Icon::PlaneAlt, D(Kind::Form(&forms::PLANE_ALONG_PATH))),
  c("Plane Through Two Edges", Icon::PlaneAlt, D(Kind::Form(&forms::PLANE_TWO_EDGES))),
  c("Plane Through Three Points", Icon::PlaneAlt, D(Kind::Form(&forms::PLANE_THREE_POINTS))),
  c("Plane Along Path", Icon::PlaneAlt, D(Kind::Form(&forms::PLANE_ALONG_PATH))),
  E::Sep,
  c("Axis Through Cylinder/Cone/Torus", Icon::Axis, D(Kind::Form(&forms::AXIS_FACE))),
  c("Axis Perpendicular To Face", Icon::Axis, D(Kind::Form(&forms::AXIS_NORMAL))),
  c("Axis Through Two Planes", Icon::Axis, D(Kind::Form(&forms::AXIS_TWO_PLANES))),
  c("Axis Through Two Points", Icon::Axis, D(Kind::Form(&forms::AXIS_TWO_POINTS))),
  c("Axis Through Edge", Icon::Axis, D(Kind::Form(&forms::AXIS_EDGE))),
  E::Sep,
  c("Point At Vertex", Icon::CPoint, D(Kind::Form(&forms::POINT_VERTEX))),
  c("Point Through Two Edges", Icon::CPoint, D(Kind::Form(&forms::POINT_TWO_EDGES))),
  c("Point Through Three Planes", Icon::CPoint, D(Kind::Form(&forms::POINT_THREE_PLANES))),
  c("Point At Center Of Circle/Sphere/Torus", Icon::CPoint, D(Kind::Form(&forms::POINT_CENTER))),
  c("Point At Edge And Plane", Icon::CPoint, D(Kind::Form(&forms::POINT_EDGE_PLANE))),
  c("Point Along Path", Icon::CPoint, D(Kind::Form(&forms::POINT_ALONG_PATH))),
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
  E::Sep,
  c("Properties", Icon::Properties, Cmd::Properties),
];

const INSERT: &[E] = &[
  x("Decal", Icon::Decal),
  c("Canvas", Icon::Canvas, Cmd::Canvas),
  c("Insert SVG", Icon::InsertSvg, Cmd::InsertStep),
  c("Insert DXF", Icon::InsertDxf, Cmd::InsertStep),
  c("Insert Mesh", Icon::InsertMesh, Cmd::InsertStep).g(),
  c("Insert Component", Icon::InsertComponent, Cmd::InsertComponent),
  x("Insert Fastener", Icon::Fastener).off(),
  x("Duplicate With Joints", Icon::DuplicateJoints).off(),
  x("Insert Derive", Icon::Derive),
  x("Insert McMaster-Carr Component", Icon::McMaster).off(),
  x("Insert a manufacturer part", Icon::Manufacturer).off(),
  x("Insert TraceParts Supplier Components", Icon::Manufacturer).off(),
  E::Sep,
  // not in Fusion's menu: FissionOxide's STEP import
  c("Insert STEP", Icon::Import, Cmd::InsertStep),
];

const ASSEMBLE: &[E] = &[
  x("Add To Assembly", Icon::AddToAssembly),
  E::Sep,
  // not in Fusion's menu: FissionOxide's component and joint commands
  c("New Component", Icon::Component, D(Kind::NewComponent)),
  c("Joint", Icon::JointOrigin, D(Kind::Joint)).k("J"),
  c("As-built Joint", Icon::JointOrigin, D(Kind::AsBuiltJoint)).k("Shift+J"),
  c("Drive Joints", Icon::DriveJoint, D(Kind::DriveJoint)),
  c("Motion Study", Icon::MotionStudy, D(Kind::MotionStudy)),
  c("Duplicate Component", Icon::Component, D(Kind::CopyComponent)),
  E::Sep,
  c("Joint Origin", Icon::JointOrigin, D(Kind::Form(&assembly_forms::JOINT_ORIGIN))),
  c("Rigid Group", Icon::Component, D(Kind::Form(&assembly_forms::RIGID_GROUP))),
  c("Motion Link", Icon::DriveJoint, D(Kind::Form(&assembly_forms::MOTION_LINK))),
  c("Contact Set", Icon::Interference, D(Kind::Form(&assembly_forms::CONTACT_SET))),
  c("Insert Component", Icon::InsertComponent, Cmd::InsertComponent),
  c("Cam Follower", Icon::DriveJoint, D(Kind::Form(&assembly_forms::CAM_FOLLOWER))),
];

const SELECTION_TOOLS: &[E] = &[
  c("Select All", Icon::Select, Cmd::SelectAll).k("Ctrl+A"),
  c("Select Other", Icon::Select, Cmd::SelectOther).k("Alt+click"),
  c("Invert Selection", Icon::Select, Cmd::SelectInvert),
];

const SELECTION_PRIORITY: &[E] = &[
  c("Select Body Priority", Icon::Body, Cmd::Priority(Priority::Body)),
  c("Select Face Priority", Icon::SplitFace, Cmd::Priority(Priority::Face)),
  c("Select Edge Priority", Icon::Line, Cmd::Priority(Priority::Edge)),
  c("Select Component Priority", Icon::Component, Cmd::Priority(Priority::Component)),
];

const SELECTION_FILTERS: &[E] = &[
  c("Bodies", Icon::Body, Cmd::Filter("bodies")),
  c("Faces", Icon::SplitFace, Cmd::Filter("faces")),
  c("Edges", Icon::Line, Cmd::Filter("edges")),
  c("Vertices", Icon::Point, Cmd::Filter("vertices")),
  c("Sketches", Icon::SketchSmall, Cmd::Filter("sketches")),
  c("Sketch Profiles", Icon::Rect, Cmd::Filter("profiles")),
  c("Construction", Icon::Plane, Cmd::Filter("construction")),
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
  c("Extrude", Icon::Extrude, D(Kind::Form(&forms::SURFACE_EXTRUDE))).o(),
  c("Revolve", Icon::Revolve, D(Kind::Form(&forms::SURFACE_REVOLVE))).o(),
  x("Sweep", Icon::Sweep).o(),
  x("Loft", Icon::Loft).o(),
  c("Patch", Icon::Patch, D(Kind::Form(&forms::PATCH))).o(),
  c("Patch from Sketch", Icon::Patch, D(Kind::Form(&forms::PATCH_SKETCH))).o(),
  c("Ruled", Icon::Ruled, D(Kind::Form(&forms::RULED))).o(),
  c("Offset", Icon::SurfOffset, D(Kind::Form(&forms::SURFACE_OFFSET))).o(),
  c("Variable Offset", Icon::SurfOffset, D(Kind::Form(&forms::VARIABLE_OFFSET))).o(),
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
  c("Trim", Icon::Trim, D(Kind::Form(&forms::TRIM))),
  c("Trim with Body", Icon::Trim, D(Kind::Form(&forms::TRIM_BODY))),
  c("Trim with Sketch", Icon::Trim, D(Kind::Form(&forms::TRIM_SKETCH))),
  c("Untrim", Icon::Untrim, D(Kind::Form(&forms::UNTRIM))).o(),
  c("Extend", Icon::Extend, D(Kind::Form(&forms::EXTEND))).o(),
  c("Stitch", Icon::Stitch, D(Kind::Form(&forms::STITCH))).o(),
  c("Unstitch", Icon::Unstitch, D(Kind::Form(&forms::UNSTITCH))).o(),
  c("Reverse Normal", Icon::ReverseNormal, D(Kind::Form(&forms::REVERSE_NORMAL))).o(),
  c("Scale", Icon::Scale, D(Kind::Form(&forms::SCALE))),
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
  x("Manage Materials", Icon::ManageMaterials),
  E::Sep,
  c("Change Parameters", Icon::Params, Cmd::Params),
  c("Compute All", Icon::ComputeAll, Cmd::ComputeAll).k("Ctrl+B"),
  E::Sep,
  c("Bill of Materials", Icon::Bom, D(Kind::Bom)),
];

const MESH_CREATE: &[E] = &[
  c("Insert Mesh", Icon::InsertMesh, Cmd::InsertStep).g(),
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
  c("Miter Flange", Icon::Flange, D(Kind::Form(&forms::MITER_FLANGE))),
  c("Fold", Icon::Fold, D(Kind::Fold)),
  c("Convert to Sheet Metal", Icon::ConvertSheet, D(Kind::ConvertSheet)).k("Ctrl+Alt+S"),
  x("Derive", Icon::Derive),
  E::Sep,
  c("Extrude", Icon::Extrude, D(Kind::Extrude)).k("E"),
  E::Sep,
  c("Hole", Icon::Hole, D(Kind::Hole)).k("H"),
  c("Thread", Icon::Thread, D(Kind::Form(&forms::THREAD))),
  E::Sep,
  E::Sub("Pattern", PATTERN),
  c("Mirror", Icon::Mirror, D(Kind::Mirror)),
  E::Sep,
  c("Create Flat Pattern", Icon::FlatPattern, D(Kind::FlatPattern)),
  c("Flat Pattern Sketch", Icon::FlatPattern, D(Kind::Form(&forms::FLAT_SKETCH))),
  c("Joint Origin", Icon::JointOrigin, D(Kind::Form(&assembly_forms::JOINT_ORIGIN))),
];

const SHEET_MODIFY: &[E] = &[
  c("Unfold", Icon::Unfold, D(Kind::Unfold)),
  c("Refold Faces", Icon::Refold, D(Kind::Refold)),
  c("Sheet Metal Rules", Icon::SmRules, D(Kind::SheetRules)),
  c("Corner Closure", Icon::CornerClosure, D(Kind::Form(&forms::CORNER_CLOSURE))),
  c("Join by bend", Icon::Fold, D(Kind::Form(&forms::JOIN_BY_BEND))),
  c("Rip", Icon::Rip, D(Kind::Rip)),
  E::Sep,
  c("Sheet Metal Fillet", Icon::Fillet, D(Kind::Form(&forms::SHEET_FILLET))),
  c("Sheet Metal Chamfer", Icon::Chamfer, D(Kind::Form(&forms::SHEET_CHAMFER))),
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
  x("Manage Materials", Icon::ManageMaterials),
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

// as in Fusion, only MESH has an EXPORT panel (elsewhere 3D Print is under UTILITIES > MAKE)
const MESH_SHARED: [Group; 7] = shared_groups!(Group { label: "EXPORT", quick: &["3D Print"], menu: EXPORT });

const SOLID: [Group; 2] = [
  Group {
    label: "CREATE",
    quick: &["Create Sketch", "Extrude", "Revolve", "Hole", "Rectangular Pattern", "Create Form"],
    menu: SOLID_CREATE,
  },
  Group {
    label: "MODIFY",
    quick: &["Press Pull", "Fillet", "Shell", "Combine", "Split Body", "Move/Copy", "Bill of Materials"],
    menu: SOLID_MODIFY,
  },
];

const SURFACE: [Group; 2] = [
  Group {
    label: "CREATE",
    quick: &["Create Sketch", "Patch", "Extrude", "Revolve", "Offset", "Create Form"],
    menu: SURFACE_CREATE,
  },
  Group {
    label: "MODIFY",
    quick: &["Stitch", "Unstitch", "Trim", "Extend", "Bill of Materials"],
    menu: SURFACE_MODIFY,
  },
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
  c("3-Point Arc", Icon::Arc3, T(Tool::Arc3)).k("A"),
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
  x("Intersect", Icon::Intersect),
  c("Include 3D Geometry", Icon::Include3D, D(Kind::Form(&forms::SK_INCLUDE))),
  c("Project To Surface", Icon::ProjectSurface, D(Kind::Form(&forms::SK_PROJECT_SURFACE))),
  c("Intersection Curve", Icon::Intersect, D(Kind::Form(&forms::SK_INTERSECTION_CURVE))),
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
  c("Trace Image", Icon::TraceImage, D(Kind::TraceImage)),
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
  c("Equal Distance Chamfer", Icon::SkChamfer, D(Kind::SkChamfer)),
  c("Distance-Distance Chamfer", Icon::SkChamfer, D(Kind::SkChamfer2)),
  c("Distance and Angle Chamfer", Icon::SkChamfer, D(Kind::SkChamferAngle)),
];

const SKETCH_MODIFY: &[E] = &[
  c("Fillet", Icon::SkFillet, D(Kind::SkFillet)),
  E::Sub("Chamfer", SK_CHAMFER),
  c("Blend Curve", Icon::BlendCurve, T(Tool::Blend)),
  c("Offset", Icon::SkOffset, D(Kind::SkOffset)).k("O"),
  E::Sep,
  c("Trim", Icon::SkTrim, T(Tool::Trim)).k("T"),
  c("Extend", Icon::SkExtend, T(Tool::Extend)),
  c("Break", Icon::SkBreak, T(Tool::Break)),
  c("Sketch Scale", Icon::SkScale, D(Kind::Form(&forms::SK_SCALE))),
  c("Scale to Size", Icon::SkScaleTo, D(Kind::Form(&forms::SK_SCALE_TO))),
  c("Move/Copy", Icon::Move, D(Kind::Move)).k("M"),
  E::Sep,
  c("Change Parameters", Icon::Params, Cmd::Params),
  E::Sep,
  c("Edit Text", Icon::SkText, D(Kind::Form(&forms::SK_TEXT_EDIT))),
  c("Explode Text", Icon::SkText, D(Kind::Form(&forms::SK_TEXT_EXPLODE))),
  E::Sep,
  c("Remove Duplicates", Icon::SkRemoveDuplicates, D(Kind::Form(&forms::SK_REMOVE_DUPLICATES))),
  c("Clean Up", Icon::SkCleanup, D(Kind::Form(&forms::SK_CLEANUP))),
];

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
  x("Decal", Icon::Decal),
  c("Canvas", Icon::Canvas, Cmd::Canvas),
  c("Insert SVG", Icon::InsertSvg, Cmd::InsertStep),
  c("Insert DXF", Icon::InsertDxf, Cmd::InsertStep),
  c("Sketch Library...", Icon::SketchLibrary, Cmd::SketchLibrary),
  c("Insert Mesh", Icon::InsertMesh, Cmd::InsertStep).g(),
  x("Insert Fastener", Icon::Fastener).off(),
  x("Duplicate With Joints", Icon::DuplicateJoints).off(),
  x("Insert Derive", Icon::Derive),
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
    quick: &["Line", "2-Point Rectangle", "Center Diameter Circle", "Fit Point Spline", "Mirror", "Sketch Dimension"],
    menu: SKETCH_CREATE,
  },
  Group { label: "MODIFY", quick: &["Fillet", "Trim", "Offset", "Blend Curve"], menu: SKETCH_MODIFY },
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

const CAM_ACTIONS: &[E] = &[
  c("Generate", Icon::Play, Cmd::CamGenerate).k("Ctrl+G"),
  c("Simulate", Icon::MotionStudy, Cmd::CamSimulate),
  c("Post Process", Icon::ExportDxf, Cmd::CamPost),
  c("Delete Setup", Icon::Remove, Cmd::CamDelete),
  c("2D Cut (Laser, Plasma, Waterjet)", Icon::ExportDxf, Cmd::Cut2d),
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

/// The Manufacture workspace's MILLING tab.
const MILLING: [Group; 5] = [
  Group { label: "SETUP", quick: &["New Setup", "Tool Library"], menu: CAM_SETUP },
  Group { label: "2D", quick: &["2D Adaptive Clearing", "2D Pocket", "Face", "2D Contour"], menu: CAM_2D },
  Group { label: "3D", quick: &["Parallel", "Scallop"], menu: CAM_3D },
  Group { label: "DRILLING", quick: &["Drill"], menu: CAM_DRILLING },
  Group { label: "ACTIONS", quick: &["Generate", "Simulate", "Post Process"], menu: CAM_ACTIONS },
];

const MANAGE: [Group; 1] =
  [Group { label: "BOM", quick: &["Bill of Materials"], menu: &[c("Bill of Materials", Icon::Bom, D(Kind::Bom))] }];

const ADD_INS: &[E] =
  &[c("Scripts and Add-Ins...", Icon::Scripts, Cmd::Plugins), x("Design and Make Marketplace", Icon::Marketplace)];

const UTILITY: &[E] = &[
  x("Manage Materials", Icon::ManageMaterials),
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
    quick: &["Sheet Metal Rules", "Join by bend", "Sheet Metal Fillet", "Fillet", "Chamfer", "Bill of Materials"],
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
const ALL_TABS: [Tab; 12] = [
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
];

/// Every command that can have a keyboard shortcut, with the key its menus show (a label that
/// shows a key on one tab has it on all of its toolbar's tabs).
pub(crate) fn bindings() -> &'static [Binding] {
  static ALL: std::sync::LazyLock<Vec<Binding>> = std::sync::LazyLock::new(|| {
    let mut out: Vec<Binding> = Vec::new();
    for e in commands(&ALL_TABS) {
      let id = e.id();
      match out.iter_mut().find(|b| b.id == id) {
        Some(b) if b.default.is_empty() => b.default = e.key,
        Some(_) => {}
        None => out.push(Binding { id, scope: Scope::of(e.tab), label: e.label, default: e.key }),
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

/// Resting this long on a command expands its tooltip.
const EXPAND_SECS: f64 = 2.0;

/// A command's tooltip, as in Fusion: its name, a line on what it does and its shortcut; after
/// resting on it for [`EXPAND_SECS`], a fuller explanation beside a large icon.
fn rich_tip(resp: egui::Response, scope: Scope, it: &Item, key: &str) -> egui::Response {
  let tip = tips::tip(scope, it.label);
  let ctx = resp.ctx.clone();
  let now = ctx.input(|i| i.time);
  // (hovered since, last seen hovered): a gap means the pointer left and came back
  let id = resp.id.with("tip-rest");
  let since = if resp.hovered() {
    ctx.data_mut(|d| {
      let (since, last) = d.get_temp::<(f64, f64)>(id).unwrap_or((now, now));
      let since = if now - last > 0.25 { now } else { since };
      d.insert_temp(id, (since, now));
      since
    })
  } else {
    now
  };
  let rested = now - since;
  resp.on_hover_ui(|ui| {
    let expanded = tip.is_some() && rested >= EXPAND_SECS;
    ui.set_max_width(if expanded { 380.0 } else { 300.0 });
    ui.label(RichText::new(it.label).strong());
    if let Some(t) = tip {
      ui.label(t.line);
    }
    if it.cmd.is_none() {
      ui.label(RichText::new("Not in FissionOxide yet").color(theme::MUTED));
    }
    if !key.is_empty() {
      ui.label(RichText::new(format!("Shortcut: {key}")).color(theme::MUTED));
    }
    match tip {
      Some(t) if expanded => {
        ui.separator();
        ui.horizontal_top(|ui| {
          let (r, _) = ui.allocate_exact_size(Vec2::splat(64.0), egui::Sense::hover());
          ui.painter().rect_filled(r, 3.0, Color32::from_gray(242));
          icons::draw_tinted(ui.painter(), r.shrink(8.0), it.icon, it.tint);
          ui.add(egui::Label::new(t.more).wrap());
        });
      }
      Some(_) => ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(EXPAND_SECS - rested)),
      None => {}
    }
  })
}

/// Every command the toolbar offers on `tabs`, in toolbar order, each label/command pair once.
pub(crate) fn commands(tabs: &[Tab]) -> Vec<Entry> {
  fn walk(menu: &[E], tab: Tab, out: &mut Vec<Entry>) {
    for e in menu {
      match e {
        E::I(Item { label, icon, tint, key, cmd: Some(cmd), disabled: false }) => {
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
  }
  out
}

/// Whether the toolbar offers `cmd` on `tab`.
pub(crate) fn offers(tab: Tab, cmd: Cmd) -> bool {
  commands(&[tab]).iter().any(|e| e.cmd == cmd)
}

/// Draws the tool groups of `tab`. Returns false for tabs without a layout yet.
pub fn tab_toolbar(app: &mut App, ui: &mut egui::Ui, tab: Tab) -> bool {
  let (own, shared) = groups(tab);
  let mut pick: Option<Item> = None;
  let mut act: Option<Action> = None;
  for g in own.iter().chain(shared.iter()) {
    show_group(app, ui, tab, g, &mut pick, &mut act);
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

/// Where a menu is: what its rows' options and tooltips need.
struct At<'a> {
  prefs: &'a Prefs,
  tab: Tab,
  group: &'static str,
  /// The group's default toolbar icons.
  quick: &'static [&'static str],
}

impl At<'_> {
  fn scope(&self) -> Scope {
    Scope::of(self.tab)
  }

  fn key(&self, it: &Item) -> &str {
    if it.cmd.is_some() { key_of(self.prefs, self.scope(), it.label) } else { it.key }
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

fn show_group(app: &App, ui: &mut egui::Ui, tab: Tab, g: &Group, pick: &mut Option<Item>, act: &mut Option<Action>) {
  let at = At { prefs: &app.prefs, tab, group: g.label, quick: g.quick };
  let popup_id = egui::Id::new(("toolbar-menu", tab, g.label));
  let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
  // FINISH SKETCH sits on light blue, like the contextual SKETCH tab
  let bg = ui.painter().add(egui::Shape::Noop);
  let inner = ui.vertical(|ui| {
    ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
    ui.add_space(2.0);
    ui.horizontal(|ui| {
      // like Fusion, a group shows a handful of its commands (and the ones pinned to it); the
      // rest live in its menu
      for it in quick_items(&app.prefs, tab, g) {
        let active = it.cmd.is_some_and(|c| c.active(app));
        let resp = icons::tool_button_bare(ui, it.icon, it.tint, active);
        let resp = rich_tip(resp, at.scope(), &it, at.key(&it));
        if it.cmd.is_some() {
          resp.context_menu(|ui| at.options(ui, &it, act));
        }
        if resp.clicked() {
          *pick = Some(it);
        }
      }
    });
    ui.add_space(3.0);
    let w = ui.min_rect().width();
    panels::group_label(ui, g.label, w, open)
  });
  // toggles (selection priority and filters) show a check mark while on
  let on: Vec<&'static str> =
    toggles(g.menu).into_iter().filter(|i| i.cmd.is_some_and(|c| c.active(app))).map(|i| i.label).collect();
  egui::Popup::menu(&inner.inner).id(popup_id).show(|ui| entries(ui, &at, g.menu, pick, act, &on));
  if matches!(g.label, "FINISH SKETCH" | "FINISH DRAWING") {
    ui.painter()
      .set(bg, egui::Shape::rect_filled(inner.response.rect.expand2(vec2(4.0, 2.0)), 2.0, theme::CONTEXT_TAB));
  }
  panels::separator(ui);
}

/// The toggle commands in a menu (and its submenus).
fn toggles(menu: &'static [E]) -> Vec<Item> {
  let mut out = Vec::new();
  for e in menu {
    match e {
      E::I(i) if matches!(i.cmd, Some(Cmd::Priority(_) | Cmd::Filter(_))) => out.push(*i),
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
            ui.painter().text(
              dots_rect.center(),
              egui::Align2::CENTER_CENTER,
              "⋮",
              egui::FontId::proportional(14.0),
              theme::TEXT,
            );
          }
          egui::containers::menu::SubMenu::new().show(ui, &dots, |ui| at.options(ui, it, act));
        }
        let r = rich_tip(r, at.scope(), it, at.key(it));
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
        match tips::tip(Scope::of(tab), l) {
          Some(t) if !t.line.is_empty() && t.more.len() > t.line.len() => {}
          _ => missing.push((tab, l)),
        }
      }
    }
    assert!(missing.is_empty(), "{missing:?}");
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
      (Tab::Sketch, "MODIFY", "Sketch Scale", D(Kind::Form(&forms::SK_SCALE))),
      (Tab::Sketch, "MODIFY", "Scale to Size", D(Kind::Form(&forms::SK_SCALE_TO))),
      (Tab::Sketch, "INSERT", "Sketch Library...", Cmd::SketchLibrary),
      (Tab::Sketch, "CREATE", "Project To Surface", D(Kind::Form(&forms::SK_PROJECT_SURFACE))),
      (Tab::Sketch, "CREATE", "Intersection Curve", D(Kind::Form(&forms::SK_INTERSECTION_CURVE))),
      (Tab::Sketch, "CONSTRAINTS", "Polygon", Cmd::Constraint("polygon")),
      (Tab::Solid, "MODIFY", "Align", D(Kind::Form(&forms::ALIGN))),
      (Tab::Surface, "MODIFY", "Align", D(Kind::Form(&forms::ALIGN))),
      (Tab::SheetMetal, "MODIFY", "Align", D(Kind::Form(&forms::ALIGN))),
      (Tab::Solid, "CREATE", "Create Base Feature", D(Kind::Form(&config_ui::BASE_START))),
      (Tab::Solid, "SELECT", "Construction", Cmd::Filter("construction")),
      (Tab::Sketch, "MODIFY", "Remove Duplicates", D(Kind::Form(&forms::SK_REMOVE_DUPLICATES))),
      (Tab::Sketch, "MODIFY", "Clean Up", D(Kind::Form(&forms::SK_CLEANUP))),
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
  }
}
