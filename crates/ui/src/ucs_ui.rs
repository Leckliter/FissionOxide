//! The canvas's user coordinate systems (the engine's `ucs.*`): each shown one drawn as a small
//! X (red), Y (green), Z (blue) triad at its origin with its name, while the navigation bar's
//! Object Visibility ▸ User Coordinate Systems is on.

use egui::{Color32, Pos2, Stroke, vec2};
use fox_engine::kernel::glam::DVec3;

use crate::App;

/// The triad's longest arm on screen (points).
const ARM_PX: f32 = 42.0;
const COLORS: [Color32; 3] =
  [Color32::from_rgb(220, 70, 60), Color32::from_rgb(60, 165, 70), Color32::from_rgb(45, 100, 230)];

/// Each arm's end on screen, the longest `ARM_PX` from the origin: the axes keep their
/// foreshortening, so the triad turns with the view.
pub(crate) fn arms(
  origin: DVec3,
  axes: [DVec3; 3],
  project: &dyn Fn(DVec3) -> Option<Pos2>,
) -> Option<(Pos2, [Pos2; 3])> {
  let o = project(origin)?;
  // a step small enough to stay in front of the camera, large enough to measure
  let step = 1e-2;
  let v: Vec<egui::Vec2> = axes.iter().filter_map(|a| project(origin + *a * step).map(|p| p - o)).collect();
  let longest = v.iter().map(|d| d.length()).fold(0.0f32, f32::max);
  if v.len() < 3 || longest < 1e-9 {
    return None;
  }
  let k = ARM_PX / longest;
  Some((o, [o + v[0] * k, o + v[1] * k, o + v[2] * k]))
}

pub(crate) fn overlay(app: &App, painter: &egui::Painter, project: &dyn Fn(DVec3) -> Option<Pos2>) {
  if !app.objects.user_coordinate_systems {
    return;
  }
  for u in app.s.design.user_coordinate_systems.iter().filter(|u| u.visible) {
    let Some((o, ends)) = arms(DVec3::from(u.origin), u.axes(), project) else { continue };
    for (end, c) in ends.iter().zip(COLORS) {
      painter.line_segment([o, *end], Stroke::new(2.0, c));
    }
    painter.circle(o, 3.0, Color32::WHITE, Stroke::new(1.0, Color32::from_gray(90)));
    painter.text(
      o + vec2(6.0, 6.0),
      egui::Align2::LEFT_TOP,
      &u.name,
      egui::FontId::proportional(11.0),
      crate::theme::TEXT,
    );
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_triad_keeps_a_fixed_size_on_screen() {
    // a flat projection: x right, y up, z out of the screen
    let project = |p: DVec3| Some(Pos2::new(100.0 + p.x as f32 * 3.0, 100.0 - p.y as f32 * 3.0));
    let (o, [x, y, z]) = arms(DVec3::ZERO, [DVec3::X, DVec3::Y, DVec3::Z], &project).unwrap();
    assert_eq!(o, Pos2::new(100.0, 100.0));
    assert!((x - Pos2::new(100.0 + ARM_PX, 100.0)).length() < 1e-3);
    assert!((y - Pos2::new(100.0, 100.0 - ARM_PX)).length() < 1e-3);
    assert!((z - o).length() < 1e-3, "Z points at the viewer");
    // the shown ones are drawn only with the row on
    let mut app = App::with_session(fox_engine::Session::default());
    app.run("ucs.create", serde_json::json!({}));
    assert_eq!(app.s.design.user_coordinate_systems.len(), 1);
    let mut v = app.objects;
    assert!(v.rows().iter().any(|(l, _, drawn)| *l == "User Coordinate Systems" && *drawn), "the row isn't greyed");
  }
}
