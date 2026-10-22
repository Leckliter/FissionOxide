//! Dropdowns and toggles with icons, as Fusion's command dialogs draw their values: every value
//! of Extrude's Start, Direction, Extent Type, Wall Location and Operation dropdowns shows an
//! icon before its text, and its Type row is a pair of icon toggles (Solid / Thin).

use egui::{Rect, Vec2, vec2};

use crate::icons::{self, Icon};
use crate::theme;

/// The icon of an Extrude dropdown value, by the key the dialog stores for it.
pub(super) fn extrude_icon(key: &str) -> Option<Icon> {
  Some(match key {
    // Start
    "" => Icon::ProfilePlane,
    "offset" => Icon::StartOffset,
    "object" => Icon::StartObject,
    // Direction
    "one_side" => Icon::OneSide,
    "two_sides" => Icon::TwoSides,
    "symmetric" => Icon::SymmetricExtent,
    // Extent Type
    "distance" => Icon::ExtentDistance,
    "to_object" => Icon::ToObject,
    "all" => Icon::ThroughAll,
    // Wall Location
    "inside" => Icon::WallSide1,
    "outside" => Icon::WallSide2,
    "center" => Icon::WallCenter,
    // Operation
    "join" => Icon::OpJoin,
    "cut" => Icon::OpCut,
    "intersect" => Icon::OpIntersect,
    "new_body" => Icon::OpNewBody,
    "new_component" => Icon::OpNewComponent,
    _ => return None,
  })
}

/// The icon of a form drop-down's value (Rib's Extent Type, Thicken's Direction, Operation).
pub(super) fn form_icon(key: &str) -> Option<Icon> {
  match key {
    "to_next" => Some(Icon::ToNext),
    "depth" => Some(Icon::ExtentDistance),
    k => extrude_icon(k),
  }
}

/// Room left before a value's text for its icon.
const ICON_PAD: &str = "      ";
const ICON_SIZE: f32 = 16.0;

/// Paints `icon` in the left margin of a row drawn with [`ICON_PAD`] before its text.
fn paint_icon(ui: &egui::Ui, row: Rect, icon: Icon) {
  let at = Rect::from_center_size(row.left_center() + vec2(14.0, 0.0), Vec2::splat(ICON_SIZE));
  icons::draw_color(ui.painter(), at, icon);
}

/// A dropdown whose values carry icons; `icon_of` names the icon of a value's key (None draws
/// the text alone). Returns true when the choice changed.
pub(super) fn combo(
  ui: &mut egui::Ui,
  id: &str,
  items: &[(&'static str, &'static str)],
  cur: &mut &'static str,
  icon_of: fn(&str) -> Option<Icon>,
) -> bool {
  let label = items.iter().find(|(k, _)| k == cur).map(|(_, l)| *l).unwrap_or(cur);
  let shown = match icon_of(cur) {
    Some(_) => format!("{ICON_PAD}{label}"),
    None => label.to_string(),
  };
  let mut changed = false;
  let r = egui::ComboBox::from_id_salt(id).selected_text(shown).width(140.0).show_ui(ui, |ui| {
    for (k, l) in items {
      let text = if icon_of(k).is_some() { format!("{ICON_PAD}{l}") } else { l.to_string() };
      let r = ui.selectable_label(cur == k, text);
      if let Some(icon) = icon_of(k) {
        paint_icon(ui, r.rect, icon);
      }
      if r.clicked() && cur != k {
        *cur = k;
        changed = true;
      }
    }
  });
  if let Some(icon) = icon_of(cur) {
    paint_icon(ui, r.response.rect, icon);
  }
  changed
}

/// A row of icon toggles (one pressed at a time), each with a tooltip; `cur` is the pressed
/// one's index. Returns true when it changed.
pub(super) fn toggles(ui: &mut egui::Ui, items: &[(Icon, &str)], cur: &mut usize) -> bool {
  let mut changed = false;
  ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing.x = 2.0;
    for (i, (icon, hint)) in items.iter().enumerate() {
      let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 24.0), egui::Sense::click());
      let resp = resp.on_hover_text(*hint);
      let active = *cur == i;
      let (fill, stroke) = if active {
        (theme::SELECTED, theme::ACCENT)
      } else if resp.hovered() {
        (theme::HOVER, theme::SELECTED)
      } else {
        (egui::Color32::WHITE, theme::BORDER)
      };
      ui.painter().rect(rect, 2.0, fill, egui::Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
      icons::draw_color(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(18.0)), *icon);
      if resp.clicked() && !active {
        *cur = i;
        changed = true;
      }
    }
  });
  changed
}
