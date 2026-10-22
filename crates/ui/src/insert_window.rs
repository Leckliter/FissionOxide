//! Fusion's Insert window, which INSERT > Decal, Canvas, Insert SVG and Insert Mesh open: the
//! PROJECT list at the left, the picked folder's files (NAME, LAST UPDATED) at the right, "Insert
//! from my computer..." ("Select from my computer..." for a mesh), Cancel and Insert (Select),
//! greyed until a file is picked. Insert Derive opens the same window as "Select Source", with
//! designs only and no computer button. INSERT > Insert DXF opens the docked INSERT DXF dialog
//! instead: Plane/Sketch, Select DXF File and Insert Mode.
//!
//! A hub file is written to a scratch file first, then inserted as a file from this computer.

use std::path::{Path, PathBuf};

use egui::{Color32, RichText, Stroke, Vec2, pos2, vec2};
use serde_json::{Value, json};

use crate::file_dialogs::{footer, modal, row, row_text};
use crate::icons::{self, Icon};
use crate::{App, panels, theme};

/// What the window inserts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
  Decal,
  Canvas,
  Svg,
  Mesh,
  Derive,
}

impl Target {
  /// The window's title.
  pub fn title(self) -> &'static str {
    if self == Target::Derive { "Select Source" } else { "Insert" }
  }

  /// The blue button.
  pub fn primary(self) -> &'static str {
    match self {
      Target::Mesh | Target::Derive => "Select",
      _ => "Insert",
    }
  }

  /// The white button at the left (none for Derive).
  pub fn computer_button(self) -> Option<&'static str> {
    match self {
      Target::Mesh => Some("Select from my computer..."),
      Target::Derive => None,
      _ => Some("Insert from my computer..."),
    }
  }

  /// The file types it takes (lower case, no dot); empty: FissionOxide designs.
  pub fn extensions(self) -> &'static [&'static str] {
    match self {
      Target::Decal | Target::Canvas => &crate::trace::EXTENSIONS,
      Target::Svg => &["svg"],
      Target::Mesh => &["stl", "obj", "3mf", "ply"],
      Target::Derive => &[],
    }
  }

  /// Whether a hub item (as `hub.list` gives it) can be picked: a design for Derive, else a file
  /// of a type the entry takes, or a design translated from one (an uploaded SVG or mesh).
  pub fn takes(self, item: &Value) -> bool {
    let kind = s(item, "kind");
    match self {
      Target::Derive => kind == "design" && original_ext(item).is_none(),
      _ => {
        let ext = original_ext(item).unwrap_or_else(|| s(item, "ext").to_ascii_lowercase());
        self.extensions().contains(&ext.as_str())
      }
    }
  }

  fn filter_name(self) -> &'static str {
    match self {
      Target::Decal | Target::Canvas => "Images",
      Target::Svg => "SVG files",
      Target::Mesh => "Mesh files",
      Target::Derive => "Designs",
    }
  }
}

/// The toolbar's icon, name and tooltip for an Insert window entry.
pub(crate) fn info(t: Target) -> (Icon, &'static str, &'static str) {
  match t {
    Target::Decal => (Icon::Decal, "Decal", "Place an image on a face of the design"),
    Target::Canvas => (Icon::Canvas, "Canvas", "Place an image on a plane to model over"),
    Target::Svg => (Icon::InsertSvg, "Insert SVG", "Insert an SVG file into a sketch"),
    Target::Mesh => (Icon::InsertMesh, "Insert Mesh", "Insert a mesh file (STL, OBJ, 3MF, PLY) as a mesh body"),
    Target::Derive => (Icon::Derive, "Insert Derive", "Bring geometry in from another design, linked to it"),
  }
}

/// The open window: the target, the project and folder shown, the file picked.
#[derive(Debug)]
pub struct InsertWindow {
  pub target: Target,
  pub project: Option<String>,
  pub folder: Option<String>,
  pub selected: Option<String>,
  pub projects: Vec<Value>,
  pub listing: Value,
  stale: bool,
}

/// The docked INSERT DXF dialog.
#[derive(Debug, Default)]
pub struct DxfDialog {
  pub file: Option<PathBuf>,
  /// Index into [`INSERT_MODES`] (One Sketch Per Layer is greyed).
  pub mode: usize,
}

/// INSERT DXF's Insert Mode list, as Fusion's.
pub const INSERT_MODES: [&str; 2] = ["Single Sketch", "One Sketch Per Layer"];

/// The Insert window and the INSERT DXF dialog, while open.
#[derive(Debug, Default)]
pub struct Inserts {
  pub window: Option<InsertWindow>,
  pub dxf: Option<DxfDialog>,
}

impl Inserts {
  pub(crate) fn modal_open(&self) -> bool {
    self.window.is_some()
  }
}

/// The extension of the file a translated upload came from (its latest version's original).
fn original_ext(item: &Value) -> Option<String> {
  let name = item.pointer("/latest/original").and_then(Value::as_str)?;
  Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase())
}

fn s(v: &Value, k: &str) -> String {
  v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// Opens the Insert window for `target` on the folder the data panel shows.
pub(crate) fn open(app: &mut App, target: Target) {
  app.cancel_dialog();
  let (project, folder) = app.data.location();
  app.inserts.window = Some(InsertWindow {
    target,
    project,
    folder,
    selected: None,
    projects: Vec::new(),
    listing: Value::Null,
    stale: true,
  });
  refresh(app);
}

/// Re-reads the projects and the folder's listing when they are stale.
fn refresh(app: &mut App) {
  if !app.inserts.window.as_ref().is_some_and(|w| w.stale) {
    return;
  }
  let projects = app.s.execute("hub.projects", &json!({})).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
  let Some(w) = app.inserts.window.as_mut() else { return };
  w.stale = false;
  if w.project.as_ref().is_none_or(|p| !projects.iter().any(|x| s(x, "id") == *p)) {
    let first = projects.iter().find(|p| s(p, "name") == "Default Project").or(projects.first());
    w.project = first.map(|p| s(p, "id"));
    w.folder = None;
  }
  w.projects = projects;
  let listing = match w.project.clone() {
    Some(p) => {
      let folder = w.folder.clone();
      app.s.execute("hub.list", &json!({"project": p, "folder": folder})).unwrap_or(Value::Null)
    }
    None => Value::Null,
  };
  if let Some(w) = app.inserts.window.as_mut() {
    w.listing = listing;
  }
}

enum Act {
  None,
  Close,
  Navigate(String),
  Pick(String),
  FromComputer,
}

/// The Insert window and the INSERT DXF dialog.
pub(crate) fn windows(app: &mut App, ctx: &egui::Context) {
  insert_window(app, ctx);
  dxf_dialog(app, ctx);
}

fn insert_window(app: &mut App, ctx: &egui::Context) {
  refresh(app);
  let Some(mut w) = app.inserts.window.take() else { return };
  let now = fox_engine::store::now();
  let mut act = Act::None;
  const TABLE_H: f32 = 330.0;
  let (_, closed) = modal(ctx, "insert", w.target.title(), 760.0, |ui| {
    ui.horizontal_top(|ui| {
      // the projects
      egui::Frame::new().stroke(Stroke::new(1.0, theme::BORDER)).show(ui, |ui| {
        ui.set_width(220.0);
        ui.set_height(TABLE_H);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let (head, _) = ui.allocate_exact_size(vec2(220.0, 26.0), egui::Sense::hover());
        ui.painter().rect_filled(head, 0.0, Color32::from_gray(246));
        ui.painter().text(
          pos2(head.left() + 10.0, head.center().y),
          egui::Align2::LEFT_CENTER,
          "PROJECT",
          egui::FontId::proportional(11.0),
          theme::GROUP_LABEL,
        );
        egui::ScrollArea::vertical().id_salt("insert-projects").auto_shrink([false, false]).show(ui, |ui| {
          for p in w.projects.clone() {
            let id = s(&p, "id");
            let on = w.project.as_deref() == Some(id.as_str());
            let (rect, r) = row(ui, on);
            row_text(ui, rect, 10.0, &s(&p, "name"), theme::TEXT);
            if r.clicked() {
              w.project = Some(id);
              w.folder = None;
              w.selected = None;
              w.stale = true;
            }
          }
          if w.projects.is_empty() {
            let (rect, _) = row(ui, false);
            row_text(ui, rect, 10.0, "No projects yet", theme::MUTED);
          }
        });
      });
      ui.add_space(12.0);
      ui.vertical(|ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
          let pname = w.projects.iter().find(|p| Some(s(p, "id")) == w.project).map(|p| s(p, "name"));
          if ui.link(RichText::new(pname.unwrap_or_default()).strong()).clicked() {
            act = Act::Navigate(String::new());
          }
          for p in w.listing.get("path").and_then(Value::as_array).cloned().unwrap_or_default() {
            ui.label(RichText::new("›").color(theme::MUTED));
            if ui.link(s(&p, "name")).clicked() {
              act = Act::Navigate(s(&p, "id"));
            }
          }
        });
        ui.add_space(4.0);
        egui::Frame::new().stroke(Stroke::new(1.0, theme::BORDER)).show(ui, |ui| {
          ui.set_height(TABLE_H - 30.0);
          ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
          let width = ui.available_width();
          let (head, _) = ui.allocate_exact_size(vec2(width, 26.0), egui::Sense::hover());
          ui.painter().rect_filled(head, 0.0, Color32::from_gray(246));
          let small = egui::FontId::proportional(11.0);
          let hx = |x: f32| pos2(head.left() + x, head.center().y);
          ui.painter().text(hx(10.0), egui::Align2::LEFT_CENTER, "NAME", small.clone(), theme::GROUP_LABEL);
          ui.painter().text(hx(width - 170.0), egui::Align2::LEFT_CENTER, "LAST UPDATED", small, theme::GROUP_LABEL);
          ui.painter().hline(head.x_range(), head.bottom(), Stroke::new(1.0, theme::BORDER));
          egui::ScrollArea::vertical().id_salt("insert-table").auto_shrink([false, false]).show(ui, |ui| {
            let folders = w.listing.get("folders").and_then(Value::as_array).cloned().unwrap_or_default();
            let items: Vec<Value> = w
              .listing
              .get("items")
              .and_then(Value::as_array)
              .cloned()
              .unwrap_or_default()
              .into_iter()
              .filter(|it| w.target.takes(it))
              .collect();
            for d in &folders {
              let (rect, r) = row(ui, false);
              icons::draw(
                ui.painter(),
                egui::Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), Vec2::splat(16.0)),
                Icon::Folder,
                theme::ICON_INK,
              );
              row_text(ui, rect, 34.0, &s(d, "name"), theme::TEXT);
              if r.double_clicked() || r.clicked() {
                act = Act::Navigate(s(d, "id"));
              }
            }
            for it in &items {
              let id = s(it, "id");
              let (rect, r) = row(ui, w.selected.as_deref() == Some(id.as_str()));
              let ir = egui::Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), Vec2::splat(16.0));
              if s(it, "kind") == "design" {
                icons::draw_color(ui.painter(), ir, Icon::Component);
              } else {
                icons::draw(ui.painter(), ir, Icon::File, theme::ICON_INK);
              }
              row_text(ui, rect, 34.0, &s(it, "name"), theme::TEXT);
              let when = it
                .pointer("/latest/created")
                .and_then(Value::as_u64)
                .map(|t| fox_engine::store::relative_time(t, now))
                .unwrap_or_default();
              row_text(ui, rect, width - 170.0, &when, theme::MUTED);
              if r.double_clicked() {
                act = Act::Pick(id);
              } else if r.clicked() {
                w.selected = Some(id);
              }
            }
            if folders.is_empty() && items.is_empty() {
              ui.add_space(12.0);
              ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new("This folder is empty.").color(theme::MUTED));
              });
            }
          });
        });
      });
    });
    let b = footer(ui, w.target.computer_button(), w.target.primary(), w.selected.is_some());
    if b.left {
      act = Act::FromComputer;
    } else if b.primary
      && let Some(id) = w.selected.clone()
    {
      act = Act::Pick(id);
    } else if b.cancel {
      act = Act::Close;
    }
  });
  if closed {
    act = Act::Close;
  }
  let target = w.target;
  match act {
    Act::None => app.inserts.window = Some(w),
    Act::Close => {}
    Act::Navigate(id) => {
      w.folder = Some(id).filter(|id| !id.is_empty());
      w.selected = None;
      w.stale = true;
      app.inserts.window = Some(w);
    }
    Act::Pick(id) => pick_hub_item(app, target, &id),
    Act::FromComputer => {
      let exts = target.extensions();
      let picked = rfd::FileDialog::new().set_title(target.title()).add_filter(target.filter_name(), exts).pick_file();
      match picked {
        Some(p) => insert_path(app, target, &p),
        // the window stays when the file dialog is cancelled
        None => app.inserts.window = Some(w),
      }
    }
  }
}

/// A hub item picked in the window: Derive inserts the design itself; files are written to a
/// scratch file and inserted from there.
pub(crate) fn pick_hub_item(app: &mut App, target: Target, id: &str) {
  if target == Target::Derive {
    // the source design comes in linked, so it follows the source's newer versions
    if app.run("component.insert", json!({"item": id, "link": true})).is_some() {
      app.fit_view_now();
    }
    return;
  }
  match scratch_copy(app, id) {
    Ok(p) => insert_path(app, target, &p),
    Err(e) => app.status = Some((e, true)),
  }
}

/// Writes the latest version of hub item `id` to a scratch file under its own name.
fn scratch_copy(app: &mut App, id: &str) -> Result<PathBuf, String> {
  let item = app.s.execute("hub.versions", &json!({"item": id})).map_err(|e| e.0)?;
  let name = item.pointer("/item/name").and_then(Value::as_str).unwrap_or("insert").to_string();
  // a translated upload comes back as the file it was uploaded as
  let ext = item
    .get("item")
    .and_then(original_ext)
    .or_else(|| item.pointer("/item/ext").and_then(Value::as_str).map(str::to_string))
    .unwrap_or_default();
  let dir = std::env::temp_dir().join(format!("fissionoxide-insert-{}", std::process::id()));
  std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  let file = if ext.is_empty() || name.to_ascii_lowercase().ends_with(&format!(".{ext}")) {
    name
  } else {
    format!("{name}.{ext}")
  };
  let path = dir.join(file);
  app.s.execute("hub.download", &json!({"item": id, "path": path})).map_err(|e| e.0)?;
  Ok(path)
}

/// Inserts a file from this computer (or a hub file's scratch copy).
pub(crate) fn insert_path(app: &mut App, target: Target, p: &Path) {
  match target {
    Target::Canvas => crate::canvas_ui::start_insert_with(app, Some(p.to_path_buf())),
    Target::Decal => {
      // FissionOxide has no decal feature: the image goes on as an opaque canvas
      let name = p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Decal".into());
      if app.run("canvas.insert", json!({"image": p, "opacity": 1.0, "name": name})).is_some() {
        app.status = Some((format!("Inserted {} as a decal on the XY plane", p.display()), false));
      }
    }
    Target::Svg | Target::Mesh => app.import_path(p),
    Target::Derive => {}
  }
}

// ---------------------------------------------------------------- INSERT DXF

/// INSERT > Insert DXF: the docked dialog.
pub(crate) fn open_dxf(app: &mut App) {
  app.cancel_dialog();
  app.inserts.dxf = Some(DxfDialog::default());
  app.hint("Select a plane or sketch, then a DXF file");
}

/// The plane or sketch INSERT DXF puts the curves on: the selected sketch, construction plane or
/// planar face, else the XY plane.
fn dxf_plane(app: &App) -> (String, Value) {
  if let Some(p) = &app.picked_plane {
    let label = p.as_str().map(|k| format!("{} Plane", k.to_uppercase())).unwrap_or_else(|| "1 selected".into());
    return (label, p.clone());
  }
  match app.s.selection.first() {
    Some(fox_engine::Sel::Face { body, face }) => ("1 selected".into(), json!({"body": body.0, "face": face})),
    _ => ("XY Plane".into(), json!("xy")),
  }
}

fn dxf_dialog(app: &mut App, ctx: &egui::Context) {
  let Some(mut d) = app.inserts.dxf.take() else { return };
  if app.dialog.is_some() {
    // another command took the dialog's place
    app.end_hint();
    return;
  }
  let (plane_label, plane) = dxf_plane(app);
  let width = 300.0;
  let dock = panels::dialog_dock(app.canvas, app.s.edit_sketch.is_some());
  let (mut ok, mut cancel, mut browse) = (false, false, false);
  egui::Area::new(egui::Id::new("insert-dxf"))
    .pivot(egui::Align2::RIGHT_TOP)
    .fixed_pos(dock)
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      panels::dialog_frame().show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        panels::dialog_header(ui, Icon::InsertDxf, "INSERT DXF", width);
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
          egui::Grid::new("insert-dxf-grid").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
            ui.label("Plane/Sketch");
            let plane_label = if app.s.edit_sketch.is_some() { "1 selected".to_string() } else { plane_label.clone() };
            let _ = ui.add(egui::Button::new(RichText::new(&plane_label).color(theme::TEXT)).fill(theme::CONTEXT_TAB));
            ui.end_row();
            ui.label("Select DXF File");
            ui.horizontal(|ui| {
              if icons::icon_button(ui, Icon::Folder, 22.0, "Select DXF File", false, None).clicked() {
                browse = true;
              }
              if let Some(name) = d.file.as_ref().and_then(|p| p.file_name()) {
                ui.label(name.to_string_lossy());
              }
            });
            ui.end_row();
            ui.label("Insert Mode");
            egui::ComboBox::from_id_salt("insert-dxf-mode").selected_text(INSERT_MODES[d.mode]).width(150.0).show_ui(
              ui,
              |ui| {
                ui.selectable_value(&mut d.mode, 0, INSERT_MODES[0]);
                // the DXF reader doesn't keep layers apart yet
                ui.add_enabled(false, egui::Button::selectable(false, INSERT_MODES[1]))
                  .on_disabled_hover_text("FissionOxide reads every layer into one sketch");
              },
            );
            ui.end_row();
          });
        });
        let sep = ui.allocate_exact_size(vec2(width, 1.0), egui::Sense::hover()).0;
        ui.painter().rect_filled(sep, 0.0, theme::SEPARATOR);
        egui::Frame::new().fill(theme::DIALOG_BODY).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
          ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
              if panels::secondary_button(ui, "Cancel").clicked() {
                cancel = true;
              }
              let r = ui.add_enabled(d.file.is_some(), egui::Button::new("OK"));
              ok = r.clicked();
            });
          });
        });
      });
    });
  if browse && let Some(p) = rfd::FileDialog::new().set_title("Select DXF File").add_filter("DXF", &["dxf"]).pick_file()
  {
    d.file = Some(p);
  }
  if cancel {
    app.end_hint();
    return;
  }
  if ok && let Some(file) = d.file.clone() {
    app.end_hint();
    // a sketch being edited takes the curves; otherwise a new sketch on the plane
    let done = if app.s.edit_sketch.is_some() {
      app.run("sketch.insert", json!({"from": {"path": file}})).is_some()
    } else {
      app.run("import", json!({"path": file, "plane": plane})).is_some()
    };
    if done {
      app.status = Some((format!("Inserted {}", file.display()), false));
      return;
    }
  }
  app.inserts.dxf = Some(d);
}

/// Escape closes INSERT DXF. Returns whether it was open.
pub(crate) fn escape(app: &mut App) -> bool {
  if app.inserts.dxf.take().is_some() {
    app.end_hint();
    return true;
  }
  false
}
