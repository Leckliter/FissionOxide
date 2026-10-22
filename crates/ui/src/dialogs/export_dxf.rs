//! Export DXF, from a sketch row's menu, as Fusion's: a docked EXPORT DXF dialog (the sketch,
//! the unit and the Visible Geometry check boxes), whose OK opens Fusion's own Export DXF window
//! (the name, the type and where to save) rather than the system's file dialog.

use std::path::{Path, PathBuf};

use fox_engine::model::FeatureId;
use fox_engine::{Session, exchange::dxf::DXF_UNITS};
use serde_json::{Value, json};

use super::Dialog;
use crate::{App, file_dialogs, theme};

/// The EXPORT DXF dialog's choices.
#[derive(Debug, Clone)]
pub(super) struct Options {
  /// A [`DXF_UNITS`] key.
  pub units: &'static str,
  pub points: bool,
  pub projected: bool,
  pub construction: bool,
  /// The Visible Geometry group is open.
  pub open: bool,
}

impl Default for Options {
  fn default() -> Options {
    Options { units: "mm", points: true, projected: true, construction: true, open: true }
  }
}

/// Fusion's Export DXF window, opened by the dialog's OK.
#[derive(Debug, Clone)]
pub(crate) struct SaveWindow {
  pub sketch: FeatureId,
  /// The file name (without .dxf), at first the sketch's name, selected so typing replaces it.
  pub name: String,
  /// The folder "Save to my computer" writes to (Downloads at first, as in Fusion).
  pub location: String,
  /// "Save to my computer" is ticked.
  pub local: bool,
  pub units: &'static str,
  pub points: bool,
  pub projected: bool,
  pub construction: bool,
  /// The name is selected the first time the window shows.
  first: bool,
}

impl SaveWindow {
  /// The file written and the `export` command's parameters.
  pub(crate) fn export(&self) -> (PathBuf, Value) {
    let name = self.name.trim();
    let file = if name.to_ascii_lowercase().ends_with(".dxf") { name.to_string() } else { format!("{name}.dxf") };
    let path = Path::new(&self.location).join(file);
    let p = json!({
      "path": path, "sketch": self.sketch.0, "units": self.units,
      "points": self.points, "projected": self.projected, "construction": self.construction,
    });
    (path, p)
  }

  /// Save: writes the file; returns whether it was written (the window then closes).
  pub(crate) fn save(&self, app: &mut App) -> bool {
    if !Path::new(&self.location).is_dir() {
      app.status = Some((format!("The folder {} doesn't exist", self.location), true));
      return false;
    }
    let (path, p) = self.export();
    if app.run("export", p).is_none() {
      return false;
    }
    app.status = Some((format!("Exported {}", path.display().to_string().replace('\\', "/")), false));
    true
  }
}

impl Dialog {
  /// Points the dialog at `sketch`, in the design's unit.
  pub(crate) fn dxf_begin(&mut self, s: &Session, sketch: FeatureId) {
    self.sketch = Some(sketch);
    let unit = s.design.units.symbol();
    self.dxf.units = DXF_UNITS.iter().find(|u| u.0 == unit).map_or("mm", |u| u.0);
  }

  /// OK: on to the Export DXF window.
  pub(super) fn dxf_ok(&self, app: &mut App) {
    let Some(sketch) = self.sketch else { return };
    let name = app.s.design.feature(sketch).map(|f| f.name.clone()).unwrap_or_default();
    let o = &self.dxf;
    app.export_dxf = Some(SaveWindow {
      sketch,
      name,
      location: file_dialogs::downloads(),
      local: true,
      units: o.units,
      points: o.points,
      projected: o.projected,
      construction: o.construction,
      first: true,
    });
  }

  /// Sketch (read-only), Units, and the Visible Geometry group's check boxes, whose labels the
  /// label column clips as Fusion's does ("Projected Geome...").
  pub(super) fn dxf_rows(&mut self, app: &mut App, ui: &mut egui::Ui) {
    ui.label("Sketch");
    let name = self.sketch.and_then(|f| app.s.design.feature(f)).map(|f| f.name.clone()).unwrap_or_default();
    ui.add(egui::TextEdit::singleline(&mut name.as_str()).desired_width(140.0));
    ui.end_row();
    ui.label("Units");
    let cur = DXF_UNITS.iter().find(|u| u.0 == self.dxf.units).map_or("Millimeter", |u| u.1);
    egui::ComboBox::from_id_salt("dxf-units").selected_text(cur).width(140.0).show_ui(ui, |ui| {
      for (key, label, _, _) in DXF_UNITS {
        if ui.selectable_label(self.dxf.units == key, label).clicked() {
          self.dxf.units = key;
        }
      }
    });
    ui.end_row();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(140.0, 20.0), egui::Sense::click());
    let open = self.dxf.open;
    let c = egui::pos2(r.left() + 6.0, r.center().y);
    let tri = if open {
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
      self.dxf.open = !open;
    }
    ui.end_row();
    if !self.dxf.open {
      return;
    }
    let o = &mut self.dxf;
    for (label, v) in [
      ("Points", &mut o.points),
      ("Projected Geometries", &mut o.projected),
      ("Construction Geometries", &mut o.construction),
    ] {
      ui.allocate_ui(egui::vec2(96.0, 18.0), |ui| {
        ui.set_max_width(96.0);
        ui.add(egui::Label::new(label).truncate()).on_hover_text(label);
      });
      ui.checkbox(v, "");
      ui.end_row();
    }
  }
}

/// Fusion's Export DXF window: Name, Type, the cloud project (greyed: FissionOxide saves no
/// sketches to the hub), Save to my computer with its folder and "...", Cancel and a blue Save.
/// Like Fusion's other file windows it is modal and closes only with Cancel or its X.
pub(crate) fn save_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut w) = app.export_dxf.take() else { return };
  let mut save = false;
  let mut cancel = false;
  let (_, closed) = file_dialogs::modal(ctx, "export-dxf", "Export DXF", 480.0, |ui| {
    ui.label("Name:");
    let r = file_dialogs::name_field(ui, &mut w.name, &mut w.first);
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
      save = true;
    }
    ui.add_space(4.0);
    ui.label("Type:");
    egui::ComboBox::from_id_salt("export-dxf-type")
      .selected_text("DXF Files (*.dxf)")
      .width(ui.available_width())
      .show_ui(ui, |ui| {
        let _ = ui.selectable_label(true, "DXF Files (*.dxf)");
      });
    ui.add_space(8.0);
    ui.add_enabled_ui(false, |ui| {
      let mut off = false;
      ui.checkbox(&mut off, "Save to a project in the cloud");
      egui::ComboBox::from_id_salt("export-dxf-project")
        .selected_text("")
        .width(ui.available_width())
        .show_ui(ui, |_| {});
    })
    .response
    .on_disabled_hover_text("Not in FissionOxide yet");
    ui.add_space(4.0);
    ui.checkbox(&mut w.local, "Save to my computer");
    ui.add_enabled_ui(w.local, |ui| {
      ui.horizontal(|ui| {
        ui.add(
          egui::TextEdit::singleline(&mut w.location)
            .desired_width(ui.available_width() - 40.0)
            .margin(egui::vec2(6.0, 5.0)),
        );
        if ui
          .add(file_dialogs::white_button("...").min_size(egui::vec2(32.0, 26.0)))
          .on_hover_text("Choose the folder")
          .clicked()
          && let Some(p) = rfd::FileDialog::new().set_title("Location").set_directory(&w.location).pick_folder()
        {
          w.location = p.display().to_string().replace('\\', "/");
        }
      });
    });
    let b = file_dialogs::footer(ui, None, "Save", w.local && !w.name.trim().is_empty());
    save |= b.primary;
    cancel = b.cancel;
  });
  if closed || cancel {
    return;
  }
  if save && w.local && !w.name.trim().is_empty() && w.save(app) {
    return;
  }
  app.export_dxf = Some(w);
}
