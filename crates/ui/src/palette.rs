//! The SKETCH PALETTE's state: its Options checkboxes, and whether it is docked open, collapsed
//! into a vertical tab at the canvas's right edge, or floating out of that tab until pinned.
//! The palette itself is drawn by `panels::sketch_palette`.

use egui::{Rect, Stroke, pos2, vec2};
use fox_engine::scene::SketchShow;

use crate::{App, theme};

/// Where the palette is: docked open (its chevron collapses it), collapsed into the tab (the tab
/// floats it out), or floating out of the tab, which collapses again on a click elsewhere unless
/// its pin docks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dock {
  #[default]
  Open,
  Collapsed,
  Floating,
}

/// The Sketch Palette's options, as Fusion's palette starts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SketchPalette {
  /// Section the model at the sketch plane.
  pub slice: bool,
  /// Shade closed profiles.
  pub profile: bool,
  /// Draw sketch points.
  pub points: bool,
  /// Draw projected geometry.
  pub projected: bool,
  /// Draw construction geometry.
  pub construction: bool,
  /// 3D Sketch: out-of-plane input.
  pub sketch_3d: bool,
  /// Linetype ▸ Construction with nothing selected: new curves are drawn as construction (X
  /// toggles it, as in Fusion; it stays on after Escape).
  pub construction_linetype: bool,
  pub dock: Dock,
}

impl Default for SketchPalette {
  fn default() -> Self {
    SketchPalette {
      slice: false,
      profile: true,
      points: true,
      projected: true,
      construction: true,
      sketch_3d: false,
      construction_linetype: false,
      dock: Dock::Open,
    }
  }
}

impl SketchPalette {
  /// What the scene draws of the sketch being edited.
  pub fn show(&self) -> SketchShow {
    SketchShow {
      profiles: self.profile,
      points: self.points,
      projected: self.projected,
      construction: self.construction,
    }
  }

  /// The palette is drawn (open or floating), not just its tab.
  pub fn shown(&self) -> bool {
    self.dock != Dock::Collapsed
  }
}

/// The collapsed palette's tab: at the canvas's right edge, under the AUTODESK ASSISTANT tab.
pub(crate) fn tab_rect(canvas: Rect) -> Rect {
  Rect::from_min_max(canvas.right_top() + vec2(-26.0, 478.0), canvas.right_top() + vec2(-2.0, 478.0 + 176.0))
}

/// The vertical "SKETCH PALETTE" tab the collapsed palette docks into; clicking it floats the
/// palette out over the canvas.
pub fn palette_tab(app: &mut App, ctx: &egui::Context) {
  let rect = tab_rect(app.canvas);
  egui::Area::new(egui::Id::new("sketch-palette-tab")).fixed_pos(rect.min).order(egui::Order::Middle).show(ctx, |ui| {
    let (rect, resp) = ui.allocate_exact_size(rect.size(), egui::Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, if resp.hovered() { theme::HOVER } else { theme::FLOAT_HEADER });
    let ink = theme::GROUP_LABEL;
    for dy in [-3.0, 3.0] {
      let t = rect.center_top() + vec2(dy, 14.0);
      p.add(egui::Shape::convex_polygon(
        vec![t + vec2(-3.0, 0.0), t + vec2(2.0, -4.0), t + vec2(2.0, 4.0)],
        ink,
        Stroke::NONE,
      ));
    }
    let galley = p.layout_no_wrap("SKETCH PALETTE".into(), egui::FontId::proportional(12.0), ink);
    let w = galley.size().x;
    let pos = pos2(rect.center().x - galley.size().y / 2.0, rect.top() + 36.0 + w);
    p.add(egui::epaint::TextShape::new(pos, galley, ink).with_angle(-std::f32::consts::FRAC_PI_2));
    if resp.clicked() {
      app.palette.dock = Dock::Floating;
    }
  });
}

/// The controls at the right end of the palette's header: the chevron that collapses it into
/// its tab and, while it floats out of the tab, the pin that docks it open again.
pub(crate) fn header_controls(app: &mut App, ui: &mut egui::Ui, header: Rect) {
  let ink = theme::GROUP_LABEL;
  let chevron =
    Rect::from_min_max(pos2(header.right() - 24.0, header.top() + 4.0), header.right_bottom() - vec2(2.0, 4.0));
  let c = ui.interact(chevron, ui.id().with("palette-collapse"), egui::Sense::click());
  if c.hovered() {
    ui.painter().rect_filled(chevron, 2.0, theme::HOVER);
  }
  // the header painted the chevron already; repaint it over the hover tint
  for dx in [-14.0, -8.0] {
    let t = header.right_center() + vec2(dx, 0.0);
    ui.painter().add(egui::Shape::convex_polygon(
      vec![t + vec2(-2.0, -4.0), t + vec2(3.0, 0.0), t + vec2(-2.0, 4.0)],
      ink,
      Stroke::NONE,
    ));
  }
  if c.clicked() {
    app.palette.dock = Dock::Collapsed;
  }
  if app.palette.dock == Dock::Floating {
    let pin =
      Rect::from_min_max(pos2(chevron.left() - 22.0, chevron.top()), pos2(chevron.left() - 2.0, chevron.bottom()));
    let r = ui.interact(pin, ui.id().with("palette-pin"), egui::Sense::click());
    let p = ui.painter();
    if r.hovered() {
      p.rect_filled(pin, 2.0, theme::HOVER);
    }
    // a pushpin: the head, its collar and the needle, leaning right
    let c = pin.center();
    p.add(egui::Shape::convex_polygon(
      vec![c + vec2(-5.0, -5.0), c + vec2(0.0, -7.0), c + vec2(3.0, -2.0), c + vec2(-2.0, 0.0)],
      ink,
      Stroke::NONE,
    ));
    p.line_segment([c + vec2(-6.0, 1.0), c + vec2(3.0, 0.0)], Stroke::new(1.5, ink));
    p.line_segment([c + vec2(-1.5, 0.5), c + vec2(-5.0, 7.0)], Stroke::new(1.5, ink));
    if r.on_hover_text("Pin").clicked() {
      app.palette.dock = Dock::Open;
    }
  }
}

/// A floating palette slides back into its tab when the pointer presses anywhere else.
pub(crate) fn end_frame(app: &mut App, ctx: &egui::Context, palette: Option<Rect>) {
  if app.palette.dock != Dock::Floating {
    return;
  }
  let pressed = ctx.input(|i| i.pointer.any_pressed());
  let over = ctx.pointer_interact_pos().is_some_and(|p| palette.is_some_and(|r| r.expand(2.0).contains(p)));
  if pressed && !over {
    app.palette.dock = Dock::Collapsed;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_options_start_as_fusions_and_map_to_the_scene() {
    let p = SketchPalette::default();
    assert!(!p.slice && p.profile && p.points && p.projected && p.construction && !p.sketch_3d);
    assert_eq!(p.show(), SketchShow::default());
    let off = SketchPalette { profile: false, points: false, ..p };
    assert_eq!(off.show(), SketchShow { profiles: false, points: false, ..SketchShow::default() });
    assert!(p.shown() && SketchPalette { dock: Dock::Floating, ..p }.shown());
    assert!(!SketchPalette { dock: Dock::Collapsed, ..p }.shown());
  }

  #[test]
  fn the_tab_sits_at_the_canvas_edge_under_the_assistant_tab() {
    let canvas = Rect::from_min_size(pos2(0.0, 100.0), vec2(1000.0, 800.0));
    let t = tab_rect(canvas);
    assert_eq!(t.right(), 998.0);
    assert!(t.top() > 100.0 + 210.0 + 260.0, "under the AUTODESK ASSISTANT tab");
    assert_eq!(t.width(), 24.0);
  }
}
