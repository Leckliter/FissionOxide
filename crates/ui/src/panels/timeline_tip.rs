//! A timeline item's tooltip, as in Fusion: a dark rounded panel above the item holding the
//! feature's icon and name, a divider and the name of the component that owns it ("(Unsaved)"
//! until the design is saved). Resting on an item shows it; dragging the marker or a feature
//! shows the one of the feature just left of the slot, following the pointer.

use egui::{Rect, RichText, Stroke, Vec2, vec2};

use crate::icons::{self, Icon};
use crate::theme;

/// What a timeline tooltip shows.
pub(crate) struct Tip {
  pub icon: Icon,
  pub name: String,
  /// The owning component's name.
  pub component: String,
  /// A failed or warned feature's message, or why a dragged feature can't go to the slot.
  pub note: Option<String>,
}

/// Fusion's tooltip panel: dark grey, rounded.
fn frame(ctx: &egui::Context) -> egui::Frame {
  egui::Frame::new()
    .fill(theme::TOOLTIP)
    .corner_radius(egui::CornerRadius::same(4))
    .inner_margin(egui::Margin::symmetric(8, 6))
    .shadow(ctx.global_style().visuals.popup_shadow)
}

fn contents(ui: &mut egui::Ui, tip: &Tip) {
  ui.spacing_mut().item_spacing.y = 5.0;
  ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing.x = 6.0;
    let (r, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
    icons::draw_color(ui.painter(), r, tip.icon);
    ui.label(RichText::new(&tip.name).color(theme::TOOLTIP_TITLE));
  });
  // the divider spans the panel
  let w = ui.min_rect().width().max(90.0);
  let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), egui::Sense::hover());
  ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(1.0, theme::TOOLTIP_MUTED));
  ui.label(RichText::new(&tip.component).color(theme::TOOLTIP_TEXT));
  if let Some(n) = &tip.note {
    ui.add(egui::Label::new(RichText::new(n).color(theme::TOOLTIP_TEXT)).wrap());
  }
}

/// Shows `tip` above the hovered item `resp` (after the usual tooltip delay).
pub(crate) fn on_hover(resp: &egui::Response, tip: &Tip) {
  let mut tooltip = egui::Tooltip::for_enabled(resp);
  tooltip.popup = tooltip.popup.anchor(resp.rect).align(egui::RectAlign::TOP).gap(6.0).frame(frame(&resp.ctx));
  tooltip.show(|ui| {
    ui.set_max_width(260.0);
    contents(ui, tip);
  });
}

/// Shows `tip` above `anchor` at once, for as long as it is called each frame (a drag's).
pub(crate) fn above(ctx: &egui::Context, anchor: Rect, tip: &Tip) {
  egui::Area::new(egui::Id::new("timeline-drag-tip"))
    .order(egui::Order::Tooltip)
    .interactable(false)
    .pivot(egui::Align2::CENTER_BOTTOM)
    .fixed_pos(anchor.center_top() - vec2(0.0, 6.0))
    .show(ctx, |ui| {
      frame(ctx).show(ui, |ui| {
        ui.set_max_width(260.0);
        contents(ui, tip);
      });
    });
}
