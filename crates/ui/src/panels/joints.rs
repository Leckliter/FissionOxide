//! Joints in the browser (Relationships > Joints, as in Fusion) and on the canvas: the joint
//! right-click menu shared by both, Edit Motion Limits, and Animate Joint.

use egui::{Pos2, vec2};
use fox_engine::Sel;
use fox_engine::model::{BodyId, ComponentId, FeatureId, FeatureKind, JointKind};
use serde_json::json;

use super::{browser_assembly, component_bodies, is_open, menu_item, reveal_row, tree_row};
use crate::dialogs::Kind;
use crate::icons::Icon;
use crate::{App, RenameTarget, marking, theme};

/// Joint UI state kept between frames.
#[derive(Default)]
pub(crate) struct JointUi {
  /// The joint whose browser row the pointer was on last frame: its canvas glyph lights up.
  pub(crate) hover: Option<FeatureId>,
  /// A canvas glyph's right-click menu: the joint, and where it opened.
  pub(crate) popup: Option<(FeatureId, Pos2)>,
  /// Find in Timeline: the timeline scrolls this feature into view.
  pub(crate) reveal_timeline: Option<FeatureId>,
  limits: Option<LimitsEdit>,
  anim: Option<Anim>,
}

/// The Edit Motion Limits window's fields.
struct LimitsEdit {
  joint: FeatureId,
  rotation: Option<(String, String)>,
  slide: Option<(String, String)>,
}

/// Animate Joint: the joint swings through its motion and back, then returns to where it was.
struct Anim {
  joint: FeatureId,
  field: &'static str,
  from: String,
  to: String,
  start: f64,
  last: f64,
  /// A drive step is applied (undone before the next and at the end).
  applied: bool,
}

/// How long one swing there and back takes, and how many swings an animation plays.
const ANIM_PERIOD: f64 = 3.0;
const ANIM_SWINGS: f64 = 2.0;

/// A command on a joint's right-click menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum JointCmd {
  Drive,
  Edit,
  Limits,
  Lock,
  Suppress,
  Animate,
  AnimateRelationships,
  MotionStudy,
  SelectComponents,
  Delete,
  Rename,
  ShowHide,
  FindInWindow,
  FindInTimeline,
  /// Listed, greyed, as Fusion has it but FissionOxide doesn't yet.
  NotYet,
}

/// One row of the joint menu.
pub(crate) struct JointItem {
  pub icon: Icon,
  pub label: String,
  pub key: Option<&'static str>,
  pub cmd: JointCmd,
}

fn item(icon: Icon, label: &str, key: Option<&'static str>, cmd: JointCmd) -> Option<JointItem> {
  Some(JointItem { icon, label: label.into(), key, cmd })
}

/// A joint's right-click menu, as Fusion's (`None` is a separator).
pub(crate) fn joint_menu(app: &App, fid: FeatureId) -> Vec<Option<JointItem>> {
  let f = app.s.design.feature(fid);
  let locked = matches!(f.map(|f| &f.kind), Some(FeatureKind::Joint { locked: true, .. }));
  let suppressed = f.is_some_and(|f| f.suppressed);
  let visible = f.is_none_or(|f| f.visible != Some(false));
  vec![
    item(Icon::Select, "Create Selection Set", None, JointCmd::NotYet),
    item(Icon::Configure, "Configure", None, JointCmd::NotYet),
    None,
    item(Icon::DriveJoint, "Drive Joints", None, JointCmd::Drive),
    item(Icon::JointOrigin, "Edit Joint", None, JointCmd::Edit),
    item(Icon::Measure, "Edit Motion Limits", None, JointCmd::Limits),
    item(Icon::Fix, if locked { "Unlock Motion" } else { "Lock Motion" }, None, JointCmd::Lock),
    item(Icon::Remove, if suppressed { "Unsuppress" } else { "Suppress" }, None, JointCmd::Suppress),
    item(Icon::Play, "Animate Joint", None, JointCmd::Animate),
    item(Icon::Play, "Animate Joint Relationships", None, JointCmd::AnimateRelationships),
    item(Icon::MotionStudy, "Motion Study", None, JointCmd::MotionStudy),
    None,
    item(Icon::Component, "Select Components", None, JointCmd::SelectComponents),
    None,
    item(Icon::Remove, "Delete", Some("Del"), JointCmd::Delete),
    item(Icon::Params, "Rename", None, JointCmd::Rename),
    None,
    item(if visible { Icon::EyeOff } else { Icon::Eye }, "Show/Hide", Some("V"), JointCmd::ShowHide),
    item(Icon::Fit, "Find in Window", None, JointCmd::FindInWindow),
    item(Icon::Clock, "Find in Timeline", None, JointCmd::FindInTimeline),
  ]
}

/// Draws the joint menu; returns the command picked.
pub(crate) fn show_menu(ui: &mut egui::Ui, items: &[Option<JointItem>]) -> Option<JointCmd> {
  let mut picked = None;
  for it in items {
    let Some(it) = it else {
      ui.separator();
      continue;
    };
    if it.cmd == JointCmd::NotYet {
      ui.add_enabled_ui(false, |ui| menu_item(ui, it.icon, &it.label, it.key))
        .inner
        .on_disabled_hover_text("Not in FissionOxide yet");
    } else if menu_item(ui, it.icon, &it.label, it.key).clicked() {
      ui.close();
      picked = Some(it.cmd);
    }
  }
  picked
}

/// The components a joint connects: (moving, fixed; `None` = the world).
fn joint_components(app: &App, fid: FeatureId) -> Option<(ComponentId, Option<ComponentId>)> {
  match &app.s.design.feature(fid)?.kind {
    FeatureKind::Joint { moving, fixed, .. } => Some((*moving, *fixed)),
    _ => None,
  }
}

/// The bodies of the components a joint connects.
fn joint_bodies(app: &App, fid: FeatureId) -> Vec<BodyId> {
  let Some((moving, fixed)) = joint_components(app, fid) else { return Vec::new() };
  let mut v = component_bodies(app, moving.0);
  if let Some(f) = fixed {
    v.extend(component_bodies(app, f.0));
  }
  v
}

/// Runs a joint menu command.
pub(crate) fn run_joint_cmd(app: &mut App, fid: FeatureId, cmd: JointCmd) {
  let Some(f) = app.s.design.feature(fid) else { return };
  let name = f.name.clone();
  let (suppressed, visible) = (f.suppressed, f.visible != Some(false));
  let drive = |app: &mut App, kind: Kind| {
    app.start(kind);
    if let Some(d) = app.dialog.as_mut() {
      d.set_target(&app.s, fid.0);
    }
  };
  match cmd {
    JointCmd::Drive => drive(app, Kind::DriveJoint),
    JointCmd::MotionStudy => drive(app, Kind::MotionStudy),
    JointCmd::Edit => super::edit_feature(app, fid),
    JointCmd::Limits => open_limits(app, fid),
    JointCmd::Lock => {
      app.run("joint.lock", json!({"joint": fid.0}));
    }
    JointCmd::Suppress => {
      app.cancel_dialog();
      app.run("feature.suppress", json!({"feature": fid.0, "suppressed": !suppressed}));
    }
    JointCmd::Animate | JointCmd::AnimateRelationships => start_animation(app, fid),
    JointCmd::SelectComponents => {
      app.s.selection = joint_bodies(app, fid).into_iter().map(|b| Sel::Body { body: b }).collect();
    }
    JointCmd::Delete => {
      app.cancel_dialog();
      app.run("feature.delete", json!({"feature": fid.0}));
    }
    JointCmd::Rename => app.rename = Some((RenameTarget::Feature(fid.0), name)),
    JointCmd::ShowHide => {
      app.run("joint.visible", json!({"joint": fid.0, "visible": !visible}));
    }
    JointCmd::FindInWindow => {
      let bodies = joint_bodies(app, fid);
      marking::find_in_window(app, &bodies, None);
    }
    JointCmd::FindInTimeline => {
      app.s.selection = vec![Sel::Feature { feature: fid }];
      app.joints.reveal_timeline = Some(fid);
    }
    JointCmd::NotYet => {}
  }
}

/// Relationships > Joints (and the other relationship folders: rigid groups, motion links,
/// contact sets), as Fusion's browser has them. Click a joint to select it, double-click to drive
/// it; hovering a row lights its glyph up on the canvas.
pub(super) fn relationships_section(app: &mut App, ui: &mut egui::Ui) {
  let joints: Vec<(FeatureId, String, bool)> = app
    .s
    .design
    .features
    .iter()
    .filter(|f| matches!(f.kind, FeatureKind::Joint { .. }))
    .map(|f| (f.id, f.name.clone(), f.suppressed))
    .collect();
  let others = browser_assembly::has_relationships(app);
  if joints.is_empty() && !others {
    return;
  }
  let rel = egui::Id::new("tree-relationships");
  tree_row(ui, rel, 1, Some(true), None, Icon::Folder, "Relationships", false, theme::TEXT);
  if !is_open(ui, rel, true) {
    return;
  }
  if !joints.is_empty() {
    let id = egui::Id::new("tree-joints");
    tree_row(ui, id, 2, Some(true), None, Icon::Folder, "Joints", false, theme::TEXT);
    if is_open(ui, id, true) {
      for (fid, name, suppressed) in joints {
        joint_row(app, ui, id.with(fid.0), fid, &name, suppressed);
      }
    }
  }
  browser_assembly::relationship_folders(app, ui, 2);
}

fn joint_row(app: &mut App, ui: &mut egui::Ui, row: egui::Id, fid: FeatureId, name: &str, suppressed: bool) {
  let sel = Sel::Feature { feature: fid };
  let selected = app.s.selection.contains(&sel);
  let failed = app.s.model.status(fid).and_then(|st| st.error.clone());
  let color = if failed.is_some() {
    theme::WARNING
  } else if suppressed {
    theme::MUTED
  } else {
    theme::TEXT
  };
  let visible = app.s.design.feature(fid).is_some_and(|f| f.visible != Some(false));
  let ev = tree_row(ui, row, 3, None, Some(visible), Icon::JointOrigin, name, selected, color);
  if ev.eye {
    run_joint_cmd(app, fid, JointCmd::ShowHide);
  } else if ev.double_clicked {
    run_joint_cmd(app, fid, JointCmd::Drive);
  } else if ev.clicked {
    super::toggle_sel(&mut app.s, sel, ui.input(|i| i.modifiers.shift || i.modifiers.command));
  }
  let Some(r) = ev.response else { return };
  reveal_row(app, sel, &r);
  if r.hovered() {
    app.joints.hover = Some(fid);
    marking::peek(app, joint_bodies(app, fid), name.to_string());
  }
  let r = match &failed {
    Some(e) => r.on_hover_text(e.as_str()),
    None => r.on_hover_text("Double-click to drive"),
  };
  r.context_menu(|ui| {
    let items = joint_menu(app, fid);
    if let Some(cmd) = show_menu(ui, &items) {
      run_joint_cmd(app, fid, cmd);
    }
  });
}

/// The menu a right-click on a joint's canvas glyph opens.
pub fn canvas_popup(app: &mut App, ctx: &egui::Context) {
  let Some((fid, at)) = app.joints.popup else { return };
  if app.s.design.feature(fid).is_none() {
    app.joints.popup = None;
    return;
  }
  let id = egui::Id::new("joint-popup");
  let mut picked = None;
  let r = egui::Area::new(id).fixed_pos(at + vec2(2.0, 2.0)).order(egui::Order::Foreground).show(ctx, |ui| {
    egui::Frame::menu(ui.style()).show(ui, |ui| {
      ui.set_min_width(190.0);
      let items = joint_menu(app, fid);
      picked = show_menu(ui, &items);
    });
  });
  // a click elsewhere, or Escape, closes it
  let away = ctx.input(|i| {
    i.key_pressed(egui::Key::Escape)
      || (i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !r.response.rect.contains(p)))
  });
  if picked.is_some() || away {
    app.joints.popup = None;
  }
  if let Some(cmd) = picked {
    run_joint_cmd(app, fid, cmd);
  }
}

// ---------------------------------------------------------------- Edit Motion Limits

fn open_limits(app: &mut App, fid: FeatureId) {
  let Some(FeatureKind::Joint { limits, .. }) = app.s.design.feature(fid).map(|f| &f.kind) else { return };
  app.joints.limits = Some(LimitsEdit { joint: fid, rotation: limits.rotation.clone(), slide: limits.slide.clone() });
}

/// The Edit Motion Limits window: a minimum and maximum for the joint's rotation and slide.
pub fn limits_window(app: &mut App, ctx: &egui::Context) {
  let Some(mut e) = app.joints.limits.take() else { return };
  let Some(f) = app.s.design.feature(e.joint) else { return };
  let (rot, slide) = match &f.kind {
    FeatureKind::Joint { kind, .. } => {
      let (r, s, _, _) = kind.freedoms();
      (r, s)
    }
    _ => return,
  };
  let title = format!("EDIT MOTION LIMITS — {}", f.name);
  let mut keep = true;
  let mut apply = false;
  egui::Window::new(title)
    .collapsible(false)
    .resizable(false)
    .default_pos(app.canvas.center() - vec2(150.0, 80.0))
    .show(ctx, |ui| {
      let row = |ui: &mut egui::Ui, label: &str, on: bool, v: &mut Option<(String, String)>, d: (&str, &str)| {
        ui.add_enabled_ui(on, |ui| {
          let mut set = v.is_some();
          ui.checkbox(&mut set, label);
          match (set, v.is_some()) {
            (true, false) => *v = Some((d.0.into(), d.1.into())),
            (false, true) => *v = None,
            _ => {}
          }
          if let Some((lo, hi)) = v.as_mut() {
            ui.horizontal(|ui| {
              ui.label("Minimum");
              ui.add(egui::TextEdit::singleline(lo).desired_width(80.0));
              ui.label("Maximum");
              ui.add(egui::TextEdit::singleline(hi).desired_width(80.0));
            });
          }
        });
      };
      row(ui, "Rotation limits", rot, &mut e.rotation, ("0 deg", "90 deg"));
      row(ui, "Slide limits", slide, &mut e.slide, ("0 mm", "10 mm"));
      ui.separator();
      ui.horizontal(|ui| {
        if ui.button("OK").clicked() {
          apply = true;
          keep = false;
        }
        if ui.button("Cancel").clicked() {
          keep = false;
        }
      });
    });
  if apply {
    let pair = |v: &Option<(String, String)>| v.as_ref().map(|(lo, hi)| json!([lo, hi])).unwrap_or(json!(null));
    app.run("joint.limits", json!({"joint": e.joint.0, "rotation": pair(&e.rotation), "slide": pair(&e.slide)}));
  }
  if keep {
    app.joints.limits = Some(e);
  }
}

// ---------------------------------------------------------------- Animate Joint

/// The motion Animate Joint plays and its range: the joint's limits, else a full turn (or 20 mm
/// of slide).
fn anim_range(kind: JointKind, limits: &fox_engine::model::JointLimits) -> Option<(&'static str, String, String)> {
  let (rot, slide, _, _) = kind.freedoms();
  if rot {
    let (lo, hi) = limits.rotation.clone().unwrap_or(("0 deg".into(), "360 deg".into()));
    Some(("rotation", lo, hi))
  } else if slide {
    let (lo, hi) = limits.slide.clone().unwrap_or(("0 mm".into(), "20 mm".into()));
    Some(("slide", lo, hi))
  } else {
    None
  }
}

fn start_animation(app: &mut App, fid: FeatureId) {
  stop_animation(app);
  let Some(f) = app.s.design.feature(fid) else { return };
  let FeatureKind::Joint { kind, limits, locked, .. } = &f.kind else { return };
  if *locked {
    app.status = Some((format!("{} is locked: unlock its motion to animate it", f.name), true));
    return;
  }
  let Some((field, from, to)) = anim_range(*kind, limits) else {
    app.status = Some((format!("{} is rigid: it has no motion to animate", f.name), false));
    return;
  };
  let msg = format!("Animating {} (Esc or click to stop)", f.name);
  app.cancel_dialog();
  app.joints.anim =
    Some(Anim { joint: fid, field, from, to, start: f64::NAN, last: f64::NEG_INFINITY, applied: false });
  app.status = Some((msg, false));
}

fn stop_animation(app: &mut App) {
  if let Some(a) = app.joints.anim.take()
    && a.applied
  {
    app.s.undo();
    app.s.drop_redo();
  }
}

/// The value of an animation `t` seconds in: there and back once per period.
fn anim_value(from: &str, to: &str, t: f64) -> String {
  let phase = (t / ANIM_PERIOD).fract();
  let k = if phase < 0.5 { phase * 2.0 } else { 2.0 - phase * 2.0 };
  format!("({from}) + (({to}) - ({from})) * {k:.4}")
}

/// Steps a running animation; called every frame.
pub fn animate(app: &mut App, ctx: &egui::Context) {
  let Some(a) = app.joints.anim.as_mut() else { return };
  let now = ctx.input(|i| i.time);
  if a.start.is_nan() {
    a.start = now;
  }
  let t = now - a.start;
  let stop = t > ANIM_PERIOD * ANIM_SWINGS
    || app.s.design.feature(a.joint).is_none()
    || app.dialog.is_some()
    || ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.pointer.any_pressed());
  if stop {
    stop_animation(app);
    app.status = None;
    return;
  }
  ctx.request_repaint();
  // about 20 steps a second: each one rebuilds the joint's components
  if now - a.last < 0.05 {
    return;
  }
  a.last = now;
  let (joint, field, value) = (a.joint, a.field, anim_value(&a.from, &a.to, t));
  let applied = a.applied;
  if applied {
    app.s.undo();
    app.s.drop_redo();
  }
  let ok = app.s.execute("joint.drive", &json!({"joint": joint.0, field: value})).is_ok();
  if let Some(a) = app.joints.anim.as_mut() {
    a.applied = ok;
  }
  if !ok {
    app.joints.anim = None;
  }
}

/// Whether a joint is being animated.
#[cfg(test)]
pub(crate) fn animating(app: &App) -> Option<FeatureId> {
  app.joints.anim.as_ref().map(|a| a.joint)
}

#[cfg(test)]
mod tests {
  use fox_engine::Session;

  use super::*;

  /// A base and an arm jointed by a revolute; returns the app and the joint.
  pub(crate) fn hinge() -> (App, FeatureId) {
    let mut app = App::with_session(Session::default());
    for (name, at) in [("Base", [0, 0]), ("Arm", [0, 100])] {
      app.run("box", json!({"width": 20, "depth": 20, "height": 5, "at": at}));
      let b = app.s.model.bodies.last().unwrap().id.0;
      app.run("component.create", json!({"name": name, "bodies": [b]}));
    }
    let base = app.s.design.components[0].id.0;
    let arm = app.s.design.components[1].id.0;
    app.run("component.ground", json!({"component": base}));
    let j = app
      .run(
        "joint",
        json!({"moving": arm, "moving_origin": {"at": [0, 100, 0]}, "fixed": base, "fixed_origin": {"at": [0, 0, 5]}, "kind": "revolute"}),
      )
      .unwrap();
    (app, FeatureId(j["feature"].as_u64().unwrap() as u32))
  }

  #[test]
  fn joint_menu_is_fusions() {
    let (app, j) = hinge();
    let labels: Vec<String> =
      joint_menu(&app, j).iter().map(|i| i.as_ref().map(|i| i.label.clone()).unwrap_or("---".into())).collect();
    assert_eq!(
      labels,
      [
        "Create Selection Set",
        "Configure",
        "---",
        "Drive Joints",
        "Edit Joint",
        "Edit Motion Limits",
        "Lock Motion",
        "Suppress",
        "Animate Joint",
        "Animate Joint Relationships",
        "Motion Study",
        "---",
        "Select Components",
        "---",
        "Delete",
        "Rename",
        "---",
        "Show/Hide",
        "Find in Window",
        "Find in Timeline",
      ]
    );
  }

  #[test]
  fn joint_menu_commands_reach_the_engine() {
    let (mut app, j) = hinge();
    let joint = |app: &App| app.s.design.feature(j).unwrap().clone();
    run_joint_cmd(&mut app, j, JointCmd::Lock);
    assert!(matches!(joint(&app).kind, FeatureKind::Joint { locked: true, .. }));
    assert!(joint_menu(&app, j).iter().flatten().any(|i| i.label == "Unlock Motion"));
    run_joint_cmd(&mut app, j, JointCmd::Lock);
    assert!(matches!(joint(&app).kind, FeatureKind::Joint { locked: false, .. }));
    run_joint_cmd(&mut app, j, JointCmd::ShowHide);
    assert_eq!(joint(&app).visible, Some(false));
    run_joint_cmd(&mut app, j, JointCmd::ShowHide);
    assert_eq!(joint(&app).visible, None);
    run_joint_cmd(&mut app, j, JointCmd::Suppress);
    assert!(joint(&app).suppressed);
    run_joint_cmd(&mut app, j, JointCmd::Suppress);
    assert!(!joint(&app).suppressed);
    run_joint_cmd(&mut app, j, JointCmd::SelectComponents);
    assert_eq!(app.s.selection.len(), 2, "both components' bodies");
    run_joint_cmd(&mut app, j, JointCmd::FindInTimeline);
    assert_eq!(app.s.selection, vec![Sel::Feature { feature: j }]);
    assert_eq!(app.joints.reveal_timeline, Some(j));
    // Edit Joint reopens the Joint dialog on this joint
    run_joint_cmd(&mut app, j, JointCmd::Edit);
    let d = app.dialog.as_ref().expect("the joint dialog");
    assert_eq!(d.kind, Kind::Joint);
    assert_eq!(d.editing, Some(j));
    app.cancel_dialog();
    // Edit Motion Limits opens its window on the joint's limits
    run_joint_cmd(&mut app, j, JointCmd::Limits);
    assert_eq!(app.joints.limits.as_ref().map(|l| l.joint), Some(j));
    // Animate Joint starts an animation; a locked joint refuses
    run_joint_cmd(&mut app, j, JointCmd::Animate);
    assert_eq!(animating(&app), Some(j));
    stop_animation(&mut app);
    run_joint_cmd(&mut app, j, JointCmd::Lock);
    run_joint_cmd(&mut app, j, JointCmd::Animate);
    assert_eq!(animating(&app), None);
    run_joint_cmd(&mut app, j, JointCmd::Delete);
    assert!(app.s.design.feature(j).is_none());
  }

  #[test]
  fn animation_swings_there_and_back() {
    assert_eq!(anim_value("0 deg", "360 deg", 0.0), "(0 deg) + ((360 deg) - (0 deg)) * 0.0000");
    assert_eq!(anim_value("0 deg", "360 deg", ANIM_PERIOD / 2.0), "(0 deg) + ((360 deg) - (0 deg)) * 1.0000");
    assert!(anim_value("0", "1", ANIM_PERIOD * 0.75).ends_with("0.5000"));
    let limits = fox_engine::model::JointLimits { rotation: Some(("-10 deg".into(), "45 deg".into())), slide: None };
    assert_eq!(anim_range(JointKind::Revolute, &limits), Some(("rotation", "-10 deg".into(), "45 deg".into())));
    assert_eq!(anim_range(JointKind::Slider, &limits), Some(("slide", "0 mm".into(), "20 mm".into())));
    assert_eq!(anim_range(JointKind::Rigid, &limits), None);
  }
}
