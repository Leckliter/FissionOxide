//! The Fusion-style workspace chrome: application bar with document tabs, the toolbar
//! (workspace switcher, tabs and tool groups), the browser, comments, navigation bar and
//! sketch palette floating over the canvas, the timeline, and the parameters window.

use egui::{Color32, Pos2, Rect, RichText, Stroke, Vec2, pos2, vec2};
use fox_engine::model::{BodyId, FeatureId, FeatureKind};
use fox_engine::render::StandardView;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use crate::dialogs::Kind;
use crate::icons::{self, Icon};
use crate::logo;
use crate::marking;
use crate::theme;
use crate::viewport::NavMode;
use crate::{App, RenameTarget, Tool};

/// Toolbar tabs of the Design workspace; `Sketch` is the contextual tab shown while sketching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tab {
  Solid,
  Surface,
  Mesh,
  SheetMetal,
  Plastic,
  Utilities,
  Manage,
  Sketch,
  /// The Drawing workspace's single tab.
  Drawing,
  /// The Manufacture workspace's milling tab.
  Milling,
  /// A tab named by installed plugins (an index into their tab names).
  Plugin(u8),
  /// The Form workspace's contextual tab (T-spline sculpting).
  Form,
  /// The Generative Design workspace's tab.
  Generative,
  /// The Render workspace's tab.
  Render,
  /// The Manufacture workspace's turning tab.
  Turning,
}

impl Tab {
  const MAIN: [Tab; 7] =
    [Tab::Solid, Tab::Surface, Tab::Mesh, Tab::SheetMetal, Tab::Plastic, Tab::Manage, Tab::Utilities];

  pub(crate) fn label(self) -> &'static str {
    match self {
      Tab::Solid => "SOLID",
      Tab::Surface => "SURFACE",
      Tab::Mesh => "MESH",
      Tab::SheetMetal => "SHEET METAL",
      Tab::Plastic => "PLASTIC",
      Tab::Utilities => "UTILITIES",
      Tab::Manage => "MANAGE",
      Tab::Sketch => "SKETCH",
      Tab::Drawing => "DRAWING",
      Tab::Milling => "MILLING",
      Tab::Plugin(_) => "PLUGINS",
      Tab::Form => "FORM",
      Tab::Generative => "GENERATIVE",
      Tab::Render => "RENDER",
      Tab::Turning => "TURNING",
    }
  }
}

const WORKSPACES: [&str; 8] =
  ["Design", "Generative Design", "Render", "Animation", "Simulation", "Manufacture", "Drawing", "Electronics"];

// ---------------------------------------------------------------- application bar

fn bar_icon(ui: &mut egui::Ui, icon: Icon, hint: &str, enabled: bool) -> egui::Response {
  let color = if enabled { None } else { Some(Color32::from_gray(175)) };
  let r = icons::icon_button(ui, icon, 28.0, hint, false, color);
  if enabled { r } else { r.on_hover_cursor(egui::CursorIcon::NotAllowed) }
}

pub fn app_bar(app: &mut App, ui: &mut egui::Ui) {
  egui::Panel::top("app_bar")
    .exact_size(38.0)
    .frame(egui::Frame::NONE.fill(theme::APP_BAR).inner_margin(egui::Margin { left: 6, right: 8, top: 0, bottom: 0 }))
    .show(ui, |ui| {
      ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let (r, _) = ui.allocate_exact_size(vec2(26.0, 28.0), egui::Sense::hover());
        logo::paint(ui.painter(), r.shrink(2.0));
        ui.add_space(4.0);
        if bar_icon(ui, Icon::DataPanel, if app.data.open { "Hide Data Panel" } else { "Show Data Panel" }, true)
          .clicked()
        {
          app.data.toggle();
        }
        ui.add_space(6.0);
        let file = caret_button(ui, Icon::File, "File", true);
        egui::Popup::menu(&file).show(|ui| file_menu(app, ui));
        ui.add_space(2.0);
        if bar_icon(ui, Icon::Save, "Save (Ctrl+S)", true).clicked() {
          app.save(false);
        }
        ui.add_space(6.0);
        // the icon undoes one step; the caret lists the steps to jump back (or forward) to
        let undo = caret_button(ui, Icon::Undo, "Undo (Ctrl+Z)", app.s.can_undo());
        let undo_list = caret_hit(&undo);
        if undo.clicked() && !undo_list && app.s.can_undo() {
          app.cancel_dialog();
          app.s.undo();
        }
        egui::Popup::menu(&undo)
          .open_memory(undo_list.then_some(egui::SetOpenCommand::Toggle))
          .show(|ui| history_menu(app, ui, true));
        let redo = caret_button(ui, Icon::Redo, "Redo (Ctrl+Y)", app.s.can_redo());
        let redo_list = caret_hit(&redo);
        if redo.clicked() && !redo_list && app.s.can_redo() {
          app.s.redo();
        }
        egui::Popup::menu(&redo)
          .open_memory(redo_list.then_some(egui::SetOpenCommand::Toggle))
          .show(|ui| history_menu(app, ui, false));
        ui.add_space(10.0);
        // the document tab fills the middle of the bar, its title centred, as in Fusion
        let right_w = 250.0;
        let tab_w = (ui.available_width() - right_w - 40.0).max(160.0);
        document_tab(app, ui, tab_w);
        ui.add_space(4.0);
        let (r, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), egui::Sense::click());
        ui.painter().rect_filled(
          r,
          2.0,
          if resp.hovered() { Color32::from_gray(192) } else { Color32::from_gray(205) },
        );
        icons::draw(ui.painter(), r.shrink(8.0), Icon::Plus, Color32::from_gray(70));
        if resp.on_hover_text("New Design (Ctrl+N)").clicked() {
          app.new_design();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          ui.spacing_mut().item_spacing.x = 4.0;
          avatar(app, ui);
          let help = bar_icon(ui, Icon::Help, "Help", true);
          egui::Popup::menu(&help).show(|ui| help_menu(app, ui));
          // the bell opens the Notifications panel under it; the dot shows while there is news
          let bell = bar_icon(ui, Icon::Bell, "Notifications", true);
          if bell.clicked() {
            crate::notifications::toggle(app);
          }
          app.notifications.anchor = bell.rect;
          if crate::notifications::unread(app) {
            ui.painter().circle_filled(bell.rect.center() + vec2(6.0, -7.0), 3.5, theme::ICON_ORANGE);
          }
          let _ = bar_icon(ui, Icon::Clock, "Recent activity", true);
          let _ = bar_icon(ui, Icon::Gauge, "Job Status: all changes are local", true);
          // the Autodesk Assistant button, shown pressed as in Fusion
          let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), egui::Sense::click());
          ui.painter().rect(r, 3.0, theme::CONTEXT_TAB, Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
          icons::draw_color(ui.painter(), r.shrink(5.0), Icon::Assistant);
          if resp.on_hover_text("Autodesk Assistant").clicked() {
            app.status = Some(("The Autodesk Assistant isn't part of FissionOxide".into(), false));
          }
          if icons::icon_button(ui, Icon::Configure, 28.0, "Plugins", app.plugins.open, None).clicked() {
            crate::plugins_ui::open_window(app);
          }
        });
      });
    });
}

/// Whether a click on a [`caret_button`] landed on its caret (the drop-down) rather than its icon.
fn caret_hit(resp: &egui::Response) -> bool {
  resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| p.x >= resp.rect.right() - 14.0)
}

/// Most steps the Undo and Redo lists show.
const HISTORY_ROWS: usize = 25;

/// The Undo (or Redo) drop-down, as Fusion's: "Undo Fillet", "Undo Finish Sketch", … newest
/// first, each with its command's icon; picking one undoes (redoes) back to before (after) it.
fn history_menu(app: &mut App, ui: &mut egui::Ui, undo: bool) {
  ui.set_min_width(230.0);
  let names = if undo { app.s.undo_history() } else { app.s.redo_history() };
  if names.is_empty() {
    ui.label(RichText::new(if undo { "Nothing to undo" } else { "Nothing to redo" }).color(theme::MUTED));
    return;
  }
  for (i, name) in names.iter().enumerate().take(HISTORY_ROWS) {
    let steps = i + 1;
    let verb = if undo { "Undo" } else { "Redo" };
    let hint = format!("{verb} {steps} step{}", if steps == 1 { "" } else { "s" });
    if menu_item(ui, step_icon(name), &format!("{verb} {name}"), None).on_hover_text(hint).clicked() {
      if undo {
        app.cancel_dialog();
        app.s.undo_steps(steps);
      } else {
        app.s.redo_steps(steps);
      }
      ui.close();
    }
  }
  if names.len() > HISTORY_ROWS {
    ui.label(RichText::new(format!("… {} more", names.len() - HISTORY_ROWS)).small().color(theme::MUTED));
  }
}

/// The icon the Undo list shows for a step: its command's (as the toolbar menus draw it), the
/// edited feature's command's for "Edit Fillet2" and the like, else a generic history icon.
pub(crate) fn step_icon(name: &str) -> Icon {
  if let Some(icon) = crate::menus::icon_of_label(name) {
    return icon;
  }
  let (verb, rest) = name.split_once(' ').unwrap_or((name, ""));
  match verb {
    "Finish" => Icon::Finish,
    "Delete" | "Remove" => Icon::Remove,
    "Edit" | "Suppress" | "Unsuppress" | "Rename" | "Reorder" | "Hide" | "Show" => {
      let base = rest.split(' ').next().unwrap_or("").trim_end_matches(|c: char| c.is_ascii_digit());
      crate::menus::icon_of_label(base).unwrap_or(Icon::History)
    }
    "Change" if rest == "Units" => Icon::Gear,
    "Add" | "Change" if rest.starts_with("Parameter") => Icon::Params,
    _ => Icon::History,
  }
}

/// An application bar icon with a drop-down caret (File, Undo, Redo).
fn caret_button(ui: &mut egui::Ui, icon: Icon, hint: &str, enabled: bool) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 28.0), egui::Sense::click());
  if resp.hovered() && enabled {
    ui.painter().rect(rect, 2.0, theme::HOVER, Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
  }
  let ink = if enabled { theme::ICON_INK } else { Color32::from_gray(175) };
  let ir = Rect::from_center_size(rect.center() - vec2(5.0, 0.0), Vec2::splat(19.0));
  if enabled && icon == Icon::File {
    icons::draw_color(ui.painter(), ir, icon);
  } else {
    icons::draw(ui.painter(), ir, icon, ink);
  }
  icons::chevron(ui.painter(), rect.right_center() - vec2(7.0, 0.0), ink);
  resp.on_hover_text(hint)
}

/// Fusion's File menu entries, in its order. The ones that need a Fusion Team cloud project are
/// greyed, as Fusion greys them on an unsaved design.
pub(crate) const FILE_MENU: [(&str, &str, bool); 15] = [
  ("New...", "", true),
  ("Add To Assembly", "", false),
  ("Open...", "Ctrl+O", true),
  ("Open Recovered Documents (0)", "", false),
  ("Upload...", "", true),
  ("Save", "Ctrl+S", true),
  ("Save As", "", true),
  ("Save As Latest", "", false),
  ("Export...", "", true),
  ("Create Version", "", false),
  ("3D Print", "", true),
  ("Capture Image...", "", true),
  ("Share File Link", "", false),
  ("View Details on Web", "", false),
  ("View", "", true),
];

/// Why a greyed File menu entry is greyed.
const CLOUD_ONLY: &str = "Needs a design saved to a Fusion Team cloud project; FissionOxide keeps designs locally";

fn file_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(240.0);
  let item = |ui: &mut egui::Ui, label: &str, key: &str, enabled: bool| -> bool {
    let mut b = egui::Button::new(label).min_size(vec2(220.0, 0.0));
    if !key.is_empty() {
      b = b.shortcut_text(key);
    }
    let r = ui.add_enabled(enabled, b);
    if enabled { r.clicked() } else { r.on_disabled_hover_text(CLOUD_ONLY).clicked() }
  };
  for (i, (label, key, enabled)) in FILE_MENU.iter().enumerate() {
    // the groups Fusion separates
    if matches!(i, 2 | 5 | 8 | 12 | 14) {
      ui.separator();
    }
    match *label {
      "Export..." => {
        ui.menu_button("Export...", |ui| {
          for f in fox_engine::exchange::FORMATS.iter().filter(|f| f.export) {
            let label = format!("{} (*.{})", f.name, f.extensions.join(", *."));
            if ui.button(label).on_hover_text(f.note).clicked() {
              ui.close();
              app.export(f.id);
            }
          }
        });
        continue;
      }
      "View" => {
        ui.menu_button("View", |ui| {
          ui.checkbox(&mut app.browser_open, "Show Browser");
          ui.checkbox(&mut app.show_comments, "Show Comments");
          ui.checkbox(&mut app.show_navbar, "Show Navigation Bar");
          ui.checkbox(&mut app.show_viewcube, "Show ViewCube");
          crate::explode_ui::view_menu(app, ui);
        });
        continue;
      }
      _ => {}
    }
    if !item(ui, label, key, *enabled) {
      continue;
    }
    ui.close();
    file_menu_action(app, label);
  }
}

/// Runs a File menu entry.
pub(crate) fn file_menu_action(app: &mut App, label: &str) {
  match label {
    "New..." => app.new_design(),
    "Open..." => app.open_dialog(),
    "Upload..." => crate::data_panel::upload_dialog(app),
    "Save" => app.save(false),
    "Save As" => app.save(true),
    "3D Print" => crate::capture::open_print(app),
    "Capture Image..." => crate::capture::open_image(app),
    _ => {}
  }
}

/// Fusion's Help (?) menu, less what FissionOxide has no counterpart for (Quick Setup waits for
/// the Preferences dialog).
fn help_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(260.0);
  if ui.button("Learning and Documentation").on_hover_text(crate::help::DOCS).clicked() {
    ui.close();
    crate::help::open(app, crate::help::DOCS);
  }
  if ui.button("Keyboard Shortcuts").on_hover_text("Every key FissionOxide binds").clicked() {
    ui.close();
    app.help.shortcuts = true;
  }
  if ui.button("What's New").on_hover_text("This version's release notes").clicked() {
    ui.close();
    crate::help::whats_new(app);
  }
  ui.separator();
  if ui.button("Diagnostic Log").on_hover_text("Open the folder with FissionOxide's log").clicked() {
    ui.close();
    crate::help::diagnostic_log(app);
  }
  if ui.button(app.updates.menu_label()).clicked() {
    ui.close();
    app.updates.check_now();
  }
  ui.separator();
  ui.label(RichText::new("Navigation").strong());
  let nav = app.preferences.values.nav;
  {
    use crate::preferences::NavAction::*;
    ui.label(format!("Orbit: {} · Pan: {} · Zoom: {}", nav.describe(Orbit), nav.describe(Pan), nav.describe(Zoom)));
  }
  ui.separator();
  ui.horizontal(|ui| {
    let (r, _) = ui.allocate_exact_size(vec2(20.0, 22.0), egui::Sense::hover());
    logo::paint(ui.painter(), r);
    ui.label(RichText::new("FissionOxide").strong());
    ui.label(RichText::new(format!("{} · parametric CAD on OpenCASCADE", fox_desktop::VERSION)).color(theme::MUTED));
  });
}

/// The profile button; its menu has Preferences, as in Fusion.
fn avatar(app: &mut App, ui: &mut egui::Ui) {
  let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), egui::Sense::click());
  ui.painter().circle_filled(rect.center(), 12.0, Color32::from_rgb(92, 84, 196));
  ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "CL", egui::FontId::proportional(11.0), Color32::WHITE);
  let resp = resp.on_hover_text("Local user");
  egui::Popup::menu(&resp).show(|ui| {
    ui.set_min_width(180.0);
    ui.label(RichText::new("Local user").strong());
    ui.separator();
    if ui.button("Preferences").clicked() {
      ui.close();
      crate::preferences::open(app);
    }
  });
}

fn document_tab(app: &mut App, ui: &mut egui::Ui, w: f32) {
  let name = app
    .s
    .path
    .as_ref()
    .and_then(|p| p.file_stem())
    .map(|n| n.to_string_lossy().to_string())
    .unwrap_or_else(|| app.s.design.name.clone());
  let label = if app.s.dirty { format!("{name} v1*") } else { format!("{name} v1") };
  let font = egui::FontId::proportional(13.5);
  let full = ui.max_rect();
  let (slot, _) = ui.allocate_exact_size(vec2(w, 30.0), egui::Sense::hover());
  // the tab runs to the bottom edge of the bar and merges into the toolbar
  let rect = Rect::from_min_max(pos2(slot.min.x, full.bottom() - 33.0), pos2(slot.max.x, full.bottom() + 1.0));
  let p = ui.painter();
  p.rect_filled(rect, egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, theme::TOOLBAR);
  let galley = p.layout_no_wrap(label, font, theme::TEXT);
  let tx = rect.center().x - galley.size().x / 2.0 + 11.0;
  icons::draw_color(p, Rect::from_center_size(pos2(tx - 14.0, rect.center().y), Vec2::splat(17.0)), Icon::Component);
  p.galley(pos2(tx, rect.center().y - galley.size().y / 2.0), galley, theme::TEXT);
  let close = Rect::from_center_size(rect.right_center() - vec2(16.0, 0.0), Vec2::splat(18.0));
  let cr = ui.interact(close, ui.id().with("doc-close"), egui::Sense::click());
  if cr.hovered() {
    ui.painter().rect_filled(close, 2.0, Color32::from_gray(222));
  }
  icons::draw(ui.painter(), close, Icon::Close, theme::ICON_INK);
  if cr.on_hover_text("Close (opens a new empty design)").clicked() {
    app.new_design();
  }
}

// ---------------------------------------------------------------- toolbar

/// One command: shown as an icon in its toolbar group, as a row in the group's menu and in the
/// marking menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Cmd {
  NewSketch,
  Dialog(Kind),
  Params,
  InsertStep,
  Select,
  SketchTool(Tool),
  Construction,
  Constraint(&'static str),
  FinishSketch,
  /// Remove the selected bodies (a timeline feature, unlike Delete).
  RemoveBodies,
  /// EXPORT > 3D Print: the design as an STL mesh.
  Print3d,
  Undo,
  Redo,
  Delete,
  LookAt,
  Fit,
  Home,
  /// Show or hide the selected bodies.
  ToggleVisibility,
  /// A drawing view dialog preset to a view type (front, section, detail, …).
  DrawingView(&'static str),
  /// Write the open drawing as pdf / dxf / svg.
  DrawingExport(&'static str),
  /// Back to the Design workspace.
  FinishDrawing,
  SelectAll,
  SelectInvert,
  SelectOther,
  /// Window selection (a left-drag with the select tool; this just switches to it).
  WindowSelect,
  Priority(crate::select::Priority),
  /// Toggle a selection filter: bodies, faces, edges, sketches or profiles.
  Filter(&'static str),
  /// Recompute the whole timeline (in the background).
  ComputeAll,
  /// Manufacture: compute the toolpaths, write G-code, delete the setup.
  CamGenerate,
  CamPost,
  CamDelete,
  /// Insert a design from the data hub as a component (opens the data panel).
  InsertComponent,
  /// Manufacture: run the stock simulation; open the tool library.
  CamSimulate,
  CamTools,
  /// Show or hide the configuration table.
  ConfigTable,
  /// Turn "capture design history" off (collapsing the timeline into a base feature) or on.
  CaptureHistory,
  /// Freeform or paint selection (window selection is [`Cmd::WindowSelect`]).
  SelectMode(crate::select::Mode),
  /// Fabrication: the 2D cut window (laser, plasma, waterjet G-code).
  Cut2d,
  /// INSERT > Canvas: a reference image on a plane.
  Canvas,
  /// UTILITIES > ADD-INS: the Plugins window.
  Plugins,
  /// INSPECT > Properties: show or hide the docked Properties panel.
  Properties,
  /// A Form workspace command (see [`crate::form_ui`]).
  Form(crate::form_ui::Act),
  /// Generative Design: study, preserve, obstacle, start, load, constraint, criteria,
  /// precheck, generate, explore.
  Gen(&'static str),
  /// INSERT > Sketch Library: pick a part, then click in the sketch to place it.
  SketchLibrary,
  /// The Render workspace's commands.
  Render(crate::render_ui::RenderCmd),
  /// Fabrication: check a cut file for a cutting service, with the cut estimate.
  CutCheck,
}

impl Cmd {
  pub(crate) fn info(self) -> (Icon, &'static str, &'static str) {
    match self {
      Cmd::NewSketch => (Icon::Sketch, "Create Sketch", "Pick a plane or planar face to sketch on"),
      Cmd::Form(a) => crate::form_ui::info(a),
      Cmd::Dialog(k) => match k {
        Kind::Extrude => (Icon::Extrude, "Extrude", "Add depth to a sketch profile (E)"),
        Kind::Revolve => (Icon::Revolve, "Revolve", "Revolve a sketch profile around an axis"),
        Kind::Box => (Icon::Box, "Box", "Create a solid box"),
        Kind::Cylinder => (Icon::Cylinder, "Cylinder", "Create a solid cylinder"),
        Kind::Sphere => (Icon::Sphere, "Sphere", "Create a solid sphere"),
        Kind::Torus => (Icon::Torus, "Torus", "Create a solid torus"),
        Kind::RectPattern => (Icon::Pattern, "Rectangular Pattern", "Copy bodies in rows and columns"),
        Kind::CircPattern => (Icon::CircPattern, "Circular Pattern", "Copy bodies around an axis"),
        Kind::Mirror => (Icon::Mirror, "Mirror", "Mirror bodies or features across a plane"),
        Kind::Fillet => (Icon::Fillet, "Fillet", "Round selected edges (F)"),
        Kind::Chamfer => (Icon::Chamfer, "Chamfer", "Bevel selected edges"),
        Kind::Shell => (Icon::Shell, "Shell", "Hollow a body, removing selected faces"),
        Kind::Combine => (Icon::Combine, "Combine", "Join, cut or intersect bodies"),
        Kind::Move => (Icon::Move, "Move/Copy", "Move or rotate bodies"),
        Kind::OffsetPlane => (Icon::Plane, "Offset Plane", "Construction plane offset from a plane or face"),
        Kind::Inspect => (Icon::Measure, "Measure", "Measure and inspect the selection"),
        Kind::PickPlane => (Icon::Sketch, "Create Sketch", "Pick a plane"),
        k => k.info(),
      },
      Cmd::Params => (Icon::Params, "Change Parameters", "User parameters that drive the design"),
      Cmd::InsertStep => (Icon::Import, "Insert STEP", "Insert a STEP file as bodies"),
      Cmd::Select => (Icon::Select, "Select", "Select faces, edges, bodies and profiles"),
      Cmd::SketchTool(t) => match t {
        Tool::Line => (Icon::Line, "Line", "Line (L): click points, Esc to finish"),
        Tool::Rect => (Icon::Rect, "2-Point Rectangle", "Rectangle from two corners (R)"),
        Tool::CenterRect => (Icon::CenterRect, "Center Rectangle", "Rectangle from centre and corner"),
        Tool::Circle => (Icon::Circle, "Center Diameter Circle", "Circle from centre and a point (C)"),
        Tool::Arc3 => (Icon::Arc3, "3-Point Arc", "Arc through start, end and a point (A)"),
        Tool::CenterArc => (Icon::CenterArc, "Center Point Arc", "Arc from centre, start and end"),
        Tool::Polygon => (Icon::Polygon, "Inscribed Polygon", "Hexagon from centre and vertex"),
        Tool::Slot => (Icon::Slot, "Center to Center Slot", "Slot from two centres and a width"),
        Tool::Point => (Icon::Point, "Point", "Sketch point"),
        Tool::Dimension => (Icon::Dimension, "Sketch Dimension", "Dimension geometry (D)"),
        Tool::Spline => (Icon::SplineFit, "Fit Point Spline", "Smooth curve through clicked points"),
        Tool::Trim => (Icon::SkTrim, "Trim", "Remove the piece of a curve you click (T)"),
        Tool::Extend => (Icon::SkExtend, "Extend", "Extend a line to the next curve"),
        Tool::Rect3 => (Icon::Rect3, "3-Point Rectangle", "Rectangle at any angle from one side and a height"),
        Tool::Circle2P => (Icon::Circle2P, "2-Point Circle", "Circle from the ends of a diameter"),
        Tool::Circle3P => (Icon::Circle3P, "3-Point Circle", "Circle through three points"),
        Tool::TangentArc => (Icon::TangentArc, "Tangent Arc", "Arc continuing smoothly from a line or arc"),
        Tool::CircPolygon => (Icon::PolygonCirc, "Circumscribed Polygon", "Hexagon from centre and a side's middle"),
        Tool::EdgePolygon => (Icon::PolygonEdge, "Edge Polygon", "Hexagon from one side"),
        Tool::OverallSlot => (Icon::OverallSlot, "Overall Slot", "Slot from its overall length and width"),
        Tool::CenterSlot => (Icon::CenterPointSlot, "Center Point Slot", "Slot from its centre, an end and width"),
        Tool::ArcSlot3 => (Icon::ArcSlot3, "Three Point Arc Slot", "Curved slot through three points"),
        Tool::ArcSlotCenter => (Icon::ArcSlotCenter, "Center Point Arc Slot", "Curved slot around a centre"),
        Tool::ControlSpline => (Icon::SplineCtrl, "Control Point Spline", "Spline shaped by control points"),
        Tool::Ellipse => (Icon::Ellipse, "Ellipse", "Ellipse from centre, major axis and minor radius"),
        Tool::Conic => (Icon::Conic, "Conic Curve", "Curve from start to end bent towards an apex"),
        Tool::MidpointLine => (Icon::MidpointLine, "Midpoint Line", "Line from its midpoint"),
        Tool::Select => (Icon::Select, "Select", "Select sketch geometry"),
        Tool::Break => (Icon::SkBreak, "Break", "Split a curve at its crossings, keeping every piece"),
        Tool::Circle2T => (Icon::Circle2T, "2-Tangent Circle", "Circle tangent to two curves"),
        Tool::Circle3T => (Icon::Circle3T, "3-Tangent Circle", "Circle tangent to three curves"),
        Tool::Blend => {
          (Icon::BlendCurve, "Blend Curve", "Tangent (or curvature) continuous spline joining two curve ends")
        }
      },
      Cmd::Construction => (Icon::Construction, "Construction", "Toggle construction linetype (X)"),
      Cmd::Constraint(kind) => {
        let (icon, name) = constraint_info(kind);
        (icon, name, "Applies to the current selection")
      }
      Cmd::FinishSketch => (Icon::Finish, "Finish Sketch", "Finish the sketch and return to the model"),
      Cmd::Undo => (Icon::Undo, "Undo", "Undo (Ctrl+Z)"),
      Cmd::Redo => (Icon::Redo, "Redo", "Redo (Ctrl+Y)"),
      Cmd::Delete => (Icon::Remove, "Delete", "Delete the selection (Del)"),
      Cmd::RemoveBodies => (Icon::RemoveBody, "Remove", "Remove the selected bodies (kept in the timeline)"),
      Cmd::Print3d => (Icon::Print3d, "3D Print", "Export the design as an STL mesh for printing"),
      Cmd::LookAt => (Icon::LookAt, "Look At", "Look at the sketch plane or a selected planar face"),
      Cmd::Fit => (Icon::Fit, "Fit", "Fit the design in the view (F6)"),
      Cmd::Home => (Icon::Views, "Home View", "Go to the home view (Home)"),
      Cmd::ToggleVisibility => (Icon::Eye, "Show/Hide", "Show or hide the selected bodies"),
      Cmd::DrawingView(k) => match k {
        "section" => (Icon::SectionView, "Section View", "A view cut across the part"),
        "detail" => (Icon::DetailView, "Detail View", "An enlarged circle of another view"),
        "flat" => (Icon::FlatPattern, "Flat Pattern View", "A sheet metal body's flat pattern"),
        "top" => (Icon::ProjectedView, "Projected View", "A view projected from the base view"),
        _ => (Icon::BaseView, "Base View", "A standard view of the design"),
      },
      Cmd::DrawingExport(e) => match e {
        "pdf" => (Icon::ExportPdf, "PDF Export", "Write the drawing as a PDF"),
        "dxf" => (Icon::ExportDxf, "DXF Export", "Write the drawing as a DXF"),
        _ => (Icon::ExportSvg, "SVG Export", "Write the drawing as an SVG"),
      },
      Cmd::FinishDrawing => (Icon::Finish, "Finish Drawing", "Back to the Design workspace"),
      Cmd::ComputeAll => (Icon::ComputeAll, "Compute All", "Recompute the whole timeline (Ctrl+B)"),
      Cmd::CamGenerate => (Icon::Play, "Generate", "Compute the setup's toolpaths and machining time"),
      Cmd::CamPost => (Icon::ExportDxf, "Post Process", "Write the setup's G-code with its post-processor"),
      Cmd::CamDelete => (Icon::Remove, "Delete Setup", "Delete the last setup and its operations"),
      Cmd::InsertComponent => {
        (Icon::InsertComponent, "Insert Component", "Insert a design from the data panel as a linked component")
      }
      Cmd::CamSimulate => {
        (Icon::MotionStudy, "Simulate", "Cut the stock with the toolpaths and check for gouges and collisions")
      }
      Cmd::CamTools => (Icon::Configure, "Tool Library", "The hub's cutting tools with their feeds and speeds"),
      Cmd::SelectAll => (Icon::Select, "Select All", "Select every body (every curve in a sketch)"),
      Cmd::SelectInvert => (Icon::Select, "Invert Selection", "Select what isn't selected"),
      Cmd::SelectOther => {
        (Icon::Select, "Select Other", "Everything under the last click, front to back (or Alt+click, or hold)")
      }
      Cmd::WindowSelect => (
        Icon::WindowSel,
        "Window Selection",
        "Drag a box: left to right selects what's inside, right to left what it touches",
      ),
      Cmd::Priority(p) => match p {
        crate::select::Priority::Body => (Icon::Body, "Select Body Priority", "Clicks and boxes select whole bodies"),
        crate::select::Priority::Face => (Icon::SplitFace, "Select Face Priority", "Clicks select faces (and edges)"),
        crate::select::Priority::Edge => (Icon::Line, "Select Edge Priority", "Clicks and boxes select edges only"),
        crate::select::Priority::Component => {
          (Icon::Component, "Select Component Priority", "Clicks select every body of a component")
        }
      },
      Cmd::Filter(f) => match f {
        "bodies" => (Icon::Body, "Bodies", "Allow selecting bodies"),
        "faces" => (Icon::SplitFace, "Faces", "Allow selecting faces"),
        "edges" => (Icon::Line, "Edges", "Allow selecting edges"),
        "sketches" => (Icon::SketchSmall, "Sketches", "Allow selecting sketch curves and points"),
        "vertices" => (Icon::Point, "Vertices", "Show body vertices and allow selecting them"),
        "construction" => (Icon::Plane, "Construction", "Allow selecting construction planes, axes and points"),
        _ => (Icon::Rect, "Sketch Profiles", "Allow selecting sketch profiles"),
      },
      Cmd::ConfigTable => (
        Icon::ConfigTable,
        "Configuration Table",
        "Named variants overriding parameters, suppression, appearances and joints",
      ),
      Cmd::CaptureHistory => (
        Icon::Box,
        "Capture Design History",
        "On: a parametric timeline. Off: the timeline becomes a base feature for direct edits",
      ),
      Cmd::SelectMode(crate::select::Mode::Paint) => {
        (Icon::PaintSel, "Paint Selection", "Drag over faces, edges or bodies to select everything the brush touches")
      }
      Cmd::SelectMode(_) => (
        Icon::FreeformSel,
        "Freeform Selection",
        "Drag a loop: setting off to the right selects what's inside, to the left what it touches",
      ),
      Cmd::Gen(w) => match w {
        "study" => (Icon::Plus, "New Generative Study", "A new generative design study"),
        "preserve" => (Icon::Fix, "Preserve Geometry", "Bodies the outcomes must keep (select them first)"),
        "obstacle" => (Icon::Remove, "Obstacle Geometry", "Bodies the outcomes must stay out of (select them first)"),
        "start" => (Icon::Body, "Starting Shape", "The body the optimisation starts from (select it first)"),
        "load" => (Icon::Move, "Structural Loads", "A force or pressure on the selected faces"),
        "constraint" => (Icon::Fix, "Structural Constraints", "Hold the selected faces fixed"),
        "precheck" => (Icon::Interference, "Pre-check", "Check the study is ready to generate"),
        "generate" => (Icon::Play, "Generate", "Generate the outcomes in the background"),
        "explore" => {
          (Icon::Views, "Explore", "The outcomes: mass, stiffness, safety factor; insert one into the design")
        }
        _ => (Icon::Gauge, "Study Settings", "Objectives, limits, materials and manufacturing methods"),
      },
      Cmd::Cut2d => (
        Icon::ExportDxf,
        "2D Cut",
        "Laser, plasma or waterjet G-code (or DXF) for a flat pattern or sketch: kerf, leads, micro-joints",
      ),
      Cmd::Canvas => {
        (Icon::Canvas, "Canvas", "Place an image on a plane to model over; right-click it to Calibrate its size")
      }
      Cmd::Plugins => (Icon::Scripts, "Scripts and Add-Ins", "Install, update and manage plugins (.foxplugin)"),
      Cmd::Properties => (
        Icon::Properties,
        "Properties",
        "A panel with the type, geometry and overall size of the selection (or of the whole design)",
      ),
      Cmd::SketchLibrary => {
        (Icon::SketchLibrary, "Sketch Library...", "Place DXF, SVG or .fox library parts where you click in the sketch")
      }
      Cmd::Render(r) => r.info(),
      Cmd::CutCheck => (
        Icon::Interference,
        "Cut Service Check",
        "Check a sketch, flat pattern or DXF for SendCutSend, OSH Cut, Ponoko or your shop, with a cut estimate",
      ),
    }
  }

  pub(crate) fn active(self, app: &App) -> bool {
    match self {
      Cmd::SketchTool(t) => app.tool == t,
      Cmd::Select => false,
      Cmd::Params => app.show_params,
      Cmd::Dialog(k) => app.dialog.as_ref().is_some_and(|d| d.kind == k),
      Cmd::Priority(p) => app.sel_priority == p,
      Cmd::Filter(f) => app.sel_filter.get(f),
      Cmd::WindowSelect => app.sel_mode == crate::select::Mode::Window,
      Cmd::SelectMode(m) => app.sel_mode == m,
      Cmd::Properties => crate::props_ui::is_open(),
      Cmd::Render(r) => r.active(app),
      _ => false,
    }
  }

  /// Commands that the marking menu's Repeat can run again.
  pub(crate) fn repeatable(self) -> bool {
    match self {
      Cmd::NewSketch | Cmd::Dialog(_) => true,
      Cmd::SketchTool(t) => t != Tool::Select,
      _ => false,
    }
  }

  pub(crate) fn run(self, app: &mut App) {
    // a new command ends the last one's hint
    app.end_hint();
    app.note_cmd(self);
    match self {
      Cmd::NewSketch => app.new_sketch(),
      Cmd::Form(a) => crate::form_ui::run(app, a),
      Cmd::Dialog(k) => app.start(k),
      Cmd::Params => app.show_params = !app.show_params,
      Cmd::InsertStep => app.import(),
      Cmd::Select => {
        app.nav_mode = None;
        app.set_tool(Tool::Select);
      }
      Cmd::SketchTool(t) => {
        app.nav_mode = None;
        app.set_tool(t);
      }
      Cmd::Construction => app.toggle_construction(),
      Cmd::Constraint(kind) => apply_constraint(app, kind),
      Cmd::FinishSketch => app.finish_sketch(),
      Cmd::Undo => {
        if app.s.can_undo() {
          app.cancel_dialog();
          app.s.undo();
        }
      }
      Cmd::Redo => {
        if app.s.can_redo() {
          app.s.redo();
        }
      }
      Cmd::Delete => app.delete_selection(),
      Cmd::Print3d => app.export("stl"),
      Cmd::RemoveBodies => {
        let bodies: Vec<u32> =
          app.s.selection.iter().filter_map(|x| if let Sel::Body { body } = x { Some(body.0) } else { None }).collect();
        if bodies.is_empty() {
          app.hint("Remove: select bodies first");
        } else {
          app.run("body.remove", json!({"bodies": bodies}));
          app.s.selection.clear();
        }
      }
      Cmd::LookAt => app.look_at_selection(),
      Cmd::DrawingView(k) => {
        app.start(Kind::DrawingView);
        if let Some(d) = app.dialog.as_mut() {
          d.set_choice(k);
        }
      }
      Cmd::DrawingExport(e) => app.export_drawing(e),
      Cmd::FinishDrawing => app.close_drawing(),
      Cmd::ComputeAll => app.run_background("compute_all", json!({})),
      Cmd::CamGenerate => app.cam_generate(true),
      Cmd::CamPost => app.cam_post(),
      Cmd::CamDelete => {
        if app.run("cam.delete", json!({})).is_some() {
          app.status = Some(("Setup deleted".into(), false));
        }
      }
      Cmd::InsertComponent => {
        if !app.data.open {
          app.data.toggle();
        }
        app.status =
          Some(("Right-click a design in the Data Panel and choose Insert into Current Design".into(), false));
      }
      Cmd::CamSimulate => app.cam_simulate(),
      Cmd::CamTools => app.open_tool_library(),
      Cmd::SelectAll => {
        app.run("select.all", json!({}));
      }
      Cmd::SelectInvert => {
        app.run("select.invert", json!({}));
      }
      Cmd::SelectOther => crate::viewport::select_other_at_last_click(app),
      Cmd::WindowSelect => {
        app.nav_mode = None;
        app.sel_mode = crate::select::Mode::Window;
        app.set_tool(Tool::Select);
        app.hint("Drag a box: left to right selects what's inside, right to left what it touches");
      }
      Cmd::Priority(p) => app.sel_priority = p,
      Cmd::Filter(f) => app.sel_filter.toggle(f),
      Cmd::Fit => app.fit_view(),
      Cmd::Home => app.go_home(),
      Cmd::ToggleVisibility => app.toggle_visibility(),
      Cmd::ConfigTable => crate::config_ui::toggle_table(),
      Cmd::CaptureHistory => crate::config_ui::toggle_history(app),
      Cmd::SelectMode(m) => {
        app.nav_mode = None;
        app.sel_mode = m;
        app.set_tool(Tool::Select);
        app.hint(Cmd::SelectMode(m).info().2);
      }
      Cmd::Cut2d => app.open_cut_window(),
      Cmd::Gen(w) => app.gen_command(w),
      Cmd::Canvas => crate::canvas_ui::start_insert(app),
      Cmd::Plugins => crate::plugins_ui::open_window(app),
      Cmd::Properties => crate::props_ui::toggle(),
      Cmd::SketchLibrary => crate::library_ui::open(app),
      Cmd::Render(r) => r.run(app),
      Cmd::CutCheck => crate::service_ui::open(app),
    }
  }
}

fn constraint_info(kind: &str) -> (Icon, &'static str) {
  match kind {
    "hv" => (Icon::HorizVert, "Horizontal/Vertical"),
    "horizontal" => (Icon::Horizontal, "Horizontal"),
    "vertical" => (Icon::Vertical, "Vertical"),
    "coincident" => (Icon::Coincident, "Coincident"),
    "tangent" => (Icon::Tangent, "Tangent"),
    "equal" => (Icon::Equal, "Equal"),
    "parallel" => (Icon::Parallel, "Parallel"),
    "perpendicular" => (Icon::Perpendicular, "Perpendicular"),
    "fix" => (Icon::Fix, "Fix/UnFix"),
    "midpoint" => (Icon::Midpoint, "Midpoint"),
    "concentric" => (Icon::Concentric, "Concentric"),
    "collinear" => (Icon::Collinear, "Collinear"),
    "curvature" => (Icon::Curvature, "Curvature"),
    "polygon" => (Icon::PolygonCon, "Polygon"),
    _ => (Icon::Symmetric, "Symmetry"),
  }
}

/// The group name with its drop-down chevron (clickable); light blue while its menu is open.
pub(crate) fn group_label(ui: &mut egui::Ui, label: &str, width: f32, open: bool) -> egui::Response {
  let font = egui::FontId::proportional(12.0);
  let tw = ui.painter().layout_no_wrap(label.to_string(), font.clone(), theme::GROUP_LABEL).size().x;
  let (rect, resp) = ui.allocate_exact_size(vec2(width.max(tw + 22.0), 18.0), egui::Sense::click());
  if open {
    ui.painter().rect_filled(rect, 0.0, theme::CONTEXT_TAB);
  } else if resp.hovered() {
    ui.painter().rect_filled(rect, 2.0, theme::HOVER);
  }
  let x0 = rect.center().x - (tw + 12.0) / 2.0;
  ui.painter().text(pos2(x0, rect.center().y), egui::Align2::LEFT_CENTER, label, font, theme::GROUP_LABEL);
  icons::chevron(ui.painter(), pos2(x0 + tw + 8.0, rect.center().y), theme::GROUP_LABEL);
  resp
}

pub(crate) fn separator(ui: &mut egui::Ui) {
  let (r, _) = ui.allocate_exact_size(vec2(7.0, 58.0), egui::Sense::hover());
  ui.painter().line_segment([r.center_top() + vec2(0.0, 4.0), r.center_bottom()], Stroke::new(1.0, theme::SEPARATOR));
}

pub fn toolbar(app: &mut App, ui: &mut egui::Ui) {
  let sketching = app.s.edit_sketch.is_some();
  if !sketching && app.tab == Tab::Sketch {
    app.tab = Tab::Solid;
  }
  if !app.form.active && app.tab == Tab::Form {
    app.tab = Tab::Solid;
  }
  egui::Panel::top("toolbar")
    .exact_size(96.0)
    .frame(egui::Frame::NONE.fill(theme::TOOLBAR).inner_margin(egui::Margin { left: 6, right: 6, top: 0, bottom: 0 }))
    .show(ui, |ui| {
      let full = ui.max_rect();
      ui.painter().line_segment([full.left_bottom(), full.right_bottom()], Stroke::new(1.0, theme::BORDER));
      ui.horizontal(|ui| {
        workspace_switcher(app, ui);
        ui.add_space(6.0);
        ui.vertical(|ui| {
          ui.spacing_mut().item_spacing.y = 0.0;
          if app.electronics.is_some() {
            crate::ecad_ui::toolbar(app, ui);
            return;
          }
          if app.simulation.is_some() {
            crate::sim_ui::toolbar(app, ui);
            return;
          }
          tab_row(app, ui, sketching);
          let width = ui.available_width();
          egui::ScrollArea::horizontal()
            .id_salt("tools")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
              ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let tab = app.tab;
                match tab {
                  Tab::Plugin(i) => crate::plugins_ui::toolbar(app, ui, i),
                  other if crate::menus::tab_toolbar(app, ui, other, width) => {}
                  other => {
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                      ui.add_space(18.0);
                      ui.label(
                        RichText::new(format!("{} tools aren't in FissionOxide yet", other.label()))
                          .color(theme::MUTED),
                      );
                    });
                  }
                }
              });
            });
        });
      });
    });
}

fn workspace_switcher(app: &mut App, ui: &mut egui::Ui) {
  ui.vertical(|ui| {
    ui.add_space(9.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(92.0, 76.0), egui::Sense::click());
    let fill = if resp.hovered() { theme::HOVER } else { Color32::from_rgb(248, 248, 248) };
    ui.painter().rect(rect, 4.0, fill, Stroke::new(1.0, Color32::from_gray(196)), egui::StrokeKind::Inside);
    ui.painter().text(
      rect.center() - vec2(8.0, 0.0),
      egui::Align2::CENTER_CENTER,
      if app.electronics.is_some() {
        "ELECTRONICS"
      } else if app.drawing.is_some() {
        "DRAWING"
      } else if app.generative {
        "GENERATIVE"
      } else if app.render.active {
        "RENDER"
      } else if app.manufacture {
        "MANUFACTURE"
      } else if app.simulation.is_some() {
        "SIMULATION"
      } else {
        "DESIGN"
      },
      egui::FontId::proportional(
        if app.manufacture || app.electronics.is_some() || app.generative || app.simulation.is_some() {
          10.0
        } else if app.drawing.is_some() {
          12.5
        } else {
          13.5
        },
      ),
      Color32::from_gray(30),
    );
    icons::chevron(ui.painter(), rect.center() + vec2(32.0, 0.0), Color32::from_gray(30));
    let resp = resp.on_hover_text("Change Workspace");
    egui::Popup::menu(&resp).show(|ui| workspace_menu(app, ui));
  });
}

/// Fusion's workspace list: upper-case names, a padlock on the workspaces that need an
/// extension, and DRAWING ▸ and ELECTRONICS ▸ submenus for the documents they create.
fn workspace_menu(app: &mut App, ui: &mut egui::Ui) {
  const WIDTH: f32 = 200.0;
  ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
  ui.set_min_width(WIDTH);
  let drawing = app.drawing.is_some();
  let ecad = app.electronics.is_some();
  let plain = !drawing && !ecad;
  for w in WORKSPACES {
    let current = match w {
      "Drawing" => drawing && !ecad,
      "Manufacture" => app.manufacture && plain,
      "Electronics" => ecad,
      "Generative Design" => app.generative && plain,
      "Render" => app.render.active && plain,
      "Simulation" => app.simulation.is_some() && plain,
      "Animation" => false,
      _ => plain && !app.manufacture && !app.generative && !app.render.active && app.simulation.is_none(),
    };
    let lock = matches!(w, "Generative Design" | "Simulation");
    let sub: &[&str] = match w {
      "Drawing" => &["From Design", "From Animation"],
      "Electronics" => &["New Electronics Design", "New Schematic", "New PCB", "New Electronics Library", "New 3D PCB"],
      _ => &[],
    };
    let r = workspace_row(ui, WIDTH, &w.to_uppercase(), current, lock, !sub.is_empty());
    if sub.is_empty() {
      if r.clicked() {
        ui.close();
        if w == "Animation" {
          app.status = Some(("Animation isn't in FissionOxide yet".into(), false));
        } else {
          switch_workspace(app, w);
        }
      }
      continue;
    }
    egui::containers::menu::SubMenu::new().show(ui, &r, |ui| {
      ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
      ui.set_min_width(WIDTH);
      for item in sub {
        if workspace_row(ui, WIDTH, item, false, false, false).clicked() {
          ui.close();
          open_workspace_document(app, w, item);
        }
      }
    });
  }
}

/// One row of the workspace menu (or of its submenus): the name, a padlock, or a ▸ arrow.
fn workspace_row(ui: &mut egui::Ui, width: f32, label: &str, current: bool, lock: bool, sub: bool) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(width, 24.0), egui::Sense::click());
  let p = ui.painter();
  if current {
    p.rect_filled(rect, 0.0, theme::SELECTED);
  } else if resp.hovered() {
    p.rect_filled(rect, 0.0, theme::HOVER);
  }
  p.text(
    rect.left_center() + vec2(10.0, 0.0),
    egui::Align2::LEFT_CENTER,
    label,
    egui::FontId::proportional(12.5),
    theme::TEXT,
  );
  let ink = Color32::from_gray(80);
  if lock {
    // a padlock: the shackle over the body
    let c = rect.right_center() - vec2(16.0, 1.0);
    p.rect_filled(Rect::from_min_max(c + vec2(-5.0, -1.0), c + vec2(5.0, 6.0)), 1.0, ink);
    let arc: Vec<Pos2> = (0..=8)
      .map(|i| {
        let a = std::f32::consts::PI * (1.0 + i as f32 / 8.0);
        c + vec2(3.5 * a.cos(), 3.5 * a.sin())
      })
      .collect();
    p.add(egui::Shape::line(arc, Stroke::new(1.5, ink)));
    p.line_segment([c + vec2(-3.5, -1.0), c + vec2(-3.5, -3.0)], Stroke::new(1.5, ink));
    p.line_segment([c + vec2(3.5, -1.0), c + vec2(3.5, -3.0)], Stroke::new(1.5, ink));
  }
  if sub {
    let c = rect.right_center() - vec2(12.0, 0.0);
    p.add(egui::Shape::convex_polygon(
      vec![c + vec2(-3.0, -5.0), c + vec2(4.0, 0.0), c + vec2(-3.0, 5.0)],
      ink,
      Stroke::NONE,
    ));
  }
  resp
}

/// A DRAWING ▸ or ELECTRONICS ▸ submenu entry: opens that workspace on the document it names.
pub(crate) fn open_workspace_document(app: &mut App, workspace: &str, item: &str) {
  match (workspace, item) {
    ("Drawing", "From Design") => switch_workspace(app, "Drawing"),
    ("Electronics", "New Electronics Design" | "New Schematic" | "New PCB") => {
      switch_workspace(app, "Electronics");
      if let Some(e) = app.electronics.as_mut() {
        e.sheet = if item == "New PCB" { crate::ecad_ui::Sheet::Board } else { crate::ecad_ui::Sheet::Schematic };
      }
    }
    _ => app.status = Some((format!("{item} isn't in FissionOxide yet"), false)),
  }
}

/// Switches to the Design, Render, Manufacture or Drawing workspace.
pub(crate) fn switch_workspace(app: &mut App, w: &str) {
  let drawing = app.drawing.is_some();
  app.render.active = w == "Render";
  if w == "Electronics" {
    if drawing {
      app.close_drawing();
    }
    app.manufacture = false;
    app.generative = false;
    app.simulation = None;
    app.open_electronics();
    return;
  }
  app.electronics = None;
  if w == "Simulation" {
    if drawing {
      app.close_drawing();
    }
    app.manufacture = false;
    app.generative = false;
    app.open_simulation();
    return;
  }
  app.simulation = None;
  match w {
    "Render" => {
      app.manufacture = false;
      app.generative = false;
      if drawing {
        app.close_drawing();
      }
      app.cancel_dialog();
      if app.s.edit_sketch.is_some() {
        app.finish_sketch();
      }
    }
    "Drawing" if !drawing => {
      app.manufacture = false;
      app.generative = false;
      app.open_drawing_workspace();
    }
    "Design" => {
      app.manufacture = false;
      app.generative = false;
      if drawing {
        app.close_drawing();
      }
    }
    "Manufacture" => {
      if drawing {
        app.close_drawing();
      }
      app.cancel_dialog();
      if app.s.edit_sketch.is_some() {
        app.finish_sketch();
      }
      app.generative = false;
      app.manufacture = true;
    }
    "Generative Design" => {
      if drawing {
        app.close_drawing();
      }
      app.cancel_dialog();
      if app.s.edit_sketch.is_some() {
        app.finish_sketch();
      }
      app.manufacture = false;
      app.generative = true;
      app.gd.open = true;
    }
    _ => {}
  }
}

fn tab_row(app: &mut App, ui: &mut egui::Ui, sketching: bool) {
  ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing.x = 4.0;
    let mut tabs: Vec<Tab> = Tab::MAIN.to_vec();
    if app.drawing.is_some() {
      tabs = vec![Tab::Drawing];
      app.tab = Tab::Drawing;
    } else if app.render.active {
      tabs = vec![Tab::Render];
      app.tab = Tab::Render;
    } else if app.generative {
      tabs = vec![Tab::Generative];
      app.tab = Tab::Generative;
    } else if app.manufacture {
      tabs = vec![Tab::Milling, Tab::Turning];
      if !matches!(app.tab, Tab::Milling | Tab::Turning) {
        app.tab = Tab::Milling;
      }
    } else if matches!(app.tab, Tab::Drawing | Tab::Milling | Tab::Turning | Tab::Generative | Tab::Render) {
      app.tab = Tab::Solid;
    }
    if app.drawing.is_none() && !app.manufacture && !app.render.active {
      tabs.extend((0..app.plugins.tabs.len().min(usize::from(u8::MAX))).map(|i| Tab::Plugin(i as u8)));
    }
    if let Tab::Plugin(i) = app.tab
      && usize::from(i) >= app.plugins.tabs.len()
    {
      app.tab = Tab::Solid;
    }
    if sketching {
      tabs.push(Tab::Sketch);
    }
    if app.form.active {
      tabs.push(Tab::Form);
    }
    for t in tabs {
      let font = egui::FontId::proportional(12.5);
      let active = app.tab == t;
      // Fusion: the active tab (the contextual SKETCH tab too) is bold black with a black
      // underline; the others plain dark grey
      let color = if active { theme::TAB_ACTIVE } else { Color32::from_gray(45) };
      let label = match t {
        Tab::Plugin(i) => crate::plugins_ui::tab_label(app, i),
        _ => t.label().to_string(),
      };
      let tw = ui.painter().layout_no_wrap(label.clone(), font.clone(), color).size().x;
      let (rect, resp) = ui.allocate_exact_size(vec2(tw + 26.0, 28.0), egui::Sense::click());
      let pill = rect.shrink2(vec2(0.0, 3.0));
      if resp.hovered() && !active {
        ui.painter().rect_filled(pill, 2.0, theme::HOVER);
      }
      ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label.clone(), font.clone(), color);
      if active {
        // the font has no bold face: a second pass half a pixel over gives the weight
        ui.painter().text(rect.center() + vec2(0.6, 0.0), egui::Align2::CENTER_CENTER, label, font, color);
        ui.painter().line_segment(
          [
            pos2(rect.center().x - tw / 2.0 - 4.0, rect.bottom() - 2.0),
            pos2(rect.center().x + tw / 2.0 + 4.0, rect.bottom() - 2.0),
          ],
          Stroke::new(3.0, theme::TAB_ACTIVE),
        );
      }
      if resp.clicked() {
        app.tab = t;
      }
    }
  });
}

// ---------------------------------------------------------------- canvas overlays

/// A floating panel's header: collapse arrow and title.
/// The header bar of the browser and comments panels: ◂◂ collapse arrows, title, and the
/// options dot and pin bar on the right.
pub(crate) fn float_header(ui: &mut egui::Ui, title: &str, open: &mut bool, width: f32) {
  let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), egui::Sense::click());
  let p = ui.painter();
  p.rect(rect, 2.0, theme::FLOAT_HEADER, Stroke::new(1.0, Color32::from_gray(212)), egui::StrokeKind::Inside);
  let c = rect.left_center() + vec2(12.0, 0.0);
  let ink = theme::GROUP_LABEL;
  for dx in [-3.0, 3.0] {
    let t = c + vec2(dx, 0.0);
    let pts = if *open {
      vec![t + vec2(-3.0, 0.0), t + vec2(2.0, -4.0), t + vec2(2.0, 4.0)]
    } else {
      vec![t + vec2(3.0, 0.0), t + vec2(-2.0, -4.0), t + vec2(-2.0, 4.0)]
    };
    p.add(egui::Shape::convex_polygon(pts, ink, Stroke::NONE));
  }
  p.text(rect.left_center() + vec2(28.0, 0.0), egui::Align2::LEFT_CENTER, title, egui::FontId::proportional(12.0), ink);
  let dot = rect.right_center() - vec2(24.0, 0.0);
  p.circle_filled(dot, 5.0, ink);
  p.circle_filled(dot, 1.5, Color32::WHITE);
  p.line_segment(
    [rect.right_center() + vec2(-9.0, -6.0), rect.right_center() + vec2(-9.0, 6.0)],
    Stroke::new(1.0, ink),
  );
  if resp.clicked() {
    *open = !*open;
  }
}

/// Tree expander: ▸ closed, ▾ open.
fn triangle(p: &egui::Painter, c: Pos2, open: bool, color: Color32) {
  let pts = if open {
    vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
  } else {
    vec![c + vec2(-2.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.0, 4.0)]
  };
  p.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// Browser expander: a hollow ▷ when closed, a hollow ▽ when open, and the filled ◢ of the
/// root component.
fn tree_triangle(p: &egui::Painter, c: Pos2, open: bool, root: bool, color: Color32) {
  let s = Stroke::new(1.0, color);
  if root {
    p.add(egui::Shape::convex_polygon(
      vec![c + vec2(4.0, -4.0), c + vec2(4.0, 4.0), c + vec2(-4.0, 4.0)],
      color,
      Stroke::NONE,
    ));
  } else if open {
    p.add(egui::Shape::closed_line(vec![c + vec2(-5.0, -3.0), c + vec2(5.0, -3.0), c + vec2(0.0, 4.0)], s));
  } else {
    p.add(egui::Shape::closed_line(vec![c + vec2(-3.0, -5.0), c + vec2(4.0, 0.0), c + vec2(-3.0, 5.0)], s));
  }
}

/// What happened to a browser row.
#[derive(Default)]
struct RowEvents {
  clicked: bool,
  double_clicked: bool,
  eye: bool,
  response: Option<egui::Response>,
}

/// One browser row, drawn straight on the canvas as in Fusion: expander, visibility eye, icon
/// and the name on a grey pill (a dark pill with an activation radio for the root component).
fn tree_row(
  ui: &mut egui::Ui,
  id: egui::Id,
  depth: usize,
  // Some(default open state) for rows with children
  expand: Option<bool>,
  eye: Option<bool>,
  icon: Icon,
  label: &str,
  selected: bool,
  label_color: Color32,
) -> RowEvents {
  let width = ui.available_width();
  // dragged rows move bodies and components (see browser_edit)
  let (rect, resp) = ui.allocate_exact_size(vec2(width, 25.0), egui::Sense::click_and_drag());
  let open_id = id.with("open");
  let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).or(expand).unwrap_or(false);
  let p = ui.painter().clone();
  let root = depth == 0;
  let x = rect.min.x + 8.0 + depth as f32 * 16.0;
  let cy = rect.center().y;
  let mut ev = RowEvents::default();
  if expand.is_some() {
    let tr = Rect::from_center_size(pos2(x + 4.0, cy), Vec2::splat(14.0));
    let r = ui.interact(tr, id.with("tri"), egui::Sense::click());
    tree_triangle(&p, tr.center(), open, root, if r.hovered() { theme::ACCENT } else { theme::GROUP_LABEL });
    if r.clicked() {
      open = !open;
      ui.data_mut(|d| d.insert_temp(open_id, open));
    }
  }
  if let Some(visible) = eye {
    let er = Rect::from_center_size(pos2(x + 24.0, cy), vec2(18.0, 14.0));
    let r = ui.interact(er, id.with("eye"), egui::Sense::click());
    icons::draw_color(&p, er, if visible { Icon::Eye } else { Icon::EyeOff });
    if r.on_hover_text(if visible { "Hide" } else { "Show" }).clicked() {
      ev.eye = true;
    }
  }
  let font = egui::FontId::proportional(13.0);
  let text_color = if root { Color32::WHITE } else { label_color };
  let galley = p.layout_no_wrap(label.to_string(), font, text_color);
  let icon_x = x + 44.0;
  let pill = Rect::from_min_size(pos2(icon_x - 11.0, cy - 10.5), vec2(galley.size().x + 36.0, 21.0));
  let (fill, stroke) = if selected {
    (theme::SELECTED, Stroke::new(1.0, theme::ACCENT))
  } else if resp.hovered() {
    (if root { Color32::from_rgb(72, 86, 106) } else { theme::HOVER }, Stroke::new(1.0, theme::SELECTED))
  } else {
    (if root { theme::PILL_ROOT } else { theme::PILL }, Stroke::NONE)
  };
  p.rect(pill, 2.0, fill, stroke, egui::StrokeKind::Inside);
  icons::draw_color(&p, Rect::from_center_size(pos2(icon_x, cy), Vec2::splat(16.0)), icon);
  let text_color = if selected && root { theme::TEXT } else { text_color };
  p.galley_with_override_text_color(pos2(icon_x + 13.0, cy - galley.size().y / 2.0), galley, text_color);
  if root {
    // the activation radio of the root component
    let c = pos2(pill.right() + 12.0, cy);
    p.circle(c, 7.0, Color32::WHITE, Stroke::new(1.0, theme::GROUP_LABEL));
    p.circle_filled(c, 3.5, theme::GROUP_LABEL);
  }
  if resp.double_clicked() {
    ev.double_clicked = true;
  } else if resp.clicked() {
    ev.clicked = true;
  }
  ev.response = Some(resp);
  ev
}

fn is_open(ui: &egui::Ui, id: egui::Id, default: bool) -> bool {
  ui.data(|d| d.get_temp::<bool>(id.with("open"))).unwrap_or(default)
}

pub fn browser(app: &mut App, ctx: &egui::Context) {
  let canvas = app.canvas;
  egui::Area::new(egui::Id::new("browser")).fixed_pos(canvas.min + vec2(6.0, 6.0)).order(egui::Order::Middle).show(
    ctx,
    |ui| {
      let width = 270.0;
      ui.set_width(width);
      // an Area's room is last frame's size; give the tree room to grow when rows are added
      ui.set_max_height(canvas.height().max(150.0));
      ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
      float_header(ui, "BROWSER", &mut app.browser_open, width);
      if !app.browser_open {
        return;
      }
      // the rows sit directly on the canvas, without a panel behind them
      let max_h = (canvas.height() - 90.0).max(120.0);
      egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
        ui.add_space(4.0);
        browser_tree(app, ui);
        app.ctx_menu.reveal = None;
        ui.add_space(4.0);
      });
    },
  );
}

fn browser_tree(app: &mut App, ui: &mut egui::Ui) {
  let root = egui::Id::new("tree-root");
  // Find in Browser: open the folders down to the node (its row scrolls to itself)
  if let Some(sel) = app.ctx_menu.reveal {
    let folder = match sel {
      Sel::Body { .. } => Some("tree-bodies"),
      Sel::Feature { feature } => app.s.design.feature(feature).and_then(|f| match f.kind {
        FeatureKind::Sketch { .. } => Some("tree-sketches"),
        FeatureKind::ConstructionPlane { .. } => Some("tree-construction"),
        FeatureKind::Joint { .. } => Some("tree-joints"),
        FeatureKind::Canvas { .. } => Some("tree-canvases"),
        _ => None,
      }),
      _ => None,
    };
    ui.data_mut(|d| {
      d.insert_temp(root.with("open"), true);
      if let Some(f) = folder {
        d.insert_temp(egui::Id::new(f).with("open"), true);
      }
    });
  }
  let name = app
    .s
    .path
    .as_ref()
    .and_then(|p| p.file_stem())
    .map(|n| n.to_string_lossy().to_string())
    .unwrap_or_else(|| app.s.design.name.clone());
  tree_row(ui, root, 0, Some(true), None, Icon::Component, &format!("{name} v1"), false, theme::TEXT);
  if !is_open(ui, root, true) {
    return;
  }
  // Document Settings
  let ds = egui::Id::new("tree-docsettings");
  tree_row(ui, ds, 1, Some(false), None, Icon::Gear, "Document Settings", false, theme::TEXT);
  if is_open(ui, ds, false) {
    let label = format!("Units: {}", app.s.design.units.symbol());
    let ev = tree_row(ui, ds.with(1), 2, None, None, Icon::Measure, &label, false, theme::TEXT);
    if let Some(r) = &ev.response {
      crate::units::settings_row(app, ui, r, 2);
    }
  }
  // Named Views
  let nv = egui::Id::new("tree-views");
  let ev = tree_row(ui, nv, 1, Some(false), None, Icon::Folder, "Named Views", false, theme::TEXT);
  if let Some(r) = ev.response {
    r.context_menu(|ui| {
      if menu_item(ui, Icon::Views, "New Named View", None).clicked() {
        ui.close();
        if app.save_named_view().is_some() {
          ui.data_mut(|d| d.insert_temp(nv.with("open"), true));
        }
      }
    });
  }
  if is_open(ui, nv, false) {
    for (label, v) in [
      ("TOP", StandardView::Top),
      ("FRONT", StandardView::Front),
      ("RIGHT", StandardView::Right),
      ("HOME", StandardView::Home),
    ] {
      let ev = tree_row(ui, nv.with(label), 2, None, None, Icon::Views, label, false, theme::TEXT);
      if ev.clicked || ev.double_clicked {
        app.fly_standard_view(v);
      }
    }
    // the user's views, saved in the design
    let names: Vec<String> = app.s.design.named_views.iter().map(|v| v.name.clone()).collect();
    for name in names {
      let ev = tree_row(ui, nv.with(("user", &name)), 2, None, None, Icon::Views, &name, false, theme::TEXT);
      if ev.double_clicked {
        app.restore_named_view(&name);
      }
      if let Some(r) = ev.response {
        r.on_hover_text("Double-click to restore this view").context_menu(|ui| named_view_menu(app, ui, &name));
      }
    }
  }
  // Origin
  let og = egui::Id::new("tree-origin");
  // a plane picked here stays picked until something else is selected
  if !app.s.selection.is_empty() {
    app.picked_plane = None;
  }
  tree_row(ui, og, 1, Some(false), None, Icon::Folder, "Origin", false, theme::TEXT);
  if is_open(ui, og, false) {
    tree_row(ui, og.with("o"), 2, None, None, Icon::Origin, "O", false, theme::TEXT);
    for (label, plane) in [("XY", "xy"), ("XZ", "xz"), ("YZ", "yz")] {
      let picked = app.picked_plane.as_ref().is_some_and(|p| p == plane);
      let ev = tree_row(ui, og.with(label), 2, None, None, Icon::Plane, label, picked, theme::TEXT);
      if ev.double_clicked {
        // the first click already started it if Create Sketch was waiting for a plane
        if app.s.edit_sketch.is_none() {
          app.create_sketch(json!(plane));
        }
      } else if ev.clicked {
        pick_plane(app, json!(plane));
      }
      if let Some(r) = ev.response {
        r.on_hover_text("Double-click to sketch on this plane").context_menu(|ui| {
          let items = origin_plane_menu(app, plane);
          if let Some(cmd) = row_menu(ui, &items) {
            run_origin_plane_cmd(app, plane, cmd);
          }
        });
      }
    }
  }
  components_section(app, ui);
  bodies_section(app, ui);
  browser_canvas::section(app, ui);
  sketches_section(app, ui);
  planes_section(app, ui);
  joints_section(app, ui);
  browser_assembly::sections(app, ui);
  drawings_section(app, ui);
  browser_config::section(app, ui);
}

/// The tag on an isolated body's browser row.
fn isolate_badge(ui: &egui::Ui, row: Rect) {
  let p = ui.painter();
  let galley = p.layout_no_wrap("ISOLATED".into(), egui::FontId::proportional(9.5), Color32::WHITE);
  let size = galley.size() + vec2(10.0, 4.0);
  let r = Rect::from_min_size(pos2(row.right() - size.x - 6.0, row.center().y - size.y / 2.0), size);
  p.rect_filled(r, 3.0, theme::ACCENT);
  p.galley(r.min + vec2(5.0, 2.0), galley, Color32::WHITE);
}

/// While bodies are isolated: a banner at the top of the canvas saying so, with Unisolate (as
/// Fusion shows isolate mode).
pub fn isolate_banner(app: &mut App, ctx: &egui::Context) {
  let Some(only) = &app.ctx_menu.isolate else { return };
  let label = match only.as_slice() {
    [b] => format!("Isolated: {}", app.s.body_name(*b)),
    v => format!("Isolated: {} bodies", v.len()),
  };
  let mut unisolate = false;
  egui::Area::new(egui::Id::new("isolate-banner"))
    .fixed_pos(pos2(app.canvas.center().x, app.canvas.min.y + 8.0))
    .pivot(egui::Align2::CENTER_TOP)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0, theme::ACCENT))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
          ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(10.0, 0.0);
            ui.label(RichText::new(label).color(theme::TEXT).strong());
            if ui.button("Unisolate").on_hover_text("Show every body again").clicked() {
              unisolate = true;
            }
          });
        });
    });
  if unisolate {
    app.ctx_menu.isolate = None;
  }
}

/// A user named view's right-click menu: restore, update to the current camera, rename, delete.
fn named_view_menu(app: &mut App, ui: &mut egui::Ui, name: &str) {
  if menu_item(ui, Icon::Views, "Restore", None).clicked() {
    ui.close();
    app.restore_named_view(name);
  }
  if menu_item(ui, Icon::Views, "Update to Current View", None).clicked() {
    ui.close();
    let c = app.cam;
    app.run(
      "view.named.save",
      json!({"name": name, "target": c.target, "yaw": c.yaw, "pitch": c.pitch, "distance": c.distance, "fov": c.fov, "ortho": c.ortho}),
    );
  }
  ui.separator();
  // rename in place: type the new name, Enter
  let key = egui::Id::new(("rename-named-view", name));
  let mut text = ui.data(|d| d.get_temp::<String>(key)).unwrap_or_else(|| name.to_string());
  ui.horizontal(|ui| {
    ui.label("Rename");
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(110.0));
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && text.trim() != name {
      app.run("view.named.rename", json!({"name": name, "to": text.trim()}));
      ui.close();
    }
  });
  ui.data_mut(|d| d.insert_temp(key, text));
  ui.separator();
  if menu_item(ui, Icon::Remove, "Delete", None).clicked() {
    ui.close();
    app.run("view.named.delete", json!({"name": name}));
  }
}

/// Components with their bodies; right-click to rename, ground, duplicate or delete.
fn components_section(app: &mut App, ui: &mut egui::Ui) {
  // sub-components follow their parent, one level deeper
  let comps: Vec<(u32, String, bool, Vec<BodyId>, usize)> = browser_assembly::component_tree(app)
    .into_iter()
    .filter_map(|(id, depth)| app.s.design.component(fox_engine::model::ComponentId(id)).map(|c| (id, c, depth)))
    .map(|(id, c, depth)| (id, c.name.clone(), c.grounded, c.bodies.clone(), depth))
    .collect();
  if comps.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-components");
  let ev = tree_row(ui, id, 1, Some(true), None, Icon::Folder, "Components", false, theme::TEXT);
  browser_edit::drop_target(app, ui, &ev, browser_edit::DropOn::TopLevel);
  if !is_open(ui, id, true) {
    return;
  }
  let mut closed_at: Option<usize> = None;
  for (cid, name, grounded, bodies, depth) in comps {
    if closed_at.is_some_and(|d| depth > d) {
      continue;
    }
    closed_at = None;
    let flexible = browser_assembly::flexible_of(app, cid);
    let label = if grounded { format!("{name}  (grounded)") } else { name.clone() };
    let label = if app.s.active_component.is_some_and(|a| a.0 == cid) { format!("{label}  (active)") } else { label };
    let label = match flexible {
      Some(true) => format!("{label}  (flexible)"),
      _ => label,
    };
    let selected = !bodies.is_empty() && bodies.iter().all(|b| app.s.selection.contains(&Sel::Body { body: *b }));
    let row = id.with(cid);
    let ev = tree_row(ui, row, 2 + depth, Some(false), None, Icon::Component, &label, selected, theme::TEXT);
    let target = RenameTarget::Component(cid);
    browser_edit::slow_click(ui, &ev, row, target, &name, selected);
    if ev.double_clicked {
      browser_edit::begin(ui, row, target, &name);
    } else if ev.clicked {
      app.s.selection = bodies.iter().map(|b| Sel::Body { body: *b }).collect();
    }
    let now = !bodies.is_empty() && bodies.iter().all(|b| app.s.selection.contains(&Sel::Body { body: *b }));
    browser_edit::slow_click_due(ui, row, now);
    browser_edit::drag_source(ui, &ev, browser_edit::Dragged::Component(cid), &name);
    browser_edit::drop_target(app, ui, &ev, browser_edit::DropOn::Component(cid));
    browser_edit::edit_box(app, ui, &ev, row, 2 + depth);
    if let Some(r) = ev.response {
      if r.hovered() {
        marking::peek(app, bodies.clone(), name.clone());
      }
      r.context_menu(|ui| {
        component_menu(app, ui, cid, &name);
        ui.separator();
        if ui.button("Rename").clicked() {
          ui.close();
          browser_edit::begin(ui, row, target, &name);
        }
        if ui.button(if grounded { "Unground" } else { "Ground" }).clicked() {
          ui.close();
          app.run("component.ground", json!({"component": cid, "grounded": !grounded}));
        }
        if ui.button("Duplicate").clicked() {
          ui.close();
          app.s.selection = bodies.iter().map(|b| Sel::Body { body: *b }).collect();
          app.start(Kind::CopyComponent);
        }
        if ui.button("Physical Material").clicked() {
          ui.close();
          app.s.selection = bodies.iter().map(|b| Sel::Body { body: *b }).collect();
          app.start(Kind::Material);
        }
        if let Some(flex) = flexible {
          let (text, tip) = if flex {
            ("Make Rigid", "Keep the sub-assembly's joints where its source design put them")
          } else {
            ("Make Flexible", "Solve the sub-assembly's own joints here so they can be driven")
          };
          if ui.button(text).on_hover_text(tip).clicked() {
            ui.close();
            app.run("component.flexible", json!({"component": cid, "flexible": !flex}));
          }
        }
        ui.separator();
        if ui.button("Delete Component").on_hover_text("Its bodies go back to the root").clicked() {
          ui.close();
          app.run("component.delete", json!({"component": cid}));
        }
      });
    }
    if !is_open(ui, row, false) {
      closed_at = Some(depth);
    } else {
      for b in bodies {
        let name = app.s.body_name(b);
        let sel = app.s.selection.contains(&Sel::Body { body: b });
        let brow = row.with(b.0);
        let ev = tree_row(ui, brow, 3 + depth, None, None, Icon::Body, &name, sel, theme::TEXT);
        body_row_edits(app, ui, &ev, brow, 3 + depth, b, &name, sel);
        if ev.clicked {
          toggle_sel(&mut app.s, Sel::Body { body: b }, ui.input(|i| i.modifiers.shift || i.modifiers.command));
        }
        browser_edit::slow_click_due(ui, brow, app.s.selection.contains(&Sel::Body { body: b }));
        browser_edit::drop_target(app, ui, &ev, browser_edit::DropOn::Component(cid));
      }
    }
  }
}

/// The Fusion part of a browser component's right-click menu: Activate, New Component,
/// Show/Hide, Isolate, Save Copy As, Properties and Find in Window. (Fusion's Open, Insert and
/// Create Selection Set need engine work FissionOxide doesn't have yet.)
fn component_menu(app: &mut App, ui: &mut egui::Ui, cid: u32, name: &str) {
  let active = app.s.active_component.is_some_and(|a| a.0 == cid);
  let (text, param) =
    if active { ("Activate Top Level", json!({})) } else { ("Activate Component", json!({"component": cid})) };
  if menu_item(ui, Icon::Component, text, None).on_hover_text("New bodies go into the active component").clicked() {
    ui.close();
    app.run("component.activate", param);
  }
  if menu_item(ui, Icon::Component, "New Component", None).on_hover_text("An empty component inside this one").clicked()
  {
    ui.close();
    app.s.selection.clear();
    app.start(Kind::NewComponent);
    if let Some(d) = app.dialog.as_mut() {
      d.for_component(&app.s, cid);
    }
  }
  ui.separator();
  let bodies = component_bodies(app, cid);
  let visible = bodies.iter().any(|b| app.s.body_visible(*b));
  if menu_item(ui, if visible { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", Some("V")).clicked() {
    ui.close();
    marking::show_hide(app, &bodies);
  }
  let isolated = app.ctx_menu.isolate.is_some();
  if menu_item(ui, Icon::Body, if isolated { "Unisolate" } else { "Isolate" }, None).clicked() {
    ui.close();
    app.ctx_menu.isolate = if isolated { None } else { Some(bodies.clone()) };
  }
  if menu_item(ui, Icon::Fit, "Find in Window", None).clicked() {
    ui.close();
    marking::find_in_window(app, &bodies, None);
  }
  ui.separator();
  if menu_item(ui, Icon::Save, "Save Copy As", None)
    .on_hover_text("Save the component's bodies to a STEP, IGES or BRep file")
    .clicked()
  {
    ui.close();
    save_component_copy(app, name, &bodies);
  }
  if menu_item(ui, Icon::Measure, "Properties", None).clicked() {
    ui.close();
    app.start(Kind::ComponentProps);
    if let Some(d) = app.dialog.as_mut() {
      d.for_component(&app.s, cid);
    }
  }
}

/// A component's bodies and those of every component below it.
fn component_bodies(app: &App, cid: u32) -> Vec<BodyId> {
  let d = &app.s.design;
  let mut comps = vec![fox_engine::model::ComponentId(cid)];
  let mut k = 0;
  while k < comps.len() && comps.len() < 4096 {
    let p = comps[k];
    comps.extend(
      d.components.iter().filter(|c| c.parent == Some(p) && !comps.contains(&c.id)).map(|c| c.id).collect::<Vec<_>>(),
    );
    k += 1;
  }
  comps.iter().filter_map(|c| d.component(*c)).flat_map(|c| c.bodies.iter().copied()).collect()
}

/// Save Copy As: the component's bodies to an exact (STEP, IGES or BRep) file.
fn save_component_copy(app: &mut App, name: &str, bodies: &[BodyId]) {
  if bodies.is_empty() {
    app.status = Some((format!("{name} has no bodies to save"), true));
    return;
  }
  let Some(p) = rfd::FileDialog::new()
    .add_filter("STEP", &["step", "stp"])
    .add_filter("IGES", &["iges", "igs"])
    .add_filter("BRep", &["brep"])
    .set_file_name(format!("{name}.step"))
    .save_file()
  else {
    return;
  };
  let ids: Vec<u32> = bodies.iter().map(|b| b.0).collect();
  if app.run("export", json!({"path": p, "bodies": ids})).is_some() {
    app.status = Some((format!("Saved a copy of {name} as {}", p.display()), false));
  }
}

/// Joints: double-click to drive one.
fn joints_section(app: &mut App, ui: &mut egui::Ui) {
  let joints: Vec<(FeatureId, String)> = app
    .s
    .design
    .features
    .iter()
    .filter(|f| matches!(f.kind, FeatureKind::Joint { .. }))
    .map(|f| (f.id, f.name.clone()))
    .collect();
  if joints.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-joints");
  tree_row(ui, id, 1, Some(true), None, Icon::Folder, "Joints", false, theme::TEXT);
  if !is_open(ui, id, true) {
    return;
  }
  for (fid, name) in joints {
    let ev = tree_row(ui, id.with(fid.0), 2, None, None, Icon::JointOrigin, &name, false, theme::TEXT);
    let drive = |app: &mut App, kind: Kind| {
      app.start(kind);
      if let Some(d) = app.dialog.as_mut() {
        d.set_target(&app.s, fid.0);
      }
    };
    if ev.double_clicked {
      drive(app, Kind::DriveJoint);
    }
    if let Some(r) = ev.response {
      reveal_row(app, Sel::Feature { feature: fid }, &r);
      r.on_hover_text("Double-click to drive").context_menu(|ui| {
        if ui.button("Drive Joint").clicked() {
          ui.close();
          drive(app, Kind::DriveJoint);
        }
        if ui.button("Motion Study").clicked() {
          ui.close();
          drive(app, Kind::MotionStudy);
        }
        if ui.button("Rename").clicked() {
          ui.close();
          app.rename = Some((RenameTarget::Feature(fid.0), name.clone()));
        }
        ui.separator();
        if ui.button("Delete").clicked() {
          ui.close();
          app.run("feature.delete", json!({"feature": fid.0}));
        }
      });
    }
  }
}

/// Drawings of the design: double-click to open one in the Drawing workspace.
fn drawings_section(app: &mut App, ui: &mut egui::Ui) {
  let drawings: Vec<(u32, String)> = app.s.design.drawings.iter().map(|d| (d.id, d.name.clone())).collect();
  if drawings.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-drawings");
  tree_row(ui, id, 1, Some(true), None, Icon::Folder, "Drawings", false, theme::TEXT);
  if !is_open(ui, id, true) {
    return;
  }
  for (did, name) in drawings {
    let ev = tree_row(ui, id.with(did), 2, None, None, Icon::NewDrawing, &name, false, theme::TEXT);
    if ev.double_clicked {
      app.open_drawing(did);
    }
    if let Some(r) = ev.response {
      r.on_hover_text("Double-click to open").context_menu(|ui| {
        if ui.button("Open").clicked() {
          ui.close();
          app.open_drawing(did);
        }
        ui.separator();
        if ui.button("Delete").clicked() {
          ui.close();
          app.run("drawing.delete", json!({"drawing": did}));
        }
      });
    }
  }
}

fn bodies_section(app: &mut App, ui: &mut egui::Ui) {
  // bodies a dialog's preview added appear on OK
  let previewed = app.dialog.as_ref().map(|d| d.preview_bodies(&app.s)).unwrap_or_default();
  let bodies: Vec<(BodyId, String, bool)> = app
    .s
    .model
    .bodies
    .iter()
    .filter(|b| !previewed.contains(&b.id))
    .map(|b| (b.id, app.s.body_name(b.id), app.s.body_visible(b.id)))
    .collect();
  if bodies.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-bodies");
  let all_visible = bodies.iter().all(|b| b.2);
  let ev = tree_row(ui, id, 1, Some(true), Some(all_visible), Icon::Folder, "Bodies", false, theme::TEXT);
  if ev.eye {
    for (b, _, _) in &bodies {
      app.run("body.visible", json!({"body": b.0, "visible": !all_visible}));
    }
  }
  if !is_open(ui, id, true) {
    return;
  }
  for (bid, name, visible) in bodies {
    let selected = app.s.selection.contains(&Sel::Body { body: bid });
    // while isolated, the bodies left out are hidden from the canvas: grey them, as Fusion does
    let isolated = app.ctx_menu.isolate.as_ref().map(|only| only.contains(&bid));
    let color = if visible && isolated != Some(false) { theme::TEXT } else { theme::MUTED };
    let row = id.with(bid.0);
    let ev = tree_row(ui, row, 2, None, Some(visible), Icon::Body, &name, selected, color);
    if isolated == Some(true)
      && let Some(r) = &ev.response
    {
      isolate_badge(ui, r.rect);
    }
    if ev.eye {
      app.run("body.visible", json!({"body": bid.0, "visible": !visible}));
    } else {
      body_row_edits(app, ui, &ev, row, 2, bid, &name, selected);
      if ev.clicked {
        toggle_sel(&mut app.s, Sel::Body { body: bid }, ui.input(|i| i.modifiers.shift || i.modifiers.command));
      }
      browser_edit::slow_click_due(ui, row, app.s.selection.contains(&Sel::Body { body: bid }));
    }
    if let Some(r) = ev.response {
      reveal_row(app, Sel::Body { body: bid }, &r);
      if r.hovered() {
        marking::peek(app, vec![bid], name.clone());
      }
      r.context_menu(|ui| body_menu(app, ui, row, bid, &name, visible));
    }
  }
}

/// A body row's in-place rename (double-click or slow-click) and dragging it onto a component.
#[allow(clippy::too_many_arguments)]
fn body_row_edits(
  app: &mut App,
  ui: &mut egui::Ui,
  ev: &RowEvents,
  row: egui::Id,
  depth: usize,
  bid: BodyId,
  name: &str,
  selected: bool,
) {
  let target = RenameTarget::Body(bid.0);
  browser_edit::slow_click(ui, ev, row, target, name, selected);
  if ev.double_clicked {
    browser_edit::begin(ui, row, target, name);
  }
  browser_edit::drag_source(ui, ev, browser_edit::dragged_bodies(app, bid), name);
  browser_edit::edit_box(app, ui, ev, row, depth);
}

/// A browser body's right-click menu, as in Fusion (less what FissionOxide can't do yet).
fn body_menu(app: &mut App, ui: &mut egui::Ui, row: egui::Id, bid: BodyId, name: &str, visible: bool) {
  let only = vec![Sel::Body { body: bid }];
  let start = |app: &mut App, ui: &mut egui::Ui, kind: Kind| {
    ui.close();
    app.s.selection = only.clone();
    app.start(kind);
  };
  if menu_item(ui, Icon::Move, "Move/Copy", Some("M")).clicked() {
    start(app, ui, Kind::Move);
  }
  ui.separator();
  // like Fusion's, these act on every selected body when the row is one of them
  let browser_edit::Dragged::Bodies(bodies) = browser_edit::dragged_bodies(app, bid) else { return };
  if menu_item(ui, Icon::Component, "Create Components from Bodies", None)
    .on_hover_text("Each body becomes a component of its own, named after it")
    .clicked()
  {
    ui.close();
    for b in &bodies {
      let name = app.s.body_name(*b);
      app.run("component.create", json!({"name": name, "bodies": [b.0]}));
    }
  }
  ui.separator();
  // physical materials belong to components in FissionOxide
  let in_component = app.s.design.component_of(bid).is_some();
  if ui
    .add_enabled_ui(in_component, |ui| menu_item(ui, Icon::PhysMaterial, "Physical Material", None))
    .inner
    .on_disabled_hover_text("Create a component from the body to give it a physical material")
    .clicked()
  {
    start(app, ui, Kind::Material);
  }
  if menu_item(ui, Icon::Appearance, "Appearance", Some("A")).clicked() {
    start(app, ui, Kind::Form(&crate::forms::APPEARANCE));
  }
  if menu_item(ui, Icon::Measure, "Properties", None).clicked() {
    start(app, ui, Kind::Inspect);
  }
  ui.separator();
  if menu_item(ui, Icon::ConvertMesh, "Save As Mesh", None)
    .on_hover_text("Export this body as STL, OBJ or 3MF")
    .clicked()
  {
    ui.close();
    marking::save_as_mesh(app, bid);
  }
  ui.separator();
  if menu_item(ui, Icon::Remove, "Delete", Some("Del")).clicked() {
    ui.close();
    app.s.selection = vec![Sel::Body { body: bid }];
    app.delete_selection();
  }
  if menu_item(ui, Icon::RemoveBody, "Remove", None).on_hover_text("Remove the body (kept in the timeline)").clicked() {
    ui.close();
    app.run("body.remove", json!({"bodies": [bid.0]}));
  }
  if menu_item(ui, Icon::Params, "Rename", None).clicked() {
    ui.close();
    browser_edit::begin(ui, row, RenameTarget::Body(bid.0), name);
  }
  ui.separator();
  if menu_item(ui, if visible { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", Some("V")).clicked() {
    ui.close();
    marking::show_hide(app, &[bid]);
  }
  let isolated = app.ctx_menu.isolate.is_some();
  if menu_item(ui, Icon::Body, if isolated { "Unisolate" } else { "Isolate" }, None).clicked() {
    ui.close();
    app.ctx_menu.isolate = if isolated { None } else { Some(vec![bid]) };
  }
  if menu_item(ui, Icon::Fit, "Find in Window", None).clicked() {
    ui.close();
    marking::find_in_window(app, &[bid], None);
  }
  ui.separator();
  let locked = app.ctx_menu.unselectable.contains(&bid);
  if menu_item(ui, Icon::Select, if locked { "Selectable" } else { "Selectable/Unselectable" }, None)
    .on_hover_text(if locked { "Let the canvas pick the body again" } else { "Stop the canvas picking the body" })
    .clicked()
  {
    ui.close();
    let m = &mut app.ctx_menu;
    m.unselectable.retain(|b| !bodies.contains(b));
    if !locked {
      m.unselectable.extend(&bodies);
      app.s.selection.retain(
        |s| !matches!(s, Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } if bodies.contains(body)),
      );
    }
  }
  browser_edit::opacity_menu(app, ui, &bodies);
}

/// A context-menu row with an icon on the left and its shortcut key on the right.
pub(crate) fn menu_item(ui: &mut egui::Ui, icon: Icon, label: &str, key: Option<&str>) -> egui::Response {
  // the leading spaces leave room for the icon
  let mut b = egui::Button::new(format!("       {label}")).min_size(vec2(170.0, 0.0));
  if let Some(k) = key {
    b = b.shortcut_text(k);
  }
  let r = ui.add(b);
  icons::draw_color(
    ui.painter(),
    Rect::from_center_size(r.rect.left_center() + vec2(13.0, 0.0), Vec2::splat(15.0)),
    icon,
  );
  r
}

/// Find in Browser: the node's row scrolls into view.
fn reveal_row(app: &App, sel: Sel, r: &egui::Response) {
  if app.ctx_menu.reveal == Some(sel) {
    r.scroll_to_me(Some(egui::Align::Center));
  }
}

/// A command on a browser row's context menu (sketch and origin-plane rows).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RowCmd {
  EditSketch,
  ProfileVisibility,
  DimensionVisibility,
  LookAt,
  SaveAsDxf,
  FindInWindow,
  ShowHide,
  Rename,
  Delete,
  CreateSketch,
  OffsetPlane,
  /// Listed, greyed, as Fusion has it but FissionOxide doesn't yet.
  NotYet,
}

/// One row of a browser context menu.
pub(crate) struct RowItem {
  pub icon: Icon,
  pub label: String,
  pub cmd: RowCmd,
}

fn row_item(icon: Icon, label: &str, cmd: RowCmd) -> Option<RowItem> {
  Some(RowItem { icon, label: label.into(), cmd })
}

/// A sketch row's menu, as Fusion's (`None` is a separator).
pub(crate) fn sketch_menu(app: &App, fid: FeatureId) -> Vec<Option<RowItem>> {
  let profiles = !app.hidden_profiles.contains(&fid);
  let dims = app.shown_dims.contains(&fid);
  let visible = fox_engine::scene::sketch_visible(&app.s, fid);
  vec![
    row_item(Icon::Sketch, "Edit Sketch", RowCmd::EditSketch),
    row_item(Icon::Plane, "Redefine Sketch Plane", RowCmd::NotYet),
    row_item(Icon::Section, "Slice Sketch", RowCmd::NotYet),
    None,
    row_item(Icon::SketchSmall, if profiles { "Hide Profile" } else { "Show Profile" }, RowCmd::ProfileVisibility),
    row_item(Icon::Dimension, if dims { "Hide Dimensions" } else { "Show Dimensions" }, RowCmd::DimensionVisibility),
    row_item(Icon::LookAt, "Look At", RowCmd::LookAt),
    row_item(Icon::ExportDxf, "Save As DXF", RowCmd::SaveAsDxf),
    None,
    row_item(Icon::Params, "Rename", RowCmd::Rename),
    row_item(Icon::Remove, "Delete", RowCmd::Delete),
    None,
    row_item(if visible { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", RowCmd::ShowHide),
    row_item(Icon::Fit, "Find in Window", RowCmd::FindInWindow),
  ]
}

/// An origin plane row's menu ("xy", "xz" or "yz"), as Fusion's.
pub(crate) fn origin_plane_menu(app: &App, plane: &str) -> Vec<Option<RowItem>> {
  let shown = app.shown_planes.contains(&json!(plane));
  vec![
    row_item(Icon::Sketch, "Create Sketch", RowCmd::CreateSketch),
    row_item(Icon::Plane, "Offset Plane", RowCmd::OffsetPlane),
    None,
    row_item(if shown { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", RowCmd::ShowHide),
    row_item(Icon::LookAt, "Look At", RowCmd::LookAt),
    row_item(Icon::Fit, "Find in Window", RowCmd::FindInWindow),
  ]
}

/// Draws a row menu; returns the command clicked.
fn row_menu(ui: &mut egui::Ui, items: &[Option<RowItem>]) -> Option<RowCmd> {
  let mut picked = None;
  for it in items {
    let Some(it) = it else {
      ui.separator();
      continue;
    };
    if it.cmd == RowCmd::NotYet {
      ui.add_enabled_ui(false, |ui| menu_item(ui, it.icon, &it.label, None))
        .inner
        .on_disabled_hover_text("Not in FissionOxide yet");
    } else if menu_item(ui, it.icon, &it.label, None).clicked() {
      ui.close();
      picked = Some(it.cmd);
    }
  }
  picked
}

/// Runs a sketch row's menu command.
pub(crate) fn run_sketch_cmd(app: &mut App, fid: FeatureId, name: &str, cmd: RowCmd) {
  let toggle = |v: &mut Vec<FeatureId>| {
    if let Some(i) = v.iter().position(|f| *f == fid) {
      v.remove(i);
    } else {
      v.push(fid);
    }
  };
  match cmd {
    RowCmd::EditSketch => app.edit_sketch(fid),
    RowCmd::ProfileVisibility => toggle(&mut app.hidden_profiles),
    RowCmd::DimensionVisibility => toggle(&mut app.shown_dims),
    RowCmd::LookAt => {
      if let Some(plane) = app.s.model.sketches.get(&fid).map(|r| r.plane) {
        app.look_at_plane(&plane);
      }
    }
    RowCmd::SaveAsDxf => {
      let Some(p) = rfd::FileDialog::new().add_filter("DXF", &["dxf"]).set_file_name(format!("{name}.dxf")).save_file()
      else {
        return;
      };
      if app.run("export", json!({"path": p, "sketch": fid.0})).is_some() {
        app.status = Some((format!("Saved {name} as {}", p.display()), false));
      }
    }
    RowCmd::FindInWindow => marking::find_in_window(app, &[], Some(fid)),
    RowCmd::ShowHide => {
      let visible = fox_engine::scene::sketch_visible(&app.s, fid);
      app.run("sketch.visible", json!({"sketch": fid.0, "visible": !visible}));
    }
    RowCmd::Rename => app.rename = Some((RenameTarget::Feature(fid.0), name.to_string())),
    RowCmd::Delete => {
      app.run("feature.delete", json!({"feature": fid.0}));
    }
    RowCmd::CreateSketch | RowCmd::OffsetPlane | RowCmd::NotYet => {}
  }
}

/// Runs an origin plane row's menu command.
pub(crate) fn run_origin_plane_cmd(app: &mut App, plane: &str, cmd: RowCmd) {
  use fox_engine::model::OriginPlane;
  let origin = match plane {
    "xz" => OriginPlane::Xz,
    "yz" => OriginPlane::Yz,
    _ => OriginPlane::Xy,
  };
  let pl = origin.plane(app.s.design.frames());
  match cmd {
    RowCmd::CreateSketch => app.create_sketch(json!(plane)),
    RowCmd::OffsetPlane => {
      app.s.selection.clear();
      app.start(Kind::OffsetPlane);
      if let Some(d) = app.dialog.as_mut() {
        d.use_origin_plane(&mut app.s, plane);
      }
    }
    RowCmd::ShowHide => {
      let v = json!(plane);
      if let Some(i) = app.shown_planes.iter().position(|p| *p == v) {
        app.shown_planes.remove(i);
      } else {
        app.shown_planes.push(v);
      }
    }
    RowCmd::LookAt => app.look_at_plane(&pl),
    RowCmd::FindInWindow => marking::find_plane_in_window(app, &pl),
    _ => {}
  }
}

fn sketches_section(app: &mut App, ui: &mut egui::Ui) {
  let sketches: Vec<(FeatureId, String)> = app
    .s
    .design
    .features
    .iter()
    .filter(|f| matches!(f.kind, FeatureKind::Sketch { .. }))
    .map(|f| (f.id, f.name.clone()))
    .collect();
  if sketches.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-sketches");
  tree_row(ui, id, 1, Some(true), None, Icon::Folder, "Sketches", false, theme::TEXT);
  if !is_open(ui, id, true) {
    return;
  }
  for (fid, name) in sketches {
    let editing = app.s.edit_sketch == Some(fid);
    let selected = editing || app.s.selection.contains(&Sel::Feature { feature: fid });
    let st = app.s.model.status(fid).cloned().unwrap_or_default();
    let color = if st.error.is_some() {
      theme::ERROR
    } else if st.warning.is_some() {
      theme::WARNING
    } else if st.rolled_back {
      theme::MUTED
    } else {
      theme::TEXT
    };
    let row = id.with(fid.0);
    let ev = tree_row(ui, row, 2, None, None, Icon::SketchSmall, &name, selected, color);
    // double-click edits the sketch, as in Fusion; a slow click renames it
    let target = RenameTarget::Feature(fid.0);
    browser_edit::slow_click(ui, &ev, row, target, &name, selected && !editing);
    if ev.double_clicked {
      app.edit_sketch(fid);
    } else if ev.clicked {
      app.s.selection = vec![Sel::Feature { feature: fid }];
    }
    browser_edit::slow_click_due(ui, row, app.s.selection.contains(&Sel::Feature { feature: fid }));
    browser_edit::edit_box(app, ui, &ev, row, 2);
    if let Some(r) = ev.response {
      reveal_row(app, Sel::Feature { feature: fid }, &r);
      if r.hovered() {
        marking::peek(app, Vec::new(), name.clone());
      }
      r.on_hover_text("Double-click to edit").context_menu(|ui| {
        let items = sketch_menu(app, fid);
        match row_menu(ui, &items) {
          // renamed in place in the browser row
          Some(RowCmd::Rename) => browser_edit::begin(ui, row, target, &name),
          Some(cmd) => run_sketch_cmd(app, fid, &name, cmd),
          None => {}
        }
      });
    }
  }
}

fn planes_section(app: &mut App, ui: &mut egui::Ui) {
  let planes: Vec<(FeatureId, String)> = app
    .s
    .design
    .features
    .iter()
    .filter(|f| matches!(f.kind, FeatureKind::ConstructionPlane { .. }))
    .map(|f| (f.id, f.name.clone()))
    .collect();
  if planes.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-construction");
  tree_row(ui, id, 1, Some(true), None, Icon::Folder, "Construction", false, theme::TEXT);
  if !is_open(ui, id, true) {
    return;
  }
  for (fid, name) in planes {
    let plane = json!({"feature": fid.0});
    let picked = app.picked_plane.as_ref() == Some(&plane);
    let row = id.with(fid.0);
    let ev = tree_row(ui, row, 2, None, None, Icon::Plane, &name, picked, theme::TEXT);
    // double-click sketches on the plane, as on the origin planes; a slow click renames it
    let target = RenameTarget::Feature(fid.0);
    browser_edit::slow_click(ui, &ev, row, target, &name, picked);
    if ev.double_clicked {
      // the first click already started it if Create Sketch was waiting for a plane
      if app.s.edit_sketch.is_none() {
        app.create_sketch(plane.clone());
      }
    } else if ev.clicked && !picked {
      pick_plane(app, plane.clone());
    }
    browser_edit::slow_click_due(ui, row, app.picked_plane.as_ref() == Some(&plane));
    browser_edit::edit_box(app, ui, &ev, row, 2);
    if let Some(r) = ev.response {
      reveal_row(app, Sel::Feature { feature: fid }, &r);
      r.on_hover_text("Double-click to sketch on this plane").context_menu(|ui| {
        if ui.button("Create Sketch").clicked() {
          ui.close();
          app.create_sketch(plane.clone());
        }
        if ui.button("Rename").clicked() {
          ui.close();
          browser_edit::begin(ui, row, target, &name);
        }
        ui.separator();
        if ui.button("Delete").clicked() {
          ui.close();
          app.run("feature.delete", json!({"feature": fid.0}));
        }
      });
    }
  }
}

/// A click on a plane in the browser: while Create Sketch waits for a plane it starts the sketch
/// there, else it selects the plane (Create Sketch then starts on it).
fn pick_plane(app: &mut App, plane: Value) {
  if app.dialog.as_ref().is_some_and(|d| d.kind == Kind::PickPlane) {
    app.cancel_dialog();
    app.create_sketch(plane);
  } else {
    app.s.selection.clear();
    app.picked_plane = if app.picked_plane.as_ref() == Some(&plane) { None } else { Some(plane) };
  }
}

/// The COMMENTS panel in the canvas's bottom-left corner (see [`crate::comments`]).
pub fn comments(app: &mut App, ctx: &egui::Context) {
  crate::comments::show(app, ctx);
}

/// Fusion's navigation bar at the bottom centre of the canvas.
pub fn nav_bar(app: &mut App, ctx: &egui::Context) {
  let canvas = app.canvas;
  egui::Area::new(egui::Id::new("navbar"))
    .fixed_pos(canvas.center_bottom() + vec2(0.0, -8.0))
    .pivot(egui::Align2::CENTER_BOTTOM)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      // Fusion draws the navigation bar as bare icons on the canvas
      ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let mode_btn = |ui: &mut egui::Ui, app: &mut App, icon, hint, mode| {
          if icons::icon_button(ui, icon, 28.0, hint, app.nav_mode == Some(mode), None).clicked() {
            app.nav_mode = if app.nav_mode == Some(mode) { None } else { Some(mode) };
          }
        };
        let orbit_hint = if app.free_orbit {
          "Free Orbit: left-drag to orbit and roll (Esc to stop)"
        } else {
          "Constrained Orbit: left-drag to orbit (Esc to stop)"
        };
        mode_btn(ui, app, Icon::Orbit, orbit_hint, NavMode::Orbit);
        let c = nav_caret(ui, "Orbit type");
        egui::Popup::menu(&c).show(|ui| crate::navbar::orbit_menu(app, ui));
        if icons::icon_button(
          ui,
          Icon::LookAt,
          28.0,
          "Look At: the sketch plane or a selected planar face",
          false,
          None,
        )
        .clicked()
        {
          app.look_at_selection();
        }
        mode_btn(ui, app, Icon::Pan, "Pan: left-drag to pan (Esc to stop)", NavMode::Pan);
        mode_btn(ui, app, Icon::Zoom, "Zoom: left-drag to zoom (Esc to stop)", NavMode::Zoom);
        let window = app.nav_mode == Some(NavMode::ZoomWindow);
        let hint = if window { "Zoom Window: drag a box to zoom into (Esc to stop)" } else { "Fit (F6)" };
        if icons::icon_button(ui, Icon::Fit, 28.0, hint, window, None).clicked() {
          if window {
            app.nav_mode = None;
          } else {
            app.fit_view();
          }
        }
        let c = nav_caret(ui, "Fit and Zoom Window");
        egui::Popup::menu(&c).show(|ui| crate::navbar::fit_menu(app, ui));
        nav_sep(ui);
        let d = icons::icon_button(ui, Icon::Display, 28.0, "Display Settings", false, None);
        let c = nav_caret(ui, "Display Settings");
        egui::Popup::menu(&d).show(|ui| crate::navbar::display_menu(app, ui));
        egui::Popup::menu(&c).show(|ui| crate::navbar::display_menu(app, ui));
        let g = icons::icon_button(ui, Icon::Grid, 28.0, "Grid and Snaps", false, None);
        let c = nav_caret(ui, "Grid and Snaps");
        let adaptive = fox_engine::scene::grid_step(&app.s);
        egui::Popup::menu(&g).show(|ui| crate::navbar::grid_menu(app, ui, adaptive));
        egui::Popup::menu(&c).show(|ui| crate::navbar::grid_menu(app, ui, adaptive));
        let v = icons::icon_button(ui, Icon::Viewports, 28.0, "Viewports", false, None);
        let c = nav_caret(ui, "Viewports");
        egui::Popup::menu(&v).show(|ui| {
          let _ = ui.selectable_label(true, "Single View");
        });
        egui::Popup::menu(&c).show(|ui| {
          let _ = ui.selectable_label(true, "Single View");
        });
      });
    });
}

/// The small drop-down caret after a navigation bar icon: clicking it opens the icon's fly-out.
fn nav_caret(ui: &mut egui::Ui, hint: &str) -> egui::Response {
  let (r, resp) = ui.allocate_exact_size(vec2(10.0, 28.0), egui::Sense::click());
  if resp.hovered() {
    ui.painter().rect_filled(r, 2.0, theme::HOVER);
  }
  icons::chevron(ui.painter(), r.center(), theme::GROUP_LABEL);
  resp.on_hover_text(hint)
}

fn nav_sep(ui: &mut egui::Ui) {
  let (r, _) = ui.allocate_exact_size(vec2(7.0, 24.0), egui::Sense::hover());
  ui.painter().line_segment(
    [r.center_top() + vec2(0.0, 3.0), r.center_bottom() - vec2(0.0, 3.0)],
    Stroke::new(1.0, theme::SEPARATOR),
  );
}

/// Where the SKETCH PALETTE's top-right corner goes: below an open command dialog that sits over
/// its place (as Fusion stacks them), else at its usual place.
pub(crate) fn palette_pos(canvas: Rect, dialog: Option<Rect>) -> egui::Pos2 {
  let home = canvas.right_top() + vec2(-34.0, 190.0);
  match dialog {
    Some(d) if d.max.x > home.x - 270.0 && d.min.x < home.x && d.min.y <= home.y + 40.0 => {
      egui::pos2(home.x, home.y.max(d.max.y + 8.0))
    }
    _ => home,
  }
}

/// The SKETCH PALETTE shown on the right of the canvas while a sketch is open.
pub fn sketch_palette(app: &mut App, ctx: &egui::Context) {
  let Some(sk) = app.s.edit_sketch else { return };
  let canvas = app.canvas;
  let at = palette_pos(
    canvas,
    app.dialog.is_some().then(|| ctx.memory(|m| m.area_rect(egui::Id::new("feature-dialog")))).flatten(),
  );
  egui::Area::new(egui::Id::new("sketch-palette"))
    .fixed_pos(at)
    .pivot(egui::Align2::RIGHT_TOP)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      dialog_frame().show(ui, |ui| {
        let width = 270.0;
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        dialog_header(ui, Icon::Sketch, "SKETCH PALETTE", width);
        egui::Frame::new().inner_margin(egui::Margin { left: 10, right: 14, top: 6, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
          let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), egui::Sense::hover());
          triangle(ui.painter(), r.left_center() + vec2(5.0, 0.0), true, theme::GROUP_LABEL);
          ui.painter().text(
            r.left_center() + vec2(16.0, 0.0),
            egui::Align2::LEFT_CENTER,
            "Options",
            egui::FontId::proportional(13.0),
            theme::TEXT,
          );
          let row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            ui.horizontal(|ui| {
              ui.set_height(27.0);
              ui.add_space(14.0);
              ui.label(label);
              ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| add(ui));
            });
          };
          row(ui, "Linetype", &mut |ui| {
            if icons::icon_button(ui, Icon::Construction, 22.0, "Normal/Construction (X)", false, None).clicked() {
              app.toggle_construction();
            }
          });
          row(ui, "Look At", &mut |ui| {
            if icons::icon_button(ui, Icon::LookAt, 22.0, "Look at the sketch plane", false, None).clicked() {
              app.look_at_selection();
            }
          });
          row(ui, "Sketch Grid", &mut |ui| {
            ui.checkbox(&mut app.show_grid, "");
          });
          row(ui, "Snap", &mut |ui| {
            ui.checkbox(&mut app.snap, "");
          });
          // options FissionOxide doesn't have yet are listed, greyed, as in Fusion's palette
          for (label, on) in [("Slice", false), ("Profile", true), ("Points", true)] {
            row(ui, label, &mut |ui| {
              let mut v = on;
              ui.add_enabled(false, egui::Checkbox::without_text(&mut v))
                .on_disabled_hover_text("Not in FissionOxide yet");
            });
          }
          row(ui, "Dimensions", &mut |ui| {
            ui.checkbox(&mut app.show_dims, "");
          });
          row(ui, "Constraints", &mut |ui| {
            ui.checkbox(&mut app.show_constraints, "");
          });
          for (label, on) in [("Projected Geometries", true), ("Construction Geometries", true), ("3D Sketch", false)] {
            row(ui, label, &mut |ui| {
              let mut v = on;
              ui.add_enabled(false, egui::Checkbox::without_text(&mut v))
                .on_disabled_hover_text("Not in FissionOxide yet");
            });
          }
          if let Some(r) = app.s.model.sketches.get(&sk) {
            ui.add_space(4.0);
            let (txt, col) = if !r.report.ok() {
              ("Over-constrained / conflicting".to_string(), theme::ERROR)
            } else if r.report.dof == 0 {
              ("Fully constrained".to_string(), theme::TEXT)
            } else {
              (format!("{} degrees of freedom", r.report.dof), theme::ACCENT)
            };
            ui.horizontal(|ui| {
              ui.add_space(14.0);
              ui.label(RichText::new(txt).color(col).small());
              ui.label(RichText::new(format!("· {} profiles", r.profiles.len())).color(theme::MUTED).small());
            });
          }
        });
        let sep = ui.allocate_exact_size(vec2(width, 1.0), egui::Sense::hover()).0;
        ui.painter().rect_filled(sep, 0.0, Color32::from_gray(215));
        egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
          ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if primary_button(ui, "Finish Sketch", icons::accent_of(Icon::Sketch)).clicked() {
              app.finish_sketch();
            }
          });
        });
      });
    });
}

/// Frame of a command dialog (and the sketch palette).
pub fn dialog_frame() -> egui::Frame {
  egui::Frame::new()
    .fill(theme::DIALOG_BODY)
    .stroke(Stroke::new(1.0, Color32::from_gray(190)))
    .corner_radius(2)
    .shadow(egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(35) })
}

/// The title bar of a command dialog: the command's icon and title, ▸▸, and a thin rule in the
/// command family's accent colour.
pub fn dialog_header(ui: &mut egui::Ui, icon: Icon, title: &str, width: f32) {
  let (rect, _) = ui.allocate_exact_size(vec2(width, 30.0), egui::Sense::hover());
  let p = ui.painter();
  p.rect_filled(rect, egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, theme::DIALOG_HEADER);
  p.line_segment(
    [rect.left_bottom() + vec2(0.0, -1.0), rect.right_bottom() + vec2(0.0, -1.0)],
    Stroke::new(2.0, icons::accent_of(icon)),
  );
  let ink = theme::GROUP_LABEL;
  icons::draw_color(p, Rect::from_center_size(rect.left_center() + vec2(16.0, 0.0), Vec2::splat(18.0)), icon);
  p.text(rect.left_center() + vec2(32.0, 0.0), egui::Align2::LEFT_CENTER, title, egui::FontId::proportional(12.5), ink);
  for dx in [-14.0, -8.0] {
    let t = rect.right_center() + vec2(dx, 0.0);
    p.add(egui::Shape::convex_polygon(
      vec![t + vec2(-2.0, -4.0), t + vec2(3.0, 0.0), t + vec2(-2.0, 4.0)],
      ink,
      Stroke::NONE,
    ));
  }
}

/// The primary dialog button (OK, Finish Sketch): a light tint of the command's accent with an
/// accent border.
pub fn primary_button(ui: &mut egui::Ui, label: &str, accent: Color32) -> egui::Response {
  ui.add(
    egui::Button::new(RichText::new(label).color(theme::TEXT))
      .fill(icons::tint_of(accent, 0.84))
      .stroke(Stroke::new(1.0, accent))
      .corner_radius(3)
      .min_size(vec2(72.0, 24.0)),
  )
}

/// The secondary dialog button (Cancel, Close): white with a slate-grey border.
pub fn secondary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
  ui.add(
    egui::Button::new(RichText::new(label).color(theme::TEXT))
      .fill(Color32::WHITE)
      .stroke(Stroke::new(1.0, theme::BORDER))
      .corner_radius(3)
      .min_size(vec2(72.0, 24.0)),
  )
}

/// The collapsed AUTODESK ASSISTANT tab on the right edge of the canvas, under the ViewCube.
pub fn assistant_tab(app: &mut App, ctx: &egui::Context) {
  let canvas = app.canvas;
  egui::Area::new(egui::Id::new("assistant-tab"))
    .fixed_pos(canvas.right_top() + vec2(-2.0, 210.0))
    .pivot(egui::Align2::RIGHT_TOP)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 260.0), egui::Sense::click());
      let p = ui.painter();
      p.rect_filled(rect, 2.0, if resp.hovered() { theme::HOVER } else { theme::FLOAT_HEADER });
      let ink = theme::GROUP_LABEL;
      for dy in [-3.0, 3.0] {
        let t = rect.center_top() + vec2(dy, 14.0);
        p.add(egui::Shape::convex_polygon(
          vec![t + vec2(-3.0, 0.0), t + vec2(2.0, -4.0), t + vec2(2.0, 4.0)],
          ink,
          Stroke::NONE,
        ));
      }
      let galley = p.layout_no_wrap("AUTODESK ASSISTANT".into(), egui::FontId::proportional(12.0), ink);
      let w = galley.size().x;
      let pos = pos2(rect.center().x - galley.size().y / 2.0, rect.top() + 36.0 + w);
      p.add(egui::epaint::TextShape::new(pos, galley, ink).with_angle(-std::f32::consts::FRAC_PI_2));
      if resp.clicked() {
        app.status = Some(("The Autodesk Assistant isn't part of FissionOxide".into(), false));
      }
    });
}

/// Fusion's "Unsaved: Changes may be lost — Save" notice at the top of the canvas, shown for a
/// changed design that has never been saved.
pub fn unsaved_banner(app: &mut App, ctx: &egui::Context) {
  if app.s.path.is_some() || !app.s.dirty {
    return;
  }
  let canvas = app.canvas;
  egui::Area::new(egui::Id::new("unsaved"))
    .fixed_pos(canvas.center_top() + vec2(0.0, 8.0))
    .pivot(egui::Align2::CENTER_TOP)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), egui::Sense::hover());
        let p = ui.painter();
        p.add(egui::Shape::convex_polygon(
          vec![r.center_top(), r.right_bottom(), r.left_bottom()],
          theme::ICON_ORANGE,
          Stroke::NONE,
        ));
        p.text(
          r.center() + vec2(0.0, 2.0),
          egui::Align2::CENTER_CENTER,
          "!",
          egui::FontId::proportional(11.0),
          Color32::WHITE,
        );
        ui.label(RichText::new("Unsaved:").strong().color(theme::TEXT));
        ui.label(RichText::new("Changes may be lost").color(theme::TEXT));
        ui.add_space(6.0);
        if ui.link("Save").clicked() {
          app.save(false);
        }
      });
    });
}

/// What is selected (or hovered), at the canvas's bottom-right corner, as Fusion shows it.
pub fn selection_info(app: &mut App, ctx: &egui::Context) {
  // an extrude's picked profiles read "1 Profile | Area: …"; otherwise what's selected or hovered
  let text = match app.dialog.as_ref().and_then(|d| d.profile_status(&app.s)) {
    Some(p) => p,
    None => {
      let Some(t) = selection_text(app) else { return };
      t
    }
  };
  let canvas = app.canvas;
  let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("selection-info")));
  painter.text(
    canvas.right_bottom() - vec2(14.0, 12.0),
    egui::Align2::RIGHT_BOTTOM,
    text,
    egui::FontId::proportional(12.5),
    Color32::from_gray(50),
  );
}

/// The selection readout: `1 Face | Area: 100.00 mm^2`, `2 Edges | Length: 20.00 mm`, a body's
/// or feature's name; the browser or timeline row under the pointer names itself instead.
pub(crate) fn selection_text(app: &App) -> Option<String> {
  if let Some(l) = &app.ctx_menu.peek_label {
    return Some(l.clone());
  }
  let hovered;
  let sel = match app.s.selection.as_slice() {
    [] => {
      hovered = [app.hover?.to_sel()];
      &hovered[..]
    }
    s => s,
  };
  let noun = |s: &Sel| -> (&'static str, &'static str) {
    match s {
      Sel::Face { .. } => ("Face", "Faces"),
      Sel::Edge { .. } => ("Edge", "Edges"),
      Sel::Body { .. } => ("Body", "Bodies"),
      Sel::Profile { .. } => ("Profile", "Profiles"),
      Sel::Feature { .. } => ("Feature", "Features"),
      Sel::SketchCurve { .. } => ("Sketch Curve", "Sketch Curves"),
      Sel::SketchPoint { .. } => ("Sketch Point", "Sketch Points"),
      Sel::SketchConstraint { .. } => ("Constraint", "Constraints"),
      Sel::Vertex { .. } => ("Vertex", "Vertices"),
    }
  };
  let first = noun(&sel[0]);
  if !sel.iter().all(|s| noun(s) == first) {
    return Some(format!("{} objects", sel.len()));
  }
  let n = sel.len();
  let count = format!("{n} {}", if n == 1 { first.0 } else { first.1 });
  let model = &app.s.model;
  let sum = |f: &dyn Fn(&Sel) -> Option<f64>| sel.iter().map(f).sum::<Option<f64>>();
  let u = app.s.design.units;
  let readout = match sel[0] {
    Sel::Face { .. } => sum(&|s| match s {
      Sel::Face { body, face } => model.body(*body)?.faces().iter().find(|f| f.id == *face).map(|f| f.area),
      _ => None,
    })
    .map(|a| format!("Area: {}", u.area(a, 2))),
    Sel::Edge { .. } => sum(&|s| match s {
      Sel::Edge { body, edge } => model.body(*body)?.edges().iter().find(|e| e.id == *edge).map(|e| e.length),
      _ => None,
    })
    .map(|l| format!("Length: {}", u.fixed(l, 2))),
    Sel::Profile { .. } => sum(&|s| match s {
      Sel::Profile { sketch, index } => model.sketches.get(sketch)?.profiles.get(*index).map(|p| p.area().abs()),
      _ => None,
    })
    .map(|a| format!("Area: {}", u.area(a, 2))),
    Sel::Body { body } if n == 1 => return Some(app.s.body_name(body)),
    Sel::Feature { feature } if n == 1 => return app.s.design.feature(feature).map(|f| f.name.clone()),
    _ => None,
  };
  Some(match readout {
    Some(r) => format!("{count} | {r}"),
    None => count,
  })
}

/// Status message and sketch hint at the bottom-left of the canvas, above COMMENTS.
pub fn canvas_status(app: &mut App, ctx: &egui::Context) {
  let Some((msg, err)) = app.status.clone() else { return };
  // Preferences → General → Show command prompt
  if !err && !app.preferences.values.show_prompt && app.hint.as_deref() == Some(msg.as_str()) {
    return;
  }
  let canvas = app.canvas;
  let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("canvas-status")));
  let font = egui::FontId::proportional(12.0);
  let color = if err { theme::ERROR } else { Color32::from_gray(70) };
  let galley = painter.layout_no_wrap(msg, font, color);
  let at = canvas.left_bottom() + vec2(12.0, -46.0 - galley.size().y);
  let bg = Rect::from_min_size(at - vec2(5.0, 3.0), galley.size() + vec2(10.0, 6.0));
  painter.rect_filled(bg, 2.0, Color32::from_white_alpha(200));
  painter.galley(at, galley, color);
}

// ---------------------------------------------------------------- timeline

pub fn timeline(app: &mut App, ui: &mut egui::Ui) {
  // playback
  if let Some(next) = app.playing {
    let now = ui.input(|i| i.time);
    if now >= next {
      let n = app.s.design.features.len();
      let active = app.s.design.active_len();
      if active >= n {
        app.playing = None;
      } else {
        let at = if active + 1 >= n { serde_json::Value::Null } else { json!(active + 1) };
        app.run("timeline.marker", json!({"at": at}));
        app.playing = Some(now + 0.45);
      }
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
  }
  egui::Panel::bottom("timeline")
    .exact_size(36.0)
    .frame(egui::Frame::NONE.fill(theme::TIMELINE).inner_margin(egui::Margin::symmetric(8, 2)))
    .show(ui, |ui| {
      let full = ui.max_rect();
      ui.painter().line_segment([full.left_top(), full.right_top()], Stroke::new(1.0, theme::BORDER));
      ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let n = app.s.design.features.len();
        let active = app.s.design.active_len();
        let ink = Some(theme::GROUP_LABEL);
        if icons::icon_button(ui, Icon::ToStart, 22.0, "Move to beginning", false, ink).clicked() {
          app.run("timeline.marker", json!({"at": 0}));
        }
        if icons::icon_button(ui, Icon::StepBack, 22.0, "Move to previous step", false, ink).clicked() && active > 0 {
          app.run("timeline.marker", json!({"at": active - 1}));
        }
        let playing = app.playing.is_some();
        if icons::icon_button(ui, Icon::Play, 22.0, "Play", playing, ink).clicked() {
          if playing {
            app.playing = None;
          } else {
            if active >= n {
              app.run("timeline.marker", json!({"at": 0}));
            }
            app.playing = Some(ui.input(|i| i.time) + 0.3);
          }
        }
        if icons::icon_button(ui, Icon::StepForward, 22.0, "Move to next step", false, ink).clicked() {
          let at = if active + 1 >= n { serde_json::Value::Null } else { json!(active + 1) };
          app.run("timeline.marker", json!({"at": at}));
        }
        if icons::icon_button(ui, Icon::ToEnd, 22.0, "Move to end", false, ink).clicked() {
          app.run("timeline.marker", json!({"at": null}));
        }
        ui.add_space(10.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          let g = icons::icon_button(ui, Icon::Gear, 24.0, "Timeline settings", false, None);
          egui::Popup::menu(&g).show(|ui| {
            if ui.button("Roll to end").clicked() {
              ui.close();
              app.run("timeline.marker", json!({"at": null}));
            }
            if ui.button("Roll to beginning").clicked() {
              ui.close();
              app.run("timeline.marker", json!({"at": 0}));
            }
          });
          ui.add_space(6.0);
          ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            timeline_items(app, ui, n, active);
          });
        });
      });
    });
  rename_popup(app, ui.ctx());
}

/// A drag in progress on the timeline; `gap` is the slot under the pointer (0 = before the
/// first item, n = after the last).
#[derive(Clone, Copy, Debug)]
enum TimelineDrag {
  Marker { gap: usize },
  Feature { id: FeatureId, from: usize, gap: usize },
}

fn timeline_items(app: &mut App, ui: &mut egui::Ui, n: usize, active: usize) {
  let key = egui::Id::new("timeline_drag");
  let mut drag: Option<TimelineDrag> = ui.data(|d| d.get_temp(key));
  egui::ScrollArea::horizontal()
    .auto_shrink([false, false])
    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
    .stick_to_right(true)
    .show(ui, |ui| {
      ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        let features: Vec<(FeatureId, String, Icon, bool)> =
          app.s.design.features.iter().map(|f| (f.id, f.name.clone(), feature_icon(&f.kind), f.suppressed)).collect();
        let mut rects = Vec::with_capacity(features.len());
        let marker_dragged = matches!(drag, Some(TimelineDrag::Marker { .. }));
        let groups = timeline_groups::shown(app);
        // a collapsed group's icon stands for each of its features (drag slots skip over it)
        let mut collapsed: Option<(Rect, usize)> = None;
        // expanded groups: (open folder, last feature) for the bracket under them
        let mut brackets: Vec<(Rect, usize)> = Vec::new();
        for (i, (id, name, icon, suppressed)) in features.into_iter().enumerate() {
          if let Some((rect, last)) = collapsed {
            if i <= last {
              rects.push(rect);
              continue;
            }
            collapsed = None;
          }
          if i == active && marker(ui, marker_dragged).drag_started() {
            drag = Some(TimelineDrag::Marker { gap: active });
          }
          if let Some(g) = groups.iter().find(|g| g.first == i) {
            let open = timeline_groups::is_open(ui, g.id);
            let rolled_back = match drag {
              Some(TimelineDrag::Marker { gap }) => i >= gap,
              _ => i >= active,
            };
            let rect = timeline_groups::group_icon(app, ui, g, open, rolled_back);
            if open {
              brackets.push((rect, g.last));
            } else {
              rects.push(rect);
              collapsed = Some((rect, g.last));
              // the marker inside a collapsed group shows just after it
              if active > i && active <= g.last && marker(ui, marker_dragged).drag_started() {
                drag = Some(TimelineDrag::Marker { gap: active });
              }
              continue;
            }
          }
          let st = app.s.model.status(id).cloned().unwrap_or_default();
          // while the marker is dragged, what would roll back greys out already
          let rolled_back = match drag {
            Some(TimelineDrag::Marker { gap }) => i >= gap,
            _ => st.rolled_back,
          };
          let dim = rolled_back || suppressed;
          let moving = matches!(drag, Some(TimelineDrag::Feature { from, .. }) if from == i);
          let selected = app.s.selection.contains(&Sel::Feature { feature: id });
          let editing = app.s.edit_sketch == Some(id);
          let mut hover = name.clone();
          if let Some(e) = &st.error {
            hover = format!("{name}\n⚠ {e}");
          } else if let Some(w) = &st.warning {
            hover = format!("{name}\n{w}");
          }
          if suppressed {
            hover.push_str("\n(suppressed)");
          }
          let (rect, r) = ui.allocate_exact_size(vec2(26.0, 28.0), egui::Sense::click_and_drag());
          rects.push(rect);
          let p = ui.painter();
          if st.error.is_some() {
            p.rect(
              rect,
              2.0,
              Color32::from_rgb(253, 225, 222),
              Stroke::new(1.0, theme::ERROR),
              egui::StrokeKind::Inside,
            );
          } else if st.warning.is_some() {
            p.rect(
              rect,
              2.0,
              Color32::from_rgb(255, 243, 210),
              Stroke::new(1.0, theme::WARNING),
              egui::StrokeKind::Inside,
            );
          }
          if selected || editing || moving {
            p.rect(rect, 2.0, theme::SELECTED, Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Inside);
          } else if r.hovered() && drag.is_none() {
            p.rect(rect, 2.0, theme::HOVER, Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
          }
          let ir = Rect::from_center_size(rect.center(), Vec2::splat(20.0));
          if dim {
            icons::draw(p, ir, icon, Color32::from_gray(185));
          } else {
            icons::draw_color(p, ir, icon);
          }
          if suppressed {
            p.line_segment(
              [rect.left_bottom() + vec2(3.0, -3.0), rect.right_top() + vec2(-3.0, 3.0)],
              Stroke::new(1.0, theme::ERROR),
            );
          }
          if r.drag_started() {
            app.s.selection = vec![Sel::Feature { feature: id }];
            drag = Some(TimelineDrag::Feature { id, from: i, gap: i });
          }
          let r = if drag.is_none() { r.on_hover_text(hover) } else { r };
          if r.clicked() {
            let m = ui.input(|i| i.modifiers);
            timeline_groups::click_feature(app, id, m.shift, m.command || m.ctrl);
          }
          if r.double_clicked() {
            edit_feature(app, id);
          }
          // hovering an item highlights what it made (a fillet's rounds, an extrude's body, a
          // sketch's curves) and names it in the status bar
          if r.hovered() && drag.is_none() {
            marking::peek_feature(app, id, name.clone());
          }
          r.context_menu(|ui| timeline_menu(app, ui, i, id, &name, icon, suppressed));
        }
        for (open, last) in brackets {
          if let Some(end) = rects.get(last) {
            timeline_groups::bracket(ui, open, *end);
          }
        }
        if active >= n && marker(ui, marker_dragged).drag_started() {
          drag = Some(TimelineDrag::Marker { gap: active });
        }
        if let Some(d) = drag.as_mut() {
          timeline_drag(app, ui, d, &rects, active);
          if !ui.input(|i| i.pointer.primary_down()) {
            timeline_drop(app, *d, n, active);
            drag = None;
          }
        }
      });
    });
  ui.data_mut(|d| match drag {
    Some(v) => _ = d.insert_temp(key, v),
    None => d.remove::<TimelineDrag>(key),
  });
}

/// Where a feature dragged from `from` and dropped in slot `gap` ends up.
fn moved_to(from: usize, gap: usize) -> usize {
  if gap > from { gap - 1 } else { gap }
}

/// Follows the pointer during a timeline drag: snaps to the slot between icons and draws the
/// marker there, or the insertion line (red where the feature can't go).
fn timeline_drag(app: &App, ui: &egui::Ui, d: &mut TimelineDrag, rects: &[Rect], active: usize) {
  let (Some(first), Some(last)) = (rects.first(), rects.last()) else { return };
  if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
    let gap = rects.iter().filter(|r| r.center().x < pos.x).count();
    match d {
      TimelineDrag::Marker { gap: g } | TimelineDrag::Feature { gap: g, .. } => *g = gap,
    }
  }
  ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
  let x = |gap: usize| match gap {
    0 => first.left() - 1.0,
    g if g >= rects.len() => last.right() + 1.0,
    g => (rects[g - 1].right() + rects[g].left()) / 2.0,
  };
  let (top, bottom) = (first.center().y - 16.0, first.center().y + 16.0);
  let p = ui.painter();
  match *d {
    TimelineDrag::Marker { gap } if gap != active => paint_marker(p, x(gap), top, bottom, Color32::from_gray(55)),
    TimelineDrag::Feature { id, from, gap } if moved_to(from, gap) != from => {
      let ok = app.s.design.check_move(id, moved_to(from, gap)).is_ok();
      let c = if ok { theme::ACCENT } else { theme::ERROR };
      let x = x(gap);
      p.line_segment([pos2(x, top + 2.0), pos2(x, bottom - 2.0)], Stroke::new(2.0, c));
      for y in [top + 2.0, bottom - 2.0] {
        p.line_segment([pos2(x - 3.0, y), pos2(x + 3.0, y)], Stroke::new(2.0, c));
      }
      if !ok {
        ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
      }
    }
    _ => {}
  }
}

/// Finishes a timeline drag: rolls the marker or moves the feature.
fn timeline_drop(app: &mut App, d: TimelineDrag, n: usize, active: usize) {
  match d {
    TimelineDrag::Marker { gap } if gap != active => {
      let at = if gap >= n { Value::Null } else { json!(gap) };
      app.run("timeline.marker", json!({ "at": at }));
    }
    TimelineDrag::Feature { id, from, gap } if moved_to(from, gap) != from => {
      let to = moved_to(from, gap);
      match app.s.design.check_move(id, to) {
        Ok(()) => {
          app.run("feature.move", json!({"feature": id.0, "to": to}));
        }
        Err(e) => app.status = Some((format!("Can't move there: {e}"), true)),
      }
    }
    _ => {}
  }
}

/// A timeline item's right-click menu, as in Fusion (less what FissionOxide can't do yet).
fn timeline_menu(app: &mut App, ui: &mut egui::Ui, i: usize, id: FeatureId, name: &str, icon: Icon, suppressed: bool) {
  if menu_item(ui, icon, "Edit Feature", None).clicked() {
    ui.close();
    edit_feature(app, id);
  }
  if let Some(sk) = marking::profile_sketch(app, id)
    && menu_item(ui, Icon::Sketch, "Edit Profile Sketch", None).clicked()
  {
    ui.close();
    app.edit_sketch(sk);
  }
  // Configure: the feature's suppression becomes a column of the configuration table
  if let Some(first) = app.s.design.configurations.list.first().map(|c| c.name.clone())
    && menu_item(ui, Icon::ConfigTable, "Configure", None)
      .on_hover_text("Add this feature's suppression to the configuration table")
      .clicked()
  {
    ui.close();
    app.run("config.set", json!({"configuration": first, "feature": id.0, "suppressed": suppressed}));
    crate::config_ui::open_table();
  }
  ui.separator();
  if menu_item(ui, Icon::Remove, "Delete", Some("Del")).clicked() {
    ui.close();
    app.run("feature.delete", json!({"feature": id.0}));
  }
  if menu_item(ui, Icon::Params, "Rename", None).clicked() {
    ui.close();
    app.rename = Some((RenameTarget::Feature(id.0), name.to_string()));
  }
  if menu_item(ui, Icon::History, "Roll Timeline Marker Here", None).clicked() {
    ui.close();
    app.run("timeline.marker", json!({"at": i + 1}));
  }
  let label = if suppressed { "Unsuppress Features" } else { "Suppress Features" };
  if menu_item(ui, Icon::EyeOff, label, None).clicked() {
    ui.close();
    app.run("feature.suppress", json!({"feature": id.0}));
  }
  timeline_groups::feature_menu_items(app, ui, i, id);
  ui.separator();
  // features without a browser node of their own are found by the bodies they made
  let sel = Sel::Feature { feature: id };
  let node = if marking::in_browser(app, &sel) {
    Some(sel)
  } else {
    marking::feature_bodies(app, id).first().map(|b| Sel::Body { body: *b })
  };
  if ui.add_enabled_ui(node.is_some(), |ui| menu_item(ui, Icon::Folder, "Find in Browser", None)).inner.clicked()
    && let Some(node) = node
  {
    ui.close();
    marking::find_in_browser(app, node);
  }
  if menu_item(ui, Icon::Fit, "Find in Window", None).clicked() {
    ui.close();
    let bodies = marking::feature_bodies(app, id);
    marking::find_in_window(app, &bodies, Some(id));
  }
}

/// The history marker: a dark bar with a grab handle, as in Fusion's timeline. Drag it to roll
/// the timeline back or forward; while it's dragged it stays faded where it was.
fn marker(ui: &mut egui::Ui, dragged: bool) -> egui::Response {
  let (r, resp) = ui.allocate_exact_size(vec2(12.0, 32.0), egui::Sense::drag());
  let c = if dragged { Color32::from_gray(175) } else { Color32::from_gray(55) };
  paint_marker(ui.painter(), r.center().x, r.top(), r.bottom(), c);
  if resp.hovered() && !dragged {
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
  }
  resp.on_hover_text("Drag to roll the timeline back or forward")
}

fn paint_marker(p: &egui::Painter, x: f32, top: f32, bottom: f32, c: Color32) {
  p.line_segment([pos2(x, top + 4.0), pos2(x, bottom)], Stroke::new(2.0, c));
  // the grab handle sits on top, as in Fusion
  p.add(egui::Shape::convex_polygon(
    vec![
      pos2(x - 5.0, top),
      pos2(x + 5.0, top),
      pos2(x + 5.0, top + 7.0),
      pos2(x, top + 11.0),
      pos2(x - 5.0, top + 7.0),
    ],
    c,
    Stroke::NONE,
  ));
}

/// Applies a constraint to the current sketch selection.
/// UnFix: when every selected point, and every point of each selected curve, is fixed, deletes
/// those Fix constraints (and a selected circle's fixed radius). Returns whether it did.
fn unfix(app: &mut App, sk: FeatureId, points: &[u32], curves: &[u32]) -> bool {
  use fox_engine::sketch::{ConstraintKind, CurveId, PointId};
  let Ok(sketch) = app.s.design.sketch(sk) else { return false };
  let mut pts: Vec<PointId> = points.iter().map(|p| PointId(*p)).collect();
  for c in curves {
    pts.extend(sketch.curve_points(CurveId(*c)));
  }
  if pts.is_empty() {
    return false;
  }
  let fix_of = |p: PointId| {
    sketch.constraints.iter().find(|c| matches!(c.kind, ConstraintKind::Fix { p: q, .. } if q == p)).map(|c| c.id.0)
  };
  // points pinned by the sketch itself (its origin) count as fixed but have nothing to delete
  let pinned = |p: PointId| sketch.point(p).is_some_and(|pt| pt.fixed);
  if !pts.iter().all(|p| fix_of(*p).is_some() || pinned(*p)) {
    return false;
  }
  let mut cons: Vec<u32> = pts.iter().filter_map(|p| fix_of(*p)).collect();
  if cons.is_empty() {
    return false;
  }
  cons.extend(sketch.constraints.iter().filter_map(|c| match c.kind {
    ConstraintKind::FixRadius { curve, .. } if curves.contains(&curve.0) => Some(c.id.0),
    _ => None,
  }));
  cons.sort_unstable();
  cons.dedup();
  app.run("sketch.delete", json!({"sketch": sk.0, "constraints": cons})).is_some()
}

pub fn apply_constraint(app: &mut App, kind: &str) {
  let Some(sk) = app.s.edit_sketch else { return };
  let mut points = Vec::new();
  let mut curves = Vec::new();
  for s in &app.s.selection {
    match s {
      Sel::SketchPoint { point, .. } => points.push(point.0),
      Sel::SketchCurve { curve, .. } => curves.push(curve.0),
      _ => {}
    }
  }
  // Horizontal/Vertical picks per line (or point pair) whichever is closer
  if kind == "hv" {
    let Ok(sketch) = app.s.design.sketch(sk) else { return };
    let mut jobs: Vec<(&'static str, serde_json::Value)> = Vec::new();
    if points.len() == 2
      && curves.is_empty()
      && let (Some(a), Some(b)) =
        (sketch.pos(fox_engine::sketch::PointId(points[0])), sketch.pos(fox_engine::sketch::PointId(points[1])))
    {
      let kind = if (b[0] - a[0]).abs() >= (b[1] - a[1]).abs() { "horizontal" } else { "vertical" };
      jobs.push((kind, json!({"sketch": sk.0, "type": kind, "points": points})));
    }
    for c in &curves {
      let ends = sketch.curve_points(fox_engine::sketch::CurveId(*c));
      if let Some(curve) = sketch.curve(fox_engine::sketch::CurveId(*c))
        && matches!(curve.kind, fox_engine::sketch::CurveKind::Line { .. })
        && let (Some(a), Some(b)) =
          (ends.first().and_then(|p| sketch.pos(*p)), ends.get(1).and_then(|p| sketch.pos(*p)))
      {
        let kind = if (b[0] - a[0]).abs() >= (b[1] - a[1]).abs() { "horizontal" } else { "vertical" };
        jobs.push((kind, json!({"sketch": sk.0, "type": kind, "curves": [c]})));
      }
    }
    if jobs.is_empty() {
      app.hint("Horizontal/Vertical: select lines or two points");
      return;
    }
    let mut any = false;
    for (_, params) in jobs {
      any |= app.run("sketch.constrain", params).is_some();
    }
    if any {
      app.s.selection.clear();
    }
    return;
  }
  // Fix/UnFix: geometry that's already fixed is unfixed
  if kind == "fix" && unfix(app, sk, &points, &curves) {
    return;
  }
  // H/V on several lines: one constraint each
  if kind == "fix" || (matches!(kind, "horizontal" | "vertical") && points.len() != 2) {
    let mut any = false;
    if kind == "fix" {
      for p in &points {
        any |= app.run("sketch.constrain", json!({"sketch": sk.0, "type": kind, "points": [p]})).is_some();
      }
      if !curves.is_empty() {
        any |= app.run("sketch.constrain", json!({"sketch": sk.0, "type": kind, "curves": curves})).is_some();
      }
    } else {
      for c in &curves {
        any |= app.run("sketch.constrain", json!({"sketch": sk.0, "type": kind, "curves": [c]})).is_some();
      }
    }
    if any {
      app.s.selection.clear();
    } else if curves.is_empty() && points.is_empty() {
      app.hint(format!("Select geometry, then apply {kind}"));
    }
    return;
  }
  if app.run("sketch.constrain", json!({"sketch": sk.0, "type": kind, "points": points, "curves": curves})).is_some() {
    app.s.selection.clear();
  }
}

fn feature_icon(k: &FeatureKind) -> Icon {
  use fox_engine::model::Primitive;
  match k {
    FeatureKind::Sketch { .. } => Icon::Sketch,
    FeatureKind::Extrude { .. } => Icon::Extrude,
    FeatureKind::Revolve { .. } => Icon::Revolve,
    FeatureKind::Fillet { .. } => Icon::Fillet,
    FeatureKind::OffsetFaces { .. } => Icon::Extrude,
    FeatureKind::SplitByBody { .. } => Icon::Combine,
    FeatureKind::Chamfer { .. } => Icon::Chamfer,
    FeatureKind::Shell { .. } => Icon::Shell,
    FeatureKind::Primitive { shape: Primitive::Box { .. }, .. } => Icon::Box,
    FeatureKind::Primitive { shape: Primitive::Cylinder { .. }, .. } => Icon::Cylinder,
    FeatureKind::Primitive { shape: Primitive::Sphere { .. }, .. } => Icon::Sphere,
    FeatureKind::Primitive { shape: Primitive::Torus { .. }, .. } => Icon::Torus,
    FeatureKind::ConstructionPlane { .. } => Icon::Plane,
    FeatureKind::Mirror { .. } => Icon::Mirror,
    FeatureKind::RectangularPattern { .. } => Icon::Pattern,
    FeatureKind::CircularPattern { .. } => Icon::CircPattern,
    FeatureKind::PathPattern { .. } => Icon::Pattern,
    FeatureKind::Move { .. } => Icon::Move,
    FeatureKind::Combine { .. } => Icon::Combine,
    FeatureKind::Import { .. } => Icon::Import,
    FeatureKind::RemoveBodies { .. } => Icon::Remove,
    FeatureKind::Flange { .. }
    | FeatureKind::ContourFlange { .. }
    | FeatureKind::EdgeFlange { .. }
    | FeatureKind::Hem { .. }
    | FeatureKind::Bend { .. } => Icon::Extrude,
    FeatureKind::ConvertToSheetMetal { .. }
    | FeatureKind::Unfold { .. }
    | FeatureKind::Refold { .. }
    | FeatureKind::Rip { .. } => Icon::Shell,
    FeatureKind::Loft { .. } | FeatureKind::Sweep { .. } => Icon::Extrude,
    FeatureKind::Hole { .. } => Icon::Cylinder,
    FeatureKind::SplitBody { .. } | FeatureKind::Joint { .. } | FeatureKind::CopyComponent { .. } => Icon::Combine,
    FeatureKind::Draft { .. } => Icon::Chamfer,
    FeatureKind::PlaneBy { .. } => Icon::PlaneAlt,
    FeatureKind::ConstructionAxis { .. } => Icon::Axis,
    FeatureKind::ConstructionPoint { .. } => Icon::CPoint,
    FeatureKind::Thread { .. } => Icon::Thread,
    FeatureKind::Coil { .. } => Icon::Coil,
    FeatureKind::Pipe { .. } => Icon::Pipe,
    FeatureKind::Rib { web: true, .. } => Icon::Web,
    FeatureKind::Rib { .. } => Icon::Rib,
    FeatureKind::Scale { .. } => Icon::Scale,
    FeatureKind::SplitFace { .. } => Icon::SplitFace,
    FeatureKind::RemoveFaces { .. } => Icon::RemoveBody,
    FeatureKind::Thicken { .. } => Icon::Thicken,
    FeatureKind::SurfaceExtrude { .. } => Icon::Extrude,
    FeatureKind::SurfaceRevolve { .. } => Icon::Revolve,
    FeatureKind::Patch { .. } => Icon::Patch,
    FeatureKind::Stitch { .. } => Icon::Stitch,
    FeatureKind::Unstitch { .. } => Icon::Unstitch,
    FeatureKind::ReverseNormal { .. } => Icon::ReverseNormal,
    FeatureKind::Ruled { .. } => Icon::Ruled,
    FeatureKind::SurfaceOffset { .. } => Icon::SurfOffset,
    FeatureKind::Trim { .. } => Icon::Trim,
    FeatureKind::Mesh { op, .. } => match op {
      fox_engine::model::construct::MeshOp::Reduce => Icon::Reduce,
      fox_engine::model::construct::MeshOp::Smooth => Icon::Smooth,
      fox_engine::model::construct::MeshOp::Convert => Icon::ConvertMesh,
    },
    FeatureKind::Emboss { .. } => Icon::Emboss,
    FeatureKind::BoundaryFill { .. } => Icon::BoundaryFill,
    FeatureKind::ReplaceFace { .. } => Icon::ReplaceFace,
    FeatureKind::SurfaceExtend { .. } => Icon::Extend,
    FeatureKind::Untrim { .. } => Icon::Untrim,
    FeatureKind::TrimWith { .. } => Icon::Trim,
    FeatureKind::LoftedFlange { .. } => Icon::LoftedFlange,
    FeatureKind::MiterFlange { .. } => Icon::Flange,
    FeatureKind::JoinByBend { .. } => Icon::Fold,
    FeatureKind::SheetFillet { .. } => Icon::Fillet,
    FeatureKind::SheetChamfer { .. } => Icon::Chamfer,
    FeatureKind::JointOrigin { .. } => Icon::JointOrigin,
    FeatureKind::RigidGroup { .. } => Icon::Component,
    FeatureKind::MotionLink { .. } => Icon::DriveJoint,
    FeatureKind::InsertDesign { .. } => Icon::InsertComponent,
    FeatureKind::BaseFeature { .. } => Icon::Box,
    FeatureKind::CamFollower { .. } => Icon::DriveJoint,
    FeatureKind::Form { .. } => Icon::CreateForm,
    FeatureKind::AlignConstraint { .. } => Icon::JointOrigin,
    FeatureKind::PlaneConstraint { .. } => Icon::Combine,
    FeatureKind::BoundingSolid { .. } => Icon::Box,
    FeatureKind::ProjectToSurface { .. } => Icon::ProjectSurface,
    FeatureKind::Arrange { .. } => Icon::Component,
    FeatureKind::Pcb { .. } => Icon::Component,
    FeatureKind::FullRoundFillet { .. } => Icon::Fillet,
    FeatureKind::IntersectionCurve { .. } => Icon::SplineFit,
    FeatureKind::Canvas { .. } => Icon::Canvas,
  }
}

/// Double-click in the timeline: sketches open for editing, other features get their dialog.
pub fn edit_feature(app: &mut App, id: FeatureId) {
  let Some(f) = app.s.design.feature(id) else { return };
  match &f.kind {
    FeatureKind::Sketch { .. } => app.edit_sketch(id),
    FeatureKind::Canvas { .. } => crate::canvas_ui::start_edit(app, id),
    FeatureKind::Form { .. } => crate::form_ui::enter(app, Some(id)),
    _ => {
      app.cancel_dialog();
      app.dialog = crate::dialogs::Dialog::edit(app, id);
      if app.dialog.is_none() {
        app.status = Some(("This feature has no editable properties yet".into(), false));
      }
    }
  }
}

fn rename_popup(app: &mut App, ctx: &egui::Context) {
  let Some((target, mut text)) = app.rename.take() else { return };
  let mut keep = true;
  egui::Window::new("Rename")
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
    .show(ctx, |ui| {
      let r = ui.text_edit_singleline(&mut text);
      r.request_focus();
      let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
      ui.horizontal(|ui| {
        if ui.button("OK").clicked() || enter {
          match target {
            RenameTarget::Feature(f) => {
              app.run("feature.rename", json!({"feature": f, "name": text}));
            }
            RenameTarget::Body(b) => {
              app.run("body.rename", json!({"body": b, "name": text}));
            }
            RenameTarget::Component(c) => {
              app.run("component.rename", json!({"component": c, "name": text}));
            }
            RenameTarget::Group(g) => {
              app.run("timeline.group_rename", json!({"group": g, "name": text}));
            }
          }
          keep = false;
        }
        if ui.button("Cancel").clicked() {
          keep = false;
        }
      });
    });
  if keep {
    app.rename = Some((target, text));
  }
}

pub fn toggle_sel(s: &mut Session, item: Sel, add: bool) {
  if add {
    if let Some(i) = s.selection.iter().position(|x| *x == item) {
      s.selection.remove(i);
    } else {
      s.selection.push(item);
    }
  } else {
    s.selection = vec![item];
  }
}

mod browser_assembly;
mod browser_canvas;
mod browser_config;
mod browser_edit;
mod timeline_groups;

#[cfg(test)]
mod row_menu_tests {
  use super::*;

  fn labels(items: &[Option<RowItem>]) -> Vec<String> {
    items.iter().map(|i| i.as_ref().map(|i| i.label.clone()).unwrap_or_else(|| "-".into())).collect()
  }

  fn sketched() -> (App, FeatureId) {
    let mut app = App::with_session(Session::default());
    app.create_sketch(json!("xy"));
    let sk = app.s.edit_sketch.expect("sketching");
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 5]}));
    app.finish_sketch();
    (app, sk)
  }

  #[test]
  fn sketch_row_menu_is_fusions() {
    let (mut app, sk) = sketched();
    let l = labels(&sketch_menu(&app, sk));
    assert_eq!(
      l,
      [
        "Edit Sketch",
        "Redefine Sketch Plane",
        "Slice Sketch",
        "-",
        "Hide Profile",
        "Show Dimensions",
        "Look At",
        "Save As DXF",
        "-",
        "Rename",
        "Delete",
        "-",
        "Show/Hide",
        "Find in Window",
      ]
    );
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::ProfileVisibility);
    assert_eq!(app.hidden_profiles, vec![sk]);
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::DimensionVisibility);
    assert_eq!(app.shown_dims, vec![sk]);
    let l = labels(&sketch_menu(&app, sk));
    assert!(l.contains(&"Show Profile".into()) && l.contains(&"Hide Dimensions".into()), "{l:?}");
    assert!(fox_engine::scene::sketch_visible(&app.s, sk));
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::ShowHide);
    assert!(!fox_engine::scene::sketch_visible(&app.s, sk));
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::ShowHide);
    assert!(fox_engine::scene::sketch_visible(&app.s, sk));
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::FindInWindow);
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::LookAt);
    assert!(app.status.as_ref().is_none_or(|(_, err)| !err), "{:?}", app.status);
    run_sketch_cmd(&mut app, sk, "Sketch1", RowCmd::EditSketch);
    assert_eq!(app.s.edit_sketch, Some(sk));
  }

  #[test]
  fn origin_plane_menu_is_fusions() {
    let mut app = App::with_session(Session::default());
    let l = labels(&origin_plane_menu(&app, "xz"));
    assert_eq!(l, ["Create Sketch", "Offset Plane", "-", "Show/Hide", "Look At", "Find in Window"]);
    run_origin_plane_cmd(&mut app, "xz", RowCmd::ShowHide);
    assert_eq!(app.shown_planes, vec![json!("xz")]);
    run_origin_plane_cmd(&mut app, "xz", RowCmd::ShowHide);
    assert!(app.shown_planes.is_empty());
    run_origin_plane_cmd(&mut app, "xz", RowCmd::LookAt);
    run_origin_plane_cmd(&mut app, "xz", RowCmd::FindInWindow);
    run_origin_plane_cmd(&mut app, "xz", RowCmd::OffsetPlane);
    assert!(app.dialog.as_ref().is_some_and(|d| d.kind == Kind::OffsetPlane));
    run_origin_plane_cmd(&mut app, "yz", RowCmd::CreateSketch);
    assert!(app.s.edit_sketch.is_some());
  }
}

#[cfg(test)]
mod chrome_tests {
  use super::*;
  use fox_engine::Session;

  #[test]
  fn the_file_menu_lists_fusions_entries_and_greys_the_cloud_ones() {
    let labels: Vec<&str> = FILE_MENU.iter().map(|(l, ..)| *l).collect();
    assert_eq!(
      labels,
      [
        "New...",
        "Add To Assembly",
        "Open...",
        "Open Recovered Documents (0)",
        "Upload...",
        "Save",
        "Save As",
        "Save As Latest",
        "Export...",
        "Create Version",
        "3D Print",
        "Capture Image...",
        "Share File Link",
        "View Details on Web",
        "View",
      ]
    );
    let greyed: Vec<&str> = FILE_MENU.iter().filter(|(_, _, on)| !on).map(|(l, ..)| *l).collect();
    assert_eq!(
      greyed,
      [
        "Add To Assembly",
        "Open Recovered Documents (0)",
        "Save As Latest",
        "Create Version",
        "Share File Link",
        "View Details on Web"
      ]
    );
    let keys: Vec<(&str, &str)> =
      FILE_MENU.iter().filter(|(_, k, _)| !k.is_empty()).map(|(l, k, _)| (*l, *k)).collect();
    assert_eq!(keys, [("Open...", "Ctrl+O"), ("Save", "Ctrl+S")]);
    // the new entries open their dialogs
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    file_menu_action(&mut app, "3D Print");
    assert!(app.capture.print.is_some());
    file_menu_action(&mut app, "Capture Image...");
    assert!(app.capture.image.is_some());
    file_menu_action(&mut app, "Save");
    assert!(app.data.save.is_some(), "Save opens the save form");
  }

  #[test]
  fn undo_steps_carry_their_commands_icons() {
    assert_eq!(step_icon("Fillet"), Icon::Fillet);
    assert_eq!(step_icon("Extrude"), Icon::Extrude);
    assert_eq!(step_icon("Create Sketch"), Icon::Sketch);
    assert_eq!(step_icon("Finish Sketch"), Icon::Finish);
    assert_eq!(step_icon("2-Point Rectangle"), Icon::Rect);
    assert_eq!(step_icon("Line"), Icon::Line);
    assert_eq!(step_icon("Edit Fillet2"), Icon::Fillet);
    assert_eq!(step_icon("Suppress Extrude1"), Icon::Extrude);
    assert_eq!(step_icon("Delete Extrude1"), Icon::Remove);
    assert_eq!(step_icon("Change Units"), Icon::Gear);
    assert_eq!(step_icon("Add Parameter w"), Icon::Params);
    assert_eq!(step_icon("Move Timeline Marker"), Icon::History);
    // the list the Undo drop-down shows, as Fusion names it
    let mut app = App::with_session(Session::default());
    app.create_sketch(json!("xy"));
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 5]}));
    app.finish_sketch();
    app.run("extrude", json!({"distance": 5}));
    let names = app.s.undo_history();
    assert_eq!(names, ["Extrude", "Finish Sketch", "2-Point Rectangle", "Create Sketch"]);
    let icons: Vec<Icon> = names.iter().map(|n| step_icon(n)).collect();
    assert_eq!(icons, [Icon::Extrude, Icon::Finish, Icon::Rect, Icon::Sketch]);
  }
}
