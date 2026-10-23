//! Interference, as Fusion's dialog: Select (the bodies to check), Include Coincident Faces and
//! a Compute button, with OK greyed until computed. The result lists each pair that overlaps
//! (or only touches, with coincident faces included) and can add the overlaps as bodies.

use egui::RichText;
use fox_engine::model::BodyId;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::{Dialog, chip_text, sel_bodies, sel_chip};
use crate::icons::Icon;
use crate::{App, panels, theme};

impl Dialog {
  /// The rows: Select, Include Coincident Faces, Compute, then the result once computed.
  pub(super) fn interference_fields(&mut self, app: &mut App, ui: &mut egui::Ui) {
    // Select picks whole bodies while the dialog is open (put back when it closes)
    if self.saved_filter.is_none() {
      self.saved_filter = Some((app.sel_priority, app.sel_filter.vertices));
      app.sel_priority = crate::select::Priority::Body;
    }
    ui.label("Select");
    sel_chip(ui, self.accent(), &chip_text(self.bodies.len(), "Bodies"), !self.bodies.is_empty());
    ui.end_row();
    ui.label("Include Coincident Faces");
    if ui.checkbox(&mut self.flag2, "").changed() {
      self.report = None;
    }
    ui.end_row();
    ui.label("Compute");
    let can = self.bodies.len() >= 2;
    let hint = if can { "Compute" } else { "Select two or more bodies" };
    if ui
      .add_enabled_ui(can, |ui| crate::icons::icon_button(ui, Icon::Interference, 26.0, hint, false, None))
      .inner
      .clicked()
    {
      self.compute_interference(&mut app.s);
    }
    ui.end_row();
    let Some(rows) = self.report.as_ref().and_then(Value::as_array).cloned() else { return };
    ui.label("Result");
    ui.label(match rows.len() {
      0 => "No interference".to_string(),
      1 => "1 interference".to_string(),
      n => format!("{n} interferences"),
    });
    ui.end_row();
    for r in &rows {
      let ids: Vec<u32> = r
        .get("bodies")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_u64().map(|v| v as u32)).collect())
        .unwrap_or_default();
      let names: Vec<String> = ids.iter().map(|b| app.s.body_name(BodyId(*b))).collect();
      if ui.link(names.join(" ↔ ")).clicked() {
        app.s.selection = ids.iter().map(|b| Sel::Body { body: BodyId(*b) }).collect();
      }
      if r.get("coincident").and_then(Value::as_bool) == Some(true) {
        ui.label(RichText::new("Coincident faces").color(theme::MUTED));
      } else {
        ui.label(app.measure_fmt.volume(r.get("volume_mm3").and_then(Value::as_f64).unwrap_or(0.0)));
      }
      ui.end_row();
    }
    let overlap = rows.iter().any(|r| r.get("coincident").is_none());
    if overlap {
      // adds each overlap as a static body (in a new component when ticked)
      ui.label("Create components");
      ui.checkbox(&mut self.flag, "");
      ui.end_row();
      ui.label("");
      if panels::secondary_button(ui, "Create Bodies").clicked() {
        self.create_interference(&mut app.s);
      }
      ui.end_row();
    }
  }

  /// Picks up the bodies selected for Select; a change drops the last result.
  pub(super) fn interference_sync(&mut self, s: &Session) {
    let b = sel_bodies(s);
    if b != self.bodies {
      self.bodies = b;
      self.report = None;
    }
  }

  /// Compute: the interferences between the picked bodies.
  pub(super) fn compute_interference(&mut self, s: &mut Session) {
    match s.execute("assembly.interference", &json!({"bodies": self.bodies, "coincident": self.flag2})) {
      Ok(v) => {
        self.report = Some(v);
        self.error = None;
      }
      Err(e) => {
        self.report = None;
        self.error = Some(e.0);
      }
    }
  }
}
