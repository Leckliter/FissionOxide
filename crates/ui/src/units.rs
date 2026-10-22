//! Document units in the UI: the helpers that make value boxes and read-outs follow the design's
//! unit (see `fox_model::units`). The Units row's dialog is in `crate::doc_dialogs`.
//!
//! The unit is published into egui's memory each frame so value boxes deep inside dialogs can
//! pin a bare number to it without being handed the session.

use fox_engine::model::LengthUnit;
use fox_engine::model::units::names_length_unit;

fn key() -> egui::Id {
  egui::Id::new("doc-units")
}

/// Makes the design's unit known to this frame's value boxes.
pub fn publish(ctx: &egui::Context, unit: LengthUnit) {
  ctx.data_mut(|d| d.insert_temp(key(), unit));
}

/// The design's unit (mm until published).
pub fn current(ctx: &egui::Context) -> LengthUnit {
  ctx.data(|d| d.get_temp::<LengthUnit>(key())).unwrap_or_default()
}

fn field_key(id: egui::Id) -> egui::Id {
  id.with("takes-length")
}

/// Remembers whether a value box holds a length, from the text it had before typing replaced it
/// (see [`pin`]).
pub fn note(ctx: &egui::Context, id: egui::Id, before: &str) {
  ctx.data_mut(|d| d.insert_temp(field_key(id), names_length_unit(before)));
}

/// A dialog value box that took a length: when editing ends, a bare number typed in it means the
/// design's unit and is pinned to it (`2` → `2 in`), as Fusion fills in the unit. Whether the box
/// takes a length is judged from the value it held when editing began (`10 mm` does, `6` or
/// `30 deg` don't), so counts and angles are left alone.
pub fn pin(ui: &egui::Ui, r: &mut egui::Response, text: &mut String) {
  let k = field_key(r.id);
  if r.gained_focus() && ui.data(|d| d.get_temp::<bool>(k)).is_none() {
    note(ui.ctx(), r.id, text);
  }
  if r.lost_focus() {
    if ui.data(|d| d.get_temp::<bool>(k)) == Some(true) {
      let pinned = current(ui.ctx()).typed(text);
      if pinned != *text {
        *text = pinned;
        r.mark_changed();
      }
    }
    ui.data_mut(|d| d.remove::<bool>(k));
  }
}

thread_local! {
  /// Preferences → Unit and Value Display: the general precision (`None`: the unit's own) and the
  /// angular precision, in decimals. The UI runs on one thread, which sets these at start and on
  /// OK or Apply.
  static PRECISION: std::cell::Cell<(Option<u8>, u8)> = const { std::cell::Cell::new((None, 1)) };
}

/// Sets the precisions lengths and angles are shown with (see [`length_decimals`]).
pub fn set_precision(general: Option<u8>, angular: u8) {
  PRECISION.with(|p| p.set((general, angular)));
}

/// The decimals a length in `unit` is shown with: Preferences' General precision, else Fusion's
/// default display precision for the unit (Document Settings → Units).
pub fn length_decimals(unit: LengthUnit) -> usize {
  PRECISION.with(|p| p.get().0).map_or_else(|| unit_decimals(unit), usize::from)
}

/// Fusion's default display precision for a unit: two decimals in millimetres and centimetres,
/// three in inches, four in metres and feet.
pub fn unit_decimals(unit: LengthUnit) -> usize {
  match unit {
    LengthUnit::Mm | LengthUnit::Cm => 2,
    LengthUnit::In => 3,
    LengthUnit::M | LengthUnit::Ft => 4,
  }
}

/// The decimals an angle is shown with (Preferences' Angular precision, one by default).
pub fn angle_decimals() -> usize {
  PRECISION.with(|p| p.get().1).into()
}

/// The Units row's label, as Fusion's: the length unit, then the mass unit ("Units: in, ouncemass").
pub(crate) fn row_label(unit: LengthUnit) -> String {
  format!("Units: {}, {}", unit.symbol(), unit.mass_symbol())
}

#[cfg(test)]
mod tests {
  use super::*;
  use egui::vec2;

  #[test]
  fn pinning_follows_the_published_unit() {
    let ctx = egui::Context::default();
    publish(&ctx, LengthUnit::In);
    assert_eq!(current(&ctx), LengthUnit::In);
    publish(&ctx, LengthUnit::Mm);
    assert_eq!(current(&ctx), LengthUnit::Mm);
  }

  /// Edits a box through focus → typing → focus leaving, returning its final text.
  fn edit_box(start: &str, typed: &str, unit: LengthUnit) -> String {
    let ctx = egui::Context::default();
    let mut text = start.to_string();
    let id = egui::Id::new("box");
    for step in 0..4 {
      let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 300.0))),
        ..Default::default()
      };
      let mut out = ctx.run_ui(raw, |ui| {
        publish(ui.ctx(), unit);
        if step == 2 {
          text = typed.to_string();
        }
        let mut r = ui.add(egui::TextEdit::singleline(&mut text).id(id));
        match step {
          0 => r.request_focus(),
          2 => r.surrender_focus(),
          _ => {}
        }
        pin(ui, &mut r, &mut text);
      });
      out.textures_delta.clear();
    }
    text
  }

  #[test]
  fn a_length_box_pins_bare_numbers_to_the_unit() {
    assert_eq!(edit_box("10 mm", "2", LengthUnit::In), "2 in");
    assert_eq!(edit_box("10 mm", "2 mm", LengthUnit::In), "2 mm");
    assert_eq!(edit_box("6", "8", LengthUnit::In), "8", "a count box keeps bare numbers");
    assert_eq!(edit_box("30 deg", "45", LengthUnit::In), "45", "an angle box too");
    assert_eq!(edit_box("10 mm", "2", LengthUnit::Mm), "2", "bare numbers already mean mm");
  }
}
