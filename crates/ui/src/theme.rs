//! Colours and style: light panels and a dark application bar, like Fusion.

use egui::Color32;

pub const APP_BAR: Color32 = Color32::from_rgb(38, 42, 50);
pub const APP_BAR_TEXT: Color32 = Color32::from_rgb(225, 228, 235);
pub const TOOLBAR: Color32 = Color32::from_rgb(244, 245, 247);
pub const PANEL: Color32 = Color32::from_rgb(250, 250, 251);
pub const BORDER: Color32 = Color32::from_rgb(214, 217, 222);
pub const ACCENT: Color32 = Color32::from_rgb(0, 120, 212);
pub const SKETCH_ACCENT: Color32 = Color32::from_rgb(40, 150, 70);
pub const ERROR: Color32 = Color32::from_rgb(200, 40, 40);
pub const WARNING: Color32 = Color32::from_rgb(200, 140, 0);
pub const MUTED: Color32 = Color32::from_rgb(120, 125, 135);
pub const GROUP_LABEL: Color32 = Color32::from_rgb(90, 96, 108);

pub fn apply(ctx: &egui::Context) {
  ctx.set_visuals(egui::Visuals::light());
  ctx.all_styles_mut(|style| {
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(6.0, 3.0);
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);
    style.visuals.window_corner_radius = 6.0.into();
    style.visuals.panel_fill = PANEL;
  });
}
