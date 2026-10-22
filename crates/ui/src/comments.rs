//! The COMMENTS panel at the bottom of the browser column, as Fusion's: collapsed to its header
//! (a + at the right) to start; the header or its + expands it into a docked panel, its header
//! now at the top with a - that collapses it, and the browser above shrinks to make room. An
//! unsaved design's panel only says to save it first; a saved one lists its comments with a +
//! to add one. Comments live with the session only; Fusion keeps them with the cloud document.

use egui::{Color32, RichText, Stroke, vec2};

use crate::{App, theme};

#[derive(Debug, Default)]
pub struct Comments {
  /// The panel is expanded above its header.
  pub open: bool,
  /// A comment being typed (the + was pressed).
  pub draft: Option<String>,
  /// Comments added in this session.
  pub posted: Vec<String>,
}

/// Width of the header and panel: the browser column's.
const WIDTH: f32 = 270.0;
/// Height of the header.
const HEADER_H: f32 = 26.0;
/// Height of the expanded panel's body, below its header.
pub const BODY_H: f32 = 220.0;

/// What the expanded panel says for a design that isn't saved, as Fusion's.
pub const UNSAVED_TEXT: &str = "Save this document to add comments, screenshots, and attachments.";

/// The panel's area id.
pub const AREA: &str = "comments";

/// How much of the browser column the expanded panel takes beyond its header.
pub fn extra_height(app: &App) -> f32 {
  if app.show_comments && app.comments.open { BODY_H } else { 0.0 }
}

/// Posts the draft, if it says anything.
pub fn post_draft(app: &mut App) {
  if let Some(d) = app.comments.draft.take() {
    let d = d.trim().to_string();
    if !d.is_empty() {
      app.comments.posted.push(d);
    }
  }
}

/// The header, and the panel's body below it while expanded.
pub fn show(app: &mut App, ctx: &egui::Context) {
  let canvas = app.canvas;
  egui::Area::new(egui::Id::new(AREA))
    .fixed_pos(canvas.left_bottom() + vec2(6.0, -6.0))
    .pivot(egui::Align2::LEFT_BOTTOM)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
      if header(ui, app.comments.open) {
        app.comments.open = !app.comments.open;
      }
      if app.comments.open {
        panel(app, ui);
      }
    });
}

/// The COMMENTS header: ◂◂ and the title at the left, + (collapsed) or - (expanded) at the
/// right. A click anywhere on it toggles the panel. Returns whether it was clicked.
fn header(ui: &mut egui::Ui, open: bool) -> bool {
  let (rect, resp) = ui.allocate_exact_size(vec2(WIDTH, HEADER_H), egui::Sense::click());
  let p = ui.painter();
  p.rect(rect, 2.0, theme::FLOAT_HEADER, Stroke::new(1.0, Color32::from_gray(212)), egui::StrokeKind::Inside);
  let c = rect.left_center() + vec2(12.0, 0.0);
  let ink = theme::GROUP_LABEL;
  for dx in [-3.0, 3.0] {
    let t = c + vec2(dx, 0.0);
    let pts = if open {
      vec![t + vec2(-3.0, 0.0), t + vec2(2.0, -4.0), t + vec2(2.0, 4.0)]
    } else {
      vec![t + vec2(3.0, 0.0), t + vec2(-2.0, -4.0), t + vec2(-2.0, 4.0)]
    };
    p.add(egui::Shape::convex_polygon(pts, ink, Stroke::NONE));
  }
  p.text(
    rect.left_center() + vec2(28.0, 0.0),
    egui::Align2::LEFT_CENTER,
    "COMMENTS",
    egui::FontId::proportional(12.0),
    ink,
  );
  let sign = rect.right_center() - vec2(16.0, 0.0);
  let sign_ink = if resp.hovered() { theme::ACCENT } else { ink };
  p.line_segment([sign - vec2(5.0, 0.0), sign + vec2(5.0, 0.0)], Stroke::new(1.6, sign_ink));
  if !open {
    p.line_segment([sign - vec2(0.0, 5.0), sign + vec2(0.0, 5.0)], Stroke::new(1.6, sign_ink));
  }
  resp.clicked()
}

fn panel(app: &mut App, ui: &mut egui::Ui) {
  let unsaved = crate::panels::is_unsaved(app);
  egui::Frame::new().fill(Color32::WHITE).stroke(Stroke::new(1.0, Color32::from_gray(212))).inner_margin(8.0).show(
    ui,
    |ui| {
      ui.set_width(WIDTH - 18.0);
      ui.set_height(BODY_H - 18.0);
      ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
      if unsaved {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| ui.label(RichText::new(UNSAVED_TEXT).color(theme::MUTED)));
        return;
      }
      ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          if ui.add(egui::Button::new("+").min_size(vec2(22.0, 22.0))).on_hover_text("Add a comment").clicked()
            && app.comments.draft.is_none()
          {
            app.comments.draft = Some(String::new());
          }
        });
      });
      if app.comments.posted.is_empty() && app.comments.draft.is_none() {
        ui.add_space(10.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("No comments").color(theme::MUTED)));
        ui.add_space(10.0);
      }
      egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        for c in &app.comments.posted {
          egui::Frame::new().fill(theme::DIALOG_BODY).inner_margin(6.0).corner_radius(2.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Local user").small().color(theme::MUTED));
            ui.label(c);
          });
        }
      });
      if let Some(draft) = app.comments.draft.as_mut() {
        let r = ui.add(egui::TextEdit::multiline(draft).desired_rows(2).hint_text("Add a comment"));
        r.request_focus();
        ui.horizontal(|ui| {
          if ui.button("Post").clicked() {
            post_draft(app);
          } else if ui.button("Cancel").clicked() {
            app.comments.draft = None;
          }
        });
      }
    },
  );
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;

  fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) {
    let time = ctx.input(|i| i.time) + 1.0 / 60.0;
    let input = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 800.0))),
      time: Some(time),
      events,
      ..Default::default()
    };
    ctx.run_ui(input, |ui| show(app, ui.ctx())).drop_without_applying_deltas();
  }

  /// The pointer moves to `at`, presses and releases over the next frames.
  fn click(app: &mut App, ctx: &egui::Context, at: egui::Pos2) {
    let button = |pressed| egui::Event::PointerButton {
      pos: at,
      button: egui::PointerButton::Primary,
      pressed,
      modifiers: Default::default(),
    };
    frame(app, ctx, vec![egui::Event::PointerMoved(at)]);
    frame(app, ctx, vec![button(true)]);
    frame(app, ctx, vec![button(false)]);
    frame(app, ctx, vec![]);
  }

  fn area(ctx: &egui::Context) -> egui::Rect {
    ctx.memory(|m| m.area_rect(egui::Id::new(AREA))).expect("drawn")
  }

  #[test]
  fn the_header_expands_a_docked_panel_and_collapses_it_again() {
    let mut app = App::with_session(Session::default());
    assert!(!app.comments.open, "collapsed to its header at first");
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    app.canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 800.0));
    frame(&mut app, &ctx, vec![]);
    frame(&mut app, &ctx, vec![]);
    let r = area(&ctx);
    assert!(r.height() < HEADER_H + 4.0, "just the header: {r:?}");
    assert_eq!(extra_height(&app), 0.0);
    // a click on the header's + (at its right) expands it
    click(&mut app, &ctx, egui::pos2(r.right() - 16.0, r.center().y));
    assert!(app.comments.open);
    frame(&mut app, &ctx, vec![]);
    let r = area(&ctx);
    assert!((r.height() - (HEADER_H + BODY_H)).abs() < 6.0, "header over the panel's body: {r:?}");
    assert!((r.bottom() - 794.0).abs() < 1.0 && (r.left() - 6.0).abs() < 1.0, "docked in the browser column: {r:?}");
    assert_eq!(extra_height(&app), BODY_H, "the browser gives up the room");
    // the header is now at the top; its - collapses the panel
    click(&mut app, &ctx, egui::pos2(r.right() - 16.0, r.top() + HEADER_H / 2.0));
    assert!(!app.comments.open);
  }

  #[test]
  fn posting_keeps_what_was_typed() {
    let mut app = App::with_session(Session::default());
    app.comments.draft = Some("  looks good  ".into());
    post_draft(&mut app);
    assert_eq!(app.comments.posted, ["looks good"]);
    assert_eq!(app.comments.draft, None);
    app.comments.draft = Some("   ".into());
    post_draft(&mut app);
    assert_eq!(app.comments.posted.len(), 1, "an empty comment isn't posted");
  }
}
