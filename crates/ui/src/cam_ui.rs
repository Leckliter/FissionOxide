//! Manufacture workspace windows: the tool library and the stock simulation's report; New
//! Setup's canvas preview.

use fox_engine::kernel::glam::DVec3;
use serde_json::{Value, json};

use crate::App;

/// The tool library window: the hub's tools and one being edited.
#[derive(Debug, Clone, Default)]
pub struct ToolLibrary {
  pub tools: Vec<Value>,
  /// The tool being edited: its fields as text.
  pub edit: Option<ToolEdit>,
  pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolEdit {
  pub number: String,
  pub name: String,
  pub kind: String,
  pub diameter: String,
  pub corner_radius: String,
  pub angle: String,
  pub tip_diameter: String,
  pub flute_length: String,
  pub flutes: String,
  pub rpm: String,
  pub feed: String,
  pub plunge: String,
  pub stickout: String,
  /// The holder's steps, "Ø×length, …" from its face up (one number: a plain cylinder).
  pub holder: String,
}

const FIELDS: [(&str, &str); 11] = [
  ("diameter", "Diameter (mm)"),
  ("corner_radius", "Corner radius (mm)"),
  ("angle", "Tip angle (°)"),
  ("tip_diameter", "Tip diameter (mm)"),
  ("flute_length", "Flute length (mm)"),
  ("flutes", "Flutes"),
  ("rpm", "Spindle speed (rpm)"),
  ("feed", "Cutting feed (mm/min)"),
  ("plunge", "Plunge feed (mm/min)"),
  ("stickout", "Stickout from the holder (mm)"),
  ("number", "Tool number"),
];

const KINDS: [(&str, &str); 5] = [
  ("flat", "Flat end mill"),
  ("ball", "Ball end mill"),
  ("bull_nose", "Bull nose end mill"),
  ("drill", "Drill"),
  ("chamfer", "Chamfer mill"),
];

impl ToolEdit {
  fn from(t: &Value) -> ToolEdit {
    let n = |k: &str| t.get(k).and_then(Value::as_f64).map(|v| format!("{v}")).unwrap_or_default();
    let s = |k: &str| t.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    ToolEdit {
      number: n("number"),
      name: s("name"),
      kind: s("kind"),
      diameter: n("diameter"),
      corner_radius: n("corner_radius"),
      angle: n("angle"),
      tip_diameter: n("tip_diameter"),
      flute_length: n("flute_length"),
      flutes: n("flutes"),
      rpm: n("rpm"),
      feed: n("feed"),
      plunge: n("plunge"),
      stickout: n("stickout"),
      // a plain holder from older libraries reads as one step
      holder: match (s("holder_text"), t.get("holder_diameter").and_then(Value::as_f64)) {
        (h, Some(d)) if h.is_empty() && d > 0.0 => format!("{d}"),
        (h, _) => h,
      },
    }
  }

  fn field(&mut self, k: &str) -> &mut String {
    match k {
      "number" => &mut self.number,
      "diameter" => &mut self.diameter,
      "corner_radius" => &mut self.corner_radius,
      "angle" => &mut self.angle,
      "tip_diameter" => &mut self.tip_diameter,
      "flute_length" => &mut self.flute_length,
      "flutes" => &mut self.flutes,
      "rpm" => &mut self.rpm,
      "feed" => &mut self.feed,
      "stickout" => &mut self.stickout,
      _ => &mut self.plunge,
    }
  }

  /// The `cam.tool.add` parameters.
  pub fn params(&self) -> Value {
    let mut p = json!({"name": self.name, "kind": self.kind});
    let mut me = self.clone();
    for (k, _) in FIELDS {
      let v = me.field(k).trim().to_string();
      if !v.is_empty() {
        p[k] = v.parse::<f64>().map(|x| json!(x)).unwrap_or(json!(v));
      }
    }
    // the steps are the whole holder
    p["holder"] = json!(self.holder.trim());
    p["holder_diameter"] = json!(0);
    p
  }
}

impl App {
  /// Opens (or refreshes) the tool library window.
  pub fn open_tool_library(&mut self) {
    match self.s.execute("cam.tools", &json!({})) {
      Ok(v) => {
        let edit = self.tool_library.as_ref().and_then(|l| l.edit.clone());
        self.tool_library = Some(ToolLibrary { tools: v.as_array().cloned().unwrap_or_default(), edit, error: None });
      }
      Err(e) => self.status = Some((e.0, true)),
    }
  }

  /// Runs the last setup's toolpaths through the stock and reports what it found.
  pub fn cam_simulate(&mut self) {
    if self.s.design.cam.is_empty() {
      self.status = Some(("Create a setup first (SETUP ▸ New Setup)".into(), false));
      return;
    }
    if let Some(r) = self.run("cam.simulate", json!({})) {
      let warnings = r["warnings"].as_array().cloned().unwrap_or_default();
      let left = r["remaining_mm3"].as_f64().unwrap_or(0.0) / 1000.0;
      self.status = Some(match warnings.first() {
        None => (format!("Simulated: no gouges or collisions · {left:.1} cm³ of stock left over the part"), false),
        Some(w) => (
          format!(
            "{} warning(s): {}{}",
            warnings.len(),
            w["message"].as_str().unwrap_or(""),
            if warnings.len() > 1 { " …" } else { "" }
          ),
          true,
        ),
      });
    }
  }
}

/// The tool library window, while open.
pub fn tool_library_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut lib) = app.tool_library.take() else { return };
  let mut open = true;
  let mut reload = false;
  egui::Window::new("Tool Library").open(&mut open).resizable(true).default_width(520.0).show(ctx, |ui| {
    ui.label(egui::RichText::new("The hub's tools (tools.json); operations copy the tool they use.").weak());
    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
      egui::Grid::new("cam-tools").striped(true).num_columns(5).show(ui, |ui| {
        for h in ["#", "Tool", "Ø", "rpm", "Feed"] {
          ui.strong(h);
        }
        ui.end_row();
        for t in &lib.tools {
          let f = |k: &str| t.get(k).and_then(Value::as_f64).unwrap_or(0.0);
          let selected = lib.edit.as_ref().is_some_and(|e| e.number == format!("{}", f("number")));
          if ui.selectable_label(selected, format!("T{}", f("number"))).clicked() {
            lib.edit = Some(ToolEdit::from(t));
          }
          ui.label(t.get("description").and_then(Value::as_str).unwrap_or(""));
          ui.label(format!("{}", f("diameter")));
          ui.label(format!("{}", f("rpm")));
          ui.label(format!("{}", f("feed")));
          ui.end_row();
        }
      });
    });
    ui.horizontal(|ui| {
      if ui.button("New tool").clicked() {
        let next = lib.tools.iter().filter_map(|t| t["number"].as_u64()).max().unwrap_or(0) + 1;
        lib.edit = Some(ToolEdit::from(&json!({
          "number": next, "kind": "flat", "diameter": 6, "angle": 118, "rpm": 10000, "feed": 1000, "plunge": 300,
        })));
      }
    });
    if let Some(e) = lib.edit.as_mut() {
      ui.separator();
      egui::Grid::new("cam-tool-edit").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut e.name).desired_width(220.0));
        ui.end_row();
        ui.label("Type");
        let current = KINDS.iter().find(|k| k.0 == e.kind).map(|k| k.1).unwrap_or("Flat end mill");
        egui::ComboBox::from_id_salt("cam-tool-kind").selected_text(current).show_ui(ui, |ui| {
          for (id, label) in KINDS {
            ui.selectable_value(&mut e.kind, id.to_string(), label);
          }
        });
        ui.end_row();
        for (k, label) in FIELDS {
          let relevant = match k {
            "corner_radius" => e.kind == "bull_nose",
            "angle" => matches!(e.kind.as_str(), "drill" | "chamfer"),
            "tip_diameter" => e.kind == "chamfer",
            _ => true,
          };
          if relevant {
            ui.label(label);
            ui.add(egui::TextEdit::singleline(e.field(k)).desired_width(120.0));
            ui.end_row();
          }
        }
        ui.label("Holder (Ø×length, … from its face up)")
          .on_hover_text("Stepped cylinders checked for collisions, e.g. \"20x15, 32x20, 50\"; empty = no holder");
        ui.add(egui::TextEdit::singleline(&mut e.holder).desired_width(220.0).hint_text("20x15, 32x20, 50"));
        ui.end_row();
      });
      ui.horizontal(|ui| {
        if ui.button("Save tool").clicked() {
          match app.s.execute("cam.tool.add", &e.params()) {
            Ok(_) => {
              lib.error = None;
              reload = true;
            }
            Err(err) => lib.error = Some(err.0),
          }
        }
        if ui.button("Delete tool").clicked() {
          match app.s.execute("cam.tool.delete", &json!({"number": e.number.trim().parse::<f64>().unwrap_or(0.0)})) {
            Ok(_) => {
              lib.error = None;
              reload = true;
            }
            Err(err) => lib.error = Some(err.0),
          }
        }
      });
    }
    if let Some(err) = &lib.error {
      ui.colored_label(egui::Color32::from_rgb(220, 60, 60), err);
    }
  });
  if open {
    app.tool_library = Some(lib);
    if reload {
      app.open_tool_library();
    }
  }
}

/// The stock box's fill and edges while New Setup is open, Fusion's translucent grey-blue.
const STOCK_FILL: egui::Color32 = egui::Color32::from_rgba_premultiplied(52, 58, 66, 56);
const STOCK_EDGE: egui::Color32 = egui::Color32::from_rgba_premultiplied(96, 106, 122, 170);
/// The WCS triad's arrows: their length on screen (points).
const TRIAD_PX: f64 = 64.0;

/// The setup New Setup previews: its stock box (milling) and its work origin.
pub(crate) fn setup_preview_geometry(app: &App) -> Option<(Option<(DVec3, DVec3)>, DVec3)> {
  if !app.dialog.as_ref().is_some_and(|d| d.setup_previewed()) {
    return None;
  }
  let st = app.s.design.cam.last()?;
  let origin = fox_engine::model::cam::origin(st, &app.s.model).ok()?;
  let stock = if st.lathe.is_none() { fox_engine::model::cam::stock(st, &app.s.model).ok() } else { None };
  Some((stock, origin))
}

/// While New Setup is open, as Fusion previews it: the stock as a translucent box around the
/// part and the WCS triad (red X, green Y, blue Z arrows) at its origin, the stock box's corner.
pub(crate) fn setup_preview(app: &App, painter: &egui::Painter, project: &dyn Fn(DVec3) -> Option<egui::Pos2>) {
  let Some((stock, origin)) = setup_preview_geometry(app) else { return };
  if let Some((lo, hi)) = stock {
    let c = |x: bool, y: bool, z: bool| {
      DVec3::new(if x { hi.x } else { lo.x }, if y { hi.y } else { lo.y }, if z { hi.z } else { lo.z })
    };
    let faces = [
      [c(false, false, false), c(true, false, false), c(true, true, false), c(false, true, false)],
      [c(false, false, true), c(true, false, true), c(true, true, true), c(false, true, true)],
      [c(false, false, false), c(true, false, false), c(true, false, true), c(false, false, true)],
      [c(false, true, false), c(true, true, false), c(true, true, true), c(false, true, true)],
      [c(false, false, false), c(false, true, false), c(false, true, true), c(false, false, true)],
      [c(true, false, false), c(true, true, false), c(true, true, true), c(true, false, true)],
    ];
    for f in faces {
      if let Some(pts) = f.iter().map(|p| project(*p)).collect::<Option<Vec<_>>>() {
        painter.add(egui::Shape::convex_polygon(pts, STOCK_FILL, egui::Stroke::new(1.0, STOCK_EDGE)));
      }
    }
  }
  let (right, _, _) = app.cam.basis();
  let (Some(a), Some(b)) = (project(origin), project(origin + right)) else { return };
  let per_unit = f64::from((b - a).length());
  if per_unit < 1e-9 {
    return;
  }
  let reach = TRIAD_PX / per_unit;
  for (dir, col) in [
    (DVec3::X, fox_engine::scene::ORIGIN_X_COLOR),
    (DVec3::Y, fox_engine::scene::ORIGIN_Y_COLOR),
    (DVec3::Z, fox_engine::scene::ORIGIN_Z_COLOR),
  ] {
    let Some(tip) = project(origin + dir * reach) else { continue };
    let color = egui::Color32::from_rgb(col[0], col[1], col[2]);
    let d = (tip - a).normalized();
    if !d.is_finite() {
      continue;
    }
    painter.line_segment([a, tip - d * 8.0], egui::Stroke::new(2.0, color));
    let n = egui::vec2(-d.y, d.x);
    painter.add(egui::Shape::convex_polygon(
      vec![tip, tip - d * 11.0 + n * 4.5, tip - d * 11.0 - n * 4.5],
      color,
      egui::Stroke::NONE,
    ));
  }
}
