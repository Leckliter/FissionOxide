//! The navigation bar's fly-outs: Orbit and Zoom choices, Display Settings (visual style,
//! environment, graphics preset, effects, object visibility, camera, ground plane offset, full
//! screen) and Grid and Snaps, as in Fusion. The bar itself is [`crate::panels::nav_bar`].

use egui::RichText;
use fox_engine::render::{Effects, VisualStyle};

use crate::icons::Icon;
use crate::viewcube::Projection;
use crate::{App, NavMode, theme};

/// Display Settings ▸ Object Visibility: which kinds of things the viewport draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectVisibility {
  /// The origin axes on the grid.
  pub origins: bool,
  pub sketches: bool,
  /// Construction planes, axes and points.
  pub construction: bool,
  /// Joint glyphs and joint origins.
  pub joints: bool,
  pub bodies: bool,
}

impl Default for ObjectVisibility {
  fn default() -> Self {
    ObjectVisibility { origins: true, sketches: true, construction: true, joints: true, bodies: true }
  }
}

/// Grid and Snaps ▸ Grid Settings, and Layout Grid Lock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridSettings {
  /// The spacing follows the model's size; else it is `spacing`.
  pub adaptive: bool,
  /// Fixed spacing (mm).
  pub spacing: f64,
  /// Layout Grid Lock: the spacing frozen when the lock went on.
  pub locked: Option<f64>,
  /// Reference Numbers: the coordinate values printed along the grid's axes.
  pub reference_numbers: bool,
}

impl Default for GridSettings {
  fn default() -> Self {
    GridSettings { adaptive: true, spacing: 10.0, locked: None, reference_numbers: true }
  }
}

impl GridSettings {
  /// The grid's spacing when it doesn't adapt (locked or fixed).
  pub fn step(&self) -> Option<f64> {
    self.locked.or((!self.adaptive).then_some(self.spacing)).filter(|s| s.is_finite() && *s > 0.0)
  }

  /// Turns Layout Grid Lock on, freezing the grid at `now` (the spacing it has), or off.
  pub fn set_lock(&mut self, on: bool, now: f64) {
    self.locked = on.then(|| self.step().unwrap_or(now));
  }
}

/// Grid and Snaps ▸ Incremental Move and Set Increments: manipulator drags move and turn in
/// these steps. Fusion has Incremental Move on and the increments Adaptive by default: the steps
/// follow the grid (and the view) until Set Increments fixes them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Increments {
  pub on: bool,
  /// The steps follow the grid; else they are `linear` and `angular`.
  pub adaptive: bool,
  /// Linear step (mm).
  pub linear: f64,
  /// Angular step (degrees).
  pub angular: f64,
}

impl Default for Increments {
  fn default() -> Self {
    Increments { on: true, adaptive: true, linear: 1.0, angular: 15.0 }
  }
}

impl Increments {
  /// The step a move snaps to: the fixed linear increment with Incremental Move on and the
  /// increments Fixed, else `free` (the grid's own step, which the adaptive increments follow).
  pub fn linear_step(&self, free: f64) -> f64 {
    if self.on && !self.adaptive && self.linear > 0.0 { self.linear } else { free }
  }

  /// The step a turn snaps to (radians).
  pub fn angular_step(&self, free: f64) -> f64 {
    if self.on && !self.adaptive && self.angular > 0.0 { self.angular.to_radians() } else { free }
  }
}

/// The Orbit fly-out: Free Orbit lets the view roll and go over the top, Constrained Orbit keeps
/// +Z up. Choosing one starts orbiting.
pub fn orbit_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(170.0);
  for (free, label) in [(false, "Constrained Orbit"), (true, "Free Orbit")] {
    if ui.radio(app.free_orbit == free, label).clicked() {
      app.free_orbit = free;
      if !free {
        // back to +Z up, as Fusion's constrained orbit is
        let mut to = app.cam;
        to.roll = 0.0;
        app.fly_to(to);
      }
      app.nav_mode = Some(NavMode::Orbit);
      ui.close();
    }
  }
}

/// The navigation bar's buttons, left to right, by their tooltips: Fusion's one or two words.
pub const BUTTONS: [&str; 8] =
  ["Orbit", "Look At", "Pan", "Zoom", "Zoom Window", "Display Settings", "Grid and Snaps", "Viewports"];

/// The Zoom Window button's fly-out, as Fusion's: Zoom Window, then Fit with its F6 key, each
/// with its icon.
pub const ZOOM_MENU: [(Icon, &str, Option<&str>); 2] =
  [(Icon::ZoomWindow, "Zoom Window", None), (Icon::Fit, "Fit", Some("F6"))];

/// A navigation bar button's tooltip, as Fusion draws it: the button's name in white on a dark
/// panel, nothing more.
pub(crate) fn dark_tip(resp: egui::Response, text: &str) -> egui::Response {
  let style = resp.ctx.global_style();
  let frame = egui::Frame::new()
    .fill(theme::TOOLTIP)
    .corner_radius(egui::CornerRadius::same(3))
    .inner_margin(egui::Margin::symmetric(8, 4))
    .shadow(style.visuals.popup_shadow);
  let mut tip = egui::Tooltip::for_enabled(&resp);
  tip.popup = tip.popup.frame(frame);
  tip.show(|ui| {
    ui.style_mut().visuals.override_text_color = Some(theme::TOOLTIP_TITLE);
    ui.add(egui::Label::new(text).extend());
  });
  resp
}

/// The fly-out after Zoom Window: Zoom Window (drag a box to zoom into) and Fit.
pub fn zoom_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(150.0);
  for (icon, label, key) in ZOOM_MENU {
    if crate::panels::menu_item(ui, icon, label, key).clicked() {
      if icon == Icon::Fit {
        app.fit_view();
      } else {
        app.nav_mode = Some(NavMode::ZoomWindow);
      }
      ui.close();
    }
  }
}

/// Display Settings ▸ Environment: Fusion's environment library, as the Design workspace lists
/// it. The choice is kept for the Render workspace; the modelling view's lighting is fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Environment {
  #[default]
  Photobooth,
  SharpHighlights,
  SoftLight,
  DryLakeBed,
  DimField,
  Field,
  GreyRoom,
  InfinityPool,
  Plaza,
  RimHighlights,
  SnowField,
  TranquilityBlue,
  WarmLight,
  CoolLight,
}

impl Environment {
  pub const ALL: [Environment; 14] = [
    Environment::Photobooth,
    Environment::SharpHighlights,
    Environment::SoftLight,
    Environment::DryLakeBed,
    Environment::DimField,
    Environment::Field,
    Environment::GreyRoom,
    Environment::InfinityPool,
    Environment::Plaza,
    Environment::RimHighlights,
    Environment::SnowField,
    Environment::TranquilityBlue,
    Environment::WarmLight,
    Environment::CoolLight,
  ];

  pub fn label(self) -> &'static str {
    match self {
      Environment::Photobooth => "Photobooth",
      Environment::SharpHighlights => "Sharp Highlights",
      Environment::SoftLight => "Soft Light",
      Environment::DryLakeBed => "Dry Lake Bed",
      Environment::DimField => "Dim Field",
      Environment::Field => "Field",
      Environment::GreyRoom => "Grey Room",
      Environment::InfinityPool => "Infinity Pool",
      Environment::Plaza => "Plaza",
      Environment::RimHighlights => "Rim Highlights",
      Environment::SnowField => "Snow Field",
      Environment::TranquilityBlue => "Tranquility Blue",
      Environment::WarmLight => "Warm Light",
      Environment::CoolLight => "Cool Light",
    }
  }
}

/// Display Settings ▸ Graphics Preset, as Fusion's: Performance turns the effects off, Quality
/// turns them on, and Custom is whatever the Effects menu says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphicsPreset {
  Performance,
  Quality,
  #[default]
  Custom,
}

impl GraphicsPreset {
  pub const ALL: [GraphicsPreset; 3] = [GraphicsPreset::Performance, GraphicsPreset::Quality, GraphicsPreset::Custom];

  pub fn label(self) -> &'static str {
    match self {
      GraphicsPreset::Performance => "Performance",
      GraphicsPreset::Quality => "Quality",
      GraphicsPreset::Custom => "Custom",
    }
  }

  /// The effects a preset sets (`None`: Custom leaves them as they are).
  pub fn effects(self) -> Option<Effects> {
    match self {
      GraphicsPreset::Performance => Some(Effects { ground_shadow: false, reflections: false, ground_plane: false }),
      GraphicsPreset::Quality => Some(Effects { ground_shadow: true, reflections: true, ground_plane: true }),
      GraphicsPreset::Custom => None,
    }
  }
}

/// Display Settings' own settings.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplaySettings {
  pub environment: Environment,
  pub preset: GraphicsPreset,
  /// Ground Plane Offset: how far below the model's lowest point the ground sits (mm).
  pub ground_offset: f64,
  /// The Ground Plane Offset dialog, with the value being edited.
  pub ground_offset_form: Option<f64>,
}

impl Default for DisplaySettings {
  fn default() -> Self {
    DisplaySettings {
      environment: Environment::default(),
      preset: GraphicsPreset::default(),
      ground_offset: 0.0,
      ground_offset_form: None,
    }
  }
}

impl DisplaySettings {
  /// Picks a graphics preset, applying its effects.
  pub fn set_preset(&mut self, preset: GraphicsPreset, effects: &mut Effects) {
    self.preset = preset;
    if let Some(e) = preset.effects() {
      *effects = e;
    }
  }

  /// The preset the effects match now: a change in the Effects menu makes it Custom.
  pub fn preset_for(effects: Effects) -> GraphicsPreset {
    GraphicsPreset::ALL.into_iter().find(|p| p.effects() == Some(effects)).unwrap_or(GraphicsPreset::Custom)
  }
}

/// Display Settings ▸ Enter Full Screen / Exit Full Screen (Ctrl+Shift+F).
pub fn toggle_full_screen(ctx: &egui::Context) {
  let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
  ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
}

/// Fusion's Display Settings fly-out, in its order.
pub fn display_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(220.0);
  ui.menu_button("Visual Style", |ui| {
    ui.set_min_width(250.0);
    for s in VisualStyle::ALL {
      if ui.radio(app.visual_style == s, s.label()).clicked() {
        app.visual_style = s;
      }
    }
  });
  ui.menu_button("Environment", |ui| {
    ui.set_min_width(180.0);
    for e in Environment::ALL {
      if ui.radio(app.display.environment == e, e.label()).clicked() {
        app.display.environment = e;
      }
    }
  });
  ui.menu_button("Graphics Preset", |ui| {
    ui.set_min_width(160.0);
    let current = DisplaySettings::preset_for(app.effects);
    for p in GraphicsPreset::ALL {
      if ui.radio(current == p, p.label()).clicked() {
        app.display.set_preset(p, &mut app.effects);
      }
    }
  });
  ui.menu_button("Effects", |ui| {
    ui.set_min_width(160.0);
    let e: &mut Effects = &mut app.effects;
    ui.checkbox(&mut e.ground_shadow, "Ground Shadow");
    ui.checkbox(&mut e.reflections, "Reflections");
    ui.checkbox(&mut e.ground_plane, "Ground Plane");
  });
  ui.menu_button("Object Visibility", |ui| {
    ui.set_min_width(180.0);
    let v = &mut app.objects;
    ui.checkbox(&mut v.origins, "Origins");
    ui.checkbox(&mut v.sketches, "Sketches");
    ui.checkbox(&mut v.construction, "Construction Geometry");
    ui.checkbox(&mut v.joints, "Joints");
    ui.checkbox(&mut v.bodies, "Bodies");
  });
  ui.menu_button("Camera", |ui| {
    ui.set_min_width(220.0);
    let now = app.cube.projection(&app.cam);
    for (label, mode) in [
      ("Orthographic", Projection::Orthographic),
      ("Perspective", Projection::Perspective),
      ("Perspective with Ortho Faces", Projection::OrthoFaces),
    ] {
      if ui.radio(now == mode, label).clicked() {
        set_projection(app, mode);
      }
    }
  });
  if ui.button("Ground Plane Offset").clicked() {
    app.display.ground_offset_form = Some(app.display.ground_offset);
    ui.close();
  }
  let full = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
  let label = if full { "Exit Full Screen" } else { "Enter Full Screen" };
  if ui.add(egui::Button::new(label).shortcut_text("Ctrl+Shift+F")).clicked() {
    toggle_full_screen(ui.ctx());
    ui.close();
  }
}

/// Camera ▸ a projection (shared with the ViewCube menu).
pub(crate) fn set_projection(app: &mut App, mode: Projection) {
  app.cube.ortho_faces = mode == Projection::OrthoFaces;
  app.cam.ortho = mode == Projection::Orthographic;
  crate::viewcube::keep_projection(app);
}

/// The Ground Plane Offset dialog, while open.
pub fn ground_offset_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut value) = app.display.ground_offset_form else { return };
  let mut open = true;
  let mut done = false;
  let mut cancel = false;
  egui::Window::new("Ground Plane Offset")
    .open(&mut open)
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
    .show(ctx, |ui| {
      ui.horizontal(|ui| {
        ui.label("Offset");
        ui.add(egui::DragValue::new(&mut value).speed(0.5).suffix(" mm"));
      });
      ui.label(RichText::new("How far below the model the ground plane sits.").small().weak());
      ui.separator();
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.button("Cancel").clicked() {
          cancel = true;
        }
        if ui.button("OK").clicked() {
          done = true;
        }
      });
    });
  if done {
    app.display.ground_offset = value;
    app.display.ground_offset_form = None;
  } else if !open || cancel {
    app.display.ground_offset_form = None;
  } else {
    app.display.ground_offset_form = Some(value);
  }
}

/// Grid and Snaps, as Fusion lists it: Layout Grid (the 3D view's ground grid, off by default),
/// Layout Grid Lock (greyed while the grid is off, and checked when the grid goes on), Snap to
/// Grid, Grid Settings, then Incremental Move and Set Increments. The two command rows carry icons
/// and open the docked GRID SETTINGS and SET INCREMENTS dialogs (see [`crate::grid_dialogs`]).
/// `adaptive_now` is the spacing the adaptive grid has (what a lock freezes).
pub fn grid_menu(app: &mut App, ui: &mut egui::Ui, adaptive_now: f64) {
  ui.set_min_width(190.0);
  if ui.checkbox(&mut app.layout_grid, "Layout Grid").changed() {
    set_layout_grid(app, app.layout_grid, adaptive_now);
  }
  let mut lock = app.grid.locked.is_some();
  if ui
    .add_enabled(app.layout_grid, egui::Checkbox::new(&mut lock, "Layout Grid Lock"))
    .on_hover_text("Keep the grid's spacing as it is now")
    .changed()
  {
    app.grid.set_lock(lock, adaptive_now);
  }
  ui.checkbox(&mut app.snap, "Snap to Grid");
  if icon_row(ui, Icon::GridSettings, "Grid Settings").clicked() {
    crate::grid_dialogs::open_grid_settings(app);
    ui.close();
  }
  ui.separator();
  ui.checkbox(&mut app.increments.on, "Incremental Move")
    .on_hover_text("Manipulator drags move and turn in the set increments");
  if icon_row(ui, Icon::SetIncrements, "Set Increments").clicked() {
    crate::grid_dialogs::open_set_increments(app);
    ui.close();
  }
}

/// Layout Grid on or off. Fusion locks the grid's spacing as the grid goes on (Layout Grid Lock
/// comes up checked) and drops the lock with the grid, whose row greys.
pub fn set_layout_grid(app: &mut App, on: bool, adaptive_now: f64) {
  app.layout_grid = on;
  app.grid.set_lock(on, adaptive_now);
}

/// A menu row with an icon at its left, as Fusion draws the Grid Settings and Set Increments
/// commands.
fn icon_row(ui: &mut egui::Ui, icon: Icon, label: &str) -> egui::Response {
  let width = ui.available_width();
  let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 22.0), egui::Sense::click());
  let p = ui.painter();
  if resp.hovered() {
    p.rect_filled(rect, 0.0, crate::theme::HOVER);
  }
  let ir = egui::Rect::from_center_size(rect.left_center() + egui::vec2(12.0, 0.0), egui::Vec2::splat(17.0));
  crate::icons::draw_color(p, ir, icon);
  p.text(
    rect.left_center() + egui::vec2(26.0, 0.0),
    egui::Align2::LEFT_CENTER,
    label,
    egui::FontId::proportional(12.5),
    crate::theme::TEXT,
  );
  resp
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn grid_lock_freezes_the_spacing_it_has() {
    let mut g = GridSettings::default();
    assert_eq!(g.step(), None, "adaptive");
    g.set_lock(true, 5.0);
    assert_eq!(g.step(), Some(5.0));
    // switching to fixed doesn't move a locked grid
    g.adaptive = false;
    g.spacing = 2.0;
    assert_eq!(g.step(), Some(5.0));
    g.set_lock(false, 5.0);
    assert_eq!(g.step(), Some(2.0));
    g.set_lock(true, 50.0);
    assert_eq!(g.step(), Some(2.0), "a fixed grid locks at its own spacing");
    g.spacing = 0.0;
    g.locked = None;
    assert_eq!(g.step(), None, "no zero spacing");
  }

  #[test]
  fn graphics_presets_set_the_effects_and_a_change_makes_custom() {
    let mut d = DisplaySettings::default();
    let mut e = Effects::default();
    d.set_preset(GraphicsPreset::Performance, &mut e);
    assert_eq!(e, Effects { ground_shadow: false, reflections: false, ground_plane: false });
    assert_eq!(DisplaySettings::preset_for(e), GraphicsPreset::Performance);
    d.set_preset(GraphicsPreset::Quality, &mut e);
    assert!(e.ground_shadow && e.reflections && e.ground_plane);
    assert_eq!(DisplaySettings::preset_for(e), GraphicsPreset::Quality);
    e.reflections = false;
    assert_eq!(DisplaySettings::preset_for(e), GraphicsPreset::Custom);
    d.set_preset(GraphicsPreset::Custom, &mut e);
    assert!(!e.reflections, "Custom leaves the effects alone");
    assert_eq!(Environment::ALL.len(), 14);
    assert_eq!(Environment::default().label(), "Photobooth");
  }

  #[test]
  fn the_camera_submenu_sets_the_projection() {
    let mut app = App::with_session(fox_engine::Session::default());
    set_projection(&mut app, Projection::Orthographic);
    assert!(app.cam.ortho && !app.cube.ortho_faces);
    assert_eq!(app.cube.projection(&app.cam), Projection::Orthographic);
    set_projection(&mut app, Projection::Perspective);
    assert!(!app.cam.ortho && !app.cube.ortho_faces);
    set_projection(&mut app, Projection::OrthoFaces);
    assert!(app.cube.ortho_faces);
    assert_eq!(app.cube.projection(&app.cam), Projection::OrthoFaces);
    // Ground Plane Offset keeps its value through its dialog
    app.display.ground_offset_form = Some(12.5);
    let ctx = egui::Context::default();
    let input = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
      ..Default::default()
    };
    ctx.run_ui(input, |ui| ground_offset_window(&mut app, ui.ctx())).drop_without_applying_deltas();
    assert_eq!(app.display.ground_offset_form, Some(12.5), "open until OK");
    assert_eq!(app.display.ground_offset, 0.0);
  }

  #[test]
  fn the_bar_and_the_zoom_fly_out_read_as_fusions() {
    assert_eq!(
      BUTTONS,
      ["Orbit", "Look At", "Pan", "Zoom", "Zoom Window", "Display Settings", "Grid and Snaps", "Viewports"]
    );
    let rows: Vec<(&str, Option<&str>)> = ZOOM_MENU.iter().map(|(_, l, k)| (*l, *k)).collect();
    assert_eq!(rows, [("Zoom Window", None), ("Fit", Some("F6"))]);
    assert_eq!(ZOOM_MENU.map(|r| r.0), [Icon::ZoomWindow, Icon::Fit], "each row has its icon");
  }

  #[test]
  fn increments_apply_only_with_incremental_move_and_fixed_steps() {
    let mut i = Increments::default();
    assert!(i.on && i.adaptive, "Fusion: Incremental Move on, Adaptive increments");
    assert_eq!(i.linear_step(0.25), 0.25, "adaptive increments follow the grid's step");
    assert_eq!(i.angular_step(0.1), 0.1);
    i.adaptive = false;
    assert_eq!(i.linear_step(0.25), 1.0);
    assert!((i.angular_step(0.1) - 15f64.to_radians()).abs() < 1e-12);
    i.on = false;
    assert_eq!(i.linear_step(0.25), 0.25);
    assert_eq!(i.angular_step(0.1), 0.1);
  }

  #[test]
  fn layout_grid_is_off_by_default_and_locks_the_grid_as_it_goes_on() {
    let mut app = App::with_session(fox_engine::Session::default());
    assert!(!app.layout_grid, "no grid in the 3D view until Layout Grid goes on");
    assert!(app.show_grid, "the sketch grid stays on");
    assert!(!app.snap);
    assert_eq!(app.grid.locked, None, "Layout Grid Lock greyed and off while the grid is off");
    assert!(app.grid.reference_numbers);
    set_layout_grid(&mut app, true, 5.0);
    assert!(app.layout_grid);
    assert_eq!(app.grid.locked, Some(5.0), "the lock comes up checked with the grid");
    set_layout_grid(&mut app, false, 7.0);
    assert!(!app.layout_grid);
    assert_eq!(app.grid.locked, None);
  }
}
