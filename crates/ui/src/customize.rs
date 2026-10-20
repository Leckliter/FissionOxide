//! Toolbar customisation, as in Fusion: a command's ⋮ options (Pin to Toolbar, Add to Shortcuts,
//! Change Keyboard Shortcut) and the user's keyboard shortcuts, kept in `settings.ini`.
//!
//! A command is known by its toolbar scope and label (`design.Extrude`, `sketch.Trim`); a pin by
//! the tab and group it sits in as well (`SOLID/CREATE/Shell`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{Key, Modifiers, RichText};
use fox_desktop::settings::Settings;

use crate::panels::Tab;
use crate::tips::Scope;
use crate::{App, theme};

impl Scope {
  pub(crate) fn key(self) -> &'static str {
    match self {
      Scope::Design => "design",
      Scope::Sketch => "sketch",
      Scope::Drawing => "drawing",
      Scope::Manufacture => "manufacture",
    }
  }

  /// The toolbar a tab's commands belong to.
  pub(crate) fn of(tab: Tab) -> Scope {
    match tab {
      Tab::Sketch => Scope::Sketch,
      Tab::Drawing => Scope::Drawing,
      Tab::Milling => Scope::Manufacture,
      _ => Scope::Design,
    }
  }
}

/// A command's id in the key map and the S toolbox's shortcuts.
pub(crate) fn cmd_id(scope: Scope, label: &str) -> String {
  format!("{}.{label}", scope.key())
}

/// A pinned toolbar icon's id.
pub(crate) fn pin_id(tab: Tab, group: &str, label: &str) -> String {
  format!("{}/{group}/{label}", tab.label())
}

/// A command that can have a keyboard shortcut.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Binding {
  pub id: String,
  pub scope: Scope,
  pub label: &'static str,
  /// The key it has unless the user changed it ("" for none).
  pub default: &'static str,
}

/// Keys the app keeps for itself: (combo, what it does, the toolbar it's taken on; None = all).
const RESERVED: &[(&str, &str, Option<Scope>)] = &[
  ("Ctrl+Z", "Undo", None),
  ("Ctrl+Y", "Redo", None),
  ("Ctrl+Shift+Z", "Redo", None),
  ("Ctrl+S", "Save", None),
  ("Ctrl+Shift+S", "Save As", None),
  ("Ctrl+O", "Open", None),
  ("Ctrl+N", "New Design", None),
  ("Escape", "Cancel", None),
  ("S", "the S toolbox", None),
  ("Enter", "Finish", None),
  ("Backspace", "Delete", None),
  ("F6", "Fit", None),
  ("Home", "Home View", None),
  ("V", "Show/Hide", None),
  ("X", "Construction", Some(Scope::Sketch)),
];

/// Parses "Ctrl+Shift+E" (Cmd counts as Ctrl). None for an empty or unknown key.
pub(crate) fn parse_combo(s: &str) -> Option<(Modifiers, Key)> {
  let mut m = Modifiers::NONE;
  let mut parts: Vec<&str> = s.split('+').map(str::trim).collect();
  let key = parts.pop()?;
  for p in parts {
    match p.to_ascii_lowercase().as_str() {
      "ctrl" | "cmd" | "command" => m.command = true,
      "shift" => m.shift = true,
      "alt" | "option" => m.alt = true,
      _ => return None,
    }
  }
  let key = match key {
    "Del" | "del" => Key::Delete,
    "Esc" | "esc" => Key::Escape,
    k => Key::from_name(k).or_else(|| Key::from_name(&k.to_ascii_uppercase()))?,
  };
  Some((m, key))
}

/// The text a combo is shown and stored as: "Ctrl+Shift+E", "Del".
pub(crate) fn format_combo(m: Modifiers, key: Key) -> String {
  let mut s = String::new();
  if m.command || m.ctrl || m.mac_cmd {
    s.push_str("Ctrl+");
  }
  if m.shift {
    s.push_str("Shift+");
  }
  if m.alt {
    s.push_str("Alt+");
  }
  s.push_str(if key == Key::Delete { "Del" } else { key.name() });
  s
}

/// `s` in the stored form, "" when it isn't a key.
fn normalize(s: &str) -> String {
  parse_combo(s).map(|(m, k)| format_combo(m, k)).unwrap_or_default()
}

/// What a new shortcut would clash with.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Conflict {
  /// Another command on the same toolbar has it: (its id, its label).
  Command(String, &'static str),
  /// The app uses it for this.
  Reserved(&'static str),
}

impl Conflict {
  pub(crate) fn message(&self) -> String {
    match self {
      Conflict::Command(_, label) => format!("Already used by {label}. OK moves it to this command."),
      Conflict::Reserved(what) => format!("Reserved for {what}; pick another key."),
    }
  }
}

/// The user's keyboard shortcuts: what they changed from the toolbar's defaults.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct KeyMap {
  /// Command id → combo ("" = no shortcut).
  changed: BTreeMap<String, String>,
}

impl KeyMap {
  /// From the `[Keys]` section's lines; keys that don't parse are dropped.
  pub(crate) fn from_entries(entries: &[(String, String)]) -> KeyMap {
    let mut changed = BTreeMap::new();
    for (id, v) in entries {
      let combo = normalize(v);
      if !id.is_empty() && (combo.is_empty() == v.trim().is_empty()) {
        changed.insert(id.clone(), combo);
      }
    }
    KeyMap { changed }
  }

  /// The `[Keys]` section's lines.
  pub(crate) fn entries(&self) -> Vec<(String, String)> {
    self.changed.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
  }

  /// The shortcut of command `id`, whose default is `default`.
  pub(crate) fn key<'a>(&'a self, id: &str, default: &'a str) -> &'a str {
    self.changed.get(id).map_or(default, String::as_str)
  }

  /// Gives command `id` the shortcut `combo` ("" for none).
  pub(crate) fn set(&mut self, id: &str, combo: &str, default: &str) {
    let combo = normalize(combo);
    if combo == normalize(default) {
      self.changed.remove(id);
    } else {
      self.changed.insert(id.to_string(), combo);
    }
  }

  /// What giving `id` (on toolbar `scope`) the shortcut `combo` would clash with, among
  /// `all` commands.
  pub(crate) fn conflict(&self, id: &str, scope: Scope, combo: &str, all: &[Binding]) -> Option<Conflict> {
    let (m, key) = parse_combo(combo)?;
    // the app's own keys: its Ctrl ones are looked at after the commands' with two or more
    // modifiers, its others before every command's and with Shift or Alt held too
    for (r, what, on) in RESERVED {
      if on.is_none_or(|s| s == scope)
        && let Some((rm, rk)) = parse_combo(r)
        && rk == key
        && if rm.command { m == rm } else { m.matches_logically(rm) }
      {
        return Some(Conflict::Reserved(what));
      }
    }
    let combo = format_combo(m, key);
    all
      .iter()
      .filter(|b| b.scope == scope && b.id != id)
      .find(|b| normalize(self.key(&b.id, b.default)) == combo)
      .map(|b| Conflict::Command(b.id.clone(), b.label))
  }
}

/// The toolbar and shortcut preferences.
#[derive(Clone, Debug, Default)]
pub(crate) struct Prefs {
  /// Icons pinned to the toolbar ([`pin_id`]s).
  pub pinned: Vec<String>,
  /// Default toolbar icons the user unpinned.
  pub unpinned: Vec<String>,
  /// Commands added to the S toolbox's shortcuts ([`cmd_id`]s).
  pub shortcuts: Vec<String>,
  pub keys: KeyMap,
  /// Where they are saved; None keeps them in memory (tests).
  file: Option<PathBuf>,
}

const TOOLBAR: &str = "Toolbar";
const KEYS: &str = "Keys";

fn list(s: &str) -> Vec<String> {
  s.split('|').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

impl Prefs {
  pub(crate) fn load() -> Prefs {
    let path = Settings::path();
    Prefs { file: Some(path.clone()), ..Prefs::from_settings(&Settings::load_from(&path)) }
  }

  pub(crate) fn from_settings(s: &Settings) -> Prefs {
    let mut p = Prefs::default();
    for (k, v) in s.section(TOOLBAR) {
      match k.to_ascii_lowercase().as_str() {
        "pinned" => p.pinned = list(&v),
        "unpinned" => p.unpinned = list(&v),
        "shortcuts" => p.shortcuts = list(&v),
        _ => {}
      }
    }
    p.keys = KeyMap::from_entries(&s.section(KEYS));
    p
  }

  /// Writes the toolbar and key sections into `s`, leaving the rest of it.
  pub(crate) fn write(&self, s: &mut Settings) {
    let toolbar = [("Pinned", &self.pinned), ("Unpinned", &self.unpinned), ("Shortcuts", &self.shortcuts)];
    s.set_section(TOOLBAR, &toolbar.map(|(k, v)| (k.to_string(), v.join("|"))));
    s.set_section(KEYS, &self.keys.entries());
  }

  /// Saves to `settings.ini` (re-read first, so other sections written since are kept).
  pub(crate) fn save(&self) -> Result<(), String> {
    let Some(path) = &self.file else { return Ok(()) };
    let mut s = Settings::load_from(path);
    self.write(&mut s);
    s.save_to(path).map_err(|e| format!("Couldn't save the toolbar settings: {e}"))
  }

  /// Whether a toolbar group shows `label`: a default icon the user didn't unpin, or one pinned.
  pub(crate) fn shows(&self, tab: Tab, group: &str, label: &str, default: bool) -> bool {
    let id = pin_id(tab, group, label);
    if default { !self.unpinned.contains(&id) } else { self.pinned.contains(&id) }
  }
}

/// A change picked from a command's options.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Action {
  /// Pin (true) or unpin a command's icon in a toolbar group: (tab, group, label, a default icon).
  Pin(Tab, &'static str, &'static str, bool, bool),
  /// Add (true) or remove a command from the S toolbox's shortcuts.
  Shortcut(String, bool),
  /// Open Change Keyboard Shortcut.
  ChangeKey(Binding),
}

/// A command's options, under its ⋮ button or right-click.
pub(crate) fn options_menu(
  ui: &mut egui::Ui,
  prefs: &Prefs,
  pin: (Tab, &'static str, &'static str, bool),
  binding: &Binding,
  act: &mut Option<Action>,
) {
  let (tab, group, label, default) = pin;
  let shown = prefs.shows(tab, group, label, default);
  if ui.button(if shown { "Unpin from Toolbar" } else { "Pin to Toolbar" }).clicked() {
    *act = Some(Action::Pin(tab, group, label, default, !shown));
    ui.close();
  }
  ui.add_enabled(false, egui::Button::new("Pin to Panel")).on_disabled_hover_text("Not in FissionOxide yet");
  let added = prefs.shortcuts.contains(&binding.id);
  if ui.button(if added { "Remove from Shortcuts" } else { "Add to Shortcuts" }).clicked() {
    *act = Some(Action::Shortcut(binding.id.clone(), !added));
    ui.close();
  }
  if ui.button("Change Keyboard Shortcut...").clicked() {
    *act = Some(Action::ChangeKey(binding.clone()));
    ui.close();
  }
}

/// Carries out an option picked from a command's menu.
pub(crate) fn apply(app: &mut App, act: Action) {
  let p = &mut app.prefs;
  match act {
    Action::Pin(tab, group, label, default, on) => {
      let id = pin_id(tab, group, label);
      let (add, remove) = if default { (&mut p.unpinned, &mut p.pinned) } else { (&mut p.pinned, &mut p.unpinned) };
      remove.retain(|x| *x != id);
      add.retain(|x| *x != id);
      // a default icon is shown unless listed as unpinned; any other only when listed as pinned
      if on != default {
        add.push(id);
      }
    }
    Action::Shortcut(id, on) => {
      p.shortcuts.retain(|x| *x != id);
      if on {
        p.shortcuts.insert(0, id);
      }
    }
    Action::ChangeKey(b) => {
      app.key_capture = Some(KeyCapture { combo: app.prefs.keys.key(&b.id, b.default).to_string(), binding: b });
      return;
    }
  }
  if let Err(e) = app.prefs.save() {
    app.status = Some((e, true));
  }
}

/// The open Change Keyboard Shortcut window.
#[derive(Clone, Debug)]
pub(crate) struct KeyCapture {
  binding: Binding,
  /// The key shown: the current one, then what was pressed.
  combo: String,
}

#[cfg(test)]
impl KeyCapture {
  pub(crate) fn for_test() -> KeyCapture {
    let binding = Binding { id: "design.Extrude".into(), scope: Scope::Design, label: "Extrude", default: "E" };
    KeyCapture { binding, combo: String::new() }
  }
}

/// The key pressed this frame, if any (modifier keys alone don't count).
fn captured(ctx: &egui::Context) -> Option<(Modifiers, Key)> {
  ctx.input(|i| {
    i.events.iter().find_map(|e| match e {
      egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } if *key != Key::Escape => {
        Some((*modifiers, *key))
      }
      _ => None,
    })
  })
}

/// Change Keyboard Shortcut: press a key, see what it clashes with, OK to keep it.
pub fn key_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut kc) = app.key_capture.take() else { return };
  if let Some((m, k)) = captured(ctx) {
    kc.combo = format_combo(m, k);
  }
  let all = crate::menus::bindings();
  let b = &kc.binding;
  let conflict = app.prefs.keys.conflict(&b.id, b.scope, &kc.combo, all);
  let mut done: Option<Option<String>> = None;
  egui::Window::new("Change Keyboard Shortcut")
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
    .show(ctx, |ui| {
      ui.set_width(300.0);
      ui.label(RichText::new(b.label).strong());
      ui.label(RichText::new("Press the new key, with Ctrl, Shift or Alt if you like.").color(theme::MUTED));
      ui.add_space(6.0);
      let shown = if kc.combo.is_empty() { "None".to_string() } else { kc.combo.clone() };
      ui.label(RichText::new(shown).size(18.0).strong());
      if let Some(c) = &conflict {
        ui.label(RichText::new(c.message()).color(theme::WARNING));
      }
      ui.add_space(6.0);
      ui.horizontal(|ui| {
        let ok = !matches!(conflict, Some(Conflict::Reserved(_)));
        if ui.add_enabled(ok, egui::Button::new("OK")).clicked() {
          done = Some(Some(kc.combo.clone()));
        }
        if ui.button("No Shortcut").clicked() {
          kc.combo.clear();
        }
        if !b.default.is_empty() && ui.button("Default").clicked() {
          kc.combo = normalize(b.default);
        }
        if ui.button("Cancel").clicked() {
          done = Some(None);
        }
      });
    });
  if ctx.input(|i| i.key_pressed(Key::Escape)) {
    done = Some(None);
  }
  match done {
    None => app.key_capture = Some(kc),
    Some(None) => {}
    Some(Some(combo)) => {
      let b = &kc.binding;
      if let Some(Conflict::Command(other, _)) = &conflict
        && let Some(o) = all.iter().find(|x| x.id == *other)
      {
        app.prefs.keys.set(&o.id, "", o.default);
      }
      app.prefs.keys.set(&b.id, &combo, b.default);
      app.status = Some((format!("{}: {}", b.label, if combo.is_empty() { "no shortcut" } else { &combo }), false));
      if let Err(e) = app.prefs.save() {
        app.status = Some((e, true));
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn b(id: &str, scope: Scope, label: &'static str, default: &'static str) -> Binding {
    Binding { id: id.to_string(), scope, label, default }
  }

  #[test]
  fn combos_parse_and_print() {
    assert_eq!(parse_combo("E"), Some((Modifiers::NONE, Key::E)));
    assert_eq!(parse_combo("Ctrl+Shift+e"), Some((Modifiers::COMMAND | Modifiers::SHIFT, Key::E)));
    assert_eq!(parse_combo("Del"), Some((Modifiers::NONE, Key::Delete)));
    assert_eq!(parse_combo("1"), Some((Modifiers::NONE, Key::Num1)));
    assert_eq!(parse_combo("Alt+click"), None);
    assert_eq!(parse_combo(""), None);
    assert_eq!(format_combo(Modifiers::COMMAND | Modifiers::ALT, Key::B), "Ctrl+Alt+B");
    assert_eq!(normalize("shift + del"), "Shift+Del");
  }

  #[test]
  fn key_map_loads_and_saves() {
    let mut s = Settings::parse("[Updates]\nMode=off\n[Keys]\ndesign.Extrude=shift+e\nsketch.Line=\nbad=Alt+click\n");
    let p = Prefs::from_settings(&s);
    assert_eq!(p.keys.key("design.Extrude", "E"), "Shift+E");
    assert_eq!(p.keys.key("sketch.Line", "L"), "");
    assert_eq!(p.keys.key("design.Fillet", "F"), "F");
    assert_eq!(p.keys.key("bad", ""), "");
    let mut p = p;
    p.keys.set("design.Fillet", "Ctrl+F", "F");
    // back to the default isn't stored
    p.keys.set("design.Extrude", "E", "E");
    p.pinned = vec!["SOLID/CREATE/Shell".into()];
    p.shortcuts = vec!["design.Fillet".into(), "sketch.Line".into()];
    p.write(&mut s);
    let text = s.to_text();
    assert!(text.contains("[Keys]\ndesign.Fillet=Ctrl+F\nsketch.Line=\n"), "{text}");
    assert!(text.contains("Mode=off"));
    let back = Prefs::from_settings(&Settings::parse(&text));
    assert_eq!(back.keys, p.keys);
    assert_eq!((back.pinned, back.shortcuts), (p.pinned.clone(), p.shortcuts.clone()));
    assert!(back.unpinned.is_empty());
    // through the file
    let dir = std::env::temp_dir().join(format!("fox-keys-{}", std::process::id()));
    let path = dir.join("settings.ini");
    let p = Prefs { file: Some(path.clone()), ..p };
    p.save().unwrap();
    assert_eq!(Prefs::from_settings(&Settings::load_from(&path)).keys.key("design.Fillet", "F"), "Ctrl+F");
    std::fs::remove_dir_all(dir).unwrap();
  }

  #[test]
  fn conflicts_are_found() {
    let all = [
      b("design.Extrude", Scope::Design, "Extrude", "E"),
      b("design.Fillet", Scope::Design, "Fillet", "F"),
      b("sketch.Line", Scope::Sketch, "Line", "L"),
      b("sketch.Trim", Scope::Sketch, "Trim", "T"),
    ];
    let mut k = KeyMap::default();
    assert_eq!(
      k.conflict("design.Extrude", Scope::Design, "F", &all),
      Some(Conflict::Command("design.Fillet".into(), "Fillet"))
    );
    // its own key, another toolbar's, and a free one don't clash
    assert_eq!(k.conflict("design.Extrude", Scope::Design, "E", &all), None);
    assert_eq!(k.conflict("design.Extrude", Scope::Design, "L", &all), None);
    assert_eq!(k.conflict("design.Extrude", Scope::Design, "Shift+F", &all), None);
    // the app's own keys, Shift included since those ignore it
    assert_eq!(k.conflict("design.Extrude", Scope::Design, "Ctrl+Z", &all), Some(Conflict::Reserved("Undo")));
    assert_eq!(k.conflict("design.Extrude", Scope::Design, "Ctrl+Shift+S", &all), Some(Conflict::Reserved("Save As")));
    assert_eq!(k.conflict("design.Extrude", Scope::Design, "Ctrl+Alt+S", &all), None);
    assert_eq!(k.conflict("design.Extrude", Scope::Design, "Shift+S", &all), Some(Conflict::Reserved("the S toolbox")));
    assert_eq!(k.conflict("sketch.Line", Scope::Sketch, "X", &all), Some(Conflict::Reserved("Construction")));
    assert_eq!(k.conflict("design.Extrude", Scope::Design, "X", &all), None);
    // changed keys count, not the defaults
    k.set("design.Fillet", "G", "F");
    assert_eq!(k.conflict("design.Extrude", Scope::Design, "F", &all), None);
    assert!(k.conflict("design.Extrude", Scope::Design, "G", &all).is_some());
    k.set("sketch.Trim", "", "T");
    assert_eq!(k.conflict("sketch.Line", Scope::Sketch, "T", &all), None);
  }

  #[test]
  fn pins_toggle() {
    let mut app = App::with_session(fox_engine::Session::default());
    assert!(app.prefs.shows(Tab::Solid, "CREATE", "Extrude", true));
    assert!(!app.prefs.shows(Tab::Solid, "CREATE", "Shell", false));
    apply(&mut app, Action::Pin(Tab::Solid, "CREATE", "Shell", false, true));
    apply(&mut app, Action::Pin(Tab::Solid, "CREATE", "Extrude", true, false));
    assert!(app.prefs.shows(Tab::Solid, "CREATE", "Shell", false));
    assert!(!app.prefs.shows(Tab::Solid, "CREATE", "Extrude", true));
    // only on that tab
    assert!(!app.prefs.shows(Tab::Plastic, "CREATE", "Shell", false));
    apply(&mut app, Action::Pin(Tab::Solid, "CREATE", "Extrude", true, true));
    assert!(app.prefs.shows(Tab::Solid, "CREATE", "Extrude", true));
    assert!(app.prefs.unpinned.is_empty());
    apply(&mut app, Action::Shortcut("design.Fillet".into(), true));
    apply(&mut app, Action::Shortcut("design.Shell".into(), true));
    assert_eq!(app.prefs.shortcuts, ["design.Shell", "design.Fillet"]);
    apply(&mut app, Action::Shortcut("design.Fillet".into(), false));
    assert_eq!(app.prefs.shortcuts, ["design.Shell"]);
  }
}
