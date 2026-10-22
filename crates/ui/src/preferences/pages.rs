//! Fusion's Preferences pages, row by row, in Fusion's order with its labels and defaults.
//!
//! Most of Fusion's rows control behaviour FissionOxide doesn't have; they are listed here with
//! Fusion's default, stored under their own key in `[Preferences]` (see [`stored_key`]) and drawn
//! greyed with a hover reason. The rows FissionOxide acts on are [`Live`] and drawn by
//! `preferences::live_row`.

use super::Page;

/// A row FissionOxide acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Live {
  Theme,
  GraphicsDriver,
  Recovery,
  UpAxis,
  Tooltips,
  Prompt,
  MouseSetup,
  OrbitType,
  ReverseZoom,
  CameraPivot,
  CubeSize,
  SpaceRepeats,
  ModelingMode,
  AutoLookAt,
  GraphicsPreset,
  /// One of the Canvas Effects, by its index in `preferences::EFFECTS`.
  Effect(usize),
  DesignUnits,
  DesignLength,
  DesignMass,
  GeneralPrecision,
  AngularPrecision,
  RenderEnvironment,
  RenderBrightness,
  RenderBackground,
  RenderColor,
  RenderGround,
  RenderReflections,
  MaterialLibrary,
  MaterialCategory,
  MaterialName,
  ApiPath,
  McpPort,
}

/// One row of a page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Item {
  /// A bold group heading.
  Heading(&'static str),
  /// A row FissionOxide acts on: its label and what it sets.
  Live(&'static str, Live),
  /// A check box: label, Fusion's default.
  Check(&'static str, bool),
  /// A drop-down: label, Fusion's default.
  Combo(&'static str, &'static str),
  /// A number or text box: label, Fusion's default.
  Field(&'static str, &'static str),
  /// Radio buttons: label, Fusion's default, the choices.
  Radio(&'static str, &'static str, &'static [&'static str]),
  /// Several drop-downs side by side separated by " : ": label, each one's default.
  Combos(&'static str, &'static [&'static str]),
}

impl Item {
  /// The label in the page's label column, if the row has one.
  pub(crate) fn label(&self) -> Option<&'static str> {
    match self {
      Item::Heading(_) => None,
      Item::Live(l, _) | Item::Check(l, _) | Item::Combo(l, _) | Item::Field(l, _) => Some(l),
      Item::Radio(l, ..) | Item::Combos(l, _) => Some(l),
    }
  }

  /// Fusion's default for a stored row, as stored.
  pub(crate) fn default_value(&self) -> Option<String> {
    match self {
      Item::Check(_, v) => Some(if *v { "1" } else { "0" }.to_string()),
      Item::Combo(_, v) | Item::Field(_, v) | Item::Radio(_, v, _) => Some(v.to_string()),
      Item::Combos(_, v) => Some(v.join("|")),
      Item::Heading(_) | Item::Live(..) => None,
    }
  }
}

/// The `[Preferences]` key a stored row is saved under: its page, group and label
/// (`Fusion.Design.DefaultStandardforPMI`).
pub(crate) fn stored_key(page: Page, heading: &str, label: &str) -> String {
  let squash = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>();
  let mut k = format!("Fusion.{}", page.key());
  if !heading.is_empty() {
    k = k + "." + &squash(heading);
  }
  k + "." + &squash(label)
}

/// Every stored row with its key and Fusion's default.
pub(crate) fn stored_rows() -> Vec<(String, Item)> {
  let mut out = Vec::new();
  for page in Page::ALL {
    let mut heading = "";
    for item in items(page) {
      if let Item::Heading(h) = item {
        heading = h;
      } else if item.default_value().is_some() {
        out.push((stored_key(page, heading, item.label().unwrap_or("")), *item));
      }
    }
  }
  out
}

/// Whether a page's rows sit left-aligned under their headings (Electronics) rather than with
/// their labels right-aligned against the controls.
pub(crate) fn left_aligned(page: Page) -> bool {
  page == Page::Electronics
}

/// A page's rows, in Fusion's order. The pages with text rather than rows (Data Collection and
/// Use, Tokens, Preview Features, Default Units) are drawn by `preferences::page_ui` itself.
pub(crate) fn items(page: Page) -> &'static [Item] {
  use self::Live as L;
  use Item::*;
  match page {
    Page::General => &[
      Combo("User language", "English"),
      Live("Theme", L::Theme),
      Live("Graphics driver", L::GraphicsDriver),
      Combo("Start-up experience", "Home"),
      Combo("Offline cache period", "14 days"),
      Combo("Purpose for using Fusion", "Commercial"),
      Combo("Default design type", "Part Design"),
      Check("New document tab (+) creates default design", false),
      Check("Create new shortcuts when updating", true),
      Check("Automatic save on close", false),
      Live("Automatic recovery backup interval", L::Recovery),
      Live("Default modeling orientation", L::UpAxis),
      Live("Show tooltips", L::Tooltips),
      Live("Show command prompt", L::Prompt),
      Check("Show default measure", true),
      Check("Show in-command errors and warnings", true),
      Live("Mouse setup (pan, zoom, orbit)", L::MouseSetup),
      Live("Default Orbit type", L::OrbitType),
      Live("Reverse zoom direction", L::ReverseZoom),
      Live("Enable camera pivot", L::CameraPivot),
      Live("ViewCube size", L::CubeSize),
      Check("Use gesture-based view navigation", false),
      Check("Use Windows touchpad for gestures", false),
      Live("Spacebar repeats last command", L::SpaceRepeats),
    ],
    Page::Api => &[
      Combo("Default Language for Scripts", "Prompt"),
      Combo("Default Language for Add-Ins", "Prompt"),
      Live("Default Path for Scripts and Add-Ins", L::ApiPath),
      Field("Debugging Port", "9000"),
      Field("TypeScript Debugging Port", "9229"),
      Check("Developer Tools", false),
      Check("Legacy Scripts Dialog", false),
      Check("Fusion MCP Server (runs locally on this device)", false),
      Live("Fusion MCP Server Port", L::McpPort),
    ],
    Page::Design => &[
      Live("Default Modeling Mode", L::ModelingMode),
      Combo("Default Standard for PMI", "Infer from Units"),
      Check("Use Automatic Size for PMI", true),
      Field("Default Tolerance for PMI", "0.01"),
      Check("Auto project edges on reference", false),
      Live("Auto look at sketch", L::AutoLookAt),
      Check("Edit dimension when created", true),
      Check("Show ghosted result body", true),
      Check("Auto project geometry on active sketch plane", true),
      Check("Auto hide sketch on feature creation", true),
      Check("Scale entire sketch at first dimension", false),
      Check("Enable Arrange and Simplify tools", true),
      Check("Import coordinate systems from STEP files", false),
      Heading("Preferences that control inserting a mesh"),
      Check("Triangulate mesh polygons", false),
      Combo("Default Shading", "Smooth shaded (Organic/smooth shapes; increased performance)"),
    ],
    Page::DesignAssembly => &[
      Check("Active component visibility", true),
      Combo("Default Joints Object Visibility", "Visible"),
      Check("Animate joint preview", true),
      Check("First component grounded to parent", true),
      Combo("In Hybrid designs create new components as", "Internal"),
      Combo("Default constrained position", "Click point"),
      Heading("Assembly component in-place edit"),
      Check("Allow assembly references", true),
      Combo("Assembly Reference timeline dependency:", "Timeline independent references"),
    ],
    Page::DesignConfigurations => &[
      Combo("Export configured components as", "Configured Designs"),
      Combos("Display configured components in Browser", &["Configuration Name", "None", "None"]),
    ],
    Page::Manufacture => &[
      Field("Post processor editor", ""),
      Field("NC file editor", ""),
      Check("Enable Cloud Libraries", true),
      Combo("Default workspace tab", "Last Used"),
      Combo("Default operation type", "From Active Tab"),
      Combo("Default operation tool", "From previous operation"),
      Heading("Browser Display Properties"),
      Heading("Prompts"),
      Heading("Additive"),
      Heading("Inspection Results"),
      Heading("Colors"),
      Heading("Tool Library"),
      Heading("Manufacturing Model"),
      Heading("Turning"),
      Heading("IPS and Remaining Stock"),
    ],
    Page::Electronics => &[
      Heading("Schematic"),
      Radio("Theme", "Light", &["Dark", "Light"]),
      Radio("Cursor", "Small", &["Small", "Large"]),
      Radio("Group Annotations", "On", &["On", "Hover only", "Off"]),
      Heading("PCB"),
      Radio("Theme", "Dark", &["Dark", "Light"]),
      Radio("Cursor", "Small", &["Small", "Large"]),
      Check("Detect Board Shape", true),
      Heading("Misc"),
      Check("Always vector font", false),
      Check("Limit zoom factor", true),
      Check("Legacy command line focus", false),
      Check("Show Inference", true),
      Field("Mouse wheel zoom", "1.2"),
    ],
    Page::Render => &[
      Heading("Environment"),
      Live("Environment", L::RenderEnvironment),
      Live("Brightness (lx)", L::RenderBrightness),
      Live("Background", L::RenderBackground),
      Live("Color (R, G, B)", L::RenderColor),
      Heading("Ground Plane"),
      Live("Ground Plane", L::RenderGround),
      Check("Flatten Ground", false),
      Live("Reflections", L::RenderReflections),
      Field("Roughness", "0.1"),
      Heading("Camera"),
      Field("Focal Length (mm)", "90"),
      Field("Exposure (EV)", "9.5"),
      Combo("Aspect Ratio", "Viewport aspect ratio"),
    ],
    Page::Drawing => &[
      Combo("Standard", "ISO"),
      Combo("Units", "mm"),
      Combo("Sheet Size", "A3 Landscape"),
      Combo("Delimiter", "Decimal"),
      Check("Edge", false),
      Check("Override or Restore Format Defaults Below", false),
      Check("Include Mesh Bodies", false),
      Check("Include Surface Bodies", false),
      Combo("Projection Angle", "First Angle"),
      Combo("Text Height Group", "Medium"),
      Field("Small Text Height", "2.5mm"),
      Field("Medium Text Height", "3.5mm"),
      Field("Large Text Height", "5mm"),
    ],
    Page::Simulation => &[
      Combo("Default Simulation Study Type", "Prompt"),
      Combo("Load Glyph Graphics", "Screen Size"),
      Field("Load Glyph Relative Size Scale (%)", "100"),
      Check("Warn before solving a model with hidden bodies or components", true),
      Check("Warn before solving a model with hidden settings", true),
    ],
    Page::Tokens => &[Check("Show balances in dialogs for tasks requiring tokens", true)],
    Page::Material => &[
      Live("Physical Material Library", L::MaterialLibrary),
      Live("Physical Material Category", L::MaterialCategory),
      Live("Physical Material Name", L::MaterialName),
      Check("Apply a different appearance", false),
      Combo("Appearance Material Library", "Fusion Appearance Library"),
      Combo("Appearance Category", "Metal/Aluminum"),
      Combo("Material Name", "Prism-028"),
      Field("Exposure", "0"),
    ],
    Page::Graphics => &[
      Live("Graphics Preset", L::GraphicsPreset),
      Check("Dynamic", true),
      Field("Minimum framerate during navigation (FPS)", "35.00"),
      Combo("Selection display style", "Normal"),
      Combo("Degraded selection display style", "Simple display (without glow or halo)"),
      Combo("Transparency Effect", "Better Performance"),
      Check("Wood Bump", true),
      Check("Animate view transitions", true),
      Check("Enable Surface Normal Display", true),
      Field("Hidden Edge Dimming (%)", "50"),
      Heading("Display-specific Settings"),
      Check("High-resolution canvas graphics", true),
      Heading("Canvas Effects"),
      Live("Environment Dome", L::Effect(0)),
      Live("Ground Plane", L::Effect(1)),
      Live("Ground Shadow", L::Effect(2)),
      Live("Ground Reflection", L::Effect(3)),
      Live("Object Shadow", L::Effect(4)),
      Live("Ambient Occlusion", L::Effect(5)),
      Live("Anti-Aliasing", L::Effect(6)),
    ],
    Page::GraphicsRdp => &[
      Check("Remote Desktop Performance Optimization", false),
      Field("Maximum framerate during navigation (FPS)", "10.00"),
      Check("Show wireframe during navigation", false),
      Check("Render with only basic material", false),
      Check("Disable advanced render effect", false),
    ],
    Page::Network => &[
      Combo("Windows network proxy setting", "Automatic"),
      Field("Proxy Host", ""),
      Field("Proxy Port", "0"),
      Combo("Server Verification", "Warn when accessing through an intermediate server"),
    ],
    Page::DataCollection => &[
      Check("Required data collection", true),
      Check("Help develop new products and services", false),
      Check("Personalize our offers", false),
    ],
    Page::UnitDisplay => &[
      Live("General precision", L::GeneralPrecision),
      Live("Angular precision", L::AngularPrecision),
      Combo("Scientific notation precision", "1.123E+04"),
      Combo("Foot and inch display format", "Decimal (e.g. 1.5 in)"),
      Combo("Degree display format", "Decimal (e.g. 39.25 deg)"),
      Combo("Decimal point display", "123.45"),
      Check("Display abbreviations for units", true),
      Check("Display symbols for units", false),
      Check("Use scientific notation for big and small values", true),
      Field("Use scientific notation above", "99999.9999"),
      Field("below", "0.0001"),
      Check("Hide trailing zeroes", true),
      Combo("Minimum precision when hiding trailing zeroes", "4.00"),
      Combo("Material Unit Display", "English Standard (ksi, psi, BTU)"),
    ],
    Page::UnitDisplaySim => &[
      Check("Set precision preferences for simulation and generative design studies", true),
      Combo("Scientific notation precision", "1.123E+04"),
      Field("above", "99999.9999"),
      Field("below", "0.001"),
      Check("Hide trailing zeroes", true),
    ],
    Page::DefaultUnitsManufacture => &[Combo("Default units", "mm")],
    Page::DefaultUnitsDesign => &[
      Live("Default units for new design", L::DesignUnits),
      Live("Length", L::DesignLength),
      Live("Mass", L::DesignMass),
    ],
    Page::DefaultUnitsSimulation => &[
      Combo("Default units", "Custom"),
      Combo("Length", "mm"),
      Combo("Mass", "kg"),
      Combo("Time", "s"),
      Combo("Force", "N"),
      Combo("Pressure", "MPa"),
      Combo("Temperature", "C"),
      Combo("Energy", "J"),
      Combo("Power", "W"),
      Combo("Acceleration", "m / s^2"),
      Combo("Flow Rate", "CMH"),
      Combo("Angle", "deg"),
      Combo("Velocity", "m / s"),
      Combo("Viscosity", "N s / m^2"),
    ],
    Page::Compatibility => &[
      Check("High-DPI Application Scaling", true),
      Combo("Qt Rendering Hardware Interface API", "Automatic"),
      Combo("Chromium Graphics Backend", "Auto-select"),
      Check("Disable data transfer acceleration", false),
      Check("Event performance logging", false),
      Check("Cache graphics on document save", true),
      Combo("SpaceMouse driver", "Latest"),
      Check("Override Chromium GPU Workarounds", false),
      Check("Log HTTP Request and Response Bodies", true),
      Check("Enable hang detection", false),
    ],
    Page::ManufactureOptional
    | Page::ElecColor
    | Page::ElecMisc
    | Page::ElecGrid
    | Page::ElecDefaults
    | Page::ElecDrill
    | Page::ElecDirectory
    | Page::Elec3d
    | Page::DefaultUnits
    | Page::PreviewFeatures => &[],
  }
}
