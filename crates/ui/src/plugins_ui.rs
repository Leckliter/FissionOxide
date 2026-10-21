//! Plugins in the UI: the Plugins window (File → Plugins, the app bar's Extensions button,
//! UTILITIES → ADD-INS), the install dialog (also for `.foxplugin` files dropped on the window),
//! a toolbar tab for each tab the installed plugins name, and the dialog each plugin command
//! opens, drawn from its manifest's inputs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Color32, RichText, TextureHandle, pos2, vec2};
use fox_plugin::hub::HubPlugin;
use fox_plugin::manifest::{InputKind, condition_holds};
use fox_plugin::{CommandRef, Input, InstallOptions, Package, Permission, Plan, Plugins, RunOptions};
use serde_json::{Map, Value, json};

use crate::{App, icons, panels, theme};

#[derive(Default)]
pub(crate) struct PluginsUi {
  /// The Plugins window is open.
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
  /// The plugin whose details are shown.
  expanded: Option<String>,
  icons: HashMap<PathBuf, Option<TextureHandle>>,
  /// The open plugin command dialog.
  dialog: Option<CommandDialog>,
  /// The folder plugins last read from and wrote to.
  folder: Option<PathBuf>,
  /// The Plugins window's last message (text, is an error).
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
  if let Some(p) = rfd::FileDialog::new().set_title("Install plugin").add_filter("FissionOxide plugin", &["foxplugin"]).pick_file()
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
        let (tab, panel) = m.placement(c);
        ui.label(format!("• {}   ({tab} → {panel})", c.label()));
      }
      ui.add_space(6.0);
      if pending.grant.is_empty() {
        ui.label("It asks for no permissions: it can only change the open design.");
      } else {
        ui.label(RichText::new("It asks to").strong());
        for (p, on) in &mut pending.grant {
          ui.checkbox(on, p.describe()).on_hover_text(p.id());
        }
        ui.label(RichText::new("Untick what you don't want to allow; commands that need it will fail.").color(theme::MUTED));
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

// ---------------------------------------------------------------- the Plugins window

pub(crate) fn window(app: &mut App, ctx: &egui::Context) {
  install_window(app, ctx);
  command_dialog(app, ctx);
  if !app.plugins.open {
    return;
  }
  let hub = app.hub_root();
  let mut open = true;
  let mut act: Option<Act> = None;
  {
    let ui_state = &mut app.plugins;
    let reg = ui_state.registry(&hub);
    let list: Vec<(fox_plugin::Installed, String)> = reg.list.iter().map(|p| (p.clone(), reg.log(&p.record.id))).collect();
    egui::Window::new("Plugins").open(&mut open).collapsible(false).resizable(true).default_width(560.0).show(ctx, |ui| {
      ui.horizontal(|ui| {
        if ui.button("Install from file…").on_hover_text("Install or update a plugin from a .foxplugin file").clicked() {
          act = Some(Act::PickFile);
        }
        if ui.button("Data hub…").on_hover_text("Plugins shared in your data hub: install them or get updates").clicked() {
          act = Some(Act::Hub);
        }
      });
      ui.label(
        RichText::new("You can also drop a .foxplugin file on the window.").color(theme::MUTED).size(11.5),
      );
      if let Some((m, err)) = &ui_state.message {
        ui.colored_label(if *err { Color32::from_rgb(200, 40, 40) } else { theme::ACCENT }, m);
      }
      ui.separator();
      egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
        if list.is_empty() {
          ui.label(RichText::new("No plugins installed.").color(theme::MUTED));
        }
        for (p, log) in &list {
          let id = p.record.id.clone();
          ui.horizontal(|ui| {
            let mut on = p.record.enabled;
            if ui.checkbox(&mut on, "").on_hover_text("Turn the plugin on or off").changed() {
              act = Some(Act::Enable(id.clone(), on));
            }
            ui.label(RichText::new(p.name()).strong());
            ui.label(RichText::new(&p.record.version).color(theme::MUTED));
            if p.bundled {
              ui.label(RichText::new("built in").color(theme::MUTED).size(11.0));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
              if !p.bundled && ui.button("Remove").clicked() {
                act = Some(Act::Remove(id.clone()));
              }
              let shown = ui_state.expanded.as_deref() == Some(id.as_str());
              if ui.button(if shown { "Hide" } else { "Details" }).clicked() {
                ui_state.expanded = if shown { None } else { Some(id.clone()) };
              }
            });
          });
          if let Some(e) = &p.error {
            ui.colored_label(Color32::from_rgb(200, 40, 40), format!("Not loaded: {e}"));
          }
          if ui_state.expanded.as_deref() == Some(id.as_str()) {
            details(ui, p, log, &mut act);
          }
          ui.separator();
        }
        if let Some(h) = &ui_state.hub {
          ui.label(RichText::new("In the data hub").strong());
          match h {
            Err(e) => {
              ui.colored_label(Color32::from_rgb(200, 40, 40), e);
            }
            Ok(list) if list.is_empty() => {
              ui.label(
                RichText::new("No plugins in the data hub. Publish one with: fissionoxide-cli plugin publish FILE")
                  .color(theme::MUTED),
              );
            }
            Ok(list) => {
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
                          act = Some(Act::FromHub(h.item.clone(), h.hub_version));
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
          }
        }
      });
    });
  }
  if !open {
    app.plugins.open = false;
  }
  if let Some(a) = act {
    apply(app, a);
  }
}

enum Act {
  PickFile,
  Hub,
  FromHub(String, u32),
  Enable(String, bool),
  Remove(String),
  Grant(String, Vec<Permission>),
  ClearLog(String),
}

fn details(ui: &mut egui::Ui, p: &fox_plugin::Installed, log: &str, act: &mut Option<Act>) {
  let id = &p.record.id;
  ui.indent(("plugin-details", id), |ui| {
    let Some(m) = &p.manifest else { return };
    if !m.description.is_empty() {
      ui.label(&m.description);
    }
    let mut facts = vec![format!("id {id}")];
    if !m.author.is_empty() {
      facts.push(format!("by {}", m.author));
    }
    if !p.record.source.is_empty() {
      facts.push(format!("from {}", p.record.source));
    }
    ui.label(RichText::new(facts.join(" · ")).color(theme::MUTED).size(11.5));
    if !m.homepage.is_empty() {
      ui.hyperlink(&m.homepage);
    }
    ui.label(RichText::new("Commands").strong());
    for c in &m.commands {
      let (tab, panel) = m.placement(c);
      ui.label(format!("• {} ({tab} → {panel}){}", c.label(), if c.tooltip.is_empty() { String::new() } else { format!(": {}", c.tooltip) }));
    }
    if !m.permissions.is_empty() {
      ui.label(RichText::new("Permissions").strong());
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
    }
    ui.horizontal(|ui| {
      ui.label(RichText::new("Log").strong());
      if !log.is_empty() && ui.small_button("Clear").clicked() {
        *act = Some(Act::ClearLog(id.clone()));
      }
    });
    egui::ScrollArea::vertical().id_salt(("plugin-log", id)).max_height(120.0).stick_to_bottom(true).show(ui, |ui| {
      let text = if log.is_empty() { "(empty)" } else { log };
      ui.label(RichText::new(text).monospace().size(11.0));
    });
  });
}

fn apply(app: &mut App, a: Act) {
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
    Act::Enable(id, on) => app.plugins.registry(&hub).set_enabled(&id, on),
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
  match c.icon.as_ref().and_then(|p| icon(app, ui.ctx(), p)) {
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
    ui.painter().text(badge.center(), egui::Align2::CENTER_CENTER, n.to_string(), egui::FontId::proportional(9.5), Color32::from_gray(40));
  }
  let tip = if c.command.tooltip.is_empty() {
    format!("{}\n({})", c.command.label(), c.plugin_name)
  } else {
    format!("{}\n{}\n({})", c.command.label(), c.command.tooltip, c.plugin_name)
  };
  resp.on_hover_text(tip)
}

fn icon(app: &mut App, ctx: &egui::Context, path: &Path) -> Option<TextureHandle> {
  app
    .plugins
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

// ---------------------------------------------------------------- command dialogs

/// Opens a plugin command's dialog; a built-in command without inputs just runs.
fn start(app: &mut App, cmd: CommandRef) {
  app.cancel_dialog();
  if cmd.command.builtin.is_some() && cmd.command.inputs.is_empty() {
    app.plugins.dialog = Some(CommandDialog { cmd, values: Map::new(), over: HashMap::new(), active_sel: None, error: None });
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
          let folder = app.plugins.folder.as_ref().map(|f| f.display().to_string()).unwrap_or_else(|| "(not chosen)".into());
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
  o.and_then(|o| o.visible).unwrap_or(true) && condition_holds(i.visible_when.as_ref(), &d.values)
    && i.group.as_ref().is_none_or(|g| {
      d.cmd.command.inputs.iter().find(|x| &x.id == g).is_none_or(|g| condition_holds(g.visible_when.as_ref(), &d.values))
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
  let enabled = d.over.get(&i.id).and_then(|o| o.enabled).unwrap_or(true) && condition_holds(i.enabled_when.as_ref(), &d.values);
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
          0 => if i.prompt.is_empty() { "Select".to_string() } else { i.prompt.clone() },
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
      let text = d.over.get(&i.id).and_then(|o| o.text.clone()).unwrap_or_else(|| {
        i.default.as_ref().and_then(Value::as_str).unwrap_or_default().to_string()
      });
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
    assert_eq!(app.plugins.dialog.as_ref().and_then(|d| d.over["spacing"].text.clone()).as_deref(), Some("A hole every 90°"));
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
}
