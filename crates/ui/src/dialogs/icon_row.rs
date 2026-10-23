//! Fusion's icon rows: a choice shown as a row of icon toggles (Move Type, Shell Type, Hole
//! Placement, Hole Type, Hole Tap Type, Drill Point), the chosen one lit. The name is the
//! tooltip, as on the toolbar. A choice the engine can't make yet is drawn greyed; one Fusion
//! keeps for subscribers carries its small blue badge.

use crate::icons::{self, Icon};
use crate::theme;

/// One choice of a row: its key, icon, name (the tooltip), whether it can be picked and whether
/// it carries the subscription badge.
pub(super) struct Choice {
  pub key: &'static str,
  pub icon: Icon,
  pub name: &'static str,
  pub enabled: bool,
  pub badge: bool,
}

pub(super) const fn choice(key: &'static str, icon: Icon, name: &'static str) -> Choice {
  Choice { key, icon, name, enabled: true, badge: false }
}

/// Like [`choice`], greyed: shown as Fusion shows it, but not pickable (the tooltip says why).
pub(super) const fn greyed(key: &'static str, icon: Icon, name: &'static str) -> Choice {
  Choice { key, icon, name, enabled: false, badge: false }
}

/// Like [`choice`], with Fusion's subscription badge on the icon's top right corner (Convert
/// Mesh's Prismatic): still pickable.
pub(super) const fn badged(key: &'static str, icon: Icon, name: &'static str) -> Choice {
  Choice { key, icon, name, enabled: true, badge: true }
}

/// The badge's radius: smaller than the workspace menu's, as on Fusion's icon toggles.
const BADGE_R: f32 = 3.5;

/// Draws the row and returns true when `cur` changed. `why` is the tooltip of a greyed choice.
pub(super) fn icon_row(ui: &mut egui::Ui, cur: &mut &'static str, choices: &[Choice], why: &str) -> bool {
  let mut changed = false;
  ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing.x = 2.0;
    for c in choices {
      if c.enabled {
        let r = icons::icon_button(ui, c.icon, 26.0, c.name, *cur == c.key, None);
        if c.badge {
          let at = r.rect.right_top() + egui::vec2(-BADGE_R - 1.0, BADGE_R + 1.0);
          ui.painter().circle_filled(at, BADGE_R, crate::panels::WORKSPACE_BADGE);
        }
        if r.clicked() && *cur != c.key {
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

/// Fusion's Flip toggle: the row's label, then the Flip icon button, lit while flipped
/// (Draft, Flange, Hem, Fold and Joint flip a direction this way, not with a checkbox).
pub(super) fn flip_toggle(ui: &mut egui::Ui, label: &str, hint: &str, v: &mut bool, dirty: &mut bool) {
  ui.label(label);
  if icons::icon_button(ui, Icon::Flip, 24.0, hint, *v, None).clicked() {
    *v = !*v;
    *dirty = true;
  }
  ui.end_row();
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

  #[test]
  fn a_badged_icon_draws_the_blue_disc_and_stays_pickable() {
    let ctx = egui::Context::default();
    let raw = || egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
      ..Default::default()
    };
    let discs = |choices: &[Choice]| {
      let mut cur: &'static str = "a";
      let out = ctx.run_ui(raw(), |ui| {
        icon_row(ui, &mut cur, choices, "");
      });
      let n = out
        .shapes
        .iter()
        .filter(|s| matches!(&s.shape, egui::Shape::Circle(c) if c.fill == crate::panels::WORKSPACE_BADGE))
        .count();
      out.drop_without_applying_deltas();
      n
    };
    assert_eq!(discs(&[choice("a", Icon::HoleSimple, "A"), badged("b", Icon::HoleCounterbore, "B")]), 1);
    assert_eq!(discs(&[choice("a", Icon::HoleSimple, "A"), choice("b", Icon::HoleCounterbore, "B")]), 0);
    assert!(badged("b", Icon::HoleCounterbore, "B").enabled);
  }
}
