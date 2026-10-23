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

/// What a value box takes, judged from the text it held before typing replaced it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Takes {
  Length,
  Angle,
  Plain,
}

fn takes(text: &str) -> Takes {
  if names_length_unit(text) {
    Takes::Length
  } else if fox_engine::expr::identifiers(text).iter().any(|i| i == "deg") {
    Takes::Angle
  } else {
    Takes::Plain
  }
}

/// Remembers whether a value box holds a length or an angle, from the text it had before typing
/// replaced it (see [`pin`]).
pub fn note(ctx: &egui::Context, id: egui::Id, before: &str) {
  ctx.data_mut(|d| d.insert_temp(field_key(id), takes(before)));
}

/// An angle typed as a bare number, as Fusion's angle boxes show it: in degrees at the angular
/// precision (`5` → `5.0 deg`). Anything else is kept as typed.
pub fn typed_angle(text: &str) -> String {
  match text.trim().parse::<f64>() {
    Ok(v) => format!("{v:.*} deg", angle_decimals()),
    Err(_) => text.to_string(),
  }
}

/// An angle as Fusion's angle boxes show it: a plain number of degrees at the angular precision
/// (`118 deg` → `118.0 deg`). An expression is kept as it is.
pub fn shown_angle(text: &str) -> String {
  let t = text.trim();
  match t.strip_suffix("deg").map(str::trim).and_then(|n| n.parse::<f64>().ok()) {
    Some(v) => format!("{v:.*} deg", angle_decimals()),
    None => text.to_string(),
  }
}

/// A dialog value box that took a length or an angle: when editing ends, a bare number typed in
/// a length box means the design's unit and is pinned to it (`2` → `2 in`), and one typed in an
/// angle box is degrees (`5` → `5.0 deg`), as Fusion fills in the unit. What the box takes is
/// judged from the value it held when editing began (`10 mm` a length, `30 deg` an angle, `6`
/// neither), so counts are left alone.
pub fn pin(ui: &egui::Ui, r: &mut egui::Response, text: &mut String) {
  let k = field_key(r.id);
  if r.gained_focus() && ui.data(|d| d.get_temp::<Takes>(k)).is_none() {
    note(ui.ctx(), r.id, text);
  }
  if r.lost_focus() {
    let pinned = match ui.data(|d| d.get_temp::<Takes>(k)) {
      Some(Takes::Length) => current(ui.ctx()).typed(text),
      Some(Takes::Angle) => typed_angle(text),
      _ => text.clone(),
    };
    if pinned != *text {
      *text = pinned;
      r.mark_changed();
    }
    ui.data_mut(|d| d.remove::<Takes>(k));
  }
}

/// Preferences → Unit and Value Display, as the readouts follow it.
#[derive(Clone, Copy)]
struct Display {
  /// General precision: the most decimals a length is shown with.
  general: u8,
  /// Angular precision, in decimals.
  angular: u8,
  /// Hide trailing zeroes: `Some` with the minimum precision the zeroes are trimmed down to.
  trim_to: Option<u8>,
}

thread_local! {
  /// The display preferences. The UI runs on one thread, which sets these at start and on OK or
  /// Apply; until then they are Fusion's defaults (0.123, 0.1, trailing zeroes hidden down to
  /// 4.00).
  static DISPLAY: std::cell::Cell<Display> =
    const { std::cell::Cell::new(Display { general: 3, angular: 1, trim_to: Some(2) }) };
}

/// Sets the precisions lengths and angles are shown with and whether trailing zeroes are hidden,
/// down to which precision (see [`number`]).
pub fn set_precision(general: u8, angular: u8, trim_to: Option<u8>) {
  DISPLAY.with(|p| p.set(Display { general, angular, trim_to }));
}

/// A number (already in the display unit) as Fusion's readouts show it: at the General precision
/// (three decimals by default, whatever the unit, as Fusion's 0.123),
/// trailing zeroes dropped down to the minimum precision while Hide trailing zeroes is on
/// (12.3456 → "12.346", 12.3 → "12.30", 10 → "10.00" at the defaults), never "-0.00".
pub fn number(v: f64) -> String {
  let d = DISPLAY.with(|p| p.get());
  let s = format!("{v:.*}", usize::from(d.general));
  let s = match (d.trim_to, s.find('.')) {
    (Some(min), Some(dot)) => {
      let keep = dot + 1 + usize::from(min.min(d.general));
      let trimmed = s.trim_end_matches('0').len().max(keep);
      let out = &s[..trimmed];
      out.strip_suffix('.').unwrap_or(out).to_string()
    }
    _ => s,
  };
  let zero = s.trim_start_matches('-').chars().all(|c| c == '0' || c == '.');
  if zero { s.trim_start_matches('-').to_string() } else { s }
}

/// A length (mm) in `unit` as the readouts show it, without the unit (see [`number`]).
pub fn length_number(unit: LengthUnit, mm: f64) -> String {
  number(unit.from_mm(mm))
}

/// A length (mm) in `unit` as the readouts show it, with the unit ("12.346 mm").
pub fn length_text(unit: LengthUnit, mm: f64) -> String {
  format!("{} {}", length_number(unit, mm), unit.symbol())
}

/// The decimals an angle is shown with (Preferences' Angular precision, one by default).
pub fn angle_decimals() -> usize {
  DISPLAY.with(|p| p.get().angular).into()
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
    assert_eq!(edit_box("30 deg", "45", LengthUnit::In), "45.0 deg", "an angle box's number is degrees");
    assert_eq!(edit_box("30 deg", "a1 * 2", LengthUnit::In), "a1 * 2", "an expression stays as typed");
    assert_eq!(edit_box("10 mm", "2", LengthUnit::Mm), "2", "bare numbers already mean mm");
  }

  /// Fusion's defaults (0.123, Hide trailing zeroes down to 4.00) read 12.3456 mm as "12.346",
  /// 12.3 as "12.30" and 10 as "10.00"; without hiding every decimal stays.
  #[test]
  fn lengths_trim_trailing_zeroes_down_to_the_minimum_precision() {
    set_precision(3, 1, Some(2));
    assert_eq!(length_number(LengthUnit::Mm, 12.3456), "12.346");
    assert_eq!(length_number(LengthUnit::Mm, 12.3), "12.30");
    assert_eq!(length_number(LengthUnit::Mm, 10.0), "10.00");
    assert_eq!(length_number(LengthUnit::Mm, -0.0001), "0.00");
    assert_eq!(length_text(LengthUnit::In, 12.5), "0.492 in");
    set_precision(3, 1, None);
    assert_eq!(length_number(LengthUnit::Mm, 10.0), "10.000", "zeroes kept");
    set_precision(4, 1, Some(0));
    assert_eq!(length_number(LengthUnit::Mm, 10.0), "10");
    assert_eq!(length_number(LengthUnit::Mm, 10.25), "10.25");
    set_precision(1, 1, Some(2));
    assert_eq!(length_number(LengthUnit::Mm, 10.0), "10.0", "never more than the precision");
    set_precision(3, 1, Some(2));
  }
}
