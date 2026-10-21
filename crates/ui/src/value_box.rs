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

pub(crate) fn id() -> egui::Id {
  egui::Id::new("floating-value-box")
}

/// The `n`th box's id (a dialog may show one beside each of its handles).
fn id_nth(n: usize) -> egui::Id {
  if n == 0 { id() } else { id().with(n) }
}

/// The box's layer: a focused box counts as the dialog's own for Enter = OK.
pub(crate) fn layer() -> egui::LayerId {
  egui::LayerId::new(egui::Order::Foreground, id())
}

/// Is `layer` one of the boxes' (see [`layer`])?
pub(crate) fn is_box_layer(layer: egui::LayerId) -> bool {
  (0..8).any(|n| layer == egui::LayerId::new(egui::Order::Foreground, id_nth(n)))
}

/// Whether the box has taken the keyboard since it appeared, and the text it showed last frame.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct State {
  focused: bool,
  /// The text the box showed last frame: when the field changes under the box (the arrow was
  /// dragged, the dialog's row was edited) the whole new value is selected again, so typing
  /// still replaces it all (a select-all range kept from a longer text left part of the new one).
  last: Option<String>,
  /// Where the box was pinned when it appeared by the pointer (see [`show_pinned`]).
  pinned: Option<Pos2>,
}

impl State {
  /// No box this frame: the next one to appear takes the keyboard again.
  pub fn hide(&mut self) {
    self.focused = false;
    self.last = None;
    self.pinned = None;
  }

  /// The box pinned where it first appeared (Sketch Offset's, by the pointer, while the offset
  /// still sits on its curve): `at` the first time, then the same place until it is hidden or
  /// [`State::unpin`] lets it go back to the arrow's head.
  pub fn pin(&mut self, at: Pos2) -> Pos2 {
    *self.pinned.get_or_insert(at)
  }

  /// The box goes back to the arrow's head.
  pub fn unpin(&mut self) {
    self.pinned = None;
  }
}

/// What the box did this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
  pub changed: bool,
  /// The box took the keyboard this frame: it just appeared (its value selected, so typing
  /// replaces it) or typing was aimed at it.
  pub focused: bool,
  /// Enter was pressed while the box had the keyboard (commit).
  pub enter: bool,
  /// Escape was pressed while the box had the keyboard.
  pub escape: bool,
}

/// Shows the box with its left edge at `at` (beside the arrow's head), editing `text`. `params`
/// are the user parameters its ⋮ menu offers.
pub(crate) fn show(ctx: &egui::Context, state: &mut State, at: Pos2, text: &mut String, params: &[String]) -> Shown {
  show_nth(ctx, state, 0, at, text, params)
}

/// [`show`] for the `n`th box of a dialog with several (one beside each handle): only the first
/// takes the keyboard when it appears. A whole-number box (a quantity) gets Fusion's spinner.
pub(crate) fn show_nth(
  ctx: &egui::Context,
  state: &mut State,
  n: usize,
  at: Pos2,
  text: &mut String,
  params: &[String],
) -> Shown {
  let mut out = Shown::default();
  egui::Area::new(id_nth(n)).fixed_pos(at).pivot(Align2::LEFT_CENTER).order(egui::Order::Foreground).show(ctx, |ui| {
    egui::Frame::new()
      .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 240))
      .stroke(Stroke::new(1.0, INK))
      .corner_radius(2.0)
      .inner_margin(egui::Margin::symmetric(2, 1))
      .show(ui, |ui| {
        ui.horizontal(|ui| {
          ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
          let edit_id = id_nth(n).with("edit");
          // the field changed under a box that has the keyboard: select the whole new value
          // before the edit box reads this frame's typing, so a digit replaces all of it
          if state.focused && state.last.as_deref().is_some_and(|l| l != text.as_str()) {
            crate::viewport::select_text(ctx, edit_id, text, true);
          }
          let mut r = ui.add(
            egui::TextEdit::singleline(text)
              .id(edit_id)
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
          } else if !state.focused && n == 0 {
            crate::viewport::focus_text(ctx, &r, text, true);
            state.focused = true;
            out.focused = true;
          }
          crate::units::pin(ui, &mut r, text);
          out.changed |= r.changed();
          if text.trim().parse::<u64>().is_ok()
            && let Some(step) = crate::dialogs::spinner(ui)
          {
            let v = text.trim().parse::<i64>().unwrap_or(1);
            *text = (v + step).max(1).to_string();
            out.changed = true;
          }
          state.last = Some(text.clone());
          if r.has_focus() || r.lost_focus() {
            let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            out.enter = enter;
            out.escape = esc;
          }
          // the ⋮ is painted: the UI font has no glyph for it
          let menu = ui.menu_button(" ", |ui| {
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
          crate::menus::options_dots(ui.painter(), menu.response.rect.center(), INK);
          if out.changed {
            state.last = Some(text.clone());
          }
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

  /// `ui:sketch:value-box-stale-selection`: the field changes under the focused box (the offset
  /// followed the pointer from "53.5 mm" to "-6 mm"); typing a digit still replaces the whole
  /// value, as in Fusion, instead of leaving the tail the old select-all range didn't cover.
  #[test]
  fn typing_after_the_value_changed_under_the_box_replaces_all_of_it() {
    let ctx = egui::Context::default();
    let mut state = State::default();
    let mut text = "53.5 mm".to_string();
    frame(&ctx, &mut state, &mut text, vec![]);
    frame(&ctx, &mut state, &mut text, vec![]);
    // the value changes between frames and a digit is typed in the next one
    text = "-6 mm".to_string();
    frame(&ctx, &mut state, &mut text, vec![]);
    let shown = frame(&ctx, &mut state, &mut text, vec![egui::Event::Text("3".into())]);
    assert!(shown.changed);
    assert_eq!(text, "3");
    // the value changes and the digit arrives in the same frame
    text = "-6 mm".to_string();
    frame(&ctx, &mut state, &mut text, vec![]);
    text = "12.25 mm".to_string();
    frame(&ctx, &mut state, &mut text, vec![]);
    text = "-6 mm".to_string();
    frame(&ctx, &mut state, &mut text, vec![egui::Event::Text("4".into())]);
    assert_eq!(text, "4");
    // typing on keeps going (the box's own edits don't reselect)
    frame(&ctx, &mut state, &mut text, vec![egui::Event::Text("5".into())]);
    assert_eq!(text, "45");
    // the pin holds the first place it is given until the box is hidden
    assert_eq!(state.pin(Pos2::new(10.0, 10.0)), Pos2::new(10.0, 10.0));
    assert_eq!(state.pin(Pos2::new(50.0, 50.0)), Pos2::new(10.0, 10.0));
    state.unpin();
    assert_eq!(state.pin(Pos2::new(50.0, 50.0)), Pos2::new(50.0, 50.0));
    state.hide();
    assert_eq!(state.pin(Pos2::new(1.0, 1.0)), Pos2::new(1.0, 1.0));
  }
}
