//! The Manufacture workspace's browser, as Fusion's: "CAM Root" with its eye, the Units row,
//! then (after the Named Views and Origin folders the Design browser shares) Models, the design
//! being machined, and Setups, each setup with its operations. A setup's or operation's
//! right-click menu deletes it.

use serde_json::json;

use super::{is_open, menu_item, tree_row};
use crate::icons::Icon;
use crate::{App, theme};

/// The CAM Root row and its Units row. Returns whether the root is open.
pub(super) fn root(app: &mut App, ui: &mut egui::Ui) -> bool {
  let root = egui::Id::new("tree-cam-root");
  let visible = super::browser_rows::root_visible(app) || !app.cam_setups_hidden;
  let ev = tree_row(ui, root, 0, Some(true), Some(visible), Icon::Component, "CAM Root", false, theme::TEXT);
  if ev.eye {
    // the root's eye hides the models and the setups' toolpaths together
    if super::browser_rows::root_visible(app) == visible {
      super::browser_rows::root_show_hide(app);
    }
    app.cam_setups_hidden = visible;
  }
  if !is_open(ui, root, true) {
    return false;
  }
  let label = units_label(app.s.design.units);
  tree_row(ui, root.with("units"), 1, None, None, Icon::Measure, &label, false, theme::TEXT);
  true
}

/// The Units row: "Units: In" in an inch design, as Fusion's CAM Root shows it.
pub(crate) fn units_label(unit: fox_engine::model::LengthUnit) -> String {
  use fox_engine::model::LengthUnit;
  let name = match unit {
    LengthUnit::In => "In",
    LengthUnit::Ft => "Ft",
    u => u.symbol(),
  };
  format!("Units: {name}")
}

/// Models: the design the setups machine, with an eye that shows or hides it.
pub(super) fn models(app: &mut App, ui: &mut egui::Ui) {
  let id = egui::Id::new("tree-cam-models");
  let visible = super::browser_rows::root_visible(app);
  let ev = tree_row(ui, id, 1, Some(false), Some(visible), Icon::Folder, "Models", false, theme::TEXT);
  if ev.eye {
    super::browser_rows::root_show_hide(app);
  }
  if !is_open(ui, id, false) {
    return;
  }
  let name = super::browser_rows::doc_name(app);
  let color = if visible { theme::TEXT } else { theme::MUTED };
  tree_row(ui, id.with("model"), 2, None, None, Icon::Component, &name, false, color);
}

/// Setups: one row per setup with its operations; the eye shows or hides their toolpaths and
/// stock.
pub(super) fn setups(app: &mut App, ui: &mut egui::Ui) {
  let id = egui::Id::new("tree-cam-setups");
  let shown = !app.cam_setups_hidden;
  let ev = tree_row(ui, id, 1, Some(true), Some(shown), Icon::Folder, "Setups", false, theme::TEXT);
  if ev.eye {
    app.cam_setups_hidden = shown;
  }
  if !is_open(ui, id, true) {
    return;
  }
  let setups: Vec<(u32, String, bool, Vec<String>)> = app
    .s
    .design
    .cam
    .iter()
    .map(|st| (st.id, st.name.clone(), st.lathe.is_some(), st.operations.iter().map(|o| o.name.clone()).collect()))
    .collect();
  let color = if shown { theme::TEXT } else { theme::MUTED };
  for (sid, name, lathe, ops) in setups {
    let row = id.with(sid);
    let icon = if lathe { Icon::Revolve } else { Icon::Box };
    let expand = (!ops.is_empty()).then_some(true);
    let ev = tree_row(ui, row, 2, expand, None, icon, &name, false, color);
    if let Some(r) = ev.response {
      r.context_menu(|ui| {
        if menu_item(ui, Icon::Remove, "Delete", None).clicked() {
          ui.close();
          if app.run("cam.delete", json!({"setup": sid})).is_some() {
            app.status = Some((format!("{name} deleted"), false));
          }
        }
      });
    }
    if ops.is_empty() || !is_open(ui, row, true) {
      continue;
    }
    for op in ops {
      let ev = tree_row(ui, row.with(&op), 3, None, None, Icon::Shell, &op, false, color);
      if let Some(r) = ev.response {
        r.context_menu(|ui| {
          if menu_item(ui, Icon::Remove, "Delete", None).clicked() {
            ui.close();
            app.run("cam.delete", json!({"setup": sid, "operation": op}));
          }
        });
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::model::LengthUnit;

  #[test]
  fn the_units_row_reads_like_fusions() {
    assert_eq!(super::units_label(LengthUnit::In), "Units: In");
    assert_eq!(super::units_label(LengthUnit::Mm), "Units: mm");
  }
}
