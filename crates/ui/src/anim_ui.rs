//! The Animation workspace, laid out as Fusion's: the ANIMATION tab's commands, the ANIMATION
//! TIMELINE with its storyboards (in place of the design timeline), and the docked NEW
//! STORYBOARD and TRANSFORM COMPONENTS dialogs. The browser's Components-only tree is
//! `panels::animation_tree`.
//!
//! Storyboards hold no actions yet: Transform Components picks components but records no move,
//! and the explode, callout, view and publish commands are listed but not run (see
//! `menus::ANIMATION`).

use egui::{Color32, Pos2, Rect, RichText, Stroke, pos2, vec2};

use crate::icons::{self, Icon};
use crate::{App, panels, theme};

/// The ANIMATION tab's commands FissionOxide runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimCmd {
  NewStoryboard,
  TransformComponents,
  ShowHide,
}

impl AnimCmd {
  pub(crate) fn info(self) -> (Icon, &'static str, &'static str) {
    match self {
      AnimCmd::NewStoryboard => (Icon::Storyboard, "New Storyboard", "Adds a storyboard to the animation"),
      AnimCmd::TransformComponents => {
        (Icon::TransformComponents, "Transform Components", "Moves and rotates components over time (M)")
      }
      AnimCmd::ShowHide => (Icon::Eye, "Show/Hide", "Shows or hides the selected components"),
    }
  }

  /// Shown pressed: its dialog is open.
  pub(crate) fn active(self, app: &App) -> bool {
    match self {
      AnimCmd::NewStoryboard => matches!(app.anim.dialog, Some(AnimDialog::NewStoryboard { .. })),
      AnimCmd::TransformComponents => app.anim.dialog == Some(AnimDialog::TransformComponents),
      AnimCmd::ShowHide => false,
    }
  }

  pub(crate) fn run(self, app: &mut App) {
    match self {
      AnimCmd::NewStoryboard => open(app, AnimDialog::NewStoryboard { kind: StoryboardType::Clean }),
      AnimCmd::TransformComponents => open(app, AnimDialog::TransformComponents),
      AnimCmd::ShowHide => app.toggle_visibility(),
    }
  }
}

/// New Storyboard's Storyboard Type: start from the assembled design, or from where the
/// previous storyboard ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryboardType {
  Clean,
  StartFromEnd,
}

impl StoryboardType {
  pub const ALL: [StoryboardType; 2] = [StoryboardType::Clean, StoryboardType::StartFromEnd];

  pub fn label(self) -> &'static str {
    match self {
      StoryboardType::Clean => "Clean",
      StoryboardType::StartFromEnd => "Start from end of previous",
    }
  }
}

/// The open Animation dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimDialog {
  NewStoryboard { kind: StoryboardType },
  TransformComponents,
}

impl AnimDialog {
  pub fn title(self) -> &'static str {
    match self {
      AnimDialog::NewStoryboard { .. } => "NEW STORYBOARD",
      AnimDialog::TransformComponents => "TRANSFORM COMPONENTS",
    }
  }

  fn icon(self) -> Icon {
    match self {
      AnimDialog::NewStoryboard { .. } => Icon::Storyboard,
      AnimDialog::TransformComponents => Icon::TransformComponents,
    }
  }
}

/// A storyboard: its name and how it starts.
#[derive(Debug, Clone, PartialEq)]
pub struct Storyboard {
  pub name: String,
  pub kind: StoryboardType,
}

/// The ANIMATION TIMELINE's ruler: Fusion's shows 0 to 24 seconds at the start.
pub const RULER_SECONDS: f32 = 24.0;
/// The timeline zoom's range (the slider and its - and +).
pub const ZOOM: std::ops::RangeInclusive<f32> = 0.25..=4.0;

/// The Animation workspace's state.
pub struct AnimUi {
  /// In the Animation workspace.
  pub active: bool,
  /// The storyboards, as the timeline's tab bar lists them; one to start, as in Fusion.
  pub storyboards: Vec<Storyboard>,
  /// The storyboard shown.
  pub current: usize,
  /// The playhead, in seconds.
  pub playhead: f32,
  /// Playing: the time of the last frame.
  pub playing: Option<f64>,
  /// The ANIMATION TIMELINE panel expanded (its header's arrow).
  pub timeline_open: bool,
  /// The timeline maximised (its full-screen icon).
  pub full: bool,
  /// The ruler's zoom (1 shows 0..24 s).
  pub zoom: f32,
  pub dialog: Option<AnimDialog>,
}

impl Default for AnimUi {
  fn default() -> Self {
    AnimUi {
      active: false,
      storyboards: vec![Storyboard { name: "Storyboard1".into(), kind: StoryboardType::Clean }],
      current: 0,
      playhead: 0.0,
      playing: None,
      timeline_open: true,
      full: false,
      zoom: 1.0,
      dialog: None,
    }
  }
}

impl AnimUi {
  /// The name the next storyboard gets: Storyboard1, Storyboard2, … past the highest in use.
  pub fn next_name(&self) -> String {
    let n = self
      .storyboards
      .iter()
      .filter_map(|s| s.name.strip_prefix("Storyboard")?.parse::<usize>().ok())
      .max()
      .unwrap_or(0);
    format!("Storyboard{}", n + 1)
  }

  /// The length of the current storyboard in seconds: the end of its last action (none yet).
  pub fn length(&self) -> f32 {
    0.0
  }
}

/// Opens `d`, cancelling a command dialog that was open as starting any command does.
pub fn open(app: &mut App, d: AnimDialog) {
  app.cancel_dialog();
  crate::doc_dialogs::close(app);
  app.anim.dialog = Some(d);
}

/// Closes the dialog without applying it (Cancel, Escape). Returns whether one was open.
pub fn close(app: &mut App) -> bool {
  app.anim.dialog.take().is_some()
}

/// Whether OK is enabled: New Storyboard's always, Transform Components' once a component is
/// picked (greyed with Components at "Select", as in Fusion).
pub fn can_ok(app: &App) -> bool {
  match app.anim.dialog {
    Some(AnimDialog::NewStoryboard { .. }) => true,
    Some(AnimDialog::TransformComponents) => picked(app) > 0,
    None => false,
  }
}

/// How many components Transform Components has: the components whose bodies are selected (a
/// body of the root counts as its own).
pub fn picked(app: &App) -> usize {
  // (is a component, its id or the body's)
  let mut owners: Vec<(bool, u32)> = Vec::new();
  for s in &app.s.selection {
    if let fox_engine::Sel::Body { body } = s {
      let owner = match app.s.design.components.iter().find(|c| c.bodies.contains(body)) {
        Some(c) => (true, c.id.0),
        None => (false, body.0),
      };
      if !owners.contains(&owner) {
        owners.push(owner);
      }
    }
  }
  owners.len()
}

/// OK: New Storyboard adds the storyboard and shows it; Transform Components closes.
pub fn confirm(app: &mut App) {
  if !can_ok(app) {
    return;
  }
  match app.anim.dialog.take() {
    Some(AnimDialog::NewStoryboard { kind }) => {
      let name = app.anim.next_name();
      app.anim.storyboards.push(Storyboard { name, kind });
      app.anim.current = app.anim.storyboards.len() - 1;
      app.anim.playhead = 0.0;
      app.anim.playing = None;
    }
    Some(AnimDialog::TransformComponents) => app.s.selection.clear(),
    None => {}
  }
}

/// Draws the open dialog, docked where command dialogs dock.
pub fn show_dialog(app: &mut App, ctx: &egui::Context) {
  let Some(mut d) = app.anim.dialog else { return };
  if !app.anim.active || app.dialog.is_some() || app.doc_dialog.is_some() {
    // another workspace, or a command started from elsewhere takes the dialog's place
    app.anim.dialog = None;
    return;
  }
  let width = 300.0;
  let icon = d.icon();
  let accent = icons::accent_of(icon);
  let n = picked(app);
  let dock = panels::dialog_dock(app.canvas, false);
  let (mut ok, mut cancel) = (false, false);
  egui::Area::new(egui::Id::new("anim-dialog"))
    .pivot(egui::Align2::RIGHT_TOP)
    .fixed_pos(dock)
    .constrain(true)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      panels::dialog_frame().show(ui, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        panels::dialog_header(ui, icon, d.title(), width);
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
          ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
          egui::Grid::new("anim-dlg").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(
            ui,
            |ui| match &mut d {
              AnimDialog::NewStoryboard { kind } => {
                ui.label("Storyboard Type");
                egui::ComboBox::from_id_salt("storyboard-type").selected_text(kind.label()).width(170.0).show_ui(
                  ui,
                  |ui| {
                    for t in StoryboardType::ALL {
                      ui.selectable_value(kind, t, t.label());
                    }
                  },
                );
                ui.end_row();
              }
              AnimDialog::TransformComponents => {
                ui.label("Components");
                // the only selection input, so it is the active one: lit from the start
                let text = if n == 0 { "Select".to_string() } else { format!("{n} selected") };
                crate::dialogs::sel_chip_lit(ui, accent, &text, true, n > 0);
                ui.end_row();
              }
            },
          );
        });
        let sep = ui.allocate_exact_size(vec2(width, 1.0), egui::Sense::hover()).0;
        ui.painter().rect_filled(sep, 0.0, theme::SEPARATOR);
        egui::Frame::new().fill(theme::DIALOG_BODY).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
          ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 24.0), egui::Sense::hover());
            ui.painter().circle_stroke(r.center(), 7.0, Stroke::new(1.2, theme::GROUP_LABEL));
            ui.painter().text(
              r.center(),
              egui::Align2::CENTER_CENTER,
              "i",
              egui::FontId::proportional(11.0),
              theme::GROUP_LABEL,
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
              if panels::secondary_button(ui, "Cancel").clicked() {
                cancel = true;
              }
              if crate::doc_dialogs::ok_button(ui, "OK", accent, can_ok_for(d, n)).clicked() {
                ok = true;
              }
            });
          });
        });
      });
    });
  if app.anim.dialog.is_none() {
    return;
  }
  app.anim.dialog = Some(d);
  if cancel {
    close(app);
  } else if ok {
    confirm(app);
  }
}

fn can_ok_for(d: AnimDialog, picked: usize) -> bool {
  match d {
    AnimDialog::NewStoryboard { .. } => true,
    AnimDialog::TransformComponents => picked > 0,
  }
}

// ---------------------------------------------------------------- the ANIMATION TIMELINE

const HEADER_H: f32 = 24.0;
const BODY_H: f32 = 104.0;
const BAR_H: f32 = 28.0;
/// The scale column at the left of the ruler.
const SCALE_W: f32 = 56.0;
/// The ruler's band at the top of the body.
const RULER_H: f32 = 22.0;
const KEYFRAME: Color32 = Color32::from_rgb(214, 48, 39);

/// The timeline panel's height: the header alone when collapsed, twice the body when maximised.
pub fn timeline_height(a: &AnimUi) -> f32 {
  if !a.timeline_open {
    HEADER_H
  } else if a.full {
    HEADER_H + BODY_H * 2.5 + BAR_H
  } else {
    HEADER_H + BODY_H + BAR_H
  }
}

/// Pixels per second on the ruler: 0..24 s fill `width` at zoom 1.
pub fn px_per_second(width: f32, zoom: f32) -> f32 {
  (width / (RULER_SECONDS + 0.5)).max(1.0) * zoom
}

/// The step between labelled seconds so labels stay at least 24 px apart.
pub fn label_step(px_per_s: f32) -> u32 {
  [1, 2, 5, 10, 20, 30, 60].into_iter().find(|s| *s as f32 * px_per_s >= 24.0).unwrap_or(60)
}

/// Advances the playhead while playing; stops at the storyboard's end.
pub fn step_playback(a: &mut AnimUi, now: f64) {
  let Some(last) = a.playing else { return };
  let end = a.length();
  a.playhead = (a.playhead + (now - last) as f32).min(end);
  a.playing = if a.playhead >= end { None } else { Some(now) };
}

/// The ANIMATION TIMELINE, at the bottom in place of the design timeline.
pub fn timeline(app: &mut App, ui: &mut egui::Ui) {
  let now = ui.input(|i| i.time);
  if app.anim.playing.is_some() {
    step_playback(&mut app.anim, now);
    ui.ctx().request_repaint();
  }
  egui::Panel::bottom("anim_timeline")
    .exact_size(timeline_height(&app.anim))
    .frame(egui::Frame::NONE.fill(theme::TIMELINE))
    .show(ui, |ui| {
      ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
      let full = ui.max_rect();
      ui.painter().line_segment([full.left_top(), full.right_top()], Stroke::new(1.0, theme::BORDER));
      header(app, ui, full.width());
      if !app.anim.timeline_open {
        return;
      }
      let body_h = ui.available_height() - BAR_H;
      let (body, _) = ui.allocate_exact_size(vec2(full.width(), body_h), egui::Sense::hover());
      ruler(app, ui, body);
      bottom_bar(app, ui, full.width(), now);
    });
}

/// The panel's header: the collapse arrow and ANIMATION TIMELINE.
fn header(app: &mut App, ui: &mut egui::Ui, width: f32) {
  let (rect, resp) = ui.allocate_exact_size(vec2(width, HEADER_H), egui::Sense::click());
  let p = ui.painter();
  p.rect_filled(rect, 0.0, if resp.hovered() { theme::HOVER } else { theme::FLOAT_HEADER });
  p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, Color32::from_gray(212)));
  let ink = theme::GROUP_LABEL;
  let c = rect.left_center() + vec2(14.0, 0.0);
  let pts = if app.anim.timeline_open {
    vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
  } else {
    vec![c + vec2(-4.0, 2.0), c + vec2(4.0, 2.0), c + vec2(0.0, -3.0)]
  };
  p.add(egui::Shape::convex_polygon(pts, ink, Stroke::NONE));
  p.text(
    rect.left_center() + vec2(28.0, 0.0),
    egui::Align2::LEFT_CENTER,
    "ANIMATION TIMELINE",
    egui::FontId::proportional(12.0),
    ink,
  );
  if resp.clicked() {
    app.anim.timeline_open = !app.anim.timeline_open;
  }
}

/// The scale column, the time ruler with the keyframe marker at 0 and the playhead, and the
/// track area below; a click or drag on the ruler moves the playhead.
fn ruler(app: &mut App, ui: &mut egui::Ui, body: Rect) {
  let p = ui.painter_at(body);
  p.rect_filled(body, 0.0, Color32::WHITE);
  let scale = Rect::from_min_max(body.min, pos2(body.min.x + SCALE_W, body.max.y));
  p.rect_filled(scale, 0.0, theme::TIMELINE);
  p.line_segment([scale.right_top(), scale.right_bottom()], Stroke::new(1.0, theme::BORDER));
  p.text(
    pos2(scale.center().x, scale.top() + RULER_H / 2.0),
    egui::Align2::CENTER_CENTER,
    format!("{:.1}", app.anim.zoom),
    egui::FontId::proportional(11.5),
    theme::TEXT,
  );
  let track = Rect::from_min_max(pos2(scale.right() + 8.0, body.top()), body.max);
  let band = Rect::from_min_max(track.min, pos2(track.right(), track.top() + RULER_H));
  p.line_segment([pos2(scale.right(), band.bottom()), band.right_bottom()], Stroke::new(1.0, theme::BORDER));
  let pps = px_per_second(track.width(), app.anim.zoom);
  let step = label_step(pps);
  let x_of = |t: f32| track.left() + t * pps;
  let seconds = (track.width() / pps).ceil() as u32;
  for s in 0..=seconds {
    let x = x_of(s as f32);
    let labelled = s % step == 0;
    let h = if labelled { 7.0 } else { 4.0 };
    p.line_segment([pos2(x, band.bottom() - h), pos2(x, band.bottom())], Stroke::new(1.0, theme::GROUP_LABEL));
    if labelled {
      p.text(
        pos2(x + 2.0, band.top() + 2.0),
        egui::Align2::LEFT_TOP,
        s.to_string(),
        egui::FontId::proportional(10.5),
        theme::GROUP_LABEL,
      );
    }
    // a faint line down the tracks each labelled second
    if labelled && s > 0 {
      p.line_segment([pos2(x, band.bottom()), pos2(x, track.bottom())], Stroke::new(1.0, Color32::from_gray(236)));
    }
  }
  // the keyframe marker at 0: a red flag on the ruler
  let k = x_of(0.0);
  p.add(egui::Shape::convex_polygon(
    vec![pos2(k - 4.0, band.top() + 4.0), pos2(k + 4.0, band.top() + 4.0), pos2(k, band.top() + 10.0)],
    KEYFRAME,
    Stroke::NONE,
  ));
  p.line_segment([pos2(k, band.top() + 10.0), pos2(k, band.bottom())], Stroke::new(1.5, KEYFRAME));
  // the playhead: a grey handle on the ruler with its line down the tracks
  let x = x_of(app.anim.playhead);
  let grey = Color32::from_gray(128);
  let handle = Rect::from_center_size(pos2(x, band.bottom() - 4.0), vec2(9.0, 8.0));
  p.rect_filled(handle, 1.0, grey);
  p.line_segment([pos2(x, band.bottom()), pos2(x, track.bottom())], Stroke::new(1.5, grey));
  let resp = ui.interact(band, ui.id().with("anim-ruler"), egui::Sense::click_and_drag());
  if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.clicked() || resp.dragged()) {
    app.anim.playhead = time_at(pos, track.left(), pps);
    app.anim.playing = None;
  }
}

/// The time under `pos` on a ruler starting at `left`, never before 0.
pub fn time_at(pos: Pos2, left: f32, pps: f32) -> f32 {
  ((pos.x - left) / pps).max(0.0)
}

/// The bottom bar: the storyboard tabs and +, the transport buttons, full screen and settings,
/// and the zoom - / slider / +.
fn bottom_bar(app: &mut App, ui: &mut egui::Ui, width: f32, now: f64) {
  let (bar, _) = ui.allocate_exact_size(vec2(width, BAR_H), egui::Sense::hover());
  ui.painter().line_segment([bar.left_top(), bar.right_top()], Stroke::new(1.0, theme::BORDER));
  let mut child = ui.new_child(
    egui::UiBuilder::new()
      .max_rect(bar.shrink2(vec2(6.0, 2.0)))
      .layout(egui::Layout::left_to_right(egui::Align::Center)),
  );
  let ui = &mut child;
  ui.spacing_mut().item_spacing.x = 2.0;
  let ink = Some(theme::GROUP_LABEL);
  for i in 0..app.anim.storyboards.len() {
    if storyboard_tab(ui, &app.anim.storyboards[i].name, i == app.anim.current).clicked() {
      app.anim.current = i;
      app.anim.playhead = 0.0;
      app.anim.playing = None;
    }
  }
  if icons::icon_button(ui, Icon::Plus, 20.0, "New Storyboard", false, ink).clicked() {
    AnimCmd::NewStoryboard.run(app);
  }
  ui.add_space(12.0);
  if icons::icon_button(ui, Icon::ToStart, 22.0, "Go to start", false, ink).clicked() {
    app.anim.playhead = 0.0;
    app.anim.playing = None;
  }
  let playing = app.anim.playing.is_some();
  if icons::icon_button(ui, Icon::Play, 22.0, if playing { "Pause" } else { "Play" }, playing, ink).clicked() {
    app.anim.playing = if playing { None } else { Some(now) };
  }
  if icons::icon_button(ui, Icon::ToEnd, 22.0, "Go to end", false, ink).clicked() {
    app.anim.playhead = app.anim.length();
    app.anim.playing = None;
  }
  ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
    ui.spacing_mut().item_spacing.x = 2.0;
    if icons::icon_button(ui, Icon::Plus, 18.0, "Zoom in", false, ink).clicked() {
      app.anim.zoom = (app.anim.zoom * 1.25).min(*ZOOM.end());
    }
    ui.spacing_mut().slider_width = 90.0;
    ui.add(egui::Slider::new(&mut app.anim.zoom, ZOOM).show_value(false).logarithmic(true));
    if minus_button(ui).on_hover_text("Zoom out").clicked() {
      app.anim.zoom = (app.anim.zoom / 1.25).max(*ZOOM.start());
    }
    ui.add_space(10.0);
    if icons::icon_button(ui, Icon::Gear, 20.0, "Animation timeline settings", false, ink).clicked() {
      app.status = Some(("Animation timeline settings aren't in FissionOxide yet".into(), false));
    }
    let tip = if app.anim.full { "Restore timeline" } else { "Maximise timeline" };
    if icons::icon_button(ui, Icon::Fit, 20.0, tip, app.anim.full, ink).clicked() {
      app.anim.full = !app.anim.full;
    }
  });
}

/// A storyboard's tab: its name, the current one white with a bold underline.
fn storyboard_tab(ui: &mut egui::Ui, name: &str, current: bool) -> egui::Response {
  let font = egui::FontId::proportional(12.0);
  let tw = ui.painter().layout_no_wrap(name.to_string(), font.clone(), theme::TEXT).size().x;
  let (rect, resp) = ui.allocate_exact_size(vec2(tw + 20.0, 22.0), egui::Sense::click());
  let p = ui.painter();
  let fill = if current {
    Color32::WHITE
  } else if resp.hovered() {
    theme::HOVER
  } else {
    theme::TIMELINE
  };
  p.rect(rect, 2.0, fill, Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
  p.text(rect.center(), egui::Align2::CENTER_CENTER, name, font, theme::TEXT);
  if current {
    p.line_segment(
      [rect.left_bottom() + vec2(3.0, -1.5), rect.right_bottom() + vec2(-3.0, -1.5)],
      Stroke::new(2.0, theme::ACCENT),
    );
  }
  resp.on_hover_text(RichText::new(name))
}

/// The zoom's −: a flat bar in an icon-sized button.
fn minus_button(ui: &mut egui::Ui) -> egui::Response {
  let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::click());
  if resp.hovered() {
    ui.painter().rect_filled(rect, 2.0, theme::HOVER);
  }
  ui.painter().line_segment(
    [rect.center() - vec2(5.0, 0.0), rect.center() + vec2(5.0, 0.0)],
    Stroke::new(1.6, theme::GROUP_LABEL),
  );
  resp
}

#[cfg(test)]
mod tests {
  use super::*;
  use fox_engine::Session;
  use serde_json::json;

  #[test]
  fn storyboards_start_with_one_and_number_on() {
    let mut a = AnimUi::default();
    assert_eq!(a.storyboards.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Storyboard1"]);
    assert_eq!(a.next_name(), "Storyboard2");
    a.storyboards.push(Storyboard { name: "Storyboard7".into(), kind: StoryboardType::Clean });
    assert_eq!(a.next_name(), "Storyboard8");
    assert_eq!(StoryboardType::Clean.label(), "Clean");
  }

  #[test]
  fn the_ruler_shows_24_seconds_at_the_start() {
    let pps = px_per_second(800.0, 1.0);
    assert!((800.0 / pps) > RULER_SECONDS && (800.0 / pps) < RULER_SECONDS + 1.0);
    assert_eq!(label_step(pps), 1, "every second is labelled at 800 px");
    assert_eq!(label_step(px_per_second(300.0, 1.0)), 2);
    assert!((time_at(pos2(100.0 + 3.0 * pps, 0.0), 100.0, pps) - 3.0).abs() < 1e-4);
    assert_eq!(time_at(pos2(50.0, 0.0), 100.0, pps), 0.0, "never before 0");
    assert_eq!(timeline_height(&AnimUi::default()), HEADER_H + BODY_H + BAR_H);
    let closed = AnimUi { timeline_open: false, ..AnimUi::default() };
    assert_eq!(timeline_height(&closed), HEADER_H);
  }

  #[test]
  fn playing_an_empty_storyboard_stops_at_its_end() {
    let mut a = AnimUi { playing: Some(1.0), ..AnimUi::default() };
    step_playback(&mut a, 1.5);
    assert_eq!((a.playhead, a.playing), (0.0, None));
  }

  #[test]
  fn new_storyboard_adds_one_on_ok_and_cancel_drops_it() {
    let mut app = App::with_session(Session::default());
    crate::panels::switch_workspace(&mut app, "Animation");
    AnimCmd::NewStoryboard.run(&mut app);
    assert_eq!(app.anim.dialog, Some(AnimDialog::NewStoryboard { kind: StoryboardType::Clean }));
    assert_eq!(app.anim.dialog.map(AnimDialog::title), Some("NEW STORYBOARD"));
    assert!(AnimCmd::NewStoryboard.active(&app));
    assert!(can_ok(&app), "New Storyboard's OK is enabled from the start");
    assert!(close(&mut app));
    assert_eq!(app.anim.storyboards.len(), 1, "Cancel adds nothing");
    AnimCmd::NewStoryboard.run(&mut app);
    app.anim.dialog = Some(AnimDialog::NewStoryboard { kind: StoryboardType::StartFromEnd });
    confirm(&mut app);
    assert!(app.anim.dialog.is_none());
    assert_eq!(app.anim.storyboards[1], Storyboard { name: "Storyboard2".into(), kind: StoryboardType::StartFromEnd });
    assert_eq!(app.anim.current, 1, "the new storyboard is shown");
  }

  #[test]
  fn transform_components_ok_is_greyed_until_a_component_is_picked() {
    let mut app = App::with_session(Session::default());
    app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
    app.s.selection.clear();
    crate::panels::switch_workspace(&mut app, "Animation");
    crate::panels::Cmd::Anim(AnimCmd::TransformComponents).run(&mut app);
    assert_eq!(app.anim.dialog.map(AnimDialog::title), Some("TRANSFORM COMPONENTS"));
    assert_eq!(picked(&app), 0);
    assert!(!can_ok(&app), "OK is greyed with Components at Select");
    confirm(&mut app);
    assert!(app.anim.dialog.is_some(), "a greyed OK does nothing");
    let body = app.s.model.bodies[0].id;
    app.s.selection = vec![fox_engine::Sel::Body { body }];
    assert_eq!(picked(&app), 1);
    assert!(can_ok(&app));
    confirm(&mut app);
    assert!(app.anim.dialog.is_none());
    // Escape closes it, changing nothing
    AnimCmd::TransformComponents.run(&mut app);
    app.escape();
    assert!(app.anim.dialog.is_none());
  }
}
