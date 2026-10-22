//! Fusion's REMOVE FEATURES (Simplify): a Bodies chip, then a Features group of the types to
//! look for, each ticked, under Select All. With bodies picked, the features found on them are
//! listed and Remove takes them off (a Remove Faces feature); the dialog stays open, with only
//! a Close button.

use egui::RichText;
use fox_engine::model::BodyId;
use serde_json::{Value, json};

use super::{Dialog, sel_chip};
use crate::forms::{REMOVE_FEATURE_TYPES, REMOVE_FEATURES, greyed};
use crate::{App, theme};

impl Dialog {
  fn rf_value(&self, key: &str) -> Option<&Value> {
    REMOVE_FEATURES.inputs.iter().position(|x| x.key() == key).and_then(|i| self.form.values.get(i))
  }

  fn rf_set(&mut self, key: &str, v: Value) {
    if let Some(i) = REMOVE_FEATURES.inputs.iter().position(|x| x.key() == key) {
      self.form.values[i] = v;
    }
  }

  /// The ticked types, in the group's order.
  pub(crate) fn rf_types(&self) -> Vec<&'static str> {
    REMOVE_FEATURE_TYPES
      .iter()
      .copied()
      .filter(|t| self.rf_value(t).and_then(Value::as_bool).unwrap_or(false))
      .collect()
  }

  fn rf_bodies(&self) -> Vec<u32> {
    self
      .rf_value("bodies")
      .and_then(Value::as_array)
      .map(|a| a.iter().filter_map(Value::as_u64).map(|b| b as u32).collect())
      .unwrap_or_default()
  }

  /// Ticks or clears a type (or, for "all", every type); Select All stays ticked only while
  /// every type is.
  pub(crate) fn rf_tick(&mut self, key: &str, on: bool) {
    if key == "all" {
      for t in REMOVE_FEATURE_TYPES {
        self.rf_set(t, json!(on));
      }
    } else {
      self.rf_set(key, json!(on));
    }
    let all = REMOVE_FEATURE_TYPES.iter().all(|t| self.rf_value(t).and_then(Value::as_bool).unwrap_or(false));
    self.rf_set("all", json!(all));
  }

  /// The features found on the picked bodies for the ticked types (looked up again when either
  /// changes, or after a removal).
  pub(crate) fn rf_found(&mut self, s: &mut fox_engine::Session) -> Vec<Value> {
    let bodies = self.rf_bodies();
    if bodies.is_empty() {
      return Vec::new();
    }
    let key = json!([bodies, self.rf_types(), s.design.features.len(), s.model.bodies.len()]);
    if self.report.as_ref().and_then(|r| r.get("for")) != Some(&key) {
      let found = s
        .execute("remove_features", &json!({"bodies": bodies, "types": self.rf_types(), "dry_run": true}))
        .ok()
        .and_then(|r| r["features"].as_array().cloned())
        .unwrap_or_default();
      self.report = Some(json!({"for": key, "features": found}));
    }
    self.report.as_ref().and_then(|r| r["features"].as_array().cloned()).unwrap_or_default()
  }

  /// Removes the found features; the bodies stay picked for another pass.
  pub(crate) fn rf_remove(&mut self, s: &mut fox_engine::Session) {
    let bodies = self.rf_bodies();
    match s.execute("remove_features", &json!({"bodies": bodies, "types": self.rf_types()})) {
      Ok(r) => match r.get("error").and_then(Value::as_str) {
        Some(e) => self.error = Some(e.to_string()),
        None => self.error = None,
      },
      Err(e) => self.error = Some(e.0),
    }
    self.report = None;
  }

  pub(super) fn remove_features_fields(&mut self, app: &mut App, ui: &mut egui::Ui) {
    let accent = self.accent();
    let bodies = self.rf_bodies();
    ui.label("Bodies");
    let text = match bodies.len() {
      0 => "no selection".to_string(),
      1 => app.s.body_name(BodyId(bodies[0])),
      n => format!("{n} bodies"),
    };
    sel_chip(ui, accent, &text, !bodies.is_empty());
    ui.end_row();
    // the Features group: its heading across the dialog, then the types
    ui.label(RichText::new("▾ Features").strong());
    ui.label("");
    ui.end_row();
    let found = self.rf_found(&mut app.s);
    for key in std::iter::once("all").chain(REMOVE_FEATURE_TYPES) {
      let i = REMOVE_FEATURES.inputs.iter().position(|x| x.key() == key).unwrap_or(0);
      let label = match &REMOVE_FEATURES.inputs[i] {
        crate::forms::Input::CheckOn { label, .. } => *label,
        _ => key,
      };
      let mut on = self.form.values[i].as_bool().unwrap_or(true);
      let n = found.iter().filter(|f| f["type"] == key).count();
      let text = if bodies.is_empty() || key == "all" { label.to_string() } else { format!("{label} ({n})") };
      ui.label("");
      let changed = match greyed(&REMOVE_FEATURES, key) {
        Some(why) => {
          ui.add_enabled_ui(false, |ui| ui.checkbox(&mut on, text)).inner.on_disabled_hover_text(why);
          false
        }
        None => ui.checkbox(&mut on, text).changed(),
      };
      ui.end_row();
      if changed {
        self.rf_tick(key, on);
      }
    }
    if !bodies.is_empty() {
      ui.label("");
      let remove = ui.add_enabled(!found.is_empty(), egui::Button::new("Remove"));
      ui.end_row();
      if found.is_empty() {
        ui.label("");
        ui.label(RichText::new("No features of the ticked types found").small().color(theme::MUTED));
        ui.end_row();
      }
      if remove.clicked() {
        self.rf_remove(&mut app.s);
      }
    }
  }
}
