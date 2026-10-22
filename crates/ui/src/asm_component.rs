//! New Component in an Assembly Design, as Fusion's: the component is an external design. The
//! docked NEW COMPONENT dialog has Parent, Design Type (part, assembly, hybrid, electronics),
//! Part Type (standard or sheet metal), Design Name ("External1"), Location (the project it is
//! saved to) and Activate. OK saves a new empty design of that type to the project and inserts
//! it, linked, as a component of the assembly.

use egui::{Color32, RichText, Stroke, vec2};
use fox_engine::Session;
use fox_engine::model::DesignType;
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::{App, panels, theme};

/// The Design Type toggles: Fusion's four, electronics greyed (FissionOxide can't make an
/// external electronics design).
pub const DESIGN_TYPES: [(&str, Icon, &str); 4] = [
  ("part", Icon::Body, "Part Design"),
  ("assembly", Icon::InsertComponent, "Assembly Design"),
  ("hybrid", Icon::Component, "Hybrid Design"),
  ("electronics", Icon::Configure, "Electronics Design"),
];

/// The Part Type toggles.
pub const PART_TYPES: [(&str, Icon, &str); 2] =
  [("standard", Icon::Body, "Standard Part"), ("sheet_metal", Icon::Flange, "Sheet Metal Part")];

/// The open dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct AsmComponent {
  pub design_type: &'static str,
  pub part_type: &'static str,
  pub name: String,
  /// The project the design is saved to (None: the hub's default).
  pub project: Option<String>,
  pub activate: bool,
}

/// Whether New Component makes an external design: in an Assembly Design.
pub(crate) fn applies(app: &App) -> bool {
  app.s.design.design_type == DesignType::Assembly
}

/// "External1", "External2", ...: the first name no component has.
fn next_name(app: &App) -> String {
  (1..)
    .map(|n| format!("External{n}"))
    .find(|n| !app.s.design.components.iter().any(|c| &c.name == n))
    .unwrap_or_default()
}

/// The hub's projects: (id, name).
fn projects(app: &mut App) -> Vec<(String, String)> {
  let list = app.s.execute("hub.projects", &json!({})).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
  let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
  list.iter().map(|p| (s(p, "id"), s(p, "name"))).collect()
}

/// CREATE > New Component in an Assembly Design.
pub(crate) fn open(app: &mut App) {
  app.cancel_dialog();
  let name = next_name(app);
  app.asm_component =
    Some(AsmComponent { design_type: "part", part_type: "standard", name, project: None, activate: true });
  app.hint("Select a parent component");
}

/// The project shown in Location: the one picked, else "Default Project".
fn location_label(app: &mut App, project: &Option<String>) -> String {
  match project {
    Some(id) => projects(app).into_iter().find(|(p, _)| p == id).map(|(_, n)| n).unwrap_or_else(|| id.clone()),
    None => "Default Project".into(),
  }
}

/// Draws the dialog, docked where command dialogs dock.
pub(crate) fn show(app: &mut App, ctx: &egui::Context) {
  let Some(mut d) = app.asm_component.clone() else { return };
  if app.dialog.is_some() {
    // another command took the dialog's place
    app.asm_component = None;
    app.end_hint();
    return;
  }
  let width = 320.0;
  let dock = panels::dialog_dock(app.canvas, app.s.edit_sketch.is_some());
  let root =
    if crate::documents::unsaved(&app.s) { "(Unsaved)".to_string() } else { crate::documents::tab_label(&app.s) };
  let location = location_label(app, &d.project);
  let all_projects = projects(app);
  let (mut ok, mut cancel) = (false, false);
  egui::Area::new(egui::Id::new("asm-new-component"))
    .pivot(egui::Align2::RIGHT_TOP)
    .fixed_pos(dock)
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      panels::dialog_frame().show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        panels::dialog_header(ui, Icon::Component, "NEW COMPONENT", width);
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
          egui::Grid::new("asm-newcomp").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
            ui.label("Parent");
            ui.vertical(|ui| {
              chip(ui, "1 selected");
              ui.label(RichText::new(&root).color(theme::MUTED).size(11.5));
            });
            ui.end_row();
            ui.label("Design Type");
            ui.horizontal(|ui| {
              for (key, icon, tip) in DESIGN_TYPES {
                let off = key == "electronics";
                let r =
                  icons::icon_button(ui, icon, 24.0, tip, d.design_type == key, off.then_some(Color32::from_gray(185)));
                if r.clicked() && !off {
                  d.design_type = key;
                }
              }
            });
            ui.end_row();
            ui.label("Part Type");
            ui.horizontal(|ui| {
              let on = d.design_type == "part";
              for (key, icon, tip) in PART_TYPES {
                let tint = (!on).then_some(Color32::from_gray(185));
                if icons::icon_button(ui, icon, 24.0, tip, on && d.part_type == key, tint).clicked() && on {
                  d.part_type = key;
                }
              }
            });
            ui.end_row();
            ui.label("Design Name");
            ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(170.0));
            ui.end_row();
            ui.label("Location");
            ui.horizontal(|ui| {
              let r = chip(ui, &location);
              egui::Popup::menu(&r).show(|ui| {
                for (id, name) in &all_projects {
                  if ui.selectable_label(d.project.as_deref() == Some(id.as_str()), name).clicked() {
                    d.project = Some(id.clone());
                  }
                }
              });
              if icons::icon_button(ui, Icon::Close, 18.0, "Back to the Default Project", false, None).clicked() {
                d.project = None;
              }
            });
            ui.end_row();
            ui.label("Activate");
            if icons::icon_button(ui, Icon::Component, 24.0, "Activate the new component", d.activate, None).clicked() {
              d.activate = !d.activate;
            }
            ui.end_row();
          });
        });
        let sep = ui.allocate_exact_size(vec2(width, 1.0), egui::Sense::hover()).0;
        ui.painter().rect_filled(sep, 0.0, theme::SEPARATOR);
        egui::Frame::new().fill(theme::DIALOG_BODY).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
          ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
              if panels::secondary_button(ui, "Cancel").clicked() {
                cancel = true;
              }
              if !d.name.trim().is_empty() && panels::primary_button(ui, "OK", theme::ACCENT).clicked() {
                ok = true;
              }
            });
          });
        });
      });
    });
  if app.asm_component.is_none() {
    return;
  }
  app.asm_component = Some(d.clone());
  if cancel {
    close(app);
  } else if ok {
    close(app);
    create(app, &d);
  }
}

/// A grey selection chip.
fn chip(ui: &mut egui::Ui, text: &str) -> egui::Response {
  ui.add(
    egui::Button::new(RichText::new(text).color(theme::TEXT).size(12.0))
      .fill(theme::CONTEXT_TAB)
      .stroke(Stroke::new(1.0, theme::BORDER)),
  )
}

/// Closes the dialog (Cancel, Escape). Returns whether it was open.
pub(crate) fn close(app: &mut App) -> bool {
  if app.asm_component.take().is_some() {
    app.end_hint();
    return true;
  }
  false
}

/// OK: saves an empty design of the picked type to the project and inserts it, linked.
pub(crate) fn create(app: &mut App, d: &AsmComponent) {
  let name = d.name.trim().to_string();
  let mut ext = Session::default();
  ext.store_root = app.s.store_root.clone();
  ext.design.name = name.clone();
  let mut setup = vec![("design.units", json!({"unit": app.s.design.units.symbol()}))];
  if d.design_type != "part" {
    setup.push(("design.type", json!({"type": d.design_type})));
  }
  for (cmd, p) in setup {
    if let Err(e) = ext.execute(cmd, &p) {
      app.status = Some((e.0, true));
      return;
    }
  }
  let mut save = json!({"name": name, "new": true});
  if let Some(p) = &d.project {
    save["project"] = json!(p);
  }
  let item = match ext.execute("hub.save", &save) {
    Ok(r) => r.get("item").and_then(Value::as_str).unwrap_or_default().to_string(),
    Err(e) => {
      app.status = Some((format!("Couldn't save {name}: {}", e.0), true));
      return;
    }
  };
  app.data.invalidate();
  let Some(r) = app.run("component.insert", json!({"item": item, "link": true, "name": name, "empty": true})) else {
    return;
  };
  let comp = r.get("component").and_then(Value::as_u64);
  if d.part_type == "sheet_metal"
    && d.design_type == "part"
    && let Some(c) = comp
  {
    app.run("component.properties", json!({"component": c, "type": "sheet_metal"}));
  }
  if d.activate
    && let Some(c) = comp
  {
    app.run("component.activate", json!({"component": c}));
  }
  app.status = Some((format!("Created {name}, an external design in the project"), false));
}
