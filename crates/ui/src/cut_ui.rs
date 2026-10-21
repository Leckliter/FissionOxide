//! The 2D cut window (laser, plasma, waterjet): machine, process and a few overrides, nesting on
//! sheets, a summary of the cut, G-code (or DXF) written to a file, and cut jobs kept in the
//! design with their toolpaths previewed in the viewport and lead positions picked on them.

use egui::Pos2;
use fox_engine::kernel::glam::DVec3;
use fox_engine::render::ray_plane;
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
  /// The stored job being edited (`None` = a new cut).
  pub job: Option<u32>,
  /// Nest copies on sheets.
  pub nest: bool,
  pub sheet_width: String,
  pub sheet_height: String,
  pub spacing: String,
  pub margin: String,
  pub quantity: String,
  /// Orientations a part may take (1, 2, 4 or 8).
  pub rotations: u32,
  pub mirror: bool,
  /// Clicks on the previewed cut pick where each contour's lead-in starts.
  pub pick_leads: bool,
  /// The last summary or error.
  pub result: Option<Result<Value, String>>,
}

const LEADS: [(&str, &str); 4] = [("", "Process default"), ("arc", "Arc"), ("line", "Line"), ("none", "None")];
const ROTATIONS: [(u32, &str); 4] = [(1, "As drawn"), (2, "0° and 180°"), (4, "Every 90°"), (8, "Every 45°")];

impl App {
  pub fn open_cut_window(&mut self) {
    match self.s.execute("cut.machines", &json!({})) {
      Ok(v) => {
        let mut w = CutWindow { machines: v.as_array().cloned().unwrap_or_default(), ..Default::default() };
        w.reset_nest();
        // pick up the last stored job, as the Manufacture workspace shows it
        if let Some(j) = self.s.design.cut_jobs.last() {
          let (id, params) = (j.id, j.params.clone());
          w.load(id, &params);
          self.cut_preview(id);
        }
        self.cut_window = Some(w);
      }
      Err(e) => self.status = Some((e.0, true)),
    }
  }

  /// Plans a stored job and shows its toolpaths.
  pub fn cut_preview(&mut self, job: u32) -> Option<Result<Value, String>> {
    if self.rebuild.is_some() {
      return None;
    }
    Some(self.s.execute("cut.job.preview", &json!({"job": job})).map_err(|e| e.0))
  }

  /// Keeps the previewed cut job in step with the design (Manufacture workspace or the cut
  /// window open).
  pub fn refresh_cut_preview(&mut self) {
    if !(self.manufacture || self.cut_window.is_some()) {
      return;
    }
    let Some((job, rev)) = self.s.cut_preview.as_ref().map(|c| (c.job, c.revision)) else {
      // the Manufacture workspace shows the last job
      if self.manufacture
        && let Some(j) = self.s.design.cut_jobs.last()
        && self.cut_preview_failed != Some((j.id, self.s.revision))
      {
        let id = j.id;
        if let Some(Err(_)) = self.cut_preview(id) {
          self.cut_preview_failed = Some((id, self.s.revision));
        }
      }
      return;
    };
    if rev != self.s.revision && self.s.design.cut_jobs.iter().any(|j| j.id == job) {
      let r = self.cut_preview(job);
      if let (Some(r), Some(w)) = (r, self.cut_window.as_mut())
        && w.job == Some(job)
      {
        w.result = Some(r);
      }
    }
  }
}

fn text(v: &Value) -> String {
  match v {
    Value::Number(n) => n.to_string(),
    Value::String(s) => s.clone(),
    _ => String::new(),
  }
}

impl CutWindow {
  fn reset_nest(&mut self) {
    self.nest = false;
    self.sheet_width = "1250".into();
    self.sheet_height = "2500".into();
    self.spacing = String::new();
    self.margin = "5".into();
    self.quantity = "1".into();
    self.rotations = 4;
    self.mirror = false;
  }

  /// Fills the form from a stored job's parameters.
  fn load(&mut self, id: u32, p: &Value) {
    self.job = Some(id);
    let mid = p["machine"].as_str().unwrap_or("grbl_laser");
    self.machine =
      self.machines.iter().position(|m| m["id"].as_str().is_some_and(|i| i.eq_ignore_ascii_case(mid))).unwrap_or(0);
    self.process = 0;
    if let Some(name) = p["process"].as_str()
      && let Some(procs) = self.machines.get(self.machine).and_then(|m| m["processes"].as_array())
    {
      self.process = procs.iter().position(|x| x["name"].as_str() == Some(name)).unwrap_or(0);
    }
    self.kerf = text(&p["kerf"]);
    self.feed = text(&p["feed"]);
    self.tabs = text(&p["tabs"]);
    self.lead_in = p["lead_in"].as_str().unwrap_or("").to_string();
    self.inch = matches!(p["units"].as_str(), Some("inch" | "in"));
    self.reset_nest();
    if let Some(n) = p.get("nest").filter(|n| n.is_object()) {
      self.nest = true;
      if let Some(s) = n["sheet"].as_array() {
        self.sheet_width = text(&s[0]);
        self.sheet_height = text(&s[1]);
      }
      self.spacing = text(&n["spacing"]);
      if !n["margin"].is_null() {
        self.margin = text(&n["margin"]);
      }
      self.rotations = n["rotations"].as_u64().map(|r| r as u32).unwrap_or(4);
      self.mirror = n["mirror"].as_bool().unwrap_or(false);
      if !p["quantity"].is_null() {
        self.quantity = text(&p["quantity"]);
      }
    }
  }

  /// The form as `cut.gcode` parameters. `edit`: empty fields are null (removed from a job).
  fn params(&self, app: &App, edit: bool) -> Value {
    let mut p = json!({});
    if let Some(m) = self.machines.get(self.machine) {
      p["machine"] = m["id"].clone();
      if let Some(pr) = m["processes"].get(self.process) {
        p["process"] = pr["name"].clone();
      }
    }
    if !edit && let Some(id) = app.s.edit_sketch {
      p["sketch"] = json!(id.0);
    }
    let unset = Value::Null;
    for (k, v) in [("kerf", &self.kerf), ("feed", &self.feed)] {
      p[k] = if v.trim().is_empty() { unset.clone() } else { json!(v.trim()) };
    }
    p["tabs"] = match self.tabs.trim().parse::<u32>() {
      Ok(t) => json!(t),
      Err(_) => unset.clone(),
    };
    p["lead_in"] = if self.lead_in.is_empty() { unset.clone() } else { json!(self.lead_in) };
    p["units"] = json!(if self.inch { "inch" } else { "mm" });
    if self.nest {
      let mut n = json!({
        "sheet": [self.sheet_width.trim(), self.sheet_height.trim()],
        "rotations": self.rotations,
        "mirror": self.mirror,
      });
      for (k, v) in [("spacing", &self.spacing), ("margin", &self.margin)] {
        if !v.trim().is_empty() {
          n[k] = json!(v.trim());
        }
      }
      p["nest"] = n;
      p["quantity"] = json!(self.quantity.trim().parse::<u32>().unwrap_or(1));
    } else {
      p["nest"] = unset.clone();
      p["quantity"] = unset;
    }
    if !edit && let Some(o) = p.as_object_mut() {
      o.retain(|_, v| !v.is_null());
    }
    p
  }
}

/// With lead picking on, a click on the previewed cut's plane sets where the nearest contour's
/// lead-in starts.
pub fn lead_clicks(app: &mut App, resp: &egui::Response, ray: &dyn Fn(Pos2) -> (DVec3, DVec3)) -> bool {
  let Some(job) = app.cut_window.as_ref().filter(|w| w.pick_leads).and_then(|w| w.job) else { return false };
  let Some(cp) = app.s.cut_preview.as_ref().filter(|c| c.job == job) else { return false };
  let n = cp.plane.normal();
  let origin = cp.plane.origin + n * cp.lift;
  let hit = resp.hover_pos().and_then(|p| {
    let (o, d) = ray(p);
    ray_plane(o, d, origin, n)
  });
  if hit.is_some() {
    resp.clone().on_hover_cursor(egui::CursorIcon::Crosshair);
  }
  if !resp.clicked() {
    return hit.is_some();
  }
  let Some(p) = hit else { return false };
  if let Some(r) = app.run("cut.job.lead", json!({"job": job, "point": [p.x, p.y, p.z]})) {
    app.status = Some(("Lead-in moved; click another contour, or turn picking off".into(), false));
    let r = app.cut_preview(job).unwrap_or(Ok(r));
    if let Some(w) = app.cut_window.as_mut() {
      w.result = Some(r);
    }
  }
  true
}

pub fn cut_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut w) = app.cut_window.take() else { return };
  let mut open = true;
  let (mut preview, mut save, mut keep, mut update, mut delete, mut clear_leads) =
    (false, false, false, false, false, false);
  let mut load: Option<Option<u32>> = None;
  egui::Window::new("2D Cut").open(&mut open).resizable(true).default_width(440.0).show(ctx, |ui| {
    let jobs: Vec<(u32, String)> = app.s.design.cut_jobs.iter().map(|j| (j.id, j.name.clone())).collect();
    ui.horizontal(|ui| {
      ui.label("Cut");
      let current = match w.job.and_then(|id| jobs.iter().find(|j| j.0 == id)) {
        Some(j) => j.1.clone(),
        None => "New cut".into(),
      };
      egui::ComboBox::from_id_salt("cut-job").selected_text(current).width(220.0).show_ui(ui, |ui| {
        if ui.selectable_label(w.job.is_none(), "New cut").clicked() {
          load = Some(None);
        }
        for (id, name) in &jobs {
          if ui.selectable_label(w.job == Some(*id), name).clicked() {
            load = Some(Some(*id));
          }
        }
      });
    });
    let source = match (w.job, app.s.edit_sketch) {
      (Some(_), _) => "A cut kept in the design: it follows design changes and shows in the viewport.".to_string(),
      (None, Some(id)) => format!("Cuts the closed loops of sketch {} (being edited).", id.0),
      (None, None) => "Cuts the flat pattern of the sheet metal body.".into(),
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
    ui.checkbox(&mut w.nest, "Nest on sheets")
      .on_hover_text("Lay copies of the part out on sheets, as few as it takes");
    if w.nest {
      egui::Grid::new("cut-nest").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Sheet (mm)");
        ui.horizontal(|ui| {
          ui.add(egui::TextEdit::singleline(&mut w.sheet_width).desired_width(60.0));
          ui.label("×");
          ui.add(egui::TextEdit::singleline(&mut w.sheet_height).desired_width(60.0));
        });
        ui.end_row();
        ui.label("Copies");
        ui.add(egui::TextEdit::singleline(&mut w.quantity).desired_width(60.0));
        ui.end_row();
        ui.label("Spacing (mm)");
        ui.add(egui::TextEdit::singleline(&mut w.spacing).hint_text("3 or 2 × kerf").desired_width(120.0));
        ui.end_row();
        ui.label("Margin (mm)");
        ui.add(egui::TextEdit::singleline(&mut w.margin).desired_width(60.0));
        ui.end_row();
        ui.label("Rotations");
        let r = ROTATIONS.iter().find(|r| r.0 == w.rotations).map(|r| r.1).unwrap_or("Every 90°");
        egui::ComboBox::from_id_salt("cut-rot").selected_text(r).show_ui(ui, |ui| {
          for (n, label) in ROTATIONS {
            ui.selectable_value(&mut w.rotations, n, label);
          }
        });
        ui.end_row();
        ui.label("");
        ui.checkbox(&mut w.mirror, "Parts may be flipped over");
        ui.end_row();
      });
    }
    ui.horizontal(|ui| {
      preview = ui.button("Preview").clicked();
      save = ui.button("Save…").clicked();
      match w.job {
        None => {
          keep = ui
            .button("Keep in design")
            .on_hover_text("Store the cut with the design; it follows design changes")
            .clicked()
        }
        Some(_) => {
          update = ui.button("Update").clicked();
          delete = ui.button("Delete").clicked();
        }
      }
    });
    if w.job.is_some() {
      ui.horizontal(|ui| {
        ui.toggle_value(&mut w.pick_leads, "Pick lead positions")
          .on_hover_text("Click on the previewed cut where a contour's lead-in should start");
        clear_leads = ui.button("Reset leads").clicked();
      });
    }
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
        if let Some(n) = r.get("nest") {
          let sheets = n["sheets"].as_array().cloned().unwrap_or_default();
          ui.label(format!(
            "{} sheet(s), {:.0} % of the material used",
            sheets.len(),
            n["utilisation"].as_f64().unwrap_or(0.0) * 100.0
          ));
          for (k, s) in sheets.iter().enumerate() {
            ui.label(format!(
              "  Sheet {}: {} parts, {:.0} %",
              k + 1,
              s["parts"],
              s["utilisation"].as_f64().unwrap_or(0.0) * 100.0
            ));
          }
          for u in n["unplaced"].as_array().into_iter().flatten() {
            ui.colored_label(
              egui::Color32::from_rgb(220, 160, 40),
              format!("{} × {} not placed", u["count"], u["part"].as_str().unwrap_or("")),
            );
          }
        }
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
  if let Some(j) = load {
    w.pick_leads = false;
    w.result = None;
    match j {
      Some(id) => {
        if let Some(job) = app.s.design.cut_jobs.iter().find(|x| x.id == id) {
          let params = job.params.clone();
          w.load(id, &params);
          w.result = app.cut_preview(id);
        }
      }
      None => {
        w.job = None;
        app.s.cut_preview = None;
      }
    }
  }
  if preview {
    w.result = match w.job {
      Some(id) => app.cut_preview(id),
      None => Some(app.s.execute("cut.gcode", &w.params(app, false)).map_err(|e| e.0)),
    };
  }
  if keep {
    let mut p = w.params(app, false);
    p["name"] = json!(format!("Cut{}", app.s.design.cut_jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1));
    match app.s.execute("cut.job.create", &p) {
      Ok(r) => {
        let id = r["job"].as_u64().unwrap_or(0) as u32;
        w.job = Some(id);
        w.result = app.cut_preview(id).or(Some(Ok(r)));
        app.status = Some(("The cut is kept in the design and follows its changes".into(), false));
      }
      Err(e) => w.result = Some(Err(e.0)),
    }
  }
  if update && let Some(id) = w.job {
    let mut p = w.params(app, true);
    p["job"] = json!(id);
    w.result = match app.s.execute("cut.job.edit", &p) {
      Ok(r) => app.cut_preview(id).or(Some(Ok(r))),
      Err(e) => Some(Err(e.0)),
    };
  }
  if clear_leads && let Some(id) = w.job {
    w.result = match app.s.execute("cut.job.lead", &json!({"job": id, "clear": true})) {
      Ok(r) => app.cut_preview(id).or(Some(Ok(r))),
      Err(e) => Some(Err(e.0)),
    };
  }
  if delete
    && let Some(id) = w.job
    && app.run("cut.job.delete", json!({"job": id})).is_some()
  {
    w.job = None;
    w.pick_leads = false;
    w.result = None;
  }
  if save {
    let ext = w.machines.get(w.machine).and_then(|m| m["extension"].as_str()).unwrap_or("nc").to_string();
    if let Some(path) = rfd::FileDialog::new()
      .add_filter("Cut program", &[ext.as_str()])
      .set_file_name(format!("{}.{ext}", app.s.design.name))
      .save_file()
    {
      let mut p = match w.job {
        Some(id) => json!({"job": id}),
        None => w.params(app, false),
      };
      p["path"] = json!(path);
      let r = app.s.execute("cut.gcode", &p).map_err(|e| e.0);
      if let Ok(v) = &r {
        let n = v["paths"].as_array().map(Vec::len).unwrap_or(1);
        app.status = Some(match n {
          1 => (format!("Wrote the cut to {}", path.display()), false),
          n => (format!("Wrote {n} programs, one per sheet, next to {}", path.display()), false),
        });
      }
      w.result = Some(r);
    }
  }
  if open {
    app.cut_window = Some(w);
  } else if !app.manufacture {
    app.s.cut_preview = None;
  }
}
