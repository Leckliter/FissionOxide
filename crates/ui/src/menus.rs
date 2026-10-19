//! The toolbar's tool groups and their drop-down menus for the SOLID, SURFACE, MESH, SHEET
//! METAL and PLASTIC tabs,
//! laid out entry for entry like Fusion's: same order, separators, shortcuts, submenus and
//! greyed-out items. Entries CLFusion can run carry a [`Cmd`]; the rest are listed so the
//! toolbar reads like Fusion and say so in the status line when clicked.
//!
//! To wire up a new command, give its entry a `Cmd` (and add the command itself in
//! `panels::Cmd`).

use egui::{Color32, Rect, Stroke, Vec2, pos2, vec2};

use crate::dialogs::Kind;
use crate::icons::{self, Icon, Tint};
use crate::panels::{self, Cmd, Tab};
use crate::{App, theme};

#[derive(Clone, Copy)]
pub(crate) struct Item {
  label: &'static str,
  icon: Icon,
  tint: Tint,
  /// Keyboard shortcut shown in the menu.
  key: &'static str,
  /// What it runs; None when CLFusion doesn't have the command yet.
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

/// A command CLFusion runs.
const fn c(label: &'static str, icon: Icon, cmd: Cmd) -> E {
  E::I(Item { label, icon, tint: Tint::Blue, key: "", cmd: Some(cmd), disabled: false })
}

/// A Fusion command CLFusion doesn't have yet.
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

pub(crate) struct Group {
  label: &'static str,
  /// Labels of the entries shown as toolbar icons.
  quick: &'static [&'static str],
  menu: &'static [E],
}

use Cmd::Dialog as D;

const PATTERN: &[E] = &[
  c("Rectangular Pattern", Icon::Pattern, D(Kind::RectPattern)),
  c("Circular Pattern", Icon::CircPattern, D(Kind::CircPattern)),
  x("Pattern on Path", Icon::Pattern),
];

const SIMPLIFY: &[E] = &[
  x("Remove Faces", Icon::RemoveBody),
  x("Remove Features", Icon::RemoveBody),
  x("Replace with Primitives", Icon::Box),
];

const CREATE_PCB: &[E] = &[x("Associative PCB", Icon::InsertComponent), x("Create 3D PCB", Icon::InsertComponent)];

const SOLID_CREATE: &[E] = &[
  c("Create Sketch", Icon::Sketch, Cmd::NewSketch),
  x("Create Form", Icon::CreateForm),
  x("Derive", Icon::Derive),
  E::Sep,
  c("Extrude", Icon::Extrude, D(Kind::Extrude)).k("E"),
  c("Revolve", Icon::Revolve, D(Kind::Revolve)),
  x("Sweep", Icon::Sweep),
  x("Loft", Icon::Loft),
  x("Rib", Icon::Rib),
  x("Web", Icon::Web),
  x("Emboss", Icon::Emboss),
  E::Sep,
  x("Hole", Icon::Hole).k("H"),
  x("Thread", Icon::Thread),
  E::Sep,
  c("Box", Icon::Box, D(Kind::Box)),
  c("Cylinder", Icon::Cylinder, D(Kind::Cylinder)),
  c("Sphere", Icon::Sphere, D(Kind::Sphere)),
  c("Torus", Icon::Torus, D(Kind::Torus)),
  x("Coil", Icon::Coil),
  x("Pipe", Icon::Pipe),
  E::Sep,
  E::Sub("Pattern", PATTERN),
  c("Mirror", Icon::Mirror, D(Kind::Mirror)),
  E::Sep,
  x("Thicken", Icon::Thicken),
  x("Boundary Fill", Icon::BoundaryFill),
  E::Sep,
  x("Create Base Feature", Icon::BaseFeature),
  E::Sub("Create PCB", CREATE_PCB),
  x("Joint Origin", Icon::JointOrigin),
];

const SOLID_MODIFY: &[E] = &[
  x("Press Pull", Icon::PressPull).k("Q"),
  E::Sep,
  c("Fillet", Icon::Fillet, D(Kind::Fillet)).k("F"),
  c("Chamfer", Icon::Chamfer, D(Kind::Chamfer)),
  E::Sep,
  c("Shell", Icon::Shell, D(Kind::Shell)),
  x("Draft", Icon::Draft),
  x("Scale", Icon::Scale),
  c("Combine", Icon::Combine, D(Kind::Combine)),
  x("Offset Face", Icon::OffsetFace),
  x("Replace Face", Icon::ReplaceFace),
  x("Split Face", Icon::SplitFace),
  x("Split Body", Icon::SplitBody),
  x("Silhouette Split", Icon::SilhouetteSplit),
  E::Sep,
  c("Move/Copy", Icon::Move, D(Kind::Move)).k("M"),
  x("Align", Icon::Align),
  c("Delete", Icon::Remove, Cmd::Delete).k("Del"),
  c("Remove", Icon::RemoveBody, Cmd::RemoveBodies),
  E::Sep,
  E::Sub("Simplify", SIMPLIFY),
  E::Sep,
  x("Physical Material", Icon::PhysMaterial),
  x("Appearance", Icon::Appearance).k("A"),
  x("Manage Materials", Icon::ManageMaterials),
  E::Sep,
  c("Change Parameters", Icon::Params, Cmd::Params),
  x("Compute All", Icon::ComputeAll).k("Ctrl+B"),
  E::Sep,
  x("Bill of Materials", Icon::Bom),
];

const CONFIGURE: &[E] = &[
  x("Configure", Icon::Configure),
  x("Display Configuration Table", Icon::ConfigTable),
  x("Create Configuration Rules", Icon::ConfigRules),
];

const CONSTRUCT: &[E] = &[
  x("User Coordinate System (UCS)", Icon::Ucs),
  E::Sep,
  c("Offset Plane", Icon::Plane, D(Kind::OffsetPlane)),
  x("Plane at Angle", Icon::PlaneAlt),
  x("Tangent Plane", Icon::PlaneAlt),
  x("Midplane", Icon::Plane),
  x("Perpendicular Plane", Icon::PlaneAlt),
  x("Plane Through Two Edges", Icon::PlaneAlt),
  x("Plane Through Three Points", Icon::PlaneAlt),
  x("Plane Along Path", Icon::PlaneAlt),
  E::Sep,
  x("Axis Through Cylinder/Cone/Torus", Icon::Axis),
  x("Axis Perpendicular To Face", Icon::Axis),
  x("Axis Through Two Planes", Icon::Axis),
  x("Axis Through Two Points", Icon::Axis),
  x("Axis Through Edge", Icon::Axis),
  E::Sep,
  x("Point At Vertex", Icon::CPoint),
  x("Point Through Two Edges", Icon::CPoint),
  x("Point Through Three Planes", Icon::CPoint),
  x("Point At Center Of Circle/Sphere/Torus", Icon::CPoint),
  x("Point At Edge And Plane", Icon::CPoint),
  x("Point Along Path", Icon::CPoint),
];

const INSPECT: &[E] = &[
  c("Measure", Icon::Measure, D(Kind::Inspect)).k("I"),
  x("Interference", Icon::Interference),
  x("Section Analysis", Icon::Section),
  E::Sep,
  x("Curvature Comb Analysis", Icon::CurvComb),
  x("Curvature Map Analysis", Icon::CurvMap),
  x("Draft Analysis", Icon::DraftAnalysis),
  x("Environment Map Analysis", Icon::EnvMap),
  x("Isocurve Analysis", Icon::Isocurve),
  x("Zebra Analysis", Icon::Zebra),
  E::Sep,
  x("Accessibility Analysis", Icon::Accessibility),
  x("Center of Mass", Icon::CenterOfMass),
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
  x("Decal", Icon::Decal),
  x("Canvas", Icon::Canvas),
  x("Insert SVG", Icon::InsertSvg),
  x("Insert DXF", Icon::InsertDxf),
  x("Insert Mesh", Icon::InsertMesh).g(),
  x("Insert Component", Icon::InsertComponent).off(),
  x("Insert Fastener", Icon::Fastener).off(),
  x("Duplicate With Joints", Icon::DuplicateJoints).off(),
  x("Insert Derive", Icon::Derive),
  x("Insert McMaster-Carr Component", Icon::McMaster).off(),
  x("Insert a manufacturer part", Icon::Manufacturer).off(),
  x("Insert TraceParts Supplier Components", Icon::Manufacturer).off(),
  E::Sep,
  // not in Fusion's menu: CLFusion's STEP import
  c("Insert STEP", Icon::Import, Cmd::InsertStep),
];

const ASSEMBLE: &[E] = &[x("Add To Assembly", Icon::AddToAssembly)];

const SELECTION_TOOLS: &[E] =
  &[x("Select All", Icon::Select).k("Ctrl+A"), x("Select Other", Icon::Select), x("Invert Selection", Icon::Select)];

const SELECTION_PRIORITY: &[E] = &[
  x("Select Body Priority", Icon::Body),
  x("Select Face Priority", Icon::SplitFace),
  x("Select Edge Priority", Icon::Line),
  x("Select Component Priority", Icon::Component),
];

const SELECTION_FILTERS: &[E] = &[
  x("Bodies", Icon::Body),
  x("Faces", Icon::SplitFace),
  x("Edges", Icon::Line),
  x("Vertices", Icon::Point),
  x("Sketches", Icon::SketchSmall),
  x("Sketch Profiles", Icon::Rect),
  x("Construction", Icon::Plane),
];

const SELECT: &[E] = &[
  c("Select", Icon::Select, Cmd::Select),
  E::Sep,
  x("Window Selection", Icon::WindowSel).k("1"),
  x("Freeform Selection", Icon::FreeformSel).k("2"),
  x("Paint Selection", Icon::PaintSel).k("3"),
  E::Sep,
  E::Sub("Selection Tools", SELECTION_TOOLS),
  E::Sep,
  E::Sub("Selection Priority", SELECTION_PRIORITY),
  E::Sub("Selection Filters", SELECTION_FILTERS),
];

const SURFACE_CREATE: &[E] = &[
  c("Create Sketch", Icon::Sketch, Cmd::NewSketch),
  x("Create Form", Icon::CreateForm),
  x("Derive", Icon::Derive),
  E::Sep,
  x("Extrude", Icon::Extrude).o(),
  x("Revolve", Icon::Revolve).o(),
  x("Sweep", Icon::Sweep).o(),
  x("Loft", Icon::Loft).o(),
  x("Patch", Icon::Patch).o(),
  x("Ruled", Icon::Ruled).o(),
  x("Offset", Icon::SurfOffset).o(),
  E::Sep,
  E::Sub("Pattern", PATTERN),
  c("Mirror", Icon::Mirror, D(Kind::Mirror)),
  E::Sep,
  x("Thicken", Icon::Thicken),
  x("Boundary Fill", Icon::BoundaryFill),
  x("Joint Origin", Icon::JointOrigin),
];

const SURFACE_MODIFY: &[E] = &[
  x("Press Pull", Icon::PressPull).k("Q"),
  c("Fillet", Icon::Fillet, D(Kind::Fillet)).k("F"),
  c("Chamfer", Icon::Chamfer, D(Kind::Chamfer)),
  E::Sep,
  x("Trim", Icon::Trim),
  x("Untrim", Icon::Untrim).o(),
  x("Extend", Icon::Extend).o(),
  x("Stitch", Icon::Stitch).o(),
  x("Unstitch", Icon::Unstitch).o(),
  x("Reverse Normal", Icon::ReverseNormal).o(),
  x("Scale", Icon::Scale),
  x("Split Face", Icon::SplitFace),
  x("Split Body", Icon::SplitBody),
  E::Sep,
  c("Move/Copy", Icon::Move, D(Kind::Move)).k("M"),
  x("Align", Icon::Align),
  c("Delete", Icon::Remove, Cmd::Delete).k("Del"),
  c("Remove", Icon::RemoveBody, Cmd::RemoveBodies),
  E::Sep,
  x("Physical Material", Icon::PhysMaterial),
  x("Appearance", Icon::Appearance).k("A"),
  x("Manage Materials", Icon::ManageMaterials),
  E::Sep,
  c("Change Parameters", Icon::Params, Cmd::Params),
  x("Compute All", Icon::ComputeAll).k("Ctrl+B"),
  E::Sep,
  x("Bill of Materials", Icon::Bom),
];

const MESH_CREATE: &[E] = &[
  x("Insert Mesh", Icon::InsertMesh).g(),
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
  x("Flange (Base/Edge/Contour)", Icon::Flange),
  x("Hem", Icon::Hem),
  x("Lofted Flange", Icon::LoftedFlange),
  x("Fold", Icon::Fold),
  x("Convert to Sheet Metal", Icon::ConvertSheet).k("Ctrl+Alt+S"),
  x("Derive", Icon::Derive),
  E::Sep,
  c("Extrude", Icon::Extrude, D(Kind::Extrude)).k("E"),
  E::Sep,
  x("Hole", Icon::Hole).k("H"),
  x("Thread", Icon::Thread),
  E::Sep,
  E::Sub("Pattern", PATTERN),
  c("Mirror", Icon::Mirror, D(Kind::Mirror)),
  E::Sep,
  x("Create Flat Pattern", Icon::FlatPattern),
  x("Joint Origin", Icon::JointOrigin),
];

const SHEET_MODIFY: &[E] = &[
  x("Unfold", Icon::Unfold),
  x("Sheet Metal Rules", Icon::SmRules),
  x("Corner Closure", Icon::CornerClosure),
  x("Join by bend", Icon::Configure),
  x("Rip", Icon::Rip),
  E::Sep,
  x("Sheet Metal Fillet", Icon::Configure),
  x("Sheet Metal Chamfer", Icon::Configure),
  c("Fillet", Icon::Fillet, D(Kind::Fillet)).k("F"),
  c("Chamfer", Icon::Chamfer, D(Kind::Chamfer)),
  E::Sep,
  c("Move/Copy", Icon::Move, D(Kind::Move)).k("M"),
  x("Align", Icon::Align),
  c("Delete", Icon::Remove, Cmd::Delete).k("Del"),
  c("Remove", Icon::RemoveBody, Cmd::RemoveBodies),
  E::Sep,
  x("Physical Material", Icon::PhysMaterial),
  x("Appearance", Icon::Appearance).k("A"),
  x("Manage Materials", Icon::ManageMaterials),
  E::Sep,
  c("Change Parameters", Icon::Params, Cmd::Params),
  x("Compute All", Icon::ComputeAll).k("Ctrl+B"),
  E::Sep,
  x("Bill of Materials", Icon::Bom),
];

const MESH_MODIFY: &[E] = &[
  x("Direct Edit", Icon::DirectEdit).g(),
  x("Remesh", Icon::Remesh).g(),
  x("Reduce", Icon::Reduce).g(),
  x("Plane Cut", Icon::PlaneCut).g(),
  x("Shell", Icon::Shell),
  x("Combine", Icon::Combine).g(),
  x("Smooth", Icon::Smooth).g(),
  x("Reverse Normal", Icon::ReverseNormal).g(),
  E::Sep,
  x("Separate", Icon::Separate).g(),
  E::Sep,
  c("Move/Copy", Icon::Move, D(Kind::Move)).k("M"),
  x("Scale Mesh", Icon::ScaleMesh).g(),
  c("Delete", Icon::Remove, Cmd::Delete).k("Del"),
  c("Remove", Icon::RemoveBody, Cmd::RemoveBodies),
  E::Sep,
  x("Convert Mesh", Icon::ConvertMesh).g(),
  E::Sep,
  x("Physical Material", Icon::PhysMaterial),
  x("Appearance", Icon::Appearance).k("A"),
  E::Sep,
  c("Change Parameters", Icon::Params, Cmd::Params),
  x("Compute All", Icon::ComputeAll).k("Ctrl+B"),
  E::Sep,
  x("Bill of Materials", Icon::Bom),
];

/// Groups every Design tab ends with.
macro_rules! shared_groups {
  () => {
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
      Group { label: "EXPORT", quick: &["3D Print"], menu: EXPORT },
    ]
  };
}

const SHARED: [Group; 7] = shared_groups!();

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

/// Draws the tool groups of `tab`. Returns false for tabs without a layout yet.
pub fn tab_toolbar(app: &mut App, ui: &mut egui::Ui, tab: Tab) -> bool {
  let own: &[Group] = match tab {
    Tab::Solid => &SOLID,
    Tab::Surface => &SURFACE,
    Tab::Mesh => &MESH,
    Tab::SheetMetal => &SHEET_METAL,
    Tab::Plastic => &PLASTIC,
    _ => return false,
  };
  let mut pick: Option<Item> = None;
  for g in own.iter().chain(SHARED.iter()) {
    show_group(app, ui, tab, g, &mut pick);
  }
  if let Some(it) = pick {
    match it.cmd {
      Some(c) => c.run(app),
      None => app.status = Some((format!("{} isn't in CLFusion yet", it.label), false)),
    }
  }
  true
}

fn show_group(app: &App, ui: &mut egui::Ui, tab: Tab, g: &Group, pick: &mut Option<Item>) {
  let popup_id = egui::Id::new(("toolbar-menu", tab, g.label));
  let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
  let inner = ui.vertical(|ui| {
    ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
    ui.add_space(2.0);
    ui.horizontal(|ui| {
      for label in g.quick {
        let Some(it) = find(g.menu, label) else { continue };
        let hint = match (it.cmd.is_some(), it.key.is_empty()) {
          (false, _) => "Not in CLFusion yet".to_string(),
          (true, true) => String::new(),
          (true, false) => format!("Shortcut: {}", it.key),
        };
        let active = it.cmd.is_some_and(|c| c.active(app));
        if icons::tool_button_tinted(ui, it.icon, it.tint, it.label, &hint, active).clicked() {
          *pick = Some(it);
        }
      }
    });
    ui.add_space(3.0);
    let w = ui.min_rect().width();
    panels::group_label(ui, g.label, w, open)
  });
  egui::Popup::menu(&inner.inner).id(popup_id).show(|ui| entries(ui, g.menu, pick));
  panels::separator(ui);
}

fn entries(ui: &mut egui::Ui, menu: &'static [E], pick: &mut Option<Item>) {
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
  let keys = menu.iter().any(|e| matches!(e, E::I(i) if !i.key.is_empty()));
  let subs = menu.iter().any(|e| matches!(e, E::Sub(..)));
  let width = widest + 44.0 + if keys { 72.0 } else { 0.0 } + if subs { 24.0 } else { 0.0 };
  ui.set_min_width(width);
  ui.set_max_width(width);
  for e in menu {
    match e {
      E::I(it) => {
        if row(ui, width, Some(it), it.label, false).clicked() && !it.disabled {
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
        let resp = row(ui, width, None, label, true);
        egui::containers::menu::SubMenu::new().show(ui, &resp, |ui| entries(ui, sub, pick));
      }
    }
  }
}

/// One menu row: icon, name, shortcut column, or a submenu arrow.
fn row(ui: &mut egui::Ui, width: f32, it: Option<&Item>, label: &str, sub: bool) -> egui::Response {
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
    if !i.key.is_empty() {
      p.text(
        pos2(rect.right() - 68.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        i.key,
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
