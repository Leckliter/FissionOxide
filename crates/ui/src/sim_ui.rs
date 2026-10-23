//! The Simulation workspace: studies (static stress, modal frequencies, thermal), their
//! materials, constraints, loads, contacts and mesh, solved with `fox-sim` and shown as contours
//! with a legend, probes and animated mode shapes. Everything goes through the `sim.*`
//! commands; this module is the toolbar, the study browser and the viewport overlays.

use egui::{Color32, Pos2, vec2};
use fox_engine::Sel;
use serde_json::{Value, json};

use crate::{App, theme};

/// How results are deformed in the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Deform {
  /// The largest displacement shows as 5 % of the model (Fusion's Adjusted).
  #[default]
  Adjusted,
  Actual,
  None,
}

/// A constraint or load being set up: its kind and the values typed so far.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
  /// `sim.constraint` or `sim.load`.
  pub cmd: &'static str,
  pub kind: &'static str,
  pub title: &'static str,
  /// (parameter, label, text).
  pub fields: Vec<(&'static str, &'static str, String)>,
  /// Direction for forces: normal, x, y, z, -x, -y, -z.
  pub direction: String,
  pub flags: Vec<(&'static str, &'static str, bool)>,
  /// Takes no faces (gravity).
  pub no_faces: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SimUi {
  /// The study shown (`None`: the last).
  pub study: Option<u32>,
  pub field: String,
  pub mode: usize,
  pub deform: Deform,
  pub animate: bool,
  pub show_mesh: bool,
  /// Clicks in the viewport place probes.
  pub probing: bool,
  /// (point, value text).
  pub probes: Vec<([f64; 3], String)>,
  pub form: Option<Form>,
  /// The last solve's summary rows.
  pub summary: Vec<(String, String)>,
  /// Bumped on every solve so the viewport redraws.
  pub nonce: u64,
  pub mesh_size: String,
  pub refine_size: String,
  seen_click: Option<Pos2>,
  started: f64,
}

impl App {
  /// Opens the Simulation workspace.
  pub fn open_simulation(&mut self) {
    self.cancel_dialog();
    if self.s.edit_sketch.is_some() {
      self.finish_sketch();
    }
    let ui = SimUi { mode: 1, mesh_size: String::new(), refine_size: "1".into(), ..Default::default() };
    self.simulation = Some(ui);
  }
}

/// The study shown: the chosen one if it still exists, else the last.
fn current(app: &App) -> Option<u32> {
  let want = app.simulation.as_ref().and_then(|v| v.study);
  match want {
    Some(id) if app.s.design.sim.iter().any(|s| s.id == id) => Some(id),
    _ => app.s.design.sim.last().map(|s| s.id),
  }
}

fn kind_of(app: &App) -> Option<fox_engine::model::sim::StudyKind> {
  let id = current(app)?;
  app.s.design.sim.iter().find(|s| s.id == id).map(|s| s.kind)
}

/// The `sim` view parameters for the viewport, while results (or the mesh) are shown.
pub fn view_json(app: &App) -> Option<Value> {
  let v = app.simulation.as_ref()?;
  let study = current(app)?;
  let ran = app.s.sim_runs.iter().any(|r| r.study == study);
  if !ran {
    return None;
  }
  let phase = if v.animate { Some((now() - v.started) * 4.0) } else { None };
  let scale = match v.deform {
    Deform::Adjusted => json!("adjusted"),
    Deform::Actual => json!(1.0),
    Deform::None => json!(0.0),
  };
  Some(json!({
    "study": study, "field": v.field, "mode": v.mode, "scale": scale, "phase": phase,
    "mesh": v.show_mesh, "nonce": v.nonce,
  }))
}

fn now() -> f64 {
  std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn selected_faces(app: &App) -> Vec<Value> {
  app
    .s
    .selection
    .iter()
    .filter_map(|s| if let Sel::Face { body, face } = s { Some(json!({"body": body.0, "face": face})) } else { None })
    .collect()
}

fn selected_bodies(app: &App) -> Vec<u32> {
  app.s.selection.iter().filter_map(|s| if let Sel::Body { body } = s { Some(body.0) } else { None }).collect()
}

fn form(cmd: &'static str, kind: &'static str, title: &'static str) -> Form {
  let f = |k: &'static str, l: &'static str, v: &str| (k, l, v.to_string());
  let (fields, flags, no_faces) = match kind {
    "fixed" => (vec![], vec![("x", "X", true), ("y", "Y", true), ("z", "Z", true)], false),
    "pin" => {
      (vec![], vec![("radial", "Radial", true), ("axial", "Axial", true), ("tangential", "Tangential", false)], false)
    }
    "frictionless" => (vec![], vec![], false),
    "force" | "bearing" => (vec![f("magnitude", "Magnitude (N)", "100")], vec![], false),
    "pressure" => (vec![f("pressure", "Pressure (MPa)", "1")], vec![], false),
    "moment" => (vec![f("magnitude", "Magnitude (N·mm)", "1000")], vec![], false),
    "gravity" => (vec![f("magnitude", "Acceleration (mm/s²)", "9806.65")], vec![], true),
    "temperature" => (vec![f("temperature", "Temperature (°C)", "100")], vec![], false),
    "heat_flux" => (vec![f("flux", "Heat flux (W/m²)", "1000")], vec![], false),
    "heat" => (vec![f("power", "Heat (W)", "10")], vec![], false),
    "convection" => {
      (vec![f("coefficient", "Coefficient (W/(m²·K))", "10"), f("ambient", "Ambient (°C)", "20")], vec![], false)
    }
    _ => (vec![], vec![], false),
  };
  let direction = if matches!(kind, "gravity") { "-z".to_string() } else { "normal".to_string() };
  Form { cmd, kind, title, fields, direction, flags, no_faces }
}

fn tool(ui: &mut egui::Ui, label: &str, tip: &str, enabled: bool) -> bool {
  ui.add_enabled(enabled, egui::Button::new(egui::RichText::new(label).size(12.0)).min_size(vec2(0.0, 26.0)))
    .on_hover_text(tip)
    .clicked()
}

fn group(ui: &mut egui::Ui, name: &str, add: impl FnOnce(&mut egui::Ui)) {
  ui.vertical(|ui| {
    ui.horizontal(|ui| add(ui));
    ui.label(egui::RichText::new(name).size(10.0).color(theme::GROUP_LABEL));
  });
  ui.separator();
}

/// The SETUP toolbar.
pub fn toolbar(app: &mut App, ui: &mut egui::Ui) {
  use fox_engine::model::sim::StudyKind;
  let Some(mut v) = app.simulation.take() else { return };
  ui.add_space(4.0);
  ui.horizontal(|ui| {
    ui.label(egui::RichText::new("SETUP").strong().color(theme::ACCENT));
  });
  let kind = kind_of(app);
  let has = kind.is_some();
  let thermal = kind == Some(StudyKind::Thermal);
  let modal = kind == Some(StudyKind::Modal);
  ui.horizontal(|ui| {
    group(ui, "STUDY", |ui| {
      for (t, label) in [("static", "Static Stress"), ("modal", "Modal"), ("thermal", "Thermal")] {
        if tool(ui, label, &format!("New {label} study of the visible bodies (or the selected ones)"), true) {
          let bodies = selected_bodies(app);
          let mut p = json!({"type": t});
          if !bodies.is_empty() {
            p["bodies"] = json!(bodies);
          }
          if let Some(r) = app.run("sim.study", p) {
            v.study = r["study"].as_u64().map(|x| x as u32);
            v.summary.clear();
            v.probes.clear();
            v.field.clear();
          }
        }
      }
    });
    group(ui, "CONSTRAINTS", |ui| {
      for (k, label, tip) in [
        ("fixed", "Fixed", "Hold the selected faces in place (in X, Y and/or Z)"),
        ("pin", "Pin", "Hold cylindrical faces on a pin: radially, axially and/or tangentially"),
        ("frictionless", "Frictionless", "Stop the selected faces moving along their normals"),
      ] {
        if tool(ui, label, tip, has && !thermal) {
          v.form = Some(form("sim.constraint", k, label));
        }
      }
    });
    group(ui, "LOADS", |ui| {
      let loads: &[(&'static str, &'static str, &str)] = if thermal {
        &[
          ("temperature", "Temperature", "Hold the selected faces at a temperature"),
          ("heat_flux", "Heat Flux", "Heat flowing in through the selected faces (W/m²)"),
          ("heat", "Heat Source", "A total heat input over the selected faces (W)"),
          ("convection", "Convection", "Faces losing heat to a fluid"),
        ]
      } else {
        &[
          ("force", "Force", "A total force over the selected faces"),
          ("pressure", "Pressure", "A pressure pushing into the selected faces"),
          ("bearing", "Bearing", "A shaft or pin pushing on cylindrical faces"),
          ("moment", "Moment", "A moment about the selected faces' centroid"),
          ("gravity", "Gravity", "The bodies' weight"),
        ]
      };
      for (k, label, tip) in loads {
        if tool(ui, label, tip, has && !modal) {
          v.form = Some(form("sim.load", k, label));
        }
      }
    });
    group(ui, "MESH", |ui| {
      if tool(ui, "Generate Mesh", "Mesh the study's bodies into quadratic tetrahedra", has) {
        let mut p = json!({});
        if let Ok(s) = v.mesh_size.trim().parse::<f64>() {
          p["size"] = json!(s);
        }
        app.run("sim.mesh", p);
        v.show_mesh = true;
        v.nonce += 1;
      }
      if tool(ui, "Local Mesh Control", "A finer mesh on the selected faces", has) {
        let faces = selected_faces(app);
        if faces.is_empty() {
          app.status = Some(("Select the faces to refine first".into(), false));
        } else if let Ok(size) = v.refine_size.trim().parse::<f64>() {
          app.run("sim.mesh", json!({"refine": [{"faces": faces, "size": size}]}));
          v.show_mesh = true;
          v.nonce += 1;
        }
      }
    });
    group(ui, "SOLVE", |ui| {
      if tool(ui, "Solve", "Mesh (if need be) and solve the study", has) {
        solve(app, &mut v);
      }
    });
    group(ui, "RESULTS", |ui| {
      if tool(ui, "Probe", "Click on the model to read the result there", has) {
        v.probing = !v.probing;
      }
      if tool(ui, "Report", "Write the study's report (HTML)", has) {
        report(app);
      }
    });
  });
  app.simulation = Some(v);
}

fn solve(app: &mut App, v: &mut SimUi) {
  if let Some(r) = app.run("sim.solve", json!({"study": current(app)})) {
    v.summary = r["summary"]
      .as_array()
      .into_iter()
      .flatten()
      .map(|row| (row["quantity"].as_str().unwrap_or("").to_string(), row["value"].as_str().unwrap_or("").to_string()))
      .collect();
    v.show_mesh = false;
    v.nonce += 1;
    v.probes.clear();
    app.status = Some(("Solved".into(), false));
  }
}

fn report(app: &mut App) {
  let path = rfd::FileDialog::new()
    .add_filter("HTML", &["html"])
    .add_filter("Markdown", &["md"])
    .set_file_name("report.html")
    .save_file();
  if let Some(path) = path
    && app.run("sim.report", json!({"study": current(app), "path": path.to_string_lossy()})).is_some()
  {
    app.status = Some((format!("Report written to {}", path.display()), false));
  }
}

/// The study browser, the input form, the legend and probes, over the viewport.
pub fn show(app: &mut App, ctx: &egui::Context) {
  let Some(mut v) = app.simulation.take() else { return };
  browser(app, &mut v, ctx);
  if let Some(f) = v.form.take() {
    v.form = form_window(app, f, ctx);
  }
  legend(app, &v, ctx);
  probes(app, &mut v, ctx);
  if v.animate {
    ctx.request_repaint_after(std::time::Duration::from_millis(33));
  }
  app.simulation = Some(v);
}

fn browser(app: &mut App, v: &mut SimUi, ctx: &egui::Context) {
  let canvas = app.canvas;
  let width = 270.0;
  egui::Area::new(egui::Id::new("sim-browser"))
    .fixed_pos(egui::pos2(canvas.max.x - width - 8.0, canvas.min.y + 8.0))
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      // an Area's room is last frame's size; give the tree room to grow when rows are added
      ui.set_max_height(canvas.height().max(150.0));
      egui::Frame::popup(ui.style()).fill(Color32::from_rgba_unmultiplied(255, 255, 255, 240)).show(ui, |ui| {
        ui.set_width(width);
        egui::ScrollArea::vertical()
          .max_height((canvas.height() - 40.0).max(120.0))
          .auto_shrink([false, true])
          .show(ui, |ui| study_tree(app, v, ui));
      });
    });
}

fn study_tree(app: &mut App, v: &mut SimUi, ui: &mut egui::Ui) {
  ui.label(egui::RichText::new("SIMULATION STUDIES").size(11.0).color(theme::GROUP_LABEL));
  if app.s.design.sim.is_empty() {
    ui.label("No study yet: pick Static Stress, Modal or Thermal on the toolbar.");
    return;
  }
  let cur = current(app);
  let list: Vec<(u32, String)> =
    app.s.design.sim.iter().map(|s| (s.id, format!("{} ({})", s.name, s.kind.label()))).collect();
  for (id, name) in list {
    if ui.selectable_label(Some(id) == cur, name).clicked() {
      v.study = Some(id);
      v.summary.clear();
      v.probes.clear();
      v.field.clear();
      v.nonce += 1;
    }
  }
  let Some(id) = cur else { return };
  let Some(info) = app.run("sim.info", json!({"study": id})) else { return };
  ui.separator();
  egui::CollapsingHeader::new("Materials").default_open(true).show(ui, |ui| {
    let names: Vec<String> = fox_engine::simulation::Material::library().into_iter().map(|m| m.name).collect();
    for b in info["bodies"].as_array().into_iter().flatten() {
      let body = b["body"].as_u64().unwrap_or(0);
      let mut mat = b["material"].as_str().unwrap_or("").trim_end_matches(" (custom)").to_string();
      let before = mat.clone();
      egui::ComboBox::from_label(b["name"].as_str().unwrap_or("Body")).selected_text(&mat).show_ui(ui, |ui| {
        for n in &names {
          ui.selectable_value(&mut mat, n.clone(), n);
        }
      });
      if mat != before {
        app.run("sim.material", json!({"study": id, "bodies": [body], "material": mat}));
      }
    }
  });
  let mut remove: Option<Value> = None;
  for (key, head) in [("constraints", "Constraints"), ("loads", "Loads")] {
    let items: Vec<String> =
      info[key].as_array().into_iter().flatten().map(|x| x.as_str().unwrap_or("").to_string()).collect();
    egui::CollapsingHeader::new(format!("{head} ({})", items.len())).default_open(true).show(ui, |ui| {
      for (i, text) in items.iter().enumerate() {
        ui.horizontal(|ui| {
          ui.label(text);
          if ui.small_button("🗙").on_hover_text("Remove").clicked() {
            remove = Some(json!({"study": id, key.trim_end_matches('s'): i}));
          }
        });
      }
    });
  }
  if let Some(p) = remove {
    app.run("sim.remove", p);
  }
  if info["bodies"].as_array().is_some_and(|b| b.len() > 1) {
    egui::CollapsingHeader::new("Contacts").show(ui, |ui| {
      let auto = info["contacts"] == "automatic";
      let mut a = auto;
      ui.checkbox(&mut a, "Bond touching bodies (automatic)");
      if a != auto {
        app.run("sim.contact", json!({"study": id, "mode": if a { "automatic" } else { "manual" }}));
      }
      if !a {
        let sel = selected_bodies(app);
        if ui.add_enabled(sel.len() == 2, egui::Button::new("Bond the two selected bodies")).clicked() {
          app.run("sim.contact", json!({"study": id, "bond": sel}));
        }
        for pair in info["bonded"].as_array().into_iter().flatten() {
          ui.label(format!("Bonded: body {} – body {}", pair[0], pair[1]));
        }
      }
    });
  }
  egui::CollapsingHeader::new("Mesh").show(ui, |ui| {
    ui.horizontal(|ui| {
      ui.label("Size (mm)");
      ui.add(egui::TextEdit::singleline(&mut v.mesh_size).hint_text("auto").desired_width(60.0));
    });
    ui.horizontal(|ui| {
      ui.label("Local size (mm)");
      ui.add(egui::TextEdit::singleline(&mut v.refine_size).desired_width(60.0));
    });
    if let Some(r) = info["mesh"]["refinements"].as_array() {
      for (i, r) in r.iter().enumerate() {
        ui.horizontal(|ui| {
          ui.label(format!("Local: {} face(s) at {} mm", r["faces"], r["size"]));
          if ui.small_button("🗙").clicked() {
            app.run("sim.remove", json!({"study": id, "refinement": i}));
          }
        });
      }
    }
    ui.checkbox(&mut v.show_mesh, "Show the mesh");
  });
  if info["solved"] != true {
    return;
  }
  ui.separator();
  results_panel(app, v, ui, id);
}

fn results_panel(app: &mut App, v: &mut SimUi, ui: &mut egui::Ui, id: u32) {
  ui.label(egui::RichText::new("RESULTS").size(11.0).color(theme::GROUP_LABEL));
  let Some(r) = app.run(
    "sim.results",
    json!({"study": id, "field": if v.field.is_empty() { Value::Null } else { json!(v.field) }, "mode": v.mode}),
  ) else {
    v.field.clear();
    return;
  };
  let fields: Vec<String> =
    r["fields"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(str::to_string)).collect();
  let mut field = r["field"].as_str().unwrap_or("").to_string();
  let before = field.clone();
  egui::ComboBox::from_label("Result").selected_text(r["label"].as_str().unwrap_or(&field)).show_ui(ui, |ui| {
    for f in &fields {
      ui.selectable_value(&mut field, f.clone(), f.replace('_', " "));
    }
  });
  if field != before {
    v.field = field;
    v.nonce += 1;
  }
  if let Some(freqs) = r["results"]["frequencies"].as_array() {
    for (i, f) in freqs.iter().enumerate() {
      let text = format!("Mode {}: {:.2} Hz", i + 1, f.as_f64().unwrap_or(0.0));
      if ui.selectable_label(v.mode == i + 1, text).clicked() {
        v.mode = i + 1;
        v.nonce += 1;
      }
    }
    ui.checkbox(&mut v.animate, "Animate");
    if v.animate && v.started == 0.0 {
      v.started = now();
    }
  }
  if r["results"]["frequencies"].is_null() && r["results"]["heat_flow"].is_null()
    || r["results"]["frequencies"].is_array()
  {
    ui.horizontal(|ui| {
      ui.label("Deformation");
      for (d, label) in [(Deform::Adjusted, "Adjusted"), (Deform::Actual, "Actual"), (Deform::None, "None")] {
        if ui.selectable_label(v.deform == d, label).clicked() {
          v.deform = d;
        }
      }
    });
  }
  for (k, val) in &v.summary {
    ui.label(egui::RichText::new(format!("{k}: {val}")).size(11.0));
  }
  if !v.probes.is_empty() && ui.small_button("Clear probes").clicked() {
    v.probes.clear();
  }
}

/// The input form of a constraint or load: values, then OK applies it to the selected faces.
fn form_window(app: &mut App, mut f: Form, ctx: &egui::Context) -> Option<Form> {
  let mut open = true;
  let mut apply = false;
  let mut cancel = false;
  egui::Window::new(f.title.to_uppercase()).open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
    if !f.no_faces {
      let n = selected_faces(app).len();
      ui.label(if n == 0 { "Select faces in the viewport".to_string() } else { format!("{n} face(s) selected") });
    }
    for (_, label, text) in &mut f.fields {
      ui.horizontal(|ui| {
        ui.label(*label);
        ui.add(egui::TextEdit::singleline(text).desired_width(80.0));
      });
    }
    if matches!(f.kind, "force" | "bearing" | "moment" | "gravity") {
      egui::ComboBox::from_label("Direction").selected_text(&f.direction).show_ui(ui, |ui| {
        for d in ["normal", "x", "-x", "y", "-y", "z", "-z"] {
          if f.kind == "gravity" && d == "normal" {
            continue;
          }
          ui.selectable_value(&mut f.direction, d.to_string(), d);
        }
      });
    }
    for (_, label, on) in &mut f.flags {
      ui.checkbox(on, *label);
    }
    ui.horizontal(|ui| {
      apply = ui.button("OK").clicked();
      cancel = ui.button("Cancel").clicked();
    });
  });
  if apply {
    let mut p = json!({"type": f.kind, "study": current(app)});
    if !f.no_faces {
      let faces = selected_faces(app);
      if faces.is_empty() {
        app.status = Some(("Select the faces first".into(), false));
        return Some(f);
      }
      p["faces"] = json!(faces);
    }
    for (k, _, text) in &f.fields {
      match text.trim().parse::<f64>() {
        Ok(x) => p[*k] = json!(x),
        Err(_) => {
          app.status = Some((format!("'{text}' isn't a number"), true));
          return Some(f);
        }
      }
    }
    for (k, _, on) in &f.flags {
      p[*k] = json!(on);
    }
    match f.kind {
      "force" | "bearing" | "moment" => p["direction"] = json!(f.direction),
      "gravity" => {
        let g = p["magnitude"].as_f64().unwrap_or(9806.65);
        let d = f.direction.trim_start_matches('-');
        let sign = if f.direction.starts_with('-') { -1.0 } else { 1.0 };
        let mut a = [0.0; 3];
        a[match d {
          "x" => 0,
          "y" => 1,
          _ => 2,
        }] = sign * g;
        p["acceleration"] = json!(a);
      }
      _ => {}
    }
    if app.run(f.cmd, p).is_some() {
      app.s.selection.clear();
      return None;
    }
    return Some(f);
  }
  (open && !cancel).then_some(f)
}

/// The colour bar: the field's range with its unit, on the left of the viewport.
fn legend(app: &App, v: &SimUi, ctx: &egui::Context) {
  let Some(study) = current(app) else { return };
  if v.show_mesh {
    return;
  }
  let view =
    fox_engine::sim::SimView { study, field: v.field.clone(), mode: v.mode, scale: None, phase: None, mesh: false };
  let Some((lo, hi, label, unit)) = fox_engine::sim::view_range(&app.s, &view) else { return };
  let canvas = app.canvas;
  let top = canvas.min + vec2(16.0, 60.0);
  let (bar_w, bar_h) = (18.0, 220.0);
  egui::Area::new(egui::Id::new("sim-legend")).fixed_pos(top).order(egui::Order::Middle).interactable(false).show(
    ctx,
    |ui| {
      let (rect, _) = ui.allocate_exact_size(vec2(150.0, bar_h + 40.0), egui::Sense::hover());
      let p = ui.painter();
      p.rect_filled(rect.expand(4.0), 4.0, Color32::from_rgba_unmultiplied(255, 255, 255, 220));
      let unit_text = if unit.is_empty() { String::new() } else { format!(" ({unit})") };
      p.text(
        rect.min,
        egui::Align2::LEFT_TOP,
        format!("{label}{unit_text}"),
        egui::FontId::proportional(12.0),
        theme::TEXT,
      );
      let bar = egui::Rect::from_min_size(rect.min + vec2(0.0, 20.0), vec2(bar_w, bar_h));
      let steps = 40;
      for i in 0..steps {
        let t = 1.0 - i as f64 / steps as f64;
        let c = fox_engine::simulation::colormap(t);
        let y0 = bar.min.y + bar_h * i as f32 / steps as f32;
        let r =
          egui::Rect::from_min_max(egui::pos2(bar.min.x, y0), egui::pos2(bar.max.x, y0 + bar_h / steps as f32 + 0.5));
        p.rect_filled(r, 0.0, Color32::from_rgb(c[0], c[1], c[2]));
      }
      p.rect_stroke(bar, 0.0, egui::Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Inside);
      for k in 0..=4 {
        let t = k as f64 / 4.0;
        let val = hi - (hi - lo) * t;
        let y = bar.min.y + bar_h * t as f32;
        p.text(
          egui::pos2(bar.max.x + 6.0, y),
          egui::Align2::LEFT_CENTER,
          fmt(val),
          egui::FontId::proportional(11.0),
          theme::TEXT,
        );
      }
    },
  );
}

fn fmt(v: f64) -> String {
  let a = v.abs();
  if a != 0.0 && !(1e-3..1e5).contains(&a) { format!("{v:.3e}") } else { format!("{v:.4}") }
}

/// Probing: a click on the model reads the shown result there; probes stay as labels.
fn probes(app: &mut App, v: &mut SimUi, ctx: &egui::Context) {
  let Some(study) = current(app) else { return };
  if v.probing && app.last_click != v.seen_click {
    v.seen_click = app.last_click;
    if let Some(pos) = app.last_click
      && let Some(frame) = &app.frame
    {
      let ppp = ctx.pixels_per_point();
      let x = ((pos.x - app.canvas.min.x) * ppp) as f64;
      let y = ((pos.y - app.canvas.min.y) * ppp) as f64;
      let (o, d) = app.cam.ray(x, y, frame.width, frame.height);
      if let Some((t, _)) = fox_engine::scene::ray_hits(&app.s, o, d).first() {
        let p = (o + d * *t).to_array();
        let field = if v.field.is_empty() { Value::Null } else { json!(v.field) };
        if let Some(r) = app.run("sim.probe", json!({"study": study, "field": field, "mode": v.mode, "point": p})) {
          let val = r["probes"][0]["value"]
            .as_f64()
            .map(|x| format!("{} {}", fmt(x), r["unit"].as_str().unwrap_or("")))
            .unwrap_or("—".into());
          v.probes.push((p, val));
        }
      }
    }
  }
  if v.probes.is_empty() {
    return;
  }
  let Some(frame) = &app.frame else { return };
  let ppp = ctx.pixels_per_point();
  let (w, h) = (frame.width, frame.height);
  egui::Area::new(egui::Id::new("sim-probes"))
    .fixed_pos(app.canvas.min)
    .order(egui::Order::Middle)
    .interactable(false)
    .show(ctx, |ui| {
      let p = ui.painter();
      for (pt, text) in &v.probes {
        let Some((sx, sy, _)) = app.cam.project(fox_engine::kernel::glam::DVec3::from(*pt), w, h) else { continue };
        let at = app.canvas.min + vec2(sx as f32 / ppp, sy as f32 / ppp);
        p.circle_filled(at, 4.0, Color32::BLACK);
        let g = p.layout_no_wrap(text.clone(), egui::FontId::proportional(12.0), Color32::BLACK);
        let r = egui::Rect::from_min_size(at + vec2(8.0, -18.0), g.size() + vec2(8.0, 4.0));
        p.rect_filled(r, 3.0, Color32::from_rgba_unmultiplied(255, 255, 230, 235));
        p.galley(r.min + vec2(4.0, 2.0), g, Color32::BLACK);
      }
    });
}

#[cfg(test)]
mod tests {
  use egui::{Event, Pos2, RawInput, Rect, vec2};
  use fox_engine::Session;
  use serde_json::json;

  use crate::App;

  fn texts(ctx: &egui::Context, app: &mut App, time: f64, events: Vec<Event>) -> Vec<(Rect, String)> {
    fn walk(s: &egui::Shape, out: &mut Vec<(Rect, String)>) {
      match s {
        egui::Shape::Text(t) => out.push((t.visual_bounding_rect(), t.galley.text().to_string())),
        egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        _ => {}
      }
    }
    let input = RawInput {
      screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
      time: Some(time),
      events,
      ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| app.draw(ui));
    let mut all = Vec::new();
    for c in &out.shapes {
      walk(&c.shape, &mut all);
    }
    out.drop_without_applying_deltas();
    all
  }

  #[test]
  fn simulation_workspace_runs_a_study() {
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 100, "depth": 10, "height": 10, "at": [50, 0]}));
    crate::panels::switch_workspace(&mut app, "Simulation");
    let mut t = 0.0;
    let mut frame = |app: &mut App, events: Vec<Event>| {
      t += 1.0 / 60.0;
      texts(&ctx, app, t, events)
    };
    frame(&mut app, vec![]);
    let shown = frame(&mut app, vec![]);
    let at = |shown: &[(Rect, String)], s: &str| shown.iter().find(|(_, x)| x == s).map(|(r, _)| r.center());
    assert!(at(&shown, "SIMULATION").is_some());
    // the toolbar's Static Stress button makes a study
    let button = at(&shown, "Static Stress").expect("the study button is on the toolbar");
    frame(&mut app, vec![Event::PointerMoved(button)]);
    let press = |pressed| Event::PointerButton {
      pos: button,
      button: egui::PointerButton::Primary,
      pressed,
      modifiers: Default::default(),
    };
    frame(&mut app, vec![press(true)]);
    frame(&mut app, vec![press(false)]);
    assert_eq!(app.s.design.sim.len(), 1);
    app.run("sim.constraint", json!({"faces": [{"point": [0, 0, 5]}]}));
    app.run("sim.load", json!({"type": "force", "faces": [{"point": [100, 0, 5]}], "force": [0, 0, -100]}));
    app.run("sim.mesh", json!({"size": 4}));
    let mut v = app.simulation.take().unwrap();
    super::solve(&mut app, &mut v);
    app.simulation = Some(v);
    for _ in 0..30 {
      frame(&mut app, vec![]);
    }
    let shown = frame(&mut app, vec![]);
    let all: Vec<&str> = shown.iter().map(|(_, s)| s.as_str()).collect();
    assert!(all.contains(&"Von Mises stress (MPa)"), "{all:?}");
    assert!(all.contains(&"Constraints (1)") && all.contains(&"Loads (1)"), "{all:?}");
    assert!(super::view_json(&app).is_some());
    // back to Design leaves the workspace
    crate::panels::switch_workspace(&mut app, "Design");
    assert!(app.simulation.is_none());
  }
}
