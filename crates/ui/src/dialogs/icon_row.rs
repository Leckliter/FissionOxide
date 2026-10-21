//! Fusion's icon rows: a choice shown as a row of icon toggles (Move Type, Shell Type, Hole
//! Placement, Hole Type, Hole Tap Type, Drill Point), the chosen one lit. The name is the
//! tooltip, as on the toolbar. A choice the engine can't make yet is drawn greyed.

use crate::icons::{self, Icon};
use crate::theme;

/// One choice of a row: its key, icon, name (the tooltip) and whether it can be picked.
pub(super) struct Choice {
  pub key: &'static str,
  pub icon: Icon,
  pub name: &'static str,
  pub enabled: bool,
}

pub(super) const fn choice(key: &'static str, icon: Icon, name: &'static str) -> Choice {
  Choice { key, icon, name, enabled: true }
}

/// Like [`choice`], greyed: shown as Fusion shows it, but not pickable (the tooltip says why).
pub(super) const fn greyed(key: &'static str, icon: Icon, name: &'static str) -> Choice {
  Choice { key, icon, name, enabled: false }
}

/// Draws the row and returns true when `cur` changed. `why` is the tooltip of a greyed choice.
pub(super) fn icon_row(ui: &mut egui::Ui, cur: &mut &'static str, choices: &[Choice], why: &str) -> bool {
  let mut changed = false;
  ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing.x = 2.0;
    for c in choices {
      if c.enabled {
        if icons::icon_button(ui, c.icon, 26.0, c.name, *cur == c.key, None).clicked() && *cur != c.key {
          *cur = c.key;
          changed = true;
        }
      } else {
        let hint = if why.is_empty() { c.name.to_string() } else { format!("{}: {why}", c.name) };
        ui.add_enabled_ui(false, |ui| icons::icon_button(ui, c.icon, 26.0, &hint, false, Some(theme::MUTED)))
          .inner
          .on_disabled_hover_text(hint.clone());
      }
    }
  });
  changed
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_click_on_an_enabled_icon_changes_the_choice() {
    let ctx = egui::Context::default();
    let choices = [
      choice("a", Icon::HoleSimple, "A"),
      greyed("b", Icon::HoleCounterbore, "B"),
      choice("c", Icon::HoleCountersink, "C"),
    ];
    let mut cur: &'static str = "a";
    let raw = || egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
      ..Default::default()
    };
    // lay the row out once to find the icons; each is 26 pt wide, 2 pt apart, from the left edge
    let mut left = egui::Pos2::ZERO;
    let mut changed = false;
    ctx
      .run_ui(raw(), |ui| {
        left = ui.cursor().min;
        changed = icon_row(ui, &mut cur, &choices, "not yet");
      })
      .drop_without_applying_deltas();
    assert!(!changed);
    let click = |x: f32| {
      let at = left + egui::vec2(x, 13.0);
      let mut input = raw();
      input.events.push(egui::Event::PointerMoved(at));
      for pressed in [true, false] {
        input.events.push(egui::Event::PointerButton {
          pos: at,
          button: egui::PointerButton::Primary,
          pressed,
          modifiers: Default::default(),
        });
      }
      input
    };
    // the third icon: picked
    ctx
      .run_ui(click(13.0 + 2.0 * 28.0), |ui| changed = icon_row(ui, &mut cur, &choices, "not yet"))
      .drop_without_applying_deltas();
    assert!(changed && cur == "c", "{cur}");
    // the greyed one: nothing happens
    ctx
      .run_ui(click(13.0 + 28.0), |ui| changed = icon_row(ui, &mut cur, &choices, "not yet"))
      .drop_without_applying_deltas();
    assert!(!changed && cur == "c", "{cur}");
  }
}
