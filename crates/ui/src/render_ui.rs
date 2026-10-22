//! The Render workspace: Appearance (the physically based material library, applied to bodies and
//! faces, and a material editor), Scene Settings (environment, background, ground, camera; a docked
//! dialog, see [`crate::render_panels`]), In-canvas Render (the GPU viewport shading the materials,
//! with Fusion's progress bar) and Render (the path tracer, progressive, on another thread), with
//! the Rendering Gallery kept in the design and shown in place of the timeline.
//!
//! Everything that changes the design goes through `render.*` commands.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use egui::{Color32, RichText};
use fox_engine::Sel;
use fox_engine::model::BodyId;
use fox_engine::render::Scene;
use fox_engine::rendering::raytrace::{
  self, Background, EnvMap, Environment, Material, Pattern, Progressive, Projection, RenderCamera, RenderImage,
  RenderSettings, TextureSource, TextureSpec,
};
use fox_engine::scene::{Pick, PickTable, SELECT_COLOR};
use serde_json::{Value, json};

use crate::App;
use crate::icons::Icon;
use crate::render_panels::{InCanvasRun, LightForm, SceneTab};
use crate::render_tools::{InCanvasSettings, PriorityForm, TextureMapForm};

/// The workspace's toolbar commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderCmd {
  Appearance,
  SceneSettings,
  /// Lights: the NEW LIGHT dialog.
  Lights,
  InCanvas,
  /// Capture Image...: the Image Options window.
  CaptureImage,
  Render,
  /// Texture Map Controls: the TEXTURE MAP CONTROLS dialog.
  TextureMap,
  /// Dielectric Priority Control.
  Priority,
  /// In-canvas Render Settings.
  InCanvasSettings,
}

impl RenderCmd {
  pub(crate) fn info(self) -> (Icon, &'static str, &'static str) {
    match self {
      RenderCmd::Appearance => (
        Icon::Appearance,
        "Appearance",
        "Physically based materials: pick one and apply it to selected bodies or faces",
      ),
      RenderCmd::SceneSettings => {
        (Icon::EnvMap, "Scene Settings", "Environment, background, ground plane and camera of the rendered scene")
      }
      RenderCmd::Lights => (Icon::Sun, "Light", "Add a light to control illumination, shadows and highlights"),
      RenderCmd::InCanvas => (Icon::Display, "In-canvas Render", "Render the design in the canvas"),
      RenderCmd::CaptureImage => (Icon::Camera, "Capture Image...", "Save an image of the canvas"),
      RenderCmd::Render => (Icon::Teapot, "Render", "Render an image of the design into the Rendering Gallery"),
      RenderCmd::TextureMap => (
        Icon::TextureMap,
        "Texture Map Controls",
        "Adjust how a texture is projected onto a body: its projection, size, turn and shift",
      ),
      RenderCmd::Priority => (
        Icon::DielectricPriority,
        "Dielectric Priority Control",
        "Set which transparent body fills the space where transparent bodies overlap",
      ),
      RenderCmd::InCanvasSettings => {
        (Icon::InCanvasSettings, "In-canvas Render Settings", "Set the in-canvas render's mode and quality")
      }
    }
  }

  /// Shown pressed: its window or dialog is open. In-canvas Render shows its stop button instead
  /// while it runs.
  pub(crate) fn active(self, app: &App) -> bool {
    let r = &app.render;
    match self {
      RenderCmd::Appearance => r.appearance_open,
      RenderCmd::SceneSettings => r.scene_open,
      RenderCmd::Lights => r.light.is_some(),
      RenderCmd::InCanvas => false,
      RenderCmd::CaptureImage => app.capture.image.is_some(),
      RenderCmd::Render => r.render_open,
      RenderCmd::TextureMap => r.texture_map.is_some(),
      RenderCmd::Priority => r.priority.is_some(),
      RenderCmd::InCanvasSettings => r.in_canvas_settings.is_some(),
    }
  }

  pub(crate) fn run(self, app: &mut App) {
    match self {
      RenderCmd::Appearance => app.render.appearance_open = !app.render.appearance_open,
      // the docked dialogs take the place of a command dialog, one at a time
      RenderCmd::SceneSettings => {
        let open = !app.render.scene_open;
        if open {
          crate::render_tools::close_docked(app);
          app.render.scene_tab = SceneTab::Settings;
        }
        app.render.scene_open = open;
      }
      RenderCmd::Lights => {
        crate::render_tools::close_docked(app);
        app.render.light = Some(LightForm);
      }
      RenderCmd::InCanvas => set_in_canvas(app, !app.render.in_canvas),
      RenderCmd::CaptureImage => crate::capture::open_image(app),
      // Fusion keeps renderings with the saved design, so an unsaved one has to be saved first
      RenderCmd::Render if crate::panels::is_unsaved(app) => app.render.needs_save = true,
      RenderCmd::Render => app.render.render_open = true,
      RenderCmd::TextureMap => {
        crate::render_tools::close_docked(app);
        app.render.texture_map = Some(TextureMapForm::default());
      }
      RenderCmd::Priority => {
        crate::render_tools::close_docked(app);
        app.render.priority = Some(PriorityForm::default());
      }
      RenderCmd::InCanvasSettings => {
        let current = app.render.canvas_settings;
        crate::render_tools::close_docked(app);
        app.render.in_canvas_settings = Some(current);
      }
    }
  }
}

/// In-canvas Render on or off: its progress starts over each time.
pub(crate) fn set_in_canvas(app: &mut App, on: bool) {
  app.render.in_canvas = on;
  app.render.run = InCanvasRun::default();
  if on && !crate::gpu::active(app) {
    app.status = Some(("In-canvas rendering needs the GPU renderer (Display Settings)".into(), true));
  }
}

/// Entering the Render workspace: as in Fusion, nothing opens and In-canvas Render is off.
pub(crate) fn enter(app: &mut App) {
  set_in_canvas(app, false);
  let r = &mut app.render;
  r.appearance_open = false;
  r.scene_open = false;
  r.render_open = false;
  r.light = None;
  r.texture_map = None;
  r.priority = None;
  r.in_canvas_settings = None;
  r.gallery_view = None;
}

/// A final render running on another thread.
struct Job {
  state: Arc<Mutex<JobState>>,
  cancel: Arc<AtomicBool>,
  started: Instant,
  name: String,
}

#[derive(Default)]
struct JobState {
  /// The image so far, when a newer one than the UI has is ready.
  preview: Option<RenderImage>,
  samples: u32,
  total: u32,
  finished: Option<Result<RenderImage, String>>,
}

/// The Render workspace's state.
pub struct RenderUi {
  /// In the Render workspace.
  pub active: bool,
  /// In-canvas rendering (GPU) on.
  pub in_canvas: bool,
  /// In-canvas Render's progress bar: elapsed time, iterations, paused.
  pub(crate) run: InCanvasRun,
  pub appearance_open: bool,
  /// The docked SCENE SETTINGS dialog, and its tab.
  pub scene_open: bool,
  pub(crate) scene_tab: SceneTab,
  /// The docked NEW LIGHT dialog.
  pub(crate) light: Option<LightForm>,
  /// The docked TEXTURE MAP CONTROLS, DIELECTRIC PRIORITY CONTROL and IN-CANVAS RENDER SETTINGS.
  pub(crate) texture_map: Option<TextureMapForm>,
  pub(crate) priority: Option<PriorityForm>,
  pub(crate) in_canvas_settings: Option<InCanvasSettings>,
  /// In-canvas Render's mode and quality.
  pub(crate) canvas_settings: InCanvasSettings,
  pub render_open: bool,
  /// "You need to save your design before using the Render tool." shows.
  pub(crate) needs_save: bool,
  /// The Rendering Gallery strip folded to its header.
  pub(crate) gallery_folded: bool,
  /// What Scene Settings' Save as Default keeps (Restore Saved brings it back).
  pub(crate) saved_scene: Option<(Environment, RenderCamera)>,
  filter: String,
  category: String,
  /// The library material picked in the Appearance window.
  picked: Option<String>,
  /// The material being edited (a design material), and whether it's new.
  editing: Option<Material>,
  /// The environment and camera being edited in Scene Settings (committed when no slider is
  /// held).
  pub(crate) env_edit: Option<Environment>,
  pub(crate) cam_edit: Option<RenderCamera>,
  /// Final render settings.
  settings: RenderSettings,
  /// Use the stored render camera instead of the viewport's.
  use_render_camera: bool,
  job: Option<Job>,
  /// The last render: its image, texture, time and gallery id.
  last: Option<(RenderImage, egui::TextureHandle, f64, Option<u32>)>,
  preview_tex: Option<egui::TextureHandle>,
  progress: (u32, u32),
  error: Option<String>,
  pub(crate) gallery_tex: HashMap<u32, (usize, egui::TextureHandle)>,
  /// The rendering opened from the gallery strip.
  pub(crate) gallery_view: Option<u32>,
  rename: Option<(u32, String)>,
  /// The environment map of the look, by what it was built from.
  env_cache: Option<(String, Arc<EnvMap>)>,
}

impl Default for RenderUi {
  fn default() -> Self {
    RenderUi {
      active: false,
      in_canvas: false,
      run: InCanvasRun::default(),
      appearance_open: false,
      scene_open: false,
      scene_tab: SceneTab::Settings,
      light: None,
      texture_map: None,
      priority: None,
      in_canvas_settings: None,
      canvas_settings: InCanvasSettings::default(),
      render_open: false,
      needs_save: false,
      gallery_folded: false,
      saved_scene: None,
      filter: String::new(),
      category: String::new(),
      picked: None,
      editing: None,
      env_edit: None,
      cam_edit: None,
      settings: RenderSettings { width: 1280, height: 720, samples: 128, ..RenderSettings::default() },
      use_render_camera: false,
      job: None,
      last: None,
      preview_tex: None,
      progress: (0, 0),
      error: None,
      gallery_tex: HashMap::new(),
      gallery_view: None,
      rename: None,
      env_cache: None,
    }
  }
}

/// What the GPU viewport needs to shade the scene in the Render workspace.
pub struct LookData {
  pub env: Arc<EnvMap>,
  pub environment: Environment,
  pub camera: RenderCamera,
  pub materials: Vec<Option<Material>>,
  pub origins: Vec<[f32; 3]>,
  pub bounds: ([f32; 3], [f32; 3]),
  pub lights: Vec<raytrace::Light>,
}

impl LookData {
  pub fn look(&self) -> fox_render_gl::pbr::Look<'_> {
    fox_render_gl::pbr::Look {
      env: &self.env,
      environment: &self.environment,
      camera: &self.camera,
      materials: &self.materials,
      origins: &self.origins,
      bounds: self.bounds,
      lights: &self.lights,
    }
  }
}

impl RenderUi {
  /// The environment the viewport and renders use: the one being edited, else the design's.
  pub(crate) fn environment(&self, app_env: &Environment) -> Environment {
    self.env_edit.clone().unwrap_or_else(|| app_env.clone())
  }

  fn env_map(&mut self, e: &Environment) -> Option<Arc<EnvMap>> {
    let key = format!("{:?}|{:?}|{}|{}", e.preset, e.hdri, e.intensity, e.rotation);
    if let Some((k, m)) = &self.env_cache
      && *k == key
    {
      return Some(m.clone());
    }
    match EnvMap::build(e) {
      Ok(m) => {
        let m = Arc::new(m);
        self.env_cache = Some((key, m.clone()));
        Some(m)
      }
      Err(e) => {
        self.error = Some(e);
        None
      }
    }
  }
}

/// Whether the viewport is shaded with materials now.
pub fn shading(app: &App) -> bool {
  app.render.active && app.render.in_canvas && app.drawing.is_none()
}

/// Fusion's Render canvas while In-canvas Render is off: a flat grey.
pub const CANVAS_GREY: [u8; 3] = [204, 204, 204];

/// The canvas colour in the Render workspace while In-canvas Render is off: the scene's Solid
/// Color background when it has one, else Fusion's flat grey.
pub fn canvas_background(app: &App) -> Option<[u8; 3]> {
  if !app.render.active || shading(app) || app.drawing.is_some() {
    return None;
  }
  let env = app.render.environment(&app.s.design.render.environment);
  Some(if env.background == Background::Color { env.background_color } else { CANVAS_GREY })
}

/// What decides the viewport's look (part of the render key while in-canvas rendering is on).
pub fn look_key(app: &App) -> Value {
  let r = &app.s.design.render;
  json!({
    "materials": r.materials, "bodies": r.bodies, "faces": r.faces,
    "env": app.render.environment(&r.environment),
    "camera": app.render.cam_edit.or(r.camera).map(|c| (c.exposure, c.tone_map)),
    "configuration": app.s.design.configurations.active().map(|c| c.name.clone()),
  })
}

/// Splits body meshes whose faces have different materials and gives every mesh its material,
/// for the GPU viewport. Selected faces (drawn in the selection colour) keep the usual shading.
pub fn viewport_look(app: &mut App, scene: &mut Scene, picks: &PickTable) -> Option<LookData> {
  if !shading(app) {
    return None;
  }
  let s = &app.s;
  let (bodies, faces) = fox_engine::rendering::viewport_materials(s);
  let face_of = |id: u32| match picks.get(id) {
    Some(Pick::Face { body, face }) => Some((body, face)),
    _ => None,
  };
  let mut out = Vec::with_capacity(scene.meshes.len());
  let mut materials = Vec::new();
  for mesh in scene.meshes.drain(..) {
    if !mesh.analyze || mesh.color == SELECT_COLOR || mesh.indices.len() < 3 {
      out.push(mesh);
      materials.push(None);
      continue;
    }
    let first = mesh.tri_ids.first().copied().unwrap_or(mesh.id);
    let Some((body, _)) = face_of(first) else {
      out.push(mesh);
      materials.push(None);
      continue;
    };
    let mat_of = |t: usize| -> Option<&Material> {
      let id = mesh.tri_ids.get(t).copied().unwrap_or(mesh.id);
      let (b, f) = face_of(id)?;
      faces.get(&(b, f)).or_else(|| bodies.get(&b))
    };
    // group triangles by material
    let mut groups: Vec<(Option<Material>, Vec<usize>)> = Vec::new();
    for t in 0..mesh.indices.len() / 3 {
      let m = mat_of(t).cloned();
      match groups.iter_mut().find(|g| g.0 == m) {
        Some(g) => g.1.push(t),
        None => groups.push((m, vec![t])),
      }
    }
    if groups.len() == 1 {
      out.push(mesh);
      materials.push(groups.pop().and_then(|g| g.0).or_else(|| bodies.get(&body).cloned()));
      continue;
    }
    for (m, tris) in groups {
      let mut part = fox_engine::render::Mesh { indices: Vec::new(), tri_ids: Vec::new(), ..mesh.clone() };
      for t in tris {
        part.indices.extend_from_slice(&mesh.indices[t * 3..t * 3 + 3]);
        if let Some(id) = mesh.tri_ids.get(t) {
          part.tri_ids.push(*id);
        }
      }
      out.push(part);
      materials.push(m);
    }
  }
  scene.meshes = out;
  // texture projections centre on each body's box
  let mut centers: HashMap<BodyId, [f32; 3]> = HashMap::new();
  for b in &s.model.bodies {
    if let Some(m) = b.mesh() {
      let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
      for p in &m.positions {
        for k in 0..3 {
          lo[k] = lo[k].min(p[k]);
          hi[k] = hi[k].max(p[k]);
        }
      }
      centers.insert(b.id, [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0));
    }
  }
  let origins = scene
    .meshes
    .iter()
    .map(|m| {
      let id = m.tri_ids.first().copied().unwrap_or(m.id);
      face_of(id).and_then(|(b, _)| centers.get(&b).copied()).unwrap_or([0.0; 3])
    })
    .collect();
  let bounds = s
    .model
    .bounds()
    .map(|(lo, hi)| (lo.as_vec3().to_array(), hi.as_vec3().to_array()))
    .unwrap_or(([-50.0; 3], [50.0; 3]));
  let environment = app.render.environment(&s.design.render.environment);
  let camera = app.render.cam_edit.or(s.design.render.camera).unwrap_or_default();
  let env = app.render.env_map(&environment)?;
  let lights = s.design.render.lights.clone();
  Some(LookData { env, environment, camera, materials, origins, bounds, lights })
}

// ---------------------------------------------------------------- windows

/// The workspace's windows; also takes finished renders.
pub fn windows(app: &mut App, ctx: &egui::Context) {
  poll_job(app, ctx);
  if !app.render.active {
    return;
  }
  // a command dialog (Physical Material, Move/Copy) takes the docked dialogs' place
  if app.dialog.is_some() {
    let r = &mut app.render;
    r.scene_open = false;
    r.light = None;
    r.texture_map = None;
    r.priority = None;
    r.in_canvas_settings = None;
  }
  appearance_window(app, ctx);
  crate::render_panels::scene_dialog(app, ctx);
  crate::render_panels::light_dialog(app, ctx);
  crate::render_tools::texture_map_dialog(app, ctx);
  crate::render_tools::priority_dialog(app, ctx);
  crate::render_tools::in_canvas_settings_dialog(app, ctx);
  crate::render_panels::needs_save(app, ctx);
  crate::render_panels::in_canvas_bar(app, ctx);
  render_window(app, ctx);
  gallery_window(app, ctx);
  commit_edits(app, ctx);
}

fn swatch(ui: &mut egui::Ui, m: &Material, size: f32) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
  let c = Color32::from_rgb(m.color[0], m.color[1], m.color[2]);
  let p = ui.painter();
  p.circle_filled(rect.center(), size / 2.0 - 1.0, c);
  // a highlight that's sharper the smoother the material
  let shine = (1.0 - m.roughness).powi(2) * if m.metallic > 0.5 { 1.0 } else { 0.6 };
  p.circle_filled(
    rect.center() + egui::vec2(-size * 0.15, -size * 0.15),
    size * (0.08 + 0.12 * m.roughness),
    Color32::from_white_alpha((shine * 220.0) as u8),
  );
  if m.transmission > 0.0 {
    p.circle_stroke(rect.center(), size / 2.0 - 1.5, egui::Stroke::new(1.0, Color32::from_white_alpha(160)));
  }
  if let Some(t) = &m.texture
    && let TextureSource::Pattern(_) = t.source
  {
    let c2 = Color32::from_rgb(t.color2[0], t.color2[1], t.color2[2]);
    p.line_segment([rect.left_center() + egui::vec2(3.0, 2.0), rect.right_center() - egui::vec2(3.0, -2.0)], (1.5, c2));
  }
  resp
}

/// The selected bodies and faces, as `render.material.apply` takes them.
fn selection_params(app: &App) -> Option<Value> {
  let mut bodies = Vec::new();
  let mut faces = Vec::new();
  for s in &app.s.selection {
    match s {
      Sel::Body { body } => bodies.push(body.0),
      Sel::Face { body, face } => faces.push(json!({"body": body.0, "face": face})),
      _ => {}
    }
  }
  if !faces.is_empty() {
    Some(json!({"faces": faces}))
  } else if !bodies.is_empty() {
    Some(json!({"bodies": bodies}))
  } else {
    None
  }
}

fn apply(app: &mut App, material: Option<&str>) {
  let Some(mut p) = selection_params(app) else {
    app.status = Some(("Select bodies or faces first".into(), false));
    return;
  };
  if let Some(m) = material {
    p["material"] = json!(m);
  }
  if app.run("render.material.apply", p).is_some() {
    app.status = Some((
      match material {
        Some(m) => format!("Applied {m}"),
        None => "Material removed: the appearance shows again".into(),
      },
      false,
    ));
  }
}

fn appearance_window(app: &mut App, ctx: &egui::Context) {
  let mut open = app.render.appearance_open;
  if !open {
    return;
  }
  let lib: Vec<Material> = match app.s.execute("render.materials", &json!({})) {
    Ok(v) => serde_json::from_value(v).unwrap_or_default(),
    Err(_) => Vec::new(),
  };
  let mut apply_now: Option<Option<String>> = None;
  let mut edit: Option<Material> = None;
  egui::Window::new("Appearance").open(&mut open).default_pos([60.0, 140.0]).default_width(300.0).show(ctx, |ui| {
    let r = &mut app.render;
    ui.horizontal(|ui| {
      ui.label("Search");
      ui.add(egui::TextEdit::singleline(&mut r.filter).desired_width(150.0));
      let mut cats: Vec<String> = Vec::new();
      for m in &lib {
        if !cats.contains(&m.category) {
          cats.push(m.category.clone());
        }
      }
      egui::ComboBox::from_id_salt("render-cat")
        .selected_text(if r.category.is_empty() { "All" } else { r.category.as_str() })
        .show_ui(ui, |ui| {
          ui.selectable_value(&mut r.category, String::new(), "All");
          for c in cats {
            ui.selectable_value(&mut r.category, c.clone(), c);
          }
        });
    });
    ui.label(RichText::new("Pick a material, select bodies or faces, then Apply.").weak());
    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
      let q = r.filter.to_ascii_lowercase();
      let mut last_cat = String::new();
      for m in lib.iter().filter(|m| {
        (r.category.is_empty() || m.category == r.category)
          && (q.is_empty() || m.name.to_ascii_lowercase().contains(&q))
      }) {
        if m.category != last_cat {
          ui.label(RichText::new(&m.category).strong());
          last_cat = m.category.clone();
        }
        ui.horizontal(|ui| {
          let picked = r.picked.as_deref() == Some(m.name.as_str());
          let s = swatch(ui, m, 20.0);
          let l = ui.selectable_label(picked, &m.name);
          if s.clicked() || l.clicked() {
            r.picked = Some(m.name.clone());
          }
          if s.double_clicked() || l.double_clicked() {
            apply_now = Some(Some(m.name.clone()));
          }
        });
      }
    });
    ui.separator();
    ui.horizontal(|ui| {
      let picked = r.picked.clone();
      if ui.add_enabled(picked.is_some(), egui::Button::new("Apply to Selection")).clicked() {
        apply_now = Some(picked.clone());
      }
      if ui.button("Remove").on_hover_text("Take the render material off the selection").clicked() {
        apply_now = Some(None);
      }
      if ui.add_enabled(picked.is_some(), egui::Button::new("Edit Copy…")).clicked()
        && let Some(m) = lib.iter().find(|m| Some(&m.name) == picked.as_ref())
      {
        let mut m = m.clone();
        if !lib.iter().any(|x| x.name == m.name && x.category == "Design") {
          m.name = format!("{} (copy)", m.name);
        }
        edit = Some(m);
      }
    });
    // in this design
    let used: Vec<(u32, String)> = app
      .s
      .model
      .bodies
      .iter()
      .filter(|b| app.s.body_visible(b.id))
      .map(|b| (b.id.0, fox_engine::rendering::body_material(&app.s, b.id).name))
      .collect();
    if !used.is_empty() {
      ui.separator();
      ui.label(RichText::new("In this design").strong());
      for (b, m) in used {
        ui.label(format!("{} — {m}", app.s.body_name(BodyId(b))));
      }
    }
  });
  app.render.appearance_open = open;
  if let Some(m) = edit {
    app.render.editing = Some(m);
  }
  if let Some(m) = apply_now {
    apply(app, m.as_deref());
  }
  material_editor(app, ctx);
}

pub(crate) fn slider(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) {
  ui.label(label);
  ui.add(egui::Slider::new(v, range));
  ui.end_row();
}

pub(crate) fn color_edit(ui: &mut egui::Ui, label: &str, c: &mut [u8; 3]) {
  ui.label(label);
  ui.color_edit_button_srgb(c);
  ui.end_row();
}

fn material_editor(app: &mut App, ctx: &egui::Context) {
  let Some(mut m) = app.render.editing.take() else { return };
  let mut open = true;
  let (mut save, mut cancel) = (false, false);
  egui::Window::new("Edit Material").open(&mut open).default_width(320.0).show(ctx, |ui| {
    egui::Grid::new("mat-edit").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
      ui.label("Name");
      ui.text_edit_singleline(&mut m.name);
      ui.end_row();
      color_edit(ui, "Colour", &mut m.color);
      slider(ui, "Metallic", &mut m.metallic, 0.0..=1.0);
      slider(ui, "Roughness", &mut m.roughness, 0.0..=1.0);
      slider(ui, "Clear coat", &mut m.clearcoat, 0.0..=1.0);
      slider(ui, "Coat roughness", &mut m.clearcoat_roughness, 0.0..=1.0);
      slider(ui, "Transmission", &mut m.transmission, 0.0..=1.0);
      slider(ui, "Index of refraction", &mut m.ior, 1.0..=2.5);
      color_edit(ui, "Emission", &mut m.emission);
      slider(ui, "Emission strength", &mut m.emission_strength, 0.0..=50.0);
      ui.label("Texture");
      let current = match &m.texture {
        None => "None".to_string(),
        Some(t) => match &t.source {
          TextureSource::Image(p) => {
            format!("Image: {}", std::path::Path::new(p).file_name().map(|f| f.to_string_lossy()).unwrap_or_default())
          }
          TextureSource::Pattern(p) => p.name().to_string(),
        },
      };
      egui::ComboBox::from_id_salt("mat-tex").selected_text(current).show_ui(ui, |ui| {
        if ui.selectable_label(m.texture.is_none(), "None").clicked() {
          m.texture = None;
        }
        for p in Pattern::ALL {
          if ui.selectable_label(false, p.name()).clicked() {
            let t = m.texture.clone().unwrap_or_default();
            m.texture = Some(TextureSpec { source: TextureSource::Pattern(p), ..t });
          }
        }
        if ui.selectable_label(false, "Image…").clicked()
          && let Some(path) =
            rfd::FileDialog::new().add_filter("Image", &["png", "jpg", "jpeg", "bmp", "webp"]).pick_file()
        {
          let t = m.texture.clone().unwrap_or_default();
          m.color = [255, 255, 255];
          m.texture = Some(TextureSpec { source: TextureSource::Image(path.display().to_string()), ..t });
        }
      });
      ui.end_row();
      if let Some(t) = &mut m.texture {
        if matches!(t.source, TextureSource::Pattern(_)) {
          color_edit(ui, "Pattern colour", &mut t.color2);
        }
        ui.label("Projection");
        egui::ComboBox::from_id_salt("mat-proj").selected_text(format!("{:?}", t.projection)).show_ui(ui, |ui| {
          for p in crate::render_tools::PROJECTIONS {
            ui.selectable_value(&mut t.projection, p, format!("{p:?}"));
          }
        });
        ui.end_row();
        if !matches!(t.projection, Projection::Box | Projection::Automatic) {
          ui.label("Axis");
          ui.horizontal(|ui| {
            for (a, n) in [(raytrace::Axis::X, "X"), (raytrace::Axis::Y, "Y"), (raytrace::Axis::Z, "Z")] {
              ui.selectable_value(&mut t.axis, a, n);
            }
          });
          ui.end_row();
        }
        ui.label("Scale (mm)");
        ui.add(egui::DragValue::new(&mut t.scale).range(0.1..=10000.0).speed(0.5));
        ui.end_row();
        slider(ui, "Rotation (°)", &mut t.rotation, -180.0..=180.0);
        slider(ui, "Roughness from texture", &mut t.roughness_variation, -1.0..=1.0);
      }
    });
    ui.horizontal(|ui| {
      save = ui.button("Save to Design").clicked();
      cancel = ui.button("Cancel").clicked();
    });
  });
  if save {
    let mut p = serde_json::to_value(&m).unwrap_or(json!({}));
    if let Some(o) = p.as_object_mut() {
      o.remove("category");
      // the texture goes in as the command takes it
      o.remove("texture");
    }
    p["texture"] = match &m.texture {
      None => Value::Null,
      Some(t) => {
        let mut v = json!({
          "color2": t.color2, "projection": format!("{:?}", t.projection).to_ascii_lowercase(),
          "axis": format!("{:?}", t.axis).to_ascii_lowercase(), "scale": t.scale, "rotation": t.rotation,
          "offset": t.offset, "roughness_variation": t.roughness_variation,
        });
        match &t.source {
          TextureSource::Image(path) => v["image"] = json!(path),
          TextureSource::Pattern(pat) => v["pattern"] = json!(pat.name()),
        }
        v
      }
    };
    if app.run("render.material.define", p).is_some() {
      app.render.picked = Some(m.name.clone());
      app.status = Some((format!("Saved material {}", m.name), false));
    }
    return;
  }
  if open && !cancel {
    app.render.editing = Some(m);
  }
}

/// Writes what Scene Settings changed into the design once no slider is held.
fn commit_edits(app: &mut App, ctx: &egui::Context) {
  if ctx.input(|i| i.pointer.any_down()) {
    return;
  }
  if let Some(e) = app.render.env_edit.take() {
    let mut p = serde_json::to_value(&e).unwrap_or(json!({}));
    if e.hdri.is_none() {
      p["hdri"] = Value::Null;
    }
    app.run("render.environment", p);
  }
  if let Some(c) = app.render.cam_edit.take() {
    let v = c.view;
    app.run(
      "render.camera",
      json!({
        "target": v.target, "yaw": v.yaw.to_degrees(), "pitch": v.pitch.to_degrees(), "roll": v.roll.to_degrees(),
        "distance": v.distance, "fov": v.fov.to_degrees(), "ortho": v.ortho, "fstop": c.fstop,
        "focus_distance": c.focus_distance, "exposure": c.exposure, "tone_map": c.tone_map,
      }),
    );
  }
}

const RESOLUTIONS: [(&str, u32, u32); 6] = [
  ("Thumbnail (400 × 300)", 400, 300),
  ("Web (800 × 600)", 800, 600),
  ("HD (1280 × 720)", 1280, 720),
  ("Full HD (1920 × 1080)", 1920, 1080),
  ("Square (1024 × 1024)", 1024, 1024),
  ("4K (3840 × 2160)", 3840, 2160),
];

const QUALITY: [(&str, u32); 4] =
  [("Draft (16)", 16), ("Preview (64)", 64), ("Final (256)", 256), ("Excellent (1024)", 1024)];

fn start_render(app: &mut App) {
  if app.render.job.is_some() {
    return;
  }
  let st = app.render.settings;
  let mut cam = if app.render.use_render_camera {
    fox_engine::rendering::render_camera(&app.s)
  } else {
    RenderCamera { view: app.cam, ..fox_engine::rendering::render_camera(&app.s) }
  };
  if let Some(c) = app.render.cam_edit {
    cam.exposure = c.exposure;
    cam.tone_map = c.tone_map;
    cam.fstop = c.fstop;
    cam.focus_distance = c.focus_distance;
  }
  let mut scene = fox_engine::rendering::render_scene(&app.s, cam, None);
  scene.environment = app.render.environment(&app.s.design.render.environment);
  let state = Arc::new(Mutex::new(JobState { total: st.samples, ..JobState::default() }));
  let cancel = Arc::new(AtomicBool::new(false));
  let (st2, cancel2) = (state.clone(), cancel.clone());
  std::thread::spawn(move || {
    let mut p = match Progressive::new(&scene, st) {
      Ok(p) => p,
      Err(e) => {
        if let Ok(mut s) = st2.lock() {
          s.finished = Some(Err(e));
        }
        return;
      }
    };
    let mut last: Option<Instant> = None;
    while !p.done() && !cancel2.load(Ordering::Relaxed) {
      // short passes first, so the picture appears quickly
      let pass = (p.accum.samples / 2).clamp(1, 16);
      if p.step(pass, &cancel2) == 0 {
        break;
      }
      let preview = last.is_none_or(|t| t.elapsed().as_secs_f32() > 0.3).then(|| {
        last = Some(Instant::now());
        p.image(false)
      });
      if let Ok(mut s) = st2.lock() {
        s.samples = p.accum.samples;
        if preview.is_some() {
          s.preview = preview;
        }
      }
    }
    let result = if cancel2.load(Ordering::Relaxed) && p.accum.samples == 0 {
      Err("cancelled".to_string())
    } else {
      Ok(p.image(st.denoise))
    };
    if let Ok(mut s) = st2.lock() {
      s.samples = p.accum.samples;
      s.finished = Some(result);
    }
  });
  app.render.job = Some(Job {
    state,
    cancel,
    started: Instant::now(),
    name: format!("Render {}", app.s.design.render.next_gallery_id()),
  });
  app.render.error = None;
  app.render.progress = (0, st.samples);
}

fn texture_of(ctx: &egui::Context, name: &str, img: &RenderImage) -> egui::TextureHandle {
  let c = egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.rgba);
  ctx.load_texture(name, c, egui::TextureOptions::LINEAR)
}

/// Takes the progress and the result of a running render.
fn poll_job(app: &mut App, ctx: &egui::Context) {
  let Some(job) = &app.render.job else { return };
  let (preview, finished, samples, total) = match job.state.lock() {
    Ok(mut s) => (s.preview.take(), s.finished.take(), s.samples, s.total),
    Err(_) => (None, Some(Err("the render stopped".to_string())), 0, 0),
  };
  app.render.progress = (samples, total);
  if let Some(img) = preview {
    app.render.preview_tex = Some(texture_of(ctx, "render-preview", &img));
  }
  let Some(result) = finished else {
    ctx.request_repaint_after(std::time::Duration::from_millis(150));
    return;
  };
  let Some(job) = app.render.job.take() else { return };
  let seconds = job.started.elapsed().as_secs_f64();
  match result {
    Ok(img) => {
      let tex = texture_of(ctx, "render-final", &img);
      // into the gallery (a command, so it's undoable and saved with the design)
      let mut id = None;
      match img.to_png() {
        Ok(png) => {
          let path = std::env::temp_dir().join(format!("fissionoxide-render-{}.png", std::process::id()));
          if std::fs::write(&path, png).is_ok() {
            id = app
              .run(
                "render.gallery.add",
                json!({"path": path, "name": job.name, "samples": img.samples, "seconds": seconds}),
              )
              .and_then(|v| v["id"].as_u64())
              .map(|v| v as u32);
            let _ = std::fs::remove_file(&path);
          }
        }
        Err(e) => app.render.error = Some(e),
      }
      app.status =
        Some((format!("Rendered {} samples in {seconds:.1} s; kept in the Rendering Gallery", img.samples), false));
      app.render.last = Some((img, tex, seconds, id));
      app.render.preview_tex = None;
    }
    Err(e) => {
      app.render.error = Some(e);
      app.render.preview_tex = None;
    }
  }
}

fn image_fit(ui: &mut egui::Ui, tex: &egui::TextureHandle, max: egui::Vec2) {
  let s = tex.size_vec2();
  let k = (max.x / s.x).min(max.y / s.y).min(1.0);
  ui.image((tex.id(), s * k));
}

fn render_window(app: &mut App, ctx: &egui::Context) {
  let mut open = app.render.render_open;
  if !open {
    return;
  }
  let (mut start, mut cancel, mut save) = (false, false, false);
  let has_cam = app.s.design.render.camera.is_some();
  egui::Window::new("Render").open(&mut open).default_width(460.0).show(ctx, |ui| {
    let r = &mut app.render;
    let running = r.job.is_some();
    ui.add_enabled_ui(!running, |ui| {
      egui::Grid::new("render-form").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Image size");
        let current = RESOLUTIONS
          .iter()
          .find(|x| (x.1, x.2) == (r.settings.width, r.settings.height))
          .map(|x| x.0.to_string())
          .unwrap_or_else(|| format!("Custom ({} × {})", r.settings.width, r.settings.height));
        egui::ComboBox::from_id_salt("render-res").selected_text(current).width(200.0).show_ui(ui, |ui| {
          for (n, w, h) in RESOLUTIONS {
            if ui.selectable_label((w, h) == (r.settings.width, r.settings.height), n).clicked() {
              (r.settings.width, r.settings.height) = (w, h);
            }
          }
        });
        ui.end_row();
        ui.label("Width × height");
        ui.horizontal(|ui| {
          ui.add(egui::DragValue::new(&mut r.settings.width).range(16..=raytrace::tracer::MAX_SIZE));
          ui.label("×");
          ui.add(egui::DragValue::new(&mut r.settings.height).range(16..=raytrace::tracer::MAX_SIZE));
        });
        ui.end_row();
        ui.label("Quality (samples)");
        ui.horizontal(|ui| {
          egui::ComboBox::from_id_salt("render-q")
            .selected_text(QUALITY.iter().find(|q| q.1 == r.settings.samples).map(|q| q.0).unwrap_or("Custom"))
            .show_ui(ui, |ui| {
              for (n, s) in QUALITY {
                ui.selectable_value(&mut r.settings.samples, s, n);
              }
            });
          ui.add(egui::DragValue::new(&mut r.settings.samples).range(1..=100_000));
        });
        ui.end_row();
        ui.label("Denoise");
        ui.checkbox(&mut r.settings.denoise, "");
        ui.end_row();
        ui.label("Camera");
        ui.horizontal(|ui| {
          ui.selectable_value(&mut r.use_render_camera, false, "Viewport");
          ui.add_enabled_ui(has_cam, |ui| {
            ui.selectable_value(&mut r.use_render_camera, true, "Stored render camera");
          });
        });
        ui.end_row();
      });
    });
    ui.horizontal(|ui| {
      if running {
        cancel = ui.button("Stop").clicked();
        let (done, total) = r.progress;
        ui.add(
          egui::ProgressBar::new(done as f32 / total.max(1) as f32)
            .text(format!("{done} / {total} samples"))
            .desired_width(260.0),
        );
      } else {
        start = ui.button(RichText::new("Render").strong()).clicked();
        save = ui.add_enabled(r.last.is_some(), egui::Button::new("Save Image…")).clicked();
      }
    });
    if let Some(e) = &r.error {
      ui.colored_label(Color32::from_rgb(220, 80, 60), e);
    }
    let max = egui::vec2(ui.available_width().max(300.0), 360.0);
    if let Some(t) = &r.preview_tex {
      image_fit(ui, t, max);
    } else if let Some((img, t, secs, _)) = &r.last {
      image_fit(ui, t, max);
      ui.label(RichText::new(format!("{} × {}, {} samples, {secs:.1} s", img.width, img.height, img.samples)).weak());
    }
  });
  app.render.render_open = open;
  if cancel && let Some(j) = &app.render.job {
    j.cancel.store(true, Ordering::Relaxed);
  }
  if start {
    start_render(app);
  }
  if save
    && let Some((img, ..)) = &app.render.last
    && let Some(path) = rfd::FileDialog::new()
      .add_filter("PNG image", &["png"])
      .set_file_name(format!("{}.png", app.s.design.name))
      .save_file()
  {
    match img.save_png(&path) {
      Ok(()) => app.status = Some((format!("Saved {}", path.display()), false)),
      Err(e) => app.status = Some((e, true)),
    }
  }
}

/// Decodes the gallery's thumbnails once per image.
pub(crate) fn gallery_textures(app: &mut App, ctx: &egui::Context) {
  for g in &app.s.design.render.gallery {
    let len = g.png.0.len();
    if app.render.gallery_tex.get(&g.id).is_none_or(|(l, _)| *l != len)
      && let Ok(img) = image::load_from_memory(&g.png.0)
    {
      let rgba = img.to_rgba8();
      let c = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
      let t = ctx.load_texture(format!("gallery-{}", g.id), c, egui::TextureOptions::LINEAR);
      app.render.gallery_tex.insert(g.id, (len, t));
    }
  }
}

/// A rendering opened from the Rendering Gallery: full size, with Export, Rename and Delete.
fn gallery_window(app: &mut App, ctx: &egui::Context) {
  let Some(id) = app.render.gallery_view else { return };
  let Some(g) = app.s.design.render.gallery.iter().find(|g| g.id == id).cloned() else {
    app.render.gallery_view = None;
    return;
  };
  gallery_textures(app, ctx);
  let mut open = true;
  let mut export = false;
  let mut delete = false;
  let mut rename: Option<String> = None;
  egui::Window::new(g.name.clone()).id(egui::Id::new("rendering")).open(&mut open).default_width(560.0).show(
    ctx,
    |ui| {
      let r = &mut app.render;
      if let Some((_, t)) = r.gallery_tex.get(&id) {
        image_fit(ui, t, egui::vec2(ui.available_width().max(300.0), 420.0));
      }
      ui.label(RichText::new(format!("{} × {}, {} samples, {:.1} s", g.width, g.height, g.samples, g.seconds)).weak());
      ui.horizontal(|ui| {
        export = ui.button("Export PNG…").clicked();
        match &mut r.rename {
          Some((rid, text)) if *rid == id => {
            ui.text_edit_singleline(text);
            if ui.button("OK").clicked() {
              rename = Some(text.clone());
            }
          }
          _ => {
            if ui.button("Rename").clicked() {
              r.rename = Some((id, g.name.clone()));
            }
          }
        }
        delete = ui.button("Delete").clicked();
      });
    },
  );
  if !open {
    app.render.gallery_view = None;
  }
  if let Some(name) = rename {
    app.run("render.gallery.rename", json!({"id": id, "name": name}));
    app.render.rename = None;
  }
  if delete {
    app.run("render.gallery.delete", json!({"id": id}));
    app.render.gallery_view = None;
    app.render.gallery_tex.remove(&id);
  }
  if export
    && let Some(path) = rfd::FileDialog::new().add_filter("PNG image", &["png"]).set_file_name("render.png").save_file()
    && app.run("render.gallery.export", json!({"id": id, "path": path})).is_some()
  {
    app.status = Some((format!("Wrote {}", path.display()), false));
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;
  use fox_engine::scene::ViewOptions;

  #[test]
  fn the_render_workspace_shades_bodies_with_their_materials() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 20, "depth": 20, "height": 10}));
    let body = app.s.model.bodies[0].id.0;
    crate::panels::switch_workspace(&mut app, "Render");
    // In-canvas Render is off until pressed
    assert!(app.render.active && !shading(&app));
    RenderCmd::InCanvas.run(&mut app);
    assert!(shading(&app));
    // a material on the top face splits the body's mesh in the viewport
    app.run("render.material.apply", json!({"material": "Gold - Polished", "faces": [{"point": [0, 0, 10]}]}));
    app.run("render.material.apply", json!({"material": "Rubber - Black", "body": body}));
    let (mut scene, picks) = app.scene.build(&app.s, &ViewOptions::default());
    let look = viewport_look(&mut app, &mut scene, &picks).expect("a look in the Render workspace");
    assert_eq!(look.materials.len(), scene.meshes.len());
    let names: Vec<&str> = look.materials.iter().flatten().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"Gold - Polished") && names.contains(&"Rubber - Black"), "{names:?}");
    // the look key changes with the materials, so the viewport rebuilds
    let before = look_key(&app);
    app.run("render.material.apply", json!({"body": body}));
    assert_ne!(look_key(&app), before);
    crate::panels::switch_workspace(&mut app, "Design");
    assert!(!app.render.active);
    let (mut scene, picks) = app.scene.build(&app.s, &ViewOptions::default());
    assert!(viewport_look(&mut app, &mut scene, &picks).is_none());
  }
}
