//! The cut-service check window: a cutting service (SendCutSend, OSH Cut, Ponoko or the hub's
//! own), process, material and thickness; PASS / WARNINGS / ERRORS with each issue's location
//! (Zoom flies the camera there), and the cut estimate with CSV and JSON export.

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{Value, json};

use crate::App;

/// The window's state, while open.
#[derive(Debug, Clone, Default)]
pub struct CutCheckWindow {
  pub services: Vec<Value>,
  pub service: usize,
  /// 0 = chosen by material, else processes[i - 1].
  pub process: usize,
  pub material: String,
  /// Empty = the body's or the file's, else 1/8".
  pub thickness: String,
  /// Check this DXF file instead of the sketch being edited or the sheet metal body.
  pub dxf: Option<PathBuf>,
  pub result: Option<Result<Value, String>>,
}

static WINDOW: Mutex<Option<CutCheckWindow>> = Mutex::new(None);

fn take() -> Option<CutCheckWindow> {
  WINDOW.lock().ok().and_then(|mut w| w.take())
}

fn put(w: CutCheckWindow) {
  if let Ok(mut g) = WINDOW.lock() {
    *g = Some(w);
  }
}

/// Opens the window (Manufacture and Sheet Metal → Cut Service Check).
pub fn open(app: &mut App) {
  match app.s.execute("cut.service.list", &json!({})) {
    Ok(v) => put(CutCheckWindow { services: v.as_array().cloned().unwrap_or_default(), ..Default::default() }),
    Err(e) => app.status = Some((e.0, true)),
  }
}

impl CutCheckWindow {
  fn params(&self, app: &App) -> Value {
    let mut p = json!({});
    let svc = self.services.get(self.service);
    if let Some(svc) = svc {
      p["service"] = svc["id"].clone();
      if self.process > 0 {
        if let Some(pr) = svc["processes"].get(self.process - 1) {
          let name = pr["name"].as_str().filter(|n| !n.is_empty()).map(|n| json!(n)).unwrap_or(pr["process"].clone());
          p["process"] = name;
        }
      }
    }
    if !self.material.trim().is_empty() {
      p["material"] = json!(self.material.trim());
    }
    if !self.thickness.trim().is_empty() {
      p["thickness"] = json!(self.thickness.trim());
    }
    match (&self.dxf, app.s.edit_sketch) {
      (Some(f), _) => p["dxf"] = json!(f),
      (None, Some(id)) => p["sketch"] = json!(id.0),
      (None, None) => {}
    }
    p
  }
}

const RED: egui::Color32 = egui::Color32::from_rgb(220, 80, 60);
const AMBER: egui::Color32 = egui::Color32::from_rgb(220, 160, 40);
const GREEN: egui::Color32 = egui::Color32::from_rgb(80, 180, 90);

/// Flies the camera to a model point, close enough to see a small feature.
fn zoom_to(app: &mut App, at: [f64; 3]) {
  let mut to = app.view_now();
  to.target = at;
  to.distance = to.distance.min(80.0);
  app.fly_to(to);
}

fn estimate_ui(ui: &mut egui::Ui, e: &Value) {
  let f = |k: &str| e[k].as_f64().unwrap_or(0.0);
  let pair = |k: &str, i: usize| e[k][i].as_f64().unwrap_or(0.0);
  ui.label(format!(
    "{} piece(s), {} hole(s), {} pierce(s)",
    e["pieces"].as_u64().unwrap_or(0),
    e["holes"].as_u64().unwrap_or(0),
    e["pierces"].as_u64().unwrap_or(0)
  ));
  ui.label(format!(
    "Area {:.3} in² ({:.0} mm²), cut length {:.2} in ({:.0} mm)",
    f("area_in2"),
    f("area_mm2"),
    f("cut_length_in"),
    f("cut_length_mm")
  ));
  ui.label(format!(
    "Overall {:.3}\" × {:.3}\" ({:.1} × {:.1} mm)",
    pair("size_in", 0),
    pair("size_in", 1),
    pair("size_mm", 0),
    pair("size_mm", 1)
  ));
  let parts = e["parts"].as_array().cloned().unwrap_or_default();
  if parts.len() > 1 {
    egui::CollapsingHeader::new(format!("Parts ({})", parts.len())).id_salt("cut-check-parts").show(ui, |ui| {
      egui::Grid::new("cut-check-parts-grid").striped(true).show(ui, |ui| {
        for h in ["Part", "Size (in)", "Area (in²)", "Cut (in)", "Pierces"] {
          ui.strong(h);
        }
        ui.end_row();
        for p in &parts {
          let g = |k: &str| p[k].as_f64().unwrap_or(0.0);
          ui.label(p["part"].to_string());
          ui.label(format!(
            "{:.3} × {:.3}",
            p["size_in"][0].as_f64().unwrap_or(0.0),
            p["size_in"][1].as_f64().unwrap_or(0.0)
          ));
          ui.label(format!("{:.3}", g("area_in2")));
          ui.label(format!("{:.2}", g("cut_length_in")));
          ui.label(p["pierces"].to_string());
          ui.end_row();
        }
      });
    });
  }
}

pub fn window(app: &mut App, ctx: &egui::Context) {
  let Some(mut w) = take() else { return };
  let mut open = true;
  let (mut run, mut pick_dxf, mut export): (bool, bool, Option<&str>) = (false, false, None);
  let mut zoom: Option<[f64; 3]> = None;
  egui::Window::new("Cut Service Check").open(&mut open).resizable(true).default_width(460.0).show(ctx, |ui| {
    let source = match (&w.dxf, app.s.edit_sketch) {
      (Some(f), _) => format!("Checks {}.", f.display()),
      (None, Some(id)) => format!("Checks sketch {} (being edited).", id.0),
      (None, None) => "Checks the flat pattern of the sheet metal body.".into(),
    };
    ui.label(egui::RichText::new(source).weak());
    egui::Grid::new("cut-check-form").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
      ui.label("Service");
      let name = |v: &Value| v["name"].as_str().unwrap_or("").to_string();
      let current = w.services.get(w.service).map(name).unwrap_or_default();
      egui::ComboBox::from_id_salt("cut-check-service").selected_text(current).width(260.0).show_ui(ui, |ui| {
        for (i, v) in w.services.iter().enumerate() {
          if ui.selectable_label(i == w.service, name(v)).clicked() {
            w.service = i;
            w.process = 0;
          }
        }
      });
      ui.end_row();
      ui.label("Process");
      let mut procs = vec!["By material".to_string()];
      if let Some(a) = w.services.get(w.service).and_then(|v| v["processes"].as_array()) {
        for p in a {
          let n = p["name"].as_str().filter(|n| !n.is_empty()).or(p["process"].as_str()).unwrap_or("laser");
          procs.push(n.to_string());
        }
      }
      egui::ComboBox::from_id_salt("cut-check-process")
        .selected_text(procs.get(w.process).cloned().unwrap_or_default())
        .width(260.0)
        .show_ui(ui, |ui| {
          for (i, p) in procs.iter().enumerate() {
            ui.selectable_value(&mut w.process, i, p);
          }
        });
      ui.end_row();
      ui.label("Material");
      ui.add(egui::TextEdit::singleline(&mut w.material).hint_text("e.g. 5052 aluminum").desired_width(200.0));
      ui.end_row();
      ui.label("Thickness");
      ui.add(egui::TextEdit::singleline(&mut w.thickness).hint_text("body's, or 0.125 in").desired_width(200.0));
      ui.end_row();
    });
    if let Some(note) = w.services.get(w.service).and_then(|v| v["notes"].as_str()).filter(|n| !n.is_empty()) {
      ui.label(egui::RichText::new(note).weak().small());
    }
    ui.horizontal(|ui| {
      run = ui.button("Check").clicked();
      pick_dxf = ui.button("Check a DXF…").clicked();
      if w.dxf.is_some() && ui.button("Use the model").clicked() {
        w.dxf = None;
        w.result = None;
      }
    });
    match &w.result {
      Some(Ok(r)) => {
        ui.separator();
        let color = match r["status"].as_str() {
          Some("pass") => GREEN,
          Some("warnings") => AMBER,
          _ => RED,
        };
        ui.colored_label(color, egui::RichText::new(r["summary"].as_str().unwrap_or("")).strong());
        if r["approximate"] == true {
          ui.label(egui::RichText::new("The service's limits are approximate: its material pages win.").weak().small());
        }
        if r["thickness_assumed"] == true {
          ui.label(egui::RichText::new("No thickness given: checked as 1/8\".").weak().small());
        }
        egui::ScrollArea::vertical().max_height(220.0).id_salt("cut-check-issues").show(ui, |ui| {
          for i in r["issues"].as_array().into_iter().flatten() {
            ui.horizontal(|ui| {
              let (c, tag) = match i["severity"].as_str() {
                Some("error") => (RED, "Error"),
                Some("warning") => (AMBER, "Warning"),
                _ => (ui.visuals().weak_text_color(), "Info"),
              };
              ui.colored_label(c, tag);
              ui.label(i["message"].as_str().unwrap_or(""));
              if let Some(at) = i["world"]
                .as_array()
                .and_then(|a| Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?]))
              {
                if ui.small_button("Zoom").clicked() {
                  zoom = Some(at);
                }
              }
            });
          }
        });
        ui.separator();
        ui.strong("Cut estimate");
        estimate_ui(ui, &r["estimate"]);
        ui.horizontal(|ui| {
          if ui.button("Export CSV…").clicked() {
            export = Some("csv");
          }
          if ui.button("Export JSON…").clicked() {
            export = Some("json");
          }
        });
      }
      Some(Err(e)) => {
        ui.separator();
        ui.colored_label(RED, e);
      }
      None => {}
    }
  });
  if pick_dxf {
    if let Some(f) = rfd::FileDialog::new().add_filter("DXF", &["dxf"]).pick_file() {
      w.dxf = Some(f);
      run = true;
    }
  }
  if run {
    w.result = Some(app.s.execute("cut.service.check", &w.params(app)).map_err(|e| e.0));
  }
  if let Some(ext) = export {
    if let Some(path) = rfd::FileDialog::new()
      .add_filter(if ext == "csv" { "CSV" } else { "JSON" }, &[ext])
      .set_file_name(format!("{} - cut estimate.{ext}", app.s.design.name))
      .save_file()
    {
      let mut p = w.params(app);
      p["export"] = json!(path);
      match app.s.execute("cut.service.check", &p) {
        Ok(r) => {
          app.status = Some((format!("Wrote {}", path.display()), false));
          w.result = Some(Ok(r));
        }
        Err(e) => w.result = Some(Err(e.0)),
      }
    }
  }
  if let Some(at) = zoom {
    zoom_to(app, at);
  }
  if open {
    put(w);
  }
}
