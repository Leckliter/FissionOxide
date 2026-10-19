//! Colours and style, modelled on Autodesk Fusion's light theme: a grey application bar with a
//! wide document tab, a light-grey toolbar with workspace tabs and bold blue icons, a
//! near-white canvas with the browser rows floating directly on it, and light-grey command
//! dialogs.

use egui::{Color32, CornerRadius, Stroke};

/// Application bar (data panel, file, save, undo/redo, document tabs).
pub const APP_BAR: Color32 = Color32::from_rgb(222, 222, 222);
/// Toolbar (workspace switcher, tabs and tool groups).
pub const TOOLBAR: Color32 = Color32::from_rgb(243, 243, 243);
/// Background of the contextual SKETCH tab and the FINISH SKETCH group.
pub const CONTEXT_TAB: Color32 = Color32::from_rgb(214, 233, 250);
pub const PANEL: Color32 = Color32::from_rgb(255, 255, 255);
/// Header bars of the floating BROWSER and COMMENTS panels.
pub const FLOAT_HEADER: Color32 = Color32::from_rgb(236, 236, 236);
/// Grey pills behind browser row labels.
pub const PILL: Color32 = Color32::from_rgba_premultiplied(226, 226, 226, 225);
/// The dark pill of the browser's root component.
pub const PILL_ROOT: Color32 = Color32::from_rgb(96, 96, 96);
pub const BORDER: Color32 = Color32::from_rgb(208, 208, 208);
pub const SEPARATOR: Color32 = Color32::from_rgb(220, 220, 220);
/// Fusion blue: selection, active tab underline, OK button.
pub const ACCENT: Color32 = Color32::from_rgb(6, 150, 215);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(0, 119, 178);
/// Light blue behind hovered / selected rows and buttons.
pub const HOVER: Color32 = Color32::from_rgb(229, 241, 251);
pub const SELECTED: Color32 = Color32::from_rgb(204, 228, 247);
pub const ERROR: Color32 = Color32::from_rgb(214, 48, 39);
pub const WARNING: Color32 = Color32::from_rgb(232, 155, 0);
pub const TEXT: Color32 = Color32::from_rgb(40, 40, 40);
pub const MUTED: Color32 = Color32::from_rgb(120, 120, 120);
pub const GROUP_LABEL: Color32 = Color32::from_rgb(45, 45, 45);
/// Command dialog title bar and body.
pub const DIALOG_HEADER: Color32 = Color32::from_rgb(232, 232, 232);
pub const DIALOG_BODY: Color32 = Color32::from_rgb(247, 247, 247);
/// Timeline strip.
pub const TIMELINE: Color32 = Color32::from_rgb(238, 238, 238);

/// Icon palette: outlines, solid fills, the highlighted face, sketch curves and accents.
pub const ICON_INK: Color32 = Color32::from_rgb(52, 64, 82);
pub const ICON_BLUE: Color32 = Color32::from_rgb(30, 136, 229);
pub const ICON_ORANGE: Color32 = Color32::from_rgb(245, 150, 30);
pub const ICON_GREEN: Color32 = Color32::from_rgb(56, 172, 70);
pub const ICON_RED: Color32 = Color32::from_rgb(222, 56, 50);

pub fn apply(ctx: &egui::Context) {
  ctx.set_visuals(egui::Visuals::light());
  ctx.all_styles_mut(|style| {
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.interact_size.y = 22.0;
    style.spacing.menu_margin = egui::Margin::same(4);
    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles = [
      (TextStyle::Small, FontId::new(10.0, FontFamily::Proportional)),
      (TextStyle::Body, FontId::new(12.5, FontFamily::Proportional)),
      (TextStyle::Button, FontId::new(12.5, FontFamily::Proportional)),
      (TextStyle::Heading, FontId::new(15.0, FontFamily::Proportional)),
      (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
    ]
    .into();
    let v = &mut style.visuals;
    v.override_text_color = None;
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(3);
    v.menu_corner_radius = CornerRadius::same(2);
    v.window_shadow = egui::Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_black_alpha(45) };
    v.popup_shadow = egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(40) };
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = ACCENT;
    v.extreme_bg_color = Color32::WHITE;
    v.faint_bg_color = Color32::from_rgb(246, 246, 246);
    v.text_edit_bg_color = Some(Color32::WHITE);
    let w = &mut v.widgets;
    let r = CornerRadius::same(2);
    w.noninteractive.bg_stroke = Stroke::new(1.0, SEPARATOR);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.bg_fill = Color32::from_rgb(245, 245, 245);
    w.inactive.weak_bg_fill = Color32::from_rgb(245, 245, 245);
    w.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(196, 196, 196));
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.hovered.bg_fill = HOVER;
    w.hovered.weak_bg_fill = HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    w.hovered.fg_stroke = Stroke::new(1.0, Color32::BLACK);
    w.active.bg_fill = SELECTED;
    w.active.weak_bg_fill = SELECTED;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT_DARK);
    w.active.fg_stroke = Stroke::new(1.0, Color32::BLACK);
    w.open.bg_fill = SELECTED;
    w.open.weak_bg_fill = SELECTED;
    for s in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
      s.corner_radius = r;
      s.expansion = 0.0;
    }
  });
}
