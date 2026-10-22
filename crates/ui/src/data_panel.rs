//! The data panel: projects, folders and designs with their version history, like Fusion's.
//!
//! As in Fusion, its home is the project list (My Recent Data, My Editable Documents, the
//! projects, the Assets library under LIBRARIES); a double-click opens a card. Inside a project
//! the Data tab lists its folders and designs (as a grid or a list, from the Settings gear) and
//! the People tab points at member management. The header is the hub drop-down, the capacity
//! counter, Refresh, Search (the header turns into a Search All Data bar) and Close.
//!
//! Everything goes through the engine's `hub.*` commands; the panel only caches their JSON
//! results (refreshed after each action and every few seconds, so saves from the CLI or another
//! window show up) and the thumbnails as textures.
//!
//! The hub can be the local one or a hub server (Connect to a hub server…): then what others
//! do shows up as it happens. A design's edit lock (Lock / Release / Steal lock) and who else
//! has it open are shown with its versions (a grid tile's version badge); the open design is
//! announced to the hub ([`heartbeat`]) so others see it too.

use std::collections::HashMap;
use std::path::PathBuf;

use egui::{Color32, RichText, Stroke, TextureHandle, vec2};
use serde_json::{Value, json};

use crate::{App, theme};

/// How often the listing is re-read while the panel is open (seconds).
const REFRESH_SECS: f64 = 5.0;
/// A list row's thumbnail.
const THUMB: f32 = 40.0;
/// A grid tile's thumbnail.
const TILE: f32 = 96.0;

/// The application bar button's tooltip, as Fusion's.
pub(crate) const BUTTON_TIP: &str = "Data Panel";

/// The hub drop-down's tooltip.
const HUB_TIP: &str = "FissionOxide Hub with Collaborative Editing";

/// Why the entries that need Autodesk's cloud are greyed.
const CLOUD_ONLY: &str = "Needs a Fusion Team cloud project; FissionOxide's hub doesn't offer it";

/// Why the offline cache entries are greyed.
const ALWAYS_LOCAL: &str = "FissionOxide's hub keeps every design on this computer: there is no offline cache";

/// Why Read-Only is greyed in a design's mode menu.
const ALWAYS_EDITABLE: &str = "Read-only documents are a Fusion licence limit: every FissionOxide design is editable";

/// The project the hub creates for a first save, and its card's description.
const DEFAULT_PROJECT: &str = "Default Project";
const DEFAULT_DESCRIPTION: &str =
  "This comes with FissionOxide and is where your work is stored until you create or join another project.";

/// What an empty project (or folder) says.
const EMPTY: &str = "Saved and Uploaded files will appear here";

/// The Upload window's area id (tests look it up by it).
pub(crate) const UPLOAD_ID: &str = "data-panel-upload";

/// A design's right-click menu, in Fusion's order; `false` entries need Autodesk's cloud and are
/// greyed.
pub(crate) const ITEM_MENU: [(&str, bool); 12] = [
  ("Open", true),
  ("Insert into Current Design", true),
  ("Create Drawing", true),
  ("Show in Location", true),
  ("Share Link", false),
  ("Import New Change", false),
  ("Rename", true),
  ("Move", true),
  ("Copy", true),
  ("Move to Trash", true),
  ("Add to Offline Cache", false),
  ("Create Version", false),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum View {
  /// The home: the project list.
  #[default]
  Projects,
  /// Inside a project.
  Project,
  /// My Recent Data.
  Recent,
  /// My Editable Documents.
  Editable,
}

/// A project's tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ProjectTab {
  #[default]
  Data,
  People,
}

/// The project list's ALL PROJECTS drop-down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ProjectFilter {
  #[default]
  All,
  Joined,
  Pinned,
  Owned,
  Shared,
}

impl ProjectFilter {
  const ALL: [ProjectFilter; 5] = [Self::All, Self::Joined, Self::Pinned, Self::Owned, Self::Shared];

  fn label(self) -> &'static str {
    match self {
      Self::All => "All Projects",
      Self::Joined => "Joined by Me",
      Self::Pinned => "Pinned",
      Self::Owned => "Owned by Me",
      Self::Shared => "Shared with Me",
    }
  }
}

/// The Settings gear's sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Sort {
  #[default]
  Updated,
  Name,
}

/// The tabs of a design's details card (a list row's expander).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CardTab {
  #[default]
  Uses,
  UsedIn,
  Drawings,
}

/// A card in the project list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Card {
  Recent,
  Editable,
  Project(String),
}

/// What the inline name editor renames.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Naming {
  Project(String),
  Folder(String),
  Item(String),
}

/// A name being edited in place (New Project, New Folder, Rename).
#[derive(Debug, Clone)]
struct InlineEdit {
  what: Naming,
  text: String,
  original: String,
  /// Frames shown: the first focuses it and selects the whole name.
  age: u8,
}

impl InlineEdit {
  fn new(what: Naming, name: &str) -> InlineEdit {
    InlineEdit { what, text: name.to_string(), original: name.to_string(), age: 0 }
  }
}

/// A one-line text prompt shown in the panel.
#[derive(Debug, Clone, PartialEq)]
enum Prompt {
  Milestone(String, u32, String),
  Comment(String, u32, String),
  /// A hub server's URL and access token.
  Connect(String, String),
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
  /// The name is selected the first time the window shows, as in Fusion.
  pub first: bool,
}

/// The Upload window: the files picked or dropped on it, and where they go.
#[derive(Debug, Clone, Default)]
pub struct UploadForm {
  pub files: Vec<PathBuf>,
  pub project: Option<String>,
  pub folder: Option<String>,
}

/// Move or Copy: the design and where it goes.
#[derive(Debug, Clone)]
struct Destination {
  item: String,
  name: String,
  copy: bool,
  project: Option<String>,
  folder: Option<String>,
  /// Where the design is now.
  from: (Option<String>, Option<String>),
  /// `hub.list` of the folder shown.
  listing: Value,
}

#[derive(Default)]
pub struct DataPanel {
  pub open: bool,
  view: View,
  tab: ProjectTab,
  project: Option<String>,
  folder: Option<String>,
  /// The project card a single click picked.
  picked: Option<Card>,
  filter: ProjectFilter,
  /// The Filter field under the project list.
  filter_text: String,
  /// The header is the search bar.
  searching: bool,
  focus_search: bool,
  search: String,
  sort: Sort,
  /// View as List (otherwise as Grid, Fusion's default).
  list: bool,
  /// The design a click selected.
  selected: Option<String>,
  /// The design whose details card is open (list view), and its tab.
  expanded: Option<String>,
  card_tab: CardTab,
  /// The details card's references, worked out when its tab shows.
  refs: Option<(String, CardTab, Vec<String>)>,
  /// The design whose version history shows under the list.
  history: Option<String>,
  projects: Vec<Value>,
  listing: Value,
  details: Value,
  other: Value,
  compare: Option<Value>,
  prompt: Option<Prompt>,
  edit: Option<InlineEdit>,
  pub save: Option<SaveForm>,
  pub upload: Option<UploadForm>,
  dest: Option<Destination>,
  /// Item id + version → texture (None = no thumbnail).
  thumbs: HashMap<(String, u64), Option<TextureHandle>>,
  stale: bool,
  last_refresh: f64,
  error: Option<String>,
  /// `hub.info`: where the hub is (a server?) and who we are on it.
  info: Value,
  /// The hub server news counter last seen ([`fox_engine::store::remote::generation`]).
  heard: u64,
  /// The design last announced as open here, and when.
  here: Option<(Option<String>, f64)>,
}

impl DataPanel {
  pub fn invalidate(&mut self) {
    self.stale = true;
  }

  pub fn toggle(&mut self) {
    self.open = !self.open;
    self.stale = true;
  }

  /// Where a new design is saved: the folder of the project last opened in the panel.
  pub fn location(&self) -> (Option<String>, Option<String>) {
    (self.project.clone(), self.folder.clone())
  }

  /// A modal window of the panel's (Upload, Move, Copy) is open.
  pub fn modal_open(&self) -> bool {
    self.upload.is_some() || self.dest.is_some()
  }

  /// Whether one of the panel's prompts (Mark milestone, Comment, Connect) shows.
  pub(crate) fn prompt_open(&self) -> bool {
    self.open && self.prompt.is_some()
  }

  /// Closes the prompt, as its Cancel does (Escape).
  pub(crate) fn close_prompt(&mut self) {
    self.prompt = None;
  }

  /// Shows the Connect to a hub server prompt (tests).
  #[cfg(test)]
  pub(crate) fn ask_connect(&mut self) {
    self.prompt = Some(Prompt::Connect("https://".into(), String::new()));
  }

  /// The panel takes the keys: a modal window is open, or a name is edited in place (egui drops
  /// the editor's focus on Escape before the frame, so Escape must reach the editor itself).
  pub fn takes_keys(&self) -> bool {
    self.modal_open() || self.edit.is_some()
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
  let forced = app.data.stale;
  let d = &mut app.data;
  d.stale = false;
  d.last_refresh = ctx.input(|i| i.time);
  let projects = hub(app, "hub.projects", json!({})).and_then(|v| v.as_array().cloned()).unwrap_or_default();
  app.data.info = app.s.execute("hub.info", &json!({})).unwrap_or(Value::Null);
  let d = &mut app.data;
  d.projects = projects;
  if d.project.as_ref().is_some_and(|p| !d.projects.iter().any(|x| s(x, "id") == *p)) {
    d.project = None;
    d.folder = None;
    if d.view == View::Project {
      d.view = View::Projects;
    }
  }
  if forced {
    d.refs = None;
  }
  let (project, folder, view, history) = (d.project.clone(), d.folder.clone(), d.view, d.history.clone());
  app.data.listing = match (&project, view) {
    (Some(p), View::Project) => hub(app, "hub.list", json!({"project": p, "folder": folder})).unwrap_or(Value::Null),
    _ => Value::Null,
  };
  app.data.other = if app.data.searching {
    // the list stays empty until a term is typed, as Fusion's
    let q = app.data.search.trim().to_string();
    if q.is_empty() { Value::Null } else { hub(app, "hub.search", json!({"query": q})).unwrap_or(Value::Null) }
  } else {
    match view {
      View::Recent => hub(app, "hub.recent", json!({"limit": 30})).unwrap_or(Value::Null),
      View::Editable => {
        let all =
          hub(app, "hub.recent", json!({"limit": 100_000})).and_then(|v| v.as_array().cloned()).unwrap_or_default();
        Value::Array(all.into_iter().filter(|i| s(i, "kind") == "design").collect())
      }
      _ => Value::Null,
    }
  };
  app.data.details = match &history {
    Some(i) => hub(app, "hub.versions", json!({"item": i})).unwrap_or(Value::Null),
    None => Value::Null,
  };
  if app.data.details.is_null() {
    app.data.history = None;
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
  let ctx = ui.ctx().clone();
  // File ▸ Upload… opens it with the panel closed too
  upload_window(app, &ctx);
  if !app.data.open {
    return;
  }
  let now = ctx.input(|i| i.time);
  // a hub server said something changed
  let heard = fox_engine::store::remote::generation();
  if heard != app.data.heard {
    app.data.heard = heard;
    app.data.stale = true;
  }
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
      header(app, ui);
      ui.separator();
      if app.data.searching {
        search_results(app, ui);
      } else {
        match app.data.view {
          View::Projects => project_list(app, ui),
          View::Project => project_view(app, ui),
          View::Recent | View::Editable => pseudo_project(app, ui),
        }
      }
      if let Some(e) = app.data.error.clone() {
        ui.colored_label(theme::ERROR, e);
      }
      history(app, ui);
    });
  prompt_window(app, &ctx);
  compare_window(app, &ctx);
  destination_window(app, &ctx);
}

/// The header row: the hub drop-down, the capacity counter, Refresh, Search and Close; or, while
/// searching, the search bar.
fn header(app: &mut App, ui: &mut egui::Ui) {
  if app.data.searching {
    search_bar(app, ui);
    return;
  }
  ui.horizontal(|ui| {
    hub_menu(app, ui);
    ui.add_space(6.0);
    let n = app.data.info.get("items").and_then(Value::as_u64).unwrap_or(0);
    // FissionOxide sets no limit on the designs a hub holds
    ui.label(RichText::new(format!("🗋 {n} of ∞")).small().color(theme::GROUP_LABEL)).on_hover_text("Capacity");
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
      if ui.small_button("×").on_hover_text("Close the data panel").clicked() {
        app.data.open = false;
      }
      if ui.small_button("🔍").on_hover_text("Search").clicked() {
        app.data.searching = true;
        app.data.focus_search = true;
        app.data.stale = true;
      }
      if ui.small_button("⟳").on_hover_text("Refresh").clicked() {
        app.data.stale = true;
      }
    });
  });
}

/// Search All Data: "←" leaves it (Escape doesn't), "×" clears the term.
fn search_bar(app: &mut App, ui: &mut egui::Ui) {
  ui.horizontal(|ui| {
    if ui.small_button("←").on_hover_text("Back").clicked() {
      app.data.searching = false;
      app.data.search.clear();
      app.data.stale = true;
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
      if ui.small_button("×").on_hover_text("Clear").clicked() {
        app.data.search.clear();
        app.data.stale = true;
      }
      ui.label("🔍");
      let r = ui.add(
        egui::TextEdit::singleline(&mut app.data.search).hint_text("Search All Data").desired_width(f32::INFINITY),
      );
      if app.data.focus_search {
        app.data.focus_search = false;
        r.request_focus();
      }
      if r.changed() {
        app.data.stale = true;
      }
    });
  });
}

/// The hub drop-down: the hub's name (the local one is named after you, as a Fusion personal
/// hub is), connecting to a hub server and going back to the local hub.
fn hub_menu(app: &mut App, ui: &mut egui::Ui) {
  let server = app.data.info.get("server").and_then(Value::as_str).map(str::to_string);
  let name = match &server {
    Some(url) => url.split_once("://").map(|(_, h)| h).unwrap_or(url).to_string(),
    None => Some(s(&app.data.info, "user")).filter(|u| !u.is_empty()).unwrap_or_else(fox_engine::store::user),
  };
  let tip = match &server {
    Some(url) => {
      let live = if fox_engine::store::remote::is_live(std::path::Path::new(url)) {
        "live: changes and presence arrive as they happen"
      } else {
        "offline: re-read every few seconds"
      };
      format!("{HUB_TIP}\n{url} as {}, {live}", s(&app.data.info, "user"))
    }
    None => HUB_TIP.to_string(),
  };
  let r = ui.menu_button(RichText::new(format!("{name} ▾")).strong(), |ui| {
    ui.label(RichText::new(format!("✔ {name}")).strong());
    ui.separator();
    if ui.button("Connect to a hub server…").clicked() {
      let url = server.clone().unwrap_or_else(|| "https://".into());
      app.data.prompt = Some(Prompt::Connect(url, String::new()));
      ui.close();
    }
    if server.is_some() && ui.button("Use the local hub").clicked() {
      hub(app, "hub.disconnect", json!({}));
      reset_location(app);
      ui.close();
    }
  });
  r.response.on_hover_text(tip);
}

fn reset_location(app: &mut App) {
  let d = &mut app.data;
  d.view = View::Projects;
  d.project = None;
  d.folder = None;
  d.picked = None;
  d.selected = None;
  d.expanded = None;
  d.history = None;
  d.thumbs.clear();
  d.here = None;
  d.stale = true;
}

/// Tells the hub which design is open here (on change, and every [`HEARTBEAT`] while one is),
/// so others see it in their data panel.
///
/// [`HEARTBEAT`]: fox_engine::store::presence::HEARTBEAT
pub fn heartbeat(app: &mut App, ctx: &egui::Context) {
  let now = ctx.input(|i| i.time);
  let open = app.s.hub.as_ref().map(|h| h.item.clone());
  let every = fox_engine::store::presence::HEARTBEAT.as_secs_f64();
  let due = match &app.data.here {
    None => open.is_some(),
    Some((last, t)) => *last != open || (open.is_some() && now - t > every),
  };
  if due {
    let items: Vec<String> = open.iter().cloned().collect();
    if let Err(e) = app.s.execute("hub.here", &json!({"items": items})) {
      log::debug!("couldn't announce the open design: {}", e.0);
    }
    app.data.here = Some((open.clone(), now));
  }
  if open.is_some() {
    ctx.request_repaint_after(fox_engine::store::presence::HEARTBEAT);
  }
}

/// The project list's cards for a filter and a Filter term: My Recent Data and My Editable
/// Documents (under All Projects), then the projects. A local hub has nothing pinned or shared;
/// a hub server's projects are all shared.
pub(crate) fn project_cards(
  projects: &[Value],
  filter: ProjectFilter,
  text: &str,
  shared: bool,
) -> Vec<(Card, String, String)> {
  let mut out = Vec::new();
  if filter == ProjectFilter::All {
    out.push((Card::Recent, "My Recent Data".into(), "A list of what you've recently been working on".into()));
    out.push((Card::Editable, "My Editable Documents".into(), "A list of your editable documents".into()));
  }
  let show = match filter {
    ProjectFilter::Pinned => false,
    ProjectFilter::Shared => shared,
    _ => true,
  };
  if show {
    for p in projects {
      let name = s(p, "name");
      let desc = if name == DEFAULT_PROJECT { DEFAULT_DESCRIPTION.to_string() } else { String::new() };
      out.push((Card::Project(s(p, "id")), name, desc));
    }
  }
  let t = text.trim().to_lowercase();
  out.retain(|(_, n, _)| t.is_empty() || n.to_lowercase().contains(&t));
  out
}

/// A white card with a border that turns blue when picked; the whole card takes clicks, under
/// whatever it holds.
fn card_frame(ui: &mut egui::Ui, picked: bool, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
  let stroke = if picked { Stroke::new(1.5, theme::ACCENT) } else { Stroke::new(1.0, theme::BORDER) };
  ui.scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
    egui::Frame::NONE.fill(Color32::WHITE).stroke(stroke).corner_radius(4.0).inner_margin(egui::Margin::same(6)).show(
      ui,
      |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(add);
      },
    );
  })
  .response
}

/// A card's icon: a glyph on a grey square.
fn icon_box(ui: &mut egui::Ui, glyph: &str, color: Color32) {
  let (rect, _) = ui.allocate_exact_size(vec2(32.0, 32.0), egui::Sense::hover());
  ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(232, 235, 240));
  ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(17.0), color);
}

/// Fusion's blue call-to-action button (New Project, Upload, Select Files).
fn blue_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
  ui.add(
    egui::Button::new(RichText::new(label).color(Color32::WHITE).strong())
      .fill(theme::ACCENT)
      .stroke(Stroke::NONE)
      .corner_radius(3),
  )
}

/// `base`, or `base (2)`, `base (3)`… whichever no sibling has yet.
fn unique_name(base: &str, taken: &[String]) -> String {
  let free = |n: &str| !taken.iter().any(|t| t.eq_ignore_ascii_case(n));
  if free(base) {
    return base.to_string();
  }
  (2..).map(|k| format!("{base} ({k})")).find(|n| free(n)).unwrap_or_default()
}

/// The home: the project list.
fn project_list(app: &mut App, ui: &mut egui::Ui) {
  ui.horizontal(|ui| {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
      // back to the project last opened
      if ui.add_enabled(app.data.project.is_some(), egui::Link::new("Back ›")).clicked() {
        app.data.view = View::Project;
        app.data.stale = true;
      }
    });
  });
  ui.horizontal(|ui| {
    let label =
      RichText::new(format!("{} ▾", app.data.filter.label().to_uppercase())).small().strong().color(theme::GROUP_LABEL);
    ui.menu_button(label, |ui| {
      for f in ProjectFilter::ALL {
        if ui.selectable_label(app.data.filter == f, f.label()).clicked() {
          app.data.filter = f;
          ui.close();
        }
      }
    });
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
      if blue_button(ui, "New Project").clicked() {
        new_project(app);
      }
    });
  });
  ui.add_space(4.0);
  let shared = app.data.info.get("server").is_some_and(|v| !v.is_null());
  let cards = project_cards(&app.data.projects, app.data.filter, &app.data.filter_text, shared);
  egui::ScrollArea::vertical()
    .id_salt("hub-projects")
    .max_height((ui.available_height() - 110.0).max(80.0))
    .auto_shrink([false, true])
    .show(ui, |ui| {
      for (card, name, desc) in cards {
        let picked = app.data.picked.as_ref() == Some(&card);
        let naming = match &card {
          Card::Project(id) => Some(Naming::Project(id.clone())),
          _ => None,
        };
        let (glyph, color) = match card {
          Card::Recent => ("🕘", theme::ACCENT),
          Card::Editable => ("✏", theme::ACCENT),
          Card::Project(_) => ("📁", theme::GROUP_LABEL),
        };
        let r = card_frame(ui, picked, |ui| {
          icon_box(ui, glyph, color);
          ui.vertical(|ui| {
            if !naming.as_ref().is_some_and(|n| inline_editor(app, ui, n)) {
              ui.label(RichText::new(&name).strong());
            }
            if !desc.is_empty() {
              ui.add(egui::Label::new(RichText::new(&desc).small().color(theme::MUTED)).wrap());
            }
          });
          if shared && naming.is_some() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
              ui.label(RichText::new("☁").color(theme::ACCENT)).on_hover_text("Team project on the hub server");
            });
          }
        });
        if r.clicked() {
          app.data.picked = Some(card.clone());
        }
        if r.double_clicked() {
          open_card(app, &card);
        }
        if let Card::Project(id) = &card {
          r.context_menu(|ui| {
            if ui.button("Rename").clicked() {
              app.data.edit = Some(InlineEdit::new(Naming::Project(id.clone()), &name));
              ui.close();
            }
            if ui.button("Delete project").clicked() {
              hub(app, "hub.project.delete", json!({"project": id}));
              app.data.stale = true;
              ui.close();
            }
          });
        }
        ui.add_space(4.0);
      }
    });
  ui.add_space(6.0);
  ui.label(RichText::new("LIBRARIES").small().strong().color(theme::GROUP_LABEL));
  // Fusion's Assets library lives on Autodesk's cloud
  card_frame(ui, false, |ui| {
    icon_box(ui, "📦", theme::MUTED);
    ui.label(RichText::new("Assets").strong().color(theme::MUTED));
  })
  .on_hover_text(CLOUD_ONLY);
  ui.add_space(6.0);
  ui.add(egui::TextEdit::singleline(&mut app.data.filter_text).hint_text("Filter").desired_width(f32::INFINITY));
}

/// New Project: a project named "New Project" at once, its name selected in an inline editor.
fn new_project(app: &mut App) {
  let taken: Vec<String> = app.data.projects.iter().map(|p| s(p, "name")).collect();
  let name = unique_name("New Project", &taken);
  if let Some(v) = hub(app, "hub.project.create", json!({"name": name})) {
    let id = s(&v, "project");
    let d = &mut app.data;
    d.filter = ProjectFilter::All;
    d.filter_text.clear();
    d.picked = Some(Card::Project(id.clone()));
    d.edit = Some(InlineEdit::new(Naming::Project(id), &name));
    d.stale = true;
  }
}

/// New Folder: a folder named "New Folder" at once, its name selected in an inline editor.
pub(crate) fn new_folder(app: &mut App) {
  let Some(project) = app.data.project.clone() else { return };
  let taken: Vec<String> =
    app.data.listing.get("folders").and_then(Value::as_array).into_iter().flatten().map(|f| s(f, "name")).collect();
  let name = unique_name("New Folder", &taken);
  let folder = app.data.folder.clone();
  if let Some(v) = hub(app, "hub.folder.create", json!({"project": project, "parent": folder, "name": name})) {
    app.data.edit = Some(InlineEdit::new(Naming::Folder(s(&v, "folder")), &name));
    app.data.stale = true;
  }
}

/// A double-click on a project list card.
fn open_card(app: &mut App, card: &Card) {
  let d = &mut app.data;
  d.picked = Some(card.clone());
  d.selected = None;
  d.expanded = None;
  d.edit = None;
  match card {
    Card::Recent => d.view = View::Recent,
    Card::Editable => d.view = View::Editable,
    Card::Project(id) => {
      d.project = Some(id.clone());
      d.folder = None;
      d.view = View::Project;
      d.tab = ProjectTab::Data;
    }
  }
  d.stale = true;
}

/// The breadcrumb's home: back to the project list.
fn go_home(app: &mut App) {
  let d = &mut app.data;
  d.view = View::Projects;
  d.selected = None;
  d.expanded = None;
  d.edit = None;
  d.stale = true;
}

/// The inline name editor, if `what` is being renamed: true when it took the name's place.
/// Enter or a click elsewhere keeps the typed name, Escape the old one.
fn inline_editor(app: &mut App, ui: &mut egui::Ui, what: &Naming) -> bool {
  let Some(e) = app.data.edit.as_mut().filter(|e| e.what == *what) else { return false };
  let id = egui::Id::new(("data-panel-rename", format!("{what:?}")));
  let out =
    egui::TextEdit::singleline(&mut e.text).id(id).desired_width(ui.available_width().clamp(60.0, 220.0)).show(ui);
  if e.age == 0 {
    out.response.request_focus();
    let mut state = out.state.clone();
    let all =
      egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(e.text.chars().count()));
    state.cursor.set_char_range(Some(all));
    state.store(ui.ctx(), id);
  }
  e.age = e.age.saturating_add(1);
  if e.age > 2 && (out.response.lost_focus() || !out.response.has_focus()) {
    let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
    finish_edit(app, cancel);
  }
  true
}

/// Ends the inline name editor: the typed name is applied unless cancelled.
fn finish_edit(app: &mut App, cancel: bool) {
  let Some(e) = app.data.edit.take() else { return };
  let name = e.text.trim().to_string();
  if cancel || name.is_empty() || name == e.original {
    return;
  }
  let _ = match &e.what {
    Naming::Project(id) => hub(app, "hub.project.rename", json!({"project": id, "name": name})),
    Naming::Folder(id) => hub(app, "hub.folder.rename", json!({"folder": id, "name": name})),
    Naming::Item(id) => hub(app, "hub.item.rename", json!({"item": id, "name": name})),
  };
  app.data.stale = true;
}

/// Inside a project: Data | People.
fn project_view(app: &mut App, ui: &mut egui::Ui) {
  if app.data.project.is_none() {
    go_home(app);
    return;
  }
  ui.horizontal(|ui| {
    for (t, label) in [(ProjectTab::Data, "Data"), (ProjectTab::People, "People")] {
      if ui.selectable_label(app.data.tab == t, RichText::new(label).strong()).clicked() {
        app.data.tab = t;
      }
    }
  });
  ui.separator();
  if app.data.tab == ProjectTab::People {
    people_tab(ui);
    return;
  }
  ui.horizontal(|ui| {
    if blue_button(ui, "Upload").clicked() {
      upload_dialog(app);
    }
    if ui.button("New Folder").clicked() {
      new_folder(app);
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| settings_menu(app, ui));
  });
  breadcrumb(app, ui, None);
  let mut folders = app.data.listing.get("folders").and_then(Value::as_array).cloned().unwrap_or_default();
  let mut items = app.data.listing.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
  sort_folders(&mut folders, app.data.sort);
  sort_items(&mut items, app.data.sort);
  egui::ScrollArea::vertical()
    .id_salt("hub-files")
    .max_height(ui.available_height() * 0.6)
    .auto_shrink([false, true])
    .show(ui, |ui| {
      if folders.is_empty() && items.is_empty() {
        empty_text(ui);
        return;
      }
      for f in &folders {
        folder_row(app, ui, f);
      }
      items_view(app, ui, items, false);
    });
}

fn empty_text(ui: &mut egui::Ui) {
  ui.add_space(24.0);
  ui.vertical_centered(|ui| ui.label(RichText::new(EMPTY).color(theme::MUTED)));
}

/// The People tab: members are managed on Fusion's web client.
fn people_tab(ui: &mut egui::Ui) {
  ui.add_space(12.0);
  ui.vertical_centered(|ui| {
    // three people
    let (rect, _) = ui.allocate_exact_size(vec2(120.0, 64.0), egui::Sense::hover());
    let p = ui.painter();
    for (dx, r, c) in [(-30.0, 9.0, theme::SELECTED), (30.0, 9.0, theme::SELECTED), (0.0, 12.0, theme::ACCENT)] {
      let head = rect.center() + vec2(dx, -12.0 + (12.0 - r));
      p.circle_filled(head, r, c);
      let body = egui::Rect::from_center_size(head + vec2(0.0, r + 14.0), vec2(r * 3.0, r * 2.0));
      p.rect_filled(body, egui::CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 }, c);
    }
  });
  ui.add_space(12.0);
  egui::Frame::NONE
    .fill(Color32::WHITE)
    .stroke(Stroke::new(1.0, theme::BORDER))
    .corner_radius(4.0)
    .inner_margin(egui::Margin::same(10))
    .show(ui, |ui| {
      ui.set_width(ui.available_width());
      ui.label(RichText::new("Manage members").strong());
      ui.add(
        egui::Label::new(
          RichText::new("View and add members to this folder with the FissionOxide web client.")
            .small()
            .color(theme::MUTED),
        )
        .wrap(),
      );
    });
}

/// The Settings gear: sort order, grid or list, the offline cache.
fn settings_menu(app: &mut App, ui: &mut egui::Ui) {
  let r = crate::icons::icon_button(ui, crate::icons::Icon::Gear, 22.0, "Settings", false, None);
  egui::Popup::menu(&r).show(|ui| {
    ui.radio_value(&mut app.data.sort, Sort::Updated, "Sort by Last Updated");
    ui.radio_value(&mut app.data.sort, Sort::Name, "Sort by Name");
    ui.separator();
    ui.radio_value(&mut app.data.list, false, "View as Grid");
    ui.radio_value(&mut app.data.list, true, "View as List");
    ui.separator();
    ui.add_enabled(false, egui::Button::new("View Offline Cache")).on_disabled_hover_text(ALWAYS_LOCAL);
    ui.add_enabled(false, egui::Button::new("Cache This Project")).on_disabled_hover_text(ALWAYS_LOCAL);
  });
}

/// "🏠 › Project › folder…", or "🏠 › My Recent Data", with Open on the Web at the right.
fn breadcrumb(app: &mut App, ui: &mut egui::Ui, title: Option<&str>) {
  ui.horizontal(|ui| {
    if ui.small_button("🏠").on_hover_text("Return to project list").clicked() {
      go_home(app);
    }
    ui.label(RichText::new("›").color(theme::MUTED));
    if let Some(t) = title {
      ui.label(RichText::new(t).strong());
      return;
    }
    let pname =
      app.data.projects.iter().find(|p| Some(s(p, "id")) == app.data.project).map(|p| s(p, "name")).unwrap_or_default();
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
      ui.add_enabled(false, egui::Button::new("🌐").small())
        .on_disabled_hover_text(format!("Open on the Web\n{CLOUD_ONLY}"));
      ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
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
    });
  });
}

/// My Recent Data and My Editable Documents: designs from every project.
fn pseudo_project(app: &mut App, ui: &mut egui::Ui) {
  let recent = app.data.view == View::Recent;
  breadcrumb(app, ui, Some(if recent { "My Recent Data" } else { "My Editable Documents" }));
  let mut items = app.data.other.as_array().cloned().unwrap_or_default();
  if !recent {
    sort_items(&mut items, app.data.sort);
  }
  egui::ScrollArea::vertical()
    .id_salt("hub-items")
    .max_height(ui.available_height() * 0.6)
    .auto_shrink([false, true])
    .show(ui, |ui| {
      if items.is_empty() {
        empty_text(ui);
      }
      items_view(app, ui, items, true);
    });
}

/// What Search All Data found (nothing until a term is typed).
fn search_results(app: &mut App, ui: &mut egui::Ui) {
  if app.data.search.trim().is_empty() {
    return;
  }
  let found = app.data.other.as_array().cloned().unwrap_or_default();
  egui::ScrollArea::vertical()
    .id_salt("hub-found")
    .max_height(ui.available_height() * 0.6)
    .auto_shrink([false, true])
    .show(ui, |ui| {
      if found.is_empty() {
        ui.label(RichText::new("No results found").small().color(theme::MUTED));
      }
      items_view(app, ui, found, true);
    });
}

pub(crate) fn sort_items(items: &mut [Value], sort: Sort) {
  match sort {
    Sort::Updated => items.sort_by_key(|i| std::cmp::Reverse(i.get("modified").and_then(Value::as_u64).unwrap_or(0))),
    Sort::Name => items.sort_by_key(|i| s(i, "name").to_lowercase()),
  }
}

fn sort_folders(folders: &mut [Value], sort: Sort) {
  match sort {
    Sort::Updated => folders.sort_by_key(|f| std::cmp::Reverse(f.get("created").and_then(Value::as_u64).unwrap_or(0))),
    Sort::Name => folders.sort_by_key(|f| s(f, "name").to_lowercase()),
  }
}

fn folder_row(app: &mut App, ui: &mut egui::Ui, f: &Value) {
  let id = s(f, "id");
  let naming = Naming::Folder(id.clone());
  let r = ui
    .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
      ui.set_width(ui.available_width());
      ui.horizontal(|ui| {
        ui.label(RichText::new("📁").size(18.0).color(theme::GROUP_LABEL));
        if !inline_editor(app, ui, &naming) {
          ui.label(RichText::new(s(f, "name")).strong());
        }
      });
    })
    .response;
  if r.clicked() && app.data.edit.is_none() {
    app.data.folder = Some(id.clone());
    app.data.selected = None;
    app.data.stale = true;
  }
  r.context_menu(|ui| {
    if ui.button("Rename").clicked() {
      app.data.edit = Some(InlineEdit::new(naming.clone(), &s(f, "name")));
      ui.close();
    }
    if ui.button("Delete (if empty)").clicked() {
      hub(app, "hub.folder.delete", json!({"folder": id}));
      app.data.stale = true;
      ui.close();
    }
  });
}

fn items_view(app: &mut App, ui: &mut egui::Ui, items: Vec<Value>, show_path: bool) {
  if app.data.list {
    for it in &items {
      list_row(app, ui, it, show_path);
    }
  } else {
    ui.horizontal_wrapped(|ui| {
      for it in &items {
        tile(app, ui, it);
      }
    });
  }
}

/// Fusion's "12/15/2025 1:34 AM" (UTC, as the hub's other times).
pub(crate) fn us_time(t: u64) -> String {
  let f = fox_engine::store::format_time(t);
  let num = |a: usize, b: usize| f.get(a..b).and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
  let (y, m, d, h, min) = (num(0, 4), num(5, 7), num(8, 10), num(11, 13), num(14, 16));
  let (h12, half) = match h {
    0 => (12, "AM"),
    1..=11 => (h, "AM"),
    12 => (12, "PM"),
    _ => (h - 12, "PM"),
  };
  format!("{m}/{d}/{y} {h12}:{min:02} {half}")
}

fn updated(it: &Value) -> String {
  us_time(it.get("modified").and_then(Value::as_u64).unwrap_or(0))
}

/// The orange badge of a design (grey for an uploaded file): a white cube on a rounded square.
fn kind_badge(ui: &mut egui::Ui, design: bool) {
  let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
  let p = ui.painter();
  p.rect_filled(rect, 2.0, if design { theme::ICON_ORANGE } else { theme::MUTED });
  let c = rect.center();
  let w = Stroke::new(1.0, Color32::WHITE);
  let hex: Vec<egui::Pos2> = (0..6)
    .map(|k| c + 4.2 * egui::Vec2::angled(std::f32::consts::FRAC_PI_3 * k as f32 + std::f32::consts::FRAC_PI_6))
    .collect();
  p.add(egui::Shape::closed_line(hex.clone(), w));
  for k in [1, 3, 5] {
    p.line_segment([c, hex[k]], w);
  }
}

/// The badge, the bold name (or its inline editor) and the lock.
fn name_line(app: &mut App, ui: &mut egui::Ui, it: &Value) {
  let id = s(it, "id");
  let open_here = app.s.hub.as_ref().is_some_and(|h| h.item == id);
  ui.horizontal(|ui| {
    kind_badge(ui, s(it, "kind") == "design");
    if inline_editor(app, ui, &Naming::Item(id.clone())) {
      return;
    }
    let mut title = RichText::new(s(it, "name")).strong();
    if open_here {
      title = title.color(theme::ACCENT);
    }
    ui.add(egui::Label::new(title).truncate());
    if let Some(l) = it.get("lock").filter(|l| !l.is_null()) {
      let who = if l.get("mine") == Some(&json!(true)) { "you".to_string() } else { s(l, "user") };
      ui.label(RichText::new("🔒").small().color(theme::WARNING))
        .on_hover_text(format!("Locked for editing by {who} since {}", s(l, "since_time")));
    }
  });
}

/// What a click, a double-click and a right-click on a design do.
fn item_clicks(app: &mut App, r: &egui::Response, it: &Value) {
  let id = s(it, "id");
  if r.clicked() {
    app.data.selected = Some(id.clone());
  }
  if r.double_clicked() {
    app.open_hub_item(&id, None);
  }
  r.context_menu(|ui| item_menu(app, ui, it));
}

/// A grid tile: the thumbnail with its version badge (the version history), the name, the date.
fn tile(app: &mut App, ui: &mut egui::Ui, it: &Value) {
  let id = s(it, "id");
  let selected = app.data.selected.as_deref() == Some(id.as_str());
  let latest = it.pointer("/latest/number").and_then(Value::as_u64);
  let stroke = if selected { Stroke::new(1.5, theme::ACCENT) } else { Stroke::new(1.0, theme::BORDER) };
  let r = ui
    .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
      egui::Frame::NONE
        .fill(Color32::WHITE)
        .stroke(stroke)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::same(4))
        .show(ui, |ui| {
          ui.set_width(TILE);
          ui.vertical(|ui| {
            let t = thumb(ui, app, &id, latest, TILE);
            if let Some(n) = latest {
              let badge = egui::Rect::from_min_size(t.rect.right_bottom() - vec2(30.0, 18.0), vec2(28.0, 16.0));
              let b = ui.interact(badge, ui.id().with(("version-badge", &id)), egui::Sense::click());
              let fill = if b.hovered() { theme::ACCENT } else { theme::GROUP_LABEL };
              ui.painter().rect_filled(badge, 3.0, fill);
              ui.painter().text(
                badge.center(),
                egui::Align2::CENTER_CENTER,
                format!("V{n}"),
                egui::FontId::proportional(10.0),
                Color32::WHITE,
              );
              if b.on_hover_text("Version history").clicked() {
                app.data.history = Some(id.clone());
                app.data.stale = true;
              }
            }
            name_line(app, ui, it);
            ui.label(RichText::new(updated(it)).small().color(theme::MUTED));
          });
        });
    })
    .response;
  item_clicks(app, &r, it);
}

/// A list row: thumbnail, badge and bold name, the date; at the right the mode drop-down and
/// the expander of the details card.
fn list_row(app: &mut App, ui: &mut egui::Ui, it: &Value, show_path: bool) {
  let id = s(it, "id");
  let selected = app.data.selected.as_deref() == Some(id.as_str());
  let expanded = app.data.expanded.as_deref() == Some(id.as_str());
  let latest = it.pointer("/latest/number").and_then(Value::as_u64);
  let fill = if selected { Color32::from_rgb(214, 232, 250) } else { Color32::TRANSPARENT };
  let r = ui
    .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
      egui::Frame::NONE.fill(fill).corner_radius(4.0).inner_margin(egui::Margin::same(3)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
          thumb(ui, app, &id, latest, THUMB);
          ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.vertical(|ui| {
              ui.menu_button(RichText::new("▾").small(), |ui| {
                ui.label("✔ Editable");
                ui.add_enabled(false, egui::Button::new("    Read-Only").frame(false))
                  .on_disabled_hover_text(ALWAYS_EDITABLE);
              });
              if ui.small_button(if expanded { "▴" } else { "⌄" }).on_hover_text("Details").clicked() {
                app.data.expanded = if expanded { None } else { Some(id.clone()) };
              }
            });
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
              ui.vertical(|ui| {
                name_line(app, ui, it);
                ui.label(RichText::new(updated(it)).small().color(theme::MUTED));
                if show_path {
                  ui.label(RichText::new(location_text(it, " › ")).small().color(theme::MUTED));
                }
              });
            });
          });
        });
      });
    })
    .response;
  item_clicks(app, &r, it);
  if expanded {
    details_card(app, ui, it);
  }
}

/// "Project › folder › folder".
fn location_text(it: &Value, sep: &str) -> String {
  let mut path = vec![s(it, "project_name")];
  path.extend(
    it.get("path").and_then(Value::as_array).into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)),
  );
  path.join(sep)
}

/// A list row's details card: what it is, when and by whom it was updated, where it's stored,
/// and its Uses | Used In | Drawings.
fn details_card(app: &mut App, ui: &mut egui::Ui, it: &Value) {
  let id = s(it, "id");
  let design = s(it, "kind") == "design";
  egui::Frame::NONE
    .fill(Color32::WHITE)
    .stroke(Stroke::new(1.0, theme::BORDER))
    .corner_radius(4.0)
    .inner_margin(egui::Margin::same(8))
    .show(ui, |ui| {
      ui.set_width(ui.available_width());
      ui.label(RichText::new(s(it, "name")).strong());
      let what =
        if design { "FissionOxide Design".to_string() } else { format!("{} File", s(it, "ext").to_uppercase()) };
      ui.label(RichText::new(what).small().color(theme::MUTED));
      ui.add_enabled(false, egui::Link::new("View Details on Web")).on_disabled_hover_text(CLOUD_ONLY);
      let author = it.pointer("/latest/author").and_then(Value::as_str).unwrap_or_default();
      ui.label(RichText::new(format!("Last updated {} by {author}", updated(it))).small());
      ui.label(RichText::new(format!("Stored in {}", location_text(it, " > "))).small());
      ui.add_space(4.0);
      ui.horizontal(|ui| {
        for (t, label) in [(CardTab::Uses, "Uses"), (CardTab::UsedIn, "Used In"), (CardTab::Drawings, "Drawings")] {
          if ui.selectable_label(app.data.card_tab == t, label).clicked() {
            app.data.card_tab = t;
          }
        }
      });
      ui.separator();
      let tab = app.data.card_tab;
      let names = references(app, &id, tab);
      if names.is_empty() {
        let none = match tab {
          CardTab::Uses => "This item does not reference any other designs.",
          CardTab::UsedIn => "This item is not referenced by any other designs.",
          CardTab::Drawings => "This item has no drawings.",
        };
        ui.label(RichText::new(none).small().color(theme::MUTED));
      }
      for n in names {
        ui.label(RichText::new(format!("📄 {n}")).small());
      }
    });
}

/// A details card tab's names, cached until the hub changes.
fn references(app: &mut App, item: &str, tab: CardTab) -> Vec<String> {
  if let Some((i, t, v)) = &app.data.refs
    && i == item
    && *t == tab
  {
    return v.clone();
  }
  let v = work_out_references(&app.s, item, tab);
  app.data.refs = Some((item.to_string(), tab, v.clone()));
  v
}

/// The designs `item` inserts (Uses), the designs that insert it (Used In), or its drawings.
fn work_out_references(session: &fox_engine::Session, item: &str, tab: CardTab) -> Vec<String> {
  let Ok(st) = fox_engine::hub::open_store(session) else { return Vec::new() };
  let read = |id: &str| st.read_version(id, None).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
  match tab {
    CardTab::Uses => {
      let Some(d) = read(item) else { return Vec::new() };
      let mut ids = Vec::new();
      hub_sources(&d, &mut ids);
      let mut names: Vec<String> = Vec::new();
      for i in ids {
        if let Ok(x) = st.item(&i)
          && !names.contains(&x.name)
        {
          names.push(x.name.clone());
        }
      }
      names
    }
    CardTab::UsedIn => st
      .recent(usize::MAX)
      .into_iter()
      .filter(|i| i.id != item && i.ext == "fox")
      .filter(|i| {
        read(&i.id).is_some_and(|d| {
          let mut ids = Vec::new();
          hub_sources(&d, &mut ids);
          ids.iter().any(|x| x == item)
        })
      })
      .map(|i| i.name.clone())
      .collect(),
    CardTab::Drawings => read(item)
      .and_then(|d| d.get("drawings").and_then(Value::as_array).cloned())
      .unwrap_or_default()
      .iter()
      .enumerate()
      .map(|(k, d)| {
        d.get("name").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("Drawing {}", k + 1))
      })
      .collect(),
  }
}

/// The hub items a design's JSON inserts: every `{"source": {"item", "version"}}` in it.
fn hub_sources(v: &Value, out: &mut Vec<String>) {
  match v {
    Value::Object(m) => {
      if let Some(src) = m.get("source")
        && src.get("version").is_some()
        && let Some(i) = src.get("item").and_then(Value::as_str)
      {
        out.push(i.to_string());
      }
      m.values().for_each(|x| hub_sources(x, out));
    }
    Value::Array(a) => a.iter().for_each(|x| hub_sources(x, out)),
    _ => {}
  }
}

/// A design's right-click menu (Fusion's entries; a file has Download… too).
fn item_menu(app: &mut App, ui: &mut egui::Ui, it: &Value) {
  ui.set_min_width(200.0);
  let design = s(it, "kind") == "design";
  for (label, on) in ITEM_MENU {
    let design_only = matches!(label, "Insert into Current Design" | "Create Drawing");
    let enabled = on && (design || !design_only);
    let r = ui.add_enabled(enabled, egui::Button::new(label));
    let r = if !on {
      r.on_disabled_hover_text(CLOUD_ONLY)
    } else if !enabled {
      r.on_disabled_hover_text("Only a design can be used this way")
    } else {
      r
    };
    if r.clicked() {
      ui.close();
      item_action(app, label, it);
    }
  }
  if !design && ui.button("Download…").clicked() {
    ui.close();
    download_dialog(app, &s(it, "id"), None, &s(it, "name"), &s(it, "ext"));
  }
}

/// Runs a design menu entry.
pub(crate) fn item_action(app: &mut App, label: &str, it: &Value) {
  let id = s(it, "id");
  let is_open = |app: &App| app.s.hub.as_ref().is_some_and(|h| h.item == id);
  match label {
    "Open" => app.open_hub_item(&id, None),
    "Insert into Current Design" => insert_component(app, &id, None),
    "Create Drawing" => {
      if !is_open(app) {
        app.open_hub_item(&id, None);
      }
      if is_open(app) {
        app.start(crate::dialogs::Kind::NewDrawing);
      }
    }
    "Show in Location" => {
      let d = &mut app.data;
      d.project = Some(s(it, "project"));
      d.folder = it.get("folder").and_then(Value::as_str).map(str::to_string);
      d.view = View::Project;
      d.tab = ProjectTab::Data;
      d.searching = false;
      d.search.clear();
      d.selected = Some(id);
      d.stale = true;
    }
    "Rename" => app.data.edit = Some(InlineEdit::new(Naming::Item(id), &s(it, "name"))),
    "Move" | "Copy" => {
      let project = Some(s(it, "project"));
      let folder = it.get("folder").and_then(Value::as_str).map(str::to_string);
      app.data.dest = Some(Destination {
        item: id,
        name: s(it, "name"),
        copy: label == "Copy",
        from: (project.clone(), folder.clone()),
        project,
        folder,
        listing: Value::Null,
      });
    }
    "Move to Trash" => {
      hub(app, "hub.item.delete", json!({"item": id}));
      let d = &mut app.data;
      for slot in [&mut d.selected, &mut d.expanded, &mut d.history] {
        if slot.as_deref() == Some(id.as_str()) {
          *slot = None;
        }
      }
      d.stale = true;
    }
    _ => {}
  }
}

/// The version history of the design whose version badge was clicked.
fn history(app: &mut App, ui: &mut egui::Ui) {
  let d = app.data.details.clone();
  let Some(item) = d.get("item") else { return };
  let id = s(item, "id");
  let is_design = s(item, "kind") == "design";
  ui.separator();
  ui.horizontal(|ui| {
    ui.label(RichText::new(s(item, "name")).strong().size(14.0));
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
      if ui.small_button("×").on_hover_text("Close the version history").clicked() {
        app.data.history = None;
        app.data.details = Value::Null;
      }
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
  if is_design {
    lock_row(app, ui, &id, item.get("lock").filter(|l| !l.is_null()));
  }
  let presence = d.get("presence").and_then(Value::as_array).cloned().unwrap_or_default();
  let others: Vec<String> = presence
    .iter()
    .filter(|p| p.get("me") != Some(&json!(true)))
    .map(|p| {
      let host = s(p, "host");
      if host.is_empty() { s(p, "user") } else { format!("{} ({host})", s(p, "user")) }
    })
    .collect();
  if !others.is_empty() {
    ui.label(RichText::new(format!("👥 Open by {}", others.join(", "))).small().color(theme::ACCENT))
      .on_hover_text("They have this design open right now");
  }
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

/// The design's edit lock and what can be done about it.
fn lock_row(app: &mut App, ui: &mut egui::Ui, id: &str, lock: Option<&Value>) {
  ui.horizontal(|ui| match lock {
    None => {
      ui.label(RichText::new("🔓 Not locked").small().color(theme::MUTED));
      if ui
        .small_button("Lock")
        .on_hover_text("Take the edit lock: nobody else can save versions of it until you release it")
        .clicked()
      {
        hub(app, "hub.lock", json!({"item": id}));
        app.data.stale = true;
      }
    }
    Some(l) if l.get("mine") == Some(&json!(true)) => {
      let host = s(l, "host");
      let where_ =
        if l.get("here") == Some(&json!(true)) || host.is_empty() { String::new() } else { format!(" on {host}") };
      ui.label(RichText::new(format!("🔒 Locked by you{where_}")).small().color(theme::ICON_GREEN))
        .on_hover_text(format!("Since {}", s(l, "since_time")));
      if ui.small_button("Release").on_hover_text("Let others save versions of it again").clicked() {
        hub(app, "hub.unlock", json!({"item": id}));
        app.data.stale = true;
      }
    }
    Some(l) => {
      ui.label(RichText::new(format!("🔒 Locked by {}", s(l, "user"))).small().color(theme::WARNING))
        .on_hover_text(format!("Since {}: until they release it, only they can save versions", s(l, "since_time")));
      if ui
        .small_button("Steal lock")
        .on_hover_text("Take the lock from them: they can't save versions of it until it's released")
        .clicked()
      {
        hub(app, "hub.lock", json!({"item": id, "steal": true}));
        app.data.stale = true;
      }
    }
  });
}

fn prompt_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut p) = app.data.prompt.clone() else { return };
  let (title, hint) = match &p {
    Prompt::Milestone(..) => ("Mark milestone", "Milestone name (e.g. Sent to shop)"),
    Prompt::Comment(..) => ("Comment on version", "Your comment"),
    Prompt::Connect(..) => ("Connect to a hub server", "https://hub.example.com"),
  };
  let mut done = false;
  let mut cancel = false;
  egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
    ctx,
    |ui| {
      if let Prompt::Connect(url, token) = &mut p {
        ui.label(RichText::new("A FissionOxide hub server (fox-hub-server) and your access token").small());
        ui.add(egui::TextEdit::singleline(url).hint_text(hint).desired_width(260.0));
        let r = ui.add(
          egui::TextEdit::singleline(token)
            .hint_text("Access token (kept for this server)")
            .password(true)
            .desired_width(260.0),
        );
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
          done = true;
        }
      } else {
        let text = match &mut p {
          Prompt::Milestone(_, _, t) | Prompt::Comment(_, _, t) | Prompt::Connect(t, _) => t,
        };
        let r = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(260.0));
        r.request_focus();
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
          done = true;
        }
      }
      ui.horizontal(|ui| {
        done |= ui.button("OK").clicked();
        // Escape closes it too ([`crate::file_dialogs::escape`])
        cancel = ui.button("Cancel").clicked();
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
  let ok = match p {
    Prompt::Milestone(id, n, name) => {
      hub(app, "hub.milestone", json!({"item": id, "version": n, "name": name})).map(|_| ())
    }
    Prompt::Comment(id, n, text) => {
      hub(app, "hub.comment", json!({"item": id, "version": n, "text": text})).map(|_| ())
    }
    Prompt::Connect(url, token) => {
      let token = Some(token.trim()).filter(|t| !t.is_empty());
      let r = hub(app, "hub.connect", json!({"url": url.trim(), "token": token}));
      if r.is_some() {
        reset_location(app);
      }
      r.map(|_| ())
    }
  };
  let _ = ok;
  app.data.stale = true;
}

/// The frame of the panel's modal windows: white, as Fusion's.
fn modal_frame(ctx: &egui::Context) -> egui::Frame {
  egui::Frame::window(&ctx.global_style()).fill(Color32::WHITE).inner_margin(egui::Margin::same(16))
}

/// Move or Copy: pick a project and a folder.
fn destination_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut d) = app.data.dest.take() else { return };
  if d.listing.is_null()
    && let Some(p) = d.project.clone()
  {
    d.listing = hub(app, "hub.list", json!({"project": p, "folder": d.folder})).unwrap_or(json!({}));
  }
  let title = if d.copy { "Copy" } else { "Move" };
  let projects = app.data.projects.clone();
  let (mut done, mut cancel) = (false, false);
  let resp = egui::Modal::new(egui::Id::new("data-panel-destination"))
    .frame(modal_frame(ctx))
    .backdrop_color(Color32::from_black_alpha(60))
    .show(ctx, |ui| {
      ui.set_width(340.0);
      ui.label(RichText::new(format!("{title} \"{}\"", d.name)).strong().size(15.0));
      ui.add_space(6.0);
      let pname = projects.iter().find(|p| Some(s(p, "id")) == d.project).map(|p| s(p, "name")).unwrap_or_default();
      egui::ComboBox::from_id_salt("dest-project").selected_text(pname).width(300.0).show_ui(ui, |ui| {
        for p in &projects {
          let id = s(p, "id");
          if ui.selectable_label(d.project.as_deref() == Some(&id), s(p, "name")).clicked() {
            d.project = Some(id);
            d.folder = None;
            d.listing = Value::Null;
          }
        }
      });
      ui.horizontal_wrapped(|ui| {
        if ui.small_button("🏠").on_hover_text("The project's top level").clicked() {
          d.folder = None;
          d.listing = Value::Null;
        }
        for f in d.listing.get("path").and_then(Value::as_array).cloned().unwrap_or_default() {
          ui.label(RichText::new("›").color(theme::MUTED));
          if ui.link(s(&f, "name")).clicked() {
            d.folder = Some(s(&f, "id"));
            d.listing = Value::Null;
          }
        }
      });
      egui::ScrollArea::vertical().max_height(180.0).auto_shrink([false, true]).show(ui, |ui| {
        let folders = d.listing.get("folders").and_then(Value::as_array).cloned().unwrap_or_default();
        if folders.is_empty() {
          ui.label(RichText::new("No folders here").small().color(theme::MUTED));
        }
        for f in folders {
          if ui.selectable_label(false, format!("📁 {}", s(&f, "name"))).clicked() {
            d.folder = Some(s(&f, "id"));
            d.listing = Value::Null;
          }
        }
      });
      ui.add_space(8.0);
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        done = crate::panels::primary_button(ui, title, theme::ACCENT).clicked();
        cancel = crate::panels::secondary_button(ui, "Cancel").clicked();
      });
    });
  if cancel || resp.should_close() {
    return;
  }
  if done {
    apply_destination(app, d);
    return;
  }
  app.data.dest = Some(d);
}

/// Moves or copies the design where Move or Copy pointed.
fn apply_destination(app: &mut App, d: Destination) {
  let mut p = json!({"item": d.item, "project": d.project, "folder": d.folder});
  let elsewhere = d.from != (d.project.clone(), d.folder.clone());
  let cmd = if d.copy { "hub.item.copy" } else { "hub.item.move" };
  if d.copy && elsewhere {
    // a copy in another folder keeps the name; beside the original it's "(copy)"
    p["name"] = json!(d.name);
  }
  if hub(app, cmd, p).is_some() {
    let done = if d.copy { "Copied" } else { "Moved" };
    app.status = Some((format!("{done} {}", d.name), false));
  }
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

/// Upload (the panel's button, File ▸ Upload…): Fusion's Upload window, files from its Select
/// Files button or dropped on it, into the panel's project and folder.
pub(crate) fn upload_dialog(app: &mut App) {
  let (project, folder) = app.data.location();
  app.data.upload = Some(UploadForm { files: Vec::new(), project, folder });
}

fn upload_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut f) = app.data.upload.take() else { return };
  if app.data.projects.is_empty() {
    app.data.projects =
      app.s.execute("hub.projects", &json!({})).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
  }
  let projects = app.data.projects.clone();
  let (mut go, mut cancel) = (false, false);
  let resp = egui::Modal::new(egui::Id::new(UPLOAD_ID))
    .frame(modal_frame(ctx))
    .backdrop_color(Color32::from_black_alpha(60))
    .show(ctx, |ui| {
      ui.set_width(440.0);
      ui.horizontal(|ui| {
        ui.label(RichText::new("Upload").strong().size(16.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          if ui.add(egui::Button::new("✕").frame(false)).on_hover_text("Close").clicked() {
            cancel = true;
          }
        });
      });
      ui.add_space(10.0);
      ui.vertical_centered(|ui| {
        if blue_button(ui, "Select Files").clicked() {
          f.files.extend(pick_importable("Select Files", true));
        }
        ui.label(RichText::new("or").color(theme::MUTED));
      });
      ui.add_space(4.0);
      drop_box(ui);
      for (k, p) in f.files.clone().iter().enumerate() {
        ui.horizontal(|ui| {
          ui.label(RichText::new("📄").small());
          ui.label(p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
          if ui.small_button("×").on_hover_text("Leave it out").clicked() {
            f.files.remove(k);
          }
        });
      }
      ui.add_space(8.0);
      ui.horizontal(|ui| {
        ui.label(RichText::new("Location:").strong());
        let pname = projects
          .iter()
          .find(|p| Some(s(p, "id")) == f.project)
          .map(|p| s(p, "name"))
          .unwrap_or_else(|| DEFAULT_PROJECT.into());
        ui.label(pname);
        let r = ui.link("Change Location");
        egui::Popup::menu(&r).show(|ui| {
          for p in &projects {
            if ui.selectable_label(f.project.as_deref() == Some(&s(p, "id")), s(p, "name")).clicked() {
              f.project = Some(s(p, "id"));
              f.folder = None;
              ui.close();
            }
          }
        });
      });
      ui.add_space(12.0);
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let any = !f.files.is_empty();
        go = ui.add_enabled_ui(any, |ui| crate::panels::primary_button(ui, "Upload", theme::ACCENT)).inner.clicked();
        cancel |= crate::panels::secondary_button(ui, "Cancel").clicked();
      });
    });
  if cancel || resp.should_close() {
    return;
  }
  if go {
    upload_files(app, &f);
    return;
  }
  app.data.upload = Some(f);
}

/// The Upload window's drop zone: a cloud with an arrow, "Drag and Drop Here".
fn drop_box(ui: &mut egui::Ui) {
  let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 120.0), egui::Sense::hover());
  let p = ui.painter();
  p.rect(rect, 4.0, Color32::from_rgb(248, 249, 251), Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
  let c = rect.center() - vec2(0.0, 14.0);
  for (dx, dy, r) in [(-14.0, 4.0, 11.0), (0.0, -4.0, 15.0), (15.0, 4.0, 10.0)] {
    p.circle_filled(c + vec2(dx, dy), r, theme::SELECTED);
  }
  p.rect_filled(egui::Rect::from_center_size(c + vec2(0.0, 9.0), vec2(48.0, 12.0)), 4.0, theme::SELECTED);
  let ink = Stroke::new(2.0, theme::ACCENT);
  p.line_segment([c + vec2(0.0, 12.0), c + vec2(0.0, -8.0)], ink);
  p.line_segment([c + vec2(-6.0, -2.0), c + vec2(0.0, -8.0)], ink);
  p.line_segment([c + vec2(6.0, -2.0), c + vec2(0.0, -8.0)], ink);
  p.text(
    rect.center() + vec2(0.0, 30.0),
    egui::Align2::CENTER_CENTER,
    "Drag and Drop Here",
    egui::FontId::proportional(13.0),
    theme::GROUP_LABEL,
  );
}

fn upload_files(app: &mut App, f: &UploadForm) {
  for p in &f.files {
    if let Some(r) = hub(app, "hub.upload", json!({"path": p, "project": f.project, "folder": f.folder})) {
      let msg = match r.get("warning").and_then(Value::as_str) {
        Some(w) => (format!("Uploaded {}: {w}", p.display()), false),
        None => (format!("Uploaded {}", p.display()), false),
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

/// The Save dialog (Ctrl+S), as Fusion's white modal "Save": the first save (and Save As) asks
/// for the name and location only; a later save asks for a description and an optional
/// milestone. It closes with Cancel or its X, not Escape, as in Fusion.
pub fn save_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut f) = app.data.save.clone() else { return };
  let linked = app.s.hub.is_some() && !f.as_new;
  let title = if linked { "Save version" } else { "Save" };
  let mut done = false;
  let projects = app.data.projects.clone();
  let version = app.s.hub.as_ref().map(|h| h.version);
  let (cancel, closed) = crate::file_dialogs::modal(ctx, "save", title, 440.0, |ui| {
    if !linked {
      ui.label("Name:");
      let r = crate::file_dialogs::name_field(ui, &mut f.name, &mut f.first);
      if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        done = true;
      }
      ui.add_space(4.0);
      ui.label("Location:");
      let pname = projects
        .iter()
        .find(|p| Some(s(p, "id")) == f.project)
        .map(|p| s(p, "name"))
        .unwrap_or_else(|| "Default Project".into());
      egui::ComboBox::from_id_salt("save-project").selected_text(pname).width(ui.available_width()).show_ui(ui, |ui| {
        for p in &projects {
          if ui.selectable_label(f.project.as_deref() == Some(&s(p, "id")), s(p, "name")).clicked() {
            if f.project.as_deref() != Some(&s(p, "id")) {
              f.folder = None;
            }
            f.project = Some(s(p, "id"));
          }
        }
      });
    } else {
      ui.label("Description:");
      let r = ui.add(
        egui::TextEdit::singleline(&mut f.description)
          .hint_text("What changed (optional)")
          .desired_width(f32::INFINITY)
          .margin(egui::vec2(6.0, 5.0)),
      );
      if std::mem::take(&mut f.first) {
        r.request_focus();
      }
      if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        done = true;
      }
      ui.add_space(4.0);
      ui.label("Milestone:");
      ui.add(
        egui::TextEdit::singleline(&mut f.milestone)
          .hint_text("Name it to mark a milestone (optional)")
          .desired_width(f32::INFINITY)
          .margin(egui::vec2(6.0, 5.0)),
      );
      if let Some(v) = version {
        ui.label(
          RichText::new(format!("Saves a new version after v{v} — earlier versions are kept."))
            .small()
            .color(theme::MUTED),
        );
      }
    }
    let b = crate::file_dialogs::footer(ui, None, "Save", true);
    done |= b.primary;
    b.cancel
  });
  let cancel = cancel || closed;
  if cancel {
    app.data.save = None;
    return;
  }
  // the click or Enter that brought the window to the front wasn't aimed at this dialog
  if !done || app.just_activated(ctx) {
    app.data.save = Some(f);
    return;
  }
  app.data.save = None;
  // the user asked for this save: even an empty "Untitled" design is kept
  let mut p = json!({"description": f.description, "new": f.as_new || app.s.hub.is_none(), "explicit": true});
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

#[cfg(test)]
mod tests {
  use egui::{Event, PointerButton, Pos2, RawInput, Rect, vec2};
  use fox_engine::Session;

  use crate::App;

  /// Frames through a real egui context, with the window in front or not.
  struct Window {
    ctx: egui::Context,
    app: App,
    time: f64,
  }

  impl Window {
    fn frame(&mut self, focused: bool, events: Vec<Event>) {
      self.time += 1.0 / 60.0;
      let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        time: Some(self.time),
        focused,
        events,
        ..Default::default()
      };
      let app = &mut self.app;
      self.ctx.run_ui(input, |ui| app.draw(ui)).drop_without_applying_deltas();
    }

    fn click(&mut self, focused: bool, pos: Pos2) {
      let button =
        |pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() };
      self.frame(focused, vec![Event::PointerMoved(pos), button(true), button(false)]);
      self.frame(true, vec![]);
    }

    fn hub_items(&mut self) -> usize {
      self
        .app
        .s
        .execute("hub.recent", &serde_json::json!({}))
        .ok()
        .and_then(|v| v.as_array().map(Vec::len))
        .unwrap_or(0)
    }
  }

  #[test]
  fn the_click_that_brings_the_window_to_the_front_doesnt_save() {
    let root = std::env::temp_dir().join(format!("fox-ui-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut app = App::with_session(Session::default());
    app.s.store_root = Some(root.clone());
    let mut w = Window { ctx: egui::Context::default(), app, time: 0.0 };
    crate::theme::apply(&w.ctx);
    for _ in 0..3 {
      w.frame(true, vec![]);
    }
    w.app.save(false);
    w.frame(true, vec![]);
    w.frame(true, vec![]);
    let dialog = w.ctx.memory(|m| m.area_rect(crate::file_dialogs::modal_id("save"))).expect("the Save dialog shows");
    // Save is the blue button at the lower right, as in Fusion
    use crate::file_dialogs::{BUTTON, MARGIN};
    let save = dialog.right_bottom() - vec2(MARGIN + BUTTON.x / 2.0, MARGIN + BUTTON.y / 2.0);

    // the window goes to the back, then a click on the dialog's Save brings it to the front
    w.frame(false, vec![]);
    w.click(true, save);
    assert!(w.app.data.save.is_some(), "the dialog stays open");
    assert_eq!(w.hub_items(), 0, "nothing was saved");

    // opening another design closes the dialog that was for the old one
    w.app.new_design();
    assert!(w.app.data.save.is_none());

    // a click once the window is in front saves, even a blank design: the user asked for it
    w.app.save(false);
    for _ in 0..40 {
      w.frame(true, vec![]);
    }
    w.click(true, save);
    assert!(w.app.data.save.is_none(), "{:?}", w.app.status);
    assert_eq!(w.hub_items(), 1);
    let _ = std::fs::remove_dir_all(root);
  }

  use serde_json::{Value, json};

  use super::{
    BUTTON_TIP, Card, CardTab, ITEM_MENU, Naming, ProjectFilter, ProjectTab, Sort, UPLOAD_ID, View, item_action,
    new_folder, open_card, project_cards, sort_items, upload_dialog, us_time, work_out_references,
  };

  fn key(key: egui::Key) -> Event {
    Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }
  }

  /// A window on an app with its own empty hub.
  fn window(tag: &str) -> (Window, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("fox-ui-dp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut app = App::with_session(Session::default());
    app.s.store_root = Some(root.clone());
    let mut w = Window { ctx: egui::Context::default(), app, time: 0.0 };
    crate::theme::apply(&w.ctx);
    w.frames(3);
    (w, root)
  }

  impl Window {
    fn frames(&mut self, n: usize) {
      for _ in 0..n {
        self.frame(true, vec![]);
      }
    }

    fn save_as(&mut self, name: &str) -> String {
      let r = self.app.s.execute("hub.save", &json!({"name": name, "new": true, "explicit": true})).unwrap();
      r["item"].as_str().unwrap().to_string()
    }

    fn list(&mut self, project: &str, folder: Option<&str>) -> Value {
      self.app.s.execute("hub.list", &json!({"project": project, "folder": folder})).unwrap()
    }
  }

  fn names(v: &Value, k: &str) -> Vec<String> {
    v[k].as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap().to_string()).collect()
  }

  #[test]
  fn the_button_reads_data_panel() {
    assert_eq!(BUTTON_TIP, "Data Panel");
  }

  #[test]
  fn the_project_list_has_fusions_cards_and_filters() {
    let projects = vec![json!({"id": "p1", "name": "Default Project"}), json!({"id": "p2", "name": "LumenArt"})];
    let all = project_cards(&projects, ProjectFilter::All, "", false);
    let labels: Vec<&str> = all.iter().map(|(_, n, _)| n.as_str()).collect();
    assert_eq!(labels, ["My Recent Data", "My Editable Documents", "Default Project", "LumenArt"]);
    assert_eq!(all[0].2, "A list of what you've recently been working on");
    assert_eq!(all[1].2, "A list of your editable documents");
    assert!(all[2].2.starts_with("This comes with FissionOxide and is where your work is stored"));
    assert_eq!(all[3].0, Card::Project("p2".into()));
    // a local hub has nothing pinned or shared; a hub server's projects are shared
    assert!(project_cards(&projects, ProjectFilter::Pinned, "", false).is_empty());
    assert!(project_cards(&projects, ProjectFilter::Shared, "", false).is_empty());
    assert_eq!(project_cards(&projects, ProjectFilter::Shared, "", true).len(), 2);
    assert_eq!(project_cards(&projects, ProjectFilter::Owned, "", false).len(), 2);
    // the Filter field
    let lumen = project_cards(&projects, ProjectFilter::All, "lumen", false);
    assert_eq!(lumen.len(), 1);
    assert_eq!(lumen[0].1, "LumenArt");
    let labels: Vec<&str> = ProjectFilter::ALL.iter().map(|f| f.label()).collect();
    assert_eq!(labels, ["All Projects", "Joined by Me", "Pinned", "Owned by Me", "Shared with Me"]);
  }

  #[test]
  fn the_design_menu_lists_fusions_entries_and_greys_the_cloud_ones() {
    let labels: Vec<&str> = ITEM_MENU.iter().map(|(l, _)| *l).collect();
    assert_eq!(
      labels,
      [
        "Open",
        "Insert into Current Design",
        "Create Drawing",
        "Show in Location",
        "Share Link",
        "Import New Change",
        "Rename",
        "Move",
        "Copy",
        "Move to Trash",
        "Add to Offline Cache",
        "Create Version",
      ]
    );
    let greyed: Vec<&str> = ITEM_MENU.iter().filter(|(_, on)| !on).map(|(l, _)| *l).collect();
    assert_eq!(greyed, ["Share Link", "Import New Change", "Add to Offline Cache", "Create Version"]);
  }

  #[test]
  fn dates_read_as_fusions() {
    assert_eq!(us_time(1_765_762_440), "12/15/2025 1:34 AM");
    assert_eq!(us_time(1_772_456_700), "3/2/2026 1:05 PM");
    assert_eq!(us_time(1_772_410_020), "3/2/2026 12:07 AM");
  }

  #[test]
  fn the_settings_gear_sorts_by_last_updated_or_name() {
    let mut items = vec![
      json!({"name": "beta", "modified": 5}),
      json!({"name": "Alpha", "modified": 1}),
      json!({"name": "gamma", "modified": 9}),
    ];
    sort_items(&mut items, Sort::Updated);
    let order: Vec<&str> = items.iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert_eq!(order, ["gamma", "beta", "Alpha"]);
    sort_items(&mut items, Sort::Name);
    let order: Vec<&str> = items.iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert_eq!(order, ["Alpha", "beta", "gamma"]);
  }

  /// The home is the project list; a double-click opens a project; New Folder adds "New Folder"
  /// with its name selected in an inline editor (typing replaces it, Enter keeps it, Escape
  /// leaves "New Folder"); the home icon goes back.
  #[test]
  fn the_project_list_opens_projects_and_new_folder_names_in_place() {
    let (mut w, root) = window("home");
    w.save_as("Bracket");
    w.app.data.toggle();
    w.frames(3);
    assert_eq!(w.app.data.view, View::Projects);
    let cards = project_cards(&w.app.data.projects, ProjectFilter::All, "", false);
    assert_eq!(cards.len(), 3, "{cards:?}");
    let Card::Project(project) = cards[2].0.clone() else { panic!("{cards:?}") };
    open_card(&mut w.app, &cards[2].0);
    w.frames(3);
    assert_eq!(w.app.data.view, View::Project);
    assert_eq!(w.app.data.tab, ProjectTab::Data);
    assert_eq!(names(&w.app.data.listing, "items"), ["Bracket"]);
    // every way of showing it draws
    for list in [false, true] {
      w.app.data.list = list;
      w.frames(2);
    }

    new_folder(&mut w.app);
    assert_eq!(names(&w.list(&project, None), "folders"), ["New Folder"], "made at once");
    w.frames(4);
    assert!(w.app.data.edit.as_ref().is_some_and(|e| matches!(e.what, Naming::Folder(_))));
    w.frame(true, vec![Event::Text("Parts".into())]);
    w.frame(true, vec![key(egui::Key::Enter)]);
    w.frames(3);
    assert!(w.app.data.edit.is_none());
    assert_eq!(names(&w.list(&project, None), "folders"), ["Parts"], "the typed name replaced the selected one");

    new_folder(&mut w.app);
    w.frames(4);
    w.frame(true, vec![Event::Text("Junk".into())]);
    w.frame(true, vec![key(egui::Key::Escape)]);
    w.frames(3);
    assert!(w.app.data.edit.is_none(), "Escape ends the editor");
    let mut folders = names(&w.list(&project, None), "folders");
    folders.sort();
    assert_eq!(folders, ["New Folder", "Parts"]);
    assert!(w.app.data.open, "Escape left the panel open");

    w.app.data.tab = ProjectTab::People;
    w.frames(2);
    super::go_home(&mut w.app);
    w.frames(2);
    assert_eq!(w.app.data.view, View::Projects);
    let _ = std::fs::remove_dir_all(root);
  }

  /// Search turns the header into Search All Data: nothing listed until a term is typed.
  #[test]
  fn search_lists_nothing_until_a_term_is_typed() {
    let (mut w, root) = window("search");
    w.save_as("Bracket");
    w.save_as("Hinge");
    w.app.data.toggle();
    w.frames(2);
    w.app.data.searching = true;
    w.app.data.stale = true;
    w.frames(2);
    assert!(w.app.data.other.is_null(), "{:?}", w.app.data.other);
    w.app.data.search = "brack".into();
    w.app.data.stale = true;
    w.frames(2);
    assert_eq!(w.app.data.other.as_array().map(Vec::len), Some(1));
    // Escape doesn't leave it
    w.frame(true, vec![key(egui::Key::Escape)]);
    w.frames(1);
    assert!(w.app.data.searching);
    let _ = std::fs::remove_dir_all(root);
  }

  /// Upload opens Fusion's Upload window first (no file dialog), File ▸ Upload… too; Escape
  /// closes it.
  #[test]
  fn upload_opens_the_upload_window() {
    let (mut w, root) = window("upload");
    upload_dialog(&mut w.app);
    w.frames(2);
    let f = w.app.data.upload.as_ref().expect("the Upload window is open");
    assert!(f.files.is_empty());
    assert!(w.ctx.memory(|m| m.area_rect(egui::Id::new(UPLOAD_ID))).is_some(), "it shows");
    w.frame(true, vec![key(egui::Key::Escape)]);
    w.frames(1);
    assert!(w.app.data.upload.is_none(), "Escape closes it");
    crate::panels::file_menu_action(&mut w.app, "Upload...");
    assert!(w.app.data.upload.is_some(), "File ▸ Upload… opens the same window");
    let _ = std::fs::remove_dir_all(root);
  }

  /// Show in Location, Move (to another project), Copy, Move to Trash; and the details card's
  /// Uses / Used In.
  #[test]
  fn the_design_menu_moves_copies_and_trashes() {
    let (mut w, root) = window("menu");
    let plate = w.save_as("Plate");
    let other = w.app.s.execute("hub.project.create", &json!({"name": "LumenArt"})).unwrap()["project"]
      .as_str()
      .unwrap()
      .to_string();
    w.app.data.toggle();
    w.frames(2);
    open_card(&mut w.app, &Card::Recent);
    w.frames(2);
    let it = w.app.data.other[0].clone();
    assert_eq!(it["name"], "Plate");
    w.app.data.list = true;
    w.app.data.expanded = Some(plate.clone());
    for tab in [CardTab::Uses, CardTab::UsedIn, CardTab::Drawings] {
      w.app.data.card_tab = tab;
      w.frames(1);
    }

    item_action(&mut w.app, "Show in Location", &it);
    assert_eq!(w.app.data.view, View::Project);
    assert_eq!(w.app.data.project.as_deref(), it["project"].as_str());
    assert_eq!(w.app.data.selected.as_deref(), Some(plate.as_str()));

    item_action(&mut w.app, "Copy", &it);
    let d = w.app.data.dest.clone().expect("Copy asks where to");
    super::apply_destination(&mut w.app, d);
    let home = it["project"].as_str().unwrap().to_string();
    let mut here = names(&w.list(&home, None), "items");
    here.sort();
    assert_eq!(here, ["Plate", "Plate (copy)"], "a copy beside the original");

    item_action(&mut w.app, "Move", &it);
    w.frames(2);
    assert!(w.app.data.modal_open());
    let mut d = w.app.data.dest.take().unwrap();
    d.project = Some(other.clone());
    super::apply_destination(&mut w.app, d);
    assert_eq!(names(&w.list(&other, None), "items"), ["Plate"]);

    item_action(&mut w.app, "Move to Trash", &it);
    assert!(names(&w.list(&other, None), "items").is_empty());
    let trash = w.app.s.execute("hub.trash", &json!({})).unwrap();
    assert_eq!(names(&trash, "items"), ["Plate"]);
    let _ = std::fs::remove_dir_all(root);
  }

  #[test]
  fn the_details_card_lists_what_a_design_uses_and_is_used_in() {
    let (mut w, root) = window("refs");
    w.app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    let plate = w.save_as("Plate");
    w.app.new_design();
    w.app.run("component.insert", json!({"item": plate})).expect("inserted");
    let asm = w.save_as("Assembly");
    assert_eq!(work_out_references(&w.app.s, &asm, CardTab::Uses), ["Plate"]);
    assert_eq!(work_out_references(&w.app.s, &plate, CardTab::UsedIn), ["Assembly"]);
    assert!(work_out_references(&w.app.s, &plate, CardTab::Uses).is_empty());
    assert!(work_out_references(&w.app.s, &plate, CardTab::Drawings).is_empty());
    let _ = std::fs::remove_dir_all(root);
  }
}
