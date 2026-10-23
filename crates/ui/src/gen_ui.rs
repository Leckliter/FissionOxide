//! The Generative Design workspace: its GENERATIVE toolbar's commands and the study window
//! (design space, load cases, objectives, materials, manufacturing, precheck, generating on a
//! thread of its own with progress, and the outcomes explorer with insert and export).

use std::sync::Arc;
use std::sync::atomic::Ordering;

use egui::Color32;
use fox_engine::Sel;
use fox_engine::generative::{GenError, Job, Progress};
use serde_json::{Value, json};

use crate::App;

/// Solves running in the background.
pub struct Running {
  pub progress: Arc<Progress>,
  /// Jobs done so far, and how many there are.
  pub done: Arc<std::sync::atomic::AtomicUsize>,
  pub jobs: usize,
  handle: std::thread::JoinHandle<Vec<(Job, Result<fox_engine::generative::Solution, GenError>)>>,
}

/// The study window's state.
#[derive(Default)]
pub struct GenState {
  pub open: bool,
  /// The load case being edited.
  pub case: usize,
  pub force: [String; 3],
  pub pressure: String,
  pub fix: [bool; 3],
  pub new_case: String,
  pub precheck: Option<Result<Value, String>>,
  pub running: Option<Running>,
  /// The outcome picked in the explorer.
  pub outcome: Option<u64>,
}

impl App {
  /// A GENERATIVE toolbar command.
  pub(crate) fn gen_command(&mut self, what: &str) {
    self.gd.open = true;
    if self.gd.fix == [false; 3] {
      self.gd.fix = [true; 3];
    }
    let has_study = !self.s.design.generative.is_empty();
    if !has_study || what == "study" {
      if self.run("gen.study", json!({})).is_some() {
        self.status = Some(("New generative study: add preserve geometry, loads and constraints".into(), false));
      }
      if what == "study" {
        return;
      }
    }
    let bodies: Vec<u32> = self
      .s
      .selection
      .iter()
      .filter_map(|s| match s {
        Sel::Body { body } => Some(body.0),
        Sel::Face { body, .. } => Some(body.0),
        _ => None,
      })
      .fold(Vec::new(), |mut v, b| {
        if !v.contains(&b) {
          v.push(b);
        }
        v
      });
    match what {
      "preserve" | "obstacle" if !bodies.is_empty() => {
        if self.run(&format!("gen.{what}"), json!({"bodies": bodies})).is_some() {
          self.s.selection.clear();
        }
      }
      "start" if bodies.len() == 1 => {
        self.run("gen.starting_shape", json!({"body": bodies[0]}));
      }
      "preserve" | "obstacle" | "start" => self.hint("Select bodies first, then pick the command"),
      "load" => self.gen_load(),
      "constraint" => self.gen_constraint(),
      "precheck" => self.gd.precheck = Some(self.s.execute("gen.precheck", &json!({})).map_err(|e| e.0)),
      "generate" => self.gen_start(),
      _ => {}
    }
  }

  fn selected_faces(&self) -> Vec<Value> {
    self
      .s
      .selection
      .iter()
      .filter_map(|s| match s {
        Sel::Face { body, face } => Some(json!({"body": body.0, "face": face})),
        _ => None,
      })
      .collect()
  }

  fn case_param(&self) -> Value {
    json!(self.gd.case)
  }

  fn gen_load(&mut self) {
    let faces = self.selected_faces();
    if faces.is_empty() {
      self.hint("Select the faces the load acts on (on preserve geometry), set it in the window, then Add Load");
      return;
    }
    let mut p = json!({"faces": faces, "case": self.case_param()});
    let f: Vec<f64> = self.gd.force.iter().map(|s| s.trim().parse().unwrap_or(0.0)).collect();
    match self.gd.pressure.trim().parse::<f64>() {
      Ok(pr) if pr != 0.0 => p["pressure"] = json!(pr),
      _ if f.iter().any(|v| *v != 0.0) => p["force"] = json!(f),
      _ => {
        self.hint("Give the load a force (N) or a pressure (MPa) in the Generative Design window");
        return;
      }
    }
    if self.run("gen.load", p).is_some() {
      self.s.selection.clear();
    }
  }

  fn gen_constraint(&mut self) {
    let faces = self.selected_faces();
    if faces.is_empty() {
      self.hint("Select the faces to hold fixed (bolt holes, mounting faces), then Add Constraint");
      return;
    }
    let fix: String = ["x", "y", "z"].iter().zip(self.gd.fix).filter(|(_, f)| *f).map(|(a, _)| *a).collect();
    if fix.is_empty() {
      self.hint("Fix at least one axis");
      return;
    }
    if self.run("gen.constraint", json!({"faces": faces, "case": self.case_param(), "fix": fix})).is_some() {
      self.s.selection.clear();
    }
  }

  fn gen_start(&mut self) {
    if self.gd.running.is_some() {
      return;
    }
    let jobs = match fox_engine::generative::jobs(&self.s, None) {
      Ok(j) => j,
      Err(e) => {
        self.status = Some((e.0, true));
        return;
      }
    };
    let progress = Arc::new(Progress::default());
    let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (p, d) = (progress.clone(), done.clone());
    let n = jobs.len();
    let handle = std::thread::spawn(move || {
      let mut out = Vec::new();
      for job in jobs {
        p.iteration.store(0, Ordering::Relaxed);
        let r = fox_engine::generative::solve(&job.problem, &p);
        out.push((job, r));
        d.fetch_add(1, Ordering::Relaxed);
      }
      out
    });
    self.gd.running = Some(Running { progress, done, jobs: n, handle });
    self.status = Some((format!("Generating {n} outcome{}…", if n == 1 { "" } else { "s" }), false));
  }

  /// Picks up finished solves.
  pub(crate) fn gen_poll(&mut self) {
    if !self.gd.running.as_ref().is_some_and(|r| r.handle.is_finished()) {
      return;
    }
    let Some(r) = self.gd.running.take() else { return };
    let cancelled = r.progress.cancelled();
    let results = match r.handle.join() {
      Ok(v) => v,
      Err(_) => {
        self.status = Some(("The generative solve stopped with an internal error".into(), true));
        return;
      }
    };
    let mut ok = Vec::new();
    for (job, res) in results {
      match res {
        Ok(o) => ok.push((job, o)),
        Err(e) => {
          self.status = Some((e.0, true));
          return;
        }
      }
    }
    match fox_engine::generative::finish(&mut self.s, ok) {
      Ok(v) => {
        let n = v["outcomes"].as_array().map(Vec::len).unwrap_or(0);
        let msg = if cancelled { "Stopped: the outcomes so far are in the explorer" } else { "Outcomes ready" };
        self.status = Some((format!("{msg} ({n})"), false));
      }
      Err(e) => self.status = Some((e.0, true)),
    }
  }
}

fn warn(ui: &mut egui::Ui, s: &str) {
  ui.colored_label(Color32::from_rgb(220, 160, 40), s);
}

fn ids(v: &Value) -> String {
  let a: Vec<String> = v.as_array().into_iter().flatten().map(|b| b.to_string()).collect();
  if a.is_empty() { "none".into() } else { a.join(", ") }
}

/// The study window.
pub fn window(app: &mut App, ctx: &egui::Context) {
  app.gen_poll();
  if app.gd.running.is_some() {
    ctx.request_repaint_after(std::time::Duration::from_millis(150));
  }
  if !app.gd.open {
    return;
  }
  let mut open = true;
  let mut calls: Vec<(&'static str, Value)> = Vec::new();
  let mut action: Option<&'static str> = None;
  let study = fox_engine::generative::info(&app.s);
  egui::Window::new("Generative Design").open(&mut open).resizable(true).default_width(460.0).show(ctx, |ui| {
    let Some(st) = study else {
      ui.label("No generative study yet.");
      if ui.button("New Study").clicked() {
        action = Some("study");
      }
      return;
    };
    egui::ScrollArea::vertical().max_height(620.0).show(ui, |ui| {
      ui.horizontal(|ui| {
        ui.strong(st["name"].as_str().unwrap_or("Study"));
        let n = app.s.design.generative.len();
        if n > 1 {
          ui.label(egui::RichText::new(format!("(last of {n} studies)")).weak());
        }
        if ui.small_button("New Study").clicked() {
          action = Some("study");
        }
      });
      egui::CollapsingHeader::new("Design space").default_open(true).show(ui, |ui| {
        ui.label(format!("Preserve geometry: {}", ids(&st["preserve"])));
        ui.label(format!("Obstacle geometry: {}", ids(&st["obstacles"])));
        ui.label(format!(
          "Starting shape: {}",
          st["starting_shape"].as_u64().map(|b| b.to_string()).unwrap_or_else(|| "none".into())
        ));
        ui.horizontal(|ui| {
          if ui.button("Preserve selected").clicked() {
            action = Some("preserve");
          }
          if ui.button("Obstacle selected").clicked() {
            action = Some("obstacle");
          }
          if ui.button("Starting shape").clicked() {
            action = Some("start");
          }
        });
        ui.horizontal(|ui| {
          let mut res = st["resolution"].as_f64().unwrap_or(0.0);
          ui.label("Element size (mm, 0 = auto)");
          if ui.add(egui::DragValue::new(&mut res).range(0.0..=100.0).speed(0.1)).changed() {
            calls.push(("gen.design_space", json!({"resolution": res})));
          }
          let mut m = st["margin"].as_f64().unwrap_or(0.0);
          ui.label("Margin");
          if ui.add(egui::DragValue::new(&mut m).range(0.0..=1000.0).speed(0.5)).changed() {
            calls.push(("gen.design_space", json!({"margin": m})));
          }
        });
      });
      egui::CollapsingHeader::new("Load cases").default_open(true).show(ui, |ui| {
        let cases = st["cases"].as_array().cloned().unwrap_or_default();
        app.gd.case = app.gd.case.min(cases.len().saturating_sub(1));
        ui.horizontal(|ui| {
          for (i, c) in cases.iter().enumerate() {
            let name = c["name"].as_str().unwrap_or("Case");
            if ui.selectable_label(app.gd.case == i, name).clicked() {
              app.gd.case = i;
            }
          }
        });
        ui.horizontal(|ui| {
          ui.add(egui::TextEdit::singleline(&mut app.gd.new_case).hint_text("new case").desired_width(110.0));
          if ui.button("Add Case").clicked() && !app.gd.new_case.trim().is_empty() {
            calls.push(("gen.case", json!({"case": app.gd.new_case.trim()})));
            app.gd.case = cases.len();
            app.gd.new_case.clear();
          }
          if cases.len() > 1 && ui.button("Delete Case").clicked() {
            calls.push(("gen.case", json!({"case": app.gd.case, "delete": true})));
            app.gd.case = 0;
          }
        });
        if let Some(c) = cases.get(app.gd.case) {
          for l in c["loads"].as_array().into_iter().flatten() {
            let what = match (l.get("force"), l.get("pressure")) {
              (Some(f), _) => format!("Force {f} N"),
              (_, Some(p)) => format!("Pressure {p} MPa"),
              _ => "Load".into(),
            };
            ui.label(format!("• {what} on body {}", l["body"].as_u64().map(|b| b.to_string()).unwrap_or("-".into())));
          }
          for k in c["constraints"].as_array().into_iter().flatten() {
            ui.label(format!(
              "• Fixed {} on body {}",
              k["fix"].as_str().unwrap_or(""),
              k["body"].as_u64().map(|b| b.to_string()).unwrap_or("-".into())
            ));
          }
          if c["loads"].as_array().is_none_or(Vec::is_empty) {
            warn(ui, "No loads in this case");
          }
        }
        ui.separator();
        ui.horizontal(|ui| {
          ui.label("Force (N)");
          for (f, h) in app.gd.force.iter_mut().zip(["x", "y", "z"]) {
            ui.add(egui::TextEdit::singleline(f).hint_text(h).desired_width(50.0));
          }
          ui.label("or pressure (MPa)");
          ui.add(egui::TextEdit::singleline(&mut app.gd.pressure).desired_width(50.0));
        });
        ui.horizontal(|ui| {
          if ui.button("Add Load on selected faces").clicked() {
            action = Some("load");
          }
        });
        ui.horizontal(|ui| {
          ui.label("Fixed");
          for (f, a) in app.gd.fix.iter_mut().zip(["X", "Y", "Z"]) {
            ui.checkbox(f, a);
          }
          if ui.button("Add Constraint on selected faces").clicked() {
            action = Some("constraint");
          }
        });
      });
      egui::CollapsingHeader::new("Objectives and limits").default_open(true).show(ui, |ui| {
        let stiff = st["objective"] == "max_stiffness";
        ui.horizontal(|ui| {
          if ui.radio(stiff, "Maximise stiffness").clicked() {
            calls.push(("gen.objective", json!({"objective": "max_stiffness"})));
          }
          if ui.radio(!stiff, "Minimise mass").clicked() {
            calls.push(("gen.objective", json!({"objective": "min_mass"})));
          }
        });
        let mut target = st["mass_target"].as_f64().unwrap_or(0.3);
        let label = if stiff { "Mass target (fraction of the design space)" } else { "Starting fraction" };
        if ui.add(egui::Slider::new(&mut target, 0.05..=0.9).text(label)).drag_stopped() {
          calls.push(("gen.objective", json!({"mass_target": target})));
        }
        ui.horizontal(|ui| {
          let mut sf = st["safety_factor"].as_f64().unwrap_or(2.0);
          ui.label("Safety factor");
          if ui.add(egui::DragValue::new(&mut sf).range(0.0..=20.0).speed(0.05)).changed() {
            calls.push(("gen.objective", json!({"safety_factor": sf})));
          }
          let mut d = st["max_displacement"].as_f64().unwrap_or(0.0);
          ui.label("Max displacement (mm, 0 = none)");
          if ui.add(egui::DragValue::new(&mut d).range(0.0..=1000.0).speed(0.01)).changed() {
            calls.push(("gen.objective", json!({"max_displacement": if d > 0.0 { json!(d) } else { Value::Null }})));
          }
        });
      });
      egui::CollapsingHeader::new("Materials").show(ui, |ui| {
        let chosen: Vec<String> =
          st["materials"].as_array().into_iter().flatten().filter_map(|m| m.as_str().map(str::to_string)).collect();
        for m in fox_engine::generative::materials() {
          let mut on = chosen.contains(&m.name);
          if ui
            .checkbox(&mut on, &m.name)
            .on_hover_text(format!("E {:.0} MPa, {:.2} g/cm³, yield {:.0} MPa", m.youngs, m.density, m.yield_strength))
            .changed()
          {
            let mut next: Vec<String> = chosen.iter().filter(|c| **c != m.name).cloned().collect();
            if on {
              next.push(m.name.clone());
            }
            if !next.is_empty() {
              calls.push(("gen.materials", json!({"materials": next})));
            }
          }
        }
      });
      egui::CollapsingHeader::new("Manufacturing").show(ui, |ui| {
        let labels: Vec<String> =
          st["methods"].as_array().into_iter().flatten().filter_map(|m| m.as_str().map(str::to_string)).collect();
        let has = |p: &str| labels.iter().any(|l| l.starts_with(p));
        let mut methods: Vec<Value> = Vec::new();
        let mut changed = false;
        for (prefix, kind, label) in [
          ("Unrestricted", "unrestricted", "Unrestricted (additive)"),
          ("3-axis", "milling_3axis", "3-axis milling from +Z"),
          ("2.5-axis", "milling_2_5axis", "2.5-axis milling along Z"),
        ] {
          let mut on = has(prefix);
          changed |= ui.checkbox(&mut on, label).changed();
          if on {
            methods.push(json!(kind));
          }
        }
        if changed && !methods.is_empty() {
          calls.push(("gen.manufacturing", json!({"methods": methods})));
        }
        let sym: Vec<String> = st["symmetry"]
          .as_array()
          .into_iter()
          .flatten()
          .filter_map(|s| s["plane"].as_str().map(str::to_string))
          .collect();
        ui.horizontal(|ui| {
          ui.label("Symmetry planes (through the design space's middle)");
          let mut planes = sym.clone();
          let mut changed = false;
          for pl in ["yz", "xz", "xy"] {
            let mut on = sym.iter().any(|s| s == pl);
            if ui.checkbox(&mut on, pl.to_uppercase()).changed() {
              changed = true;
              planes.retain(|p| p != pl);
              if on {
                planes.push(pl.to_string());
              }
            }
          }
          if changed {
            // the middle of the preserve geometry's bounds
            let mid = app.gen_mid();
            let v: Vec<Value> = planes
              .iter()
              .map(|p| json!({"plane": p, "offset": mid[["yz", "xz", "xy"].iter().position(|x| x == p).unwrap_or(0)]}))
              .collect();
            calls.push(("gen.manufacturing", json!({"symmetry": v})));
          }
        });
        ui.horizontal(|ui| {
          let mut m = st["min_member"].as_f64().unwrap_or(0.0);
          ui.label("Minimum member size (mm, 0 = auto)");
          if ui.add(egui::DragValue::new(&mut m).range(0.0..=500.0).speed(0.1)).changed() {
            calls.push(("gen.manufacturing", json!({"min_member": m})));
          }
        });
      });
      ui.separator();
      ui.horizontal(|ui| {
        if ui.button("Pre-check").clicked() {
          action = Some("precheck");
        }
        let running = app.gd.running.is_some();
        if !running && ui.button("Generate").clicked() {
          action = Some("generate");
        }
        if running
          && ui.button("Stop").clicked()
          && let Some(r) = &app.gd.running
        {
          r.progress.cancel.store(true, Ordering::Relaxed);
        }
      });
      if let Some(r) = &app.gd.running {
        let it = r.progress.iteration.load(Ordering::Relaxed) as f32;
        let total = r.progress.total.load(Ordering::Relaxed).max(1) as f32;
        let done = r.done.load(Ordering::Relaxed);
        let frac = (done as f32 + (it / total).min(1.0)) / r.jobs.max(1) as f32;
        ui.add(egui::ProgressBar::new(frac).text(format!("outcome {} of {}, iteration {}", done + 1, r.jobs, it)));
      }
      match &app.gd.precheck {
        Some(Ok(pc)) => {
          ui.label(format!(
            "Grid {} ({:.2} mm elements): {} to optimise, {} preserved, {} outcomes to generate",
            pc["grid"],
            pc["element_size"].as_f64().unwrap_or(0.0),
            pc["design"],
            pc["preserve"],
            pc["outcomes_to_generate"]
          ));
          for w in pc["warnings"].as_array().into_iter().flatten() {
            warn(ui, w.as_str().unwrap_or(""));
          }
          if pc["ready"] == true {
            ui.colored_label(Color32::from_rgb(60, 160, 80), "Ready to generate");
          }
        }
        Some(Err(e)) => warn(ui, e),
        None => {}
      }
      explorer(app, ui, &mut calls);
    });
  });
  if !open {
    app.gd.open = false;
  }
  if let Some(a) = action {
    app.gen_command(a);
  }
  for (cmd, p) in calls {
    match cmd {
      // file dialogs and the like
      "export" => {
        if let Some(path) = rfd::FileDialog::new().add_filter("STL", &["stl"]).set_file_name("outcome.stl").save_file()
        {
          let mut p = p;
          p["path"] = json!(path);
          if app.run("gen.export", p).is_some() {
            app.status = Some((format!("Wrote {}", path.display()), false));
          }
        }
      }
      _ => match (app.run(cmd, p), cmd) {
        (Some(v), "gen.insert") => {
          app.status = Some((format!("Inserted body {} ({})", v["body"], v["name"].as_str().unwrap_or("")), false));
        }
        (Some(v), "gen.verify") => {
          let f = |k: &str| v[k].as_f64().unwrap_or(0.0);
          app.status = Some((
            format!(
              "Finite elements: safety factor {:.2}, max stress {:.1} MPa, max displacement {:.4} mm",
              f("safety_factor"),
              f("max_stress"),
              f("max_displacement")
            ),
            false,
          ));
        }
        _ => {}
      },
    }
  }
}

impl App {
  /// The middle of the study's preserve geometry (for symmetry planes).
  fn gen_mid(&self) -> [f64; 3] {
    let Some(st) = self.s.design.generative.last() else { return [0.0; 3] };
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for b in st.preserve.iter().chain(st.starting_shape.iter()) {
      if let Some(m) = self.s.model.body(*b).and_then(|b| b.mesh()) {
        for p in &m.positions {
          for a in 0..3 {
            lo[a] = lo[a].min(p[a] as f64);
            hi[a] = hi[a].max(p[a] as f64);
          }
        }
      }
    }
    if lo[0] > hi[0] {
      return [0.0; 3];
    }
    [0, 1, 2].map(|a| 0.5 * (lo[a] + hi[a]))
  }
}

/// The outcomes explorer: a table of the outcomes, the picked one's insert and export.
fn explorer(app: &mut App, ui: &mut egui::Ui, calls: &mut Vec<(&'static str, Value)>) {
  let outcomes = fox_engine::generative::outcomes(&app.s);
  if outcomes.is_empty() {
    return;
  }
  ui.separator();
  ui.strong("Outcomes");
  egui::Grid::new("gen-outcomes").striped(true).num_columns(9).spacing([10.0, 3.0]).show(ui, |ui| {
    for h in ["#", "Material", "Method", "Mass (g)", "Volume", "Max disp (mm)", "Safety factor", "Limits", "FEA"] {
      ui.strong(h);
    }
    ui.end_row();
    for o in &outcomes {
      let id = o["outcome"].as_u64().unwrap_or(0);
      if ui.selectable_label(app.gd.outcome == Some(id), id.to_string()).clicked() {
        app.gd.outcome = Some(id);
      }
      ui.label(o["material"].as_str().unwrap_or(""));
      ui.label(o["method"].as_str().unwrap_or(""));
      let f = |k: &str| o[k].as_f64().unwrap_or(0.0);
      ui.label(format!("{:.1}", f("mass")));
      ui.label(format!("{:.0}%", 100.0 * f("volume_fraction")));
      ui.label(format!("{:.4}", f("max_displacement")));
      ui.label(format!("{:.2}", f("safety_factor")));
      if o["meets_limits"] == true {
        ui.colored_label(Color32::from_rgb(60, 160, 80), "✔");
      } else {
        ui.colored_label(Color32::from_rgb(220, 80, 60), "✖");
      }
      match o["verified"].as_object() {
        Some(v) => {
          let sf = v.get("safety_factor").and_then(Value::as_f64).unwrap_or(0.0);
          let ok = v.get("meets_limits").and_then(Value::as_bool).unwrap_or(false);
          let c = if ok { Color32::from_rgb(60, 160, 80) } else { Color32::from_rgb(220, 80, 60) };
          ui.colored_label(c, format!("SF {sf:.2}")).on_hover_text(format!(
            "Finite element check of body {}: {} elements, max stress {:.1} MPa, max displacement {:.4} mm",
            v.get("body").and_then(Value::as_u64).unwrap_or(0),
            v.get("elements").and_then(Value::as_u64).unwrap_or(0),
            v.get("max_stress").and_then(Value::as_f64).unwrap_or(0.0),
            v.get("max_displacement").and_then(Value::as_f64).unwrap_or(0.0),
          ));
        }
        None => {
          ui.label("–");
        }
      }
      ui.end_row();
    }
  });
  let picked = app.gd.outcome.filter(|id| outcomes.iter().any(|o| o["outcome"].as_u64() == Some(*id)));
  ui.horizontal(|ui| {
    let Some(id) = picked else {
      ui.label(egui::RichText::new("Pick an outcome to insert it").weak());
      return;
    };
    if ui.button("Insert as Mesh").clicked() {
      calls.push(("gen.insert", json!({"outcome": id})));
    }
    if ui
      .button("Insert as B-Rep")
      .on_hover_text("Converted to B-rep, joined with the exact preserve geometry, obstacles cut")
      .clicked()
    {
      calls.push(("gen.insert", json!({"outcome": id, "reduce": 0.5, "brep": true, "merge_preserve": true})));
    }
    let inserted = outcomes.iter().any(|o| o["outcome"].as_u64() == Some(id) && o["body"].is_u64());
    if ui
      .add_enabled(inserted, egui::Button::new("Verify (FEA)"))
      .on_hover_text("Check the inserted body with finite elements under the study's loads (insert it first)")
      .clicked()
    {
      calls.push(("gen.verify", json!({"outcome": id})));
    }
    if ui.button("Export STL…").clicked() {
      calls.push(("export", json!({"outcome": id})));
    }
  });
}

#[cfg(test)]
mod tests {
  use fox_engine::{Sel, Session};
  use serde_json::json;

  use crate::App;
  use crate::panels::{Tab, switch_workspace};

  #[test]
  fn generative_workspace_from_selection_to_outcome() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 40, "depth": 10, "height": 10, "at": [20, 5]}));
    let body = app.s.model.bodies[0].id;
    switch_workspace(&mut app, "Generative Design");
    assert!(app.generative && !app.manufacture && app.gd.open);
    assert!(crate::menus::commands(&[Tab::Generative]).iter().any(|e| e.label == "Generate"));
    // the selected body becomes preserve geometry (a study is made on the way)
    app.s.selection = vec![Sel::Body { body }];
    app.gen_command("preserve");
    assert_eq!(app.s.design.generative[0].preserve, vec![body]);
    let faces = app.s.model.body(body).unwrap().faces().to_vec();
    let end = |x: f64| faces.iter().find(|f| (f.center.x - x).abs() < 1e-6).unwrap().id;
    app.s.selection = vec![Sel::Face { body, face: end(0.0) }];
    app.gen_command("constraint");
    app.gd.force = ["0".into(), "0".into(), "-100".into()];
    app.s.selection = vec![Sel::Face { body, face: end(40.0) }];
    app.gen_command("load");
    let st = &app.s.design.generative[0];
    assert_eq!((st.cases[0].loads.len(), st.cases[0].constraints.len()), (1, 1));
    app.run("gen.design_space", json!({"resolution": 2.5, "iterations": 10, "margin": 5}));
    app.gen_command("generate");
    assert!(app.gd.running.is_some());
    while app.gd.running.is_some() {
      std::thread::sleep(std::time::Duration::from_millis(20));
      app.gen_poll();
    }
    assert_eq!(app.s.design.generative[0].outcomes.len(), 1, "{:?}", app.status);
    switch_workspace(&mut app, "Design");
    assert!(!app.generative);
  }
}
