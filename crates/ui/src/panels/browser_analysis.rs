//! The browser's Analysis folder, between Origin and Bodies as in Fusion: one row per kept
//! analysis ("Section1", "Zebra1", "CenterOfMass1") with its eye; the folder's eye hides them all
//! (restoring the full model) and a row's menu shows, hides or deletes it.

use super::{is_open, tree_row};
use crate::icons::Icon;
use crate::{App, analyses, theme};

pub(super) fn section(app: &mut App, ui: &mut egui::Ui) {
  if app.sections.is_empty() && app.analyses.rows.is_empty() {
    return;
  }
  let id = egui::Id::new("tree-analysis");
  let shown = !app.analysis_hidden;
  let ev = tree_row(ui, id, 1, Some(true), Some(shown), Icon::Folder, "Analysis", false, theme::TEXT);
  if ev.eye {
    app.analysis_hidden = shown;
  }
  if !is_open(ui, id, true) {
    return;
  }
  let mut delete = None;
  for i in 0..app.sections.len() {
    let (name, visible) = (app.sections[i].name.clone(), app.sections[i].visible);
    let color = if visible && shown { theme::TEXT } else { theme::MUTED };
    let ev = tree_row(ui, id.with(i), 2, None, Some(visible), Icon::Section, &name, false, color);
    if ev.eye {
      app.sections[i].visible = !visible;
    }
    if let Some(r) = ev.response {
      r.context_menu(|ui| {
        if ui.button(if visible { "Hide" } else { "Show" }).clicked() {
          ui.close();
          app.sections[i].visible = !visible;
        }
        if ui.button("Delete").clicked() {
          ui.close();
          delete = Some(i);
        }
      });
    }
  }
  if let Some(i) = delete {
    app.sections.remove(i);
  }
  // the other analyses (see [`analyses`])
  let mut delete = None;
  for i in 0..app.analyses.rows.len() {
    let (name, kind, visible) = {
      let r = &app.analyses.rows[i];
      (r.name.clone(), r.kind.clone(), r.visible)
    };
    let color = if visible && shown { theme::TEXT } else { theme::MUTED };
    let ev = tree_row(ui, id.with(&name), 2, None, Some(visible), analyses::icon(&kind), &name, false, color);
    if ev.eye {
      app.analyses.rows[i].visible = !visible;
    }
    if let Some(r) = ev.response {
      r.context_menu(|ui| {
        if ui.button(if visible { "Hide" } else { "Show" }).clicked() {
          ui.close();
          app.analyses.rows[i].visible = !visible;
        }
        if ui.button("Delete").clicked() {
          ui.close();
          delete = Some(i);
        }
      });
    }
  }
  if let Some(i) = delete {
    analyses::delete(app, i);
  }
}
