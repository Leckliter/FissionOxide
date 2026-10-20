//! The 2D cut window (laser, plasma, waterjet): machine, process and a few overrides, a summary
//! of the cut, and G-code (or DXF) written to a file.

use serde_json::{Value, json};

use crate::App;

/// The cut window's state, while open.
#[derive(Debug, Clone, Default)]
pub struct CutWindow {
  pub machines: Vec<Value>,
  pub machine: usize,
  pub process: usize,
  /// Overrides as text; empty = the process's value.
  pub kerf: String,
  pub feed: String,
  pub tabs: String,
  pub lead_in: String,
  pub inch: bool,
  /// The last summary or error.
  pub result: Option<Result<Value, String>>,
}

const LEADS: [(&str, &str); 4] = [("", "Process default"), ("arc", "Arc"), ("line", "Line"), ("none", "None")];

impl App {
  pub fn open_cut_window(&mut self) {
    match self.s.execute("cut.machines", &json!({})) {
      Ok(v) => {
        self.cut_window = Some(CutWindow { machines: v.as_array().cloned().unwrap_or_default(), ..Default::default() });
      }
      Err(e) => self.status = Some((e.0, true)),
    }
  }
}

impl CutWindow {
  fn params(&self, app: &App) -> Value {
    let mut p = json!({});
    if let Some(m) = self.machines.get(self.machine) {
      p["machine"] = m["id"].clone();
      if let Some(pr) = m["processes"].get(self.process) {
        p["process"] = pr["name"].clone();
      }
    }
    if let Some(id) = app.s.edit_sketch {
      p["sketch"] = json!(id.0);
    }
    for (k, v) in [("kerf", &self.kerf), ("feed", &self.feed)] {
      if !v.trim().is_empty() {
        p[k] = json!(v.trim());
      }
    }
    if let Ok(t) = self.tabs.trim().parse::<u32>() {
      p["tabs"] = json!(t);
    }
    if !self.lead_in.is_empty() {
      p["lead_in"] = json!(self.lead_in);
    }
    p["units"] = json!(if self.inch { "inch" } else { "mm" });
    p
  }
}

pub fn cut_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut w) = app.cut_window.take() else { return };
  let mut open = true;
  let (mut preview, mut save) = (false, false);
  egui::Window::new("2D Cut").open(&mut open).resizable(true).default_width(420.0).show(ctx, |ui| {
    let source = match app.s.edit_sketch {
      Some(id) => format!("Cuts the closed loops of sketch {} (being edited).", id.0),
      None => "Cuts the flat pattern of the sheet metal body.".into(),
    };
    ui.label(egui::RichText::new(source).weak());
    egui::Grid::new("cut-form").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
      ui.label("Machine");
      let name = |m: &Value| m["name"].as_str().unwrap_or("").to_string();
      let current = w.machines.get(w.machine).map(name).unwrap_or_default();
      egui::ComboBox::from_id_salt("cut-machine").selected_text(current).width(260.0).show_ui(ui, |ui| {
        for (i, m) in w.machines.iter().enumerate() {
          if ui.selectable_label(i == w.machine, name(m)).clicked() {
            w.machine = i;
            w.process = 0;
          }
        }
      });
      ui.end_row();
      ui.label("Process");
      let procs: Vec<String> = w
        .machines
        .get(w.machine)
        .and_then(|m| m["processes"].as_array())
        .map(|a| a.iter().map(|p| p["name"].as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_default();
      egui::ComboBox::from_id_salt("cut-process")
        .selected_text(procs.get(w.process).cloned().unwrap_or_default())
        .width(260.0)
        .show_ui(ui, |ui| {
          for (i, p) in procs.iter().enumerate() {
            ui.selectable_value(&mut w.process, i, p);
          }
        });
      ui.end_row();
      for (label, f) in
        [("Kerf (mm)", &mut w.kerf), ("Feed (mm/min)", &mut w.feed), ("Micro-joints per part", &mut w.tabs)]
      {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(f).hint_text("process default").desired_width(120.0));
        ui.end_row();
      }
      ui.label("Lead-in");
      let lead = LEADS.iter().find(|l| l.0 == w.lead_in).map(|l| l.1).unwrap_or("Process default");
      egui::ComboBox::from_id_salt("cut-lead").selected_text(lead).show_ui(ui, |ui| {
        for (id, label) in LEADS {
          ui.selectable_value(&mut w.lead_in, id.to_string(), label);
        }
      });
      ui.end_row();
      ui.label("Units");
      ui.checkbox(&mut w.inch, "Inches");
      ui.end_row();
    });
    ui.horizontal(|ui| {
      preview = ui.button("Preview").clicked();
      save = ui.button("Save…").clicked();
    });
    match &w.result {
      Some(Ok(r)) => {
        ui.separator();
        let f = |k: &str| r[k].as_f64().unwrap_or(0.0);
        ui.label(format!(
          "{} contours ({} holes), {} pierces, {:.0} mm cut, about {:.1} min",
          r["contours"],
          r["holes"],
          r["pierces"],
          f("cut_length"),
          f("time_s") / 60.0
        ));
        for warn in r["warnings"].as_array().into_iter().flatten() {
          ui.colored_label(egui::Color32::from_rgb(220, 160, 40), warn.as_str().unwrap_or(""));
        }
      }
      Some(Err(e)) => {
        ui.separator();
        ui.colored_label(egui::Color32::from_rgb(220, 80, 60), e);
      }
      None => {}
    }
  });
  if preview {
    w.result = Some(app.s.execute("cut.gcode", &w.params(app)).map_err(|e| e.0));
  }
  if save {
    let ext = w.machines.get(w.machine).and_then(|m| m["extension"].as_str()).unwrap_or("nc").to_string();
    if let Some(path) = rfd::FileDialog::new()
      .add_filter("Cut program", &[ext.as_str()])
      .set_file_name(format!("{}.{ext}", app.s.design.name))
      .save_file()
    {
      let mut p = w.params(app);
      p["path"] = json!(path);
      let r = app.s.execute("cut.gcode", &p).map_err(|e| e.0);
      if r.is_ok() {
        app.status = Some((format!("Wrote the cut to {}", path.display()), false));
      }
      w.result = Some(r);
    }
  }
  if open {
    app.cut_window = Some(w);
  }
}
