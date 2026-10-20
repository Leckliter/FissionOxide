//! The navigation bar's fly-outs: Orbit and Zoom choices, Display Settings (visual style,
//! object visibility, effects, camera) and Grid and Snaps, as in Fusion. The bar itself is
//! [`crate::panels::nav_bar`].

use egui::RichText;
use fox_engine::render::{Effects, VisualStyle};

use crate::{App, NavMode};

/// Display Settings ▸ Object Visibility: which kinds of things the viewport draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectVisibility {
  /// The origin axes on the grid.
  pub origins: bool,
  pub sketches: bool,
  /// Construction planes, axes and points.
  pub construction: bool,
  /// Joint glyphs and joint origins.
  pub joints: bool,
  pub bodies: bool,
}

impl Default for ObjectVisibility {
  fn default() -> Self {
    ObjectVisibility { origins: true, sketches: true, construction: true, joints: true, bodies: true }
  }
}

/// Grid and Snaps ▸ Grid Settings, and Layout Grid Lock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridSettings {
  /// The spacing follows the model's size; else it is `spacing`.
  pub adaptive: bool,
  /// Fixed spacing (mm).
  pub spacing: f64,
  /// Layout Grid Lock: the spacing frozen when the lock went on.
  pub locked: Option<f64>,
}

impl Default for GridSettings {
  fn default() -> Self {
    GridSettings { adaptive: true, spacing: 10.0, locked: None }
  }
}

impl GridSettings {
  /// The grid's spacing when it doesn't adapt (locked or fixed).
  pub fn step(&self) -> Option<f64> {
    self.locked.or((!self.adaptive).then_some(self.spacing)).filter(|s| s.is_finite() && *s > 0.0)
  }

  /// Turns Layout Grid Lock on, freezing the grid at `now` (the spacing it has), or off.
  pub fn set_lock(&mut self, on: bool, now: f64) {
    self.locked = on.then(|| self.step().unwrap_or(now));
  }
}

/// Grid and Snaps ▸ Incremental Move and Set Increments: manipulator drags move and turn in
/// these steps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Increments {
  pub on: bool,
  /// Linear step (mm).
  pub linear: f64,
  /// Angular step (degrees).
  pub angular: f64,
}

impl Default for Increments {
  fn default() -> Self {
    Increments { on: false, linear: 1.0, angular: 15.0 }
  }
}

impl Increments {
  /// The step a move snaps to: the linear increment with Incremental Move on, else `free`.
  pub fn linear_step(&self, free: f64) -> f64 {
    if self.on && self.linear > 0.0 { self.linear } else { free }
  }

  /// The step a turn snaps to (radians).
  pub fn angular_step(&self, free: f64) -> f64 {
    if self.on && self.angular > 0.0 { self.angular.to_radians() } else { free }
  }
}

/// The Orbit fly-out: Free Orbit lets the view roll and go over the top, Constrained Orbit keeps
/// +Z up. Choosing one starts orbiting.
pub fn orbit_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(170.0);
  for (free, label) in [(false, "Constrained Orbit"), (true, "Free Orbit")] {
    if ui.radio(app.free_orbit == free, label).clicked() {
      app.free_orbit = free;
      if !free {
        // back to +Z up, as Fusion's constrained orbit is
        let mut to = app.cam;
        to.roll = 0.0;
        app.fly_to(to);
      }
      app.nav_mode = Some(NavMode::Orbit);
      ui.close();
    }
  }
}

/// The fly-out after Fit: Fit, and Zoom Window (drag a box to zoom into).
pub fn fit_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(150.0);
  if ui.button("Fit").clicked() {
    app.fit_view();
    ui.close();
  }
  if ui.selectable_label(app.nav_mode == Some(NavMode::ZoomWindow), "Zoom Window").clicked() {
    app.nav_mode = Some(NavMode::ZoomWindow);
    ui.close();
  }
}

pub fn display_menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(220.0);
  ui.menu_button("Visual Style", |ui| {
    ui.set_min_width(250.0);
    for s in VisualStyle::ALL {
      if ui.radio(app.visual_style == s, s.label()).clicked() {
        app.visual_style = s;
      }
    }
  });
  ui.menu_button("Object Visibility", |ui| {
    ui.set_min_width(180.0);
    let v = &mut app.objects;
    ui.checkbox(&mut v.origins, "Origins");
    ui.checkbox(&mut v.sketches, "Sketches");
    ui.checkbox(&mut v.construction, "Construction Geometry");
    ui.checkbox(&mut v.joints, "Joints");
    ui.checkbox(&mut v.bodies, "Bodies");
  });
  ui.menu_button("Effects", |ui| {
    ui.set_min_width(160.0);
    let e: &mut Effects = &mut app.effects;
    ui.checkbox(&mut e.ground_shadow, "Ground Shadow");
    ui.checkbox(&mut e.reflections, "Reflections");
    ui.checkbox(&mut e.ground_plane, "Ground Plane");
  });
  ui.separator();
  ui.label(RichText::new("Camera").strong());
  if ui.radio(app.cam.ortho, "Orthographic").clicked() {
    app.cam.ortho = true;
  }
  if ui.radio(!app.cam.ortho, "Perspective").clicked() {
    app.cam.ortho = false;
  }
  ui.separator();
  ui.checkbox(&mut app.show_viewcube, "ViewCube");
  let full = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
  if ui.selectable_label(full, "Full Screen").clicked() {
    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
    ui.close();
  }
  ui.separator();
  crate::gpu::settings(app, ui);
}

/// Grid and Snaps. `adaptive_now` is the spacing the adaptive grid has (what a lock freezes).
pub fn grid_menu(app: &mut App, ui: &mut egui::Ui, adaptive_now: f64) {
  ui.set_min_width(190.0);
  ui.checkbox(&mut app.show_grid, "Layout Grid");
  let mut lock = app.grid.locked.is_some();
  if ui.checkbox(&mut lock, "Layout Grid Lock").on_hover_text("Keep the grid's spacing as it is now").changed() {
    app.grid.set_lock(lock, adaptive_now);
  }
  ui.checkbox(&mut app.snap, "Snap to Grid");
  ui.menu_button("Grid Settings", |ui| {
    ui.set_min_width(200.0);
    let g = &mut app.grid;
    ui.radio_value(&mut g.adaptive, true, "Adaptive");
    ui.radio_value(&mut g.adaptive, false, "Fixed");
    ui.add_enabled_ui(!g.adaptive, |ui| {
      ui.horizontal(|ui| {
        ui.label("Spacing");
        ui.add(egui::DragValue::new(&mut g.spacing).range(0.01..=10_000.0).speed(0.5).suffix(" mm"));
      });
    });
    if g.locked.is_some() {
      ui.label(RichText::new("Layout Grid Lock holds the spacing").small().weak());
    }
  });
  ui.separator();
  ui.checkbox(&mut app.increments.on, "Incremental Move")
    .on_hover_text("Manipulator drags move and turn in the set increments");
  ui.menu_button("Set Increments", |ui| {
    ui.set_min_width(200.0);
    let inc = &mut app.increments;
    egui::Grid::new("increments").num_columns(2).show(ui, |ui| {
      ui.label("Linear");
      ui.add(egui::DragValue::new(&mut inc.linear).range(0.001..=10_000.0).speed(0.1).suffix(" mm"));
      ui.end_row();
      ui.label("Angular");
      ui.add(egui::DragValue::new(&mut inc.angular).range(0.01..=180.0).speed(0.5).suffix("°"));
      ui.end_row();
    });
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn grid_lock_freezes_the_spacing_it_has() {
    let mut g = GridSettings::default();
    assert_eq!(g.step(), None, "adaptive");
    g.set_lock(true, 5.0);
    assert_eq!(g.step(), Some(5.0));
    // switching to fixed doesn't move a locked grid
    g.adaptive = false;
    g.spacing = 2.0;
    assert_eq!(g.step(), Some(5.0));
    g.set_lock(false, 5.0);
    assert_eq!(g.step(), Some(2.0));
    g.set_lock(true, 50.0);
    assert_eq!(g.step(), Some(2.0), "a fixed grid locks at its own spacing");
    g.spacing = 0.0;
    g.locked = None;
    assert_eq!(g.step(), None, "no zero spacing");
  }

  #[test]
  fn increments_apply_only_with_incremental_move() {
    let mut i = Increments::default();
    assert_eq!(i.linear_step(0.25), 0.25);
    assert_eq!(i.angular_step(0.1), 0.1);
    i.on = true;
    assert_eq!(i.linear_step(0.25), 1.0);
    assert!((i.angular_step(0.1) - 15f64.to_radians()).abs() < 1e-12);
  }
}
