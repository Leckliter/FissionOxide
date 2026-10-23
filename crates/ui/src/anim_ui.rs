//! The Animation workspace, laid out as Fusion's: the ANIMATION tab's commands, the ANIMATION
//! TIMELINE with its storyboards (in place of the design timeline) and their actions, and the
//! docked NEW STORYBOARD, TRANSFORM COMPONENTS, MANUAL EXPLODE, APPEARANCE and PUBLISH VIDEO
//! dialogs. The browser's Components-only tree is `panels::animation_tree`.
//!
//! Every command records actions in the current storyboard from the playhead (the engine's
//! `animation.*` commands, saved with the design), and the playhead moves to where they end. The
//! canvas shows the storyboard at the playhead: components moved, faded and hidden, its callouts
//! and, once it has views, its camera; Play runs it.

use egui::{Color32, Pos2, Rect, RichText, Stroke, pos2, vec2};
use fox_engine::kernel::glam::{DAffine3, DVec3};
use fox_engine::model::BodyId;
use fox_engine::model::animation::{self as anim, ActionKind, Pose, Storyboard, Targets};
use fox_engine::render::Camera;
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::manipulators::{Triad, TriadEdit};
use crate::{App, panels, theme};

/// The ANIMATION tab's commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimCmd {
  NewStoryboard,
  TransformComponents,
  RestoreHome,
  ExplodeOne,
  ExplodeAll,
  ManualExplode,
  ShowHide,
  Appearance,
  Callout,
  View,
  Publish,
}

impl AnimCmd {
  pub(crate) fn info(self) -> (Icon, &'static str, &'static str) {
    match self {
      AnimCmd::NewStoryboard => (Icon::Storyboard, "New Storyboard", "Adds a storyboard to the animation"),
      AnimCmd::TransformComponents => {
        (Icon::TransformComponents, "Transform Components", "Moves and rotates components over time (M)")
      }
      AnimCmd::RestoreHome => (Icon::RestoreHome, "Restore Home", "Puts components back where they are in the design"),
      AnimCmd::ExplodeOne => {
        (Icon::Explode, "Auto Explode: One Level", "Explodes the selected components one level down")
      }
      AnimCmd::ExplodeAll => (Icon::Explode, "Auto Explode: All Levels", "Explodes all levels of the components (U)"),
      AnimCmd::ManualExplode => (Icon::ManualExplode, "Manual Explode", "Explodes components along a direction (E)"),
      AnimCmd::ShowHide => (Icon::Eye, "Show/Hide", "Shows or hides the selected components"),
      AnimCmd::Appearance => (Icon::Appearance, "Appearance", "Changes the opacity of components over time"),
      AnimCmd::Callout => (Icon::Callout, "Create Callout", "Adds a text callout attached to the model"),
      AnimCmd::View => (Icon::AnimView, "View", "Records the camera as a view action (Ctrl+R)"),
      AnimCmd::Publish => (Icon::Video, "Publish Video", "Publishes the storyboards as a video (P)"),
    }
  }

  /// Shown pressed: its dialog is open.
  pub(crate) fn active(self, app: &App) -> bool {
    matches!(
      (self, &app.anim.dialog),
      (AnimCmd::NewStoryboard, Some(AnimDialog::NewStoryboard { .. }))
        | (AnimCmd::TransformComponents, Some(AnimDialog::TransformComponents))
        | (AnimCmd::ExplodeOne, Some(AnimDialog::AutoExplode { all: false, .. }))
        | (AnimCmd::ExplodeAll, Some(AnimDialog::AutoExplode { all: true, .. }))
        | (AnimCmd::ManualExplode, Some(AnimDialog::ManualExplode { .. }))
        | (AnimCmd::Appearance, Some(AnimDialog::Appearance { .. }))
        | (AnimCmd::Callout, Some(AnimDialog::Callout { .. }))
        | (AnimCmd::Publish, Some(AnimDialog::Publish { .. }))
    )
  }

  pub(crate) fn run(self, app: &mut App) {
    match self {
      AnimCmd::NewStoryboard => open(app, AnimDialog::NewStoryboard { kind: StoryboardType::Clean }),
      AnimCmd::TransformComponents => open(app, AnimDialog::TransformComponents),
      AnimCmd::RestoreHome => restore_home(app),
      AnimCmd::ExplodeOne => auto_explode(app, false),
      AnimCmd::ExplodeAll => auto_explode(app, true),
      AnimCmd::ManualExplode => open(app, AnimDialog::ManualExplode { axis: 2, distance: 0.0 }),
      AnimCmd::ShowHide => show_hide(app),
      AnimCmd::Appearance => open(app, AnimDialog::Appearance { opacity: 50.0 }),
      AnimCmd::Callout => {
        open(app, AnimDialog::Callout { at: None, text: String::new() });
        app.status = Some(("Click a point on the model for the callout".into(), false));
      }
      AnimCmd::View => record_view(app),
      AnimCmd::Publish => open(app, AnimDialog::Publish { all: false, resolution: Resolution::Window }),
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

  fn key(self) -> &'static str {
    match self {
      StoryboardType::Clean => "clean",
      StoryboardType::StartFromEnd => "start_from_end",
    }
  }
}

/// Publish Video's Video Resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
  /// The canvas's own size.
  Window,
  Hd720,
  Hd1080,
}

impl Resolution {
  pub const ALL: [Resolution; 3] = [Resolution::Window, Resolution::Hd720, Resolution::Hd1080];

  pub fn label(self) -> &'static str {
    match self {
      Resolution::Window => "Current document window",
      Resolution::Hd720 => "1280 x 720",
      Resolution::Hd1080 => "1920 x 1080",
    }
  }

  fn size(self, canvas_px: [u32; 2]) -> [u32; 2] {
    match self {
      Resolution::Window => [canvas_px[0].max(16), canvas_px[1].max(16)],
      Resolution::Hd720 => [1280, 720],
      Resolution::Hd1080 => [1920, 1080],
    }
  }
}

/// The open Animation dialog.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimDialog {
  NewStoryboard {
    kind: StoryboardType,
  },
  TransformComponents,
  /// After Auto Explode recorded its actions: the canvas toolbar's spacing slider re-records
  /// them; Cancel takes them back.
  AutoExplode {
    all: bool,
    scale: f32,
    start: f32,
  },
  /// The axis (0 X, 1 Y, 2 Z) and how far along it (mm).
  ManualExplode {
    axis: usize,
    distance: f64,
  },
  /// Opacity in percent.
  Appearance {
    opacity: f32,
  },
  /// The picked point (and its body), then the text being typed.
  Callout {
    at: Option<(DVec3, Option<BodyId>)>,
    text: String,
  },
  Publish {
    all: bool,
    resolution: Resolution,
  },
}

impl AnimDialog {
  pub fn title(&self) -> &'static str {
    match self {
      AnimDialog::NewStoryboard { .. } => "NEW STORYBOARD",
      AnimDialog::TransformComponents => "TRANSFORM COMPONENTS",
      AnimDialog::AutoExplode { .. } => "AUTO EXPLODE",
      AnimDialog::ManualExplode { .. } => "MANUAL EXPLODE",
      AnimDialog::Appearance { .. } => "APPEARANCE",
      AnimDialog::Callout { .. } => "CREATE CALLOUT",
      AnimDialog::Publish { .. } => "PUBLISH VIDEO",
    }
  }

  fn icon(&self) -> Icon {
    match self {
      AnimDialog::NewStoryboard { .. } => Icon::Storyboard,
      AnimDialog::TransformComponents => Icon::TransformComponents,
      AnimDialog::AutoExplode { .. } => Icon::Explode,
      AnimDialog::ManualExplode { .. } => Icon::ManualExplode,
      AnimDialog::Appearance { .. } => Icon::Appearance,
      AnimDialog::Callout { .. } => Icon::Callout,
      AnimDialog::Publish { .. } => Icon::Video,
    }
  }

  /// Drawn docked with the command dialogs (Auto Explode and callouts work in the canvas).
  fn docked(&self) -> bool {
    !matches!(self, AnimDialog::AutoExplode { .. } | AnimDialog::Callout { .. })
  }

  /// Picks components (its Components row).
  fn picks(&self) -> bool {
    matches!(self, AnimDialog::TransformComponents | AnimDialog::ManualExplode { .. } | AnimDialog::Appearance { .. })
  }
}

/// Transform Components' move so far (before OK records it): about `pivot`, the components'
/// centre, turned by `angles` (radians about X, Y, Z) and moved by `translate`.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
  /// The selection it is for: a new pick starts over.
  pub for_sel: Vec<fox_engine::Sel>,
  pub pivot: DVec3,
  pub translate: DVec3,
  pub angles: [f64; 3],
}

/// A storyboard bar being dragged on the timeline: moved, or (by its right end) lengthened.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarDrag {
  pub action: u32,
  pub resize: bool,
  /// The pointer's x and the action's start and duration at the grab.
  pub grab_x: f32,
  pub start: f64,
  pub duration: f64,
  /// Where it would go now.
  pub to: f64,
}

/// The ANIMATION TIMELINE's ruler: Fusion's shows 0 to 24 seconds at the start.
pub const RULER_SECONDS: f32 = 24.0;
/// The timeline zoom's range (the slider and its - and +).
pub const ZOOM: std::ops::RangeInclusive<f32> = 0.25..=4.0;

/// The Animation workspace's state (the storyboards themselves are the design's).
pub struct AnimUi {
  /// In the Animation workspace.
  pub active: bool,
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
  pub pending: Option<Pending>,
  /// The action picked on the timeline (Delete deletes it).
  pub selected: Option<u32>,
  pub bar_drag: Option<BarDrag>,
  /// The first track row shown (the wheel scrolls the tracks).
  pub scroll: usize,
  /// Where the camera was when the playhead last moved: View records the move from there.
  pub view_mark: Option<Camera>,
  /// The playhead the canvas's camera last followed the storyboard's views at.
  pub cam_at: Option<f32>,
  /// Trails: dashed lines from where moved components start to where they are.
  pub trails: bool,
}

impl Default for AnimUi {
  fn default() -> Self {
    AnimUi {
      active: false,
      current: 0,
      playhead: 0.0,
      playing: None,
      timeline_open: true,
      full: false,
      zoom: 1.0,
      dialog: None,
      pending: None,
      selected: None,
      bar_drag: None,
      scroll: 0,
      view_mark: None,
      cam_at: None,
      trails: true,
    }
  }
}

/// The design's storyboards as the timeline lists them.
pub fn storyboards(app: &App) -> Vec<Storyboard> {
  app.s.design.animation.listed()
}

/// The current storyboard (kept in range as undo takes storyboards away).
fn current(app: &mut App) -> usize {
  let n = app.s.design.animation.listed().len();
  if app.anim.current >= n {
    app.anim.current = n - 1;
  }
  app.anim.current
}

/// The length of the current storyboard in seconds: the end of its last action.
pub fn length(app: &App) -> f32 {
  storyboards(app).get(app.anim.current).map(|s| s.length() as f32).unwrap_or(0.0)
}

/// Opens `d`, cancelling a command dialog that was open as starting any command does.
pub fn open(app: &mut App, d: AnimDialog) {
  app.cancel_dialog();
  crate::doc_dialogs::close(app);
  finish(app);
  app.anim.dialog = Some(d);
}

/// Puts the open dialog away as another command starts: an Auto Explode keeps its actions, a
/// move not yet OK'd is dropped.
fn finish(app: &mut App) {
  app.anim.dialog = None;
  app.anim.pending = None;
}

/// Closes the dialog without applying it (Cancel, Escape). Returns whether one was open. Auto
/// Explode's Cancel takes its actions back.
pub fn close(app: &mut App) -> bool {
  let Some(d) = app.anim.dialog.take() else { return false };
  app.anim.pending = None;
  if let AnimDialog::AutoExplode { start, .. } = d {
    app.s.undo();
    app.anim.playhead = start;
  }
  true
}

/// The components and root bodies picked: the components whose bodies are selected (a body of
/// the root stands for itself).
pub fn picked_targets(app: &App) -> Targets {
  let mut t = Targets::default();
  for s in &app.s.selection {
    if let fox_engine::Sel::Body { body } = s {
      match app.s.design.components.iter().find(|c| c.bodies.contains(body)) {
        Some(c) if !t.components.contains(&c.id) => t.components.push(c.id),
        Some(_) => {}
        None if !t.bodies.contains(body) => t.bodies.push(*body),
        None => {}
      }
    }
  }
  t
}

/// How many components the dialog has: the components whose bodies are selected.
pub fn picked(app: &App) -> usize {
  let t = picked_targets(app);
  t.components.len() + t.bodies.len()
}

fn targets_json(t: &Targets) -> Value {
  json!({
    "components": t.components.iter().map(|c| c.0).collect::<Vec<_>>(),
    "bodies": t.bodies.iter().map(|b| b.0).collect::<Vec<_>>(),
  })
}

/// Runs an `animation.*` command on the current storyboard from the playhead; the playhead
/// moves to where its actions end.
pub fn record(app: &mut App, cmd: &str, mut p: Value) -> Option<Value> {
  p["storyboard"] = json!(current(app));
  if p.get("start").is_none() {
    p["start"] = json!(app.anim.playhead as f64);
  }
  let r = app.run(cmd, p)?;
  if let Some(end) = r["end"].as_f64() {
    app.anim.playhead = end as f32;
  }
  app.anim.playing = None;
  app.anim.view_mark = Some(app.cam);
  Some(r)
}

/// Whether OK is enabled: Transform Components' once a component is picked and moved, Manual
/// Explode's once they are picked and the distance isn't 0, Appearance's once they are picked.
pub fn can_ok(app: &App) -> bool {
  let n = picked(app);
  match &app.anim.dialog {
    Some(AnimDialog::NewStoryboard { .. } | AnimDialog::Publish { .. } | AnimDialog::AutoExplode { .. }) => true,
    Some(AnimDialog::TransformComponents) => n > 0,
    Some(AnimDialog::ManualExplode { distance, .. }) => n > 0 && distance.abs() > 1e-9,
    Some(AnimDialog::Appearance { .. }) => n > 0,
    Some(AnimDialog::Callout { at, text }) => at.is_some() && !text.trim().is_empty(),
    None => false,
  }
}

/// OK: records the dialog's action (New Storyboard adds the storyboard and shows it).
pub fn confirm(app: &mut App) {
  if !can_ok(app) {
    return;
  }
  let Some(d) = app.anim.dialog.take() else { return };
  let pending = app.anim.pending.take();
  let t = picked_targets(app);
  match d {
    AnimDialog::NewStoryboard { kind } => {
      if let Some(r) = app.run("animation.storyboard.add", json!({"type": kind.key()})) {
        app.anim.current = r["index"].as_u64().unwrap_or(0) as usize;
        app.anim.playhead = 0.0;
        app.anim.playing = None;
        app.anim.selected = None;
      }
    }
    AnimDialog::TransformComponents => {
      if let Some(m) = pending.filter(|m| m.for_sel == app.s.selection && moved(m)) {
        let mut p = targets_json(&t);
        p["translate"] = json!(m.translate.to_array());
        p["rotate"] = json!(m.angles.map(f64::to_degrees));
        p["pivot"] = json!(m.pivot.to_array());
        record(app, "animation.transform", p);
      }
      app.s.selection.clear();
    }
    AnimDialog::ManualExplode { axis, distance } => {
      let mut p = targets_json(&t);
      p["direction"] = json!(["x", "y", "z"][axis.min(2)]);
      p["distance"] = json!(distance);
      record(app, "animation.manual_explode", p);
      app.s.selection.clear();
    }
    AnimDialog::Appearance { opacity } => {
      let mut p = targets_json(&t);
      p["opacity"] = json!((opacity as f64 / 100.0).clamp(0.0, 1.0));
      record(app, "animation.opacity", p);
      app.s.selection.clear();
    }
    AnimDialog::Callout { at: Some((point, body)), text } => {
      let mut p = json!({"point": point.to_array(), "text": text.trim()});
      if let Some(b) = body {
        p["body"] = json!(b.0);
      }
      record(app, "animation.callout", p);
      app.status = None;
    }
    AnimDialog::Callout { at: None, .. } | AnimDialog::AutoExplode { .. } => {}
    AnimDialog::Publish { all, resolution } => {
      let name = format!("{}.avi", app.s.design.name);
      if let Some(path) = rfd::FileDialog::new().add_filter("AVI video", &["avi"]).set_file_name(name).save_file() {
        publish_to(app, &path, all, resolution);
      }
    }
  }
}

fn moved(m: &Pending) -> bool {
  m.translate.length() > 1e-9 || m.angles.iter().any(|a| a.abs() > 1e-9)
}

/// Publish Video to `path`: the current storyboard (or all of them) at the resolution, seen from
/// the canvas's camera where the storyboard has no views.
pub fn publish_to(app: &mut App, path: &std::path::Path, all: bool, resolution: Resolution) -> Option<Value> {
  let [w, h] = resolution.size(app.canvas_px);
  let c = app.cam;
  let p = json!({
    "path": path,
    "scope": if all { "all" } else { "current" },
    "storyboard": current(app),
    "width": w,
    "height": h,
    "camera": {"target": c.target, "yaw": c.yaw, "pitch": c.pitch, "distance": c.distance, "fov": c.fov, "ortho": c.ortho},
  });
  let r = app.run("animation.publish", p)?;
  app.status = Some((format!("Published {}", path.display()), false));
  Some(r)
}

/// Restore Home: the picked components (or every moved one) go back where the design has them.
fn restore_home(app: &mut App) {
  finish(app);
  let t = picked_targets(app);
  record(app, "animation.restore_home", targets_json(&t));
}

/// Auto Explode: records the explode and shows its spacing slider.
fn auto_explode(app: &mut App, all: bool) {
  finish(app);
  let start = app.anim.playhead;
  let mut p = targets_json(&picked_targets(app));
  p["levels"] = json!(if all { "all" } else { "one" });
  if record(app, "animation.explode", p).is_some() {
    app.anim.dialog = Some(AnimDialog::AutoExplode { all, scale: 1.0, start });
  }
}

/// The spacing slider moved: the explode is taken back and recorded again at the new spacing.
fn re_explode(app: &mut App, all: bool, scale: f32, start: f32) {
  app.s.undo();
  app.anim.playhead = start;
  let mut p = targets_json(&picked_targets(app));
  p["levels"] = json!(if all { "all" } else { "one" });
  p["scale"] = json!(scale as f64);
  if record(app, "animation.explode", p).is_none() {
    app.anim.dialog = None;
  }
}

/// Show/Hide: an action hiding the picked components that show and showing those hidden.
fn show_hide(app: &mut App) {
  finish(app);
  let t = picked_targets(app);
  if t.is_empty() {
    app.status = Some(("Select the components to show or hide".into(), false));
    return;
  }
  record(app, "animation.visibility", targets_json(&t));
}

fn camera_json(c: &Camera) -> Value {
  json!({"target": c.target, "yaw": c.yaw, "pitch": c.pitch, "distance": c.distance, "fov": c.fov, "ortho": c.ortho})
}

/// View: records the camera's move to where it is now, from where the storyboard has it at the
/// playhead (else from where it was when the playhead last moved).
fn record_view(app: &mut App) {
  finish(app);
  let i = current(app);
  let from = fox_engine::animation::pose(&app.s, i, app.anim.playhead as f64)
    .camera
    .map(|c| json!({"target": c.target, "yaw": c.yaw, "pitch": c.pitch, "distance": c.distance, "fov": c.fov, "ortho": c.ortho}))
    .unwrap_or_else(|| camera_json(&app.anim.view_mark.unwrap_or(app.cam)));
  let to = camera_json(&app.cam);
  if record(app, "animation.view", json!({"from": from, "to": to})).is_some() {
    // the canvas already shows where the view ends
    app.anim.cam_at = Some(app.anim.playhead);
  }
}

/// Deletes the action picked on the timeline. Returns whether there was one.
pub fn delete_selected(app: &mut App) -> bool {
  let Some(id) = app.anim.selected.filter(|_| app.anim.active) else { return false };
  app.anim.selected = None;
  let i = current(app);
  app.run("animation.action.delete", json!({"storyboard": i, "action": id}));
  true
}

// ---------------------------------------------------------------- the pose on the canvas

/// The bodies Transform Components or Manual Explode move.
fn picked_bodies(app: &App) -> Vec<BodyId> {
  picked_targets(app).bodies(&app.s.design)
}

/// What the canvas shows: the current storyboard at the playhead, with the open dialog's move
/// previewed.
pub fn pose(app: &App) -> Pose {
  let i = app.anim.current.min(storyboards(app).len() - 1);
  let mut pose = fox_engine::animation::pose(&app.s, i, app.anim.playhead as f64);
  match &app.anim.dialog {
    Some(AnimDialog::TransformComponents) => {
      if let Some(m) = app.anim.pending.as_ref().filter(|m| m.for_sel == app.s.selection) {
        let deg = m.angles.map(f64::to_degrees);
        let by = anim::transform_at(m.translate.to_array(), deg, m.pivot.to_array(), 1.0);
        for b in picked_bodies(app) {
          let total = by * pose.transform(b);
          pose.transforms.insert(b, total);
        }
      }
    }
    Some(AnimDialog::ManualExplode { axis, distance }) if distance.abs() > 1e-9 => {
      let dir = [DVec3::X, DVec3::Y, DVec3::Z][(*axis).min(2)];
      let t = picked_targets(app);
      for (part, by) in fox_engine::animation::manual_parts(&app.s, &pose.clone(), &t, dir, *distance) {
        for b in part.bodies(&app.s.design) {
          let total = DAffine3::from_translation(by) * pose.transform(b);
          pose.transforms.insert(b, total);
        }
      }
    }
    _ => {}
  }
  pose
}

/// Draws the canvas as the storyboard has it: in the Animation workspace the scene's bodies move,
/// fade and hide as the pose says.
pub fn dress_options(app: &App, o: &mut fox_engine::scene::ViewOptions) {
  if !app.anim.active {
    return;
  }
  let pose = pose(app);
  let shown = fox_engine::animation::shown_bodies(&app.s, &pose);
  o.only_bodies = Some(match o.only_bodies.take() {
    Some(only) => only.into_iter().filter(|b| pose.shows(*b, true)).collect(),
    None => shown,
  });
  for (b, a) in fox_engine::animation::faded(&pose) {
    o.opacity.retain(|(x, _)| *x != b);
    o.opacity.push((b, a));
  }
  o.transforms = pose.transforms;
}

/// What the canvas's image depends on in the Animation workspace (part of the render key).
pub fn render_key(app: &App) -> Vec<u64> {
  if !app.anim.active {
    return Vec::new();
  }
  use std::hash::{Hash, Hasher};
  let mut h = std::collections::hash_map::DefaultHasher::new();
  format!("{:?}", app.s.design.animation).hash(&mut h);
  format!("{:?} {:?}", app.anim.dialog, app.anim.pending).hash(&mut h);
  app.s.selection.len().hash(&mut h);
  vec![h.finish(), app.anim.current as u64, app.anim.playhead.to_bits() as u64]
}

/// The canvas follows the storyboard's views when the playhead moves (playing or scrubbing).
pub fn follow_camera(app: &mut App) {
  if !app.anim.active || app.anim.cam_at == Some(app.anim.playhead) {
    return;
  }
  app.anim.cam_at = Some(app.anim.playhead);
  if let Some(c) = pose(app).camera {
    app.stop_cam_move();
    (app.cam.target, app.cam.yaw, app.cam.pitch, app.cam.distance, app.cam.fov, app.cam.ortho) =
      (c.target, c.yaw, c.pitch, c.distance, c.fov, c.ortho);
    app.cam.roll = 0.0;
  }
}

/// Moves the playhead (a scrub, the transport buttons): the camera mark moves with it.
fn set_playhead(app: &mut App, t: f32) {
  app.anim.playhead = t.max(0.0);
  app.anim.playing = None;
  app.anim.view_mark = Some(app.cam);
}

/// The triad Transform Components (move and turn) or Manual Explode (one arrow) shows.
pub fn triad(app: &App) -> Option<Triad> {
  if !app.anim.active || app.dialog.is_some() || picked(app) == 0 {
    return None;
  }
  match &app.anim.dialog {
    Some(AnimDialog::TransformComponents) => {
      let (pivot, translate, angles) = match app.anim.pending.as_ref().filter(|m| m.for_sel == app.s.selection) {
        Some(m) => (m.pivot, m.translate, m.angles),
        None => (home_centre(app)?, DVec3::ZERO, [0.0; 3]),
      };
      let mut t = Triad::world(pivot + translate);
      t.home = pivot;
      t.angles = angles;
      Some(t)
    }
    Some(AnimDialog::ManualExplode { axis, distance }) => {
      let axis = (*axis).min(2);
      let c = home_centre(app)?;
      let dir = [DVec3::X, DVec3::Y, DVec3::Z][axis];
      let mut t = Triad::world(c + dir * *distance);
      t.home = c;
      t.arrows = [false; 3];
      t.arrows[axis] = true;
      t.rings = [false; 3];
      t.planes = [false; 3];
      Some(t)
    }
    _ => None,
  }
}

/// The picked components' centre where the storyboard has them at the playhead.
fn home_centre(app: &App) -> Option<DVec3> {
  let i = app.anim.current.min(storyboards(app).len() - 1);
  let now = fox_engine::animation::pose(&app.s, i, app.anim.playhead as f64);
  fox_engine::animation::centre(&app.s, &now, &picked_bodies(app))
}

/// Takes a triad drag (or a typed value) into Transform Components' move or Manual Explode's
/// distance.
pub fn set_triad(app: &mut App, edit: TriadEdit) {
  match app.anim.dialog.clone() {
    Some(AnimDialog::TransformComponents) => {
      if app.anim.pending.as_ref().is_none_or(|m| m.for_sel != app.s.selection) {
        let Some(pivot) = home_centre(app) else { return };
        app.anim.pending =
          Some(Pending { for_sel: app.s.selection.clone(), pivot, translate: DVec3::ZERO, angles: [0.0; 3] });
      }
      let Some(m) = app.anim.pending.as_mut() else { return };
      match edit {
        TriadEdit::Move(p) => m.translate = p - m.pivot,
        TriadEdit::Rotate(i, a) => m.angles[i.min(2)] = a,
        TriadEdit::Orient(q) => m.angles = crate::manipulators::xyz_of(q),
      }
    }
    Some(AnimDialog::ManualExplode { axis, .. }) => {
      let Some(c) = home_centre(app) else { return };
      if let TriadEdit::Move(p) = edit {
        let dir = [DVec3::X, DVec3::Y, DVec3::Z][axis.min(2)];
        app.anim.dialog = Some(AnimDialog::ManualExplode { axis, distance: (p - c).dot(dir) });
      }
    }
    _ => {}
  }
}

/// Where a pointer ray first meets a body shown in the pose (moved where the pose has it).
pub fn ray_hit(app: &App, pose: &Pose, o: DVec3, d: DVec3) -> Option<(DVec3, BodyId)> {
  let mut best: Option<(f64, BodyId)> = None;
  for body in &app.s.model.bodies {
    if !pose.shows(body.id, app.s.body_visible(body.id)) {
      continue;
    }
    let Some(m) = body.mesh() else { continue };
    let to_body = pose.transform(body.id).inverse();
    let (lo, ld) = (to_body.transform_point3(o), to_body.transform_vector3(d));
    let v = |i: u32| m.positions.get(i as usize).map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
    for tri in m.indices.chunks_exact(3) {
      let (Some(a), Some(b), Some(c)) = (v(tri[0]), v(tri[1]), v(tri[2])) else { continue };
      if let Some(t) = ray_triangle(lo, ld, a, b, c)
        && best.is_none_or(|(bt, _)| t < bt)
      {
        best = Some((t, body.id));
      }
    }
  }
  best.map(|(t, b)| (o + d * t, b))
}

/// Möller–Trumbore: the ray parameter where `o + d·t` crosses the triangle.
fn ray_triangle(o: DVec3, d: DVec3, a: DVec3, b: DVec3, c: DVec3) -> Option<f64> {
  let (e1, e2) = (b - a, c - a);
  let p = d.cross(e2);
  let det = e1.dot(p);
  if det.abs() < 1e-12 {
    return None;
  }
  let inv = 1.0 / det;
  let s = o - a;
  let u = s.dot(p) * inv;
  if !(0.0..=1.0).contains(&u) {
    return None;
  }
  let q = s.cross(e1);
  let v = d.dot(q) * inv;
  if v < 0.0 || u + v > 1.0 {
    return None;
  }
  let t = e2.dot(q) * inv;
  (t > 1e-9).then_some(t)
}

const CALLOUT_INK: Color32 = Color32::from_rgb(40, 40, 40);
const TRAIL: Color32 = Color32::from_rgb(70, 120, 210);

/// The canvas's Animation overlay: trails, the storyboard's callouts, a callout being placed and
/// Auto Explode's spacing toolbar. Returns true while it has the pointer (a callout's pick).
pub(crate) fn canvas(
  app: &mut App,
  resp: &egui::Response,
  painter: &egui::Painter,
  ctx: &egui::Context,
  project: &dyn Fn(DVec3) -> Option<Pos2>,
  ray: &dyn Fn(Pos2) -> (DVec3, DVec3),
) -> bool {
  if !app.anim.active {
    return false;
  }
  let pose = pose(app);
  if app.anim.trails {
    let i = app.anim.current.min(storyboards(app).len() - 1);
    for (from, to) in fox_engine::animation::trails(&app.s, i, app.anim.playhead as f64) {
      if let (Some(a), Some(b)) = (project(from), project(to)) {
        painter.add(egui::Shape::dashed_line(&[a, b], Stroke::new(1.2, TRAIL), 5.0, 4.0));
        painter.circle_filled(a, 2.5, TRAIL);
      }
    }
  }
  for c in &pose.callouts {
    if let Some(at) = project(c.anchor) {
      callout_label(painter, at, &c.text);
    }
  }
  let mut consumed = false;
  match app.anim.dialog.clone() {
    Some(AnimDialog::Callout { at: None, .. }) => {
      if let Some(hp) = resp.hover_pos() {
        let (o, d) = ray(hp);
        if ray_hit(app, &pose, o, d).is_some() {
          painter.circle_stroke(hp, 4.0, Stroke::new(1.5, theme::ACCENT));
        }
        consumed = true;
      }
      if resp.clicked()
        && let Some(cp) = resp.interact_pointer_pos()
      {
        let (o, d) = ray(cp);
        if let Some((p, b)) = ray_hit(app, &pose, o, d) {
          app.anim.dialog = Some(AnimDialog::Callout { at: Some((p, Some(b))), text: String::new() });
          app.status = Some(("Type the callout's text, then press Enter".into(), false));
        }
      }
    }
    Some(AnimDialog::Callout { at: Some((p, b)), mut text }) => {
      if let Some(at) = project(p) {
        callout_editor(app, ctx, painter, at, p, b, &mut text);
      }
    }
    Some(AnimDialog::AutoExplode { all, scale, start }) => explode_toolbar(app, ctx, all, scale, start),
    _ => {}
  }
  consumed
}

/// A callout: a leader from the point up and right to its text in a white box.
fn callout_label(painter: &egui::Painter, at: Pos2, text: &str) -> Rect {
  let elbow = at + vec2(36.0, -36.0);
  let font = egui::FontId::proportional(12.0);
  let galley = painter.layout_no_wrap(text.to_string(), font, CALLOUT_INK);
  let r = Rect::from_min_size(elbow + vec2(4.0, -galley.size().y / 2.0 - 4.0), galley.size() + vec2(12.0, 8.0));
  painter.circle_filled(at, 2.5, CALLOUT_INK);
  painter.line_segment([at, elbow], Stroke::new(1.0, CALLOUT_INK));
  painter.line_segment([elbow, pos2(r.left(), elbow.y)], Stroke::new(1.0, CALLOUT_INK));
  painter.rect(r, 2.0, Color32::WHITE, Stroke::new(1.0, CALLOUT_INK), egui::StrokeKind::Inside);
  painter.galley(r.min + vec2(6.0, 4.0), galley, CALLOUT_INK);
  r
}

/// The callout's text box at its point: Enter places it, Escape drops it.
fn callout_editor(
  app: &mut App,
  ctx: &egui::Context,
  painter: &egui::Painter,
  at: Pos2,
  point: DVec3,
  body: Option<BodyId>,
  text: &mut String,
) {
  let elbow = at + vec2(36.0, -36.0);
  painter.circle_filled(at, 2.5, CALLOUT_INK);
  painter.line_segment([at, elbow], Stroke::new(1.0, CALLOUT_INK));
  let (mut enter, mut esc) = (false, false);
  egui::Area::new(egui::Id::new("anim-callout-text"))
    .fixed_pos(elbow + vec2(4.0, -12.0))
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      egui::Frame::popup(ui.style()).inner_margin(3.0).stroke(Stroke::new(1.0, CALLOUT_INK)).show(ui, |ui| {
        let r = ui.add(egui::TextEdit::singleline(text).desired_width(140.0).hint_text("Callout text"));
        if !r.has_focus() && !r.lost_focus() {
          r.request_focus();
        }
        (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
      });
    });
  app.anim.dialog = Some(AnimDialog::Callout { at: Some((point, body)), text: text.clone() });
  if esc {
    close(app);
    app.status = None;
  } else if enter {
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    confirm(app);
  }
}

/// Auto Explode's toolbar over the canvas: the spacing slider, Trails, OK and Cancel.
fn explode_toolbar(app: &mut App, ctx: &egui::Context, all: bool, mut scale: f32, start: f32) {
  let (mut ok, mut cancel, mut changed) = (false, false, false);
  let mut trails = app.anim.trails;
  egui::Area::new(egui::Id::new("anim-explode-toolbar"))
    .fixed_pos(pos2(app.canvas.center().x, app.canvas.bottom() - 16.0))
    .pivot(egui::Align2::CENTER_BOTTOM)
    .order(egui::Order::Foreground)
    .show(ctx, |ui| {
      egui::Frame::popup(ui.style()).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
        ui.horizontal(|ui| {
          ui.label("Explode Scale");
          ui.spacing_mut().slider_width = 140.0;
          let r = ui.add(egui::Slider::new(&mut scale, 0.1..=4.0).show_value(false));
          changed = r.drag_stopped() || r.changed() && !r.dragged();
          ui.checkbox(&mut trails, "Show Trails");
          if ui.button("OK").clicked() {
            ok = true;
          }
          if ui.button("Cancel").clicked() {
            cancel = true;
          }
        });
      });
    });
  app.anim.trails = trails;
  if cancel {
    close(app);
    return;
  }
  app.anim.dialog = Some(AnimDialog::AutoExplode { all, scale, start });
  if changed {
    re_explode(app, all, scale, start);
  }
  if ok && app.anim.dialog.is_some() {
    app.anim.dialog = None;
  }
}

// ---------------------------------------------------------------- the docked dialogs

/// Draws the open dialog, docked where command dialogs dock.
pub fn show_dialog(app: &mut App, ctx: &egui::Context) {
  let Some(mut d) = app.anim.dialog.clone() else { return };
  if !app.anim.active || app.dialog.is_some() || app.doc_dialog.is_some() {
    // another workspace, or a command started from elsewhere takes the dialog's place
    app.anim.dialog = None;
    app.anim.pending = None;
    return;
  }
  if !d.docked() {
    return;
  }
  let width = 300.0;
  let icon = d.icon();
  let accent = icons::accent_of(icon);
  let n = picked(app);
  let dock = panels::dialog_dock(app.canvas, false);
  let (mut ok, mut cancel) = (false, false);
  let enabled = can_ok(app);
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
          egui::Grid::new("anim-dlg").num_columns(2).min_col_width(96.0).spacing([10.0, 7.0]).show(ui, |ui| {
            if d.picks() {
              ui.label("Components");
              // the only selection input, so it is the active one: lit from the start
              let text = if n == 0 { "Select".to_string() } else { format!("{n} selected") };
              crate::dialogs::sel_chip_lit(ui, accent, &text, true, n > 0);
              ui.end_row();
            }
            match &mut d {
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
              AnimDialog::ManualExplode { axis, distance } => {
                ui.label("Direction");
                egui::ComboBox::from_id_salt("explode-axis").selected_text(["X", "Y", "Z"][(*axis).min(2)]).show_ui(
                  ui,
                  |ui| {
                    for (i, a) in ["X", "Y", "Z"].iter().enumerate() {
                      ui.selectable_value(axis, i, *a);
                    }
                  },
                );
                ui.end_row();
                ui.label("Distance");
                ui.add(egui::DragValue::new(distance).suffix(" mm").speed(0.5));
                ui.end_row();
              }
              AnimDialog::Appearance { opacity } => {
                ui.label("Opacity");
                ui.add(egui::Slider::new(opacity, 0.0..=100.0).suffix(" %").fixed_decimals(0));
                ui.end_row();
              }
              AnimDialog::Publish { all, resolution } => {
                ui.label("Video Scope");
                let scope = if *all { "All storyboards" } else { "Current storyboard" };
                egui::ComboBox::from_id_salt("video-scope").selected_text(scope).width(170.0).show_ui(ui, |ui| {
                  ui.selectable_value(all, false, "Current storyboard");
                  ui.selectable_value(all, true, "All storyboards");
                });
                ui.end_row();
                ui.label("Video Resolution");
                egui::ComboBox::from_id_salt("video-res").selected_text(resolution.label()).width(170.0).show_ui(
                  ui,
                  |ui| {
                    for r in Resolution::ALL {
                      ui.selectable_value(resolution, r, r.label());
                    }
                  },
                );
                ui.end_row();
              }
              AnimDialog::TransformComponents | AnimDialog::AutoExplode { .. } | AnimDialog::Callout { .. } => {}
            }
          });
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
              if crate::doc_dialogs::ok_button(ui, "OK", accent, enabled).clicked() {
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

// ---------------------------------------------------------------- the ANIMATION TIMELINE

const HEADER_H: f32 = 24.0;
const BODY_H: f32 = 104.0;
const BAR_H: f32 = 28.0;
/// The scale column at the left of the ruler.
const SCALE_W: f32 = 56.0;
/// The ruler's band at the top of the body.
const RULER_H: f32 = 22.0;
/// A track row under the ruler.
const ROW_H: f32 = 16.0;
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

/// Advances the playhead while playing; stops at the storyboard's end (`end`).
pub fn step_playback(a: &mut AnimUi, now: f64, end: f32) {
  let Some(last) = a.playing else { return };
  a.playhead = (a.playhead + (now - last) as f32).min(end);
  a.playing = if a.playhead >= end { None } else { Some(now) };
}

/// Play: from the start again when the playhead is at the end, as Fusion's.
pub fn play(app: &mut App, now: f64) {
  if app.anim.playhead >= length(app) {
    app.anim.playhead = 0.0;
  }
  app.anim.playing = Some(now);
}

/// The ANIMATION TIMELINE, at the bottom in place of the design timeline.
pub fn timeline(app: &mut App, ui: &mut egui::Ui) {
  let now = ui.input(|i| i.time);
  current(app);
  if app.anim.playing.is_some() {
    let end = length(app);
    step_playback(&mut app.anim, now, end);
    ui.ctx().request_repaint();
  }
  follow_camera(app);
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

/// A track's name: what its actions act on (View and callouts have rows of their own).
fn row_key(app: &App, kind: &ActionKind) -> String {
  match kind {
    ActionKind::View { .. } => "View".into(),
    ActionKind::Callout { .. } => "Callouts".into(),
    k => {
      let Some(t) = k.targets() else { return String::new() };
      let mut names: Vec<String> = t
        .components
        .iter()
        .filter_map(|c| app.s.design.component(*c).map(|c| c.name.clone()))
        .chain(t.bodies.iter().map(|b| app.s.body_name(*b)))
        .collect();
      names.sort();
      names.join(", ")
    }
  }
}

/// The storyboard's tracks: one row per thing acted on (views first), each with its actions.
pub fn tracks(app: &App, sb: &Storyboard) -> Vec<(String, Vec<u32>)> {
  let mut rows: Vec<(String, Vec<u32>)> = Vec::new();
  let mut actions: Vec<_> = sb.actions.iter().collect();
  actions.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.id.cmp(&b.id)));
  for a in actions {
    let key = row_key(app, &a.kind);
    match rows.iter_mut().find(|(k, _)| *k == key) {
      Some((_, ids)) => ids.push(a.id),
      None => rows.push((key, vec![a.id])),
    }
  }
  rows.sort_by_key(|(k, _)| k.as_str() != "View");
  rows
}

fn bar_color(kind: &ActionKind) -> Color32 {
  match kind {
    ActionKind::Transform { .. } => Color32::from_rgb(92, 150, 220),
    ActionKind::Visibility { .. } => Color32::from_rgb(150, 150, 150),
    ActionKind::Opacity { .. } => Color32::from_rgb(160, 125, 205),
    ActionKind::Callout { .. } => Color32::from_rgb(235, 150, 40),
    ActionKind::View { .. } => Color32::from_rgb(60, 170, 165),
  }
}

/// The scale column, the time ruler with the keyframe marker at 0 and the playhead, and the
/// tracks below with the storyboard's actions; a click or drag on the ruler moves the playhead,
/// a bar is picked with a click, moved with a drag and lengthened by its right end.
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
  bars(app, ui, &p, track, band, pps);
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
    set_playhead(app, time_at(pos, track.left(), pps));
  }
}

/// The storyboard's action bars in their track rows, and their picking and dragging.
fn bars(app: &mut App, ui: &mut egui::Ui, p: &egui::Painter, track: Rect, band: Rect, pps: f32) {
  let Some(sb) = storyboards(app).get(app.anim.current).cloned() else { return };
  let rows = tracks(app, &sb);
  let area = Rect::from_min_max(pos2(track.left(), band.bottom()), track.max);
  let fit = ((area.height() - 4.0) / ROW_H).floor().max(1.0) as usize;
  app.anim.scroll = app.anim.scroll.min(rows.len().saturating_sub(fit));
  let area_resp = ui.interact(area, ui.id().with("anim-tracks"), egui::Sense::click());
  if area_resp.hovered() {
    let wheel = ui.input(|i| i.smooth_scroll_delta.y);
    if wheel < -1.0 {
      app.anim.scroll = (app.anim.scroll + 1).min(rows.len().saturating_sub(fit));
    } else if wheel > 1.0 {
      app.anim.scroll = app.anim.scroll.saturating_sub(1);
    }
  }
  if area_resp.clicked() {
    app.anim.selected = None;
  }
  let font = egui::FontId::proportional(10.5);
  let mut released: Option<BarDrag> = None;
  for (row, (name, ids)) in rows.iter().enumerate().skip(app.anim.scroll).take(fit) {
    let y = area.top() + 3.0 + (row - app.anim.scroll) as f32 * ROW_H;
    for id in ids {
      let Some(a) = sb.action(*id) else { continue };
      let (mut start, mut dur) = (a.start, a.duration);
      if let Some(d) = app.anim.bar_drag.filter(|d| d.action == *id) {
        if d.resize {
          dur = d.to;
        } else {
          start = d.to;
        }
      }
      let x0 = track.left() + start as f32 * pps;
      let w = (dur as f32 * pps).max(7.0);
      let r = Rect::from_min_size(pos2(x0, y), vec2(w, ROW_H - 3.0));
      let picked = app.anim.selected == Some(*id);
      let fill = bar_color(&a.kind);
      if matches!(a.kind, ActionKind::Callout { .. }) {
        let c = pos2(x0, r.center().y);
        let pts = vec![c + vec2(0.0, -5.5), c + vec2(5.5, 0.0), c + vec2(0.0, 5.5), c + vec2(-5.5, 0.0)];
        p.add(egui::Shape::convex_polygon(pts, fill, Stroke::new(if picked { 2.0 } else { 0.0 }, theme::ACCENT)));
      } else {
        p.rect(
          r,
          2.0,
          fill,
          Stroke::new(if picked { 2.0 } else { 0.5 }, if picked { theme::ACCENT } else { fill }),
          egui::StrokeKind::Inside,
        );
        let label = a.kind.label();
        if p.layout_no_wrap(label.into(), font.clone(), Color32::WHITE).size().x + 6.0 < w {
          p.text(r.left_center() + vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, label, font.clone(), Color32::WHITE);
        }
      }
      let hit = r.expand2(vec2(4.0, 0.0));
      let resp = ui.interact(hit, ui.id().with(("anim-bar", *id)), egui::Sense::click_and_drag());
      let resp = resp.on_hover_text(format!("{} · {name}\n{:.1} s – {:.1} s", a.kind.label(), a.start, a.end()));
      if resp.clicked() {
        app.anim.selected = Some(*id);
      }
      let at_end = |x: f32| x > r.right() - 5.0 && !matches!(a.kind, ActionKind::Callout { .. });
      if resp.hovered() && resp.hover_pos().is_some_and(|h| at_end(h.x)) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
      }
      if resp.drag_started()
        && let Some(g) = resp.interact_pointer_pos()
      {
        let resize = at_end(g.x);
        app.anim.selected = Some(*id);
        app.anim.bar_drag = Some(BarDrag {
          action: *id,
          resize,
          grab_x: g.x,
          start: a.start,
          duration: a.duration,
          to: if resize { a.duration } else { a.start },
        });
      }
      if resp.dragged()
        && let (Some(g), Some(d)) = (resp.interact_pointer_pos(), app.anim.bar_drag.as_mut())
        && d.action == *id
      {
        let dt = ((g.x - d.grab_x) / pps) as f64;
        // to a tenth of a second, never before 0
        let snap = |t: f64| ((t * 10.0).round() / 10.0).max(0.0);
        d.to = if d.resize { snap(d.duration + dt) } else { snap(d.start + dt) };
      }
      if resp.drag_stopped() && app.anim.bar_drag.is_some_and(|d| d.action == *id) {
        released = app.anim.bar_drag.take();
      }
    }
  }
  if let Some(d) = released {
    let i = app.anim.current;
    let (key, was) = if d.resize { ("duration", d.duration) } else { ("start", d.start) };
    if (d.to - was).abs() > 1e-9 {
      let mut p = json!({"storyboard": i, "action": d.action});
      p[key] = json!(d.to);
      app.run("animation.action.edit", p);
    }
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
  let boards = storyboards(app);
  for (i, sb) in boards.iter().enumerate() {
    if storyboard_tab(ui, &sb.name, i == app.anim.current).clicked() {
      app.anim.current = i;
      app.anim.selected = None;
      set_playhead(app, 0.0);
    }
  }
  if icons::icon_button(ui, Icon::Plus, 20.0, "New Storyboard", false, ink).clicked() {
    AnimCmd::NewStoryboard.run(app);
  }
  ui.add_space(12.0);
  if icons::icon_button(ui, Icon::ToStart, 22.0, "Go to start", false, ink).clicked() {
    set_playhead(app, 0.0);
  }
  let playing = app.anim.playing.is_some();
  if icons::icon_button(ui, Icon::Play, 22.0, if playing { "Pause" } else { "Play" }, playing, ink).clicked() {
    if playing {
      app.anim.playing = None;
    } else {
      play(app, now);
    }
  }
  if icons::icon_button(ui, Icon::ToEnd, 22.0, "Go to end", false, ink).clicked() {
    let end = length(app);
    set_playhead(app, end);
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
  use fox_engine::model::ComponentId;
  use fox_engine::model::animation::StoryboardKind;

  fn kind_of(app: &App, i: usize) -> StoryboardKind {
    storyboards(app)[i].kind
  }

  /// Two 10 mm cubes, each its own component, at x = 0 and 30; the Animation workspace open.
  fn assembly() -> (App, [ComponentId; 2], [BodyId; 2]) {
    let mut app = App::with_session(Session::default());
    let mut comps = [ComponentId(0); 2];
    let mut bodies = [BodyId(0); 2];
    for (k, x) in [0.0, 30.0].into_iter().enumerate() {
      app.run("box", json!({"width": 10, "depth": 10, "height": 10}));
      let b = app.s.model.bodies.last().unwrap().id;
      if x != 0.0 {
        app.run("move", json!({"bodies": [b.0], "translate": [x, 0, 0]}));
      }
      let c = app.run("component.create", json!({"name": format!("Part{}", k + 1), "bodies": [b.0]})).unwrap();
      comps[k] = ComponentId(c["component"].as_u64().unwrap() as u32);
      bodies[k] = b;
    }
    app.s.selection.clear();
    crate::panels::switch_workspace(&mut app, "Animation");
    (app, comps, bodies)
  }

  fn at(app: &App, b: BodyId) -> DVec3 {
    pose(app).transform(b).translation
  }

  #[test]
  fn storyboards_start_with_one_and_number_on() {
    let mut app = App::with_session(Session::default());
    assert_eq!(storyboards(&app).iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Storyboard1"]);
    assert_eq!(app.s.design.animation.next_name(), "Storyboard2");
    crate::panels::switch_workspace(&mut app, "Animation");
    AnimCmd::NewStoryboard.run(&mut app);
    app.anim.dialog = Some(AnimDialog::NewStoryboard { kind: StoryboardType::StartFromEnd });
    confirm(&mut app);
    assert_eq!(storyboards(&app)[1].name, "Storyboard2");
    assert_eq!(kind_of(&app, 1), StoryboardKind::StartFromEnd);
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
    step_playback(&mut a, 1.5, 0.0);
    assert_eq!((a.playhead, a.playing), (0.0, None));
    let mut a = AnimUi { playing: Some(1.0), ..AnimUi::default() };
    step_playback(&mut a, 1.5, 2.0);
    assert_eq!((a.playhead, a.playing), (0.5, Some(1.5)));
  }

  #[test]
  fn new_storyboard_adds_one_on_ok_and_cancel_drops_it() {
    let mut app = App::with_session(Session::default());
    crate::panels::switch_workspace(&mut app, "Animation");
    AnimCmd::NewStoryboard.run(&mut app);
    assert_eq!(app.anim.dialog, Some(AnimDialog::NewStoryboard { kind: StoryboardType::Clean }));
    assert_eq!(app.anim.dialog.as_ref().map(AnimDialog::title), Some("NEW STORYBOARD"));
    assert!(AnimCmd::NewStoryboard.active(&app));
    assert!(can_ok(&app), "New Storyboard's OK is enabled from the start");
    assert!(close(&mut app));
    assert_eq!(storyboards(&app).len(), 1, "Cancel adds nothing");
    AnimCmd::NewStoryboard.run(&mut app);
    confirm(&mut app);
    assert!(app.anim.dialog.is_none());
    assert_eq!(storyboards(&app)[1].name, "Storyboard2");
    assert_eq!(app.anim.current, 1, "the new storyboard is shown");
  }

  #[test]
  fn transform_components_drags_a_triad_and_records_the_move() {
    let (mut app, comps, bodies) = assembly();
    crate::panels::Cmd::Anim(AnimCmd::TransformComponents).run(&mut app);
    assert_eq!(app.anim.dialog.as_ref().map(AnimDialog::title), Some("TRANSFORM COMPONENTS"));
    assert_eq!(picked(&app), 0);
    assert!(!can_ok(&app), "OK is greyed with Components at Select");
    assert!(triad(&app).is_none(), "no triad until a component is picked");
    confirm(&mut app);
    assert!(app.anim.dialog.is_some(), "a greyed OK does nothing");
    app.s.selection = vec![fox_engine::Sel::Body { body: bodies[1] }];
    assert_eq!(picked_targets(&app).components, vec![comps[1]]);
    let t = triad(&app).expect("the triad on the component");
    assert!(t.origin.abs_diff_eq(DVec3::new(30.0, 0.0, 5.0), 1e-6), "at its centre: {:?}", t.origin);
    // dragging the Z arrow up 15 and turning about Z previews the move
    crate::manipulators::apply_edit(&mut app, TriadEdit::Move(t.origin + DVec3::Z * 15.0));
    crate::manipulators::apply_edit(&mut app, TriadEdit::Rotate(2, std::f64::consts::FRAC_PI_2));
    let m = pose(&app).transform(bodies[1]);
    assert!(m.transform_point3(t.origin).abs_diff_eq(t.origin + DVec3::Z * 15.0, 1e-6));
    let side = m.transform_point3(t.origin + DVec3::X);
    assert!(side.abs_diff_eq(t.origin + DVec3::Z * 15.0 + DVec3::Y, 1e-6), "turned a quarter: {side:?}");
    assert!(storyboards(&app)[0].actions.is_empty(), "nothing recorded before OK");
    confirm(&mut app);
    assert!(app.anim.dialog.is_none());
    let sb = &storyboards(&app)[0];
    assert_eq!(sb.actions.len(), 1);
    assert_eq!(sb.actions[0].kind.label(), "Transform");
    assert_eq!(app.anim.playhead, 1.0, "the playhead moves to the action's end");
    assert!((at(&app, bodies[1]) - DVec3::new(0.0, 0.0, 0.0)).length() > 1.0);
    // back to the start the component is home again
    set_playhead(&mut app, 0.0);
    assert!(pose(&app).transforms.is_empty());
    // Escape closes the dialog, changing nothing
    AnimCmd::TransformComponents.run(&mut app);
    app.escape();
    assert!(app.anim.dialog.is_none());
    assert_eq!(storyboards(&app)[0].actions.len(), 1);
  }

  #[test]
  fn explode_restore_home_show_hide_and_appearance_record_actions() {
    let (mut app, comps, bodies) = assembly();
    AnimCmd::ExplodeAll.run(&mut app);
    assert!(matches!(app.anim.dialog, Some(AnimDialog::AutoExplode { all: true, .. })));
    assert!(AnimCmd::ExplodeAll.active(&app));
    let n = storyboards(&app)[0].actions.len();
    assert_eq!(n, 1, "Part2 moves away from Part1");
    let away = at(&app, bodies[1]);
    assert!(away.x > 1.0, "{away:?}");
    // the spacing slider records it again further apart
    re_explode(&mut app, true, 2.0, 0.0);
    assert_eq!(storyboards(&app)[0].actions.len(), 1, "replaced, not added");
    assert!(at(&app, bodies[1]).x > away.x * 1.5);
    app.anim.dialog = None;
    // Restore Home brings it back
    AnimCmd::RestoreHome.run(&mut app);
    assert_eq!(storyboards(&app)[0].actions.len(), 2);
    assert!(at(&app, bodies[1]).length() < 1e-6, "home: {:?}", at(&app, bodies[1]));
    // Show/Hide needs a pick, then fades the component out
    AnimCmd::ShowHide.run(&mut app);
    assert_eq!(storyboards(&app)[0].actions.len(), 2);
    app.s.selection = vec![fox_engine::Sel::Body { body: bodies[0] }];
    AnimCmd::ShowHide.run(&mut app);
    let p = pose(&app);
    assert!(!p.shows(bodies[0], true), "hidden at the playhead");
    let mut o = fox_engine::scene::ViewOptions::default();
    dress_options(&app, &mut o);
    assert_eq!(o.only_bodies, Some(vec![bodies[1]]), "the canvas draws only what shows");
    // Appearance: opacity 30 %
    app.s.selection = vec![fox_engine::Sel::Body { body: bodies[1] }];
    AnimCmd::Appearance.run(&mut app);
    app.anim.dialog = Some(AnimDialog::Appearance { opacity: 30.0 });
    confirm(&mut app);
    let p = pose(&app);
    assert!((p.opacity[&bodies[1]] - 0.3).abs() < 1e-6);
    // Manual Explode along X by 40 mm through its arrow
    app.s.selection = bodies.iter().map(|b| fox_engine::Sel::Body { body: *b }).collect();
    AnimCmd::ManualExplode.run(&mut app);
    app.anim.dialog = Some(AnimDialog::ManualExplode { axis: 0, distance: 0.0 });
    assert!(!can_ok(&app), "a distance of 0 records nothing");
    let t = triad(&app).unwrap();
    assert_eq!(t.arrows, [true, false, false], "one arrow, along the direction");
    crate::manipulators::apply_edit(&mut app, TriadEdit::Move(t.origin + DVec3::X * 40.0));
    assert_eq!(app.anim.dialog, Some(AnimDialog::ManualExplode { axis: 0, distance: 40.0 }));
    confirm(&mut app);
    let p = pose(&app);
    assert!((p.transform(bodies[0]).translation.x - 20.0).abs() < 1e-6);
    assert!((p.transform(bodies[1]).translation.x - 40.0).abs() < 1e-6);
    let _ = comps;
    // the timeline's rows: one per component acted on
    let rows = tracks(&app, &storyboards(&app)[0]);
    assert_eq!(rows.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["Part2", "Part1"]);
    // Delete removes the picked action; undo puts it back
    app.anim.selected = Some(storyboards(&app)[0].actions.last().unwrap().id);
    let before = storyboards(&app)[0].actions.len();
    assert!(delete_selected(&mut app));
    assert_eq!(storyboards(&app)[0].actions.len(), before - 1);
    app.s.undo();
    assert_eq!(storyboards(&app)[0].actions.len(), before);
  }

  #[test]
  fn callouts_views_play_and_publish() {
    let (mut app, _, bodies) = assembly();
    AnimCmd::Callout.run(&mut app);
    assert!(!can_ok(&app), "no point picked yet");
    // a ray straight down onto Part1's top
    let p = pose(&app);
    let hit = ray_hit(&app, &p, DVec3::new(1.0, 2.0, 100.0), -DVec3::Z).unwrap();
    assert!(hit.0.abs_diff_eq(DVec3::new(1.0, 2.0, 10.0), 1e-6) && hit.1 == bodies[0], "{hit:?}");
    app.anim.dialog = Some(AnimDialog::Callout { at: Some((hit.0, Some(hit.1))), text: "Lid".into() });
    confirm(&mut app);
    assert_eq!(pose(&app).callouts.len(), 1);
    // View records the camera's move; scrubbing back follows it
    let start = app.cam;
    app.anim.view_mark = Some(start);
    app.cam.yaw += 1.0;
    let end = app.cam;
    AnimCmd::View.run(&mut app);
    assert_eq!(app.anim.playhead, 1.0);
    set_playhead(&mut app, 0.0);
    follow_camera(&mut app);
    assert!((app.cam.yaw - start.yaw).abs() < 1e-9, "the storyboard's camera at 0");
    // Play runs to the end
    play(&mut app, 10.0);
    let end_t = length(&app);
    step_playback(&mut app.anim, 10.6, end_t);
    assert!((app.anim.playhead - 0.6).abs() < 1e-5);
    step_playback(&mut app.anim, 12.0, end_t);
    assert_eq!((app.anim.playhead, app.anim.playing), (1.0, None));
    follow_camera(&mut app);
    assert!((app.cam.yaw - end.yaw).abs() < 1e-9);
    // Publish Video writes the storyboard as an AVI
    app.canvas_px = [40, 30];
    let dir = std::env::temp_dir().join(format!("fox-anim-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("v.avi");
    let r = publish_to(&mut app, &path, false, Resolution::Window).expect("published");
    assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(40), Some(30)));
    assert_eq!(&std::fs::read(&path).unwrap()[..4], b"RIFF");
    let _ = std::fs::remove_dir_all(&dir);
  }
}
