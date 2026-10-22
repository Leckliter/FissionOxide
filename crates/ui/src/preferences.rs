//! The Preferences dialog, as in Fusion (profile menu or File → Preferences): a centred modal
//! with Fusion's tree of sections on the left, the page on the right and Restore Defaults,
//! Apply, OK and Cancel at the bottom. Changes take effect on OK or Apply. The values live in
//! the `[Preferences]` section of `settings.ini` (see [`fox_desktop::settings`]).

use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{Color32, PointerButton, RichText, Sense, Stroke, Vec2, pos2, vec2};
use fox_engine::render::Effects;
use fox_engine::rendering::raytrace::{Background, Environment};

use crate::viewcube::CubeSize;
use crate::{App, theme};

mod pages;
use pages::{Item, Live};

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
      // Fusion's: middle pans, Shift+middle orbits; the right button is the marking menu's (a
      // click opens it, a drag runs the item it points at)
      NavPreset::Fusion => &[(Middle, NONE, Pan), (Middle, SHIFT, Orbit)],
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

/// General → Graphics driver. Both of FissionOxide's choices draw with OpenGL; Fusion's DirectX
/// entries are listed greyed, and FissionOxide's CPU renderer is the list's last entry
/// ([`Prefs::gpu`] off).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Driver {
  #[default]
  Auto,
  OpenGl,
}

impl Driver {
  pub fn key(self) -> &'static str {
    match self {
      Driver::Auto => "auto",
      Driver::OpenGl => "opengl",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Driver::Auto => "Auto-select",
      Driver::OpenGl => "OpenGL",
    }
  }
}

/// The Graphics page's Canvas Effects, in Fusion's order: its label and saved key.
pub(crate) const EFFECTS: [(&str, &str); 7] = [
  ("Environment Dome", "EnvironmentDome"),
  ("Ground Plane", "GroundPlane"),
  ("Ground Shadow", "GroundShadow"),
  ("Ground Reflection", "GroundReflection"),
  ("Object Shadow", "ObjectShadow"),
  ("Ambient Occlusion", "AmbientOcclusion"),
  ("Anti-Aliasing", "AntiAliasing"),
];

/// One of [`EFFECTS`] in the effects.
pub(crate) fn effect(e: &mut Effects, i: usize) -> &mut bool {
  match i {
    0 => &mut e.environment_dome,
    1 => &mut e.ground_plane,
    2 => &mut e.ground_shadow,
    3 => &mut e.reflections,
    4 => &mut e.object_shadow,
    5 => &mut e.ambient_occlusion,
    _ => &mut e.anti_aliasing,
  }
}

/// Unit and Value Display → the precisions offered: "0", "0.1", "0.12" … eight decimals.
pub const PRECISIONS: std::ops::RangeInclusive<u8> = 0..=8;

/// A precision as Fusion's drop-down shows it: "0.123" for three decimals.
pub fn precision_label(decimals: u8) -> String {
  if decimals == 0 { "0".into() } else { format!("0.{}", &"123456789"[..decimals as usize]) }
}

/// Render → the Scene Settings a new design starts with.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderDefaults {
  /// The Environment Library entry, by name (see [`crate::render_panels::LIBRARY`]).
  pub environment: String,
  /// Brightness in lux.
  pub brightness: f32,
  /// Background: a solid colour, else the environment.
  pub solid_background: bool,
  pub color: [u8; 3],
  pub ground: bool,
  pub reflections: bool,
}

impl Default for RenderDefaults {
  /// Fusion's: Sharp Highlights at 1000 lx before a solid grey, the ground plane on.
  fn default() -> Self {
    RenderDefaults {
      environment: "Sharp Highlights".into(),
      brightness: crate::render_panels::LUX,
      solid_background: true,
      color: [178, 178, 178],
      ground: true,
      reflections: false,
    }
  }
}

impl RenderDefaults {
  /// The Scene Settings these make.
  pub fn environment(&self) -> Environment {
    let preset = crate::render_panels::LIBRARY.iter().find(|l| l.0 == self.environment).map(|l| l.1);
    Environment {
      preset: preset.unwrap_or_default(),
      intensity: self.brightness / crate::render_panels::LUX,
      background: if self.solid_background { Background::Color } else { Background::Environment },
      background_color: self.color,
      ground: self.ground,
      ground_reflection: if self.ground && self.reflections { 0.5 } else { 0.0 },
      ..Environment::default()
    }
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
  /// General → Graphics driver (with [`Prefs::gpu`] on).
  pub driver: Driver,
  /// Graphics → Canvas Effects: the Display Settings effects the canvas starts with.
  pub effects: Effects,
  /// Unit and Value Display → General precision: the decimals lengths are shown with; `None`
  /// takes the design unit's (two in millimetres, three in inches).
  pub general_precision: Option<u8>,
  /// Unit and Value Display → Angular precision: the decimals angles are shown with.
  pub angular_precision: u8,
  /// Render → the Scene Settings of new designs.
  pub render: RenderDefaults,
  /// Material → Physical Material Name: what new designs' root component is made of.
  pub material: String,
  /// The rows FissionOxide shows but doesn't act on (see [`pages`]), by key, where they hold
  /// something other than Fusion's default.
  pub stored: BTreeMap<String, String>,
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
      driver: Driver::Auto,
      effects: crate::navbar::default_effects(),
      general_precision: None,
      angular_precision: 1,
      render: RenderDefaults::default(),
      material: "Steel".into(),
      stored: BTreeMap::new(),
    }
  }
}

pub const SECTION: &str = "Preferences";
/// Where the window keeps the page it showed last.
const WINDOW_SECTION: &str = "PreferencesWindow";

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
      ("GraphicsDriver", self.driver.key().to_string()),
      ("GeneralPrecision", self.general_precision.map_or("auto".to_string(), |d| d.to_string())),
      ("AngularPrecision", self.angular_precision.to_string()),
      ("RenderEnvironment", self.render.environment.clone()),
      ("RenderBrightness", self.render.brightness.to_string()),
      ("RenderBackground", (if self.render.solid_background { "color" } else { "environment" }).to_string()),
      ("RenderColor", self.render.color.map(|c| c.to_string()).join(",")),
      ("RenderGroundPlane", flag(self.render.ground)),
      ("RenderReflections", flag(self.render.reflections)),
      ("Material", self.material.clone()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .chain((0..EFFECTS.len()).map(|i| {
      let mut e = self.effects;
      (format!("Effect{}", EFFECTS[i].1), flag(*effect(&mut e, i)))
    }))
    .chain(self.stored.iter().map(|(k, v)| (k.clone(), v.clone())))
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
        "graphicsdriver" => p.driver = pick(&[Driver::Auto, Driver::OpenGl], Driver::key, v).unwrap_or(p.driver),
        "generalprecision" => {
          p.general_precision = v.trim().parse::<u8>().ok().filter(|d| PRECISIONS.contains(d));
        }
        "angularprecision" => {
          p.angular_precision = v.trim().parse::<u8>().ok().filter(|d| PRECISIONS.contains(d)).unwrap_or(1);
        }
        "renderenvironment" => {
          if let Some(l) = crate::render_panels::LIBRARY.iter().find(|l| l.0.eq_ignore_ascii_case(v.trim())) {
            p.render.environment = l.0.to_string();
          }
        }
        "renderbrightness" => p.render.brightness = v.trim().parse().unwrap_or(p.render.brightness),
        "renderbackground" => p.render.solid_background = !v.trim().eq_ignore_ascii_case("environment"),
        "rendercolor" => {
          let c: Vec<u8> = v.split(',').filter_map(|c| c.trim().parse().ok()).collect();
          if let [r, g, b] = c[..] {
            p.render.color = [r, g, b];
          }
        }
        "rendergroundplane" => p.render.ground = b.unwrap_or(p.render.ground),
        "renderreflections" => p.render.reflections = b.unwrap_or(p.render.reflections),
        "material" => {
          if let Some((m, _)) = fox_engine::model::assembly::MATERIALS.iter().find(|(m, _)| m.eq_ignore_ascii_case(v)) {
            p.material = m.to_string();
          }
        }
        k if k.starts_with("effect") => {
          if let Some(i) = EFFECTS.iter().position(|e| e.1.eq_ignore_ascii_case(&k["effect".len()..])) {
            *effect(&mut p.effects, i) = b.unwrap_or(*effect(&mut p.effects, i));
          }
        }
        k if k.starts_with("fusion.") => {
          if let Some((key, _)) = pages::stored_rows().into_iter().find(|(key, _)| key.eq_ignore_ascii_case(k)) {
            p.stored.insert(key, v.clone());
          }
        }
        _ => {}
      }
    }
    p
  }

  /// A stored row's value: what was saved, else Fusion's default.
  fn stored_value(&self, key: &str, item: &Item) -> String {
    self.stored.get(key).cloned().or_else(|| item.default_value()).unwrap_or_default()
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

/// A page of Fusion's tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
  #[default]
  General,
  Api,
  Design,
  DesignAssembly,
  DesignConfigurations,
  Manufacture,
  ManufactureOptional,
  Electronics,
  ElecColor,
  ElecMisc,
  ElecGrid,
  ElecDefaults,
  ElecDrill,
  ElecDirectory,
  Elec3d,
  Render,
  Drawing,
  /// "Simulation and Generative Design", one node under General.
  Simulation,
  Tokens,
  Material,
  Graphics,
  GraphicsRdp,
  Network,
  DataCollection,
  UnitDisplay,
  UnitDisplaySim,
  DefaultUnits,
  DefaultUnitsManufacture,
  DefaultUnitsDesign,
  DefaultUnitsSimulation,
  PreviewFeatures,
  Compatibility,
}

/// A node of the tree: its page and the nodes under it.
pub(crate) struct Node {
  pub page: Page,
  pub children: &'static [Node],
}

const fn leaf(page: Page) -> Node {
  Node { page, children: &[] }
}

/// Fusion's tree, in its order; a node with children starts collapsed.
pub(crate) const TREE: &[Node] = &[
  Node {
    page: Page::General,
    children: &[
      leaf(Page::Api),
      Node { page: Page::Design, children: &[leaf(Page::DesignAssembly), leaf(Page::DesignConfigurations)] },
      Node { page: Page::Manufacture, children: &[leaf(Page::ManufactureOptional)] },
      Node {
        page: Page::Electronics,
        children: &[
          leaf(Page::ElecColor),
          leaf(Page::ElecMisc),
          leaf(Page::ElecGrid),
          leaf(Page::ElecDefaults),
          leaf(Page::ElecDrill),
          leaf(Page::ElecDirectory),
          leaf(Page::Elec3d),
        ],
      },
      leaf(Page::Render),
      leaf(Page::Drawing),
      leaf(Page::Simulation),
    ],
  },
  leaf(Page::Tokens),
  leaf(Page::Material),
  Node { page: Page::Graphics, children: &[leaf(Page::GraphicsRdp)] },
  leaf(Page::Network),
  leaf(Page::DataCollection),
  Node { page: Page::UnitDisplay, children: &[leaf(Page::UnitDisplaySim)] },
  Node {
    page: Page::DefaultUnits,
    children: &[
      leaf(Page::DefaultUnitsManufacture),
      leaf(Page::DefaultUnitsDesign),
      leaf(Page::DefaultUnitsSimulation),
    ],
  },
  leaf(Page::PreviewFeatures),
  leaf(Page::Compatibility),
];

/// The pages above `page` in the tree, outermost first.
pub(crate) fn ancestors(page: Page) -> Vec<Page> {
  fn walk(nodes: &[Node], page: Page, path: &mut Vec<Page>) -> bool {
    for n in nodes {
      if n.page == page {
        return true;
      }
      path.push(n.page);
      if walk(n.children, page, path) {
        return true;
      }
      path.pop();
    }
    false
  }
  let mut path = Vec::new();
  walk(TREE, page, &mut path);
  path
}

impl Page {
  pub const ALL: [Page; 32] = [
    Page::General,
    Page::Api,
    Page::Design,
    Page::DesignAssembly,
    Page::DesignConfigurations,
    Page::Manufacture,
    Page::ManufactureOptional,
    Page::Electronics,
    Page::ElecColor,
    Page::ElecMisc,
    Page::ElecGrid,
    Page::ElecDefaults,
    Page::ElecDrill,
    Page::ElecDirectory,
    Page::Elec3d,
    Page::Render,
    Page::Drawing,
    Page::Simulation,
    Page::Tokens,
    Page::Material,
    Page::Graphics,
    Page::GraphicsRdp,
    Page::Network,
    Page::DataCollection,
    Page::UnitDisplay,
    Page::UnitDisplaySim,
    Page::DefaultUnits,
    Page::DefaultUnitsManufacture,
    Page::DefaultUnitsDesign,
    Page::DefaultUnitsSimulation,
    Page::PreviewFeatures,
    Page::Compatibility,
  ];

  /// The page's name in `settings.ini` (the page last shown, and the stored rows' keys).
  pub fn key(self) -> &'static str {
    match self {
      Page::General => "General",
      Page::Api => "API",
      Page::Design => "Design",
      Page::DesignAssembly => "Design.Assembly",
      Page::DesignConfigurations => "Design.Configurations",
      Page::Manufacture => "Manufacture",
      Page::ManufactureOptional => "Manufacture.OptionalFeatures",
      Page::Electronics => "Electronics",
      Page::ElecColor => "Electronics.Color",
      Page::ElecMisc => "Electronics.Misc",
      Page::ElecGrid => "Electronics.Grid",
      Page::ElecDefaults => "Electronics.DefaultValues",
      Page::ElecDrill => "Electronics.DrillLegend",
      Page::ElecDirectory => "Electronics.Directory",
      Page::Elec3d => "Electronics.3DPCB",
      Page::Render => "Render",
      Page::Drawing => "Drawing",
      Page::Simulation => "Simulation",
      Page::Tokens => "Tokens",
      Page::Material => "Material",
      Page::Graphics => "Graphics",
      Page::GraphicsRdp => "Graphics.RDP",
      Page::Network => "Network",
      Page::DataCollection => "DataCollection",
      Page::UnitDisplay => "UnitDisplay",
      Page::UnitDisplaySim => "UnitDisplay.Simulation",
      Page::DefaultUnits => "DefaultUnits",
      Page::DefaultUnitsManufacture => "DefaultUnits.Manufacture",
      Page::DefaultUnitsDesign => "DefaultUnits.Design",
      Page::DefaultUnitsSimulation => "DefaultUnits.Simulation",
      Page::PreviewFeatures => "PreviewFeatures",
      Page::Compatibility => "Compatibility",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Page::General => "General",
      Page::Api => "API",
      Page::Design | Page::DefaultUnitsDesign => "Design",
      Page::DesignAssembly => "Assembly and Hybrid Design",
      Page::DesignConfigurations => "Configurations",
      Page::Manufacture | Page::DefaultUnitsManufacture => "Manufacture",
      Page::ManufactureOptional => "Optional Features",
      Page::Electronics => "Electronics",
      Page::ElecColor => "Color",
      Page::ElecMisc => "Misc",
      Page::ElecGrid => "Default Grid and Units",
      Page::ElecDefaults => "Default Values",
      Page::ElecDrill => "Drill Legend",
      Page::ElecDirectory => "Directory",
      Page::Elec3d => "3D PCB",
      Page::Render => "Render",
      Page::Drawing => "Drawing",
      Page::Simulation | Page::UnitDisplaySim | Page::DefaultUnitsSimulation => "Simulation and Generative Design",
      Page::Tokens => "Tokens",
      Page::Material => "Material",
      Page::Graphics => "Graphics",
      Page::GraphicsRdp => "RDP Optimization",
      Page::Network => "Network",
      Page::DataCollection => "Data Collection and Use",
      Page::UnitDisplay => "Unit and Value Display",
      Page::DefaultUnits => "Default Units",
      Page::PreviewFeatures => "Preview Features",
      Page::Compatibility => "Compatibility & Troubleshooting",
    }
  }

  /// The page's title box, as Fusion words it (a page whose title wasn't recorded shows its
  /// name).
  pub fn title(self) -> &'static str {
    match self {
      Page::General => "Preferences controlling general UI behavior",
      Page::Api => "Preferences for Scripting and Programming",
      Page::Design => "Preferences controlling general Design behavior",
      Page::DesignAssembly => "Preferences for Assembly and Hybrid Designs",
      Page::DesignConfigurations => "Preferences that control Configurations behavior",
      Page::Manufacture => "Preferences controlling behavior in the Manufacture workspace",
      Page::Electronics => "Preferences for Electronics",
      Page::Render => "Preferences that set Render Scene Settings defaults for newly created designs",
      Page::Drawing => "Preferences for the Drawing and Drawing Template \"From Scratch\" option",
      Page::Simulation => "Preferences for the Simulation and Generative Design workspaces",
      Page::Tokens => "Token preferences",
      Page::Material => "Preferences that control default material and appearance",
      Page::Graphics => "Preferences used to control the graphics display",
      Page::GraphicsRdp => "Preferences used to optimize the graphics performance in RDP session.",
      Page::Network => "These are a set of options that are used for network access.",
      Page::UnitDisplay => "Preferences that control how units and values are displayed",
      Page::UnitDisplaySim => "Display of output values in the Simulation and Generative Design workspaces",
      Page::DefaultUnitsManufacture => "Preferences that set default units for new Manufacture documents",
      Page::DefaultUnitsDesign => "Preferences that sets default units used in new documents",
      Page::DefaultUnitsSimulation => "Default units for simulation and generative design studies",
      Page::PreviewFeatures => "Preferences to try Preview Functionality",
      Page::Compatibility => {
        "Miscellaneous options which may enable Fusion to perform better on certain hardware or network \
         configurations, and to help diagnose undesirable application behavior."
      }
      p => p.label(),
    }
  }

  fn parse(key: &str) -> Option<Page> {
    Page::ALL.into_iter().find(|p| p.key().eq_ignore_ascii_case(key.trim()))
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
  /// The page shown; the window reopens on it, as Fusion's does (kept in `[PreferencesWindow]`).
  pub page: Page,
  /// The tree's expanded branches (all collapsed to begin with, as in Fusion).
  pub(crate) expanded: Vec<Page>,
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
    let page = fox_desktop::settings::section(&text, WINDOW_SECTION)
      .iter()
      .find(|(k, _)| k.eq_ignore_ascii_case("page"))
      .and_then(|(_, v)| Page::parse(v))
      .unwrap_or_default();
    Preferences { draft: values.clone(), values, path: Some(path), page, ..Preferences::default() }
  }

  /// Keeps the page shown for the next time the window opens.
  fn save_page(&self) {
    if let Some(path) = &self.path
      && let Err(e) =
        fox_desktop::settings::save_section(path, WINDOW_SECTION, &[("Page".into(), self.page.key().into())])
    {
      log::warn!("couldn't save {}: {e}", path.display());
    }
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
  if changed(before, &p, |p| p.effects) {
    app.effects = p.effects;
  }
  if changed(before, &p, |p| (p.general_precision, p.angular_precision)) {
    crate::units::set_precision(p.general_precision, p.angular_precision);
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

/// Opens the Preferences dialog on the values as they are, at the page shown last.
pub fn open(app: &mut App) {
  app.preferences.draft = app.preferences.values.clone();
  app.preferences.open = true;
  let p = &mut app.preferences;
  for a in ancestors(p.page) {
    if !p.expanded.contains(&a) {
      p.expanded.push(a);
    }
  }
}

/// Closes the window, keeping the page it showed.
fn close(app: &mut App) {
  app.preferences.open = false;
  app.preferences.save_page();
}

/// What a new design takes from the preferences beyond its units and history: the Render page's
/// Scene Settings and the Material page's physical material. Returns whether it changed `s`.
pub(crate) fn seed_new_design(p: &Prefs, s: &mut fox_engine::Session) -> bool {
  let mut set = false;
  let env = p.render.environment();
  if env != s.design.render.environment {
    let background = if env.background == Background::Color { "color" } else { "environment" };
    let args = serde_json::json!({
      "preset": env.preset, "intensity": env.intensity, "background": background,
      "background_color": env.background_color, "ground": env.ground, "ground_reflection": env.ground_reflection,
    });
    set |= s.execute("render.environment", &args).is_ok();
  }
  if p.material != s.design.root_material() {
    set |= s.execute("design.material", &serde_json::json!({"material": p.material})).is_ok();
  }
  set
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
  close(app);
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
  let r = egui::ComboBox::from_id_salt(id).selected_text(label(*value)).width(COMBO_W).show_ui(ui, |ui| {
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

/// The id under which a tree row's rect is kept, for tests.
pub(crate) fn tree_row_id(page: Page) -> egui::Id {
  egui::Id::new(("prefs-tree-row", page.key()))
}

/// The id under which the title bar's X is kept, for tests.
pub(crate) fn close_rect_id() -> egui::Id {
  egui::Id::new("prefs-close-rect")
}

fn note(ui: &mut egui::Ui, text: &str) {
  ui.label(RichText::new(text).small().color(theme::MUTED));
}

/// The modal's egui area id, for tests.
pub const MODAL_ID: &str = "preferences-modal";
/// The modal's size, as Fusion's (about 748 x 447).
pub const SIZE: Vec2 = vec2(748.0, 447.0);
/// The tree's width.
const TREE_WIDTH: f32 = 222.0;
/// A tree row's height.
const TREE_ROW: f32 = 20.0;
/// A page row's least height.
const ROW_H: f32 = 20.0;
/// A drop-down's width on a page.
const COMBO_W: f32 = 190.0;
/// A number or text box's width on a page.
const FIELD_W: f32 = 90.0;
/// Why a greyed row is greyed.
const NOT_YET: &str = "Not in FissionOxide yet: kept at Fusion's default";

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
      // the title bar: the app's icon and "Preferences" at the left, an X at the right
      ui.horizontal(|ui| {
        let (icon, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
        crate::logo::paint(ui.painter(), icon);
        ui.label(RichText::new("Preferences").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          if close_button(ui).on_hover_text("Close without applying").clicked() {
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
          // the page's title in a bordered box across the top
          egui::Frame::NONE.stroke(Stroke::new(1.0, theme::BORDER)).inner_margin(egui::Margin::symmetric(8, 5)).show(
            ui,
            |ui| {
              ui.set_width(ui.available_width());
              ui.add(egui::Label::new(RichText::new(page.title()).strong()).wrap());
            },
          );
          ui.add_space(6.0);
          let rest = ui.available_height();
          egui::ScrollArea::vertical().id_salt(("prefs-page", page.key())).max_height(rest).show(ui, |ui| {
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
      close(app);
    }
    Some(Action::Cancel) => cancel(app),
    None => {}
  }
}

/// The title bar's X, drawn rather than taken from the font.
fn close_button(ui: &mut egui::Ui) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 20.0), Sense::click());
  if resp.hovered() {
    ui.painter().rect_filled(rect, 2.0, theme::HOVER);
  }
  let (c, d) = (rect.center(), 4.5);
  let stroke = Stroke::new(1.4, ui.visuals().text_color());
  ui.painter().line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
  ui.painter().line_segment([c + vec2(-d, d), c + vec2(d, -d)], stroke);
  ui.ctx().data_mut(|m| m.insert_temp(close_rect_id(), rect));
  resp
}

/// The ⓘ beside Restore Defaults and in the info boxes.
fn info_icon(ui: &mut egui::Ui) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
  let c = rect.center();
  let p = ui.painter();
  p.circle_stroke(c, 7.0, Stroke::new(1.2, theme::MUTED));
  p.text(c, egui::Align2::CENTER_CENTER, "i", egui::FontId::proportional(11.0), theme::MUTED);
  resp
}

/// A ▸ / ▾ expander for a branch of the tree, in `rect`.
fn paint_expander(painter: &egui::Painter, rect: egui::Rect, open: bool) {
  let c = rect.center();
  let pts = if open {
    [c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
  } else {
    [c + vec2(-2.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.0, 4.0)]
  };
  painter.add(egui::Shape::convex_polygon(pts.to_vec(), theme::MUTED, Stroke::NONE));
}

/// Opens or closes a branch of the tree.
pub(crate) fn toggle(prefs: &mut Preferences, page: Page) {
  if let Some(i) = prefs.expanded.iter().position(|p| *p == page) {
    prefs.expanded.remove(i);
  } else {
    prefs.expanded.push(page);
  }
}

/// Fusion's tree of pages: the shown one light blue with dark text, the hovered one grey; a
/// branch opens with its arrow or a double click.
fn tree(app: &mut App, ui: &mut egui::Ui) {
  ui.spacing_mut().item_spacing.y = 0.0;
  tree_nodes(app, ui, TREE, 0);
}

fn tree_nodes(app: &mut App, ui: &mut egui::Ui, nodes: &'static [Node], depth: usize) {
  for n in nodes {
    let open = app.preferences.expanded.contains(&n.page);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), TREE_ROW), Sense::click());
    let shown = app.preferences.page == n.page;
    if shown {
      ui.painter().rect_filled(rect, 0.0, theme::SELECTED);
    } else if resp.hovered() {
      ui.painter().rect_filled(rect, 0.0, theme::HOVER);
    }
    let x = rect.left() + 4.0 + depth as f32 * 14.0;
    let arrow = egui::Rect::from_min_size(pos2(x, rect.top()), vec2(12.0, TREE_ROW));
    if !n.children.is_empty() {
      paint_expander(ui.painter(), arrow, open);
    }
    let ink = if shown { theme::TEXT } else { ui.visuals().text_color() };
    ui.painter().with_clip_rect(rect).text(
      pos2(arrow.right() + 3.0, rect.center().y),
      egui::Align2::LEFT_CENTER,
      n.page.label(),
      egui::TextStyle::Body.resolve(ui.style()),
      ink,
    );
    ui.ctx().data_mut(|d| d.insert_temp(tree_row_id(n.page), rect));
    let on_arrow = resp.interact_pointer_pos().is_some_and(|p| p.x < arrow.right() + 1.0);
    if (resp.double_clicked() || (resp.clicked() && on_arrow)) && !n.children.is_empty() {
      toggle(&mut app.preferences, n.page);
    }
    if resp.clicked() && !on_arrow {
      app.preferences.page = n.page;
    }
    if open {
      tree_nodes(app, ui, n.children, depth + 1);
    }
  }
}

/// A page row: its label right-aligned against the control (left-aligned on the Electronics
/// page), wrapping when long.
fn row(ui: &mut egui::Ui, label_w: f32, label: &str, left: bool, control: impl FnOnce(&mut egui::Ui)) {
  ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing.x = 8.0;
    if left {
      ui.label(label);
    } else {
      let font = egui::TextStyle::Body.resolve(ui.style());
      let ink = ui.visuals().text_color();
      let mut job = egui::text::LayoutJob::simple(label.to_string(), font, ink, label_w);
      job.halign = egui::Align::RIGHT;
      let galley = ui.painter().layout_job(job);
      let (rect, _) = ui.allocate_exact_size(vec2(label_w, galley.size().y.max(ROW_H)), Sense::hover());
      ui.painter().galley(pos2(rect.right(), rect.center().y - galley.size().y / 2.0), galley, ink);
    }
    control(ui);
  });
}

/// A greyed drop-down showing `value`.
fn fixed_combo(ui: &mut egui::Ui, id: &str, value: &str, hover: &str) {
  ui.add_enabled_ui(false, |ui| {
    egui::ComboBox::from_id_salt(("prefs-fixed", id))
      .selected_text(value)
      .width(COMBO_W)
      .truncate()
      .show_ui(ui, |_| {})
      .response
      .on_disabled_hover_text(hover);
  });
}

/// A greyed box showing `value`.
fn fixed_field(ui: &mut egui::Ui, value: &str, width: f32, hover: &str) {
  let mut text = value.to_string();
  ui.add_enabled(false, egui::TextEdit::singleline(&mut text).desired_width(width)).on_disabled_hover_text(hover);
}

/// A row FissionOxide doesn't act on: its control greyed at the stored value.
fn fixed_control(ui: &mut egui::Ui, key: &str, item: &Item, value: &str) {
  match item {
    Item::Check(..) => {
      let mut on = value == "1";
      ui.add_enabled(false, egui::Checkbox::without_text(&mut on)).on_disabled_hover_text(NOT_YET);
    }
    Item::Combo(..) => fixed_combo(ui, key, value, NOT_YET),
    Item::Field(..) => fixed_field(ui, value, FIELD_W, NOT_YET),
    Item::Radio(_, _, choices) => {
      for c in choices.iter() {
        ui.add_enabled(false, egui::RadioButton::new(value == *c, *c)).on_disabled_hover_text(NOT_YET);
      }
    }
    Item::Combos(..) => {
      for (i, v) in value.split('|').enumerate() {
        if i > 0 {
          ui.label(":");
        }
        ui.add_enabled_ui(false, |ui| {
          egui::ComboBox::from_id_salt(("prefs-fixed", key, i))
            .selected_text(v)
            .width(110.0)
            .truncate()
            .show_ui(ui, |_| {})
            .response
            .on_disabled_hover_text(NOT_YET);
        });
      }
    }
    Item::Heading(_) | Item::Live(..) => {}
  }
}

/// Link-styled text; Fusion's links open Autodesk's pages, which FissionOxide has none of.
fn link(ui: &mut egui::Ui, text: &str) {
  ui.add(egui::Label::new(RichText::new(text).color(theme::ACCENT).underline()).sense(Sense::hover()))
    .on_hover_text("Opens an Autodesk page in Fusion; FissionOxide has none");
}

/// A light box with an ⓘ and text, as Fusion's info boxes.
fn info_box(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
  egui::Frame::NONE
    .fill(theme::SELECTED.gamma_multiply(0.45))
    .stroke(Stroke::new(1.0, theme::SELECTED))
    .inner_margin(egui::Margin::same(6))
    .show(ui, |ui| {
      ui.set_width(ui.available_width());
      ui.horizontal_top(|ui| {
        info_icon(ui);
        ui.horizontal_wrapped(|ui| {
          ui.spacing_mut().item_spacing.x = 0.0;
          add(ui);
        });
      });
    });
}

/// A page's rows (see [`pages::items`]).
fn rows(app: &App, ui: &mut egui::Ui, page: Page, p: &mut Prefs) {
  let items = pages::items(page);
  if items.is_empty() {
    return;
  }
  let left = pages::left_aligned(page);
  let font = egui::TextStyle::Body.resolve(ui.style());
  let widest = items
    .iter()
    .filter_map(Item::label)
    .map(|l| ui.painter().layout_no_wrap(l.to_string(), font.clone(), theme::TEXT).size().x)
    .fold(0.0, f32::max);
  let label_w = (widest + 2.0).min(ui.available_width() * 0.62);
  let mut heading = "";
  ui.spacing_mut().item_spacing.y = 5.0;
  for item in items {
    match item {
      Item::Heading(h) => {
        heading = h;
        ui.add_space(4.0);
        ui.label(RichText::new(*h).strong());
      }
      Item::Live(label, live) => {
        row(ui, label_w, label, left, |ui| live_row(app, ui, *live, p));
        if *live == Live::GraphicsPreset {
          info_box(ui, |ui| {
            ui.label(
              "Select a graphics preset to quickly change the related settings. Changes to Quality or Performance \
               presets will override the Custom preset.",
            );
          });
        }
      }
      _ => {
        let label = item.label().unwrap_or_default();
        let key = pages::stored_key(page, heading, label);
        let value = p.stored_value(&key, item);
        row(ui, label_w, label, left, |ui| fixed_control(ui, &key, item, &value));
      }
    }
  }
}

/// A row FissionOxide acts on.
fn live_row(app: &App, ui: &mut egui::Ui, live: Live, p: &mut Prefs) {
  use fox_engine::model::LengthUnit;
  let length_unit = |u: Units| LengthUnit::parse(u.key()).unwrap_or_default();
  match live {
    Live::Theme => {
      choice(ui, "prefs-theme", &mut p.theme, &Theme::ALL, Theme::label);
    }
    Live::GraphicsDriver => {
      let shown = if p.gpu { p.driver.label() } else { CPU_DRIVER };
      let r = egui::ComboBox::from_id_salt("prefs-driver").selected_text(shown).width(COMBO_W).show_ui(ui, |ui| {
        if ui.selectable_label(p.gpu && p.driver == Driver::Auto, Driver::Auto.label()).clicked() {
          (p.gpu, p.driver) = (true, Driver::Auto);
        }
        for dx in ["DirectX 11", "DirectX 9"] {
          ui.add_enabled(false, egui::Button::selectable(false, dx))
            .on_disabled_hover_text("FissionOxide draws with OpenGL");
        }
        if ui.selectable_label(p.gpu && p.driver == Driver::OpenGl, Driver::OpenGl.label()).clicked() {
          (p.gpu, p.driver) = (true, Driver::OpenGl);
        }
        ui.separator();
        if ui
          .selectable_label(!p.gpu, CPU_DRIVER)
          .on_hover_text("FissionOxide's CPU renderer draws the canvas")
          .clicked()
        {
          p.gpu = false;
        }
      });
      ui.ctx().data_mut(|d| d.insert_temp(combo_rect_id("prefs-driver"), r.response.rect));
      match &app.gpu.failed {
        Some(why) => {
          r.response.on_hover_text(format!("OpenGL is unavailable: {why}. The CPU renderer draws the canvas."))
        }
        None => r.response.on_hover_text("FOX_RENDERER=cpu picks the CPU renderer at start whatever this says"),
      };
    }
    Live::Recovery => {
      egui::ComboBox::from_id_salt("prefs-recovery")
        .selected_text(recovery_label(p.recovery_minutes))
        .width(COMBO_W)
        .show_ui(ui, |ui| {
          for m in RECOVERY_CHOICES {
            ui.selectable_value(&mut p.recovery_minutes, m, recovery_label(m));
          }
        });
    }
    Live::UpAxis => {
      choice(ui, "prefs-up", &mut p.up_axis, &UpAxis::ALL, UpAxis::label);
    }
    Live::Tooltips => {
      ui.checkbox(&mut p.show_tooltips, "").on_hover_text("Hover texts on the toolbar and the controls");
    }
    Live::Prompt => {
      ui.checkbox(&mut p.show_prompt, "").on_hover_text("The command's prompt at the cursor");
    }
    Live::MouseSetup => {
      choice(ui, "prefs-nav-preset", &mut p.nav, &NavPreset::ALL, NavPreset::label);
    }
    Live::OrbitType => {
      choice(ui, "prefs-orbit", &mut p.orbit, &OrbitType::ALL, OrbitType::label);
    }
    Live::ReverseZoom => {
      ui.checkbox(&mut p.reverse_zoom, "").on_hover_text("Off, as in Fusion: rolling the wheel toward you zooms in");
    }
    Live::CameraPivot => {
      ui.checkbox(&mut p.camera_pivot, "").on_hover_text("Shows the orbit's pivot while orbiting");
    }
    Live::CubeSize => {
      choice(ui, "prefs-cube", &mut p.cube_size, &CubeSize::ALL, CubeSize::label);
    }
    Live::SpaceRepeats => {
      ui.checkbox(&mut p.space_repeats, "");
    }
    Live::ModelingMode => {
      let mut parametric = p.capture_history;
      choice(ui, "prefs-modeling-mode", &mut parametric, &[true, false], |v| {
        if v { "Parametric Modeling" } else { "Direct Modeling" }
      });
      p.capture_history = parametric;
    }
    Live::AutoLookAt => {
      choice(
        ui,
        "prefs-look-at",
        &mut p.auto_look_at,
        &[true, false],
        |v| if v { "Always Orthographic" } else { "Off" },
      );
    }
    Live::GraphicsPreset => {
      use crate::navbar::{DisplaySettings, GraphicsPreset};
      let mut preset = DisplaySettings::preset_for(p.effects);
      if choice(ui, "prefs-graphics-preset", &mut preset, &GraphicsPreset::ALL, GraphicsPreset::label)
        && let Some(e) = preset.effects()
      {
        p.effects = e;
      }
    }
    Live::Effect(i) => {
      ui.checkbox(effect(&mut p.effects, i), "");
    }
    Live::DesignUnits => {
      choice(ui, "prefs-units", &mut p.units, &Units::ALL, Units::key);
    }
    Live::DesignLength => fixed_combo(ui, "design-length", p.units.key(), "Follows the default units for new design"),
    Live::DesignMass => {
      fixed_combo(ui, "design-mass", length_unit(p.units).mass_symbol(), "Follows the default units for new design")
    }
    Live::GeneralPrecision => {
      let unit_default = crate::units::unit_decimals(length_unit(p.units)) as u8;
      let shown = p.general_precision.unwrap_or(unit_default);
      let r = egui::ComboBox::from_id_salt("prefs-general-precision")
        .selected_text(precision_label(shown))
        .width(COMBO_W)
        .show_ui(ui, |ui| {
          for d in PRECISIONS {
            if ui.selectable_label(shown == d, precision_label(d)).clicked() {
              p.general_precision = Some(d);
            }
          }
        });
      ui.ctx().data_mut(|d| d.insert_temp(combo_rect_id("prefs-general-precision"), r.response.rect));
    }
    Live::AngularPrecision => {
      let r = egui::ComboBox::from_id_salt("prefs-angular-precision")
        .selected_text(precision_label(p.angular_precision))
        .width(COMBO_W)
        .show_ui(ui, |ui| {
          for d in PRECISIONS {
            ui.selectable_value(&mut p.angular_precision, d, precision_label(d));
          }
        });
      ui.ctx().data_mut(|d| d.insert_temp(combo_rect_id("prefs-angular-precision"), r.response.rect));
    }
    Live::RenderEnvironment => {
      egui::ComboBox::from_id_salt("prefs-render-env")
        .selected_text(p.render.environment.as_str())
        .width(COMBO_W)
        .show_ui(ui, |ui| {
          for (name, _) in crate::render_panels::LIBRARY {
            if ui.selectable_label(p.render.environment == name, name).clicked() {
              p.render.environment = name.to_string();
            }
          }
        });
    }
    Live::RenderBrightness => {
      let max = 4.0 * crate::render_panels::LUX;
      ui.add(egui::DragValue::new(&mut p.render.brightness).range(0.0..=max).fixed_decimals(2));
    }
    Live::RenderBackground => {
      choice(ui, "prefs-render-bg", &mut p.render.solid_background, &[true, false], |v| {
        if v { "Solid Color" } else { "Environment" }
      });
    }
    Live::RenderColor => {
      ui.add_enabled_ui(p.render.solid_background, |ui| {
        for c in p.render.color.iter_mut() {
          ui.add(egui::DragValue::new(c).range(0..=255));
        }
      });
    }
    Live::RenderGround => {
      ui.checkbox(&mut p.render.ground, "");
    }
    Live::RenderReflections => {
      ui.add_enabled(p.render.ground, egui::Checkbox::without_text(&mut p.render.reflections));
    }
    Live::MaterialLibrary => fixed_combo(ui, "material-library", crate::material_ui::MATERIAL_LIBRARY, NOT_YET),
    Live::MaterialCategory => fixed_combo(
      ui,
      "material-category",
      fox_engine::model::assembly::material_folder(&p.material),
      "Follows the material",
    ),
    Live::MaterialName => {
      egui::ComboBox::from_id_salt("prefs-material").selected_text(p.material.as_str()).width(COMBO_W).show_ui(
        ui,
        |ui| {
          for (name, _) in fox_engine::model::assembly::MATERIALS {
            if ui.selectable_label(p.material == *name, *name).clicked() {
              p.material = name.to_string();
            }
          }
        },
      );
    }
    Live::ApiPath => {
      let path = app.hub_root().join("plugins");
      fixed_field(ui, &path.display().to_string(), 220.0, "Where FissionOxide's add-ins are installed");
      ui.add_enabled(false, egui::Button::new("...")).on_disabled_hover_text(NOT_YET);
    }
    Live::McpPort => {
      fixed_field(ui, "27182", FIELD_W, NOT_YET);
      ui.label(RichText::new("MCP server is not running").color(theme::MUTED));
      let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
      let ink = Stroke::new(1.0, theme::MUTED);
      let back = egui::Rect::from_min_size(rect.min + vec2(2.0, 2.0), vec2(8.0, 9.0));
      ui.painter().rect_stroke(back, 1.0, ink, egui::StrokeKind::Inside);
      ui.painter().rect_stroke(back.translate(vec2(4.0, 3.0)), 1.0, ink, egui::StrokeKind::Inside);
      resp.on_hover_text(NOT_YET);
    }
  }
}

/// The Graphics driver list's entry for FissionOxide's CPU renderer.
const CPU_DRIVER: &str = "CPU (software)";

fn page_ui(app: &mut App, ui: &mut egui::Ui) {
  let page = app.preferences.page;
  let mut values = app.preferences.draft.clone();
  match page {
    Page::DefaultUnits => {
      ui.label(RichText::new("Select a sub page under the Default Units control").strong());
    }
    Page::DataCollection => data_collection(ui, &values),
    Page::Tokens => tokens(ui),
    Page::PreviewFeatures => preview_features(ui),
    _ => {}
  }
  if page != Page::DataCollection {
    rows(app, ui, page, &mut values);
  }
  if page == Page::General {
    ui.add_space(6.0);
    note(ui, "The orientation is saved, but the view stays Z up until the up axis can be changed.");
  }
  app.preferences.draft = values;
}

/// Data Collection and Use: Fusion's text and its three cards.
fn data_collection(ui: &mut egui::Ui, p: &Prefs) {
  ui.label(RichText::new("Improve your Fusion experience").strong());
  ui.horizontal_wrapped(|ui| {
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.label("Autodesk uses data about how you use its products to improve them. To learn more, visit the ");
    link(ui, "analytics programs page");
    ui.label(".");
  });
  ui.horizontal_wrapped(|ui| {
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.label("Your choices below apply to this device. Read the ");
    link(ui, "Privacy Statement");
    ui.label(" for how the data is used.");
  });
  ui.add_space(4.0);
  let cards = [
    ("Required data collection", "Data needed to run, secure and support the product."),
    ("Help develop new products and services", "Use data about how you work to shape new features."),
    ("Personalize our offers", "Use data about how you work to tailor messages and offers."),
  ];
  for (lead, text) in cards {
    let item = Item::Check(lead, false);
    let key = pages::stored_key(Page::DataCollection, "", lead);
    let default = pages::items(Page::DataCollection).iter().find(|i| i.label() == Some(lead)).copied().unwrap_or(item);
    let mut on = p.stored_value(&key, &default) == "1";
    egui::Frame::NONE.fill(theme::PANEL).inner_margin(egui::Margin::same(8)).corner_radius(3.0).show(ui, |ui| {
      ui.set_width(ui.available_width());
      ui.horizontal_top(|ui| {
        ui.add_enabled(false, egui::Checkbox::without_text(&mut on)).on_disabled_hover_text(NOT_YET);
        ui.horizontal_wrapped(|ui| {
          ui.spacing_mut().item_spacing.x = 0.0;
          ui.label(RichText::new(format!("{lead}. ")).strong());
          ui.label(text);
        });
      });
    });
    ui.add_space(4.0);
  }
}

/// Tokens: what they are, the account and task tables; the page's check box follows.
fn tokens(ui: &mut egui::Ui) {
  ui.horizontal_wrapped(|ui| {
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.label(RichText::new("What are tokens? ").strong());
    ui.label("Tokens pay for cloud tasks such as simulation solves. ");
    link(ui, "Learn More");
  });
  ui.add_space(4.0);
  let table = |ui: &mut egui::Ui, id: &str, head: &[&str], rows: &[&[&str]]| {
    egui::Frame::NONE.stroke(Stroke::new(1.0, theme::BORDER)).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
      ui.set_width(ui.available_width());
      egui::Grid::new(id).num_columns(head.len()).min_col_width(120.0).striped(true).show(ui, |ui| {
        for h in head {
          ui.label(RichText::new(*h).strong());
        }
        ui.end_row();
        for r in rows {
          for c in r.iter() {
            ui.label(RichText::new(*c).color(theme::MUTED));
          }
          ui.end_row();
        }
      });
    });
  };
  table(ui, "prefs-token-accounts", &["Account", "Balance", "Type"], &[&["—", "—", "—"]]);
  ui.add_space(4.0);
  egui::Frame::NONE.stroke(Stroke::new(1.0, theme::BORDER)).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
    ui.set_width(ui.available_width());
    egui::Grid::new("prefs-token-tasks").num_columns(2).min_col_width(120.0).show(ui, |ui| {
      ui.label(RichText::new("Task").strong());
      ui.label(RichText::new("Account to use").strong());
      ui.end_row();
      ui.label("Simulation tasks");
      ui.add_enabled(false, egui::Checkbox::without_text(&mut false)).on_disabled_hover_text(NOT_YET);
      ui.end_row();
    });
  });
  ui.add_space(6.0);
}

/// Preview Features: the Insider box, the terms, the filter bar and the (empty) list.
fn preview_features(ui: &mut egui::Ui) {
  info_box(ui, |ui| {
    ui.label("Want to try features before they are released? Join the ");
    link(ui, "Fusion Insider Program");
    ui.label(".");
  });
  ui.add_space(4.0);
  ui.horizontal_wrapped(|ui| {
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.label("Preview features are still in development. Turning one on is subject to the ");
    link(ui, "Autodesk Terms of Use");
    ui.label(".");
  });
  ui.add_space(6.0);
  ui.horizontal(|ui| {
    fixed_combo(ui, "preview-workspaces", "All workspaces", NOT_YET);
    for glyph in 0..3 {
      let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 20.0), Sense::hover());
      paint_filter_glyph(ui.painter(), rect, glyph);
      resp.on_hover_text(["Shown features", "Locked features", "Turned-on features"][glyph]);
    }
    let mut search = String::new();
    ui.add_enabled(false, egui::TextEdit::singleline(&mut search).hint_text("Search").desired_width(120.0));
    link(ui, "Reset filters");
  });
  ui.add_space(6.0);
  note(ui, "FissionOxide has no preview features.");
}

/// The Preview Features filter bar's toggles: an eye, a lock and a tick.
fn paint_filter_glyph(p: &egui::Painter, rect: egui::Rect, which: usize) {
  let c = rect.center();
  let ink = Stroke::new(1.3, theme::MUTED);
  match which {
    0 => {
      let pts: Vec<egui::Pos2> = (0..=16)
        .map(|i| {
          let t = std::f32::consts::TAU * i as f32 / 16.0;
          c + vec2(6.5 * t.cos(), 3.5 * t.sin())
        })
        .collect();
      p.add(egui::Shape::line(pts, ink));
      p.circle_filled(c, 1.8, theme::MUTED);
    }
    1 => {
      let body = egui::Rect::from_center_size(c + vec2(0.0, 2.0), vec2(9.0, 7.0));
      p.rect_stroke(body, 1.0, ink, egui::StrokeKind::Inside);
      let arc: Vec<egui::Pos2> = (0..=8)
        .map(|i| {
          let t = std::f32::consts::PI * i as f32 / 8.0;
          c + vec2(-3.0 * t.cos(), -1.5 - 3.5 * t.sin())
        })
        .collect();
      p.add(egui::Shape::line(arc, ink));
    }
    _ => {
      p.add(egui::Shape::line(vec![c + vec2(-4.5, 0.0), c + vec2(-1.5, 3.0), c + vec2(4.5, -3.5)], ink));
    }
  }
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
      driver: Driver::OpenGl,
      effects: Effects { environment_dome: true, ground_shadow: false, ..crate::navbar::default_effects() },
      general_precision: Some(4),
      angular_precision: 3,
      render: RenderDefaults {
        environment: "Soft Light".into(),
        brightness: 1500.0,
        solid_background: false,
        color: [10, 20, 30],
        ground: false,
        reflections: true,
      },
      material: "Brass".into(),
      stored: [("Fusion.Electronics.PCB.Theme".to_string(), "Light".to_string())].into_iter().collect(),
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
    let top: Vec<&str> = TREE.iter().map(|n| n.page.label()).collect();
    assert_eq!(top[0], "General");
    assert_eq!(top.last().copied(), Some("Compatibility & Troubleshooting"));
    assert_eq!(TREE[0].children.first().map(|n| n.page.label()), Some("API"));
    assert_eq!(Page::General.title(), "Preferences controlling general UI behavior");
  }

  /// The labels of a node's children.
  fn children(page: Page) -> Vec<&'static str> {
    fn find(nodes: &'static [Node], page: Page) -> Option<&'static Node> {
      nodes.iter().find_map(|n| if n.page == page { Some(n) } else { find(n.children, page) })
    }
    find(TREE, page).expect("in the tree").children.iter().map(|n| n.page.label()).collect()
  }

  #[test]
  fn the_tree_has_fusions_nodes_in_its_order() {
    let top: Vec<&str> = TREE.iter().map(|n| n.page.label()).collect();
    assert_eq!(
      top,
      [
        "General",
        "Tokens",
        "Material",
        "Graphics",
        "Network",
        "Data Collection and Use",
        "Unit and Value Display",
        "Default Units",
        "Preview Features",
        "Compatibility & Troubleshooting"
      ]
    );
    assert_eq!(
      children(Page::General),
      ["API", "Design", "Manufacture", "Electronics", "Render", "Drawing", "Simulation and Generative Design"]
    );
    assert_eq!(children(Page::Design), ["Assembly and Hybrid Design", "Configurations"]);
    assert_eq!(children(Page::Manufacture), ["Optional Features"]);
    assert_eq!(
      children(Page::Electronics),
      ["Color", "Misc", "Default Grid and Units", "Default Values", "Drill Legend", "Directory", "3D PCB"]
    );
    assert_eq!(children(Page::Graphics), ["RDP Optimization"]);
    assert_eq!(children(Page::UnitDisplay), ["Simulation and Generative Design"]);
    assert_eq!(children(Page::DefaultUnits), ["Manufacture", "Design", "Simulation and Generative Design"]);
    assert_eq!(ancestors(Page::ElecDrill), [Page::General, Page::Electronics]);
    assert_eq!(ancestors(Page::Tokens), []);
    // every page is in the tree once, and its saved name reads back as it
    let mut seen = Vec::new();
    fn walk(nodes: &'static [Node], seen: &mut Vec<Page>) {
      for n in nodes {
        seen.push(n.page);
        walk(n.children, seen);
      }
    }
    walk(TREE, &mut seen);
    assert_eq!(seen.len(), Page::ALL.len());
    for p in Page::ALL {
      assert!(seen.contains(&p), "{p:?} in the tree");
      assert_eq!(Page::parse(p.key()), Some(p));
    }
    // the tree starts collapsed
    assert!(Preferences::default().expanded.is_empty());
  }

  #[test]
  fn pages_carry_fusions_titles_and_rows() {
    assert_eq!(Page::Design.title(), "Preferences controlling general Design behavior");
    assert_eq!(Page::Graphics.title(), "Preferences used to control the graphics display");
    assert_eq!(Page::UnitDisplay.title(), "Preferences that control how units and values are displayed");
    assert_eq!(Page::Api.title(), "Preferences for Scripting and Programming");
    assert!(Page::Compatibility.title().starts_with("Miscellaneous options which may enable Fusion"));
    let labels = |p: Page| pages::items(p).iter().filter_map(Item::label).collect::<Vec<_>>();
    // Design: Fusion's rows, no Default units (that is Default Units > Design)
    let design = labels(Page::Design);
    assert_eq!(design[0], "Default Modeling Mode");
    assert!(design.contains(&"Auto look at sketch") && !design.contains(&"Default units"));
    assert_eq!(labels(Page::Graphics)[0], "Graphics Preset");
    assert!(!labels(Page::Graphics).iter().any(|l| l.contains("ViewCube")), "ViewCube settings are on General");
    // General: Fusion's rows in its order, the ten missing ones included
    let general = labels(Page::General);
    let at = |l: &str| general.iter().position(|x| *x == l).unwrap_or_else(|| panic!("{l} on General"));
    assert!(at("Theme") < at("Graphics driver") && at("Graphics driver") < at("Start-up experience"));
    assert!(at("Automatic save on close") < at("Automatic recovery backup interval"));
    assert!(at("Show command prompt") < at("Show default measure"));
    assert!(at("Show in-command errors and warnings") < at("Mouse setup (pan, zoom, orbit)"));
    for l in ["Offline cache period", "Purpose for using Fusion", "Default design type"] {
      at(l);
    }
    assert_eq!(labels(Page::DefaultUnitsDesign), ["Default units for new design", "Length", "Mass"]);
    assert_eq!(labels(Page::UnitDisplay)[..2], ["General precision", "Angular precision"]);
    // each stored row has its own key
    let rows = pages::stored_rows();
    let mut keys: Vec<&str> = rows.iter().map(|(k, _)| k.as_str()).collect();
    keys.sort();
    let n = keys.len();
    keys.dedup();
    assert_eq!(keys.len(), n, "duplicate keys");
    assert!(rows.iter().any(|(k, _)| k == "Fusion.Electronics.Schematic.Theme"));
    assert!(rows.iter().any(|(k, _)| k == "Fusion.Electronics.PCB.Theme"));
  }

  #[test]
  fn stored_rows_keep_their_values_and_fall_back_to_fusions() {
    let p = Prefs::default();
    let key = pages::stored_key(Page::Design, "", "Default Standard for PMI");
    let item = Item::Combo("Default Standard for PMI", "Infer from Units");
    assert_eq!(p.stored_value(&key, &item), "Infer from Units");
    let back =
      Prefs::from_pairs(&[(key.to_ascii_lowercase(), "ASME".into()), ("Fusion.Nowhere.Thing".into(), "1".into())]);
    assert_eq!(back.stored_value(&key, &item), "ASME", "read back under its own key");
    assert_eq!(back.stored.len(), 1, "unknown rows aren't kept");
    // Fusion's defaults aren't written
    assert!(!Prefs::default().to_pairs().iter().any(|(k, _)| k.starts_with("Fusion.")));
  }

  #[test]
  fn precision_drives_the_readouts() {
    use fox_engine::model::LengthUnit;
    let mut app = App::with_session(fox_engine::Session::default());
    apply(&mut app, None);
    assert_eq!(crate::viewport::display_decimals(LengthUnit::Mm), 2, "the unit's own by default");
    assert_eq!(crate::viewport::display_decimals(LengthUnit::In), 3);
    assert_eq!(crate::units::angle_decimals(), 1);
    assert_eq!(precision_label(0), "0");
    assert_eq!(precision_label(3), "0.123");
    open(&mut app);
    app.preferences.draft.general_precision = Some(4);
    app.preferences.draft.angular_precision = 2;
    commit(&mut app, None);
    assert_eq!(crate::viewport::display_decimals(LengthUnit::Mm), 4);
    assert_eq!(crate::viewport::display_decimals(LengthUnit::In), 4);
    assert_eq!(crate::units::angle_decimals(), 2);
    app.preferences.draft = Prefs::default();
    commit(&mut app, None);
    assert_eq!(crate::viewport::display_decimals(LengthUnit::In), 3);
  }

  #[test]
  fn canvas_effects_and_the_graphics_preset_set_the_display() {
    let mut app = App::with_session(fox_engine::Session::default());
    assert_eq!(app.effects, crate::navbar::default_effects());
    open(&mut app);
    app.preferences.draft.effects.object_shadow = true;
    assert!(!app.effects.object_shadow, "not before OK or Apply");
    commit(&mut app, None);
    assert!(app.effects.object_shadow);
    // the preset reads from the effects, and picking one sets them
    use crate::navbar::{DisplaySettings, GraphicsPreset};
    assert_eq!(DisplaySettings::preset_for(Prefs::default().effects), GraphicsPreset::Custom);
    // the effects are saved
    let back = Prefs::from_pairs(&app.preferences.values.to_pairs());
    assert!(back.effects.object_shadow && back.effects.ground_plane);
  }

  #[test]
  fn new_designs_take_the_render_and_material_defaults() {
    let mut app = App::with_session(fox_engine::Session::default());
    app.new_design();
    let env = &app.s.design.render.environment;
    assert_eq!(env.background, Background::Color, "Fusion's solid background");
    assert_eq!(env.background_color, [178, 178, 178]);
    assert_eq!(app.s.design.root_material(), "Steel");
    assert!(!app.s.can_undo() && !app.s.dirty, "the defaults aren't undo steps");
    app.preferences.values.render.environment = "Soft Light".into();
    app.preferences.values.render.brightness = 2000.0;
    app.preferences.values.material = "Aluminum".into();
    app.new_design();
    let env = &app.s.design.render.environment;
    assert_eq!(env.preset, fox_engine::rendering::raytrace::Preset::Soft);
    assert!((env.intensity - 2.0).abs() < 1e-6);
    assert_eq!(app.s.design.material, "Aluminum");
    assert!(!app.s.can_undo() && !app.s.dirty);
  }

  #[test]
  fn the_window_reopens_on_the_page_shown_last() {
    let dir = std::env::temp_dir().join(format!("fox-prefs-page-{}", std::process::id()));
    let path = dir.join("settings.ini");
    let mut app = App::with_session(fox_engine::Session::default());
    app.preferences = Preferences::load_from(path.clone());
    assert_eq!(app.preferences.page, Page::General);
    open(&mut app);
    app.preferences.page = Page::ElecDrill;
    cancel(&mut app);
    let mut again = App::with_session(fox_engine::Session::default());
    again.preferences = Preferences::load_from(path);
    assert_eq!(again.preferences.page, Page::ElecDrill);
    open(&mut again);
    assert!(
      again.preferences.expanded.contains(&Page::General) && again.preferences.expanded.contains(&Page::Electronics)
    );
    let _ = std::fs::remove_dir_all(dir);
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
    assert_eq!(f.action(Secondary, none), None, "right drags are marking gestures");
    assert_eq!(f.action(Secondary, shift), None);
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
