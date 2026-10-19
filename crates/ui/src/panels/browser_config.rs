//! The browser's Configurations folder: one row per configuration (click to activate), the
//! design as modelled, and the configuration table.

use serde_json::json;

use super::{is_open, tree_row};
use crate::icons::Icon;
use crate::{App, config_ui, theme};

pub(super) fn section(app: &mut App, ui: &mut egui::Ui) {
  let rows: Vec<String> = app.s.design.configurations.list.iter().map(|c| c.name.clone()).collect();
  let history_off = app.s.design.history_off;
  if rows.is_empty() && !history_off {
    return;
  }
  if history_off {
    let ev =
      tree_row(ui, egui::Id::new("tree-history"), 1, None, None, Icon::Box, "Design history off", false, theme::MUTED);
    if let Some(r) = ev.response {
      r.on_hover_text("Direct edits go into the base feature; right-click to capture history again").context_menu(
        |ui| {
          if ui.button("Capture Design History").clicked() {
            ui.close();
            config_ui::toggle_history(app);
          }
        },
      );
    }
  }
  if rows.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-configurations");
  let ev = tree_row(ui, id, 1, Some(true), None, Icon::ConfigTable, "Configurations", false, theme::TEXT);
  if ev.double_clicked {
    config_ui::open_table();
  }
  if let Some(r) = ev.response {
    r.on_hover_text("Double-click for the configuration table").context_menu(|ui| {
      if ui.button("Configuration Table").clicked() {
        ui.close();
        config_ui::open_table();
      }
    });
  }
  if !is_open(ui, id, true) {
    return;
  }
  let active = app.s.design.configurations.active.clone();
  let ev = tree_row(ui, id.with("default"), 2, None, None, Icon::Configure, "(Default)", active.is_none(), theme::TEXT);
  if ev.clicked && active.is_some() {
    app.run_background("config.activate", json!({}));
  }
  for name in rows {
    let on = active.as_deref() == Some(name.as_str());
    let ev = tree_row(ui, id.with(&name), 2, None, None, Icon::Configure, &name, on, theme::TEXT);
    if ev.clicked && !on {
      app.run_background("config.activate", json!({"name": name}));
    }
    if ev.double_clicked {
      config_ui::open_table();
    }
    if let Some(r) = ev.response {
      r.on_hover_text("Click to make it active").context_menu(|ui| {
        if ui.button("Activate").clicked() {
          ui.close();
          app.run_background("config.activate", json!({"name": name}));
        }
        if ui.button("Duplicate").clicked() {
          ui.close();
          let mut k = 2;
          while app.s.design.configurations.get(&format!("{name} {k}")).is_some() {
            k += 1;
          }
          app.run("config.create", json!({"name": format!("{name} {k}"), "copy": name}));
        }
        if ui.button("Edit in Table").clicked() {
          ui.close();
          config_ui::open_table();
        }
        ui.separator();
        if ui.button("Delete").clicked() {
          ui.close();
          app.run("config.delete", json!({"name": name}));
        }
      });
    }
  }
}
