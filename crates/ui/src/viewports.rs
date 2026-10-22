//! The navigation bar's Viewports fly-out, as Fusion's: Single View, or Multiple Views, which
//! splits the canvas into four views of the design (Top, Front and Right, and the view that was
//! showing). One view is active at a time: it is the one the mouse, the ViewCube, the navigation
//! bar and every command work in, and clicking another one makes that one active. The others
//! keep their own cameras and are drawn with each change of the design.

use egui::{Color32, Pos2, Rect, Stroke, vec2};
use fox_engine::render::{Camera, StandardView};

use crate::{App, RenderKey, theme};

/// The four views' names, in their places: top left, top right, bottom left, bottom right.
pub const NAMES: [&str; 4] = ["Top", "Front", "Right", "Home"];
const VIEWS: [StandardView; 3] = [StandardView::Top, StandardView::Front, StandardView::Right];

/// The gap between the views.
const GAP: f32 = 2.0;

pub struct Viewports {
  /// Multiple Views is on.
  pub multiple: bool,
  /// The active view (its camera is `App::cam`).
  pub active: usize,
  /// The other views' cameras (the active one's is kept in `App::cam` instead).
  pub cams: [Camera; 4],
  /// The whole canvas, split between the views.
  pub full: Rect,
  /// The CPU renderer's pictures of the other views, and what each was drawn from.
  cpu: [Option<(RenderKey, egui::TextureHandle)>; 4],
}

impl Default for Viewports {
  fn default() -> Self {
    Viewports { multiple: false, active: 0, cams: [Camera::default(); 4], full: Rect::NOTHING, cpu: Default::default() }
  }
}

/// Single View (`false`) or Multiple Views. Turning Multiple Views on keeps the current view in
/// the bottom-right place, active, and fits Top, Front and Right to the design.
pub fn set_multiple(app: &mut App, on: bool) {
  if app.viewports.multiple == on {
    return;
  }
  app.viewports.multiple = on;
  app.viewports.cpu = Default::default();
  if !on {
    return;
  }
  for (i, v) in VIEWS.into_iter().enumerate() {
    let mut cam = Camera { ortho: true, ..Camera::default() };
    let (dir, up) = app.cube.standard_view(v);
    cam.look_from(dir, up);
    app.fit_cam(&mut cam);
    app.viewports.cams[i] = cam;
  }
  app.viewports.active = 3;
  app.viewports.cams[3] = app.cam;
}

/// Makes view `i` the active one: its camera becomes the one the canvas works with.
pub fn activate(app: &mut App, i: usize) {
  let v = &mut app.viewports;
  if !v.multiple || i == v.active || i >= 4 {
    return;
  }
  app.cam_move = None;
  v.cams[v.active] = app.cam;
  v.active = i;
  app.cam = v.cams[i];
}

/// The places of the four views in `full`.
pub fn rects(full: Rect) -> [Rect; 4] {
  let mid = full.center();
  let h = GAP / 2.0;
  [
    Rect::from_min_max(full.min, Pos2::new(mid.x - h, mid.y - h)),
    Rect::from_min_max(Pos2::new(mid.x + h, full.min.y), Pos2::new(full.max.x, mid.y - h)),
    Rect::from_min_max(Pos2::new(full.min.x, mid.y + h), Pos2::new(mid.x - h, full.max.y)),
    Rect::from_min_max(Pos2::new(mid.x + h, mid.y + h), full.max),
  ]
}

/// The whole canvas: with Multiple Views on, all four views (the navigation bar stays centred
/// under them), else the canvas.
pub fn canvas(app: &App) -> Rect {
  if app.viewports.multiple { app.viewports.full } else { app.canvas }
}

/// Lays the views out in the canvas `full`: draws the views that aren't active (a click in one
/// makes it active) and returns where the active one goes.
pub fn show_others(app: &mut App, ui: &mut egui::Ui, full: Rect) -> Rect {
  app.viewports.full = full;
  if !app.viewports.multiple {
    return full;
  }
  let places = rects(full);
  let painter = ui.painter_at(full);
  painter.rect_filled(full, 0.0, theme::BORDER);
  let ppp = ui.ctx().pixels_per_point();
  // the CPU renderer's scene, built once for the views it draws again
  let mut scene = None;
  for (i, r) in places.into_iter().enumerate() {
    if i == app.viewports.active {
      continue;
    }
    let resp = ui.allocate_rect(r, egui::Sense::click_and_drag());
    let cam = app.viewports.cams[i];
    let size = [((r.width() * ppp).round() as u32).max(1), ((r.height() * ppp).round() as u32).max(1)];
    let p = ui.painter_at(r);
    if !crate::gpu::paint_view(app, &p, r, cam, size) {
      cpu_view(app, ui.ctx(), &p, (i, r, size), &mut scene);
    }
    crate::viewcube::paint_passive(app, &p, r, &cam);
    p.text(
      r.left_top() + vec2(10.0, 8.0),
      egui::Align2::LEFT_TOP,
      NAMES[i].to_uppercase(),
      egui::FontId::proportional(11.0),
      theme::MUTED,
    );
    if resp.clicked() || resp.drag_started() {
      activate(app, i);
      ui.ctx().request_repaint();
    }
  }
  let active = places[app.viewports.active];
  // the active view is outlined in the accent blue
  ui.painter().rect_stroke(active, 0.0, Stroke::new(GAP, theme::ACCENT), egui::StrokeKind::Outside);
  active
}

/// The CPU renderer's picture of view `i` at `r`, `size` pixels, drawn again when the design or
/// its size changes (from `scene`, built by the first view that needs it).
fn cpu_view(
  app: &mut App,
  ctx: &egui::Context,
  painter: &egui::Painter,
  (i, r, size): (usize, Rect, [u32; 2]),
  scene: &mut Option<fox_engine::render::Scene>,
) {
  let Some(key) = app.render_key.clone() else { return };
  let key = RenderKey { cam: app.viewports.cams[i], size, hover: None, ..key };
  let fresh = app.viewports.cpu[i].as_ref().is_some_and(|(k, _)| *k == key);
  if !fresh {
    let scene = scene.get_or_insert_with(|| {
      let opts = crate::viewport::scene_options(app);
      let (mut scene, _) = app.scene.build(&app.s, &opts);
      scene.style = app.visual_style;
      app.effects.apply(&mut scene);
      if theme::is_dark(ctx) {
        let (top, bottom) = theme::dark_canvas();
        let plain = scene.background_top == scene.background_bottom;
        (scene.background_top, scene.background_bottom) = (top, if plain { top } else { bottom });
      }
      scene
    });
    let frame = fox_engine::render::render(scene, &key.cam, size[0], size[1]);
    let img = egui::ColorImage::from_rgba_unmultiplied([frame.width as usize, frame.height as usize], &frame.rgba);
    let tex = ctx.load_texture(format!("viewport-{i}"), img, egui::TextureOptions::LINEAR);
    app.viewports.cpu[i] = Some((key, tex));
  }
  if let Some((_, tex)) = &app.viewports.cpu[i] {
    painter.image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
  }
}

/// The Viewports fly-out: Single View and Multiple Views, the current one ticked.
pub fn menu(app: &mut App, ui: &mut egui::Ui) {
  let multiple = app.viewports.multiple;
  if ui.selectable_label(!multiple, "Single View").clicked() {
    set_multiple(app, false);
    ui.close();
  }
  if ui.selectable_label(multiple, "Multiple Views").clicked() {
    set_multiple(app, true);
    ui.close();
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;
  use serde_json::json;

  #[test]
  fn multiple_views_split_the_canvas_into_four_with_the_view_that_was_showing_active() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    let home = app.cam;
    set_multiple(&mut app, true);
    assert_eq!(app.viewports.active, 3);
    assert_eq!(app.cam, home, "the view that was showing stays, bottom right");
    // Top looks down, Front along +Y, Right along -X, all three orthographic
    let [top, front, right, _] = app.viewports.cams;
    assert!(top.eye_dir().z > 0.99 && top.ortho);
    assert!(front.eye_dir().y < -0.99 && front.ortho);
    assert!(right.eye_dir().x > 0.99 && right.ortho);
    // a click in Front makes it the active view; Home keeps where it was
    app.cam.orbit(0.2, 0.1);
    let moved = app.cam;
    activate(&mut app, 1);
    assert_eq!(app.cam, front);
    assert_eq!(app.viewports.cams[3], moved);
    activate(&mut app, 3);
    assert_eq!(app.cam, moved);
    // Single View keeps the active one
    set_multiple(&mut app, false);
    assert!(!app.viewports.multiple);
    assert_eq!(app.cam, moved);
  }

  #[test]
  fn the_four_views_share_the_canvas() {
    let full = Rect::from_min_size(Pos2::new(0.0, 100.0), vec2(800.0, 600.0));
    let r = rects(full);
    assert_eq!(r[0].min, full.min);
    assert_eq!(r[3].max, full.max);
    assert!((r[0].width() - (400.0 - GAP / 2.0)).abs() < 1e-3 && (r[1].min.x - (400.0 + GAP / 2.0)).abs() < 1e-3);
    assert!(r.iter().all(|a| r.iter().all(|b| a == b || !a.intersects(b.shrink(0.5)))));
  }
}
