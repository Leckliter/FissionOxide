//! The Preferences dialog, as in Fusion (profile menu or File → Preferences): General, Navigation,
//! Design, Sketch and Graphics pages. The values live in the `[Preferences]` section of
//! `settings.ini` (see [`fox_desktop::settings`]), written whenever one changes.

use std::path::PathBuf;

use egui::{PointerButton, RichText};

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
  Y,
  #[default]
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
}

/// Modifier keys a mapping needs held: (shift, ctrl, alt).
type Held = (bool, bool, bool);

impl NavPreset {
  pub const ALL: [NavPreset; 4] = [NavPreset::Fusion, NavPreset::SolidWorks, NavPreset::Inventor, NavPreset::Alias];

  pub fn key(self) -> &'static str {
    match self {
      NavPreset::Fusion => "fusion",
      NavPreset::SolidWorks => "solidworks",
      NavPreset::Inventor => "inventor",
      NavPreset::Alias => "alias",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      NavPreset::Fusion => "Fusion",
      NavPreset::SolidWorks => "SolidWorks",
      NavPreset::Inventor => "Inventor",
      NavPreset::Alias => "Alias",
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
    }
  }

  /// What a drag started with `button` and `mods` held does, if anything.
  pub fn action(self, button: PointerButton, mods: egui::Modifiers) -> Option<NavAction> {
    let held = (mods.shift, mods.ctrl || mods.command, mods.alt);
    self.mappings().iter().find(|(b, h, _)| *b == button && *h == held).map(|m| m.2)
  }

  /// "Middle drag", "Shift+Middle drag" … for the action, as the Preferences page lists them.
  pub fn describe(self, action: NavAction) -> String {
    let list: Vec<String> = self
      .mappings()
      .iter()
      .filter(|m| m.2 == action)
      .map(|(b, (shift, ctrl, alt), _)| {
        let mut s = String::new();
        for (on, name) in [(*shift, "Shift+"), (*ctrl, "Ctrl+"), (*alt, "Alt+")] {
          if on {
            s.push_str(name);
          }
        }
        s.push_str(match b {
          PointerButton::Primary => "Left",
          PointerButton::Secondary => "Right",
          _ => "Middle",
        });
        s + " drag"
      })
      .collect();
    let mut out = list.join(", ");
    if action == NavAction::Zoom {
      out = if out.is_empty() { "Wheel".into() } else { format!("Wheel, {out}") };
    }
    out
  }
}

/// The saved preferences.
#[derive(Debug, Clone, PartialEq)]
pub struct Prefs {
  pub units: Units,
  /// Stored, not yet applied: which axis is up needs a decision on the camera and the ViewCube.
  pub up_axis: UpAxis,
  pub nav: NavPreset,
  /// Wheel forward zooms out (and dragging down zooms in).
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
}

impl Default for Prefs {
  fn default() -> Self {
    Prefs {
      units: Units::Mm,
      up_axis: UpAxis::Z,
      nav: NavPreset::Fusion,
      reverse_zoom: false,
      zoom_to_cursor: true,
      capture_history: true,
      auto_look_at: true,
      show_dims: true,
      show_constraints: true,
      snap: false,
      gpu: true,
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
      ("ReverseZoom", flag(self.reverse_zoom)),
      ("ZoomToCursor", flag(self.zoom_to_cursor)),
      ("CaptureHistory", flag(self.capture_history)),
      ("SketchLookAt", flag(self.auto_look_at)),
      ("SketchDimensions", flag(self.show_dims)),
      ("SketchConstraints", flag(self.show_constraints)),
      ("SketchSnap", flag(self.snap)),
      ("GpuRenderer", flag(self.gpu)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
  }

  /// Reads the pairs of a `[Preferences]` section; missing or unknown values keep the defaults.
  pub fn from_pairs(pairs: &[(String, String)]) -> Prefs {
    let mut p = Prefs::default();
    for (k, v) in pairs {
      let b = parse_flag(v);
      match k.to_ascii_lowercase().as_str() {
        "units" => p.units = pick(&Units::ALL, Units::key, v).unwrap_or(p.units),
        "upaxis" => p.up_axis = pick(&UpAxis::ALL, UpAxis::key, v).unwrap_or(p.up_axis),
        "navigation" => p.nav = pick(&NavPreset::ALL, NavPreset::key, v).unwrap_or(p.nav),
        "reversezoom" => p.reverse_zoom = b.unwrap_or(p.reverse_zoom),
        "zoomtocursor" => p.zoom_to_cursor = b.unwrap_or(p.zoom_to_cursor),
        "capturehistory" => p.capture_history = b.unwrap_or(p.capture_history),
        "sketchlookat" => p.auto_look_at = b.unwrap_or(p.auto_look_at),
        "sketchdimensions" => p.show_dims = b.unwrap_or(p.show_dims),
        "sketchconstraints" => p.show_constraints = b.unwrap_or(p.show_constraints),
        "sketchsnap" => p.snap = b.unwrap_or(p.snap),
        "gpurenderer" => p.gpu = b.unwrap_or(p.gpu),
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

/// Pages of the dialog, in Fusion's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
  #[default]
  General,
  Navigation,
  Design,
  Sketch,
  Graphics,
}

impl Page {
  const ALL: [Page; 5] = [Page::General, Page::Navigation, Page::Design, Page::Sketch, Page::Graphics];

  fn label(self) -> &'static str {
    match self {
      Page::General => "General",
      Page::Navigation => "Navigation",
      Page::Design => "Design",
      Page::Sketch => "Sketch",
      Page::Graphics => "Graphics",
    }
  }
}

/// The preferences, where they are saved and the dialog's state.
#[derive(Debug, Default)]
pub struct Preferences {
  pub values: Prefs,
  /// `settings.ini`; `None` keeps them in memory (tests).
  path: Option<PathBuf>,
  pub open: bool,
  page: Page,
}

impl Preferences {
  /// The user's preferences from `settings.ini`.
  pub fn load() -> Preferences {
    Self::load_from(fox_desktop::settings::Settings::path())
  }

  pub fn load_from(path: PathBuf) -> Preferences {
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    Preferences { values: Prefs::parse(&text), path: Some(path), ..Preferences::default() }
  }

  pub fn save(&self) {
    if let Some(path) = &self.path
      && let Err(e) = fox_desktop::settings::save_section(path, SECTION, &self.values.to_pairs())
    {
      log::warn!("couldn't save {}: {e}", path.display());
    }
  }
}

/// Puts the saved defaults into the app (at start).
pub fn apply(app: &mut App) {
  let p = app.prefs.values.clone();
  app.show_dims = p.show_dims;
  app.show_constraints = p.show_constraints;
  app.snap = p.snap;
  if !p.gpu {
    app.gpu.enabled = false;
  }
}

/// Opens the Preferences dialog.
pub fn open(app: &mut App) {
  app.prefs.open = true;
}

fn choice<T: Copy + PartialEq>(
  ui: &mut egui::Ui,
  id: &str,
  value: &mut T,
  all: &[T],
  label: impl Fn(T) -> &'static str,
) -> bool {
  let mut changed = false;
  egui::ComboBox::from_id_salt(id).selected_text(label(*value)).width(170.0).show_ui(ui, |ui| {
    for v in all {
      changed |= ui.selectable_value(value, *v, label(*v)).changed();
    }
  });
  changed
}

fn note(ui: &mut egui::Ui, text: &str) {
  ui.label(RichText::new(text).small().color(theme::MUTED));
}

/// The dialog, while open.
pub fn window(app: &mut App, ctx: &egui::Context) {
  if !app.prefs.open {
    return;
  }
  let mut open = true;
  let before = app.prefs.values.clone();
  egui::Window::new("Preferences").open(&mut open).collapsible(false).resizable(false).default_width(560.0).show(
    ctx,
    |ui| {
      ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
          ui.set_width(120.0);
          for p in Page::ALL {
            if ui.selectable_label(app.prefs.page == p, p.label()).clicked() {
              app.prefs.page = p;
            }
          }
        });
        ui.separator();
        ui.vertical(|ui| {
          ui.set_min_width(380.0);
          ui.set_min_height(260.0);
          ui.heading(app.prefs.page.label());
          ui.add_space(6.0);
          page(app, ui);
        });
      });
      ui.separator();
      ui.horizontal(|ui| {
        if ui.button("Restore Defaults").on_hover_text("Every page back to its default").clicked() {
          app.prefs.values = Prefs::default();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          if ui.button("Close").clicked() {
            app.prefs.open = false;
          }
        });
      });
    },
  );
  if !open {
    app.prefs.open = false;
  }
  let now = app.prefs.values.clone();
  if now != before {
    // the sketch defaults and the renderer take effect at once, as in Fusion
    if now.show_dims != before.show_dims {
      app.show_dims = now.show_dims;
    }
    if now.show_constraints != before.show_constraints {
      app.show_constraints = now.show_constraints;
    }
    if now.snap != before.snap {
      app.snap = now.snap;
    }
    if now.gpu != before.gpu {
      app.gpu.enabled = now.gpu;
    }
    app.prefs.save();
  }
}

fn page(app: &mut App, ui: &mut egui::Ui) {
  let mut values = app.prefs.values.clone();
  let p = &mut values;
  match app.prefs.page {
    Page::General => {
      egui::Grid::new("prefs-general").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Default units");
        choice(ui, "prefs-units", &mut p.units, &Units::ALL, Units::label);
        ui.end_row();
        ui.label("Default modeling orientation");
        choice(ui, "prefs-up", &mut p.up_axis, &UpAxis::ALL, UpAxis::label);
        ui.end_row();
      });
      ui.add_space(6.0);
      note(ui, "Default units apply to new designs; FissionOxide designs are in millimetres for now.");
      note(ui, "The orientation is saved, but the view stays Z up until the up axis can be changed.");
    }
    Page::Navigation => {
      egui::Grid::new("prefs-nav").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Pan, Zoom, Orbit shortcuts");
        choice(ui, "prefs-nav-preset", &mut p.nav, &NavPreset::ALL, NavPreset::label);
        ui.end_row();
        for (name, a) in [("Orbit", NavAction::Orbit), ("Pan", NavAction::Pan), ("Zoom", NavAction::Zoom)] {
          ui.label(RichText::new(name).color(theme::MUTED));
          ui.label(p.nav.describe(a));
          ui.end_row();
        }
      });
      ui.add_space(6.0);
      ui.checkbox(&mut p.reverse_zoom, "Reverse zoom direction");
      ui.checkbox(&mut p.zoom_to_cursor, "Zoom toward the cursor");
    }
    Page::Design => {
      ui.checkbox(&mut p.capture_history, "Capture design history for new designs");
      note(ui, "Off: new designs start without a timeline, edits go into a base feature.");
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
    Page::Sketch => {
      ui.checkbox(&mut p.auto_look_at, "Auto look at sketch");
      note(ui, "Creating or editing a sketch turns the view to face it.");
      ui.add_space(4.0);
      ui.checkbox(&mut p.show_dims, "Show dimensions");
      ui.checkbox(&mut p.show_constraints, "Show constraints");
      ui.checkbox(&mut p.snap, "Snap to grid");
    }
    Page::Graphics => {
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
  }
  app.prefs.values = values;
}

#[cfg(test)]
mod tests {
  use egui::Modifiers;

  use super::*;

  #[test]
  fn preferences_round_trip_through_settings_ini() {
    let p = Prefs {
      units: Units::In,
      up_axis: UpAxis::Y,
      nav: NavPreset::SolidWorks,
      reverse_zoom: true,
      zoom_to_cursor: false,
      capture_history: false,
      auto_look_at: false,
      show_dims: false,
      show_constraints: false,
      snap: true,
      gpu: false,
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
    app.prefs.values.capture_history = false;
    app.prefs.values.show_dims = false;
    app.prefs.values.snap = true;
    app.prefs.values.gpu = false;
    apply(&mut app);
    assert!(!app.show_dims && app.snap && !app.gpu.enabled);
    app.new_design();
    assert!(app.s.design.history_off, "no timeline");
    assert!(!app.s.can_undo() && !app.s.dirty, "the setting isn't an undo step");
    app.prefs.values.capture_history = true;
    app.new_design();
    assert!(!app.s.design.history_off);
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
    assert_eq!(sw.describe(Pan), "Ctrl+Middle drag");
    assert_eq!(al.describe(Zoom), "Wheel, Shift+Alt+Right drag");
    assert_eq!(f.describe(Orbit), "Shift+Middle drag, Right drag");
  }
}
