//! Configurations and base features in the UI: the configuration table window and the direct
//! edit dialogs (see [`crate::forms`] and `docs/CONFIGURATIONS.md`).

use std::sync::atomic::{AtomicBool, Ordering};

use egui::RichText;
use serde_json::{Value, json};

use crate::forms::{Form, Input};
use crate::icons::Icon;
use crate::{App, theme};

pub static DIRECT_MOVE_FACE: Form = Form {
  title: "MOVE FACE",
  name: "Move Face (direct)",
  hint: "Move or turn faces, flat or curved (a hole, a boss), without history: the edit goes into a base \
         feature",
  icon: Icon::Move,
  cmd: "direct.move_face",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "faces", label: "Faces" },
    Input::Vec3 { key: "translate", label: "Move", default: ["0", "0", "1"] },
    Input::Expr { key: "angle", label: "Turn", default: "0 deg" },
    Input::Vec3 { key: "axis", label: "Turn axis", default: ["0", "0", "1"] },
    Input::Vec3 { key: "origin", label: "Axis through", default: ["0", "0", "0"] },
  ],
  picks_model: true,
};

pub static DIRECT_PRESS_PULL: Form = Form {
  title: "PRESS PULL",
  name: "Press Pull (direct)",
  hint: "Push or pull faces by a fixed distance, without history: the edit goes into a base feature",
  icon: Icon::PressPull,
  cmd: "direct.press_pull",
  fixed: &[],
  inputs: &[
    Input::Faces { key: "faces", label: "Faces" },
    Input::Expr { key: "distance", label: "Distance", default: "1 mm" },
  ],
  picks_model: true,
};

pub static DIRECT_DELETE_FACE: Form = Form {
  title: "DELETE FACE",
  name: "Delete Face (direct)",
  hint: "Delete faces and heal the body, without history: the edit goes into a base feature",
  icon: Icon::RemoveBody,
  cmd: "direct.delete_face",
  fixed: &[],
  inputs: &[Input::Faces { key: "faces", label: "Faces" }],
  picks_model: true,
};

/// Create Base Feature is a mode, as in Fusion: no dialog. A base feature joins the timeline and
/// the ribbon shows BASE FEATURE SOLID / BASE FEATURE SURFACE with a green FINISH BASE FEATURE
/// button (`start` false), which closes it and brings the SOLID tab back.
pub fn base_mode(app: &mut App, start: bool) {
  app.cancel_dialog();
  if start {
    if app.base_mode {
      return;
    }
    if app.s.edit_sketch.is_some() {
      app.finish_sketch();
    }
    if app.run("base.start", json!({})).is_some() {
      app.base_mode = true;
      app.tab = crate::panels::Tab::Solid;
    }
  } else {
    if app.run("base.finish", json!({})).is_none() {
      return;
    }
    app.base_mode = false;
    app.tab = crate::panels::Tab::Solid;
  }
}

/// Leaves the base feature mode when its base feature is gone (undone or deleted).
pub fn check_base_mode(app: &mut App) {
  if app.base_mode
    && !app.s.design.features.iter().any(|f| matches!(f.kind, fox_engine::model::FeatureKind::BaseFeature { .. }))
  {
    app.base_mode = false;
  }
}

/// Whether the configuration table window is showing.
static TABLE_OPEN: AtomicBool = AtomicBool::new(false);

pub fn toggle_table() {
  TABLE_OPEN.fetch_xor(true, Ordering::Relaxed);
}

pub fn open_table() {
  TABLE_OPEN.store(true, Ordering::Relaxed);
}

pub fn table_open() -> bool {
  TABLE_OPEN.load(Ordering::Relaxed)
}

/// Turns "capture design history" on or off (off collapses the timeline; Undo brings it back).
pub fn toggle_history(app: &mut App) {
  let capture = app.s.design.history_off;
  if app.run("design.history", json!({"capture": capture})).is_some() {
    let msg = if capture {
      "Capturing design history: new features go in the timeline after the base feature"
    } else {
      "Design history off: the timeline is now a base feature (Undo restores it)"
    };
    app.status = Some((msg.into(), false));
  }
}

/// The keys naming a table column in `config.set` / `config.rule`.
fn column_params(column: &Value) -> serde_json::Map<String, Value> {
  let mut p = serde_json::Map::new();
  match column["type"].as_str().unwrap_or("") {
    "parameter" => {
      p.insert("parameter".into(), column["name"].clone());
    }
    "suppress" => {
      p.insert("feature".into(), column["feature"].clone());
    }
    "appearance" => {
      p.insert("body".into(), column["body"].clone());
    }
    _ => {
      p.insert("joint".into(), column["joint"].clone());
      p.insert("motion".into(), column["motion"].clone());
    }
  }
  p
}

/// A colour as typed in a rule: an appearance name, `#rrggbb`, or empty/"none" for none.
fn typed_color(text: &str) -> Value {
  match text.trim() {
    "" | "none" => Value::Null,
    t => json!(t),
  }
}

/// `config.rule` parameters for a column's rule (`expr` empty: remove the rule).
fn rule_params(column: &Value, expr: &str, color: &str, otherwise: &str) -> Value {
  let mut p = column_params(column);
  let expr = expr.trim();
  p.insert("expr".into(), if expr.is_empty() { Value::Null } else { json!(expr) });
  if column["type"] == "appearance" && !expr.is_empty() {
    p.insert("color".into(), typed_color(color));
    p.insert("otherwise".into(), typed_color(otherwise));
  }
  Value::Object(p)
}

/// A rule colour as text for its field.
fn color_text(c: &Value) -> String {
  match c.as_array().map(|a| a.iter().map(|x| x.as_u64().unwrap_or(0)).collect::<Vec<_>>()) {
    Some(rgb) if rgb.len() == 3 => format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]),
    _ => String::new(),
  }
}

/// The value a typed cell sets: `None` clears it (the design's own value).
fn cell_params(column: &Value, text: &str) -> Value {
  let text = text.trim();
  let mut p = serde_json::Map::new();
  match column["type"].as_str().unwrap_or("") {
    "parameter" => {
      p.insert("parameter".into(), column["name"].clone());
      p.insert("expr".into(), if text.is_empty() { Value::Null } else { json!(text) });
    }
    "suppress" => {
      p.insert("feature".into(), column["feature"].clone());
      let v = match text {
        "" => Value::Null,
        t => json!(t.starts_with('s') || t == "true" || t == "yes"),
      };
      p.insert("suppressed".into(), v);
    }
    "appearance" => {
      p.insert("body".into(), column["body"].clone());
      match text {
        "" => {}
        "none" => {
          p.insert("color".into(), Value::Null);
        }
        t if t.starts_with('#') => {
          p.insert("color".into(), json!(t));
        }
        t => {
          p.insert("appearance".into(), json!(t));
        }
      }
    }
    _ => {
      p.insert("joint".into(), column["joint"].clone());
      p.insert("motion".into(), column["motion"].clone());
      p.insert("value".into(), if text.is_empty() { Value::Null } else { json!(text) });
    }
  }
  Value::Object(p)
}

/// The configuration table: a row per configuration, a column per override; type in a cell to
/// override the design's value there, clear it to go back to the default.
pub fn table_window(app: &mut App, ctx: &egui::Context) {
  if !table_open() {
    return;
  }
  let mut open = true;
  egui::Window::new("Configuration Table").open(&mut open).default_width(560.0).show(ctx, |ui| {
    ui.label(
      RichText::new("Each row is a variant of the design. Empty cells keep the design's own value.")
        .small()
        .color(theme::MUTED),
    );
    ui.add_space(4.0);
    let t = fox_engine::config::table(&app.s.design);
    let cols = t["columns"].as_array().cloned().unwrap_or_default();
    let rows = t["rows"].as_array().cloned().unwrap_or_default();
    egui::ScrollArea::horizontal().show(ui, |ui| {
      egui::Grid::new("config-table").striped(true).num_columns(cols.len() + 3).show(ui, |ui| {
        ui.label("");
        ui.label(RichText::new("Configuration").strong());
        for c in &cols {
          let label = c["label"].as_str().unwrap_or("");
          if c["rule"].is_null() {
            ui.label(RichText::new(label).strong());
          } else {
            let rule = c["rule"]["expr"].as_str().unwrap_or("");
            ui.label(RichText::new(format!("ƒ {label}")).strong().color(theme::ACCENT))
              .on_hover_text(format!("Driven by a rule: {rule}"));
          }
        }
        ui.label("");
        ui.end_row();
        // the design as modelled
        let none_active = t["active"].is_null();
        if ui.radio(none_active, "").on_hover_text("Show the design as modelled").clicked() && !none_active {
          app.run_background("config.activate", json!({}));
        }
        ui.label(RichText::new("(Default)").italics());
        for c in &cols {
          ui.label(RichText::new(c["default"].as_str().unwrap_or("")).color(theme::MUTED));
        }
        ui.label("");
        ui.end_row();
        for r in &rows {
          let name = r["name"].as_str().unwrap_or("").to_string();
          let active = r["active"].as_bool().unwrap_or(false);
          if ui.radio(active, "").on_hover_text("Make this configuration active").clicked() && !active {
            app.run_background("config.activate", json!({"name": name}));
          }
          edit_text(ui, ("config-name", &name), &name, 110.0, |text| {
            if !text.trim().is_empty() && text != name {
              app.run("config.rename", json!({"name": name, "new_name": text}));
            }
          });
          for (i, c) in cols.iter().enumerate() {
            let cur = r["cells"][i].as_str().unwrap_or("").to_string();
            if !c["rule"].is_null() {
              // computed from the row: shown, not typed
              let failed = r["errors"].as_array().and_then(|e| e.iter().find(|e| e["column"] == c["column"]));
              match failed {
                Some(e) => {
                  ui.label(RichText::new("error").color(theme::ERROR)).on_hover_text(e["error"].as_str().unwrap_or(""));
                }
                None => {
                  ui.label(RichText::new(if cur.is_empty() { "default" } else { &cur }).italics());
                }
              }
              continue;
            }
            edit_text(ui, ("config-cell", &name, i), &cur, 90.0, |text| {
              if text.trim() != cur {
                let mut p = cell_params(&c["column"], &text);
                p["configuration"] = json!(name);
                app.run("config.set", p);
              }
            });
          }
          if ui.small_button("🗑").on_hover_text("Delete this configuration").clicked() {
            app.run("config.delete", json!({"name": name}));
          }
          ui.end_row();
        }
      });
    });
    ui.separator();
    add_row(app, ui);
    add_column(app, ui, rows.first().and_then(|r| r["name"].as_str()).map(str::to_string));
    ui.separator();
    rules(app, ui, &cols);
  });
  if !open {
    TABLE_OPEN.store(false, Ordering::Relaxed);
  }
}

/// The rules section: a driven column's cell in every row comes from its rule, evaluated with
/// that row's parameters (`if(length > 100 mm, 8 mm, 5 mm)`, `length < 50 mm`).
fn rules(app: &mut App, ui: &mut egui::Ui, cols: &[Value]) {
  ui.label(RichText::new("Rules").strong()).on_hover_text(
    "A rule drives a column from each row's parameters: a formula for a parameter or joint, a condition \
     for suppression (true = suppressed), a condition choosing between two colours for an appearance",
  );
  for c in cols.iter().filter(|c| !c["rule"].is_null()) {
    let rule = &c["rule"];
    let label = c["label"].as_str().unwrap_or("").to_string();
    let expr = rule["expr"].as_str().unwrap_or("").to_string();
    let appearance = c["column"]["type"] == "appearance";
    let (color, otherwise) = (color_text(&rule["color"]), color_text(&rule["otherwise"]));
    let mut send = None;
    ui.horizontal(|ui| {
      ui.label(format!("{label} ="));
      edit_text(ui, ("config-rule", &label), &expr, 220.0, |text| {
        if text.trim() != expr && !text.trim().is_empty() {
          send = Some(rule_params(&c["column"], &text, &color, &otherwise));
        }
      });
      if appearance {
        ui.label("then");
        edit_text(ui, ("config-rule-color", &label), &color, 90.0, |text| {
          if text.trim() != color {
            send = Some(rule_params(&c["column"], &expr, &text, &otherwise));
          }
        });
        ui.label("else");
        edit_text(ui, ("config-rule-else", &label), &otherwise, 90.0, |text| {
          if text.trim() != otherwise {
            send = Some(rule_params(&c["column"], &expr, &color, &text));
          }
        });
      }
      if ui.small_button("🗑").on_hover_text("Remove the rule: the column takes typed values again").clicked() {
        send = Some(rule_params(&c["column"], "", "", ""));
      }
    });
    if let Some(p) = send {
      app.run("config.rule", p);
    }
  }
  // a new rule on any column the table could have
  let choices = column_choices(app);
  let id = egui::Id::new("config-new-rule");
  let (mut pick, mut expr) = ui.data_mut(|d| d.get_temp::<(usize, String)>(id)).unwrap_or_default();
  pick = pick.min(choices.len().saturating_sub(1));
  let mut add = false;
  ui.horizontal(|ui| {
    ui.label("New rule:");
    egui::ComboBox::from_id_salt("config-rule-pick")
      .selected_text(choices.get(pick).map(|c| c.0.as_str()).unwrap_or("nothing to drive"))
      .width(180.0)
      .show_ui(ui, |ui| {
        for (i, (label, _)) in choices.iter().enumerate() {
          ui.selectable_value(&mut pick, i, label);
        }
      });
    ui.add(egui::TextEdit::singleline(&mut expr).hint_text("if(length > 100 mm, 8 mm, 5 mm)").desired_width(200.0));
    add = ui.button("+ Add").clicked() && !expr.trim().is_empty();
  });
  if add && let Some((_, col)) = choices.get(pick) {
    // an appearance rule starts red when true, no colour otherwise (edit them above)
    let p = rule_params(col, &expr, "#ff0000", "");
    if app.run("config.rule", p).is_some() {
      expr.clear();
    }
  }
  ui.data_mut(|d| d.insert_temp(id, (pick, expr)));
}

/// Every column a rule could drive, as `(label, column JSON)`.
fn column_choices(app: &App) -> Vec<(String, Value)> {
  let d = &app.s.design;
  let mut out = Vec::new();
  for p in &d.parameters {
    out.push((format!("Parameter {}", p.name), json!({"type": "parameter", "name": p.name})));
  }
  for f in &d.features {
    out.push((format!("Suppress {}", f.name), json!({"type": "suppress", "feature": f.id.0})));
    if let fox_engine::model::FeatureKind::Joint { kind, .. } = &f.kind {
      for m in ["rotation", "slide", "slide2", "pitch"] {
        if fox_engine::model::assembly::MotionAxis::parse(m).is_some_and(|a| a.allowed(*kind)) {
          out.push((format!("{} {m}", f.name), json!({"type": "joint", "joint": f.id.0, "motion": m})));
        }
      }
    }
  }
  for b in &app.s.model.bodies {
    out.push((format!("Appearance of {}", app.s.body_name(b.id)), json!({"type": "appearance", "body": b.id.0})));
  }
  out
}

/// A text field that reports its text when editing ends (Enter or focus lost).
fn edit_text(
  ui: &mut egui::Ui,
  key: impl std::hash::Hash + std::fmt::Debug,
  current: &str,
  width: f32,
  done: impl FnOnce(String),
) {
  let id = egui::Id::new(key);
  let mut text = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| current.to_string());
  let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(width).hint_text("default"));
  if r.changed() {
    ui.data_mut(|d| d.insert_temp(id, text.clone()));
  }
  if r.lost_focus() {
    ui.data_mut(|d| d.remove::<String>(id));
    done(text);
  }
}

fn add_row(app: &mut App, ui: &mut egui::Ui) {
  let id = egui::Id::new("config-new-row");
  let mut name = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_default();
  ui.horizontal(|ui| {
    ui.label("New configuration:");
    ui.add(egui::TextEdit::singleline(&mut name).hint_text("name").desired_width(120.0));
    let copy = app.s.design.configurations.active.clone();
    if ui.button("+ Add").clicked() && !name.trim().is_empty() {
      let mut p = json!({"name": name.trim()});
      if let Some(c) = copy {
        p["copy"] = json!(c);
      }
      if app.run("config.create", p).is_some() {
        name.clear();
      }
    }
  });
  ui.data_mut(|d| d.insert_temp(id, name));
}

/// Adds a column: overrides the chosen parameter, feature, body or joint in the first row with
/// its current value (edit it from there).
fn add_column(app: &mut App, ui: &mut egui::Ui, first: Option<String>) {
  let Some(row) = first else {
    ui.label(RichText::new("Add a configuration to start the table.").small().color(theme::MUTED));
    return;
  };
  let d = &app.s.design;
  let mut choices: Vec<(String, Value)> = Vec::new();
  for p in &d.parameters {
    choices.push((format!("Parameter {}", p.name), json!({"parameter": p.name, "expr": p.expr})));
  }
  for f in &d.features {
    choices.push((format!("Suppress {}", f.name), json!({"feature": f.id.0, "suppressed": f.suppressed})));
    if let fox_engine::model::FeatureKind::Joint { kind, rotation, slide, slide2, pitch, .. } = &f.kind {
      for (m, v) in [("rotation", rotation), ("slide", slide), ("slide2", slide2), ("pitch", pitch)] {
        let axis = fox_engine::model::assembly::MotionAxis::parse(m);
        if axis.is_some_and(|a| a.allowed(*kind)) {
          choices.push((format!("{} {m}", f.name), json!({"joint": f.id.0, "motion": m, "value": v})));
        }
      }
    }
  }
  for b in &app.s.model.bodies {
    let color = d.body_settings(b.id).and_then(|s| s.color);
    let v = color.map(|c| json!(c)).unwrap_or(Value::Null);
    choices.push((format!("Appearance of {}", app.s.body_name(b.id)), json!({"body": b.id.0, "color": v})));
  }
  let id = egui::Id::new("config-new-col");
  let mut pick = ui.data_mut(|d| d.get_temp::<usize>(id)).unwrap_or(0).min(choices.len().saturating_sub(1));
  let mut add = None;
  ui.horizontal(|ui| {
    ui.label("New column:");
    egui::ComboBox::from_id_salt("config-col-pick")
      .selected_text(choices.get(pick).map(|c| c.0.as_str()).unwrap_or("nothing to add"))
      .width(220.0)
      .show_ui(ui, |ui| {
        for (i, (label, _)) in choices.iter().enumerate() {
          ui.selectable_value(&mut pick, i, label);
        }
      });
    if ui.button("+ Add").clicked() {
      add = choices.get(pick).map(|c| c.1.clone());
    }
  });
  ui.data_mut(|d| d.insert_temp(id, pick));
  if let Some(mut p) = add {
    p["configuration"] = json!(row);
    app.run("config.set", p);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn typed_cells_become_config_set_parameters() {
    let p = cell_params(&json!({"type": "parameter", "name": "h"}), " 30 mm ");
    assert_eq!(p, json!({"parameter": "h", "expr": "30 mm"}));
    assert_eq!(cell_params(&json!({"type": "parameter", "name": "h"}), "")["expr"], Value::Null);
    assert_eq!(cell_params(&json!({"type": "suppress", "feature": 3}), "suppressed")["suppressed"], true);
    assert_eq!(cell_params(&json!({"type": "suppress", "feature": 3}), "active")["suppressed"], false);
    assert_eq!(cell_params(&json!({"type": "appearance", "body": 7}), "#ff0000")["color"], "#ff0000");
    assert_eq!(cell_params(&json!({"type": "appearance", "body": 7}), "ABS (Red)")["appearance"], "ABS (Red)");
    assert!(cell_params(&json!({"type": "appearance", "body": 7}), "").get("color").is_none());
    let j = cell_params(&json!({"type": "joint", "joint": 4, "motion": "slide"}), "5 mm");
    assert_eq!(j, json!({"joint": 4, "motion": "slide", "value": "5 mm"}));
  }

  #[test]
  fn rule_fields_become_config_rule_parameters() {
    let col = json!({"type": "parameter", "name": "h"});
    assert_eq!(rule_params(&col, " w / 2 ", "", ""), json!({"parameter": "h", "expr": "w / 2"}));
    assert_eq!(rule_params(&col, "", "", "")["expr"], Value::Null);
    let col = json!({"type": "appearance", "body": 7});
    let p = rule_params(&col, "w > 1", "ABS (Red)", "none");
    assert_eq!(p, json!({"body": 7, "expr": "w > 1", "color": "ABS (Red)", "otherwise": null}));
    assert_eq!(color_text(&json!([255, 0, 16])), "#ff0010");
    assert_eq!(color_text(&Value::Null), "");
    let mut s = fox_engine::Session::default();
    s.execute("param.add", &json!({"name": "w", "expr": "10 mm"})).unwrap();
    s.execute("box", &json!({"width": "w", "depth": 10, "height": 10})).unwrap();
    s.execute("config.create", &json!({"name": "A"})).unwrap();
    s.execute("config.set", &json!({"configuration": "A", "parameter": "w", "expr": "30 mm"})).unwrap();
    let b = s.model.bodies[0].id.0;
    let p = rule_params(&json!({"type": "appearance", "body": b}), "w > 20 mm", "#102030", "");
    let t = s.execute("config.rule", &p).unwrap();
    assert_eq!(t["rows"][0]["cells"][1], "#102030", "{t}");
    assert_eq!(color_text(&t["columns"][1]["rule"]["color"]), "#102030");
  }

  #[test]
  fn table_cells_drive_the_engine() {
    let mut s = fox_engine::Session::default();
    s.execute("box", &json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    s.execute("config.create", &json!({"name": "A"})).unwrap();
    let b = s.model.bodies[0].id.0;
    let mut p = cell_params(&json!({"type": "appearance", "body": b}), "#102030");
    p["configuration"] = json!("A");
    s.execute("config.set", &p).unwrap();
    let t = s.execute("config.table", &json!({})).unwrap();
    assert_eq!(t["rows"][0]["cells"][0], "#102030");
    // the column a cell names round-trips through the table's JSON
    let mut p = cell_params(&t["columns"][0]["column"], "");
    p["configuration"] = json!("A");
    s.execute("config.set", &p).unwrap();
    assert!(s.design.configurations.list[0].overrides.is_empty());
  }
}
