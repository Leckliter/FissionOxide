//! Review Warning, as in Fusion: a red badge at the canvas's lower right counts the failed
//! features, and the Error window lists them as a tree (Fillet1 > Compute Failed > what went
//! wrong) with a copy button and Close. The timeline's right-click menu opens the window for one
//! feature; the badge opens it for all of them.

use egui::{Color32, Rect, RichText, Stroke, vec2};
use fox_engine::model::FeatureId;
use fox_engine::review::{self, Review, Severity};

use crate::{App, theme};

/// The Error window: closed, or open for one feature or every failure.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) enum ErrorWindow {
  #[default]
  Closed,
  /// Every failed (and warned) feature: the canvas badge.
  All,
  /// One feature: the timeline's Review warning.
  Feature(FeatureId),
}

/// One row of the window's tree; `children` are drawn indented under it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node {
  pub label: String,
  pub children: Vec<Node>,
}

/// What the open window lists.
pub(crate) fn items(app: &App) -> Vec<Review> {
  match app.error_window {
    ErrorWindow::Closed => Vec::new(),
    ErrorWindow::All => review::review(&app.s, false),
    ErrorWindow::Feature(f) => review::review_feature(&app.s, f).into_iter().collect(),
  }
}

/// The tree the window draws: feature, then Compute Failed (or Compute Warning), then each line
/// of the message.
pub(crate) fn tree(items: &[Review]) -> Vec<Node> {
  items
    .iter()
    .map(|r| Node {
      label: r.name.clone(),
      children: vec![Node {
        label: r.severity.heading().to_string(),
        children: r.message.lines().map(|l| Node { label: l.to_string(), children: Vec::new() }).collect(),
      }],
    })
    .collect()
}

/// How many features failed: the canvas badge's count (warnings don't raise it).
pub(crate) fn failures(app: &App) -> usize {
  app.s.model.status.iter().filter(|s| s.error.is_some()).count()
}

/// Whether a timeline item's menu offers Review warning.
pub(crate) fn reviewable(app: &App, f: FeatureId) -> bool {
  app.s.model.status(f).is_some_and(|s| s.error.is_some() || s.warning.is_some())
}

/// The red badge at the canvas's lower right, while any feature failed. A click opens the Error
/// window for every failure.
pub fn badge(app: &mut App, ctx: &egui::Context) {
  let n = failures(app);
  if n == 0 {
    return;
  }
  let size = vec2(if n > 9 { 46.0 } else { 38.0 }, 26.0);
  let at = app.canvas.right_bottom() - vec2(size.x + 14.0, size.y + 14.0);
  egui::Area::new(egui::Id::new("error-badge")).fixed_pos(at).order(egui::Order::Middle).show(ctx, |ui| {
    let (r, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let fill = if resp.hovered() { Color32::from_rgb(200, 40, 34) } else { theme::ERROR };
    ui.painter().rect(r, 13.0, fill, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
    let dot = Rect::from_center_size(r.left_center() + vec2(13.0, 0.0), egui::Vec2::splat(16.0));
    paint_mark(ui.painter(), dot, Color32::WHITE, fill);
    ui.painter().text(
      r.right_center() - vec2(9.0, 0.0),
      egui::Align2::RIGHT_CENTER,
      n.to_string(),
      egui::FontId::proportional(13.0),
      Color32::WHITE,
    );
    let hint = if n == 1 { "1 feature failed: Review warning".to_string() } else { format!("{n} features failed") };
    if resp.on_hover_text(hint).clicked() {
      app.error_window = ErrorWindow::All;
    }
  });
}

/// The error mark: a filled circle with an exclamation point.
pub(crate) fn paint_mark(p: &egui::Painter, r: Rect, fill: Color32, ink: Color32) {
  let c = r.center();
  let rad = r.width().min(r.height()) / 2.0;
  p.circle_filled(c, rad, fill);
  let s = Stroke::new((rad * 0.28).max(1.4), ink);
  p.line_segment([c - vec2(0.0, rad * 0.55), c + vec2(0.0, rad * 0.15)], s);
  p.circle_filled(c + vec2(0.0, rad * 0.5), s.width * 0.6, ink);
}

/// The timeline menu's Review warning row, with its red mark. Returns whether it was clicked.
pub(crate) fn menu_row(ui: &mut egui::Ui) -> bool {
  let r = ui.add(egui::Button::new("       Review warning").min_size(vec2(170.0, 0.0)));
  let ir = Rect::from_center_size(r.rect.left_center() + vec2(13.0, 0.0), egui::Vec2::splat(14.0));
  paint_mark(ui.painter(), ir, theme::ERROR, Color32::WHITE);
  r.clicked()
}

/// The window's copy button: two overlapping sheets.
fn copy_button(ui: &mut egui::Ui) -> egui::Response {
  let (r, resp) = ui.allocate_exact_size(vec2(26.0, 24.0), egui::Sense::click());
  if resp.hovered() {
    ui.painter().rect(r, 2.0, theme::HOVER, Stroke::new(1.0, theme::SELECTED), egui::StrokeKind::Inside);
  }
  let ink = Stroke::new(1.2, theme::ICON_INK);
  let back = Rect::from_min_size(r.center() - vec2(6.0, 7.0), vec2(9.0, 11.0));
  let front = back.translate(vec2(3.5, 3.5));
  ui.painter().rect_stroke(back, 1.0, ink, egui::StrokeKind::Inside);
  ui.painter().rect(front, 1.0, Color32::WHITE, ink, egui::StrokeKind::Inside);
  resp.on_hover_text("Copy the messages")
}

/// The Error window.
pub fn window(app: &mut App, ctx: &egui::Context) {
  if app.error_window == ErrorWindow::Closed {
    return;
  }
  let list = items(app);
  if list.is_empty() {
    // fixed (or undone) while the window was open
    app.error_window = ErrorWindow::Closed;
    return;
  }
  let mut open = true;
  let mut close = false;
  egui::Window::new("Error")
    .open(&mut open)
    .collapsible(false)
    .resizable(true)
    .default_size([460.0, 220.0])
    .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
    .show(ctx, |ui| {
      egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
        for (node, r) in tree(&list).iter().zip(&list) {
          let color = if r.severity == Severity::Error { theme::ERROR } else { theme::WARNING };
          egui::CollapsingHeader::new(RichText::new(&node.label).strong())
            .id_salt(("error-feature", r.feature.0))
            .default_open(true)
            .show(ui, |ui| {
              for heading in &node.children {
                egui::CollapsingHeader::new(RichText::new(&heading.label).color(color))
                  .id_salt(("error-heading", r.feature.0))
                  .default_open(true)
                  .show(ui, |ui| {
                    for line in &heading.children {
                      ui.add(egui::Label::new(&line.label).wrap());
                    }
                  });
              }
            });
        }
      });
      ui.separator();
      ui.horizontal(|ui| {
        if copy_button(ui).clicked() {
          ctx.copy_text(review::report_text(&list));
          app.status = Some(("Copied the error messages".into(), false));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          if ui.button("Close").clicked() {
            close = true;
          }
        });
      });
    });
  if !open || close {
    app.error_window = ErrorWindow::Closed;
  }
}

#[cfg(test)]
mod tests {
  use fox_engine::Session;
  use serde_json::json;

  use super::*;

  /// A 10 mm box with every edge filleted by filletR, then filletR raised from 2 to 8 mm.
  fn failed_fillet() -> App {
    let mut app = App::with_session(Session::default());
    app.run("param.add", json!({"name": "filletR", "expr": "2 mm"}));
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    let body = app.s.model.bodies[0].id.0;
    let edges: Vec<_> = app.s.model.bodies[0].edges().iter().map(|e| json!({"body": body, "edge": e.id})).collect();
    app.run("fillet", json!({"edges": edges, "radius": "filletR"}));
    assert_eq!(failures(&app), 0);
    app.run("param.set", json!({"name": "filletR", "expr": "8 mm"}));
    app
  }

  #[test]
  fn the_error_window_lists_feature_compute_failed_and_the_message() {
    let mut app = failed_fillet();
    assert_eq!(failures(&app), 1, "the badge counts the failed fillet");
    let fillet = app.s.design.features.iter().find(|f| f.name == "Fillet1").map(|f| f.id).unwrap();
    assert!(reviewable(&app, fillet));
    assert!(!reviewable(&app, app.s.design.features[0].id), "the box computed");
    assert!(items(&app).is_empty(), "the window starts closed");

    // the timeline's Review warning: just that feature
    app.error_window = ErrorWindow::Feature(fillet);
    let t = tree(&items(&app));
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].label, "Fillet1");
    assert_eq!(t[0].children.len(), 1);
    assert_eq!(t[0].children[0].label, "Compute Failed");
    let msg = &t[0].children[0].children;
    assert!(msg[0].label.starts_with("The fillet/chamfer could not be created"), "{msg:?}");
    // the engine's own words follow
    let engine = app.s.model.status(fillet).and_then(|s| s.error.clone()).unwrap();
    assert!(msg.iter().any(|l| l.label.to_lowercase().contains(&engine.trim().to_lowercase())), "{msg:?}");

    // the badge: every failure
    app.error_window = ErrorWindow::All;
    assert_eq!(items(&app).len(), 1);

    // the copy button's text is the tree, indented
    let text = review::report_text(&items(&app));
    assert!(text.starts_with("Fillet1\n  Compute Failed\n    The fillet/chamfer"), "{text}");

    // fixed: the badge goes and nothing is left to review
    app.run("param.set", json!({"name": "filletR", "expr": "2 mm"}));
    assert_eq!(failures(&app), 0);
    assert!(items(&app).is_empty());
  }
}
