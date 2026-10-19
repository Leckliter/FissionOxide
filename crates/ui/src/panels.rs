//! The Fusion-style workspace chrome: application bar with document tabs, the toolbar
//! (workspace switcher, tabs and tool groups), the browser, comments, navigation bar and
//! sketch palette floating over the canvas, the timeline, and the parameters window.

use clf_engine::model::{BodyId, FeatureId, FeatureKind};
use clf_engine::render::StandardView;
use clf_engine::{Sel, Session};
use egui::{Color32, Pos2, Rect, RichText, Stroke, Vec2, pos2, vec2};
use serde_json::json;

use crate::dialogs::Kind;
use crate::icons::{self, Icon};
use crate::theme;
use crate::viewport::NavMode;
use crate::{App, RenameTarget, Tool};

/// Toolbar tabs of the Design workspace; `Sketch` is the contextual tab shown while sketching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
  Solid,
  Surface,
  Mesh,
  SheetMetal,
  Plastic,
  Utilities,
  Manage,
  Sketch,
}

impl Tab {
  const MAIN: [Tab; 7] =
    [Tab::Solid, Tab::Surface, Tab::Mesh, Tab::SheetMetal, Tab::Plastic, Tab::Utilities, Tab::Manage];

  fn label(self) -> &'static str {
    match self {
      Tab::Solid => "SOLID",
      Tab::Surface => "SURFACE",
      Tab::Mesh => "MESH",
      Tab::SheetMetal => "SHEET METAL",
      Tab::Plastic => "PLASTIC",
      Tab::Utilities => "UTILITIES",
      Tab::Manage => "MANAGE",
      Tab::Sketch => "SKETCH",
    }
  }
}

const WORKSPACES: [&str; 7] =
  ["Design", "Generative Design", "Render", "Animation", "Simulation", "Manufacture", "Drawing"];

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
        ui.spacing_mut().item_spacing.x = 2.0;
        if bar_icon(ui, Icon::DataPanel, "Show Data Panel", true).clicked() {
          app.status = Some(("The Data Panel (cloud projects) isn't part of CLFusion yet".into(), false));
        }
        ui.add_space(4.0);
        let file = file_button(ui);
        egui::Popup::menu(&file).show(|ui| file_menu(app, ui));
        if bar_icon(ui, Icon::Save, "Save (Ctrl+S)", true).clicked() {
          app.save(false);
        }
        ui.add_space(6.0);
        if bar_icon(ui, Icon::Undo, "Undo (Ctrl+Z)", app.s.can_undo()).clicked() && app.s.can_undo() {
          app.cancel_dialog();
          app.s.undo();
        }
        if bar_icon(ui, Icon::Redo, "Redo (Ctrl+Y)", app.s.can_redo()).clicked() && app.s.can_redo() {
          app.s.redo();
        }
        ui.add_space(24.0);
        document_tab(app, ui);
        if bar_icon(ui, Icon::Plus, "New Design (Ctrl+N)", true).clicked() {
          app.new_design();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          ui.spacing_mut().item_spacing.x = 4.0;
          avatar(ui);
          let help = bar_icon(ui, Icon::Help, "Help", true);
          egui::Popup::menu(&help).show(help_menu);
          let _ = bar_icon(ui, Icon::Bell, "Notifications", true);
          let _ = bar_icon(ui, Icon::JobStatus, "Job Status: all changes are local", true);
          let _ = bar_icon(ui, Icon::Extensions, "Extensions", true);
        });
      });
    });
}

fn file_button(ui: &mut egui::Ui) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(38.0, 28.0), egui::Sense::click());
  if resp.hovered() {
    ui.painter().rect(rect, 2.0, theme::HOVER, Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
  }
  icons::draw_color(
    ui.painter(),
    Rect::from_center_size(rect.center() - vec2(5.0, 0.0), Vec2::splat(19.0)),
    Icon::File,
  );
  icons::chevron(ui.painter(), rect.right_center() - vec2(7.0, 0.0), theme::ICON_INK);
  resp.on_hover_text("File")
}

fn file_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(220.0);
  if ui.button("New Design            Ctrl+N").clicked() {
    app.new_design();
    ui.close();
  }
  if ui.button("Open...                  Ctrl+O").clicked() {
    ui.close();
    app.open_dialog();
  }
  ui.separator();
  if ui.button("Save                      Ctrl+S").clicked() {
    ui.close();
    app.save(false);
  }
  if ui.button("Save As...").clicked() {
    ui.close();
    app.save(true);
  }
  ui.separator();
  if ui.button("Insert STEP...").clicked() {
    ui.close();
    app.import();
  }
  ui.menu_button("Export", |ui| {
    for (label, ext) in [("STEP Files (*.step)", "step"), ("STL Files (*.stl)", "stl"), ("OBJ Files (*.obj)", "obj")] {
      if ui.button(label).clicked() {
        ui.close();
        app.export(ext);
      }
    }
  });
  ui.separator();
  ui.menu_button("View", |ui| {
    ui.checkbox(&mut app.browser_open, "Show Browser");
    ui.checkbox(&mut app.show_comments, "Show Comments");
    ui.checkbox(&mut app.show_navbar, "Show Navigation Bar");
    ui.checkbox(&mut app.show_viewcube, "Show ViewCube");
  });
}

fn help_menu(ui: &mut egui::Ui) {
  ui.set_min_width(300.0);
  ui.label(RichText::new("Navigation").strong());
  ui.label("Middle-drag pan · Shift+middle-drag or right-drag orbit · wheel zoom");
  ui.add_space(4.0);
  ui.label(RichText::new("Shortcuts").strong());
  ui.label("Sketch: L line · R rectangle · C circle · A arc · P point · D dimension · X construction");
  ui.label("Model: E extrude · F fillet · Esc cancel · Del delete · F6 fit · Home view");
  ui.separator();
  ui.label(RichText::new("CLFusion — parametric CAD on OpenCASCADE").color(theme::MUTED));
}

fn avatar(ui: &mut egui::Ui) {
  let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), egui::Sense::click());
  ui.painter().circle_filled(rect.center(), 12.0, Color32::from_rgb(0, 112, 192));
  ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "CL", egui::FontId::proportional(11.0), Color32::WHITE);
  resp.on_hover_text("Local user");
}

fn document_tab(app: &mut App, ui: &mut egui::Ui) {
  let name = app
    .s
    .path
    .as_ref()
    .and_then(|p| p.file_stem())
    .map(|n| n.to_string_lossy().to_string())
    .unwrap_or_else(|| app.s.design.name.clone());
  let label = if app.s.dirty { format!("{name}*") } else { name };
  let font = egui::FontId::proportional(12.5);
  let tw = ui.painter().layout_no_wrap(label.clone(), font.clone(), theme::TEXT).size().x;
  let w = (tw + 70.0).clamp(150.0, 320.0);
  let full = ui.max_rect();
  let (slot, _) = ui.allocate_exact_size(vec2(w, 30.0), egui::Sense::hover());
  // the tab runs to the bottom edge of the bar and merges into the toolbar
  let rect = Rect::from_min_max(pos2(slot.min.x, full.bottom() - 31.0), pos2(slot.max.x, full.bottom() + 1.0));
  let p = ui.painter();
  p.rect(
    rect,
    egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 },
    theme::TOOLBAR,
    Stroke::new(1.0, theme::BORDER),
    egui::StrokeKind::Inside,
  );
  p.line_segment(
    [rect.left_bottom() + vec2(1.0, -0.5), rect.right_bottom() + vec2(-1.0, -0.5)],
    Stroke::new(2.0, theme::TOOLBAR),
  );
  icons::draw_color(
    p,
    Rect::from_center_size(rect.left_center() + vec2(16.0, 0.0), Vec2::splat(16.0)),
    Icon::Component,
  );
  p.text(rect.left_center() + vec2(30.0, 0.0), egui::Align2::LEFT_CENTER, label, font, theme::TEXT);
  let close = Rect::from_center_size(rect.right_center() - vec2(14.0, 0.0), Vec2::splat(16.0));
  let cr = ui.interact(close, ui.id().with("doc-close"), egui::Sense::click());
  if cr.hovered() {
    ui.painter().rect_filled(close, 2.0, Color32::from_gray(220));
  }
  icons::draw(ui.painter(), close, Icon::Close, theme::ICON_INK);
  if cr.on_hover_text("Close (opens a new empty design)").clicked() {
    app.new_design();
  }
}

// ---------------------------------------------------------------- toolbar

/// One toolbar command: shown as an icon in its group and as a row in the group's menu.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Cmd {
  NewSketch,
  Dialog(Kind),
  Params,
  InsertStep,
  Select,
  SketchTool(Tool),
  Construction,
  Constraint(&'static str),
  FinishSketch,
}

impl Cmd {
  fn info(self) -> (Icon, &'static str, &'static str) {
    match self {
      Cmd::NewSketch => (Icon::Sketch, "Create Sketch", "Pick a plane or planar face to sketch on"),
      Cmd::Dialog(k) => match k {
        Kind::Extrude => (Icon::Extrude, "Extrude", "Add depth to a sketch profile (E)"),
        Kind::Revolve => (Icon::Revolve, "Revolve", "Revolve a sketch profile around an axis"),
        Kind::Box => (Icon::Box, "Box", "Create a solid box"),
        Kind::Cylinder => (Icon::Cylinder, "Cylinder", "Create a solid cylinder"),
        Kind::Sphere => (Icon::Sphere, "Sphere", "Create a solid sphere"),
        Kind::Torus => (Icon::Torus, "Torus", "Create a solid torus"),
        Kind::RectPattern => (Icon::Pattern, "Rectangular Pattern", "Copy bodies in rows and columns"),
        Kind::CircPattern => (Icon::CircPattern, "Circular Pattern", "Copy bodies around an axis"),
        Kind::Mirror => (Icon::Mirror, "Mirror", "Mirror bodies across a plane"),
        Kind::Fillet => (Icon::Fillet, "Fillet", "Round selected edges (F)"),
        Kind::Chamfer => (Icon::Chamfer, "Chamfer", "Bevel selected edges"),
        Kind::Shell => (Icon::Shell, "Shell", "Hollow a body, removing selected faces"),
        Kind::Combine => (Icon::Combine, "Combine", "Join, cut or intersect bodies"),
        Kind::Move => (Icon::Move, "Move/Copy", "Move or rotate bodies"),
        Kind::OffsetPlane => (Icon::Plane, "Offset Plane", "Construction plane offset from a plane or face"),
        Kind::Inspect => (Icon::Measure, "Measure", "Measure and inspect the selection"),
        Kind::PickPlane => (Icon::Sketch, "Create Sketch", "Pick a plane"),
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
        Tool::Point => (Icon::Point, "Point", "Sketch point (P)"),
        Tool::Dimension => (Icon::Dimension, "Sketch Dimension", "Dimension geometry (D)"),
        Tool::Select => (Icon::Select, "Select", "Select sketch geometry"),
      },
      Cmd::Construction => (Icon::Construction, "Construction", "Toggle construction linetype (X)"),
      Cmd::Constraint(kind) => {
        let (icon, name) = constraint_info(kind);
        (icon, name, "Applies to the current selection")
      }
      Cmd::FinishSketch => (Icon::Finish, "Finish Sketch", "Finish the sketch and return to the model"),
    }
  }

  fn active(self, app: &App) -> bool {
    match self {
      Cmd::SketchTool(t) => app.tool == t,
      Cmd::Select => false,
      Cmd::Params => app.show_params,
      Cmd::Dialog(k) => app.dialog.as_ref().is_some_and(|d| d.kind == k),
      _ => false,
    }
  }

  fn run(self, app: &mut App) {
    match self {
      Cmd::NewSketch => app.new_sketch(),
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
    }
  }
}

const CONSTRAINTS: [&str; 12] = [
  "horizontal",
  "vertical",
  "coincident",
  "tangent",
  "equal",
  "parallel",
  "perpendicular",
  "fix",
  "midpoint",
  "concentric",
  "collinear",
  "symmetric",
];

fn constraint_info(kind: &str) -> (Icon, &'static str) {
  match kind {
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
    _ => (Icon::Symmetric, "Symmetry"),
  }
}

/// A tool group: the icon row with its label and drop-down underneath, then a separator.
/// `menu` lists every command of the group (`None` entries are separators).
fn group(app: &mut App, ui: &mut egui::Ui, label: &str, quick: &[Cmd], menu: &[Option<Cmd>]) {
  let mut run = None;
  let inner = ui.vertical(|ui| {
    ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
    ui.add_space(2.0);
    ui.horizontal(|ui| {
      for c in quick {
        let (icon, name, hint) = c.info();
        if icons::tool_button(ui, icon, name, hint, c.active(app)).clicked() {
          run = Some(*c);
        }
      }
    });
    ui.add_space(3.0);
    let w = ui.min_rect().width();
    group_label(ui, label, w)
  });
  let label_resp = inner.inner;
  egui::Popup::menu(&label_resp).show(|ui| {
    for item in menu {
      match item {
        Some(c) => {
          let (icon, name, _) = c.info();
          if icons::menu_item(ui, icon, name).clicked() {
            run = Some(*c);
            ui.close();
          }
        }
        None => {
          ui.separator();
        }
      }
    }
  });
  separator(ui);
  if let Some(c) = run {
    c.run(app);
  }
}

/// The group name with its drop-down chevron (clickable).
fn group_label(ui: &mut egui::Ui, label: &str, width: f32) -> egui::Response {
  let font = egui::FontId::proportional(11.0);
  let tw = ui.painter().layout_no_wrap(label.to_string(), font.clone(), theme::GROUP_LABEL).size().x;
  let (rect, resp) = ui.allocate_exact_size(vec2(width.max(tw + 22.0), 18.0), egui::Sense::click());
  if resp.hovered() {
    ui.painter().rect_filled(rect, 2.0, theme::HOVER);
  }
  let x0 = rect.center().x - (tw + 12.0) / 2.0;
  ui.painter().text(pos2(x0, rect.center().y), egui::Align2::LEFT_CENTER, label, font, theme::GROUP_LABEL);
  icons::chevron(ui.painter(), pos2(x0 + tw + 8.0, rect.center().y), theme::GROUP_LABEL);
  resp
}

fn separator(ui: &mut egui::Ui) {
  let (r, _) = ui.allocate_exact_size(vec2(9.0, 58.0), egui::Sense::hover());
  ui.painter().line_segment([r.center_top() + vec2(0.0, 4.0), r.center_bottom()], Stroke::new(1.0, theme::SEPARATOR));
}

pub fn toolbar(app: &mut App, ui: &mut egui::Ui) {
  let sketching = app.s.edit_sketch.is_some();
  if !sketching && app.tab == Tab::Sketch {
    app.tab = Tab::Solid;
  }
  egui::Panel::top("toolbar")
    .exact_size(92.0)
    .frame(egui::Frame::NONE.fill(theme::TOOLBAR).inner_margin(egui::Margin { left: 6, right: 6, top: 0, bottom: 0 }))
    .show(ui, |ui| {
      let full = ui.max_rect();
      ui.painter().line_segment([full.left_bottom(), full.right_bottom()], Stroke::new(1.0, theme::BORDER));
      ui.horizontal(|ui| {
        workspace_switcher(app, ui);
        ui.add_space(6.0);
        ui.vertical(|ui| {
          ui.spacing_mut().item_spacing.y = 0.0;
          tab_row(app, ui, sketching);
          egui::ScrollArea::horizontal()
            .id_salt("tools")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
              ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                match app.tab {
                  Tab::Solid => solid_toolbar(app, ui),
                  Tab::Sketch => sketch_toolbar(app, ui),
                  other => {
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                      ui.add_space(18.0);
                      ui.label(
                        RichText::new(format!("{} tools aren't in CLFusion yet", other.label())).color(theme::MUTED),
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
    ui.add_space(10.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(96.0, 68.0), egui::Sense::click());
    let fill = if resp.hovered() { theme::HOVER } else { Color32::WHITE };
    ui.painter().rect(rect, 2.0, fill, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
    icons::draw_color(
      ui.painter(),
      Rect::from_center_size(rect.center() - vec2(0.0, 12.0), Vec2::splat(24.0)),
      Icon::Extrude,
    );
    ui.painter().text(
      rect.center() + vec2(-6.0, 16.0),
      egui::Align2::CENTER_CENTER,
      "DESIGN",
      egui::FontId::proportional(12.5),
      theme::TEXT,
    );
    icons::chevron(ui.painter(), rect.center() + vec2(28.0, 16.0), theme::TEXT);
    egui::Popup::menu(&resp).show(|ui| {
      ui.set_min_width(180.0);
      for (i, w) in WORKSPACES.iter().enumerate() {
        let r = ui.add_enabled(i == 0, egui::Button::selectable(i == 0, *w));
        if r.clicked() {
          ui.close();
        }
      }
    });
    let _ = app;
  });
}

fn tab_row(app: &mut App, ui: &mut egui::Ui, sketching: bool) {
  ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing.x = 2.0;
    let mut tabs: Vec<Tab> = Tab::MAIN.to_vec();
    if sketching {
      tabs.push(Tab::Sketch);
    }
    for t in tabs {
      let font = egui::FontId::proportional(12.0);
      let active = app.tab == t;
      let color = if active { Color32::BLACK } else { Color32::from_gray(95) };
      let tw = ui.painter().layout_no_wrap(t.label().into(), font.clone(), color).size().x;
      let (rect, resp) = ui.allocate_exact_size(vec2(tw + 22.0, 28.0), egui::Sense::click());
      if resp.hovered() && !active {
        ui.painter().rect_filled(rect.shrink2(vec2(0.0, 3.0)), 2.0, theme::HOVER);
      }
      ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, t.label(), font, color);
      if active {
        let c = if t == Tab::Sketch { theme::SKETCH_ACCENT } else { theme::ACCENT };
        ui.painter().line_segment(
          [rect.left_bottom() + vec2(6.0, -2.0), rect.right_bottom() + vec2(-6.0, -2.0)],
          Stroke::new(2.5, c),
        );
      }
      if resp.clicked() {
        app.tab = t;
      }
    }
  });
}

fn solid_toolbar(app: &mut App, ui: &mut egui::Ui) {
  use Cmd::*;
  use Kind as K;
  group(
    app,
    ui,
    "CREATE",
    &[NewSketch, Dialog(K::Extrude), Dialog(K::Revolve), Dialog(K::Box), Dialog(K::RectPattern), Dialog(K::Mirror)],
    &[
      Some(NewSketch),
      None,
      Some(Dialog(K::Extrude)),
      Some(Dialog(K::Revolve)),
      None,
      Some(Dialog(K::Box)),
      Some(Dialog(K::Cylinder)),
      Some(Dialog(K::Sphere)),
      Some(Dialog(K::Torus)),
      None,
      Some(Dialog(K::RectPattern)),
      Some(Dialog(K::CircPattern)),
      Some(Dialog(K::Mirror)),
    ],
  );
  group(
    app,
    ui,
    "MODIFY",
    &[Dialog(K::Fillet), Dialog(K::Chamfer), Dialog(K::Shell), Dialog(K::Combine), Dialog(K::Move), Params],
    &[
      Some(Dialog(K::Fillet)),
      Some(Dialog(K::Chamfer)),
      Some(Dialog(K::Shell)),
      None,
      Some(Dialog(K::Combine)),
      Some(Dialog(K::Move)),
      None,
      Some(Params),
    ],
  );
  group(app, ui, "CONSTRUCT", &[Dialog(K::OffsetPlane)], &[Some(Dialog(K::OffsetPlane))]);
  group(app, ui, "INSPECT", &[Dialog(K::Inspect)], &[Some(Dialog(K::Inspect))]);
  group(app, ui, "INSERT", &[InsertStep], &[Some(InsertStep)]);
  group(app, ui, "SELECT", &[Select], &[Some(Select)]);
}

fn sketch_toolbar(app: &mut App, ui: &mut egui::Ui) {
  use Cmd::*;
  let t = SketchTool;
  group(
    app,
    ui,
    "CREATE",
    &[
      t(Tool::Line),
      t(Tool::Rect),
      t(Tool::Circle),
      t(Tool::Arc3),
      t(Tool::Polygon),
      t(Tool::Slot),
      t(Tool::Dimension),
    ],
    &[
      Some(t(Tool::Line)),
      None,
      Some(t(Tool::Rect)),
      Some(t(Tool::CenterRect)),
      Some(t(Tool::Circle)),
      Some(t(Tool::Arc3)),
      Some(t(Tool::CenterArc)),
      Some(t(Tool::Polygon)),
      Some(t(Tool::Slot)),
      Some(t(Tool::Point)),
      None,
      Some(t(Tool::Dimension)),
    ],
  );
  group(app, ui, "MODIFY", &[Construction], &[Some(Construction)]);
  // constraints: a compact two-row grid, as in Fusion's CONSTRAINTS group
  let mut run = None;
  let inner = ui.vertical(|ui| {
    ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
    ui.spacing_mut().interact_size.y = 18.0;
    ui.add_space(2.0);
    for row in CONSTRAINTS.chunks(6) {
      ui.horizontal(|ui| {
        for kind in row {
          let (icon, name) = constraint_info(kind);
          if icons::icon_button(ui, icon, 18.0, name, false, None).clicked() {
            run = Some(Constraint(kind));
          }
        }
      });
    }
    ui.add_space(3.0);
    let w = ui.min_rect().width();
    group_label(ui, "CONSTRAINTS", w)
  });
  egui::Popup::menu(&inner.inner).show(|ui| {
    for kind in CONSTRAINTS {
      let (icon, name) = constraint_info(kind);
      if icons::menu_item(ui, icon, name).clicked() {
        run = Some(Constraint(kind));
        ui.close();
      }
    }
  });
  separator(ui);
  if let Some(c) = run {
    c.run(app);
  }
  group(app, ui, "INSPECT", &[Dialog(Kind::Inspect)], &[Some(Dialog(Kind::Inspect))]);
  group(app, ui, "SELECT", &[t(Tool::Select)], &[Some(t(Tool::Select))]);
  group(app, ui, "FINISH SKETCH", &[FinishSketch], &[Some(FinishSketch)]);
}

// ---------------------------------------------------------------- canvas overlays

fn float_frame() -> egui::Frame {
  egui::Frame::new().fill(theme::FLOAT).stroke(Stroke::new(1.0, theme::BORDER)).corner_radius(2).shadow(egui::Shadow {
    offset: [0, 1],
    blur: 6,
    spread: 0,
    color: Color32::from_black_alpha(30),
  })
}

/// A floating panel's header: collapse arrow and title.
fn float_header(ui: &mut egui::Ui, title: &str, open: &mut bool, width: f32) {
  let (rect, resp) = ui.allocate_exact_size(vec2(width, 24.0), egui::Sense::click());
  ui.painter().rect_filled(rect, egui::CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, theme::FLOAT_HEADER);
  let tri = rect.left_center() + vec2(12.0, 0.0);
  triangle(ui.painter(), tri, *open, theme::GROUP_LABEL);
  ui.painter().text(
    rect.left_center() + vec2(24.0, 0.0),
    egui::Align2::LEFT_CENTER,
    title,
    egui::FontId::proportional(11.0),
    theme::GROUP_LABEL,
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

/// What happened to a browser row.
#[derive(Default)]
struct RowEvents {
  clicked: bool,
  double_clicked: bool,
  eye: bool,
  response: Option<egui::Response>,
}

/// One browser row: expander, visibility eye, icon and name.
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
  let (rect, resp) = ui.allocate_exact_size(vec2(width, 22.0), egui::Sense::click());
  let open_id = id.with("open");
  let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).or(expand).unwrap_or(false);
  let p = ui.painter().clone();
  if selected {
    p.rect_filled(rect, 0.0, theme::SELECTED);
  } else if resp.hovered() {
    p.rect_filled(rect, 0.0, theme::HOVER);
  }
  let x = rect.min.x + 6.0 + depth as f32 * 14.0;
  let cy = rect.center().y;
  let mut ev = RowEvents::default();
  if expand.is_some() {
    let tr = Rect::from_center_size(pos2(x + 5.0, cy), Vec2::splat(14.0));
    let r = ui.interact(tr, id.with("tri"), egui::Sense::click());
    triangle(&p, tr.center(), open, if r.hovered() { theme::ACCENT } else { Color32::from_gray(90) });
    if r.clicked() {
      open = !open;
      ui.data_mut(|d| d.insert_temp(open_id, open));
    }
  }
  if let Some(visible) = eye {
    let er = Rect::from_center_size(pos2(x + 22.0, cy), vec2(16.0, 14.0));
    let r = ui.interact(er, id.with("eye"), egui::Sense::click());
    icons::draw_color(&p, er, if visible { Icon::Eye } else { Icon::EyeOff });
    if r.on_hover_text(if visible { "Hide" } else { "Show" }).clicked() {
      ev.eye = true;
    }
  }
  icons::draw_color(&p, Rect::from_center_size(pos2(x + 41.0, cy), Vec2::splat(16.0)), icon);
  p.text(pos2(x + 54.0, cy), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(12.5), label_color);
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
  egui::Area::new(egui::Id::new("browser")).fixed_pos(canvas.min + vec2(8.0, 8.0)).order(egui::Order::Middle).show(
    ctx,
    |ui| {
      float_frame().show(ui, |ui| {
        let width = 250.0;
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        float_header(ui, "BROWSER", &mut app.browser_open, width);
        if !app.browser_open {
          return;
        }
        let max_h = (canvas.height() - 90.0).max(120.0);
        egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
          ui.add_space(3.0);
          browser_tree(app, ui);
          ui.add_space(4.0);
        });
      });
    },
  );
}

fn browser_tree(app: &mut App, ui: &mut egui::Ui) {
  let root = egui::Id::new("tree-root");
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
    tree_row(ui, ds.with(1), 2, None, None, Icon::Measure, "Units: mm", false, theme::TEXT);
  }
  // Named Views
  let nv = egui::Id::new("tree-views");
  tree_row(ui, nv, 1, Some(false), None, Icon::Folder, "Named Views", false, theme::TEXT);
  if is_open(ui, nv, false) {
    for (label, v) in [
      ("TOP", StandardView::Top),
      ("FRONT", StandardView::Front),
      ("RIGHT", StandardView::Right),
      ("HOME", StandardView::Home),
    ] {
      let ev = tree_row(ui, nv.with(label), 2, None, None, Icon::Views, label, false, theme::TEXT);
      if ev.clicked || ev.double_clicked {
        app.cam.set_view(v);
        if v == StandardView::Home {
          app.fit_view();
        }
      }
    }
  }
  // Origin
  let og = egui::Id::new("tree-origin");
  tree_row(ui, og, 1, Some(false), None, Icon::Folder, "Origin", false, theme::TEXT);
  if is_open(ui, og, false) {
    tree_row(ui, og.with("o"), 2, None, None, Icon::Origin, "O", false, theme::TEXT);
    for (label, plane) in [("XY", "xy"), ("XZ", "xz"), ("YZ", "yz")] {
      let ev = tree_row(ui, og.with(label), 2, None, None, Icon::Plane, label, false, theme::TEXT);
      if ev.double_clicked {
        app.create_sketch(json!(plane));
      }
      if let Some(r) = ev.response {
        r.on_hover_text("Double-click to sketch on this plane").context_menu(|ui| {
          if ui.button("Create Sketch").clicked() {
            ui.close();
            app.create_sketch(json!(plane));
          }
        });
      }
    }
  }
  bodies_section(app, ui);
  sketches_section(app, ui);
  planes_section(app, ui);
}

fn bodies_section(app: &mut App, ui: &mut egui::Ui) {
  let bodies: Vec<(BodyId, String, bool)> =
    app.s.model.bodies.iter().map(|b| (b.id, app.s.body_name(b.id), app.s.body_visible(b.id))).collect();
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
    let color = if visible { theme::TEXT } else { theme::MUTED };
    let ev = tree_row(ui, id.with(bid.0), 2, None, Some(visible), Icon::Body, &name, selected, color);
    if ev.eye {
      app.run("body.visible", json!({"body": bid.0, "visible": !visible}));
    } else if ev.clicked || ev.double_clicked {
      toggle_sel(&mut app.s, Sel::Body { body: bid }, ui.input(|i| i.modifiers.shift || i.modifiers.command));
    }
    if let Some(r) = ev.response {
      r.context_menu(|ui| {
        if ui.button("Rename").clicked() {
          ui.close();
          app.rename = Some((RenameTarget::Body(bid.0), name.clone()));
        }
        if ui.button("Properties").clicked() {
          ui.close();
          app.s.selection = vec![Sel::Body { body: bid }];
          app.start(Kind::Inspect);
        }
        if ui.button(if visible { "Hide" } else { "Show" }).clicked() {
          ui.close();
          app.run("body.visible", json!({"body": bid.0, "visible": !visible}));
        }
        ui.separator();
        if ui.button("Remove").clicked() {
          ui.close();
          app.run("body.remove", json!({"bodies": [bid.0]}));
        }
      });
    }
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
    let ev = tree_row(ui, id.with(fid.0), 2, None, None, Icon::SketchSmall, &name, selected, color);
    if ev.double_clicked {
      app.edit_sketch(fid);
    } else if ev.clicked {
      app.s.selection = vec![Sel::Feature { feature: fid }];
    }
    if let Some(r) = ev.response {
      r.on_hover_text("Double-click to edit").context_menu(|ui| {
        if ui.button("Edit Sketch").clicked() {
          ui.close();
          app.edit_sketch(fid);
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
    let ev = tree_row(ui, id.with(fid.0), 2, None, None, Icon::Plane, &name, false, theme::TEXT);
    if ev.double_clicked {
      app.create_sketch(json!({"feature": fid.0}));
    }
    if let Some(r) = ev.response {
      r.on_hover_text("Double-click to sketch on this plane");
    }
  }
}

/// The collapsed COMMENTS panel in the canvas's bottom-left corner.
pub fn comments(app: &mut App, ctx: &egui::Context) {
  let canvas = app.canvas;
  egui::Area::new(egui::Id::new("comments"))
    .fixed_pos(canvas.left_bottom() + vec2(8.0, -8.0))
    .pivot(egui::Align2::LEFT_BOTTOM)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      float_frame().show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let open = false;
        let (rect, _) = ui.allocate_exact_size(vec2(250.0, 24.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, theme::FLOAT_HEADER);
        triangle(ui.painter(), rect.left_center() + vec2(12.0, 0.0), open, theme::GROUP_LABEL);
        ui.painter().text(
          rect.left_center() + vec2(24.0, 0.0),
          egui::Align2::LEFT_CENTER,
          "COMMENTS",
          egui::FontId::proportional(11.0),
          theme::GROUP_LABEL,
        );
        let plus = Rect::from_center_size(rect.right_center() - vec2(14.0, 0.0), Vec2::splat(14.0));
        icons::draw(ui.painter(), plus, Icon::Plus, theme::GROUP_LABEL);
      });
    });
}

/// Fusion's navigation bar at the bottom centre of the canvas.
pub fn nav_bar(app: &mut App, ctx: &egui::Context) {
  let canvas = app.canvas;
  egui::Area::new(egui::Id::new("navbar"))
    .fixed_pos(canvas.center_bottom() + vec2(0.0, -8.0))
    .pivot(egui::Align2::CENTER_BOTTOM)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      float_frame().inner_margin(egui::Margin::symmetric(4, 2)).show(ui, |ui| {
        ui.horizontal(|ui| {
          ui.spacing_mut().item_spacing.x = 1.0;
          let mode_btn = |ui: &mut egui::Ui, app: &mut App, icon, hint, mode| {
            if icons::icon_button(ui, icon, 26.0, hint, app.nav_mode == Some(mode), None).clicked() {
              app.nav_mode = if app.nav_mode == Some(mode) { None } else { Some(mode) };
            }
          };
          mode_btn(ui, app, Icon::Orbit, "Orbit: left-drag to orbit (Esc to stop)", NavMode::Orbit);
          if icons::icon_button(
            ui,
            Icon::LookAt,
            26.0,
            "Look At: the sketch plane or a selected planar face",
            false,
            None,
          )
          .clicked()
          {
            app.look_at_selection();
          }
          nav_sep(ui);
          mode_btn(ui, app, Icon::Pan, "Pan: left-drag to pan (Esc to stop)", NavMode::Pan);
          mode_btn(ui, app, Icon::Zoom, "Zoom: left-drag to zoom (Esc to stop)", NavMode::Zoom);
          if icons::icon_button(ui, Icon::Fit, 26.0, "Fit (F6)", false, None).clicked() {
            app.fit_view();
          }
          nav_sep(ui);
          let d = icons::icon_button(ui, Icon::Display, 26.0, "Display Settings", false, None);
          egui::Popup::menu(&d).show(|ui| {
            ui.set_min_width(220.0);
            ui.label(RichText::new("Visual Style").strong());
            if ui.radio(app.show_edges, "Shaded with Visible Edges Only").clicked() {
              app.show_edges = true;
            }
            if ui.radio(!app.show_edges, "Shaded").clicked() {
              app.show_edges = false;
            }
            ui.separator();
            ui.label(RichText::new("Camera").strong());
            if ui.radio(app.cam.ortho, "Orthographic").clicked() {
              app.cam.ortho = true;
            }
            if ui.radio(!app.cam.ortho, "Perspective").clicked() {
              app.cam.ortho = false;
            }
            ui.separator();
            ui.checkbox(&mut app.show_viewcube, "ViewCube");
          });
          let g = icons::icon_button(ui, Icon::Grid, 26.0, "Grid and Snaps", false, None);
          egui::Popup::menu(&g).show(|ui| {
            ui.set_min_width(160.0);
            ui.checkbox(&mut app.show_grid, "Layout Grid");
          });
          let v = icons::icon_button(ui, Icon::Viewports, 26.0, "Viewports", false, None);
          egui::Popup::menu(&v).show(|ui| {
            let _ = ui.selectable_label(true, "Single View");
          });
        });
      });
    });
}

fn nav_sep(ui: &mut egui::Ui) {
  let (r, _) = ui.allocate_exact_size(vec2(7.0, 24.0), egui::Sense::hover());
  ui.painter().line_segment(
    [r.center_top() + vec2(0.0, 3.0), r.center_bottom() - vec2(0.0, 3.0)],
    Stroke::new(1.0, theme::SEPARATOR),
  );
}

/// The SKETCH PALETTE shown on the right of the canvas while a sketch is open.
pub fn sketch_palette(app: &mut App, ctx: &egui::Context) {
  let Some(sk) = app.s.edit_sketch else { return };
  let canvas = app.canvas;
  egui::Area::new(egui::Id::new("sketch-palette"))
    .fixed_pos(canvas.right_top() + vec2(-8.0, 150.0))
    .pivot(egui::Align2::RIGHT_TOP)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      dialog_frame().show(ui, |ui| {
        let width = 250.0;
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        dialog_header(ui, Icon::Sketch, "SKETCH PALETTE", width);
        egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(6.0, 5.0);
          let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), egui::Sense::hover());
          triangle(ui.painter(), r.left_center() + vec2(4.0, 0.0), true, theme::GROUP_LABEL);
          ui.painter().text(
            r.left_center() + vec2(14.0, 0.0),
            egui::Align2::LEFT_CENTER,
            "Options",
            egui::FontId::proportional(12.5),
            theme::TEXT,
          );
          egui::Grid::new("palette").num_columns(2).spacing([12.0, 5.0]).show(ui, |ui| {
            ui.label("Linetype");
            if icons::icon_button(ui, Icon::Construction, 22.0, "Construction (X)", false, None).clicked() {
              app.toggle_construction();
            }
            ui.end_row();
            ui.label("Look At");
            if icons::icon_button(ui, Icon::LookAt, 22.0, "Look at the sketch plane", false, None).clicked() {
              app.look_at_selection();
            }
            ui.end_row();
            ui.label("Sketch Grid");
            ui.checkbox(&mut app.show_grid, "");
            ui.end_row();
            ui.label("Show Dimensions");
            ui.checkbox(&mut app.show_dims, "");
            ui.end_row();
            ui.label("Show Constraints");
            ui.checkbox(&mut app.show_constraints, "");
            ui.end_row();
          });
          if let Some(r) = app.s.model.sketches.get(&sk) {
            ui.add_space(2.0);
            let (txt, col) = if !r.report.ok() {
              ("Over-constrained / conflicting".to_string(), theme::ERROR)
            } else if r.report.dof == 0 {
              ("Fully constrained".to_string(), theme::TEXT)
            } else {
              (format!("{} degrees of freedom", r.report.dof), theme::ACCENT)
            };
            ui.label(RichText::new(txt).color(col).small());
            ui.label(RichText::new(format!("{} profiles", r.profiles.len())).color(theme::MUTED).small());
          }
        });
        let sep = ui.allocate_exact_size(vec2(width, 1.0), egui::Sense::hover()).0;
        ui.painter().rect_filled(sep, 0.0, theme::SEPARATOR);
        egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
          ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if primary_button(ui, "Finish Sketch").clicked() {
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
    .fill(theme::PANEL)
    .stroke(Stroke::new(1.0, Color32::from_gray(150)))
    .corner_radius(2)
    .shadow(egui::Shadow { offset: [0, 3], blur: 12, spread: 0, color: Color32::from_black_alpha(55) })
}

/// The dark title bar of a command dialog: command icon, title and the » collapse mark.
pub fn dialog_header(ui: &mut egui::Ui, icon: Icon, title: &str, width: f32) {
  let (rect, _) = ui.allocate_exact_size(vec2(width, 28.0), egui::Sense::hover());
  ui.painter().rect_filled(rect, egui::CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, theme::DIALOG_HEADER);
  icons::draw_color(
    ui.painter(),
    Rect::from_center_size(rect.left_center() + vec2(15.0, 0.0), Vec2::splat(17.0)),
    icon,
  );
  ui.painter().text(
    rect.left_center() + vec2(30.0, 0.0),
    egui::Align2::LEFT_CENTER,
    title,
    egui::FontId::proportional(12.5),
    Color32::WHITE,
  );
  ui.painter().text(
    rect.right_center() - vec2(12.0, 1.0),
    egui::Align2::CENTER_CENTER,
    "»",
    egui::FontId::proportional(15.0),
    Color32::from_gray(200),
  );
}

/// Fusion's blue OK-style button.
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
  let fill = theme::ACCENT;
  let r = ui.add(
    egui::Button::new(RichText::new(label).color(Color32::WHITE))
      .fill(fill)
      .stroke(Stroke::new(1.0, theme::ACCENT_DARK))
      .min_size(vec2(72.0, 24.0)),
  );
  if r.hovered() {
    ui.painter().rect_filled(r.rect, 2.0, Color32::from_white_alpha(25));
  }
  r
}

/// White secondary button (Cancel).
pub fn secondary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
  ui.add(
    egui::Button::new(label)
      .fill(Color32::WHITE)
      .stroke(Stroke::new(1.0, Color32::from_gray(170)))
      .min_size(vec2(72.0, 24.0)),
  )
}

/// Status message and sketch hint at the bottom-left of the canvas, above COMMENTS.
pub fn canvas_status(app: &mut App, ctx: &egui::Context) {
  let Some((msg, err)) = app.status.clone() else { return };
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
        let ink = Some(Color32::from_gray(70));
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

fn timeline_items(app: &mut App, ui: &mut egui::Ui, n: usize, active: usize) {
  egui::ScrollArea::horizontal()
    .auto_shrink([false, false])
    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
    .stick_to_right(true)
    .show(ui, |ui| {
      ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        let features: Vec<(FeatureId, String, Icon, bool)> =
          app.s.design.features.iter().map(|f| (f.id, f.name.clone(), feature_icon(&f.kind), f.suppressed)).collect();
        for (i, (id, name, icon, suppressed)) in features.into_iter().enumerate() {
          if i == active {
            marker(ui);
          }
          let st = app.s.model.status(id).cloned().unwrap_or_default();
          let dim = st.rolled_back || suppressed;
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
          let (rect, r) = ui.allocate_exact_size(vec2(26.0, 28.0), egui::Sense::click());
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
          if selected || editing {
            p.rect(rect, 2.0, theme::SELECTED, Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Inside);
          } else if r.hovered() {
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
          let r = r.on_hover_text(hover);
          if r.clicked() {
            app.s.selection = vec![Sel::Feature { feature: id }];
          }
          if r.double_clicked() {
            edit_feature(app, id);
          }
          r.context_menu(|ui| {
            if ui.button("Edit Feature").clicked() {
              ui.close();
              edit_feature(app, id);
            }
            if ui.button("Rename").clicked() {
              ui.close();
              app.rename = Some((RenameTarget::Feature(id.0), name.clone()));
            }
            if ui.button(if suppressed { "Unsuppress Features" } else { "Suppress Features" }).clicked() {
              ui.close();
              app.run("feature.suppress", json!({"feature": id.0}));
            }
            if ui.button("Roll History Marker Here").clicked() {
              ui.close();
              app.run("timeline.marker", json!({"at": i + 1}));
            }
            ui.separator();
            if ui.button(RichText::new("Delete").color(theme::ERROR)).clicked() {
              ui.close();
              app.run("feature.delete", json!({"feature": id.0}));
            }
          });
        }
        if active >= n {
          marker(ui);
        }
      });
    });
}

/// The history marker: a dark bar with a grab handle, as in Fusion's timeline.
fn marker(ui: &mut egui::Ui) {
  let (r, _) = ui.allocate_exact_size(vec2(10.0, 30.0), egui::Sense::hover());
  let x = r.center().x;
  let c = Color32::from_gray(60);
  ui.painter().line_segment([pos2(x, r.top() + 2.0), pos2(x, r.bottom() - 2.0)], Stroke::new(2.0, c));
  ui.painter().rect_filled(Rect::from_center_size(pos2(x, r.bottom() - 6.0), vec2(8.0, 8.0)), 1.0, c);
}

/// Applies a constraint to the current sketch selection.
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
  // H/V on several lines: one constraint each
  if matches!(kind, "horizontal" | "vertical" | "fix") && points.len() != 2 {
    let mut any = false;
    if kind == "fix" {
      for p in &points {
        any |= app.run("sketch.constrain", json!({"sketch": sk.0, "type": kind, "points": [p]})).is_some();
      }
    } else {
      for c in &curves {
        any |= app.run("sketch.constrain", json!({"sketch": sk.0, "type": kind, "curves": [c]})).is_some();
      }
    }
    if any {
      app.s.selection.clear();
    } else if curves.is_empty() && points.is_empty() {
      app.status = Some((format!("Select geometry, then apply {kind}"), false));
    }
    return;
  }
  if app.run("sketch.constrain", json!({"sketch": sk.0, "type": kind, "points": points, "curves": curves})).is_some() {
    app.s.selection.clear();
  }
}

fn feature_icon(k: &FeatureKind) -> Icon {
  use clf_engine::model::Primitive;
  match k {
    FeatureKind::Sketch { .. } => Icon::Sketch,
    FeatureKind::Extrude { .. } => Icon::Extrude,
    FeatureKind::Revolve { .. } => Icon::Revolve,
    FeatureKind::Fillet { .. } => Icon::Fillet,
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
    FeatureKind::Move { .. } => Icon::Move,
    FeatureKind::Combine { .. } => Icon::Combine,
    FeatureKind::Import { .. } => Icon::Import,
    FeatureKind::RemoveBodies { .. } => Icon::Remove,
  }
}

/// Double-click in the timeline: sketches open for editing, other features get their dialog.
pub fn edit_feature(app: &mut App, id: FeatureId) {
  let Some(f) = app.s.design.feature(id) else { return };
  match &f.kind {
    FeatureKind::Sketch { .. } => app.edit_sketch(id),
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

pub fn parameters_window(app: &mut App, ctx: &egui::Context) {
  let mut open = true;
  egui::Window::new("ƒx Parameters").open(&mut open).default_width(460.0).show(ctx, |ui| {
    ui.label(
      RichText::new("User parameters drive dimensions and features: type a name anywhere a value is expected.")
        .small()
        .color(theme::MUTED),
    );
    ui.add_space(4.0);
    let params = app.s.design.parameters.clone();
    egui::Grid::new("params").striped(true).num_columns(5).show(ui, |ui| {
      ui.label(RichText::new("Name").strong());
      ui.label(RichText::new("Expression").strong());
      ui.label(RichText::new("Value").strong());
      ui.label(RichText::new("Comment").strong());
      ui.label("");
      ui.end_row();
      for p in params {
        ui.label(&p.name);
        let id = egui::Id::new(("param-expr", &p.name));
        let mut text = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| p.expr.clone());
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(120.0));
        if r.changed() {
          ui.data_mut(|d| d.insert_temp(id, text.clone()));
        }
        if r.lost_focus() {
          if text != p.expr {
            app.run("param.set", json!({"name": p.name, "expr": text}));
          }
          ui.data_mut(|d| d.remove::<String>(id));
        }
        let value = app.s.model.params.get(&p.name).copied();
        let shown = match (value, p.kind) {
          (Some(v), clf_engine::model::ParamKind::Angle) => {
            clf_engine::model::sketch::Dim::new(v.to_degrees()).expr + "°"
          }
          (Some(v), clf_engine::model::ParamKind::Length) => {
            format!("{} mm", clf_engine::model::sketch::Dim::new(v).expr)
          }
          (Some(v), _) => clf_engine::model::sketch::Dim::new(v).expr,
          (None, _) => "error".into(),
        };
        let err = app.s.model.param_errors.get(&p.name).cloned();
        let l = ui.label(RichText::new(shown).color(if err.is_some() { theme::ERROR } else { theme::MUTED }));
        if let Some(e) = err {
          l.on_hover_text(e);
        }
        ui.label(RichText::new(&p.comment).small());
        if ui.small_button("🗑").on_hover_text("Delete (only if unused)").clicked() {
          app.run("param.remove", json!({"name": p.name}));
        }
        ui.end_row();
      }
    });
    ui.separator();
    let id = egui::Id::new("new-param");
    let (mut name, mut expr) = ui.data_mut(|d| d.get_temp::<(String, String)>(id)).unwrap_or_default();
    ui.horizontal(|ui| {
      ui.label("New:");
      ui.add(egui::TextEdit::singleline(&mut name).hint_text("name").desired_width(90.0));
      ui.add(egui::TextEdit::singleline(&mut expr).hint_text("40 mm").desired_width(120.0));
      if ui.button("＋ Add").clicked() && !name.is_empty() && !expr.is_empty() {
        let kind = if expr.contains("deg") || expr.contains('°') { "angle" } else { "length" };
        if app.run("param.add", json!({"name": name, "expr": expr, "kind": kind})).is_some() {
          name.clear();
          expr.clear();
        }
      }
    });
    ui.data_mut(|d| d.insert_temp(id, (name, expr)));
  });
  if !open {
    app.show_params = false;
  }
}
