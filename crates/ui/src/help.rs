//! The Help (?) menu's windows and links: Keyboard Shortcuts (made from [`crate::keymap`], so it
//! lists exactly the keys the app binds), What's New, the diagnostic log and the documentation.

use egui::RichText;

use crate::keymap::{self, Scope};
use crate::{App, theme};

/// Where FissionOxide's documentation lives.
pub const DOCS: &str = "https://github.com/leckliter/FissionOxide#readme";

#[derive(Default)]
pub struct Help {
  /// The Keyboard Shortcuts window is open.
  pub shortcuts: bool,
  /// Its search text.
  filter: String,
}

/// The mouse, which the keymap doesn't cover.
const MOUSE: [(&str, &str); 6] = [
  ("Middle-drag", "Pan"),
  ("Shift+middle-drag", "Orbit"),
  ("Right-drag", "Orbit"),
  ("Wheel", "Zoom"),
  ("Right-click", "Marking menu"),
  ("Alt+click", "Select Other"),
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

/// Diagnostic Log: the folder the app writes its log to.
pub fn diagnostic_log(app: &mut App) {
  let dir = fox_desktop::log_dir();
  if let Err(e) = std::fs::create_dir_all(&dir) {
    app.status = Some((format!("Couldn't make {}: {e}", dir.display()), true));
    return;
  }
  open(app, &dir.to_string_lossy());
}

/// The rows the Keyboard Shortcuts window lists, by section: every binding of the keymap, then
/// the mouse. `filter` keeps rows whose key or action contains it.
pub fn sections(filter: &str) -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
  let f = filter.trim().to_lowercase();
  let keep = |(k, l): &(&str, &str)| f.is_empty() || k.to_lowercase().contains(&f) || l.to_lowercase().contains(&f);
  let mut out: Vec<(&'static str, Vec<(&'static str, &'static str)>)> =
    Scope::ALL.iter().map(|s| (s.title(), keymap::rows(*s).into_iter().filter(keep).collect())).collect();
  out.push(("Mouse", MOUSE.into_iter().filter(keep).collect()));
  out.retain(|(_, rows)| !rows.is_empty());
  out
}

/// The Keyboard Shortcuts window.
pub fn window(app: &mut App, ctx: &egui::Context) {
  if !app.help.shortcuts {
    return;
  }
  let mut open = true;
  let max_h = (ctx.content_rect().height() * 0.75).max(200.0);
  egui::Window::new("Keyboard Shortcuts")
    .open(&mut open)
    .collapsible(false)
    .resizable(true)
    .default_size([420.0, max_h])
    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
    .show(ctx, |ui| {
      ui.horizontal(|ui| {
        ui.label("Search");
        ui.add(egui::TextEdit::singleline(&mut app.help.filter).desired_width(220.0));
      });
      ui.separator();
      egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
        for (title, rows) in sections(&app.help.filter) {
          ui.label(RichText::new(title).strong().color(theme::TEXT));
          egui::Grid::new(("shortcuts", title)).num_columns(2).spacing([24.0, 3.0]).striped(true).show(ui, |ui| {
            for (key, what) in rows {
              ui.label(RichText::new(key).monospace());
              ui.label(what);
              ui.end_row();
            }
          });
          ui.add_space(8.0);
        }
      });
    });
  if !open {
    app.help.shortcuts = false;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  /// Every bit of text drawn in a frame.
  fn drawn_text(out: &egui::FullOutput) -> Vec<String> {
    out
      .shapes
      .iter()
      .filter_map(|s| match &s.shape {
        egui::Shape::Text(t) => Some(t.galley.text().to_string()),
        _ => None,
      })
      .collect()
  }

  #[test]
  fn the_shortcuts_window_lists_every_bound_key() {
    let ctx = egui::Context::default();
    let mut app = App::with_session(Session::default());
    app.help.shortcuts = true;
    let input = || egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 4000.0))),
      ..Default::default()
    };
    // a window sizes itself over its first frames
    let mut text = Vec::new();
    for _ in 0..3 {
      let out = ctx.run_ui(input(), |ui| window(&mut app, ui.ctx()));
      text = drawn_text(&out);
      out.drop_without_applying_deltas();
    }
    for b in keymap::BINDINGS {
      let key = keymap::key_text(b);
      assert!(text.contains(&key), "{key} ({}) isn't listed", b.label);
      assert!(text.iter().any(|t| t == b.label), "{} isn't listed", b.label);
    }
    for (k, _) in MOUSE {
      assert!(text.iter().any(|t| t == k), "{k}");
    }
  }

  #[test]
  fn shortcut_search_filters_rows() {
    let rows: Vec<_> = sections("extrude").into_iter().flat_map(|(_, r)| r).collect();
    assert_eq!(rows, vec![("E", "Extrude")]);
    assert!(sections("no such thing").is_empty());
  }
}
