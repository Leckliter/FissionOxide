//! The COMMENTS panel in the canvas's bottom-left corner, as Fusion's: its header collapses and
//! expands a panel above it that lists the design's comments ("No comments" until there are
//! some) with a + to add one. Comments live with the session only; Fusion keeps them with the
//! cloud document.

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

/// Width of the header and panel.
const WIDTH: f32 = 270.0;

/// Posts the draft, if it says anything.
pub fn post_draft(app: &mut App) {
  if let Some(d) = app.comments.draft.take() {
    let d = d.trim().to_string();
    if !d.is_empty() {
      app.comments.posted.push(d);
    }
  }
}

/// The header, and the panel above it while open.
pub fn show(app: &mut App, ctx: &egui::Context) {
  let canvas = app.canvas;
  egui::Area::new(egui::Id::new("comments"))
    .fixed_pos(canvas.left_bottom() + vec2(6.0, -6.0))
    .pivot(egui::Align2::LEFT_BOTTOM)
    .order(egui::Order::Middle)
    .show(ctx, |ui| {
      ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
      if app.comments.open {
        panel(app, ui);
      }
      let mut open = app.comments.open;
      crate::panels::float_header(ui, "COMMENTS", &mut open, WIDTH);
      app.comments.open = open;
    });
}

fn panel(app: &mut App, ui: &mut egui::Ui) {
  egui::Frame::new().fill(Color32::WHITE).stroke(Stroke::new(1.0, Color32::from_gray(212))).inner_margin(8.0).show(
    ui,
    |ui| {
      ui.set_width(WIDTH - 16.0);
      ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
      ui.horizontal(|ui| {
        ui.label(RichText::new("Comments").strong());
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

  #[test]
  fn the_header_keeps_its_open_state_and_the_plus_adds_a_comment() {
    let mut app = App::with_session(Session::default());
    assert!(!app.comments.open, "collapsed to its header at first");
    app.comments.open = true;
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let input = || egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 800.0))),
      ..Default::default()
    };
    app.canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 800.0));
    ctx.run_ui(input(), |ui| show(&mut app, ui.ctx())).drop_without_applying_deltas();
    assert!(app.comments.open, "a frame without a click keeps it open");
    let r = ctx.memory(|m| m.area_rect(egui::Id::new("comments"))).expect("drawn");
    assert!(r.height() > 60.0, "the panel is above the header: {r:?}");
    // typing a comment and posting it
    app.comments.draft = Some("  looks good  ".into());
    post_draft(&mut app);
    assert_eq!(app.comments.posted, ["looks good"]);
    assert_eq!(app.comments.draft, None);
    app.comments.draft = Some("   ".into());
    post_draft(&mut app);
    assert_eq!(app.comments.posted.len(), 1, "an empty comment isn't posted");
  }
}
