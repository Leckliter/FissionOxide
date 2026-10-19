//! The CLFusion desktop app: a Fusion 360-style modeling workspace on egui.
//!
//! Layout, as in Fusion: the application bar (document tabs) and toolbar (workspace switcher,
//! tabs, tool groups) at the top; the canvas with the browser, comments, navigation bar,
//! ViewCube, sketch palette and command dialogs floating over it; the timeline at the bottom. The UI is thin: every action goes through [`Session::execute`] commands.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

mod data_panel;
mod dialogs;
mod icons;
mod marking;
mod menus;
mod panels;
mod theme;
mod viewport;

use std::path::PathBuf;

use clf_engine::render::Camera;
use clf_engine::scene::{Pick, PickTable, SceneBuilder};
use clf_engine::{Sel, Session};
use egui::TextureHandle;
use serde_json::{Value, json};

pub use dialogs::Dialog;
pub use viewport::{NavMode, Tool};

pub struct App {
  pub s: Session,
  pub cam: Camera,
  scene: SceneBuilder,
  picks: PickTable,
  frame: Option<clf_engine::render::Frame>,
  tex: Option<TextureHandle>,
  render_key: Option<RenderKey>,
  pub hover: Option<Pick>,
  pub tool: Tool,
  pub dialog: Option<Dialog>,
  /// Status-bar message: (text, is_error).
  pub status: Option<(String, bool)>,
  pub show_params: bool,
  pub show_edges: bool,
  pub show_grid: bool,
  /// Sketch palette options.
  pub show_dims: bool,
  /// Snap sketch clicks to the grid.
  pub snap: bool,
  pub show_constraints: bool,
  /// Floating panels.
  pub browser_open: bool,
  pub show_comments: bool,
  pub show_navbar: bool,
  pub show_viewcube: bool,
  /// Navigation bar mode (orbit / pan / zoom with the left button).
  pub nav_mode: Option<NavMode>,
  /// Toolbar tab; opening a sketch switches to the contextual SKETCH tab.
  pub tab: panels::Tab,
  /// Timeline playback: time of the next step.
  playing: Option<f64>,
  /// Last repeatable command, for the marking menu's Repeat.
  last_cmd: Option<panels::Cmd>,
  /// The open right-click marking menu.
  marking: Option<marking::MarkingMenu>,
  /// The canvas (viewport) rectangle, for panels floating over it.
  pub(crate) canvas: egui::Rect,
  /// Inline dimension value editor: (constraint id, text, screen position).
  dim_edit: Option<(u32, String, egui::Pos2)>,
  /// Feature id being renamed in the timeline/browser.
  rename: Option<(RenameTarget, String)>,
  drag: Option<viewport::Drag>,
  tstate: viewport::ToolState,
  fitted_once: bool,
  last_sketch: Option<clf_engine::model::FeatureId>,
  /// The data panel (projects, designs, versions).
  pub data: data_panel::DataPanel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RenameTarget {
  Feature(u32),
  Body(u32),
}

/// Everything that changes the rendered image.
#[derive(Debug, Clone, PartialEq)]
struct RenderKey {
  revision: u64,
  cam: Camera,
  size: [u32; 2],
  hover: Option<Pick>,
  selection: Vec<Sel>,
  edit_sketch: Option<clf_engine::model::FeatureId>,
  show_edges: bool,
  show_grid: bool,
  visible: Vec<(u32, bool)>,
  profiles_of: Vec<clf_engine::model::FeatureId>,
}

impl App {
  pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> App {
    theme::apply(&cc.egui_ctx);
    let mut app = App::with_session(Session::default());
    match open {
      Some(p) => app.open_path(&p),
      // like Fusion, start with the data panel showing
      None => app.data.toggle(),
    }
    app
  }

  pub fn with_session(s: Session) -> App {
    App {
      s,
      cam: Camera::default(),
      scene: SceneBuilder::default(),
      picks: PickTable::default(),
      frame: None,
      tex: None,
      render_key: None,
      hover: None,
      tool: Tool::Select,
      dialog: None,
      status: None,
      show_params: false,
      show_edges: true,
      show_grid: true,
      show_dims: true,
      snap: false,
      show_constraints: true,
      browser_open: true,
      show_comments: true,
      show_navbar: true,
      show_viewcube: true,
      nav_mode: None,
      tab: panels::Tab::Solid,
      playing: None,
      last_cmd: None,
      marking: None,
      canvas: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 700.0)),
      dim_edit: None,
      rename: None,
      drag: None,
      tstate: viewport::ToolState::default(),
      fitted_once: false,
      last_sketch: None,
      data: data_panel::DataPanel::default(),
    }
  }

  /// Runs a command, reporting errors in the status bar. Returns the result on success.
  pub fn run(&mut self, cmd: &str, params: Value) -> Option<Value> {
    match self.s.execute(cmd, &params) {
      Ok(v) => {
        if let Some(e) = v.get("error").and_then(Value::as_str) {
          self.status = Some((format!("{cmd}: {e}"), true));
        } else if let Some(w) = v.get("warning").and_then(Value::as_str) {
          self.status = Some((w.to_string(), false));
        }
        Some(v)
      }
      Err(e) => {
        self.status = Some((e.0, true));
        None
      }
    }
  }

  pub fn open_path(&mut self, p: &std::path::Path) {
    match Session::open(p) {
      Ok(s) => {
        self.replace_session(s);
        let msg = match self.s.path {
          Some(_) => format!("Opened {}", p.display()),
          None => format!("Opened {} as a new design (Save to keep it)", p.display()),
        };
        self.status = Some((msg, false));
      }
      Err(e) => self.status = Some((format!("Couldn't open {}: {e}", p.display()), true)),
    }
  }

  /// Switches to another session, keeping the hub location.
  fn replace_session(&mut self, mut s: Session) {
    s.store_root = self.s.store_root.clone();
    self.s = s;
    self.dialog = None;
    self.tool = Tool::Select;
    self.fit_view();
    self.data.invalidate();
  }

  /// Opens a design (latest or a given version) from the data panel.
  pub fn open_hub_item(&mut self, item: &str, version: Option<u32>) {
    self.cancel_dialog();
    if let Some(r) = self.run("hub.open", json!({"item": item, "version": version})) {
      self.dialog = None;
      self.tool = Tool::Select;
      self.fit_view();
      self.data.invalidate();
      if r.get("warning").is_none() {
        let v = r.get("version").and_then(Value::as_u64).unwrap_or(0);
        self.status = Some((format!("Opened {} v{v}", self.s.design.name), false));
      }
    }
  }

  pub fn fit_view(&mut self) {
    let (lo, hi) = self
      .s
      .model
      .bounds()
      .unwrap_or((clf_engine::kernel::glam::DVec3::splat(-50.0), clf_engine::kernel::glam::DVec3::splat(50.0)));
    let aspect = self.frame.as_ref().map(|f| f.width as f64 / f.height.max(1) as f64).unwrap_or(1.6);
    self.cam.fit(lo, hi, aspect);
  }

  fn title(&self) -> String {
    let name = match (&self.s.hub, &self.s.path) {
      (Some(h), _) => format!("{} v{}", self.s.design.name, h.version),
      (None, Some(p)) => {
        p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| self.s.design.name.clone())
      }
      (None, None) => self.s.design.name.clone(),
    };
    format!("{}{} — CLFusion", name, if self.s.dirty { " •" } else { "" })
  }

  // ---- file actions ----

  pub fn new_design(&mut self) {
    let root = self.s.store_root.clone();
    self.s = Session::default();
    self.s.store_root = root;
    self.dialog = None;
    self.tool = Tool::Select;
    self.nav_mode = None;
    self.cam = Camera::default();
    self.fitted_once = false;
  }

  /// Open from this computer: designs, or any importable file as a new design.
  pub fn open_dialog(&mut self) {
    if let Some(p) = data_panel::pick_importable("Open", false).into_iter().next() {
      self.open_path(&p);
    }
  }

  /// Save (Ctrl+S): a new version in the data panel. The first save, or Save As, asks for a
  /// name and location.
  pub fn save(&mut self, save_as: bool) {
    self.save_to_hub(save_as);
  }

  pub fn save_to_hub(&mut self, as_new: bool) {
    let (project, folder) = self.data.location();
    self.data.invalidate();
    let name = if as_new && self.s.hub.is_some() {
      format!("{} (copy)", self.s.design.name)
    } else {
      self.s.design.name.clone()
    };
    self.data.save = Some(data_panel::SaveForm { name, as_new, project, folder, ..Default::default() });
  }

  /// Save to a file on this computer (.clf, or .clfz archive).
  pub fn save_file(&mut self, save_as: bool) {
    let path = match (&self.s.path, save_as) {
      (Some(p), false) => Some(p.clone()),
      _ => rfd::FileDialog::new()
        .add_filter("CLFusion design", &["clf"])
        .add_filter("CLFusion archive (design + thumbnail)", &["clfz"])
        .set_file_name(format!("{}.clf", self.s.design.name))
        .save_file(),
    };
    if let Some(p) = path
      && self.run("save", json!({"path": p})).is_some()
    {
      self.status = Some((format!("Saved {}", p.display()), false));
    }
  }

  /// Export in any writable format (by its id in [`clf_engine::exchange::FORMATS`]).
  pub fn export(&mut self, id: &str) {
    let Some(f) = clf_engine::exchange::FORMATS.iter().find(|f| f.id == id) else { return };
    let ext = f.extensions.first().copied().unwrap_or(id);
    if let Some(p) = rfd::FileDialog::new()
      .add_filter(f.name, f.extensions)
      .set_file_name(format!("{}.{ext}", self.s.design.name))
      .save_file()
    {
      let mut params = json!({"path": p});
      if id == "dxf" {
        params["sketch"] = json!(self.s.edit_sketch.or(self.last_sketch).map(|s| s.0));
      }
      if self.run("export", params).is_some() {
        self.status = Some((format!("Exported {}", p.display()), false));
      }
    }
  }

  /// Insert a file into the current design.
  pub fn import(&mut self) {
    for p in data_panel::pick_importable("Insert into the design", true) {
      self.import_path(&p);
    }
  }

  pub fn import_path(&mut self, p: &std::path::Path) {
    if let Some(r) = self.run("import", json!({"path": p})) {
      if r.get("warning").is_none() {
        self.status = Some((format!("Inserted {}", p.display()), false));
      }
      self.fit_view();
    }
  }

  /// Files dropped on the window: uploaded when the data panel is open, otherwise opened (into
  /// an empty design) or inserted.
  fn dropped_files(&mut self, ctx: &egui::Context) {
    let files: Vec<PathBuf> =
      ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| p.is_absolute()).collect());
    for p in files {
      if self.data.open {
        let (project, folder) = self.data.location();
        if let Some(r) = self.run("hub.upload", json!({"path": p, "project": project, "folder": folder}))
          && r.get("warning").is_none()
        {
          self.status = Some((format!("Uploaded {}", p.display()), false));
        }
        self.data.invalidate();
      } else if self.s.design.features.is_empty() && !self.s.dirty {
        self.open_path(&p);
      } else {
        self.import_path(&p);
      }
    }
  }

  fn shortcuts(&mut self, ctx: &egui::Context) {
    if ctx.egui_wants_keyboard_input() {
      return;
    }
    use egui::{Key, Modifiers};
    let pressed = |k: Key, m: Modifiers| ctx.input_mut(|i| i.consume_key(m, k));
    if pressed(Key::Z, Modifiers::COMMAND) {
      self.cancel_dialog();
      self.s.undo();
    }
    if pressed(Key::Y, Modifiers::COMMAND) || pressed(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT) {
      self.s.redo();
    }
    if pressed(Key::S, Modifiers::COMMAND | Modifiers::SHIFT) {
      self.save(true);
    }
    if pressed(Key::S, Modifiers::COMMAND) {
      self.save(false);
    }
    if pressed(Key::O, Modifiers::COMMAND) {
      self.open_dialog();
    }
    if pressed(Key::N, Modifiers::COMMAND) {
      self.new_design();
    }
    if pressed(Key::Escape, Modifiers::NONE) {
      if self.marking.is_some() {
        self.marking = None;
      } else if self.dim_edit.is_some() {
        self.dim_edit = None;
      } else if self.nav_mode.is_some() {
        self.nav_mode = None;
      } else if !self.tstate.is_empty() {
        viewport::reset_tool_state(self);
      } else if self.tool != Tool::Select {
        self.tool = Tool::Select;
      } else if self.dialog.is_some() {
        self.cancel_dialog();
      } else {
        self.s.selection.clear();
      }
      self.dim_edit = None;
    }
    if pressed(Key::Delete, Modifiers::NONE) || pressed(Key::Backspace, Modifiers::NONE) {
      self.delete_selection();
    }
    if pressed(Key::F6, Modifiers::NONE) {
      self.fit_view();
    }
    if pressed(Key::Home, Modifiers::NONE) {
      self.cam.set_view(clf_engine::render::StandardView::Home);
      self.fit_view();
    }
    let sketching = self.s.edit_sketch.is_some();
    if sketching {
      for (k, t) in [
        (Key::L, Tool::Line),
        (Key::R, Tool::Rect),
        (Key::C, Tool::Circle),
        (Key::A, Tool::Arc3),
        (Key::P, Tool::Point),
        (Key::D, Tool::Dimension),
      ] {
        if pressed(k, Modifiers::NONE) {
          self.set_tool(t);
        }
      }
      if pressed(Key::X, Modifiers::NONE) {
        self.toggle_construction();
      }
    } else {
      if pressed(Key::E, Modifiers::NONE) {
        self.start(dialogs::Kind::Extrude);
      }
      if pressed(Key::F, Modifiers::NONE) {
        self.start(dialogs::Kind::Fillet);
      }
      if pressed(Key::M, Modifiers::NONE) {
        self.start(dialogs::Kind::Move);
      }
      if pressed(Key::I, Modifiers::NONE) {
        self.start(dialogs::Kind::Inspect);
      }
    }
  }

  pub fn set_tool(&mut self, t: Tool) {
    self.tool = t;
    self.drag = None;
    viewport::reset_tool_state(self);
  }

  pub fn start(&mut self, kind: dialogs::Kind) {
    self.cancel_dialog();
    self.tool = Tool::Select;
    self.dialog = Some(Dialog::open(self, kind));
    if let Some(d) = self.dialog.as_mut() {
      d.preview(&mut self.s);
    }
  }

  pub fn cancel_dialog(&mut self) {
    if let Some(mut d) = self.dialog.take() {
      d.cancel(&mut self.s);
    }
  }

  /// Starts a sketch on the selected planar face, or asks for a plane.
  pub fn new_sketch(&mut self) {
    self.cancel_dialog();
    let face = self.s.selection.iter().find_map(|s| match s {
      Sel::Face { body, face } => Some((*body, *face)),
      _ => None,
    });
    if let Some((body, face)) = face {
      let planar = self
        .s
        .model
        .body(body)
        .and_then(|b| b.faces().iter().find(|f| f.id == face).map(|f| f.kind == clf_engine::kernel::SurfaceKind::Plane))
        .unwrap_or(false);
      if planar {
        self.create_sketch(json!({"body": body.0, "face": face}));
        return;
      }
    }
    self.dialog = Some(Dialog::open(self, dialogs::Kind::PickPlane));
  }

  pub fn create_sketch(&mut self, plane: Value) {
    if self.run("sketch.create", json!({"plane": plane})).is_some() {
      self.s.selection.clear();
      self.look_at_sketch();
      self.tab = panels::Tab::Sketch;
      self.tool = Tool::Line;
      self.status = Some(("Sketching — draw with the Create tools; Finish Sketch when done".into(), false));
    }
  }

  pub fn edit_sketch(&mut self, id: clf_engine::model::FeatureId) {
    self.cancel_dialog();
    if self.run("sketch.edit", json!({"sketch": id.0})).is_some() {
      self.look_at_sketch();
      self.tab = panels::Tab::Sketch;
      self.tool = Tool::Select;
    }
  }

  pub fn finish_sketch(&mut self) {
    self.last_sketch = self.s.edit_sketch;
    self.run("sketch.finish", json!({}));
    self.tool = Tool::Select;
    self.s.selection.clear();
  }

  fn look_at_sketch(&mut self) {
    let Some(r) = self.s.edit_sketch.and_then(|e| self.s.model.sketches.get(&e)) else { return };
    let n = r.plane.normal();
    let o = r.plane.origin;
    self.look_along(n);
    if self.s.model.bodies.is_empty() {
      self.cam.target = o.to_array();
    }
  }

  /// Navigation bar Look At: the open sketch's plane, else the selected planar face.
  pub fn look_at_selection(&mut self) {
    if self.s.edit_sketch.is_some() {
      self.look_at_sketch();
      return;
    }
    let face = self.s.selection.iter().find_map(|s| match s {
      Sel::Face { body, face } => Some((*body, *face)),
      _ => None,
    });
    let normal = face.and_then(|(body, face)| {
      let b = self.s.model.body(body)?;
      let f = b.faces().iter().find(|f| f.id == face)?;
      (f.kind == clf_engine::kernel::SurfaceKind::Plane).then_some(f.normal)
    });
    match normal {
      Some(n) => self.look_along(n),
      None => self.status = Some(("Look At: select a planar face".into(), false)),
    }
  }

  /// Turns the camera to look straight at a plane with normal `n`.
  fn look_along(&mut self, n: clf_engine::kernel::glam::DVec3) {
    self.cam.pitch =
      n.z.clamp(-1.0, 1.0).asin().clamp(-std::f64::consts::FRAC_PI_2 + 1e-6, std::f64::consts::FRAC_PI_2 - 1e-6);
    if n.z.abs() < 0.999 {
      self.cam.yaw = n.x.atan2(-n.y);
    } else {
      self.cam.yaw = 0.0;
    }
  }

  pub fn toggle_construction(&mut self) {
    let curves: Vec<u32> = self
      .s
      .selection
      .iter()
      .filter_map(|s| if let Sel::SketchCurve { curve, .. } = s { Some(curve.0) } else { None })
      .collect();
    if !curves.is_empty() {
      self.run("sketch.construction", json!({"curves": curves}));
    }
  }

  pub fn delete_selection(&mut self) {
    if let Some(sk) = self.s.edit_sketch {
      let mut curves = Vec::new();
      let mut points = Vec::new();
      let mut cons = Vec::new();
      for s in &self.s.selection {
        match s {
          Sel::SketchCurve { curve, .. } => curves.push(curve.0),
          Sel::SketchPoint { point, .. } => points.push(point.0),
          Sel::SketchConstraint { constraint, .. } => cons.push(constraint.0),
          _ => {}
        }
      }
      if !(curves.is_empty() && points.is_empty() && cons.is_empty()) {
        self.run("sketch.delete", json!({"sketch": sk.0, "curves": curves, "points": points, "constraints": cons}));
        self.s.selection.clear();
      }
      return;
    }
    let feats: Vec<u32> = self
      .s
      .selection
      .iter()
      .filter_map(|s| if let Sel::Feature { feature } = s { Some(feature.0) } else { None })
      .collect();
    for f in feats {
      self.run("feature.delete", json!({"feature": f}));
    }
    let bodies: Vec<u32> =
      self.s.selection.iter().filter_map(|s| if let Sel::Body { body } = s { Some(body.0) } else { None }).collect();
    if !bodies.is_empty() {
      self.run("body.remove", json!({"bodies": bodies}));
    }
    self.s.selection.clear();
  }

  /// The full UI for one frame.
  pub fn draw(&mut self, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
    if self.data.save.is_none() {
      self.shortcuts(&ctx);
    }
    self.dropped_files(&ctx);
    panels::app_bar(self, ui);
    panels::toolbar(self, ui);
    panels::timeline(self, ui);
    data_panel::show(self, ui);
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
      viewport::show(self, ui);
    });
    if self.show_comments {
      panels::comments(self, &ctx);
    }
    panels::browser(self, &ctx);
    if self.show_navbar {
      panels::nav_bar(self, &ctx);
    }
    panels::assistant_tab(self, &ctx);
    panels::sketch_palette(self, &ctx);
    marking::show(self, &ctx);
    panels::canvas_status(self, &ctx);
    panels::unsaved_banner(self, &ctx);
    panels::selection_info(self, &ctx);
    if let Some(mut d) = self.dialog.take() {
      let keep = d.show(self, &ctx);
      if keep && self.dialog.is_none() {
        self.dialog = Some(d);
      }
    }
    if self.show_params {
      panels::parameters_window(self, &ctx);
    }
    data_panel::save_window(self, &ctx);
    if !self.fitted_once && self.frame.is_some() {
      self.fitted_once = true;
      if !self.s.model.bodies.is_empty() {
        self.fit_view();
      }
    }
  }
}

impl eframe::App for App {
  fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    self.draw(ui);
  }
}
