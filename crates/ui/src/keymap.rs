//! Every keyboard shortcut, in one table. `App::shortcuts` runs the keys from it, the marking
//! menu shows its keys from it, and Help > Keyboard Shortcuts lists it, so the three can't
//! disagree.

use std::sync::OnceLock;

use egui::{Key, Modifiers};

use crate::dialogs::Kind;
use crate::panels::Cmd;
use crate::select::Mode;
use crate::viewport::Tool;

/// Where a key works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
  /// Anywhere, the Drawing workspace too.
  Everywhere,
  /// The Manufacture workspace.
  Manufacture,
  /// The Design workspace (not on a drawing sheet).
  Design,
  /// While a sketch is open.
  Sketch,
  /// The Design workspace outside a sketch.
  Model,
}

impl Scope {
  pub const ALL: [Scope; 5] = [Scope::Everywhere, Scope::Design, Scope::Sketch, Scope::Model, Scope::Manufacture];

  pub fn title(self) -> &'static str {
    match self {
      Scope::Everywhere => "General",
      Scope::Manufacture => "Manufacture",
      Scope::Design => "Design",
      Scope::Sketch => "Sketch",
      Scope::Model => "Design, outside a sketch",
    }
  }
}

/// What a key does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Act {
  /// A command, started as from the toolbar (see `App::launch`).
  Cmd(Cmd),
  Save,
  SaveAs,
  Open,
  NewDesign,
  /// Close a menu, cancel the tool or command, clear the selection.
  Escape,
  /// The S toolbox, at the pointer.
  Toolbox,
  /// Finish the spline being drawn.
  FinishSpline,
}

#[derive(Debug, Clone, Copy)]
pub struct Binding {
  pub key: Key,
  pub mods: Modifiers,
  pub scope: Scope,
  pub act: Act,
  /// What the key does, as the shortcuts window lists it.
  pub label: &'static str,
}

const NONE: Modifiers = Modifiers::NONE;
const CTRL: Modifiers = Modifiers::COMMAND;
const CTRL_SHIFT: Modifiers = Modifiers { shift: true, ..Modifiers::COMMAND };

const fn b(key: Key, mods: Modifiers, scope: Scope, act: Act, label: &'static str) -> Binding {
  Binding { key, mods, scope, act, label }
}

const fn dialog(key: Key, kind: Kind, label: &'static str) -> Binding {
  b(key, NONE, Scope::Design, Act::Cmd(Cmd::Dialog(kind)), label)
}

const fn tool(key: Key, t: Tool, label: &'static str) -> Binding {
  b(key, NONE, Scope::Sketch, Act::Cmd(Cmd::SketchTool(t)), label)
}

const APPEARANCE: Cmd = Cmd::Dialog(Kind::Form(&crate::forms::APPEARANCE));

/// Every key the app binds, in the order they are tried: a key with more modifiers comes before
/// the same key with fewer (egui lets Ctrl+Z match a press of Ctrl+Shift+Z).
pub const BINDINGS: &[Binding] = &[
  b(Key::Y, CTRL, Scope::Everywhere, Act::Cmd(Cmd::Redo), "Redo"),
  b(Key::Z, CTRL_SHIFT, Scope::Everywhere, Act::Cmd(Cmd::Redo), "Redo"),
  b(Key::Z, CTRL, Scope::Everywhere, Act::Cmd(Cmd::Undo), "Undo"),
  b(Key::S, CTRL_SHIFT, Scope::Everywhere, Act::SaveAs, "Save As"),
  b(Key::S, CTRL, Scope::Everywhere, Act::Save, "Save"),
  b(Key::O, CTRL, Scope::Everywhere, Act::Open, "Open"),
  b(Key::N, CTRL, Scope::Everywhere, Act::NewDesign, "New Design"),
  b(Key::G, CTRL, Scope::Manufacture, Act::Cmd(Cmd::CamGenerate), "Generate toolpaths"),
  b(Key::B, CTRL, Scope::Design, Act::Cmd(Cmd::ComputeAll), "Compute All"),
  b(Key::A, CTRL, Scope::Design, Act::Cmd(Cmd::SelectAll), "Select All"),
  b(Key::Escape, NONE, Scope::Everywhere, Act::Escape, "Close the menu, cancel the command, clear the selection"),
  b(Key::S, NONE, Scope::Everywhere, Act::Toolbox, "Toolbox (shortcuts and search, at the pointer)"),
  b(Key::Enter, NONE, Scope::Sketch, Act::FinishSpline, "Finish the spline"),
  b(Key::Delete, NONE, Scope::Design, Act::Cmd(Cmd::Delete), "Delete"),
  b(Key::Backspace, NONE, Scope::Design, Act::Cmd(Cmd::Delete), "Delete"),
  b(Key::F6, NONE, Scope::Design, Act::Cmd(Cmd::Fit), "Fit"),
  b(Key::Home, NONE, Scope::Design, Act::Cmd(Cmd::Home), "Home view"),
  b(Key::V, NONE, Scope::Design, Act::Cmd(Cmd::ToggleVisibility), "Show/Hide"),
  // modelling commands; in a sketch they finish it first (E extrudes its profile)
  dialog(Key::E, Kind::Extrude, "Extrude"),
  dialog(Key::F, Kind::Fillet, "Fillet"),
  dialog(Key::M, Kind::Move, "Move/Copy"),
  dialog(Key::I, Kind::Inspect, "Measure"),
  dialog(Key::H, Kind::Hole, "Hole"),
  dialog(Key::Q, Kind::PressPull, "Press Pull"),
  dialog(Key::J, Kind::Joint, "Joint"),
  tool(Key::L, Tool::Line, "Line"),
  tool(Key::R, Tool::Rect, "2-Point Rectangle"),
  tool(Key::C, Tool::Circle, "Center Diameter Circle"),
  tool(Key::A, Tool::Arc3, "3-Point Arc"),
  tool(Key::P, Tool::Point, "Point"),
  tool(Key::D, Tool::Dimension, "Sketch Dimension"),
  tool(Key::T, Tool::Trim, "Trim"),
  b(Key::X, NONE, Scope::Sketch, Act::Cmd(Cmd::Construction), "Normal/Construction"),
  b(Key::O, NONE, Scope::Sketch, Act::Cmd(Cmd::Dialog(Kind::SkOffset)), "Offset"),
  b(Key::A, NONE, Scope::Model, Act::Cmd(APPEARANCE), "Appearance"),
  b(Key::Num1, NONE, Scope::Model, Act::Cmd(Cmd::SelectMode(Mode::Window)), "Window Selection"),
  b(Key::Num2, NONE, Scope::Model, Act::Cmd(Cmd::SelectMode(Mode::Freeform)), "Freeform Selection"),
  b(Key::Num3, NONE, Scope::Model, Act::Cmd(Cmd::SelectMode(Mode::Paint)), "Paint Selection"),
];

/// The key as the menus write it: "Ctrl+Shift+Z", "Esc", "F6".
pub fn key_text(b: &Binding) -> String {
  let mut s = String::new();
  if b.mods.command || b.mods.ctrl || b.mods.mac_cmd {
    s.push_str("Ctrl+");
  }
  if b.mods.alt {
    s.push_str("Alt+");
  }
  if b.mods.shift {
    s.push_str("Shift+");
  }
  s.push_str(match b.key {
    Key::Escape => "Esc",
    Key::Delete => "Del",
    k => k.name(),
  });
  s
}

/// [`key_text`] of every binding, made once (menus want `&'static str`).
fn texts() -> &'static [String] {
  static T: OnceLock<Vec<String>> = OnceLock::new();
  T.get_or_init(|| BINDINGS.iter().map(key_text).collect())
}

/// The first key that runs `c`, as the marking menu shows it.
pub fn key_for(c: Cmd) -> Option<&'static str> {
  BINDINGS.iter().position(|b| b.act == Act::Cmd(c)).map(|i| texts()[i].as_str())
}

/// The bindings of a scope, for the shortcuts window: (key, what it does).
pub fn rows(scope: Scope) -> Vec<(&'static str, &'static str)> {
  BINDINGS.iter().zip(texts()).filter(|(b, _)| b.scope == scope).map(|(b, t)| (t.as_str(), b.label)).collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn keys_read_as_fusion_writes_them() {
    assert_eq!(key_for(Cmd::Undo), Some("Ctrl+Z"));
    assert_eq!(key_for(Cmd::Redo), Some("Ctrl+Y"));
    assert_eq!(key_for(Cmd::Delete), Some("Del"));
    assert_eq!(key_for(Cmd::Dialog(Kind::Extrude)), Some("E"));
    assert_eq!(key_for(Cmd::SelectMode(Mode::Paint)), Some("3"));
    assert_eq!(key_for(Cmd::LookAt), None);
  }

  #[test]
  fn no_key_is_bound_twice_where_both_could_fire() {
    // the same key and modifiers may only repeat in scopes that never apply together
    let exclusive =
      |a: Scope, b: Scope| matches!((a, b), (Scope::Sketch, Scope::Model) | (Scope::Model, Scope::Sketch));
    for (i, a) in BINDINGS.iter().enumerate() {
      for c in &BINDINGS[i + 1..] {
        if a.key == c.key && a.mods == c.mods {
          assert!(exclusive(a.scope, c.scope), "{} bound twice", key_text(a));
        }
      }
    }
  }
}
