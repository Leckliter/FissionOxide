//! The FissionOxide desktop app: a Fusion 360-style modeling workspace on egui.
//!
//! Layout, as in Fusion: the application bar (document tabs) and toolbar (workspace switcher,
//! tabs, tool groups) at the top; the canvas with the browser, comments, navigation bar,
//! ViewCube, sketch palette and command dialogs floating over it; the timeline at the bottom. The UI is thin: every action goes through [`Session::execute`] commands.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

mod assembly_forms;
mod cam_ui;
mod canvas_ui;
mod capture;
mod clipboard;
mod comments;
mod comp_drag;
mod config_ui;
mod customize;
mod cut_ui;
mod data_panel;
mod dialogs;
mod ecad_ui;
mod explode_ui;
mod form_ui;
mod forms;
mod gen_ui;
mod glyphs;
mod gpu;
mod handles;
mod help;
mod icons;
#[cfg(test)]
mod input_tests;
mod library_ui;
mod logo;
mod manipulators;
mod marking;
mod menus;
mod navbar;
mod notifications;
mod panels;
mod params_ui;
mod plugins_ui;
mod preferences;
mod props_ui;
mod render_ui;
mod scale_to_ui;
mod select;
mod service_ui;
mod sheet;
mod sim_ui;
mod theme;
mod tips;
mod toolbox;
mod trace;
mod units;
mod updates;
mod value_box;
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

/// Whether a hub root is a hub server's URL rather than a directory.
pub fn is_remote_hub(root: &std::path::Path) -> bool {
  fox_engine::store::remote::is_remote(root)
}

pub struct App {
  pub s: Session,
  pub cam: Camera,
  scene: SceneBuilder,
  picks: PickTable,
  frame: Option<fox_engine::render::Frame>,
  tex: Option<TextureHandle>,
  render_key: Option<RenderKey>,
  pub hover: Option<Pick>,
  /// What the hovered constraint glyph constrains, drawn in the hover colour.
  pub glyph_hover: Vec<Pick>,
  pub tool: Tool,
  pub dialog: Option<Dialog>,
  /// Status-bar message: (text, is_error).
  pub status: Option<(String, bool)>,
  /// The status text when it is a tool's or command's hint ("select lines or two points"): it
  /// clears when that tool or command ends, as Fusion's canvas hint does.
  hint: Option<String>,
  pub show_params: bool,
  /// Display Settings ▸ Visual Style.
  pub visual_style: fox_engine::render::VisualStyle,
  /// Display Settings ▸ Object Visibility and Effects.
  pub objects: navbar::ObjectVisibility,
  pub effects: fox_engine::render::Effects,
  pub show_grid: bool,
  /// Grid and Snaps: the grid's spacing, and manipulator increments.
  pub grid: navbar::GridSettings,
  pub increments: navbar::Increments,
  /// Orbiting lets the view roll (Free Orbit) instead of keeping +Z up.
  pub free_orbit: bool,
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
  /// The COMMENTS panel: open or collapsed to its header, and its comments.
  pub comments: comments::Comments,
  /// The bell's Notifications panel.
  pub notifications: notifications::Notifications,
  /// File ▸ Capture Image... and 3D Print dialogs.
  pub(crate) capture: capture::Capture,
  /// Display Settings: environment, graphics preset, ground plane offset.
  pub display: navbar::DisplaySettings,
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
  /// Pinned toolbar icons, the S toolbox's shortcuts and the user's keyboard shortcuts.
  pub(crate) prefs: customize::Prefs,
  /// The open Change Keyboard Shortcut window.
  pub(crate) key_capture: Option<customize::KeyCapture>,
  /// The open right-click marking menu.
  marking: Option<marking::MarkingMenu>,
  /// Context-menu state: browser/timeline hover highlight, Find in Browser.
  pub(crate) ctx_menu: marking::MenuState,
  /// The canvas (viewport) rectangle, for panels floating over it.
  pub(crate) canvas: egui::Rect,
  /// The canvas's size in pixels, as last drawn (what Capture Image's window size is).
  pub(crate) canvas_px: [u32; 2],
  /// Inline dimension value editor: (constraint id, text, screen position).
  dim_edit: Option<(u32, String, egui::Pos2)>,
  /// Feature id being renamed in the timeline/browser.
  rename: Option<(RenameTarget, String)>,
  drag: Option<viewport::Drag>,
  /// The component being dragged in the viewport.
  comp_drag: Option<comp_drag::CompDrag>,
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
  /// Origin planes shown in the viewport (the browser's Show/Hide), as `sketch.create` planes.
  pub(crate) shown_planes: Vec<Value>,
  /// The browser's Origin folder is shown (its light bulb): the origin planes and short axes
  /// draw at the origin. Off by default, as in Fusion.
  pub(crate) origin_shown: bool,
  /// A pending open/closed state for the browser's Sketches folder: Edit Sketch opens it,
  /// Finish Sketch collapses it (the browser applies and clears it).
  pub(crate) browser_sketches: Option<bool>,
  /// The format picked in the root row's Export... sub-menu, run once the menu closes.
  pub(crate) pending_export: Option<&'static str>,
  /// Sketches whose profiles the browser's Hide Profile turned off.
  pub(crate) hidden_profiles: Vec<fox_engine::model::FeatureId>,
  /// Sketches whose dimensions show outside Edit Sketch (the browser's Show Dimensions).
  pub(crate) shown_dims: Vec<fox_engine::model::FeatureId>,
  /// The data panel (projects, designs, versions).
  pub data: data_panel::DataPanel,
  /// The drawing open in the Drawing workspace; None = the Design workspace.
  pub drawing: Option<u32>,
  /// The open drawing's rendered sheet, keyed by (design revision, drawing, sheet).
  sheet_cache: Option<((u64, u32, u32), Result<crate::sheet::Rendered, String>)>,
  /// Sheet zoom and pan in the Drawing workspace.
  sheet_view: (f32, egui::Vec2),
  /// What clicks and window selection pick: faces (default), edges, bodies or components.
  pub sel_priority: select::Priority,
  /// Which kinds of things can be picked.
  pub sel_filter: select::Filters,
  /// The open Select Other list.
  pub(crate) other: Option<select::OtherMenu>,
  /// The open Selection Tools dialog (Select By Name, By Boundary, By Size, Seed And Boundary).
  pub(crate) sel_tool: Option<select::ToolDialog>,
  /// Where the last viewport click was (Select Other from the menu opens there).
  pub(crate) last_click: Option<egui::Pos2>,
  /// The Measure dialog's result line: closest points and the label.
  pub(crate) measure: Option<(fox_engine::kernel::glam::DVec3, fox_engine::kernel::glam::DVec3, String)>,
  /// Measure's precision and units.
  pub(crate) measure_fmt: dialogs::MeasureFmt,
  /// Section analysis: `{plane, offset, flip}` (see [`fox_engine::scene::view_options`]).
  pub section: Option<Value>,
  /// Surface analysis: `{analysis, pull?, angle?}`.
  pub analysis: Option<Value>,
  /// A regeneration running in the background, and when it started. While it runs the UI shows
  /// the last image and takes no input (nothing may call the kernel; see `Session::begin`).
  pub(crate) rebuild: Option<(fox_engine::Rebuild, f64)>,
  /// In the Manufacture workspace: the MILLING tab, toolpaths and stock drawn.
  pub manufacture: bool,
  /// The Form workspace's FORM tab, while open.
  pub form: form_ui::FormEdit,
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
  /// The 2D cut window (laser, plasma, waterjet), while open.
  pub cut_window: Option<cut_ui::CutWindow>,
  /// The Electronics workspace, while it is open (in place of the Design workspace).
  pub electronics: Option<ecad_ui::EcadView>,
  /// A cut job whose preview failed at this design revision (not retried every frame).
  pub cut_preview_failed: Option<(u32, u64)>,
  /// In the Generative Design workspace.
  pub generative: bool,
  /// The generative study window and its background solves.
  pub(crate) gd: gen_ui::GenState,
  /// The Simulation workspace, while it is open: the study shown, result display, probes.
  pub simulation: Option<sim_ui::SimUi>,
  /// Software updates (Help → Check for Updates).
  pub updates: updates::Updates,
  /// The Preferences dialog and the saved preferences.
  pub(crate) preferences: preferences::Preferences,
  /// A design being opened on another thread (the one the app was started with).
  opening: Option<(PathBuf, std::thread::JoinHandle<Result<Session, fox_engine::CmdError>>)>,
  /// Whether the window had the keyboard focus last frame, and when it last gained it (egui time).
  window_focused: bool,
  activated_at: f64,
  /// Insert > Canvas, Edit Canvas and Calibrate.
  pub(crate) canvas_ui: canvas_ui::CanvasUi,
  /// The Help menu's windows.
  pub(crate) help: help::Help,
  /// A command started: the app bar's menus close at the end of the frame, as in Fusion.
  close_menus: bool,
  /// What Ctrl+C copied, for Ctrl+V.
  pub(crate) clipboard: Option<clipboard::Clip>,
  /// Installed plugins: the Plugins window, their toolbar tabs and command dialogs.
  pub(crate) plugins: plugins_ui::PluginsUi,
  /// Insert > Sketch Library: the part picker and placing parts with clicks.
  pub(crate) library_ui: library_ui::LibraryUi,
  /// View ▸ Render Components….
  pub(crate) render_components: explode_ui::RenderComponents,
  /// The Render workspace.
  pub render: render_ui::RenderUi,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RenameTarget {
  Feature(u32),
  Body(u32),
  Component(u32),
  /// A timeline group.
  Group(u32),
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
  style: fox_engine::render::VisualStyle,
  objects: navbar::ObjectVisibility,
  effects: fox_engine::render::Effects,
  show_grid: bool,
  grid_step: Option<f64>,
  visible: Vec<(u32, bool)>,
  profiles_of: Vec<fox_engine::model::FeatureId>,
  hidden_profiles: Vec<fox_engine::model::FeatureId>,
  /// Section and analysis settings.
  view: Value,
  /// Body vertices drawn (the Vertices selection filter).
  vertices: bool,
  /// Bodies highlighted from the browser or timeline, and the isolated bodies.
  peek: Vec<u32>,
  isolate: Option<Vec<u32>>,
  opacity: Vec<(u32, u8)>,
  /// Faces and sketches highlighted from the timeline.
  peek_faces: Vec<(u32, u32)>,
  peek_sketches: Vec<u32>,
  /// Sketch items a hovered constraint glyph highlights.
  glyph_hover: Vec<Pick>,
}

impl App {
  pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> App {
    theme::apply(&cc.egui_ctx);
    let mut app = App::with_session(Session::default());
    app.gpu = gpu::Gpu::new(cc.gl.clone());
    app.updates = updates::Updates::start(&cc.egui_ctx);
    notifications::post(&mut app, notifications::whats_new_notice());
    app.prefs = customize::Prefs::load();
    app.preferences = preferences::Preferences::load();
    preferences::apply(&mut app, Some(&cc.egui_ctx));
    // back on the hub server it was connected to last time
    if app.s.store_root.is_none()
      && std::env::var_os("FOX_DATA_DIR").is_none()
      && let Some(url) = fox_engine::store::remote::last_server()
    {
      log::info!("using the hub server {url}");
      app.s.store_root = Some(url.into());
    }
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
    // each test's app gets its own scratch hub, never the user's
    #[cfg(test)]
    let s = {
      let mut s = s;
      s.store_root.get_or_insert_with(fox_engine::store::Store::scratch_root);
      s
    };
    App {
      s,
      cam: Camera::default(),
      scene: SceneBuilder::default(),
      picks: PickTable::default(),
      frame: None,
      tex: None,
      render_key: None,
      hover: None,
      glyph_hover: Vec::new(),
      tool: Tool::Select,
      dialog: None,
      status: None,
      hint: None,
      show_params: false,
      drawing: None,
      sheet_cache: None,
      sheet_view: (1.0, egui::Vec2::ZERO),
      visual_style: Default::default(),
      objects: Default::default(),
      effects: Default::default(),
      show_grid: true,
      grid: Default::default(),
      increments: Default::default(),
      free_orbit: false,
      show_dims: true,
      snap: false,
      show_constraints: true,
      browser_open: true,
      show_comments: true,
      canvas_px: [0, 0],
      comments: comments::Comments::default(),
      notifications: notifications::Notifications::default(),
      capture: capture::Capture::default(),
      display: navbar::DisplaySettings::default(),
      show_navbar: true,
      show_viewcube: true,
      nav_mode: None,
      tab: panels::Tab::Solid,
      playing: None,
      last_cmd: None,
      recent: Vec::new(),
      toolbox: None,
      prefs: customize::Prefs::default(),
      key_capture: None,
      marking: None,
      ctx_menu: marking::MenuState::default(),
      canvas: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 700.0)),
      dim_edit: None,
      rename: None,
      drag: None,
      comp_drag: None,
      tstate: viewport::ToolState::default(),
      fitted_once: false,
      last_sketch: None,
      cam_move: None,
      pre_sketch_cam: None,
      cube: viewcube::CubeState::default(),
      picked_plane: None,
      shown_planes: Vec::new(),
      origin_shown: false,
      browser_sketches: None,
      pending_export: None,
      hidden_profiles: Vec::new(),
      shown_dims: Vec::new(),
      data: data_panel::DataPanel::default(),
      sel_priority: select::Priority::Face,
      sel_filter: select::Filters::default(),
      other: None,
      sel_tool: None,
      last_click: None,
      measure: None,
      measure_fmt: dialogs::MeasureFmt::default(),
      section: None,
      analysis: None,
      rebuild: None,
      manufacture: false,
      form: form_ui::FormEdit::default(),
      tool_library: None,
      gpu: gpu::Gpu::new(None),
      sel_mode: select::Mode::default(),
      region: select::Region::default(),
      manip: manipulators::State::default(),
      cut_window: None,
      electronics: None,
      cut_preview_failed: None,
      generative: false,
      gd: gen_ui::GenState::default(),
      simulation: None,
      updates: updates::Updates::default(),
      preferences: preferences::Preferences::default(),
      opening: None,
      window_focused: true,
      activated_at: f64::NEG_INFINITY,
      canvas_ui: canvas_ui::CanvasUi::default(),
      help: help::Help::default(),
      close_menus: false,
      clipboard: None,
      plugins: plugins_ui::PluginsUi::default(),
      library_ui: library_ui::LibraryUi::default(),
      render_components: explode_ui::RenderComponents::default(),
      render: render_ui::RenderUi::default(),
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

  /// Shows a tool's or command's hint at the lower left of the canvas until it ends.
  pub(crate) fn hint(&mut self, msg: impl Into<String>) {
    let msg = msg.into();
    self.status = Some((msg.clone(), false));
    self.hint = Some(msg);
  }

  /// Clears the hint of the tool or command that ended (other status messages stay).
  pub(crate) fn end_hint(&mut self) {
    if let Some(h) = self.hint.take()
      && self.status.as_ref().is_some_and(|(t, err)| !err && *t == h)
    {
      self.status = None;
    }
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
    let revision = self.s.revision;
    self.s = s;
    self.swapped_session(revision);
    self.drawing = None;
    self.dialog = None;
    self.tool = Tool::Select;
    self.fit_view_now();
    self.data.invalidate();
  }

  /// After the session was replaced by another design (opened, new, from the hub). Every session
  /// starts counting revisions at the same number, and the viewport only rebuilds its scene when
  /// the revision (or what's shown) changes, so the new one carries on from `old_revision`:
  /// otherwise the previous design's scene could stay up. The Save dialog was for the old design.
  fn swapped_session(&mut self, old_revision: u64) {
    self.s.revision = self.s.revision.max(old_revision + 1);
    self.render_key = None;
    self.hover = None;
    self.ctx_menu.peek.clear();
    self.ctx_menu.peek_faces.clear();
    self.ctx_menu.peek_sketches.clear();
    self.ctx_menu.isolate = None;
    self.data.save = None;
  }

  /// Opens a design (latest or a given version) from the data panel.
  pub fn open_hub_item(&mut self, item: &str, version: Option<u32>) {
    self.cancel_dialog();
    let revision = self.s.revision;
    if let Some(r) = self.run("hub.open", json!({"item": item, "version": version})) {
      self.swapped_session(revision);
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

  /// The window title, as Fusion's: "Untitled* (user) - FissionOxide" until the design is
  /// saved, then its name and version, starred while it has unsaved changes.
  pub fn title(&self) -> String {
    panels::window_title(self)
  }

  // ---- file actions ----

  pub fn new_design(&mut self) {
    let root = self.s.store_root.clone();
    let revision = self.s.revision;
    self.s = Session::default();
    // Preferences: new designs in the default units (General) and without a timeline (Design),
    // in a fresh session so the settings aren't undo steps
    let units = self.preferences.values.units;
    let mut set = false;
    if units != preferences::Units::Mm {
      set |= self.s.execute("design.units", &json!({"unit": units.key()})).is_ok();
    }
    if !self.preferences.values.capture_history {
      set |= self.s.execute("design.history", &json!({"capture": false})).is_ok();
    }
    if set {
      self.s = Session::new(self.s.design.clone());
    }
    self.swapped_session(revision);
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
        let warnings = r["warnings"].as_array().cloned().unwrap_or_default();
        let time =
          format!("{ops} operation(s) · about {:.0} min {:.0} s machining", min.floor(), (min.fract() * 60.0).round());
        // holder and fixture collisions
        self.status = Some(match warnings.first() {
          None => (time, false),
          Some(w) => (
            format!(
              "{time} · {} warning(s): {}{}",
              warnings.len(),
              w["message"].as_str().unwrap_or(""),
              if warnings.len() > 1 { " …" } else { "" }
            ),
            true,
          ),
        });
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

  /// Files dropped on the window: a `.foxplugin` asks to be installed; others are uploaded when
  /// the data panel is open, otherwise opened (into an empty design) or inserted.
  fn dropped_files(&mut self, ctx: &egui::Context) {
    let files: Vec<PathBuf> =
      ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| p.is_absolute()).collect());
    for p in files {
      if p.extension().is_some_and(|e| e.eq_ignore_ascii_case(fox_plugin::package::EXTENSION)) {
        plugins_ui::offer_file(self, &p);
      } else if self.data.open {
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
    if self.key_capture.is_some() || self.preferences.open {
      // Change Keyboard Shortcut and the Preferences modal take the keys
      return;
    }
    // Esc closes an open menu (File, Help, a panel's drop-down, a right-click menu) before it
    // does anything else, as in Fusion
    if egui::Popup::is_any_open(ctx) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
      egui::Popup::close_all(ctx);
      return;
    }
    if ctx.egui_wants_keyboard_input() {
      // Esc in a command dialog's field cancels the command and drops its selection, as in Fusion
      if self.dialog.is_some()
        && self.rename.is_none()
        && self.dim_edit.is_none()
        && ctx.input(|i| i.key_pressed(egui::Key::Escape))
      {
        self.cancel_dialog();
        self.s.selection.clear();
      } else if self.sel_tool.is_some() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        self.sel_tool = None;
        self.end_hint();
      }
      return;
    }
    use egui::{Key, Modifiers};
    // commands with two or more modifiers first: Ctrl+S would take Ctrl+Alt+S too
    if self.command_keys(ctx, true) {
      return;
    }
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
    // Display Settings ▸ Enter/Exit Full Screen
    if pressed(Key::F, Modifiers::COMMAND | Modifiers::SHIFT) {
      navbar::toggle_full_screen(ctx);
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
      if self.other.is_some() {
        // the Select Other list closes, keeping the selection
        self.other = None;
      } else if self.sel_tool.is_some() {
        self.sel_tool = None;
        self.end_hint();
      } else if self.canvas_ui.cancel() || self.library_ui.cancel() {
      } else if self.marking.is_some() {
        self.marking = None;
      } else if self.dim_edit.is_some() {
        // a new dimension stays at its current value, as in Fusion
        viewport::settle_dim_edit(self);
      } else if self.nav_mode.is_some() {
        self.nav_mode = None;
      } else if !self.tstate.is_empty() {
        // an unfinished spline is discarded (Enter or its check mark commits it); with the
        // Select tool a stray pick state never keeps the selection
        self.end_hint();
        viewport::reset_tool_state(self);
        if self.tool == Tool::Select {
          self.s.selection.clear();
        }
      } else if self.tool != Tool::Select {
        self.end_hint();
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
      // the model's shortcuts don't apply on a drawing sheet; its own commands' do
      self.command_keys(ctx, false);
      return;
    }
    // Enter commits the spline being drawn or the conic being shaped
    if matches!(self.tool, Tool::Spline | Tool::ControlSpline | Tool::Conic) && pressed(Key::Enter, Modifiers::NONE) {
      viewport::accept_tool(self);
    }
    // Ctrl+C, Ctrl+V and Ctrl+Shift+V (Paste New); the window delivers them as Copy and Paste
    // events rather than keys
    let (copy, paste) = ctx.input_mut(|i| {
      let (mut copy, mut paste) = (false, false);
      i.events.retain(|e| match e {
        egui::Event::Copy => {
          copy = true;
          false
        }
        egui::Event::Paste(_) => {
          paste = true;
          false
        }
        _ => true,
      });
      (copy, paste.then_some(i.modifiers.shift))
    });
    let paste = paste
      .or_else(|| pressed(Key::V, Modifiers::COMMAND | Modifiers::SHIFT).then_some(true))
      .or_else(|| pressed(Key::V, Modifiers::COMMAND).then_some(false));
    if copy || pressed(Key::C, Modifiers::COMMAND) {
      clipboard::copy(self, ctx);
    }
    if let Some(new) = paste {
      clipboard::paste(self, new);
      return;
    }
    // Space (with Preferences → General → Spacebar repeats last command, on by default) and
    // Enter repeat the last command when none is running, as the marking menu's Repeat does
    if self.idle()
      && ((self.preferences.values.space_repeats && pressed(Key::Space, Modifiers::NONE))
        || pressed(Key::Enter, Modifiers::NONE))
    {
      self.repeat_last();
      return;
    }
    // Del is Delete's key in the map; Backspace always deletes too
    if pressed(Key::Backspace, Modifiers::NONE) {
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
    // the toolbar commands' keys: E extrude, L line, … (in a sketch, E finishes it first)
    self.command_keys(ctx, false);
    if self.s.edit_sketch.is_some() && pressed(Key::X, Modifiers::NONE) {
      self.toggle_construction();
    }
  }

  /// The toolbar commands' keyboard shortcuts where the app is now: the user's keys over the
  /// menus' defaults, the open sketch's toolbar first, then the tab's, then the rest of the
  /// workspace's; a key goes to the first command that has it.
  fn command_key_table(&self) -> Vec<(egui::Modifiers, egui::Key, panels::Cmd)> {
    let tabs = if self.generative && self.drawing.is_none() {
      vec![panels::Tab::Generative]
    } else if self.manufacture && self.drawing.is_none() {
      vec![panels::Tab::Milling, panels::Tab::Turning, panels::Tab::Utilities]
    } else {
      toolbox::workspace_tabs(self)
    };
    let mut keys: Vec<(egui::Modifiers, egui::Key, panels::Cmd)> = Vec::new();
    for e in menus::commands(&tabs) {
      let key = menus::key_of(&self.prefs, tips::Scope::of(e.tab), e.label);
      if let Some((m, k)) = customize::parse_combo(key)
        && !keys.iter().any(|(m2, k2, _)| (*m2, *k2) == (m, k))
      {
        keys.push((m, k, e.cmd));
      }
    }
    keys
  }

  /// The key that runs `c` where the app is now, as the marking menu shows it: its toolbar
  /// command's key (the user's or the menus' default), else the app's own (Ctrl+Z, F6, …).
  pub(crate) fn key_for(&self, c: panels::Cmd) -> Option<String> {
    use panels::Cmd;
    if let Some((m, k, _)) = self.command_key_table().into_iter().find(|(.., c2)| *c2 == c) {
      return Some(customize::format_combo(m, k));
    }
    let what = match c {
      Cmd::Undo => "Undo",
      Cmd::Redo => "Redo",
      Cmd::Fit => "Fit",
      Cmd::ToggleVisibility => "Show/Hide",
      Cmd::Construction if self.s.edit_sketch.is_some() => "Construction",
      _ => return None,
    };
    customize::reserved_key(what).map(str::to_string)
  }

  /// Runs the toolbar command whose keyboard shortcut was pressed: the user's keys over the
  /// menus' defaults, looked up on the open sketch's toolbar first, then the tab's, then the rest
  /// of the workspace's (so in a sketch A is Arc, elsewhere Appearance). `specific` looks only at
  /// keys with two or more modifiers. Returns whether one ran.
  fn command_keys(&mut self, ctx: &egui::Context, specific: bool) -> bool {
    use egui::{Key, Modifiers};
    use panels::Cmd;
    if !ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))) {
      return false;
    }
    let count = |m: Modifiers| m.command as u8 + m.shift as u8 + m.alt as u8;
    let mut keys: Vec<(Modifiers, Key, Cmd)> =
      self.command_key_table().into_iter().filter(|(m, ..)| !specific || count(*m) >= 2).collect();
    // a key matches with Shift or Alt held too, so Shift+E is looked for before E
    keys.sort_by_key(|(m, ..)| std::cmp::Reverse(count(*m)));
    for (m, k, c) in keys {
      if ctx.input_mut(|i| i.consume_key(m, k)) {
        match c {
          // these work in a sketch without finishing it
          Cmd::Delete | Cmd::ComputeAll => c.run(self),
          _ => self.launch(c),
        }
        return true;
      }
    }
    false
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
    self.end_hint();
    self.tool = t;
    self.drag = None;
    viewport::reset_tool_state(self);
  }

  /// No command, tool, dialog or pop-up is running: Space and Enter can repeat the last command.
  fn idle(&self) -> bool {
    self.dialog.is_none()
      && self.tool == Tool::Select
      && self.tstate.is_empty()
      && self.marking.is_none()
      && self.toolbox.is_none()
      && self.dim_edit.is_none()
      && self.rename.is_none()
      && self.nav_mode.is_none()
      && self.drawing.is_none()
  }

  /// Runs the last repeatable command again, as the marking menu's Repeat does (Space, Enter).
  pub(crate) fn repeat_last(&mut self) {
    match self.last_cmd {
      Some(c) => self.launch(c),
      None => self.hint("Repeat: no command to repeat yet"),
    }
  }

  /// Remembers a command the user started, for the marking menu's Repeat and the S toolbox's
  /// recent row. Every way of starting one (shortcut, toolbox, toolbar, menus) comes through
  /// here, via [`panels::Cmd::run`] or [`App::start`].
  pub(crate) fn note_cmd(&mut self, c: panels::Cmd) {
    // a command starting closes the File and Help menus (and any other drop-down), as in Fusion
    self.close_menus = true;
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
      self.hint("Show/Hide: select bodies, faces or edges first");
      return;
    }
    for b in bodies {
      let visible = self.s.body_visible(b);
      self.run("body.visible", json!({"body": b.0, "visible": !visible}));
    }
    self.s.selection.clear();
  }

  pub fn start(&mut self, kind: dialogs::Kind) {
    self.end_hint();
    self.note_cmd(panels::Cmd::Dialog(kind));
    self.cancel_dialog();
    self.tool = Tool::Select;
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

  /// Opens Trace Image on canvas `id`: in the sketch being edited, or in a new sketch on the
  /// canvas's plane. The trace lands on the image where the canvas lies, at its calibrated size.
  pub fn trace_canvas(&mut self, id: fox_engine::model::FeatureId) {
    let Some(fox_engine::model::FeatureKind::Canvas { image, plane, .. }) =
      self.s.design.feature(id).map(|f| f.kind.clone())
    else {
      return;
    };
    if self.s.edit_sketch.is_none() {
      let on = match plane {
        fox_engine::model::PlaneRef::Origin { plane } => json!(match plane {
          fox_engine::model::OriginPlane::Xy => "xy",
          fox_engine::model::OriginPlane::Xz => "xz",
          fox_engine::model::OriginPlane::Yz => "yz",
        }),
        fox_engine::model::PlaneRef::Construction { feature } => json!({"feature": feature.0}),
        _ => match self.s.model.canvases.get(&id) {
          Some(p) => {
            let n = p.x.cross(p.y);
            json!({"origin": p.origin.to_array(), "normal": n.to_array(), "x": p.x.to_array()})
          }
          None => return,
        },
      };
      self.create_sketch(on);
      if self.s.edit_sketch.is_none() {
        return;
      }
    }
    self.cancel_dialog();
    self.tool = Tool::Select;
    let mut d = Dialog::open(self, dialogs::Kind::TraceImage);
    d.trace = Some(Box::new(trace::TraceState::with_canvas(id, PathBuf::from(image))));
    d.preview(&mut self.s);
    self.dialog = Some(d);
  }

  pub fn cancel_dialog(&mut self) {
    if let Some(mut d) = self.dialog.take() {
      d.cancel(&mut self.s);
      d.leave(self);
    }
  }

  /// Starts a sketch on the selected plane or planar face, or asks for one: the origin planes
  /// show in the viewport to click, as in Fusion.
  pub fn new_sketch(&mut self) {
    self.note_cmd(panels::Cmd::NewSketch);
    // the pressed ribbon button cancels the pick, as in Fusion
    if self.dialog.as_ref().is_some_and(|d| d.kind == dialogs::Kind::PickPlane) {
      self.cancel_dialog();
      return;
    }
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
      self.auto_look_at_sketch();
      self.tab = panels::Tab::Sketch;
      self.browser_sketches = Some(true);
      // as in Fusion, no tool is active until one is picked
      self.set_tool(Tool::Select);
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
      self.auto_look_at_sketch();
      self.tab = panels::Tab::Sketch;
      self.browser_sketches = Some(true);
      self.tool = Tool::Select;
    }
  }

  /// Finish Sketch: back to the 3D view with nothing selected and the browser's Sketches folder
  /// collapsed, as in Fusion.
  pub fn finish_sketch(&mut self) {
    self.end_hint();
    self.last_sketch = self.s.edit_sketch;
    self.left_sketch();
    self.run("sketch.finish", json!({}));
    self.tool = Tool::Select;
    self.s.selection.clear();
    self.browser_sketches = Some(false);
  }

  /// When the open sketch is finished: back to the 3D view from before it.
  pub(crate) fn left_sketch(&mut self) {
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

  /// Named Views > New Named View: saves where the camera is (or is headed) in the design.
  /// Returns the new view's name.
  pub(crate) fn save_named_view(&mut self) -> Option<String> {
    let c = self.view_now();
    let r = self.run(
      "view.named.save",
      json!({"target": c.target, "yaw": c.yaw, "pitch": c.pitch, "distance": c.distance, "fov": c.fov, "ortho": c.ortho}),
    )?;
    let name = r.get("name").and_then(Value::as_str)?.to_string();
    self.status = Some((format!("Saved the view as '{name}'"), false));
    Some(name)
  }

  /// Flies the camera to a named view (a browser double-click).
  pub(crate) fn restore_named_view(&mut self, name: &str) {
    let Some(v) = self.s.design.named_views.iter().find(|v| v.name == name) else { return };
    // a named view doesn't keep the camera's roll: it comes back upright
    let to = Camera {
      target: v.target,
      yaw: v.yaw,
      pitch: v.pitch,
      distance: v.distance,
      fov: v.fov,
      ortho: v.ortho,
      roll: 0.0,
    };
    self.fly_to(to);
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

  /// Turns the camera to the opened sketch, when Preferences → Sketch → Auto look at sketch is on.
  fn auto_look_at_sketch(&mut self) {
    if self.preferences.values.auto_look_at {
      self.look_at_sketch();
    }
  }

  fn look_at_sketch(&mut self) {
    let Some(plane) = self.s.edit_sketch.and_then(|e| self.s.model.sketches.get(&e)).map(|r| r.plane) else {
      return;
    };
    self.look_at_plane(&plane);
  }

  /// Turns the camera to look straight at a plane (a sketch's, or an origin plane).
  pub(crate) fn look_at_plane(&mut self, plane: &fox_engine::kernel::Plane) {
    let n = plane.normal();
    let o = plane.origin;
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
      None => self.hint("Look At: select a planar face"),
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

  /// The Sketch Palette's Centerline toggle: the selected curves become centerlines (construction
  /// drawn dash-dot) or normal curves again.
  pub fn toggle_centerline(&mut self) {
    let curves: Vec<u32> = self
      .s
      .selection
      .iter()
      .filter_map(|s| if let Sel::SketchCurve { curve, .. } = s { Some(curve.0) } else { None })
      .collect();
    if !curves.is_empty() {
      self.run("sketch.centerline", json!({"curves": curves}));
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
    let (focused, time) = ctx.input(|i| (i.focused, i.time));
    if focused && !self.window_focused {
      self.activated_at = time;
    }
    self.window_focused = focused;
    units::publish(&ctx, self.s.design.units);
    if self.poll_opening() {
      self.draw_busy(ui, &ctx);
      return;
    }
    if self.poll_rebuild() {
      self.draw_busy(ui, &ctx);
      return;
    }
    if self.data.save.is_none() && self.electronics.is_none() {
      // typed values go to the heads-up box or dialog before shortcuts see the keys
      viewport::route_typing(self, &ctx);
      self.shortcuts(&ctx);
    }
    // the Manufacture workspace keeps its toolpaths in step with the design
    if self.manufacture && !self.s.design.cam.is_empty() && self.s.toolpaths_of.map(|t| t.1) != Some(self.s.revision) {
      self.cam_generate(false);
      self.s.toolpaths_of = Some((self.s.toolpaths_of.map(|t| t.0).unwrap_or(0), self.s.revision));
    }
    // and its 2D cut preview
    self.refresh_cut_preview();
    self.dropped_files(&ctx);
    plugins_ui::sync(self);
    preferences::recovery_tick(self, now());
    panels::app_bar(self, ui);
    panels::toolbar(self, ui);
    if self.drawing.is_none() && self.electronics.is_none() {
      panels::timeline(self, ui);
    }
    data_panel::heartbeat(self, &ctx);
    data_panel::show(self, ui);
    if self.drawing.is_none() {
      props_ui::show(self, ui);
    }
    if self.step_cam_move(now()) {
      ctx.request_repaint();
    }
    viewcube::keep_projection(self);
    if self.electronics.is_some() {
      egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
        ecad_ui::show(self, ui);
      });
    } else if self.drawing.is_some() {
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
      form_ui::panel(self, &ctx);
      panels::isolate_banner(self, &ctx);
      if self.show_navbar {
        panels::nav_bar(self, &ctx);
      }
      panels::assistant_tab(self, &ctx);
      panels::sketch_palette(self, &ctx);
      marking::show(self, &ctx);
      sim_ui::show(self, &ctx);
    }
    toolbox::show(self, &ctx);
    panels::canvas_status(self, &ctx);
    panels::unsaved_banner(self, &ctx);
    panels::selection_info(self, &ctx);
    notifications::panel(self, &ctx);
    marking::end_frame(self, &ctx);
    if let Some(mut d) = self.dialog.take() {
      let keep = d.show(self, &ctx);
      if keep && self.dialog.is_none() {
        self.dialog = Some(d);
      } else {
        d.leave(self);
      }
    }
    if self.show_params {
      params_ui::window(self, &ctx);
    }
    data_panel::save_window(self, &ctx);
    updates::window(self, &ctx);
    capture::windows(self, &ctx);
    navbar::ground_offset_window(self, &ctx);
    customize::key_window(self, &ctx);
    cam_ui::tool_library_window(self, &ctx);
    config_ui::table_window(self, &ctx);
    preferences::window(self, &ctx);
    cut_ui::cut_window(self, &ctx);
    gen_ui::window(self, &ctx);
    render_ui::windows(self, &ctx);
    canvas_ui::windows(self, &ctx);
    library_ui::window(self, &ctx);
    help::window(self, &ctx);
    plugins_ui::window(self, &ctx);
    explode_ui::window(self, &ctx);
    service_ui::window(self, &ctx);
    if std::mem::take(&mut self.close_menus) {
      egui::Popup::close_all(&ctx);
    }
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

impl App {
  /// Whether the window came to the front just now. The click or key that activates it (or one
  /// sent along with a later launch's file) wasn't meant for what happens to be under it.
  pub(crate) fn just_activated(&self, ctx: &egui::Context) -> bool {
    ctx.input(|i| i.time) - self.activated_at < ACTIVATION_GRACE_SECS
  }
}

/// How long after the window comes to the front its input isn't taken as confirming a save.
const ACTIVATION_GRACE_SECS: f64 = 0.4;

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

  #[test]
  fn a_design_opened_over_another_is_drawn_and_fitted() {
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/bracket.fox"));
    let mut app = App::with_session(Session::open(&path).unwrap());
    let before = app.s.revision;
    app.cam = Camera::default();
    app.save(false);
    // a later launch hands over a file: it opens on another thread (Desktop::open_handed)
    app.open_file(path.clone());
    while app.poll_opening() {
      std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // every session counts its revisions from the same start, and the viewport rebuilds its
    // scene only when the revision or what's shown changes
    assert!(app.s.revision > before, "{} after {before}", app.s.revision);
    assert!(app.render_key.is_none());
    assert!(app.data.save.is_none(), "the Save dialog was for the design that was replaced");
    let mut fitted = Camera::default();
    let (lo, hi) = app.s.model.bounds().unwrap();
    fitted.fit(lo, hi, 1.6);
    assert_eq!(app.cam, fitted, "the view is fitted to the opened design");
    // and so with File > Open and a new design
    let before = app.s.revision;
    app.open_path(&path);
    assert!(app.s.revision > before);
    let before = app.s.revision;
    app.new_design();
    assert!(app.s.revision > before);
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
    // Fusion writes nothing in the canvas corner while sketching
    assert!(app.status.is_none(), "no Sketching hint: {:?}", app.status);
    app.finish_sketch();
    settle_camera(&mut app);
    assert_eq!(app.cam, before);
    assert!(app.status.is_none(), "{:?}", app.status);
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
    assert!(!app.step_cam_move(t0 + CAM_MOVE_SECS + 1e-9), "done after {CAM_MOVE_SECS} s");
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
  fn space_and_enter_repeat_the_last_command() {
    let ctx = egui::Context::default();
    let mut app = with_box();
    let mid = app.canvas.center();
    let fillet = Some(dialogs::Kind::Fillet);
    // nothing to repeat yet
    press(&mut app, &ctx, egui::Key::Space, mid);
    assert!(app.dialog.is_none());
    press(&mut app, &ctx, egui::Key::F, mid);
    assert_eq!(app.dialog.as_ref().map(|d| d.kind), fillet);
    // while the command runs Space doesn't start it again
    press(&mut app, &ctx, egui::Key::Space, mid);
    assert_eq!(app.dialog.as_ref().map(|d| d.kind), fillet);
    app.cancel_dialog();
    assert!(app.preferences.values.space_repeats, "on by default, as in Fusion");
    press(&mut app, &ctx, egui::Key::Space, mid);
    assert_eq!(app.dialog.as_ref().map(|d| d.kind), fillet, "Space repeats Fillet");
    app.cancel_dialog();
    press(&mut app, &ctx, egui::Key::Enter, mid);
    assert_eq!(app.dialog.as_ref().map(|d| d.kind), fillet, "so does Enter");
    app.cancel_dialog();
    // Preferences → General → Spacebar repeats last command off: Space does nothing
    app.preferences.values.space_repeats = false;
    press(&mut app, &ctx, egui::Key::Space, mid);
    assert!(app.dialog.is_none());
    // a sketch tool running isn't idle either
    app.preferences.values.space_repeats = true;
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    app.set_tool(Tool::Line);
    press(&mut app, &ctx, egui::Key::Space, mid);
    assert!(app.dialog.is_none() && app.tool == Tool::Line);
  }

  #[test]
  fn ctrl_c_and_ctrl_v_paste_a_placed_copy() {
    let ctx = egui::Context::default();
    let mut app = with_box();
    let body = app.s.model.bodies[0].id;
    app.s.selection = vec![Sel::Body { body }];
    let frame = |app: &mut App, mut events: Vec<egui::Event>, modifiers: egui::Modifiers| {
      events.insert(0, egui::Event::ModifiersChanged(modifiers));
      let input = egui::RawInput { events, ..Default::default() };
      ctx.run_ui(input, |ui| app.shortcuts(ui.ctx())).drop_without_applying_deltas();
    };
    // as the window delivers them: Copy and Paste events
    frame(&mut app, vec![egui::Event::Copy], egui::Modifiers::COMMAND);
    assert!(app.clipboard.is_some());
    frame(&mut app, vec![egui::Event::Paste("FissionOxide: Body1".into())], egui::Modifiers::COMMAND);
    assert_eq!(app.dialog.as_ref().map(|d| d.kind), Some(dialogs::Kind::Move));
    assert_eq!(app.s.model.bodies.len(), 2);
    app.cancel_dialog();
    assert_eq!(app.s.model.bodies.len(), 1);
    // and as keys
    let key = |key, modifiers| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
    let ctrl_shift = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    frame(&mut app, vec![key(egui::Key::V, ctrl_shift)], ctrl_shift);
    assert!(matches!(app.dialog.as_ref().and_then(|d| d.paste()), Some(dialogs::Paste::Copy)));
    app.cancel_dialog();
    frame(&mut app, vec![key(egui::Key::V, egui::Modifiers::COMMAND)], egui::Modifiers::COMMAND);
    assert_eq!(app.s.model.bodies.len(), 2);
    app.cancel_dialog();
    // a paste isn't Repeat's command
    assert_ne!(app.last_cmd, Some(panels::Cmd::Dialog(dialogs::Kind::Move)));
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
  fn changed_keys_run_their_commands() {
    let ctx = egui::Context::default();
    let mut app = with_box();
    let mid = app.canvas.center();
    app.prefs.keys.set("design.Extrude", "G", "E");
    press(&mut app, &ctx, egui::Key::E, mid);
    assert!(app.dialog.is_none());
    press(&mut app, &ctx, egui::Key::G, mid);
    assert_eq!(app.dialog.as_ref().map(|d| d.kind), Some(dialogs::Kind::Extrude));
    // a captured key goes to Change Keyboard Shortcut, not to the commands
    app.cancel_dialog();
    app.key_capture = Some(customize::KeyCapture::for_test());
    press(&mut app, &ctx, egui::Key::G, mid);
    assert!(app.dialog.is_none());
  }

  #[test]
  fn p_in_a_sketch_is_project_not_point() {
    let ctx = egui::Context::default();
    let mut app = with_box();
    app.run("sketch.create", json!({"plane": "xy"})).unwrap();
    let sketch = app.s.edit_sketch;
    let mid = app.canvas.center();
    press(&mut app, &ctx, egui::Key::P, mid);
    assert_ne!(app.tool, Tool::Point);
    assert_eq!(app.dialog.as_ref().map(|d| d.kind), Some(dialogs::Kind::Project));
    // Project works inside the sketch: it stays open
    assert_eq!(app.s.edit_sketch, sketch);
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

  #[test]
  fn the_sketch_palette_stacks_below_a_command_dialog() {
    let canvas = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1200.0, 800.0));
    let home = panels::palette_pos(canvas, None);
    assert_eq!(home, egui::pos2(1166.0, 190.0));
    // the OFFSET dialog sits where the palette goes: the palette moves below it
    let dialog = egui::Rect::from_min_max(egui::pos2(866.0, 190.0), egui::pos2(1166.0, 330.0));
    assert_eq!(panels::palette_pos(canvas, Some(dialog)), egui::pos2(1166.0, 338.0));
    // a dialog dragged elsewhere leaves it be
    let moved = dialog.translate(egui::vec2(-600.0, 0.0));
    assert_eq!(panels::palette_pos(canvas, Some(moved)), home);
  }

  /// `ui:sketch:offset-dialog-rows`: while a sketch is open a command dialog docks beside the
  /// palette on its left, the two side by side, so the palette stays at its usual place.
  #[test]
  fn a_sketch_dialog_docks_left_of_the_palette() {
    let canvas = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1200.0, 800.0));
    assert_eq!(panels::dialog_dock(canvas, false), egui::pos2(1166.0, 190.0));
    let dock = panels::dialog_dock(canvas, true);
    assert_eq!(dock, egui::pos2(888.0, 190.0));
    // a 300 px dialog docked there doesn't reach the palette's place: the palette stays home
    let dialog = egui::Rect::from_min_max(egui::pos2(dock.x - 300.0, dock.y), egui::pos2(dock.x, dock.y + 200.0));
    assert_eq!(panels::palette_pos(canvas, Some(dialog)), panels::palette_pos(canvas, None));
  }

  /// `ui:sketch:status-readout-under-palette`: the selection readout sits at the canvas's lower
  /// right, left of the Sketch Palette when there is one, never under it.
  #[test]
  fn the_selection_readout_keeps_left_of_the_palette() {
    let canvas = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1200.0, 800.0));
    assert_eq!(panels::readout_anchor(canvas, None), egui::pos2(1186.0, 788.0));
    // the palette pushed down by a dialog reaches the canvas's bottom: the readout moves left of it
    let palette = egui::Rect::from_min_max(egui::pos2(896.0, 338.0), egui::pos2(1166.0, 820.0));
    assert_eq!(panels::readout_anchor(canvas, Some(palette)), egui::pos2(882.0, 788.0));
    // so does a palette at its usual place
    let home = egui::Rect::from_min_max(egui::pos2(896.0, 190.0), egui::pos2(1166.0, 700.0));
    assert_eq!(panels::readout_anchor(canvas, Some(home)), egui::pos2(882.0, 788.0));
  }

  /// `ui:sketch:horizontal-conflict`: the palette's Horizontal/Vertical on a line trimmed onto a
  /// circle applies (Fusion turns the line about the end on the circle) instead of reporting a
  /// conflict.
  #[test]
  fn horizontal_vertical_applies_to_a_line_trimmed_to_a_circle() {
    let mut app = App::with_session(Session::default());
    app.create_sketch(json!("xy"));
    let sk = app.s.edit_sketch.unwrap();
    app.run("sketch.circle", json!({"sketch": sk.0, "center": [5, 5], "radius": 10})).unwrap();
    let r = app.run("sketch.line", json!({"sketch": sk.0, "points": [[6, 5.5], [35, 25]]})).unwrap();
    let line = fox_engine::sketch::CurveId(r["curves"][0].as_u64().unwrap() as u32);
    app.run("sketch.trim", json!({"sketch": sk.0, "curve": line.0, "at": [6.3, 5.7]})).unwrap();
    app.s.selection = vec![Sel::SketchCurve { sketch: sk, curve: line }];
    panels::Cmd::Constraint("hv").run(&mut app);
    assert!(app.status.as_ref().is_none_or(|s| !s.1), "{:?}", app.status);
    let s = app.s.design.sketch(sk).unwrap();
    assert!(
      s.constraints
        .iter()
        .any(|c| matches!(c.kind, fox_engine::sketch::ConstraintKind::Horizontal { line: l } if l == line)),
      "{:?}",
      s.constraints
    );
    let [a, b] = [s.curve_points(line)[0], s.curve_points(line)[1]];
    assert!((s.pos(a).unwrap()[1] - s.pos(b).unwrap()[1]).abs() < 1e-9);
    assert!(app.s.selection.is_empty(), "the selection clears once the constraint is on");
  }

  #[test]
  fn a_tool_hint_clears_when_the_tool_ends() {
    let mut app = App::with_session(Session::default());
    app.create_sketch(json!("xy"));
    // Horizontal/Vertical with nothing selected asks for lines
    panels::Cmd::Constraint("hv").run(&mut app);
    assert!(app.status.as_ref().is_some_and(|s| s.0.starts_with("Horizontal/Vertical")));
    // picking a tool ends it: the canvas hint describes the tool again
    app.set_tool(Tool::Trim);
    assert!(app.status.is_none(), "{:?}", app.status);
    panels::Cmd::Constraint("hv").run(&mut app);
    app.finish_sketch();
    assert!(app.status.is_none(), "no stale hint after Finish Sketch: {:?}", app.status);
    // other messages stay
    app.hint("Look At: select a planar face");
    app.status = Some(("Saved".into(), false));
    app.end_hint();
    assert_eq!(app.status, Some(("Saved".into(), false)));
  }
}
