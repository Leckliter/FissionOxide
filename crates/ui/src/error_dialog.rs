//! Fusion's Error window and the red badge that opens it: the badge sits at the status bar's
//! right end while a timeline feature fails; clicking it (or Review warning in a timeline item's
//! menu) opens a window titled "Error" with a tree of the failures: each feature's row (its icon
//! outlined in red) > "Compute Failed" > the message, a copy icon at the lower left and Close.

use egui::{Color32, RichText, Stroke, Vec2, pos2, vec2};
use fox_engine::model::FeatureId;

use crate::icons::{self, Icon};
use crate::{App, file_dialogs, panels, theme};

/// The window's name for [`file_dialogs::floating`] (and its area, for tests).
pub(crate) const NAME: &str = "error-dialog";
/// The badge's size.
const BADGE: f32 = 16.0;

/// One failed (or warned) timeline feature, as the Error window lists it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Failure {
  pub feature: FeatureId,
  pub name: String,
  pub icon: Icon,
  pub message: String,
  /// A failure (red), else a warning.
  pub error: bool,
}

/// The features that failed or warned at their last compute, in timeline order: those before
/// the marker and not suppressed (a suppressed or rolled-back feature isn't computed).
pub(crate) fn failures(app: &App) -> Vec<Failure> {
  let d = &app.s.design;
  d.features
    .iter()
    .take(d.active_len())
    .filter(|f| !f.suppressed)
    .filter_map(|f| {
      let st = app.s.model.status(f.id)?;
      let (message, error) = match (&st.error, &st.warning) {
        (Some(e), _) => (e.clone(), true),
        (None, Some(w)) => (w.clone(), false),
        _ => return None,
      };
      Some(Failure { feature: f.id, name: f.name.clone(), icon: panels::feature_icon(&f.kind), message, error })
    })
    .collect()
}

/// Whether a feature failed or warned (its menu offers Review warning).
pub(crate) fn reviewable(app: &App, id: FeatureId) -> bool {
  failures(app).iter().any(|f| f.feature == id)
}

/// The window's title: "Error" when anything failed, else "Warning".
pub(crate) fn title(list: &[Failure]) -> &'static str {
  if list.iter().any(|f| f.error) { "Error" } else { "Warning" }
}

/// The text the copy icon puts on the clipboard: each feature, its state and its message.
pub(crate) fn report(list: &[Failure]) -> String {
  list
    .iter()
    .map(|f| format!("{}\n  {}\n    {}", f.name, state_label(f.error), f.message))
    .collect::<Vec<_>>()
    .join("\n")
}

fn state_label(error: bool) -> &'static str {
  if error { "Compute Failed" } else { "Compute Warning" }
}

fn open_id() -> egui::Id {
  egui::Id::new("error-dialog-open")
}

/// Opens the Error window (the badge, Review warning).
pub(crate) fn open(ctx: &egui::Context) {
  ctx.data_mut(|d| d.insert_temp(open_id(), true));
}

pub(crate) fn is_open(ctx: &egui::Context) -> bool {
  ctx.data(|d| d.get_temp(open_id())).unwrap_or(false)
}

fn close(ctx: &egui::Context) {
  ctx.data_mut(|d| d.remove::<bool>(open_id()));
}

/// Fusion's error glyph: a red disc with a white X.
pub(crate) fn error_glyph(p: &egui::Painter, rect: egui::Rect) {
  p.circle_filled(rect.center(), rect.width() / 2.0, theme::ERROR);
  let k = rect.width() * 0.2;
  let c = rect.center();
  let s = Stroke::new(1.6, Color32::WHITE);
  p.line_segment([c + vec2(-k, -k), c + vec2(k, k)], s);
  p.line_segment([c + vec2(-k, k), c + vec2(k, -k)], s);
}

/// The red X badge at the right end of the status readout while a feature fails (left of the
/// warning icon when that shows too); a click opens the Error window.
pub fn badge(app: &mut App, ctx: &egui::Context) {
  // while any feature's error shows, as the error toast counts them
  if panels::failed_features(app).is_empty() {
    return;
  }
  let palette =
    app.s.edit_sketch.is_some().then(|| ctx.memory(|m| m.area_rect(egui::Id::new("sketch-palette")))).flatten();
  let mut at = panels::readout_anchor(app.canvas, palette);
  if app.warning_toast.is_some() {
    at.x -= BADGE + 6.0;
  }
  let rect = egui::Rect::from_min_size(at - Vec2::splat(BADGE), Vec2::splat(BADGE));
  egui::Area::new(egui::Id::new("error-badge")).fixed_pos(rect.min).order(egui::Order::Middle).show(ctx, |ui| {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(BADGE), egui::Sense::click());
    error_glyph(ui.painter(), r);
    if resp.on_hover_text("Errors in the design: click to review").clicked() {
      open(ctx);
    }
  });
}

/// The Error window, while open: the tree of failures, the copy icon and Close.
pub fn window(app: &mut App, ctx: &egui::Context) {
  if !is_open(ctx) {
    return;
  }
  let list = failures(app);
  if list.is_empty() {
    close(ctx);
    return;
  }
  let (done, x) = file_dialogs::floating(ctx, NAME, title(&list), 460.0, |ui| {
    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
      ui.spacing_mut().item_spacing.y = 4.0;
      for f in &list {
        tree(ui, f);
      }
    });
    ui.add_space(10.0);
    let mut done = false;
    ui.horizontal(|ui| {
      let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::click());
      if resp.hovered() {
        ui.painter().rect_filled(r, 2.0, theme::HOVER);
      }
      copy_glyph(ui.painter(), r.shrink(4.0));
      if resp.on_hover_text("Copy").clicked() {
        ui.ctx().copy_text(report(&list));
      }
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        done = ui.add(file_dialogs::white_button("Close")).clicked();
      });
    });
    done
  });
  if done || x {
    close(ctx);
  }
}

/// One feature's branch: its row (icon outlined in red for a failure), "Compute Failed" with the
/// red glyph (expandable), and the message. Each row is Fusion's: the expand arrow, the icon in
/// its own slot, then the label.
fn tree(ui: &mut egui::Ui, f: &Failure) {
  let (icon, error) = (f.icon, f.error);
  let ink = if error { theme::ERROR } else { theme::WARNING };
  let feature = ui.make_persistent_id(("error-feature", f.feature.0));
  egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), feature, true)
    .show_header(ui, |ui| {
      row(ui, 18.0, &f.name, |p, r| {
        let r = egui::Rect::from_center_size(r.center(), Vec2::splat(16.0));
        p.rect_stroke(r.expand(1.0), 2.0, Stroke::new(1.0, ink), egui::StrokeKind::Outside);
        icons::draw_color(p, r, icon);
      });
    })
    .body(|ui| {
      let state = ui.make_persistent_id(("error-state", f.feature.0));
      egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), state, true)
        .show_header(ui, |ui| {
          row(ui, 14.0, state_label(error), |p, r| state_glyph(p, r, error));
        })
        .body(|ui| {
          ui.horizontal_top(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::hover());
            state_glyph(ui.painter(), r, error);
            ui.add(egui::Label::new(RichText::new(&f.message).color(theme::TEXT)).wrap());
          });
        });
    });
}

/// A tree row's icon (a `size` square laid out before the text, never over it) and its label.
fn row(ui: &mut egui::Ui, size: f32, label: &str, icon: impl FnOnce(&egui::Painter, egui::Rect)) {
  ui.spacing_mut().item_spacing.x = 6.0;
  let (r, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
  icon(ui.painter(), r);
  ui.label(RichText::new(label).color(theme::TEXT));
}

fn state_glyph(p: &egui::Painter, r: egui::Rect, error: bool) {
  if error {
    error_glyph(p, r);
  } else {
    let tri = vec![r.center_top(), r.right_bottom(), r.left_bottom()];
    p.add(egui::Shape::convex_polygon(tri, theme::WARNING, Stroke::NONE));
  }
}

/// Two overlapping sheets, the copy icon.
fn copy_glyph(p: &egui::Painter, r: egui::Rect) {
  let s = Stroke::new(1.2, theme::ICON_INK);
  let w = r.width() * 0.68;
  let back = egui::Rect::from_min_size(r.min, vec2(w, w));
  let front = egui::Rect::from_min_size(pos2(r.max.x - w, r.max.y - w), vec2(w, w));
  p.rect_stroke(back, 1.0, s, egui::StrokeKind::Inside);
  p.rect_filled(front, 1.0, Color32::WHITE);
  p.rect_stroke(front, 1.0, s, egui::StrokeKind::Inside);
}
