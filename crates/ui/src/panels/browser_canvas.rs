//! The browser's Canvases folder: one row per canvas with its visibility eye; click selects it,
//! double-click edits it, right-click offers Edit Canvas, Calibrate, Trace, Rename and Delete.

use fox_engine::Sel;
use fox_engine::model::{FeatureId, FeatureKind};
use serde_json::json;

use super::{browser_edit, is_open, reveal_row, tree_row};
use crate::icons::Icon;
use crate::{App, RenameTarget, canvas_ui, theme};

pub(super) fn section(app: &mut App, ui: &mut egui::Ui) {
  let canvases: Vec<(FeatureId, String, bool)> = app
    .s
    .design
    .features
    .iter()
    .filter(|f| matches!(f.kind, FeatureKind::Canvas { .. }))
    .map(|f| (f.id, f.name.clone(), f.visible != Some(false)))
    .collect();
  if canvases.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-canvases");
  let all_visible = canvases.iter().all(|c| c.2);
  let ev = tree_row(ui, id, 1, Some(true), Some(all_visible), Icon::Folder, "Canvases", false, theme::TEXT);
  if ev.eye {
    for (c, _, _) in &canvases {
      app.run("canvas.edit", json!({"feature": c.0, "visible": !all_visible}));
    }
  }
  if !is_open(ui, id, true) {
    return;
  }
  for (fid, name, visible) in canvases {
    let sel = Sel::Feature { feature: fid };
    let selected = app.s.selection.contains(&sel);
    let row = id.with(fid.0);
    let color = if visible { theme::TEXT } else { theme::MUTED };
    let ev = tree_row(ui, row, 2, None, Some(visible), Icon::Canvas, &name, selected, color);
    let target = RenameTarget::Feature(fid.0);
    browser_edit::slow_click(ui, &ev, row, target, &name, selected);
    if ev.eye {
      app.run("canvas.edit", json!({"feature": fid.0, "visible": !visible}));
    } else if ev.double_clicked {
      canvas_ui::start_edit(app, fid);
    } else if ev.clicked {
      app.s.selection = vec![sel];
    }
    browser_edit::slow_click_due(ui, row, app.s.selection.contains(&sel));
    browser_edit::edit_box(app, ui, &ev, row, 2);
    if let Some(r) = ev.response {
      reveal_row(app, sel, &r);
      r.on_hover_text("Double-click to edit; right-click to Calibrate or Trace").context_menu(|ui| {
        if ui.button("Edit Canvas").clicked() {
          ui.close();
          canvas_ui::start_edit(app, fid);
        }
        if ui.button("Calibrate").clicked() {
          ui.close();
          canvas_ui::start_calibrate(app, fid);
        }
        if ui.button("Trace Canvas").on_hover_text("Trace the image into sketch curves where it lies").clicked() {
          ui.close();
          app.trace_canvas(fid);
        }
        if ui.button(if visible { "Hide" } else { "Show" }).clicked() {
          ui.close();
          app.run("canvas.edit", json!({"feature": fid.0, "visible": !visible}));
        }
        if ui.button("Rename").clicked() {
          ui.close();
          browser_edit::begin(ui, row, target, &name);
        }
        ui.separator();
        if ui.button("Delete").clicked() {
          ui.close();
          app.run("feature.delete", json!({"feature": fid.0}));
        }
      });
    }
  }
}
