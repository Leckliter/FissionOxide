//! The floating value box beside a manipulator, as Fusion shows it: the dialog's primary value in
//! a small box at the arrow's head, with the keyboard, so typing goes straight into it and the
//! dialog's row mirrors it (both edit the same text); Enter is the dialog's OK. Its ⋮ menu puts a
//! user parameter in the box. Every dialog with a drag arrow shows one (Extrude, Fillet, Chamfer,
//! Press Pull, Offset Plane, Sketch Offset): the arrow's drawing code (`viewport::manipulator`)
//! asks the dialog which field the box edits ([`crate::Dialog::value_box_field`]), so a dialog
//! with another arrow or handle (Chamfer, Shell) reuses it by naming its field there.

use egui::{Align2, Color32, Pos2, Stroke, Vec2};

/// The box's border and the ⋮ glyph, Fusion's manipulator blue.
const INK: Color32 = Color32::from_rgb(30, 110, 235);

fn id() -> egui::Id {
  egui::Id::new("floating-value-box")
}

/// The box's layer: a focused box counts as the dialog's own for Enter = OK.
pub(crate) fn layer() -> egui::LayerId {
  egui::LayerId::new(egui::Order::Foreground, id())
}

/// Whether the box has taken the keyboard since it appeared.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct State {
  focused: bool,
}

impl State {
  /// No box this frame: the next one to appear takes the keyboard again.
  pub fn hide(&mut self) {
    self.focused = false;
  }
}

/// What the box did this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
  pub changed: bool,
  /// The box took the keyboard this frame: it just appeared (its value selected, so typing
  /// replaces it) or typing was aimed at it.
  pub focused: bool,
}

/// Shows the box with its left edge at `at` (beside the arrow's head), editing `text`. `params`
/// are the user parameters its ⋮ menu offers.
pub(crate) fn show(ctx: &egui::Context, state: &mut State, at: Pos2, text: &mut String, params: &[String]) -> Shown {
  let mut out = Shown::default();
  egui::Area::new(id()).fixed_pos(at).pivot(Align2::LEFT_CENTER).order(egui::Order::Foreground).show(ctx, |ui| {
    egui::Frame::new()
      .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 240))
      .stroke(Stroke::new(1.0, INK))
      .corner_radius(2.0)
      .inner_margin(egui::Margin::symmetric(2, 1))
      .show(ui, |ui| {
        ui.horizontal(|ui| {
          ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
          let mut r = ui.add(
            egui::TextEdit::singleline(text)
              .desired_width(62.0)
              .font(egui::FontId::proportional(12.0))
              .margin(egui::Margin::symmetric(3, 1)),
          );
          if let Some(typed) = crate::dialogs::take_typing(ctx) {
            // what the box held tells whether it takes a length (see [`crate::units::pin`])
            crate::units::note(ctx, r.id, text);
            *text = typed;
            crate::viewport::focus_text(ctx, &r, text, false);
            r.mark_changed();
            state.focused = true;
            out.focused = true;
          } else if !state.focused {
            crate::viewport::focus_text(ctx, &r, text, true);
            state.focused = true;
            out.focused = true;
          }
          crate::units::pin(ui, &mut r, text);
          out.changed |= r.changed();
          ui.menu_button(egui::RichText::new("⋮").color(INK), |ui| {
            if params.is_empty() {
              ui.add_enabled(false, egui::Label::new("No user parameters"));
            }
            for p in params {
              if ui.button(p).clicked() {
                *text = p.clone();
                out.changed = true;
                ui.close();
              }
            }
          });
        });
      });
  });
  out
}

/// The user parameters a value box's menu offers, by name.
pub(crate) fn params_of(s: &fox_engine::Session) -> Vec<String> {
  let mut names: Vec<String> = s.model.params.keys().cloned().collect();
  names.sort();
  names
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Runs one frame showing the box, returning what it did and the text after it.
  fn frame(ctx: &egui::Context, state: &mut State, text: &mut String, events: Vec<egui::Event>) -> Shown {
    let input = egui::RawInput {
      screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
      events,
      ..Default::default()
    };
    let mut shown = Shown::default();
    ctx
      .run_ui(input, |ui| shown = show(ui.ctx(), state, Pos2::new(200.0, 200.0), text, &[]))
      .drop_without_applying_deltas();
    shown
  }

  #[test]
  fn the_box_takes_the_keyboard_once_and_typing_replaces_its_value() {
    let ctx = egui::Context::default();
    let mut state = State::default();
    let mut text = "10 mm".to_string();
    let shown = frame(&ctx, &mut state, &mut text, vec![]);
    assert!(shown.focused && !shown.changed);
    assert!(ctx.memory(|m| m.focused()).is_some(), "the box has the keyboard");
    // a second frame leaves the keyboard where it is
    let shown = frame(&ctx, &mut state, &mut text, vec![]);
    assert!(!shown.focused);
    // the value was selected: typing replaces it
    let shown = frame(&ctx, &mut state, &mut text, vec![egui::Event::Text("5".into())]);
    assert!(shown.changed, "typing changes the text");
    assert_eq!(text, "5");
    // typing aimed at the dialog (no box focused) lands in it too
    let focused = ctx.memory(|m| m.focused()).unwrap();
    ctx.memory_mut(|m| m.surrender_focus(focused));
    crate::dialogs::type_into(&ctx, "7".into());
    let shown = frame(&ctx, &mut state, &mut text, vec![]);
    assert!(shown.changed && shown.focused);
    assert_eq!(text, "7");
    // once hidden, the next box to appear takes the keyboard again
    state.hide();
    let shown = frame(&ctx, &mut state, &mut text, vec![]);
    assert!(shown.focused);
    // the focused box is on its own layer, which the dialog counts as its own for Enter
    let id = ctx.memory(|m| m.focused()).unwrap();
    assert_eq!(ctx.read_response(id).map(|r| r.layer_id), Some(layer()));
  }
}
