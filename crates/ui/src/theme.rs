//! Colours and style. The layout follows Autodesk Fusion's light theme (grey application bar,
//! light toolbar, browser rows floating on a near-white canvas, light command dialogs); the
//! colours are FissionOxide's own, from the brand tokens (`brand/tokens.json`, via `fox-brand`)
//! and shared with the icons and the logo: slate for text and lines, one blue accent for
//! selection and primary actions, and each command family's accent in its dialogs.

use egui::{Color32, CornerRadius, Stroke};
use fox_brand::{Rgb, app};

/// A brand colour as an egui colour.
pub const fn color(c: Rgb) -> Color32 {
  Color32::from_rgb(c.0, c.1, c.2)
}

/// Application bar (data panel, file, save, undo/redo, document tabs).
pub const APP_BAR: Color32 = color(app::APP_BAR);
/// Toolbar (workspace switcher, tabs and tool groups).
pub const TOOLBAR: Color32 = color(app::TOOLBAR);
/// Background of the contextual SKETCH tab and the FINISH SKETCH group.
pub const CONTEXT_TAB: Color32 = color(app::CONTEXT_TAB);
pub const PANEL: Color32 = color(app::PANEL);
/// Header bars of the floating BROWSER and COMMENTS panels.
pub const FLOAT_HEADER: Color32 = color(app::FLOAT_HEADER);
/// Grey pills behind browser row labels.
pub const PILL: Color32 = Color32::from_rgba_premultiplied(app::PILL.0, app::PILL.1, app::PILL.2, 230);
/// The dark pill of the browser's root component.
pub const PILL_ROOT: Color32 = color(app::PILL_ROOT);
pub const BORDER: Color32 = color(app::BORDER);
pub const SEPARATOR: Color32 = color(app::SEPARATOR);
/// The blue accent (the solid-modelling icon blue): selection, primary actions.
pub const ACCENT: Color32 = color(app::ACCENT);
/// The active toolbar tab: bold black text and underline, as in Fusion.
pub const TAB_ACTIVE: Color32 = Color32::BLACK;
pub const ACCENT_DARK: Color32 = color(app::ACCENT_DARK);
/// Light blue behind hovered / selected rows and buttons.
pub const HOVER: Color32 = color(app::HOVER);
pub const SELECTED: Color32 = color(app::SELECTED);
pub const ERROR: Color32 = color(app::ERROR);
pub const WARNING: Color32 = color(app::WARNING);
pub const TEXT: Color32 = color(app::TEXT);
pub const MUTED: Color32 = color(app::MUTED);
pub const GROUP_LABEL: Color32 = color(app::GROUP_LABEL);
/// Command dialog title bar and body.
pub const DIALOG_HEADER: Color32 = color(app::DIALOG_HEADER);
pub const DIALOG_BODY: Color32 = color(app::DIALOG_BODY);
/// Timeline strip.
pub const TIMELINE: Color32 = color(app::TIMELINE);
/// The piece Trim would remove, under the pointer: Fusion's dark blue / purple highlight.
pub const TRIM_PIECE: Color32 = color(fox_brand::PURPLE.deep);
/// A menu row's tooltip: Fusion's dark panel with white text.
pub const TOOLTIP: Color32 = color(fox_brand::neutral::GRAPHITE);
/// The ribbon tooltip on [`TOOLTIP`]: a white title, light grey paragraphs, a dimmer note.
pub const TOOLTIP_TITLE: Color32 = color(fox_brand::neutral::WHITE);
pub const TOOLTIP_TEXT: Color32 = color(fox_brand::neutral::BORDER);
pub const TOOLTIP_MUTED: Color32 = color(fox_brand::neutral::SILVER);
/// The expanded ribbon tooltip's illustration and its light grey frame.
pub const ILLUSTRATION: Color32 = color(fox_brand::neutral::CLOUD);
pub const ILLUSTRATION_FRAME: Color32 = color(fox_brand::neutral::BORDER);
/// A curve under the pointer with Extend or Break: Fusion's bold blue.
pub const MODIFY_HOVER: Color32 = color(fox_brand::BLUE.base);
/// The piece Extend will add, previewed in Fusion's light red.
pub const EXTEND_PIECE: Color32 = color(fox_brand::RED.light);
/// Break's red x at each point the curve will split at.
pub const BREAK_MARK: Color32 = color(fox_brand::RED.base);

/// Icon palette: outlines, solid fills, the highlighted face, sketch curves and accents.
pub const ICON_INK: Color32 = color(app::ICON_INK);
pub const ICON_BLUE: Color32 = color(fox_brand::BLUE.base);
pub const ICON_ORANGE: Color32 = color(fox_brand::ORANGE.base);
pub const ICON_GREEN: Color32 = color(fox_brand::GREEN.base);
pub const ICON_RED: Color32 = color(fox_brand::RED.base);
/// The FINISH BASE FEATURE group's ground, Fusion's green.
pub const BASE_FEATURE: Color32 = color(fox_brand::GREEN.light);

/// The light style (the default) and the dark one, for Preferences → General → Theme.
pub fn apply(ctx: &egui::Context) {
  ctx.options_mut(|o| o.fallback_theme = egui::Theme::Light);
  ctx.set_theme(egui::ThemePreference::Light);
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
  });
  ctx.style_mut_of(egui::Theme::Light, |style| {
    style.visuals = egui::Visuals::light();
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
  ctx.style_mut_of(egui::Theme::Dark, |style| {
    style.visuals = egui::Visuals::dark();
    let v = &mut style.visuals;
    v.panel_fill = Color32::from_rgb(43, 43, 43);
    v.window_fill = Color32::from_rgb(43, 43, 43);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(80, 80, 80));
    v.window_corner_radius = CornerRadius::same(3);
    v.menu_corner_radius = CornerRadius::same(2);
    v.window_shadow = egui::Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_black_alpha(120) };
    v.popup_shadow = egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(100) };
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = ACCENT;
    let w = &mut v.widgets;
    w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT_DARK);
    for s in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
      s.corner_radius = CornerRadius::same(2);
      s.expansion = 0.0;
    }
  });
}

/// Preferences → General → Theme: light, dark, or the device's.
pub fn set_theme(ctx: &egui::Context, theme: crate::preferences::Theme) {
  use crate::preferences::Theme;
  ctx.set_theme(match theme {
    Theme::Light => egui::ThemePreference::Light,
    Theme::Dark => egui::ThemePreference::Dark,
    Theme::MatchDevice => egui::ThemePreference::System,
  });
}

/// Preferences → General → Show tooltips: off, no hover text ever reaches its delay.
pub fn set_tooltips(ctx: &egui::Context, show: bool) {
  let default = egui::style::Interaction::default();
  ctx.all_styles_mut(|style| {
    let i = &mut style.interaction;
    (i.tooltip_delay, i.tooltip_grace_time) =
      if show { (default.tooltip_delay, default.tooltip_grace_time) } else { (1.0e6, 0.0) };
  });
}
