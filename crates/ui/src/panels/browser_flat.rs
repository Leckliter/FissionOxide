//! The flat pattern in the browser, as Fusion's: a Flat Pattern row under the Rule row once one
//! exists (Activate, Export Flat Pattern, Delete; a yellow warning triangle while a body is
//! unfolded), and in the flat pattern environment the flat pattern's own tree: the "Flat
//! Pattern" root, and Bodies with a Bend Lines folder (Center Lines, Extent Lines, each with an
//! eye) above the body.

use egui::{Rect, Vec2, pos2, vec2};
use fox_engine::Sel;

use super::{RowCmd, RowItem, is_open, row_item, row_key, row_menu, tree_row, warning_glyph};
use crate::icons::Icon;
use crate::{App, marking, theme};

/// The Flat Pattern row's menu.
pub(crate) fn row_menu_items() -> Vec<Option<RowItem>> {
  vec![
    row_item(Icon::FlatPattern, "Activate", RowCmd::Activate),
    row_item(Icon::Export, "Export Flat Pattern", RowCmd::ExportFlatPattern),
    None,
    row_key(Icon::Remove, "Delete", "Del", RowCmd::Delete),
  ]
}

/// Runs a Flat Pattern row menu command.
pub(crate) fn run_row_cmd(app: &mut App, cmd: RowCmd) {
  match cmd {
    RowCmd::Activate => crate::flat_ui::activate(app),
    RowCmd::ExportFlatPattern => crate::flat_ui::open_export(app),
    RowCmd::Delete => crate::flat_ui::delete(app),
    _ => {}
  }
}

/// The design's Flat Pattern row (none until Create Flat Pattern). Double-click activates it.
pub(super) fn row(app: &mut App, ui: &mut egui::Ui) {
  if app.s.design.flat_pattern.is_none() {
    return;
  }
  let id = egui::Id::new("tree-flat-pattern");
  let ev = tree_row(ui, id, 1, None, None, Icon::FlatPattern, "Flat Pattern", false, theme::TEXT);
  let Some(r) = ev.response else { return };
  // a later Unfold leaves the flat pattern out of date until the model is refolded
  if !crate::flat_ui::unfolded(app).is_empty() {
    let c = pos2(r.rect.min.x + 8.0 + 16.0 + 44.0, r.rect.center().y);
    let ir = Rect::from_center_size(c, Vec2::splat(16.0));
    let fill = if r.hovered() { theme::HOVER } else { theme::PILL };
    ui.painter().rect_filled(ir.expand(1.0), 0.0, fill);
    warning_glyph(ui.painter(), ir.shrink2(vec2(0.0, 1.0)));
  }
  if r.hovered() {
    marking::peek(app, Vec::new(), "Flat Pattern".into());
  }
  if ev.double_clicked {
    crate::flat_ui::activate(app);
  }
  r.context_menu(|ui| {
    if let Some(cmd) = row_menu(ui, &row_menu_items()) {
      run_row_cmd(app, cmd);
    }
  });
}

/// The browser in the flat pattern environment.
pub(super) fn env_tree(app: &mut App, ui: &mut egui::Ui) {
  let root = egui::Id::new("tree-flat-root");
  tree_row(ui, root, 0, Some(true), None, Icon::FlatPattern, "Flat Pattern", false, theme::TEXT);
  if !is_open(ui, root, true) {
    return;
  }
  let bodies = egui::Id::new("tree-flat-bodies");
  let Some(body) = app.s.design.flat_pattern.as_ref().map(|f| f.body) else { return };
  tree_row(ui, bodies, 1, Some(true), Some(true), Icon::Folder, "Bodies", false, theme::TEXT);
  if !is_open(ui, bodies, true) {
    return;
  }
  let lines = bodies.with("bend-lines");
  let all = app.flat_lines.iter().all(|v| *v);
  let ev = tree_row(ui, lines, 2, Some(true), Some(all), Icon::Folder, "Bend Lines", false, theme::TEXT);
  if ev.eye {
    app.flat_lines = [!all; 2];
  }
  if is_open(ui, lines, true) {
    for (i, label) in ["Center Lines", "Extent Lines"].into_iter().enumerate() {
      let on = app.flat_lines[i];
      let ev = tree_row(ui, lines.with(i), 3, None, Some(on), Icon::Line, label, false, theme::TEXT);
      if ev.eye {
        app.flat_lines[i] = !on;
      }
    }
  }
  let name = app.s.body_name(body);
  let selected = app.s.selection.contains(&Sel::Body { body });
  let ev = tree_row(ui, bodies.with(body.0), 2, None, Some(true), Icon::Body, &name, selected, theme::TEXT);
  if let Some(r) = ev.response
    && r.hovered()
  {
    marking::peek(app, vec![body], name);
  }
}
