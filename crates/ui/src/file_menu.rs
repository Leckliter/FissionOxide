//! The File menu's rich tooltips, which of its entries are enabled, File ▸ View (Fusion's Hide /
//! Show entries with their Ctrl+Alt shortcuts) and the performance overlay it shows.

use egui::{Color32, Key, Modifiers, RichText, vec2};

use crate::{App, theme};

/// Each File menu entry's tooltip, as Fusion's: a line on what it does and how to use it.
const FILE_TIPS: [(&str, &str, &str); 15] = [
  (
    "New...",
    "Creates a new design.",
    "The new design starts empty, in the default units set in Preferences. Save it to give it a name and a \
     place in a project.",
  ),
  (
    "Add To Assembly",
    "Inserts the open design into another design as a component.",
    "Pick the design to add it to; the open design goes in as a linked component that updates when it changes.",
  ),
  (
    "Open...",
    "Opens a design or drawing from your projects or local computer.",
    "Select a design or drawing from the project browser and click Open, or click 'Open from my computer' and \
     select a supported file.",
  ),
  (
    "Open Recovered Documents (0)",
    "Opens the designs recovered after the app closed unexpectedly.",
    "Recovered designs open with the changes made since their last save. Save them to keep those changes.",
  ),
  (
    "Upload...",
    "Uploads files from your computer to the active project.",
    "Select one or more files: designs, STEP, IGES, meshes, DXF and Fusion archives are added to the folder the \
     Data Panel shows.",
  ),
  (
    "Save",
    "Saves the design as a new version.",
    "The first save asks for a name and a project location. Each later save adds a version you can go back to \
     from the Data Panel.",
  ),
  (
    "Save As",
    "Saves a copy of the design with a new name or location.",
    "Available once the design is saved. The copy starts its own version history; the original stays as it was.",
  ),
  (
    "Save As Latest",
    "Saves the older version you opened as the newest version.",
    "Open an earlier version from the Data Panel, then use Save As Latest to make it the current version.",
  ),
  (
    "Export...",
    "Exports the design to a file on your computer.",
    "Enter a name, choose a file type and a location, and click Export. STEP keeps exact solid geometry; STL, \
     OBJ and 3MF meshes are for 3D printing and visualisation.",
  ),
  (
    "Create Version",
    "Saves the current state of a design shared live as a version.",
    "While others edit a design with you, Create Version records everyone's changes so far as one version.",
  ),
  (
    "3D Print",
    "Prepares the design for 3D printing.",
    "Choose Manufacture to set up an additive build, or Mesh Export to write the bodies as an STL, 3MF or OBJ \
     mesh at the refinement you pick.",
  ),
  (
    "Capture Image...",
    "Saves an image of the canvas.",
    "Choose the window's size or a custom size and resolution, a transparent background and anti-aliasing, then \
     click OK and pick where to save the image.",
  ),
  (
    "Share File Link",
    "Creates a link others can use to view or download the design.",
    "Anyone with the link can view the design in a browser; you choose whether they can download it.",
  ),
  (
    "View Details on Web",
    "Opens the design's page on the web.",
    "The page shows the design's versions, comments and the people it is shared with.",
  ),
  (
    "View",
    "Shows or hides parts of the user interface.",
    "Hide the ViewCube, browser, comments, navigation bar or toolbar for more room, or reset the layout.",
  ),
];

/// The tooltip of the File menu entry `label`: (what it does, how to use it).
pub(crate) fn file_tip(label: &str) -> Option<(&'static str, &'static str)> {
  FILE_TIPS.iter().find(|(l, ..)| *l == label).map(|(_, line, more)| (*line, *more))
}

/// Why a greyed entry is greyed.
const CLOUD_ONLY: &str = "Needs a design saved to a Fusion Team cloud project; FissionOxide keeps designs locally";

/// Whether the File menu entry `label` is enabled: Save As only once the design is saved, as in
/// Fusion; the cloud entries never.
pub(crate) fn enabled(app: &App, label: &str, default: bool) -> bool {
  match label {
    "Save As" => app.s.hub.is_some(),
    _ => default,
  }
}

/// The File menu entry's tooltip on `resp`, enabled or greyed.
pub(crate) fn tip(resp: egui::Response, label: &'static str) -> egui::Response {
  let Some((line, more)) = file_tip(label) else { return resp };
  let note = match label {
    "Save As" => "Save the design first.",
    _ => CLOUD_ONLY,
  };
  let show = move |ui: &mut egui::Ui, greyed: bool| {
    ui.set_max_width(320.0);
    ui.label(RichText::new(label).strong());
    ui.label(line);
    ui.add(egui::Label::new(RichText::new(more).color(theme::TEXT)).wrap());
    if greyed {
      ui.label(RichText::new(note).color(theme::MUTED));
    }
    ui.label(RichText::new(crate::tips::MORE_HELP).color(theme::MUTED));
  };
  resp.on_hover_ui(move |ui| show(ui, false)).on_disabled_hover_ui(move |ui| show(ui, true))
}

// ---------------------------------------------------------------- View

/// File ▸ View's shortcuts, in its order: ViewCube, Browser, Comments, Text Commands, Navigation
/// Bar, Command Toolbar, Data Panel, Performance Overlay, Reset to Default Layout (all Ctrl+Alt).
pub(crate) const VIEW_KEYS: [Key; 9] = [Key::V, Key::B, Key::A, Key::C, Key::N, Key::T, Key::P, Key::O, Key::R];

/// File ▸ View's entries: the label (Hide while shown, Show while hidden), the shortcut and
/// whether it is enabled.
pub(crate) fn view_entries(app: &App) -> Vec<(String, String, bool)> {
  let hs = |shown: bool, what: &str| format!("{} {what}", if shown { "Hide" } else { "Show" });
  let labels = [
    hs(app.show_viewcube, "ViewCube"),
    hs(app.browser_open, "Browser"),
    hs(app.show_comments, "Comments"),
    hs(app.text_commands.open, "Text Commands"),
    hs(app.show_navbar, "Navigation Bar"),
    hs(app.show_toolbar, "Command Toolbar"),
    hs(app.data.open, "Data Panel"),
    hs(app.show_perf, "Performance Overlay"),
    "Reset to Default Layout".to_string(),
  ];
  labels.into_iter().zip(VIEW_KEYS).map(|(label, k)| (label, format!("Ctrl+Alt+{}", k.name()), true)).collect()
}

/// Runs File ▸ View's entry `i`.
pub(crate) fn view_action(app: &mut App, i: usize) {
  match i {
    0 => app.show_viewcube = !app.show_viewcube,
    1 => app.browser_open = !app.browser_open,
    2 => app.show_comments = !app.show_comments,
    3 => app.text_commands.open = !app.text_commands.open,
    4 => app.show_navbar = !app.show_navbar,
    5 => app.show_toolbar = !app.show_toolbar,
    6 => app.data.toggle(),
    7 => app.show_perf = !app.show_perf,
    8 => {
      // Fusion's default layout: everything shows but the data panel and the overlay, TEXT
      // COMMANDS as its header bar
      app.show_viewcube = true;
      app.browser_open = true;
      app.show_comments = true;
      app.show_navbar = true;
      app.show_toolbar = true;
      app.data.open = false;
      app.show_perf = false;
      app.text_commands.open = true;
      app.text_commands.expanded = false;
    }
    _ => {}
  }
}

/// File ▸ View's Ctrl+Alt shortcuts. Returns whether one was pressed.
pub(crate) fn view_keys(app: &mut App, ctx: &egui::Context) -> bool {
  for (i, k) in VIEW_KEYS.iter().enumerate() {
    if ctx.input_mut(|inp| inp.consume_key(Modifiers::COMMAND | Modifiers::ALT, *k)) {
      view_action(app, i);
      return true;
    }
  }
  false
}

/// File ▸ View: Fusion's nine Hide / Show entries and Reset to Default Layout, nothing else.
pub(crate) fn view_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(260.0);
  for (i, (label, key, on)) in view_entries(app).into_iter().enumerate() {
    let r = ui.add_enabled(on, egui::Button::new(label).shortcut_text(key).min_size(vec2(240.0, 0.0)));
    if r.clicked() {
      ui.close();
      view_action(app, i);
    }
  }
}

/// File ▸ View ▸ Show Performance Overlay: the frame rate and time at the canvas's top-left.
pub(crate) fn perf_overlay(app: &App, ctx: &egui::Context) {
  if !app.show_perf {
    return;
  }
  let dt = ctx.input(|i| i.stable_dt).max(1e-4);
  let bodies = app.s.model.bodies.len();
  egui::Area::new(egui::Id::new("perf-overlay"))
    .fixed_pos(app.canvas.left_top() + vec2(12.0, 12.0))
    .order(egui::Order::Middle)
    .interactable(false)
    .show(ctx, |ui| {
      egui::Frame::new().fill(Color32::from_black_alpha(150)).corner_radius(3).inner_margin(6).show(ui, |ui| {
        let line = |ui: &mut egui::Ui, t: String| ui.label(RichText::new(t).monospace().color(Color32::WHITE));
        line(ui, format!("{:>5.0} FPS", 1.0 / dt));
        line(ui, format!("{:>5.1} ms", dt * 1000.0));
        line(ui, format!("{bodies:>5} bodies"));
      });
    });
}
