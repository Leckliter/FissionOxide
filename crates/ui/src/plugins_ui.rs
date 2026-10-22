//! Plugins in the UI: Fusion's Scripts and Add-Ins window (UTILITIES > ADD-INS, Shift+S, the
//! app bar's Plugins button), the install dialog (also for `.foxplugin` files dropped on the window),
//! a toolbar tab for each tab the installed plugins name, and the dialog each plugin command
//! opens, drawn from its manifest's inputs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Color32, RichText, TextureHandle, pos2, vec2};
use fox_plugin::hub::HubPlugin;
use fox_plugin::manifest::{InputKind, condition_holds};
use fox_plugin::{CommandRef, Input, InstallOptions, Package, Permission, Plan, Plugins, RunOptions};
use serde_json::{Map, Value, json};

use crate::icons::Icon;
use crate::{App, icons, panels, theme};

#[derive(Default)]
pub(crate) struct PluginsUi {
  /// The Scripts and Add-Ins window is open.
  pub open: bool,
  /// The registry, for the hub directory it was read from.
  reg: Option<(PathBuf, Plugins)>,
  /// Commands of the usable plugins, and the toolbar tabs they name (in order).
  cmds: Vec<CommandRef>,
  pub tabs: Vec<String>,
  /// A package waiting for the user to confirm its install.
  install: Option<PendingInstall>,
  /// The data hub's plugins, once looked up.
  hub: Option<Result<Vec<HubPlugin>, String>>,
  /// The Scripts and Add-Ins window: its search, Favorites or All, filters, folded groups
  /// and the row picked (whose details show at the right).
  search: String,
  favorites_only: bool,
  favorites: Vec<String>,
  filter: Filter,
  folded: Vec<&'static str>,
  selected: Option<String>,
  icons: HashMap<PathBuf, Option<TextureHandle>>,
  /// The open plugin command dialog.
  dialog: Option<CommandDialog>,
  /// The folder plugins last read from and wrote to.
  folder: Option<PathBuf>,
  /// The Scripts and Add-Ins window's last message (text, is an error).
  message: Option<(String, bool)>,
}

struct PendingInstall {
  pkg: Package,
  plan: Plan,
  grant: Vec<(Permission, bool)>,
  source: String,
}

/// A plugin command's dialog.
struct CommandDialog {
  cmd: CommandRef,
  /// Values by input id: expressions as typed for value inputs, picks for selections.
  values: Map<String, Value>,
  /// What the plugin changed about inputs while the dialog is open.
  over: HashMap<String, Over>,
  /// The selection input that follows the viewport selection.
  active_sel: Option<String>,
  error: Option<String>,
}

#[derive(Default)]
struct Over {
  visible: Option<bool>,
  enabled: Option<bool>,
  text: Option<String>,
  label: Option<String>,
  options: Option<Vec<String>>,
}

impl PluginsUi {
  /// The registry of the app's data hub (read again when the hub changes).
  fn registry(&mut self, hub: &Path) -> &mut Plugins {
    if self.reg.as_ref().is_none_or(|(h, _)| h != hub) {
      self.reg = Some((hub.to_path_buf(), Plugins::for_hub(hub)));
      self.refresh();
    }
    &mut self.reg.get_or_insert_with(|| (hub.to_path_buf(), Plugins::for_hub(hub))).1
  }

  /// Re-reads what the toolbar shows after the registry changed.
  fn refresh(&mut self) {
    self.cmds = self.reg.as_ref().map(|(_, p)| p.commands()).unwrap_or_default();
    let mut tabs: Vec<String> = Vec::new();
    for c in &self.cmds {
      if !tabs.contains(&c.tab) {
        tabs.push(c.tab.clone());
      }
    }
    self.tabs = tabs;
  }
}

/// Loads the registry for the app's hub; call once per frame before the toolbar.
pub(crate) fn sync(app: &mut App) {
  let hub = app.hub_root();
  app.plugins.registry(&hub);
}

pub(crate) fn open_window(app: &mut App) {
  app.plugins.open = true;
}

// ---------------------------------------------------------------- installing

/// Asks to install a `.foxplugin` file (Install… or dropped on the window).
pub(crate) fn offer_file(app: &mut App, path: &Path) {
  let read = std::fs::read(path).map_err(|e| e.to_string()).and_then(|b| Package::read(&b));
  match read {
    Ok(pkg) => offer(app, pkg, path.to_string_lossy().into_owned()),
    Err(e) => app.status = Some((format!("{}: {e}", path.display()), true)),
  }
}

fn offer(app: &mut App, pkg: Package, source: String) {
  let hub = app.hub_root();
  let plan = app.plugins.registry(&hub).plan(&pkg);
  let grant = pkg.manifest.permissions.iter().map(|p| (*p, true)).collect();
  app.plugins.install = Some(PendingInstall { pkg, plan, grant, source });
}

fn pick_file(app: &mut App) {
  if let Some(p) =
    rfd::FileDialog::new().set_title("Install plugin").add_filter("FissionOxide plugin", &["foxplugin"]).pick_file()
  {
    offer_file(app, &p);
  }
}

fn install_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut pending) = app.plugins.install.take() else { return };
  let mut keep = true;
  let mut confirmed = false;
  let m = pending.pkg.manifest.clone();
  egui::Window::new(format!("Install {}", m.name))
    .collapsible(false)
    .resizable(false)
    .default_width(420.0)
    .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
    .show(ctx, |ui| {
      ui.label(RichText::new(format!("{} {}", m.name, m.version)).strong().size(15.0));
      if !m.author.is_empty() {
        ui.label(RichText::new(format!("by {}", m.author)).color(theme::MUTED));
      }
      if !m.description.is_empty() {
        ui.label(&m.description);
      }
      let note = match &pending.plan {
        Plan::Install => "A new plugin.".to_string(),
        Plan::Update { from } => format!("Updates version {from}."),
        Plan::Reinstall => "This version is already installed; installing replaces it.".to_string(),
        Plan::Downgrade { from } => format!("Version {from} is installed; this one is OLDER."),
      };
      ui.label(note);
      ui.add_space(6.0);
      ui.label(RichText::new("Adds").strong());
      for c in &m.commands {
        ui.label(command_line(&m, c));
      }
      ui.add_space(6.0);
      if pending.grant.is_empty() {
        ui.label("It asks for no permissions: it can only change the open design.");
      } else {
        ui.label(RichText::new("It asks to").strong());
        for (p, on) in &mut pending.grant {
          ui.checkbox(on, p.describe()).on_hover_text(p.id());
        }
        ui.label(
          RichText::new("Untick what you don't want to allow; commands that need it will fail.").color(theme::MUTED),
        );
      }
      if let Some(readme) = pending.pkg.readme() {
        ui.collapsing("Read me", |ui| {
          egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| ui.label(readme));
        });
      }
      ui.add_space(8.0);
      ui.horizontal(|ui| {
        let label = match pending.plan {
          Plan::Install => "Install",
          Plan::Update { .. } => "Update",
          Plan::Reinstall => "Reinstall",
          Plan::Downgrade { .. } => "Install older version",
        };
        if ui.button(label).clicked() {
          confirmed = true;
        }
        if ui.button("Cancel").clicked() {
          keep = false;
        }
      });
    });
  if confirmed {
    let opts = InstallOptions {
      grant: Some(pending.grant.iter().filter(|(_, on)| *on).map(|(p, _)| *p).collect()),
      allow_downgrade: true,
      source: pending.source.clone(),
    };
    let hub = app.hub_root();
    let r = app.plugins.registry(&hub).install(&pending.pkg, opts);
    app.plugins.refresh();
    app.plugins.hub = None;
    match r {
      Ok(_) => {
        let msg = format!("Installed {} {}: its commands are on the {} tab", m.name, m.version, tab_of(&m));
        app.plugins.message = Some((msg.clone(), false));
        app.status = Some((msg, false));
      }
      Err(e) => {
        app.plugins.message = Some((e.clone(), true));
        app.status = Some((e, true));
      }
    }
    return;
  }
  if keep {
    app.plugins.install = Some(pending);
  }
}

fn tab_of(m: &fox_plugin::Manifest) -> String {
  m.commands.first().map(|c| m.placement(c).0).unwrap_or_else(|| m.name.clone())
}

// ---------------------------------------------------------------- the Scripts and Add-Ins window

/// The window's width and the height of its three panes, as Fusion's (about 800 x 510).
const WINDOW_W: f32 = 800.0;
const PANES_H: f32 = 414.0;
const LEFT_W: f32 = 170.0;
const RIGHT_W: f32 = 230.0;
/// The table's columns: where Run, Run on Startup and Version start.
const RUN_X: f32 = 160.0;
const STARTUP_X: f32 = 205.0;
const VERSION_X: f32 = 300.0;
const ROW_H: f32 = 26.0;

/// The left pane's filters, in Fusion's order.
const TYPES: [&str; 2] = ["Scripts", "Add-Ins"];
const STATUSES: [&str; 2] = ["Running", "Run on startup"];
const SOURCES: [&str; 5] = ["Installed", "From device", "Created by me", "Fusion", "Samples"];

/// The filters ticked in the left pane; none ticked in a group lets everything through.
#[derive(Default)]
pub(crate) struct Filter {
  kinds: [bool; 2],
  status: [bool; 2],
  source: [bool; 5],
}

/// Which of [`SOURCES`] a plugin came from: shipped with the app (Fusion's own), from the data
/// hub (installed from the marketplace) or from a file.
fn source_of(p: &fox_plugin::Installed) -> usize {
  if p.bundled {
    3
  } else if p.record.source.starts_with("hub:") {
    0
  } else {
    1
  }
}

/// The rows the table shows: the plugins the search, Favorites and the filters let through, by
/// name. Every plugin is an add-in; FissionOxide has no scripts.
fn rows<'a>(st: &PluginsUi, list: &'a [fox_plugin::Installed]) -> Vec<&'a fox_plugin::Installed> {
  let any = |ticked: &[bool], has: &dyn Fn(usize) -> bool| {
    !ticked.contains(&true) || (0..ticked.len()).any(|i| ticked[i] && has(i))
  };
  let search = st.search.trim().to_lowercase();
  let mut out: Vec<&fox_plugin::Installed> = list
    .iter()
    .filter(|p| search.is_empty() || p.name().to_lowercase().contains(&search))
    .filter(|p| !st.favorites_only || st.favorites.contains(&p.record.id))
    .filter(|_| any(&st.filter.kinds, &|i| i == 1))
    .filter(|p| any(&st.filter.status, &|i| if i == 0 { p.record.enabled } else { p.startup }))
    .filter(|p| any(&st.filter.source, &|i| i == source_of(p)))
    .collect();
  out.sort_by_key(|p| p.name().to_lowercase());
  out
}

pub(crate) fn window(app: &mut App, ctx: &egui::Context) {
  install_window(app, ctx);
  command_dialog(app, ctx);
  if !app.plugins.open {
    return;
  }
  let hub = app.hub_root();
  let mut act: Option<Act> = None;
  let list = app.plugins.registry(&hub).list.clone();
  let log = match app.plugins.selected.clone() {
    Some(id) => app.plugins.registry(&hub).log(&id),
    None => String::new(),
  };
  let st = &mut app.plugins;
  if st.selected.as_ref().is_some_and(|id| !list.iter().any(|p| &p.record.id == id)) {
    st.selected = None;
  }
  let (close, x) = crate::file_dialogs::floating(ctx, "scripts-addins", "Scripts and Add-Ins", WINDOW_W, |ui| {
    ui.horizontal_top(|ui| {
      pane(ui, LEFT_W, |ui| left_pane(ui, st));
      divider(ui);
      let mid = ui.available_width() - RIGHT_W - 9.0;
      pane(ui, mid, |ui| middle_pane(ui, st, &list, &mut act));
      divider(ui);
      pane(ui, ui.available_width(), |ui| right_pane(ui, st, &list, &log, &mut act));
    });
    ui.add_space(12.0);
    let mut close = false;
    ui.allocate_ui_with_layout(
      vec2(ui.available_width(), crate::file_dialogs::BUTTON.y),
      egui::Layout::left_to_right(egui::Align::Center),
      |ui| {
        if let Some((m, err)) = &st.message {
          ui.colored_label(if *err { Color32::from_rgb(200, 40, 40) } else { theme::MUTED }, m);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          close = ui.add(crate::file_dialogs::blue_button("Close")).clicked();
        });
      },
    );
    close
  });
  if close || x {
    app.plugins.open = false;
  }
  if let Some(a) = act {
    apply(app, ctx, a);
  }
}

/// One of the window's three panes, `width` wide and the panes' height.
fn pane(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
  ui.allocate_ui_with_layout(vec2(width, PANES_H), egui::Layout::top_down(egui::Align::Min), |ui| {
    ui.set_min_size(vec2(width, PANES_H));
    ui.set_max_width(width);
    add(ui);
  });
}

/// The line between two panes.
fn divider(ui: &mut egui::Ui) {
  let (r, _) = ui.allocate_exact_size(vec2(1.0, PANES_H), egui::Sense::hover());
  ui.painter().rect_filled(r, 0.0, Color32::from_gray(220));
}

/// A group's header: its name, and at the right a "-" that folds it ("+" unfolds it). Returns
/// whether it is open.
fn group_header(ui: &mut egui::Ui, st: &mut PluginsUi, name: &'static str, strong: bool) -> bool {
  let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), egui::Sense::click());
  let open = !st.folded.contains(&name);
  let font = egui::FontId::proportional(if strong { 13.0 } else { 12.5 });
  let color = if strong { theme::TEXT } else { theme::MUTED };
  ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, name, font, color);
  let c = rect.right_center() - vec2(8.0, 0.0);
  let stroke = egui::Stroke::new(1.2, theme::ICON_INK);
  ui.painter().line_segment([c - vec2(4.0, 0.0), c + vec2(4.0, 0.0)], stroke);
  if !open {
    ui.painter().line_segment([c - vec2(0.0, 4.0), c + vec2(0.0, 4.0)], stroke);
  }
  if r.clicked() {
    if open {
      st.folded.push(name);
    } else {
      st.folded.retain(|n| *n != name);
    }
  }
  open
}

fn left_pane(ui: &mut egui::Ui, st: &mut PluginsUi) {
  ui.spacing_mut().item_spacing = vec2(6.0, 2.0);
  ui.add(
    egui::TextEdit::singleline(&mut st.search).hint_text("Search").desired_width(f32::INFINITY).margin(vec2(6.0, 4.0)),
  );
  ui.add_space(6.0);
  egui::ScrollArea::vertical().id_salt("scripts-addins-left").auto_shrink([false, false]).show(ui, |ui| {
    if group_header(ui, st, "Browse", true) {
      for (fav, icon, label) in [(true, Icon::Star, "Favorites"), (false, Icon::Scripts, "All scripts and add-ins")] {
        let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), egui::Sense::click());
        if st.favorites_only == fav {
          ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(214, 232, 250));
        } else if r.hovered() {
          ui.painter().rect_filled(rect, 0.0, theme::HOVER);
        }
        let ir = egui::Rect::from_center_size(rect.left_center() + vec2(12.0, 0.0), egui::Vec2::splat(15.0));
        icons::draw(ui.painter(), ir, icon, theme::ICON_INK);
        ui.painter().text(
          rect.left_center() + vec2(26.0, 0.0),
          egui::Align2::LEFT_CENTER,
          label,
          egui::FontId::proportional(12.5),
          theme::TEXT,
        );
        if r.clicked() {
          st.favorites_only = fav;
        }
      }
    }
    ui.add_space(6.0);
    if group_header(ui, st, "Filters", true) {
      ui.indent("scripts-addins-filters", |ui| {
        let groups: [(&'static str, &[&str]); 3] = [("Type", &TYPES), ("Status", &STATUSES), ("Source", &SOURCES)];
        for (k, (name, labels)) in groups.into_iter().enumerate() {
          if group_header(ui, st, name, false) {
            for (i, label) in labels.iter().enumerate() {
              let on = match k {
                0 => &mut st.filter.kinds[i],
                1 => &mut st.filter.status[i],
                _ => &mut st.filter.source[i],
              };
              ui.checkbox(on, *label);
            }
          }
        }
      });
    }
  });
}

/// A toolbar button over the table; `caret` adds the drop-down arrow ("+").
fn bar_button(ui: &mut egui::Ui, icon: Icon, hint: &str, enabled: bool, caret: bool) -> egui::Response {
  let size = vec2(if caret { 34.0 } else { 26.0 }, 26.0);
  let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
  let (rect, r) = ui.allocate_exact_size(size, sense);
  if enabled && r.hovered() {
    ui.painter().rect_filled(rect, 2.0, theme::HOVER);
  }
  let color = if enabled { theme::ICON_INK } else { Color32::from_gray(190) };
  let ir = egui::Rect::from_center_size(pos2(rect.left() + 13.0, rect.center().y), egui::Vec2::splat(16.0));
  match icon {
    // two links pulled apart (Fusion's unlink)
    Icon::Close => {
      let stroke = egui::Stroke::new(1.5, color);
      for (a, b) in [(vec2(-7.0, 5.0), vec2(-1.0, -1.0)), (vec2(1.0, 1.0), vec2(7.0, -5.0))] {
        ui.painter().line_segment([ir.center() + a, ir.center() + b], egui::Stroke::new(4.0, color));
        ui.painter()
          .line_segment([ir.center() + a * 0.6, ir.center() + b * 0.6], egui::Stroke::new(1.5, Color32::WHITE));
      }
      ui.painter().line_segment([ir.center() + vec2(-2.0, -6.0), ir.center() + vec2(-1.0, -4.0)], stroke);
      ui.painter().line_segment([ir.center() + vec2(2.0, 6.0), ir.center() + vec2(1.0, 4.0)], stroke);
    }
    _ => icons::draw(ui.painter(), ir, icon, color),
  }
  if caret {
    let c = pos2(rect.right() - 8.0, rect.center().y + 1.0);
    ui.painter().add(egui::Shape::convex_polygon(
      vec![c + vec2(-3.5, -2.0), c + vec2(3.5, -2.0), c + vec2(0.0, 2.0)],
      color,
      egui::Stroke::NONE,
    ));
  }
  r.on_hover_text(hint)
}

/// The switch in an add-in's Run column, blue while it runs.
fn switch(ui: &mut egui::Ui, at: egui::Pos2, on: bool, id: egui::Id) -> egui::Response {
  let rect = egui::Rect::from_center_size(at, vec2(28.0, 14.0));
  let r = ui.interact(rect, id, egui::Sense::click());
  let fill = if on { theme::ACCENT } else { Color32::from_gray(190) };
  ui.painter().rect_filled(rect, 7.0, fill);
  let knob = if on { rect.right_center() - vec2(7.0, 0.0) } else { rect.left_center() + vec2(7.0, 0.0) };
  ui.painter().circle_filled(knob, 5.0, Color32::WHITE);
  r
}

fn middle_pane(ui: &mut egui::Ui, st: &mut PluginsUi, list: &[fox_plugin::Installed], act: &mut Option<Act>) {
  ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
  let picked = st.selected.as_ref().and_then(|id| list.iter().find(|p| &p.record.id == id));
  ui.horizontal(|ui| {
    let plus = bar_button(ui, Icon::Plus, "Add a script or add-in", true, true);
    egui::Popup::menu(&plus).id(egui::Id::new("scripts-addins-plus")).show(|ui| {
      if ui.button("Download add-in from marketplace").on_hover_text("Plugins shared in your data hub").clicked() {
        *act = Some(Act::Hub);
      }
      if ui.button("Script or add-in from device").on_hover_text("Install a .foxplugin file").clicked() {
        *act = Some(Act::PickFile);
      }
      ui.add_enabled(false, egui::Button::new("Create script or add-in"))
        .on_disabled_hover_text("FissionOxide plugins are built outside the app: see the plugin guide");
    });
    if bar_button(ui, Icon::Pencil, "Edit", true, false).clicked() {
      *act = Some(picked.map_or(Act::Hint, |p| Act::Reveal(p.dir.join("plugin.toml"))));
    }
    if bar_button(ui, Icon::Folder, "Open File Location", true, false).clicked() {
      *act = Some(picked.map_or(Act::Hint, |p| Act::Reveal(p.dir.clone())));
    }
    let removable = picked.is_some_and(|p| !p.bundled);
    if bar_button(ui, Icon::Close, "Remove from the list", removable, false).clicked()
      && let Some(p) = picked
    {
      *act = Some(Act::Remove(p.record.id.clone()));
    }
  });
  ui.add_space(4.0);
  // the header: Name sorted A to Z (the arrow up)
  let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), egui::Sense::hover());
  let font = egui::FontId::proportional(12.0);
  let p = ui.painter();
  let name = p.text(head.left_center() + vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, "Name", font.clone(), theme::MUTED);
  let a = pos2(name.right() + 8.0, head.center().y);
  p.add(egui::Shape::convex_polygon(
    vec![a + vec2(-3.5, 2.0), a + vec2(3.5, 2.0), a + vec2(0.0, -2.5)],
    theme::MUTED,
    egui::Stroke::NONE,
  ));
  for (x, t) in [(RUN_X, "Run"), (STARTUP_X, "Run on Startup"), (VERSION_X, "Version")] {
    p.text(pos2(head.left() + x, head.center().y), egui::Align2::LEFT_CENTER, t, font.clone(), theme::MUTED);
  }
  p.line_segment([head.left_bottom(), head.right_bottom()], egui::Stroke::new(1.0, Color32::from_gray(220)));
  let shown = rows(st, list);
  egui::ScrollArea::vertical().id_salt("scripts-addins-table").auto_shrink([false, false]).show(ui, |ui| {
    for p in shown {
      let id = &p.record.id;
      let selected = st.selected.as_deref() == Some(id.as_str());
      let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), egui::Sense::click());
      if selected {
        ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(214, 232, 250));
      } else if r.hovered() {
        ui.painter().rect_filled(rect, 0.0, theme::HOVER);
      }
      let color = if p.error.is_some() { Color32::from_rgb(200, 40, 40) } else { theme::TEXT };
      let font = egui::FontId::proportional(12.5);
      let name = ui.painter().layout_no_wrap(p.name().to_string(), font.clone(), color);
      let clip = egui::Rect::from_min_max(rect.left_top(), pos2(rect.left() + RUN_X - 6.0, rect.bottom()));
      ui.painter().with_clip_rect(clip).galley(
        pos2(rect.left() + 6.0, rect.center().y - name.size().y / 2.0),
        name,
        color,
      );
      let run =
        switch(ui, pos2(rect.left() + RUN_X + 14.0, rect.center().y), p.record.enabled, ui.id().with(("run", id)));
      if run.on_hover_text(if p.record.enabled { "Stop" } else { "Run" }).clicked() {
        *act = Some(Act::Run(id.clone(), !p.record.enabled));
      }
      let cb = egui::Rect::from_center_size(pos2(rect.left() + STARTUP_X + 10.0, rect.center().y), vec2(18.0, 18.0));
      let mut on = p.startup;
      if ui.put(cb, egui::Checkbox::without_text(&mut on)).changed() {
        *act = Some(Act::Startup(id.clone(), on));
      }
      ui.painter().text(
        pos2(rect.left() + VERSION_X, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &p.record.version,
        font,
        theme::TEXT,
      );
      if r.clicked() {
        st.selected = Some(id.clone());
      }
    }
  });
}

fn right_pane(ui: &mut egui::Ui, st: &mut PluginsUi, list: &[fox_plugin::Installed], log: &str, act: &mut Option<Act>) {
  ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
  let picked = st.selected.as_ref().and_then(|id| list.iter().find(|p| &p.record.id == id));
  egui::ScrollArea::vertical().id_salt("scripts-addins-details").auto_shrink([false, false]).show(ui, |ui| {
    ui.set_max_width(ui.available_width() - 6.0);
    match picked {
      Some(p) => details(ui, st, p, log, act),
      None if st.hub.is_some() => hub_list(ui, st, act),
      None => {
        ui.add_space(PANES_H / 2.0 - 20.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("Select an item to see details here").color(theme::MUTED)));
      }
    }
  });
}

/// The data hub's plugins (the "+" menu's Download add-in from marketplace).
fn hub_list(ui: &mut egui::Ui, st: &PluginsUi, act: &mut Option<Act>) {
  ui.label(RichText::new("In the data hub").strong().size(14.0));
  match &st.hub {
    Some(Err(e)) => {
      ui.colored_label(Color32::from_rgb(200, 40, 40), e);
    }
    Some(Ok(list)) if list.is_empty() => {
      ui.label(
        RichText::new("No plugins in the data hub. Publish one with: fissionoxide-cli plugin publish FILE")
          .color(theme::MUTED),
      );
    }
    Some(Ok(list)) => {
      for h in list {
        ui.horizontal(|ui| {
          ui.label(RichText::new(&h.name).strong());
          ui.label(RichText::new(&h.version).color(theme::MUTED));
          ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = match (&h.installed, h.update) {
              (None, _) => Some("Install"),
              (Some(_), true) => Some("Update"),
              (Some(_), false) => None,
            };
            match label {
              Some(l) => {
                if ui.button(l).clicked() {
                  *act = Some(Act::FromHub(h.item.clone(), h.hub_version));
                }
              }
              None => {
                ui.label(RichText::new("installed").color(theme::MUTED));
              }
            }
          });
        });
        if !h.description.is_empty() {
          ui.label(RichText::new(&h.description).color(theme::MUTED).size(11.5));
        }
      }
    }
    None => {}
  }
}

/// A collapsible group of the details pane, open at first unless `closed`.
fn section(ui: &mut egui::Ui, name: &str, closed: bool, add: impl FnOnce(&mut egui::Ui)) {
  egui::CollapsingHeader::new(RichText::new(name).strong())
    .id_salt(("scripts-addins-section", name))
    .default_open(!closed)
    .show(ui, add);
}

/// The picked add-in: its name and star, a picture, then Information, Description and Full Path
/// as in Fusion, and FissionOxide's own Commands, Permissions and Log.
fn details(ui: &mut egui::Ui, st: &mut PluginsUi, p: &fox_plugin::Installed, log: &str, act: &mut Option<Act>) {
  let id = &p.record.id;
  ui.horizontal(|ui| {
    ui.label(RichText::new(p.name()).strong().size(15.0));
    let fav = st.favorites.contains(id);
    let (r, resp) = ui.allocate_exact_size(egui::Vec2::splat(20.0), egui::Sense::click());
    let color = if fav { Color32::from_rgb(240, 180, 20) } else { Color32::from_gray(170) };
    icons::draw(ui.painter(), r.shrink(2.0), Icon::Star, color);
    if resp.on_hover_text(if fav { "Remove from Favorites" } else { "Add to Favorites" }).clicked() {
      if fav {
        st.favorites.retain(|f| f != id);
      } else {
        st.favorites.push(id.clone());
      }
    }
  });
  // the picture: the add-in's first command icon, else the Scripts and Add-Ins icon
  let (pic, _) = ui.allocate_exact_size(vec2(ui.available_width(), 90.0), egui::Sense::hover());
  ui.painter().rect_filled(pic, 3.0, Color32::from_gray(245));
  let ir = egui::Rect::from_center_size(pic.center(), egui::Vec2::splat(64.0));
  let first = p.manifest.as_ref().and_then(|m| m.commands.iter().find_map(|c| c.icon.as_ref())).map(|i| p.dir.join(i));
  match first.and_then(|f| st.icon(ui.ctx(), &f)) {
    Some(tex) => {
      ui.painter().image(tex.id(), ir, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    None => icons::draw_color(ui.painter(), ir, Icon::Scripts),
  }
  let m = p.manifest.as_ref();
  section(ui, "Information", false, |ui| {
    egui::Grid::new(("scripts-addins-info", id)).num_columns(2).spacing(vec2(10.0, 4.0)).show(ui, |ui| {
      let author = m.map(|m| m.author.as_str()).filter(|a| !a.is_empty()).unwrap_or("Unknown");
      for (k, v) in [
        ("Author", author),
        ("Version", p.record.version.as_str()),
        ("Supported OS", "Windows, Mac, Linux"),
        ("Programming Language", "WebAssembly"),
      ] {
        ui.label(RichText::new(k).color(theme::MUTED));
        ui.label(v);
        ui.end_row();
      }
    });
  });
  section(ui, "Description", false, |ui| {
    match m.map(|m| m.description.as_str()).filter(|d| !d.is_empty()) {
      Some(d) => ui.label(drawable(d)),
      None => ui.label(RichText::new("No description").color(theme::MUTED)),
    };
    if let Some(e) = &p.error {
      ui.colored_label(Color32::from_rgb(200, 40, 40), format!("Not loaded: {e}"));
    }
    if let Some(home) = m.map(|m| m.homepage.as_str()).filter(|h| !h.is_empty()) {
      ui.hyperlink(home);
    }
  });
  section(ui, "Full Path", false, |ui| {
    ui.add(egui::Label::new(RichText::new(p.dir.display().to_string()).size(11.5)).wrap());
  });
  if ui.link("Open File Location").clicked() {
    *act = Some(Act::Reveal(p.dir.clone()));
  }
  let Some(m) = m else { return };
  section(ui, "Commands", true, |ui| {
    for c in &m.commands {
      ui.label(command_line(m, c));
    }
  });
  if !m.permissions.is_empty() {
    section(ui, "Permissions", true, |ui| {
      let mut granted = p.record.granted.clone();
      let mut changed = false;
      for perm in &m.permissions {
        let mut on = granted.contains(perm);
        let r = ui.add_enabled(!p.bundled, egui::Checkbox::new(&mut on, perm.describe()));
        if r.changed() {
          changed = true;
          granted.retain(|g| g != perm);
          if on {
            granted.push(*perm);
          }
        }
      }
      if changed {
        *act = Some(Act::Grant(id.clone(), granted));
      }
    });
  }
  section(ui, "Log", true, |ui| {
    if !log.is_empty() && ui.small_button("Clear").clicked() {
      *act = Some(Act::ClearLog(id.clone()));
    }
    egui::ScrollArea::vertical().id_salt(("plugin-log", id)).max_height(120.0).stick_to_bottom(true).show(ui, |ui| {
      let text = if log.is_empty() { "(empty)" } else { log };
      ui.label(RichText::new(text).monospace().size(11.0));
    });
  });
}

/// A command's line in the details and the install window: "Clean Up Sketch (LumenArt > PREPARE)",
/// then its tooltip.
fn command_line(m: &fox_plugin::Manifest, c: &fox_plugin::manifest::Command) -> String {
  let (tab, panel) = m.placement(c);
  let tip = if c.tooltip.is_empty() { String::new() } else { format!(": {}", c.tooltip) };
  drawable(&format!("• {} ({tab} > {panel}){tip}", c.label()))
}

/// A plugin's text with ">" for the arrows the UI font has no glyph for (it would draw a box).
fn drawable(text: &str) -> String {
  text.replace(['→', '➔', '⇒'], ">")
}

enum Act {
  PickFile,
  Hub,
  FromHub(String, u32),
  Run(String, bool),
  Startup(String, bool),
  Remove(String),
  Grant(String, Vec<Permission>),
  ClearLog(String),
  /// Opens a file or folder in the system's file manager.
  Reveal(PathBuf),
  /// The pencil or folder with nothing picked.
  Hint,
}

fn apply(app: &mut App, ctx: &egui::Context, a: Act) {
  let hub = app.hub_root();
  let result = match a {
    Act::PickFile => {
      pick_file(app);
      return;
    }
    Act::Hub => {
      let list = fox_engine::hub::open_store(&app.s)
        .map_err(|e| e.0)
        .map(|st| fox_plugin::hub::available(app.plugins.registry(&hub), &st));
      app.plugins.hub = Some(list);
      app.plugins.selected = None;
      return;
    }
    Act::Reveal(path) => {
      let url = format!("file://{}", path.display());
      ctx.open_url(egui::OpenUrl::new_tab(url));
      return;
    }
    Act::Hint => {
      app.plugins.message = Some(("Select a script or add-in first".into(), false));
      return;
    }
    Act::FromHub(item, n) => {
      let pkg = fox_engine::hub::open_store(&app.s)
        .map_err(|e| e.0)
        .and_then(|st| st.read_version(&item, Some(n)).map_err(|e| e.to_string()))
        .and_then(|b| Package::read(&b));
      match pkg {
        Ok(pkg) => offer(app, pkg, format!("hub:{item}/{n}")),
        Err(e) => app.plugins.message = Some((e, true)),
      }
      return;
    }
    Act::Run(id, on) => app.plugins.registry(&hub).set_running(&id, on),
    Act::Startup(id, on) => app.plugins.registry(&hub).set_run_on_startup(&id, on),
    Act::Remove(id) => {
      let r = app.plugins.registry(&hub).uninstall(&id);
      if r.is_ok() {
        app.plugins.message = Some((format!("Removed {id}"), false));
      }
      r
    }
    Act::Grant(id, g) => app.plugins.registry(&hub).set_granted(&id, &g),
    Act::ClearLog(id) => {
      app.plugins.registry(&hub).clear_log(&id);
      Ok(())
    }
  };
  if let Err(e) = result {
    app.plugins.message = Some((e, true));
  }
  app.plugins.refresh();
  app.plugins.hub = None;
  // a tab whose plugin went away
  if let panels::Tab::Plugin(i) = app.tab
    && usize::from(i) >= app.plugins.tabs.len()
  {
    app.tab = panels::Tab::Solid;
  }
}

// ---------------------------------------------------------------- toolbar

/// The tab's label.
pub(crate) fn tab_label(app: &App, i: u8) -> String {
  app.plugins.tabs.get(usize::from(i)).map(|t| t.to_uppercase()).unwrap_or_default()
}

/// The tool groups of a plugin tab: one per panel, its commands as icons.
pub(crate) fn toolbar(app: &mut App, ui: &mut egui::Ui, i: u8) {
  let Some(tab) = app.plugins.tabs.get(usize::from(i)).cloned() else {
    app.tab = panels::Tab::Solid;
    return;
  };
  let cmds: Vec<CommandRef> = app.plugins.cmds.iter().filter(|c| c.tab == tab).cloned().collect();
  let mut panels_in_order: Vec<String> = Vec::new();
  for c in &cmds {
    if !panels_in_order.contains(&c.panel) {
      panels_in_order.push(c.panel.clone());
    }
  }
  let mut pick: Option<CommandRef> = None;
  for panel in panels_in_order {
    let group: Vec<&CommandRef> = cmds.iter().filter(|c| c.panel == panel).collect();
    let promoted: Vec<&CommandRef> = group.iter().copied().filter(|c| c.command.promoted).collect();
    let quick = if promoted.is_empty() { group.clone() } else { promoted };
    let popup_id = egui::Id::new(("plugin-menu", &tab, &panel));
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let inner = ui.vertical(|ui| {
      ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
      ui.add_space(2.0);
      ui.horizontal(|ui| {
        for c in &quick {
          let active = app.plugins.dialog.as_ref().is_some_and(|d| d.cmd.full_id == c.full_id);
          let resp = tool_button(app, ui, c, active);
          if resp.clicked() {
            pick = Some((*c).clone());
          }
        }
      });
      ui.add_space(3.0);
      let w = ui.min_rect().width();
      panels::group_label(ui, &panel, w, open)
    });
    egui::Popup::menu(&inner.inner).id(popup_id).show(|ui| {
      for c in &group {
        if ui.button(c.command.label()).on_hover_text(&c.command.tooltip).clicked() {
          pick = Some((*c).clone());
        }
      }
    });
    panels::separator(ui);
  }
  ui.vertical(|ui| {
    ui.add_space(2.0);
    let resp = icons::tool_button_bare(ui, icons::Icon::Scripts, icons::Tint::Blue, app.plugins.open)
      .on_hover_text("Manage plugins");
    if resp.clicked() {
      app.plugins.open = true;
    }
    ui.add_space(3.0);
    let w = ui.min_rect().width();
    let _ = panels::group_label(ui, "PLUGINS", w.max(60.0), false);
  });
  if let Some(c) = pick {
    start(app, c);
  }
}

fn tool_button(app: &mut App, ui: &mut egui::Ui, c: &CommandRef, active: bool) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(35.0, 38.0), egui::Sense::click());
  if active {
    ui.painter().rect_filled(rect, 2.0, theme::ACCENT);
  } else if resp.hovered() {
    ui.painter().rect(rect, 2.0, theme::HOVER, egui::Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
  }
  let ic = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(27.0));
  match c.icon.as_ref().and_then(|p| app.plugins.icon(ui.ctx(), p)) {
    Some(tex) => {
      ui.painter().image(tex.id(), ic, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    None => {
      // no icon: the label's first letter on a badge
      ui.painter().rect_filled(ic.shrink(2.0), 4.0, if active { Color32::WHITE } else { theme::ACCENT });
      let letter: String = c.command.label().chars().take(1).collect();
      let color = if active { theme::ACCENT } else { Color32::WHITE };
      ui.painter().text(ic.center(), egui::Align2::CENTER_CENTER, letter, egui::FontId::proportional(15.0), color);
    }
  }
  // a workflow step carries its number
  if let Some(n) = c.command.step {
    let badge = egui::Rect::from_min_size(rect.left_top() + vec2(1.0, 1.0), vec2(13.0, 13.0));
    ui.painter().rect_filled(badge, 3.0, Color32::from_rgb(250, 204, 21));
    ui.painter().text(
      badge.center(),
      egui::Align2::CENTER_CENTER,
      n.to_string(),
      egui::FontId::proportional(9.5),
      Color32::from_gray(40),
    );
  }
  let tip = if c.command.tooltip.is_empty() {
    format!("{}\n({})", c.command.label(), c.plugin_name)
  } else {
    format!("{}\n{}\n({})", c.command.label(), c.command.tooltip, c.plugin_name)
  };
  resp.on_hover_text(tip)
}

impl PluginsUi {
  /// A plugin's icon file as a texture (None when it doesn't load).
  fn icon(&mut self, ctx: &egui::Context, path: &Path) -> Option<TextureHandle> {
    self
      .icons
      .entry(path.to_path_buf())
      .or_insert_with(|| {
        let img = image::open(path).ok()?.to_rgba8();
        let color =
          egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        Some(ctx.load_texture(path.to_string_lossy(), color, egui::TextureOptions::LINEAR))
      })
      .clone()
  }
}

// ---------------------------------------------------------------- command dialogs

/// Opens a plugin command's dialog; a built-in command without inputs just runs.
fn start(app: &mut App, cmd: CommandRef) {
  app.cancel_dialog();
  if cmd.command.builtin.is_some() && cmd.command.inputs.is_empty() {
    app.plugins.dialog =
      Some(CommandDialog { cmd, values: Map::new(), over: HashMap::new(), active_sel: None, error: None });
    run(app);
    if app.plugins.dialog.as_ref().is_some_and(|d| d.cmd.command.inputs.is_empty() && d.cmd.command.builtin.is_some()) {
      // a failed run shows its error in the status bar, not an empty dialog
      app.plugins.dialog = None;
    }
    return;
  }
  let mut values = Map::new();
  for i in cmd.command.inputs.iter().filter(|i| i.kind.has_value()) {
    let v = fox_plugin::inputs::initial(i);
    let v = match (i.kind, v) {
      (InputKind::Value, Value::Number(n)) => json!(n.to_string()),
      (_, v) => v,
    };
    values.insert(i.id.clone(), v);
  }
  let active_sel = cmd.command.inputs.iter().find(|i| i.kind == InputKind::Selection).map(|i| i.id.clone());
  app.plugins.dialog = Some(CommandDialog { cmd, values, over: HashMap::new(), active_sel, error: None });
  // the plugin sets up the dialog (labels, linked values)
  changed(app, "");
}

/// Tells the plugin an input changed and applies the updates it returns.
fn changed(app: &mut App, input: &str) {
  let hub = app.hub_root();
  let Some(d) = app.plugins.dialog.take() else { return };
  let mut d = d;
  let r = app.plugins.registry(&hub).input_changed(&mut app.s, &d.cmd.full_id, input, &Value::Object(d.values.clone()));
  match r {
    Ok(Value::Object(updates)) => {
      for (id, u) in updates {
        let Some(input) = d.cmd.command.inputs.iter().find(|i| i.id == id) else { continue };
        if let Some(v) = u.get("value") {
          let v = match (input.kind, v) {
            (InputKind::Value, Value::Number(n)) => json!(n.to_string()),
            (_, v) => v.clone(),
          };
          d.values.insert(id.clone(), v);
        }
        let o = d.over.entry(id).or_default();
        if let Some(b) = u.get("visible").and_then(Value::as_bool) {
          o.visible = Some(b);
        }
        if let Some(b) = u.get("enabled").and_then(Value::as_bool) {
          o.enabled = Some(b);
        }
        if let Some(t) = u.get("text").and_then(Value::as_str) {
          o.text = Some(t.to_string());
        }
        if let Some(t) = u.get("label").and_then(Value::as_str) {
          o.label = Some(t.to_string());
        }
        if let Some(opts) = u.get("options").and_then(Value::as_array) {
          o.options = Some(opts.iter().filter_map(|x| x.as_str().map(str::to_string)).collect());
        }
      }
      d.error = None;
    }
    Ok(_) => {}
    Err(e) => d.error = Some(e),
  }
  app.plugins.dialog = Some(d);
}

fn command_dialog(app: &mut App, ctx: &egui::Context) {
  let Some(mut d) = app.plugins.dialog.take() else { return };
  // the active selection input follows the viewport selection
  let mut sel_changed = None;
  if let Some(id) = d.active_sel.clone()
    && let Some(input) = d.cmd.command.inputs.iter().find(|i| i.id == id)
  {
    let picks: Vec<Value> = app
      .s
      .selection
      .iter()
      .filter(|p| fox_plugin::inputs::accepts(input, p, &app.s))
      .filter_map(|p| serde_json::to_value(p).ok())
      .collect();
    if d.values.get(&id) != Some(&json!(picks)) {
      d.values.insert(id.clone(), json!(picks));
      sel_changed = Some(id);
    }
  }
  let mut keep = true;
  let mut ok = false;
  let mut edited: Option<String> = sel_changed;
  let mut pick_folder = false;
  let title = d.cmd.command.label().to_string();
  egui::Window::new(&title)
    .id(egui::Id::new(("plugin-dialog", &d.cmd.full_id)))
    .collapsible(false)
    .resizable(false)
    .default_pos(pos2(900.0, 140.0))
    .show(ctx, |ui| {
      ui.label(RichText::new(&d.cmd.plugin_name).color(theme::MUTED).size(11.0));
      egui::Grid::new(("plugin-inputs", &d.cmd.full_id)).num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
        let inputs = d.cmd.command.inputs.clone();
        for i in inputs.iter().filter(|i| i.group.is_none()) {
          if i.kind == InputKind::Group {
            if !shown(&d, i) {
              continue;
            }
            ui.label(RichText::new(label_of(&d, i)).strong());
            ui.end_row();
            for member in inputs.iter().filter(|m| m.group.as_deref() == Some(i.id.as_str())) {
              row(ui, &mut d, member, &mut edited);
            }
          } else {
            row(ui, &mut d, i, &mut edited);
          }
        }
      });
      if d.cmd.uses_files {
        ui.horizontal(|ui| {
          let folder =
            app.plugins.folder.as_ref().map(|f| f.display().to_string()).unwrap_or_else(|| "(not chosen)".into());
          ui.label("Folder").on_hover_text("Where the plugin reads and writes files");
          ui.label(RichText::new(folder).color(theme::MUTED));
          if ui.small_button("Choose…").clicked() {
            pick_folder = true;
          }
        });
      }
      if let Some(e) = &d.error {
        ui.colored_label(Color32::from_rgb(200, 40, 40), format!("⚠ {e}"));
      }
      ui.add_space(4.0);
      ui.horizontal(|ui| {
        if ui.button("OK").clicked() {
          ok = true;
        }
        if ui.button("Cancel").clicked() {
          keep = false;
        }
      });
    });
  if pick_folder && let Some(f) = rfd::FileDialog::new().set_title("Folder for the plugin's files").pick_folder() {
    app.plugins.folder = Some(f);
  }
  app.plugins.dialog = Some(d);
  if let Some(id) = edited {
    changed(app, &id);
  }
  if ok {
    run(app);
  } else if !keep {
    app.plugins.dialog = None;
  }
}

fn shown(d: &CommandDialog, i: &Input) -> bool {
  let o = d.over.get(&i.id);
  o.and_then(|o| o.visible).unwrap_or(true)
    && condition_holds(i.visible_when.as_ref(), &d.values)
    && i.group.as_ref().is_none_or(|g| {
      d.cmd
        .command
        .inputs
        .iter()
        .find(|x| &x.id == g)
        .is_none_or(|g| condition_holds(g.visible_when.as_ref(), &d.values))
    })
}

fn label_of(d: &CommandDialog, i: &Input) -> String {
  d.over.get(&i.id).and_then(|o| o.label.clone()).unwrap_or_else(|| i.label().to_string())
}

/// One input's row; records its id in `edited` when the user changes it.
fn row(ui: &mut egui::Ui, d: &mut CommandDialog, i: &Input, edited: &mut Option<String>) {
  if !shown(d, i) {
    return;
  }
  let enabled =
    d.over.get(&i.id).and_then(|o| o.enabled).unwrap_or(true) && condition_holds(i.enabled_when.as_ref(), &d.values);
  let label = label_of(d, i);
  let resp = ui.label(&label);
  if !i.prompt.is_empty() {
    resp.on_hover_text(&i.prompt);
  }
  let before = d.values.get(&i.id).cloned();
  ui.add_enabled_ui(enabled, |ui| match i.kind {
    InputKind::Selection => {
      let n = d.values.get(&i.id).and_then(Value::as_array).map_or(0, Vec::len);
      let active = d.active_sel.as_deref() == Some(i.id.as_str());
      ui.horizontal(|ui| {
        let text = match n {
          0 => {
            if i.prompt.is_empty() {
              "Select".to_string()
            } else {
              i.prompt.clone()
            }
          }
          n => format!("{n} selected"),
        };
        if ui.selectable_label(active, text).on_hover_text("Pick in the viewport while this is highlighted").clicked() {
          d.active_sel = Some(i.id.clone());
        }
      });
    }
    InputKind::Value => {
      let mut text = match d.values.get(&i.id) {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
      };
      if ui.add(egui::TextEdit::singleline(&mut text).desired_width(120.0)).changed() {
        d.values.insert(i.id.clone(), json!(text));
      }
    }
    InputKind::Integer => {
      let mut n = d.values.get(&i.id).and_then(Value::as_i64).unwrap_or(0);
      let lo = i.min.map_or(i64::MIN, |m| m.ceil() as i64);
      let hi = i.max.map_or(i64::MAX, |m| m.floor() as i64);
      if ui.add(egui::DragValue::new(&mut n).range(lo..=hi).speed(i.step.unwrap_or(0.1))).changed() {
        d.values.insert(i.id.clone(), json!(n));
      }
    }
    InputKind::Slider => {
      let mut x = d.values.get(&i.id).and_then(Value::as_f64).unwrap_or(0.0);
      let (lo, hi) = (i.min.unwrap_or(0.0), i.max.unwrap_or(1.0));
      let mut s = egui::Slider::new(&mut x, lo..=hi);
      if let Some(st) = i.step {
        s = s.step_by(st);
      }
      if ui.add(s).changed() {
        d.values.insert(i.id.clone(), json!(x));
      }
    }
    InputKind::Checkbox => {
      let mut b = d.values.get(&i.id).and_then(Value::as_bool).unwrap_or(false);
      if ui.checkbox(&mut b, "").changed() {
        d.values.insert(i.id.clone(), json!(b));
      }
    }
    InputKind::Choice => {
      let options = d.over.get(&i.id).and_then(|o| o.options.clone()).unwrap_or_else(|| i.options.clone());
      let cur = d.values.get(&i.id).and_then(Value::as_str).unwrap_or_default().to_string();
      egui::ComboBox::from_id_salt(("plugin-choice", &i.id)).selected_text(&cur).show_ui(ui, |ui| {
        for o in options {
          if ui.selectable_label(o == cur, &o).clicked() {
            d.values.insert(i.id.clone(), json!(o));
          }
        }
      });
    }
    InputKind::Text => {
      let mut text = d.values.get(&i.id).and_then(Value::as_str).unwrap_or_default().to_string();
      if ui.add(egui::TextEdit::singleline(&mut text).desired_width(160.0)).changed() {
        d.values.insert(i.id.clone(), json!(text));
      }
    }
    InputKind::Button => {
      if ui.button(&label).clicked() {
        *edited = Some(i.id.clone());
      }
    }
    InputKind::Label => {
      let text = d
        .over
        .get(&i.id)
        .and_then(|o| o.text.clone())
        .unwrap_or_else(|| i.default.as_ref().and_then(Value::as_str).unwrap_or_default().to_string());
      ui.label(RichText::new(text).color(theme::MUTED));
    }
    InputKind::Group => {}
  });
  ui.end_row();
  if d.values.get(&i.id) != before.as_ref() {
    *edited = Some(i.id.clone());
  }
}

/// OK: runs the command; the dialog stays open for repeatable commands.
fn run(app: &mut App) {
  let Some(mut d) = app.plugins.dialog.take() else { return };
  if app.rebuild.is_some() {
    app.status = Some(("Still computing — wait for it to finish".into(), false));
    app.plugins.dialog = Some(d);
    return;
  }
  let hub = app.hub_root();
  let values = Value::Object(d.values.clone());
  let opts = RunOptions { folder: app.plugins.folder.clone(), ..Default::default() };
  let mut r = app.plugins.registry(&hub).run(&mut app.s, &d.cmd.full_id, &values, opts);
  // it wants files and no folder is chosen yet: ask, and run again (the failed run was undone)
  if matches!(&r, Err(e) if e.contains(fox_plugin::sandbox::NO_FOLDER))
    && let Some(f) = rfd::FileDialog::new().set_title("Folder for the plugin's files").pick_folder()
  {
    app.plugins.folder = Some(f);
    let opts = RunOptions { folder: app.plugins.folder.clone(), ..Default::default() };
    r = app.plugins.registry(&hub).run(&mut app.s, &d.cmd.full_id, &values, opts);
  }
  match r {
    Ok(r) => {
      let mut msg = format!("{} done", d.cmd.command.label());
      if let Some(m) = r.result.get("message").and_then(Value::as_str) {
        msg = format!("{msg}: {m}");
      }
      if !r.files.is_empty() {
        msg.push_str(&format!(" (wrote {} file{})", r.files.len(), if r.files.len() == 1 { "" } else { "s" }));
      }
      app.status = Some((msg, false));
      if d.cmd.command.repeatable {
        d.error = None;
        app.plugins.dialog = Some(d);
      }
    }
    Err(e) => {
      app.status = Some((format!("{}: {e}", d.cmd.command.label()), true));
      d.error = Some(e);
      app.plugins.dialog = Some(d);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/flange")
  }

  #[test]
  fn install_and_run_a_plugin_from_the_ui() {
    let mut app = App::with_session(fox_engine::Session::default());
    let ctx = egui::Context::default();
    // a dropped package waits for the user to confirm
    let pkg = Package::from_dir(&example()).unwrap();
    offer(&mut app, pkg, "test".into());
    assert!(app.plugins.install.as_ref().is_some_and(|p| p.plan == Plan::Install && p.grant.len() == 1));
    let pending = app.plugins.install.take().unwrap();
    let hub = app.hub_root();
    let opts = InstallOptions { grant: Some(vec![]), ..Default::default() };
    app.plugins.registry(&hub).install(&pending.pkg, opts).unwrap();
    app.plugins.refresh();
    assert_eq!(app.plugins.tabs, vec!["Flange Tools".to_string()]);
    assert_eq!(tab_label(&app, 0), "FLANGE TOOLS");
    let cmd = app.plugins.cmds[0].clone();
    start(&mut app, cmd);
    {
      let d = app.plugins.dialog.as_ref().unwrap();
      assert_eq!(d.values["outer"], "100 mm");
      assert_eq!(d.over["spacing"].text.as_deref(), Some("A hole every 60°"), "the plugin filled in the hint");
    }
    if let Some(d) = app.plugins.dialog.as_mut() {
      d.values.insert("holes".into(), json!(4));
    }
    changed(&mut app, "holes");
    assert_eq!(
      app.plugins.dialog.as_ref().and_then(|d| d.over["spacing"].text.clone()).as_deref(),
      Some("A hole every 90°")
    );
    run(&mut app);
    assert_eq!(app.s.model.bodies.len(), 1, "{:?}", app.status);
    assert!(app.plugins.dialog.is_some(), "Bolt Flange is repeatable");
    // the plugin's tab and dialog draw with the rest of the app
    app.tab = panels::Tab::Plugin(0);
    let input = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1440.0, 900.0))),
      ..Default::default()
    };
    ctx.run_ui(input, |ui| app.draw(ui)).drop_without_applying_deltas();
    assert_eq!(app.tab, panels::Tab::Plugin(0));
    assert!(app.plugins.dialog.is_some());
    assert!(app.s.undo());
    assert!(app.s.model.bodies.is_empty());
  }

  /// The text one frame of the whole app draws.
  fn drawn(app: &mut App, ctx: &egui::Context) -> Vec<String> {
    fn walk(s: &egui::Shape, out: &mut Vec<String>) {
      match s {
        egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
        egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        _ => {}
      }
    }
    let input = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1920.0, 1080.0))),
      ..Default::default()
    };
    // a few frames: dialogs open on the frame after they are started
    for _ in 0..3 {
      ctx.run_ui(input.clone(), |ui| app.draw(ui)).drop_without_applying_deltas();
    }
    let out = ctx.run_ui(input, |ui| app.draw(ui));
    let mut all = Vec::new();
    for c in &out.shapes {
      walk(&c.shape, &mut all);
    }
    out.drop_without_applying_deltas();
    all
  }

  /// The LumenArt add-in (addins/lumenart) is off until turned on; then its tab lists the sign
  /// workflow in step order, built-in tools included, and its dialogs draw from the manifest.
  #[test]
  fn the_lumenart_tab_lists_the_sign_workflow() {
    let hub = std::env::temp_dir().join(format!("fox-ui-lumenart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&hub);
    let mut s = fox_engine::Session::default();
    s.store_root = Some(hub.clone());
    let mut app = App::with_session(s);
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let id = "shop.lumenart.signs";
    let hub = app.hub_root();
    if app.plugins.registry(&hub).get(id).is_none() {
      return; // a build without the repository's addins/
    }
    assert!(!app.plugins.tabs.iter().any(|t| t == "LumenArt"), "off until turned on");
    app.plugins.registry(&hub).set_enabled(id, true).unwrap();
    app.plugins.refresh();
    let i = app.plugins.tabs.iter().position(|t| t == "LumenArt").expect("the LumenArt tab");
    let on_tab: Vec<CommandRef> = app.plugins.cmds.iter().filter(|c| c.tab == "LumenArt").cloned().collect();
    let labels: Vec<(Option<u32>, &str)> = on_tab.iter().map(|c| (c.command.step, c.command.label())).collect();
    let want = [
      "Clean Up Sketch",
      "Remove Duplicate Lines",
      "Size Sign",
      "Place Hardware",
      "New Layer",
      "Quick Tab",
      "Auto Flange",
      "Slot Projector",
      "Tab Builder",
      "Collect Flat Patterns",
      "Check for SendCutSend",
      "Cut Estimate",
      "Explode Layers",
      "Render Parts",
      "Auto Export",
    ];
    for (k, w) in want.iter().enumerate() {
      assert_eq!(labels[k], (Some(k as u32 + 1), *w), "step {} of {labels:?}", k + 1);
    }
    assert!(on_tab.iter().all(|c| c.icon.as_ref().is_some_and(|p| p.is_file())), "every button has its icon");
    // the tab draws: its name, its panels in workflow order, a numbered badge per step
    app.tab = panels::Tab::Plugin(i as u8);
    let text = drawn(&mut app, &ctx);
    let at =
      |t: &str| text.iter().position(|x| x.starts_with(t)).unwrap_or_else(|| panic!("'{t}' not drawn: {text:?}"));
    assert!(at("LUMENART") < usize::MAX);
    let panel_order: Vec<usize> =
      ["PREPARE", "BUILD", "TABS", "CUT FILE", "EXPORT", "ANALYZE"].iter().map(|p| at(p)).collect();
    assert!(panel_order.windows(2).all(|w| w[0] < w[1]), "panels in workflow order: {panel_order:?}");
    for n in 1..=15 {
      assert!(text.iter().any(|t| t == &n.to_string()), "step badge {n}");
    }
    // New Layer: its dialog with LumenArt's defaults
    let new_layer = on_tab.iter().find(|c| c.command.key() == "new_layer").cloned().unwrap();
    start(&mut app, new_layer);
    {
      let d = app.plugins.dialog.as_ref().expect("New Layer opens a dialog");
      assert_eq!(d.values["thickness"], "0.063 in");
      assert_eq!(d.values["height"], "0 in");
      assert_eq!(d.active_sel.as_deref(), Some("profiles"), "picking profiles first");
    }
    let text = drawn(&mut app, &ctx);
    for t in ["New Layer", "Thickness", "Height", "Name"] {
      at_least(&text, t);
    }
    // Size Sign: fitting in a box shows the box, not the width
    let size = on_tab.iter().find(|c| c.command.key() == "size_sign").cloned().unwrap();
    start(&mut app, size);
    let text = drawn(&mut app, &ctx);
    for t in ["Size Sign", "Size by", "Box", "Turn to fit larger"] {
      at_least(&text, t);
    }
    assert!(!text.iter().any(|t| t == "Box width"), "custom box sizes only for Custom");
    // turning it off takes the tab away again
    app.plugins.registry(&hub).set_enabled(id, false).unwrap();
    app.plugins.refresh();
    assert!(!app.plugins.tabs.iter().any(|t| t == "LumenArt"));
    let _ = std::fs::remove_dir_all(&hub);
  }

  /// Scripts and Add-Ins lays out Fusion's three panes; LumenArt is an add-in row whose Run switch
  /// and Run on Startup checkbox are separate, and picking it fills the details pane with text
  /// the UI font can draw.
  #[test]
  fn scripts_and_add_ins_shows_fusions_panes_and_the_add_in_rows() {
    let hub = std::env::temp_dir().join(format!("fox-ui-addins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&hub);
    let mut s = fox_engine::Session::default();
    s.store_root = Some(hub.clone());
    let mut app = App::with_session(s);
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    panels::Cmd::Plugins.run(&mut app);
    let text = drawn(&mut app, &ctx);
    for t in [
      "Scripts and Add-Ins",
      "Search",
      "Browse",
      "Favorites",
      "All scripts and add-ins",
      "Filters",
      "Type",
      "Scripts",
      "Add-Ins",
      "Status",
      "Running",
      "Run on startup",
      "Source",
      "Installed",
      "From device",
      "Created by me",
      "Fusion",
      "Samples",
      "Name",
      "Run",
      "Run on Startup",
      "Version",
      "Select an item to see details here",
      "Close",
    ] {
      assert!(text.iter().any(|x| x == t), "'{t}' not drawn: {text:?}");
    }
    assert!(!text.iter().any(|x| x == "Plugins" || x.starts_with("Install from file")), "{text:?}");
    let id = "shop.lumenart.signs";
    let hub_root = app.hub_root();
    if app.plugins.registry(&hub_root).get(id).is_none() {
      return; // a build without the repository's addins/
    }
    assert!(text.iter().any(|x| x == "LumenArt"), "its row: {text:?}");
    // the filters: it is a bundled add-in, off until run
    let list = app.plugins.registry(&hub_root).list.clone();
    let names = |st: &PluginsUi| rows(st, &list).iter().map(|p| p.name().to_string()).collect::<Vec<_>>();
    assert!(names(&app.plugins).contains(&"LumenArt".to_string()));
    app.plugins.filter.kinds = [true, false];
    assert!(names(&app.plugins).is_empty(), "no scripts");
    app.plugins.filter.kinds = [false, true];
    app.plugins.filter.source = [false, false, false, true, false];
    assert!(names(&app.plugins).contains(&"LumenArt".to_string()), "shipped with the app");
    app.plugins.filter.status = [true, false];
    assert!(!names(&app.plugins).contains(&"LumenArt".to_string()), "not running");
    app.plugins.filter = Filter::default();
    app.plugins.search = "lumen".into();
    assert_eq!(names(&app.plugins), ["LumenArt"]);
    app.plugins.search = "zzz".into();
    assert!(names(&app.plugins).is_empty());
    app.plugins.search.clear();
    app.plugins.favorites_only = true;
    assert!(names(&app.plugins).is_empty(), "no favorites yet");
    app.plugins.favorites.push(id.into());
    assert_eq!(names(&app.plugins), ["LumenArt"]);
    app.plugins.favorites_only = false;
    // Run starts it now without making it start with the app
    apply(&mut app, &ctx, Act::Run(id.into(), true));
    let p = app.plugins.registry(&hub_root).get(id).unwrap();
    assert!(p.record.enabled && !p.startup);
    assert!(app.plugins.tabs.iter().any(|t| t == "LumenArt"), "its tab shows while it runs");
    apply(&mut app, &ctx, Act::Startup(id.into(), true));
    let p = app.plugins.registry(&hub_root).get(id).unwrap();
    assert!(p.record.enabled && p.startup);
    // the details
    app.plugins.selected = Some(id.into());
    let text = drawn(&mut app, &ctx);
    for t in ["Information", "Author", "Version", "Supported OS", "Programming Language", "Description", "Full Path"] {
      assert!(text.iter().any(|x| x == t), "'{t}' not drawn: {text:?}");
    }
    at_least(&text, "Open File Location");
    assert!(!text.iter().any(|x| x == "Select an item to see details here"));
    // no missing-glyph boxes: every character drawn, and every command line, is in the font
    let m = app.plugins.registry(&hub_root).get(id).unwrap().manifest.clone().unwrap();
    let lines: Vec<String> = m.commands.iter().map(|c| command_line(&m, c)).collect();
    assert!(lines.iter().any(|l| l.starts_with("• Clean Up Sketch (LumenArt > PREPARE)")), "{lines:?}");
    let font = egui::FontId::proportional(13.0);
    for t in text.iter().chain(&lines) {
      assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, t)), "a glyph the font lacks in '{t}'");
    }
    let _ = std::fs::remove_dir_all(&hub);
  }

  fn at_least(text: &[String], t: &str) {
    assert!(text.iter().any(|x| x == t || x.starts_with(t)), "'{t}' not drawn: {text:?}");
  }
}
