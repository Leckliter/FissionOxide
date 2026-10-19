//! The data panel: projects, folders and designs with their version history, like Fusion's.
//!
//! Everything goes through the engine's `hub.*` commands; the panel only caches their JSON
//! results (refreshed after each action and every few seconds, so saves from the CLI or another
//! window show up) and the thumbnails as textures.

use std::collections::HashMap;

use egui::{Color32, RichText, TextureHandle};
use serde_json::{Value, json};

use crate::{App, theme};

/// How often the listing is re-read while the panel is open (seconds).
const REFRESH_SECS: f64 = 5.0;
const THUMB: f32 = 56.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
  #[default]
  Files,
  Recent,
  Activity,
  Trash,
}

/// A one-line text prompt shown in the panel.
#[derive(Debug, Clone, PartialEq)]
enum Prompt {
  NewProject(String),
  NewFolder(String),
  RenameProject(String),
  RenameFolder(String, String),
  RenameItem(String, String),
  Milestone(String, u32, String),
  Comment(String, u32, String),
}

/// The Save dialog's fields.
#[derive(Debug, Clone, Default)]
pub struct SaveForm {
  pub name: String,
  pub description: String,
  pub milestone: String,
  /// Save as a new item (Save As) instead of a new version of the linked item.
  pub as_new: bool,
  pub project: Option<String>,
  pub folder: Option<String>,
}

#[derive(Default)]
pub struct DataPanel {
  pub open: bool,
  tab: Tab,
  project: Option<String>,
  folder: Option<String>,
  selected: Option<String>,
  search: String,
  projects: Vec<Value>,
  listing: Value,
  details: Value,
  other: Value,
  compare: Option<Value>,
  prompt: Option<Prompt>,
  pub save: Option<SaveForm>,
  /// Item id + version → texture (None = no thumbnail).
  thumbs: HashMap<(String, u64), Option<TextureHandle>>,
  stale: bool,
  last_refresh: f64,
  error: Option<String>,
}

impl DataPanel {
  pub fn invalidate(&mut self) {
    self.stale = true;
  }

  pub fn toggle(&mut self) {
    self.open = !self.open;
    self.stale = true;
  }

  /// Where a new design is saved: the folder the panel shows.
  pub fn location(&self) -> (Option<String>, Option<String>) {
    (self.project.clone(), self.folder.clone())
  }
}

fn hub(app: &mut App, cmd: &str, p: Value) -> Option<Value> {
  match app.s.execute(cmd, &p) {
    Ok(v) => {
      app.data.error = None;
      Some(v)
    }
    Err(e) => {
      app.data.error = Some(e.0.clone());
      app.status = Some((e.0, true));
      None
    }
  }
}

fn s(v: &Value, k: &str) -> String {
  v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn refresh(app: &mut App, ctx: &egui::Context) {
  let d = &mut app.data;
  d.stale = false;
  d.last_refresh = ctx.input(|i| i.time);
  let projects = hub(app, "hub.projects", json!({})).and_then(|v| v.as_array().cloned()).unwrap_or_default();
  let d = &mut app.data;
  d.projects = projects;
  if d.project.as_ref().is_none_or(|p| !d.projects.iter().any(|x| s(x, "id") == *p)) {
    d.project = d.projects.first().map(|p| s(p, "id"));
    d.folder = None;
  }
  let (project, folder, tab, selected) = (d.project.clone(), d.folder.clone(), d.tab, d.selected.clone());
  app.data.listing = match &project {
    Some(p) => hub(app, "hub.list", json!({"project": p, "folder": folder})).unwrap_or(Value::Null),
    None => Value::Null,
  };
  app.data.other = match tab {
    Tab::Files if !app.data.search.trim().is_empty() => {
      let q = app.data.search.clone();
      hub(app, "hub.search", json!({"query": q})).unwrap_or(Value::Null)
    }
    Tab::Recent => hub(app, "hub.recent", json!({"limit": 30})).unwrap_or(Value::Null),
    Tab::Activity => hub(app, "hub.activity", json!({"limit": 80})).unwrap_or(Value::Null),
    Tab::Trash => hub(app, "hub.trash", json!({})).unwrap_or(Value::Null),
    _ => Value::Null,
  };
  app.data.details = match &selected {
    Some(i) => hub(app, "hub.versions", json!({"item": i})).unwrap_or(Value::Null),
    None => Value::Null,
  };
  if app.data.details.is_null() {
    app.data.selected = None;
  }
  load_thumbs(app, ctx);
}

/// Loads the thumbnails of everything listed that isn't cached yet.
fn load_thumbs(app: &mut App, ctx: &egui::Context) {
  let mut want: Vec<(String, u64)> = Vec::new();
  let mut items: Vec<Value> = app.data.listing.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
  if let Some(a) = app.data.other.as_array() {
    items.extend(a.iter().cloned());
  }
  for it in &items {
    if let Some(n) = it.pointer("/latest/number").and_then(Value::as_u64) {
      want.push((s(it, "id"), n));
    }
  }
  if let Some(vs) = app.data.details.get("versions").and_then(Value::as_array) {
    let id = app.data.details.pointer("/item/id").and_then(Value::as_str).unwrap_or_default().to_string();
    for v in vs {
      if let Some(n) = v.get("number").and_then(Value::as_u64) {
        want.push((id.clone(), n));
      }
    }
  }
  want.retain(|k| !app.data.thumbs.contains_key(k));
  if want.is_empty() {
    return;
  }
  let Ok(store) = fox_engine::hub::open_store(&app.s) else { return };
  for (item, n) in want {
    let tex = store.thumbnail(&item, Some(n as u32)).and_then(|png| {
      let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png).ok()?.to_rgba8();
      let size = [img.width() as usize, img.height() as usize];
      let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
      Some(ctx.load_texture(format!("thumb-{item}-v{n}"), color, egui::TextureOptions::LINEAR))
    });
    app.data.thumbs.insert((item, n), tex);
  }
}

fn thumb(ui: &mut egui::Ui, app: &App, item: &str, version: Option<u64>, size: f32) -> egui::Response {
  let tex = version.and_then(|n| app.data.thumbs.get(&(item.to_string(), n))).and_then(Option::as_ref);
  match tex {
    Some(t) => ui.add(egui::Image::new(t).fit_to_exact_size(egui::vec2(size, size)).corner_radius(4.0)),
    None => {
      let (rect, r) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
      ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(232, 235, 240));
      ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "📄",
        egui::FontId::proportional(size * 0.4),
        theme::MUTED,
      );
      r
    }
  }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
  if !app.data.open {
    return;
  }
  let ctx = ui.ctx().clone();
  let now = ctx.input(|i| i.time);
  if app.data.stale || now - app.data.last_refresh > REFRESH_SECS {
    refresh(app, &ctx);
  }
  ctx.request_repaint_after(std::time::Duration::from_secs_f64(REFRESH_SECS));
  egui::Panel::left("data_panel")
    .default_size(320.0)
    .resizable(true)
    .frame(
      egui::Frame::NONE
        .fill(Color32::from_rgb(243, 245, 248))
        .inner_margin(egui::Margin::same(8))
        .stroke(egui::Stroke::new(1.0, theme::BORDER)),
    )
    .show(ui, |ui| {
      ui.horizontal(|ui| {
        ui.label(RichText::new("DATA PANEL").small().strong().color(theme::GROUP_LABEL));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          if ui.small_button("×").on_hover_text("Close the data panel").clicked() {
            app.data.open = false;
          }
          if ui.small_button("⟳").on_hover_text("Refresh").clicked() {
            app.data.stale = true;
          }
        });
      });
      ui.horizontal(|ui| {
        for (t, label) in
          [(Tab::Files, "Files"), (Tab::Recent, "Recent"), (Tab::Activity, "Activity"), (Tab::Trash, "Trash")]
        {
          if ui.selectable_label(app.data.tab == t, label).clicked() {
            app.data.tab = t;
            app.data.stale = true;
          }
        }
      });
      ui.separator();
      match app.data.tab {
        Tab::Files => files_tab(app, ui),
        Tab::Recent => {
          let recent = app.data.other.as_array().cloned().unwrap_or_default();
          item_list(app, ui, recent, true)
        }
        Tab::Activity => activity_tab(app, ui),
        Tab::Trash => trash_tab(app, ui),
      }
      if let Some(e) = app.data.error.clone() {
        ui.colored_label(theme::ERROR, e);
      }
      details(app, ui);
    });
  prompt_window(app, &ctx);
  compare_window(app, &ctx);
}

fn files_tab(app: &mut App, ui: &mut egui::Ui) {
  // project picker
  ui.horizontal(|ui| {
    let current = app.data.project.clone().unwrap_or_default();
    let name = app
      .data
      .projects
      .iter()
      .find(|p| s(p, "id") == current)
      .map(|p| s(p, "name"))
      .unwrap_or_else(|| "No projects yet".into());
    egui::ComboBox::from_id_salt("hub-project").selected_text(RichText::new(name).strong()).width(170.0).show_ui(
      ui,
      |ui| {
        for p in app.data.projects.clone() {
          let id = s(&p, "id");
          if ui.selectable_label(current == id, s(&p, "name")).clicked() {
            app.data.project = Some(id);
            app.data.folder = None;
            app.data.selected = None;
            app.data.stale = true;
          }
        }
      },
    );
    if ui.small_button("+").on_hover_text("New project").clicked() {
      app.data.prompt = Some(Prompt::NewProject(String::new()));
    }
    if let Some(p) = app.data.project.clone() {
      ui.menu_button("…", |ui| {
        if ui.button("Rename project…").clicked() {
          app.data.prompt = Some(Prompt::RenameProject(p.clone()));
          ui.close();
        }
        if ui.button("Delete project").clicked() {
          hub(app, "hub.project.delete", json!({"project": p}));
          app.data.project = None;
          app.data.stale = true;
          ui.close();
        }
      });
    }
  });
  ui.horizontal(|ui| {
    let r = ui.add(
      egui::TextEdit::singleline(&mut app.data.search)
        .hint_text("🔍 Search designs, versions, milestones")
        .desired_width(f32::INFINITY),
    );
    if r.changed() {
      app.data.stale = true;
    }
  });
  if !app.data.search.trim().is_empty() {
    let found = app.data.other.as_array().cloned().unwrap_or_default();
    ui.label(RichText::new(format!("{} found", found.len())).small().color(theme::MUTED));
    item_list(app, ui, found, true);
    return;
  }
  // breadcrumb
  ui.horizontal_wrapped(|ui| {
    let pname =
      app.data.projects.iter().find(|p| Some(s(p, "id")) == app.data.project).map(|p| s(p, "name")).unwrap_or_default();
    if ui.link(pname).clicked() {
      app.data.folder = None;
      app.data.stale = true;
    }
    for f in app.data.listing.get("path").and_then(Value::as_array).cloned().unwrap_or_default() {
      ui.label(RichText::new("›").color(theme::MUTED));
      if ui.link(s(&f, "name")).clicked() {
        app.data.folder = Some(s(&f, "id"));
        app.data.stale = true;
      }
    }
  });
  ui.horizontal(|ui| {
    if ui.button("📁 New folder").on_hover_text("New folder here").clicked() && app.data.project.is_some() {
      app.data.prompt = Some(Prompt::NewFolder(String::new()));
    }
    if ui.button("⬆ Upload").on_hover_text("Upload files: designs, STEP, IGES, meshes, DXF, Fusion .f3d …").clicked()
    {
      upload_dialog(app);
    }
    if ui.button("💾 Save here").on_hover_text("Save the open design as a new design in this folder").clicked() {
      app.save_to_hub(true);
    }
  });
  ui.add_space(4.0);
  let folders = app.data.listing.get("folders").and_then(Value::as_array).cloned().unwrap_or_default();
  let items = app.data.listing.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
  egui::ScrollArea::vertical()
    .id_salt("hub-files")
    .max_height(ui.available_height() * 0.55)
    .auto_shrink([false, true])
    .show(ui, |ui| {
      for f in folders {
        let id = s(&f, "id");
        let r = ui.add(
          egui::Button::new(RichText::new(format!("📁  {}", s(&f, "name"))))
            .frame(false)
            .min_size(egui::vec2(ui.available_width(), 24.0)),
        );
        if r.clicked() {
          app.data.folder = Some(id.clone());
          app.data.selected = None;
          app.data.stale = true;
        }
        r.context_menu(|ui| {
          if ui.button("Rename…").clicked() {
            app.data.prompt = Some(Prompt::RenameFolder(id.clone(), s(&f, "name")));
            ui.close();
          }
          if ui.button("Delete (if empty)").clicked() {
            hub(app, "hub.folder.delete", json!({"folder": id}));
            app.data.stale = true;
            ui.close();
          }
        });
      }
      if items.is_empty() && app.data.project.is_some() {
        ui.label(RichText::new("No designs here yet. Save (Ctrl+S) or upload a file.").small().color(theme::MUTED));
      }
      if app.data.project.is_none() {
        ui.label(RichText::new("Save a design (Ctrl+S) to create your first project.").small().color(theme::MUTED));
      }
      item_rows(app, ui, items, false);
    });
}

fn item_list(app: &mut App, ui: &mut egui::Ui, items: Vec<Value>, show_path: bool) {
  egui::ScrollArea::vertical()
    .id_salt("hub-items")
    .max_height(ui.available_height() * 0.55)
    .auto_shrink([false, true])
    .show(ui, |ui| {
      if items.is_empty() {
        ui.label(RichText::new("Nothing here yet.").small().color(theme::MUTED));
      }
      item_rows(app, ui, items, show_path);
    });
}

fn item_rows(app: &mut App, ui: &mut egui::Ui, items: Vec<Value>, show_path: bool) {
  let now = fox_engine::store::now();
  for it in items {
    let id = s(&it, "id");
    let latest = it.get("latest").cloned().unwrap_or(Value::Null);
    let selected = app.data.selected.as_deref() == Some(id.as_str());
    let open_here = app.s.hub.as_ref().is_some_and(|h| h.item == id);
    let frame = egui::Frame::NONE
      .fill(if selected { Color32::from_rgb(214, 232, 250) } else { Color32::TRANSPARENT })
      .corner_radius(4.0)
      .inner_margin(egui::Margin::same(3));
    let r = frame
      .show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
          thumb(ui, app, &id, latest.get("number").and_then(Value::as_u64), THUMB);
          ui.vertical(|ui| {
            let mut title = RichText::new(s(&it, "name")).strong();
            if open_here {
              title = title.color(theme::ACCENT);
            }
            ui.label(title);
            let n = latest.get("number").and_then(Value::as_u64).unwrap_or(0);
            let when = latest
              .get("created")
              .and_then(Value::as_u64)
              .map(|t| fox_engine::store::relative_time(t, now))
              .unwrap_or_default();
            let kind = if s(&it, "kind") == "file" { format!(" · .{}", s(&it, "ext")) } else { String::new() };
            ui.label(RichText::new(format!("v{n} · {when}{kind}")).small().color(theme::MUTED));
            if let Some(m) = latest.get("milestone").and_then(Value::as_str) {
              ui.label(RichText::new(format!("⭐ {m}")).small().color(theme::WARNING));
            }
            if show_path {
              let mut path = vec![s(&it, "project_name")];
              path.extend(
                it.get("path")
                  .and_then(Value::as_array)
                  .into_iter()
                  .flatten()
                  .filter_map(|v| v.as_str().map(str::to_string)),
              );
              ui.label(RichText::new(path.join(" › ")).small().color(theme::MUTED));
            }
          });
        });
      })
      .response
      .interact(egui::Sense::click());
    if r.clicked() {
      app.data.selected = Some(id.clone());
      app.data.stale = true;
    }
    if r.double_clicked() {
      app.open_hub_item(&id, None);
    }
    r.context_menu(|ui| {
      if ui.button("Open").clicked() {
        app.open_hub_item(&id, None);
        ui.close();
      }
      if s(&it, "kind") == "design"
        && ui.button("Insert into Current Design").on_hover_text("Insert as a linked component").clicked()
      {
        insert_component(app, &id, None);
        ui.close();
      }
      if ui.button("Rename…").clicked() {
        app.data.prompt = Some(Prompt::RenameItem(id.clone(), s(&it, "name")));
        ui.close();
      }
      if ui.button("Copy").clicked() {
        hub(app, "hub.item.copy", json!({"item": id}));
        app.data.stale = true;
        ui.close();
      }
      if ui.button("Download…").clicked() {
        ui.close();
        download_dialog(app, &id, None, &s(&it, "name"), &s(&it, "ext"));
      }
      ui.separator();
      if ui.button("Delete").clicked() {
        hub(app, "hub.item.delete", json!({"item": id}));
        app.data.selected = None;
        app.data.stale = true;
        ui.close();
      }
    });
  }
}

fn details(app: &mut App, ui: &mut egui::Ui) {
  let d = app.data.details.clone();
  let Some(item) = d.get("item") else { return };
  let id = s(item, "id");
  let is_design = s(item, "kind") == "design";
  ui.separator();
  ui.horizontal(|ui| {
    ui.label(RichText::new(s(item, "name")).strong().size(14.0));
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
      if ui.button("Open").clicked() {
        app.open_hub_item(&id, None);
      }
      if is_design
        && ui.button("Insert").on_hover_text("Insert into the current design as a linked component").clicked()
      {
        insert_component(app, &id, None);
      }
    });
  });
  let versions = d.get("versions").and_then(Value::as_array).cloned().unwrap_or_default();
  ui.label(RichText::new(format!("{} versions", versions.len())).small().color(theme::MUTED));
  let latest = versions.first().and_then(|v| v.get("number")).and_then(Value::as_u64);
  egui::ScrollArea::vertical().id_salt("hub-versions").auto_shrink([false, false]).show(ui, |ui| {
    for v in versions {
      let n = v.get("number").and_then(Value::as_u64).unwrap_or(0);
      let is_latest = Some(n) == latest;
      let opened = app.s.hub.as_ref().is_some_and(|h| h.item == id && h.version as u64 == n);
      ui.horizontal(|ui| {
        thumb(ui, app, &id, Some(n), 40.0);
        ui.vertical(|ui| {
          ui.horizontal(|ui| {
            let mut t = RichText::new(format!("v{n}")).strong();
            if opened {
              t = t.color(theme::ACCENT);
            }
            ui.label(t);
            if is_latest {
              ui.label(RichText::new("latest").small().color(theme::ICON_GREEN));
            }
            if let Some(m) = v.get("milestone").and_then(Value::as_str) {
              ui.label(RichText::new(format!("⭐ {m}")).small().color(theme::WARNING));
            }
          });
          let desc = s(&v, "description");
          if !desc.is_empty() {
            ui.label(RichText::new(desc).small());
          }
          ui.label(RichText::new(format!("{} · {}", s(&v, "time"), s(&v, "author"))).small().color(theme::MUTED));
          version_comments(app, ui, &id, &v);
          ui.horizontal(|ui| {
            if ui.small_button("Open").clicked() {
              app.open_hub_item(&id, Some(n as u32));
            }
            if !is_latest
              && ui
                .small_button("Promote")
                .on_hover_text("Make this the latest version (copies it as a new version)")
                .clicked()
            {
              hub(app, "hub.promote", json!({"item": id, "version": n}));
              app.data.stale = true;
            }
            let star =
              if v.get("milestone").is_some_and(|m| !m.is_null()) { "Clear milestone" } else { "⭐ Milestone" };
            if ui.small_button(star).clicked() {
              if v.get("milestone").is_some_and(|m| !m.is_null()) {
                hub(app, "hub.milestone", json!({"item": id, "version": n}));
                app.data.stale = true;
              } else {
                app.data.prompt = Some(Prompt::Milestone(id.clone(), n as u32, String::new()));
              }
            }
            if ui.small_button("💬").on_hover_text("Comment on this version").clicked() {
              app.data.prompt = Some(Prompt::Comment(id.clone(), n as u32, String::new()));
            }
            if is_design
              && !is_latest
              && ui
                .small_button("Compare")
                .on_hover_text("What changed from this version to the latest, shown in the viewport too")
                .clicked()
            {
              compare(app, &id, n);
            }
            if !is_latest && ui.small_button("🗑").on_hover_text("Delete this version").clicked() {
              hub(app, "hub.version.delete", json!({"item": id, "version": n}));
              app.data.stale = true;
            }
          });
        });
      });
      ui.add_space(4.0);
    }
  });
}

fn activity_tab(app: &mut App, ui: &mut egui::Ui) {
  let now = fox_engine::store::now();
  let list = app.data.other.as_array().cloned().unwrap_or_default();
  egui::ScrollArea::vertical()
    .id_salt("hub-activity")
    .max_height(ui.available_height() * 0.55)
    .auto_shrink([false, true])
    .show(ui, |ui| {
      for a in list {
        let when =
          a.get("time").and_then(Value::as_u64).map(|t| fox_engine::store::relative_time(t, now)).unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
          ui.label(RichText::new(s(&a, "author")).strong().small());
          ui.label(RichText::new(s(&a, "text")).small());
          ui.label(RichText::new(when).small().color(theme::MUTED));
        });
      }
    });
}

fn trash_tab(app: &mut App, ui: &mut egui::Ui) {
  let items = app.data.other.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
  let projects = app.data.other.get("projects").and_then(Value::as_array).cloned().unwrap_or_default();
  egui::ScrollArea::vertical()
    .id_salt("hub-trash")
    .max_height(ui.available_height() * 0.55)
    .auto_shrink([false, true])
    .show(ui, |ui| {
      if items.is_empty() && projects.is_empty() {
        ui.label(RichText::new("The trash is empty.").small().color(theme::MUTED));
      }
      for p in projects {
        ui.horizontal(|ui| {
          ui.label(format!("🗂 {}", s(&p, "name")));
          if ui.small_button("Restore").clicked() {
            hub(app, "hub.project.restore", json!({"project": s(&p, "id")}));
            app.data.stale = true;
          }
        });
      }
      for it in items {
        let id = s(&it, "id");
        ui.horizontal(|ui| {
          ui.label(format!("📄 {}", s(&it, "name")));
          if ui.small_button("Restore").clicked() {
            hub(app, "hub.item.restore", json!({"item": id}));
            app.data.stale = true;
          }
          if ui.small_button("Delete forever").clicked() {
            hub(app, "hub.item.purge", json!({"item": id}));
            app.data.stale = true;
          }
        });
      }
    });
}

fn prompt_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut p) = app.data.prompt.clone() else { return };
  let (title, hint) = match &p {
    Prompt::NewProject(_) => ("New project", "Project name"),
    Prompt::NewFolder(_) => ("New folder", "Folder name"),
    Prompt::RenameProject(_) => ("Rename project", "New name"),
    Prompt::RenameFolder(..) => ("Rename folder", "New name"),
    Prompt::RenameItem(..) => ("Rename design", "New name"),
    Prompt::Milestone(..) => ("Mark milestone", "Milestone name (e.g. Sent to shop)"),
    Prompt::Comment(..) => ("Comment on version", "Your comment"),
  };
  let mut done = false;
  let mut cancel = false;
  egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
    ctx,
    |ui| {
      let text = match &mut p {
        Prompt::NewProject(t) | Prompt::NewFolder(t) | Prompt::RenameProject(t) => t,
        Prompt::RenameFolder(_, t) | Prompt::RenameItem(_, t) | Prompt::Milestone(_, _, t) => t,
        Prompt::Comment(_, _, t) => t,
      };
      let r = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(260.0));
      r.request_focus();
      if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        done = true;
      }
      ui.horizontal(|ui| {
        done |= ui.button("OK").clicked();
        cancel = ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
      });
    },
  );
  if cancel {
    app.data.prompt = None;
    return;
  }
  if !done {
    app.data.prompt = Some(p);
    return;
  }
  app.data.prompt = None;
  let project = app.data.project.clone();
  let folder = app.data.folder.clone();
  let ok = match p {
    Prompt::NewProject(name) => hub(app, "hub.project.create", json!({"name": name})).map(|v| {
      app.data.project = v.get("project").and_then(Value::as_str).map(str::to_string);
      app.data.folder = None;
    }),
    Prompt::NewFolder(name) => {
      hub(app, "hub.folder.create", json!({"project": project, "parent": folder, "name": name})).map(|_| ())
    }
    Prompt::RenameProject(name) => {
      hub(app, "hub.project.rename", json!({"project": project, "name": name})).map(|_| ())
    }
    Prompt::RenameFolder(id, name) => hub(app, "hub.folder.rename", json!({"folder": id, "name": name})).map(|_| ()),
    Prompt::RenameItem(id, name) => hub(app, "hub.item.rename", json!({"item": id, "name": name})).map(|_| ()),
    Prompt::Milestone(id, n, name) => {
      hub(app, "hub.milestone", json!({"item": id, "version": n, "name": name})).map(|_| ())
    }
    Prompt::Comment(id, n, text) => {
      hub(app, "hub.comment", json!({"item": id, "version": n, "text": text})).map(|_| ())
    }
  };
  let _ = ok;
  app.data.stale = true;
}

fn compare_window(app: &mut App, ctx: &egui::Context) {
  let Some(c) = app.data.compare.clone() else { return };
  let mut open = true;
  let title = format!(
    "Changes v{} → v{}",
    c.get("a").and_then(Value::as_u64).unwrap_or(0),
    c.get("b").and_then(Value::as_u64).unwrap_or(0)
  );
  egui::Window::new(title).open(&mut open).default_width(360.0).show(ctx, |ui| {
    if let Some(d) = c.get("diff") {
      let vol = |k: &str| d.get(k).and_then(Value::as_f64).unwrap_or(0.0);
      ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("■ added").color(theme::ICON_GREEN));
        ui.label(RichText::new(format!("{:.1} mm³", vol("added_mm3"))).small());
        ui.label(RichText::new("■ removed").color(theme::ERROR));
        ui.label(RichText::new(format!("{:.1} mm³", vol("removed_mm3"))).small());
        ui.label(RichText::new("□ unchanged").color(theme::MUTED));
      });
      ui.label(
        RichText::new("The viewport shows the comparison until this window closes.").small().color(theme::MUTED),
      );
      ui.separator();
    }
    let changes = c.get("changes").and_then(Value::as_array).cloned().unwrap_or_default();
    if changes.is_empty() {
      ui.label("No differences in the timeline or parameters.");
    }
    for ch in changes {
      let (sym, col) = match s(&ch, "kind").as_str() {
        "added" => ("+", theme::ICON_GREEN),
        "removed" => ("-", theme::ERROR),
        _ => ("~", theme::ACCENT),
      };
      ui.horizontal(|ui| {
        ui.label(RichText::new(sym).color(col).strong());
        ui.label(format!("{} {} {}", s(&ch, "kind"), s(&ch, "what"), s(&ch, "name")));
        let d = s(&ch, "detail");
        if !d.is_empty() {
          ui.label(RichText::new(d).small().color(theme::MUTED));
        }
      });
    }
  });
  if !open {
    app.data.compare = None;
    if app.s.diff.is_some() {
      let _ = app.s.execute("hub.diff", &json!({"clear": true}));
      app.render_key = None;
    }
  }
}

/// Compares version `n` with the latest: the timeline/parameter changes, and the geometry in
/// the viewport (added green, removed red, unchanged ghosted).
fn compare(app: &mut App, item: &str, n: u64) {
  let Some(mut c) = hub(app, "hub.compare", json!({"item": item, "a": n})) else { return };
  match app.s.execute("hub.diff", &json!({"item": item, "a": n})) {
    Ok(d) => c["diff"] = d,
    // a version without bodies (or that fails to rebuild) still gets the text comparison
    Err(e) => app.status = Some((format!("no geometry comparison: {}", e.0), false)),
  }
  app.render_key = None;
  app.data.compare = Some(c);
}

/// A version's comments, each with a delete button.
fn version_comments(app: &mut App, ui: &mut egui::Ui, item: &str, v: &Value) {
  let comments = v.get("comments").and_then(Value::as_array).cloned().unwrap_or_default();
  for c in comments {
    ui.horizontal_wrapped(|ui| {
      ui.label(RichText::new("💬").small());
      ui.label(RichText::new(s(&c, "author")).strong().small());
      ui.label(RichText::new(s(&c, "text")).small());
      ui.label(RichText::new(s(&c, "time_text")).small().color(theme::MUTED));
      if ui.small_button("×").on_hover_text("Delete this comment").clicked() {
        hub(app, "hub.comment.delete", json!({"item": item, "comment": s(&c, "id")}));
        app.data.stale = true;
      }
    });
  }
}

fn import_filter() -> (Vec<&'static str>, Vec<(&'static str, Vec<&'static str>)>) {
  let mut all = Vec::new();
  let mut each = Vec::new();
  for f in fox_engine::exchange::FORMATS.iter().filter(|f| f.import) {
    all.extend_from_slice(f.extensions);
    each.push((f.name, f.extensions.to_vec()));
  }
  (all, each)
}

/// A file dialog listing every format FissionOxide reads.
pub fn pick_importable(title: &str, multiple: bool) -> Vec<std::path::PathBuf> {
  let (all, each) = import_filter();
  let mut d = rfd::FileDialog::new().set_title(title).add_filter("All supported files", &all);
  for (name, exts) in &each {
    d = d.add_filter(*name, exts);
  }
  if multiple { d.pick_files().unwrap_or_default() } else { d.pick_file().into_iter().collect() }
}

fn upload_dialog(app: &mut App) {
  let files = pick_importable("Upload to the data panel", true);
  let (project, folder) = app.data.location();
  for f in files {
    if let Some(r) = hub(app, "hub.upload", json!({"path": f, "project": project, "folder": folder})) {
      let msg = match r.get("warning").and_then(Value::as_str) {
        Some(w) => (format!("Uploaded {}: {w}", f.display()), false),
        None => (format!("Uploaded {}", f.display()), false),
      };
      app.status = Some(msg);
    }
  }
  app.data.stale = true;
}

fn download_dialog(app: &mut App, item: &str, version: Option<u32>, name: &str, ext: &str) {
  let mut d = rfd::FileDialog::new().set_file_name(format!("{name}.{ext}"));
  if ext == "fox" {
    d = d.add_filter("FissionOxide archive with history", &["foxz"]);
  }
  d = d.add_filter("Same format", &[ext]);
  for f in fox_engine::exchange::FORMATS.iter().filter(|f| f.export && f.id != "foxz") {
    d = d.add_filter(f.name, f.extensions);
  }
  if let Some(p) = d.save_file()
    && hub(app, "hub.download", json!({"item": item, "version": version, "path": p})).is_some()
  {
    app.status = Some((format!("Downloaded to {}", p.display()), false));
  }
}

/// The Save dialog (Ctrl+S): name and location for a new design, a description and optional
/// milestone for every version.
pub fn save_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut f) = app.data.save.clone() else { return };
  let linked = app.s.hub.is_some() && !f.as_new;
  let title = if linked { "Save version" } else { "Save" };
  let mut done = false;
  let mut cancel = false;
  egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, -80.0]).show(
    ctx,
    |ui| {
      egui::Grid::new("save-form").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        if !linked {
          ui.label("Name");
          let r = ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(260.0));
          if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            done = true;
          }
          ui.end_row();
          ui.label("Location");
          let pname = app
            .data
            .projects
            .iter()
            .find(|p| Some(s(p, "id")) == f.project)
            .map(|p| s(p, "name"))
            .unwrap_or_else(|| "Default Project".into());
          egui::ComboBox::from_id_salt("save-project").selected_text(pname).width(260.0).show_ui(ui, |ui| {
            for p in app.data.projects.clone() {
              if ui.selectable_label(f.project.as_deref() == Some(&s(&p, "id")), s(&p, "name")).clicked() {
                if f.project.as_deref() != Some(&s(&p, "id")) {
                  f.folder = None;
                }
                f.project = Some(s(&p, "id"));
              }
            }
          });
          ui.end_row();
        }
        ui.label("Description");
        let r = ui.add(
          egui::TextEdit::singleline(&mut f.description).hint_text("What changed (optional)").desired_width(260.0),
        );
        if linked {
          r.request_focus();
        }
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
          done = true;
        }
        ui.end_row();
        ui.label("Milestone");
        ui.add(
          egui::TextEdit::singleline(&mut f.milestone)
            .hint_text("Name it to mark a milestone (optional)")
            .desired_width(260.0),
        );
        ui.end_row();
      });
      if linked && let Some(h) = &app.s.hub {
        ui.label(
          RichText::new(format!("Saves a new version after v{} — earlier versions are kept.", h.version))
            .small()
            .color(theme::MUTED),
        );
      }
      ui.horizontal(|ui| {
        done |= ui.button(RichText::new("Save").strong()).clicked();
        cancel = ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
      });
    },
  );
  if cancel {
    app.data.save = None;
    return;
  }
  if !done {
    app.data.save = Some(f);
    return;
  }
  app.data.save = None;
  let mut p = json!({"description": f.description, "new": f.as_new || app.s.hub.is_none()});
  if !f.milestone.trim().is_empty() {
    p["milestone"] = json!(f.milestone);
  }
  if !linked {
    p["name"] = json!(f.name);
    p["project"] = json!(f.project);
    p["folder"] = json!(f.folder);
  }
  if let Some(r) = app.run("hub.save", p) {
    let v = r.get("version").and_then(Value::as_u64).unwrap_or(0);
    app.status = Some((format!("Saved {} v{v}", app.s.design.name), false));
    app.data.selected = r.get("item").and_then(Value::as_str).map(str::to_string);
    app.data.stale = true;
  }
}

/// Inserts a hub design into the open design as a linked component.
fn insert_component(app: &mut App, item: &str, version: Option<u32>) {
  if let Some(r) = app.run("component.insert", json!({"item": item, "version": version})) {
    let name = app.s.design.components.iter().find(|c| Some(c.id.0 as u64) == r["component"].as_u64());
    let name = name.map(|c| c.name.clone()).unwrap_or_default();
    app.status = Some((format!("Inserted {name} (v{})", r["version"]), false));
  }
}
