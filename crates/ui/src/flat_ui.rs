//! Create Flat Pattern's environment, as Fusion's: OK on CREATE FLAT PATTERN (or the browser's
//! Flat Pattern row > Activate) swaps the ribbon for FLAT PATTERN SOLID / SURFACE with an
//! EXPORT panel and a green FINISH FLAT PATTERN, the browser for the flat pattern's own tree
//! (Bodies with a Bend Lines folder), the timeline for the flat pattern's one item, and the
//! canvas for the flat, seen from the Front. Finish Flat Pattern goes back to the design, whose
//! browser then has a Flat Pattern row. Also the REFOLD panel an Unfold leaves on the ribbon.

use egui::{Rect, Stroke, Vec2, vec2};
use serde_json::{Value, json};

use crate::dialogs::Kind;
use crate::icons::{self, Icon};
use crate::panels::Tab;
use crate::{App, theme};

/// The environment's own commands, and the REFOLD panel's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FlatAct {
  /// FINISH FLAT PATTERN.
  Finish,
  /// EXPORT > Export Flat Pattern (the dialog).
  Export,
  /// REFOLD > Refold: runs at once, no dialog.
  Refold,
}

pub(crate) fn info(a: FlatAct) -> (Icon, &'static str, &'static str) {
  match a {
    FlatAct::Finish => (Icon::Finish, "Finish Flat Pattern", "Finish the flat pattern and return to the folded model"),
    FlatAct::Export => (Icon::Export, "Export Flat Pattern", "Export the flat pattern as a DXF or STEP file"),
    FlatAct::Refold => (Icon::Refold, "Refold", "Refold the unfolded bends"),
  }
}

pub(crate) fn run(app: &mut App, a: FlatAct) {
  match a {
    FlatAct::Finish => finish(app),
    FlatAct::Export => open_export(app),
    FlatAct::Refold => refold(app),
  }
}

/// Enters the environment on the design's flat pattern: the FLAT PATTERN SOLID tab, the
/// canvas turned to Front and framed on the flat.
pub(crate) fn enter(app: &mut App) {
  if app.s.design.flat_pattern.is_none() {
    return;
  }
  if app.s.edit_sketch.is_some() {
    app.finish_sketch();
  }
  app.flat_mode = true;
  app.tab = Tab::Solid;
  app.s.selection.clear();
  let mut to = app.view_now();
  let (dir, up) = app.cube.standard_view(fox_engine::render::StandardView::Front);
  to.look_from(dir, up);
  if let Some((lo, hi)) = fox_engine::flat::flat_bounds(&app.s) {
    let aspect = app.frame.as_ref().map(|f| f.width as f64 / f.height.max(1) as f64).unwrap_or(1.6);
    to.fit(lo, hi, aspect);
  }
  app.fly_to(to);
}

/// FINISH FLAT PATTERN: back to the folded design on the SHEET METAL tab, the camera Home.
pub(crate) fn finish(app: &mut App) {
  if !app.flat_mode {
    return;
  }
  app.cancel_dialog();
  app.flat_mode = false;
  app.tab = Tab::SheetMetal;
  app.s.selection.clear();
  app.go_home();
}

/// Leaves the environment when its flat pattern is gone (undone or deleted).
pub(crate) fn check(app: &mut App) {
  if app.flat_mode && app.s.design.flat_pattern.is_none() {
    app.flat_mode = false;
    app.tab = Tab::SheetMetal;
  }
}

/// The browser row's Activate.
pub(crate) fn activate(app: &mut App) {
  app.cancel_dialog();
  enter(app);
}

/// The browser row's Delete (Del).
pub(crate) fn delete(app: &mut App) {
  app.run("sheet.delete_flat_pattern", json!({}));
}

/// Opens EXPORT FLAT PATTERN.
pub(crate) fn open_export(app: &mut App) {
  app.start(Kind::ExportFlat);
  if let Some(d) = app.dialog.as_mut().filter(|d| d.kind == Kind::ExportFlat) {
    d.export_flat_begin(&app.s);
  }
}

/// REFOLD > Refold: folds the unfolded body back up at once, as Fusion's (no dialog).
fn refold(app: &mut App) {
  let Some(body) = unfolded(app).last().copied() else {
    app.status = Some(("Nothing is unfolded".into(), true));
    return;
  };
  app.cancel_dialog();
  app.run("sheet.refold", json!({"body": body}));
}

/// The bodies an Unfold left flat (not refolded yet).
pub(crate) fn unfolded(app: &App) -> Vec<u32> {
  app.s.model.bodies.iter().filter(|b| b.unfolded.is_some()).map(|b| b.id.0).collect()
}

/// The REFOLD panel on the SHEET METAL ribbon: Some(enabled) while an Unfold dialog is open
/// (greyed) or a body is unfolded (lit), None otherwise.
pub(crate) fn refold_panel(app: &App) -> Option<bool> {
  if !unfolded(app).is_empty() {
    return Some(true);
  }
  app.dialog.as_ref().is_some_and(|d| d.kind == Kind::Unfold).then_some(false)
}

/// The tab's name in the environment: FLAT PATTERN SOLID, FLAT PATTERN SURFACE.
pub(crate) fn tab_label(t: Tab) -> String {
  format!("FLAT PATTERN {}", t.label())
}

/// The canvas's render parameters in the environment: the flat in place of its body, with the
/// Bend Lines the browser shows.
pub(crate) fn view_params(app: &App, v: &mut Value) {
  if app.flat_mode {
    v["flat"] = json!({"center_lines": app.flat_lines[0], "extent_lines": app.flat_lines[1]});
  }
}

/// The timeline in the environment: the flat pattern's own item, nothing else.
pub(crate) fn timeline_item(ui: &mut egui::Ui) {
  let (rect, r) = ui.allocate_exact_size(vec2(26.0, 28.0), egui::Sense::click());
  let p = ui.painter();
  if r.hovered() {
    p.rect(rect, 2.0, theme::HOVER, Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
  }
  icons::draw_color(p, Rect::from_center_size(rect.center(), Vec2::splat(20.0)), Icon::FlatPattern);
  r.on_hover_text("Flat Pattern");
}
