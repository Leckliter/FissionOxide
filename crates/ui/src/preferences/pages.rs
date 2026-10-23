//! Fusion's Preferences pages, row by row, in Fusion's order with its labels and defaults.
//!
//! Most of Fusion's rows control behaviour FissionOxide doesn't have; they are listed here with
//! Fusion's default, stored under their own key in `[Preferences]` (see [`stored_key`]), and
//! drawn greyed with a hover reason or, where Fusion's page is all live (Drawing, Manufacture,
//! Network, Preview Features), as controls the user can change and that keep their values. The
//! rows FissionOxide acts on are [`Live`] and drawn by `preferences::live_row`.

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
  HideZeroes,
  MinPrecision,
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
  /// A number box with up and down arrows: label, Fusion's default.
  Spin(&'static str, &'static str),
  /// Radio buttons: label, Fusion's default, the choices.
  Radio(&'static str, &'static str, &'static [&'static str]),
  /// Several drop-downs side by side separated by " : ": label, each one's default.
  Combos(&'static str, &'static [&'static str]),
  /// A check box the user can tick though FissionOxide doesn't act on it: label, Fusion's default.
  Toggle(&'static str, bool),
  /// A drop-down the user can pick from though FissionOxide doesn't act on it: label, Fusion's
  /// default, the choices.
  Choice(&'static str, &'static str, &'static [&'static str]),
  /// A path box with a "..." button beside it: label, Fusion's default.
  Browse(&'static str, &'static str),
  /// A text box the user can type in though FissionOxide doesn't act on it: label, Fusion's
  /// default.
  Entry(&'static str, &'static str),
  /// A number box with up and down arrows the user can change though FissionOxide doesn't act
  /// on it: label, Fusion's default (a whole number, or a decimal with its places: "1.00").
  Number(&'static str, &'static str),
  /// A [`Item::Browse`] the user can type in or pick a file for: label, Fusion's default.
  Path(&'static str, &'static str),
  /// Greyed text in the value column, not a control: label, the text.
  Note(&'static str, &'static str),
  /// A [`Item::Toggle`] whose box, while unticked, greys every row after it (Drawing's "Override
  /// or Restore Format Defaults Below").
  Gate(&'static str, bool),
}

impl Item {
  /// The label in the page's label column, if the row has one.
  pub(crate) fn label(&self) -> Option<&'static str> {
    match self {
      Item::Heading(_) => None,
      Item::Live(l, _) | Item::Check(l, _) | Item::Combo(l, _) | Item::Field(l, _) | Item::Spin(l, _) => Some(l),
      Item::Radio(l, ..) | Item::Combos(l, _) => Some(l),
      Item::Toggle(l, _) | Item::Choice(l, ..) | Item::Browse(l, _) | Item::Note(l, _) | Item::Gate(l, _) => Some(l),
      Item::Entry(l, _) | Item::Number(l, _) | Item::Path(l, _) => Some(l),
    }
  }

  /// Fusion's default for a stored row, as stored.
  pub(crate) fn default_value(&self) -> Option<String> {
    match self {
      Item::Check(_, v) | Item::Toggle(_, v) | Item::Gate(_, v) => Some(if *v { "1" } else { "0" }.to_string()),
      Item::Combo(_, v) | Item::Field(_, v) | Item::Spin(_, v) | Item::Radio(_, v, _) => Some(v.to_string()),
      Item::Choice(_, v, _) | Item::Browse(_, v) => Some(v.to_string()),
      Item::Entry(_, v) | Item::Number(_, v) | Item::Path(_, v) => Some(v.to_string()),
      Item::Combos(_, v) => Some(v.join("|")),
      Item::Heading(_) | Item::Live(..) | Item::Note(..) => None,
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

/// Drawing's text height drop-downs' choices.
const TEXT_HEIGHTS: &[&str] = &["1.8mm", "2.5mm", "3.5mm", "5mm", "7mm", "10mm"];
/// Drawing's line width drop-downs' choices.
const LINE_WIDTHS: &[&str] = &["0.13mm", "0.18mm", "0.25mm", "0.35mm", "0.50mm", "0.70mm", "1.00mm"];
/// Drawing's dimension precision drop-downs' choices.
const DIM_PRECISIONS: &[&str] = &["0", "0.1", "0.12", "0.123", "0.1234"];

/// A page's rows, in Fusion's order. The pages with text rather than rows (Data Collection and
/// Use, Tokens, Preview Features, Default Units) are drawn by `preferences::page_ui` itself.
pub(crate) fn items(page: Page) -> &'static [Item] {
  use self::Live as L;
  use Item::*;
  match page {
    Page::General => &[
      Combo("User language", "English | English"),
      Live("Theme", L::Theme),
      Live("Graphics driver", L::GraphicsDriver),
      Combo("Start-up experience", "Default Design Type"),
      Spin("Offline cache time period (days)", "15"),
      Combo("Purpose for using Fusion", "Design"),
      Combo("Default design type", "Part Design"),
      Check("New document tab (+) creates default design", false),
      Check("Create new shortcuts when updating Fusion", false),
      Check("Automatic save on close", true),
      Live("Automatic recovery backup interval (minutes)", L::Recovery),
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
      Check("Use gesture-based view navigation", true),
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
      Path("Post processor editor", ""),
      Path("NC file editor", ""),
      Toggle("Enable Cloud Libraries", false),
      Choice(
        "Default workspace tab",
        "Last Used",
        &["Last Used", "Milling", "Turning", "Additive", "Inspection", "Fabrication", "Utilities"],
      ),
      Choice(
        "Default operation type",
        "From Active Tab",
        &["From Active Tab", "Milling", "Turning", "Additive", "Inspection", "Fabrication"],
      ),
      Choice("Default operation tool", "From previous operation", &["From previous operation", "None"]),
      Heading("Browser Display Properties"),
      Toggle("Show tool number", true),
      Toggle("Show tool compensation offsets", false),
      Toggle("Show operation machining time", false),
      Toggle("Show tool description", false),
      Toggle("Show setup work offset and spindle", false),
      Toggle("Show cycle type", true),
      Toggle("Show manual NC", true),
      Toggle("Show pattern type", true),
      Toggle("Highlight toolpaths in canvas", true),
      Toggle("Show toolpath when editing", false),
      Toggle("Show toolpaths in NC Program", false),
      Heading("Prompts"),
      Toggle("Confirm generation when already valid", true),
      Toggle("Automatically generate toolpath on operation change", true),
      Toggle("Show overridden parameters after reference document update", true),
      Toggle("Show the Save As dialog when posting from NC Programs", false),
      Heading("Additive"),
      Toggle("Generate Additive Arrange on creation or edit", true),
      Toggle("Generate Supports on creation or edit", true),
      Toggle("Generate Optimized Orientations on creation or edit", true),
      Toggle("Generate Additive Toolpath on Setup creation", false),
      Toggle("Generate Interference Analysis on creation or edit", true),
      Heading("Inspection Results"),
      Toggle("Use project-to-CAD calculation", true),
      Number("Projection search distance (mm)", "1.00"),
      Choice("Results display type", "Cylinder", &["Cylinder", "Cone", "Sphere"]),
      Heading("Colors"),
      Choice(
        "Tolerance Colors (above upper tolerance / below lower tolerance)",
        "Blue (above) / Red (below) Default",
        &["Blue (above) / Red (below) Default", "Red (above) / Blue (below)"],
      ),
      Heading("Tool Library"),
      Toggle("Enable logging", false),
      Toggle("Reset 3D tool preview", false),
      Toggle("Show validation errors and warnings in tool list", false),
      Heading("Manufacturing Model"),
      Toggle("Enable component sources", false),
      Heading("Turning"),
      Toggle("Diameter Programming", false),
      Heading("IPS and Remaining Stock"),
      Choice(
        "Processing Mode",
        "Standard (computer processor)",
        &["Standard (computer processor)", "Accelerated (graphics processor)"],
      ),
      Toggle("In-Process Stock - Mesh Check Safety", true),
      Heading("CAM Data"),
      Toggle("Enable CAM data import", true),
    ],
    Page::ManufactureOptional => &[
      Check("Automatically get latest Posts, Machines and Print Settings", true),
      Check("Disable discarding unused CAD references", false),
      Check("Disable legacy face selection conversion", false),
      Check("Enable legacy dependency analysis", false),
      Check("Flow (Old)", false),
      Check("Legacy Chaining", false),
      Check("Legacy Contour Compensation for Wrap", false),
      Check("Legacy Drilling", false),
      Check("Legacy flat area detection in 3D Offset Roughing (Pocket)", false),
      Check("Legacy handling of ambiguous references", false),
      Check("Legacy surface tessellation", false),
      Check("Legacy toolpath calculation format", false),
      Check("Modifications for 3D Adaptive Roughing and 3D Offset Roughing", false),
      Check("Old DXF Export for Nesting", false),
      Check("Old Profile Turning", false),
      Check("Re-enable the use of the Model Override", false),
      Check("Skip Collision Detection for Visual Stock Simulation", true),
      Check("Spun Profile", true),
      Check("Tool Library - 3D Preview", true),
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
      Choice("Standard", "ISO", &["ASME", "ISO"]),
      Choice("Units", "mm", &["mm", "in"]),
      Choice(
        "Sheet Size",
        "A3 Landscape",
        &["A4 Portrait", "A4 Landscape", "A3 Landscape", "A2 Landscape", "A1 Landscape", "A0 Landscape"],
      ),
      Choice("Delimiter", "Decimal", &["Decimal", "Comma"]),
      Choice("Edge", "", &[""]),
      Gate("Override or Restore Format Defaults Below", false),
      Toggle("Include Mesh Bodies", true),
      Toggle("Include Surface Bodies", true),
      Choice("Projection Angle", "First Angle", &["First Angle", "Third Angle"]),
      Choice("Text Height Group", "Medium", &["Small", "Medium", "Large"]),
      Note("Small Text Height", "2.5mm"),
      Note("Medium Text Height", "3.5mm"),
      Note("Large Text Height", "5mm"),
      Choice("Small Text Height", "1.8mm", TEXT_HEIGHTS),
      Choice("Medium Text Height", "1.8mm", TEXT_HEIGHTS),
      Choice("Large Text Height", "1.8mm", TEXT_HEIGHTS),
      Choice("Font", "Arial", &["Arial"]),
      Choice("Dimension Format", "Decimal", &["Decimal", "Fractional"]),
      Choice("Linear Dimension Precision", "0.12", DIM_PRECISIONS),
      Choice("Angular Dimension Precision", "0.12", DIM_PRECISIONS),
      Toggle("Display Dimension Units", false),
      Toggle("Display Trailing Zeros", false),
      Toggle("Display Leading Zeros", true),
      Choice("Line Width Group", "Thin", &["Thin", "Medium", "Thick"]),
      Note("Thin Line Width", "0.18mm"),
      Note("Medium Line Width", "0.25mm"),
      Note("Thick Line Width", "0.35mm"),
      Choice("Thin Line Width", "0.25mm", LINE_WIDTHS),
      Choice("Medium Line Width", "0.35mm", LINE_WIDTHS),
      Choice("Thick Line Width", "0.50mm", LINE_WIDTHS),
      Toggle("Display Line Widths", false),
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
      Choice("Windows network proxy setting", "Automatic", &["Automatic", "No Proxy", "Manual"]),
      Entry("Proxy Host", ""),
      Number("Proxy Port", "0"),
      Choice(
        "Server Verification",
        "Warn when accessing through an intermediate server",
        &[
          "Warn when accessing through an intermediate server",
          "Allow access through an intermediate server",
          "Block access through an intermediate server",
        ],
      ),
    ],
    Page::DataCollection => &[
      Check("Required data collection", true),
      Toggle("Help develop new products and services", true),
      Toggle("Personalize our offers", true),
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
      Live("Hide trailing zeroes", L::HideZeroes),
      Live("Minimum precision when hiding trailing zeroes", L::MinPrecision),
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
    Page::ElecDirectory => &[
      Browse("Directory for Library", ".../LibEagle/examples/libraries/examples"),
      Browse("Directory for Design Rules", ".../design rules/examples"),
      Check("Use Library.io for Design Rules by default", true),
      Browse("Directory for ULP", ".../ulps/examples"),
      Check("Use Library.io for ULP by default", true),
      Browse("Directory for Scripts", ".../scripts/examples"),
      Check("Use Library.io for Scripts by default", true),
      Browse("Directory for CAM Jobs", ".../cam/examples"),
      Check("Use Library.io for CAM Jobs by default", true),
      Browse("Directory for Simulation", ".../LibEagle/ngspice/bin"),
      Browse("Directory for SPICE Models", ".../spice/examples"),
    ],
    Page::Elec3d => &[Check("Open 3D PCB automatically when an electronics design file is opened", false)],
    // drawn by `preferences::electronics`: swatch grids, two-column lists and tables
    Page::ElecColor | Page::ElecMisc | Page::ElecGrid | Page::ElecDefaults | Page::ElecDrill => &[],
    // drawn by `preferences::preview_features`: the workspace filter, then each section's boxes
    Page::PreviewFeatures => &[
      Choice("Workspaces", "All workspaces", &["All workspaces", "General", "Design", "Simulation"]),
      Heading("General"),
      Toggle("Enable Close Speedup Experience", true),
      Heading("Design"),
      Toggle("Preview Volumetric Modeling Features", false),
      Heading("Simulation"),
      Toggle("New Simulation results experience", true),
    ],
    Page::DefaultUnits => &[],
  }
}
