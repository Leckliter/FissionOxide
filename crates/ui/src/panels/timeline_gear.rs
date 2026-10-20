//! The gear at the timeline's right end, as in Fusion: Change to Direct Modeling (stop capturing
//! design history, after a confirmation; Capture Design History turns it back on), Component
//! Color Swatch (a colour strip per component under the icons) and Hide all inactive features
//! (checked by default: while a component is active, other components' features leave the
//! timeline). Rolling the marker to either end is the playback buttons' job, not the gear's.

use egui::{Color32, Rect, RichText, Stroke, vec2};
use fox_engine::model::{ComponentId, FeatureId};

use crate::{App, config_ui, marking, theme};

/// What a gear menu entry does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GearCmd {
  /// Change to Direct Modeling (asks first) or, with history off, Capture Design History.
  History,
  ComponentSwatch,
  HideInactive,
}

/// One entry: its label, whether it shows a check (and which), and what it does.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GearItem {
  pub label: &'static str,
  pub checked: Option<bool>,
  pub cmd: GearCmd,
}

/// The gear menu, as Fusion lists it.
pub(crate) fn items(app: &App) -> Vec<GearItem> {
  let history = if app.s.design.history_off { "Capture Design History" } else { "Change to Direct Modeling" };
  vec![
    GearItem { label: history, checked: None, cmd: GearCmd::History },
    GearItem {
      label: "Component Color Swatch",
      checked: Some(app.timeline_view.swatch),
      cmd: GearCmd::ComponentSwatch,
    },
    GearItem {
      label: "Hide all inactive features",
      checked: Some(app.timeline_view.hide_inactive),
      cmd: GearCmd::HideInactive,
    },
  ]
}

/// The timeline's view settings and the confirmation the gear may be waiting on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TimelineView {
  pub swatch: bool,
  pub hide_inactive: bool,
  /// Change to Direct Modeling is asking whether to go ahead.
  pub confirm_direct: bool,
}

impl Default for TimelineView {
  fn default() -> Self {
    TimelineView { swatch: false, hide_inactive: true, confirm_direct: false }
  }
}

/// Runs a gear entry. Turning history off only asks; [`confirm_direct`] does it.
pub(crate) fn run(app: &mut App, cmd: GearCmd) {
  match cmd {
    GearCmd::History if app.s.design.history_off => config_ui::toggle_history(app),
    GearCmd::History => app.timeline_view.confirm_direct = true,
    GearCmd::ComponentSwatch => app.timeline_view.swatch ^= true,
    GearCmd::HideInactive => app.timeline_view.hide_inactive ^= true,
  }
}

/// The answer to Change to Direct Modeling's question: yes stops capturing history.
pub(crate) fn confirm_direct(app: &mut App, yes: bool) {
  app.timeline_view.confirm_direct = false;
  if yes && !app.s.design.history_off {
    config_ui::toggle_history(app);
  }
}

/// The gear's drop-down.
pub(super) fn menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(230.0);
  for it in items(app) {
    let clicked = match it.checked {
      Some(mut on) => ui.checkbox(&mut on, it.label).clicked(),
      None => ui.button(it.label).clicked(),
    };
    if clicked {
      ui.close();
      run(app, it.cmd);
    }
  }
}

/// Change to Direct Modeling's confirmation window.
pub fn confirm_window(app: &mut App, ctx: &egui::Context) {
  if !app.timeline_view.confirm_direct {
    return;
  }
  let mut answer = None;
  egui::Window::new("Change to Direct Modeling")
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
    .show(ctx, |ui| {
      ui.set_max_width(380.0);
      ui.label(
        "Design history will stop being captured. The timeline's features become a base feature, \
         and later changes are direct edits. Do you want to continue?",
      );
      ui.label(RichText::new("Undo restores the timeline.").color(theme::MUTED));
      ui.add_space(6.0);
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.button("No").clicked() {
          answer = Some(false);
        }
        if ui.button("Yes").clicked() {
          answer = Some(true);
        }
      });
    });
  if let Some(yes) = answer {
    confirm_direct(app, yes);
  }
}

/// The component a feature's bodies belong to.
pub(super) fn feature_component(app: &App, f: FeatureId) -> Option<ComponentId> {
  marking::feature_bodies(app, f).iter().find_map(|b| app.s.design.component_of(*b).map(|c| c.id))
}

/// Whether Hide all inactive features takes this feature out of the timeline: a component is
/// active and the feature made bodies of another one.
pub(super) fn hidden(app: &App, f: FeatureId) -> bool {
  let Some(active) = app.s.active_component else { return false };
  app.timeline_view.hide_inactive && feature_component(app, f).is_some_and(|c| c != active)
}

/// A component's swatch colour (fixed per component, so it doesn't change as features move).
pub(super) fn swatch_color(c: ComponentId) -> Color32 {
  const PALETTE: [Color32; 8] = [
    Color32::from_rgb(52, 120, 198),
    Color32::from_rgb(226, 132, 38),
    Color32::from_rgb(76, 160, 72),
    Color32::from_rgb(196, 60, 60),
    Color32::from_rgb(138, 92, 186),
    Color32::from_rgb(40, 160, 160),
    Color32::from_rgb(200, 170, 40),
    Color32::from_rgb(160, 96, 64),
  ];
  PALETTE[c.0 as usize % PALETTE.len()]
}

/// The swatch strip under a timeline icon.
pub(super) fn paint_swatch(app: &App, p: &egui::Painter, icon: Rect, f: FeatureId) {
  if !app.timeline_view.swatch {
    return;
  }
  let color = feature_component(app, f).map(swatch_color).unwrap_or(Color32::from_gray(150));
  let strip = Rect::from_min_max(icon.left_bottom() + vec2(2.0, -3.0), icon.right_bottom() + vec2(-2.0, 0.0));
  p.rect(strip, 0.0, color, Stroke::NONE, egui::StrokeKind::Inside);
}

#[cfg(test)]
mod tests {
  use fox_engine::Session;
  use serde_json::json;

  use super::*;

  fn labels(app: &App) -> Vec<&'static str> {
    items(app).iter().map(|i| i.label).collect()
  }

  #[test]
  fn the_gear_lists_fusions_three_entries_and_no_roll_items() {
    let app = App::with_session(Session::default());
    assert_eq!(labels(&app), ["Change to Direct Modeling", "Component Color Swatch", "Hide all inactive features"]);
    let checks: Vec<_> = items(&app).iter().map(|i| i.checked).collect();
    assert_eq!(checks, [None, Some(false), Some(true)], "Hide all inactive features is checked by default");
    assert!(!labels(&app).iter().any(|l| l.starts_with("Roll")), "the playback buttons roll the marker");
  }

  #[test]
  fn direct_modeling_asks_first_and_capture_history_turns_it_back() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    run(&mut app, GearCmd::History);
    assert!(app.timeline_view.confirm_direct, "a confirmation comes first");
    assert!(!app.s.design.history_off, "nothing changes until it's answered");
    confirm_direct(&mut app, false);
    assert!(!app.timeline_view.confirm_direct && !app.s.design.history_off, "No keeps the history");
    run(&mut app, GearCmd::History);
    confirm_direct(&mut app, true);
    assert!(app.s.design.history_off, "Yes stops capturing history");
    assert_eq!(labels(&app)[0], "Capture Design History");
    // back to parametric: no question asked
    run(&mut app, GearCmd::History);
    assert!(!app.timeline_view.confirm_direct && !app.s.design.history_off);
  }

  #[test]
  fn swatch_and_hide_inactive_toggle() {
    let mut app = App::with_session(Session::default());
    run(&mut app, GearCmd::ComponentSwatch);
    run(&mut app, GearCmd::HideInactive);
    let checks: Vec<_> = items(&app).iter().map(|i| i.checked).collect();
    assert_eq!(checks, [None, Some(true), Some(false)]);
  }

  #[test]
  fn hide_inactive_hides_other_components_features_while_one_is_active() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    let a = app.s.model.bodies[0].id.0;
    app.run("box", json!({"width": 5, "depth": 5, "height": 5, "at": [30, 30]}));
    let b = app.s.model.bodies[1].id.0;
    let ca = app.run("component.create", json!({"name": "A", "bodies": [a]})).unwrap()["component"].as_u64().unwrap();
    app.run("component.create", json!({"name": "B", "bodies": [b]}));
    let (box1, box2) = (app.s.design.features[0].id, app.s.design.features[1].id);
    assert!(!hidden(&app, box1) && !hidden(&app, box2), "no component active: everything shows");
    app.run("component.activate", json!({"component": ca}));
    assert!(!hidden(&app, box1), "the active component's feature shows");
    assert!(hidden(&app, box2), "another component's feature is hidden");
    run(&mut app, GearCmd::HideInactive);
    assert!(!hidden(&app, box2), "unchecked: inactive features show again");
    assert_ne!(
      feature_component(&app, box1).map(swatch_color),
      feature_component(&app, box2).map(swatch_color),
      "each component its own swatch"
    );
  }
}
