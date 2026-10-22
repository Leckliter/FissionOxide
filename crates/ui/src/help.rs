//! The Help (?) menu, in Fusion's order: the Search Help field, Take a Tour, the Learning and
//! Documentation, Community and Support and Diagnostics submenus, Check for Updates, What's New,
//! Release Notes and About. Fusion has no Keyboard Shortcuts window: keys show on the menu rows.
//! Rows whose Autodesk service FissionOxide has no counterpart for say so in the status line.

use egui::{RichText, vec2};

use crate::{App, logo, theme};

/// FissionOxide's repository: its documentation, discussions and issues.
pub const REPO: &str = "https://github.com/leckliter/FissionOxide";
/// Where FissionOxide's documentation lives.
pub const DOCS: &str = "https://github.com/leckliter/FissionOxide#readme";

#[derive(Default)]
pub struct Help {
  /// The Search Help field's text.
  search: String,
  /// The About window is open.
  pub about: bool,
}

/// One row of the Help menu: (label, key shown at the right, enabled), or a submenu.
pub enum Row {
  Item(&'static str, &'static str, bool),
  Sub(&'static str, &'static [Row]),
}

use Row::{Item, Sub};

/// Fusion's Help menu below the Search Help field (Quick Setup and Clear Cache Data greyed, as
/// in Fusion).
pub const MENU: &[Row] = &[
  Item("Take a Tour", "", true),
  Sub(
    "Learning and Documentation",
    &[
      Item("Self-Paced Learning", "", true),
      Item("Product Documentation", "Ctrl+/", true),
      Item("Fusion API", "", true),
    ],
  ),
  Item("Quick Setup", "", false),
  Item("Autodesk Assistant", "Shift+H", true),
  Sub("Community", &[Item("Forum", "", true), Item("Insider Program", "", true), Item("Blog", "", true)]),
  Sub(
    "Support and Diagnostics",
    &[
      Item("Get Support", "", true),
      Item("Annotate Log File", "", true),
      Item("Diagnostic Log Files", "", true),
      Item("Graphics Diagnostic", "", true),
      Item("Clear Cache Data", "", false),
      Item("Service Utility", "", true),
      Item("Report Performance Issue", "", true),
    ],
  ),
  Item("Check for Updates", "", true),
  Item("What's New", "", true),
  Item("Release Notes", "", true),
  Item("About", "", true),
];

/// Opens a page or folder, saying so in the status line if it can't.
pub fn open(app: &mut App, target: &str) {
  if let Err(e) = fox_desktop::open(target) {
    app.status = Some((format!("Couldn't open {target}: {e}"), true));
  }
}

/// What's New: there is no changelog in the app, so the release notes of this version on GitHub
/// (the notes the Software Update window shows for a new one).
pub fn whats_new(app: &mut App) {
  let page = format!("{}/tag/v{}", fox_desktop::update::RELEASES_PAGE, fox_desktop::VERSION);
  open(app, &page);
}

/// Diagnostic Log Files: the folder the app writes its log to.
pub fn diagnostic_log(app: &mut App) {
  let dir = fox_desktop::log_dir();
  if let Err(e) = std::fs::create_dir_all(&dir) {
    app.status = Some((format!("Couldn't make {}: {e}", dir.display()), true));
    return;
  }
  open(app, &dir.to_string_lossy());
}

/// The Autodesk Assistant (the Help row, Shift+H and the title bar's button).
pub fn assistant(app: &mut App) {
  app.status = Some(("The Autodesk Assistant isn't part of FissionOxide".into(), false));
}

/// The page Search Help looks `text` up on: the repository's search.
pub fn search_url(text: &str) -> String {
  let mut q = String::new();
  for b in text.trim().bytes() {
    match b {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => q.push(b as char),
      b' ' => q.push('+'),
      _ => q.push_str(&format!("%{b:02X}")),
    }
  }
  format!("{REPO}/search?q={q}")
}

/// Runs a Help menu row.
pub fn action(app: &mut App, label: &str) {
  let missing = |app: &mut App, what: &str| app.status = Some((format!("{what} isn't part of FissionOxide"), false));
  match label {
    "Self-Paced Learning" | "Product Documentation" => open(app, DOCS),
    "Fusion API" => open(app, &format!("{REPO}/blob/main/CONTRIBUTING.md")),
    "Autodesk Assistant" => assistant(app),
    "Forum" => open(app, &format!("{REPO}/discussions")),
    "Get Support" => open(app, &format!("{REPO}/issues")),
    "Report Performance Issue" => open(app, &format!("{REPO}/issues/new")),
    "Diagnostic Log Files" => diagnostic_log(app),
    "Graphics Diagnostic" => {
      let gpu = if crate::gpu::active(app) { "OpenGL on the GPU" } else { "the software renderer" };
      app.status = Some((format!("Graphics: the canvas draws with {gpu}"), false));
    }
    "Check for Updates" => crate::job_status::check_for_updates(app),
    "What's New" => whats_new(app),
    "Release Notes" => open(app, fox_desktop::update::RELEASES_PAGE),
    "About" => app.help.about = true,
    "Take a Tour" => missing(app, "The guided tour"),
    "Insider Program" => missing(app, "The Insider Program"),
    "Blog" => missing(app, "The Fusion blog"),
    "Annotate Log File" => missing(app, "Annotate Log File"),
    "Service Utility" => missing(app, "The Service Utility"),
    _ => {}
  }
}

/// The Help (?) menu.
pub fn menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(260.0);
  let r = ui.add(egui::TextEdit::singleline(&mut app.help.search).hint_text("Search Help").desired_width(240.0));
  if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !app.help.search.trim().is_empty() {
    let url = search_url(&app.help.search);
    ui.close();
    open(app, &url);
  }
  ui.separator();
  rows(app, ui, MENU);
}

fn rows(app: &mut App, ui: &mut egui::Ui, rows: &[Row]) {
  for row in rows {
    match row {
      Sub(label, sub) => {
        ui.menu_button(*label, |ui| {
          ui.set_min_width(220.0);
          self::rows(app, ui, sub);
        });
      }
      Item(label, key, enabled) => {
        // Check for Updates names an update that is ready or on its way
        let text = if *label == "Check for Updates" { app.updates.menu_label() } else { label.to_string() };
        let mut b = egui::Button::new(text).min_size(vec2(220.0, 0.0));
        if !key.is_empty() {
          b = b.shortcut_text(*key);
        }
        if ui.add_enabled(*enabled, b).clicked() {
          ui.close();
          action(app, label);
        }
      }
    }
  }
}

/// The Help rows' keys: Ctrl+/ (Product Documentation) and Shift+H (Autodesk Assistant).
/// Returns whether one was pressed.
pub(crate) fn keys(app: &mut App, ctx: &egui::Context) -> bool {
  use egui::{Key, Modifiers};
  if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Slash)) {
    action(app, "Product Documentation");
    return true;
  }
  if ctx.input(|i| i.modifiers.shift_only()) && ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::H)) {
    assistant(app);
    return true;
  }
  false
}

/// The About window: the logo, name and version.
pub fn window(app: &mut App, ctx: &egui::Context) {
  if !app.help.about {
    return;
  }
  let mut open = true;
  let mut close = false;
  egui::Window::new("About")
    .open(&mut open)
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
    .show(ctx, |ui| {
      ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(40.0, 44.0), egui::Sense::hover());
        logo::paint(ui.painter(), r);
        ui.vertical(|ui| {
          ui.label(RichText::new("FissionOxide").strong().size(18.0));
          ui.label(format!("Version {}", fox_desktop::VERSION));
          ui.label(RichText::new("Parametric CAD on OpenCASCADE").color(theme::MUTED));
        });
      });
      ui.separator();
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| close = ui.button("Close").clicked());
    });
  if !open || close {
    app.help.about = false;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  fn labels(rows: &[Row]) -> Vec<String> {
    rows
      .iter()
      .map(|r| match r {
        Item(l, "", true) => l.to_string(),
        Item(l, "", false) => format!("{l} (grey)"),
        Item(l, k, _) => format!("{l} {k}"),
        Sub(l, sub) => format!("{l} ▸ ({})", labels(sub).join(", ")),
      })
      .collect()
  }

  /// `ui:chrome:help-menu-current`: Fusion's Help menu, row for row, with no Keyboard Shortcuts.
  #[test]
  fn the_help_menu_is_fusions() {
    assert_eq!(
      labels(MENU),
      [
        "Take a Tour",
        "Learning and Documentation ▸ (Self-Paced Learning, Product Documentation Ctrl+/, Fusion API)",
        "Quick Setup (grey)",
        "Autodesk Assistant Shift+H",
        "Community ▸ (Forum, Insider Program, Blog)",
        "Support and Diagnostics ▸ (Get Support, Annotate Log File, Diagnostic Log Files, Graphics Diagnostic, \
         Clear Cache Data (grey), Service Utility, Report Performance Issue)",
        "Check for Updates",
        "What's New",
        "Release Notes",
        "About",
      ]
    );
  }

  #[test]
  fn search_help_and_about() {
    assert_eq!(search_url(" fillet edge "), format!("{REPO}/search?q=fillet+edge"));
    assert_eq!(search_url("a&b/c"), format!("{REPO}/search?q=a%26b%2Fc"));
    let mut app = App::with_session(Session::default());
    action(&mut app, "About");
    assert!(app.help.about);
    action(&mut app, "Take a Tour");
    assert_eq!(app.status, Some(("The guided tour isn't part of FissionOxide".into(), false)));
    let ctx = egui::Context::default();
    let input = egui::RawInput {
      events: vec![
        egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
        egui::Event::Key {
          key: egui::Key::H,
          physical_key: None,
          pressed: true,
          repeat: false,
          modifiers: egui::Modifiers::SHIFT,
        },
      ],
      ..Default::default()
    };
    app.status = None;
    let mut hit = false;
    ctx.run_ui(input, |ui| hit = keys(&mut app, ui.ctx())).drop_without_applying_deltas();
    assert!(hit);
    assert_eq!(app.status, Some(("The Autodesk Assistant isn't part of FissionOxide".into(), false)));
  }
}
