//! The sheet metal flat pattern dialogs, as Fusion's:
//!
//! - CREATE FLAT PATTERN: one Stationary Face row (lit), OK greyed until a planar face of the
//!   sheet metal body is picked; OK makes the design's flat pattern and enters its environment.
//! - UNFOLD: Reference (lit), Bends (greyed: FissionOxide's kernel unfolds every bend at once)
//!   and "Unfold all bends"; OK greyed until the reference is picked and the box ticked, when
//!   the bends preview green. OK adds the Unfold, and the ribbon's REFOLD panel lights.
//! - EXPORT FLAT PATTERN: the name, File Type (DXF, STEP), Units, Splines to Polylines and the
//!   Visible Geometry group (Center Lines, Extent Lines); OK asks where to save.

use fox_engine::exchange::dxf::DXF_UNITS;
use fox_engine::model::BodyId;
use fox_engine::{Sel, Session};
use serde_json::{Value, json};

use super::{Dialog, Kind, is_planar, sel_chip_lit};
use crate::{App, theme};

/// The cursor prompts (Fusion's wording).
pub(crate) const FLAT_HINT: &str = "Select Stationary face to create Flat Pattern";
pub(crate) const UNFOLD_HINT: &str = "Select stationary entity";
pub(crate) const UNFOLD_BENDS_HINT: &str = "Select bends to unfold";

/// Unfold's bends previewed green, as Fusion's.
pub(crate) const BEND_PREVIEW: [u8; 4] = [60, 180, 75, 255];

/// The choices of the flat pattern dialogs.
#[derive(Debug, Clone)]
pub(super) struct State {
  /// Unfold's "Unfold all bends".
  pub all_bends: bool,
  /// Export Flat Pattern: "dxf" or "step".
  pub file_type: &'static str,
  /// A [`DXF_UNITS`] key.
  pub units: &'static str,
  pub polylines: bool,
  pub center_lines: bool,
  pub extent_lines: bool,
  /// The Visible Geometry group is open.
  pub open: bool,
}

impl Default for State {
  fn default() -> State {
    State {
      all_bends: false,
      file_type: "dxf",
      units: "mm",
      polylines: false,
      center_lines: true,
      extent_lines: true,
      open: true,
    }
  }
}

impl Dialog {
  /// Create Flat Pattern's and Unfold's pick: the first planar face selected (one only).
  pub(super) fn flat_sync(&mut self, s: &Session) {
    let f: Vec<Value> = s
      .selection
      .iter()
      .find_map(|x| match *x {
        Sel::Face { body, face } if is_planar(s, body, face) => Some(json!({"body": body.0, "face": face})),
        _ => None,
      })
      .into_iter()
      .collect();
    if f != self.faces {
      self.bodies = f.iter().filter_map(|v| v["body"].as_u64()).map(|b| b as u32).collect();
      self.faces = f;
    }
  }

  /// Export Flat Pattern opens in the design's unit.
  pub(crate) fn export_flat_begin(&mut self, s: &Session) {
    let unit = s.design.units.symbol();
    self.flat.units = DXF_UNITS.iter().find(|u| u.0 == unit).map_or("mm", |u| u.0);
  }

  /// The `sheet.flat_pattern` call that writes the file at `path`.
  pub(crate) fn export_flat_params(&self, path: &std::path::Path) -> Value {
    let o = &self.flat;
    let mut p = json!({"path": path});
    if o.file_type == "dxf" {
      p["units"] = json!(o.units);
      p["polylines"] = json!(o.polylines);
      p["center_lines"] = json!(o.center_lines);
      p["extent_lines"] = json!(o.extent_lines);
    }
    p
  }

  /// Export Flat Pattern's OK: where to save, then the file. Returns whether the dialog closes.
  pub(super) fn export_flat_ok(&self, app: &mut App) -> bool {
    let (filter, ext) = if self.flat.file_type == "step" { ("STEP", "step") } else { ("DXF", "dxf") };
    let name = crate::panels::doc_name(app);
    let Some(path) =
      rfd::FileDialog::new().add_filter(filter, &[ext]).set_file_name(format!("{name}.{ext}")).save_file()
    else {
      return false;
    };
    if app.run("sheet.flat_pattern", self.export_flat_params(&path)).is_some() {
      app.status = Some((format!("Exported {}", path.display()), false));
    }
    true
  }

  /// Unfold's bends previewed green: the bend (cylindrical) faces of the picked body once the
  /// reference is in and Unfold all bends is ticked.
  pub(crate) fn tint_faces(&self, s: &Session) -> Vec<(BodyId, u32, [u8; 4])> {
    if self.kind != Kind::Unfold || self.applied || !self.flat.all_bends {
      return Vec::new();
    }
    let Some(b) = self.bodies.first().map(|b| BodyId(*b)) else { return Vec::new() };
    let Some(body) = s.model.body(b) else { return Vec::new() };
    body
      .faces()
      .iter()
      .filter(|f| f.kind == fox_engine::kernel::SurfaceKind::Cylinder)
      .map(|f| (b, f.id, BEND_PREVIEW))
      .collect()
  }

  /// A Select chip that takes one face: lit while `lit`, "1 selected" with an x after a pick.
  fn face_row(&mut self, app: &mut App, ui: &mut egui::Ui, label: &str, lit: bool) {
    ui.label(label);
    let has = !self.faces.is_empty();
    let chip = sel_chip_lit(ui, self.accent(), if has { "1 selected" } else { "Select" }, lit, has);
    if has {
      let x = egui::Rect::from_min_max(egui::pos2(chip.rect.right() - 20.0, chip.rect.top()), chip.rect.max);
      if ui.interact(x, ui.id().with(("flat-clear", label)), egui::Sense::click()).clicked() {
        self.faces.clear();
        self.bodies.clear();
        app.s.selection.clear();
      }
    }
    ui.end_row();
  }

  /// CREATE FLAT PATTERN's one row.
  pub(super) fn create_flat_rows(&mut self, app: &mut App, ui: &mut egui::Ui) {
    self.face_row(app, ui, "Stationary Face", true);
  }

  /// UNFOLD's rows: Reference, Bends (greyed) and Unfold all bends.
  pub(super) fn unfold_rows(&mut self, app: &mut App, ui: &mut egui::Ui) {
    self.face_row(app, ui, "Reference", true);
    ui.label("Bends");
    ui.add_enabled_ui(false, |ui| sel_chip_lit(ui, self.accent(), "Select", false, false))
      .response
      .on_disabled_hover_text("FissionOxide unfolds every bend at once: tick Unfold all bends");
    ui.end_row();
    ui.label("Unfold all bends");
    ui.checkbox(&mut self.flat.all_bends, "");
    ui.end_row();
  }

  /// EXPORT FLAT PATTERN's rows.
  pub(super) fn export_flat_rows(&mut self, app: &mut App, ui: &mut egui::Ui) {
    ui.label("Flat Pattern Name");
    let name = crate::panels::root_label(app);
    ui.add(egui::TextEdit::singleline(&mut name.as_str()).desired_width(140.0));
    ui.end_row();
    let o = &mut self.flat;
    ui.label("File Type");
    egui::ComboBox::from_id_salt("flat-file-type")
      .selected_text(if o.file_type == "step" { "STEP" } else { "DXF" })
      .width(140.0)
      .show_ui(ui, |ui| {
        for (key, label) in [("dxf", "DXF"), ("step", "STEP")] {
          if ui.selectable_label(o.file_type == key, label).clicked() {
            o.file_type = key;
          }
        }
      });
    ui.end_row();
    let dxf = o.file_type == "dxf";
    ui.label("Units");
    ui.add_enabled_ui(dxf, |ui| {
      let cur = DXF_UNITS.iter().find(|u| u.0 == o.units).map_or("Millimeter", |u| u.1);
      egui::ComboBox::from_id_salt("flat-units").selected_text(cur).width(140.0).show_ui(ui, |ui| {
        for (key, label, _, _) in DXF_UNITS {
          if ui.selectable_label(o.units == key, label).clicked() {
            o.units = key;
          }
        }
      });
    });
    ui.end_row();
    ui.label("Splines to Polylines");
    ui.add_enabled(dxf, egui::Checkbox::without_text(&mut o.polylines));
    ui.end_row();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(140.0, 20.0), egui::Sense::click());
    let c = egui::pos2(r.left() + 6.0, r.center().y);
    let tri = if o.open {
      vec![c + egui::vec2(-4.0, -2.0), c + egui::vec2(4.0, -2.0), c + egui::vec2(0.0, 3.0)]
    } else {
      vec![c + egui::vec2(-2.0, -4.0), c + egui::vec2(3.0, 0.0), c + egui::vec2(-2.0, 4.0)]
    };
    ui.painter().add(egui::Shape::convex_polygon(tri, theme::GROUP_LABEL, egui::Stroke::NONE));
    ui.painter().text(
      egui::pos2(r.left() + 16.0, r.center().y),
      egui::Align2::LEFT_CENTER,
      "Visible Geometry",
      egui::FontId::proportional(12.0),
      theme::TEXT,
    );
    if resp.clicked() {
      o.open = !o.open;
    }
    ui.end_row();
    if o.open {
      for (label, v) in [("Center Lines", &mut o.center_lines), ("Extent Lines", &mut o.extent_lines)] {
        ui.label(label);
        ui.add_enabled(dxf, egui::Checkbox::without_text(v));
        ui.end_row();
      }
    }
  }
}
