//! The navigation bar's fly-outs: Orbit and Zoom choices, Display Settings (visual style,
//! environment, graphics preset, effects, object visibility, camera, ground plane offset, full
//! screen) and Grid and Snaps, as in Fusion. The bar itself is [`crate::panels::nav_bar`].

use egui::RichText;
use fox_engine::render::{Dome, Effects, Scene, VisualStyle};

use crate::icons::Icon;
use crate::viewcube::Projection;
use crate::{App, NavMode, theme};

/// Display Settings ▸ Object Visibility: which kinds of things the viewport draws, Fusion's rows
/// (all on to begin with).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectVisibility {
  pub origin_planes: bool,
  /// Each origin plane's name in its square (see `origin::plane_names`).
  pub origin_plane_names: bool,
  pub origin_axes: bool,
  /// The origin point.
  pub origin_points: bool,
  pub construction_planes: bool,
  /// Each construction plane's name in its square (see `construction::plane_names`).
  pub construction_plane_names: bool,
  pub construction_axes: bool,
  pub construction_points: bool,
  pub sketches: bool,
  /// Kept, greyed: FissionOxide has no user coordinate systems.
  pub user_coordinate_systems: bool,
  /// The saved joint origins' glyphs.
  pub joint_origins: bool,
  /// The X, Y, Z stubs on the joint origins' glyphs.
  pub joint_origin_axes: bool,
  /// The joints' glyphs.
  pub joints: bool,
}

impl Default for ObjectVisibility {
  fn default() -> Self {
    ObjectVisibility {
      origin_planes: true,
      origin_plane_names: true,
      origin_axes: true,
      origin_points: true,
      construction_planes: true,
      construction_plane_names: true,
      construction_axes: true,
      construction_points: true,
      sketches: true,
      user_coordinate_systems: true,
      joint_origins: true,
      joint_origin_axes: true,
      joints: true,
    }
  }
}

impl ObjectVisibility {
  /// The rows under All Work Features, in Fusion's order: (label, flag, drawn here). A row whose
  /// things FissionOxide doesn't draw is greyed.
  pub fn rows(&mut self) -> [(&'static str, &mut bool, bool); 13] {
    [
      ("Origin Planes", &mut self.origin_planes, true),
      ("Origin Plane Names", &mut self.origin_plane_names, true),
      ("Origin Axes", &mut self.origin_axes, true),
      ("Origin Points", &mut self.origin_points, true),
      ("Construction Planes", &mut self.construction_planes, true),
      ("Construction Plane Names", &mut self.construction_plane_names, true),
      ("Construction Axes", &mut self.construction_axes, true),
      ("Construction Points", &mut self.construction_points, true),
      ("Sketches", &mut self.sketches, true),
      ("User Coordinate Systems", &mut self.user_coordinate_systems, false),
      ("Joint Origins", &mut self.joint_origins, true),
      ("Joint Origin Axes", &mut self.joint_origin_axes, true),
      ("Joints", &mut self.joints, true),
    ]
  }

  /// All Work Features: every row on.
  pub fn all(&mut self) -> bool {
    self.rows().iter().all(|(_, on, _)| **on)
  }

  /// All Work Features ticked or cleared: every row follows.
  pub fn set_all(&mut self, on: bool) {
    for (_, flag, _) in self.rows() {
      *flag = on;
    }
  }

  /// Turns the origin planes, axes and point on (the browser's Show All on the Origin folder).
  pub fn show_origin(&mut self) {
    self.origin_planes = true;
    self.origin_axes = true;
    self.origin_points = true;
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

/// A navigation bar button's tooltip (and a sketch dimension's parameter), as Fusion draws it:
/// the text in white on a dark panel, nothing more.
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

/// Display Settings ▸ Environment, as Fusion's Design workspace lists it: the theme's own
/// environment (the default), three theme environments and three legacy ones. The choice is kept
/// for the Render workspace and gives Environment Dome its colours ([`Environment::dome`]); the
/// modelling view's lighting is fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Environment {
  #[default]
  Theme,
  PhotoBooth,
  RiverRubicon,
  DarkSky,
  InfinityPool,
  GreyRoom,
  TranquilityBlue,
}

impl Environment {
  /// The environment's dome as Environment Dome draws it: its sky at the zenith and horizon and
  /// its ground, after the environment's image (a studio's grey walls, a river valley's sky over
  /// green banks, a night sky, a pool's blue).
  pub fn dome(self) -> Dome {
    let c = |r: u8, g: u8, b: u8| [r, g, b, 255];
    let (zenith, horizon, ground) = match self {
      Environment::Theme | Environment::GreyRoom => (c(196, 199, 204), c(232, 233, 235), c(160, 160, 162)),
      Environment::PhotoBooth => (c(214, 216, 220), c(246, 246, 247), c(188, 188, 190)),
      Environment::RiverRubicon => (c(118, 160, 214), c(214, 226, 236), c(104, 118, 92)),
      Environment::DarkSky => (c(22, 26, 40), c(70, 78, 98), c(30, 30, 34)),
      Environment::InfinityPool => (c(92, 150, 220), c(206, 226, 240), c(70, 140, 170)),
      Environment::TranquilityBlue => (c(128, 164, 206), c(220, 230, 240), c(150, 160, 172)),
    };
    Dome { zenith, horizon, ground }
  }

  /// The rows under the "Theme Environments" header.
  pub const THEME: [Environment; 3] = [Environment::PhotoBooth, Environment::RiverRubicon, Environment::DarkSky];
  /// The rows under the "Legacy Environments" header.
  pub const LEGACY: [Environment; 3] = [Environment::InfinityPool, Environment::GreyRoom, Environment::TranquilityBlue];

  pub fn label(self) -> &'static str {
    match self {
      Environment::Theme => "Theme (default)",
      Environment::PhotoBooth => "Photo Booth",
      Environment::RiverRubicon => "River Rubicon",
      Environment::DarkSky => "Dark Sky",
      Environment::InfinityPool => "Infinity Pool",
      Environment::GreyRoom => "Grey Room",
      Environment::TranquilityBlue => "Tranquility Blue",
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
    let all = |on: bool| Effects {
      environment_dome: on,
      ground_plane: on,
      ground_shadow: on,
      reflections: on,
      object_shadow: on,
      ambient_occlusion: on,
      anti_aliasing: on,
    };
    match self {
      GraphicsPreset::Performance => Some(all(false)),
      GraphicsPreset::Quality => Some(all(true)),
      GraphicsPreset::Custom => None,
    }
  }
}

/// The effects a new session starts with, Fusion's: Ground Plane, Ground Shadow, Ambient Occlusion
/// and Anti-Aliasing on, the rest off (so the preset reads Custom).
pub fn default_effects() -> Effects {
  Effects {
    environment_dome: false,
    ground_plane: true,
    ground_shadow: true,
    reflections: false,
    object_shadow: false,
    ambient_occlusion: true,
    anti_aliasing: true,
  }
}

/// Dresses a built scene as Display Settings ask: the visual style (Shaded with Visible Edges
/// Only and Shaded with Hidden Edges darken the plain edges), and the effects with the ground at Ground Plane Offset and the
/// environment's dome.
pub fn dress(app: &App, scene: &mut Scene) {
  scene.style = app.visual_style;
  scene.restyle_edges(fox_engine::scene::EDGE_COLOR);
  app.effects.apply_with(scene, app.display.ground(), app.display.environment.dome());
}

/// Display Settings ▸ Visual Style: Fusion's six rows in its order, with their shortcuts
/// (Ctrl+4 to Ctrl+9, which work from the canvas too).
pub const VISUAL_STYLES: [(VisualStyle, egui::Key, &str); 6] = [
  (VisualStyle::Shaded, egui::Key::Num4, "Ctrl+4"),
  (VisualStyle::ShadedHiddenEdges, egui::Key::Num5, "Ctrl+5"),
  (VisualStyle::ShadedVisibleEdges, egui::Key::Num6, "Ctrl+6"),
  (VisualStyle::Wireframe, egui::Key::Num7, "Ctrl+7"),
  (VisualStyle::WireframeHiddenEdges, egui::Key::Num8, "Ctrl+8"),
  (VisualStyle::WireframeVisibleEdges, egui::Key::Num9, "Ctrl+9"),
];

/// Ctrl+4 to Ctrl+9 from the canvas: picks that visual style. True when one was pressed.
pub fn visual_style_keys(app: &mut App, ctx: &egui::Context) -> bool {
  for (style, key, _) in VISUAL_STYLES {
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, key)) {
      app.visual_style = style;
      return true;
    }
  }
  false
}

/// Display Settings' own settings.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplaySettings {
  pub environment: Environment,
  pub preset: GraphicsPreset,
  /// Ground Plane Offset ▸ Adaptive: the ground sits at the model's lowest point (else
  /// `ground_offset` below it).
  pub ground_adaptive: bool,
  /// Ground Plane Offset: how far below the model's lowest point the ground sits (mm).
  pub ground_offset: f64,
  /// The GROUND PLANE OFFSET dialog, with the values being edited.
  pub ground_offset_form: Option<GroundOffsetForm>,
}

/// What the GROUND PLANE OFFSET dialog edits.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundOffsetForm {
  pub adaptive: bool,
  /// The Offset box's text, a typed distance or expression ("20", "20 mm", "1 in / 2"); its
  /// field shows only with Adaptive off.
  pub offset: String,
}

impl Default for DisplaySettings {
  fn default() -> Self {
    DisplaySettings {
      environment: Environment::default(),
      preset: GraphicsPreset::default(),
      ground_adaptive: true,
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

  /// How far below the model's lowest point the effects' ground sits: `None` with Adaptive (at
  /// it).
  pub fn ground(&self) -> Option<f32> {
    (!self.ground_adaptive).then_some(self.ground_offset as f32)
  }

  /// Opens the GROUND PLANE OFFSET dialog on the current values, the offset in the design's unit.
  pub fn open_ground_offset(&mut self, unit: fox_engine::model::LengthUnit) {
    let offset = unit.fixed(self.ground_offset, 2);
    self.ground_offset_form = Some(GroundOffsetForm { adaptive: self.ground_adaptive, offset });
  }
}

/// The Offset box's distance (mm), as the dialogs' value boxes read one: a bare number or unitless
/// expression is in the design's unit; parameters work. None when it does not evaluate to a
/// length of zero or more.
pub(crate) fn ground_offset_mm(s: &fox_engine::Session, text: &str) -> Option<f64> {
  use fox_engine::expr::{Empty, Kind, eval, eval_raw};
  let t = text.trim();
  if t.is_empty() {
    return None;
  }
  let bare = matches!(eval_raw(t, &Empty), Ok(v) if !v.has_unit);
  let e = if bare { format!("({t}) * 1 {}", s.design.units.symbol()) } else { t.to_string() };
  eval(&e, Kind::Length, &s.model.params).ok().filter(|v| v.is_finite() && *v >= 0.0)
}

/// Display Settings ▸ Enter Full Screen / Exit Full Screen (Ctrl+Shift+F).
pub fn toggle_full_screen(ctx: &egui::Context) {
  let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
  ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
}

/// The Visual Style fly-out's width: its longest row ("Shaded with Visible Edges Only") and the
/// shortcut column.
const VISUAL_STYLE_WIDTH: f32 = 250.0;
/// The Grid and Snaps menu's width.
const GRID_MENU_WIDTH: f32 = 190.0;

/// Fusion's Display Settings fly-out, in its order.
pub fn display_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(220.0);
  ui.menu_button("Visual Style", |ui| {
    // as wide as the longest row and the shortcut column, as Fusion's: the right-aligned
    // shortcuts would otherwise stretch the fly-out to the widest a menu gets
    ui.set_width(VISUAL_STYLE_WIDTH);
    for (s, _, keys) in VISUAL_STYLES {
      ui.horizontal(|ui| {
        if ui.radio(app.visual_style == s, s.label()).clicked() {
          app.visual_style = s;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          ui.label(RichText::new(keys).weak());
        });
      });
    }
  });
  ui.menu_button("Environment", |ui| {
    ui.set_min_width(180.0);
    let mut row = |ui: &mut egui::Ui, e: Environment| {
      if ui.radio(app.display.environment == e, e.label()).clicked() {
        app.display.environment = e;
      }
    };
    row(ui, Environment::Theme);
    for (header, list) in [("Theme Environments", Environment::THEME), ("Legacy Environments", Environment::LEGACY)] {
      ui.label(RichText::new(header).weak());
      for e in list {
        row(ui, e);
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
    ui.set_min_width(200.0);
    // the preset in force, greyed and ticked, as Fusion heads the list
    let preset = DisplaySettings::preset_for(app.effects);
    ui.add_enabled(false, egui::Checkbox::new(&mut true, format!("Graphics Preset: {}", preset.label())));
    ui.separator();
    let e: &mut Effects = &mut app.effects;
    for (label, flag) in [
      ("Environment Dome", &mut e.environment_dome),
      ("Ground Plane", &mut e.ground_plane),
      ("Ground Shadow", &mut e.ground_shadow),
      ("Ground Reflection", &mut e.reflections),
      ("Object Shadow", &mut e.object_shadow),
      ("Ambient Occlusion", &mut e.ambient_occlusion),
      ("Anti-Aliasing", &mut e.anti_aliasing),
    ] {
      ui.checkbox(flag, label);
    }
  });
  ui.menu_button("Object Visibility", |ui| {
    ui.set_min_width(200.0);
    let v = &mut app.objects;
    let mut all = v.all();
    if ui.checkbox(&mut all, "All Work Features").changed() {
      v.set_all(all);
    }
    ui.separator();
    for (label, flag, drawn) in v.rows() {
      ui.add_enabled(drawn, egui::Checkbox::new(flag, label))
        .on_disabled_hover_text("FissionOxide has no user coordinate systems yet");
    }
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
    app.display.open_ground_offset(app.s.design.units);
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

/// The GROUND PLANE OFFSET dialog, while open: docked like a command dialog, with Adaptive
/// (and the offset field when Adaptive is off) and OK / Cancel. Escape closes it (`App::escape`).
pub fn ground_offset_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut form) = app.display.ground_offset_form.take() else { return };
  let mut done = false;
  let mut cancel = false;
  let width = 300.0;
  let dock = crate::panels::dialog_dock(app.canvas, app.s.edit_sketch.is_some());
  // what OK applies: the typed offset (OK greys while it does not evaluate), or with Adaptive on
  // the offset kept for later
  let mut offset = None;
  egui::Area::new(egui::Id::new("ground-plane-offset"))
    .pivot(egui::Align2::RIGHT_TOP)
    .default_pos(dock)
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      crate::panels::dialog_frame().show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        crate::panels::dialog_header(ui, crate::icons::Icon::Display, "GROUND PLANE OFFSET", width);
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
          egui::Grid::new("ground-offset").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
            ui.label("Adaptive");
            ui.checkbox(&mut form.adaptive, "");
            ui.end_row();
            if !form.adaptive {
              // a typed value box, as the command dialogs' distance rows (units, expressions)
              ui.label("Offset");
              crate::dialogs::value_edit_w(ui, &mut form.offset, 140.0);
              ui.end_row();
            }
          });
        });
        offset = if form.adaptive {
          Some(ground_offset_mm(&app.s, &form.offset).unwrap_or(app.display.ground_offset))
        } else {
          ground_offset_mm(&app.s, &form.offset)
        };
        let sep = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover()).0;
        ui.painter().rect_filled(sep, 0.0, crate::theme::SEPARATOR);
        egui::Frame::new().fill(crate::theme::DIALOG_BODY).inner_margin(egui::Margin::symmetric(10, 7)).show(
          ui,
          |ui| {
            ui.horizontal(|ui| {
              ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
              ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::panels::secondary_button(ui, "Cancel").clicked() {
                  cancel = true;
                }
                let accent = crate::icons::accent_of(crate::icons::Icon::Display);
                if crate::dialogs::ok_button(ui, accent, offset.is_some()).clicked() {
                  done = true;
                }
              });
            });
          },
        );
      });
    });
  if done && let Some(mm) = offset {
    app.display.ground_adaptive = form.adaptive;
    app.display.ground_offset = mm;
    app.display.ground_offset_form = None;
  } else if cancel {
    app.display.ground_offset_form = None;
  } else {
    app.display.ground_offset_form = Some(form);
  }
}

/// Grid and Snaps, as Fusion lists it: Layout Grid (the 3D view's ground grid, off by default),
/// Layout Grid Lock (greyed while the grid is off, and checked when the grid goes on), Snap to
/// Grid, Grid Settings, then Incremental Move and Set Increments. The two command rows carry icons
/// and open the docked GRID SETTINGS and SET INCREMENTS dialogs (see [`crate::grid_dialogs`]).
/// `adaptive_now` is the spacing the adaptive grid has (what a lock freezes).
pub fn grid_menu(app: &mut App, ui: &mut egui::Ui, adaptive_now: f64) {
  // its rows' width, as Fusion's (the icon rows fill what they're given)
  ui.set_width(GRID_MENU_WIDTH);
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
    // Fusion's defaults: plane, shadow, ambient occlusion and anti-aliasing on, so Custom
    let mut e = default_effects();
    assert!(e.ground_plane && e.ground_shadow && e.ambient_occlusion && e.anti_aliasing);
    assert!(!e.environment_dome && !e.reflections && !e.object_shadow);
    assert_eq!(DisplaySettings::preset_for(e), GraphicsPreset::Custom);
    assert_eq!(App::with_session(fox_engine::Session::default()).effects, e, "a new session starts with them");
    d.set_preset(GraphicsPreset::Performance, &mut e);
    assert_eq!(e, Effects::default());
    assert_eq!(DisplaySettings::preset_for(e), GraphicsPreset::Performance);
    d.set_preset(GraphicsPreset::Quality, &mut e);
    assert!(e.ground_shadow && e.reflections && e.ground_plane && e.ambient_occlusion && e.environment_dome);
    assert_eq!(DisplaySettings::preset_for(e), GraphicsPreset::Quality);
    e.reflections = false;
    assert_eq!(DisplaySettings::preset_for(e), GraphicsPreset::Custom);
    d.set_preset(GraphicsPreset::Custom, &mut e);
    assert!(!e.reflections, "Custom leaves the effects alone");
  }

  #[test]
  fn the_environment_list_is_fusions() {
    assert_eq!(Environment::default(), Environment::Theme);
    assert_eq!(Environment::Theme.label(), "Theme (default)");
    assert_eq!(Environment::THEME.map(|e| e.label()), ["Photo Booth", "River Rubicon", "Dark Sky"]);
    assert_eq!(Environment::LEGACY.map(|e| e.label()), ["Infinity Pool", "Grey Room", "Tranquility Blue"]);
  }

  #[test]
  fn visual_styles_are_fusions_six_with_ctrl_digit_keys() {
    let rows: Vec<(&str, &str)> = VISUAL_STYLES.iter().map(|(s, _, k)| (s.label(), *k)).collect();
    assert_eq!(
      rows,
      [
        ("Shaded", "Ctrl+4"),
        ("Shaded with Hidden Edges", "Ctrl+5"),
        ("Shaded with Visible Edges Only", "Ctrl+6"),
        ("Wireframe", "Ctrl+7"),
        ("Wireframe with Hidden Edges", "Ctrl+8"),
        ("Wireframe with Visible Edges Only", "Ctrl+9"),
      ]
    );
    let mut app = App::with_session(fox_engine::Session::default());
    assert_eq!(app.visual_style, VisualStyle::Shaded, "Shaded is the default");
    assert!(app.visual_style.edges(), "and draws the edges, as Fusion's does");
    // Ctrl+7, Ctrl+5 and Ctrl+4 pressed over the canvas
    let ctx = egui::Context::default();
    for (key, want) in [
      (egui::Key::Num7, VisualStyle::Wireframe),
      (egui::Key::Num5, VisualStyle::ShadedHiddenEdges),
      (egui::Key::Num4, VisualStyle::Shaded),
    ] {
      let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))),
        events: vec![egui::Event::Key {
          key,
          physical_key: None,
          pressed: true,
          repeat: false,
          modifiers: egui::Modifiers::COMMAND,
        }],
        ..Default::default()
      };
      ctx.run_ui(input, |ui| app.draw(ui)).drop_without_applying_deltas();
      assert_eq!(app.visual_style, want, "{key:?}");
    }
  }

  #[test]
  fn object_visibility_has_fusions_rows_and_all_work_features() {
    let mut v = ObjectVisibility::default();
    let labels: Vec<&str> = v.rows().iter().map(|(l, ..)| *l).collect();
    assert_eq!(
      labels,
      [
        "Origin Planes",
        "Origin Plane Names",
        "Origin Axes",
        "Origin Points",
        "Construction Planes",
        "Construction Plane Names",
        "Construction Axes",
        "Construction Points",
        "Sketches",
        "User Coordinate Systems",
        "Joint Origins",
        "Joint Origin Axes",
        "Joints",
      ]
    );
    assert!(v.all(), "all on");
    v.sketches = false;
    assert!(!v.all());
    v.set_all(true);
    assert!(v.sketches && v.all());
    v.set_all(false);
    assert!(v.rows().iter().all(|(_, on, _)| !**on));
    // the rows reach the scene
    let mut app = App::with_session(fox_engine::Session::default());
    app.objects.construction_axes = false;
    let o = crate::viewport::scene_options(&mut app);
    assert!(o.hide_construction_axes && !o.hide_construction_planes && !o.hide_construction_points);
  }

  /// The texts drawn in a frame, with where.
  fn drawn(out: &egui::FullOutput) -> Vec<(egui::Rect, String)> {
    fn walk(s: &egui::Shape, out: &mut Vec<(egui::Rect, String)>) {
      match s {
        egui::Shape::Text(t) => out.push((t.visual_bounding_rect(), t.galley.text().to_string())),
        egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        _ => {}
      }
    }
    let mut all = Vec::new();
    for c in &out.shapes {
      walk(&c.shape, &mut all);
    }
    all
  }

  /// The Grid and Snaps menu and the Visual Style fly-out are as wide as their rows (Fusion's
  /// are about 200-260 px), and the fly-out opens to the right of its row, from a navigation
  /// bar button at the bottom of the window.
  #[test]
  fn the_grid_menu_and_the_visual_style_fly_out_are_narrow_and_open_right() {
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut app = App::with_session(fox_engine::Session::default());
    let time = std::cell::Cell::new(0.0);
    let frame = |app: &mut App, grid: bool, events: Vec<egui::Event>| {
      time.set(time.get() + 1.0 / 60.0);
      let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0))),
        time: Some(time.get()),
        events,
        ..Default::default()
      };
      let mut button = None;
      let out = ctx.run_ui(input, |ui| {
        egui::Area::new(egui::Id::new("bar")).fixed_pos(egui::pos2(700.0, 860.0)).show(ui.ctx(), |ui| {
          let b = ui.button(if grid { "Grid" } else { "Display" });
          if grid {
            egui::Popup::menu(&b).show(|ui| grid_menu(app, ui, 10.0));
          } else {
            egui::Popup::menu(&b).show(|ui| display_menu(app, ui));
          }
          button = Some((b.rect, egui::Popup::default_response_id(&b)));
        });
      });
      let t = drawn(&out);
      out.drop_without_applying_deltas();
      (t, button.unwrap())
    };
    let button = |p: egui::Pos2, pressed| egui::Event::PointerButton {
      pos: p,
      button: egui::PointerButton::Primary,
      pressed,
      modifiers: Default::default(),
    };
    let click = |app: &mut App, grid: bool, p: egui::Pos2| {
      frame(app, grid, vec![egui::Event::PointerMoved(p)]);
      frame(app, grid, vec![button(p, true)]);
      frame(app, grid, vec![button(p, false)])
    };
    // Grid and Snaps
    frame(&mut app, true, vec![]);
    let (_, (b, id)) = frame(&mut app, true, vec![]);
    click(&mut app, true, b.center());
    for _ in 0..4 {
      frame(&mut app, true, vec![]);
    }
    let r = ctx.memory(|m| m.area_rect(id)).expect("the grid menu is open");
    assert!(r.width() < 240.0, "the grid menu is {} px wide", r.width());
    // Display Settings ▸ Visual Style
    egui::Popup::close_all(&ctx);
    let (_, (b, _)) = frame(&mut app, false, vec![egui::Event::PointerMoved(egui::pos2(10.0, 10.0))]);
    click(&mut app, false, b.center());
    frame(&mut app, false, vec![]);
    let (t, _) = frame(&mut app, false, vec![]);
    let row = t.iter().find(|(_, s)| s == "Visual Style").expect("the Display Settings menu is open").0;
    frame(&mut app, false, vec![egui::Event::PointerMoved(row.center())]);
    let mut t = Vec::new();
    for _ in 0..30 {
      time.set(time.get() + 0.05);
      t = frame(&mut app, false, vec![]).0;
    }
    let at = |s: &str| t.iter().find(|(_, x)| x == s).unwrap_or_else(|| panic!("{s} not drawn: {t:?}")).0;
    let (first, longest, key) = (at("Shaded"), at("Shaded with Visible Edges Only"), at("Ctrl+9"));
    assert!(first.left() > row.right(), "the fly-out opens right: {first:?} beside {row:?}");
    assert!(key.right() - first.left() < 260.0, "the fly-out is narrow: {first:?} .. {key:?}");
    assert!(key.left() - longest.right() < 50.0, "no gap before the shortcuts: {longest:?} {key:?}");
    assert!(key.left() > longest.right(), "the shortcuts sit after the rows: {longest:?} {key:?}");
  }

  #[test]
  fn the_ground_plane_offset_dialog_is_docked_with_adaptive() {
    let mut app = App::with_session(fox_engine::Session::default());
    app.canvas = egui::Rect::from_min_max(egui::pos2(0.0, 80.0), egui::pos2(1200.0, 760.0));
    app.display.open_ground_offset(app.s.design.units);
    assert_eq!(app.display.ground_offset_form, Some(GroundOffsetForm { adaptive: true, offset: "0.00 mm".into() }));
    let ctx = egui::Context::default();
    let frame = |app: &mut App, events: Vec<egui::Event>| {
      let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))),
        events,
        ..Default::default()
      };
      ctx.run_ui(input, |ui| ground_offset_window(app, ui.ctx())).drop_without_applying_deltas();
    };
    frame(&mut app, vec![]);
    // docked at the canvas's top right, as a command dialog, and only as tall as its rows
    let r = ctx.memory(|m| m.area_rect(egui::Id::new("ground-plane-offset"))).expect("drawn");
    let dock = crate::panels::dialog_dock(app.canvas, false);
    assert!((r.right() - dock.x).abs() < 1.0 && (r.top() - dock.y).abs() < 1.0, "{r:?} {dock:?}");
    assert!(r.width() <= 302.0 && r.height() < 130.0, "{r:?}");
    // Adaptive off keeps its value through the dialog; OK is needed to apply it
    app.display.ground_offset_form = Some(GroundOffsetForm { adaptive: false, offset: "12.5".into() });
    frame(&mut app, vec![]);
    assert_eq!(app.display.ground_offset_form, Some(GroundOffsetForm { adaptive: false, offset: "12.5".into() }));
    assert!(app.display.ground_adaptive && app.display.ground_offset == 0.0, "open until OK");
    // Escape closes it, changing nothing
    app.escape();
    assert_eq!(app.display.ground_offset_form, None);
    assert!(app.display.ground_adaptive);
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

  /// Display Settings reach the scene: Shaded with Visible Edges Only darkens the plain edges,
  /// the Effects go to the picture (Environment Dome in the environment's colours), and with
  /// Adaptive off the ground sits Ground Plane Offset below the model.
  #[test]
  fn display_settings_dress_the_scene() {
    let mut app = App::with_session(fox_engine::Session::default());
    app.run("box", serde_json::json!({"width": 20, "depth": 20, "height": 20})).unwrap();
    let opts = crate::viewport::scene_options(&mut app);
    let (built, _) = app.scene.build(&app.s, &opts);
    let edge = fox_engine::scene::EDGE_COLOR;
    let plain_edges = |s: &Scene| s.lines.iter().filter(|l| fox_engine::render::is_edge(l) && l.color == edge).count();
    assert!(plain_edges(&built) > 0);
    let mut s = built.clone();
    app.visual_style = VisualStyle::ShadedVisibleEdges;
    dress(&app, &mut s);
    assert_eq!(plain_edges(&s), 0, "dark edges");
    // Fusion's default effects: ambient occlusion and anti-aliasing are drawn
    assert!(s.post.ambient_occlusion && s.post.anti_aliasing && s.post.dome.is_none());
    app.effects.environment_dome = true;
    app.display.environment = Environment::DarkSky;
    let mut s = built.clone();
    dress(&app, &mut s);
    assert_eq!(s.post.dome, Some(Environment::DarkSky.dome()));
    let ground_top = |s: &Scene| {
      s.meshes
        .iter()
        .filter(|m| !m.analyze && m.id == 0)
        .flat_map(|m| m.positions.iter().map(|p| p[2]))
        .fold(f32::NEG_INFINITY, f32::max)
    };
    assert!(ground_top(&s).abs() < 0.5, "adaptive: at the model's foot ({})", ground_top(&s));
    app.display.ground_adaptive = false;
    app.display.ground_offset = 15.0;
    let mut s = built;
    dress(&app, &mut s);
    assert!((ground_top(&s) + 15.0).abs() < 0.5, "15 mm below ({})", ground_top(&s));
  }

  /// Every Effects row can be ticked now that the viewport draws them all.
  #[test]
  fn every_effect_is_drawn() {
    let mut e = default_effects();
    assert!(e.ambient_occlusion && e.anti_aliasing && !e.object_shadow && !e.environment_dome);
    let mut s = Scene::default();
    e.object_shadow = true;
    e.apply_with(&mut s, None, Environment::Theme.dome());
    assert!(s.post.object_shadow);
  }
}
