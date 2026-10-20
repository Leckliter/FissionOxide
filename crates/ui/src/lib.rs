//! The FissionOxide desktop app: a Fusion 360-style modeling workspace on egui.
//!
//! Layout, as in Fusion: the application bar (document tabs) and toolbar (workspace switcher,
//! tabs, tool groups) at the top; the canvas with the browser, comments, navigation bar,
//! ViewCube, sketch palette and command dialogs floating over it; the timeline at the bottom. The UI is thin: every action goes through [`Session::execute`] commands.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

mod assembly_forms;
mod cam_ui;
mod config_ui;
mod cut_ui;
mod data_panel;
mod dialogs;
mod forms;
mod gpu;
mod icons;
mod logo;
mod manipulators;
mod marking;
mod menus;
mod panels;
mod preview;
mod select;
mod sheet;
mod theme;
mod trace;
mod updates;
mod viewport;

use std::path::PathBuf;

use egui::TextureHandle;
use fox_engine::render::Camera;
use fox_engine::scene::{Pick, PickTable, SceneBuilder};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

pub use dialogs::Dialog;
pub use updates::Updates;
pub use viewport::{NavMode, Tool};

pub struct App {
  pub s: Session,
  pub cam: Camera,
  scene: SceneBuilder,
  picks: PickTable,
  frame: Option<fox_engine::render::Frame>,
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
  last_sketch: Option<fox_engine::model::FeatureId>,
  /// The data panel (projects, designs, versions).
  pub data: data_panel::DataPanel,
  /// The drawing open in the Drawing workspace; None = the Design workspace.
  pub drawing: Option<u32>,
  /// The open drawing's rendered sheet, keyed by (design revision, drawing, sheet).
  sheet_cache: Option<((u64, u32, u32), Result<fox_engine::model::plot::Plot, String>)>,
  /// Sheet zoom and pan in the Drawing workspace.
  sheet_view: (f32, egui::Vec2),
  /// What clicks and window selection pick: faces (default), edges, bodies or components.
  pub sel_priority: select::Priority,
  /// Which kinds of things can be picked.
  pub sel_filter: select::Filters,
  /// The open Select Other list.
  pub(crate) other: Option<select::OtherMenu>,
  /// Where the last viewport click was (Select Other from the menu opens there).
  pub(crate) last_click: Option<egui::Pos2>,
  /// The Measure dialog's result line: closest points and the label.
  pub(crate) measure: Option<(fox_engine::kernel::glam::DVec3, fox_engine::kernel::glam::DVec3, String)>,
  /// Section analysis: `{plane, offset, flip}` (see [`fox_engine::scene::view_options`]).
  pub section: Option<Value>,
  /// Surface analysis: `{analysis, pull?, angle?}`.
  pub analysis: Option<Value>,
  /// A regeneration running in the background, and when it started. While it runs the UI shows
  /// the last image and takes no input (nothing may call the kernel; see `Session::begin`).
  pub(crate) rebuild: Option<(fox_engine::Rebuild, f64)>,
  /// In the Manufacture workspace: the MILLING tab, toolpaths and stock drawn.
  pub manufacture: bool,
  /// The tool library window, while open.
  pub tool_library: Option<cam_ui::ToolLibrary>,
  /// The GPU viewport renderer and its setting.
  pub(crate) gpu: gpu::Gpu,
  /// What a left-drag with the select tool does: window, freeform or paint selection.
  pub sel_mode: select::Mode,
  /// The freeform loop or paint stroke being dragged.
  pub(crate) region: select::Region,
  /// The move/rotate triad drag in progress.
  pub(crate) manip: manipulators::State,
  /// The bodies as the open dialog found them, drawn as ghosts behind its preview.
  pub(crate) ghost: preview::Ghost,
  /// The 2D cut window (laser, plasma, waterjet), while open.
  pub cut_window: Option<cut_ui::CutWindow>,
  /// Software updates (Help → Check for Updates).
  pub updates: updates::Updates,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RenameTarget {
  Feature(u32),
  Body(u32),
  Component(u32),
}

/// Everything that changes the rendered image.
#[derive(Debug, Clone, PartialEq)]
struct RenderKey {
  revision: u64,
  cam: Camera,
  size: [u32; 2],
  hover: Option<Pick>,
  selection: Vec<Sel>,
  edit_sketch: Option<fox_engine::model::FeatureId>,
  show_edges: bool,
  show_grid: bool,
  visible: Vec<(u32, bool)>,
  profiles_of: Vec<fox_engine::model::FeatureId>,
  /// Section and analysis settings.
  view: Value,
  /// Body vertices drawn (the Vertices selection filter).
  vertices: bool,
}

impl App {
  pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> App {
    theme::apply(&cc.egui_ctx);
    let mut app = App::with_session(Session::default());
    app.gpu = gpu::Gpu::new(cc.gl.clone());
    app.updates = updates::Updates::start(&cc.egui_ctx);
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
      drawing: None,
      sheet_cache: None,
      sheet_view: (1.0, egui::Vec2::ZERO),
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
      sel_priority: select::Priority::Face,
      sel_filter: select::Filters::default(),
      other: None,
      last_click: None,
      measure: None,
      section: None,
      analysis: None,
      rebuild: None,
      manufacture: false,
      tool_library: None,
      gpu: gpu::Gpu::new(None),
      sel_mode: select::Mode::default(),
      region: select::Region::default(),
      manip: manipulators::State::default(),
      ghost: preview::Ghost::default(),
      cut_window: None,
      updates: updates::Updates::default(),
    }
  }

  /// Runs a command whose regeneration can happen in the background (parameter changes, Compute
  /// All), so a long timeline doesn't freeze the window.
  pub fn run_background(&mut self, cmd: &str, params: Value) {
    if self.rebuild.is_some() {
      self.status = Some(("Still computing — wait for it to finish".into(), false));
      return;
    }
    self.cancel_dialog();
    match self.s.begin(cmd, &params) {
      Ok((_, Some(job))) => self.rebuild = Some((job, now())),
      Ok((_, None)) => {}
      Err(e) => self.status = Some((e.0, true)),
    }
  }

  /// Takes a finished background regeneration back. Returns whether one is still running.
  fn poll_rebuild(&mut self) -> bool {
    if self.rebuild.as_ref().is_some_and(|(job, _)| job.is_finished())
      && let Some((job, start)) = self.rebuild.take()
    {
      self.s.finish_rebuild(job);
      let errors = self.s.model.errors();
      self.status = Some(match errors.first() {
        Some(e) => (e.clone(), true),
        None => (format!("Computed in {:.1} s", now() - start), false),
      });
    }
    self.rebuild.is_some()
  }

  /// Runs a command, reporting errors in the status bar. Returns the result on success.
  pub fn run(&mut self, cmd: &str, params: Value) -> Option<Value> {
    if self.rebuild.is_some() {
      self.status = Some(("Still computing — wait for it to finish".into(), false));
      return None;
    }
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
    if trace::is_image(p) {
      // a picture opens as a new design with a sketch to trace it into
      self.replace_session(Session::default());
      self.trace_image(p.to_path_buf());
      return;
    }
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
    self.drawing = None;
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
      .unwrap_or((fox_engine::kernel::glam::DVec3::splat(-50.0), fox_engine::kernel::glam::DVec3::splat(50.0)));
    let aspect = self.frame.as_ref().map(|f| f.width as f64 / f.height.max(1) as f64).unwrap_or(1.6);
    self.cam.fit(lo, hi, aspect);
  }

  /// The data hub's folder.
  pub fn hub_root(&self) -> PathBuf {
    self.s.store_root.clone().unwrap_or_else(fox_engine::store::Store::default_root)
  }

  /// The window title: the design's name and version, and • when it has unsaved changes.
  pub fn title(&self) -> String {
    let name = match (&self.s.hub, &self.s.path) {
      (Some(h), _) => format!("{} v{}", self.s.design.name, h.version),
      (None, Some(p)) => {
        p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| self.s.design.name.clone())
      }
      (None, None) => self.s.design.name.clone(),
    };
    format!("{}{} — FissionOxide", name, if self.s.dirty { " •" } else { "" })
  }

  // ---- file actions ----

  pub fn new_design(&mut self) {
    let root = self.s.store_root.clone();
    self.s = Session::default();
    self.drawing = None;
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

  /// Save to a file on this computer (.fox, or .foxz archive).
  pub fn save_file(&mut self, save_as: bool) {
    let path = match (&self.s.path, save_as) {
      (Some(p), false) => Some(p.clone()),
      _ => rfd::FileDialog::new()
        .add_filter("FissionOxide design", &["fox"])
        .add_filter("FissionOxide archive (design + thumbnail)", &["foxz"])
        .set_file_name(format!("{}.fox", self.s.design.name))
        .save_file(),
    };
    if let Some(p) = path
      && self.run("save", json!({"path": p})).is_some()
    {
      self.status = Some((format!("Saved {}", p.display()), false));
    }
  }

  /// Export in any writable format (by its id in [`fox_engine::exchange::FORMATS`]).
  pub fn export(&mut self, id: &str) {
    let Some(f) = fox_engine::exchange::FORMATS.iter().find(|f| f.id == id) else { return };
    let ext = f.extensions.first().copied().unwrap_or(id);
    if let Some(p) = rfd::FileDialog::new()
      .add_filter(f.name, f.extensions)
      .set_file_name(format!("{}.{ext}", self.s.design.name))
      .save_file()
    {
      let mut params = json!({"path": p});
      if id == "dxf" || id == "svg" {
        params["sketch"] = json!(self.s.edit_sketch.or(self.last_sketch).map(|s| s.0));
      }
      if self.run("export", params).is_some() {
        self.status = Some((format!("Exported {}", p.display()), false));
      }
    }
  }

  /// Computes the last setup's toolpaths; `report` puts the machining time in the status bar.
  pub fn cam_generate(&mut self, report: bool) {
    if self.s.design.cam.is_empty() {
      if report {
        self.status = Some(("Create a setup first (SETUP ▸ New Setup)".into(), false));
      }
      return;
    }
    match self.s.execute("cam.generate", &json!({})) {
      Ok(r) if report => {
        let ops = r["operations"].as_array().map(Vec::len).unwrap_or(0);
        let min = r["minutes"].as_f64().unwrap_or(0.0);
        self.status = Some((
          format!("{ops} operation(s) · about {:.0} min {:.0} s machining", min.floor(), (min.fract() * 60.0).round()),
          false,
        ));
      }
      Ok(_) => {}
      Err(e) => {
        self.s.toolpaths.clear();
        self.s.toolpaths_of = None;
        self.status = Some((e.0, true));
      }
    }
  }

  /// Writes the last setup's G-code to a file the user picks.
  pub fn cam_post(&mut self) {
    if self.s.design.cam.is_empty() {
      self.status = Some(("Create a setup first (SETUP ▸ New Setup)".into(), false));
      return;
    }
    let st = self.s.design.cam.last().map(|st| st.post.clone()).unwrap_or_default();
    let ext = fox_engine::model::cam::post::find(&st).map(|p| p.extension).unwrap_or("nc");
    if let Some(p) = rfd::FileDialog::new()
      .add_filter("G-code", &["nc", "tap", "gcode", "ngc"])
      .set_file_name(format!("{}.{ext}", self.s.design.name))
      .save_file()
      && let Some(r) = self.run("cam.post", json!({"path": p}))
    {
      self.status = Some((format!("Wrote {} lines of G-code to {}", r["lines"], p.display()), false));
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
      } else if trace::is_image(&p) {
        self.trace_image(p);
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
    if pressed(Key::G, Modifiers::COMMAND) && self.manufacture {
      self.cam_generate(true);
    }
    if pressed(Key::B, Modifiers::COMMAND) && self.drawing.is_none() {
      self.run_background("compute_all", json!({}));
    }
    if pressed(Key::A, Modifiers::COMMAND) && self.drawing.is_none() {
      self.run("select.all", json!({}));
    }
    if pressed(Key::Escape, Modifiers::NONE) {
      if self.marking.is_some() {
        self.marking = None;
      } else if self.dim_edit.is_some() {
        self.dim_edit = None;
      } else if self.nav_mode.is_some() {
        self.nav_mode = None;
      } else if self.tool == Tool::Spline && viewport::finish_spline(self) {
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
    if self.drawing.is_some() {
      // the model's shortcuts don't apply on a drawing sheet
      return;
    }
    if pressed(Key::Enter, Modifiers::NONE) && self.tool == Tool::Spline {
      viewport::finish_spline(self);
    }
    if pressed(Key::Delete, Modifiers::NONE) || pressed(Key::Backspace, Modifiers::NONE) {
      self.delete_selection();
    }
    if pressed(Key::F6, Modifiers::NONE) {
      self.fit_view();
    }
    if pressed(Key::Home, Modifiers::NONE) {
      self.cam.set_view(fox_engine::render::StandardView::Home);
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
        (Key::T, Tool::Trim),
      ] {
        if pressed(k, Modifiers::NONE) {
          self.set_tool(t);
        }
      }
      if pressed(Key::X, Modifiers::NONE) {
        self.toggle_construction();
      }
      if pressed(Key::O, Modifiers::NONE) {
        self.start(dialogs::Kind::SkOffset);
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
      for (k, d) in [
        (Key::I, dialogs::Kind::Inspect),
        (Key::H, dialogs::Kind::Hole),
        (Key::Q, dialogs::Kind::PressPull),
        (Key::J, dialogs::Kind::Joint),
        (Key::A, dialogs::Kind::Form(&forms::APPEARANCE)),
      ] {
        if pressed(k, Modifiers::NONE) {
          self.start(d);
        }
      }
      // window, freeform and paint selection
      for (k, m) in
        [(Key::Num1, select::Mode::Window), (Key::Num2, select::Mode::Freeform), (Key::Num3, select::Mode::Paint)]
      {
        if pressed(k, Modifiers::NONE) {
          self.sel_mode = m;
        }
      }
    }
  }

  /// Switches to the Drawing workspace: the last drawing, or Create Drawing when there is none.
  pub fn open_drawing_workspace(&mut self) {
    match self.s.design.drawings.last().map(|d| d.id) {
      Some(id) => self.open_drawing(id),
      None => self.start(dialogs::Kind::NewDrawing),
    }
  }

  pub fn open_drawing(&mut self, id: u32) {
    self.cancel_dialog();
    if self.s.edit_sketch.is_some() {
      self.finish_sketch();
    }
    self.drawing = Some(id);
    self.sheet_view = (1.0, egui::Vec2::ZERO);
  }

  /// Back to the Design workspace.
  pub fn close_drawing(&mut self) {
    if self.dialog.as_ref().is_some_and(|d| d.kind.drawing()) {
      self.cancel_dialog();
    }
    self.drawing = None;
    self.tab = panels::Tab::Solid;
  }

  /// Writes the open drawing as .pdf, .dxf or .svg.
  pub fn export_drawing(&mut self, ext: &str) {
    let Some(id) = self.drawing else { return };
    let name = self.s.design.drawing(id).map(|d| d.name.clone()).unwrap_or_else(|| "drawing".into());
    if let Some(p) =
      rfd::FileDialog::new().add_filter(ext.to_uppercase(), &[ext]).set_file_name(format!("{name}.{ext}")).save_file()
      && self.run("drawing.export", json!({"drawing": id, "path": p})).is_some()
    {
      self.status = Some((format!("Exported {}", p.display()), false));
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
    self.ghost.capture(&self.s);
    self.dialog = Some(Dialog::open(self, kind));
    if let Some(d) = self.dialog.as_mut() {
      // manipulators hang on the picks as they are before the preview changes them
      d.refresh_manip(&self.s);
      d.preview(&mut self.s);
    }
  }

  /// Opens Trace Image on `path` (an opened or dropped image) in the sketch being edited, or in
  /// a new sketch on the XY plane.
  pub fn trace_image(&mut self, path: PathBuf) {
    if self.s.edit_sketch.is_none() {
      self.create_sketch(json!("xy"));
      if self.s.edit_sketch.is_none() {
        return;
      }
    }
    self.cancel_dialog();
    self.tool = Tool::Select;
    let mut d = Dialog::open(self, dialogs::Kind::TraceImage);
    d.trace = Some(Box::new(trace::TraceState::with_image(path)));
    d.preview(&mut self.s);
    self.dialog = Some(d);
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
        .and_then(|b| b.faces().iter().find(|f| f.id == face).map(|f| f.kind == fox_engine::kernel::SurfaceKind::Plane))
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

  pub fn edit_sketch(&mut self, id: fox_engine::model::FeatureId) {
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
      (f.kind == fox_engine::kernel::SurfaceKind::Plane).then_some(f.normal)
    });
    match normal {
      Some(n) => self.look_along(n),
      None => self.status = Some(("Look At: select a planar face".into(), false)),
    }
  }

  /// Turns the camera to look straight at a plane with normal `n`.
  fn look_along(&mut self, n: fox_engine::kernel::glam::DVec3) {
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
    if self.poll_rebuild() {
      self.draw_busy(ui, &ctx);
      return;
    }
    if self.data.save.is_none() {
      self.shortcuts(&ctx);
    }
    // the Manufacture workspace keeps its toolpaths in step with the design
    if self.manufacture && !self.s.design.cam.is_empty() && self.s.toolpaths_of.map(|t| t.1) != Some(self.s.revision) {
      self.cam_generate(false);
      self.s.toolpaths_of = Some((self.s.toolpaths_of.map(|t| t.0).unwrap_or(0), self.s.revision));
    }
    self.dropped_files(&ctx);
    panels::app_bar(self, ui);
    panels::toolbar(self, ui);
    if self.drawing.is_none() {
      panels::timeline(self, ui);
    }
    data_panel::show(self, ui);
    if self.drawing.is_some() {
      egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
        sheet::show(self, ui);
      });
    } else {
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
    }
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
    updates::window(self, &ctx);
    cam_ui::tool_library_window(self, &ctx);
    config_ui::table_window(self, &ctx);
    cut_ui::cut_window(self, &ctx);
    if !self.fitted_once && self.frame.is_some() {
      self.fitted_once = true;
      if !self.s.model.bodies.is_empty() {
        self.fit_view();
      }
    }
  }
}

impl App {
  /// The window while a background regeneration runs: the panels as they were, the last
  /// viewport image dimmed with a progress note, and a cover that takes all input.
  fn draw_busy(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
    panels::app_bar(self, ui);
    panels::toolbar(self, ui);
    if self.drawing.is_none() {
      panels::timeline(self, ui);
    }
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
      let rect = ui.available_rect_before_wrap();
      let painter = ui.painter_at(rect);
      gpu::paint_last(self, &painter, rect, egui::Color32::from_gray(200));
      if let Some(tex) = &self.tex {
        painter.image(
          tex.id(),
          rect,
          egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
          egui::Color32::from_gray(200),
        );
      }
    });
    panels::browser(self, ctx);
    let elapsed = self.rebuild.as_ref().map(|(_, t)| now() - t).unwrap_or(0.0);
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("busy-cover")).fixed_pos(screen.min).order(egui::Order::Foreground).show(ctx, |ui| {
      let (r, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
      ui.painter().rect_filled(r, 0.0, egui::Color32::from_black_alpha(16));
      if elapsed > 0.15 {
        let c = self.canvas.center();
        let text = format!("Computing…  {elapsed:.1} s");
        let g = ui.painter().layout_no_wrap(text, egui::FontId::proportional(15.0), theme::TEXT);
        let b = egui::Rect::from_center_size(c, g.size() + egui::vec2(36.0, 22.0));
        ui.painter().rect_filled(b, 6.0, egui::Color32::WHITE);
        ui.painter().rect_stroke(b, 6.0, egui::Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
        let a = (elapsed * 6.0) as f32;
        let dot = b.left_center() + egui::vec2(14.0, 0.0);
        ui.painter().circle_stroke(dot, 5.0, egui::Stroke::new(1.0, theme::BORDER));
        ui.painter().circle_filled(
          dot + egui::vec2(a.cos(), a.sin()) * 5.0,
          2.0,
          egui::Color32::from_rgb(30, 110, 235),
        );
        ui.painter().galley(b.min + egui::vec2(26.0, 11.0), g, theme::TEXT);
      }
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(50));
  }
}

/// Seconds since the app started (for timing background work).
fn now() -> f64 {
  static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
  START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
}

impl eframe::App for App {
  fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    self.draw(ui);
  }

  fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
    self.updates.on_exit();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parameter_changes_run_in_the_background() {
    let mut app = App::with_session(Session::default());
    for (c, p) in [
      ("param.add", json!({"name": "h", "expr": "10"})),
      ("sketch.create", json!({"plane": "xy"})),
      ("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]})),
      ("extrude", json!({"distance": "h"})),
    ] {
      app.run(c, p).unwrap();
    }
    app.run_background("param.set", json!({"name": "h", "expr": "30"}));
    assert!(app.rebuild.is_some());
    // commands wait for it
    assert!(app.run("info", json!({})).is_none());
    while app.poll_rebuild() {
      std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let v = app.s.model.bodies[0].shape.mass().unwrap().volume;
    assert!((v - 3000.0).abs() < 1e-6, "{v}");
    app.run_background("param.set", json!({"name": "h", "expr": "1 +"}));
    assert!(app.rebuild.is_none() && app.status.as_ref().is_some_and(|s| s.1));
  }
}
