//! Fusion's file windows: the white frame Save, Open, Export and Image Options share, File ▸
//! Open... (the projects and Fusion's sample groups, a design table, "Open from my computer..."),
//! File ▸ Export... (name, type, location), and which windows Escape closes.

use std::path::{Path, PathBuf};

use egui::{Color32, RichText, Stroke, Vec2, pos2, vec2};
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::{App, theme};

/// The windows' inner margin.
pub(crate) const MARGIN: f32 = 16.0;
/// The size of the windows' buttons.
pub(crate) const BUTTON: Vec2 = vec2(80.0, 26.0);
/// The height of the windows' title bar.
pub(crate) const TITLE_H: f32 = 36.0;

/// File ▸ Open... and File ▸ Export..., while open.
#[derive(Debug, Default)]
pub struct Files {
  pub open: Option<OpenForm>,
  pub export: Option<ExportForm>,
}

/// The id of a modal window's area (for tests and for finding it on screen).
pub(crate) fn modal_id(name: &str) -> egui::Id {
  egui::Id::new(("file-modal", name))
}

/// The id of a floating (not modal) window's area.
pub(crate) fn floating_id(name: &str) -> egui::Id {
  egui::Id::new(("file-floating", name))
}

/// The white frame of the file windows.
fn window_frame() -> egui::Frame {
  egui::Frame::new()
    .fill(Color32::WHITE)
    .stroke(Stroke::new(1.0, Color32::from_gray(180)))
    .corner_radius(3)
    .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(45) })
}

/// The title, an X at the right and (Image Options) the app's icon at the left. Returns whether
/// the X was clicked.
fn title_bar(ui: &mut egui::Ui, title: &str, width: f32, logo: bool) -> bool {
  let (rect, _) = ui.allocate_exact_size(vec2(width, TITLE_H), egui::Sense::hover());
  let mut x = rect.left() + MARGIN;
  if logo {
    crate::logo::paint(ui.painter(), egui::Rect::from_center_size(pos2(x + 9.0, rect.center().y), Vec2::splat(18.0)));
    x += 26.0;
  }
  ui.painter().text(
    pos2(x, rect.center().y),
    egui::Align2::LEFT_CENTER,
    title,
    egui::FontId::proportional(14.0),
    theme::TEXT,
  );
  let close = egui::Rect::from_center_size(rect.right_center() - vec2(20.0, 0.0), Vec2::splat(22.0));
  let r = ui.interact(close, ui.id().with(("title-x", title)), egui::Sense::click());
  if r.hovered() {
    ui.painter().rect_filled(close, 2.0, Color32::from_gray(228));
  }
  icons::draw(ui.painter(), close.shrink(3.0), Icon::Close, theme::ICON_INK);
  r.on_hover_text("Close").clicked()
}

/// A window's title bar and contents.
fn body<R>(ui: &mut egui::Ui, title: &str, width: f32, logo: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> (R, bool) {
  ui.set_width(width);
  ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
  let closed = title_bar(ui, title, width, logo);
  let inner = egui::Frame::new()
    .inner_margin(egui::Margin { left: MARGIN as i8, right: MARGIN as i8, top: 4, bottom: MARGIN as i8 })
    .show(ui, |ui| {
      ui.set_width(width - 2.0 * MARGIN);
      ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
      add(ui)
    })
    .inner;
  (inner, closed)
}

/// A white modal window, as Fusion's Save, Open and Export: the title and an X on top, then the
/// contents. Nothing behind it takes clicks while it shows, and Escape doesn't close it (only
/// Cancel or the X do, as in Fusion). Returns the contents' value and whether the X was clicked.
pub(crate) fn modal<R>(
  ctx: &egui::Context,
  name: &str,
  title: &str,
  width: f32,
  add: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, bool) {
  egui::Modal::new(modal_id(name))
    .backdrop_color(Color32::from_black_alpha(20))
    .frame(window_frame())
    .show(ctx, |ui| body(ui, title, width, false, add))
    .inner
}

/// A white window in the middle of the screen with the app's icon in its title bar, as Fusion's
/// Image Options: the rest of the app still works while it shows, and Escape closes it.
pub(crate) fn floating<R>(
  ctx: &egui::Context,
  name: &str,
  title: &str,
  width: f32,
  add: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, bool) {
  egui::Area::new(floating_id(name))
    .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
    .order(egui::Order::Foreground)
    .show(ctx, |ui| window_frame().show(ui, |ui| body(ui, title, width, true, add)).inner)
    .inner
}

/// The blue button of the file windows (Save, Open, Export, OK).
pub(crate) fn blue_button(label: &str) -> egui::Button<'_> {
  egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(theme::ACCENT).corner_radius(3).min_size(BUTTON)
}

/// The white button of the file windows (Cancel, Open from my computer...).
pub(crate) fn white_button(label: &str) -> egui::Button<'_> {
  egui::Button::new(RichText::new(label).color(theme::TEXT))
    .fill(Color32::WHITE)
    .stroke(Stroke::new(1.0, theme::BORDER))
    .corner_radius(3)
    .min_size(BUTTON)
}

/// What a window's button row was clicked on.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Footer {
  pub left: bool,
  pub primary: bool,
  pub cancel: bool,
}

/// The window's last row: an optional button at the left, Cancel and the blue `primary` at the
/// right, as in Fusion.
pub(crate) fn footer(ui: &mut egui::Ui, left: Option<&str>, primary: &str, enabled: bool) -> Footer {
  let mut out = Footer::default();
  ui.add_space(12.0);
  ui.allocate_ui_with_layout(
    vec2(ui.available_width(), BUTTON.y),
    egui::Layout::left_to_right(egui::Align::Center),
    |ui| {
      if let Some(l) = left {
        out.left = ui.add(white_button(l)).clicked();
      }
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        out.primary = ui.add_enabled(enabled, blue_button(primary)).clicked();
        out.cancel = ui.add(white_button("Cancel")).clicked();
      });
    },
  );
  out
}

/// A field as wide as the window whose text is selected the first time it shows (`first`), as
/// Fusion's Name fields.
pub(crate) fn name_field(ui: &mut egui::Ui, text: &mut String, first: &mut bool) -> egui::Response {
  let out = egui::TextEdit::singleline(text).desired_width(f32::INFINITY).margin(vec2(6.0, 5.0)).show(ui);
  if std::mem::take(first) {
    out.response.response.request_focus();
    let mut state = out.state;
    let all =
      egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(text.chars().count()));
    state.cursor.set_char_range(Some(all));
    state.store(ui.ctx(), out.response.response.id);
  }
  out.response.response
}

/// Whether one of the modal windows (Save, Open, Export, a sketch's Export DXF, Render's "save
/// first") shows: the app's shortcuts wait.
pub(crate) fn modal_open(app: &App) -> bool {
  app.data.save.is_some()
    || app.files.open.is_some()
    || app.files.export.is_some()
    || app.docs.modal_open()
    || app.inserts.modal_open()
    || app.export_dxf.is_some()
    || app.render.needs_save
}

/// Escape closes the docked 3D PRINT dialog, the Image Options window and the data panel's
/// prompts (one per press, the prompt first), as in Fusion. An open drop-down closes first (egui
/// does that), and the modal Save, Open and Export windows close only with Cancel or their X.
/// Returns whether the key was taken.
pub(crate) fn escape(app: &mut App, ctx: &egui::Context) -> bool {
  let pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape) && !i.modifiers.any());
  // the data panel's Upload, Move and Copy windows and its inline name editor take Escape
  if !pressed || egui::Popup::is_any_open(ctx) || modal_open(app) || app.data.takes_keys() {
    return false;
  }
  let closed = if app.data.prompt_open() {
    app.data.close_prompt();
    true
  } else {
    app.capture.image.take().is_some() || app.capture.print.take().is_some()
  };
  if closed {
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
  }
  closed
}

/// Open and Export, while open.
pub fn windows(app: &mut App, ctx: &egui::Context) {
  open_window(app, ctx);
  export_window(app, ctx);
}

fn s(v: &Value, k: &str) -> String {
  v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// A row of the Open window's tree or table, filled when selected and tinted on hover.
pub(crate) fn row(ui: &mut egui::Ui, selected: bool) -> (egui::Rect, egui::Response) {
  let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), egui::Sense::click());
  if selected {
    ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(214, 232, 250));
  } else if r.hovered() {
    ui.painter().rect_filled(rect, 0.0, theme::HOVER);
  }
  (rect, r)
}

pub(crate) fn row_text(ui: &egui::Ui, rect: egui::Rect, x: f32, text: &str, color: Color32) {
  let font = egui::FontId::proportional(13.0);
  ui.painter().text(pos2(rect.left() + x, rect.center().y), egui::Align2::LEFT_CENTER, text, font, color);
}

// ---------------------------------------------------------------- Open

/// Fusion's sample groups in the Open window's tree. They are Autodesk's cloud content, which
/// FissionOxide doesn't have, so they list nothing.
pub(crate) const SAMPLES: [&str; 6] = [
  "Workshops & Events",
  "CAM Samples",
  "Electronics Samples",
  "Simulation Samples",
  "Community Samples",
  "Design Samples",
];

const SAMPLES_EMPTY: &str = "Fusion's samples live in Autodesk's cloud; FissionOxide has none to show.";

/// What is picked in the Open window's table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
  Folder(String),
  Item(String),
}

/// File ▸ Open...'s window.
#[derive(Debug, Default)]
pub struct OpenForm {
  /// The project picked in the tree (None while a sample group is).
  pub project: Option<String>,
  /// The sample group picked in the tree.
  pub sample: Option<usize>,
  pub folder: Option<String>,
  pub selected: Option<Pick>,
  /// The new project's name while its field shows, and whether it just appeared.
  pub new_project: Option<(String, bool)>,
  pub projects: Vec<Value>,
  pub listing: Value,
  stale: bool,
}

/// File ▸ Open... (Ctrl+O): Fusion's Open window on the folder the data panel shows.
pub fn open(app: &mut App) {
  let (project, folder) = app.data.location();
  app.files.open = Some(OpenForm { project, folder, stale: true, ..Default::default() });
  refresh_open(app);
}

/// Re-reads the projects and the picked folder's listing when they are stale.
pub(crate) fn refresh_open(app: &mut App) {
  if !app.files.open.as_ref().is_some_and(|f| f.stale) {
    return;
  }
  let projects = app.s.execute("hub.projects", &json!({})).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
  let Some(f) = app.files.open.as_mut() else { return };
  f.stale = false;
  if f.sample.is_none() && f.project.as_ref().is_none_or(|p| !projects.iter().any(|x| s(x, "id") == *p)) {
    // Fusion opens on the default project
    let first = projects.iter().find(|p| s(p, "name") == "Default Project").or(projects.first());
    f.project = first.map(|p| s(p, "id"));
    f.folder = None;
  }
  f.projects = projects;
  let (project, folder, sample) = (f.project.clone(), f.folder.clone(), f.sample);
  let listing = match (project, sample) {
    (Some(p), None) => app.s.execute("hub.list", &json!({"project": p, "folder": folder})).unwrap_or(Value::Null),
    _ => Value::Null,
  };
  if let Some(f) = app.files.open.as_mut() {
    f.listing = listing;
  }
}

/// What the Open window asks for.
enum OpenAct {
  None,
  Open,
  Navigate(String),
  OpenItem(String),
  FromComputer,
  Close,
  CreateProject(String),
}

fn open_window(app: &mut App, ctx: &egui::Context) {
  refresh_open(app);
  let Some(mut f) = app.files.open.take() else { return };
  let now = fox_engine::store::now();
  let mut act = OpenAct::None;
  const TABLE_H: f32 = 330.0;
  let (_, closed) = modal(ctx, "open", "Open", 760.0, |ui| {
    ui.horizontal_top(|ui| {
      // the tree: the projects, New Project, then Fusion's sample groups
      egui::Frame::new().stroke(Stroke::new(1.0, theme::BORDER)).show(ui, |ui| {
        ui.set_width(220.0);
        ui.set_height(TABLE_H);
        egui::ScrollArea::vertical().id_salt("open-tree").auto_shrink([false, false]).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
          for p in f.projects.clone() {
            let id = s(&p, "id");
            let on = f.sample.is_none() && f.project.as_deref() == Some(id.as_str());
            let (rect, r) = row(ui, on);
            row_text(ui, rect, 10.0, &s(&p, "name"), theme::TEXT);
            if r.clicked() {
              f.project = Some(id);
              f.sample = None;
              f.folder = None;
              f.selected = None;
              f.stale = true;
            }
          }
          if f.projects.is_empty() {
            let (rect, _) = row(ui, false);
            row_text(ui, rect, 10.0, "No projects yet", theme::MUTED);
          }
          match &mut f.new_project {
            Some((name, first)) => {
              let r = name_field(ui, name, first);
              if r.lost_focus() {
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !name.trim().is_empty() {
                  act = OpenAct::CreateProject(name.trim().to_string());
                }
                f.new_project = None;
              }
            }
            None => {
              let (rect, r) = row(ui, false);
              row_text(ui, rect, 10.0, "+  New Project", theme::ACCENT);
              if r.on_hover_text("Create a project").clicked() {
                f.new_project = Some((String::new(), true));
              }
            }
          }
          ui.add_space(10.0);
          ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new("Samples").strong());
          });
          ui.add_space(2.0);
          for (i, name) in SAMPLES.iter().enumerate() {
            let (rect, r) = row(ui, f.sample == Some(i));
            row_text(ui, rect, 22.0, name, theme::TEXT);
            if r.on_hover_text(SAMPLES_EMPTY).clicked() {
              f.sample = Some(i);
              f.selected = None;
              f.stale = true;
            }
          }
        });
      });
      ui.add_space(12.0);
      // the breadcrumb and the table
      ui.vertical(|ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| match f.sample {
          Some(i) => {
            ui.label(RichText::new(SAMPLES[i]).strong());
          }
          None => {
            let pname = f.projects.iter().find(|p| Some(s(p, "id")) == f.project).map(|p| s(p, "name"));
            if ui.link(RichText::new(pname.unwrap_or_default()).strong()).clicked() {
              act = OpenAct::Navigate(String::new());
            }
            for p in f.listing.get("path").and_then(Value::as_array).cloned().unwrap_or_default() {
              ui.label(RichText::new("›").color(theme::MUTED));
              if ui.link(s(&p, "name")).clicked() {
                act = OpenAct::Navigate(s(&p, "id"));
              }
            }
          }
        });
        ui.add_space(4.0);
        egui::Frame::new().stroke(Stroke::new(1.0, theme::BORDER)).show(ui, |ui| {
          ui.set_height(TABLE_H - 30.0);
          ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
          let w = ui.available_width();
          let (head, _) = ui.allocate_exact_size(vec2(w, 26.0), egui::Sense::hover());
          ui.painter().rect_filled(head, 0.0, Color32::from_gray(246));
          let small = egui::FontId::proportional(11.0);
          let hx = |x: f32| pos2(head.left() + x, head.center().y);
          ui.painter().text(hx(10.0), egui::Align2::LEFT_CENTER, "NAME", small.clone(), theme::GROUP_LABEL);
          ui.painter().text(hx(w - 170.0), egui::Align2::LEFT_CENTER, "LAST UPDATED", small, theme::GROUP_LABEL);
          ui.painter().hline(head.x_range(), head.bottom(), Stroke::new(1.0, theme::BORDER));
          egui::ScrollArea::vertical().id_salt("open-table").auto_shrink([false, false]).show(ui, |ui| {
            if f.sample.is_some() {
              ui.add_space(12.0);
              ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new(SAMPLES_EMPTY).color(theme::MUTED));
              });
              return;
            }
            let folders = f.listing.get("folders").and_then(Value::as_array).cloned().unwrap_or_default();
            let items = f.listing.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
            for d in &folders {
              let pick = Pick::Folder(s(d, "id"));
              let (rect, r) = row(ui, f.selected.as_ref() == Some(&pick));
              icons::draw(
                ui.painter(),
                egui::Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), Vec2::splat(16.0)),
                Icon::Folder,
                theme::ICON_INK,
              );
              row_text(ui, rect, 34.0, &s(d, "name"), theme::TEXT);
              if r.double_clicked() {
                act = OpenAct::Navigate(s(d, "id"));
              } else if r.clicked() {
                f.selected = Some(pick);
              }
            }
            for it in &items {
              let id = s(it, "id");
              let pick = Pick::Item(id.clone());
              let (rect, r) = row(ui, f.selected.as_ref() == Some(&pick));
              icons::draw_color(
                ui.painter(),
                egui::Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), Vec2::splat(16.0)),
                Icon::Component,
              );
              row_text(ui, rect, 34.0, &s(it, "name"), theme::TEXT);
              let when = it
                .pointer("/latest/created")
                .and_then(Value::as_u64)
                .map(|t| fox_engine::store::relative_time(t, now))
                .unwrap_or_default();
              row_text(ui, rect, w - 170.0, &when, theme::MUTED);
              if r.double_clicked() {
                act = OpenAct::OpenItem(id);
              } else if r.clicked() {
                f.selected = Some(pick);
              }
            }
            if folders.is_empty() && items.is_empty() {
              ui.add_space(12.0);
              ui.horizontal(|ui| {
                ui.add_space(10.0);
                let note = if f.project.is_none() {
                  "Save a design (Ctrl+S) to create your first project."
                } else {
                  "This folder is empty."
                };
                ui.label(RichText::new(note).color(theme::MUTED));
              });
            }
          });
        });
      });
    });
    let b = footer(ui, Some("Open from my computer..."), "Open", f.selected.is_some());
    if b.left {
      act = OpenAct::FromComputer;
    } else if b.primary {
      act = OpenAct::Open;
    } else if b.cancel {
      act = OpenAct::Close;
    }
  });
  if closed {
    act = OpenAct::Close;
  }
  if let OpenAct::Open = act {
    act = match f.selected.clone() {
      Some(Pick::Folder(id)) => OpenAct::Navigate(id),
      Some(Pick::Item(id)) => OpenAct::OpenItem(id),
      None => OpenAct::None,
    };
  }
  match act {
    OpenAct::None | OpenAct::Open => app.files.open = Some(f),
    OpenAct::Navigate(id) => {
      f.folder = Some(id).filter(|id| !id.is_empty());
      f.selected = None;
      f.stale = true;
      app.files.open = Some(f);
    }
    OpenAct::OpenItem(id) => app.open_hub_item(&id, None),
    OpenAct::FromComputer => app.open_from_computer(),
    OpenAct::Close => {}
    OpenAct::CreateProject(name) => {
      app.files.open = Some(f);
      create_project(app, &name);
    }
  }
}

/// The Open window's New Project: a project named `name`, picked in the tree.
pub(crate) fn create_project(app: &mut App, name: &str) {
  match app.s.execute("hub.project.create", &json!({"name": name})) {
    Ok(v) => {
      if let Some(f) = app.files.open.as_mut() {
        f.project = v.get("project").and_then(Value::as_str).map(str::to_string);
        f.sample = None;
        f.folder = None;
        f.selected = None;
        f.stale = true;
      }
      app.data.invalidate();
    }
    Err(e) => app.status = Some((e.0, true)),
  }
}

// ---------------------------------------------------------------- Export

/// One entry of the Export window's Type list: Fusion's label, and the FissionOxide format that
/// writes it (None: greyed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExportType {
  pub label: &'static str,
  pub format: Option<&'static str>,
  pub ext: &'static str,
  /// Fusion greys it too, pointing at "Need to export to IGES, SAT, DWG or DXF?".
  pub fusion_greys: bool,
}

const fn t(label: &'static str, format: Option<&'static str>, ext: &'static str, fusion_greys: bool) -> ExportType {
  ExportType { label, format, ext, fusion_greys }
}

/// The Type list: FissionOxide's archive (its counterpart of Fusion's default .f3d archive), then
/// Fusion's types in its order, greyed where FissionOxide can't write them, then the formats only
/// FissionOxide writes.
pub(crate) const EXPORT_TYPES: [ExportType; 19] = [
  t("FissionOxide Archive Files (*.foxz)", Some("foxz"), "foxz", false),
  t("Autodesk Fusion Archive Files (*.f3d)", None, "f3d", false),
  t("3MF Files (*.3mf)", Some("3mf"), "3mf", false),
  t("Autodesk Inventor 2025 Files (*.ipt)", None, "ipt", false),
  t("DWG Files (*.dwg)", None, "dwg", true),
  t("DXF Files (*.dxf)", None, "dxf", true),
  t("FBX Files (*.fbx)", None, "fbx", false),
  t("IGES Files (*.igs *.iges)", None, "igs", true),
  t("OBJ Files (*.obj)", Some("obj"), "obj", false),
  t("SAT Files (*.sat)", None, "sat", true),
  t("SketchUp Files (*.skp)", None, "skp", false),
  t("SMT Files (*.smt)", None, "smt", false),
  t("STEP Files (*.stp *.step)", Some("step"), "step", false),
  t("STL Files (*.stl)", Some("stl"), "stl", false),
  t("USD Files (*.usdz)", Some("usdz"), "usdz", false),
  t("FissionOxide Design Files (*.fox)", Some("fox"), "fox", false),
  t("OpenCASCADE BRep Files (*.brep)", Some("brep"), "brep", false),
  t("glTF Binary Files (*.glb)", Some("glb"), "glb", false),
  t("PLY Files (*.ply)", Some("ply"), "ply", false),
];

/// File ▸ Export...'s window.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportForm {
  pub name: String,
  /// Index into [`EXPORT_TYPES`].
  pub kind: usize,
  pub location: String,
  /// The name is selected the first time the window shows.
  pub first: bool,
}

impl ExportForm {
  /// The file the export writes.
  pub fn path(&self) -> PathBuf {
    let ext = EXPORT_TYPES.get(self.kind).map(|t| t.ext).unwrap_or("foxz");
    Path::new(&self.location).join(format!("{}.{ext}", self.name.trim()))
  }
}

/// The Downloads folder, written with forward slashes as Fusion shows it.
pub(crate) fn downloads() -> String {
  let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
  let dir = match home {
    Some(h) if h.join("Downloads").is_dir() => h.join("Downloads"),
    Some(h) => h,
    None => std::env::current_dir().unwrap_or_default(),
  };
  dir.display().to_string().replace('\\', "/")
}

/// File ▸ Export...: Fusion's Export window, named after the design.
pub fn open_export(app: &mut App) {
  app.files.export = Some(ExportForm { name: app.s.design.name.clone(), kind: 0, location: downloads(), first: true });
}

/// Writes the Export window's file. Returns whether it was written (the window then closes).
pub(crate) fn export_now(app: &mut App, form: &ExportForm) -> bool {
  if EXPORT_TYPES.get(form.kind).and_then(|t| t.format).is_none() {
    return false;
  }
  if !Path::new(&form.location).is_dir() {
    app.status = Some((format!("The folder {} doesn't exist", form.location), true));
    return false;
  }
  let path = form.path();
  if app.run("export", json!({"path": path})).is_none() {
    return false;
  }
  app.status = Some((format!("Exported {}", path.display()), false));
  true
}

const CANT_WRITE: &str = "FissionOxide can't write this format";
const ON_THE_WEB: &str =
  "Fusion exports this from the design's page on the web: see \"Need to export to IGES, SAT, DWG or DXF?\"";

fn export_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut f) = app.files.export.take() else { return };
  let mut go = false;
  let mut cancel = false;
  let mut other: Option<&'static str> = None;
  let (_, closed) = modal(ctx, "export", "Export", 480.0, |ui| {
    ui.label("Name:");
    name_field(ui, &mut f.name, &mut f.first);
    ui.add_space(4.0);
    ui.label("Type:");
    let current = EXPORT_TYPES.get(f.kind).map(|t| t.label).unwrap_or_default();
    egui::ComboBox::from_id_salt("export-type")
      .selected_text(current)
      .width(ui.available_width())
      .height(420.0)
      .show_ui(ui, |ui| {
        for (i, t) in EXPORT_TYPES.iter().enumerate() {
          let enabled = t.format.is_some();
          let r = ui.add_enabled(enabled, egui::Button::selectable(f.kind == i, t.label));
          if r.clicked() {
            f.kind = i;
          }
          r.on_disabled_hover_text(if t.fusion_greys { ON_THE_WEB } else { CANT_WRITE });
        }
      });
    ui.add_space(4.0);
    ui.label("Location:");
    ui.horizontal(|ui| {
      ui.add(
        egui::TextEdit::singleline(&mut f.location).desired_width(ui.available_width() - 40.0).margin(vec2(6.0, 5.0)),
      );
      if ui.add(white_button("...").min_size(vec2(32.0, 26.0))).on_hover_text("Choose the folder").clicked()
        && let Some(p) = rfd::FileDialog::new().set_title("Location").set_directory(&f.location).pick_folder()
      {
        f.location = p.display().to_string().replace('\\', "/");
      }
    });
    ui.add_space(6.0);
    let link = ui.link("Need to export to IGES, SAT, DWG or DXF?");
    egui::Popup::menu(&link).show(|ui| {
      if ui.button("IGES Files (*.igs *.iges)...").on_hover_text("Write the bodies as IGES").clicked() {
        other = Some("iges");
      }
      if ui.button("DXF Files (*.dxf)...").on_hover_text("Write the active (or last) sketch as DXF").clicked() {
        other = Some("dxf");
      }
      ui.add_enabled(false, egui::Button::new("SAT Files (*.sat)")).on_disabled_hover_text(CANT_WRITE);
      ui.add_enabled(false, egui::Button::new("DWG Files (*.dwg)")).on_disabled_hover_text(CANT_WRITE);
    });
    let enabled = !f.name.trim().is_empty() && EXPORT_TYPES.get(f.kind).is_some_and(|t| t.format.is_some());
    let b = footer(ui, None, "Export", enabled);
    go = b.primary;
    cancel = b.cancel;
  });
  if closed || cancel {
    return;
  }
  if let Some(id) = other {
    app.export(id);
    return;
  }
  if go && export_now(app, &f) {
    return;
  }
  app.files.export = Some(f);
}
