//! The viewport's GPU renderer: [`fox_render_gl`] drawn through an egui paint callback.
//!
//! The scene is the same [`fox_engine::render::Scene`] the CPU rasterizer draws; it is built only
//! when something other than the camera changes and uploaded once, so orbiting, panning and
//! zooming only redraw what's on the GPU. Hover-only changes keep the uploaded meshes and just
//! change their highlight. Picking reads the GPU's id buffer back into `App::frame` whenever the
//! image settles (not during orbit/pan/zoom drags, which don't pick), so `Frame::pick` and
//! window selection work unchanged, with the pick table that belongs to those ids. While the
//! view moves, distant parts of large models are drawn at a coarser level of detail; the settled
//! image, and the ids read from it, are always at full detail.
//!
//! The CPU rasterizer takes over when the setting is off (`FOX_RENDERER=cpu` turns it off at
//! start), when eframe has no OpenGL context (headless, tests) and when the GPU renderer fails to
//! start (no OpenGL 3.3 / ES 3.0, shader errors): the error shows in the status bar and in the
//! Display Settings menu.

use std::sync::{Arc, Mutex};

use eframe::{egui_glow, glow};
use egui::{Color32, Rect};
use fox_engine::render::{Camera, Frame, Scene};
use fox_engine::scene::PickTable;
use fox_render_gl::Renderer;

use crate::viewport::Drag;
use crate::{App, RenderKey};

pub struct Gpu {
  /// The setting: draw the viewport on the GPU when it can.
  pub enabled: bool,
  gl: Option<Arc<glow::Context>>,
  /// Why the GPU renderer can't be used.
  pub failed: Option<String>,
  shared: Arc<Mutex<Shared>>,
  /// The scene key last uploaded (the render key, camera and size aside).
  key: Option<RenderKey>,
  generation: u64,
  /// Pick tables of uploaded scenes whose ids haven't come back yet.
  pending_picks: Vec<(u64, PickTable)>,
  /// Generation of the pick table in `App::picks`.
  picks_generation: u64,
}

/// What the paint callback (on the render thread, with the GL context current) shares with the UI.
#[derive(Default)]
struct Shared {
  renderer: Option<Renderer>,
  error: Option<String>,
  /// A scene to upload: generation, scene, whether it only differs in hover.
  scene: Option<(u64, Scene, bool)>,
  uploaded: u64,
  /// The last image drawn, and whether at full detail (else with levels of detail, while navigating).
  drawn: Option<(u64, Camera, [u32; 2], bool)>,
  read: Option<(u64, Camera, [u32; 2])>,
  /// The id buffer of a drawn image, for the UI to collect.
  ids: Option<(u64, Frame)>,
  /// The Render workspace's look for the next scene uploaded (`Some(None)`: the usual shading).
  look: Option<Option<crate::render_ui::LookData>>,
}

impl Gpu {
  pub fn new(gl: Option<Arc<glow::Context>>) -> Gpu {
    let enabled = std::env::var("FOX_RENDERER").map(|v| !v.eq_ignore_ascii_case("cpu")).unwrap_or(true);
    Gpu {
      enabled,
      failed: gl.is_none().then(|| "no OpenGL context".to_string()),
      gl,
      shared: Arc::default(),
      key: None,
      generation: 0,
      pending_picks: Vec::new(),
      picks_generation: 0,
    }
  }
}

/// Whether the viewport is drawn on the GPU.
pub fn active(app: &App) -> bool {
  app.gpu.enabled && app.gpu.gl.is_some() && app.gpu.failed.is_none()
}

fn scene_part(k: &RenderKey) -> RenderKey {
  RenderKey { cam: Camera::default(), size: [0, 0], ..k.clone() }
}

/// Whether the scene must be rebuilt for `key` (anything but the camera and size changed).
pub fn scene_changed(app: &App, key: &RenderKey) -> bool {
  app.gpu.key.as_ref().is_none_or(|k| *k != scene_part(key))
}

/// Hands a freshly built scene to the GPU.
pub fn set_scene(app: &mut App, scene: Scene, picks: PickTable, key: RenderKey) {
  let key = scene_part(&key);
  let hover_only = app.gpu.key.as_ref().is_some_and(|k| RenderKey { hover: key.hover, ..k.clone() } == key);
  let g = &mut app.gpu;
  g.generation += 1;
  g.key = Some(key.clone());
  g.pending_picks.push((g.generation, picks));
  if let Ok(mut sh) = g.shared.lock() {
    // a scene not uploaded yet is replaced: the meshes it would have left are gone with it
    let reuse = hover_only && sh.scene.as_ref().is_none_or(|s| s.2);
    sh.scene = Some((g.generation, scene, reuse));
  }
  app.render_key = Some(key);
}

/// The look the next scene is drawn with (materials and environment in the Render workspace,
/// `None` for the usual shading).
pub fn set_look(app: &mut App, look: Option<crate::render_ui::LookData>) {
  if let Ok(mut sh) = app.gpu.shared.lock() {
    sh.look = Some(look);
  }
}

/// Draws the viewport image (when the GPU renderer is active) and collects the pick ids of the
/// last one. Returns false when the CPU image should be drawn instead.
pub fn paint(app: &mut App, painter: &egui::Painter, rect: Rect, size: [u32; 2], ctx: &egui::Context) -> bool {
  if !active(app) {
    app.gpu.key = None;
    return false;
  }
  app.tex = None;
  let navigating = matches!(app.drag, Some(Drag::Orbit | Drag::Pan | Drag::Zoom));
  let cam = app.cam;
  let g = &mut app.gpu;
  let Ok(mut sh) = g.shared.lock() else { return false };
  if let Some(e) = sh.error.take() {
    log::warn!("GPU renderer unavailable, using the CPU one: {e}");
    app.status = Some((format!("GPU renderer unavailable ({e}); using the CPU renderer"), true));
    g.failed = Some(e);
    g.key = None;
    app.render_key = None;
    ctx.request_repaint();
    return false;
  }
  if let Some((generation, frame)) = sh.ids.take() {
    g.pending_picks.retain(|(pg, _)| *pg >= generation);
    if let Some(i) = g.pending_picks.iter().position(|(pg, _)| *pg == generation) {
      app.picks = g.pending_picks.remove(i).1;
      g.picks_generation = generation;
    }
    if generation == g.picks_generation {
      app.frame = Some(frame);
    }
  }
  // the ids of this image come back after this frame is painted: look at them in the next one
  let next = sh.scene.as_ref().map(|s| s.0).unwrap_or(sh.uploaded);
  if !navigating && sh.read != Some((next, cam, size)) {
    ctx.request_repaint();
  }
  drop(sh);
  add_callback(painter, rect, g.shared.clone(), Some((cam, size, !navigating)), Color32::WHITE);
  true
}

/// The last GPU image, dimmed (while a regeneration runs). False when there is none.
pub fn paint_last(app: &App, painter: &egui::Painter, rect: Rect, tint: Color32) -> bool {
  if !active(app) || app.gpu.key.is_none() {
    return false;
  }
  add_callback(painter, rect, app.gpu.shared.clone(), None, tint);
  true
}

/// The callback: upload, draw (`view` = camera, size, whether to read the ids back), composite.
fn add_callback(
  painter: &egui::Painter,
  rect: Rect,
  shared: Arc<Mutex<Shared>>,
  view: Option<(Camera, [u32; 2], bool)>,
  tint: Color32,
) {
  let tint = egui::Rgba::from(tint).to_array();
  let callback = egui_glow::CallbackFn::new(move |info, painter| {
    let gl = painter.gl();
    let Ok(mut sh) = shared.lock() else { return };
    if sh.error.is_some() {
      return;
    }
    if let Err(e) = draw(gl, &mut sh, view) {
      sh.error = Some(e);
      return;
    }
    let (v, c) = (info.viewport_in_pixels(), info.clip_rect_in_pixels());
    if let Some(r) = &sh.renderer {
      r.composite(
        gl,
        painter.intermediate_fbo(),
        [v.left_px, v.from_bottom_px, v.width_px, v.height_px],
        [c.left_px, c.from_bottom_px, c.width_px, c.height_px],
        tint,
      );
    }
  });
  painter.add(egui::PaintCallback { rect, callback: Arc::new(callback) });
}

fn draw(gl: &glow::Context, sh: &mut Shared, view: Option<(Camera, [u32; 2], bool)>) -> Result<(), String> {
  if sh.renderer.is_none() {
    sh.renderer = Some(Renderer::new(gl)?);
  }
  let Some(r) = sh.renderer.as_mut() else { return Ok(()) };
  if let Some((generation, scene, reuse)) = sh.scene.take() {
    r.set_scene(gl, &scene, reuse)?;
    sh.uploaded = generation;
    if let Some(look) = sh.look.take() {
      r.set_look(gl, look.as_ref().map(|l| l.look()).as_ref())?;
    }
  }
  let Some((cam, size, read)) = view else { return Ok(()) };
  let state = Some((sh.uploaded, cam, size));
  let drawn = sh.drawn.map(|(g, c, s, full)| (Some((g, c, s)), full));
  if read && drawn != Some((state, true)) {
    // the view settled: full detail, as the ids read back are drawn
    r.draw(gl, &cam, size[0], size[1])?;
    sh.drawn = Some((sh.uploaded, cam, size, true));
  } else if !read && drawn.map(|d| d.0) != Some(state) {
    r.draw_navigating(gl, &cam, size[0], size[1])?;
    sh.drawn = Some((sh.uploaded, cam, size, false));
  }
  if read && sh.read != state {
    if let Some(f) = r.read_ids(gl) {
      sh.ids = Some((sh.uploaded, f));
    }
    sh.read = state;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  #[test]
  fn without_opengl_the_cpu_renderer_draws() {
    let mut app = App::with_session(Session::default());
    assert!(!active(&app), "no GL context: CPU");
    assert!(app.gpu.failed.is_some());
    app.gpu.failed = None;
    app.gpu.gl = None;
    assert!(!active(&app));
  }

  #[test]
  fn camera_moves_dont_rebuild_the_scene_but_hover_does() {
    let mut app = App::with_session(Session::default());
    let key = RenderKey {
      revision: 1,
      cam: Camera::default(),
      size: [100, 80],
      hover: None,
      selection: Vec::new(),
      edit_sketch: None,
      style: Default::default(),
      objects: Default::default(),
      effects: Default::default(),
      show_grid: true,
      grid_step: None,
      visible: Vec::new(),
      profiles_of: Vec::new(),
      hidden_profiles: Vec::new(),
      view: serde_json::json!({}),
      vertices: false,
      peek: Vec::new(),
      isolate: None,
      opacity: Vec::new(),
      peek_faces: Vec::new(),
      peek_sketches: Vec::new(),
    };
    assert!(scene_changed(&app, &key));
    set_scene(&mut app, Scene::default(), PickTable::default(), key.clone());
    let mut moved = key.clone();
    moved.cam.orbit(0.3, 0.1);
    moved.size = [200, 160];
    assert!(!scene_changed(&app, &moved));
    let hovered = RenderKey {
      hover: Some(fox_engine::scene::Pick::Face { body: fox_engine::model::BodyId(1), face: 2 }),
      ..key.clone()
    };
    assert!(scene_changed(&app, &hovered));
    set_scene(&mut app, Scene::default(), PickTable::default(), hovered);
    let sh = app.gpu.shared.lock().unwrap();
    assert!(sh.scene.as_ref().is_some_and(|s| s.0 == 2 && !s.2), "the first scene was never uploaded: no reuse");
  }
}
