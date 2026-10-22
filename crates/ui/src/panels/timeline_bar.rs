//! The timeline's playback and settings, as in Fusion: Play replays the timeline from the
//! beginning to where the marker was (its button a stop square meanwhile), the gear's menu
//! (Change to Direct Modeling, Component Color Swatch, Hide all inactive features), the swatch
//! under the items, and which suppressed items failed before they were suppressed (only those
//! keep the pink square behind their faded icon).

use std::collections::HashSet;

use egui::{Color32, Rect, Stroke, Vec2};
use fox_engine::model::FeatureId;
use serde_json::{Value, json};

use super::menu_item;
use crate::icons::{self, Icon};
use crate::{App, theme};

/// A playback in progress: when the marker steps next, and the design index it stops at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Playback {
  pub next: f64,
  pub until: usize,
}

/// Seconds between Play's steps.
pub(crate) const STEP_SECS: f64 = 0.45;

/// Starts Play: the marker jumps to the beginning and will step forward to where it was (to the
/// end when it was at the beginning).
pub(crate) fn play(app: &mut App, now: f64) {
  let n = app.s.design.features.len();
  let active = app.s.design.active_len();
  let until = if active == 0 { n } else { active };
  app.run("timeline.marker", json!({"at": 0}));
  app.playing = Some(Playback { next: now + 0.3, until });
}

/// Steps a playback due at `now`; it ends at its stop.
pub(crate) fn step(app: &mut App, now: f64) {
  let Some(p) = app.playing else { return };
  if now < p.next {
    return;
  }
  let n = app.s.design.features.len();
  let active = app.s.design.active_len();
  if active >= p.until.min(n) {
    app.playing = None;
    return;
  }
  let at = if active + 1 >= n { Value::Null } else { json!(active + 1) };
  app.run("timeline.marker", json!({"at": at}));
  app.playing = Some(Playback { next: now + STEP_SECS, ..p });
}

/// The Play button; while playing it is a stop square that stops the playback.
pub(crate) fn play_button(app: &mut App, ui: &mut egui::Ui, ink: Option<Color32>) {
  let now = ui.input(|i| i.time);
  if app.playing.is_none() {
    if icons::icon_button(ui, Icon::Play, 22.0, "Play", false, ink).clicked() {
      play(app, now);
    }
    return;
  }
  let (rect, resp) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::click());
  if resp.hovered() {
    ui.painter().rect(rect, 2.0, theme::HOVER, Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
  }
  let c = ink.unwrap_or(theme::GROUP_LABEL);
  ui.painter().rect_filled(Rect::from_center_size(rect.center(), Vec2::splat(9.0)), 1.0, c);
  if resp.on_hover_text("Stop").clicked() {
    app.playing = None;
  }
}

/// The gear menu's two checkboxes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Settings {
  pub swatch: bool,
  pub hide_inactive: bool,
}

impl Default for Settings {
  fn default() -> Settings {
    Settings { swatch: false, hide_inactive: true }
  }
}

fn settings_id() -> egui::Id {
  egui::Id::new("timeline-settings")
}

pub(crate) fn settings(ctx: &egui::Context) -> Settings {
  ctx.data(|d| d.get_temp(settings_id())).unwrap_or_default()
}

/// The gear's menu, as Fusion's: Change to Direct Modeling (the browser root's), then the two
/// checkboxes. Each feature belongs to the component it was made in (`Feature::owner`): Hide all
/// inactive features leaves out the other components' features while one is active (see
/// [`inactive`]), and the swatch colours each item by its component.
pub(crate) fn settings_menu(app: &mut App, ui: &mut egui::Ui) {
  let direct = app.s.design.history_off;
  let (icon, label) =
    if direct { (Icon::History, "Capture Design History") } else { (Icon::DirectEdit, "Change to Direct Modeling") };
  if menu_item(ui, icon, label, None).clicked() {
    ui.close();
    crate::config_ui::toggle_history(app);
  }
  let mut s = settings(ui.ctx());
  ui.checkbox(&mut s.swatch, "Component Color Swatch");
  ui.checkbox(&mut s.hide_inactive, "Hide all inactive features");
  ui.ctx().data_mut(|d| d.insert_temp(settings_id(), s));
}

/// The swatch colour of the item that made component `c`.
fn swatch_color(c: u32) -> Color32 {
  const COLORS: [fox_brand::Rgb; 5] =
    [fox_brand::BLUE.base, fox_brand::ORANGE.base, fox_brand::GREEN.base, fox_brand::PURPLE.base, fox_brand::RED.base];
  theme::color(COLORS[c as usize % COLORS.len()])
}

/// Draws the Component Color Swatch under the item `rect` of feature `id`, when it is on.
pub(crate) fn swatch(app: &App, ui: &egui::Ui, rect: Rect, id: FeatureId) {
  if !settings(ui.ctx()).swatch {
    return;
  }
  // the item's component; an item of the top level that made a component takes that one's
  let Some(c) = app.s.design.feature(id).and_then(|f| owner(app, f).or(f.component)) else { return };
  let bar = Rect::from_min_max(rect.left_bottom() + egui::vec2(3.0, -3.0), rect.right_bottom() - egui::vec2(3.0, 1.0));
  ui.painter().rect_filled(bar, 0.0, swatch_color(c.0));
}

fn failed_id() -> egui::Id {
  egui::Id::new("timeline-failed")
}

/// Notes whether feature `id` computes (`error`) while it isn't suppressed or rolled back, and
/// returns whether it failed when it was last computed: a suppressed feature isn't computed, so
/// this is what tells a suppressed failure (pink square) from a suppressed healthy feature.
pub(crate) fn failed(ctx: &egui::Context, id: FeatureId, computed: Option<bool>) -> bool {
  ctx.data_mut(|d| {
    let set = d.get_temp_mut_or_default::<HashSet<FeatureId>>(failed_id());
    match computed {
      Some(true) => {
        set.insert(id);
        true
      }
      Some(false) => {
        set.remove(&id);
        false
      }
      None => set.contains(&id),
    }
  })
}

/// The component feature `f` belongs to, while it still exists (`None`: the top level).
fn owner(app: &App, f: &fox_engine::model::Feature) -> Option<fox_engine::model::ComponentId> {
  f.owner.filter(|c| app.s.design.component(*c).is_some())
}

/// The name of the component feature `id` belongs to, as its tooltip shows it (the top level's
/// "(Unsaved)" until saved).
pub(crate) fn owner_label(app: &App, id: FeatureId) -> String {
  match app.s.design.feature(id).and_then(|f| owner(app, f)).and_then(|c| app.s.design.component(c)) {
    Some(c) => c.name.clone(),
    None => super::browser_rows::root_label(app),
  }
}

/// The design indices of the features Hide all inactive features leaves out (`hide`): while a
/// component is active, those that belong to neither it nor a component inside it.
pub(crate) fn inactive(app: &App, hide: bool) -> Vec<usize> {
  let Some(active) = app.s.active_component.filter(|c| app.s.design.component(*c).is_some()).filter(|_| hide) else {
    return Vec::new();
  };
  let inside = fox_engine::model::explode::subtree(&app.s.design, active);
  let d = &app.s.design;
  d.features
    .iter()
    .enumerate()
    .filter(|(_, f)| !owner(app, f).is_some_and(|c| inside.contains(&c)))
    .map(|(i, _)| i)
    .collect()
}

#[cfg(test)]
mod tests {
  use fox_engine::Session;
  use serde_json::json;

  use crate::App;

  /// A feature belongs to the component active when it was made: its tooltip names that
  /// component, Hide all inactive features leaves out the other components' features while one
  /// is active, and nothing is hidden at the top level.
  #[test]
  fn features_belong_to_the_component_they_were_made_in() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    let r = app.run("component.create", json!({"name": "Bracket", "activate": true})).unwrap();
    let c = fox_engine::model::ComponentId(r["component"].as_u64().unwrap() as u32);
    app.run("box", json!({"width": 5, "depth": 5, "height": 5, "at": [30, 0]})).unwrap();
    let (first, second) = (app.s.design.features[0].id, app.s.design.features[1].id);
    assert_eq!(app.s.design.features[0].owner, None, "made at the top level");
    assert_eq!(app.s.design.features[1].owner, Some(c), "made in Bracket");
    assert_eq!(super::owner_label(&app, first), "(Unsaved)");
    assert_eq!(super::owner_label(&app, second), "Bracket");
    assert_eq!(super::inactive(&app, true), vec![0], "the top level's box is inactive");
    assert!(super::inactive(&app, false).is_empty(), "the setting off hides nothing");
    let hidden = crate::panels::hidden_items(&app, app.s.design.active_len()).with_owned(super::inactive(&app, true));
    assert_eq!(hidden.shown(2), 1, "one item shows");
    app.run("component.activate", json!({"component": null})).unwrap();
    assert!(super::inactive(&app, true).is_empty(), "the top level active: every item shows");
    // the owner is saved with the design
    let text = serde_json::to_string(&app.s.design.features[1]).unwrap();
    let back: fox_engine::model::Feature = serde_json::from_str(&text).unwrap();
    assert_eq!(back.owner, Some(c));
  }
}
