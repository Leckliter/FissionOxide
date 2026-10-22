//! Document units in the UI: the Units row of Document Settings, and the helpers that make value
//! boxes and read-outs follow the design's unit (see `fox_model::units`).
//!
//! The unit is published into egui's memory each frame so value boxes deep inside dialogs can
//! pin a bare number to it without being handed the session.

use egui::{Color32, Stroke, pos2, vec2};
use fox_engine::model::LengthUnit;
use fox_engine::model::units::names_length_unit;
use serde_json::json;

use crate::App;
use crate::theme;

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

/// The Units row's label, as Fusion's: the length unit, then the mass unit ("Units: in, ouncemass").
pub(crate) fn row_label(unit: LengthUnit) -> String {
  format!("Units: {}, {}", unit.symbol(), unit.mass_symbol())
}

/// The Units row of Document Settings: hovering shows a pencil that picks the design's unit.
pub fn settings_row(app: &mut App, ui: &mut egui::Ui, row: &egui::Response, depth: usize) {
  let unit = app.s.design.units;
  let popup_id = popup_id();
  let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
  let rect = row.rect;
  let x = rect.min.x + 8.0 + depth as f32 * 16.0 + 44.0;
  let label_w = ui.painter().layout_no_wrap(row_label(unit), egui::FontId::proportional(13.0), theme::TEXT).size().x;
  let pr = egui::Rect::from_center_size(pos2(x + label_w + 36.0, rect.center().y), vec2(20.0, 20.0));
  let pencil = ui.interact(pr, egui::Id::new("units-pencil"), egui::Sense::click());
  let hovered = row.hovered() || pencil.hovered() || open;
  if hovered {
    let p = ui.painter();
    if pencil.hovered() || open {
      p.rect_filled(pr, 2.0, theme::HOVER);
    }
    paint_pencil(p, pr.shrink(4.0), if pencil.hovered() { theme::ACCENT } else { theme::GROUP_LABEL });
  }
  let pencil = pencil.on_hover_text("Change Active Units");
  egui::Popup::menu(&pencil).id(popup_id).show(|ui| {
    ui.label(egui::RichText::new("Change Active Units").strong());
    ui.separator();
    for u in LengthUnit::ALL {
      if ui.radio(u == unit, format!("{} ({})", u.label(), u.symbol())).clicked() {
        if u != unit {
          app.run("design.units", json!({"unit": u.symbol()}));
        }
        ui.close();
      }
    }
  });
}

/// The Change Active Units popup's id (the Units row's context menu opens it too).
pub(crate) fn popup_id() -> egui::Id {
  egui::Id::new("units-popup")
}

/// A small pencil: a slanted body with a tip, as Fusion's edit glyph.
fn paint_pencil(p: &egui::Painter, r: egui::Rect, c: Color32) {
  let (tip, end) = (pos2(r.left() + 1.0, r.bottom() - 1.0), pos2(r.right() - 1.0, r.top() + 1.0));
  p.line_segment([tip + vec2(3.0, -3.0), end], Stroke::new(3.2, c));
  p.add(egui::Shape::convex_polygon(vec![tip, tip + vec2(1.2, -4.4), tip + vec2(4.4, -1.2)], c, Stroke::NONE));
}

#[cfg(test)]
mod tests {
  use super::*;

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
