//! The floating value box beside a manipulator, as Fusion shows it: the dialog's primary value in
//! a small box at the arrow's head, with the keyboard, so typing goes straight into it and the
//! dialog's row mirrors it (both edit the same text, a typed bare number reaching the row with its
//! unit written out: `2` is `2 in`); Enter is the dialog's OK and Tab moves the box to the
//! command's next value (see `handles::tab`). Its ⋮ menu puts a user parameter in the box. Every
//! dialog with a drag arrow shows one (Extrude, Fillet, Chamfer, Press Pull, Offset Plane, Sketch
//! Offset): the arrow's drawing code (`viewport::manipulator`) asks the dialog which field the box
//! edits ([`crate::Dialog::value_box_field`]), so a dialog with another arrow or handle (Chamfer,
//! Shell) reuses it by naming its field there.

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
  layer == self::layer() || (1..8).any(|n| layer == egui::LayerId::new(egui::Order::Foreground, id_nth(n)))
}

/// Whether the box has taken the keyboard since it appeared, and the text it showed last frame.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct State {
  focused: bool,
  /// Each box's text (a dialog may show one beside each of its handles).
  bufs: Vec<Buf>,
  /// Where the box was pinned when it appeared by the pointer (see [`show_pinned`]).
  pinned: Option<Pos2>,
  /// The arrow is being dragged: the box follows its value with the text not selected, and
  /// selects it again on release.
  dragging: bool,
  /// Tab moved the box to the dialog's feature handle with this index (Extrude's taper handle);
  /// None: its first place (the drag arrow, else the first handle with a field).
  pub at_handle: Option<usize>,
}

/// One box's text.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Buf {
  /// The field's text last frame: when the field changes under the box (the arrow was dragged,
  /// the dialog's row was edited) the whole new value is selected again, so typing still
  /// replaces it all (a select-all range kept from a longer text left part of the new one).
  last: Option<String>,
  /// The box's own text: what was typed, while the field gets it with its unit written out
  /// (`2` in the box is `2 in` in the dialog's row, as in Fusion).
  shown: String,
  /// What the field takes, judged from its text when the box took it.
  takes: Takes,
}

/// What a box's field takes: a typed bare number gets its unit.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Takes {
  Length,
  Angle,
  /// A count or factor: left as typed, with Fusion's spinner when it is a whole number.
  #[default]
  Plain,
}

impl Takes {
  fn of(text: &str) -> Takes {
    if fox_engine::model::units::names_length_unit(text) {
      Takes::Length
    } else if fox_engine::expr::identifiers(text).iter().any(|i| i == "deg") {
      Takes::Angle
    } else {
      Takes::Plain
    }
  }

  /// The field's text for `typed`: a bare number with the field's unit written out.
  fn pin(self, unit: fox_engine::model::LengthUnit, typed: &str) -> String {
    match self {
      Takes::Length => unit.labelled(typed),
      Takes::Angle if typed.trim().parse::<f64>().is_ok() => format!("{} deg", typed.trim()),
      _ => typed.to_string(),
    }
  }
}

impl State {
  /// No box this frame: the next one to appear takes the keyboard again.
  pub fn hide(&mut self) {
    self.refocus();
    self.pinned = None;
    self.at_handle = None;
  }

  /// The box moves on (Tab): it takes its new field's value, selected.
  pub fn refocus(&mut self) {
    self.focused = false;
    self.bufs.clear();
    self.dragging = false;
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

  /// Is the arrow the box belongs to being dragged? While it is, the box shows the live value
  /// without selecting it; when the drag ends the value is selected again, as in Fusion.
  pub fn set_dragging(&mut self, dragging: bool) {
    if self.dragging
      && !dragging
      && let Some(b) = self.bufs.first_mut()
    {
      // reselect on the next frame
      b.last = None;
    }
    self.dragging = dragging;
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
  /// Tab was pressed while the box had the keyboard: it moves to the command's next value.
  pub tab: bool,
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
          // Tab moves the box to the command's next value (see `handles::tab`): taken before the
          // edit box runs, so egui's focus traversal doesn't also take the keyboard away
          if ctx.memory(|m| m.has_focus(edit_id))
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
          {
            ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
            out.tab = true;
          }
          if state.bufs.len() <= n {
            state.bufs.resize(n + 1, Buf::default());
          }
          let (focused, dragging) = (state.focused, state.dragging);
          let buf = &mut state.bufs[n];
          // the field changed under the box (or the box just appeared): show its value, selected
          // when the box has the keyboard, so a digit replaces all of it (not while the arrow is
          // dragged: Fusion shows the live value unselected then)
          if buf.last.as_deref() != Some(text.as_str()) {
            buf.shown = text.clone();
            buf.takes = Takes::of(text);
            if focused && ctx.memory(|m| m.has_focus(edit_id)) {
              crate::viewport::select_text(ctx, edit_id, &buf.shown, !dragging);
            }
          }
          let mut r = ui.add(
            egui::TextEdit::singleline(&mut buf.shown)
              .id(edit_id)
              .desired_width(62.0)
              .font(egui::FontId::proportional(12.0))
              .margin(egui::Margin::symmetric(3, 1)),
          );
          if let Some(typed) = crate::dialogs::take_typing(ctx) {
            buf.shown = typed;
            crate::viewport::focus_text(ctx, &r, &buf.shown, false);
            r.mark_changed();
            state.focused = true;
            out.focused = true;
          } else if !focused && n == 0 {
            crate::viewport::focus_text(ctx, &r, &buf.shown, !dragging);
            state.focused = true;
            out.focused = true;
          }
          let unit = crate::units::current(ctx);
          if r.changed() {
            *text = buf.takes.pin(unit, &buf.shown);
            out.changed = true;
          }
          // a quantity's box has Fusion's spinner; a length's never does
          if buf.takes == Takes::Plain
            && buf.shown.trim().parse::<u64>().is_ok()
            && let Some(step) = crate::dialogs::spinner(ui)
          {
            let v = buf.shown.trim().parse::<i64>().unwrap_or(1);
            buf.shown = (v + step).max(1).to_string();
            *text = buf.shown.clone();
            out.changed = true;
          }
          buf.last = Some(text.clone());
          if r.has_focus() || r.lost_focus() {
            let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            out.enter = enter;
            out.escape = esc;
          }
          // the ⋮ is painted: the UI font has no glyph for it
          let mut picked = false;
          let menu = ui.menu_button(" ", |ui| {
            if params.is_empty() {
              ui.add_enabled(false, egui::Label::new("No user parameters"));
            }
            for p in params {
              if ui.button(p).clicked() {
                *text = p.clone();
                picked = true;
                ui.close();
              }
            }
          });
          crate::menus::options_dots(ui.painter(), menu.response.rect.center(), INK);
          if picked {
            out.changed = true;
            state.bufs[n] = Buf { last: Some(text.clone()), shown: text.clone(), takes: Takes::Plain };
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
    assert_eq!(text, "5 mm", "with the unit written out");
    // typing aimed at the dialog (no box focused) lands in it too
    let focused = ctx.memory(|m| m.focused()).unwrap();
    ctx.memory_mut(|m| m.surrender_focus(focused));
    crate::dialogs::type_into(&ctx, "7".into());
    let shown = frame(&ctx, &mut state, &mut text, vec![]);
    assert!(shown.changed && shown.focused);
    assert_eq!(text, "7 mm");
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
    assert_eq!(text, "3 mm");
    // the value changes and the digit arrives in the same frame
    text = "-6 mm".to_string();
    frame(&ctx, &mut state, &mut text, vec![]);
    text = "12.25 mm".to_string();
    frame(&ctx, &mut state, &mut text, vec![]);
    text = "-6 mm".to_string();
    frame(&ctx, &mut state, &mut text, vec![egui::Event::Text("4".into())]);
    assert_eq!(text, "4 mm");
    // typing on keeps going (the box's own edits don't reselect)
    frame(&ctx, &mut state, &mut text, vec![egui::Event::Text("5".into())]);
    assert_eq!(text, "45 mm");
    // the pin holds the first place it is given until the box is hidden
    assert_eq!(state.pin(Pos2::new(10.0, 10.0)), Pos2::new(10.0, 10.0));
    assert_eq!(state.pin(Pos2::new(50.0, 50.0)), Pos2::new(10.0, 10.0));
    state.unpin();
    assert_eq!(state.pin(Pos2::new(50.0, 50.0)), Pos2::new(50.0, 50.0));
    state.hide();
    assert_eq!(state.pin(Pos2::new(1.0, 1.0)), Pos2::new(1.0, 1.0));
  }

  /// The text edit's selection now, as (start, end) characters.
  fn selection(ctx: &egui::Context) -> Option<(usize, usize)> {
    let state = egui::text_edit::TextEditState::load(ctx, id().with("edit"))?;
    let r = state.cursor.char_range()?;
    let (a, b) = (r.primary.index, r.secondary.index);
    let (a, b): (usize, usize) = (a.into(), b.into());
    Some((a.min(b), a.max(b)))
  }

  /// `ui:manip:typed-value-units`: a bare number typed into a length's box gets the unit written
  /// out in the field (the box itself keeps what was typed); an angle's gets "deg"; a count's
  /// stays a bare number.
  #[test]
  fn typed_numbers_get_their_fields_unit() {
    for (start, typed, field) in
      [("10 mm", "25", "25 mm"), ("0.0 deg", "5", "5 deg"), ("3", "4", "4"), ("10 mm", "w", "w")]
    {
      let ctx = egui::Context::default();
      let mut state = State::default();
      let mut text = start.to_string();
      frame(&ctx, &mut state, &mut text, vec![]);
      frame(&ctx, &mut state, &mut text, vec![]);
      frame(&ctx, &mut state, &mut text, vec![egui::Event::Text(typed.into())]);
      assert_eq!(text, field, "{start} + {typed}");
      assert_eq!(state.bufs[0].shown, typed, "the box shows what was typed");
      // and keeps it while the field holds the pinned value
      frame(&ctx, &mut state, &mut text, vec![]);
      assert_eq!(state.bufs[0].shown, typed);
    }
    // in inches, "2" is "2 in"
    let ctx = egui::Context::default();
    crate::units::publish(&ctx, fox_engine::model::LengthUnit::In);
    let mut state = State::default();
    let mut text = "0.5 in".to_string();
    frame(&ctx, &mut state, &mut text, vec![]);
    frame(&ctx, &mut state, &mut text, vec![]);
    frame(&ctx, &mut state, &mut text, vec![egui::Event::Text("2".into())]);
    assert_eq!(text, "2 in");
  }

  /// `ui:manip:drag-snap-readout`: while the arrow is dragged the box shows the live value with
  /// the text not selected; on release the value is selected again, so typing replaces it.
  #[test]
  fn the_value_is_unselected_while_the_arrow_is_dragged() {
    let ctx = egui::Context::default();
    let mut state = State::default();
    let mut text = "10 mm".to_string();
    frame(&ctx, &mut state, &mut text, vec![]);
    frame(&ctx, &mut state, &mut text, vec![]);
    assert_eq!(selection(&ctx), Some((0, 5)), "selected when it appears");
    state.set_dragging(true);
    text = "12.5 mm".into();
    frame(&ctx, &mut state, &mut text, vec![]);
    frame(&ctx, &mut state, &mut text, vec![]);
    assert_eq!(selection(&ctx), Some((7, 7)), "not selected while dragged");
    state.set_dragging(false);
    frame(&ctx, &mut state, &mut text, vec![]);
    frame(&ctx, &mut state, &mut text, vec![]);
    assert_eq!(selection(&ctx), Some((0, 7)), "selected again on release");
    frame(&ctx, &mut state, &mut text, vec![egui::Event::Text("3".into())]);
    assert_eq!(text, "3 mm");
  }

  /// `ui:manip:tab-cycles-boxes`: Tab in the box is reported (the dialog moves the box on) and
  /// doesn't take the keyboard away from it.
  #[test]
  fn tab_in_the_box_keeps_the_keyboard() {
    let ctx = egui::Context::default();
    let mut state = State::default();
    let mut text = "10 mm".to_string();
    frame(&ctx, &mut state, &mut text, vec![]);
    frame(&ctx, &mut state, &mut text, vec![]);
    let tab = egui::Event::Key {
      key: egui::Key::Tab,
      physical_key: None,
      pressed: true,
      repeat: false,
      modifiers: Default::default(),
    };
    let shown = frame(&ctx, &mut state, &mut text, vec![tab]);
    assert!(shown.tab);
    frame(&ctx, &mut state, &mut text, vec![]);
    assert!(ctx.memory(|m| m.has_focus(id().with("edit"))), "the box still has the keyboard");
  }
}
