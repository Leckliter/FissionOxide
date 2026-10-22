//! The Preferences dialog, as in Fusion (profile menu or File → Preferences): a centred modal
//! with Fusion's tree of sections on the left, the page on the right and Restore Defaults,
//! Apply, OK and Cancel at the bottom. Changes take effect on OK or Apply. The values live in
//! the `[Preferences]` section of `settings.ini` (see [`fox_desktop::settings`]).

use std::path::PathBuf;

use egui::{Color32, PointerButton, RichText, Sense, Stroke, Vec2, vec2};

use crate::viewcube::CubeSize;
use crate::{App, theme};

/// Default units for new designs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Units {
  #[default]
  Mm,
  Cm,
  M,
  In,
  Ft,
}

impl Units {
  pub const ALL: [Units; 5] = [Units::Mm, Units::Cm, Units::M, Units::In, Units::Ft];

  pub fn key(self) -> &'static str {
    match self {
      Units::Mm => "mm",
      Units::Cm => "cm",
      Units::M => "m",
      Units::In => "in",
      Units::Ft => "ft",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Units::Mm => "Millimeter",
      Units::Cm => "Centimeter",
      Units::M => "Meter",
      Units::In => "Inch",
      Units::Ft => "Foot",
    }
  }
}

/// Default modeling orientation: which world axis points up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpAxis {
  /// Fusion's default.
  #[default]
  Y,
  Z,
}

impl UpAxis {
  pub const ALL: [UpAxis; 2] = [UpAxis::Y, UpAxis::Z];

  pub fn key(self) -> &'static str {
    match self {
      UpAxis::Y => "y",
      UpAxis::Z => "z",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      UpAxis::Y => "Y up",
      UpAxis::Z => "Z up",
    }
  }
}

/// What a mouse drag does to the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
  Orbit,
  Pan,
  Zoom,
}

/// Which mouse buttons orbit, pan and zoom, after the CAD programs Fusion offers as presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NavPreset {
  #[default]
  Fusion,
  SolidWorks,
  Inventor,
  Alias,
  PowerMill,
  Tinkercad,
}

/// Modifier keys a mapping needs held: (shift, ctrl, alt).
type Held = (bool, bool, bool);

impl NavPreset {
  /// In the order Fusion's Mouse setup list shows them.
  pub const ALL: [NavPreset; 6] = [
    NavPreset::Fusion,
    NavPreset::Alias,
    NavPreset::Inventor,
    NavPreset::PowerMill,
    NavPreset::SolidWorks,
    NavPreset::Tinkercad,
  ];

  pub fn key(self) -> &'static str {
    match self {
      NavPreset::Fusion => "fusion",
      NavPreset::SolidWorks => "solidworks",
      NavPreset::Inventor => "inventor",
      NavPreset::Alias => "alias",
      NavPreset::PowerMill => "powermill",
      NavPreset::Tinkercad => "tinkercad",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      NavPreset::Fusion => "Fusion",
      NavPreset::SolidWorks => "SOLIDWORKS",
      NavPreset::Inventor => "Inventor",
      NavPreset::Alias => "Alias",
      NavPreset::PowerMill => "PowerMill",
      NavPreset::Tinkercad => "Tinkercad",
    }
  }

  /// The drags this preset maps, as (button, modifiers held, action).
  fn mappings(self) -> &'static [(PointerButton, Held, NavAction)] {
    use NavAction::*;
    use PointerButton::*;
    const NONE: Held = (false, false, false);
    const SHIFT: Held = (true, false, false);
    const CTRL: Held = (false, true, false);
    const SHIFT_ALT: Held = (true, false, true);
    match self {
      // Fusion's: middle pans, Shift+middle orbits; FissionOxide also orbits with a right drag
      // (a right click without dragging opens the marking menu)
      NavPreset::Fusion => {
        &[(Middle, NONE, Pan), (Middle, SHIFT, Orbit), (Secondary, NONE, Orbit), (Secondary, SHIFT, Pan)]
      }
      NavPreset::SolidWorks => &[(Middle, NONE, Orbit), (Middle, CTRL, Pan), (Middle, SHIFT, Zoom)],
      NavPreset::Inventor => &[(Middle, NONE, Pan), (Middle, SHIFT, Orbit)],
      NavPreset::Alias => &[(Primary, SHIFT_ALT, Orbit), (Middle, SHIFT_ALT, Pan), (Secondary, SHIFT_ALT, Zoom)],
      // PowerMill's: middle turns the view, Shift+middle pans, Ctrl+middle zooms
      NavPreset::PowerMill => &[(Middle, NONE, Orbit), (Middle, SHIFT, Pan), (Middle, CTRL, Zoom)],
      // Tinkercad's: right turns the view, middle or Shift+right pans
      NavPreset::Tinkercad => &[(Secondary, NONE, Orbit), (Middle, NONE, Pan), (Secondary, SHIFT, Pan)],
    }
  }

  /// What a drag started with `button` and `mods` held does, if anything.
  pub fn action(self, button: PointerButton, mods: egui::Modifiers) -> Option<NavAction> {
    let held = (mods.shift, mods.ctrl || mods.command, mods.alt);
    self.mappings().iter().find(|(b, h, _)| *b == button && *h == held).map(|m| m.2)
  }
}

/// General → Theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
  #[default]
  Light,
  Dark,
  /// The operating system's light or dark setting.
  MatchDevice,
}

impl Theme {
  pub const ALL: [Theme; 3] = [Theme::Light, Theme::Dark, Theme::MatchDevice];

  pub fn key(self) -> &'static str {
    match self {
      Theme::Light => "light",
      Theme::Dark => "dark",
      Theme::MatchDevice => "system",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Theme::Light => "Light",
      Theme::Dark => "Dark",
      Theme::MatchDevice => "Match device",
    }
  }
}

/// General → Default Orbit type: what the Orbit command does until the navigation bar's fly-out
/// picks the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrbitType {
  #[default]
  Constrained,
  Free,
}

impl OrbitType {
  pub const ALL: [OrbitType; 2] = [OrbitType::Constrained, OrbitType::Free];

  pub fn key(self) -> &'static str {
    match self {
      OrbitType::Constrained => "constrained",
      OrbitType::Free => "free",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      OrbitType::Constrained => "Constrained Orbit",
      OrbitType::Free => "Free Orbit",
    }
  }
}

/// General → Automatic recovery backup interval, in minutes (0 never backs up).
pub const RECOVERY_CHOICES: [u32; 7] = [0, 1, 2, 5, 10, 15, 30];

pub fn recovery_label(minutes: u32) -> String {
  match minutes {
    0 => "Never".into(),
    1 => "1 minute".into(),
    m => format!("{m} minutes"),
  }
}

/// The saved preferences.
#[derive(Debug, Clone, PartialEq)]
pub struct Prefs {
  pub units: Units,
  /// Stored, not yet applied: which axis is up needs a decision on the camera and the ViewCube.
  pub up_axis: UpAxis,
  pub nav: NavPreset,
  /// Zoom the opposite way to Fusion's default: wheel back (toward you) zooms out, and a zoom
  /// drag down zooms out. Off, as in Fusion, wheel back zooms in.
  ///
  /// Saved as `ZoomReversed`. Before FissionOxide matched Fusion's default the setting was
  /// `ReverseZoom`, with the opposite meaning: `ReverseZoom=1` gave Fusion's direction and
  /// `ReverseZoom=0` (the old default, written for everyone who saved any preference) the other.
  /// A legacy `ReverseZoom=1` reads as `false` here, so it keeps the direction it chose; a legacy
  /// `0` was the untouched default and takes the new default.
  pub reverse_zoom: bool,
  /// Zoom toward the cursor rather than the middle of the view.
  pub zoom_to_cursor: bool,
  /// New designs capture design history (a timeline).
  pub capture_history: bool,
  /// Creating or editing a sketch turns the camera to look at its plane.
  pub auto_look_at: bool,
  pub show_dims: bool,
  pub show_constraints: bool,
  pub snap: bool,
  /// Draw the viewport on the GPU.
  pub gpu: bool,
  /// General → "Spacebar repeats last command", on by default as in Fusion.
  pub space_repeats: bool,
  /// Graphics → the ViewCube is shown, and its size (the ViewCube menu's ViewCube Size).
  pub show_viewcube: bool,
  pub cube_size: CubeSize,
  pub theme: Theme,
  /// General → "Show tooltips": the toolbar's and the controls' hover texts.
  pub show_tooltips: bool,
  /// General → "Show command prompt": the tool's or command's hint at the bottom left of the
  /// canvas.
  pub show_prompt: bool,
  pub orbit: OrbitType,
  /// General → "Enable camera pivot": a pivot marker at the orbit centre while orbiting.
  pub camera_pivot: bool,
  /// General → "Automatic recovery backup interval", minutes between backups of the unsaved
  /// design; 0 never backs up.
  pub recovery_minutes: u32,
  /// Sketch → the solver's "n degrees of freedom · m profiles" line at the foot of the Sketch
  /// Palette; off, the palette ends with its rows as Fusion's does.
  pub show_dof: bool,
  /// The Bill of Materials palette's Column Settings: the columns unticked (by name, Fusion's
  /// Material (Legacy) to begin with) and the order the columns were dragged into (empty =
  /// Fusion's order). See [`crate::bom_ui`].
  pub bom_hidden: Vec<String>,
  pub bom_order: Vec<String>,
}

impl Default for Prefs {
  fn default() -> Self {
    Prefs {
      units: Units::Mm,
      up_axis: UpAxis::Y,
      nav: NavPreset::Fusion,
      reverse_zoom: false,
      zoom_to_cursor: true,
      capture_history: true,
      auto_look_at: true,
      show_dims: true,
      show_constraints: true,
      snap: false,
      gpu: true,
      space_repeats: true,
      show_viewcube: true,
      cube_size: CubeSize::Automatic,
      theme: Theme::Light,
      show_tooltips: true,
      show_prompt: true,
      orbit: OrbitType::Constrained,
      camera_pivot: true,
      recovery_minutes: 5,
      show_dof: false,
      bom_hidden: crate::bom_ui::DEFAULT_HIDDEN.iter().map(|c| c.to_string()).collect(),
      bom_order: Vec::new(),
    }
  }
}

pub const SECTION: &str = "Preferences";

fn flag(v: bool) -> String {
  (if v { "1" } else { "0" }).into()
}

fn parse_flag(v: &str) -> Option<bool> {
  match v.to_ascii_lowercase().as_str() {
    "1" | "true" | "yes" | "on" => Some(true),
    "0" | "false" | "no" | "off" => Some(false),
    _ => None,
  }
}

fn pick<T: Copy>(all: &[T], key: impl Fn(T) -> &'static str, v: &str) -> Option<T> {
  all.iter().copied().find(|x| key(*x).eq_ignore_ascii_case(v))
}

impl Prefs {
  /// The `[Preferences]` keys, in file order.
  pub fn to_pairs(&self) -> Vec<(String, String)> {
    [
      ("Units", self.units.key().to_string()),
      ("UpAxis", self.up_axis.key().to_string()),
      ("Navigation", self.nav.key().to_string()),
      ("ZoomReversed", flag(self.reverse_zoom)),
      ("ZoomToCursor", flag(self.zoom_to_cursor)),
      ("CaptureHistory", flag(self.capture_history)),
      ("SketchLookAt", flag(self.auto_look_at)),
      ("SketchDimensions", flag(self.show_dims)),
      ("SketchConstraints", flag(self.show_constraints)),
      ("SketchSnap", flag(self.snap)),
      ("GpuRenderer", flag(self.gpu)),
      ("SpaceRepeats", flag(self.space_repeats)),
      ("ShowViewCube", flag(self.show_viewcube)),
      ("ViewCubeSize", self.cube_size.key().to_string()),
      ("Theme", self.theme.key().to_string()),
      ("ShowTooltips", flag(self.show_tooltips)),
      ("ShowCommandPrompt", flag(self.show_prompt)),
      ("OrbitType", self.orbit.key().to_string()),
      ("CameraPivot", flag(self.camera_pivot)),
      ("RecoveryMinutes", self.recovery_minutes.to_string()),
      ("SketchDegreesOfFreedom", flag(self.show_dof)),
      ("BomHiddenColumns", self.bom_hidden.join("|")),
      ("BomColumnOrder", self.bom_order.join("|")),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
  }

  /// Reads the pairs of a `[Preferences]` section; missing or unknown values keep the defaults.
  pub fn from_pairs(pairs: &[(String, String)]) -> Prefs {
    let mut p = Prefs::default();
    // the current key wins over the legacy one wherever they appear
    let has_new = pairs.iter().any(|(k, _)| k.eq_ignore_ascii_case("zoomreversed"));
    for (k, v) in pairs {
      let b = parse_flag(v);
      match k.to_ascii_lowercase().as_str() {
        "units" => p.units = pick(&Units::ALL, Units::key, v).unwrap_or(p.units),
        "upaxis" => p.up_axis = pick(&UpAxis::ALL, UpAxis::key, v).unwrap_or(p.up_axis),
        "navigation" => p.nav = pick(&NavPreset::ALL, NavPreset::key, v).unwrap_or(p.nav),
        "zoomreversed" => p.reverse_zoom = b.unwrap_or(p.reverse_zoom),
        // legacy: both its values meant wheel back zooms in (=1, chosen) or the old default (=0)
        "reversezoom" if !has_new => p.reverse_zoom = false,
        "zoomtocursor" => p.zoom_to_cursor = b.unwrap_or(p.zoom_to_cursor),
        "capturehistory" => p.capture_history = b.unwrap_or(p.capture_history),
        "sketchlookat" => p.auto_look_at = b.unwrap_or(p.auto_look_at),
        "sketchdimensions" => p.show_dims = b.unwrap_or(p.show_dims),
        "sketchconstraints" => p.show_constraints = b.unwrap_or(p.show_constraints),
        "sketchsnap" => p.snap = b.unwrap_or(p.snap),
        "gpurenderer" => p.gpu = b.unwrap_or(p.gpu),
        "spacerepeats" => p.space_repeats = b.unwrap_or(p.space_repeats),
        "showviewcube" => p.show_viewcube = b.unwrap_or(p.show_viewcube),
        "viewcubesize" => p.cube_size = pick(&CubeSize::ALL, CubeSize::key, v).unwrap_or(p.cube_size),
        "theme" => p.theme = pick(&Theme::ALL, Theme::key, v).unwrap_or(p.theme),
        "showtooltips" => p.show_tooltips = b.unwrap_or(p.show_tooltips),
        "showcommandprompt" => p.show_prompt = b.unwrap_or(p.show_prompt),
        "orbittype" => p.orbit = pick(&OrbitType::ALL, OrbitType::key, v).unwrap_or(p.orbit),
        "camerapivot" => p.camera_pivot = b.unwrap_or(p.camera_pivot),
        "recoveryminutes" => p.recovery_minutes = v.trim().parse().unwrap_or(p.recovery_minutes),
        "sketchdegreesoffreedom" => p.show_dof = b.unwrap_or(p.show_dof),
        "bomhiddencolumns" => p.bom_hidden = names(v),
        "bomcolumnorder" => p.bom_order = names(v),
        _ => {}
      }
    }
    p
  }

  pub fn parse(ini: &str) -> Prefs {
    Prefs::from_pairs(&fox_desktop::settings::section(ini, SECTION))
  }

  /// The ini text with this `[Preferences]` section, the rest kept.
  #[cfg(test)]
  pub fn write_into(&self, ini: &str) -> String {
    fox_desktop::settings::with_section(ini, SECTION, &self.to_pairs())
  }
}

/// A section of Fusion's tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
  #[default]
  General,
  Api,
  Design,
  Manufacture,
  Electronics,
  Render,
  Drawing,
  Simulation,
  GenerativeDesign,
  Tokens,
  Material,
  Graphics,
  Network,
  DataCollection,
  UnitDisplay,
  DefaultUnits,
  DefaultUnitsDesign,
  DefaultUnitsManufacture,
  DefaultUnitsElectronics,
  DefaultUnitsSimulation,
  PreviewFeatures,
  Compatibility,
}

/// Fusion's tree: each top-level section with its children.
const TREE: &[(Page, &[Page])] = &[
  (
    Page::General,
    &[
      Page::Api,
      Page::Design,
      Page::Manufacture,
      Page::Electronics,
      Page::Render,
      Page::Drawing,
      Page::Simulation,
      Page::GenerativeDesign,
    ],
  ),
  (Page::Tokens, &[]),
  (Page::Material, &[]),
  (Page::Graphics, &[]),
  (Page::Network, &[]),
  (Page::DataCollection, &[]),
  (Page::UnitDisplay, &[]),
  (
    Page::DefaultUnits,
    &[
      Page::DefaultUnitsDesign,
      Page::DefaultUnitsManufacture,
      Page::DefaultUnitsElectronics,
      Page::DefaultUnitsSimulation,
    ],
  ),
  (Page::PreviewFeatures, &[]),
  (Page::Compatibility, &[]),
];

impl Page {
  pub fn label(self) -> &'static str {
    match self {
      Page::General => "General",
      Page::Api => "API",
      Page::Design | Page::DefaultUnitsDesign => "Design",
      Page::Manufacture | Page::DefaultUnitsManufacture => "Manufacture",
      Page::Electronics | Page::DefaultUnitsElectronics => "Electronics",
      Page::Render => "Render",
      Page::Drawing => "Drawing",
      Page::Simulation | Page::DefaultUnitsSimulation => "Simulation",
      Page::GenerativeDesign => "Generative Design",
      Page::Tokens => "Tokens",
      Page::Material => "Material",
      Page::Graphics => "Graphics",
      Page::Network => "Network",
      Page::DataCollection => "Data Collection and Use",
      Page::UnitDisplay => "Unit and Value Display",
      Page::DefaultUnits => "Default Units",
      Page::PreviewFeatures => "Preview Features",
      Page::Compatibility => "Compatibility & Troubleshooting",
    }
  }

  /// The page's title bar, as Fusion words it.
  pub fn title(self) -> &'static str {
    match self {
      Page::General => "Preferences controlling general UI behavior",
      Page::Api => "Preferences controlling the API",
      Page::Design => "Preferences controlling design behavior",
      Page::Manufacture => "Preferences controlling manufacture behavior",
      Page::Electronics => "Preferences controlling electronics behavior",
      Page::Render => "Preferences controlling rendering",
      Page::Drawing => "Preferences controlling drawings",
      Page::Simulation => "Preferences controlling simulation",
      Page::GenerativeDesign => "Preferences controlling generative design",
      Page::Tokens => "Preferences controlling tokens",
      Page::Material => "Preferences controlling materials",
      Page::Graphics => "Preferences controlling graphics",
      Page::Network => "Preferences controlling network settings",
      Page::DataCollection => "Preferences controlling data collection and use",
      Page::UnitDisplay => "Preferences controlling unit and value display",
      Page::DefaultUnits => "Preferences controlling default units",
      Page::DefaultUnitsDesign => "Default units for new designs",
      Page::DefaultUnitsManufacture => "Default units for manufacture",
      Page::DefaultUnitsElectronics => "Default units for electronics",
      Page::DefaultUnitsSimulation => "Default units for simulation",
      Page::PreviewFeatures => "Preview features",
      Page::Compatibility => "Compatibility and troubleshooting",
    }
  }
}

/// The recovery backups: when the interval started counting and the revision last backed up.
#[derive(Debug, Default)]
struct Recovery {
  since: Option<f64>,
  revision: Option<u64>,
}

/// The preferences, where they are saved and the dialog's state.
#[derive(Debug, Default)]
pub struct Preferences {
  pub values: Prefs,
  /// The values as edited in the dialog; OK and Apply make them the preferences.
  pub draft: Prefs,
  /// `settings.ini`; `None` keeps them in memory (tests).
  path: Option<PathBuf>,
  pub open: bool,
  pub page: Page,
  /// The tree's collapsed branches (all open to begin with).
  collapsed: Vec<Page>,
  /// Where recovery backups go; `None` writes none (tests).
  pub recovery_dir: Option<PathBuf>,
  recovery: Recovery,
}

impl Preferences {
  /// The user's preferences from `settings.ini`.
  pub fn load() -> Preferences {
    let mut p = Self::load_from(fox_desktop::settings::Settings::path());
    p.recovery_dir = Some(fox_desktop::config_dir().join("recovery"));
    p
  }

  pub fn load_from(path: PathBuf) -> Preferences {
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let values = Prefs::parse(&text);
    Preferences { draft: values.clone(), values, path: Some(path), ..Preferences::default() }
  }

  pub fn save(&self) {
    if let Some(path) = &self.path
      && let Err(e) = fox_desktop::settings::save_section(path, SECTION, &self.values.to_pairs())
    {
      log::warn!("couldn't save {}: {e}", path.display());
    }
  }
}

/// A `|`-separated list of names.
fn names(v: &str) -> Vec<String> {
  v.split('|').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

/// Whether a preference differs from `before`; everything differs from no `before` (start).
fn changed<T: PartialEq>(before: Option<&Prefs>, now: &Prefs, f: impl Fn(&Prefs) -> T) -> bool {
  before.is_none_or(|b| f(b) != f(now))
}

/// Puts the preferences into the app: all of them at start (`before` is `None`), else the ones
/// that changed. `ctx` is needed for the theme and the tooltips.
fn take_effect(app: &mut App, ctx: Option<&egui::Context>, before: Option<&Prefs>) {
  let p = app.preferences.values.clone();
  if changed(before, &p, |p| p.show_dims) {
    app.show_dims = p.show_dims;
  }
  if changed(before, &p, |p| p.show_constraints) {
    app.show_constraints = p.show_constraints;
  }
  if changed(before, &p, |p| p.snap) {
    app.snap = p.snap;
  }
  if changed(before, &p, |p| p.gpu) {
    // at start an unavailable GPU renderer stays off whatever the setting says
    if before.is_some() || !p.gpu {
      app.gpu.enabled = p.gpu;
    }
  }
  if changed(before, &p, |p| p.show_viewcube) {
    app.show_viewcube = p.show_viewcube;
  }
  if changed(before, &p, |p| p.orbit) {
    app.free_orbit = p.orbit == OrbitType::Free;
  }
  if let Some(ctx) = ctx {
    if changed(before, &p, |p| p.theme) {
      theme::set_theme(ctx, p.theme);
    }
    if changed(before, &p, |p| p.show_tooltips) {
      theme::set_tooltips(ctx, p.show_tooltips);
    }
  }
}

/// Puts the saved preferences into the app (at start).
pub fn apply(app: &mut App, ctx: Option<&egui::Context>) {
  take_effect(app, ctx, None);
}

/// Opens the Preferences dialog on the values as they are.
pub fn open(app: &mut App) {
  app.preferences.draft = app.preferences.values.clone();
  app.preferences.open = true;
}

/// OK and Apply: the edited values become the preferences, take effect and are saved.
pub fn commit(app: &mut App, ctx: Option<&egui::Context>) {
  let before = std::mem::replace(&mut app.preferences.values, app.preferences.draft.clone());
  take_effect(app, ctx, Some(&before));
  app.preferences.save();
}

/// Cancel: the edits are dropped and the dialog closes.
pub fn cancel(app: &mut App) {
  app.preferences.draft = app.preferences.values.clone();
  app.preferences.open = false;
}

/// Writes a recovery backup of the unsaved design once per interval (General → Automatic
/// recovery backup interval) when it changed since the last one; `now` is in seconds.
pub fn recovery_tick(app: &mut App, now: f64) {
  let minutes = app.preferences.values.recovery_minutes;
  if minutes == 0 || !app.s.dirty {
    return;
  }
  let r = &mut app.preferences.recovery;
  let Some(since) = r.since else {
    r.since = Some(now);
    return;
  };
  if now - since < minutes as f64 * 60.0 {
    return;
  }
  r.since = Some(now);
  if r.revision == Some(app.s.revision) {
    return;
  }
  let Some(dir) = app.preferences.recovery_dir.clone() else { return };
  let name: String = app
    .s
    .design
    .name
    .chars()
    .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') { c } else { '_' })
    .collect();
  let name = if name.trim().is_empty() { "Untitled".to_string() } else { name };
  let path = dir.join(format!("{name}.fox"));
  let written = std::fs::create_dir_all(&dir)
    .map_err(|e| e.to_string())
    .and_then(|_| fox_engine::files::save_design(&app.s, &path).map_err(|e| e.0));
  match written {
    Ok(()) => app.preferences.recovery.revision = Some(app.s.revision),
    Err(e) => log::warn!("couldn't write the recovery backup {}: {e}", path.display()),
  }
}

fn choice<T: Copy + PartialEq>(
  ui: &mut egui::Ui,
  id: &str,
  value: &mut T,
  all: &[T],
  label: impl Fn(T) -> &'static str,
) -> bool {
  let mut changed = false;
  let r = egui::ComboBox::from_id_salt(id).selected_text(label(*value)).width(170.0).show_ui(ui, |ui| {
    for v in all {
      changed |= ui.selectable_value(value, *v, label(*v)).changed();
    }
  });
  // where each drop-down sits, for the tests that click one
  ui.ctx().data_mut(|d| d.insert_temp(combo_rect_id(id), r.response.rect));
  changed
}

/// The id under which a drop-down's rect is kept (see [`choice`]).
pub(crate) fn combo_rect_id(id: &str) -> egui::Id {
  egui::Id::new(("prefs-combo-rect", id))
}

fn note(ui: &mut egui::Ui, text: &str) {
  ui.label(RichText::new(text).small().color(theme::MUTED));
}

/// The modal's egui area id, for tests.
pub const MODAL_ID: &str = "preferences-modal";
/// The modal's size, as Fusion's.
pub const SIZE: Vec2 = vec2(750.0, 450.0);
/// The tree's width.
const TREE_WIDTH: f32 = 190.0;

/// What a click in the button row asks for.
#[derive(Clone, Copy, PartialEq)]
enum Action {
  Apply,
  Ok,
  Cancel,
}

/// The dialog, while open: a centred modal; the edits take effect on OK or Apply.
pub fn window(app: &mut App, ctx: &egui::Context) {
  if !app.preferences.open {
    return;
  }
  let mut action = None;
  // Esc on an open drop-down closes the drop-down only, as in Fusion; the window waits for OK or
  // Cancel (or a second Esc)
  let dropdown = egui::Popup::is_any_open(ctx);
  if dropdown && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
    egui::Popup::close_all(ctx);
  }
  let frame = egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(8));
  egui::Modal::new(egui::Id::new(MODAL_ID)).frame(frame).backdrop_color(Color32::from_black_alpha(60)).show(
    ctx,
    |ui| {
      ui.set_width(SIZE.x);
      ui.set_height(SIZE.y);
      ui.horizontal(|ui| {
        ui.label(RichText::new("Preferences").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          if ui.add(egui::Button::new("✕").frame(false)).on_hover_text("Close without applying").clicked() {
            action = Some(Action::Cancel);
          }
        });
      });
      ui.separator();
      let body_height = SIZE.y - 2.0 * 30.0;
      ui.horizontal_top(|ui| {
        ui.set_height(body_height);
        ui.vertical(|ui| {
          ui.set_width(TREE_WIDTH);
          egui::ScrollArea::vertical().id_salt("prefs-tree").max_height(body_height).show(ui, |ui| {
            ui.set_width(TREE_WIDTH);
            tree(app, ui);
          });
        });
        ui.separator();
        ui.vertical(|ui| {
          let page = app.preferences.page;
          egui::Frame::NONE.fill(theme::FLOAT_HEADER).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(page.title()).strong());
          });
          ui.add_space(6.0);
          egui::ScrollArea::vertical().id_salt("prefs-page").max_height(body_height - 40.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            page_ui(app, ui);
          });
        });
      });
      ui.separator();
      ui.horizontal(|ui| {
        if ui.button("Restore Defaults").on_hover_text("Every page back to its default").clicked() {
          app.preferences.draft = Prefs::default();
        }
        info_icon(ui).on_hover_text("Changes take effect when you press OK or Apply.");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          if ui.button("Cancel").clicked() {
            action = Some(Action::Cancel);
          }
          if ui.button("OK").clicked() {
            action = Some(Action::Ok);
          }
          if ui.button("Apply").clicked() {
            action = Some(Action::Apply);
          }
        });
      });
    },
  );
  if action.is_none() && !dropdown && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
    action = Some(Action::Cancel);
  }
  match action {
    Some(Action::Apply) => commit(app, Some(ctx)),
    Some(Action::Ok) => {
      commit(app, Some(ctx));
      app.preferences.open = false;
    }
    Some(Action::Cancel) => cancel(app),
    None => {}
  }
}

/// The ⓘ beside Restore Defaults.
fn info_icon(ui: &mut egui::Ui) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
  let c = rect.center();
  let p = ui.painter();
  p.circle_stroke(c, 7.0, Stroke::new(1.2, theme::MUTED));
  p.text(c, egui::Align2::CENTER_CENTER, "i", egui::FontId::proportional(11.0), theme::MUTED);
  resp
}

/// A ▸ / ▾ expander for a branch of the tree.
fn expander(ui: &mut egui::Ui, open: bool) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::click());
  let c = rect.center();
  let pts = if open {
    [c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
  } else {
    [c + vec2(-2.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.0, 4.0)]
  };
  ui.painter().add(egui::Shape::convex_polygon(pts.to_vec(), theme::MUTED, Stroke::NONE));
  resp
}

/// Fusion's tree of sections.
fn tree(app: &mut App, ui: &mut egui::Ui) {
  ui.spacing_mut().item_spacing.y = 2.0;
  for (node, children) in TREE {
    let collapsed = app.preferences.collapsed.contains(node);
    ui.horizontal(|ui| {
      ui.spacing_mut().item_spacing.x = 2.0;
      if children.is_empty() {
        ui.add_space(14.0);
      } else if expander(ui, !collapsed).clicked() {
        if collapsed {
          app.preferences.collapsed.retain(|p| p != node);
        } else {
          app.preferences.collapsed.push(*node);
        }
      }
      if ui.selectable_label(app.preferences.page == *node, node.label()).clicked() {
        app.preferences.page = *node;
      }
    });
    if !collapsed {
      for child in children.iter() {
        ui.horizontal(|ui| {
          ui.add_space(26.0);
          if ui.selectable_label(app.preferences.page == *child, child.label()).clicked() {
            app.preferences.page = *child;
          }
        });
      }
    }
  }
}

/// A row of the settings grid: its label on the left, the control on the right.
fn row(ui: &mut egui::Ui, label: &str, control: impl FnOnce(&mut egui::Ui)) {
  ui.label(label);
  control(ui);
  ui.end_row();
}

fn flag_row(ui: &mut egui::Ui, label: &str, value: &mut bool, hover: &str) {
  row(ui, label, |ui| {
    let r = ui.checkbox(value, "");
    if !hover.is_empty() {
      r.on_hover_text(hover);
    }
  });
}

/// A setting Fusion has that FissionOxide has no use for yet, shown at Fusion's default.
fn fixed_row(ui: &mut egui::Ui, label: &str, value: &str) {
  row(ui, label, |ui| {
    ui.add_enabled_ui(false, |ui| {
      egui::ComboBox::from_id_salt(label).selected_text(value).width(170.0).show_ui(ui, |_| {});
    });
  });
}

/// A check box Fusion has that FissionOxide can't act on yet, greyed at Fusion's default.
fn fixed_flag_row(ui: &mut egui::Ui, label: &str, value: bool) {
  row(ui, label, |ui| {
    let mut v = value;
    ui.add_enabled(false, egui::Checkbox::new(&mut v, "")).on_disabled_hover_text("Not in FissionOxide yet");
  });
}

fn page_ui(app: &mut App, ui: &mut egui::Ui) {
  let mut values = app.preferences.draft.clone();
  let p = &mut values;
  match app.preferences.page {
    Page::General => {
      egui::Grid::new("prefs-general").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        fixed_row(ui, "User language", "English");
        row(ui, "Theme", |ui| {
          choice(ui, "prefs-theme", &mut p.theme, &Theme::ALL, Theme::label);
        });
        row(ui, "Automatic recovery backup interval", |ui| {
          egui::ComboBox::from_id_salt("prefs-recovery")
            .selected_text(recovery_label(p.recovery_minutes))
            .width(170.0)
            .show_ui(ui, |ui| {
              for m in RECOVERY_CHOICES {
                ui.selectable_value(&mut p.recovery_minutes, m, recovery_label(m));
              }
            });
        });
        row(ui, "Default modeling orientation", |ui| {
          choice(ui, "prefs-up", &mut p.up_axis, &UpAxis::ALL, UpAxis::label);
        });
        flag_row(ui, "Show tooltips", &mut p.show_tooltips, "Hover texts on the toolbar and the controls");
        flag_row(ui, "Show command prompt", &mut p.show_prompt, "The command's prompt at the cursor");
        row(ui, "Mouse setup (pan, zoom, orbit)", |ui| {
          choice(ui, "prefs-nav-preset", &mut p.nav, &NavPreset::ALL, NavPreset::label);
        });
        row(ui, "Default Orbit type", |ui| {
          choice(ui, "prefs-orbit", &mut p.orbit, &OrbitType::ALL, OrbitType::label);
        });
        flag_row(
          ui,
          "Reverse zoom direction",
          &mut p.reverse_zoom,
          "Off, as in Fusion: rolling the wheel toward you zooms in",
        );
        flag_row(ui, "Enable camera pivot", &mut p.camera_pivot, "Shows the orbit's pivot while orbiting");
        row(ui, "ViewCube size", |ui| {
          choice(ui, "prefs-cube", &mut p.cube_size, &CubeSize::ALL, CubeSize::label);
        });
        // Fusion's touch rows; FissionOxide reads no touch gestures yet
        fixed_flag_row(ui, "Use gesture-based view navigation", false);
        fixed_flag_row(ui, "Use Windows touchpad for gestures", false);
        flag_row(ui, "Spacebar repeats last command", &mut p.space_repeats, "");
      });
      ui.add_space(6.0);
      note(ui, "The orientation is saved, but the view stays Z up until the up axis can be changed.");
    }
    Page::Design => {
      egui::Grid::new("prefs-design").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        row(ui, "Default units", |ui| {
          choice(ui, "prefs-units", &mut p.units, &Units::ALL, Units::label);
        });
        flag_row(ui, "Capture design history", &mut p.capture_history, "Off: new designs start without a timeline");
        flag_row(
          ui,
          "Auto look at sketch",
          &mut p.auto_look_at,
          "Creating or editing a sketch turns the view to face it",
        );
        flag_row(ui, "Show sketch dimensions", &mut p.show_dims, "");
        flag_row(ui, "Show sketch constraints", &mut p.show_constraints, "");
        flag_row(ui, "Snap sketch points to grid", &mut p.snap, "");
        flag_row(
          ui,
          "Show degrees of freedom in the Sketch Palette",
          &mut p.show_dof,
          "A line under the palette's options: the sketch's degrees of freedom and profiles",
        );
      });
      ui.add_space(6.0);
      note(ui, "Default units apply to new designs; Document Settings changes a design's own units.");
      note(ui, "Without design history new designs have no timeline; edits go into a base feature.");
      ui.add_space(8.0);
      let capturing = !app.s.design.history_off;
      ui.horizontal(|ui| {
        ui.label(format!("This design {} design history.", if capturing { "captures" } else { "doesn't capture" }));
        let label = if capturing { "Do Not Capture" } else { "Capture Design History" };
        if ui.button(label).clicked() {
          crate::config_ui::toggle_history(app);
        }
      });
    }
    Page::Graphics => {
      ui.checkbox(&mut p.show_viewcube, "Show ViewCube");
      ui.horizontal(|ui| {
        ui.label("ViewCube size");
        choice(ui, "prefs-cube-size", &mut p.cube_size, &CubeSize::ALL, CubeSize::label);
      });
      note(ui, "Automatic sizes the ViewCube by the window; the ViewCube's own menu sets this too.");
      ui.add_space(6.0);
      let resp = ui.checkbox(&mut p.gpu, "GPU renderer");
      match &app.gpu.failed {
        Some(why) => {
          resp.on_hover_text(format!("Unavailable: {why}. The CPU renderer draws the viewport."));
          note(ui, "The GPU renderer isn't available here; the CPU renderer draws the viewport.");
        }
        None => {
          resp.on_hover_text("Draw the viewport with OpenGL; off draws it with the CPU renderer");
        }
      }
      note(ui, "FOX_RENDERER=cpu turns the GPU renderer off at start whatever this says.");
    }
    Page::DefaultUnitsDesign => {
      egui::Grid::new("prefs-default-units").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        row(ui, "Default units", |ui| {
          choice(ui, "prefs-units-2", &mut p.units, &Units::ALL, Units::label);
        });
      });
      ui.add_space(6.0);
      note(ui, "Default units apply to new designs; Document Settings changes a design's own units.");
    }
    _ => {
      note(ui, "No settings yet.");
    }
  }
  app.preferences.draft = values;
}

#[cfg(test)]
mod tests {
  use egui::Modifiers;

  use super::*;

  #[test]
  fn preferences_round_trip_through_settings_ini() {
    let p = Prefs {
      units: Units::In,
      up_axis: UpAxis::Z,
      nav: NavPreset::SolidWorks,
      reverse_zoom: true,
      zoom_to_cursor: false,
      capture_history: false,
      auto_look_at: false,
      show_dims: false,
      show_constraints: false,
      snap: true,
      gpu: false,
      space_repeats: false,
      show_viewcube: false,
      cube_size: CubeSize::Tiny,
      theme: Theme::Dark,
      show_tooltips: false,
      show_prompt: false,
      orbit: OrbitType::Free,
      camera_pivot: false,
      recovery_minutes: 15,
      show_dof: true,
      bom_hidden: vec!["Thumbnail".into(), "Change Order".into()],
      bom_order: vec!["Description".into(), "Thumbnail".into()],
    };
    let ini = "[Updates]\nMode=notify\n\n[Window]\nMaximized=1\n";
    let text = p.write_into(ini);
    assert_eq!(Prefs::parse(&text), p);
    // the other sections are kept
    assert!(text.starts_with("[Updates]\nMode=notify\n"), "{text}");
    assert!(text.contains("[Window]\nMaximized=1"), "{text}");
    // written again over itself it doesn't grow
    assert_eq!(p.write_into(&text), text);
    // missing and unknown values fall back to the defaults
    assert_eq!(Prefs::parse(""), Prefs::default());
    let odd = Prefs::parse("[preferences]\nunits=furlong\nNavigation=ALIAS\nSketchSnap=yes\n");
    assert_eq!((odd.units, odd.nav, odd.snap), (Units::Mm, NavPreset::Alias, true));

    // through the file, beside what the updater writes
    let dir = std::env::temp_dir().join(format!("fox-prefs-{}", std::process::id()));
    let path = dir.join("settings.ini");
    let mut prefs = Preferences::load_from(path.clone());
    assert_eq!(prefs.values, Prefs::default());
    prefs.values = p.clone();
    prefs.save();
    let updates = fox_desktop::settings::Settings::load_from(&path);
    updates.save_to(&path).unwrap();
    assert_eq!(Preferences::load_from(path).values, p);
    std::fs::remove_dir_all(dir).unwrap();
  }

  #[test]
  fn new_designs_follow_the_design_and_sketch_defaults() {
    let mut app = App::with_session(fox_engine::Session::default());
    app.preferences.values.capture_history = false;
    app.preferences.values.show_dims = false;
    app.preferences.values.snap = true;
    app.preferences.values.gpu = false;
    apply(&mut app, None);
    assert!(!app.show_dims && app.snap && !app.gpu.enabled);
    app.new_design();
    assert!(app.s.design.history_off, "no timeline");
    assert!(!app.s.can_undo() && !app.s.dirty, "the setting isn't an undo step");
    app.preferences.values.capture_history = true;
    app.new_design();
    assert!(!app.s.design.history_off);
  }

  #[test]
  fn new_designs_take_the_default_units() {
    let mut app = App::with_session(fox_engine::Session::default());
    app.preferences.values.units = Units::In;
    app.new_design();
    assert_eq!(app.s.design.units, fox_engine::model::LengthUnit::In);
    assert!(!app.s.can_undo() && !app.s.dirty, "the setting isn't an undo step");
    app.preferences.values.units = Units::Mm;
    app.new_design();
    assert_eq!(app.s.design.units, fox_engine::model::LengthUnit::Mm);
  }

  #[test]
  fn ok_and_apply_take_effect_and_cancel_drops_the_edits() {
    let mut app = App::with_session(fox_engine::Session::default());
    open(&mut app);
    assert!(app.preferences.open);
    app.preferences.draft.orbit = OrbitType::Free;
    app.preferences.draft.snap = true;
    // nothing happens until OK or Apply
    assert!(!app.free_orbit && !app.snap && app.preferences.values.orbit == OrbitType::Constrained);
    cancel(&mut app);
    assert!(!app.preferences.open && app.preferences.draft == app.preferences.values && !app.snap);
    open(&mut app);
    app.preferences.draft.orbit = OrbitType::Free;
    app.preferences.draft.snap = true;
    commit(&mut app, None);
    assert!(app.free_orbit && app.snap, "Apply puts the edits into the app");
    assert_eq!(app.preferences.values.orbit, OrbitType::Free);
    // Restore Defaults is an edit like any other: it needs OK or Apply too
    app.preferences.draft = Prefs::default();
    assert!(app.free_orbit && app.preferences.values.snap);
    commit(&mut app, None);
    assert!(!app.free_orbit && !app.snap);
  }

  #[test]
  fn recovery_backups_follow_the_interval_and_the_design() {
    let dir = std::env::temp_dir().join(format!("fox-recovery-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut app = App::with_session(fox_engine::Session::default());
    app.preferences.recovery_dir = Some(dir.clone());
    app.preferences.values.recovery_minutes = 1;
    app.s.design.name = "Bracket v2".into();
    let backup = dir.join("Bracket v2.fox");
    // nothing to back up while the design is saved
    recovery_tick(&mut app, 0.0);
    recovery_tick(&mut app, 600.0);
    assert!(!backup.exists());
    app.run("sketch.create", serde_json::json!({"plane": "xy"}));
    assert!(app.s.dirty);
    recovery_tick(&mut app, 600.0);
    assert!(!backup.exists(), "the interval starts counting at the first unsaved change");
    recovery_tick(&mut app, 630.0);
    assert!(!backup.exists(), "half a minute isn't the interval");
    recovery_tick(&mut app, 661.0);
    assert!(backup.exists(), "a minute later the backup is written");
    assert!(app.s.dirty, "a backup isn't a save");
    let first = std::fs::metadata(&backup).unwrap().modified().unwrap();
    std::fs::remove_file(&backup).unwrap();
    recovery_tick(&mut app, 730.0);
    assert!(!backup.exists(), "nothing changed since the last backup");
    app.run("sketch.line", serde_json::json!({"points": [[0, 0], [10, 0]]}));
    recovery_tick(&mut app, 800.0);
    assert!(backup.exists() && std::fs::metadata(&backup).unwrap().modified().unwrap() >= first);
    // Never
    app.preferences.values.recovery_minutes = 0;
    std::fs::remove_file(&backup).unwrap();
    app.run("sketch.line", serde_json::json!({"points": [[0, 5], [10, 5]]}));
    recovery_tick(&mut app, 2000.0);
    assert!(!backup.exists());
    std::fs::remove_dir_all(dir).unwrap();
  }

  #[test]
  fn the_viewcube_size_scales_the_cube() {
    let f = |c: CubeSize| c.factor(900.0);
    assert!(f(CubeSize::Tiny) < f(CubeSize::Small) && f(CubeSize::Small) < f(CubeSize::Normal));
    assert!(f(CubeSize::Normal) < f(CubeSize::Large));
    assert!(
      CubeSize::Automatic.factor(400.0) < CubeSize::Automatic.factor(900.0),
      "a short canvas gets a smaller cube"
    );
    assert_eq!(recovery_label(0), "Never");
    assert_eq!(recovery_label(1), "1 minute");
    assert_eq!(recovery_label(5), "5 minutes");
    // Fusion's tree: General's children, then the top-level sections in its order
    let top: Vec<&str> = TREE.iter().map(|(p, _)| p.label()).collect();
    assert_eq!(top[0], "General");
    assert_eq!(top.last().copied(), Some("Compatibility & Troubleshooting"));
    assert_eq!(TREE[0].1.first().map(|p| p.label()), Some("API"));
    assert_eq!(Page::General.title(), "Preferences controlling general UI behavior");
  }

  #[test]
  fn navigation_presets_map_buttons_as_their_programs_do() {
    use NavAction::*;
    use PointerButton::*;
    let none = Modifiers::NONE;
    let shift = Modifiers::SHIFT;
    let ctrl = Modifiers::CTRL;
    let shift_alt = Modifiers::SHIFT | Modifiers::ALT;
    let f = NavPreset::Fusion;
    assert_eq!(f.action(Middle, none), Some(Pan));
    assert_eq!(f.action(Middle, shift), Some(Orbit));
    assert_eq!(f.action(Secondary, none), Some(Orbit));
    assert_eq!(f.action(Secondary, shift), Some(Pan));
    assert_eq!(f.action(Primary, none), None, "left drags select");
    let sw = NavPreset::SolidWorks;
    assert_eq!(sw.action(Middle, none), Some(Orbit));
    assert_eq!(sw.action(Middle, ctrl), Some(Pan));
    assert_eq!(sw.action(Middle, shift), Some(Zoom));
    assert_eq!(sw.action(Secondary, none), None);
    let inv = NavPreset::Inventor;
    assert_eq!(inv.action(Middle, none), Some(Pan));
    assert_eq!(inv.action(Middle, shift), Some(Orbit));
    assert_eq!(inv.action(Secondary, none), None, "right is for the marking menu");
    let al = NavPreset::Alias;
    assert_eq!(al.action(Primary, shift_alt), Some(Orbit));
    assert_eq!(al.action(Middle, shift_alt), Some(Pan));
    assert_eq!(al.action(Secondary, shift_alt), Some(Zoom));
    assert_eq!(al.action(Primary, none), None);
    assert_eq!(al.action(Middle, none), None);
    let pm = NavPreset::PowerMill;
    assert_eq!(pm.action(Middle, none), Some(Orbit));
    assert_eq!(pm.action(Middle, shift), Some(Pan));
    assert_eq!(pm.action(Middle, ctrl), Some(Zoom));
    let tc = NavPreset::Tinkercad;
    assert_eq!(tc.action(Secondary, none), Some(Orbit));
    assert_eq!(tc.action(Middle, none), Some(Pan));
    assert_eq!(tc.action(Secondary, shift), Some(Pan));
    assert_eq!(tc.action(Primary, none), None, "left drags select");
  }

  #[test]
  fn mouse_setup_lists_fusions_presets_in_its_order() {
    let labels: Vec<&str> = NavPreset::ALL.iter().map(|n| n.label()).collect();
    assert_eq!(labels, ["Fusion", "Alias", "Inventor", "PowerMill", "SOLIDWORKS", "Tinkercad"]);
    // each preset's saved key reads back as that preset, and the old SolidWorks key still does
    for n in NavPreset::ALL {
      let back = Prefs::from_pairs(&[("Navigation".into(), n.key().into())]);
      assert_eq!(back.nav, n);
    }
    assert_eq!(Prefs::from_pairs(&[("Navigation".into(), "SolidWorks".into())]).nav, NavPreset::SolidWorks);
  }

  #[test]
  fn the_default_modeling_orientation_is_y_up_as_in_fusion() {
    assert_eq!(Prefs::default().up_axis, UpAxis::Y);
    assert_eq!(UpAxis::default().label(), "Y up");
    // a saved choice still wins
    assert_eq!(Prefs::from_pairs(&[("UpAxis".into(), "z".into())]).up_axis, UpAxis::Z);
  }
}
