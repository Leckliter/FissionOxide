//! The Render workspace's docked dialogs and canvas furniture, laid out as Fusion's: SCENE SETTINGS
//! (Settings and Environment Library tabs, Close), NEW LIGHT, the "save your design first" message
//! that Render gives an unsaved design, In-canvas Render's progress bar and the RENDERING GALLERY
//! strip that takes the timeline's place.

use egui::{Color32, Rect, RichText, Stroke, Vec2, pos2, vec2};
use fox_engine::rendering::raytrace::{Background, Environment, Preset, RenderCamera};

use crate::icons::{self, Icon};
use crate::render_ui::{color_edit, shading};
use crate::{App, dialogs, file_dialogs, panels, theme};

/// The SCENE SETTINGS dialog's tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SceneTab {
  Settings,
  Library,
}

/// Fusion's environment library, each on the built-in environment with the nearest look.
pub(crate) const LIBRARY: [(&str, Preset); 6] = [
  ("Cool Light", Preset::Sky),
  ("Grid Light", Preset::White),
  ("Rim Highlights", Preset::Dark),
  ("Sharp Highlights", Preset::Studio),
  ("Soft Light", Preset::Soft),
  ("Warm Light", Preset::Sunset),
];

/// The library's environments that Fusion downloads on first use; FissionOxide has none of them.
const DOWNLOADS: [&str; 6] = ["Crossroads", "Dry Lake Bed", "Field", "Infinity Pool", "Plaza", "Tranquility Blue"];

/// Brightness as Scene Settings shows it: Fusion's 1000 lx is the environment's own light.
pub(crate) const LUX: f32 = 1000.0;

/// Fusion's default exposure value. The camera's exposure in stops counts from it, a higher EV
/// giving a darker picture as on a real camera.
pub(crate) const EV0: f32 = 9.5;

/// The focal length Fusion's Scene Settings shows for the default camera.
const DEFAULT_FOCAL: f64 = 90.0;

/// Half the frame height that focal lengths are measured against: the one that makes the default
/// camera's field of view read [`DEFAULT_FOCAL`].
fn frame_half() -> f64 {
  DEFAULT_FOCAL * (RenderCamera::default().view.fov / 2.0).tan()
}

/// The focal length (mm) of a vertical field of view (radians).
pub(crate) fn focal_length(fov: f64) -> f64 {
  frame_half() / (fov / 2.0).tan()
}

/// The vertical field of view (radians) of a focal length (mm).
pub(crate) fn fov_of(focal: f64) -> f64 {
  2.0 * (frame_half() / focal).atan()
}

/// The environment's name as the library lists it, or the custom image's file name.
pub(crate) fn environment_name(env: &Environment) -> String {
  match &env.hdri {
    Some(p) => std::path::Path::new(p).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
    None => LIBRARY.iter().find(|l| l.1 == env.preset).map_or("", |l| l.0).to_string(),
  }
}

/// What a scene-settings button asks for.
enum SceneAct {
  SaveDefault,
  RestoreSaved,
  RestoreDefaults,
}

/// The docked SCENE SETTINGS dialog, while open.
pub(crate) fn scene_dialog(app: &mut App, ctx: &egui::Context) {
  if !app.render.scene_open {
    return;
  }
  let design_env = app.s.design.render.environment.clone();
  let mut env = app.render.environment(&design_env);
  let design_cam = fox_engine::rendering::render_camera(&app.s);
  let mut cam = app.render.cam_edit.unwrap_or(design_cam);
  let mut tab = app.render.scene_tab;
  let saved = app.render.saved_scene.is_some();
  let mut act = None;
  let mut close = false;
  let width = 320.0;
  egui::Area::new(egui::Id::new("scene-settings"))
    .pivot(egui::Align2::RIGHT_TOP)
    .default_pos(panels::dialog_dock(app.canvas, false))
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      panels::dialog_frame().show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        panels::dialog_header(ui, Icon::EnvMap, "SCENE SETTINGS", width);
        tab_row(ui, &mut tab, width);
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 8, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
          match tab {
            SceneTab::Settings => settings_tab(ui, &mut env, &mut cam, saved, &mut act),
            SceneTab::Library => library_tab(ui, &mut env),
          }
        });
        close = footer(ui, width, |ui| panels::secondary_button(ui, "Close").clicked());
      });
    });
  match act {
    Some(SceneAct::SaveDefault) => {
      app.render.saved_scene = Some((env.clone(), cam));
      app.status = Some(("Scene settings saved as the default".into(), false));
    }
    Some(SceneAct::RestoreSaved) => {
      if let Some((e, c)) = app.render.saved_scene.clone() {
        env = e;
        cam = c;
      }
    }
    Some(SceneAct::RestoreDefaults) => {
      // the camera keeps its framing; its lens and exposure go back to the defaults
      let mut c = RenderCamera::default();
      let (fov, ortho) = (c.view.fov, c.view.ortho);
      c.view = cam.view;
      c.view.fov = fov;
      c.view.ortho = ortho;
      env = Environment::new_design();
      cam = c;
    }
    None => {}
  }
  app.render.scene_tab = tab;
  app.render.scene_open = !close;
  app.render.env_edit = (env != design_env).then_some(env);
  app.render.cam_edit = (cam != design_cam).then_some(cam);
}

/// Settings | Environment Library, the open one underlined in the accent.
fn tab_row(ui: &mut egui::Ui, tab: &mut SceneTab, width: f32) {
  let (rect, _) = ui.allocate_exact_size(vec2(width, 30.0), egui::Sense::hover());
  ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, theme::SEPARATOR));
  let mut x = rect.left() + 12.0;
  for (t, label) in [(SceneTab::Settings, "Settings"), (SceneTab::Library, "Environment Library")] {
    let on = *tab == t;
    let g = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(12.5), theme::TEXT);
    let r = Rect::from_min_size(pos2(x - 6.0, rect.top()), vec2(g.size().x + 12.0, rect.height()));
    let resp = ui.interact(r, ui.id().with(("scene-tab", label)), egui::Sense::click());
    if resp.hovered() && !on {
      ui.painter().rect_filled(r, 2.0, theme::HOVER);
    }
    let ink = if on { theme::TEXT } else { theme::MUTED };
    ui.painter().galley(pos2(x, r.center().y - g.size().y / 2.0), g, ink);
    if on {
      ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(2.0, theme::ACCENT));
    }
    if resp.clicked() {
      *tab = t;
    }
    x = r.right() + 14.0;
  }
}

/// The dialog's last row: a rule, then `buttons` at the right.
pub(crate) fn footer<R>(ui: &mut egui::Ui, width: f32, buttons: impl FnOnce(&mut egui::Ui) -> R) -> R {
  let sep = ui.allocate_exact_size(vec2(width, 1.0), egui::Sense::hover()).0;
  ui.painter().rect_filled(sep, 0.0, theme::SEPARATOR);
  egui::Frame::new()
    .fill(theme::DIALOG_BODY)
    .inner_margin(egui::Margin::symmetric(10, 7))
    .show(ui, |ui| {
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
        buttons(ui)
      })
      .inner
    })
    .inner
}

/// Fusion's background choices, by the name its drop-down shows.
fn background_name(b: Background) -> &'static str {
  match b {
    Background::Environment => "Environment",
    Background::Color => "Solid Color",
    Background::Gradient => "Gradient",
    Background::Transparent => "Transparent",
  }
}

fn settings_tab(
  ui: &mut egui::Ui,
  env: &mut Environment,
  cam: &mut RenderCamera,
  saved: bool,
  act: &mut Option<SceneAct>,
) {
  egui::Grid::new("scene-settings-grid").num_columns(2).min_col_width(96.0).spacing([10.0, 6.0]).show(ui, |ui| {
    if dialogs::section_header(ui, egui::Id::new("scene-env"), "Environment") {
      ui.label("Brightness");
      let mut lux = env.intensity * LUX;
      if ui.add(egui::Slider::new(&mut lux, 0.0..=4.0 * LUX).suffix(" lx").fixed_decimals(3)).changed() {
        env.intensity = lux / LUX;
      }
      ui.end_row();
      ui.label("Position");
      // only the move icon, as in Fusion: dragging it turns the environment about the vertical
      let icon =
        icons::icon_button(ui, Icon::Move, 22.0, "Drag to turn the environment about the vertical", false, None);
      let turn = ui.interact(icon.rect, icon.id.with("turn"), egui::Sense::drag());
      if turn.dragged() {
        env.rotation = (env.rotation + turn.drag_delta().x).rem_euclid(360.0);
      }
      ui.end_row();
      ui.label("Background");
      egui::ComboBox::from_id_salt("scene-bg").selected_text(background_name(env.background)).width(150.0).show_ui(
        ui,
        |ui| {
          for b in [Background::Environment, Background::Color] {
            ui.selectable_value(&mut env.background, b, background_name(b));
          }
        },
      );
      ui.end_row();
      if env.background == Background::Color {
        color_edit(ui, "Color", &mut env.background_color);
      }
    }
    if dialogs::section_header(ui, egui::Id::new("scene-ground"), "Ground") {
      ui.label("Ground Plane");
      ui.checkbox(&mut env.ground, "");
      ui.end_row();
      ui.label("Flatten Ground");
      ui.checkbox(&mut env.flatten_ground, "")
        .on_hover_text("Flattens the environment's lower half onto the ground, so the model stands on it");
      ui.end_row();
      ui.label("Reflections");
      let mut reflect = env.ground_reflection > 0.0;
      if ui.add_enabled(env.ground, egui::Checkbox::without_text(&mut reflect)).changed() {
        env.ground_reflection = if reflect { 0.5 } else { 0.0 };
      }
      ui.end_row();
    }
    if dialogs::section_header(ui, egui::Id::new("scene-camera"), "Camera") {
      ui.label("Camera");
      let name = if cam.view.ortho { "Orthographic" } else { "Perspective" };
      egui::ComboBox::from_id_salt("scene-cam").selected_text(name).width(150.0).show_ui(ui, |ui| {
        ui.selectable_value(&mut cam.view.ortho, false, "Perspective");
        ui.selectable_value(&mut cam.view.ortho, true, "Orthographic");
      });
      ui.end_row();
      ui.label("Focal Length");
      let mut focal = focal_length(cam.view.fov);
      if ui
        .add_enabled(!cam.view.ortho, egui::Slider::new(&mut focal, 10.0..=300.0).suffix(" mm").fixed_decimals(0))
        .changed()
      {
        cam.view.fov = fov_of(focal);
      }
      ui.end_row();
      ui.label("Exposure");
      let mut ev = EV0 - cam.exposure;
      if ui.add(egui::Slider::new(&mut ev, EV0 - 5.0..=EV0 + 5.0).suffix(" EV").fixed_decimals(1)).changed() {
        cam.exposure = EV0 - ev;
      }
      ui.end_row();
      ui.label("Depth of Field");
      let mut dof = cam.fstop > 0.0;
      if ui.add_enabled(!cam.view.ortho, egui::Checkbox::without_text(&mut dof)).changed() {
        cam.fstop = if dof { 2.8 } else { 0.0 };
      }
      ui.end_row();
      ui.label("Aspect Ratio");
      egui::ComboBox::from_id_salt("scene-aspect").selected_text("Viewport aspect ratio").width(150.0).show_ui(
        ui,
        |ui| {
          let _ = ui.selectable_label(true, "Viewport aspect ratio");
        },
      );
      ui.end_row();
    }
  });
  ui.add_space(8.0);
  ui.horizontal_wrapped(|ui| {
    if ui.button("Save as Default").clicked() {
      *act = Some(SceneAct::SaveDefault);
    }
    if ui.add_enabled(saved, egui::Button::new("Restore Saved")).clicked() {
      *act = Some(SceneAct::RestoreSaved);
    }
    if ui.button("Restore Defaults").clicked() {
      *act = Some(SceneAct::RestoreDefaults);
    }
  });
}

/// A small picture of an environment: its sky over its ground.
fn env_thumb(ui: &mut egui::Ui, preset: Option<Preset>, size: Vec2) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
  let (sky, ground) = match preset {
    Some(Preset::Studio) => ((236, 238, 241), (150, 152, 156)),
    Some(Preset::Soft) => ((222, 224, 226), (176, 178, 180)),
    Some(Preset::Sky) => ((176, 206, 236), (128, 140, 150)),
    Some(Preset::Sunset) => ((246, 196, 140), (150, 120, 96)),
    Some(Preset::Dark) => ((60, 62, 68), (24, 24, 28)),
    Some(Preset::White) => ((250, 250, 250), (214, 216, 218)),
    None => ((200, 200, 200), (120, 120, 120)),
  };
  let p = ui.painter();
  let mid = rect.top() + rect.height() * 0.62;
  p.rect_filled(Rect::from_min_max(rect.min, pos2(rect.right(), mid)), 0.0, Color32::from_rgb(sky.0, sky.1, sky.2));
  p.rect_filled(
    Rect::from_min_max(pos2(rect.left(), mid), rect.max),
    0.0,
    Color32::from_rgb(ground.0, ground.1, ground.2),
  );
  p.circle_filled(pos2(rect.center().x, mid - 2.0), rect.height() * 0.2, Color32::from_gray(200));
  p.rect_stroke(rect, 0.0, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
  resp
}

fn library_tab(ui: &mut egui::Ui, env: &mut Environment) {
  ui.label(RichText::new("Current Environment").strong());
  ui.horizontal(|ui| {
    env_thumb(ui, env.hdri.is_none().then_some(env.preset), vec2(64.0, 40.0));
    ui.label(environment_name(env));
  });
  ui.add_space(6.0);
  ui.horizontal(|ui| {
    ui.label(RichText::new("Library").strong());
    egui::ComboBox::from_id_salt("scene-library").selected_text("FissionOxide Library").show_ui(ui, |ui| {
      let _ = ui.selectable_label(true, "FissionOxide Library");
    });
  });
  egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
    for (name, preset) in LIBRARY {
      let on = env.hdri.is_none() && env.preset == preset;
      let r = ui
        .horizontal(|ui| {
          let t = env_thumb(ui, Some(preset), vec2(48.0, 30.0));
          let l = ui.selectable_label(on, name);
          t.clicked() || l.clicked()
        })
        .inner;
      if r {
        env.preset = preset;
        env.hdri = None;
      }
    }
    for name in DOWNLOADS {
      ui.horizontal(|ui| {
        env_thumb(ui, None, vec2(48.0, 30.0));
        ui.add_enabled(false, egui::Label::new(name));
        let (r, resp) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
        let c = r.center();
        let s = Stroke::new(1.4, theme::MUTED);
        ui.painter().line_segment([c + vec2(0.0, -6.0), c + vec2(0.0, 3.0)], s);
        ui.painter().line_segment([c + vec2(-4.0, -1.0), c + vec2(0.0, 3.0)], s);
        ui.painter().line_segment([c + vec2(4.0, -1.0), c + vec2(0.0, 3.0)], s);
        ui.painter().line_segment([c + vec2(-6.0, 6.0), c + vec2(6.0, 6.0)], s);
        resp.on_hover_text(
          "FissionOxide can't download Fusion's environments: use a built-in one or attach an HDR image",
        );
      });
    }
  });
  ui.add_space(6.0);
  ui.horizontal(|ui| {
    let pick = icons::icon_button(ui, Icon::Folder, 22.0, "", false, None).clicked();
    let label = ui.add(egui::Label::new("Attach Custom Environment").sense(egui::Sense::click())).clicked();
    if (pick || label)
      && let Some(path) = rfd::FileDialog::new().add_filter("HDR image", &["hdr", "png", "jpg", "jpeg"]).pick_file()
    {
      env.hdri = Some(path.display().to_string());
    }
  });
}

// ---------------------------------------------------------------- lights

// NEW LIGHT and EDIT LIGHT live in `light_ui`.
pub(crate) use crate::light_ui::{LightForm, light_dialog};

// ---------------------------------------------------------------- save first

/// Fusion's message when Render is pressed on a design that has never been saved.
pub(crate) const NEEDS_SAVE: &str = "You need to save your design before using the Render tool.";

/// The modal "save first" message, while it shows.
pub(crate) fn needs_save(app: &mut App, ctx: &egui::Context) {
  if !app.render.needs_save {
    return;
  }
  let mut ok = false;
  let (_, closed) = file_dialogs::modal(ctx, "render-needs-save", "FissionOxide", 400.0, |ui| {
    ui.add_space(8.0);
    ui.label(NEEDS_SAVE);
    ui.add_space(12.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
      ok = ui.add(file_dialogs::blue_button("OK")).clicked();
    });
  });
  if ok || closed {
    app.render.needs_save = false;
  }
}

// ---------------------------------------------------------------- in-canvas render

/// In-canvas Render's progress: how long it has run and whether it is paused.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct InCanvasRun {
  /// Seconds rendered (pauses not counted).
  pub elapsed: f64,
  /// When the last frame counted was drawn; None while paused or before the first frame.
  pub last: Option<f64>,
  pub paused: bool,
}

/// How fast the iterations go, as Fusion's in-canvas render on a design of a few bodies.
const ITERATIONS_PER_SEC: f64 = 7.0;

/// The iterations at Fusion's quality marks, and where the marks sit on the bar.
pub(crate) const EXCELLENT: (u32, f32) = (100, 0.5);
pub(crate) const FINAL: (u32, f32) = (500, 0.8);

impl InCanvasRun {
  /// Counts the time since the last frame, unless paused.
  pub(crate) fn tick(&mut self, now: f64) {
    if self.paused {
      self.last = None;
      return;
    }
    if let Some(t) = self.last {
      self.elapsed += (now - t).max(0.0);
    }
    self.last = Some(now);
  }

  pub(crate) fn iterations(&self) -> u32 {
    (self.elapsed * ITERATIONS_PER_SEC) as u32
  }

  /// Stops at `n` iterations (In-canvas Render Settings' Quality); true once there.
  pub(crate) fn stop_at(&mut self, n: Option<u32>) -> bool {
    let Some(n) = n else { return false };
    if self.iterations() < n {
      return false;
    }
    self.elapsed = (n as f64 + 0.5) / ITERATIONS_PER_SEC;
    true
  }
}

/// How far along the bar `n` iterations reach: Excellent half way, Final at four fifths, and the
/// far end (Infinite) never.
pub(crate) fn progress(n: u32) -> f32 {
  let n = n as f32;
  let (e, f) = ((EXCELLENT.0 as f32, EXCELLENT.1), (FINAL.0 as f32, FINAL.1));
  if n <= e.0 {
    n / e.0 * e.1
  } else if n <= f.0 {
    e.1 + (n - e.0) / (f.0 - e.0) * (f.1 - e.1)
  } else {
    f.1 + (1.0 - f.0 / n) * (1.0 - f.1)
  }
}

/// "Elapsed time: 2 sec", "Elapsed time: 1 min 5 sec".
pub(crate) fn elapsed_text(secs: f64) -> String {
  let s = secs as u64;
  if s < 60 { format!("Elapsed time: {s} sec") } else { format!("Elapsed time: {} min {} sec", s / 60, s % 60) }
}

/// The progress bar at the lower right of the canvas while In-canvas Render runs: a pause/play
/// button, the line with its Excellent / Final / Infinite marks, the elapsed time and the
/// iteration number.
pub(crate) fn in_canvas_bar(app: &mut App, ctx: &egui::Context) {
  if !shading(app) {
    return;
  }
  let now = ctx.input(|i| i.time);
  app.render.run.tick(now);
  let st = app.render.canvas_settings;
  let limit = if st.mode == crate::render_tools::InCanvasMode::Advanced { st.quality.iterations() } else { None };
  let done = app.render.run.stop_at(limit);
  let run = app.render.run;
  if !run.paused && !done {
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
  }
  let mut toggle = false;
  let width = 300.0;
  egui::Area::new(egui::Id::new("in-canvas-progress"))
    .pivot(egui::Align2::RIGHT_BOTTOM)
    .fixed_pos(app.canvas.right_bottom() - vec2(14.0, 12.0))
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      egui::Frame::new()
        .fill(theme::DIALOG_BODY)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
          let (rect, _) = ui.allocate_exact_size(vec2(width, 50.0), egui::Sense::hover());
          let button = Rect::from_center_size(pos2(rect.left() + 10.0, rect.center().y), Vec2::splat(20.0));
          let resp = ui.interact(button, ui.id().with("in-canvas-pause"), egui::Sense::click());
          let resp = resp.on_hover_text(if run.paused { "Resume" } else { "Pause" });
          toggle = resp.clicked();
          let p = ui.painter();
          if resp.hovered() {
            p.rect_filled(button, 2.0, theme::HOVER);
          }
          let c = button.center();
          if run.paused {
            p.add(egui::Shape::convex_polygon(
              vec![c + vec2(-4.0, -6.0), c + vec2(6.0, 0.0), c + vec2(-4.0, 6.0)],
              theme::ICON_INK,
              Stroke::NONE,
            ));
          } else {
            for dx in [-3.0, 3.0] {
              p.rect_filled(Rect::from_center_size(c + vec2(dx, 0.0), vec2(3.0, 12.0)), 0.0, theme::ICON_INK);
            }
          }
          let x0 = button.right() + 10.0;
          let x1 = rect.right() - 4.0;
          let y = rect.center().y;
          let at = |k: f32| x0 + (x1 - x0) * k;
          let small = egui::FontId::proportional(10.5);
          p.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(3.0, Color32::from_gray(205)));
          p.line_segment([pos2(x0, y), pos2(at(progress(run.iterations())), y)], Stroke::new(3.0, theme::ACCENT));
          for (k, label, align) in [
            (EXCELLENT.1, "Excellent", egui::Align2::CENTER_BOTTOM),
            (FINAL.1, "Final", egui::Align2::CENTER_BOTTOM),
            (1.0, "Infinite", egui::Align2::RIGHT_BOTTOM),
          ] {
            p.line_segment([pos2(at(k), y - 4.0), pos2(at(k), y + 4.0)], Stroke::new(1.0, theme::GROUP_LABEL));
            p.text(pos2(at(k), y - 7.0), align, label, small.clone(), theme::MUTED);
          }
          p.text(pos2(x0, y + 8.0), egui::Align2::LEFT_TOP, elapsed_text(run.elapsed), small.clone(), theme::TEXT);
          p.text(
            pos2(x1, y + 8.0),
            egui::Align2::RIGHT_TOP,
            format!("Iteration number: {}", run.iterations()),
            small,
            theme::TEXT,
          );
        });
    });
  if toggle {
    let r = &mut app.render.run;
    r.paused = !r.paused;
    r.last = None;
  }
}

// ---------------------------------------------------------------- rendering gallery

/// The RENDERING GALLERY strip at the bottom of the window, in the timeline's place: a header
/// that folds it (- / +), then the renderings, or how to make one while there are none.
pub(crate) fn gallery_strip(app: &mut App, ui: &mut egui::Ui) {
  crate::render_ui::gallery_textures(app, ui.ctx());
  let folded = app.render.gallery_folded;
  let mut fold = false;
  let mut open: Option<u32> = None;
  egui::Panel::bottom("rendering-gallery")
    .exact_size(if folded { HEADER_H } else { 150.0 })
    .frame(egui::Frame::NONE.fill(theme::TIMELINE))
    .show(ui, |ui| {
      ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
      let full = ui.max_rect();
      let (head, _) = ui.allocate_exact_size(vec2(full.width(), HEADER_H), egui::Sense::hover());
      let p = ui.painter();
      p.line_segment([head.left_top(), head.right_top()], Stroke::new(1.0, theme::BORDER));
      p.line_segment([head.left_bottom(), head.right_bottom()], Stroke::new(1.0, theme::SEPARATOR));
      let b = Rect::from_center_size(pos2(head.left() + 14.0, head.center().y), Vec2::splat(16.0));
      let resp = ui.interact(b, ui.id().with("gallery-fold"), egui::Sense::click());
      let resp = resp.on_hover_text(if folded { "Expand" } else { "Collapse" });
      fold = resp.clicked();
      let p = ui.painter();
      if resp.hovered() {
        p.rect_filled(b, 2.0, theme::HOVER);
      }
      let s = Stroke::new(1.5, theme::GROUP_LABEL);
      p.line_segment([b.center() - vec2(5.0, 0.0), b.center() + vec2(5.0, 0.0)], s);
      if folded {
        p.line_segment([b.center() - vec2(0.0, 5.0), b.center() + vec2(0.0, 5.0)], s);
      }
      p.text(
        pos2(b.right() + 8.0, head.center().y),
        egui::Align2::LEFT_CENTER,
        "RENDERING GALLERY",
        egui::FontId::proportional(12.0),
        theme::GROUP_LABEL,
      );
      if folded {
        return;
      }
      let gallery = &app.s.design.render.gallery;
      if gallery.is_empty() {
        let body = Rect::from_min_max(pos2(full.left(), head.bottom()), full.max);
        empty_text(ui, body);
        return;
      }
      egui::ScrollArea::horizontal().show(ui, |ui| {
        ui.horizontal(|ui| {
          ui.spacing_mut().item_spacing = vec2(10.0, 2.0);
          ui.add_space(10.0);
          for g in gallery {
            ui.vertical(|ui| {
              ui.add_space(6.0);
              if let Some((_, t)) = app.render.gallery_tex.get(&g.id) {
                let s = t.size_vec2();
                let k = (150.0 / s.x).min(92.0 / s.y);
                if ui.add(egui::Button::image((t.id(), s * k))).on_hover_text("Open").clicked() {
                  open = Some(g.id);
                }
              }
              ui.label(RichText::new(&g.name).size(11.0));
            });
          }
        });
      });
    });
  if fold {
    app.render.gallery_folded = !folded;
  }
  if open.is_some() {
    app.render.gallery_view = open;
  }
}

const HEADER_H: f32 = 26.0;

/// "Use the Render tool [teapot] to save renderings to your gallery.", centred in `body`.
fn empty_text(ui: &egui::Ui, body: Rect) {
  let font = egui::FontId::proportional(12.5);
  let p = ui.painter();
  let a = p.layout_no_wrap("Use the Render tool".into(), font.clone(), theme::MUTED);
  let b = p.layout_no_wrap("to save renderings to your gallery.".into(), font, theme::MUTED);
  let icon = 18.0;
  let w = a.size().x + b.size().x + icon + 12.0;
  let mut x = body.center().x - w / 2.0;
  let y = body.center().y;
  let (aw, bh) = (a.size().x, b.size().y);
  p.galley(pos2(x, y - a.size().y / 2.0), a, theme::MUTED);
  x += aw + 6.0;
  icons::draw_color(p, Rect::from_min_size(pos2(x, y - icon / 2.0), Vec2::splat(icon)), Icon::Teapot);
  x += icon + 6.0;
  p.galley(pos2(x, y - bh / 2.0), b, theme::MUTED);
}
