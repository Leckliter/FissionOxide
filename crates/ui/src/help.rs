//! The Help (?) menu's windows and links: Keyboard Shortcuts (made from the menus' key table
//! and the user's own keys, see [`crate::menus::key_of`], so it lists exactly the keys that work), What's New, the diagnostic log and the documentation.

use egui::RichText;

use crate::customize::{Prefs, RESERVED};
use crate::tips::Scope;
use crate::{App, menus, theme};

/// Where FissionOxide's documentation lives.
pub const DOCS: &str = "https://github.com/leckliter/FissionOxide#readme";

#[derive(Default)]
pub struct Help {
  /// The Keyboard Shortcuts window is open.
  pub shortcuts: bool,
  /// Its search text.
  filter: String,
}

/// The mouse, which the key table doesn't cover.
const MOUSE: [(&str, &str); 6] = [
  ("Middle-drag", "Pan"),
  ("Shift+middle-drag", "Orbit"),
  ("Right-drag", "Orbit"),
  ("Wheel", "Zoom"),
  ("Right-click", "Marking menu"),
  ("Click and hold", "Select Other"),
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

/// A section of the Keyboard Shortcuts window: its title and (key, what it does) rows.
pub type Section = (&'static str, Vec<(String, String)>);

/// The rows the Keyboard Shortcuts window lists, by section: the keys the app keeps for itself,
/// then each toolbar's commands with the key they have now (the user's, or the menus' default),
/// then the mouse. `filter` keeps rows whose key or action contains it.
pub fn sections(prefs: &Prefs, filter: &str) -> Vec<Section> {
  let f = filter.trim().to_lowercase();
  let keep = |(k, l): &(String, String)| f.is_empty() || k.to_lowercase().contains(&f) || l.to_lowercase().contains(&f);
  let reserved = |scope: Option<Scope>| {
    RESERVED.iter().filter(move |(.., s)| *s == scope).map(|(k, what, _)| (k.to_string(), what.to_string()))
  };
  let mut out: Vec<Section> = vec![("General", reserved(None).collect())];
  for (title, scope) in [
    ("Design", Scope::Design),
    ("Sketch", Scope::Sketch),
    ("Drawing", Scope::Drawing),
    ("Manufacture", Scope::Manufacture),
    ("Form", Scope::Form),
    ("Animation", Scope::Animation),
  ] {
    let mut rows: Vec<(String, String)> = menus::bindings()
      .iter()
      .filter(|b| b.scope == scope)
      .map(|b| (menus::key_of(prefs, scope, b.label).to_string(), b.label.to_string()))
      .filter(|(k, _)| !k.is_empty())
      .collect();
    rows.extend(reserved(Some(scope)));
    out.push((title, rows));
  }
  out.push(("Mouse", MOUSE.into_iter().map(|(k, l)| (k.to_string(), l.to_string())).collect()));
  for (_, rows) in &mut out {
    rows.retain(keep);
  }
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
        for (title, rows) in sections(&app.prefs, &app.help.filter) {
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
    // the user's keys: Extrude moved off E, Torus given one
    app.prefs.keys.set("design.Extrude", "Shift+X", "E");
    app.prefs.keys.set("design.Torus", "Shift+T", "");
    app.help.shortcuts = true;
    let input = || egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 6000.0))),
      ..Default::default()
    };
    // a window sizes itself over its first frames
    let mut text = Vec::new();
    for _ in 0..3 {
      let out = ctx.run_ui(input(), |ui| window(&mut app, ui.ctx()));
      text = drawn_text(&out);
      out.drop_without_applying_deltas();
    }
    let rows: Vec<(String, String)> = sections(&app.prefs, "").into_iter().flat_map(|(_, r)| r).collect();
    let listed = |k: &str, l: &str| rows.iter().any(|(k2, l2)| k2 == k && l2 == l);
    // every command the menus' table (or the user) gives a key, with the key it has now
    for b in menus::bindings() {
      let key = menus::key_of(&app.prefs, b.scope, b.label);
      if key.is_empty() {
        continue;
      }
      assert!(listed(key, b.label), "{key} ({}) isn't listed", b.label);
      assert!(text.iter().any(|t| t == key), "{key} ({}) isn't drawn", b.label);
      assert!(text.iter().any(|t| t == b.label), "{} isn't drawn", b.label);
    }
    // the keys the app keeps for itself
    for (key, what, _) in RESERVED {
      assert!(listed(key, what), "{key} ({what}) isn't listed");
      assert!(text.iter().any(|t| t == key) && text.iter().any(|t| t == what), "{key} ({what}) isn't drawn");
    }
    // the user's keys show, the replaced default doesn't
    assert!(listed("Shift+X", "Extrude") && listed("Shift+T", "Torus"));
    assert!(!listed("E", "Extrude"));
    // the keys of Project (P) and As-built Joint (Shift+J) come through the same table
    assert!(listed("P", "Project") && listed("Shift+J", "As-built Joint"));
    for (k, _) in MOUSE {
      assert!(text.iter().any(|t| t == k), "{k}");
    }
  }

  #[test]
  fn shortcut_search_filters_rows() {
    let prefs = Prefs::default();
    let rows: Vec<_> = sections(&prefs, "extrude").into_iter().flat_map(|(_, r)| r).collect();
    assert!(rows.contains(&("E".to_string(), "Extrude".to_string())), "{rows:?}");
    assert!(rows.iter().all(|(_, l)| l.to_lowercase().contains("extrude")), "{rows:?}");
    assert!(sections(&prefs, "no such thing").is_empty());
  }
}
