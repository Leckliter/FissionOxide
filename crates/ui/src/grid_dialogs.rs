//! Grid and Snaps' two dialogs, docked where Fusion docks its command dialogs: GRID SETTINGS
//! (Adaptive or Fixed spacing, Reference Numbers) and SET INCREMENTS (Adaptive or Fixed steps for
//! manipulator drags). Each edits a copy of the settings; OK (or Enter) keeps it, Cancel or
//! Escape drops it.

use egui::RichText;

use crate::icons::{self, Icon};
use crate::{App, panels, theme};

/// The open dialog and the values it edits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum GridDialog {
  Settings { adaptive: bool, spacing: f64, reference_numbers: bool },
  Increments { adaptive: bool, linear: f64, angular: f64 },
}

impl GridDialog {
  fn title(&self) -> &'static str {
    match self {
      GridDialog::Settings { .. } => "GRID SETTINGS",
      GridDialog::Increments { .. } => "SET INCREMENTS",
    }
  }

  fn icon(&self) -> Icon {
    match self {
      GridDialog::Settings { .. } => Icon::GridSettings,
      GridDialog::Increments { .. } => Icon::SetIncrements,
    }
  }
}

/// Grid and Snaps ▸ Grid Settings: opens GRID SETTINGS on the current settings.
pub(crate) fn open_grid_settings(app: &mut App) {
  let g = app.grid;
  app.grid_dialog =
    Some(GridDialog::Settings { adaptive: g.adaptive, spacing: g.spacing, reference_numbers: g.reference_numbers });
}

/// Grid and Snaps ▸ Set Increments: opens SET INCREMENTS on the current increments.
pub(crate) fn open_set_increments(app: &mut App) {
  let i = app.increments;
  app.grid_dialog = Some(GridDialog::Increments { adaptive: i.adaptive, linear: i.linear, angular: i.angular });
}

/// OK: the dialog's values become the settings and it closes. A locked grid is locked again at
/// the spacing the new settings give it, so a Fixed spacing shows at once.
pub(crate) fn ok(app: &mut App) {
  match app.grid_dialog.take() {
    Some(GridDialog::Settings { adaptive, spacing, reference_numbers }) => {
      let locked = app.grid.locked.is_some();
      app.grid.locked = None;
      app.grid.adaptive = adaptive;
      if spacing.is_finite() && spacing > 0.0 {
        app.grid.spacing = spacing;
      }
      app.grid.reference_numbers = reference_numbers;
      if locked {
        app.grid.set_lock(true, fox_engine::scene::grid_step(&app.s));
      }
    }
    Some(GridDialog::Increments { adaptive, linear, angular }) => {
      let inc = &mut app.increments;
      inc.adaptive = adaptive;
      if linear.is_finite() && linear > 0.0 {
        inc.linear = linear;
      }
      if angular.is_finite() && angular > 0.0 {
        inc.angular = angular;
      }
    }
    None => {}
  }
}

/// The open dialog, docked at the canvas's top right as the feature dialogs are.
pub(crate) fn window(app: &mut App, ctx: &egui::Context) {
  let Some(mut d) = app.grid_dialog else { return };
  let id = egui::Id::new("grid-dialog");
  let layer = egui::LayerId::new(egui::Order::Foreground, id);
  let mut done = ctx.input(|i| i.key_pressed(egui::Key::Enter))
    && match ctx.memory(|m| m.focused()) {
      None => true,
      Some(f) => ctx.read_response(f).is_some_and(|r| r.layer_id == layer),
    };
  let mut cancel = false;
  let width = 300.0;
  let accent = icons::accent_of(d.icon());
  egui::Area::new(id)
    .fixed_pos(panels::dialog_dock(app.canvas, app.s.edit_sketch.is_some()))
    .pivot(egui::Align2::RIGHT_TOP)
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      panels::dialog_frame().show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        panels::dialog_header(ui, d.icon(), d.title(), width);
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
          fields(ui, &mut d);
        });
        let sep = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover()).0;
        ui.painter().rect_filled(sep, 0.0, theme::SEPARATOR);
        egui::Frame::new().fill(theme::DIALOG_BODY).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
          ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
              if panels::secondary_button(ui, "Cancel").clicked() {
                cancel = true;
              }
              if panels::primary_button(ui, "OK", accent).clicked() {
                done = true;
              }
            });
          });
        });
      });
    });
  app.grid_dialog = Some(d);
  if cancel {
    app.grid_dialog = None;
  } else if done {
    ok(app);
  }
}

/// The dialog's rows: the Adaptive and Fixed radios, the values Fixed reveals, and GRID
/// SETTINGS' Reference Numbers.
fn fields(ui: &mut egui::Ui, d: &mut GridDialog) {
  let value = |ui: &mut egui::Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>, suffix: &str| {
    ui.horizontal(|ui| {
      ui.add_space(22.0);
      ui.add_sized([120.0, 20.0], egui::Label::new(label));
      ui.add(egui::DragValue::new(v).range(range).speed(0.1).suffix(suffix.to_string()));
    });
  };
  match d {
    GridDialog::Settings { adaptive, spacing, reference_numbers } => {
      ui.radio_value(adaptive, true, "Adaptive");
      ui.radio_value(adaptive, false, "Fixed");
      if !*adaptive {
        value(ui, "Spacing", spacing, 0.01..=10_000.0, " mm");
      }
      ui.add_space(2.0);
      ui.checkbox(reference_numbers, "Reference Numbers")
        .on_hover_text(RichText::new("The coordinate values along the grid's axes"));
    }
    GridDialog::Increments { adaptive, linear, angular } => {
      ui.radio_value(adaptive, true, "Adaptive");
      ui.radio_value(adaptive, false, "Fixed");
      if !*adaptive {
        value(ui, "Linear Increment", linear, 0.001..=10_000.0, " mm");
        value(ui, "Angular Increment", angular, 0.01..=180.0, " deg");
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) {
    let input = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0))),
      events,
      ..Default::default()
    };
    ctx.run_ui(input, |ui| app.draw(ui)).drop_without_applying_deltas();
  }

  fn key(k: egui::Key) -> egui::Event {
    egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }
  }

  #[test]
  fn grid_settings_opens_on_the_settings_and_ok_keeps_the_changes() {
    let mut app = App::with_session(fox_engine::Session::default());
    open_grid_settings(&mut app);
    assert_eq!(
      app.grid_dialog,
      Some(GridDialog::Settings { adaptive: true, spacing: 10.0, reference_numbers: true }),
      "Adaptive and Reference Numbers on, as Fusion opens it"
    );
    assert_eq!(app.grid_dialog.unwrap().title(), "GRID SETTINGS");
    app.grid_dialog = Some(GridDialog::Settings { adaptive: false, spacing: 2.5, reference_numbers: false });
    ok(&mut app);
    assert!(app.grid_dialog.is_none());
    assert_eq!(app.grid.step(), Some(2.5));
    assert!(!app.grid.reference_numbers);
    // with Layout Grid Lock on, OK re-locks the grid at the new spacing
    app.grid.set_lock(true, 7.0);
    open_grid_settings(&mut app);
    if let Some(GridDialog::Settings { spacing, .. }) = &mut app.grid_dialog {
      *spacing = 4.0;
    }
    ok(&mut app);
    assert_eq!(app.grid.locked, Some(4.0));
  }

  #[test]
  fn set_increments_fixed_sets_the_steps_and_adaptive_follows_the_grid() {
    let mut app = App::with_session(fox_engine::Session::default());
    open_set_increments(&mut app);
    assert_eq!(app.grid_dialog, Some(GridDialog::Increments { adaptive: true, linear: 1.0, angular: 15.0 }));
    assert_eq!(app.grid_dialog.unwrap().title(), "SET INCREMENTS");
    app.grid_dialog = Some(GridDialog::Increments { adaptive: false, linear: 0.5, angular: 5.0 });
    ok(&mut app);
    assert_eq!(app.increments.linear_step(3.0), 0.5);
    assert!((app.increments.angular_step(0.1) - 5f64.to_radians()).abs() < 1e-12);
    open_set_increments(&mut app);
    if let Some(GridDialog::Increments { adaptive, .. }) = &mut app.grid_dialog {
      *adaptive = true;
    }
    ok(&mut app);
    assert_eq!(app.increments.linear_step(3.0), 3.0, "adaptive: the grid's step");
  }

  #[test]
  fn escape_cancels_and_enter_is_ok() {
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut app = App::with_session(fox_engine::Session::default());
    for _ in 0..2 {
      frame(&mut app, &ctx, vec![]);
    }
    open_grid_settings(&mut app);
    frame(&mut app, &ctx, vec![]);
    assert!(ctx.memory(|m| m.area_rect(egui::Id::new("grid-dialog"))).is_some(), "docked dialog shown");
    app.grid_dialog = Some(GridDialog::Settings { adaptive: false, spacing: 3.0, reference_numbers: true });
    frame(&mut app, &ctx, vec![key(egui::Key::Escape)]);
    assert!(app.grid_dialog.is_none(), "Escape closes it");
    assert!(app.grid.adaptive, "and drops its changes");
    open_set_increments(&mut app);
    app.grid_dialog = Some(GridDialog::Increments { adaptive: false, linear: 2.0, angular: 15.0 });
    frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    assert!(app.grid_dialog.is_none(), "Enter is OK");
    assert!(!app.increments.adaptive);
    assert_eq!(app.increments.linear, 2.0);
  }
}
