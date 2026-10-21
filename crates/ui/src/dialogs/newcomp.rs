//! The Operation drop-down's New Component choice: a Component name row under it, and the
//! `component_name` it adds to the command.

use serde_json::{Value, json};

use super::Dialog;

impl Dialog {
  /// The Component name row, shown while creating a feature whose operation is New Component.
  pub(super) fn comp_name_row(&mut self, ui: &mut egui::Ui) {
    let mut dirty = false;
    comp_name_field(ui, self.editing.is_none(), self.op, &mut self.comp_name, &mut dirty);
    self.dirty |= dirty;
  }

  /// `command` plus the component name for a New Component feature.
  pub(super) fn with_comp_name(&self, cmd: Option<(&'static str, Value)>) -> Option<(&'static str, Value)> {
    let (name, mut p) = cmd?;
    let given = self.comp_name.trim();
    if self.editing.is_none() && self.op == "new_component" && !given.is_empty() && p.is_object() {
      p["component_name"] = json!(given);
    }
    Some((name, p))
  }
}

/// The row itself, for dialogs that keep their dirty flag apart.
pub(super) fn comp_name_field(ui: &mut egui::Ui, creating: bool, op: &str, name: &mut String, dirty: &mut bool) {
  if !creating || op != "new_component" {
    return;
  }
  ui.label("Component name");
  *dirty |= ui.add(egui::TextEdit::singleline(name).hint_text("Next ComponentN").desired_width(140.0)).changed();
  ui.end_row();
}
