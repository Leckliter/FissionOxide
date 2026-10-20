//! The File menu, as Fusion's (less its cloud-only Share File Link and View Details on Web):
//! New ▸, Add To Assembly, Open, Open Recovered Documents, Upload | Save, Save As, Save As Latest,
//! Export | Create Version | 3D Print, Capture Image | View ▸ (the toolbar and panels), plus what
//! FissionOxide adds (Open Recent, Save to File, Insert into Design, Preferences).
//!
//! Also here: Capture Image (the viewport as a PNG at a chosen size), the auto-recovery copies
//! Open Recovered Documents lists, and the Text Commands panel View ▸ shows.

use std::path::{Path, PathBuf};

use egui::RichText;
use serde_json::{Value, json};

use crate::dialogs::Kind;
use crate::panels::Cmd;
use crate::{App, theme};

/// What a File menu row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileCmd {
  /// ▸ the new document types.
  New,
  AddToAssembly,
  Open,
  /// ▸ designs saved lately.
  OpenRecent,
  /// ▸ the auto-recovery copies.
  OpenRecovered,
  Upload,
  Save,
  SaveAs,
  SaveAsLatest,
  SaveToFile,
  /// ▸ the export formats.
  Export,
  InsertIntoDesign,
  CreateVersion,
  Print3d,
  CaptureImage,
  /// ▸ the toolbar and panel switches.
  View,
  Preferences,
}

impl FileCmd {
  /// Whether the row opens a sub-menu.
  pub(crate) fn submenu(self) -> bool {
    matches!(self, FileCmd::New | FileCmd::OpenRecent | FileCmd::OpenRecovered | FileCmd::Export | FileCmd::View)
  }
}

/// One File menu row: label, shortcut key and command.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileItem {
  pub label: String,
  pub key: Option<&'static str>,
  pub cmd: FileCmd,
  pub enabled: bool,
}

fn item(label: impl Into<String>, key: Option<&'static str>, cmd: FileCmd) -> Option<FileItem> {
  Some(FileItem { label: label.into(), key, cmd, enabled: true })
}

/// The File menu's rows (`None` is a separator), as Fusion orders them.
pub(crate) fn items(app: &App) -> Vec<Option<FileItem>> {
  let recovered = recovered(app).len();
  let linked = app.s.hub.is_some();
  vec![
    item("New", None, FileCmd::New),
    item("Add To Assembly", None, FileCmd::AddToAssembly),
    item("Open...", Some("Ctrl+O"), FileCmd::Open),
    item("Open Recent", None, FileCmd::OpenRecent),
    Some(FileItem {
      label: format!("Open Recovered Documents ({recovered})"),
      key: None,
      cmd: FileCmd::OpenRecovered,
      enabled: recovered > 0,
    }),
    item("Upload...", None, FileCmd::Upload),
    None,
    item("Save", Some("Ctrl+S"), FileCmd::Save),
    item("Save As", Some("Ctrl+Shift+S"), FileCmd::SaveAs),
    Some(FileItem { label: "Save As Latest".into(), key: None, cmd: FileCmd::SaveAsLatest, enabled: linked }),
    item("Save to File...", None, FileCmd::SaveToFile),
    item("Export...", None, FileCmd::Export),
    item("Insert into Design...", None, FileCmd::InsertIntoDesign),
    None,
    item("Create Version", None, FileCmd::CreateVersion),
    None,
    item("3D Print", None, FileCmd::Print3d),
    item("Capture Image...", None, FileCmd::CaptureImage),
    None,
    item("View", None, FileCmd::View),
    None,
    item("Preferences...", None, FileCmd::Preferences),
  ]
}

/// New ▸: the document types FissionOxide makes.
pub(crate) const NEW_ITEMS: [(&str, Option<&str>); 2] =
  [("New Design", Some("Ctrl+N")), ("New Drawing from Design", None)];

/// A panel View ▸ shows or hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewSwitch {
  Toolbar,
  Browser,
  Comments,
  TextCommands,
  Timeline,
  ViewCube,
  NavigationBar,
  DataPanel,
}

/// View ▸: each panel's switch, as Fusion lists them, and whether it shows now.
pub(crate) fn view_items(app: &App) -> Vec<(&'static str, bool, ViewSwitch)> {
  vec![
    ("Show Toolbar", app.show_toolbar, ViewSwitch::Toolbar),
    ("Show Browser", app.browser_open, ViewSwitch::Browser),
    ("Show Comments", app.show_comments, ViewSwitch::Comments),
    ("Show Text Commands", app.text_commands.open, ViewSwitch::TextCommands),
    ("Show Timeline", app.show_timeline, ViewSwitch::Timeline),
    ("Show ViewCube", app.show_viewcube, ViewSwitch::ViewCube),
    ("Show Navigation Bar", app.show_navbar, ViewSwitch::NavigationBar),
    ("Show Data Panel", app.data.open, ViewSwitch::DataPanel),
  ]
}

/// Shows the panel if it's hidden, hides it if it shows.
pub(crate) fn toggle(app: &mut App, p: ViewSwitch) {
  match p {
    ViewSwitch::Toolbar => app.show_toolbar ^= true,
    ViewSwitch::Browser => app.browser_open ^= true,
    ViewSwitch::Comments => app.show_comments ^= true,
    ViewSwitch::TextCommands => app.text_commands.open ^= true,
    ViewSwitch::Timeline => app.show_timeline ^= true,
    ViewSwitch::ViewCube => app.show_viewcube ^= true,
    ViewSwitch::NavigationBar => app.show_navbar ^= true,
    ViewSwitch::DataPanel => app.data.toggle(),
  }
}

/// Runs a File menu row that isn't a sub-menu.
pub(crate) fn run(app: &mut App, cmd: FileCmd) {
  match cmd {
    FileCmd::AddToAssembly => add_to_assembly(app),
    FileCmd::Open => app.open_dialog(),
    FileCmd::Upload => upload(app),
    FileCmd::Save => app.save(false),
    FileCmd::SaveAs => app.save(true),
    FileCmd::SaveAsLatest => save_as_latest(app),
    FileCmd::SaveToFile => app.save_file(true),
    FileCmd::InsertIntoDesign => app.import(),
    FileCmd::CreateVersion => create_version(app),
    FileCmd::Print3d => Cmd::Print3d.run(app),
    FileCmd::CaptureImage => app.capture = Some(Capture::new(app)),
    FileCmd::Preferences => crate::preferences::open(app),
    FileCmd::New | FileCmd::OpenRecent | FileCmd::OpenRecovered | FileCmd::Export | FileCmd::View => {}
  }
}

/// The File menu's drop-down.
pub(crate) fn menu(app: &mut App, ui: &mut egui::Ui) {
  ui.set_min_width(250.0);
  for it in items(app) {
    let Some(it) = it else {
      ui.separator();
      continue;
    };
    if it.cmd.submenu() {
      ui.add_enabled_ui(it.enabled, |ui| {
        ui.menu_button(&it.label, |ui| submenu(app, ui, it.cmd));
      });
      continue;
    }
    let mut b = egui::Button::new(&it.label);
    if let Some(k) = it.key {
      b = b.shortcut_text(k);
    }
    let r = ui.add_enabled(it.enabled, b);
    let r = match it.cmd {
      FileCmd::Open => r.on_hover_text("Designs, or STEP, IGES, STL, OBJ, 3MF, PLY, DXF … as a new design"),
      FileCmd::AddToAssembly => r.on_hover_text("A new design with this one inserted as a component"),
      FileCmd::Upload => r.on_hover_text("Copy files from this computer into the Data Panel"),
      FileCmd::SaveAsLatest => {
        r.on_hover_text("Save this version as the newest one").on_disabled_hover_text("Save the design first")
      }
      FileCmd::SaveToFile => r.on_hover_text("Write a .fox or .foxz file on this computer"),
      FileCmd::InsertIntoDesign => r.on_hover_text("Add the bodies (or DXF sketch) from a file"),
      FileCmd::CreateVersion => r.on_hover_text("Save the design as a new version in the Data Panel"),
      FileCmd::CaptureImage => r.on_hover_text("Save the canvas as a PNG picture"),
      _ => r,
    };
    if r.clicked() {
      ui.close();
      run(app, it.cmd);
    }
  }
}

fn submenu(app: &mut App, ui: &mut egui::Ui, cmd: FileCmd) {
  match cmd {
    FileCmd::New => {
      for (label, key) in NEW_ITEMS {
        let mut b = egui::Button::new(label);
        if let Some(k) = key {
          b = b.shortcut_text(k);
        }
        if ui.add(b).clicked() {
          ui.close();
          if label == "New Design" {
            app.new_design();
          } else {
            app.start(Kind::NewDrawing);
          }
        }
      }
    }
    FileCmd::OpenRecent => {
      let recent =
        app.s.execute("hub.recent", &json!({"limit": 12})).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
      if recent.is_empty() {
        ui.label(RichText::new("Nothing saved yet").color(theme::MUTED));
      }
      for it in recent {
        let id = it["id"].as_str().unwrap_or_default().to_string();
        let label =
          format!("{}  v{}", it["name"].as_str().unwrap_or_default(), it["latest"]["number"].as_u64().unwrap_or(0));
        if ui.button(label).clicked() {
          ui.close();
          app.open_hub_item(&id, None);
        }
      }
    }
    FileCmd::OpenRecovered => {
      for r in recovered(app) {
        if ui.button(&r.name).on_hover_text(r.path.display().to_string()).clicked() {
          ui.close();
          open_recovered(app, &r.path);
        }
      }
    }
    FileCmd::Export => {
      for f in fox_engine::exchange::FORMATS.iter().filter(|f| f.export) {
        let label = format!("{} (*.{})", f.name, f.extensions.join(", *."));
        if ui.button(label).on_hover_text(f.note).clicked() {
          ui.close();
          app.export(f.id);
        }
      }
    }
    FileCmd::View => {
      for (label, mut on, p) in view_items(app) {
        if ui.checkbox(&mut on, label).clicked() {
          toggle(app, p);
        }
      }
    }
    _ => {}
  }
}

// ---------------------------------------------------------------- Data Panel commands

/// Add To Assembly: a new design with this one (as saved in the Data Panel) inserted as a linked
/// component.
fn add_to_assembly(app: &mut App) {
  let Some(item) = app.s.hub.as_ref().map(|h| h.item.clone()) else {
    app.status = Some(("Save the design to the Data Panel first, then Add To Assembly".into(), false));
    return;
  };
  if app.s.dirty {
    app.status = Some(("Save the design's changes first: the assembly inserts the saved version".into(), false));
    return;
  }
  app.new_design();
  crate::data_panel::insert_component(app, &item, None);
}

/// Upload: files from this computer into the Data Panel's open project and folder.
fn upload(app: &mut App) {
  let Some(files) = rfd::FileDialog::new().set_title("Upload").pick_files() else { return };
  let (project, folder) = app.data.location();
  let mut n = 0;
  for p in files {
    if app.run("hub.upload", json!({"path": p, "project": project, "folder": folder})).is_some() {
      n += 1;
    }
  }
  app.data.invalidate();
  if n > 0 {
    app.status = Some((format!("Uploaded {n} file{}", if n == 1 { "" } else { "s" }), false));
  }
}

/// Save As Latest: the open version (an older one, say) becomes the design's newest version.
fn save_as_latest(app: &mut App) {
  if app.s.hub.is_none() {
    app.status = Some(("Save the design to the Data Panel first".into(), false));
    return;
  }
  if let Some(r) = app.run("hub.save", json!({"explicit": true})) {
    app.status = Some((format!("Saved as the latest version, v{}", r["version"]), false));
    app.data.invalidate();
  }
}

/// Create Version: a new version in the Data Panel now (the first one asks where, as Save does).
fn create_version(app: &mut App) {
  if app.s.hub.is_none() {
    app.save(false);
    return;
  }
  if let Some(r) = app.run("hub.save", json!({"explicit": true})) {
    app.status = Some((format!("Created version v{}", r["version"]), false));
    app.data.invalidate();
  }
}

// ---------------------------------------------------------------- Capture Image

/// Capture Image's sizes: (label, width, height); 0 × 0 is the canvas's own size.
pub(crate) const CAPTURE_SIZES: [(&str, u32, u32); 5] = [
  ("Use current document window size", 0, 0),
  ("1280 x 720 (HD)", 1280, 720),
  ("1920 x 1080 (Full HD)", 1920, 1080),
  ("3840 x 2160 (4K)", 3840, 2160),
  ("Custom", 0, 0),
];

/// The open Capture Image window.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Capture {
  /// Which of [`CAPTURE_SIZES`].
  pub size: usize,
  pub width: u32,
  pub height: u32,
}

impl Capture {
  fn new(app: &App) -> Capture {
    let (width, height) = canvas_size(app);
    Capture { size: 0, width, height }
  }

  /// The picture's size in pixels.
  pub(crate) fn pixels(&self, app: &App) -> (u32, u32) {
    match CAPTURE_SIZES.get(self.size) {
      Some((_, 0, 0)) if self.size == 0 => canvas_size(app),
      Some((_, w, h)) if *w > 0 => (*w, *h),
      _ => (self.width.clamp(16, 8192), self.height.clamp(16, 8192)),
    }
  }
}

fn canvas_size(app: &App) -> (u32, u32) {
  match &app.frame {
    Some(f) => (f.width, f.height),
    None => (app.canvas.width().round().max(16.0) as u32, app.canvas.height().round().max(16.0) as u32),
  }
}

/// The canvas from the current view as a PNG, `w` × `h` pixels.
pub(crate) fn capture_png(app: &App, w: u32, h: u32) -> Result<Vec<u8>, String> {
  let vis = app.objects;
  let opts = fox_engine::scene::ViewOptions {
    hide_edges: !app.visual_style.edges(),
    hide_grid: !app.show_grid,
    hide_origin: !vis.origins,
    hide_sketches: !vis.sketches,
    hide_construction: !vis.construction,
    only_bodies: if vis.bodies { app.ctx_menu.isolate.clone() } else { Some(Vec::new()) },
    opacity: app.ctx_menu.opacity.clone(),
    ..Default::default()
  };
  let (mut scene, _) = fox_engine::scene::SceneBuilder::default().build(&app.s, &opts);
  scene.style = app.visual_style;
  app.effects.apply(&mut scene);
  fox_engine::render::render(&scene, &app.cam, w, h).to_png()
}

/// The Capture Image window.
pub fn capture_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut c) = app.capture.clone() else { return };
  let mut open = true;
  let mut done = None;
  egui::Window::new("Image Options")
    .open(&mut open)
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
    .show(ctx, |ui| {
      egui::Grid::new("capture-image").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Image Resolution");
        egui::ComboBox::from_id_salt("capture-size").selected_text(CAPTURE_SIZES[c.size].0).show_ui(ui, |ui| {
          for (i, (label, _, _)) in CAPTURE_SIZES.iter().enumerate() {
            ui.selectable_value(&mut c.size, i, *label);
          }
        });
        ui.end_row();
        let custom = c.size == CAPTURE_SIZES.len() - 1;
        let (w, h) = c.pixels(app);
        if !custom {
          c.width = w;
          c.height = h;
        }
        ui.label("Width");
        ui.add_enabled(custom, egui::DragValue::new(&mut c.width).range(16..=8192).suffix(" px"));
        ui.end_row();
        ui.label("Height");
        ui.add_enabled(custom, egui::DragValue::new(&mut c.height).range(16..=8192).suffix(" px"));
        ui.end_row();
      });
      ui.separator();
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.button("Cancel").clicked() {
          done = Some(false);
        }
        if ui.button("OK").clicked() {
          done = Some(true);
        }
      });
    });
  app.capture = Some(c.clone());
  if !open || done.is_some() {
    app.capture = None;
  }
  if done == Some(true) {
    let (w, h) = c.pixels(app);
    let Some(path) = rfd::FileDialog::new()
      .add_filter("PNG image", &["png"])
      .set_file_name(format!("{}.png", app.s.design.name))
      .save_file()
    else {
      return;
    };
    app.status =
      Some(match capture_png(app, w, h).and_then(|png| std::fs::write(&path, png).map_err(|e| e.to_string())) {
        Ok(()) => (format!("Saved a {w} x {h} image to {}", path.display()), false),
        Err(e) => (format!("Couldn't capture the image: {e}"), true),
      });
  }
}

// ---------------------------------------------------------------- auto-recovery

/// How often a design with unsaved changes is copied for recovery.
pub(crate) const RECOVERY_SECS: f64 = 120.0;

/// This session's auto-recovery copy: where it goes and when it was last written.
#[derive(Debug, Default)]
pub(crate) struct Recovery {
  file: Option<PathBuf>,
  last: Option<f64>,
}

/// A design left by a session that didn't close (Open Recovered Documents).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Recovered {
  pub path: PathBuf,
  pub name: String,
}

/// The folder recovery copies go in, inside the data hub.
pub(crate) fn recovery_dir(app: &App) -> PathBuf {
  app.hub_root().join("recovery")
}

impl Recovery {
  /// This session's copy (named for the process and the moment it was first needed).
  fn file(&mut self, dir: &Path) -> PathBuf {
    self
      .file
      .get_or_insert_with(|| {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        dir.join(format!("recovery-{}-{t}.fox", std::process::id()))
      })
      .clone()
  }

  /// Removes this session's copy (the design was saved, or the app closes normally).
  pub(crate) fn clear(&mut self) {
    if let Some(f) = &self.file
      && f.exists()
    {
      let _ = std::fs::remove_file(f);
    }
    self.last = None;
  }
}

/// Every frame: copies a design with unsaved changes every [`RECOVERY_SECS`], and drops the copy
/// once the changes are saved. Returns whether it wrote one.
pub(crate) fn autosave(app: &mut App, now: f64) -> bool {
  if !app.s.dirty {
    if app.recovery.last.is_some() {
      app.recovery.clear();
    }
    return false;
  }
  // a first change starts the clock
  let Some(since) = app.recovery.last else {
    app.recovery.last = Some(now);
    return false;
  };
  if now - since < RECOVERY_SECS || app.s.design.features.is_empty() {
    return false;
  }
  app.recovery.last = Some(now);
  let dir = recovery_dir(app);
  if let Err(e) = std::fs::create_dir_all(&dir) {
    log::warn!("couldn't make the recovery folder: {e}");
    return false;
  }
  let path = app.recovery.file(&dir);
  match fox_engine::files::save_design(&app.s, &path) {
    Ok(()) => true,
    Err(e) => {
      log::warn!("couldn't write the recovery copy: {}", e.0);
      false
    }
  }
}

/// The recovery copies other sessions left, newest first.
pub(crate) fn recovered(app: &App) -> Vec<Recovered> {
  let Ok(dir) = std::fs::read_dir(recovery_dir(app)) else { return Vec::new() };
  let mine = app.recovery.file.clone();
  let mut out: Vec<(std::time::SystemTime, Recovered)> = dir
    .flatten()
    .map(|e| e.path())
    .filter(|p| p.extension().is_some_and(|x| x == "fox") && Some(p) != mine.as_ref())
    .map(|path| {
      let when = std::fs::metadata(&path).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
      let name = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["name"].as_str().map(str::to_string))
        .unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
      (when, Recovered { path, name })
    })
    .collect();
  out.sort_by_key(|r| std::cmp::Reverse(r.0));
  out.into_iter().map(|(_, r)| r).collect()
}

/// Opens a recovery copy as an unsaved design; the copy becomes this session's, so it stays
/// until the design is saved.
pub(crate) fn open_recovered(app: &mut App, path: &Path) {
  app.open_path(path);
  if app.s.path.as_deref() != Some(path) {
    return;
  }
  app.s.path = None;
  app.s.dirty = true;
  let dir = recovery_dir(app);
  app.recovery.clear();
  let mine = app.recovery.file(&dir);
  if std::fs::rename(path, &mine).is_ok() {
    app.recovery.last = Some(crate::now());
  }
  app.status = Some((format!("Recovered {} (not saved yet)", app.s.design.name), false));
}

// ---------------------------------------------------------------- Text Commands

/// The Text Commands panel: type an engine command and its JSON parameters
/// (`box {"width": 10}`), see the result.
#[derive(Debug, Default)]
pub(crate) struct TextCommands {
  pub open: bool,
  pub input: String,
  /// The last commands and their results, oldest first.
  pub log: Vec<String>,
}

/// Runs one typed line: a command name, then optional JSON parameters.
pub(crate) fn run_text_command(app: &mut App, line: &str) {
  let line = line.trim();
  if line.is_empty() {
    return;
  }
  let (cmd, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
  let params = if rest.trim().is_empty() { Ok(json!({})) } else { serde_json::from_str::<Value>(rest.trim()) };
  let out = match params {
    Err(e) => format!("bad parameters: {e}"),
    Ok(p) => match app.s.execute(cmd, &p) {
      Ok(v) => v.to_string(),
      Err(e) => format!("error: {}", e.0),
    },
  };
  let log = &mut app.text_commands.log;
  log.push(format!("> {line}"));
  log.push(out);
  let extra = log.len().saturating_sub(200);
  log.drain(..extra);
}

/// The Text Commands panel, above the timeline.
pub fn text_commands(app: &mut App, ui: &mut egui::Ui) {
  if !app.text_commands.open {
    return;
  }
  egui::Panel::bottom("text_commands")
    .resizable(true)
    .default_size(120.0)
    .frame(egui::Frame::NONE.fill(theme::PANEL).inner_margin(egui::Margin::symmetric(8, 4)))
    .show(ui, |ui| {
      ui.horizontal(|ui| {
        ui.label(RichText::new("TEXT COMMANDS").small().strong().color(theme::GROUP_LABEL));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          if ui.small_button("✕").on_hover_text("Hide Text Commands").clicked() {
            app.text_commands.open = false;
          }
        });
      });
      let h = (ui.available_height() - 28.0).max(20.0);
      egui::ScrollArea::vertical().max_height(h).stick_to_bottom(true).auto_shrink([false, true]).show(ui, |ui| {
        for l in &app.text_commands.log {
          ui.label(RichText::new(l).monospace().small());
        }
      });
      let r = ui.add(
        egui::TextEdit::singleline(&mut app.text_commands.input)
          .hint_text("command {\"param\": value}")
          .desired_width(f32::INFINITY)
          .font(egui::TextStyle::Monospace),
      );
      if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        let line = std::mem::take(&mut app.text_commands.input);
        run_text_command(app, &line);
        r.request_focus();
      }
    });
}

#[cfg(test)]
mod tests {
  use fox_engine::Session;

  use super::*;

  fn rows(app: &App) -> Vec<(String, Option<&'static str>)> {
    items(app).into_iter().map(|i| i.map(|i| (i.label, i.key)).unwrap_or(("-".into(), None))).collect()
  }

  #[test]
  fn the_file_menu_is_fusions_with_its_keys() {
    let app = App::with_session(Session::default());
    let labels: Vec<String> = rows(&app).into_iter().map(|(l, _)| l).collect();
    assert_eq!(
      labels,
      [
        "New",
        "Add To Assembly",
        "Open...",
        "Open Recent",
        "Open Recovered Documents (0)",
        "Upload...",
        "-",
        "Save",
        "Save As",
        "Save As Latest",
        "Save to File...",
        "Export...",
        "Insert into Design...",
        "-",
        "Create Version",
        "-",
        "3D Print",
        "Capture Image...",
        "-",
        "View",
        "-",
        "Preferences...",
      ]
    );
    let key = |label: &str| rows(&app).into_iter().find(|(l, _)| l == label).and_then(|(_, k)| k);
    assert_eq!(key("Open..."), Some("Ctrl+O"));
    assert_eq!(key("Save"), Some("Ctrl+S"));
    assert_eq!(key("Save As"), Some("Ctrl+Shift+S"));
    assert_eq!(key("Upload..."), None);
    assert_eq!(NEW_ITEMS[0], ("New Design", Some("Ctrl+N")));
    // cloud-only commands stay out
    for gone in ["Share File Link", "View Details on Web"] {
      assert!(!labels.iter().any(|l| l == gone), "{gone}");
    }
    // nothing to recover, no hub version: those rows are greyed
    let off: Vec<String> = items(&app).into_iter().flatten().filter(|i| !i.enabled).map(|i| i.label).collect();
    assert_eq!(off, ["Open Recovered Documents (0)", "Save As Latest"]);
  }

  #[test]
  fn view_toggles_each_panel() {
    let mut app = App::with_session(Session::default());
    let labels: Vec<&str> = view_items(&app).iter().map(|v| v.0).collect();
    assert_eq!(
      labels,
      [
        "Show Toolbar",
        "Show Browser",
        "Show Comments",
        "Show Text Commands",
        "Show Timeline",
        "Show ViewCube",
        "Show Navigation Bar",
        "Show Data Panel",
      ]
    );
    let before: Vec<bool> = view_items(&app).iter().map(|v| v.1).collect();
    assert_eq!(before, [true, true, true, false, true, true, true, false]);
    for (_, _, p) in view_items(&app) {
      toggle(&mut app, p);
    }
    let after: Vec<bool> = view_items(&app).iter().map(|v| v.1).collect();
    assert_eq!(after, before.iter().map(|b| !b).collect::<Vec<_>>(), "each switch flips its panel");
  }

  #[test]
  fn capture_image_renders_the_canvas_at_the_chosen_size() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    let png = capture_png(&app, 320, 200).unwrap();
    assert_eq!(&png[1..4], b"PNG");
    // the IHDR chunk: width and height, big-endian
    let dim = |at: usize| u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]);
    assert_eq!((dim(16), dim(20)), (320, 200));
    // the sizes the window offers
    run(&mut app, FileCmd::CaptureImage);
    let mut c = app.capture.clone().unwrap();
    c.size = 2;
    assert_eq!(c.pixels(&app), (1920, 1080));
    c.size = CAPTURE_SIZES.len() - 1;
    (c.width, c.height) = (640, 9999);
    assert_eq!(c.pixels(&app), (640, 8192), "a custom size is kept sensible");
    c.size = 0;
    assert_eq!(c.pixels(&app), (1200, 700), "the canvas's own size");
  }

  #[test]
  fn unsaved_changes_are_copied_for_recovery_and_listed_by_another_session() {
    let mut app = App::with_session(Session::default());
    assert!(!autosave(&mut app, 0.0), "nothing to recover");
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    app.s.design.name = "Bracket".into();
    assert!(!autosave(&mut app, 1.0), "the first change starts the clock");
    assert!(!autosave(&mut app, 1.0 + RECOVERY_SECS / 2.0));
    assert!(autosave(&mut app, 1.0 + RECOVERY_SECS), "written once the interval passes");
    let mine = app.recovery.file.clone().unwrap();
    assert!(mine.exists() && mine.starts_with(recovery_dir(&app)));
    assert!(recovered(&app).is_empty(), "a session doesn't offer its own copy");

    // another session on the same hub (this one crashed): it lists the copy by its design's name
    let mut other = App::with_session(Session::default());
    other.s.store_root = app.s.store_root.clone();
    let list = recovered(&other);
    assert_eq!(list, [Recovered { path: mine.clone(), name: "Bracket".into() }]);
    assert_eq!(
      items(&other).into_iter().flatten().find(|i| i.cmd == FileCmd::OpenRecovered).unwrap().label,
      "Open Recovered Documents (1)"
    );
    open_recovered(&mut other, &mine);
    assert_eq!(other.s.design.name, "Bracket");
    assert!(other.s.dirty && other.s.path.is_none(), "recovered, not saved");
    assert_eq!(other.s.design.features.len(), 1);
    assert!(recovered(&other).is_empty(), "the copy is now that session's own");

    // saved: the copy goes
    other.s.dirty = false;
    let theirs = other.recovery.file.clone().unwrap();
    autosave(&mut other, 1000.0);
    assert!(!theirs.exists());
  }

  #[test]
  fn text_commands_run_engine_commands() {
    let mut app = App::with_session(Session::default());
    run_text_command(&mut app, r#"box {"width": 10, "depth": 10, "height": 10}"#);
    assert_eq!(app.s.model.bodies.len(), 1);
    run_text_command(&mut app, "box {oops");
    assert!(app.text_commands.log.last().unwrap().starts_with("bad parameters"));
    assert_eq!(app.text_commands.log[0], r#"> box {"width": 10, "depth": 10, "height": 10}"#);
  }
}
