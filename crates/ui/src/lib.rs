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
#[cfg(test)]
mod input_tests;
mod logo;
mod manipulators;
mod marking;
mod menus;
mod panels;
mod preview;
mod select;
mod sheet;
mod theme;
mod toolbox;
mod trace;
mod updates;
mod viewcube;
mod viewport;

use std::path::PathBuf;

use egui::TextureHandle;
use fox_engine::render::Camera;
use fox_engine::scene::{Pick, PickTable, SceneBuilder};
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

pub use dialogs::Dialog;
pub use logo::{Splash, SplashPhase};
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
  /// Recently started commands, newest first (the S toolbox's recent row).
  recent: Vec<panels::Cmd>,
  /// The open S toolbox.
  toolbox: Option<toolbox::Toolbox>,
  /// The open right-click marking menu.
  marking: Option<marking::MarkingMenu>,
  /// Context-menu state: browser/timeline hover highlight, Find in Browser.
  pub(crate) ctx_menu: marking::MenuState,
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
  /// A camera move in progress: (from, to, start time), eased over [`CAM_MOVE_SECS`].
  cam_move: Option<(Camera, Camera, f64)>,
  /// The 3D view before the open sketch turned the camera to its plane; finishing the sketch
  /// goes back to it, as in Fusion.
  pre_sketch_cam: Option<Camera>,
  /// The ViewCube's orientation, Home view and projection mode.
  cube: viewcube::CubeState,
  /// An origin or construction plane selected in the browser (a `sketch.create` plane: "xy" or
  /// `{feature}`); Create Sketch starts on it, and the viewport highlights it.
  pub(crate) picked_plane: Option<Value>,
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
  /// A design being opened on another thread (the one the app was started with).
  opening: Option<(PathBuf, std::thread::JoinHandle<Result<Session, fox_engine::CmdError>>)>,
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
  /// Bodies highlighted from the browser or timeline, and the isolated bodies.
  peek: Vec<u32>,
  isolate: Option<Vec<u32>>,
}

impl App {
  pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> App {
    theme::apply(&cc.egui_ctx);
    let mut app = App::with_session(Session::default());
    app.gpu = gpu::Gpu::new(cc.gl.clone());
    app.updates = updates::Updates::start(&cc.egui_ctx);
    // the first start puts the fox in 3D in the hub, and shows it unless a file was asked for
    let welcome = fox_engine::hub::add_welcome(&app.hub_root()).unwrap_or_else(|e| {
      log::warn!("couldn't add the welcome design to the hub: {}", e.0);
      None
    });
    match (open, welcome) {
      // read on another thread, so the start-up splash keeps playing while a big design loads
      (Some(p), _) if !trace::is_image(&p) => app.open_in_background(p),
      (Some(p), _) => app.open_path(&p),
      (None, Some(item)) => app.open_hub_item(&item, None),
      // like Fusion, start with the data panel showing
      (None, None) => app.data.toggle(),
    }
    app
  }

  /// Opens a design (or importable file) on another thread; [`App::draw`] takes it when it's read.
  pub fn open_in_background(&mut self, p: PathBuf) {
    let path = p.clone();
    self.opening = Some((p, std::thread::spawn(move || Session::open(&path))));
  }

  /// Opens a file given from outside the app (a later launch's argument): a design on another
  /// thread, a picture as a new design to trace.
  pub fn open_file(&mut self, p: PathBuf) {
    self.cancel_dialog();
    if trace::is_image(&p) { self.open_path(&p) } else { self.open_in_background(p) }
  }

  /// What the app is still getting ready, while it is: "Opening part.fox…".
  pub fn loading(&self) -> Option<String> {
    let (p, _) = self.opening.as_ref()?;
    Some(format!("Opening {}…", p.file_name().unwrap_or(p.as_os_str()).to_string_lossy()))
  }

  /// Takes the design opened on another thread once it is read. Returns whether it still runs.
  fn poll_opening(&mut self) -> bool {
    if self.opening.as_ref().is_some_and(|(_, job)| job.is_finished())
      && let Some((p, job)) = self.opening.take()
    {
      let opened = job.join().unwrap_or_else(|_| Err(fox_engine::CmdError("the file couldn't be read".into())));
      self.opened(&p, opened);
    }
    self.opening.is_some()
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
      recent: Vec::new(),
      toolbox: None,
      marking: None,
      ctx_menu: marking::MenuState::default(),
      canvas: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 700.0)),
      dim_edit: None,
      rename: None,
      drag: None,
      tstate: viewport::ToolState::default(),
      fitted_once: false,
      last_sketch: None,
      cam_move: None,
      pre_sketch_cam: None,
      cube: viewcube::CubeState::default(),
      picked_plane: None,
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
      opening: None,
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
    self.opened(p, Session::open(p));
  }

  fn opened(&mut self, p: &std::path::Path, opened: Result<Session, fox_engine::CmdError>) {
    match opened {
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
    self.fit_view_now();
    self.data.invalidate();
  }

  /// Opens a design (latest or a given version) from the data panel.
  pub fn open_hub_item(&mut self, item: &str, version: Option<u32>) {
    self.cancel_dialog();
    if let Some(r) = self.run("hub.open", json!({"item": item, "version": version})) {
      self.dialog = None;
      self.tool = Tool::Select;
      self.fit_view_now();
      self.data.invalidate();
      if r.get("warning").is_none() {
        let v = r.get("version").and_then(Value::as_u64).unwrap_or(0);
        self.status = Some((format!("Opened {} v{v}", self.s.design.name), false));
      }
    }
  }

  /// Fit: the camera moves smoothly to frame the whole model.
  pub fn fit_view(&mut self) {
    let mut to = self.view_now();
    self.fit_cam(&mut to);
    self.fly_to(to);
  }

  /// Frames the whole model at once (a design just opened).
  pub fn fit_view_now(&mut self) {
    self.cam_move = None;
    let mut cam = self.cam;
    self.fit_cam(&mut cam);
    self.cam = cam;
  }

  fn fit_cam(&self, cam: &mut Camera) {
    let (lo, hi) = self
      .s
      .model
      .bounds()
      .unwrap_or((fox_engine::kernel::glam::DVec3::splat(-50.0), fox_engine::kernel::glam::DVec3::splat(50.0)));
    let aspect = self.frame.as_ref().map(|f| f.width as f64 / f.height.max(1) as f64).unwrap_or(1.6);
    cam.fit(lo, hi, aspect);
  }

  /// Turns the camera smoothly to look from `dir` with `up` at the top of the screen, keeping
  /// its target and distance (a ViewCube click).
  pub(crate) fn fly_view(&mut self, dir: fox_engine::kernel::glam::DVec3, up: fox_engine::kernel::glam::DVec3) {
    let mut to = self.view_now();
    to.look_from(dir, up);
    self.fly_to(to);
  }

  /// A standard view (Top, Front, ...) in the ViewCube's orientation, reached smoothly.
  pub fn fly_standard_view(&mut self, v: fox_engine::render::StandardView) {
    if v == fox_engine::render::StandardView::Home {
      self.go_home();
    } else {
      let (dir, up) = self.cube.standard_view(v);
      self.fly_view(dir, up);
    }
  }

  /// Go Home: the Home view set from the ViewCube (fitted or at its own distance), else the
  /// front-right-top corner fitted to the model.
  pub fn go_home(&mut self) {
    let mut to = self.view_now();
    match self.cube.home {
      Some((home, false)) => to = Camera { ortho: to.ortho, ..home },
      Some((home, true)) => {
        to.look_from(home.eye_dir(), home.basis().1);
        self.fit_cam(&mut to);
      }
      None => {
        let (dir, up) = self.cube.standard_view(fox_engine::render::StandardView::Home);
        to.look_from(dir, up);
        self.fit_cam(&mut to);
      }
    }
    self.fly_to(to);
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
      self.fit_view_now();
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
      // Esc in a command dialog's field cancels the command and drops its selection, as in Fusion
      if self.dialog.is_some()
        && self.rename.is_none()
        && self.dim_edit.is_none()
        && ctx.input(|i| i.key_pressed(egui::Key::Escape))
      {
        self.cancel_dialog();
        self.s.selection.clear();
      }
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
        self.s.selection.clear();
      } else {
        self.s.selection.clear();
      }
      self.dim_edit = None;
    }
    // the S toolbox, at the pointer over the canvas (in a sketch and on a drawing sheet too)
    if let Some(p) = ctx.pointer_hover_pos().filter(|p| self.canvas.contains(*p))
      && pressed(Key::S, Modifiers::NONE)
    {
      // the S itself mustn't land in the search field
      ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(t) if t.eq_ignore_ascii_case("s"))));
      self.marking = None;
      self.toolbox = Some(toolbox::Toolbox::at(p));
      return;
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
      self.go_home();
    }
    if pressed(Key::V, Modifiers::NONE) {
      self.toggle_visibility();
    }
    use panels::Cmd;
    let sketching = self.s.edit_sketch.is_some();
    // modelling commands; in a sketch they finish it first (E extrudes its profiles), except A,
    // which is the sketch's Arc
    for (k, d) in [
      (Key::E, dialogs::Kind::Extrude),
      (Key::F, dialogs::Kind::Fillet),
      (Key::M, dialogs::Kind::Move),
      (Key::I, dialogs::Kind::Inspect),
      (Key::H, dialogs::Kind::Hole),
      (Key::Q, dialogs::Kind::PressPull),
      (Key::J, dialogs::Kind::Joint),
    ] {
      if pressed(k, Modifiers::NONE) {
        self.launch(Cmd::Dialog(d));
      }
    }
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
          self.launch(Cmd::SketchTool(t));
        }
      }
      if pressed(Key::X, Modifiers::NONE) {
        self.toggle_construction();
      }
      if pressed(Key::O, Modifiers::NONE) {
        self.launch(Cmd::Dialog(dialogs::Kind::SkOffset));
      }
    } else {
      if pressed(Key::A, Modifiers::NONE) {
        self.launch(Cmd::Dialog(dialogs::Kind::Form(&forms::APPEARANCE)));
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

  /// Remembers a command the user started, for the marking menu's Repeat and the S toolbox's
  /// recent row. Every way of starting one (shortcut, toolbox, toolbar, menus) comes through
  /// here, via [`panels::Cmd::run`] or [`App::start`].
  pub(crate) fn note_cmd(&mut self, c: panels::Cmd) {
    if !c.repeatable() {
      return;
    }
    self.last_cmd = Some(c);
    self.recent.retain(|r| *r != c);
    self.recent.insert(0, c);
    self.recent.truncate(toolbox::RECENT);
  }

  /// Runs a command from a shortcut, the S toolbox or Repeat. A modelling command the sketch
  /// toolbar doesn't offer finishes the open sketch first, as in Fusion (E in a sketch extrudes
  /// its profile).
  pub(crate) fn launch(&mut self, c: panels::Cmd) {
    if self.s.edit_sketch.is_some() && self.drawing.is_none() && !menus::offers(panels::Tab::Sketch, c) {
      self.finish_sketch();
    }
    c.run(self);
  }

  /// Shows or hides the bodies of the selection (bodies, or the bodies of selected faces and
  /// edges), as V does in Fusion.
  pub fn toggle_visibility(&mut self) {
    let mut bodies: Vec<fox_engine::model::BodyId> = Vec::new();
    for s in &self.s.selection {
      if let Sel::Body { body } | Sel::Face { body, .. } | Sel::Edge { body, .. } = s
        && !bodies.contains(body)
      {
        bodies.push(*body);
      }
    }
    if bodies.is_empty() {
      self.status = Some(("Show/Hide: select bodies, faces or edges first".into(), false));
      return;
    }
    for b in bodies {
      let visible = self.s.body_visible(b);
      self.run("body.visible", json!({"body": b.0, "visible": !visible}));
    }
    self.s.selection.clear();
  }

  pub fn start(&mut self, kind: dialogs::Kind) {
    self.note_cmd(panels::Cmd::Dialog(kind));
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

  /// Starts a sketch on the selected plane or planar face, or asks for one: the origin planes
  /// show in the viewport to click, as in Fusion.
  pub fn new_sketch(&mut self) {
    self.note_cmd(panels::Cmd::NewSketch);
    self.cancel_dialog();
    if let Some(plane) = self.preselected_plane() {
      self.create_sketch(plane);
      if self.s.edit_sketch.is_some() {
        return;
      }
    }
    self.s.selection.clear();
    self.picked_plane = None;
    self.dialog = Some(Dialog::open(self, dialogs::Kind::PickPlane));
  }

  /// The plane Create Sketch can start on straight away: an origin or construction plane picked
  /// in the browser, or a selected construction plane or planar face.
  pub(crate) fn preselected_plane(&self) -> Option<Value> {
    if let Some(p) = &self.picked_plane {
      return Some(p.clone());
    }
    self.s.selection.iter().find_map(|s| match *s {
      Sel::Face { body, face } => {
        let b = self.s.model.body(body)?;
        let f = b.faces().iter().find(|f| f.id == face)?;
        (f.kind == fox_engine::kernel::SurfaceKind::Plane).then(|| json!({"body": body.0, "face": face}))
      }
      Sel::Feature { feature } if self.s.model.planes.contains_key(&feature) => Some(json!({"feature": feature.0})),
      _ => None,
    })
  }

  pub fn create_sketch(&mut self, plane: Value) {
    if self.dialog.as_ref().is_some_and(|d| d.kind == dialogs::Kind::PickPlane) {
      self.cancel_dialog();
    }
    let before = self.view_now();
    let was_sketching = self.s.edit_sketch.is_some();
    if self.run("sketch.create", json!({"plane": plane})).is_some() {
      if !was_sketching {
        self.pre_sketch_cam = Some(before);
      }
      self.s.selection.clear();
      self.picked_plane = None;
      self.look_at_sketch();
      self.tab = panels::Tab::Sketch;
      // as in Fusion, no tool is active until one is picked
      self.set_tool(Tool::Select);
      self.status = Some((SKETCHING.into(), false));
    }
  }

  pub fn edit_sketch(&mut self, id: fox_engine::model::FeatureId) {
    self.cancel_dialog();
    let before = self.view_now();
    let was_sketching = self.s.edit_sketch.is_some();
    if self.run("sketch.edit", json!({"sketch": id.0})).is_some() {
      if !was_sketching {
        self.pre_sketch_cam = Some(before);
      }
      self.look_at_sketch();
      self.tab = panels::Tab::Sketch;
      self.tool = Tool::Select;
    }
  }

  pub fn finish_sketch(&mut self) {
    self.last_sketch = self.s.edit_sketch;
    self.left_sketch();
    self.run("sketch.finish", json!({}));
    self.tool = Tool::Select;
    self.s.selection.clear();
  }

  /// When the open sketch is finished: back to the 3D view from before it, without the
  /// "Sketching" hint.
  pub(crate) fn left_sketch(&mut self) {
    if self.status.as_ref().is_some_and(|(t, _)| t == SKETCHING) {
      self.status = None;
    }
    if let Some(c) = self.pre_sketch_cam.take() {
      self.fly_to(c);
    }
  }

  /// Where the camera is headed: the end of the move in progress, else where it is.
  fn view_now(&self) -> Camera {
    self.cam_move.map_or(self.cam, |m| m.1)
  }

  /// Moves the camera smoothly to `to`.
  pub(crate) fn fly_to(&mut self, to: Camera) {
    if to != self.cam {
      self.cam_move = Some((self.cam, to, now()));
    }
  }

  /// Stops a camera move where it is (the user took the camera).
  pub(crate) fn stop_cam_move(&mut self) {
    self.cam_move = None;
  }

  /// Advances the camera move to time `t`. Returns whether it still runs.
  fn step_cam_move(&mut self, t: f64) -> bool {
    let Some((from, to, start)) = self.cam_move else { return false };
    let k = ((t - start) / CAM_MOVE_SECS).clamp(0.0, 1.0);
    self.cam = lerp_camera(&from, &to, k * k * (3.0 - 2.0 * k));
    if k >= 1.0 {
      self.cam_move = None;
    }
    self.cam_move.is_some()
  }

  fn look_at_sketch(&mut self) {
    let Some(r) = self.s.edit_sketch.and_then(|e| self.s.model.sketches.get(&e)) else { return };
    let n = r.plane.normal();
    let o = r.plane.origin;
    let mut to = self.looking_along(n);
    if self.s.model.bodies.is_empty() {
      to.target = o.to_array();
    }
    self.fly_to(to);
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
      Some(n) => {
        let to = self.looking_along(n);
        self.fly_to(to);
      }
      None => self.status = Some(("Look At: select a planar face".into(), false)),
    }
  }

  /// The camera turned to look straight at a plane with normal `n`.
  fn looking_along(&self, n: fox_engine::kernel::glam::DVec3) -> Camera {
    let mut cam = self.view_now();
    cam.pitch =
      n.z.clamp(-1.0, 1.0).asin().clamp(-std::f64::consts::FRAC_PI_2 + 1e-6, std::f64::consts::FRAC_PI_2 - 1e-6);
    if n.z.abs() < 0.999 {
      cam.yaw = n.x.atan2(-n.y);
    } else {
      cam.yaw = 0.0;
    }
    cam.roll = 0.0;
    cam
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
    if self.poll_opening() {
      self.draw_busy(ui, &ctx);
      return;
    }
    if self.poll_rebuild() {
      self.draw_busy(ui, &ctx);
      return;
    }
    if self.data.save.is_none() {
      // typed values go to the heads-up box or dialog before shortcuts see the keys
      viewport::route_typing(self, &ctx);
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
    if self.step_cam_move(now()) {
      ctx.request_repaint();
    }
    viewcube::keep_projection(self);
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
    toolbox::show(self, &ctx);
    panels::canvas_status(self, &ctx);
    panels::unsaved_banner(self, &ctx);
    panels::selection_info(self, &ctx);
    marking::end_frame(self, &ctx);
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
        self.fit_view_now();
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
    let text = match self.loading() {
      Some(l) => l,
      None => format!("Computing…  {elapsed:.1} s"),
    };
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("busy-cover")).fixed_pos(screen.min).order(egui::Order::Foreground).show(ctx, |ui| {
      let (r, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
      ui.painter().rect_filled(r, 0.0, egui::Color32::from_black_alpha(16));
      if elapsed > 0.15 || self.opening.is_some() {
        let c = self.canvas.center();
        let g = ui.painter().layout_no_wrap(text, egui::FontId::proportional(15.0), theme::TEXT);
        let fox = egui::vec2(22.0, 26.0);
        let b = egui::Rect::from_center_size(c, egui::vec2(g.size().x + fox.x + 42.0, fox.y + 18.0));
        ui.painter().rect_filled(b, 6.0, egui::Color32::WHITE);
        ui.painter().rect_stroke(b, 6.0, egui::Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
        let spot = egui::Rect::from_min_size(b.left_center() + egui::vec2(14.0, -fox.y / 2.0), fox);
        logo::paint_ripple(ui.painter(), spot, ui.input(|i| i.time) as f32);
        ui.painter().galley(egui::pos2(spot.right() + 12.0, c.y - g.size().y / 2.0), g, theme::TEXT);
      }
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(16));
  }
}

/// The status hint while a new sketch is open.
const SKETCHING: &str = "Sketching — draw with the Create tools; Finish Sketch when done";

/// How long a camera move takes (ViewCube, Home, Fit, Look At, a sketch opening and closing).
const CAM_MOVE_SECS: f64 = 0.3;

/// The camera a fraction `k` of the way from `a` to `b`: the shorter way round in yaw and roll,
/// and the distance changing by a steady factor. The projection switches on arrival.
fn lerp_camera(a: &Camera, b: &Camera, k: f64) -> Camera {
  if k >= 1.0 {
    return *b;
  }
  let lerp = |x: f64, y: f64| x + (y - x) * k;
  let pi = std::f64::consts::PI;
  let turn = |x: f64, y: f64| (y - x + pi).rem_euclid(std::f64::consts::TAU) - pi;
  let dyaw = turn(a.yaw, b.yaw);
  let distance = if a.distance > 0.0 && b.distance > 0.0 {
    (a.distance.ln() + (b.distance.ln() - a.distance.ln()) * k).exp()
  } else {
    lerp(a.distance, b.distance)
  };
  Camera {
    target: [lerp(a.target[0], b.target[0]), lerp(a.target[1], b.target[1]), lerp(a.target[2], b.target[2])],
    yaw: a.yaw + dyaw * k,
    pitch: lerp(a.pitch, b.pitch),
    distance,
    fov: lerp(a.fov, b.fov),
    ortho: a.ortho,
    roll: a.roll + turn(a.roll, b.roll) * k,
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
  fn a_design_opens_in_the_background() {
    let mut app = App::with_session(Session::default());
    app.open_in_background(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/bracket.fox")));
    assert_eq!(app.loading().as_deref(), Some("Opening bracket.fox…"));
    while app.poll_opening() {
      std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(app.loading().is_none());
    assert!(!app.s.model.bodies.is_empty() && app.status.as_ref().is_some_and(|s| !s.1));
  }

  fn settle_camera(app: &mut App) {
    while app.step_cam_move(f64::INFINITY) {}
  }

  #[test]
  fn create_sketch_starts_on_a_preselected_plane() {
    // an origin plane picked in the browser
    let mut app = App::with_session(Session::default());
    app.picked_plane = Some(json!("xz"));
    app.new_sketch();
    assert!(app.s.edit_sketch.is_some() && app.dialog.is_none());
    assert_eq!(app.tool, Tool::Select, "no sketch tool is active at first, as in Fusion");
    assert!(app.picked_plane.is_none());
    // a construction plane selected
    let mut app = App::with_session(Session::default());
    let id = app.run("plane.offset", json!({"plane": "xy", "offset": 10})).and_then(|v| v["feature"].as_u64()).unwrap();
    app.s.selection = vec![Sel::Feature { feature: fox_engine::model::FeatureId(id as u32) }];
    app.new_sketch();
    let sk = app.s.edit_sketch.and_then(|e| app.s.model.sketches.get(&e)).unwrap();
    assert!((sk.plane.origin.z - 10.0).abs() < 1e-9);
    // nothing selected: the planes are offered in the viewport
    let mut app = App::with_session(Session::default());
    app.new_sketch();
    assert!(app.s.edit_sketch.is_none());
    assert!(app.dialog.as_ref().is_some_and(|d| d.kind == dialogs::Kind::PickPlane));
  }

  #[test]
  fn finishing_a_sketch_goes_back_to_the_view_before_it() {
    let mut app = App::with_session(Session::default());
    app.cam.yaw = 0.3;
    app.cam.pitch = 0.4;
    let before = app.cam;
    app.create_sketch(json!("xz"));
    settle_camera(&mut app);
    assert!(app.cam.pitch.abs() < 1e-6, "looks at the XZ plane: {:?}", app.cam);
    assert!(app.status.is_some());
    app.finish_sketch();
    settle_camera(&mut app);
    assert_eq!(app.cam, before);
    assert!(app.status.is_none(), "no stale Sketching hint: {:?}", app.status);
  }

  #[test]
  fn view_changes_glide_and_end_on_the_view() {
    use fox_engine::kernel::glam::DVec3;
    let mut app = App::with_session(Session::default());
    app.cam_move = None;
    let start = app.cam;
    let corner = DVec3::new(-1.0, -1.0, 1.0).normalize();
    app.fly_view(corner, DVec3::Z);
    assert_eq!(app.cam, start, "nothing jumps");
    let (_, _, t0) = app.cam_move.expect("a move runs");
    assert!(app.step_cam_move(t0 + CAM_MOVE_SECS / 2.0), "still running half way");
    let mid = app.cam.eye_dir();
    assert!((mid - corner).length() > 0.1 && (mid - start.eye_dir()).length() > 0.1, "{mid}");
    assert!(!app.step_cam_move(t0 + CAM_MOVE_SECS), "done after {CAM_MOVE_SECS} s");
    assert!((app.cam.eye_dir() - corner).length() < 1e-9);
    assert_eq!(app.cam.distance, start.distance);
    // Home and Fit glide too, and land where the instant versions do
    app.go_home();
    settle_camera(&mut app);
    assert!((app.cam.eye_dir() - Camera::default().eye_dir()).length() < 1e-3);
    let mut snapped = app.cam;
    snapped.distance *= 3.0;
    app.cam = snapped;
    app.fit_view();
    assert_eq!(app.cam, snapped);
    settle_camera(&mut app);
    let fitted = app.cam;
    app.cam = snapped;
    app.fit_view_now();
    assert_eq!(app.cam, fitted);
  }

  #[test]
  fn camera_moves_turn_the_short_way_round() {
    let a = Camera { yaw: 3.0, ..Camera::default() };
    let b = Camera { yaw: -3.0, distance: 800.0, ..Camera::default() };
    let mid = lerp_camera(&a, &b, 0.5);
    assert!((mid.yaw - std::f64::consts::PI).abs() < 1e-9, "{}", mid.yaw);
    assert!((mid.distance - 400.0).abs() < 1e-9);
    assert_eq!(lerp_camera(&a, &b, 1.0), b);
  }

  /// One frame of keyboard shortcuts with `key` pressed (and typed) and the pointer at `at`.
  fn press(app: &mut App, ctx: &egui::Context, key: egui::Key, at: egui::Pos2) {
    let input = egui::RawInput {
      events: vec![
        egui::Event::PointerMoved(at),
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE },
        egui::Event::Text(key.name().to_lowercase()),
      ],
      ..Default::default()
    };
    ctx.run_ui(input, |ui| app.shortcuts(ui.ctx())).drop_without_applying_deltas();
  }

  fn with_box() -> App {
    let mut app = App::with_session(Session::default());
    for (c, p) in [
      ("sketch.create", json!({"plane": "xy"})),
      ("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]})),
      ("extrude", json!({"distance": 5})),
    ] {
      app.run(c, p).unwrap();
    }
    app
  }

  #[test]
  fn shortcuts_set_what_repeat_offers() {
    let ctx = egui::Context::default();
    let mut app = with_box();
    let mid = app.canvas.center();
    app.new_sketch();
    app.cancel_dialog();
    assert_eq!(app.last_cmd, Some(panels::Cmd::NewSketch));
    press(&mut app, &ctx, egui::Key::F, mid);
    assert_eq!(app.dialog.as_ref().map(|d| d.kind), Some(dialogs::Kind::Fillet));
    assert_eq!(app.last_cmd, Some(panels::Cmd::Dialog(dialogs::Kind::Fillet)));
    assert_eq!(app.recent[..2], [panels::Cmd::Dialog(dialogs::Kind::Fillet), panels::Cmd::NewSketch]);
  }

  #[test]
  fn e_in_a_sketch_finishes_it_and_extrudes_its_profile() {
    let ctx = egui::Context::default();
    let mut app = App::with_session(Session::default());
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    app.run("sketch.rect", json!({"corner1": [0, 0], "corner2": [10, 10]})).unwrap();
    let sketch = app.s.edit_sketch;
    let mid = app.canvas.center();
    // the sketch letters still pick sketch tools
    press(&mut app, &ctx, egui::Key::R, mid);
    assert_eq!((app.tool, app.s.edit_sketch), (Tool::Rect, sketch));
    // Measure works inside the sketch
    press(&mut app, &ctx, egui::Key::I, mid);
    assert!(app.s.edit_sketch.is_some());
    press(&mut app, &ctx, egui::Key::E, mid);
    assert!(app.s.edit_sketch.is_none());
    let d = app.dialog.as_ref().unwrap();
    assert_eq!((d.kind, d.sketch), (dialogs::Kind::Extrude, sketch));
  }

  #[test]
  fn v_hides_and_shows_the_selected_face_body() {
    let ctx = egui::Context::default();
    let mut app = with_box();
    let body = app.s.model.bodies[0].id;
    let mid = app.canvas.center();
    app.s.selection = vec![Sel::Face { body, face: app.s.model.bodies[0].faces()[0].id }];
    press(&mut app, &ctx, egui::Key::V, mid);
    assert!(!app.s.body_visible(body));
    app.s.selection = vec![Sel::Body { body }];
    press(&mut app, &ctx, egui::Key::V, mid);
    assert!(app.s.body_visible(body));
  }

  #[test]
  fn s_opens_the_toolbox_over_the_canvas_only() {
    let ctx = egui::Context::default();
    let mut app = App::with_session(Session::default());
    app.canvas = egui::Rect::from_min_size(egui::pos2(0.0, 100.0), egui::vec2(800.0, 500.0));
    press(&mut app, &ctx, egui::Key::S, egui::pos2(400.0, 50.0));
    assert!(app.toolbox.is_none());
    press(&mut app, &ctx, egui::Key::S, egui::pos2(400.0, 300.0));
    assert!(app.toolbox.is_some());
  }

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
