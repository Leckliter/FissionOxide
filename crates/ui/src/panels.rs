//! The Fusion-style workspace chrome: application bar with document tabs, the toolbar
//! (workspace switcher, tabs and tool groups), the browser, comments, navigation bar and
//! sketch palette floating over the canvas, the timeline, and the parameters window.

use egui::{Color32, Pos2, Rect, RichText, Stroke, Vec2, pos2, vec2};
use fox_engine::model::{BodyId, FeatureId, FeatureKind};
use fox_engine::render::StandardView;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use crate::bar_icons::{self, Glyph};
use crate::dialogs::Kind;
use crate::icons::{self, Icon};
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
  /// An Assembly Design's tab (in place of SOLID, SURFACE, ...).
  Assembly,
  /// The DIRECT MESH EDITING contextual tab (MESH > Direct Edit, Create Base Mesh Feature).
  MeshEdit,
  /// The Animation workspace's tab.
  Animation,
  /// The Manufacture workspace's additive (3D printing) tab.
  Additive,
  /// The Manufacture workspace's inspection (probing) tab.
  Inspection,
  /// The Manufacture workspace's fabrication (cutting) tab.
  Fabrication,
  /// The Manufacture workspace's UTILITIES tab (not the Design workspace's).
  CamUtilities,
}

impl Tab {
  const MAIN: [Tab; 7] =
    [Tab::Solid, Tab::Surface, Tab::Mesh, Tab::SheetMetal, Tab::Plastic, Tab::Manage, Tab::Utilities];

  /// The Manufacture workspace's tabs, in Fusion's order.
  pub(crate) const MANUFACTURE: [Tab; 6] =
    [Tab::Milling, Tab::Turning, Tab::Additive, Tab::Inspection, Tab::Fabrication, Tab::CamUtilities];

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
      Tab::Assembly => "ASSEMBLY",
      Tab::MeshEdit => "DIRECT MESH EDITING",
      Tab::Animation => "ANIMATION",
      Tab::Additive => "ADDITIVE",
      Tab::Inspection => "INSPECTION",
      Tab::Fabrication => "FABRICATION",
      Tab::CamUtilities => "UTILITIES",
    }
  }
}

const WORKSPACES: [&str; 8] =
  ["Design", "Generative Design", "Render", "Animation", "Simulation", "Manufacture", "Drawing", "Electronics"];

// ---------------------------------------------------------------- application bar

fn bar_icon(ui: &mut egui::Ui, g: Glyph, hint: &str, enabled: bool) -> egui::Response {
  bar_icons::button(ui, g, BAR_BUTTON, hint, enabled)
}

/// The application bar's height, as Fusion's.
pub(crate) const APP_BAR_H: f32 = 34.0;
/// A bar button's square.
const BAR_BUTTON: f32 = 26.0;
/// The account icons at the bar's right: Plugins, Assistant, Job Status, Notifications and Help
/// (4 px apart) and the 28 px avatar; and the least room kept between them and the "+".
const BAR_RIGHT_W: f32 = 5.0 * BAR_BUTTON + 5.0 * 4.0 + 28.0;
const BAR_RIGHT_GAP: f32 = 8.0;

/// Fusion's application bar: Home, the Data Panel, File, Save, Undo and Redo as solid grey
/// glyphs on the grey bar, the document tabs (the shown one lighter, joined to the toolbar), the
/// "+", and the account icons at the right. The fox stays in the window's title bar, where
/// Fusion's own logo is.
pub fn app_bar(app: &mut App, ui: &mut egui::Ui) {
  egui::Panel::top("app_bar")
    .exact_size(APP_BAR_H)
    .frame(egui::Frame::NONE.fill(theme::APP_BAR).inner_margin(egui::Margin { left: 8, right: 8, top: 0, bottom: 0 }))
    .show(ui, |ui| {
      ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        // the house opens the Home view, as Fusion's first icon
        if bar_icon(ui, Glyph::Home, crate::data_panel::HOME_TIP, true).clicked() {
          crate::home::open(app);
        }
        ui.add_space(4.0);
        if bar_icon(ui, Glyph::DataPanel, crate::data_panel::BUTTON_TIP, true).clicked() {
          app.data.toggle();
        }
        ui.add_space(4.0);
        let file = caret_button(ui, Glyph::File, "File", true);
        egui::Popup::menu(&file).show(|ui| file_menu(app, ui));
        ui.add_space(2.0);
        if bar_icon(ui, Glyph::Save, "Save (Ctrl+S)", true).clicked() {
          app.save(false);
        }
        ui.add_space(4.0);
        // the icon undoes one step; the caret lists the steps to jump back (or forward) to
        let undo = caret_button(ui, Glyph::Undo, "Undo (Ctrl+Z)", app.s.can_undo());
        let undo_list = caret_hit(&undo);
        if undo.clicked() && !undo_list && app.s.can_undo() {
          app.cancel_dialog();
          app.s.undo();
        }
        egui::Popup::menu(&undo)
          .open_memory(undo_list.then_some(egui::SetOpenCommand::Toggle))
          .show(|ui| history_menu(app, ui, true));
        let redo = caret_button(ui, Glyph::Redo, "Redo (Ctrl+Y)", app.s.can_redo());
        let redo_list = caret_hit(&redo);
        if redo.clicked() && !redo_list && app.s.can_redo() {
          app.s.redo();
        }
        egui::Popup::menu(&redo)
          .open_memory(redo_list.then_some(egui::SetOpenCommand::Toggle))
          .show(|ui| history_menu(app, ui, false));
        ui.add_space(14.0);
        // the document tabs fill the middle of the bar up to the "+" (and its gap) and the
        // account icons
        let tab_w = (ui.available_width() - BAR_RIGHT_W - BAR_RIGHT_GAP - 32.0).max(crate::documents::TAB_MIN_W);
        crate::documents::tab_strip(app, ui, tab_w);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          ui.spacing_mut().item_spacing.x = 4.0;
          avatar(app, ui);
          let help = bar_icon(ui, Glyph::Help, "Help", true);
          egui::Popup::menu(&help).show(|ui| crate::help::menu(app, ui));
          // the bell opens the Notifications panel under it; the dot shows while there is news
          let bell = bar_icon(ui, Glyph::Bell, "Notifications", true);
          if bell.clicked() {
            crate::notifications::toggle(app);
          }
          app.notifications.anchor = bell.rect;
          if crate::notifications::unread(app) {
            ui.painter().circle_filled(bell.rect.center() + vec2(6.0, -7.0), 3.5, theme::ICON_ORANGE);
          }
          // Job Status opens its panel under it; Check for Updates reports there, with a badge
          let jobs = bar_icon(ui, Glyph::JobStatus, crate::job_status::TIP, true);
          if jobs.clicked() {
            crate::job_status::toggle(app);
          }
          app.jobs.anchor = jobs.rect;
          crate::job_status::paint_badge(app, ui.painter(), jobs.rect);
          if bar_icon(ui, Glyph::Assistant, "Autodesk Assistant", true).clicked() {
            crate::help::assistant(app);
          }
          let plugins = bar_icon(ui, Glyph::Plugins, "Plugins", true);
          if app.plugins.open {
            ui.painter().rect_stroke(plugins.rect, 3.0, Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Inside);
          }
          if plugins.clicked() {
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
fn caret_button(ui: &mut egui::Ui, g: Glyph, hint: &str, enabled: bool) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(34.0, BAR_BUTTON), egui::Sense::click());
  let bg = bar_icons::hover_fill(ui, rect, &resp, enabled);
  let ink = if enabled { bar_icons::INK } else { bar_icons::INK_OFF };
  let ir = Rect::from_center_size(rect.center() - vec2(6.0, 0.0), Vec2::splat(bar_icons::GLYPH));
  bar_icons::draw(ui.painter(), ir, g, ink, bg);
  icons::chevron(ui.painter(), rect.right_center() - vec2(7.0, 0.0), ink);
  let resp = resp.on_hover_text(hint);
  if enabled { resp } else { resp.on_hover_cursor(egui::CursorIcon::NotAllowed) }
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

/// The File menu entries Fusion draws a separator above: Save, Create Version, 3D Print and View
/// Details on Web.
pub(crate) const FILE_MENU_BREAKS: [usize; 4] = [5, 9, 10, 13];

fn file_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(240.0);
  for (i, (label, key, enabled)) in FILE_MENU.iter().enumerate() {
    if FILE_MENU_BREAKS.contains(&i) {
      ui.separator();
    }
    if *label == "View" {
      ui.menu_button("View", |ui| crate::file_menu::view_menu(app, ui));
      continue;
    }
    let enabled = crate::file_menu::enabled(app, label, *enabled);
    let mut b = egui::Button::new(*label).min_size(vec2(220.0, 0.0));
    if !key.is_empty() {
      b = b.shortcut_text(*key);
    }
    if crate::file_menu::tip(ui.add_enabled(enabled, b), label).clicked() {
      ui.close();
      file_menu_action(app, label);
    }
  }
}

/// Runs a File menu entry.
pub(crate) fn file_menu_action(app: &mut App, label: &str) {
  match label {
    "New..." => crate::documents::open_chooser(app),
    "Open..." => app.open_dialog(),
    "Upload..." => crate::data_panel::upload_dialog(app),
    "Save" => app.save(false),
    "Save As" => app.save(true),
    "Export..." => crate::file_dialogs::open_export(app),
    "3D Print" => crate::capture::open_print(app),
    "Capture Image..." => crate::capture::open_image(app),
    _ => {}
  }
}

/// The profile button. Its menu has Fusion's entries: Autodesk Account, Preferences, My Profile,
/// Share Feedback, Subscribe Now and Sign Out; those that need an Autodesk account are greyed.
fn avatar(app: &mut App, ui: &mut egui::Ui) {
  let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), egui::Sense::click());
  ui.painter().circle_filled(rect.center(), 12.0, Color32::from_rgb(92, 84, 196));
  ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "CL", egui::FontId::proportional(11.0), Color32::WHITE);
  let resp = resp.on_hover_text(user_name());
  egui::Popup::menu(&resp).show(|ui| profile_menu(app, ui));
}

/// Why the profile menu's account entries are greyed.
pub(crate) const NO_ACCOUNT: &str = "FissionOxide has no Autodesk account: it works without signing in";

/// Where Share Feedback goes: the project's issue tracker.
pub(crate) const FEEDBACK_PAGE: &str = "https://github.com/leckliter/FissionOxide/issues";

/// The profile menu's entries in Fusion's order, `None` for a separator; `false` entries are
/// greyed.
pub(crate) const PROFILE_MENU: [Option<(&str, bool)>; 8] = [
  Some(("Autodesk Account", false)),
  Some(("Preferences", true)),
  Some(("My Profile", false)),
  Some(("Share Feedback", true)),
  None,
  Some(("Subscribe Now", false)),
  None,
  Some(("Sign Out", false)),
];

/// The profile menu's width: Fusion's is a narrow list under the avatar (about 108 screenshot px,
/// 142 points).
pub(crate) const PROFILE_MENU_W: f32 = 142.0;

/// The id under which the profile menu's rect is kept, for tests.
pub(crate) fn profile_menu_rect_id() -> egui::Id {
  egui::Id::new("profile-menu-rect")
}

fn profile_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_width(PROFILE_MENU_W);
  for entry in PROFILE_MENU {
    let Some((label, enabled)) = entry else {
      ui.separator();
      continue;
    };
    let r = if label == "Sign Out" {
      // Sign Out carries an icon after its name, as in Fusion
      ui.add_enabled_ui(enabled, |ui| {
        let r = ui.add(egui::Button::new(label).frame(false).min_size(vec2(ui.available_width(), 0.0)));
        let at = egui::Rect::from_center_size(r.rect.right_center() - vec2(12.0, 0.0), vec2(14.0, 14.0));
        icons::draw(ui.painter(), at, Icon::SignOut, if enabled { theme::ICON_INK } else { theme::MUTED });
        r
      })
      .inner
    } else {
      ui.add_enabled(enabled, egui::Button::new(label).frame(false).min_size(vec2(ui.available_width(), 0.0)))
    };
    let r = r.on_disabled_hover_text(NO_ACCOUNT);
    if r.clicked() {
      ui.close();
      profile_action(app, label);
    }
  }
  ui.ctx().data_mut(|d| d.insert_temp(profile_menu_rect_id(), ui.min_rect()));
}

/// Runs a profile menu entry.
pub(crate) fn profile_action(app: &mut App, label: &str) {
  match label {
    "Preferences" => crate::preferences::open(app),
    "Share Feedback" => {
      if let Err(e) = fox_desktop::open(FEEDBACK_PAGE) {
        app.status = Some((format!("Couldn't open {FEEDBACK_PAGE}: {e}"), true));
      }
    }
    _ => {}
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
  Select,
  SketchTool(Tool),
  Construction,
  Constraint(&'static str),
  FinishSketch,
  /// Remove the selected bodies (a timeline feature, unlike Delete).
  RemoveBodies,
  /// EXPORT > 3D Print (and UTILITIES > MAKE): File > 3D Print's docked 3D PRINT dialog.
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
  SelectInvert,
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
  /// MODIFY > Manage Materials: the Material Browser window.
  ManageMaterials,
  /// INSPECT > Properties: show or hide the docked Properties panel.
  Properties,
  /// INSPECT > Display Component Colors: bodies and timeline icons in their component's colour.
  ComponentColors,
  /// A Form workspace command (see [`crate::form_ui`]).
  Form(crate::form_ui::Act),
  /// Generative Design: study, preserve, obstacle, start, load, constraint, criteria,
  /// precheck, generate, explore.
  Gen(&'static str),
  /// INSERT > Sketch Library: pick a part, then click in the sketch to place it.
  SketchLibrary,
  /// The Render workspace's commands.
  Render(crate::render_ui::RenderCmd),
  /// The Animation workspace's commands.
  Anim(crate::anim_ui::AnimCmd),
  /// Fabrication: check a cut file for a cutting service, with the cut estimate.
  CutCheck,
  /// SHEET METAL > MODIFY > Sheet Metal Rules: the SHEET METAL RULES palette.
  SheetRules,
  /// SELECT > Selection Tools: Select By Name, Select By Boundary, Select By Size, Seed And
  /// Boundary (each opens a small dialog).
  SelTool(crate::select::SelTool),
  /// Create Base Feature (true), a mode rather than a dialog, and Finish Base Feature (false).
  BaseFeature(bool),
  /// INSERT > Decal, Insert SVG, Insert Mesh, Insert Derive: Fusion's Insert (Select Source)
  /// window.
  Insert(crate::insert_window::Target),
  /// INSERT > Insert DXF: the docked INSERT DXF dialog.
  InsertDxf,
  /// MESH > Direct Edit, Create Base Mesh Feature and Finish Direct Editing (see
  /// [`crate::mesh_ui`]).
  MeshEdit(crate::mesh_ui::Act),
  /// The flat pattern environment's Finish and Export, and the REFOLD panel's Refold.
  Flat(crate::flat_ui::FlatAct),
}

impl Cmd {
  pub(crate) fn info(self) -> (Icon, &'static str, &'static str) {
    match self {
      Cmd::NewSketch => (Icon::Sketch, "Create Sketch", "Pick a plane or planar face to sketch on"),
      Cmd::Form(a) => crate::form_ui::info(a),
      Cmd::MeshEdit(a) => crate::mesh_ui::info(a),
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
      Cmd::Select => (Icon::Select, "Select", "Select faces, edges, bodies and profiles"),
      Cmd::SketchTool(t) => match t {
        Tool::Line => (Icon::Line, "Line", "Line (L): click points, Esc to finish"),
        Tool::Rect => (Icon::Rect, "2-Point Rectangle", "Rectangle from two corners (R)"),
        Tool::CenterRect => (Icon::CenterRect, "Center Rectangle", "Rectangle from centre and corner"),
        Tool::Circle => (Icon::Circle, "Center Diameter Circle", "Circle from centre and a point (C)"),
        Tool::Arc3 => (Icon::Arc3, "3-Point Arc", "Arc through start, end and a point"),
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
        Tool::Fillet => (Icon::SkFillet, "Fillet", "Round the corner where two lines or arcs meet"),
        Tool::Chamfer => (Icon::SkChamfer, "Equal Distance Chamfer", "Bevel a corner by one distance"),
        Tool::ChamferAngle => {
          (Icon::SkChamferAngle, "Distance and Angle Chamfer", "Bevel a corner by a distance and an angle")
        }
        Tool::Chamfer2 => (Icon::SkChamfer2, "Two Distance Chamfer", "Bevel a corner by two distances"),
        Tool::Constrain(k) => {
          let (icon, name) = constraint_info(k);
          (icon, name, "Pick sketch objects to constrain")
        }
      },
      Cmd::Construction => (Icon::Construction, "Construction", "Toggle construction linetype (X)"),
      Cmd::Constraint(kind) => {
        let (icon, name) = constraint_info(kind);
        (icon, name, "Pick sketch objects to constrain (a selection applies at once)")
      }
      Cmd::FinishSketch => (Icon::Finish, "Finish Sketch", "Finish the sketch and return to the model"),
      Cmd::Undo => (Icon::Undo, "Undo", "Undo (Ctrl+Z)"),
      Cmd::Redo => (Icon::Redo, "Redo", "Redo (Ctrl+Y)"),
      Cmd::Delete => (Icon::Remove, "Delete", "Delete the selection (Del)"),
      Cmd::RemoveBodies => (Icon::RemoveBody, "Remove", "Remove the selected bodies (kept in the timeline)"),
      Cmd::Print3d => (Icon::Print3d, "3D Print", "Prepare bodies for 3D printing (the 3D PRINT dialog)"),
      Cmd::LookAt => (Icon::LookAt, "Look At", "Look at the sketch plane or a selected planar face"),
      Cmd::Fit => (Icon::Fit, "Fit", "Fit the design in the view (F6)"),
      Cmd::Home => (Icon::Views, "Home View", "Go to the home view"),
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
      Cmd::SelectInvert => (Icon::Select, "Invert Selection", "Select what isn't selected"),
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
        "all" => (Icon::Select, "Select All", "Include or exclude every object type"),
        "through" => (Icon::Eye, "Select Through", "Select hidden objects during a window or freeform selection"),
        "bodies" => (Icon::Body, "Bodies", "Allow selecting bodies"),
        "faces" => (Icon::SplitFace, "Body Faces", "Allow selecting faces"),
        "edges" => (Icon::Line, "Body Edges", "Allow selecting edges"),
        "sketches" | "sketch_curves" => (Icon::SketchSmall, "Sketch Curves", "Allow selecting sketch curves"),
        "sketch_points" => (Icon::Point, "Sketch Points", "Allow selecting sketch points"),
        "vertices" => (Icon::Point, "Body Vertices", "Show body vertices and allow selecting them"),
        "construction" => {
          (Icon::Plane, "Construction Geometry", "Allow selecting construction planes, axes and points")
        }
        "components" => (Icon::Component, "Components", "Allow selecting components"),
        "profiles" => (Icon::Rect, "Sketch Profiles", "Allow selecting sketch profiles"),
        other => (
          Icon::Select,
          crate::select::FILTER_TYPES.iter().find(|(k, _)| *k == other).map_or("Filter", |(_, n)| *n),
          "Allow selecting this object type",
        ),
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
      Cmd::ManageMaterials => {
        (Icon::ManageMaterials, "Manage Materials", "Browse the physical material and appearance libraries")
      }
      Cmd::Properties => (
        Icon::Properties,
        "Properties",
        "A panel with the type, geometry and overall size of the selection (or of the whole design)",
      ),
      Cmd::ComponentColors => (
        Icon::ComponentColors,
        "Display Component Colors",
        "Show each component's bodies and timeline features in the component's colour",
      ),
      Cmd::SketchLibrary => {
        (Icon::SketchLibrary, "Sketch Library...", "Place DXF, SVG or .fox library parts where you click in the sketch")
      }
      Cmd::Render(r) => r.info(),
      Cmd::Anim(a) => a.info(),
      Cmd::Insert(t) => crate::insert_window::info(t),
      Cmd::InsertDxf => (Icon::InsertDxf, "Insert DXF", "Insert a 2D DXF file into a sketch"),
      Cmd::SheetRules => (Icon::SmRules, "Sheet Metal Rules", "Thickness, bend radius and reliefs of sheet metal"),
      Cmd::CutCheck => (
        Icon::Interference,
        "Cut Service Check",
        "Check a sketch, flat pattern or DXF for SendCutSend, OSH Cut, Ponoko or your shop, with a cut estimate",
      ),
      Cmd::SelTool(t) => {
        use crate::select::SelTool;
        let hint = match t {
          SelTool::ByName => "Select bodies and components by name, with * and ? wildcards",
          SelTool::ByBoundary => "Select the faces inside a closed chain of edges",
          SelTool::BySize => "Select bodies or faces up to a size",
          SelTool::SeedAndBoundary => "Select the faces connected to a seed face, up to boundary edges",
        };
        (Icon::Select, t.name(), hint)
      }
      Cmd::BaseFeature(true) => (
        Icon::BaseFeature,
        "Create Base Feature",
        "Start a base feature: direct edits collect in it until you finish it",
      ),
      Cmd::BaseFeature(false) => {
        (Icon::Finish, "Finish Base Feature", "Finish editing the base feature and go back to the parametric tabs")
      }
      Cmd::Flat(a) => crate::flat_ui::info(a),
    }
  }

  pub(crate) fn active(self, app: &App) -> bool {
    match self {
      Cmd::SketchTool(t) => app.tool == t,
      Cmd::Constraint(k) => app.tool == Tool::Constrain(k),
      Cmd::Select => false,
      Cmd::Params => app.show_params,
      Cmd::Dialog(k) => app.dialog.as_ref().is_some_and(|d| d.kind == k),
      // Create Sketch stays pressed while it waits for a plane (it has no dialog)
      Cmd::NewSketch => app.dialog.as_ref().is_some_and(|d| d.kind == Kind::PickPlane),
      Cmd::Priority(p) => app.sel_priority == p,
      Cmd::Filter(f) => app.sel_filter.get(f),
      Cmd::WindowSelect => app.sel_mode == crate::select::Mode::Window,
      Cmd::SelectMode(m) => app.sel_mode == m,
      Cmd::Properties => crate::props_ui::is_open(),
      Cmd::ComponentColors => app.ctx_menu.component_colors,
      Cmd::Render(r) => r.active(app),
      Cmd::Anim(a) => a.active(app),
      _ => false,
    }
  }

  /// Commands that the marking menu's Repeat can run again.
  pub(crate) fn repeatable(self) -> bool {
    match self {
      // a constraint is a command too: Fusion's Repeat names the one just used
      Cmd::NewSketch | Cmd::Dialog(_) | Cmd::Constraint(_) => true,
      // Insert Mesh (and the other inserts) too: right after one the ring's Repeat names it
      Cmd::Insert(_) => true,
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
      // an Assembly Design's New Component makes an external design
      Cmd::Dialog(Kind::NewComponent) if crate::asm_component::applies(app) => crate::asm_component::open(app),
      Cmd::MeshEdit(a) => crate::mesh_ui::run(app, a),
      Cmd::Dialog(k) => app.start(k),
      Cmd::Params => app.show_params = !app.show_params,
      Cmd::Select => {
        app.nav_mode = None;
        app.set_tool(Tool::Select);
      }
      Cmd::SketchTool(t) => {
        app.nav_mode = None;
        app.set_tool(t);
      }
      Cmd::Construction => app.toggle_construction(),
      Cmd::Constraint(kind) => crate::constrain::start(app, kind),
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
      Cmd::Print3d => crate::capture::open_print(app),
      Cmd::RemoveBodies => {
        let bodies: Vec<u32> =
          app.s.selection.iter().filter_map(|x| if let Sel::Body { body } = x { Some(body.0) } else { None }).collect();
        if bodies.is_empty() {
          // Fusion opens REMOVE to pick them
          app.start(Kind::Form(&crate::forms::REMOVE));
        } else {
          // a component picked whole goes as a component (its browser row goes too)
          app.run("body.remove", crate::dialogs::remove_params(&app.s, &bodies));
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
      Cmd::SelectInvert => {
        app.run("select.invert", json!({}));
      }
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
      // Fusion's Insert window first; the picked image opens the CANVAS form
      Cmd::Canvas => crate::insert_window::open(app, crate::insert_window::Target::Canvas),
      Cmd::Plugins => crate::plugins_ui::open_window(app),
      Cmd::ManageMaterials => crate::material_ui::open_browser(app),
      Cmd::Properties => crate::props_ui::toggle(),
      Cmd::ComponentColors => app.ctx_menu.component_colors = !app.ctx_menu.component_colors,
      Cmd::SketchLibrary => crate::library_ui::open(app),
      Cmd::Render(r) => r.run(app),
      Cmd::Anim(a) => a.run(app),
      Cmd::CutCheck => crate::service_ui::open(app),
      Cmd::SheetRules => crate::sheet_rules_ui::open_palette(app),
      Cmd::SelTool(t) => crate::select::open_tool(app, t),
      Cmd::BaseFeature(start) => crate::config_ui::base_mode(app, start),
      Cmd::Insert(target) => crate::insert_window::open(app, target),
      Cmd::InsertDxf => crate::insert_window::open_dxf(app),
      Cmd::Flat(a) => crate::flat_ui::run(app, a),
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
    // back to the tab the sketch was started from, as in Fusion
    app.tab = app.pre_sketch_tab.take().filter(|t| !matches!(t, Tab::Sketch | Tab::Form)).unwrap_or(Tab::Solid);
  }
  if !app.form.active && app.tab == Tab::Form {
    app.tab = Tab::Solid;
  }
  crate::config_ui::check_base_mode(app);
  if !app.mesh_edit.active && app.tab == Tab::MeshEdit {
    app.tab = Tab::Mesh;
  }
  crate::flat_ui::check(app);
  egui::Panel::top("toolbar")
    .exact_size(96.0)
    .frame(egui::Frame::NONE.fill(theme::TOOLBAR).inner_margin(egui::Margin { left: 6, right: 6, top: 0, bottom: 0 }))
    .show(ui, |ui| {
      let full = ui.max_rect();
      ui.painter().line_segment([full.left_bottom(), full.right_bottom()], Stroke::new(1.0, theme::BORDER));
      ui.horizontal(|ui| {
        // an electronics document has no workspace dropdown, as in Fusion
        if app.electronics.is_none() {
          workspace_switcher(app, ui);
          ui.add_space(6.0);
        }
        ui.vertical(|ui| {
          ui.spacing_mut().item_spacing.y = 0.0;
          if app.electronics.is_some() {
            crate::ecad_chrome::toolbar(app, ui);
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

/// The workspace the window shows, as the switcher names it.
pub(crate) fn workspace_label(app: &App) -> &'static str {
  if app.electronics.is_some() {
    "ELECTRONICS"
  } else if app.drawing.is_some() {
    "DRAWING"
  } else if app.generative {
    "GENERATIVE"
  } else if app.render.active {
    "RENDER"
  } else if app.anim.active {
    "ANIMATION"
  } else if app.manufacture {
    "MANUFACTURE"
  } else if app.simulation.is_some() {
    "SIMULATION"
  } else {
    "DESIGN"
  }
}

/// The switcher's label size: every workspace's name at DESIGN's, as Fusion's.
pub(crate) const SWITCHER_FONT: f32 = 13.5;
/// The ▾'s half-width and its gap after the name.
const SWITCHER_CARET: f32 = 3.5;
pub(crate) const SWITCHER_CARET_GAP: f32 = 6.0;

/// The switcher's width for a name `text_w` wide, and the x of the name's left edge and of the
/// ▾'s centre from the button's left: Fusion's 92 px button, growing for a name too long for it,
/// with the name and the caret a few px after it centred together.
pub(crate) fn switcher_layout(text_w: f32) -> (f32, f32, f32) {
  let group = text_w + SWITCHER_CARET_GAP + 2.0 * SWITCHER_CARET;
  let width = (group + 24.0).max(92.0);
  let left = (width - group) / 2.0;
  (width, left, left + text_w + SWITCHER_CARET_GAP + SWITCHER_CARET)
}

fn workspace_switcher(app: &mut App, ui: &mut egui::Ui) {
  ui.vertical(|ui| {
    ui.add_space(9.0);
    let label = workspace_label(app);
    let font = egui::FontId::proportional(SWITCHER_FONT);
    let ink = theme::SWITCHER_INK;
    let tw = ui.painter().layout_no_wrap(label.into(), font.clone(), ink).size().x;
    let (width, text_x, caret_x) = switcher_layout(tw);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 76.0), egui::Sense::click());
    let fill = if resp.hovered() { theme::HOVER } else { theme::SWITCHER };
    let p = ui.painter();
    p.rect(rect, 4.0, fill, Stroke::new(1.0, theme::SWITCHER_LINE), egui::StrokeKind::Inside);
    let at = pos2(rect.left() + text_x, rect.center().y);
    p.text(at, egui::Align2::LEFT_CENTER, label, font.clone(), ink);
    // the font has no semi-bold face: a second pass a little over gives the weight
    p.text(at + vec2(0.4, 0.0), egui::Align2::LEFT_CENTER, label, font, ink);
    icons::chevron(p, pos2(rect.left() + caret_x, rect.center().y), ink);
    let resp = resp.on_hover_text("Change Workspace");
    egui::Popup::menu(&resp).show(|ui| workspace_menu(app, ui));
  });
}

/// The workspace menu's rows: Fusion's, taller than other menus' and bold.
pub(crate) const WORKSPACE_ROW_H: f32 = 35.0;
const WORKSPACE_FONT: f32 = 12.0;
/// The room a row keeps right of its name for the subscription badge or the ▸.
const WORKSPACE_LOCK_W: f32 = 15.0;
const WORKSPACE_ARROW_W: f32 = 14.0;
/// The subscription badge's radius, and its centre's inset from the list's right edge (the
/// column the ▸ sits in).
const WORKSPACE_BADGE_R: f32 = 5.0;
const WORKSPACE_BADGE_INSET: f32 = 9.0;
pub(crate) const WORKSPACE_BADGE: Color32 = Color32::from_rgb(0, 150, 214);
/// The name's inset from the row's left edge, and the margin past the widest row's end.
const WORKSPACE_PAD: f32 = 8.0;
/// Resting on a workspace row this long opens its tooltip, as in Fusion.
pub(crate) const WORKSPACE_TIP_SECS: f64 = 1.5;
/// The tooltip's width and the height of its picture.
const WORKSPACE_TIP_W: f32 = 280.0;
const WORKSPACE_TIP_PICTURE_H: f32 = 140.0;

/// Fusion's workspace list: bold upper-case names in a list as wide as the longest, the
/// workspaces that need a subscription greyed with a blue badge at the list's right edge, DRAWING ▸ and
/// ELECTRONICS ▸ submenus (the ▸ at the list's right edge) for the documents they create, and no
/// fill on the current workspace.
fn workspace_menu(app: &mut App, ui: &mut egui::Ui) {
  const SUB_WIDTH: f32 = 200.0;
  ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
  let font = egui::FontId::proportional(WORKSPACE_FONT);
  let width = WORKSPACES
    .iter()
    .map(|w| {
      let tw = ui.painter().layout_no_wrap(w.to_uppercase(), font.clone(), theme::TEXT).size().x;
      tw + match workspace_trail(w) {
        Trail::Lock => WORKSPACE_LOCK_W,
        Trail::Arrow => WORKSPACE_ARROW_W,
        Trail::None => 0.0,
      }
    })
    .fold(0.0, f32::max)
    + 2.0 * WORKSPACE_PAD;
  ui.set_min_width(width);
  for w in WORKSPACES {
    let sub: &[&str] = match w {
      "Drawing" => &["From Design", "From Animation"],
      "Electronics" => &["New Electronics Design", "New Electronics Library"],
      _ => &[],
    };
    let r = workspace_row(ui, width, &w.to_uppercase(), workspace_trail(w), true, true);
    if sub.is_empty() {
      let r = workspace_tip(r, w);
      if r.clicked() {
        ui.close();
        switch_workspace(app, w);
      }
      continue;
    }
    egui::containers::menu::SubMenu::new().show(ui, &r, |ui| {
      ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
      ui.set_min_width(SUB_WIDTH);
      for item in sub {
        let why = workspace_document_unavailable(item);
        let r = workspace_row(ui, SUB_WIDTH, item, Trail::None, false, why.is_none());
        let r = match why {
          Some(why) => r.on_hover_text(why),
          None => r,
        };
        if r.clicked() && why.is_none() {
          ui.close();
          open_workspace_document(app, w, item);
        }
      }
    });
  }
}

/// What a workspace row shows after its name.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Trail {
  None,
  /// The workspace needs a subscription: the name greyed, a blue badge after it.
  Lock,
  /// The row opens a submenu.
  Arrow,
}

fn workspace_trail(w: &str) -> Trail {
  match w {
    "Generative Design" | "Simulation" => Trail::Lock,
    "Drawing" | "Electronics" => Trail::Arrow,
    _ => Trail::None,
  }
}

/// What a workspace submenu entry FissionOxide can't open yet says (the row is greyed).
pub(crate) fn workspace_document_unavailable(item: &str) -> Option<&'static str> {
  match item {
    "New Electronics Library" => crate::documents::NewKind::ElectronicsLibrary.unavailable(),
    _ => None,
  }
}

/// One row of the workspace menu (`top`: bold and Fusion's taller row) or of its submenus: the
/// name, then a blue subscription badge just after it (the name greyed) or a ▸ at the row's
/// end; greyed when not `enabled`.
fn workspace_row(ui: &mut egui::Ui, width: f32, label: &str, trail: Trail, top: bool, enabled: bool) -> egui::Response {
  let h = if top { WORKSPACE_ROW_H } else { 24.0 };
  let (rect, resp) = ui.allocate_exact_size(vec2(width, h), egui::Sense::click());
  let p = ui.painter();
  if resp.hovered() && enabled {
    p.rect_filled(rect, 0.0, theme::HOVER);
  }
  let font = egui::FontId::proportional(if top { WORKSPACE_FONT } else { 12.5 });
  let at = rect.left_center() + vec2(WORKSPACE_PAD, 0.0);
  let ink = if enabled && trail != Trail::Lock { theme::TEXT } else { theme::MUTED };
  p.text(at, egui::Align2::LEFT_CENTER, label, font.clone(), ink);
  if top {
    // the font has no bold face: a second pass a little over gives the weight
    p.text(at + vec2(0.8, 0.0), egui::Align2::LEFT_CENTER, label, font, ink);
  }
  let ink = Color32::from_gray(80);
  match trail {
    Trail::Lock => {
      // Fusion's subscription badge: a blue disc holding a white up arrow, right-aligned at the
      // list's edge so every badge sits in one column
      let c = pos2(rect.right() - WORKSPACE_BADGE_INSET, rect.center().y);
      p.circle_filled(c, WORKSPACE_BADGE_R, WORKSPACE_BADGE);
      let white = Color32::WHITE;
      p.add(egui::Shape::convex_polygon(
        vec![c + vec2(0.0, -3.4), c + vec2(2.9, -0.4), c + vec2(-2.9, -0.4)],
        white,
        Stroke::NONE,
      ));
      p.rect_filled(egui::Rect::from_min_max(c + vec2(-1.0, -0.6), c + vec2(1.0, 3.2)), 0.0, white);
    }
    Trail::Arrow => {
      let c = pos2(rect.right() - 8.0, rect.center().y);
      p.add(egui::Shape::convex_polygon(
        vec![c + vec2(-3.0, -5.0), c + vec2(4.0, 0.0), c + vec2(-3.0, 5.0)],
        ink,
        Stroke::NONE,
      ));
    }
    Trail::None => {}
  }
  resp
}

/// A workspace row's tooltip, as Fusion's: after resting on the row for
/// [`WORKSPACE_TIP_SECS`], a dark panel right of the menu, level with the row, with the bold
/// title "<Name> Workspace", a line saying what the workspace does, and a picture of it.
fn workspace_tip(resp: egui::Response, w: &str) -> egui::Response {
  let Some(line) = crate::tips::workspace(w) else { return resp };
  let rested = crate::ribbon_tip::rested(&resp);
  if !resp.hovered() {
    return resp;
  }
  if rested < WORKSPACE_TIP_SECS {
    resp.ctx.request_repaint_after(std::time::Duration::from_secs_f64(WORKSPACE_TIP_SECS - rested));
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
  tooltip.popup =
    tooltip.popup.anchor(anchor).align(egui::RectAlign::RIGHT_START).gap(6.0).width(WORKSPACE_TIP_W).frame(frame);
  tooltip.show(|ui| {
    ui.set_max_width(WORKSPACE_TIP_W);
    ui.spacing_mut().item_spacing.y = 10.0;
    let title = format!("{w} Workspace");
    let font = egui::TextStyle::Body.resolve(ui.style());
    let r = ui.label(RichText::new(&title).font(font.clone()).color(theme::TOOLTIP_TITLE)).rect;
    // the font has no bold face: a second pass half a pixel over gives the weight
    ui.painter().text(r.left_top() + vec2(0.6, 0.0), egui::Align2::LEFT_TOP, &title, font, theme::TOOLTIP_TITLE);
    ui.add(egui::Label::new(RichText::new(line).color(theme::TOOLTIP_TEXT)).wrap());
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), WORKSPACE_TIP_PICTURE_H), egui::Sense::hover());
    ui.painter().rect(
      r,
      2.0,
      theme::ILLUSTRATION,
      Stroke::new(1.0, theme::ILLUSTRATION_FRAME),
      egui::StrokeKind::Inside,
    );
    // until pictures of each workspace exist, a command typical of it painted large
    let icon = match w {
      "Generative Design" => Icon::CreateForm,
      "Render" => Icon::Appearance,
      "Animation" => Icon::Play,
      "Simulation" => Icon::CurvMap,
      "Manufacture" => Icon::Print3d,
      _ => Icon::Extrude,
    };
    let side = WORKSPACE_TIP_PICTURE_H - 30.0;
    icons::draw_tinted(ui.painter(), Rect::from_center_size(r.center(), Vec2::splat(side)), icon, icons::Tint::Blue);
  });
  resp
}

/// A DRAWING ▸ or ELECTRONICS ▸ submenu entry: opens that workspace on the document it names, or
/// the new document in its own tab.
pub(crate) fn open_workspace_document(app: &mut App, workspace: &str, item: &str) {
  match (workspace, item) {
    ("Drawing", "From Design") => switch_workspace(app, "Drawing"),
    // a new document tab with the electronics design's start page; the open design stays
    ("Electronics", "New Electronics Design") => crate::ecad_chrome::new_electronics_design(app, None),
    _ => app.status = Some((format!("{item} isn't in FissionOxide yet"), false)),
  }
}

/// Switches to the Design, Render, Manufacture or Drawing workspace.
pub(crate) fn switch_workspace(app: &mut App, w: &str) {
  let drawing = app.drawing.is_some();
  if w == "Render" && !app.render.active {
    crate::render_ui::enter(app);
  }
  // a docked TEXTURE MAP CONTROLS (opened from a body menu or the SETUP tab) stays behind
  if app.render.active != (w == "Render") {
    app.render.texture_map = None;
  }
  app.render.active = w == "Render";
  app.anim.active = w == "Animation";
  if !app.anim.active {
    app.anim.dialog = None;
    app.anim.pending = None;
    app.anim.playing = None;
  } else if app.anim.view_mark.is_none() {
    // View records the camera's moves from here until the playhead moves
    app.anim.view_mark = Some(app.cam);
  }
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
    "Render" | "Animation" => {
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
    } else if app.anim.active {
      tabs = vec![Tab::Animation];
      app.tab = Tab::Animation;
    } else if app.generative {
      tabs = vec![Tab::Generative];
      app.tab = Tab::Generative;
    } else if app.manufacture {
      tabs = Tab::MANUFACTURE.to_vec();
      if !Tab::MANUFACTURE.contains(&app.tab) {
        app.tab = Tab::Milling;
      }
    } else if Tab::MANUFACTURE.contains(&app.tab)
      || matches!(app.tab, Tab::Drawing | Tab::Generative | Tab::Render | Tab::Animation)
    {
      app.tab = Tab::Solid;
    }
    // an Assembly Design has the ASSEMBLY tab in place of the modelling tabs, as in Fusion
    let assembly = app.s.design.design_type == fox_engine::model::DesignType::Assembly;
    if assembly && tabs == Tab::MAIN {
      tabs = vec![Tab::Assembly, Tab::Manage, Tab::Utilities];
    }
    if assembly && matches!(app.tab, Tab::Solid | Tab::Surface | Tab::Mesh | Tab::SheetMetal | Tab::Plastic) {
      app.tab = Tab::Assembly;
    } else if !assembly && app.tab == Tab::Assembly {
      app.tab = Tab::Solid;
    }
    if app.drawing.is_none() && !app.manufacture && !app.render.active && !app.anim.active {
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
    if app.base_mode || app.flat_mode {
      // Fusion: BASE FEATURE (or FLAT PATTERN) SOLID and SURFACE replace the modelling tabs
      tabs.retain(|t| matches!(t, Tab::Solid | Tab::Surface | Tab::Sketch));
      if !matches!(app.tab, Tab::Solid | Tab::Surface | Tab::Sketch) {
        app.tab = Tab::Solid;
      }
    }
    if app.mesh_edit.active {
      tabs.push(Tab::MeshEdit);
    }
    for t in tabs {
      let active = app.tab == t;
      let label = match t {
        Tab::Plugin(i) => crate::plugins_ui::tab_label(app, i),
        Tab::Solid | Tab::Surface if app.base_mode => format!("BASE FEATURE {}", t.label()),
        Tab::Solid | Tab::Surface if app.flat_mode => crate::flat_ui::tab_label(t),
        _ => t.label().to_string(),
      };
      if ribbon_tab(ui, &label, active).clicked() {
        app.tab = t;
      }
    }
  });
}

/// One toolbar tab: every tab's name is Fusion's regular text grey; the active one (the
/// contextual SKETCH tab too) is underlined in the same grey.
pub(crate) fn ribbon_tab(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
  let font = egui::FontId::proportional(12.5);
  let color = if active { theme::TAB_ACTIVE } else { theme::TEXT };
  let tw = ui.painter().layout_no_wrap(label.to_string(), font.clone(), color).size().x;
  let (rect, resp) = ui.allocate_exact_size(vec2(tw + 26.0, 28.0), egui::Sense::click());
  let pill = rect.shrink2(vec2(0.0, 3.0));
  if resp.hovered() && !active {
    ui.painter().rect_filled(pill, 2.0, theme::HOVER);
  }
  ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, font, color);
  if active {
    ui.painter().line_segment(
      [
        pos2(rect.center().x - tw / 2.0 - 4.0, rect.bottom() - 2.0),
        pos2(rect.center().x + tw / 2.0 + 4.0, rect.bottom() - 2.0),
      ],
      Stroke::new(2.0, theme::TAB_ACTIVE),
    );
  }
  resp
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

/// Browser expander, as Fusion's: a thin chevron, ">" when closed and "v" when open (the root
/// component's too).
fn tree_triangle(p: &egui::Painter, c: Pos2, open: bool, _root: bool, color: Color32) {
  let s = Stroke::new(1.0, color);
  let pts = if open {
    vec![c + vec2(-4.0, -2.0), c + vec2(0.0, 2.0), c + vec2(4.0, -2.0)]
  } else {
    vec![c + vec2(-2.0, -4.0), c + vec2(2.0, 0.0), c + vec2(-2.0, 4.0)]
  };
  p.add(egui::Shape::line(pts, s));
}

/// What happened to a browser row.
#[derive(Default)]
pub(crate) struct RowEvents {
  pub(crate) clicked: bool,
  pub(crate) double_clicked: bool,
  pub(crate) eye: bool,
  pub(crate) response: Option<egui::Response>,
}

/// One browser row, drawn straight on the canvas as in Fusion: expander, visibility eye, icon
/// and the name on a light grey pill (bold, with an activation radio, for the root component).
pub(crate) fn tree_row(
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
    let er = eye_rect(rect, depth);
    let r = ui.interact(er, id.with("eye"), egui::Sense::click());
    icons::draw_color(&p, er, if visible { Icon::Eye } else { Icon::EyeOff });
    if r.on_hover_text(if visible { "Hide" } else { "Show" }).clicked() {
      ev.eye = true;
    }
  }
  let font = egui::FontId::proportional(13.0);
  let text_color = label_color;
  let galley = p.layout_no_wrap(label.to_string(), font, text_color);
  let icon_x = x + 44.0;
  let pill = Rect::from_min_size(pos2(icon_x - 11.0, cy - 10.5), vec2(galley.size().x + 36.0, 21.0));
  let (fill, stroke) = if selected {
    (theme::SELECTED, Stroke::new(1.0, theme::ACCENT))
  } else if resp.hovered() {
    (theme::HOVER, Stroke::new(1.0, theme::SELECTED))
  } else {
    // every row's pill is Fusion's light grey, the root component's too
    (theme::PILL, Stroke::NONE)
  };
  p.rect(pill, 2.0, fill, stroke, egui::StrokeKind::Inside);
  icons::draw_color(&p, Rect::from_center_size(pos2(icon_x, cy), Vec2::splat(16.0)), icon);
  let at = pos2(icon_x + 13.0, cy - galley.size().y / 2.0);
  if root {
    // the root component's name is bold, as in Fusion: the font has no bold face, so a second
    // pass half a pixel over gives the weight
    p.galley_with_override_text_color(at + vec2(0.5, 0.0), galley.clone(), text_color);
  }
  p.galley_with_override_text_color(at, galley, text_color);
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

/// Where a row's eye sits: right of the expander, at the row's depth.
pub(super) fn eye_rect(row: Rect, depth: usize) -> Rect {
  let x = row.min.x + 8.0 + depth as f32 * 16.0;
  Rect::from_center_size(pos2(x + 24.0, row.center().y), vec2(18.0, 14.0))
}

/// A folder row's eye, as Fusion draws it: struck through while the folder is hidden; while the
/// folder shows but some of its rows are hidden, drawn (open) only while the pointer is on the
/// folder's row, which [`tree_row`] is about to lay out next.
pub(super) fn folder_eye(ui: &egui::Ui, shown: bool, children_hidden: bool) -> Option<bool> {
  if shown && children_hidden {
    let next = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 25.0));
    ui.rect_contains_pointer(next).then_some(true)
  } else {
    Some(shown)
  }
}

fn is_open(ui: &egui::Ui, id: egui::Id, default: bool) -> bool {
  ui.data(|d| d.get_temp::<bool>(id.with("open"))).unwrap_or(default)
}

pub fn browser(app: &mut App, ctx: &egui::Context) {
  let canvas = app.canvas;
  // the rows set it again while the pointer is on one; the canvas, drawn before the browser,
  // shows it next frame
  let row_hover = app.row_hover.take();
  browser_area(app, ctx, canvas);
  if app.row_hover != row_hover {
    ctx.request_repaint();
  }
}

fn browser_area(app: &mut App, ctx: &egui::Context, canvas: egui::Rect) {
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
      // the expanded COMMENTS panel below takes its room from the browser, as in Fusion
      let max_h = browser_max_height(app);
      egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
        ui.add_space(4.0);
        browser_tree(app, ui);
        app.ctx_menu.reveal = None;
        ui.add_space(4.0);
      });
    },
  );
}

/// The browser tree's greatest height: the canvas less the room of the COMMENTS panel below it.
pub(crate) fn browser_max_height(app: &App) -> f32 {
  (app.canvas.height() - 90.0 - crate::comments::extra_height(app)).max(120.0)
}

fn browser_tree(app: &mut App, ui: &mut egui::Ui) {
  if app.flat_mode {
    browser_flat::env_tree(app, ui);
    return;
  }
  if app.anim.active {
    animation_tree(app, ui);
    return;
  }
  let root = egui::Id::new("tree-root");
  // Find in Browser: open the folders down to the node (its row scrolls to itself)
  if let Some(sel) = app.ctx_menu.reveal {
    let folder = match sel {
      Sel::Body { .. } => Some("tree-bodies"),
      Sel::Feature { feature } => app.s.design.feature(feature).and_then(|f| match f.kind {
        FeatureKind::Sketch { .. } => Some("tree-sketches"),
        FeatureKind::ConstructionPlane { .. }
        | FeatureKind::PlaneBy { .. }
        | FeatureKind::ConstructionAxis { .. }
        | FeatureKind::ConstructionPoint { .. } => Some("tree-construction"),
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
  // the Manufacture workspace's CAM Root: Units, Named Views, Origin, Models and Setups
  if app.manufacture {
    if !browser_cam::root(app, ui) {
      return;
    }
  } else {
    // the root row: "(Unsaved)" until saved, an eye for the whole component, Fusion's menu
    let root_label = browser_rows::root_label(app);
    let root_visible = browser_rows::root_visible(app);
    let root_icon = crate::sheet_rules_ui::root_icon(app);
    let ev = tree_row(ui, root, 0, Some(true), Some(root_visible), root_icon, &root_label, false, theme::TEXT);
    if ev.eye {
      browser_rows::root_show_hide(app);
    }
    browser_edit::drop_target(app, ui, &ev, browser_edit::DropOn::Root);
    if let Some(r) = ev.response {
      if r.hovered() {
        marking::peek(app, Vec::new(), root_label.clone());
      }
      r.context_menu(|ui| browser_rows::root_context_menu(app, ui));
    }
    if !is_open(ui, root, true) {
      return;
    }
    // Document Settings: expanded to start, with the units and the design type, as in Fusion
    let ds = egui::Id::new("tree-docsettings");
    let ev = tree_row(ui, ds, 1, Some(true), None, Icon::Gear, "Document Settings", false, theme::TEXT);
    if ev.response.is_some_and(|r| r.hovered()) {
      marking::peek(app, Vec::new(), "Document Settings".into());
    }
    if is_open(ui, ds, true) {
      // while Change Active Units is open the row is highlighted with its pencil, as in Fusion;
      // hovering it shows no pencil
      let label = crate::units::row_label(app.s.design.units);
      let units_open = crate::doc_dialogs::units_open(app);
      let ev = tree_row(ui, ds.with(1), 2, None, None, Icon::Measure, &label, units_open, theme::TEXT);
      if let Some(r) = &ev.response {
        let id = egui::Id::new("units-pencil");
        crate::doc_dialogs::row_pencil(ui, r, 2, &label, id, false, units_open, "Change Active Units");
        if r.hovered() {
          marking::peek(app, Vec::new(), label.clone());
        }
        r.context_menu(|ui| {
          browser_rows::folder_context_menu(ui, &browser_rows::units_menu(), |cmd| {
            browser_rows::run_units_cmd(app, cmd)
          });
        });
      }
      // the design type (Part Design, Assembly Design, ...): a pencil on hover and a right-click
      // menu, both Convert Design Type
      let convert_open = crate::doc_dialogs::convert_open(app);
      let type_label = app.s.design.design_type.label();
      let ev = tree_row(ui, ds.with(2), 2, None, None, Icon::Component, type_label, convert_open, theme::TEXT);
      if let Some(r) = &ev.response {
        let id = egui::Id::new("design-type-pencil");
        if crate::doc_dialogs::row_pencil(ui, r, 2, type_label, id, true, convert_open, "Convert Design Type") {
          crate::doc_dialogs::open_convert(app);
        }
        if r.hovered() {
          marking::peek(app, Vec::new(), type_label.into());
        }
        r.context_menu(|ui| {
          browser_rows::folder_context_menu(ui, &browser_rows::design_type_menu(), |cmd| {
            browser_rows::run_design_type_cmd(app, cmd)
          });
        });
      }
    }
    // the sheet metal rule, once the design has sheet metal
    crate::sheet_rules_ui::browser_row(app, ui);
    // the sheet metal flat pattern, under the Rule row
    browser_flat::row(app, ui);
  }
  // Named Views
  let nv = egui::Id::new("tree-views");
  let ev = tree_row(ui, nv, 1, Some(false), None, Icon::Folder, "Named Views", false, theme::TEXT);
  if let Some(r) = ev.response {
    if r.hovered() {
      marking::peek(app, Vec::new(), "Named Views".into());
    }
    r.context_menu(|ui| {
      if menu_item(ui, Icon::Views, "New Named View", None).clicked() {
        ui.close();
        new_named_view(app, ui.ctx());
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
      let ev = tree_row(ui, nv.with(label), 2, None, None, Icon::NamedView, label, false, theme::TEXT);
      if ev.clicked || ev.double_clicked {
        app.fly_standard_view(v);
      }
    }
    // the user's views, saved in the design
    let names: Vec<String> = app.s.design.named_views.iter().map(|v| v.name.clone()).collect();
    for name in names {
      let row = named_view_row(&name);
      let ev = tree_row(ui, row, 2, None, None, Icon::NamedView, &name, false, theme::TEXT);
      browser_edit::edit_box(app, ui, &ev, row, 2);
      if ev.double_clicked {
        app.restore_named_view(&name);
      }
      if let Some(r) = ev.response {
        r.on_hover_text("Double-click to restore this view").context_menu(|ui| named_view_menu(app, ui, &name));
      }
    }
  }
  // the Render workspace's Lights, above Origin
  crate::light_ui::browser(app, ui);
  // Origin
  let og = egui::Id::new("tree-origin");
  // a plane (or origin item) picked here stays picked until something else is selected
  if !app.s.selection.is_empty() {
    crate::origin::clear(app);
  }
  // the folder's light bulb shows the origin planes and short axes in the canvas (off by default)
  let ev = tree_row(ui, og, 1, Some(false), Some(app.origin_shown), Icon::Folder, "Origin", false, theme::TEXT);
  if ev.eye {
    browser_rows::origin_show_hide(app);
  }
  if let Some(r) = ev.response {
    if r.hovered() {
      marking::peek(app, Vec::new(), "Origin".into());
    }
    r.context_menu(|ui| {
      let items = browser_rows::origin_folder_menu(app);
      browser_rows::folder_context_menu(ui, &items, |cmd| browser_rows::run_origin_folder_cmd(app, cmd));
    });
  }
  if is_open(ui, og, false) {
    browser_rows::origin_children(app, ui, og);
  }
  if app.manufacture {
    browser_cam::models(app, ui);
    browser_cam::setups(app, ui);
    return;
  }
  browser_analysis::section(app, ui);
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

/// The Animation workspace's browser, as Fusion's: the design as an assembly root holding one
/// Components folder with the components; bodies, sketches and construction are not listed.
fn animation_tree(app: &mut App, ui: &mut egui::Ui) {
  let root = egui::Id::new("anim-tree-root");
  let label = if browser_rows::unsaved(app) { doc_name(app) } else { root_label(app) };
  let visible = browser_rows::root_visible(app);
  let ev = tree_row(ui, root, 0, Some(true), Some(visible), Icon::Assembly, &label, false, theme::TEXT);
  if ev.eye {
    browser_rows::root_show_hide(app);
  }
  if !is_open(ui, root, true) {
    return;
  }
  let folder = egui::Id::new("anim-tree-components");
  tree_row(ui, folder, 1, Some(false), None, Icon::Folder, "Components", false, theme::TEXT);
  if !is_open(ui, folder, false) {
    return;
  }
  for (cid, depth) in browser_assembly::component_tree(app) {
    let Some(c) = app.s.design.component(fox_engine::model::ComponentId(cid)) else { continue };
    let (name, bodies) = (c.name.clone(), c.bodies.clone());
    let selected = !bodies.is_empty() && bodies.iter().all(|b| app.s.selection.contains(&Sel::Body { body: *b }));
    let ev = tree_row(ui, folder.with(cid), 2 + depth, None, None, Icon::Component, &name, selected, theme::TEXT);
    if ev.clicked {
      app.s.selection = bodies.iter().map(|b| Sel::Body { body: *b }).collect();
    }
  }
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

/// A user named view's browser row.
fn named_view_row(name: &str) -> egui::Id {
  egui::Id::new("tree-views").with(("user", name))
}

/// Named Views > New Named View, as Fusion's: saves the view as "NamedView", opens the folder
/// and puts the new row's name in an inline editor, selected, ready to retype (Escape keeps it).
pub(crate) fn new_named_view(app: &mut App, ctx: &egui::Context) {
  let Some(name) = app.save_named_view() else { return };
  ctx.data_mut(|d| d.insert_temp(egui::Id::new("tree-views").with("open"), true));
  if let Some(i) = app.s.design.named_views.iter().position(|v| v.name == name) {
    browser_edit::begin_in(ctx, named_view_row(&name), RenameTarget::NamedView(i as u32), &name);
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
    // (an empty component REMOVE picked is lit too)
    let selected = (!bodies.is_empty() && bodies.iter().all(|b| app.s.selection.contains(&Sel::Body { body: *b })))
      || app.dialog.as_ref().is_some_and(|d| d.removes_component(cid));
    let row = id.with(cid);
    let ev = tree_row(ui, row, 2 + depth, Some(false), None, Icon::Component, &label, selected, theme::TEXT);
    let target = RenameTarget::Component(cid);
    browser_edit::slow_click(ui, &ev, row, target, &name, selected);
    if ev.double_clicked {
      browser_edit::begin(ui, row, target, &name);
    } else if ev.clicked {
      // REMOVE picks an empty component as itself; otherwise the row picks its bodies
      let s = &app.s;
      if !app.dialog.as_mut().is_some_and(|d| d.take_remove_component(s, cid)) {
        app.s.selection = bodies.iter().map(|b| Sel::Body { body: *b }).collect();
      }
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
        let ev = tree_row(ui, brow, 3 + depth, None, None, body_icon(app, b), &name, sel, theme::TEXT);
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
    .filter(|f| matches!(f.kind, FeatureKind::Joint { .. }) && !rolled_back(app, f.id))
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
  // a dialog's preview leaves the rows alone: bodies it adds appear, and bodies it consumes go,
  // on OK
  let rows = app
    .dialog
    .as_ref()
    .and_then(|d| d.browser_bodies(&app.s))
    .unwrap_or_else(|| app.s.model.bodies.iter().map(|b| (b.id, app.s.body_name(b.id))).collect());
  let bodies: Vec<(BodyId, String, bool)> =
    rows.into_iter().map(|(id, name)| (id, name, app.s.body_visible(id))).collect();
  if bodies.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-bodies");
  // the folder's own eye: hiding it leaves each body's eye as it was
  let shown = !app.ctx_menu.bodies_hidden;
  let eye = folder_eye(ui, shown, bodies.iter().any(|b| !b.2));
  // collapsed when the first body adds it, as in Fusion
  let ev = tree_row(ui, id, 1, Some(false), eye, Icon::Folder, "Bodies", false, theme::TEXT);
  browser_edit::drop_target(app, ui, &ev, browser_edit::DropOn::Root);
  if ev.eye {
    app.ctx_menu.bodies_hidden = shown;
    if shown {
      app.s.selection.retain(|s| !matches!(s, Sel::Body { .. } | Sel::Face { .. } | Sel::Edge { .. }));
    }
  }
  if let Some(r) = &ev.response {
    if r.hovered() {
      marking::peek(app, Vec::new(), "Bodies".into());
    }
    r.context_menu(|ui| {
      let items = browser_rows::bodies_folder_menu(app);
      browser_rows::folder_context_menu(ui, &items, |cmd| browser_rows::run_bodies_folder_cmd(app, cmd));
    });
  }
  if !is_open(ui, id, false) {
    return;
  }
  for (bid, name, visible) in bodies {
    let selected = app.s.selection.contains(&Sel::Body { body: bid });
    // while isolated, the bodies left out are hidden from the canvas: grey them, as Fusion does
    let isolated = app.ctx_menu.isolate.as_ref().map(|only| only.contains(&bid));
    let drawn = visible && isolated != Some(false) && shown;
    let color = if drawn { theme::TEXT } else { theme::MUTED };
    let row = id.with(bid.0);
    let ev = tree_row(ui, row, 2, None, Some(visible), body_icon(app, bid), &name, selected, color);
    if isolated == Some(true)
      && let Some(r) = &ev.response
    {
      isolate_badge(ui, r.rect);
    }
    if let Some(r) = &ev.response {
      open_mesh_badge(app, ui, r.rect, 2, bid, &name);
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
      if crate::select::is_mesh(&app.s, bid) {
        r.context_menu(|ui| mesh_rows::mesh_context_menu(app, ui, row, bid, &name));
      } else {
        r.context_menu(|ui| browser_rows::body_context_menu(app, ui, row, bid, &name));
      }
    }
  }
}

/// A body row's icon: Fusion's yellow triangulated block for a mesh body, its orange sheet for
/// a surface body, the grey block for a solid.
pub(crate) fn body_icon(app: &App, body: BodyId) -> Icon {
  if crate::select::is_mesh(&app.s, body) {
    Icon::MeshBody
  } else if app.s.body_is_surface(body) {
    Icon::SurfaceBody
  } else if app.s.model.body(body).is_some_and(|b| b.sheet.is_some()) {
    Icon::SheetBody
  } else {
    Icon::Body
  }
}

/// The hover lines of a body row's warning: Fusion's for a mesh body that isn't closed or has no
/// positive volume; none for a sound body.
pub(crate) fn body_warning(app: &App, body: BodyId) -> Vec<&'static str> {
  app.s.model.body(body).map(|b| b.mesh_problems()).unwrap_or_default()
}

/// Fusion's yellow warning triangle right of a body row's name, its hover the warning's lines.
fn open_mesh_badge(app: &App, ui: &mut egui::Ui, row: Rect, depth: usize, body: BodyId, name: &str) {
  let lines = body_warning(app, body);
  if lines.is_empty() {
    return;
  }
  // where tree_row ends the name's pill
  let text = ui.painter().layout_no_wrap(name.to_string(), egui::FontId::proportional(13.0), theme::TEXT);
  let pill_right = row.min.x + 8.0 + depth as f32 * 16.0 + 33.0 + text.size().x + 36.0;
  let r = Rect::from_center_size(pos2(pill_right + 12.0, row.center().y), Vec2::splat(15.0));
  warning_glyph(ui.painter(), r);
  ui.interact(r, egui::Id::new(("mesh-warning", body.0)), egui::Sense::hover()).on_hover_text(lines.join("\n"));
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
  /// A construction plane row's Edit Construction Plane (the timeline's Edit Feature).
  EditFeature,
  /// Listed, greyed, as Fusion has it but FissionOxide doesn't yet.
  NotYet,
  // the root, Bodies, Origin and Units rows (see browser_rows)
  CreateDrawing,
  Configure,
  PhysicalMaterial,
  Appearance,
  Properties,
  Export,
  SaveCopyAs,
  SaveAsMesh,
  ShowAllComponents,
  ShowAllBodies,
  Opacity,
  DirectModeling,
  ShowAll,
  Selectable,
  ShowHidePlanes,
  ShowHideAxes,
  ChangeUnits,
  // the sketch rows and the Sketches folder (see sketch_rows)
  Extrude,
  SelectSketchPlane,
  ProjectedVisibility,
  ConstructionVisibility,
  FindInTimeline,
  ConvertDesignType,
  /// A sketch row's Redefine Sketch Plane.
  RedefinePlane,
  // the body rows (see browser_rows)
  MoveCopy,
  Copy,
  Remove,
  Isolate,
  // the Flat Pattern row (see browser_flat)
  Activate,
  ExportFlatPattern,
}

/// One row of a browser context menu.
pub(crate) struct RowItem {
  pub icon: Icon,
  pub label: String,
  pub cmd: RowCmd,
  /// The shortcut hint at the right of the row ("E", "Del", "V").
  pub key: Option<&'static str>,
}

fn row_item(icon: Icon, label: &str, cmd: RowCmd) -> Option<RowItem> {
  Some(RowItem { icon, label: label.into(), cmd, key: None })
}

/// A menu row with a shortcut hint, as Fusion's "Extrude  E".
fn row_key(icon: Icon, label: &str, key: &'static str, cmd: RowCmd) -> Option<RowItem> {
  Some(RowItem { icon, label: label.into(), cmd, key: Some(key) })
}

/// An origin plane row's menu ("xy", "xz" or "yz"), as Fusion's.
pub(crate) fn origin_plane_menu(app: &App, plane: &str) -> Vec<Option<RowItem>> {
  let shown = crate::origin::OriginItem::from_sketch_plane(&json!(plane)).is_some_and(|p| crate::origin::eye(app, p));
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
      ui.add_enabled_ui(false, |ui| menu_item(ui, it.icon, &it.label, it.key))
        .inner
        .on_disabled_hover_text("Not in FissionOxide yet");
    } else if menu_item(ui, it.icon, &it.label, it.key).clicked() {
      ui.close();
      picked = Some(it.cmd);
    }
  }
  picked
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
      // the row's own eye (it shows only while the Origin folder's is on too)
      if let Some(item) = crate::origin::OriginItem::from_sketch_plane(&json!(plane)) {
        crate::origin::toggle_eye(app, item);
      }
    }
    RowCmd::LookAt => app.look_at_plane(&pl),
    RowCmd::FindInWindow => marking::find_plane_in_window(app, &pl),
    _ => {}
  }
}

fn sketches_section(app: &mut App, ui: &mut egui::Ui) {
  sketch_rows::sketches_section(app, ui);
}

/// Whether feature `id` is after the timeline marker (a drag or Edit Feature rolled it back):
/// its browser row goes until the marker passes it again, as in Fusion.
pub(crate) fn rolled_back(app: &App, id: FeatureId) -> bool {
  app.s.model.status(id).is_some_and(|s| s.rolled_back)
}

/// The Construction folder, as Fusion's: it appears once a plane, axis or point is made (after
/// OK), collapsed, with an eye that shows or hides everything in it; each row (planes, axes and
/// points in timeline order, "Plane1", "Axis1", "Point1") has its own eye, greyed while the
/// folder's is off. Features after the timeline marker leave it.
fn planes_section(app: &mut App, ui: &mut egui::Ui) {
  let mut items = crate::construction::rows(app);
  items.retain(|(fid, ..)| !rolled_back(app, *fid));
  if items.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-construction");
  // its eye waits for the pointer while a plane's own eye is off, as the Bodies folder's
  let some_hidden = items.iter().any(|(fid, ..)| !crate::construction::eye(app, *fid));
  let eye = folder_eye(ui, app.construction_shown, some_hidden);
  let ev = tree_row(ui, id, 1, Some(false), eye, Icon::Folder, "Construction", false, theme::TEXT);
  if ev.eye {
    app.construction_shown = !app.construction_shown;
  }
  if !is_open(ui, id, false) {
    return;
  }
  for (fid, name, is_plane) in items {
    let icon = app.s.design.feature(fid).map_or(Icon::Plane, |f| feature_icon(&f.kind));
    if !is_plane {
      construction_row(app, ui, id, fid, &name, icon);
      continue;
    }
    let plane = json!({"feature": fid.0});
    let picked = app.picked_plane.as_ref() == Some(&plane) || app.s.selection.contains(&Sel::Feature { feature: fid });
    let row = id.with(fid.0);
    let ev = construction_tree_row(app, ui, row, fid, icon, &name, picked);
    // double-click sketches on the plane, as on the origin planes; a slow click renames it
    let target = RenameTarget::Feature(fid.0);
    browser_edit::slow_click(ui, &ev, row, target, &name, picked && !construct_picking(app));
    if ev.double_clicked {
      // the first click already started it if Create Sketch was waiting for a plane
      if app.s.edit_sketch.is_none() {
        app.create_sketch(plane.clone());
      }
    } else if ev.clicked && (!picked || construct_picking(app)) {
      pick_plane(app, plane.clone());
    }
    browser_edit::slow_click_due(ui, row, app.picked_plane.as_ref() == Some(&plane));
    browser_edit::edit_box(app, ui, &ev, row, 2);
    if let Some(r) = ev.response {
      if r.hovered() {
        app.row_hover = Some(crate::origin::RowHover::Plane(fid));
      }
      reveal_row(app, Sel::Feature { feature: fid }, &r);
      r.on_hover_text("Double-click to sketch on this plane").context_menu(|ui| {
        let items = browser_rows::construction_plane_menu(app, fid);
        match row_menu(ui, &items) {
          // renamed in place in the browser row
          Some(RowCmd::Rename) => browser_edit::begin(ui, row, target, &name),
          Some(cmd) => browser_rows::run_construction_plane_cmd(app, fid, cmd),
          None => {}
        }
      });
    }
  }
}

/// A Construction folder row with its own eye (greyed while the folder's is off).
fn construction_tree_row(
  app: &mut App,
  ui: &mut egui::Ui,
  row: egui::Id,
  fid: FeatureId,
  icon: Icon,
  name: &str,
  picked: bool,
) -> RowEvents {
  let eye = crate::construction::eye(app, fid);
  let ev = tree_row(ui, row, 2, None, Some(eye), icon, name, picked, theme::TEXT);
  if ev.eye {
    crate::construction::toggle_eye(app, fid);
  }
  if let Some(r) = &ev.response
    && !app.construction_shown
  {
    ui.painter().rect_filled(eye_rect(r.rect, 2), 0.0, Color32::from_white_alpha(150));
  }
  ev
}

/// A construction axis's or point's row: a click selects it (or, while a CONSTRUCT dialog is
/// open, adds it to the dialog's picks), a slow click renames it.
fn construction_row(app: &mut App, ui: &mut egui::Ui, folder: egui::Id, fid: FeatureId, name: &str, icon: Icon) {
  let sel = Sel::Feature { feature: fid };
  let picked = app.s.selection.contains(&sel);
  let row = folder.with(fid.0);
  let ev = construction_tree_row(app, ui, row, fid, icon, name, picked);
  let target = RenameTarget::Feature(fid.0);
  browser_edit::slow_click(ui, &ev, row, target, name, picked && !construct_picking(app));
  if ev.clicked {
    let add = construct_picking(app) || ui.input(|i| i.modifiers.command || i.modifiers.shift);
    crate::origin::clear(app);
    toggle_sel(&mut app.s, sel, add);
  }
  browser_edit::slow_click_due(ui, row, app.s.selection.contains(&sel));
  browser_edit::edit_box(app, ui, &ev, row, 2);
  if let Some(r) = ev.response {
    reveal_row(app, sel, &r);
    r.context_menu(|ui| {
      if ui.button("Rename").clicked() {
        ui.close();
        browser_edit::begin(ui, row, target, name);
      }
      ui.separator();
      if ui.button("Delete").clicked() {
        ui.close();
        app.run("feature.delete", json!({"feature": fid.0}));
      }
    });
  }
}

/// Is a CONSTRUCT dialog open, taking the browser's planes, axes and points as picks?
fn construct_picking(app: &App) -> bool {
  app.dialog.as_ref().is_some_and(|d| d.takes_construct_picks())
}

/// A click on a plane in the browser or the canvas: an open CONSTRUCT dialog takes it as a pick,
/// while Create Sketch waits for a plane it starts the sketch there, with Mirror open it is the
/// mirror plane, an open Offset Plane takes it as its plane, else it selects the plane (Create
/// Sketch then starts on it).
pub(crate) fn pick_plane(app: &mut App, plane: Value) {
  // a CONSTRUCT dialog's pick: an origin plane in its own picks, a construction plane in the
  // selection
  if plane.is_string()
    && let Some(d) = app.dialog.as_mut()
    && d.take_origin_pick(plane.clone())
  {
    return;
  }
  // a selection row that takes a plane (Replace Face's Target Faces) picks it from the selection
  if let Some(f) = plane.get("feature").and_then(Value::as_u64)
    && (construct_picking(app) || app.dialog.as_ref().is_some_and(|d| d.row_takes_plane()))
  {
    toggle_sel(&mut app.s, Sel::Feature { feature: FeatureId(f as u32) }, true);
    return;
  }
  if app.dialog.as_ref().is_some_and(|d| d.kind == Kind::PickPlane) {
    // create_sketch closes the pick and runs a sketch key's command in the new sketch
    app.create_sketch(plane);
  } else if let Some(d) = app.dialog.as_mut().filter(|d| d.wants_plane()) {
    d.take_plane(plane);
  } else if app.dialog.as_ref().is_some_and(|d| d.takes_plane()) {
    let label = match plane.get("feature").and_then(Value::as_u64) {
      Some(f) => app.s.design.feature(FeatureId(f as u32)).map(|f| f.name.clone()).unwrap_or_default(),
      None => format!("{} plane", plane.as_str().unwrap_or("xy").to_uppercase()),
    };
    if let Some(d) = app.dialog.as_mut() {
      d.use_plane(&mut app.s, plane, label);
    }
  } else {
    let again = app.picked_plane.as_ref() == Some(&plane);
    app.s.selection.clear();
    crate::origin::clear(app);
    if !again {
      app.picked_plane = Some(plane);
    }
  }
}

/// The COMMENTS panel in the canvas's bottom-left corner (see [`crate::comments`]).
pub fn comments(app: &mut App, ctx: &egui::Context) {
  crate::comments::show(app, ctx);
}

/// Fusion's navigation bar at the bottom centre of the canvas.
pub fn nav_bar(app: &mut App, ctx: &egui::Context) {
  // centred under the whole canvas, all four views with Multiple Views on
  let canvas = crate::viewports::canvas(app);
  egui::Area::new(egui::Id::new("navbar"))
    .fixed_pos(canvas.center_bottom() + vec2(0.0, -8.0))
    .pivot(egui::Align2::CENTER_BOTTOM)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      // Fusion draws the navigation bar as bare icons on the canvas
      ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let [orbit, look_at, pan, zoom, zoom_window, display, grid, viewports] = crate::navbar::BUTTONS;
        let tip = crate::navbar::dark_tip;
        let mode_btn = |ui: &mut egui::Ui, app: &mut App, icon, hint, mode| {
          if tip(icons::icon_button(ui, icon, 28.0, "", app.nav_mode == Some(mode), None), hint).clicked() {
            app.nav_mode = if app.nav_mode == Some(mode) { None } else { Some(mode) };
          }
        };
        mode_btn(ui, app, Icon::Orbit, orbit, NavMode::Orbit);
        let c = nav_caret(ui, "Orbit type");
        egui::Popup::menu(&c).show(|ui| crate::navbar::orbit_menu(app, ui));
        if tip(icons::icon_button(ui, Icon::LookAt, 28.0, "", false, None), look_at).clicked() {
          app.look_at_selection();
        }
        mode_btn(ui, app, Icon::Pan, pan, NavMode::Pan);
        mode_btn(ui, app, Icon::Zoom, zoom, NavMode::Zoom);
        // Fusion's fifth button is Zoom Window; Fit is in its fly-out (and on F6)
        mode_btn(ui, app, Icon::ZoomWindow, zoom_window, NavMode::ZoomWindow);
        let c = nav_caret(ui, "Zoom Window and Fit");
        egui::Popup::menu(&c).show(|ui| crate::navbar::zoom_menu(app, ui));
        nav_sep(ui);
        let d = tip(icons::icon_button(ui, Icon::Display, 28.0, "", false, None), display);
        let c = nav_caret(ui, "Display Settings");
        egui::Popup::menu(&d).show(|ui| crate::navbar::display_menu(app, ui));
        egui::Popup::menu(&c).show(|ui| crate::navbar::display_menu(app, ui));
        let g = tip(icons::icon_button(ui, Icon::Grid, 28.0, "", false, None), grid);
        let c = nav_caret(ui, "Grid and Snaps");
        let adaptive = fox_engine::scene::grid_step(&app.s);
        egui::Popup::menu(&g).show(|ui| crate::navbar::grid_menu(app, ui, adaptive));
        egui::Popup::menu(&c).show(|ui| crate::navbar::grid_menu(app, ui, adaptive));
        let v = tip(icons::icon_button(ui, Icon::Viewports, 28.0, "", false, None), viewports);
        let c = nav_caret(ui, "Viewports");
        egui::Popup::menu(&v).show(|ui| crate::viewports::menu(app, ui));
        egui::Popup::menu(&c).show(|ui| crate::viewports::menu(app, ui));
        // the Manufacture workspace adds its six CAM display icons after the usual ones, each
        // with a fly-out arrow
        if app.manufacture {
          nav_sep(ui);
          for (i, (icon, hint)) in crate::navbar::CAM_ICONS.into_iter().enumerate() {
            let t = tip(icons::icon_button(ui, icon, 28.0, "", false, None), hint);
            let c = nav_caret(ui, hint);
            egui::Popup::menu(&t).id(egui::Id::new(("nav-cam", i))).show(|ui| crate::navbar::cam_menu(app, ui, i));
            egui::Popup::menu(&c)
              .id(egui::Id::new(("nav-cam-caret", i)))
              .show(|ui| crate::navbar::cam_menu(app, ui, i));
          }
        }
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

/// Where a command dialog's top-right corner docks: the canvas's top-right corner, or, while a
/// sketch is open, beside the SKETCH PALETTE on its left (Fusion shows the two side by side, the
/// palette at its usual place).
pub(crate) fn dialog_dock(canvas: Rect, sketching: bool) -> egui::Pos2 {
  let home = canvas.right_top() + vec2(-34.0, 190.0);
  if sketching { home - vec2(270.0 + 8.0, 0.0) } else { home }
}

/// Where the selection readout's bottom-right corner goes: the canvas's bottom-right corner, below
/// the SKETCH PALETTE (`palette`, its rect while a sketch is open) as in Fusion, or left of the
/// palette when it reaches down over that corner, never under it.
pub(crate) fn readout_anchor(canvas: Rect, palette: Option<Rect>) -> egui::Pos2 {
  let at = canvas.right_bottom() - vec2(14.0, 12.0);
  match palette {
    // only a palette reaching down to the readout's line (pushed down by a dialog) moves it
    Some(p) if p.left() < at.x && p.bottom() > at.y - READOUT_H => egui::pos2(p.left() - 14.0, at.y),
    _ => at,
  }
}

/// The selection readout's bottom-right corner this frame: `readout_anchor` for the SKETCH
/// PALETTE, and left of the BILL OF MATERIALS palette, which docks over the same corner.
pub(crate) fn readout_pos(app: &App, ctx: &egui::Context) -> egui::Pos2 {
  let palette =
    app.s.edit_sketch.is_some().then(|| ctx.memory(|m| m.area_rect(egui::Id::new("sketch-palette")))).flatten();
  let mut at = readout_anchor(app.canvas, palette);
  if app.bom.as_ref().is_some_and(|b| !b.tucked)
    && let Some(bom) = ctx.memory(|m| m.area_rect(egui::Id::new("bom-palette")))
  {
    at.x = at.x.min(readout_anchor(app.canvas, Some(bom)).x);
  }
  at
}

/// The selection readout's line height.
const READOUT_H: f32 = 16.0;

/// The SKETCH PALETTE shown on the right of the canvas while a sketch is open.
pub fn sketch_palette(app: &mut App, ctx: &egui::Context) {
  let Some(sk) = app.s.edit_sketch else { return };
  if !app.palette.shown() {
    crate::palette::palette_tab(app, ctx);
    return;
  }
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
        let header = dialog_header(ui, Icon::Sketch, "SKETCH PALETTE", width);
        crate::palette::header_controls(app, ui, header);
        egui::Frame::new().inner_margin(egui::Margin { left: 10, right: 14, top: 6, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
          let section = |ui: &mut egui::Ui, title: &str| {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), egui::Sense::hover());
            triangle(ui.painter(), r.left_center() + vec2(5.0, 0.0), true, theme::GROUP_LABEL);
            ui.painter().text(
              r.left_center() + vec2(16.0, 0.0),
              egui::Align2::LEFT_CENTER,
              title,
              egui::FontId::proportional(13.0),
              theme::TEXT,
            );
          };
          let row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            ui.horizontal(|ui| {
              ui.set_height(27.0);
              ui.add_space(14.0);
              ui.label(label);
              ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| add(ui));
            });
          };
          // while a tool with several types runs, its types sit above Options, as in Fusion: a row
          // named after the family (Line, Rectangle, ...) with an icon toggle per type, the running
          // one lit, each with the ribbon's tooltip; a chamfer's three kinds and Blend Curve's
          // continuity sit there too, so the section is always the palette's first
          let variants = feature_options(app.tool);
          if !variants.is_empty() || app.tool.is_chamfer() || app.tool == Tool::Blend {
            section(ui, "Feature Options");
            if !variants.is_empty() {
              row(ui, feature_options_label(app.tool), &mut |ui| {
                // laid out right to left, so the last listed is the leftmost
                for &t in variants.iter().rev() {
                  let (icon, _, _) = Cmd::SketchTool(t).info();
                  let resp = icons::icon_button(ui, icon, 22.0, "", app.tool == t, None);
                  let resp = crate::menus::sketch_tool_tip(resp, &app.prefs, t);
                  if resp.clicked() && app.tool != t {
                    // the points already placed carry over to the other kind
                    switch_feature_option(app, t);
                  }
                }
              });
            } else if app.tool.is_chamfer() {
              row(ui, "Chamfer", &mut |ui| {
                // laid out right to left, so the last listed is the leftmost
                for (tool, icon, hint) in [
                  (Tool::Chamfer2, Icon::SkChamfer2, "Two Distance Chamfer"),
                  (Tool::ChamferAngle, Icon::SkChamferAngle, "Distance and Angle Chamfer"),
                  (Tool::Chamfer, Icon::SkChamfer, "Equal Distance Chamfer"),
                ] {
                  if icons::icon_button(ui, icon, 22.0, hint, app.tool == tool, None).clicked() && app.tool != tool {
                    // the line already picked carries over to the other kind
                    crate::sketch_modify::switch_chamfer(app, tool);
                  }
                }
              });
            } else {
              row(ui, "Blend Curve", &mut |ui| {
                for (g2, icon, hint) in
                  [(true, Icon::Curvature, "Curvature (G2)"), (false, Icon::Tangent, "Tangent (G1)")]
                {
                  if icons::icon_button(ui, icon, 22.0, hint, app.modify.blend_g2 == g2, None).clicked() {
                    app.modify.blend_g2 = g2;
                  }
                }
              });
            }
            ui.add_space(2.0);
          }
          section(ui, "Options");
          // the toggles light up when every selected curve has that linetype
          let (construction, centerline) = linetype_state(app, sk);
          row(ui, "Linetype", &mut |ui| {
            if icons::icon_button(ui, Icon::Centerline, 22.0, "Centerline", centerline, None).clicked() {
              app.toggle_centerline();
            }
            if icons::icon_button(ui, Icon::Construction, 22.0, "Construction", construction, None).clicked() {
              app.toggle_construction();
            }
          });
          row(ui, "Look At", &mut |ui| {
            // a push button with Fusion's plain tooltip; it draws the same after the click
            if icons::icon_button(ui, Icon::LookAt, 22.0, "Look At", false, None).clicked() {
              app.look_at_selection();
            }
          });
          row(ui, "Sketch Grid", &mut |ui| {
            ui.checkbox(&mut app.show_grid, "");
          });
          row(ui, "Snap", &mut |ui| {
            ui.checkbox(&mut app.snap, "");
          });
          // Slice sections every body at the sketch plane (see `viewport::slice_params`)
          row(ui, "Slice", &mut |ui| {
            ui.checkbox(&mut app.palette.slice, "");
          });
          row(ui, "Profile", &mut |ui| {
            ui.checkbox(&mut app.palette.profile, "");
          });
          row(ui, "Points", &mut |ui| {
            ui.checkbox(&mut app.palette.points, "");
          });
          row(ui, "Dimensions", &mut |ui| {
            ui.checkbox(&mut app.show_dims, "");
          });
          row(ui, "Constraints", &mut |ui| {
            ui.checkbox(&mut app.show_constraints, "");
          });
          row(ui, "Projected Geometries", &mut |ui| {
            ui.checkbox(&mut app.palette.projected, "");
          });
          row(ui, "Construction Geometries", &mut |ui| {
            ui.checkbox(&mut app.palette.construction, "");
          });
          // 3D Sketch greys while a tool with no 3D form runs, as in Fusion
          let sketch_3d_enabled = crate::viewport::sketch_3d_enabled(app);
          row(ui, "3D Sketch", &mut |ui| {
            ui.add_enabled(sketch_3d_enabled, egui::Checkbox::without_text(&mut app.palette.sketch_3d));
          });
          // Fusion's palette ends with the 3D Sketch row; the solver's readout is a preference
          if let Some(r) = app.s.model.sketches.get(&sk).filter(|_| app.preferences.values.show_dof) {
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
              ui.label(RichText::new(format!("· {}", profile_count(r.profiles.len()))).color(theme::MUTED).small());
            });
          }
        });
        let sep = ui.allocate_exact_size(vec2(width, 1.0), egui::Sense::hover()).0;
        ui.painter().rect_filled(sep, 0.0, Color32::from_gray(215));
        egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
          // a row the button's height: a layout filling the space left would keep the area at
          // its tallest (an Area's content gets the last frame's size), leaving a blank band once
          // Feature Options goes
          let layout = egui::Layout::right_to_left(egui::Align::Center);
          ui.allocate_ui_with_layout(vec2(ui.available_width(), 24.0), layout, |ui| {
            if primary_button(ui, "Finish Sketch", icons::accent_of(Icon::Sketch)).clicked() {
              app.finish_sketch();
            }
          });
        });
      });
    });
  let shown = ctx.memory(|m| m.area_rect(egui::Id::new("sketch-palette")));
  crate::palette::end_frame(app, ctx, shown);
}

/// The types of the running sketch tool, for the palette's Feature Options section: Fusion
/// lists a tool's variants there (Line / Midpoint Line, 2-point / 3-point / center rectangle,
/// the five circles, ...) and nothing for tools with one form (Fit Point Spline, Ellipse, Point).
pub(crate) fn feature_options(tool: Tool) -> &'static [Tool] {
  for (family, _) in FEATURE_OPTION_FAMILIES {
    if family.contains(&tool) {
      return family;
    }
  }
  &[]
}

/// The families the Feature Options section shows, each with the name of its row (the tool's
/// family, as Fusion labels it).
const FEATURE_OPTION_FAMILIES: &[(&[Tool], &str)] = {
  use Tool::*;
  &[
    (&[Line, MidpointLine], "Line"),
    (&[Rect, Rect3, CenterRect], "Rectangle"),
    (&[Circle, Circle2P, Circle3P, Circle2T, Circle3T], "Circle"),
    (&[Arc3, CenterArc, TangentArc], "Arc"),
    (&[Polygon, CircPolygon, EdgePolygon], "Polygon"),
    (&[Slot, OverallSlot, CenterSlot, ArcSlot3, ArcSlotCenter], "Slot"),
  ]
};

/// The name of the Feature Options row for `tool`'s family ("" for a tool without one).
pub(crate) fn feature_options_label(tool: Tool) -> &'static str {
  FEATURE_OPTION_FAMILIES.iter().find(|(f, _)| f.contains(&tool)).map_or("", |(_, l)| l)
}

/// A Feature Options toggle switches the running tool to another type of the same family. The
/// polygon kinds keep the point already placed (they all start from a centre or a vertex); the
/// others start over, as the steps of one type don't fit another.
fn switch_feature_option(app: &mut App, tool: Tool) {
  if matches!(tool, Tool::Polygon | Tool::CircPolygon | Tool::EdgePolygon) {
    app.tool = tool;
  } else {
    app.set_tool(tool);
  }
}

/// "1 profile" / "2 profiles".
pub(crate) fn profile_count(n: usize) -> String {
  if n == 1 { "1 profile".to_string() } else { format!("{n} profiles") }
}

/// Whether every selected curve of the sketch is construction, and whether every one is a
/// centerline (nothing selected: whether the Construction linetype is on for new curves).
fn linetype_state(app: &App, sk: FeatureId) -> (bool, bool) {
  let Ok(sketch) = app.s.design.sketch(sk) else { return (false, false) };
  let curves: Vec<_> = app
    .s
    .selection
    .iter()
    .filter_map(|s| match s {
      Sel::SketchCurve { sketch: in_sk, curve } if *in_sk == sk => sketch.curve(*curve),
      _ => None,
    })
    .collect();
  if curves.is_empty() {
    return (app.palette.construction_linetype, false);
  }
  (curves.iter().all(|c| c.construction), curves.iter().all(|c| c.centerline))
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
pub fn dialog_header(ui: &mut egui::Ui, icon: Icon, title: &str, width: f32) -> Rect {
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
  rect
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

/// Fusion's "Unsaved: Changes may be lost — Save" notice at the top of the canvas, shown for a
/// changed design that has never been saved.
/// The Home page shows none of it, as it shows nothing of the design underneath.
pub fn unsaved_banner(app: &mut App, ctx: &egui::Context) {
  if app.home.open || app.s.path.is_some() || !app.s.dirty {
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

/// What is selected, at the canvas's bottom-right corner, as Fusion shows it.
pub fn selection_info(app: &mut App, ctx: &egui::Context) {
  // an extrude's picked profiles read "1 Profile | Area: …"; otherwise what's selected
  let text = match app.dialog.as_ref().and_then(|d| d.profile_status(&app.s)) {
    Some(p) => p,
    None => {
      let Some(t) = selection_text(app) else { return };
      t
    }
  };
  let at = readout_pos(app, ctx);
  let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("selection-info")));
  // the readout moves left of the warning and error icons at the status bar's right end
  let icons = usize::from(app.warning_toast.is_some()) + usize::from(!app.error_toast.failed.is_empty());
  let shift = icons as f32 * (WARNING_ICON + 6.0);
  painter.text(
    at - vec2(shift, 0.0),
    egui::Align2::RIGHT_BOTTOM,
    text,
    egui::FontId::proportional(12.5),
    Color32::from_gray(50),
  );
}

/// The selection readout, as Fusion's: `1 Face | Area : 100.00 mm^2`, two planar faces their angle
/// (`2 Faces | Angle : 90.0 deg`) or, parallel, their distance, `2 Edges | Length : 20.00 mm`, two
/// items of different kinds each counted with the least distance between them
/// (`1 Face 1 Edge | Min Distance : 0.10 in`), `Multiple selections | 9` from three or more, a
/// body's or feature's name. Only what is selected reads out: hovering a face, an edge or a
/// browser or timeline row prints nothing, as in Fusion, except while a command dialog is
/// picking, when the item under the pointer reads out as a click would pick it (Fillet aside).
pub(crate) fn selection_text(app: &App) -> Option<String> {
  // Mirror's objects and plane read as one count
  if let Some(t) = app.dialog.as_ref().and_then(|d| d.mirror_status()) {
    return Some(t);
  }
  let (hovered, picked);
  // Circular Pattern's centre just picked reads on its own
  let center = app.dialog.as_ref().and_then(|d| d.center_status());
  let sel = match (center.as_ref(), app.s.selection.as_slice()) {
    (Some(c), _) => std::slice::from_ref(c),
    // a curve-picking sketch tool's picks read out like a selection ("2 Sketch Curves | ...")
    (None, []) => {
      // a picked origin plane, axis or point: "XY Plane", "Y Axis", "Origin Point | X: …"; a
      // picked or hovered construction plane: "Plane1"
      if let Some(t) = crate::origin::status(app).or_else(|| crate::construction::status(app)) {
        return Some(t);
      }
      picked = crate::viewport::picked_curves(app);
      if picked.is_empty() {
        // a hovered item reads out only while a command is picking (PROJECT's Bodies filter reads
        // "Body1" over the body); with no command running hovering prints nothing, as in Fusion
        // (not in FILLET, whose status bar stays empty until an edge is picked)
        if !app.dialog.as_ref()?.hover_reads_out() {
          return None;
        }
        // what a click would pick: under the Bodies priority the body ("Body1"), not its face
        let h = app.hover?;
        let t = crate::select::targets(&app.s, h, app.sel_priority);
        hovered = if t.len() == 1 { t } else { vec![h.to_sel()] };
        &hovered[..]
      } else {
        &picked[..]
      }
    }
    (None, s) => s,
  };
  let noun = |s: &Sel| -> (&'static str, &'static str) {
    match s {
      Sel::Face { .. } => ("Face", "Faces"),
      Sel::Edge { .. } => ("Edge", "Edges"),
      Sel::Body { .. } => ("Body", "Bodies"),
      Sel::Profile { .. } => ("Profile", "Profiles"),
      Sel::Feature { .. } => ("Feature", "Features"),
      Sel::SketchCurve { sketch, curve } => sketch_curve_noun(app, *sketch, *curve),
      Sel::SketchPoint { .. } => ("Sketch Point", "Sketch Points"),
      Sel::SketchConstraint { .. } => ("Constraint", "Constraints"),
      Sel::Vertex { .. } => ("Vertex", "Vertices"),
    }
  };
  let first = noun(&sel[0]);
  let n = sel.len();
  // picked timeline items read "3 Features", a group counted with its members ("4 Features")
  if n >= 2 && sel.iter().all(|s| matches!(s, Sel::Feature { .. })) {
    return Some(format!("{} Features", n + timeline_groups::covered(app)));
  }
  // two items of different kinds: "1 Face 1 Edge | Min Distance : 0.10 in" (two sketch curves of
  // different kinds stay "Multiple selections | 2")
  if let [a, b] = sel
    && std::mem::discriminant(a) != std::mem::discriminant(b)
  {
    return Some(mixed_pair_text(app, *a, *b, noun(a).0, noun(b).0));
  }
  if n >= 3 || !sel.iter().all(|s| noun(s) == first) {
    return Some(format!("Multiple selections | {n}"));
  }
  let model = &app.s.model;
  // one constraint glyph reads its name, as Fusion's "Horizontal Constraint"
  if let [Sel::SketchConstraint { sketch, constraint }] = sel
    && let Some(c) = model.sketches.get(sketch).and_then(|r| r.sketch.constraints.iter().find(|c| c.id == *constraint))
    && c.kind.dim().is_none()
  {
    return Some(format!("{} Constraint", c.kind.name()));
  }
  let count = format!("{n} {}", if n == 1 { first.0 } else { first.1 });
  let sum = |f: &dyn Fn(&Sel) -> Option<f64>| sel.iter().map(f).sum::<Option<f64>>();
  let u = app.s.design.units;
  let readout = match sel[0] {
    // one face reads its area; two planar ones their angle, or their distance when parallel
    Sel::Face { body, face } if n == 1 => model
      .body(body)
      .and_then(|b| b.faces().iter().find(|f| f.id == face).map(|f| f.area))
      .map(|a| format!("Area : {}", u.area(a, 2))),
    Sel::Face { .. } if n == 2 => two_faces_readout(app, sel[0], sel[1]),
    Sel::Edge { .. } => sum(&|s| match s {
      Sel::Edge { body, edge } => model.body(*body)?.edges().iter().find(|e| e.id == *edge).map(|e| e.length),
      _ => None,
    })
    .map(|l| format!("Length : {}", u.fixed(l, 2))),
    Sel::Profile { .. } => sum(&|s| match s {
      Sel::Profile { sketch, index } => model.sketches.get(sketch)?.profiles.get(*index).map(|p| p.area().abs()),
      _ => None,
    })
    .map(|a| format!("Area : {}", u.area(a, 2))),
    Sel::Body { body } if n == 1 => return Some(app.s.body_name(body)),
    Sel::Feature { feature } if n == 1 => return app.s.design.feature(feature).map(|f| f.name.clone()),
    // two sketch curves: how close they come, at the document precision ("Min Distance : 0.206 in")
    Sel::SketchCurve { sketch, curve: a } if n == 2 => match sel[1] {
      Sel::SketchCurve { sketch: other, curve: b } if other == sketch => app
        .s
        .design
        .sketch(sketch)
        .ok()
        .and_then(|s| crate::viewport::curve_min_distance(s, a, b))
        .map(|d| format!("Min Distance : {}", crate::units::length_text(u, d))),
      _ => None,
    },
    // a circle or an arc reads its diameter, a line its length, trailing zeros trimmed:
    // "1 Sketch Arc | Diameter : 0.50 in", "1 Sketch Line | Length : 1.00 in"
    Sel::SketchCurve { sketch, curve } if n == 1 => model.sketches.get(&sketch).and_then(|r| {
      let kind = &r.sketch.curve(curve)?.kind;
      let (what, mm) = match kind {
        fox_engine::sketch::CurveKind::Circle { radius, .. } => ("Diameter", 2.0 * radius),
        fox_engine::sketch::CurveKind::Arc { center, start, .. } => {
          let (c, s) = (r.sketch.pos(*center)?, r.sketch.pos(*start)?);
          ("Diameter", 2.0 * (s[0] - c[0]).hypot(s[1] - c[1]))
        }
        _ => ("Length", sketch_curve_length(&r.sketch, kind)),
      };
      Some(format!("{what} : {} {}", sketch_coord(u, mm), u.symbol()))
    }),
    // "1 Sketch Line | Length : 0.524 in", as Fusion spaces it
    Sel::SketchCurve { .. } => sum(&|s| match s {
      Sel::SketchCurve { sketch, curve } => {
        let r = model.sketches.get(sketch)?;
        Some(sketch_curve_length(&r.sketch, &r.sketch.curve(*curve)?.kind))
      }
      _ => None,
    })
    .map(|l| format!("Length : {}", crate::units::length_text(u, l))),
    // "Sketch Point | X: -1.974 Y: 0.00 Z: -0.259 in": one point's world coordinates, no count
    Sel::SketchPoint { sketch, point } if n == 1 => {
      if let Some(w) = model.sketches.get(&sketch).and_then(|r| Some(r.plane.to_world(r.sketch.pos(point)?))) {
        let c = |v: f64| sketch_coord(u, v);
        return Some(format!("Sketch Point | X: {} Y: {} Z: {} {}", c(w.x), c(w.y), c(w.z), u.symbol()));
      }
      None
    }
    // one vertex reads out where it is: "Vertex | X: -1.974 Y: 0.00 Z: 1.741 in"
    Sel::Vertex { body, edge, end } if n == 1 => {
      let at = fox_engine::scene::vertex_pos(&app.s, body, edge, end)?;
      return Some(format!("{} | {}", first.0, crate::origin::coords(app, at)));
    }
    _ => None,
  };
  Some(match readout {
    Some(r) => format!("{count} | {r}"),
    None => count,
  })
}

/// Where a kind comes in a mixed readout: Fusion lists a face before an edge
/// ("1 Face 1 Edge") and a sketch point before a sketch line, whichever was picked first.
fn kind_rank(s: &Sel) -> u8 {
  match s {
    Sel::Body { .. } => 0,
    Sel::Face { .. } => 1,
    Sel::Edge { .. } => 2,
    Sel::Vertex { .. } => 3,
    Sel::SketchPoint { .. } => 4,
    Sel::SketchCurve { .. } => 5,
    Sel::Profile { .. } => 6,
    Sel::SketchConstraint { .. } => 7,
    Sel::Feature { .. } => 8,
  }
}

/// Two items of different kinds: each counted, then how close they come where that can be
/// measured ("1 Face 1 Edge | Min Distance : 0.10 in", "1 Sketch Point 1 Sketch Line | Min
/// Distance : 1.196 in"); sketch geometry reads at the document precision, a body's at two decimals.
fn mixed_pair_text(app: &App, a: Sel, b: Sel, na: &str, nb: &str) -> String {
  let ((a, na), (b, nb)) = if kind_rank(&b) < kind_rank(&a) { ((b, nb), (a, na)) } else { ((a, na), (b, nb)) };
  let count = format!("1 {na} 1 {nb}");
  let u = app.s.design.units;
  let sketchy = |s: &Sel| matches!(s, Sel::SketchPoint { .. } | Sel::SketchCurve { .. });
  match pair_distance(app, a, b) {
    Some(d) if sketchy(&a) || sketchy(&b) => format!("{count} | Min Distance : {}", crate::units::length_text(u, d)),
    Some(d) => format!("{count} | Min Distance : {}", u.fixed(d, 2)),
    None => count,
  }
}

/// Two faces: planar ones read the angle between their normals ("Angle : 90.0 deg"), or, when
/// parallel, how far apart they are; any other pair reads only the count.
fn two_faces_readout(app: &App, a: Sel, b: Sel) -> Option<String> {
  use fox_engine::kernel::SurfaceKind;
  let normal = |s: Sel| match s {
    Sel::Face { body, face } => app
      .s
      .model
      .body(body)?
      .faces()
      .iter()
      .find(|f| f.id == face && f.kind == SurfaceKind::Plane)
      .map(|f| f.normal.normalize()),
    _ => None,
  };
  let cos = normal(a)?.dot(normal(b)?).clamp(-1.0, 1.0);
  let u = app.s.design.units;
  if cos.abs() > 1.0 - 1e-9 {
    return pair_distance(app, a, b).map(|d| format!("Min Distance : {}", u.fixed(d, 2)));
  }
  Some(format!("Angle : {:.*} deg", crate::units::angle_decimals(), cos.acos().to_degrees()))
}

/// A least distance the readout measured: the model revision and the pair it was measured for.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct PairDistance {
  revision: u64,
  pair: (Sel, Sel),
  mm: Option<f64>,
}

/// The least distance (mm) between two selected items, measured once per model revision.
fn pair_distance(app: &App, a: Sel, b: Sel) -> Option<f64> {
  let revision = app.s.revision;
  if let Some(p) = app.readout_distance.get()
    && p.revision == revision
    && p.pair == (a, b)
  {
    return p.mm;
  }
  let mm = measure_pair(app, a, b);
  app.readout_distance.set(Some(PairDistance { revision, pair: (a, b), mm }));
  mm
}

/// The least distance between two items: a sketch point and a curve of its sketch in the sketch's
/// plane, faces, edges, vertices and sketch points by the kernel.
fn measure_pair(app: &App, a: Sel, b: Sel) -> Option<f64> {
  for (p, c) in [(a, b), (b, a)] {
    if let (Sel::SketchPoint { sketch, point }, Sel::SketchCurve { sketch: other, curve }) = (p, c)
      && sketch == other
    {
      let s = app.s.design.sketch(sketch).ok()?;
      return s.distance_to_curve(curve, s.pos(point)?);
    }
  }
  let (sa, sb) = (sel_shape(app, a)?, sel_shape(app, b)?);
  sa.distance_to(&sb).ok().map(|(d, ..)| d)
}

/// A selected face, edge, vertex or sketch point as a kernel shape to measure from.
fn sel_shape(app: &App, s: Sel) -> Option<fox_engine::kernel::Shape> {
  use fox_engine::kernel::Shape;
  let model = &app.s.model;
  match s {
    Sel::Face { body, face } => model.body(body)?.shape.face_shape(face).ok(),
    Sel::Edge { body, edge } => model.body(body)?.shape.edge_shape(edge).ok(),
    Sel::Vertex { body, edge, end } => Shape::vertex(fox_engine::scene::vertex_pos(&app.s, body, edge, end)?).ok(),
    Sel::SketchPoint { sketch, point } => {
      let r = model.sketches.get(&sketch)?;
      Shape::vertex(r.plane.to_world(r.sketch.pos(point)?)).ok()
    }
    _ => None,
  }
}

/// Fusion's name for one sketch curve by its kind ("Sketch Line", "Sketch Circle"); several read
/// "Sketch Curves" whatever their kinds ("2 Sketch Curves | Min Distance : 0.206 in").
fn sketch_curve_noun(app: &App, sketch: FeatureId, curve: fox_engine::sketch::CurveId) -> (&'static str, &'static str) {
  use fox_engine::sketch::CurveKind;
  let kind = app.s.model.sketches.get(&sketch).and_then(|r| r.sketch.curve(curve)).map(|c| &c.kind);
  let one = match kind {
    Some(CurveKind::Line { .. }) => "Sketch Line",
    Some(CurveKind::Circle { .. }) => "Sketch Circle",
    Some(CurveKind::Arc { .. }) => "Sketch Arc",
    Some(CurveKind::Ellipse { .. }) => "Sketch Ellipse",
    Some(CurveKind::EllipticalArc { .. }) => "Sketch Elliptical Arc",
    Some(CurveKind::Spline { .. }) => "Sketch Fitted Spline",
    Some(CurveKind::ControlSpline { .. } | CurveKind::QuinticSpline { .. }) => "Sketch Control Point Spline",
    Some(CurveKind::Conic { .. }) => "Sketch Conic Curve",
    _ => "Sketch Curve",
  };
  (one, "Sketch Curves")
}

/// A sketch curve's length (mm): exact for lines, circles and arcs, along the drawn polyline
/// for the rest.
fn sketch_curve_length(sk: &fox_engine::sketch::Sketch, kind: &fox_engine::sketch::CurveKind) -> f64 {
  use fox_engine::sketch::CurveKind;
  match kind {
    CurveKind::Line { a, b } => match (sk.pos(*a), sk.pos(*b)) {
      (Some(a), Some(b)) => (b[0] - a[0]).hypot(b[1] - a[1]),
      _ => 0.0,
    },
    CurveKind::Circle { radius, .. } => std::f64::consts::TAU * radius,
    CurveKind::Arc { center, start, end } => match (sk.pos(*center), sk.pos(*start), sk.pos(*end)) {
      (Some(c), Some(s), Some(e)) => {
        let r = (s[0] - c[0]).hypot(s[1] - c[1]);
        let a0 = (s[1] - c[1]).atan2(s[0] - c[0]);
        let a1 = (e[1] - c[1]).atan2(e[0] - c[0]);
        let mut sweep = (a1 - a0).rem_euclid(std::f64::consts::TAU);
        if sweep < 1e-12 {
          sweep = std::f64::consts::TAU;
        }
        r * sweep
      }
      _ => 0.0,
    },
    _ => {
      let pts = fox_engine::scene::curve_polyline(sk, kind);
      pts.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum()
    }
  }
}

/// A sketch point's coordinate (mm) as Fusion's status bar reads it out: the document precision
/// with trailing zeros hidden down to the minimum precision ("-1.974", "0.00").
fn sketch_coord(u: fox_engine::model::LengthUnit, mm: f64) -> String {
  crate::units::length_number(u, mm)
}

/// Status message and sketch hint at the bottom-left of the canvas, above COMMENTS.
pub fn canvas_status(app: &mut App, ctx: &egui::Context) {
  // the Zoom Window tool names itself there while it waits for its box, as in Fusion
  let tool = (app.nav_mode == Some(NavMode::ZoomWindow)).then(|| (crate::navbar::BUTTONS[4].to_string(), false));
  let is_tool = app.status.is_none() && tool.is_some();
  let Some((msg, err)) = app.status.clone().or(tool) else { return };
  // a message that is not a running tool's prompt or a red error is transient, as Fusion's
  let now = ctx.input(|i| i.time);
  let key = egui::Id::new("canvas-status-since");
  let since = ctx.data_mut(|d| {
    let seen = d.get_temp_mut_or_insert_with(key, || (msg.clone(), now));
    if seen.0 != msg {
      *seen = (msg.clone(), now);
    }
    seen.1
  });
  if !is_tool && status_expired(&msg, err, app.hint.as_deref(), now - since) {
    app.status = None;
    return;
  }
  if !is_tool && !err && app.hint.as_deref() != Some(msg.as_str()) {
    ctx.request_repaint_after(std::time::Duration::from_millis(500));
  }
  // Preferences → General → Show command prompt
  if !err && !app.preferences.values.show_prompt && (is_tool || app.hint.as_deref() == Some(msg.as_str())) {
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

/// How long a lower-left message that is not a prompt or an error stays.
pub(crate) const STATUS_SECS: f64 = 6.0;

/// Whether a lower-left message shown for `age` seconds goes: a running tool's prompt (`hint`)
/// stays while the tool runs and a red error until the next command succeeds; anything else
/// ("This feature has no editable properties yet", "Computed in 0.3 s") after [`STATUS_SECS`].
pub(crate) fn status_expired(msg: &str, err: bool, hint: Option<&str>, age: f64) -> bool {
  !err && hint != Some(msg) && age > STATUS_SECS
}

/// Fusion's warning toast after a command that dropped constraints or dimensions: its messages
/// and when it first showed (egui time), for the toast's few seconds on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct WarningToast {
  pub warnings: Vec<String>,
  pub since: Option<f64>,
}

/// How long the toast stays up; its status-bar icon stays until the next command.
const TOAST_SECS: f64 = 8.0;
/// The warning icon's size.
const WARNING_ICON: f32 = 16.0;

/// The toast's title: `1 warning(s)`, as Fusion counts them.
pub(crate) fn warning_title(toast: &WarningToast) -> String {
  format!("{} warning(s)", toast.warnings.len())
}

/// Fusion's warning glyph: an amber triangle with a white exclamation mark.
fn warning_glyph(p: &egui::Painter, rect: Rect) {
  let tri = vec![rect.center_top(), rect.right_bottom(), rect.left_bottom()];
  p.add(egui::Shape::convex_polygon(tri, theme::WARNING, Stroke::NONE));
  p.text(
    rect.center() + vec2(0.0, 2.0),
    egui::Align2::CENTER_CENTER,
    "!",
    egui::FontId::proportional(rect.height() * 0.7),
    Color32::WHITE,
  );
}

/// The warning toast at the canvas's lower right (a warning icon, bold "1 warning(s)" and the
/// message) for a few seconds after a command dropped constraints or dimensions, and the warning
/// icon at the right end of the status readout until the next command, as Fusion shows them.
pub fn warning_toast(app: &mut App, ctx: &egui::Context) {
  let Some(toast) = app.warning_toast.as_mut() else { return };
  let now = ctx.input(|i| i.time);
  let since = *toast.since.get_or_insert(now);
  let toast = toast.clone();
  let at = readout_pos(app, ctx);
  let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("warning-icon")));
  warning_glyph(&painter, Rect::from_min_size(at - Vec2::splat(WARNING_ICON), Vec2::splat(WARNING_ICON)));
  if now - since > TOAST_SECS {
    return;
  }
  ctx.request_repaint_after(std::time::Duration::from_millis(250));
  egui::Area::new(egui::Id::new("warning-toast"))
    .fixed_pos(at - vec2(0.0, 28.0))
    .pivot(egui::Align2::RIGHT_BOTTOM)
    .order(egui::Order::Middle)
    .interactable(false)
    .show(ctx, |ui| {
      egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .shadow(egui::epaint::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(40) })
        .show(ui, |ui| {
          ui.set_max_width(320.0);
          ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (r, _) = ui.allocate_exact_size(Vec2::splat(WARNING_ICON + 2.0), egui::Sense::hover());
            warning_glyph(ui.painter(), r.shrink(1.0));
            ui.vertical(|ui| {
              ui.spacing_mut().item_spacing.y = 3.0;
              ui.label(RichText::new(warning_title(&toast)).strong().color(theme::TEXT));
              for w in &toast.warnings {
                ui.label(RichText::new(w).color(theme::TEXT));
              }
            });
          });
        });
    });
}

/// Fusion's error toast: the failed features it has shown (it comes up again when another one
/// fails) and when it last came up (egui time).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ErrorToast {
  pub failed: Vec<FeatureId>,
  pub since: Option<f64>,
}

/// The features whose error shows (failed, not rolled back or suppressed), each with its line
/// in the error toast: the message for the feature the open dialog previews (a fillet too big
/// for its edges), the name for the others (a later feature an edit broke), as Fusion's toast
/// reads them.
pub(crate) fn failed_features(app: &App) -> Vec<(FeatureId, String)> {
  let own = app.dialog.as_ref().and_then(|d| d.own_feature());
  let preview = app.dialog.as_ref().map(|d| d.preview_features(&app.s)).unwrap_or_default();
  app
    .s
    .design
    .features
    .iter()
    .filter(|f| !f.suppressed)
    .filter_map(|f| {
      let st = app.s.model.status(f.id)?;
      let e = st.error.as_ref().filter(|_| !st.rolled_back)?;
      let own = own == Some(f.id) || preview.contains(&f.id);
      Some((f.id, if own { e.clone() } else { f.name.clone() }))
    })
    .collect()
}

/// The error toast's title: `1 error(s)`, as Fusion counts them.
pub(crate) fn error_title(n: usize) -> String {
  format!("{n} error(s)")
}

/// Brings the error toast up again when a feature fails that it hasn't shown yet.
pub(crate) fn note_failures(toast: &mut ErrorToast, failed: &[(FeatureId, String)], now: f64) {
  let ids: Vec<FeatureId> = failed.iter().map(|(id, _)| *id).collect();
  if ids.iter().any(|id| !toast.failed.contains(id)) {
    toast.since = Some(now);
  }
  toast.failed = ids;
}

/// Fusion's error glyph: a red disc with a white X.
fn error_glyph(p: &egui::Painter, rect: Rect) {
  let c = rect.center();
  let r = rect.width().min(rect.height()) / 2.0;
  p.circle_filled(c, r, theme::ERROR);
  let d = r * 0.42;
  let stroke = Stroke::new(1.6, Color32::WHITE);
  p.line_segment([c - vec2(d, d), c + vec2(d, d)], stroke);
  p.line_segment([c + vec2(-d, d), c + vec2(d, -d)], stroke);
}

/// Fusion's error toast at the canvas's lower right (a red circled X, bold "1 error(s)", a line
/// per failed feature and More Info) for a few seconds after a feature fails, and the red X at
/// the right end of the status readout while any feature's error shows: a dialog's failing
/// preview, or a later feature an edit broke.
pub fn error_toast(app: &mut App, ctx: &egui::Context) {
  let failed = failed_features(app);
  let now = ctx.input(|i| i.time);
  note_failures(&mut app.error_toast, &failed, now);
  if failed.is_empty() {
    return;
  }
  let palette =
    app.s.edit_sketch.is_some().then(|| ctx.memory(|m| m.area_rect(egui::Id::new("sketch-palette")))).flatten();
  // the red X at the status readout's end is `error_dialog::badge`, which opens the Error window
  let since = app.error_toast.since.unwrap_or(now);
  if now - since > TOAST_SECS {
    return;
  }
  ctx.request_repaint_after(std::time::Duration::from_millis(250));
  let mut more = false;
  egui::Area::new(egui::Id::new("error-toast"))
    .fixed_pos(readout_anchor(app.canvas, palette) - vec2(0.0, 28.0))
    .pivot(egui::Align2::RIGHT_BOTTOM)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(3.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .shadow(egui::epaint::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(40) })
        .show(ui, |ui| {
          ui.set_max_width(320.0);
          ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (r, _) = ui.allocate_exact_size(Vec2::splat(WARNING_ICON + 2.0), egui::Sense::hover());
            error_glyph(ui.painter(), r.shrink(1.0));
            ui.vertical(|ui| {
              ui.spacing_mut().item_spacing.y = 3.0;
              ui.label(RichText::new(error_title(failed.len())).strong().color(theme::TEXT));
              for (_, line) in &failed {
                ui.label(RichText::new(line).color(theme::TEXT));
              }
              more = ui.link("More Info").clicked();
            });
          });
        });
    });
  if more {
    // More Info closes the toast, as Fusion's does (the badge stays and opens the Error window)
    app.error_toast.since = Some(now - TOAST_SECS - 1.0);
  }
}

// ---------------------------------------------------------------- timeline

pub fn timeline(app: &mut App, ui: &mut egui::Ui) {
  if app.anim.active {
    // the Animation workspace's ANIMATION TIMELINE takes its place
    crate::anim_ui::timeline(app, ui);
    return;
  }
  // playback
  if app.playing.is_some() {
    timeline_bar::step(app, ui.input(|i| i.time));
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
        timeline_bar::play_button(app, ui, ink);
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
          egui::Popup::menu(&g).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            timeline_bar::settings_menu(app, ui);
          });
          ui.add_space(6.0);
          ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            // the flat pattern environment's timeline holds only the flat pattern
            if app.flat_mode {
              crate::flat_ui::timeline_item(ui);
              return;
            }
            let hide = timeline_bar::settings(ui.ctx()).hide_inactive;
            let hidden = hidden_items(app, active).with_owned(timeline_bar::inactive(app, hide));
            timeline_items(app, ui, hidden.shown(n), hidden.shown(active), &hidden);
          });
        });
      });
    });
  rename_popup(app, ui.ctx());
}

/// A drag in progress on the timeline; `gap` is the slot under the pointer (0 = before the
/// first item, n = after the last). The marker's drag rolls the model at each slot, as Fusion's,
/// inside one undo step (`txn`: the drag opened a transaction) from the slot it `start`ed at.
#[derive(Clone, Copy, Debug)]
enum TimelineDrag {
  Marker { gap: usize, start: usize, txn: bool },
  Feature { id: FeatureId, from: usize, gap: usize },
}

/// Starts dragging the marker from slot `active`.
fn start_marker_drag(app: &mut App, active: usize) -> TimelineDrag {
  let txn = app.rebuild.is_none() && app.s.begin_transaction().is_ok();
  TimelineDrag::Marker { gap: active, start: active, txn }
}

/// Features the timeline leaves out: those a dialog's preview added, until OK as in Fusion (the
/// run just before the marker, where a command inserts), and the sketch a single hole placed on
/// a face made for itself (Fusion's hole adds only Hole1). Timeline items are numbered without
/// them (`shown`); the design's own numbering is `real`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Hidden {
  /// Design indices, ascending, each with whether a slot at it falls after it (a preview's
  /// features) rather than before it (a hole's sketch, which goes with the hole after it).
  at: Vec<(usize, bool)>,
}

impl Hidden {
  pub(crate) const NONE: Hidden = Hidden { at: Vec::new() };

  /// The run of features at `active - ids.len() .. active` when those are exactly `ids`; a
  /// preview that landed anywhere else stays shown rather than mis-numbering the timeline.
  pub(crate) fn of(features: &[FeatureId], ids: &[FeatureId], active: usize) -> Hidden {
    let len = ids.len();
    if len == 0 || active < len || active > features.len() {
      return Hidden::NONE;
    }
    let from = active - len;
    let run = &features[from..active];
    if run.iter().all(|f| ids.contains(f)) {
      Hidden { at: (from..active).map(|i| (i, true)).collect() }
    } else {
      Hidden::NONE
    }
  }

  /// Also leaves out the features at `owned`, each belonging to the item after it.
  pub(crate) fn with_owned(mut self, owned: impl IntoIterator<Item = usize>) -> Hidden {
    for i in owned {
      if !self.hides(i) {
        self.at.push((i, false));
      }
    }
    self.at.sort_unstable();
    self
  }

  fn hides(&self, real: usize) -> bool {
    self.at.iter().any(|(h, _)| *h == real)
  }

  /// A design index or count as the timeline numbers it.
  pub(crate) fn shown(&self, real: usize) -> usize {
    real - self.at.iter().filter(|(h, _)| *h < real).count()
  }

  /// A timeline slot as the design numbers it (a slot at a preview's run is after it, one
  /// before a hole is before its sketch).
  pub(crate) fn real(&self, shown: usize) -> usize {
    let mut r = shown;
    for &(h, after) in &self.at {
      if h < r || (after && h == r) {
        r += 1;
      }
    }
    r
  }

  /// A timeline item's design index.
  pub(crate) fn item(&self, shown: usize) -> usize {
    let mut r = shown;
    for &(h, _) in &self.at {
      if h <= r {
        r += 1;
      }
    }
    r
  }
}

/// What the timeline leaves out (see [`Hidden`]).
pub(crate) fn hidden_items(app: &App, active: usize) -> Hidden {
  let d = &app.s.design;
  let placed = crate::dialogs::placement_sketches(&app.s);
  let owned = d.features.iter().enumerate().filter(|(_, f)| placed.contains(&f.id)).map(|(i, _)| i);
  hidden_preview(app, active).with_owned(owned)
}

fn hidden_preview(app: &App, active: usize) -> Hidden {
  let Some(d) = app.dialog.as_ref() else { return Hidden::NONE };
  let ids = d.preview_features(&app.s);
  let features: Vec<FeatureId> = app.s.design.features.iter().map(|f| f.id).collect();
  Hidden::of(&features, &ids, active)
}

fn timeline_items(app: &mut App, ui: &mut egui::Ui, n: usize, active: usize, hidden: &Hidden) {
  let key = egui::Id::new("timeline_drag");
  let mut drag: Option<TimelineDrag> = ui.data(|d| d.get_temp(key));
  egui::ScrollArea::horizontal()
    .auto_shrink([false, false])
    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
    .stick_to_right(true)
    .show(ui, |ui| {
      ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        let features: Vec<(FeatureId, String, Icon, icons::Tint, bool, Option<Color32>)> = app
          .s
          .design
          .features
          .iter()
          .enumerate()
          .filter(|(i, _)| !hidden.hides(*i))
          .map(|(_, f)| {
            let comp = app.ctx_menu.component_colors.then(|| feature_component_color(app, f.id));
            (f.id, f.name.clone(), feature_icon(&f.kind), feature_tint(&f.kind), f.suppressed, comp)
          })
          .collect();
        let mut rects = Vec::with_capacity(features.len());
        // the dragged marker rolls the model as it goes: it fades only where the model lags
        let marker_dragged = matches!(drag, Some(TimelineDrag::Marker { gap, .. }) if gap != active);
        // groups keep the design's numbering (their menus act on it); the timeline's is `shown`
        let groups = timeline_groups::shown(app);
        // a collapsed group's icon stands for each of its features (drag slots skip over it)
        let mut collapsed: Option<(Rect, usize)> = None;
        // expanded groups with their first and last items, for the bracket under them
        let mut brackets: Vec<(timeline_groups::Shown, usize, usize)> = Vec::new();
        for (i, (id, name, icon, tint, suppressed, comp)) in features.into_iter().enumerate() {
          if let Some((rect, last)) = collapsed {
            if i <= last {
              rects.push(rect);
              continue;
            }
            collapsed = None;
          }
          if i == active && marker(ui, marker_dragged).drag_started() {
            drag = Some(start_marker_drag(app, active));
          }
          if let Some(g) = groups.iter().find(|g| hidden.shown(g.first) == i) {
            let last = hidden.shown(g.last);
            if timeline_groups::is_open(ui, g.id) {
              // expanded: its items in the row, no group icon, the bracket under them
              brackets.push((g.clone(), i, last));
            } else {
              let rolled_back = match drag {
                Some(TimelineDrag::Marker { gap, .. }) => i >= gap,
                _ => i >= active,
              };
              let rect = timeline_groups::group_icon(app, ui, g, rolled_back);
              rects.push(rect);
              collapsed = Some((rect, last));
              // the marker inside a collapsed group shows just after it
              if active > i && active <= last && marker(ui, marker_dragged).drag_started() {
                drag = Some(start_marker_drag(app, active));
              }
              continue;
            }
          }
          let st = app.s.model.status(id).cloned().unwrap_or_default();
          // while the marker is dragged, what would roll back greys out already
          let rolled_back = match drag {
            Some(TimelineDrag::Marker { gap, .. }) => i >= gap,
            _ => st.rolled_back,
          };
          let dim = rolled_back || suppressed;
          let moving = matches!(drag, Some(TimelineDrag::Feature { from, .. }) if from == i);
          let selected = app.s.selection.contains(&Sel::Feature { feature: id });
          let editing = app.s.edit_sketch == Some(id);
          let note = st.error.as_ref().map(|e| format!("⚠ {e}")).or_else(|| st.warning.clone());
          let tip = timeline_tip::Tip { icon, name: name.clone(), component: timeline_bar::owner_label(app, id), note };
          // a suppressed feature isn't computed: whether it failed is what it last did
          let computed = (!suppressed && !st.rolled_back).then_some(st.error.is_some());
          let failed = timeline_bar::failed(ui.ctx(), id, computed);
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
          if suppressed && failed {
            // Fusion's suppressed failure: the faded icon on a pink square (a suppressed feature
            // that computes only fades)
            p.rect_filled(ir.expand(1.0), 1.0, SUPPRESSED_FILL);
          }
          if dim {
            icons::draw(p, ir, icon, Color32::from_gray(185));
          } else if let Some(c) = comp {
            icons::draw(p, ir, icon, c);
          } else {
            icons::draw_tinted(p, ir, icon, tint);
          }
          timeline_bar::swatch(app, ui, rect, id);
          if r.drag_started() {
            app.s.selection = vec![Sel::Feature { feature: id }];
            drag = Some(TimelineDrag::Feature { id, from: i, gap: i });
          }
          if drag.is_none() {
            timeline_tip::on_hover(&r, &tip);
          }
          if r.clicked() {
            let m = click_modifiers(ui, rect);
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
          r.context_menu(|ui| timeline_menu(app, ui, hidden.item(i), id, &name, icon, suppressed));
        }
        for (g, first, last) in brackets {
          if let (Some(from), Some(to)) = (rects.get(first), rects.get(last)) {
            timeline_groups::bracket(app, ui, &g, *from, *to);
          }
        }
        // a primitive's temporary sketch, at the end until OK (see `dialogs::temp_sketch_name`)
        if let Some(name) = crate::dialogs::temp_sketch_name(app) {
          let (rect, r) = ui.allocate_exact_size(vec2(26.0, 28.0), egui::Sense::hover());
          icons::draw_tinted(
            ui.painter(),
            Rect::from_center_size(rect.center(), Vec2::splat(20.0)),
            Icon::Sketch,
            icons::Tint::Blue,
          );
          r.on_hover_text(name);
        }
        if active >= n && marker(ui, marker_dragged).drag_started() {
          drag = Some(start_marker_drag(app, active));
        }
        if let Some(d) = drag.as_mut() {
          timeline_drag(app, ui, d, &rects, active, hidden);
          if !ui.input(|i| i.pointer.primary_down()) {
            timeline_drop(app, *d, hidden);
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

/// The modifiers held for a click on `rect`: those its own pointer events carried (a quick
/// Shift-down, click, Shift-up can be over before the frame ends), with the frame's.
pub(crate) fn click_modifiers(ui: &egui::Ui, rect: Rect) -> egui::Modifiers {
  ui.input(|i| {
    let own = i.events.iter().rev().find_map(|e| match e {
      egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, modifiers, .. }
        if rect.contains(*pos) =>
      {
        Some(*modifiers)
      }
      _ => None,
    });
    own.map_or(i.modifiers, |m| m | i.modifiers)
  })
}

/// The pink square behind a suppressed timeline item's faded icon.
pub(crate) const SUPPRESSED_FILL: Color32 = Color32::from_rgb(250, 214, 214);

/// Where a feature dragged from `from` and dropped in slot `gap` ends up.
fn moved_to(from: usize, gap: usize) -> usize {
  if gap > from { gap - 1 } else { gap }
}

/// Follows the pointer during a timeline drag: snaps to the slot between icons. The marker
/// rolls the model there at once, a dark tooltip above it naming the feature left of it; a
/// dragged feature shows a black insertion bar and that tooltip where it can go, and the reason
/// in the tooltip (no bar) where it can't.
fn timeline_drag(app: &mut App, ui: &egui::Ui, d: &mut TimelineDrag, rects: &[Rect], active: usize, hidden: &Hidden) {
  let (Some(first), Some(last)) = (rects.first(), rects.last()) else { return };
  if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
    let gap = rects.iter().filter(|r| r.center().x < pos.x).count();
    match d {
      TimelineDrag::Marker { gap: g, .. } | TimelineDrag::Feature { gap: g, .. } => *g = gap,
    }
  }
  ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
  let x = |gap: usize| match gap {
    0 => first.left() - 1.0,
    g if g >= rects.len() => last.right() + 1.0,
    g => (rects[g - 1].right() + rects[g].left()) / 2.0,
  };
  let (top, bottom) = (first.center().y - 16.0, first.center().y + 16.0);
  let n = rects.len();
  match *d {
    TimelineDrag::Marker { gap, .. } => {
      if gap != active {
        let at = if gap >= n { Value::Null } else { json!(hidden.real(gap)) };
        app.run("timeline.marker", json!({ "at": at }));
      }
      // the model lags when the command was refused (a background rebuild): mark the slot
      if app.s.design.active_len() != hidden.real(gap).min(app.s.design.features.len()) {
        paint_marker(ui.painter(), x(gap), top, bottom, Color32::from_gray(55));
      }
      if let Some(tip) = slot_tip(app, gap, hidden, None) {
        timeline_tip::above(ui.ctx(), Rect::from_x_y_ranges(x(gap)..=x(gap), top..=bottom), &tip);
      }
    }
    TimelineDrag::Feature { id, from, gap } if moved_to(from, gap) != from => {
      let refused = app.s.design.check_move(id, hidden.real(moved_to(from, gap))).err();
      let x = x(gap);
      if refused.is_none() {
        let bar = Rect::from_x_y_ranges(x - 1.5..=x + 1.5, top + 2.0..=bottom - 2.0);
        ui.painter().rect_filled(bar, 0.0, Color32::BLACK);
      } else {
        ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
      }
      let note = refused.map(|e| format!("Can't move there: {e}"));
      // before the first item the refusal is told on the dragged feature's own tooltip
      let own = || {
        let f = app.s.design.feature(id)?;
        let note = note.clone()?;
        Some(timeline_tip::Tip {
          icon: feature_icon(&f.kind),
          name: f.name.clone(),
          component: timeline_bar::owner_label(app, id),
          note: Some(note),
        })
      };
      if let Some(tip) = slot_tip(app, gap, hidden, note.clone()).or_else(own) {
        timeline_tip::above(ui.ctx(), Rect::from_x_y_ranges(x..=x, top..=bottom), &tip);
      }
    }
    _ => {}
  }
}

/// The tooltip of the feature just left of timeline slot `gap` (none before the first).
fn slot_tip(app: &App, gap: usize, hidden: &Hidden, note: Option<String>) -> Option<timeline_tip::Tip> {
  let f = app.s.design.features.get(hidden.item(gap.checked_sub(1)?))?;
  Some(timeline_tip::Tip {
    icon: feature_icon(&f.kind),
    name: f.name.clone(),
    component: timeline_bar::owner_label(app, f.id),
    note,
  })
}

/// Finishes a timeline drag: the marker's rolls are kept as one undo step (none when it ends
/// where it started); a feature moves when it can go there, and is left alone otherwise.
fn timeline_drop(app: &mut App, d: TimelineDrag, hidden: &Hidden) {
  match d {
    TimelineDrag::Marker { gap, start, txn } => {
      if txn && app.s.in_transaction() {
        let _ = if gap == start { app.s.rollback_transaction() } else { app.s.commit_transaction() };
      }
    }
    TimelineDrag::Feature { id, from, gap } if moved_to(from, gap) != from => {
      let to = hidden.real(moved_to(from, gap));
      if app.s.design.check_move(id, to).is_ok() {
        app.run("feature.move", json!({"feature": id.0, "to": to}));
      }
    }
    _ => {}
  }
}

/// An entry of a timeline item's right-click menu (see [`timeline_entries`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimelineEntry {
  /// Greyed: FissionOxide has no selection sets.
  SelectionSet,
  /// A sketch's (or every picked sketch's) profiles into Extrude, with its E key.
  Extrude,
  EditFeature,
  EditSketch,
  EditProfileSketch(FeatureId),
  RedefinePlane,
  SelectSketchPlane,
  /// Greyed until the design has a configuration table, and on a multi-selection.
  Configure,
  /// Two or more picked items into a group.
  CreateGroup,
  /// An item next to a group joins it.
  MoveToGroup,
  Delete,
  Rename,
  RollHere,
  /// Greyed: FissionOxide has no direct-modeling features to convert to.
  ConvertToDm,
  Suppress,
  /// A failed or warned feature: opens the Error window.
  ReviewWarning,
  FindInBrowser,
  FindInWindow,
}

impl TimelineEntry {
  pub(crate) fn label(self, suppressed: bool) -> &'static str {
    match self {
      TimelineEntry::SelectionSet => "Create Selection Set",
      TimelineEntry::Extrude => "Extrude",
      TimelineEntry::EditFeature => "Edit Feature",
      TimelineEntry::EditSketch => "Edit Sketch",
      TimelineEntry::EditProfileSketch(_) => "Edit Profile Sketch",
      TimelineEntry::RedefinePlane => "Redefine Sketch Plane",
      TimelineEntry::SelectSketchPlane => "Select Sketch Plane",
      TimelineEntry::Configure => "Configure",
      TimelineEntry::CreateGroup => "Create Group",
      TimelineEntry::MoveToGroup => "Move to Group",
      TimelineEntry::Delete => "Delete",
      TimelineEntry::Rename => "Rename",
      TimelineEntry::RollHere => "Roll Timeline Marker Here",
      TimelineEntry::ConvertToDm => "Convert to DM Feature",
      TimelineEntry::Suppress if suppressed => "Unsuppress Features",
      TimelineEntry::Suppress => "Suppress Features",
      TimelineEntry::ReviewWarning => "Review warning",
      TimelineEntry::FindInBrowser => "Find in Browser",
      TimelineEntry::FindInWindow => "Find in Window",
    }
  }

  /// The key hint Fusion's menu shows beside the entry.
  pub(crate) fn key(self) -> Option<&'static str> {
    match self {
      TimelineEntry::Extrude => Some("E"),
      TimelineEntry::Delete => Some("Del"),
      _ => None,
    }
  }
}

/// The timeline items picked, in pick order.
pub(crate) fn picked_features(app: &App) -> Vec<FeatureId> {
  app.s.selection.iter().filter_map(|s| if let Sel::Feature { feature } = s { Some(*feature) } else { None }).collect()
}

/// The picked timeline items a right-click on `id` acts on: two or more, `id` among them.
pub(crate) fn multi_pick(app: &App, id: FeatureId) -> Option<Vec<FeatureId>> {
  let picked = picked_features(app);
  (picked.len() >= 2 && picked.contains(&id)).then_some(picked)
}

fn is_sketch(app: &App, id: FeatureId) -> bool {
  app.s.design.feature(id).is_some_and(|f| matches!(f.kind, FeatureKind::Sketch { .. }))
}

/// The head of a menu for several items (a multi-selection or a group): Create Selection Set,
/// then Extrude and Edit Sketch when they are all sketches (else Edit Feature), and Configure.
pub(crate) fn several_head(app: &App, ids: &[FeatureId]) -> Vec<Option<TimelineEntry>> {
  use TimelineEntry as E;
  let mut v = vec![Some(E::SelectionSet)];
  if !ids.is_empty() && ids.iter().all(|f| is_sketch(app, *f)) {
    v.extend([Some(E::Extrude), Some(E::EditSketch)]);
  } else {
    v.push(Some(E::EditFeature));
  }
  v.push(Some(E::Configure));
  v
}

/// A timeline item's right-click menu in Fusion's order (`None`: a separator).
///
/// A sketch: Create Selection Set | Extrude (E) | Edit Sketch, Redefine Sketch Plane, Select
/// Sketch Plane, Configure | Delete (Del), Rename | Roll Timeline Marker Here, Convert to DM
/// Feature, Suppress Features | Find in Browser, Find in Window. Another feature: Create Selection
/// Set | Edit Feature, Configure | ... with Review warning after Suppress Features on a failed or
/// warned item. A suppressed item has no editing entries and no Review warning. Move to Group
/// ends the middle block next to a group. Two or more picked items (the right-clicked one among
/// them): Create Selection Set, Extrude and Edit Sketch (all sketches) or Edit Feature, Configure
/// (greyed) | Create Group | Delete, Rename | Roll Timeline Marker Here, Convert to DM Feature,
/// Suppress Features.
pub(crate) fn timeline_entries(app: &App, id: FeatureId, suppressed: bool) -> Vec<Option<TimelineEntry>> {
  use TimelineEntry as E;
  if let Some(picked) = multi_pick(app, id) {
    let mut v = several_head(app, &picked);
    v.extend([None, Some(E::CreateGroup), None, Some(E::Delete), Some(E::Rename), None, Some(E::RollHere)]);
    v.extend([Some(E::ConvertToDm), Some(E::Suppress)]);
    return v;
  }
  let mut v = vec![Some(E::SelectionSet), None];
  if is_sketch(app, id) {
    if !suppressed {
      v.extend([Some(E::Extrude), None, Some(E::EditSketch), Some(E::RedefinePlane), Some(E::SelectSketchPlane)]);
    }
  } else {
    if !suppressed {
      v.push(Some(E::EditFeature));
    }
    if let Some(sk) = marking::profile_sketch(app, id) {
      v.push(Some(E::EditProfileSketch(sk)));
    }
  }
  v.extend([Some(E::Configure), None, Some(E::Delete), Some(E::Rename), None, Some(E::RollHere)]);
  v.extend([Some(E::ConvertToDm), Some(E::Suppress)]);
  if !suppressed && crate::error_dialog::reviewable(app, id) {
    v.push(Some(E::ReviewWarning));
  }
  if let Some(i) = app.s.design.index_of(id)
    && timeline_groups::move_to_group_target(app, i).is_some()
  {
    v.push(Some(E::MoveToGroup));
  }
  v.extend([None, Some(E::FindInBrowser), Some(E::FindInWindow)]);
  v
}

/// The browser node Find in Browser opens at: the item's own (a sketch, a plane, a joint), else
/// the first body it made or last changed (Extrude1's body after Fillet1 changed it too).
pub(crate) fn browser_node(app: &App, id: FeatureId) -> Option<Sel> {
  let sel = Sel::Feature { feature: id };
  if marking::in_browser(app, &sel) {
    return Some(sel);
  }
  let bodies = marking::feature_bodies(app, id);
  let body = bodies.first().copied().or_else(|| app.s.feature_made(id).bodies.first().copied())?;
  Some(Sel::Body { body })
}

/// A timeline item's right-click menu, as in Fusion (see [`timeline_entries`]).
fn timeline_menu(app: &mut App, ui: &mut egui::Ui, i: usize, id: FeatureId, name: &str, icon: Icon, suppressed: bool) {
  for entry in timeline_entries(app, id, suppressed) {
    match entry {
      Some(entry) => timeline_entry(app, ui, entry, i, id, name, icon, suppressed),
      None => _ = ui.separator(),
    }
  }
}

/// One entry of a timeline item's menu (also a group's, for its head entries): what it does to
/// the right-clicked item `id` at design index `i`, or to every picked item on a multi-selection.
#[allow(clippy::too_many_arguments)]
pub(crate) fn timeline_entry(
  app: &mut App,
  ui: &mut egui::Ui,
  entry: TimelineEntry,
  i: usize,
  id: FeatureId,
  name: &str,
  icon: Icon,
  suppressed: bool,
) {
  use TimelineEntry as E;
  let label = entry.label(suppressed);
  let several = multi_pick(app, id);
  match entry {
    E::SelectionSet | E::ConvertToDm => {
      let icon = if entry == E::SelectionSet { Icon::Select } else { Icon::DirectEdit };
      ui.add_enabled_ui(false, |ui| menu_item(ui, icon, label, None));
    }
    E::Extrude => {
      if menu_item(ui, Icon::Extrude, label, entry.key()).clicked() {
        ui.close();
        let sketches: Vec<FeatureId> = several.unwrap_or_else(|| vec![id]);
        sketch_rows::extrude_sketches(app, &sketches);
      }
    }
    E::EditFeature => {
      if menu_item(ui, icon, label, None).clicked() {
        ui.close();
        edit_feature(app, id);
      }
    }
    E::EditSketch => {
      if menu_item(ui, Icon::Sketch, label, None).clicked() {
        ui.close();
        app.edit_sketch(id);
      }
    }
    E::EditProfileSketch(sk) => {
      if menu_item(ui, Icon::Sketch, label, None).clicked() {
        ui.close();
        app.edit_sketch(sk);
      }
    }
    E::RedefinePlane => {
      if menu_item(ui, Icon::Plane, label, None).clicked() {
        ui.close();
        sketch_rows::run_sketch_cmd(app, id, name, RowCmd::RedefinePlane);
      }
    }
    E::SelectSketchPlane => {
      let on = sketch_rows::has_plane(app, id);
      if ui.add_enabled_ui(on, |ui| menu_item(ui, Icon::Select, label, None)).inner.clicked() {
        ui.close();
        sketch_rows::run_sketch_cmd(app, id, name, RowCmd::SelectSketchPlane);
      }
    }
    // Configure: the feature's suppression becomes a column of the configuration table (greyed
    // on a multi-selection, as Fusion's)
    E::Configure => {
      let first = app.s.design.configurations.list.first().map(|c| c.name.clone()).filter(|_| several.is_none());
      let r = ui.add_enabled_ui(first.is_some(), |ui| menu_item(ui, Icon::ConfigTable, label, None)).inner;
      let r = if first.is_some() {
        r.on_hover_text("Add this feature's suppression to the configuration table")
      } else if several.is_some() {
        r
      } else {
        r.on_disabled_hover_text("The design has no configuration table")
      };
      if r.clicked()
        && let Some(first) = first
      {
        ui.close();
        app.run("config.set", json!({"configuration": first, "feature": id.0, "suppressed": suppressed}));
        crate::config_ui::open_table();
      }
    }
    E::CreateGroup | E::MoveToGroup => timeline_groups::feature_menu_item(app, ui, entry, i, id),
    E::Delete => {
      if menu_item(ui, Icon::Remove, label, entry.key()).clicked() {
        ui.close();
        // the picked items, last first (a feature a deleted one took with it is skipped)
        for f in several.unwrap_or_else(|| vec![id]).into_iter().rev() {
          if app.s.design.feature(f).is_some() {
            app.run("feature.delete", json!({"feature": f.0}));
          }
        }
      }
    }
    E::Rename => {
      if menu_item(ui, Icon::Params, label, None).clicked() {
        ui.close();
        app.rename = Some((RenameTarget::Feature(id.0), name.to_string()));
      }
    }
    E::RollHere => {
      if menu_item(ui, Icon::History, label, None).clicked() {
        ui.close();
        app.run("timeline.marker", json!({"at": i + 1}));
      }
    }
    E::Suppress => {
      if menu_item(ui, Icon::EyeOff, label, None).clicked() {
        ui.close();
        // every picked item follows the right-clicked one
        for f in several.unwrap_or_else(|| vec![id]) {
          app.run("feature.suppress", json!({"feature": f.0, "suppressed": !suppressed}));
        }
      }
    }
    E::ReviewWarning => {
      // menu_item's row with Fusion's red X in the icon slot
      let r = ui.add(egui::Button::new(format!("       {label}")).min_size(vec2(170.0, 0.0)));
      let g = Rect::from_center_size(r.rect.left_center() + vec2(13.0, 0.0), Vec2::splat(13.0));
      crate::error_dialog::error_glyph(ui.painter(), g);
      if r.clicked() {
        ui.close();
        crate::error_dialog::open(ui.ctx());
      }
    }
    E::FindInBrowser => {
      let node = browser_node(app, id);
      if ui.add_enabled_ui(node.is_some(), |ui| menu_item(ui, Icon::Folder, label, None)).inner.clicked()
        && let Some(node) = node
      {
        ui.close();
        marking::find_in_browser(app, node);
      }
    }
    E::FindInWindow => {
      if menu_item(ui, Icon::Fit, label, None).clicked() {
        ui.close();
        let bodies = marking::feature_bodies(app, id);
        marking::find_in_window(app, &bodies, Some(id));
      }
    }
  }
}

/// The history marker: a dark bar with a grab handle, as in Fusion's timeline. Drag it to roll
/// the timeline back or forward; it fades where it is while the model hasn't followed the drag
/// yet. Resting on it shows no tooltip, as in Fusion.
fn marker(ui: &mut egui::Ui, dragged: bool) -> egui::Response {
  let (r, resp) = ui.allocate_exact_size(vec2(12.0, 32.0), egui::Sense::drag());
  let c = if dragged { Color32::from_gray(175) } else { Color32::from_gray(55) };
  paint_marker(ui.painter(), r.center().x, r.top(), r.bottom(), c);
  if resp.hovered() && !dragged {
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
  }
  resp
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

/// A timeline icon's colour under Display Component Colors: the colour of the component that
/// owns what the feature made (the root's for a sketch or a feature that made nothing).
pub(crate) fn feature_component_color(app: &App, f: FeatureId) -> Color32 {
  let made = app.s.feature_made(f);
  let body = made.bodies.first().copied().or(made.faces.first().map(|(b, _)| *b));
  let c = fox_engine::scene::component_color(body.and_then(|b| app.s.design.component_of(b)).map(|c| c.id));
  Color32::from_rgb(c[0], c[1], c[2])
}

/// The colour family of a feature's timeline icon: orange for the surface features, as Fusion
/// draws them (and as the SURFACE tab's menus do), otherwise the icon's own.
pub(crate) fn feature_tint(k: &FeatureKind) -> icons::Tint {
  match k {
    FeatureKind::SurfaceExtrude { .. }
    | FeatureKind::SurfaceRevolve { .. }
    | FeatureKind::Patch { .. }
    | FeatureKind::Ruled { .. }
    | FeatureKind::SurfaceOffset { .. }
    | FeatureKind::SurfaceExtend { .. }
    | FeatureKind::Untrim { .. }
    | FeatureKind::Stitch { .. }
    | FeatureKind::Unstitch { .. }
    | FeatureKind::ReverseNormal { .. }
    | FeatureKind::RuledSurface { .. }
    | FeatureKind::SurfaceSweep { .. }
    | FeatureKind::SurfaceLoft { .. } => icons::Tint::Orange,
    _ => icons::Tint::Blue,
  }
}

pub(crate) fn feature_icon(k: &FeatureKind) -> Icon {
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
    // Insert Mesh's Base Mesh Feature
    FeatureKind::Import { breps, meshes, .. } if breps.is_empty() && !meshes.is_empty() => Icon::MeshFeature,
    FeatureKind::Import { .. } => Icon::Import,
    FeatureKind::RemoveBodies { .. } => Icon::Remove,
    // Fusion's flat sheet (BaseFlange) and bent flange (EdgeFlange) icons
    FeatureKind::Flange { .. } => Icon::BaseFlange,
    FeatureKind::EdgeFlange { .. } => Icon::EdgeFlange,
    FeatureKind::ContourFlange { .. } => Icon::Flange,
    FeatureKind::Hem { .. } => Icon::Hem,
    FeatureKind::Bend { .. } => Icon::Extrude,
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
    FeatureKind::Tessellate { .. } => Icon::Tessellate,
    FeatureKind::MeshRepair { .. } => Icon::Repair,
    FeatureKind::GenerateFaceGroups { .. } => Icon::GenFaceGroups,
    FeatureKind::CombineFaceGroups { .. } => Icon::CombineFaceGroups,
    FeatureKind::MeshPlaneCut { .. } => Icon::PlaneCut,
    FeatureKind::MeshRemesh { .. } => Icon::Remesh,
    FeatureKind::MeshShell { .. } => Icon::Shell,
    FeatureKind::MeshCombine { .. } => Icon::Combine,
    FeatureKind::MeshSeparate { .. } => Icon::Separate,
    FeatureKind::RuledSurface { .. } => Icon::Ruled,
    FeatureKind::SurfaceSweep { .. } => Icon::Sweep,
    FeatureKind::SurfaceLoft { .. } => Icon::Loft,
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
      if let Some(d) = app.dialog.as_mut() {
        d.begin_edit(&mut app.s);
      }
      if app.dialog.is_none() {
        app.status = Some(("This feature has no editable properties yet".into(), false));
      }
      // the bodies the edit acts on are drawn selected, as in Fusion
      if let Some(sel) = app.dialog.as_ref().and_then(|d| d.edit_selection(&app.s)) {
        app.s.selection = sel;
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
            RenameTarget::NamedView(i) => {
              if let Some(old) = app.s.design.named_views.get(i as usize).map(|v| v.name.clone()) {
                app.run("view.named.rename", json!({"name": old, "to": text}));
              }
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

mod browser_analysis;
mod browser_assembly;
mod browser_cam;
mod browser_canvas;
mod browser_config;
mod browser_edit;
pub(crate) mod browser_flat;
mod browser_rows;
mod mesh_rows;
pub(crate) use browser_rows::lock_glyph;
mod sketch_rows;
pub(crate) mod timeline_bar;
mod timeline_groups;
mod timeline_tip;

pub(crate) use browser_edit::set_opacity;
pub(crate) use browser_rows::user_name;
pub(crate) use browser_rows::{body_menu_items, construction_plane_menu, run_body_cmd, run_construction_plane_cmd};
pub(crate) use browser_rows::{doc_name, root_label, unsaved as is_unsaved, window_title};
pub(crate) use mesh_rows::{MeshCmd, OPACITY_LEVELS, mesh_body_menu, run_mesh_cmd};
#[cfg(test)]
pub(crate) use sketch_rows::run_sketch_cmd;
pub(crate) use sketch_rows::{SketchRowState, drawn as sketch_drawn};
pub(crate) use timeline_bar::Playback;

#[cfg(test)]
mod row_menu_tests {
  use super::*;

  /// Display Component Colors gives timeline icons the colour of the component that owns what
  /// the feature made, the root's for sketches.
  #[test]
  fn component_colors_tint_timeline_icons_by_component() {
    let mut app = App::with_session(Session::default());
    app.create_sketch(json!("xy"));
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let sk = app.s.edit_sketch.expect("sketching");
    app.run("extrude", json!({"sketch": sk.0, "distance": 5}));
    let ext = app.s.design.features.last().unwrap().id;
    let rgb = |c: [u8; 4]| Color32::from_rgb(c[0], c[1], c[2]);
    let root = rgb(fox_engine::scene::component_color(None));
    assert_eq!(feature_component_color(&app, sk), root);
    assert_eq!(feature_component_color(&app, ext), root);
    let body = app.s.model.bodies[0].id;
    let id = app.s.design.add_component("Part A", vec![body]).unwrap();
    assert_eq!(feature_component_color(&app, ext), rgb(fox_engine::scene::component_color(Some(id))));
    assert_eq!(feature_component_color(&app, sk), root);
  }

  /// Surface features show orange icons in the timeline, as Fusion's (and the SURFACE tab's
  /// menus) do; solid features keep their own colours.
  #[test]
  fn surface_features_have_orange_timeline_icons() {
    let mut app = App::with_session(Session::default());
    app.create_sketch(json!("xy"));
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]}));
    let sk = app.s.edit_sketch.expect("sketching");
    app.run("surface.patch", json!({"sketch": sk.0}));
    let f = app.s.design.features.last().unwrap();
    assert_eq!(f.name, "SurfacePatch1", "Fusion's name for a Patch");
    assert_eq!((feature_icon(&f.kind), feature_tint(&f.kind)), (Icon::Patch, icons::Tint::Orange));
    app.run("extrude", json!({"sketch": sk.0, "distance": 5}));
    let f = app.s.design.features.last().unwrap();
    assert_eq!((feature_icon(&f.kind), feature_tint(&f.kind)), (Icon::Extrude, icons::Tint::Blue));
    app.create_sketch(json!("xz"));
    app.run("sketch.line", json!({"points": [[20, 0], [30, 0]]}));
    app.run("surface.extrude", json!({"distance": 5}));
    let f = app.s.design.features.last().unwrap();
    assert_eq!(f.name, "SurfaceExtrude1");
    assert_eq!((feature_icon(&f.kind), feature_tint(&f.kind)), (Icon::Extrude, icons::Tint::Orange));
  }

  /// A sketch point's coordinate reads at the document precision with trailing zeros trimmed
  /// down to two decimals, as Fusion's status bar prints it.
  #[test]
  fn sketch_coordinates_trim_to_two_decimals() {
    use fox_engine::model::LengthUnit;
    assert_eq!(sketch_coord(LengthUnit::In, -1.974 * 25.4), "-1.974");
    assert_eq!(sketch_coord(LengthUnit::In, 0.0), "0.00");
    assert_eq!(sketch_coord(LengthUnit::In, 12.7), "0.50");
    assert_eq!(sketch_coord(LengthUnit::In, -0.0001), "0.00");
    assert_eq!(sketch_coord(LengthUnit::Mm, 12.346), "12.346");
    assert_eq!(sketch_coord(LengthUnit::Mm, 10.0), "10.00");
  }

  /// The palette's Feature Options list a running tool's types, as Fusion's does: lines,
  /// rectangles, circles, arcs, polygons and slots, in a row named after the family; the splines,
  /// Select and the single-form tools show none.
  #[test]
  fn feature_options_list_the_tools_types() {
    assert_eq!(feature_options(Tool::Rect), &[Tool::Rect, Tool::Rect3, Tool::CenterRect]);
    assert_eq!(feature_options(Tool::CenterRect), feature_options(Tool::Rect));
    assert_eq!(feature_options(Tool::Circle3T).len(), 5);
    assert!(feature_options(Tool::Circle).contains(&Tool::Circle2P));
    assert_eq!(feature_options(Tool::TangentArc), &[Tool::Arc3, Tool::CenterArc, Tool::TangentArc]);
    // Line and Midpoint Line share a "Line" row (Fusion's), the Line (L) toggle first
    assert_eq!(feature_options(Tool::Line), &[Tool::Line, Tool::MidpointLine]);
    assert_eq!(feature_options(Tool::MidpointLine), feature_options(Tool::Line));
    assert_eq!(feature_options_label(Tool::MidpointLine), "Line");
    assert_eq!(feature_options_label(Tool::Circle2T), "Circle");
    assert_eq!(feature_options_label(Tool::ArcSlot3), "Slot");
    // Fit Point Spline shows no Feature Options in Fusion (nor does Control Point Spline)
    assert!(feature_options(Tool::Spline).is_empty());
    assert!(feature_options(Tool::ControlSpline).is_empty());
    assert_eq!(feature_options_label(Tool::Spline), "");
    assert!(feature_options(Tool::Select).is_empty());
    assert!(feature_options(Tool::Dimension).is_empty());
    assert!(feature_options(Tool::Ellipse).is_empty());
    for (family, label) in FEATURE_OPTION_FAMILIES {
      assert!(!label.is_empty());
      for t in *family {
        // every type has a ribbon icon and name for its toggle
        assert!(!Cmd::SketchTool(*t).info().1.is_empty());
      }
    }
  }

  /// Switching a Feature Options toggle keeps a polygon's placed point (the kinds share their
  /// first click) and starts the other families over.
  #[test]
  fn feature_option_switch_keeps_a_polygons_point() {
    let (mut app, _) = sketched();
    app.set_tool(Tool::Polygon);
    crate::viewport::push_point_for_test(&mut app, [5.0, 5.0]);
    switch_feature_option(&mut app, Tool::CircPolygon);
    assert_eq!(app.tool, Tool::CircPolygon);
    assert!(!app.tstate.is_empty(), "the centre carries over");
    app.set_tool(Tool::Rect);
    crate::viewport::push_point_for_test(&mut app, [5.0, 5.0]);
    switch_feature_option(&mut app, Tool::Rect3);
    assert_eq!(app.tool, Tool::Rect3);
    assert!(app.tstate.is_empty(), "a rectangle starts over");
  }

  #[test]
  fn profile_count_reads_singular_and_plural() {
    assert_eq!(profile_count(0), "0 profiles");
    assert_eq!(profile_count(1), "1 profile");
    assert_eq!(profile_count(2), "2 profiles");
  }

  /// The Linetype toggles light up when every selected curve has that linetype.
  #[test]
  fn linetype_toggles_follow_the_selection() {
    let mut app = App::with_session(Session::default());
    app.create_sketch(json!("xy"));
    let sk = app.s.edit_sketch.expect("sketching");
    let r = app.run("sketch.line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let c = r["curves"][0].as_u64().unwrap() as u32;
    assert_eq!(linetype_state(&app, sk), (false, false));
    app.s.selection = vec![Sel::SketchCurve { sketch: sk, curve: fox_engine::sketch::CurveId(c) }];
    assert_eq!(linetype_state(&app, sk), (false, false));
    app.toggle_construction();
    assert_eq!(linetype_state(&app, sk), (true, false));
    app.toggle_construction();
    // a centerline lights the Centerline toggle alone; X on it keeps the centerline flag and
    // lights both, as in Fusion; X again leaves a plain centerline
    app.toggle_centerline();
    assert_eq!(linetype_state(&app, sk), (false, true));
    app.toggle_construction();
    assert_eq!(linetype_state(&app, sk), (true, true));
    app.toggle_construction();
    assert_eq!(linetype_state(&app, sk), (false, true));
    app.toggle_centerline();
    assert_eq!(linetype_state(&app, sk), (false, false));
  }

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
  fn origin_plane_menu_is_fusions() {
    let mut app = App::with_session(Session::default());
    let l = labels(&origin_plane_menu(&app, "xz"));
    assert_eq!(l, ["Create Sketch", "Offset Plane", "-", "Show/Hide", "Look At", "Find in Window"]);
    // Show/Hide is the row's own eye: on to start, and the plane shows only with the folder's
    use crate::origin::{self, OriginItem};
    assert!(origin::eye(&app, OriginItem::Xz) && !origin::visible(&app, OriginItem::Xz));
    run_origin_plane_cmd(&mut app, "xz", RowCmd::ShowHide);
    assert!(!origin::eye(&app, OriginItem::Xz) && origin::eye(&app, OriginItem::Xy));
    assert_eq!(labels(&origin_plane_menu(&app, "xz"))[3], "Show/Hide");
    run_origin_plane_cmd(&mut app, "xz", RowCmd::ShowHide);
    assert!(origin::eye(&app, OriginItem::Xz));
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

#[cfg(test)]
mod timeline_preview_tests {
  use fox_engine::model::FeatureId;

  use super::Hidden;

  #[test]
  fn a_previewed_feature_is_left_out_of_the_timeline_numbering() {
    let f: Vec<FeatureId> = (1..=5).map(FeatureId).collect();
    // the preview added feature 4 (at index 3), the marker sits after it
    let h = Hidden::of(&f, &[FeatureId(4)], 4);
    assert_eq!(h, Hidden { at: vec![(3, true)] });
    assert_eq!((0..=5).map(|i| h.shown(i)).collect::<Vec<_>>(), vec![0, 1, 2, 3, 3, 4]);
    assert_eq!((0..=4).map(|i| h.real(i)).collect::<Vec<_>>(), vec![0, 1, 2, 4, 5]);
    // nothing previewed, or a preview that didn't land just before the marker: all shown
    assert_eq!(Hidden::of(&f, &[], 5), Hidden::NONE);
    assert_eq!(Hidden::of(&f, &[FeatureId(2)], 5), Hidden::NONE);
    assert_eq!(Hidden::NONE.shown(3), 3);
    assert_eq!(Hidden::NONE.real(3), 3);
    // two features added at the end
    let h = Hidden::of(&f, &[FeatureId(4), FeatureId(5)], 5);
    assert_eq!((h.shown(5), h.shown(3), h.real(3)), (3, 3, 5));
  }

  #[test]
  fn a_holes_own_sketch_is_left_out_and_goes_with_the_hole() {
    // Box, Sketch (the hole's, at 1), Hole, Fillet: the timeline shows Box, Hole, Fillet
    let h = Hidden::NONE.with_owned([1]);
    assert_eq!((0..=4).map(|i| h.shown(i)).collect::<Vec<_>>(), vec![0, 1, 1, 2, 3]);
    // the items map to Box, Hole, Fillet; the slot before Hole is before its sketch
    assert_eq!((0..3).map(|i| h.item(i)).collect::<Vec<_>>(), vec![0, 2, 3]);
    assert_eq!((0..=3).map(|i| h.real(i)).collect::<Vec<_>>(), vec![0, 1, 3, 4]);
    // with a preview at the end as well
    let f: Vec<FeatureId> = (1..=5).map(FeatureId).collect();
    let h = Hidden::of(&f, &[FeatureId(5)], 5).with_owned([1]);
    assert_eq!((h.shown(5), h.item(2), h.real(3)), (3, 3, 5));
  }
}

/// The browser's component rows (id, depth), for tests elsewhere in the crate.
#[cfg(test)]
pub(crate) fn component_rows(app: &App) -> Vec<(u32, usize)> {
  browser_assembly::component_tree(app)
}
